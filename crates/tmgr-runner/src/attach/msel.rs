use super::find::{chars_of, row_wrapped, selection_text, ScrollText};
use super::mouse::{encode, wants, Kind, MouseEvent};
use super::view::{layout, MouseOut, PaneInfo, ViewState};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use std::time::{Duration, Instant};
use tmgr_ipc::api::Snapshot;
use unicode_width::UnicodeWidthChar;

const MULTI_CLICK: Duration = Duration::from_millis(400);
const MODIFIERS: u16 = 8 | 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gran {
    Cell,
    Word,
    Line,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    None,
    Pending,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MouseSel {
    pub session: String,
    pub anchor: (usize, u16),
    pub head: (usize, u16),
    pub press_cell: (u16, u16),
    pub pointer: (u16, u16),
    pub moved: bool,
    pub held: bool,
    pub gran: Gran,
    pub deferred: Option<MouseEvent>,
    pub finish: Finish,
    pub last_scroll: Option<Instant>,
}

#[derive(Clone, Debug)]
pub struct Click {
    pub at: Instant,
    pub session: String,
    pub cell: (u16, u16),
    pub count: u8,
}

pub fn click_count(last: Option<&Click>, now: Instant, session: &str, cell: (u16, u16)) -> u8 {
    match last {
        Some(c)
            if c.session == session
                && c.cell == cell
                && now.saturating_duration_since(c.at) <= MULTI_CLICK
                && c.count < 3 =>
        {
            c.count + 1
        }
        _ => 1,
    }
}

pub fn to_abs(scroll_max: usize, offset: usize, row: u16) -> usize {
    (scroll_max + usize::from(row)).saturating_sub(offset)
}

pub fn order(a: (usize, u16), b: (usize, u16)) -> ((usize, u16), (usize, u16)) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

pub fn row_span(a: (usize, u16), b: (usize, u16), abs: usize, width: u16) -> Option<(u16, u16)> {
    let (from, to) = order(a, b);
    if abs < from.0 || abs > to.0 {
        return None;
    }
    let s = if abs == from.0 { from.1 } else { 0 };
    let e = if abs == to.0 {
        to.1.saturating_add(1)
    } else {
        width
    };
    let e = e.min(width);
    (s < e).then_some((s, e))
}

pub fn char_at_cell(line: &str, cell: u16) -> usize {
    let cs = chars_of(line);
    let mut x = 0usize;
    for (i, ch) in cs.iter().enumerate() {
        let w = ch.width().unwrap_or(0);
        if w == 0 {
            continue;
        }
        if usize::from(cell) < x + w {
            return i;
        }
        x += w;
    }
    cs.len()
}

fn word_class(c: char) -> u8 {
    if c.is_alphanumeric() || "_-./~@:".contains(c) {
        0
    } else if c.is_whitespace() {
        1
    } else {
        2
    }
}

pub fn word_range(line: &str, idx: usize) -> Option<(usize, usize)> {
    let cs = chars_of(line);
    let class = word_class(*cs.get(idx)?);
    let mut s = idx;
    while s > 0 && word_class(cs[s - 1]) == class {
        s -= 1;
    }
    let mut e = idx;
    while e + 1 < cs.len() && word_class(cs[e + 1]) == class {
        e += 1;
    }
    Some((s, e))
}

pub fn logical_range(lines: &[String], cols: usize, flags: &[bool], line: usize) -> (usize, usize) {
    let mut s = line;
    while s > 0 && row_wrapped(flags, s - 1, &lines[s - 1], cols) {
        s -= 1;
    }
    let mut e = line;
    while e + 1 < lines.len() && row_wrapped(flags, e, &lines[e], cols) {
        e += 1;
    }
    (s, e)
}

pub fn extract(t: &ScrollText, m: &MouseSel) -> String {
    let Some(last) = t.lines.len().checked_sub(1) else {
        return String::new();
    };
    let base = (t.scroll_max + t.rows).saturating_sub(t.lines.len());
    let line_of = |abs: usize| abs.checked_sub(base).filter(|l| *l <= last);
    match m.gran {
        Gran::Cell => {
            let (from, to) = order(m.anchor, m.head);
            let start = match from.0.checked_sub(base) {
                None => (0, 0),
                Some(l) if l <= last => (l, char_at_cell(&t.lines[l], from.1)),
                Some(_) => return String::new(),
            };
            let end = match to.0.checked_sub(base) {
                None => return String::new(),
                Some(l) if l <= last => (l, char_at_cell(&t.lines[l], to.1)),
                Some(_) => (last, chars_of(&t.lines[last]).len()),
            };
            selection_text(&t.lines, t.cols, &t.wrapped, start, end)
        }
        Gran::Word => {
            let Some(l) = line_of(m.anchor.0) else {
                return String::new();
            };
            let idx = char_at_cell(&t.lines[l], m.anchor.1);
            match word_range(&t.lines[l], idx) {
                Some((s, e)) => selection_text(&t.lines, t.cols, &t.wrapped, (l, s), (l, e)),
                None => String::new(),
            }
        }
        Gran::Line => {
            let Some(l) = line_of(m.anchor.0) else {
                return String::new();
            };
            let (s, e) = logical_range(&t.lines, t.cols, &t.wrapped, l);
            let len = chars_of(&t.lines[e]).len();
            selection_text(&t.lines, t.cols, &t.wrapped, (s, 0), (e, len))
        }
    }
}

pub fn strip_modifiers(ev: MouseEvent) -> MouseEvent {
    MouseEvent {
        cb: ev.cb & !MODIFIERS,
        ..ev
    }
}

pub fn has_modifiers(ev: &MouseEvent) -> bool {
    ev.cb & MODIFIERS != 0
}

pub(crate) fn paint(buf: &mut Buffer, inner: Rect, snap: &Snapshot, m: &MouseSel) {
    if !m.moved {
        return;
    }
    let style = Style::new().add_modifier(Modifier::REVERSED);
    for r in 0..snap.lines.len().min(usize::from(inner.height)) {
        let abs = to_abs(snap.scroll_max, snap.scroll_offset, r as u16);
        if let Some((s, e)) = row_span(m.anchor, m.head, abs, inner.width) {
            buf.set_style(Rect::new(inner.x + s, inner.y + r as u16, e - s, 1), style);
        }
    }
}

impl ViewState {
    pub fn msel_waiting(&self) -> bool {
        self.msel
            .as_ref()
            .is_some_and(|m| m.finish == Finish::Pending)
    }

    pub fn msel_active(&self) -> bool {
        self.msel.is_some()
    }

    pub fn clear_msel(&mut self) {
        self.msel = None;
    }

    pub fn finish_msel(&mut self) -> bool {
        if !self.msel_waiting() {
            return false;
        }
        let Some(m) = self.msel.take() else {
            return false;
        };
        let text = self
            .stext
            .as_ref()
            .filter(|t| t.session == m.session)
            .map(|t| extract(t, &m))
            .unwrap_or_default();
        if text.is_empty() {
            self.notice = Some("nothing to copy".into());
        } else {
            self.copied = Some(text);
        }
        true
    }

    pub fn msel_tick(&mut self, area: Rect, info: &dyn Fn(&str) -> PaneInfo) -> bool {
        let now = Instant::now();
        let Some(m) = self.msel.as_mut().filter(|m| m.held) else {
            return false;
        };
        if m.last_scroll
            .is_some_and(|t| now.saturating_duration_since(t) < Duration::from_millis(60))
        {
            return false;
        }
        m.last_scroll = Some(now);
        let pos = m.pointer;
        self.msel_track(area, info, pos)
            .is_some_and(|(_, changed)| changed)
    }

    pub(super) fn msel_press(
        &mut self,
        session: &str,
        at: (u16, u16),
        ev: MouseEvent,
        agent: bool,
        pane: PaneInfo,
    ) {
        let offset = self.scroll.map_or(0, |s| s.offset);
        let abs = to_abs(pane.scroll_max, offset, at.1);
        let cell = (ev.col, ev.row);
        let now = Instant::now();
        let count = if agent {
            1
        } else {
            click_count(self.last_click.as_ref(), now, session, cell)
        };
        self.last_click = Some(Click {
            at: now,
            session: session.to_string(),
            cell,
            count,
        });
        let gran = match count {
            2 => Gran::Word,
            3 => Gran::Line,
            _ => Gran::Cell,
        };
        self.msel = Some(MouseSel {
            session: session.to_string(),
            anchor: (abs, at.0),
            head: (abs, at.0),
            press_cell: at,
            pointer: cell,
            moved: false,
            held: true,
            gran,
            deferred: agent.then_some(ev),
            finish: Finish::None,
            last_scroll: None,
        });
    }

    pub(super) fn msel_handles(&self, ev: &MouseEvent) -> bool {
        self.msel.as_ref().is_some_and(|m| m.held)
            && ev.cb & 3 == 0
            && matches!(ev.kind(), Kind::Drag | Kind::Release)
    }

    fn msel_track(
        &mut self,
        area: Rect,
        info: &dyn Fn(&str) -> PaneInfo,
        pos: (u16, u16),
    ) -> Option<((u16, u16), bool)> {
        let session = self.msel.as_ref()?.session.clone();
        let lay = layout(area, self);
        let Some(inner) = lay
            .panes
            .iter()
            .find(|p| p.session == session && !p.inner.is_empty())
            .map(|p| p.inner)
        else {
            self.msel = None;
            return None;
        };
        let pane = info(&session);
        let mut changed = false;
        let up = pos.1 < inner.y;
        if up || pos.1 >= inner.bottom() {
            if let Some(idx) = self.panes().iter().position(|s| *s == session) {
                changed = self.wheel(idx, up, pane.scroll_max);
            }
        }
        let c = pos.0.clamp(inner.x, inner.right() - 1) - inner.x;
        let r = pos.1.clamp(inner.y, inner.bottom() - 1) - inner.y;
        let offset = self.scroll.map_or(0, |s| s.offset);
        let head = (to_abs(pane.scroll_max, offset, r), c);
        let m = self.msel.as_mut()?;
        m.pointer = pos;
        changed |= m.head != head;
        m.head = head;
        if (c, r) != m.press_cell || head != m.anchor {
            changed |= !m.moved;
            m.moved = true;
            m.gran = Gran::Cell;
        }
        Some(((c, r), changed))
    }

    pub(super) fn msel_mouse(
        &mut self,
        area: Rect,
        ev: MouseEvent,
        info: &dyn Fn(&str) -> PaneInfo,
        mut out: MouseOut,
    ) -> MouseOut {
        let Some((cell, changed)) = self.msel_track(area, info, (ev.col, ev.row)) else {
            out.redraw = true;
            return out;
        };
        if ev.kind() == Kind::Drag {
            out.redraw |= changed;
            return out;
        }
        out.redraw = true;
        let Some(mut m) = self.msel.take() else {
            return out;
        };
        let click = cell == m.press_cell && m.head == m.anchor;
        if click {
            m.moved = false;
            if let Some(press) = m.deferred {
                let pane = info(&m.session);
                if wants(pane.mouse, Kind::Press) {
                    let enc = pane.mouse.encoding;
                    let (c, r) = m.press_cell;
                    let mut bytes = encode(&strip_modifiers(press), c, r, enc);
                    bytes.extend(encode(&strip_modifiers(ev), c, r, enc));
                    out.forward = Some((m.session, bytes));
                }
                return out;
            }
            if m.gran == Gran::Cell {
                return out;
            }
        }
        m.held = false;
        m.deferred = None;
        m.finish = Finish::Pending;
        out.want_text = Some(m.session.clone());
        self.msel = Some(m);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attach::keys::Cmd;
    use crate::attach::render::{render, ScreenSource};
    use crate::attach::view::SessionRow;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::collections::HashMap;
    use tmgr_ipc::api::{
        Attr, Color, Line, Modes, MouseEncoding, MouseLevel, MouseMode, Span, State, StateKind,
    };

    fn from_abs(abs: usize, scroll_max: usize, offset: usize, height: u16) -> Option<u16> {
        let r = (abs + offset).checked_sub(scroll_max)?;
        (r < usize::from(height)).then_some(r as u16)
    }

    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 140,
        height: 40,
    };

    fn ev(cb: u16, col: u16, row: u16, release: bool) -> MouseEvent {
        MouseEvent {
            cb,
            col,
            row,
            release,
        }
    }

    fn info(mode: MouseMode, scroll_max: usize) -> impl Fn(&str) -> PaneInfo {
        move |_| PaneInfo {
            mouse: mode,
            scroll_max,
            alt_screen: false,
            app_cursor: false,
        }
    }

    fn sgr(level: MouseLevel) -> MouseMode {
        MouseMode {
            level,
            encoding: MouseEncoding::Sgr,
        }
    }

    fn view(panes: usize) -> ViewState {
        let rows = ["a", "b", "c"]
            .iter()
            .map(|id| SessionRow {
                id: (*id).into(),
                name: (*id).into(),
                state: State {
                    kind: StateKind::Working,
                    since_ms: 0,
                    detail: None,
                    exit_code: None,
                },
                age: "1m".into(),
                you: None,
                esc: false,
                resumed_from: None,
            })
            .collect();
        let mut v = ViewState::new(rows, "local");
        v.width = 140;
        if panes == 2 {
            v.apply(Cmd::Split);
        }
        v
    }

    fn inner(v: &ViewState, idx: usize) -> Rect {
        layout(AREA, v).panes[idx].inner
    }

    fn text(
        lines: &[&str],
        cols: usize,
        flags: &[bool],
        scroll_max: usize,
        rows: usize,
    ) -> ScrollText {
        ScrollText {
            session: "a".into(),
            lines: lines.iter().map(|s| (*s).into()).collect(),
            cols,
            wrapped: flags.to_vec(),
            scroll_max,
            rows,
        }
    }

    fn sel(anchor: (usize, u16), head: (usize, u16), gran: Gran) -> MouseSel {
        MouseSel {
            session: "a".into(),
            anchor,
            head,
            press_cell: (0, 0),
            pointer: (0, 0),
            moved: true,
            held: false,
            gran,
            deferred: None,
            finish: Finish::Pending,
            last_scroll: None,
        }
    }

    #[test]
    fn abs_rows_map_both_ways_and_stay_stable_when_history_grows() {
        let abs = to_abs(10, 0, 3);
        assert_eq!(abs, 13);
        assert_eq!(from_abs(abs, 10, 0, 5), Some(3));
        assert_eq!(from_abs(abs, 12, 0, 5), Some(1));
        assert_eq!(from_abs(abs, 20, 0, 5), None);
        assert_eq!(from_abs(abs, 10, 0, 3), None);
        assert_eq!(to_abs(10, 4, 0), 6);
        assert_eq!(from_abs(6, 10, 4, 5), Some(0));
    }

    #[test]
    fn selection_is_normalised_and_spans_stream_style() {
        let (a, b) = ((5, 7), (3, 2));
        assert_eq!(order(a, b), (b, a));
        assert_eq!(row_span(a, b, 2, 20), None);
        assert_eq!(row_span(a, b, 3, 20), Some((2, 20)));
        assert_eq!(row_span(a, b, 4, 20), Some((0, 20)));
        assert_eq!(row_span(a, b, 5, 20), Some((0, 8)));
        assert_eq!(row_span(a, b, 6, 20), None);
        assert_eq!(row_span((2, 4), (2, 9), 2, 20), Some((4, 10)));
        assert_eq!(row_span((2, 9), (2, 4), 2, 20), Some((4, 10)));
        assert_eq!(row_span((2, 4), (2, 30), 2, 20), Some((4, 20)));
    }

    #[test]
    fn cells_map_to_chars_with_wide_glyphs() {
        assert_eq!(char_at_cell("hello", 0), 0);
        assert_eq!(char_at_cell("hello", 4), 4);
        assert_eq!(char_at_cell("hello", 5), 5);
        assert_eq!(char_at_cell("hello", 50), 5);
        assert_eq!(char_at_cell("a日b", 1), 1);
        assert_eq!(char_at_cell("a日b", 2), 1);
        assert_eq!(char_at_cell("a日b", 3), 2);
    }

    #[test]
    fn word_range_uses_path_like_word_chars() {
        let line = "cd ~/src/my-app.rs, now";
        assert_eq!(word_range(line, 4), Some((3, 17)));
        assert_eq!(word_range(line, 0), Some((0, 1)));
        assert_eq!(word_range(line, 2), Some((2, 2)));
        assert_eq!(word_range(line, 18), Some((18, 18)));
        assert_eq!(word_range(line, 99), None);
    }

    #[test]
    fn logical_range_follows_wrapped_flags() {
        let lines: Vec<String> = ["aaaa", "bbbb", "cc", "dd"].map(String::from).to_vec();
        let flags = [false, true, false, false];
        assert_eq!(logical_range(&lines, 4, &flags, 0), (0, 0));
        assert_eq!(logical_range(&lines, 4, &flags, 1), (1, 2));
        assert_eq!(logical_range(&lines, 4, &flags, 2), (1, 2));
        assert_eq!(logical_range(&lines, 4, &flags, 3), (3, 3));
    }

    #[test]
    fn click_counter_needs_same_cell_session_and_time_window() {
        let t0 = Instant::now();
        let first = Click {
            at: t0,
            session: "a".into(),
            cell: (3, 4),
            count: 1,
        };
        let ms = Duration::from_millis;
        assert_eq!(click_count(None, t0, "a", (3, 4)), 1);
        assert_eq!(click_count(Some(&first), t0 + ms(399), "a", (3, 4)), 2);
        assert_eq!(click_count(Some(&first), t0 + ms(401), "a", (3, 4)), 1);
        assert_eq!(click_count(Some(&first), t0 + ms(100), "a", (3, 5)), 1);
        assert_eq!(click_count(Some(&first), t0 + ms(100), "b", (3, 4)), 1);
        let third = Click {
            count: 3,
            ..first.clone()
        };
        assert_eq!(click_count(Some(&third), t0 + ms(100), "a", (3, 4)), 1);
        let second = Click { count: 2, ..first };
        assert_eq!(click_count(Some(&second), t0 + ms(100), "a", (3, 4)), 3);
    }

    #[test]
    fn stream_selection_joins_wrapped_rows_and_keeps_hard_newlines() {
        let t = text(
            &["hello wor", "ld", "next line"],
            9,
            &[true, false, false],
            0,
            3,
        );
        let m = sel((0, 6), (2, 3), Gran::Cell);
        assert_eq!(extract(&t, &m), "wor".to_string() + "ld\nnext");
        let rev = sel((2, 3), (0, 6), Gran::Cell);
        assert_eq!(extract(&t, &rev), extract(&t, &m));
    }

    #[test]
    fn selection_past_line_end_includes_the_whole_line() {
        let t = text(&["ab", "cd"], 10, &[false, false], 0, 2);
        assert_eq!(extract(&t, &sel((0, 0), (1, 50), Gran::Cell)), "ab\ncd");
        assert_eq!(extract(&t, &sel((0, 1), (0, 50), Gran::Cell)), "b");
    }

    #[test]
    fn extraction_maps_abs_rows_onto_capped_text() {
        let t = text(&["one", "two", "three"], 10, &[], 10, 3);
        assert_eq!(extract(&t, &sel((11, 0), (11, 2), Gran::Cell)), "two");
        assert_eq!(extract(&t, &sel((0, 0), (11, 2), Gran::Cell)), "one\ntwo");
        assert_eq!(extract(&t, &sel((0, 0), (5, 2), Gran::Cell)), "");
        assert_eq!(extract(&t, &sel((12, 0), (99, 0), Gran::Cell)), "three");
    }

    #[test]
    fn word_and_line_granularity_expand_from_the_anchor() {
        let t = text(
            &["see foo/bar.rs now", "aaaa", "bbbb", "cc"],
            4,
            &[false, true, true, false],
            0,
            4,
        );
        assert_eq!(extract(&t, &sel((0, 6), (0, 6), Gran::Word)), "foo/bar.rs");
        assert_eq!(extract(&t, &sel((2, 1), (2, 1), Gran::Line)), "aaaabbbbcc");
        assert_eq!(
            extract(&t, &sel((0, 1), (0, 1), Gran::Line)),
            "see foo/bar.rs now"
        );
    }

    fn drag_ev(col: u16, row: u16) -> MouseEvent {
        ev(32, col, row, false)
    }

    #[test]
    fn plain_press_with_agent_mouse_is_deferred_and_click_forwards_both_events() {
        let mut v = view(1);
        let i = info(sgr(MouseLevel::Press), 0);
        let rect = inner(&v, 0);
        let (x, y) = (rect.x + 4, rect.y + 2);
        let out = v.on_mouse(AREA, ev(0, x, y, false), &i);
        assert_eq!(out.forward, None);
        assert!(v.msel_active());
        assert_eq!(v.grab, None);
        let out = v.on_mouse(AREA, ev(0, x, y, true), &i);
        assert_eq!(
            out.forward,
            Some(("a".into(), b"\x1b[<0;5;3M\x1b[<0;5;3m".to_vec()))
        );
        assert!(!v.msel_active());
        assert_eq!(out.want_text, None);
    }

    #[test]
    fn click_after_wiggle_back_to_the_same_cell_still_forwards() {
        let mut v = view(1);
        let i = info(sgr(MouseLevel::Drag), 0);
        let rect = inner(&v, 0);
        let (x, y) = (rect.x + 4, rect.y + 2);
        v.on_mouse(AREA, ev(0, x, y, false), &i);
        v.on_mouse(AREA, drag_ev(x + 3, y), &i);
        v.on_mouse(AREA, drag_ev(x, y), &i);
        let out = v.on_mouse(AREA, ev(0, x, y, true), &i);
        assert_eq!(out.forward.map(|f| f.0), Some("a".to_string()));
    }

    #[test]
    fn ctrl_press_forwards_at_once_with_modifier_bits_stripped() {
        let mut v = view(1);
        let i = info(sgr(MouseLevel::Drag), 0);
        let rect = inner(&v, 0);
        let (x, y) = (rect.x + 4, rect.y + 2);
        let out = v.on_mouse(AREA, ev(16 | 4, x, y, false), &i);
        assert_eq!(out.forward, Some(("a".into(), b"\x1b[<4;5;3M".to_vec())));
        assert_eq!(v.grab.as_deref(), Some("a"));
        assert!(!v.msel_active());
        let out = v.on_mouse(AREA, ev(8, x, y, false), &i);
        assert_eq!(out.forward, Some(("a".into(), b"\x1b[<0;5;3M".to_vec())));
    }

    #[test]
    fn agentless_pane_drag_selects_and_release_asks_for_text() {
        let mut v = view(1);
        let i = info(MouseMode::default(), 7);
        let rect = inner(&v, 0);
        let (x, y) = (rect.x + 2, rect.y + 1);
        let out = v.on_mouse(AREA, ev(0, x, y, false), &i);
        assert_eq!(out.forward, None);
        assert!(v.msel.as_ref().is_some_and(|m| !m.moved));
        let out = v.on_mouse(AREA, drag_ev(x + 5, y + 2), &i);
        assert!(out.redraw);
        let m = v.msel.as_ref().unwrap();
        assert!(m.moved);
        assert_eq!(m.anchor, (8, 2));
        assert_eq!(m.head, (10, 7));
        let out = v.on_mouse(AREA, ev(0, x + 5, y + 2, true), &i);
        assert_eq!(out.want_text.as_deref(), Some("a"));
        assert!(v.msel_waiting() && v.msel_active());
        v.stext = Some(text(&["x", "y", "z"], 40, &[], 7, 3));
        assert!(v.finish_msel());
        assert!(!v.msel_active());
    }

    #[test]
    fn finish_copies_text_or_reports_nothing() {
        let mut v = view(1);
        v.stext = Some(text(&["hello world", "second"], 40, &[], 10, 2));
        v.msel = Some(sel((10, 6), (11, 2), Gran::Cell));
        assert!(v.msel_waiting());
        assert!(v.finish_msel());
        assert_eq!(v.copied.as_deref(), Some("world\nsec"));
        assert_eq!(v.msel, None);
        v.copied = None;
        v.msel = Some(sel((99, 0), (99, 3), Gran::Cell));
        assert!(v.finish_msel());
        assert_eq!(v.copied, None);
        assert_eq!(v.notice.as_deref(), Some("nothing to copy"));
        assert!(!v.finish_msel());
    }

    #[test]
    fn multi_click_selects_word_then_line_only_without_agent_mouse() {
        let mut v = view(1);
        let i = info(MouseMode::default(), 0);
        let rect = inner(&v, 0);
        let (x, y) = (rect.x + 2, rect.y + 1);
        let click = |v: &mut ViewState| {
            v.on_mouse(AREA, ev(0, x, y, false), &i);
            v.on_mouse(AREA, ev(0, x, y, true), &i)
        };
        let out = click(&mut v);
        assert_eq!(out.want_text, None);
        assert!(!v.msel_active());
        let out = click(&mut v);
        assert_eq!(out.want_text.as_deref(), Some("a"));
        assert_eq!(v.msel.as_ref().map(|m| m.gran), Some(Gran::Word));
        v.clear_msel();
        let out = click(&mut v);
        assert_eq!(out.want_text.as_deref(), Some("a"));
        assert_eq!(v.msel.as_ref().map(|m| m.gran), Some(Gran::Line));

        let mut w = view(1);
        let agent = info(sgr(MouseLevel::Press), 0);
        for _ in 0..3 {
            w.on_mouse(AREA, ev(0, x, y, false), &agent);
            let out = w.on_mouse(AREA, ev(0, x, y, true), &agent);
            assert!(out.forward.is_some());
            assert_eq!(out.want_text, None);
        }
    }

    #[test]
    fn drag_outside_the_pane_is_clamped_and_triggers_no_sidebar_action() {
        let mut v = view(1);
        let i = info(MouseMode::default(), 0);
        let rect = inner(&v, 0);
        v.on_mouse(AREA, ev(0, rect.x + 3, rect.y + 2, false), &i);
        let rows_before = v.sessions.len();
        let panes_before = v.panes().to_vec();
        let out = v.on_mouse(AREA, drag_ev(2, 1), &i);
        assert_eq!(out.control, None);
        assert_eq!(out.open_task, None);
        assert!(!out.new_session && !out.templates);
        assert_eq!(v.panes(), panes_before);
        assert_eq!(v.sessions.len(), rows_before);
        let m = v.msel.as_ref().unwrap();
        assert_eq!(m.head.1, 0);
        let out = v.on_mouse(AREA, drag_ev(300, 300), &i);
        assert_eq!(out.control, None);
        let m = v.msel.as_ref().unwrap();
        assert_eq!(m.head, (usize::from(rect.height) - 1, rect.width - 1));
    }

    #[test]
    fn dragging_above_or_below_scrolls_and_the_tick_continues() {
        let mut v = view(1);
        let i = info(MouseMode::default(), 50);
        let rect = inner(&v, 0);
        v.on_mouse(AREA, ev(0, rect.x + 3, rect.y + 2, false), &i);
        assert_eq!(v.scroll, None);
        v.on_mouse(AREA, drag_ev(rect.x + 3, rect.y.saturating_sub(1)), &i);
        assert_eq!(v.scroll.map(|s| s.offset), Some(3));
        assert_eq!(v.msel.as_ref().unwrap().head, (50 - 3, 3));
        assert!(v.msel_tick(AREA, &i));
        assert_eq!(v.scroll.map(|s| s.offset), Some(6));
        assert_eq!(v.msel.as_ref().unwrap().head.0, 50 - 6);
        v.on_mouse(AREA, drag_ev(rect.x + 3, rect.y + 5), &i);
        assert!(!v.msel_tick(AREA, &i));
        assert_eq!(v.scroll.map(|s| s.offset), Some(6));
        v.on_mouse(AREA, drag_ev(rect.x + 3, rect.bottom() + 2), &i);
        assert_eq!(v.scroll.map(|s| s.offset), Some(3));
        v.on_mouse(AREA, ev(0, rect.x + 3, rect.bottom() + 2, true), &i);
        assert!(!v.msel_tick(AREA, &i));
    }

    #[test]
    fn stale_selection_of_a_closed_pane_is_dropped_without_crashing() {
        let mut v = view(2);
        let i = info(MouseMode::default(), 0);
        let rect = inner(&v, 0);
        v.on_mouse(AREA, ev(0, rect.x + 1, rect.y + 1, false), &i);
        let session = v.msel.as_ref().unwrap().session.clone();
        v.close_pane(&session);
        assert!(!v.msel_tick(AREA, &i) || v.msel.is_none());
        v.on_mouse(AREA, drag_ev(rect.x + 4, rect.y + 1), &i);
        assert!(!v.msel_active());
        v.on_mouse(AREA, ev(0, rect.x + 4, rect.y + 1, true), &i);
        assert!(!v.msel_active());
    }

    struct Screens(HashMap<String, Snapshot>);

    impl ScreenSource for Screens {
        fn snapshot(&self, session: &str) -> Option<&Snapshot> {
            self.0.get(session)
        }
    }

    fn snapshot(rows: usize, cols: u16, scroll_max: usize) -> Snapshot {
        Snapshot {
            rows: rows as u16,
            cols,
            lines: (0..rows)
                .map(|_| Line {
                    spans: vec![Span {
                        a: Attr {
                            fg: Color::Default,
                            bg: Color::Default,
                            flags: 0,
                        },
                        t: "x".repeat(usize::from(cols)),
                    }],
                })
                .collect(),
            cursor: None,
            modes: Modes {
                app_cursor: false,
                bracketed_paste: false,
                alt_screen: false,
                hide_cursor: false,
                mouse: Default::default(),
            },
            scroll_offset: 0,
            scroll_max,
        }
    }

    #[test]
    fn two_pane_highlight_and_text_stay_inside_the_first_pane() {
        let mut v = view(2);
        let first = layout(AREA, &v).panes[0].clone();
        let second = layout(AREA, &v).panes[1].clone();
        let i = info(MouseMode::default(), 0);
        let rect = first.inner;
        let (x0, y0) = (rect.x + 2, rect.y + 1);
        v.on_mouse(AREA, ev(0, x0, y0, false), &i);
        v.on_mouse(AREA, drag_ev(second.inner.x + 4, y0 + 2), &i);
        let m = v.msel.as_ref().unwrap().clone();
        assert_eq!(m.session, first.session);
        assert_eq!(m.head.1, rect.width - 1);

        let mut screens = HashMap::new();
        for p in [&first, &second] {
            screens.insert(
                p.session.clone(),
                snapshot(usize::from(p.inner.height), p.inner.width, 0),
            );
        }
        let mut term = Terminal::new(TestBackend::new(AREA.width, AREA.height)).unwrap();
        term.draw(|f| render(f, &v, &Screens(screens))).unwrap();
        let buf = term.backend().buffer();
        let reversed = |x: u16, y: u16| {
            buf[(x, y)].modifier.contains(Modifier::REVERSED) && buf[(x, y)].symbol() == "x"
        };
        assert!(reversed(x0, y0));
        assert!(!reversed(x0 - 1, y0));
        assert!(reversed(rect.x, y0 + 1));
        assert!(reversed(rect.right() - 1, y0 + 1));
        assert!(!reversed(rect.x, y0 + 3));
        assert!(!reversed(rect.right() - 1, y0 + 2 + 1));
        for y in second.inner.y..second.inner.bottom() {
            for x in second.inner.x..second.inner.right() {
                assert!(!reversed(x, y), "second pane cell {x},{y} highlighted");
            }
        }
        for y in rect.y..rect.bottom() {
            assert!(!buf[(rect.right(), y)].modifier.contains(Modifier::REVERSED));
        }

        v.stext = Some(ScrollText {
            session: first.session.clone(),
            lines: (0..first.inner.height)
                .map(|n| format!("first-{n}"))
                .collect(),
            cols: usize::from(rect.width),
            wrapped: vec![],
            scroll_max: 0,
            rows: usize::from(first.inner.height),
        });
        v.on_mouse(AREA, ev(0, second.inner.x + 4, y0 + 2, true), &i);
        assert!(v.finish_msel());
        let copied = v.copied.take().unwrap();
        assert_eq!(copied, "rst-1\nfirst-2\nfirst-3");
        assert!(!copied.contains("second"));
    }
}
