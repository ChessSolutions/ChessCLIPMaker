use std::{iter::FusedIterator, vec};

use bytes::{BufMut, Bytes, BytesMut};
use base64::Engine;
use gift::{Encoder, block};
use image::{AnimationDecoder, DynamicImage, Frame as ImageFrame, imageops::FilterType};
use ndarray::{ArrayView2, ArrayViewMut2, s};
use rusttype::{Font, PositionedGlyph, Scale};
use shakmaty::{Bitboard, Board, File, Rank, Square, uci::UciMove};

use crate::{
    api::{Comment, Coordinates, MoveGlyph, Orientation, PlayerName, RequestBody, RequestParams},
    compose::{BoardTimelineFrame, CaptionStyle, CaptionTimelineFrame},
    theme::{Gradient, Sprite, SpriteKey, Theme, Themes},
};

const GLYPH_BADGE_RADIUS: f32 = 18.0;
const GLYPH_FONT_SIZE: f32 = 32.0;
const BAR_PADDING: f32 = 10.0;
const CLOCK_FONT_SIZE: f32 = 36.0;
const CLOCK_REGION_PADDING: usize = 20;

enum RenderState {
    Preamble,
    Frame(RenderFrame),
    Complete,
}

#[derive(Clone)]
pub struct CaptionRenderFrame {
    pub text: String,
    pub secondary_text: Option<String>,
    pub style: CaptionStyle,
    pub delay_ms: u64,
}

impl CaptionRenderFrame {
    pub fn from_caption(frame: &CaptionTimelineFrame) -> Self {
        Self {
            text: frame.text.clone(),
            secondary_text: frame.secondary_text.clone(),
            style: frame.style.clone(),
            delay_ms: frame.duration_ms,
        }
    }
}


struct PlayerBars {
    white: PlayerName,
    black: PlayerName,
}

impl PlayerBars {
    fn from(
        white: Option<PlayerName>,
        black: Option<PlayerName>,
        has_clocks: bool,
    ) -> Option<PlayerBars> {
        let white_name = white.filter(|s| !s.is_empty());
        let black_name = black.filter(|s| !s.is_empty());

        if white_name.is_some() || black_name.is_some() || has_clocks {
            Some(PlayerBars {
                white: white_name.unwrap_or_default(),
                black: black_name.unwrap_or_default(),
            })
        } else {
            None
        }
    }
}

#[derive(Default)]
pub struct RenderFrame {
    board: Board,
    highlighted: Bitboard,
    checked: Bitboard,
    delay: Option<u16>,
    glyph: Option<MoveGlyph>,
    white_clock: Option<u32>,
    black_clock: Option<u32>,
    caption: Option<CaptionRenderFrame>,
    raster: Option<Vec<u8>>,
}

impl RenderFrame {
    pub fn from_board(frame: &BoardTimelineFrame) -> Self {
        let fen = frame.fen.parse::<shakmaty::fen::Fen>().unwrap_or_default();
        let board = fen.into_setup().board;
        let mut check_bitboard = Bitboard::EMPTY;
        let mut highlighted = Bitboard::EMPTY;
        if let Some(last_move) = frame.last_move.as_ref() {
            highlighted = highlight_uci(last_move.parse::<UciMove>().ok());
        }
        if let Some(check) = frame.check.as_ref() {
            if let Ok(square) = check.parse::<Square>() {
                check_bitboard = Bitboard::from(square);
            }
        }
        let glyph = frame.glyph.as_ref().and_then(|value| value.parse::<MoveGlyph>().ok());
        Self {
            board,
            highlighted,
            checked: check_bitboard,
            delay: Some(milliseconds_to_centiseconds(frame.duration_ms)),
            glyph,
            white_clock: frame.clock.as_ref().and_then(|c| c.white),
            black_clock: frame.clock.as_ref().and_then(|c| c.black),
            caption: None,
            raster: None,
        }
    }

    pub fn from_caption(frame: &CaptionTimelineFrame) -> Self {
        Self {
            board: Board::default(),
            highlighted: Bitboard::EMPTY,
            checked: Bitboard::EMPTY,
            delay: Some(milliseconds_to_centiseconds(frame.duration_ms)),
            glyph: None,
            white_clock: None,
            black_clock: None,
            caption: Some(CaptionRenderFrame::from_caption(frame)),
            raster: None,
        }
    }

