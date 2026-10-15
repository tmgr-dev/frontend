use crate::sanitize::sanitize_text;
use tmgr_ipc::api::{
    Attr, Color, Line, Modes, Snapshot, Span, BOLD, DIM, INVERSE, ITALIC, UNDERLINE,
};

const MAX_ROWS: u16 = 200;
const MAX_COLS: u16 = 500;
const MAX_CSI_LEN: usize = 16;
const MAX_STRING: usize = 4096;

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum StrKind {
    Osc,
    Dcs,
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DcsSub {
    Entry,
    Param,
    Inter,
    Ignore,
}

#[derive(Default)]
enum ScanState {
    #[default]
    Ground,
    Esc,
    EscInter,
    Csi(Vec<u8>),
    DcsHeader(DcsSub),
    Str {
        kind: StrKind,
        len: usize,
    },
}

#[derive(Default)]
struct Scanner {
    state: ScanState,
    dropped: u64,
}

impl Scanner {
    fn scan(&mut self, bytes: &[u8]) -> (Vec<u8>, Vec<(usize, Query)>) {
        let mut found = Vec::new();
        let mut kept = Vec::with_capacity(bytes.len());
        for &b in bytes {
            let state = std::mem::take(&mut self.state);
            let mut drop = false;
            self.state = match state {
                ScanState::Ground => {
                    if b == 0x1b {
                        ScanState::Esc
                    } else {
                        ScanState::Ground
                    }
                }
                ScanState::Esc => match b {
                    0x18 | 0x1a => ScanState::Ground,
                    0x20..=0x2f => ScanState::EscInter,
                    b'[' => ScanState::Csi(Vec::new()),
                    b']' => ScanState::Str {
                        kind: StrKind::Osc,
                        len: 0,
                    },
                    b'P' => ScanState::DcsHeader(DcsSub::Entry),
                    b'X' | b'^' | b'_' => ScanState::Str {
                        kind: StrKind::Other,
                        len: 0,
                    },
                    0x30..=0x7e => ScanState::Ground,
                    _ => ScanState::Esc,
                },
                ScanState::EscInter => match b {
                    0x1b => ScanState::Esc,
                    0x18 | 0x1a | 0x30..=0x7e => ScanState::Ground,
                    _ => ScanState::EscInter,
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
                            found.push((kept.len(), q));
                        }
                        ScanState::Ground
                    }
                    _ => ScanState::Csi(buf),
                },
                ScanState::DcsHeader(sub) => match (b, sub) {
                    (0x1b, _) => ScanState::Esc,
                    (0x18 | 0x1a, _) => ScanState::Ground,
                    (0x40..=0x7e, DcsSub::Ignore) => ScanState::DcsHeader(sub),
                    (0x40..=0x7e, _) => ScanState::Str {
                        kind: StrKind::Dcs,
                        len: 0,
                    },
                    (0x20..=0x2f, DcsSub::Ignore) => ScanState::DcsHeader(sub),
                    (0x20..=0x2f, _) => ScanState::DcsHeader(DcsSub::Inter),
                    (0x30..=0x3b, DcsSub::Entry | DcsSub::Param) => {
                        ScanState::DcsHeader(DcsSub::Param)
                    }
                    (0x3c..=0x3f, DcsSub::Entry) => ScanState::DcsHeader(DcsSub::Param),
                    (0x30..=0x3f, DcsSub::Param | DcsSub::Inter) => {
                        ScanState::DcsHeader(DcsSub::Ignore)
                    }
                    _ => ScanState::DcsHeader(sub),
                },
                ScanState::Str { kind, len } => match b {
                    0x1b => ScanState::Esc,
                    0x18 | 0x1a => ScanState::Ground,
                    0x07 if kind == StrKind::Osc => ScanState::Ground,
                    0x9c if kind == StrKind::Dcs => ScanState::Ground,
                    _ if len >= MAX_STRING => {
                        drop = true;
                        ScanState::Str { kind, len }
                    }
                    _ => ScanState::Str { kind, len: len + 1 },
                },
            };
            if drop {
                self.dropped += 1;
            } else {
                kept.push(b);
            }
        }
        (kept, found)
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
        let (bytes, queries) = self.scanner.scan(bytes);
        let mut start = 0;
        for (end, query) in queries {
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

    pub fn dropped_string_bytes(&self) -> u64 {
        self.scanner.dropped
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

    pub fn text_lines(&mut self, n: Option<usize>) -> (u16, u16, u32, Vec<String>) {
        let screen = self.parser.screen_mut();
        screen.set_scrollback(usize::MAX);
        let scroll_max = screen.scrollback();
        let (rows, cols) = screen.size();
        let n = n.unwrap_or(rows as usize);
        let mut offset = n.saturating_sub(rows as usize).min(scroll_max);
        let keep = n.min(rows as usize);
        let mut lines = Vec::new();
        while offset > 0 {
            let take = offset.min(rows as usize);
            screen.set_scrollback(offset);
            lines.extend(row_texts(screen, cols).take(take));
            offset -= take;
        }
        screen.set_scrollback(0);
        let bottom: Vec<String> = row_texts(screen, cols).collect();
        lines.extend(bottom[bottom.len() - keep..].iter().cloned());
        (rows, cols, scroll_max as u32, lines)
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

fn row_texts(screen: &vt100::Screen, cols: u16) -> impl Iterator<Item = String> + '_ {
    screen
        .rows(0, cols)
        .map(|l| sanitize_text(l.trim_end()).into_owned())
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

    fn feed_endless(m: &mut ScreenModel, head: &[u8], term: &[u8]) {
        let chunk = vec![b'a'; 64 * 1024];
        m.feed(head);
        for _ in 0..(50 * 16) {
            m.feed(&chunk);
        }
        m.feed(term);
        m.feed(b"hello");
    }

    fn assert_hello_and_bounded(m: &mut ScreenModel) {
        let s = m.snapshot(0);
        assert_eq!(row_text(&s, 0).trim_end(), "hello");
        let dropped = m.dropped_string_bytes();
        assert!(dropped >= 50 * 1024 * 1024 - MAX_STRING as u64, "{dropped}");
    }

    #[test]
    fn endless_osc_is_cut_and_terminated_by_bel() {
        let mut m = model();
        feed_endless(&mut m, b"\x1b]0;", b"\x07");
        assert_hello_and_bounded(&mut m);
    }

    #[test]
    fn endless_osc_is_cut_and_terminated_by_st() {
        let mut m = model();
        feed_endless(&mut m, b"\x1b]0;", b"\x1b\\");
        assert_hello_and_bounded(&mut m);
    }

    #[test]
    fn endless_dcs_is_cut_and_terminated_by_st() {
        let mut m = model();
        feed_endless(&mut m, b"\x1bPq", b"\x1b\\");
        assert_hello_and_bounded(&mut m);
    }

    #[test]
    fn endless_apc_is_cut_and_terminated_by_st() {
        let mut m = model();
        feed_endless(&mut m, b"\x1b_G", b"\x1b\\");
        assert_hello_and_bounded(&mut m);
    }

    #[test]
    fn endless_dcs_is_terminated_by_raw_9c() {
        let mut m = model();
        feed_endless(&mut m, b"\x1bPq", b"\x9c");
        assert_hello_and_bounded(&mut m);
    }

    #[test]
    fn short_strings_are_untouched_and_long_ones_do_not_swallow_the_next() {
        let mut m = model();
        let mut long = b"\x1b]9;".to_vec();
        long.extend(std::iter::repeat_n(b'a', MAX_STRING * 2));
        long.push(0x07);
        assert!(m.feed(&long).notify);
        assert_eq!(m.dropped_string_bytes(), MAX_STRING as u64 + 2);
        let r = m.feed(b"\x1b]9;hi\x07");
        assert!(r.notify);
        assert!(m.feed(b"\x1b[6n").replies.len() == 1);
    }

    #[test]
    fn endless_osc_after_esc_c0_is_cut() {
        let mut m = model();
        feed_endless(&mut m, b"\x1b\x00]0;", b"\x07");
        assert_hello_and_bounded(&mut m);
    }

    #[test]
    fn endless_osc_after_double_esc_is_cut() {
        let mut m = model();
        feed_endless(&mut m, b"\x1b\x1b]0;", b"\x07");
        assert_hello_and_bounded(&mut m);
    }

    #[test]
    fn endless_dcs_with_c0_in_entry_is_cut() {
        let mut m = model();
        feed_endless(&mut m, b"\x1bP\x00\x01q", b"\x1b\\");
        assert_hello_and_bounded(&mut m);
    }

    #[test]
    fn can_and_sub_cancel_a_pending_escape() {
        for cancel in [0x18u8, 0x1a] {
            let mut m = model();
            let mut head = vec![0x1b, cancel];
            head.extend_from_slice(b"]0;");
            m.feed(&head);
            m.feed(&vec![b'a'; MAX_STRING * 4]);
            assert_eq!(m.dropped_string_bytes(), 0);
        }
    }

    #[test]
    fn can_ends_a_string_and_following_osc_is_cut_again() {
        let mut m = model();
        let mut v = b"\x1b]0;x\x18".to_vec();
        v.extend_from_slice(b"\x1b]0;");
        v.extend(std::iter::repeat_n(b'a', MAX_STRING * 4));
        m.feed(&v);
        assert!(m.dropped_string_bytes() >= (MAX_STRING * 3) as u64);
    }

    #[derive(Default)]
    struct Recorder {
        osc_bytes: usize,
        puts: usize,
        max_in_string: usize,
        queries: usize,
    }

    impl vte::Perform for Recorder {
        fn osc_dispatch(&mut self, params: &[&[u8]], _: bool) {
            let n: usize = params.iter().map(|p| p.len()).sum();
            self.osc_bytes = self.osc_bytes.max(n);
            self.max_in_string = self.max_in_string.max(n);
        }
        fn hook(&mut self, _: &vte::Params, _: &[u8], _: bool, _: char) {
            self.puts = 0;
        }
        fn put(&mut self, _: u8) {
            self.puts += 1;
            self.max_in_string = self.max_in_string.max(self.puts);
        }
        fn csi_dispatch(&mut self, params: &vte::Params, inter: &[u8], ignore: bool, action: char) {
            let p: Vec<Vec<u16>> = params.iter().map(|g| g.to_vec()).collect();
            let hit = !ignore
                && (matches!(
                    (inter, p.as_slice(), action),
                    ([], [x], 'n') if x == &[6] || x == &[5]
                ) || matches!((inter, p.as_slice(), action), ([] | [b'>'], [x], 'c') if x == &[0]));
            if hit {
                self.queries += 1;
            }
        }
    }

    struct Xorshift(u64);

    impl Xorshift {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    #[test]
    fn filter_bounds_strings_as_seen_by_an_independent_vte() {
        const DANGEROUS: &[u8] = b"\x1b[]PX^_\\\x07\x00\x18\x1a\x9c\x90 0123;a\x7f!?";
        let mut rng = Xorshift(0x9e37_79b9_7f4a_7c15);
        let run = vec![b'a'; 1024 * 1024];
        for _ in 0..300 {
            let n = 1 + (rng.next() % 10) as usize;
            let mut head: Vec<u8> = (0..n)
                .map(|_| DANGEROUS[(rng.next() % DANGEROUS.len() as u64) as usize])
                .collect();
            head.extend_from_slice(&run);
            head.extend_from_slice(b"\x07\x1b\\");
            let mut scanner = Scanner::default();
            let (kept, _) = scanner.scan(&head);
            let mut rec = Recorder::default();
            let mut parser = vte::Parser::new();
            parser.advance(&mut rec, &kept);
            assert!(
                rec.max_in_string <= MAX_STRING + 16,
                "{:?}: {}",
                &head[..n],
                rec.max_in_string
            );
        }
    }

    #[test]
    fn query_detection_agrees_with_an_independent_vte() {
        const ALPHA: &[u8] = b"\x1b[]PX^_\\\x07\x00\x18\x1a\x9c\x90 0156;>?ncA\x7f";
        let mut rng = Xorshift(0x1234_5678_9abc_def1);
        for _ in 0..20000 {
            let n = 1 + (rng.next() % 24) as usize;
            let v: Vec<u8> = (0..n)
                .map(|_| ALPHA[(rng.next() % ALPHA.len() as u64) as usize])
                .collect();
            let (_, found) = Scanner::default().scan(&v);
            let mut rec = Recorder::default();
            vte::Parser::new().advance(&mut rec, &v);
            assert_eq!(found.len(), rec.queries, "{v:?}");
        }
    }
}
