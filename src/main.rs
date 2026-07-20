use std::{convert::Infallible, net::SocketAddr};

use axum::{
    body::Body,
    extract::Query,
    http::{header::CONTENT_TYPE, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use futures::stream;
use listenfd::ListenFd;
use tikv_jemallocator::Jemalloc;
use tokio::net::{TcpListener, UnixListener};

mod api;
mod assets;
mod compose;
mod render;
mod theme;

use api::{RequestBody, RequestParams};
use compose::{ComposeRequest, LichessImportRequest, ParseRequest, parse_pgn};
use render::Render;
use theme::Themes;

#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[derive(Parser)]
struct Opt {
    /// Listen on this address.
    #[arg(long = "bind", env = "LILA_GIF_BIND", default_value = "127.0.0.1:6175")]
    bind: SocketAddr,
}

async fn image(themes: &'static Themes, Query(req): Query<RequestParams>) -> impl IntoResponse {
    Response::builder()
        .header(CONTENT_TYPE, "image/gif")
        .body(Body::from_stream(stream::iter(
            Render::new_image(themes, req).map(Ok::<_, Infallible>),
        )))
        .unwrap()
}

async fn game(themes: &'static Themes, Json(req): Json<RequestBody>) -> impl IntoResponse {
    Response::builder()
        .header(CONTENT_TYPE, "image/gif")
        .body(Body::from_stream(stream::iter(
            Render::new_animation(themes, req).map(Ok::<_, Infallible>),
        )))
        .unwrap()
}

async fn example(themes: &'static Themes) -> impl IntoResponse {
    game(themes, Json(RequestBody::example())).await
}

async fn editor_root() -> impl IntoResponse {
    match tokio::fs::read("web/index.html").await {
        Ok(body) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
            body,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn editor_asset(path: axum::extract::Path<String>) -> impl IntoResponse {
    let clean = path.0.trim_start_matches('/');
    let file = format!("web/{clean}");
    match tokio::fs::read(&file).await {
        Ok(body) => {
            let mime_type = match clean.rsplit('.').next() {
                Some("css") => "text/css; charset=utf-8",
                Some("js") => "application/javascript; charset=utf-8",
                Some("html") => "text/html; charset=utf-8",
                Some("png") => "image/png",
                Some("svg") => "image/svg+xml",
                Some("gif") => "image/gif",
                _ => "application/octet-stream",
            };
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, mime_type)],
                body,
            )
                .into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn editor_styles() -> impl IntoResponse {
    serve_editor_file("styles.css", "text/css; charset=utf-8").await
}

async fn editor_script() -> impl IntoResponse {
    serve_editor_file("app.js", "application/javascript; charset=utf-8").await
}

async fn serve_editor_file(name: &str, content_type: &'static str) -> Response {
    match tokio::fs::read(format!("web/{name}")).await {
        Ok(body) => (StatusCode::OK, [(CONTENT_TYPE, content_type)], body).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn compose(themes: &'static Themes, Json(req): Json<ComposeRequest>) -> impl IntoResponse {
    if let Err(err) = req.validate() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": err}))).into_response();
    }
    let mut frames = Vec::new();
    for frame in &req.timeline {
        match frame {
            compose::TimelineFrame::Board(board) => frames.push(render::RenderFrame::from_board(board)),
            compose::TimelineFrame::Caption(caption) => frames.push(render::RenderFrame::from_caption(caption)),
            compose::TimelineFrame::Media(media) => match render::RenderFrame::from_media(
                themes.get(req.theme, req.piece),
                media,
            ) {
                Ok(media_frames) => frames.extend(media_frames),
                Err(err) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": err}))).into_response(),
            },
        }
    }
    let render = render::Render::new_composed(themes, req, frames);
    Response::builder()
        .header(CONTENT_TYPE, "image/gif")
        .body(Body::from_stream(stream::iter(render.map(Ok::<_, Infallible>))))
        .unwrap()
}

async fn parse_pgn_endpoint(Json(req): Json<ParseRequest>) -> impl IntoResponse {
    match parse_pgn(&req.pgn) {
        Ok(game) => (StatusCode::OK, Json(game)).into_response(),
        Err(err) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": err}))).into_response(),
    }
}

async fn lichess_import(Json(req): Json<LichessImportRequest>) -> impl IntoResponse {
    let game_id = req.url.trim().split('/').last().unwrap_or_default().to_string();
    if game_id.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid lichess url"}))).into_response();
    }
    let pgn = match reqwest::get(format!("https://lichess.org/game/export/{}", game_id)).await {
        Ok(res) => match res.text().await {
            Ok(body) => body,
            Err(_) => return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": "import failed"}))).into_response(),
        },
        Err(_) => return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": "import failed"}))).into_response(),
    };
    if pgn.contains("Private game") || pgn.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "private or unavailable game"}))).into_response();
    }
    (StatusCode::OK, Json(serde_json::json!({"pgn": pgn}))).into_response()
}

#[tokio::main]
async fn main() {
    let opt = Opt::parse();

    let themes: &'static Themes = Box::leak(Box::new(Themes::new()));

    let app = Router::new()
        .route("/", get(editor_root))
        .route("/index.html", get(editor_root))
        .route("/assets/{*path}", get(editor_asset))
        .route("/image.gif", get(move |req| image(themes, req)))
        .route("/game.gif", post(move |req| game(themes, req)))
        .route("/compose.gif", post(move |req| compose(themes, req)))
        .route("/api/pgn/parse", post(parse_pgn_endpoint))
        .route("/api/lichess/import", post(lichess_import))
        .route("/example.gif", get(move || example(themes)))
        .route("/styles.css", get(editor_styles))
        .route("/app.js", get(editor_script));

    let mut fds = ListenFd::from_env();
    if let Ok(Some(uds)) = fds.take_unix_listener(0) {
        uds.set_nonblocking(true).expect("set nonblocking");
        let listener = UnixListener::from_std(uds).expect("listener");
        axum::serve(listener, app).await.expect("serve");
    } else if let Ok(Some(tcp)) = fds.take_tcp_listener(0) {
        tcp.set_nonblocking(true).expect("set nonblocking");
        let listener = TcpListener::from_std(tcp).expect("listener");
        axum::serve(listener, app).await.expect("serve");
    } else {
        let listener = TcpListener::bind(&opt.bind).await.expect("bind");
        axum::serve(listener, app).await.expect("serve");
    }
}