    pub fn from_media(theme: &Theme, frame: &crate::compose::MediaTimelineFrame) -> Result<Vec<Self>, String> {
        let (_, encoded) = frame.data_url.split_once(',').ok_or("invalid media data")?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| "invalid media encoding")?;
        let images: Vec<(DynamicImage, u64)> = if bytes.starts_with(b"GIF8") {
            let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(&bytes))
                .map_err(|_| "could not decode GIF")?;
            decoder.into_frames().collect_frames().map_err(|_| "could not decode GIF frames")?
                .into_iter()
                .map(|image_frame: ImageFrame| {
                    let (numerator, denominator) = image_frame.delay().numer_denom_ms();
                    let delay = (numerator as u64 / denominator.max(1) as u64).max(20);
                    (DynamicImage::ImageRgba8(image_frame.into_buffer()), delay)
                })
                .collect()
        } else {
            vec![(image::load_from_memory(&bytes).map_err(|_| "could not decode image")?, frame.duration_ms)]
        };
        if images.len() > 200 {
            return Err("animated GIF exceeds 200 frames".to_string());
        }
        Ok(images.into_iter().map(|(image, delay)| {
            let canvas_width = theme.width() as u32;
            let canvas_height = theme.height(false) as u32;
            let fit = (canvas_width as f32 / image.width() as f32)
                .min(canvas_height as f32 / image.height() as f32);
            let scale = frame.scale.clamp(0.25, 3.0);
            let resized_width = ((image.width() as f32 * fit * scale).round() as u32).max(1);
            let resized_height = ((image.height() as f32 * fit * scale).round() as u32).max(1);
            let resized = image.resize_exact(resized_width, resized_height, FilterType::Lanczos3).to_rgba8();
            let mut fitted = image::RgbaImage::new(canvas_width, canvas_height);
            let available_x = canvas_width as i64 - resized_width as i64;
            let available_y = canvas_height as i64 - resized_height as i64;
            let x = available_x / 2 + (frame.offset_x.clamp(-100.0, 100.0) / 100.0 * canvas_width as f32 / 2.0) as i64;
            let y = available_y / 2 + (frame.offset_y.clamp(-100.0, 100.0) / 100.0 * canvas_height as f32 / 2.0) as i64;
            image::imageops::overlay(&mut fitted, &resized, x, y);
            let raster = fitted.pixels().map(|pixel| {
                if pixel[3] < 32 { theme.transparent_color() } else { theme.nearest_rgb(pixel[0], pixel[1], pixel[2]) }
            }).collect();
            Self {
                delay: Some(milliseconds_to_centiseconds(delay)),
                raster: Some(raster),
                ..Self::default()
            }
        }).collect())
    }

    fn diff(&self, prev: &RenderFrame) -> Bitboard {
        (prev.checked ^ self.checked)
            | (prev.highlighted ^ self.highlighted)
            | (prev.board.white() ^ self.board.white())
            | (prev.board.pawns() ^ self.board.pawns())
            | (prev.board.knights() ^ self.board.knights())
            | (prev.board.bishops() ^ self.board.bishops())
            | (prev.board.rooks() ^ self.board.rooks())
            | (prev.board.queens() ^ self.board.queens())
            | (prev.board.kings() ^ self.board.kings())
    }
}

fn milliseconds_to_centiseconds(milliseconds: u64) -> u16 {
    milliseconds.div_ceil(10).min(u16::MAX as u64) as u16
}

pub struct Render {
    theme: &'static Theme,
    font: &'static Font<'static>,
    state: RenderState,
    buffer: Vec<u8>,
    comment: Option<Comment>,
    bars: Option<PlayerBars>,
    orientation: Orientation,
    coordinates: Coordinates,
    frames: vec::IntoIter<RenderFrame>,
    kork: bool,
    clock_widths: [usize; 2],
    bar_color: Option<u8>,
    dark_square_color: Option<u8>,
}

impl Render {
    pub fn new_image(themes: &'static Themes, params: RequestParams) -> Render {
        let bars = PlayerBars::from(params.white, params.black, false);
        let theme = themes.get(params.theme, params.piece);
        Render {
            theme,
            font: themes.font(),
            buffer: vec![0; theme.height(bars.is_some()) * theme.width()],
            state: RenderState::Preamble,
            comment: params.comment,
            bars,
            orientation: params.orientation,
            coordinates: params.coordinates,
            frames: vec![RenderFrame {
                highlighted: highlight_uci(params.last_move),
                checked: params
                    .check
                    .to_square(params.fen.as_setup())
                    .into_iter()
                    .collect(),
                board: params.fen.into_setup().board,
                delay: None,
                glyph: None,
                white_clock: None,
                black_clock: None,
                caption: None,
                raster: None,
            }]
            .into_iter(),
            kork: false,
            clock_widths: [0; 2],
            bar_color: None,
            dark_square_color: None,
        }
    }

