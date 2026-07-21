use std::{convert::Infallible, net::SocketAddr, path::PathBuf, process::Command, time::Duration};

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Query},
    http::{header::CONTENT_TYPE, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use futures::stream;
use listenfd::ListenFd;
#[cfg(not(target_os = "windows"))]
use tikv_jemallocator::Jemalloc;
use tokio::net::{TcpListener, UnixListener};

mod api;
mod assets;
mod compose;
mod render;
mod theme;

use api::{RequestBody, RequestParams};
use compose::{ComposeRequest, LatestGameRequest, LichessImportRequest, ParseRequest, parse_pgn};
use render::Render;
use theme::Themes;

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct SavedAccounts {
    #[serde(default)]
    lichess: Vec<String>,
    #[serde(default)]
    chesscom: Vec<String>,
    default_lichess: Option<String>,
    default_chesscom: Option<String>,
}

#[cfg(not(target_os = "windows"))]
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
    embedded_response(include_bytes!("../web/index.html"), "text/html; charset=utf-8")
}

async fn editor_asset(path: axum::extract::Path<String>) -> impl IntoResponse {
    let clean = path.0.trim_start_matches('/');
    match clean {
        "lichess-mark.svg" => embedded_response(include_bytes!("../web/assets/lichess-mark.svg"), "image/svg+xml"),
        "chesscom-mark.svg" => embedded_response(include_bytes!("../web/assets/chesscom-mark.svg"), "image/svg+xml"),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn editor_styles() -> impl IntoResponse {
    embedded_response(include_bytes!("../web/styles.css"), "text/css; charset=utf-8")
}

async fn editor_script() -> impl IntoResponse {
    embedded_response(include_bytes!("../web/app.js"), "application/javascript; charset=utf-8")
}

async fn google_fonts() -> impl IntoResponse {
    embedded_response(include_bytes!("../web/google-fonts.json"), "application/json")
}

fn embedded_response(body: &'static [u8], content_type: &'static str) -> Response {
    (StatusCode::OK, [(CONTENT_TYPE, content_type)], body).into_response()
}

fn accounts_path() -> PathBuf {
    let local = PathBuf::from("accounts.json");
    if local.exists() {
        return local;
    }
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("accounts.json")))
        .unwrap_or(local)
}

fn open_editor(address: SocketAddr) {
    let url = format!("http://{address}");
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(450));
        #[cfg(target_os = "windows")]
        let _ = Command::new("cmd").args(["/C", "start", "", &url]).spawn();
        #[cfg(target_os = "macos")]
        let _ = Command::new("open").arg(&url).spawn();
        #[cfg(all(unix, not(target_os = "macos")))]
        let _ = Command::new("xdg-open").arg(&url).spawn();
    });
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
    if req.url.contains("chess.com/game/") {
        return import_chesscom_url(&req.url).await;
    }
    let game_id = reqwest::Url::parse(req.url.trim())
        .ok()
        .and_then(|url| url.path_segments()?.filter(|part| !part.is_empty()).last().map(str::to_string))
        .and_then(|part| part.get(..8.min(part.len())).map(str::to_string))
        .unwrap_or_default();
    if game_id.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid lichess url"}))).into_response();
    }
    fetch_pgn(format!("https://lichess.org/game/export/{game_id}?clocks=true"), Some("application/x-chess-pgn")).await
}

async fn import_chesscom_url(url: &str) -> Response {
    let game_id = url.split('/').filter(|part| !part.is_empty()).last().unwrap_or_default();
    if game_id.is_empty() { return import_error("Invalid Chess.com game URL"); }
    let accounts: SavedAccounts = match tokio::fs::read_to_string("accounts.json").await
        .ok().and_then(|body| serde_json::from_str(&body).ok()) {
        Some(accounts) => accounts,
        None => return import_error("Save a default Chess.com username in settings first"),
    };
    let Some(username) = accounts.default_chesscom else {
        return import_error("Save a default Chess.com username in settings first");
    };
    let client = reqwest::Client::new();
    let archives: serde_json::Value = match client
        .get(format!("https://api.chess.com/pub/player/{username}/games/archives"))
        .header(reqwest::header::USER_AGENT, "ChessClipMaker/1.0").send().await {
        Ok(response) if response.status().is_success() => match response.json().await { Ok(value) => value, Err(_) => return import_error("Invalid Chess.com archive data") },
        _ => return import_error("Could not retrieve Chess.com archives"),
    };
    let archive_urls = archives["archives"].as_array().cloned().unwrap_or_default();
    for archive in archive_urls.iter().rev().take(3).filter_map(|value| value.as_str()) {
        let games: serde_json::Value = match client.get(archive)
            .header(reqwest::header::USER_AGENT, "ChessClipMaker/1.0").send().await {
            Ok(response) if response.status().is_success() => match response.json().await { Ok(value) => value, Err(_) => continue },
            _ => continue,
        };
        if let Some(pgn) = games["games"].as_array().and_then(|items| items.iter().find_map(|game| {
            let link = game["url"].as_str()?;
            (link.trim_end_matches('/').ends_with(game_id)).then(|| game["pgn"].as_str()).flatten()
        })) {
            return (StatusCode::OK, Json(serde_json::json!({"pgn": pgn}))).into_response();
        }
    }
    import_error("Game was not found in the default player's three most recent Chess.com archive months")
}

