use serde::{Deserialize, Serialize};

pub const BOLD: u8 = 1;
pub const DIM: u8 = 2;
pub const ITALIC: u8 = 4;
pub const UNDERLINE: u8 = 8;
pub const INVERSE: u8 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Color {
    Default,
    Idx(u8),
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attr {
    #[serde(rename = "f")]
    pub fg: Color,
    #[serde(rename = "b")]
    pub bg: Color,
    #[serde(rename = "x")]
    pub flags: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub a: Attr,
    pub t: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Line {
    pub spans: Vec<Span>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Modes {
    #[serde(rename = "ac")]
    pub app_cursor: bool,
    #[serde(rename = "bp")]
    pub bracketed_paste: bool,
    #[serde(rename = "alt")]
    pub alt_screen: bool,
    #[serde(rename = "hc")]
    pub hide_cursor: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(rename = "r")]
    pub rows: u16,
    #[serde(rename = "c")]
    pub cols: u16,
    #[serde(rename = "l")]
    pub lines: Vec<Line>,
    #[serde(rename = "cu")]
    pub cursor: Option<(u16, u16)>,
    #[serde(rename = "m")]
    pub modes: Modes,
    #[serde(rename = "so")]
    pub scroll_offset: usize,
    #[serde(rename = "sm")]
    pub scroll_max: usize,
}