    pub fn new_animation(themes: &'static Themes, params: RequestBody) -> Render {
        let has_clocks = params
            .frames
            .iter()
            .any(|f| f.clock.white.is_some() || f.clock.black.is_some());
        let bars = PlayerBars::from(params.white, params.black, has_clocks);
        let default_delay = params.delay;
        let theme = themes.get(params.theme, params.piece);
        Render {
            theme,
            font: themes.font(),
            buffer: vec![0; theme.height(bars.is_some()) * theme.width()],
            state: RenderState::Preamble,
            comment: params.comment,
            bars,
            orientation: params.orientation,
            coordinates: params.coordinates,
            frames: params
                .frames
                .into_iter()
                .map(|frame| RenderFrame {
                    highlighted: highlight_uci(frame.last_move),
                    checked: frame
                        .check
                        .to_square(frame.fen.as_setup())
                        .into_iter()
                        .collect(),
                    board: frame.fen.into_setup().board,
                    delay: Some(frame.delay.unwrap_or(default_delay)),
                    glyph: frame.glyph,
                    white_clock: frame.clock.white,
                    black_clock: frame.clock.black,
                    caption: None,
                    raster: None,
                })
                .collect::<Vec<_>>()
                .into_iter(),
            kork: true,
            clock_widths: [0; 2],
            bar_color: None,
            dark_square_color: None,
        }
    }

    pub fn new_composed(
        themes: &'static Themes,
        params: crate::compose::ComposeRequest,
        frames: Vec<RenderFrame>,
    ) -> Render {
        let has_clocks = frames.iter().any(|frame| frame.white_clock.is_some() || frame.black_clock.is_some());
        let bars = if params.include_player_bars {
            PlayerBars::from(
                params.white.as_deref().and_then(|name| PlayerName::from(name).ok()),
                params.black.as_deref().and_then(|name| PlayerName::from(name).ok()),
                has_clocks,
            )
        } else { None };
        let theme = themes.get(params.theme, params.piece);
        let bar_color = params.background_color.as_deref().map(|color| theme.nearest_color(color));
        let dark_square_color = params.dark_square_color.as_deref().map(|color| theme.nearest_color(color));
        Render {
            theme,
            font: themes.font(),
            buffer: vec![0; theme.height(bars.is_some()) * theme.width()],
            state: RenderState::Preamble,
            comment: None,
            bars,
            orientation: params.orientation,
            coordinates: params.coordinates,
            frames: frames.into_iter(),
            kork: false,
            clock_widths: [0; 2],
            bar_color,
            dark_square_color,
        }
    }
}

impl Iterator for Render {
    type Item = Bytes;

