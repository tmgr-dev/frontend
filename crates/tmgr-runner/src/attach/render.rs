use super::view::{layout, Layout, PaneRect, SessionRow, ViewState, WIDE_COLS};
use crate::sanitize::sanitize_text;
use crate::screen::{Attr, Color as Col, Snapshot, BOLD, DIM, INVERSE, ITALIC, UNDERLINE};
use crate::t0::StateKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Widget};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

pub trait ScreenSource {
    fn snapshot(&self, session: &str) -> Option<&Snapshot>;
}

const ACCENT: Color = Color::LightRed;
const SIDEBAR_BG: Color = Color::Indexed(237);

fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

fn text(s: &str, style: Style) -> Span<'static> {
    Span::styled(sanitize_text(s).into_owned(), style)
}

fn badge(row: &SessionRow) -> (&'static str, Style) {
    match row.state.kind {
        StateKind::Working => ("●", Style::new().fg(Color::Yellow)),
        StateKind::Waiting => ("◌", Style::new().fg(Color::Cyan)),
        StateKind::Blocked => ("◆", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)),
        StateKind::Exited if row.state.exit_code == Some(0) => ("✓", Style::new().fg(Color::Green)),
        StateKind::Exited => ("✗", Style::new().fg(Color::Red)),
        StateKind::Starting => ("○", dim()),
    }
}

fn state_label(row: &SessionRow) -> &'static str {
    match row.state.kind {
        StateKind::Starting => "starting",
        StateKind::Working => "working",
        StateKind::Waiting => "waiting",
        StateKind::Blocked => "blocked",
        StateKind::Exited if row.state.exit_code == Some(0) => "exited",
        StateKind::Exited => "failed",
    }
}

