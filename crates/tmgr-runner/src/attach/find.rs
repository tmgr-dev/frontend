use super::keys::ScrollKey;
use super::palette::{esc_key, next_char, query_tail};
use super::render::put;
use super::theme::colors;
use super::view::{ScrollState, ViewState};
use crate::sanitize::sanitize_text;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use tmgr_ipc::api::Snapshot;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const MAX_QUERY: usize = 200;
const MAX_HITS: usize = 5000;
const MAX_COPY: usize = 1 << 20;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScrollText {
    pub session: String,
    pub lines: Vec<String>,
    pub cols: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub l0: usize,
    pub c0: usize,
    pub l1: usize,
    pub c1: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchState {
    pub session: String,
    pub query: String,
    pub input: bool,
    pub hits: Vec<Span>,
    pub current: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CopyState {
    pub line: usize,
    pub col: usize,
    pub anchor: Option<(usize, usize)>,
    pub placed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchEdit {
    Editing,
    Keep,
    Cancel,
}

fn chars_of(line: &str) -> Vec<char> {
    line.chars().filter(|c| !c.is_control()).collect()
}

fn wrapped(line: &str, cols: usize) -> bool {
    cols > 0 && line.width() >= cols
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

pub fn find_hits(lines: &[String], cols: usize, query: &str) -> Vec<Span> {
    let needle: Vec<char> = query.chars().filter(|c| !c.is_control()).collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let sensitive = needle.iter().any(|c| c.is_uppercase());
    let key = |c: char| if sensitive { c } else { fold(c) };
    let needle: Vec<char> = needle.into_iter().map(key).collect();
    let mut stream: Vec<(char, Option<(usize, usize)>)> = Vec::new();
    for (l, line) in lines.iter().enumerate() {
        let cs = chars_of(line);
        for (c, ch) in cs.iter().enumerate() {
            stream.push((key(*ch), Some((l, c))));
        }
        if !wrapped(line, cols) {
            stream.push(('\n', None));
        }
    }
    let mut hits = Vec::new();
    let mut i = 0;
    while i + needle.len() <= stream.len() && hits.len() < MAX_HITS {
        let window = &stream[i..i + needle.len()];
        if window
            .iter()
            .zip(&needle)
            .all(|((c, p), n)| c == n && p.is_some())
        {
            let (l0, c0) = window[0].1.unwrap_or((0, 0));
            let (l1, c1) = window[needle.len() - 1].1.unwrap_or((0, 0));
            hits.push(Span {
                l0,
                c0,
                l1,
                c1: c1 + 1,
            });
            i += needle.len();
        } else {
            i += 1;
        }
    }
    hits
}

fn segments(span: &Span, lines: &[String]) -> Vec<(usize, usize, usize)> {
    (span.l0..=span.l1)
        .map(|l| {
            let len = lines.get(l).map_or(0, |s| chars_of(s).len());
            let a = if l == span.l0 { span.c0 } else { 0 };
            let b = if l == span.l1 { span.c1 } else { len };
            (l, a, b.min(len.max(a)))
        })
        .collect()
}

pub fn selection_text(
    lines: &[String],
    cols: usize,
    a: (usize, usize),
    b: (usize, usize),
) -> String {
    let (from, to) = if a <= b { (a, b) } else { (b, a) };
    let mut out = String::new();
    for (l, line) in lines.iter().enumerate().take(to.0 + 1).skip(from.0) {
        let cs = chars_of(line);
        let s = if l == from.0 { from.1 } else { 0 };
        let e = if l == to.0 {
            (to.1 + 1).min(cs.len())
        } else {
            cs.len()
        };
        if s < e {
            out.extend(&cs[s..e]);
        }
        if l != to.0 && !wrapped(line, cols) {
            out.push('\n');
        }
        if out.len() >= MAX_COPY {
            break;
        }
    }
    out
}

fn cell_x(line: &str, col: usize) -> u16 {
    chars_of(line)
        .iter()
        .take(col)
        .map(|c| c.width().unwrap_or(0))
        .sum::<usize>() as u16
}

impl ViewState {
    pub fn set_scroll_text(&mut self, text: ScrollText, rows: usize) {
        let session = text.session.clone();
        self.stext = Some(text);
        let keep = self
            .search
            .as_ref()
            .and_then(|s| s.current.and_then(|i| s.hits.get(i)).map(|h| (h.l0, h.c0)));
        if let Some(s) = self.search.as_mut().filter(|s| s.session == session) {
            if let Some(t) = &self.stext {
                s.hits = find_hits(&t.lines, t.cols, &s.query);
            }
            s.current = keep
                .and_then(|k| s.hits.iter().position(|h| (h.l0, h.c0) == k))
                .or_else(|| s.hits.len().checked_sub(1));
            if s.input && s.current.is_some() && keep.is_none() {
                self.reveal_current(rows);
            }
        }
        let mut placed = false;
        if let (Some(c), Some(t)) = (self.copy.as_mut(), self.stext.as_ref()) {
            if !c.placed {
                c.line = t.lines.len().saturating_sub(1);
                c.col = 0;
                c.placed = true;
                placed = true;
            }
            c.line = c.line.min(t.lines.len().saturating_sub(1));
        }
        if placed {
            self.follow_cursor(rows);
        }
    }

    pub fn open_search(&mut self) {
        let Some(session) = self.focused_session().map(str::to_string) else {
            return;
        };
        self.search = Some(SearchState {
            session,
            query: String::new(),
            input: true,
            hits: Vec::new(),
            current: None,
        });
        self.help = false;
    }

    pub fn clear_find(&mut self) {
        self.search = None;
        self.copy = None;
        self.stext = None;
        self.copied = None;
    }

    pub fn take_copied(&mut self) -> Option<String> {
        self.copied.take()
    }

    fn recompute(&mut self, rows: usize) {
        let Some(t) = self.stext.clone() else {
            return;
        };
        if let Some(s) = self.search.as_mut().filter(|s| s.session == t.session) {
            s.hits = find_hits(&t.lines, t.cols, &s.query);
            s.current = s.hits.len().checked_sub(1);
        }
        self.reveal_current(rows);
    }

    pub fn edit_search(&mut self, data: &[u8], rows: usize) -> SearchEdit {
        let Some(s) = self.search.as_mut() else {
            return SearchEdit::Cancel;
        };
        if !data.is_empty() && data.iter().all(|&b| b == 0x1b) {
            return SearchEdit::Cancel;
        }
        let mut changed = false;
        let mut i = 0;
        while i < data.len() {
            match data[i] {
                0x1b => {
                    let (_, used) = esc_key(&data[i..]);
                    i += used;
                }
                b'\r' | b'\n' => {
                    return if s.hits.is_empty() {
                        if self.stext.is_none() {
                            SearchEdit::Editing
                        } else {
                            SearchEdit::Cancel
                        }
                    } else {
                        s.input = false;
                        SearchEdit::Keep
                    };
                }
                0x7f | 0x08 => {
                    i += 1;
                    changed |= s.query.pop().is_some();
                }
                0x15 => {
                    i += 1;
                    changed |= !s.query.is_empty();
                    s.query.clear();
                }
                _ => {
                    let (c, used) = next_char(&data[i..]);
                    i += used;
                    if let Some(c) = c.filter(|c| !c.is_control()) {
                        if s.query.chars().count() < MAX_QUERY {
                            s.query.push(c);
                            changed = true;
                        }
                    }
                }
            }
        }
        if changed {
            self.recompute(rows);
        }
        SearchEdit::Editing
    }

    pub fn search_step(&mut self, older: bool, rows: usize) {
        let Some(s) = self.search.as_mut() else {
            self.notice = Some("no search yet, / starts one".into());
            return;
        };
        let n = s.hits.len();
        if n == 0 {
            self.notice = Some("no matches".into());
            return;
        }
        let at = s.current.unwrap_or(n - 1);
        s.current = Some(if older {
            (at + n - 1) % n
        } else {
            (at + 1) % n
        });
        self.reveal_current(rows);
    }

    fn reveal_current(&mut self, rows: usize) {
        let Some(len) = self.stext.as_ref().map(|t| t.lines.len()) else {
            return;
        };
        let Some(line) = self
            .search
            .as_ref()
            .and_then(|s| s.current.and_then(|i| s.hits.get(i)))
            .map(|h| h.l0)
        else {
            return;
        };
        self.reveal_line(len, line, rows);
    }

    fn reveal_line(&mut self, len: usize, line: usize, rows: usize) {
        let fb = len.saturating_sub(1).saturating_sub(line);
        let max = self.scroll.map_or(0, |s| s.max);
        let cur = self.scroll.map_or(0, |s| s.offset);
        let rows = rows.max(1);
        if fb >= cur && fb < cur + rows {
            return;
        }
        let offset = fb.saturating_sub(rows / 2).min(max);
        self.scroll = Some(ScrollState { offset, max });
    }

    fn follow_cursor(&mut self, rows: usize) {
        let (Some(len), Some(c)) = (
            self.stext.as_ref().map(|t| t.lines.len()),
            self.copy.as_ref(),
        ) else {
            return;
        };
        let line = c.line;
        let fb = len.saturating_sub(1).saturating_sub(line);
        let max = self.scroll.map_or(0, |s| s.max);
        let cur = self.scroll.map_or(0, |s| s.offset);
        let rows = rows.max(1);
        let offset = if fb < cur {
            fb
        } else if fb >= cur + rows {
            fb + 1 - rows
        } else {
            return;
        };
        self.scroll = Some(ScrollState {
            offset: offset.min(max),
            max,
        });
    }

    pub fn enter_copy(&mut self, select: bool, rows: usize) {
        let Some(t) = self.stext.as_ref() else {
            self.copy = Some(CopyState::default());
            return;
        };
        let len = t.lines.len();
        let offset = self.scroll.map_or(0, |s| s.offset);
        let _ = rows;
        let bottom = len.saturating_sub(1).saturating_sub(offset);
        let hit = self
            .search
            .as_ref()
            .and_then(|s| s.current.and_then(|i| s.hits.get(i)))
            .map(|h| (h.l0, h.c0));
        let mut c = CopyState {
            line: hit.map_or(bottom, |h| h.0),
            col: hit.map_or(0, |h| h.1),
            anchor: None,
            placed: true,
        };
        if select {
            c.anchor = Some((c.line, c.col));
        }
        self.copy = Some(c);
    }

    fn cursor_to_hit(&mut self, rows: usize) {
        let hit = self
            .search
            .as_ref()
            .and_then(|s| s.current.and_then(|i| s.hits.get(i)))
            .map(|h| (h.l0, h.c0));
        if let (Some(h), Some(c)) = (hit, self.copy.as_mut()) {
            c.line = h.0;
            c.col = h.1;
            self.follow_cursor(rows);
        }
    }

    fn line_len(&self, line: usize) -> usize {
        self.stext
            .as_ref()
            .and_then(|t| t.lines.get(line))
            .map_or(0, |s| chars_of(s).len())
    }

    pub fn copy_key(&mut self, key: ScrollKey, rows: usize) -> bool {
        let Some(len) = self.stext.as_ref().map(|t| t.lines.len()) else {
            return matches!(key, ScrollKey::Exit) && {
                self.clear_find();
                self.scroll = None;
                true
            };
        };
        let Some(mut c) = self.copy.clone() else {
            return false;
        };
        let last = len.saturating_sub(1);
        let page = rows.saturating_sub(1).max(1);
        match key {
            ScrollKey::Up => c.line = c.line.saturating_sub(1),
            ScrollKey::Down => c.line = (c.line + 1).min(last),
            ScrollKey::PageUp => c.line = c.line.saturating_sub(page),
            ScrollKey::PageDown => c.line = (c.line + page).min(last),
            ScrollKey::Top => c.line = 0,
            ScrollKey::Bottom => c.line = last,
            ScrollKey::Left => c.col = c.col.saturating_sub(1),
            ScrollKey::Right => c.col += 1,
            ScrollKey::LineStart => c.col = 0,
            ScrollKey::LineEnd => c.col = usize::MAX,
            ScrollKey::Select => {
                c.anchor = match c.anchor {
                    Some(_) => None,
                    None => Some((c.line, c.col.min(self.line_len(c.line).saturating_sub(1)))),
                };
            }
            ScrollKey::Yank => {
                let end = (c.line, c.col.min(self.line_len(c.line).saturating_sub(1)));
                let from = c.anchor.unwrap_or((end.0, 0));
                let to = if c.anchor.is_some() {
                    end
                } else {
                    (end.0, self.line_len(end.0).saturating_sub(1))
                };
                if let Some(t) = &self.stext {
                    let text = selection_text(&t.lines, t.cols, from, to);
                    if text.is_empty() {
                        self.notice = Some("nothing to copy".into());
                    } else {
                        self.copied = Some(text);
                    }
                }
                self.copy = None;
                self.search = None;
                self.stext = None;
                self.scroll = None;
                return true;
            }
            ScrollKey::Exit => {
                if c.anchor.is_some() {
                    c.anchor = None;
                } else {
                    self.clear_find();
                    self.scroll = None;
                    return true;
                }
            }
            ScrollKey::Next => {
                self.search_step(true, rows);
                self.cursor_to_hit(rows);
                return true;
            }
            ScrollKey::Prev => {
                self.search_step(false, rows);
                self.cursor_to_hit(rows);
                return true;
            }
            ScrollKey::Search | ScrollKey::CopyMode => return true,
        }
        let max_col = self.line_len(c.line).saturating_sub(1);
        c.col = c.col.min(max_col);
        self.copy = Some(c);
        self.follow_cursor(rows);
        true
    }

    pub fn click_copy(&mut self, row_in_pane: u16, col_in_pane: u16, height: u16) {
        let (Some(t), Some(c)) = (self.stext.as_ref(), self.copy.as_mut()) else {
            return;
        };
        let offset = self.scroll.map_or(0, |s| s.offset);
        let fb = usize::from(height.saturating_sub(1).saturating_sub(row_in_pane)) + offset;
        let Some(line) = t.lines.len().checked_sub(1 + fb) else {
            return;
        };
        let text = &t.lines[line];
        let mut x = 0u16;
        let mut col = 0usize;
        for (i, ch) in chars_of(text).iter().enumerate() {
            let w = ch.width().unwrap_or(0) as u16;
            if col_in_pane < x + w.max(1) {
                col = i;
                break;
            }
            x += w;
            col = i;
        }
        c.line = line;
        c.col = col;
        c.anchor = None;
    }
}

pub(crate) fn draw_overlay(
    buf: &mut Buffer,
    outer: Rect,
    inner: Rect,
    snap: &Snapshot,
    st: &ViewState,
    session: &str,
) {
    let Some(text) = st.stext.as_ref().filter(|t| t.session == session) else {
        return draw_bar(buf, outer, st, session);
    };
    let len = text.lines.len();
    let mark = |buf: &mut Buffer, line: usize, a: usize, b: usize, style: Style, y: u16| {
        let Some(s) = text.lines.get(line) else {
            return;
        };
        let x0 = cell_x(s, a);
        let x1 = cell_x(s, b).max(x0 + 1);
        for x in x0..x1 {
            if x < inner.width {
                buf.set_style(Rect::new(inner.x + x, y, 1, 1), style);
            }
        }
    };
    let hit_style = Style::new()
        .fg(colors().warn)
        .add_modifier(Modifier::UNDERLINED | Modifier::BOLD);
    let cur_style = Style::new()
        .fg(colors().warn)
        .add_modifier(Modifier::REVERSED | Modifier::BOLD);
    let sel_style = Style::new().add_modifier(Modifier::REVERSED);
    let caret = Style::new()
        .fg(colors().accent)
        .add_modifier(Modifier::REVERSED | Modifier::BOLD);
    for r in 0..snap.lines.len().min(inner.height as usize) {
        let fb = snap.lines.len().saturating_sub(1 + r) + snap.scroll_offset;
        let Some(line) = len.checked_sub(1 + fb) else {
            continue;
        };
        let y = inner.y + r as u16;
        if let Some(s) = st.search.as_ref().filter(|s| s.session == session) {
            for (i, h) in s.hits.iter().enumerate() {
                if h.l1 < line || h.l0 > line {
                    continue;
                }
                let style = if s.current == Some(i) {
                    cur_style
                } else {
                    hit_style
                };
                for (l, a, b) in segments(h, &text.lines) {
                    if l == line {
                        mark(buf, l, a, b, style, y);
                    }
                }
            }
        }
        if let Some(c) = &st.copy {
            if let Some(anchor) = c.anchor {
                let (from, to) = if anchor <= (c.line, c.col) {
                    (anchor, (c.line, c.col))
                } else {
                    ((c.line, c.col), anchor)
                };
                if line >= from.0 && line <= to.0 {
                    let n = chars_of(&text.lines[line]).len();
                    let a = if line == from.0 { from.1 } else { 0 };
                    let b = if line == to.0 { (to.1 + 1).min(n) } else { n };
                    mark(buf, line, a, b.max(a + usize::from(n == 0)), sel_style, y);
                }
            }
            if c.line == line {
                mark(buf, line, c.col, c.col + 1, caret, y);
            }
        }
    }
    draw_bar(buf, outer, st, session);
}

fn draw_bar(buf: &mut Buffer, outer: Rect, st: &ViewState, session: &str) {
    if outer.height < 3 || outer.width < 6 {
        return;
    }
    let y = outer.bottom() - 1;
    let inner = Rect::new(outer.x + 1, y, outer.width - 2, 1);
    let find = st.search.as_ref().filter(|s| s.session == session);
    let copying = st.copy.is_some();
    if find.is_none() && !copying {
        return;
    }
    let count = |s: &SearchState| match (s.current, s.hits.len()) {
        (_, 0) if s.query.is_empty() => String::new(),
        (_, 0) => "no matches".to_string(),
        (Some(i), n) => format!("{}/{}", i + 1, n),
        (None, n) => format!("{n}"),
    };
    let text = match (find, copying) {
        (Some(s), _) if s.input => {
            let q = query_tail(&s.query, usize::from(inner.width).saturating_sub(30));
            format!("/{q}\u{2588}  {}  Enter keeps, Esc closes", count(s))
        }
        (Some(s), true) => format!(
            "COPY  /{}  {}  n older N newer  v select  y copy  Esc leaves",
            query_tail(&s.query, 20),
            count(s)
        ),
        (Some(s), false) => format!(
            "/{}  {}  n older  N newer  / new search  Esc closes",
            query_tail(&s.query, 20),
            count(s)
        ),
        (None, _) => {
            let sel = st.copy.as_ref().is_some_and(|c| c.anchor.is_some());
            if sel {
                "COPY  selecting  y copy  v drops it  Esc clears".to_string()
            } else {
                "COPY  arrows move  v select  y copy line  / search  Esc leaves".to_string()
            }
        }
    };
    let text = format!(" {} ", sanitize_text(&text));
    let bar = Style::new().add_modifier(Modifier::REVERSED);
    for x in inner.x..inner.right() {
        buf[(x, y)].set_symbol(" ").set_style(bar);
    }
    put(buf, inner.x, y, &text, bar, inner.width);
}