    fn next(&mut self) -> Option<Bytes> {
        let mut output = BytesMut::new().writer();
        match self.state {
            RenderState::Preamble => {
                let mut blocks = Encoder::new(&mut output).into_block_enc();

                blocks.encode(block::Header::default()).expect("enc header");

                blocks
                    .encode(
                        block::LogicalScreenDesc::default()
                            .with_screen_height(self.theme.height(self.bars.is_some()) as u16)
                            .with_screen_width(self.theme.width() as u16)
                            .with_color_table_config(self.theme.color_table_config()),
                    )
                    .expect("enc logical screen desc");

                blocks
                    .encode(self.theme.global_color_table().clone())
                    .expect("enc global color table");

                blocks
                    .encode(block::Application::with_loop_count(0))
                    .expect("enc application");

                let comment = self
                    .comment
                    .as_ref()
                    .map_or("https://github.com/lichess-org/lila-gif".as_bytes(), |c| {
                        c.as_bytes()
                    });
                if !comment.is_empty() {
                    let mut comments = block::Comment::default();
                    comments.add_comment(comment);
                    blocks.encode(comments).expect("enc comment");
                }

                let frame = self.frames.next().unwrap_or_default();
                let mut view = ArrayViewMut2::from_shape(
                    (self.theme.height(self.bars.is_some()), self.theme.width()),
                    &mut self.buffer,
                )
                .expect("shape");

                if let Some(caption) = &frame.caption {
                    view.fill(self.theme.nearest_color(&caption.style.background_color));
                    render_caption(&mut view, self.theme, self.font, caption);
                    if let Some(delay) = frame.delay {
                        let mut ctrl = block::GraphicControl::default();
                        ctrl.set_delay_time_cs(delay);
                        blocks.encode(ctrl).expect("enc caption control");
                    }
                    blocks.encode(block::ImageDesc::default()
                        .with_height(self.theme.height(self.bars.is_some()) as u16)
                        .with_width(self.theme.width() as u16)).expect("enc caption desc");
                    let mut image_data = block::ImageData::new(self.buffer.len());
                    image_data.data_mut().extend_from_slice(&self.buffer);
                    blocks.encode(image_data).expect("enc caption data");
                    self.state = RenderState::Frame(frame);
                    return Some(output.into_inner().freeze());
                }

                let mut board_view = if let Some(ref bars) = self.bars {
                    let bar_height = self.theme.bar_height();
                    let btm_bar_y = bar_height + self.theme.width();
                    let bar_names = self.orientation.fold(
                        [(&bars.black as &str, 0), (&bars.white, btm_bar_y)],
                        [(&bars.white as &str, 0), (&bars.black, btm_bar_y)],
                    );
                    for (name, bar_top) in bar_names {
                        render_bar(
                            view.slice_mut(s!(bar_top..(bar_top + bar_height), ..)),
                            self.theme,
                            self.font,
                            name,
                            self.bar_color,
                        );
                    }

                    let mut clock_buffer = vec![0u8; bar_height * self.theme.width()];
                    for (idx, (clock, bar_top)) in
                        clock_positions(&frame, self.orientation, btm_bar_y)
                            .into_iter()
                            .enumerate()
                    {
                        if let Some(centis) = clock {
                            let (region_width, clock_left) = render_clock_region(
                                &mut clock_buffer,
                                self.theme,
                                self.font,
                                centis,
                                self.clock_widths[idx],
                            );
                            self.clock_widths[idx] = region_width;
                            let src = ArrayView2::from_shape(
                                (bar_height, region_width),
                                &clock_buffer[..bar_height * region_width],
                            )
                            .expect("clock src");
                            view.slice_mut(s!(
                                bar_top..(bar_top + bar_height),
                                clock_left..(clock_left + region_width)
                            ))
                            .assign(&src);
                        }
                    }

                    view.slice_mut(s!(bar_height..(bar_height + self.theme.width()), ..))
                } else {
                    view
                };

                if let Some(delay) = frame.delay {
                    let mut ctrl = block::GraphicControl::default();
                    ctrl.set_delay_time_cs(delay);
                    blocks.encode(ctrl).expect("enc graphic control");
                }

                render_frame_contents(
                    board_view.as_slice_mut().expect("contiguous"),
                    self.theme,
                    self.orientation,
                    self.coordinates,
                    &frame,
                    self.font,
                    self.dark_square_color,
                );

                blocks
                    .encode(
                        block::ImageDesc::default()
                            .with_height(self.theme.height(self.bars.is_some()) as u16)
                            .with_width(self.theme.width() as u16),
                    )
                    .expect("enc image desc");

                let mut image_data = block::ImageData::new(self.buffer.len());
                image_data.data_mut().extend_from_slice(&self.buffer);
                blocks.encode(image_data).expect("enc image data");

                self.state = RenderState::Frame(frame);
            }
            RenderState::Frame(_) => {
                let mut blocks = Encoder::new(&mut output).into_block_enc();

                if let Some(frame) = self.frames.next() {
                    if let Some(caption) = &frame.caption {
                        let mut full_view = ArrayViewMut2::from_shape(
                            (self.theme.height(self.bars.is_some()), self.theme.width()),
                            &mut self.buffer,
                        ).expect("caption shape");
                        full_view.fill(self.theme.nearest_color(&caption.style.background_color));
                        render_caption(&mut full_view, self.theme, self.font, caption);
                        let mut ctrl = block::GraphicControl::default();
                        if let Some(delay) = frame.delay { ctrl.set_delay_time_cs(delay); }
                        blocks.encode(ctrl).expect("enc caption control");
                        blocks.encode(block::ImageDesc::default()
                            .with_height(self.theme.height(self.bars.is_some()) as u16)
                            .with_width(self.theme.width() as u16)).expect("enc caption desc");
                        let mut image_data = block::ImageData::new(self.buffer.len());
                        image_data.data_mut().extend_from_slice(&self.buffer);
                        blocks.encode(image_data).expect("enc caption data");
                        self.state = RenderState::Frame(frame);
                        return Some(output.into_inner().freeze());
                    }
                    if self.bars.is_some() {
                        let bar_height = self.theme.bar_height();
                        let btm_bar_y = bar_height + self.theme.width();
                        if let Some(ref bars) = self.bars {
                            let curr_clocks = clock_positions(&frame, self.orientation, btm_bar_y);
                            let bar_names = self.orientation.fold(
                                [(&bars.black as &str, 0), (&bars.white as &str, btm_bar_y)],
                                [(&bars.white as &str, 0), (&bars.black as &str, btm_bar_y)],
                            );
                            for (name, bar_top) in bar_names {
                                let bar_size = bar_height * self.theme.width();
                                {
                                    let mut bar_view = ArrayViewMut2::from_shape(
                                        (bar_height, self.theme.width()),
                                        &mut self.buffer[..bar_size],
                                    ).expect("bar shape");
                                    render_bar(bar_view.view_mut(), self.theme, self.font, name, self.bar_color);
                                    if let Some((idx, (Some(centis), _))) = curr_clocks
                                        .iter().enumerate().find(|(_, (_, top))| *top == bar_top)
                                    {
                                        let mut clock_buffer = vec![0u8; bar_size];
                                        let (region_width, clock_left) = render_clock_region(
                                            &mut clock_buffer,
                                            self.theme,
                                            self.font,
                                            *centis,
                                            self.clock_widths[idx],
                                        );
                                        self.clock_widths[idx] = region_width;
                                        let clock_view = ArrayView2::from_shape(
                                            (bar_height, region_width),
                                            &clock_buffer[..bar_height * region_width],
                                        ).expect("clock shape");
                                        bar_view.slice_mut(s!(.., clock_left..clock_left + region_width)).assign(&clock_view);
                                    }
                                }
                                blocks.encode(block::ImageDesc::default()
                                    .with_top(bar_top as u16)
                                    .with_height(bar_height as u16)
                                    .with_width(self.theme.width() as u16)).expect("enc player bar desc");
                                let mut bar_data = block::ImageData::new(bar_size);
                                bar_data.data_mut().extend_from_slice(&self.buffer[..bar_size]);
                                blocks.encode(bar_data).expect("enc player bar data");
                            }
                        }
                    }

                    let mut ctrl = block::GraphicControl::default();
                    ctrl.set_disposal_method(block::DisposalMethod::Keep);
                    ctrl.set_transparent_color(Some(self.theme.transparent_color()));
                    if let Some(delay) = frame.delay {
                        ctrl.set_delay_time_cs(delay);
                    }
                    blocks.encode(ctrl).expect("enc graphic control");

                    let board_size = self.theme.width() * self.theme.width();
                    let ((left, y), (w, h)) = render_frame_contents(
                        &mut self.buffer[..board_size],
                        self.theme,
                        self.orientation,
                        self.coordinates,
                        &frame,
                        self.font,
                        self.dark_square_color,
                    );

                    let top = y + if self.bars.is_some() {
                        self.theme.bar_height()
                    } else {
                        0
                    };

                    blocks
                        .encode(
                            block::ImageDesc::default()
                                .with_left(left as u16)
                                .with_top(top as u16)
                                .with_height(h as u16)
                                .with_width(w as u16),
                        )
                        .expect("enc image desc");

                    let mut image_data = block::ImageData::new(w * h);
                    image_data
                        .data_mut()
                        .extend_from_slice(&self.buffer[..(w * h)]);
                    blocks.encode(image_data).expect("enc image data");

                    self.state = RenderState::Frame(frame);
                } else {
                    // Add a black frame at the end, to work around twitter
                    // cutting off the last frame.
                    if self.kork {
                        let mut ctrl = block::GraphicControl::default();
                        ctrl.set_disposal_method(block::DisposalMethod::Keep);
                        ctrl.set_transparent_color(Some(self.theme.transparent_color()));
                        ctrl.set_delay_time_cs(1);
                        blocks.encode(ctrl).expect("enc graphic control");

                        let height = self.theme.height(self.bars.is_some());
                        let width = self.theme.width();
                        blocks
                            .encode(
                                block::ImageDesc::default()
                                    .with_left(0)
                                    .with_top(0)
                                    .with_height(height as u16)
                                    .with_width(width as u16),
                            )
                            .expect("enc image desc");

                        let mut image_data = block::ImageData::new(height * width);
                        image_data
                            .data_mut()
                            .resize(height * width, self.theme.bar_color());
                        blocks.encode(image_data).expect("enc image data");
                    }

                    blocks
                        .encode(block::Trailer::default())
                        .expect("enc trailer");
                    self.state = RenderState::Complete;
                }
            }
            RenderState::Complete => return None,
        }
        Some(output.into_inner().freeze())
    }
}