fn convert(c: Col) -> Color {
    match c {
        Col::Default => Color::Reset,
        Col::Idx(i) => Color::Indexed(i),
        Col::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn attr_style(a: &Attr) -> Style {
    let mut m = Modifier::empty();
    for (bit, modifier) in [
        (BOLD, Modifier::BOLD),
        (DIM, Modifier::DIM),
        (ITALIC, Modifier::ITALIC),
        (UNDERLINE, Modifier::UNDERLINED),
        (INVERSE, Modifier::REVERSED),
    ] {
        if a.flags & bit != 0 {
            m |= modifier;
        }
    }
    Style::new()
        .fg(convert(a.fg))
        .bg(convert(a.bg))
        .add_modifier(m)
}

fn put(buf: &mut Buffer, x: u16, y: u16, s: &str, style: Style, max: u16) -> u16 {
    if max == 0 {
        return x;
    }
    buf.set_stringn(x, y, sanitize_text(s), max as usize, style)
        .0
}

pub fn render(frame: &mut Frame, st: &ViewState, screens: &dyn ScreenSource) {
    let area = frame.area();
    let lay = layout(area, st);
    let buf = frame.buffer_mut();
    if let Some(sb) = lay.sidebar {
        draw_sidebar(buf, sb, st);
    }
    for p in &lay.panes {
        draw_pane(buf, p, st, screens);
    }
    draw_status(buf, lay.status, st);
    if st.help {
        draw_help(buf, area);
    }
}

fn draw_sidebar(buf: &mut Buffer, area: Rect, st: &ViewState) {
    if area.is_empty() {
        return;
    }
    let hint = "Ctrl-] b";
    let hint_w = hint.width() as u16;
    let hx = area.right().saturating_sub(hint_w + 1);
    let end = put(
        buf,
        area.x + 1,
        area.y,
        "SESSIONS",
        dim().add_modifier(Modifier::BOLD),
        area.width.saturating_sub(1),
    );
    if hx > end {
        put(buf, hx, area.y, hint, dim(), hint_w);
    }
    let mut rows: Vec<&SessionRow> = st.sessions.iter().collect();
    rows.sort_by_key(|r| r.state.kind != StateKind::Blocked);
    let focused = st.focused_session();
    for (i, row) in rows.iter().enumerate() {
        let y = area.y + 1 + i as u16;
        if y >= area.bottom() {
            break;
        }
        if focused == Some(row.id.as_str()) {
            buf.set_style(
                Rect::new(area.x, y, area.width, 1),
                Style::new().bg(SIDEBAR_BG),
            );
        }
        let (glyph, style) = badge(row);
        put(buf, area.x + 1, y, glyph, style, 1);
        let age_w = row.age.width() as u16;
        let age_x = area.right().saturating_sub(age_w + 1).max(area.x + 3);
        let name_w = age_x.saturating_sub(area.x + 3).saturating_sub(1);
        let name_style = if row.state.kind == StateKind::Blocked {
            style
        } else {
            Style::new()
        };
        put(buf, area.x + 3, y, &row.name, name_style, name_w);
        put(
            buf,
            age_x,
            y,
            &row.age,
            dim(),
            area.right().saturating_sub(age_x),
        );
    }
}

fn pane_title(p: &PaneRect, st: &ViewState) -> Line<'static> {
    let index = st.panes.iter().position(|id| *id == p.session).unwrap_or(0) + 1;
    let mut spans = vec![
        text(" ", Style::new()),
        text(
            &index.to_string(),
            Style::new().add_modifier(Modifier::REVERSED),
        ),
    ];
    match st.sessions.iter().find(|s| s.id == p.session) {
        Some(row) => spans.push(text(
            &format!("  {} · {}", row.name, state_label(row)),
            Style::new(),
        )),
        None => spans.push(text(&format!("  {}", p.session), Style::new())),
    }
    if let (Some(sc), true) = (st.scroll, st.focused_session() == Some(p.session.as_str())) {
        spans.push(text(
            &format!(" · scroll -{}/{}", sc.offset, sc.max),
            Style::new().fg(Color::Yellow),
        ));
    }
    spans.push(text(" ", Style::new()));
    Line::from(spans)
}

fn draw_pane(buf: &mut Buffer, p: &PaneRect, st: &ViewState, screens: &dyn ScreenSource) {
    if p.outer.is_empty() {
        return;
    }
    let row = st.sessions.iter().find(|s| s.id == p.session);
    let focused = st.focused_session() == Some(p.session.as_str());
    let blocked = row.is_some_and(|r| r.state.kind == StateKind::Blocked);
    let border = match (blocked, focused) {
        (true, true) => Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        (true, false) => Style::new().fg(ACCENT),
        (false, true) => Style::new().fg(Color::White),
        (false, false) => dim(),
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title(pane_title(p, st))
        .render(p.outer, buf);
    let inner = p.content();
    if inner.is_empty() {
        return;
    }
    Clear.render(inner, buf);
    match screens.snapshot(&p.session) {
        Some(snap) => draw_snapshot(buf, inner, snap),
        None => {
            let label = "connecting…";
            let w = label.width() as u16;
            let x = inner.x + inner.width.saturating_sub(w) / 2;
            put(
                buf,
                x,
                inner.y + inner.height / 2,
                label,
                dim(),
                inner.right() - x,
            );
        }
    }
}

fn draw_snapshot(buf: &mut Buffer, inner: Rect, snap: &Snapshot) {
    for (r, line) in snap.lines.iter().take(inner.height as usize).enumerate() {
        let y = inner.y + r as u16;
        let mut x = inner.x;
        for span in &line.spans {
            if x >= inner.right() {
                break;
            }
            x = put(buf, x, y, &span.t, attr_style(&span.a), inner.right() - x);
        }
    }
}

pub fn cursor(st: &ViewState, lay: &Layout, screens: &dyn ScreenSource) -> Option<(u16, u16)> {
    if st.scroll.is_some() || st.help {
        return None;
    }
    let id = st.focused_session()?;
    let pane = lay.panes.iter().find(|p| p.session == id)?;
    let snap = screens.snapshot(id)?;
    if snap.scroll_offset > 0 {
        return None;
    }
    let (row, col) = snap.cursor?;
    let content = pane.content();
    let x = content.x.checked_add(col)?;
    let y = content.y.checked_add(row)?;
    (x < content.right() && y < content.bottom()).then_some((x, y))
}

fn counts(st: &ViewState) -> Vec<Span<'static>> {
    let n = |f: &dyn Fn(&SessionRow) -> bool| st.sessions.iter().filter(|r| f(r)).count();
    let mut items = vec![
        (
            "●",
            Color::Yellow,
            n(&|r| r.state.kind == StateKind::Working),
        ),
        ("◌", Color::Cyan, n(&|r| r.state.kind == StateKind::Waiting)),
        ("◆", ACCENT, n(&|r| r.state.kind == StateKind::Blocked)),
        (
            "✓",
            Color::Green,
            n(&|r| r.state.kind == StateKind::Exited && r.state.exit_code == Some(0)),
        ),
    ];
    let failed = n(&|r| r.state.kind == StateKind::Exited && r.state.exit_code != Some(0));
    if failed > 0 {
        items.push(("✗", Color::Red, failed));
    }
    let mut spans = Vec::new();
    for (i, (glyph, color, count)) in items.into_iter().enumerate() {
        if i > 0 {
            spans.push(text(" ", Style::new()));
        }
        spans.push(text(&format!("{glyph} {count}"), Style::new().fg(color)));
    }
    spans
}

fn draw_status(buf: &mut Buffer, area: Rect, st: &ViewState) {
    if area.is_empty() {
        return;
    }
    let focused = match st.focused_row() {
        Some(r) => format!("{} · {}", r.name, state_label(r)),
        None => "no sessions".to_string(),
    };
    let sep = || text(" · ", dim());
    let mut spans = vec![text(" ", Style::new())];
    if area.width < WIDE_COLS {
        match &st.notice {
            Some(n) => spans.push(text(n, Style::new().fg(Color::Yellow))),
            None => spans.push(text(&focused, Style::new())),
        }
        spans.push(sep());
        spans.push(text("Ctrl-] ?", dim()));
        buf.set_line(area.x, area.y, &Line::from(spans), area.width);
        return;
    }
    spans.push(text(
        "tmgr runner",
        Style::new().add_modifier(Modifier::BOLD),
    ));
    if !st.runner_label.is_empty() {
        spans.push(sep());
        spans.push(text(&st.runner_label, dim()));
    }
    spans.push(sep());
    match &st.notice {
        Some(n) => spans.push(text(n, Style::new().fg(Color::Yellow))),
        None => {
            spans.push(text(&focused, Style::new()));
            spans.push(sep());
            spans.extend(counts(st));
        }
    }
    let hint = "Ctrl-] ? help";
    let hint_w = hint.width() as u16 + 1;
    buf.set_line(
        area.x,
        area.y,
        &Line::from(spans),
        area.width.saturating_sub(hint_w),
    );
    put(buf, area.right() - hint_w, area.y, hint, dim(), hint_w - 1);
}

const HELP: &[(&str, &str)] = &[
    ("d", "detach, sessions keep running"),
    ("n / p", "next / previous session"),
    ("w", "session picker"),
    ("s", "split into two panes, or merge"),
    ("Tab", "move focus to the other pane"),
    ("z", "zoom the focused pane"),
    ("b", "show or hide the sidebar"),
    ("[", "scroll back through history"),
    ("?", "this help"),
    ("Ctrl-]", "send a literal Ctrl-] to the session"),
];

fn draw_help(buf: &mut Buffer, area: Rect) {
    let w = 52.min(area.width);
    let h = (HELP.len() as u16 + 6).min(area.height);
    let r = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    if r.is_empty() {
        return;
    }
    Clear.render(r, buf);
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" Keys ")
        .render(r, buf);
    let inner = r.inner(ratatui::layout::Margin::new(2, 1));
    if inner.is_empty() {
        return;
    }
    put(
        buf,
        inner.x,
        inner.y,
        "Press Ctrl-] first, then:",
        dim(),
        inner.width,
    );
    for (i, (key, what)) in HELP.iter().enumerate() {
        let y = inner.y + 2 + i as u16;
        if y >= inner.bottom() {
            return;
        }
        put(
            buf,
            inner.x,
            y,
            key,
            Style::new().add_modifier(Modifier::BOLD),
            inner.width,
        );
        let x = inner.x + 8;
        if x < inner.right() {
            put(buf, x, y, what, Style::new(), inner.right() - x);
        }
    }
    let y = inner.y + 3 + HELP.len() as u16;
    if y < inner.bottom() {
        put(
            buf,
            inner.x,
            y,
            "? again closes this box",
            dim(),
            inner.width,
        );
    }
}
