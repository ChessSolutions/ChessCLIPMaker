use serde::{Deserialize, Serialize};
use shakmaty::{CastlingMode, Chess, EnPassantMode, Position, fen::Fen, san::SanPlus, uci::UciMove};

use crate::{api::{Coordinates, Orientation}, assets::{BoardTheme, PieceSet}};

fn default_font_weight() -> u16 {
    400
}

fn default_font_size() -> u32 {
    56
}

fn default_line_height() -> f32 {
    1.1
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalAlign {
    #[default]
    Top,
    Middle,
    Bottom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptionStyle {
    #[serde(default)]
    pub font_family: String,
    #[serde(default = "default_font_weight")]
    pub font_weight: u16,
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    #[serde(default)]
    pub text_color: String,
    #[serde(default)]
    pub background_color: String,
    #[serde(default)]
    pub horizontal_align: HorizontalAlign,
    #[serde(default)]
    pub vertical_align: VerticalAlign,
    #[serde(default = "default_line_height")]
    pub line_height: f32,
    #[serde(default)]
    pub padding: u32,
    #[serde(default)]
    pub max_text_width: Option<u32>,
    #[serde(default)]
    pub border: bool,
    #[serde(default)]
    pub rounded: bool,
    #[serde(default)]
    pub secondary_text: Option<String>,
    #[serde(default)]
    pub move_reference: bool,
}

impl Default for CaptionStyle {
    fn default() -> Self {
        Self {
            font_family: "Noto Sans".to_string(),
            font_weight: default_font_weight(),
            font_size: default_font_size(),
            text_color: "#ffffff".to_string(),
            background_color: "#131313".to_string(),
            horizontal_align: HorizontalAlign::default(),
            vertical_align: VerticalAlign::default(),
            line_height: default_line_height(),
            padding: 48,
            max_text_width: None,
            border: false,
            rounded: false,
            secondary_text: None,
            move_reference: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardTimelineFrame {
    pub duration_ms: u64,
    pub fen: String,
    pub last_move: Option<String>,
    pub check: Option<String>,
    pub glyph: Option<String>,
    pub clock: Option<ClockData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClockData {
    pub white: Option<u32>,
    pub black: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptionTimelineFrame {
    pub duration_ms: u64,
    pub text: String,
    pub secondary_text: Option<String>,
    pub style: CaptionStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaTimelineFrame {
    pub duration_ms: u64,
    pub data_url: String,
    #[serde(default = "default_media_scale")]
    pub scale: f32,
    #[serde(default)]
    pub offset_x: f32,
    #[serde(default)]
    pub offset_y: f32,
}

fn default_media_scale() -> f32 { 1.0 }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TimelineFrame {
    Board(BoardTimelineFrame),
    Caption(CaptionTimelineFrame),
    Media(MediaTimelineFrame),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeRequest {
    pub white: Option<String>,
    pub black: Option<String>,
    #[serde(default)]
    pub orientation: Orientation,
    #[serde(default)]
    pub theme: BoardTheme,
    #[serde(default)]
    pub piece: PieceSet,
    #[serde(default)]
    pub coordinates: Coordinates,
    pub timeline: Vec<TimelineFrame>,
    #[serde(default)]
    pub include_player_bars: bool,
    #[serde(default)]
    pub preview: bool,
    #[serde(default)]
    pub width: Option<u16>,
    #[serde(default)]
    pub height: Option<u16>,
}

impl ComposeRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.timeline.is_empty() {
            return Err("timeline must contain at least one frame".to_string());
        }
        let total_duration: u64 = self
            .timeline
            .iter()
            .map(|frame| match frame {
                TimelineFrame::Board(board) => board.duration_ms,
                TimelineFrame::Caption(caption) => caption.duration_ms,
                TimelineFrame::Media(media) => media.duration_ms,
            })
            .sum();
        if total_duration > 300_000 {
            return Err("total animation duration exceeds 5 minutes".to_string());
        }
        for frame in &self.timeline {
            let duration_ms = match frame {
                TimelineFrame::Board(board) => board.duration_ms,
                TimelineFrame::Caption(caption) => caption.duration_ms,
                TimelineFrame::Media(media) => media.duration_ms,
            };
            if duration_ms < 20 || duration_ms > 30_000 {
                return Err("frame durations must stay between 20ms and 30s".to_string());
            }
        }
        if self.timeline.len() > 200 {
            return Err("timeline exceeds the maximum frame count".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseRequest {
    pub pgn: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedMove {
    pub ply: usize,
    pub move_number: usize,
    pub side: String,
    pub san: String,
    pub uci: String,
    pub fen_before: String,
    pub fen_after: String,
    pub is_check: bool,
    pub is_mate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedGame {
    pub metadata: serde_json::Value,
    pub initial_fen: String,
    pub moves: Vec<ParsedMove>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LichessImportRequest {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LichessImportResponse {
    pub pgn: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatestGameRequest {
    pub site: String,
    pub username: String,
}

pub fn parse_pgn(pgn: &str) -> Result<ParsedGame, String> {
    let mut metadata = serde_json::Map::new();
    let mut lines = pgn.lines();
    let mut move_text = String::new();

    while let Some(line) = lines.next() {
        if line.trim().starts_with('[') {
            if let Some((key, value)) = parse_tag(line) {
                metadata.insert(key, serde_json::Value::String(value));
            }
        } else if !line.trim().is_empty() {
            move_text.push_str(line.trim());
            move_text.push(' ');
        }
    }

    let mut chess = Chess::default();
    let mut moves = Vec::new();
    let mut ply = 0usize;
    for raw_token in tokenize_pgn(&move_text) {
        let token = strip_move_number(&raw_token);
        if token.is_empty() {
            continue;
        }
        if token == "." || token == "..." || token == "1-0" || token == "0-1" || token == "1/2-1/2" || token == "*" {
            continue;
        }
        let bytes = token.as_bytes();
        if token.starts_with('(') || token.ends_with(')') {
            continue;
        }
        if token.starts_with('{') || token.starts_with(';') {
            continue;
        }
        let (san_plus, prefix) = match SanPlus::from_ascii_prefix(bytes) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let san_text = std::str::from_utf8(&bytes[..prefix]).unwrap_or(token);
        let m = match san_plus.san.to_move(&chess) {
            Ok(m) => m,
            Err(_) => continue,
        };
        ply += 1;
        let move_number = ply.div_ceil(2);
        let side = if ply % 2 == 1 { "white" } else { "black" };
        let uci = UciMove::from_move(m, CastlingMode::Standard).to_string();
        let fen_before = Fen::from_position(&chess, EnPassantMode::Always).to_string();
        let fen_after = {
            chess.play_unchecked(m);
            Fen::from_position(&chess, EnPassantMode::Always).to_string()
        };
        let is_check = chess.is_check();
        let is_mate = chess.is_checkmate();
        moves.push(ParsedMove {
            ply,
            move_number,
            side: side.to_string(),
            san: san_text.to_string(),
            uci,
            fen_before: fen_before.clone(),
            fen_after: fen_after.clone(),
            is_check,
            is_mate,
        });
        let _ = fen_after;
    }

    let initial_fen = Fen::from_position(&Chess::default(), EnPassantMode::Always).to_string();
    Ok(ParsedGame {
        metadata: serde_json::Value::Object(metadata),
        initial_fen,
        moves,
    })
}

fn strip_move_number(token: &str) -> &str {
    let Some(dot) = token.rfind('.') else {
        return token;
    };
    let prefix = &token[..=dot];
    if prefix.chars().all(|ch| ch.is_ascii_digit() || ch == '.') {
        &token[dot + 1..]
    } else {
        token
    }
}

fn parse_tag(line: &str) -> Option<(String, String)> {
    let start = line.find('[')?;
    let end = line.rfind(']')?;
    let content = &line[start + 1..end];
    let mut parts = content.splitn(2, ' ');
    let key = parts.next()?.trim().to_string();
    let value = parts.next()?.trim().trim_matches('"').to_string();
    Some((key, value))
}

fn tokenize_pgn(move_text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for ch in move_text.chars() {
        match ch {
            '(' => {
                depth += 1;
                if !current.trim().is_empty() {
                    out.push(current.trim().to_string());
                    current.clear();
                }
            }
            ')' => {
                depth = depth.saturating_sub(1);
                if !current.trim().is_empty() {
                    out.push(current.trim().to_string());
                    current.clear();
                }
            }
            ';' => {
                if !current.trim().is_empty() {
                    out.push(current.trim().to_string());
                    current.clear();
                }
                current.push(ch);
            }
            '{' => {
                if !current.trim().is_empty() {
                    out.push(current.trim().to_string());
                    current.clear();
                }
                current.push(ch);
            }
            '}' => {
                current.push(ch);
                if !current.trim().is_empty() {
                    out.push(current.trim().to_string());
                    current.clear();
                }
            }
            ' ' | '\n' | '\t' => {
                if !current.trim().is_empty() {
                    out.push(current.trim().to_string());
                    current.clear();
                }
            }
            _ if depth > 0 => {}
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_simple_pgn() {
        let pgn = r#"[White "Alice"]
[Black "Bob"]

1. e4 e5 2. Nf3 Nc6"#;
        let parsed = parse_pgn(pgn).expect("parse game");
        assert_eq!(parsed.moves.len(), 4);
        assert_eq!(parsed.metadata["White"].as_str(), Some("Alice"));
        assert_eq!(parsed.moves[0].san, "e4");
        assert_eq!(parsed.moves[0].uci, "e2e4");
    }

    #[test]
    fn parses_move_numbers_attached_to_san() {
        let parsed = parse_pgn("1.e4 e5 2.Nf3 Nc6").expect("parse compact game");
        assert_eq!(parsed.moves.len(), 4);
        assert_eq!(parsed.moves[2].uci, "g1f3");
    }

    #[test]
    fn validates_duration_bounds() {
        let req = ComposeRequest {
            timeline: vec![TimelineFrame::Caption(CaptionTimelineFrame {
                duration_ms: 10,
                text: "hi".to_string(),
                secondary_text: None,
                style: CaptionStyle::default(),
            })],
            white: None,
            black: None,
            orientation: crate::api::Orientation::default(),
            theme: crate::assets::BoardTheme::default(),
            piece: crate::assets::PieceSet::default(),
            coordinates: crate::api::Coordinates::default(),
            include_player_bars: false,
            preview: false,
            width: None,
            height: None,
        };
        assert!(req.validate().is_err());
    }
}