impl FusedIterator for Render {}

fn render_glyph_badge(
    square_buffer: &mut ArrayViewMut2<u8>,
    theme: &Theme,
    font: &Font,
    glyph: MoveGlyph,
) {
    let square_size = theme.square();
    let center_x = square_size as f32 - GLYPH_BADGE_RADIUS;
    let center_y = GLYPH_BADGE_RADIUS;
    let bg_color = theme.glyph_background_color(glyph);
    let inner_radius_sq = (GLYPH_BADGE_RADIUS - 0.5).powi(2);
    let min_x = (center_x - GLYPH_BADGE_RADIUS).max(0.0) as usize;
    let max_x = ((center_x + GLYPH_BADGE_RADIUS).ceil() as usize).min(square_size);
    let min_y = (center_y - GLYPH_BADGE_RADIUS).max(0.0) as usize;
    let max_y = ((center_y + GLYPH_BADGE_RADIUS).ceil() as usize).min(square_size);

    for y in min_y..max_y {
        for x in min_x..max_x {
            let dx = x as f32 + 0.5 - center_x;
            let dy = y as f32 + 0.5 - center_y;
            if dx * dx + dy * dy <= inner_radius_sq {
                square_buffer[(y, x)] = bg_color;
            }
        }
    }

    let scale = Scale {
        x: GLYPH_FONT_SIZE,
        y: GLYPH_FONT_SIZE,
    };
    let v_metrics = font.v_metrics(scale);
    let glyphs: Vec<_> = font
        .layout(glyph.into(), scale, rusttype::point(0.0, v_metrics.ascent))
        .collect();

    let (gmin_x, gmax_x, gmin_y, gmax_y) =
        glyphs.iter().filter_map(|g| g.pixel_bounding_box()).fold(
            (i32::MAX, i32::MIN, i32::MAX, i32::MIN),
            |(min_x, max_x, min_y, max_y), bb| {
                (
                    min_x.min(bb.min.x),
                    max_x.max(bb.max.x),
                    min_y.min(bb.min.y),
                    max_y.max(bb.max.y),
                )
            },
        );
    if gmin_x == i32::MAX {
        return;
    }
    let offset_x = (center_x - (gmax_x + gmin_x) as f32 / 2.0).round() as usize;
    let offset_y = (center_y - (gmax_y + gmin_y) as f32 / 2.0).round() as usize;

    render_text(
        &mut square_buffer.slice_mut(s!(offset_y.., offset_x..)),
        glyphs,
        theme,
        Gradient::from(glyph),
        false,
    );
}