async fn latest_game(Json(req): Json<LatestGameRequest>) -> impl IntoResponse {
    let username = req.username.trim();
    if username.is_empty() || !username.chars().all(|ch| ch.is_ascii_alphanumeric() || "_-".contains(ch)) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid username"}))).into_response();
    }
    match req.site.as_str() {
        "lichess" => {
            let client = reqwest::Client::new();
            let response = match client.get(format!("https://lichess.org/api/games/user/{username}?max={}&clocks=true", req.offset + 1))
                .header(reqwest::header::ACCEPT, "application/x-chess-pgn")
                .header(reqwest::header::USER_AGENT, "ChessClipMaker/1.0").send().await {
                Ok(response) if response.status().is_success() => response,
                _ => return import_error("Could not retrieve Lichess games"),
            };
            let body = match response.text().await { Ok(body) => body, Err(_) => return import_error("Could not read Lichess PGN") };
            let games = split_pgn_games(&body);
            let Some(pgn) = games.get(req.offset) else { return import_error("That Lichess game was not found") };
            (StatusCode::OK, Json(serde_json::json!({"pgn": pgn}))).into_response()
        },
        "chesscom" => {
            let client = reqwest::Client::new();
            let archives: serde_json::Value = match client
                .get(format!("https://api.chess.com/pub/player/{username}/games/archives"))
                .header(reqwest::header::USER_AGENT, "ChessClipMaker/1.0")
                .send().await
            {
                Ok(response) if response.status().is_success() => match response.json().await {
                    Ok(value) => value,
                    Err(_) => return import_error("Chess.com returned invalid archive data"),
                },
                _ => return import_error("Chess.com player was not found"),
            };
            let Some(archive) = archives["archives"].as_array().and_then(|items| items.last()).and_then(|value| value.as_str()) else {
                return import_error("No Chess.com games were found");
            };
            let games: serde_json::Value = match client.get(archive).header(reqwest::header::USER_AGENT, "ChessClipMaker/1.0").send().await {
                Ok(response) if response.status().is_success() => match response.json().await {
                    Ok(value) => value,
                    Err(_) => return import_error("Chess.com returned invalid game data"),
                },
                _ => return import_error("Could not retrieve Chess.com games"),
            };
            let Some(pgn) = games["games"].as_array().and_then(|items| items.iter().rev().nth(req.offset)).and_then(|game| game["pgn"].as_str()) else {
                return import_error("No Chess.com games were found");
            };
            (StatusCode::OK, Json(serde_json::json!({"pgn": pgn}))).into_response()
        }
        _ => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "unsupported chess site"}))).into_response(),
    }
}

fn split_pgn_games(body: &str) -> Vec<&str> {
    let mut starts: Vec<_> = body.match_indices("[Event ").map(|(index, _)| index).collect();
    if starts.is_empty() { return Vec::new(); }
    starts.push(body.len());
    starts.windows(2).map(|window| body[window[0]..window[1]].trim()).collect()
}

async fn fetch_pgn(url: String, accept: Option<&str>) -> Response {
    let client = reqwest::Client::new();
    let mut request = client.get(url).header(reqwest::header::USER_AGENT, "ChessClipMaker/1.0");
    if let Some(accept) = accept { request = request.header(reqwest::header::ACCEPT, accept); }
    let response = match request.send().await {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => return import_error(&format!("Game service returned {}", response.status())),
        Err(_) => return import_error("Could not connect to the game service"),
    };
    let pgn = match response.text().await { Ok(body) => body, Err(_) => return import_error("Could not read the PGN") };
    if !pgn.trim_start().starts_with('[') || !pgn.contains("1.") {
        return import_error("The game service did not return PGN data");
    }
    (StatusCode::OK, Json(serde_json::json!({"pgn": pgn}))).into_response()
}

fn import_error(message: &str) -> Response {
    (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": message}))).into_response()
}

async fn get_accounts() -> impl IntoResponse {
    match tokio::fs::read_to_string(accounts_path()).await {
        Ok(body) => (StatusCode::OK, [(CONTENT_TYPE, "application/json")], body).into_response(),
        Err(_) => Json(SavedAccounts::default()).into_response(),
    }
}

async fn save_accounts(Json(mut accounts): Json<SavedAccounts>) -> impl IntoResponse {
    accounts.lichess.retain(|name| valid_username(name));
    accounts.chesscom.retain(|name| valid_username(name));
    accounts.lichess.sort();
    accounts.lichess.dedup();
    accounts.chesscom.sort();
    accounts.chesscom.dedup();
    if !accounts.default_lichess.as_ref().is_some_and(|name| accounts.lichess.contains(name)) {
        accounts.default_lichess = accounts.lichess.first().cloned();
    }
    if !accounts.default_chesscom.as_ref().is_some_and(|name| accounts.chesscom.contains(name)) {
        accounts.default_chesscom = accounts.chesscom.first().cloned();
    }
    let body = match serde_json::to_string_pretty(&accounts) {
        Ok(body) => body,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid accounts"}))).into_response(),
    };
    match tokio::fs::write(accounts_path(), format!("{body}\n")).await {
        Ok(_) => Json(accounts).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "could not save accounts.json"}))).into_response(),
    }
}

fn valid_username(name: &String) -> bool {
    !name.is_empty() && name.chars().all(|ch| ch.is_ascii_alphanumeric() || "_-".contains(ch))
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
        .route(
            "/compose.gif",
            post(move |req| compose(themes, req)).layer(DefaultBodyLimit::max(25 * 1024 * 1024)),
        )
        .route("/api/pgn/parse", post(parse_pgn_endpoint))
        .route("/api/lichess/import", post(lichess_import))
        .route("/api/latest-game", post(latest_game))
        .route("/api/accounts", get(get_accounts).post(save_accounts))
        .route("/example.gif", get(move || example(themes)))
        .route("/styles.css", get(editor_styles))
        .route("/app.js", get(editor_script))
        .route("/google-fonts.json", get(google_fonts));

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
        open_editor(opt.bind);
        axum::serve(listener, app).await.expect("serve");
    }
}
