use crate::sanitize::sanitize_text;
use serde::{Deserialize, Serialize};

pub const BOLD: u8 = 1;
pub const DIM: u8 = 2;
pub const ITALIC: u8 = 4;
pub const UNDERLINE: u8 = 8;
pub const INVERSE: u8 = 16;

const MAX_ROWS: u16 = 200;
const MAX_COLS: u16 = 500;
const MAX_CSI_LEN: usize = 16;

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

#[derive(Debug, Default, PartialEq, Eq)]
pub struct FeedResult {
    pub replies: Vec<Vec<u8>>,
    pub bell: bool,
    pub notify: bool,
}

#[derive(Default)]
struct Signals {
    bell: bool,
    notify: bool,
}

impl vt100::Callbacks for Signals {
    fn audible_bell(&mut self, _: &mut vt100::Screen) {
        self.bell = true;
    }

    fn unhandled_osc(&mut self, _: &mut vt100::Screen, params: &[&[u8]]) {
        if params.first() == Some(&&b"9"[..]) {
            self.notify = true;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Query {
    Dsr,
    Status,
    Da1,
    Da2,
}

#[derive(Default)]
enum ScanState {
    #[default]
    Ground,
    Esc,
    Csi(Vec<u8>),
    Str {
        osc: bool,
    },
}

#[derive(Default)]
struct Scanner {
    state: ScanState,
}

impl Scanner {
    fn scan(&mut self, bytes: &[u8]) -> Vec<(usize, Query)> {
        let mut found = Vec::new();
        for (i, &b) in bytes.iter().enumerate() {
            let state = std::mem::take(&mut self.state);
            self.state = match state {
                ScanState::Ground => {
                    if b == 0x1b {
                        ScanState::Esc
                    } else {
                        ScanState::Ground
                    }
                }
                ScanState::Esc => match b {
                    b'[' => ScanState::Csi(Vec::new()),
                    b']' => ScanState::Str { osc: true },
                    b'P' | b'X' | b'^' | b'_' => ScanState::Str { osc: false },
                    0x1b => ScanState::Esc,
                    0x18 | 0x1a => ScanState::Ground,
                    _ => ScanState::Ground,
                },
                ScanState::Csi(mut buf) => match b {
                    0x1b => ScanState::Esc,
                    0x18 | 0x1a => ScanState::Ground,
                    0x20..=0x3f => {
                        if buf.len() < MAX_CSI_LEN {
                            buf.push(b);
                        }
                        ScanState::Csi(buf)
                    }
                    0x40..=0x7e => {
                        if let Some(q) = classify(&buf, b) {
                            found.push((i, q));
                        }
                        ScanState::Ground
                    }
                    _ => ScanState::Csi(buf),
                },
                ScanState::Str { osc } => match b {
                    0x1b => ScanState::Esc,
                    0x18 | 0x1a => ScanState::Ground,
                    0x07 if osc => ScanState::Ground,
                    _ => ScanState::Str { osc },
                },
            };
        }
        found
    }
}

fn classify(params: &[u8], fin: u8) -> Option<Query> {
    match (params, fin) {
        (b"6", b'n') => Some(Query::Dsr),
        (b"5", b'n') => Some(Query::Status),
        (b"" | b"0", b'c') => Some(Query::Da1),
        (b">" | b">0", b'c') => Some(Query::Da2),
        _ => None,
    }
}

pub struct ScreenModel {
    parser: vt100::Parser<Signals>,
    scanner: Scanner,
}

fn clamp_dims(rows: u16, cols: u16) -> (u16, u16) {
    (rows.clamp(1, MAX_ROWS), cols.clamp(1, MAX_COLS))
}

impl ScreenModel {
    pub fn new(rows: u16, cols: u16, scrollback: usize) -> Self {
        let (rows, cols) = clamp_dims(rows, cols);
        Self {
            parser: vt100::Parser::new_with_callbacks(rows, cols, scrollback, Signals::default()),
            scanner: Scanner::default(),
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> FeedResult {
        let mut result = FeedResult::default();
        let mut start = 0;
        for (end, query) in self.scanner.scan(bytes) {
            self.parser.process(&bytes[start..=end]);
            start = end + 1;
            let (row, col) = self.parser.screen().cursor_position();
            result.replies.push(match query {
                Query::Dsr => format!("\x1b[{};{}R", row + 1, col + 1).into_bytes(),
                Query::Status => b"\x1b[0n".to_vec(),
                Query::Da1 => b"\x1b[?62;22c".to_vec(),
                Query::Da2 => b"\x1b[>1;10;0c".to_vec(),
            });
        }
        self.parser.process(&bytes[start..]);
        let signals = self.parser.callbacks_mut();
        result.bell = std::mem::take(&mut signals.bell);
        result.notify = std::mem::take(&mut signals.notify);
        result
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        let (rows, cols) = clamp_dims(rows, cols);
        self.parser.screen_mut().set_size(rows, cols);
    }

    pub fn cursor_position(&self) -> (u16, u16) {
        self.parser.screen().cursor_position()
    }

    pub fn snapshot(&mut self, scroll_offset: usize) -> Snapshot {
        let screen = self.parser.screen_mut();
        screen.set_scrollback(usize::MAX);
        let scroll_max = screen.scrollback();
        let offset = scroll_offset.min(scroll_max);
        screen.set_scrollback(offset);
        let (rows, cols) = screen.size();
        let lines = (0..rows).map(|r| build_line(screen, r, cols)).collect();
        let modes = Modes {
            app_cursor: screen.application_cursor(),
            bracketed_paste: screen.bracketed_paste(),
            alt_screen: screen.alternate_screen(),
            hide_cursor: screen.hide_cursor(),
        };
        let cursor = (offset == 0 && !modes.hide_cursor).then(|| screen.cursor_position());
        screen.set_scrollback(0);
        Snapshot {
            rows,
            cols,
            lines,
            cursor,
            modes,
            scroll_offset: offset,
            scroll_max,
        }
    }

    pub fn bottom_text(&self, n: usize) -> Vec<String> {
        let screen = self.parser.screen();
        let (_, cols) = screen.size();
        let mut lines: Vec<String> = screen
            .rows(0, cols)
            .map(|l| sanitize_text(l.trim_end()).into_owned())
            .collect();
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        let skip = lines.len().saturating_sub(n);
        lines.split_off(skip)
    }
}

fn convert_color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Default,
        vt100::Color::Idx(i) => Color::Idx(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn build_line(screen: &vt100::Screen, row: u16, cols: u16) -> Line {
    let mut spans: Vec<Span> = Vec::new();
    for col in 0..cols {
        let Some(cell) = screen.cell(row, col) else {
            continue;
        };
        if cell.is_wide_continuation() {
            continue;
        }
        let mut flags = 0;
        for (on, bit) in [
            (cell.bold(), BOLD),
            (cell.dim(), DIM),
            (cell.italic(), ITALIC),
            (cell.underline(), UNDERLINE),
            (cell.inverse(), INVERSE),
        ] {
            if on {
                flags |= bit;
            }
        }
        let attr = Attr {
            fg: convert_color(cell.fgcolor()),
            bg: convert_color(cell.bgcolor()),
            flags,
        };
        let text = if cell.has_contents() {
            sanitize_text(cell.contents())
        } else {
            " ".into()
        };
        match spans.last_mut() {
            Some(last) if last.a == attr => last.t.push_str(&text),
            _ => spans.push(Span {
                a: attr,
                t: text.into_owned(),
            }),
        }
    }
    Line { spans }
}

pub fn changed_lines(prev: &Snapshot, next: &Snapshot) -> Vec<usize> {
    if prev.rows != next.rows || prev.cols != next.cols || prev.lines.len() != next.lines.len() {
        return (0..next.lines.len()).collect();
    }
    (0..next.lines.len())
        .filter(|&i| prev.lines[i] != next.lines[i])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span_width(s: &Span) -> usize {
        unicode_width::UnicodeWidthStr::width(s.t.as_str())
    }

    fn model() -> ScreenModel {
        ScreenModel::new(10, 40, 5000)
    }

    fn row_text(snap: &Snapshot, r: usize) -> String {
        snap.lines[r].spans.iter().map(|s| s.t.as_str()).collect()
    }

    fn assert_clean(snap: &Snapshot) {
        for line in &snap.lines {
            for span in &line.spans {
                for c in span.t.chars() {
                    assert!(
                        !matches!(c, '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}' | '\u{feff}'),
                        "unsafe char {:?} in {:?}",
                        c,
                        span.t
                    );
                }
            }
        }
    }

    const ADVERSARIAL: &[u8] = b"hi\x1b]52;c;aGVsbG8=\x07 a\x1b]8;;http://x\x07link\x1b]8;;\x07 \x1b]0;title\x07\x1bPq#0;2;0;0;0\x1b\\ \xc2\x9b31m raw\x9b31m \xe2\x80\xaeevil\xe2\x80\xac e\xcc\x81\x01 \x1b[99999999999;99999999999H end";

    #[test]
    fn plain_text() {
        let mut m = model();
        m.feed(b"hello\r\nworld");
        let s = m.snapshot(0);
        assert_eq!(row_text(&s, 0).trim_end(), "hello");
        assert_eq!(row_text(&s, 1).trim_end(), "world");
        assert_eq!(s.lines.len(), 10);
        assert_eq!(row_text(&s, 0).chars().count(), 40);
        assert_eq!(s.cursor, Some((1, 5)));
    }

    #[test]
    fn colours_16_256_truecolor() {
        let mut m = model();
        m.feed(b"\x1b[31ma\x1b[38;5;200mb\x1b[38;2;1;2;3mc\x1b[44md");
        let s = m.snapshot(0);
        let sp = &s.lines[0].spans;
        assert_eq!(sp[0].a.fg, Color::Idx(1));
        assert_eq!(sp[1].a.fg, Color::Idx(200));
        assert_eq!(sp[2].a.fg, Color::Rgb(1, 2, 3));
        assert_eq!(sp[3].a.bg, Color::Idx(4));
        assert_eq!(sp[3].t, "d");
    }

    #[test]
    fn attrs_merge_into_spans() {
        let mut m = model();
        m.feed(b"\x1b[1mab\x1b[7mcd\x1b[0mef");
        let s = m.snapshot(0);
        let sp = &s.lines[0].spans;
        assert_eq!(sp[0].t, "ab");
        assert_eq!(sp[0].a.flags, BOLD);
        assert_eq!(sp[1].t, "cd");
        assert_eq!(sp[1].a.flags, BOLD | INVERSE);
        assert_eq!(sp[2].a.flags, 0);
        assert!(sp[2].t.starts_with("ef "));
        assert_eq!(sp.len(), 3);
    }

    #[test]
    fn wide_chars_keep_cell_count() {
        let mut m = model();
        m.feed("世界x".as_bytes());
        let s = m.snapshot(0);
        let text = row_text(&s, 0);
        assert!(text.starts_with("世界x"));
        let width: usize = s.lines[0].spans.iter().map(span_width).sum();
        assert_eq!(width, 40);
    }

    #[test]
    fn adversarial_input_is_sanitized() {
        let mut m = model();
        m.feed(ADVERSARIAL);
        let s = m.snapshot(0);
        assert_clean(&s);
        let all: String = (0..10).map(|r| row_text(&s, r)).collect();
        assert!(!all.contains("aGVsbG8"));
        assert!(!all.contains("title"));
    }

    #[test]
    fn fragmentation_does_not_change_snapshot() {
        let mut whole = model();
        whole.feed(ADVERSARIAL);
        let mut bytewise = model();
        for b in ADVERSARIAL {
            bytewise.feed(&[*b]);
        }
        assert_eq!(whole.snapshot(0), bytewise.snapshot(0));
    }

    #[test]
    fn c1_csi_is_not_a_csi_for_the_parser() {
        let mut m = model();
        let r = m.feed(b"\x9b6n\xc2\x9b6n");
        assert!(r.replies.is_empty());
        assert_clean(&m.snapshot(0));
    }

    #[test]
    fn dsr_uses_cursor_after_preceding_bytes() {
        let mut m = model();
        let r = m.feed(b"\r\n\r\nabc\x1b[6n");
        assert_eq!(r.replies, vec![b"\x1b[3;4R".to_vec()]);
    }

    #[test]
    fn dsr_midstream_sees_only_earlier_bytes() {
        let mut m = model();
        let r = m.feed(b"ab\x1b[6ncdef\x1b[6n");
        assert_eq!(
            r.replies,
            vec![b"\x1b[1;3R".to_vec(), b"\x1b[1;7R".to_vec()]
        );
    }

    #[test]
    fn split_query_answered_once() {
        let mut m = model();
        let a = m.feed(b"x\x1b[");
        let b = m.feed(b"6n");
        assert!(a.replies.is_empty());
        assert_eq!(b.replies, vec![b"\x1b[1;2R".to_vec()]);
    }

    #[test]
    fn device_queries() {
        let mut m = model();
        assert_eq!(m.feed(b"\x1b[5n").replies, vec![b"\x1b[0n".to_vec()]);
        assert_eq!(m.feed(b"\x1b[c").replies, vec![b"\x1b[?62;22c".to_vec()]);
        assert_eq!(m.feed(b"\x1b[0c").replies, vec![b"\x1b[?62;22c".to_vec()]);
        assert_eq!(m.feed(b"\x1b[>c").replies, vec![b"\x1b[>1;10;0c".to_vec()]);
        assert_eq!(m.feed(b"\x1b[>0c").replies, vec![b"\x1b[>1;10;0c".to_vec()]);
        assert!(m.feed(b"\x1b[7n\x1b[?6n\x1b[1c").replies.is_empty());
    }

    #[test]
    fn queries_inside_strings_are_ignored() {
        let mut m = model();
        assert!(m.feed(b"\x1b]0;x[6n\x07\x1bPx[6n\x1b\\").replies.is_empty());
        let mut m = model();
        let r = m.feed(b"\x1bP1;2\x1b\\\x1b[6n");
        assert_eq!(r.replies.len(), 1);
    }

    #[test]
    fn osc_color_query_gets_no_reply() {
        let mut m = model();
        let r = m.feed(b"\x1b]11;?\x07\x1b]10;?\x1b\\");
        assert!(r.replies.is_empty());
        assert!(!r.bell);
    }

    #[test]
    fn bell_and_notify_flags() {
        let mut m = model();
        assert!(!m.feed(b"\x1b]0;t\x07").bell);
        assert!(m.feed(b"x\x07").bell);
        assert!(!m.feed(b"y").bell);
        let r = m.feed(b"\x1b]9;done\x07");
        assert!(r.notify && !r.bell);
        let r = m.feed(b"\x1b]9;done\x1b\\");
        assert!(r.notify && !r.bell);
        let mut m = model();
        m.feed(b"\x1b]9;sp");
        assert!(m.feed(b"lit\x07").notify);
    }

    #[test]
    fn scrollback_snapshot() {
        let mut m = ScreenModel::new(10, 40, 5000);
        for i in 0..100 {
            m.feed(format!("line {i}\r\n").as_bytes());
        }
        let live = m.snapshot(0);
        let back = m.snapshot(20);
        assert!(back.scroll_max >= 90);
        assert_eq!(back.scroll_offset, 20);
        assert_eq!(back.cursor, None);
        assert_ne!(row_text(&back, 0), row_text(&live, 0));
        assert!(
            row_text(&back, 0).starts_with("line 7"),
            "{}",
            row_text(&back, 0)
        );
        assert_eq!(m.snapshot(0), live);
        assert_eq!(m.snapshot(1_000_000).scroll_offset, back.scroll_max);
    }

    #[test]
    fn modes_from_parser() {
        let mut m = model();
        assert_eq!(
            m.snapshot(0).modes,
            Modes {
                app_cursor: false,
                bracketed_paste: false,
                alt_screen: false,
                hide_cursor: false
            }
        );
        m.feed(b"\x1b[?1049h\x1b[?1h\x1b[?2004h\x1b[?25l");
        let s = m.snapshot(0);
        assert_eq!(
            s.modes,
            Modes {
                app_cursor: true,
                bracketed_paste: true,
                alt_screen: true,
                hide_cursor: true
            }
        );
        assert_eq!(s.cursor, None);
    }

    #[test]
    fn resize_clamps() {
        let mut m = ScreenModel::new(0, 0, 10);
        let s = m.snapshot(0);
        assert_eq!((s.rows, s.cols), (1, 1));
        m.resize(9999, 9999);
        let s = m.snapshot(0);
        assert_eq!((s.rows, s.cols), (200, 500));
        m.resize(0, 0);
        assert_eq!((m.snapshot(0).rows, m.snapshot(0).cols), (1, 1));
        let big = ScreenModel::new(1000, 1000, 0).snapshot(0);
        assert_eq!((big.rows, big.cols), (200, 500));
    }

    #[test]
    fn huge_params_do_not_panic() {
        let mut m = model();
        m.feed(b"\x1b[99999999999;99999999999H\x1b[99999999999A\x1b[99999999999@x\x1b[99999999999;99999999999r");
        m.snapshot(0);
        m.resize(3, 3);
        m.feed(b"\x1b[99999999999;1H\x1b[6n");
        m.snapshot(0);
    }

    #[test]
    fn changed_lines_diff() {
        let mut m = model();
        m.feed(b"one\r\ntwo");
        let a = m.snapshot(0);
        m.feed(b"\x1b[2;1Hxyz");
        let b = m.snapshot(0);
        assert_eq!(changed_lines(&a, &b), vec![1]);
        m.resize(5, 40);
        let c = m.snapshot(0);
        assert_eq!(changed_lines(&b, &c), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn bottom_text_trims_and_sanitizes() {
        let mut m = model();
        m.feed("a   \r\n\r\nb\u{202e}c  \r\nlast".as_bytes());
        assert_eq!(
            m.bottom_text(2),
            vec!["b?c".to_string(), "last".to_string()]
        );
        assert_eq!(m.bottom_text(100).len(), 4);
    }

    #[test]
    fn compact_serde_roundtrip() {
        let mut m = model();
        m.feed(b"\x1b[31mhi\x1b[38;2;1;2;3mx");
        let s = m.snapshot(0);
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"f\":1"), "{json}");
        assert!(json.contains("\"f\":[1,2,3]"), "{json}");
        assert!(json.contains("\"b\":null"), "{json}");
        let back: Snapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }
}