fn render_frame_contents(
    buffer: &mut [u8],
    theme: &Theme,
    orientation: Orientation,
    coordinates: Coordinates,
    frame: &RenderFrame,
    font: &Font,
    dark_square_color: Option<u8>,
) -> ((usize, usize), (usize, usize)) {
    if let Some(caption) = &frame.caption {
        let width = theme.width();
        let height = theme.height(false);
        let mut view = ArrayViewMut2::from_shape((height, width), buffer).expect("shape");
        view.fill(theme.nearest_color(&caption.style.background_color));
        render_caption(&mut view, theme, font, caption);
        return ((0, 0), (width, height));
    }

    if let Some(raster) = &frame.raster {
        buffer[..raster.len()].copy_from_slice(raster);
        return ((0, 0), (theme.width(), theme.height(false)));
    }

    let diff = Bitboard::FULL;

    let x_min = diff
        .into_iter()
        .map(|sq| orientation.x(sq))
        .min()
        .unwrap_or(0);
    let y_min = diff
        .into_iter()
        .map(|sq| orientation.y(sq))
        .min()
        .unwrap_or(0);
    let x_max = diff
        .into_iter()
        .map(|sq| orientation.x(sq))
        .max()
        .unwrap_or(0)
        + 1;
    let y_max = diff
        .into_iter()
        .map(|sq| orientation.y(sq))
        .max()
        .unwrap_or(0)
        + 1;

    let width = (x_max - x_min) * theme.square();
    let height = (y_max - y_min) * theme.square();

    let mut view = ArrayViewMut2::from_shape((height, width), buffer).expect("shape");
    view.fill(theme.transparent_color());

    for sq in diff {
        let key = SpriteKey {
            piece: frame.board.piece_at(sq),
            dark_square: sq.is_dark(),
            highlight: frame.highlighted.contains(sq),
            check: frame.checked.contains(sq),
        };

        let left = (orientation.x(sq) - x_min) * theme.square();
        let top = (orientation.y(sq) - y_min) * theme.square();

        let mut square_buffer = view.slice_mut(s!(
            top..(top + theme.square()),
            left..(left + theme.square())
        ));

        match theme.sprite(&key) {
            Sprite::Paste(paste) => square_buffer.assign(&paste),
            Sprite::Fill(fill) => square_buffer.fill(fill),
        }

        if coordinates == Coordinates::Yes {
            let coords_scale: Scale = Scale { x: 30.0, y: 30.0 };
            let (coords_rank, coords_file) = match orientation {
                Orientation::White => (Rank::First, File::H),
                Orientation::Black => (Rank::Eighth, File::A),
            };
            if sq.rank() == coords_rank {
                render_file(&mut square_buffer, &sq, &key, theme, font, coords_scale)
            };
            if sq.file() == coords_file {
                render_rank(&mut square_buffer, &sq, &key, theme, font, coords_scale)
            };
        }

        if let Some(glyph) = frame.glyph
            && frame.highlighted.contains(sq)
            && frame.board.piece_at(sq).is_some()
        {
            render_glyph_badge(&mut square_buffer, theme, font, glyph);
        }

        if let Some(custom_dark) = dark_square_color
            && key.dark_square
            && !key.highlight
        {
            let original_dark = theme.gradient_color(Gradient::LightDark, 0.0);
            square_buffer.mapv_inplace(|pixel| if pixel == original_dark { custom_dark } else { pixel });
        }
    }

    (
        (theme.square() * x_min, theme.square() * y_min),
        (width, height),
    )
}

fn render_caption(
    view: &mut ArrayViewMut2<u8>,
    theme: &Theme,
    font: &Font,
    caption: &CaptionRenderFrame,
) {
    let text = caption.text.as_str();
    let scale = Scale {
        x: caption.style.font_size as f32,
        y: caption.style.font_size as f32,
    };
    let v_metrics = font.v_metrics(scale);
    let width = view.shape()[1];
    let height = view.shape()[0];
    let mut text_view = view.slice_mut(s!(.., ..));
    text_view.fill(theme.nearest_color(&caption.style.background_color));
    let caption_gradient = theme.caption_gradient(&caption.style.text_color, &caption.style.background_color);
    let platform_gradient = caption.style.platform.as_deref().map(|platform| {
        let color = if platform == "chesscom" { "#81b64c" } else { "#d59120" };
        theme.caption_gradient(color, &caption.style.background_color)
    });
    let lines = text.lines().take(8).collect::<Vec<_>>();
    let font_height = caption.style.font_size as usize;
    let line_step = font_height + 8;
    let text_height = lines.len().saturating_mul(line_step).saturating_sub(8);
    let padding = caption.style.padding as usize;
    let mut y = match caption.style.vertical_align {
        crate::compose::VerticalAlign::Top => padding,
        crate::compose::VerticalAlign::Middle => height.saturating_sub(text_height) / 2,
        crate::compose::VerticalAlign::Bottom => height.saturating_sub(text_height + padding),
    };
    for line in lines {
        let line_glyphs: Vec<_> = font.layout(line, scale, rusttype::point(0.0, v_metrics.ascent)).collect();
        let line_width = line_glyphs.iter().filter_map(|g| g.pixel_bounding_box()).map(|bb| bb.max.x).max().unwrap_or(0) as usize;
        let x = match caption.style.horizontal_align {
            crate::compose::HorizontalAlign::Center => (width.saturating_sub(line_width)) / 2,
            crate::compose::HorizontalAlign::Right => width.saturating_sub(line_width),
            crate::compose::HorizontalAlign::Left => caption.style.padding as usize,
        };
        let glyph_offset = y.min(height.saturating_sub(font_height));
        let mut line_view = text_view.slice_mut(s!(glyph_offset..(glyph_offset + font_height), x..));
        let colors = if line.starts_with("Played on ") {
            platform_gradient.as_ref().unwrap_or(&caption_gradient)
        } else {
            &caption_gradient
        };
        render_caption_text(&mut line_view, line_glyphs, colors);
        y += line_step;
    }
    let _ = (width, height);
}

fn render_caption_text<'a>(
    view: &mut ArrayViewMut2<'_, u8>,
    glyphs: impl IntoIterator<Item = PositionedGlyph<'a>>,
    colors: &[u8; 16],
) {
    for glyph in glyphs {
        if let Some(bb) = glyph.pixel_bounding_box() {
            glyph.draw(|left, top, intensity| {
                if intensity > 0.03
                    && let Some(pixel) = view.get_mut(((bb.min.y + top as i32) as usize, (bb.min.x + left as i32) as usize))
                {
                    *pixel = colors[(intensity * 15.0).round().clamp(0.0, 15.0) as usize];
                }
            });
        }
    }
}

fn render_file(
    square_buffer: &mut ArrayViewMut2<u8>,
    sq: &Square,
    sprite_key: &SpriteKey,
    theme: &Theme,
    font: &Font,
    font_scale: Scale,
) {
    let v_metrics = font.v_metrics(font_scale);
    let square_file = format!("{}", sq.file());
    let glyphs = font.layout(
        &square_file,
        font_scale,
        rusttype::point(5.0, theme.square() as f32 + v_metrics.descent),
    );

    render_text(
        square_buffer,
        glyphs,
        theme,
        sprite_key.light_dark_gradient(),
        !sprite_key.dark_square,
    );
}

fn render_rank(
    square_buffer: &mut ArrayViewMut2<u8>,
    sq: &Square,
    sprite_key: &SpriteKey,
    theme: &Theme,
    font: &Font,
    font_scale: Scale,
) {
    let v_metrics = font.v_metrics(font_scale);
    let square_rank = format!("{}", sq.rank());
    let glyphs = font.layout(
        &square_rank,
        font_scale,
        rusttype::point(theme.square() as f32 - 15.0, v_metrics.ascent),
    );

    render_text(
        square_buffer,
        glyphs,
        theme,
        sprite_key.light_dark_gradient(),
        !sprite_key.dark_square,
    );
}

fn render_bar(mut view: ArrayViewMut2<u8>, theme: &Theme, font: &Font, player_name: &str, background: Option<u8>) {
    view.fill(background.unwrap_or_else(|| theme.bar_color()));

    let height = 40.0;
    let scale = Scale {
        x: height,
        y: height,
    };

    let v_metrics = font.v_metrics(scale);
    let mut glyphs = font.layout(
        player_name,
        scale,
        rusttype::point(BAR_PADDING, BAR_PADDING + v_metrics.ascent),
    );

    let titles = [
        "GM ", "WGM ", "IM ", "WIM ", "FM ", "WFM ", "NM ", "CM ", "WCM ", "WNM ", "LM ",
    ];
    let prefix_color = if player_name.starts_with("BOT ") {
        Gradient::BotBar
    } else if titles.iter().any(|title| player_name.starts_with(title)) {
        Gradient::GoldBar
    } else {
        Gradient::TextBar
    };
    render_text(
        &mut view,
        glyphs
            .by_ref()
            .take_while(|g| g.pixel_bounding_box().is_some()),
        theme,
        prefix_color,
        false,
    );

    render_text(&mut view, glyphs, theme, Gradient::TextBar, false);
}

fn format_clock(centis: u32) -> String {
    let total_secs = centis / 100;
    let tenths = (centis % 100) / 10;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, mins, secs)
    } else if mins > 0 {
        format!("{}:{:02}", mins, secs)
    } else {
        format!("00:{:02}.{}", secs, tenths)
    }
}

fn render_clock_region(
    buffer: &mut [u8],
    theme: &Theme,
    font: &Font,
    centis: u32,
    min_width: usize,
) -> (usize, usize) {
    let bar_height = theme.bar_height();
    let scale = Scale {
        x: CLOCK_FONT_SIZE,
        y: CLOCK_FONT_SIZE,
    };
    let v_metrics = font.v_metrics(scale);

    let clock_str = format_clock(centis);
    let glyphs: Vec<_> = font
        .layout(
            &clock_str,
            scale,
            rusttype::point(
                0.0,
                (bar_height as f32 - CLOCK_FONT_SIZE) / 2.0 + v_metrics.ascent,
            ),
        )
        .collect();
    let text_width = glyphs
        .iter()
        .filter_map(|g| g.pixel_bounding_box())
        .map(|bb| bb.max.x)
        .max()
        .unwrap_or(0) as usize;

    let region_width = text_width.max(min_width);
    let mut view = ArrayViewMut2::from_shape(
        (bar_height, region_width),
        &mut buffer[..bar_height * region_width],
    )
    .expect("clock region shape");
    view.fill(theme.bar_color());

    let text_offset = region_width - text_width;
    let mut text_view = view.slice_mut(s!(.., text_offset as usize..));
    render_text(&mut text_view, glyphs, theme, Gradient::TextBar, false);

    let clock_left = theme.width() - region_width - CLOCK_REGION_PADDING;
    (region_width, clock_left)
}

fn highlight_uci(uci: Option<UciMove>) -> Bitboard {
    match uci {
        Some(UciMove::Normal { from, to, .. }) => Bitboard::from(from) | Bitboard::from(to),
        Some(UciMove::Put { to, .. }) => Bitboard::from(to),
        _ => Bitboard::EMPTY,
    }
}

fn clock_positions(
    frame: &RenderFrame,
    orientation: Orientation,
    btm_bar_y: usize,
) -> [(Option<u32>, usize); 2] {
    orientation.fold(
        [(frame.black_clock, 0), (frame.white_clock, btm_bar_y)],
        [(frame.white_clock, 0), (frame.black_clock, btm_bar_y)],
    )
}

fn render_text<'a>(
    view: &mut ArrayViewMut2<'_, u8>,
    glyphs: impl IntoIterator<Item = PositionedGlyph<'a>>,
    theme: &Theme,
    gradient: Gradient,
    invert: bool,
) {
    for glyph in glyphs {
        if let Some(bb) = glyph.pixel_bounding_box() {
            glyph.draw(|left, top, intensity| {
                if intensity > 0.0625
                    && let Some(pixel) = view.get_mut((
                        (bb.min.y + top as i32) as usize,
                        (bb.min.x + left as i32) as usize,
                    ))
                {
                    *pixel = theme
                        .gradient_color(gradient, if invert { 1.0 - intensity } else { intensity });
                }
            });
        }
    }
}
