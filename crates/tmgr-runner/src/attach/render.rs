use super::actions::find;
use super::keys::Cmd;
use super::newsession::{draw_new_session, new_session_cursor};
use super::palette::PaletteMode;
use super::palette::{
    palette_inner, palette_list, palette_rect, palette_top, query_avail, query_tail, RowView,
};
use super::place::{fit, tail2};
use super::qmenu::{badge_text, draw_qmenu, GotoMark};
use super::status::Peek;
use super::theme::{colors, with_theme};
use super::title::{age_label, clip};
use super::tree::Nav;
use super::usage::{Part, Tone};
use super::view::{
    header_button, layout, sidebar_button, slot_control, Control, Layout, PaneRect, SessionRow,
    SidebarItem, SidebarLine, ViewState, HEADER_MIN_WIDTH, WIDE_COLS,
};
use crate::sanitize::sanitize_text;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Widget};
use ratatui::Frame;
use tmgr_ipc::api::StateKind;
use tmgr_ipc::api::{Attr, Color as Col, Snapshot, BOLD, DIM, INVERSE, ITALIC, UNDERLINE};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub trait ScreenSource {
    fn snapshot(&self, session: &str) -> Option<&Snapshot>;
}

const MIN_NAME_W: usize = 10;

pub(crate) fn dim() -> Style {
    Style::new().fg(colors().dim)
}

fn tone_style(tone: Tone, plain: Style) -> Style {
    match tone {
        Tone::Plain => plain,
        Tone::Stale => dim().add_modifier(Modifier::ITALIC),
        Tone::Warn => Style::new().fg(colors().warn),
        Tone::Hot => Style::new().fg(colors().bad).add_modifier(Modifier::BOLD),
    }
}

fn part_spans(parts: &[Part], plain: Style) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            out.push(text(" · ", dim()));
        }
        out.push(text(&p.text, tone_style(p.tone, plain)));
    }
    out
}

fn parts_width(parts: &[Part]) -> usize {
    let sep = parts.len().saturating_sub(1) * 3;
    sep + parts.iter().map(|p| p.text.width()).sum::<usize>()
}

pub(crate) fn header_meta(st: &ViewState, session: &str, spare: usize) -> Option<Vec<Part>> {
    let meta = st.session_meta(session)?;
    let all = meta.parts;
    let tail: Vec<Part> = all[1..].to_vec();
    let last: Vec<Part> = all[2..].to_vec();
    let bypass = all[2].tone == Tone::Hot;
    let mut options = vec![all, tail];
    options.push(if bypass {
        last
    } else {
        vec![options[1][0].clone()]
    });
    options.into_iter().find(|o| 3 + parts_width(o) <= spare)
}

fn text(s: &str, style: Style) -> Span<'static> {
    Span::styled(sanitize_text(s).into_owned(), style)
}

fn badge(row: &SessionRow) -> (&'static str, Style) {
    match row.state.kind {
        StateKind::Working => ("●", Style::new().fg(colors().working)),
        StateKind::Waiting => ("◌", Style::new().fg(colors().waiting)),
        StateKind::Blocked => (
            "◆",
            Style::new()
                .fg(colors().accent)
                .add_modifier(Modifier::BOLD),
        ),
        StateKind::Exited if row.state.exit_code == Some(0) => ("✓", Style::new().fg(colors().ok)),
        StateKind::Exited => ("✗", Style::new().fg(colors().bad)),
        StateKind::Starting => ("○", dim()),
    }
}

pub(crate) fn state_label(row: &SessionRow) -> &'static str {
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

pub(crate) fn put(buf: &mut Buffer, x: u16, y: u16, s: &str, style: Style, max: u16) -> u16 {
    if max == 0 {
        return x;
    }
    buf.set_stringn(x, y, sanitize_text(s), max as usize, style)
        .0
}

pub fn render(frame: &mut Frame, st: &ViewState, screens: &dyn ScreenSource) {
    with_theme(st.theme, || render_all(frame, st, screens));
}

fn render_all(frame: &mut Frame, st: &ViewState, screens: &dyn ScreenSource) {
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
    if st.palette.is_some() {
        draw_palette(buf, area, st);
    }
    if st.git_open {
        super::git::draw_git(buf, area, st);
    }
    if st.summary.is_some() {
        super::summary::draw_summary(buf, area, st);
    }
    if st.qmenu.is_some() {
        draw_qmenu(buf, area, st);
    }
    if let Some(peek) = &st.peek {
        draw_peek(buf, area, peek);
    }
    draw_new_session(buf, area, st);
}

fn draw_sidebar(buf: &mut Buffer, area: Rect, st: &ViewState) {
    if area.is_empty() {
        return;
    }
    let hint = if st.filter.is_some() {
        "Ctrl-] f"
    } else {
        "Ctrl-] b"
    };
    let hint_w = hint.width() as u16;
    let hx = area.right().saturating_sub(hint_w + 1);
    let end = match &st.filter {
        Some(f) => {
            let avail = usize::from(hx.saturating_sub(area.x + 2)).max(4);
            let tail = query_tail(&f.query, avail);
            let text = if f.input {
                format!("/{tail}\u{2588}")
            } else {
                format!("/{tail}")
            };
            put(
                buf,
                area.x + 1,
                area.y,
                &text,
                Style::new().fg(colors().warn).add_modifier(Modifier::BOLD),
                area.width.saturating_sub(1),
            )
        }
        None => put(
            buf,
            area.x + 1,
            area.y,
            "SESSIONS",
            dim().add_modifier(Modifier::BOLD),
            area.width.saturating_sub(1),
        ),
    };
    if hx > end {
        put(buf, hx, area.y, hint, dim(), hint_w);
    }
    let sl = st.sidebar_layout(area);
    if sl.lines.is_empty() && st.filter.is_some() && area.height > 1 {
        put(
            buf,
            area.x + 1,
            area.y + 1,
            "no matching session",
            dim(),
            area.width.saturating_sub(1),
        );
    }
    let focused = st.focused_session();
    let mut nested = false;
    for (line, y) in &sl.lines {
        let y = *y;
        match line {
            SidebarLine::Header {
                key,
                count,
                collapsed,
            } => draw_group_header(buf, area, st, key, *count, *collapsed, y),
            SidebarLine::Tab { id, count } => draw_tab_header(buf, area, st, *id, *count, y),
            SidebarLine::Activity(id) => draw_activity(buf, area, st, id, nested, y),
            SidebarLine::Sub { session, id } => draw_sub(buf, area, st, session, id, nested, y),
            SidebarLine::Detail(id) => draw_detail(buf, area, st, id, nested, y),
            SidebarLine::Session { id, indent } => {
                nested = *indent;
                draw_session_row(buf, area, st, id, *indent, focused, y);
            }
        }
    }
    if let Some(y) = sl.new_row {
        let hovered = st
            .pointer
            .is_some_and(|(px, py)| py == y && px >= area.x && px < area.right());
        let style = if hovered {
            Style::new()
                .fg(colors().strong)
                .add_modifier(Modifier::BOLD)
        } else {
            dim()
        };
        put(
            buf,
            area.x + 1,
            y,
            "+ New session",
            style,
            area.width.saturating_sub(1),
        );
        if st.sidebar_focus == Some(SidebarItem::New) {
            buf.set_style(
                Rect::new(area.x, y, area.width, 1),
                Style::new().add_modifier(Modifier::REVERSED),
            );
        }
    }
}

fn draw_session_row(
    buf: &mut Buffer,
    area: Rect,
    st: &ViewState,
    id: &str,
    indent: bool,
    focused: Option<&str>,
    y: u16,
) {
    let off = if indent { 2 } else { 0 };
    let Some(row) = st.sessions.iter().find(|s| s.id == id) else {
        return;
    };
    if focused == Some(row.id.as_str()) {
        buf.set_style(
            Rect::new(area.x, y, area.width, 1),
            Style::new().bg(colors().sidebar_bg),
        );
    }
    let (glyph, style) = badge(row);
    put(buf, area.x + 1 + off, y, glyph, style, 1);
    let agent = st.agent_title(row);
    let (label, auto) = match agent {
        Some(t) => (t.text.as_str(), t.auto),
        None => (row.name.as_str(), false),
    };
    let name_style = if row.state.kind == StateKind::Blocked {
        style
    } else if st.needs.contains_key(&row.id) {
        Style::new().fg(colors().warn).add_modifier(Modifier::BOLD)
    } else if auto {
        dim()
    } else {
        Style::new()
    };
    let badge = st.sidebar_badge(area, &row.id, off);
    let shift = badge.map_or(0, |(_, w)| w + 1);
    if let Some((bx, bw)) = badge {
        put(
            buf,
            bx,
            y,
            &badge_text(st.open_in(&row.id)),
            Style::new().fg(colors().warn).add_modifier(Modifier::BOLD),
            bw,
        );
    }
    let hovered = st
        .pointer
        .is_some_and(|(px, py)| py == y && px >= area.x && px < area.right());
    if hovered {
        let stop_x = sidebar_button(area, Control::Stop).unwrap_or(area.right());
        let name_end = stop_x.saturating_sub(1);
        put(
            buf,
            area.x + 3 + off + shift,
            y,
            label,
            name_style,
            name_end.saturating_sub(area.x + 3 + off + shift),
        );
        for slot in [Control::Stop, Control::Close] {
            let ctl = slot_control(row, slot, true);
            if let Some(x) = sidebar_button(area, slot) {
                let on = st.pointer.is_some_and(|p| p.0 == x);
                put(buf, x, y, glyph_of(ctl), button_style(on, true), 1);
            }
        }
    } else {
        let name_x = area.x + 3 + off + shift;
        let avail = usize::from(area.right().saturating_sub(name_x + 1));
        let subs = st.subs_of(&row.id).len();
        let suffix_w = if subs > 0 {
            format!(" (+{subs})").width() as u16
        } else {
            0
        };
        let keep = label.width().min(MIN_NAME_W) + 1 + usize::from(suffix_w);
        let room = avail.saturating_sub(keep);
        let age = match row.you.and_then(you_label) {
            Some(you) if room >= you.width() + 1 + row.age.width() => {
                let rest = room - you.width() - 1;
                format!("{you} {}", age_label(state_label(row), &row.age, rest))
            }
            _ => age_label(state_label(row), &row.age, room),
        };
        let age_w = age.width() as u16;
        let age_x = area.right().saturating_sub(age_w + 1).max(name_x);
        let name_w = age_x.saturating_sub(name_x).saturating_sub(1);
        let fits = suffix_w > 0 && name_w > suffix_w;
        let name_room = if fits { name_w - suffix_w } else { name_w };
        let shown = if agent.is_some() {
            clip(label, usize::from(name_room))
        } else {
            label.to_string()
        };
        let drawn = put(buf, name_x, y, &shown, name_style, name_room);
        if fits {
            put(buf, drawn, y, &format!(" (+{subs})"), dim(), suffix_w);
        }
        put(
            buf,
            age_x,
            y,
            &age,
            dim(),
            area.right().saturating_sub(age_x),
        );
    }
    if st.sidebar_focus == Some(SidebarItem::Session(row.id.clone())) {
        buf.set_style(
            Rect::new(area.x, y, area.width, 1),
            Style::new().add_modifier(Modifier::REVERSED),
        );
    }
}

pub(crate) fn you_label(secs: u64) -> Option<String> {
    let mins = secs / 60;
    Some(match mins {
        0 => return None,
        1..=59 => format!("you {mins}m"),
        _ if mins.is_multiple_of(60) => format!("you {}h", mins / 60),
        _ => format!("you {}h {}m", mins / 60, mins % 60),
    })
}

fn draw_group_header(
    buf: &mut Buffer,
    area: Rect,
    st: &ViewState,
    key: &str,
    count: usize,
    collapsed: bool,
    y: u16,
) {
    let holds = collapsed
        && st
            .focused_session()
            .is_some_and(|id| st.group_holds(key, id));
    if holds {
        buf.set_style(
            Rect::new(area.x, y, area.width, 1),
            Style::new().bg(colors().sidebar_bg),
        );
    }
    put(
        buf,
        area.x + 1,
        y,
        if collapsed { "▸" } else { "▾" },
        dim(),
        1,
    );
    let tail = if collapsed {
        format!("({count})")
    } else {
        String::new()
    };
    let tail_w = tail.width() as u16;
    let text_x = area.x + 3;
    let tail_x = area.right().saturating_sub(tail_w + 1).max(text_x);
    if let Some((path, branch)) = st.group_title(key) {
        let room = tail_x.saturating_sub(text_x).saturating_sub(1);
        let text = fit(&path, branch.as_deref(), usize::from(room));
        put(
            buf,
            text_x,
            y,
            &text,
            Style::new().add_modifier(Modifier::BOLD),
            room,
        );
    }
    if collapsed {
        let style = if st.group_blocked(key) {
            Style::new().fg(colors().warn)
        } else {
            dim()
        };
        put(buf, tail_x, y, &tail, style, tail_w);
    }
    if st.sidebar_focus == Some(SidebarItem::Group(key.to_string())) {
        buf.set_style(
            Rect::new(area.x, y, area.width, 1),
            Style::new().add_modifier(Modifier::REVERSED),
        );
    }
}

fn draw_tab_header(buf: &mut Buffer, area: Rect, st: &ViewState, id: u64, count: usize, y: u16) {
    if st.active_tab() == Some(id) {
        buf.set_style(
            Rect::new(area.x, y, area.width, 1),
            Style::new().bg(colors().sidebar_bg),
        );
    }
    put(buf, area.x + 1, y, "▾", dim(), 1);
    let tail = format!("({count})");
    let tail_w = tail.width() as u16;
    let text_x = area.x + 3;
    let tail_x = area.right().saturating_sub(tail_w + 1).max(text_x);
    let room = tail_x.saturating_sub(text_x).saturating_sub(1);
    let title = fit(&st.tab_title(id), None, usize::from(room));
    put(
        buf,
        text_x,
        y,
        &title,
        Style::new().add_modifier(Modifier::BOLD),
        room,
    );
    let style = if st.tab_blocked(id) {
        Style::new().fg(colors().warn)
    } else {
        dim()
    };
    put(buf, tail_x, y, &tail, style, tail_w);
    if st.sidebar_focus == Some(SidebarItem::Tab(id)) {
        buf.set_style(
            Rect::new(area.x, y, area.width, 1),
            Style::new().add_modifier(Modifier::REVERSED),
        );
    }
}

fn draw_activity(buf: &mut Buffer, area: Rect, st: &ViewState, id: &str, nested: bool, y: u16) {
    let Some(line) = st.status_line(id) else {
        return;
    };
    let x = area.x + if nested { 5 } else { 3 };
    let right = area.right().saturating_sub(1);
    let clock_w = line.clock.as_ref().map_or(0, |c| c.width() as u16 + 1);
    let room = usize::from(right.saturating_sub(x + clock_w));
    put(buf, x, y, &clip(&line.text, room), dim(), room as u16);
    if let Some(clock) = line.clock {
        let cx = right.saturating_sub(clock.width() as u16);
        if cx > x {
            put(buf, cx, y, &clock, dim(), clock.width() as u16);
        }
    }
}

fn draw_sub(
    buf: &mut Buffer,
    area: Rect,
    st: &ViewState,
    session: &str,
    id: &str,
    nested: bool,
    y: u16,
) {
    let subs = st.subs_of(session);
    let Some(sub) = subs.iter().find(|s| s.id == id) else {
        return;
    };
    let x = area.x + if nested { 7 } else { 5 };
    let right = area.right().saturating_sub(1);
    let clock = st.sub_elapsed(sub);
    let clock_w = if clock.is_empty() {
        0
    } else {
        clock.width() as u16 + 1
    };
    let room = usize::from(right.saturating_sub(x + clock_w));
    put(buf, x, y, &clip(&sub.label, room), dim(), room as u16);
    if clock_w > 0 {
        let cx = right.saturating_sub(clock.width() as u16);
        if cx > x {
            put(buf, cx, y, &clock, dim(), clock.width() as u16);
        }
    }
    let hovered = st
        .pointer
        .is_some_and(|(px, py)| py == y && px >= area.x && px < area.right());
    if hovered {
        buf.set_style(
            Rect::new(area.x, y, area.width, 1),
            Style::new().add_modifier(Modifier::UNDERLINED),
        );
    }
}

fn draw_detail(buf: &mut Buffer, area: Rect, st: &ViewState, id: &str, nested: bool, y: u16) {
    let Some(loc) = st.locs.get(id) else {
        return;
    };
    let x = area.x + if nested { 5 } else { 3 };
    let room = area.right().saturating_sub(x + 1);
    let text = fit(&tail2(&loc.dir), loc.branch.as_deref(), usize::from(room));
    put(buf, x, y, &text, dim(), room);
}

pub(crate) fn button_style(hover: bool, enabled: bool) -> Style {
    match (hover, enabled) {
        (true, true) => Style::new()
            .fg(colors().strong)
            .add_modifier(Modifier::BOLD),
        (_, true) => dim(),
        (_, false) => dim().add_modifier(Modifier::DIM),
    }
}

fn glyph_of(ctl: Control) -> &'static str {
    match ctl {
        Control::Resume => "▶",
        Control::Interrupt => "⏸",
        Control::Stop => "■",
        Control::Close => "✕",
    }
}

fn draw_header_buttons(buf: &mut Buffer, p: &PaneRect, st: &ViewState) {
    let Some(row) = st.sessions.iter().find(|s| s.id == p.session) else {
        return;
    };
    if let Some(x0) = header_button(p.outer, Control::Interrupt) {
        buf.set_string(x0 - 1, p.outer.y, "       ", Style::new());
    }
    if let Some((x, w)) = st.header_badge(p.outer, &p.session) {
        let style = Style::new().fg(colors().warn).add_modifier(Modifier::BOLD);
        put(buf, x - 1, p.outer.y, " ", Style::new(), 1);
        put(
            buf,
            x,
            p.outer.y,
            &badge_text(st.open_in(&p.session)),
            style,
            w,
        );
    }
    for slot in [Control::Interrupt, Control::Stop, Control::Close] {
        let Some(x) = header_button(p.outer, slot) else {
            return;
        };
        let ctl = slot_control(row, slot, false);
        let glyph = glyph_of(ctl);
        let enabled = match ctl {
            Control::Interrupt => row.can_interrupt(),
            _ => true,
        };
        let on = st.pointer == Some((x, p.outer.y));
        put(buf, x, p.outer.y, glyph, button_style(on, enabled), 1);
    }
}

struct HeaderGeo {
    index: usize,
    shown: String,
    state: &'static str,
    dim_label: bool,
    spare: usize,
    scroll: Option<String>,
}

fn scroll_label(p: &PaneRect, st: &ViewState) -> Option<String> {
    match (st.scroll, st.focused_session() == Some(p.session.as_str())) {
        (Some(sc), true) => Some(format!(" · scroll -{}/{}", sc.offset, sc.max)),
        _ => None,
    }
}

fn header_geo(p: &PaneRect, st: &ViewState) -> Option<HeaderGeo> {
    let index = st
        .panes()
        .iter()
        .position(|id| *id == p.session)
        .unwrap_or(0)
        + 1;
    let scroll = scroll_label(p, st);
    let row = st.sessions.iter().find(|s| s.id == p.session)?;
    let agent = st.agent_title(row);
    let (label, auto) = match agent {
        Some(t) => (t.text.as_str(), t.auto),
        None => (row.name.as_str(), false),
    };
    let state = state_label(row);
    let buttons = if p.outer.width >= HEADER_MIN_WIDTH {
        8
    } else {
        0
    };
    let fixed = 1
        + index.to_string().width()
        + 2
        + 3
        + state.width()
        + scroll.as_ref().map_or(0, |s| s.width())
        + 1;
    let badge = st
        .header_badge(p.outer, &p.session)
        .map_or(0, |(_, w)| usize::from(w) + 2);
    let room = usize::from(p.outer.width.saturating_sub(2 + buttons)).saturating_sub(fixed + badge);
    let shown = if agent.is_some() {
        clip(label, room)
    } else {
        label.to_string()
    };
    let spare = room.saturating_sub(shown.width());
    Some(HeaderGeo {
        index,
        shown,
        state,
        dim_label: auto,
        spare,
        scroll,
    })
}

fn header_chip_text(st: &ViewState, p: &PaneRect, geo: &HeaderGeo) -> Option<String> {
    let chip = st.chip_of(&p.session)?;
    super::chip::chip_text(chip, st.now_s, geo.spare)
}

pub(crate) fn header_chip(st: &ViewState, p: &PaneRect) -> Option<(u16, u16)> {
    let geo = header_geo(p, st)?;
    let chip = header_chip_text(st, p, &geo)?;
    let before = 1 + geo.index.to_string().width() + 2 + geo.shown.width() + 3 + geo.state.width();
    let x = p.outer.x + 1 + u16::try_from(before).ok()? + 3;
    let w = u16::try_from(chip.width()).ok()?.checked_sub(3)?;
    Some((x, w))
}

fn header_git_text(st: &ViewState, p: &PaneRect, geo: &HeaderGeo, chip_w: usize) -> Option<String> {
    st.git
        .get(&p.session)?
        .badge(geo.spare.saturating_sub(chip_w))
}

pub(crate) fn header_git(st: &ViewState, p: &PaneRect) -> Option<(u16, u16)> {
    let geo = header_geo(p, st)?;
    let chip_w = header_chip_text(st, p, &geo).map_or(0, |c| c.width());
    let badge = header_git_text(st, p, &geo, chip_w)?;
    let before =
        1 + geo.index.to_string().width() + 2 + geo.shown.width() + 3 + geo.state.width() + chip_w;
    let x = p.outer.x + 1 + u16::try_from(before).ok()? + 3;
    let w = u16::try_from(badge.width()).ok()?.checked_sub(3)?;
    Some((x, w))
}

fn pane_title(p: &PaneRect, st: &ViewState) -> Line<'static> {
    let geo = header_geo(p, st);
    let index = geo.as_ref().map_or_else(
        || {
            st.panes()
                .iter()
                .position(|id| *id == p.session)
                .unwrap_or(0)
                + 1
        },
        |g| g.index,
    );
    let mut spans = vec![
        text(" ", Style::new()),
        text(
            &index.to_string(),
            Style::new().add_modifier(Modifier::REVERSED),
        ),
    ];
    let mut scroll = scroll_label(p, st);
    match geo {
        Some(geo) => {
            let label_style = if geo.dim_label { dim() } else { Style::new() };
            spans.push(text("  ", Style::new()));
            spans.push(text(&geo.shown, label_style));
            spans.push(text(&format!(" · {}", geo.state), Style::new()));
            let mut spare = geo.spare;
            if let Some(chip) = header_chip_text(st, p, &geo) {
                spare = spare.saturating_sub(chip.width());
                spans.push(text(&chip, dim()));
            }
            let chip_w = header_chip_text(st, p, &geo).map_or(0, |c| c.width());
            if let Some(badge) = header_git_text(st, p, &geo, chip_w) {
                spare = spare.saturating_sub(badge.width());
                spans.push(text(&badge, dim()));
            }
            if let Some(parts) = header_meta(st, &p.session, spare) {
                spare = spare.saturating_sub(3 + parts_width(&parts));
                spans.push(text(" · ", dim()));
                spans.extend(part_spans(&parts, dim()));
            }
            if let Some(line) = st.status_line(&p.session) {
                let clock = line.clock.map(|c| format!(" · {c}")).unwrap_or_default();
                let head = spare.saturating_sub(clock.width());
                if head >= 12 {
                    let tail = format!(" · {}", line.text);
                    spans.push(text(&clip(&tail, head), dim()));
                    spans.push(text(&clock, dim()));
                }
            }
            scroll = geo.scroll;
        }
        None => spans.push(text(&format!("  {}", p.session), Style::new())),
    }
    if let Some(scroll) = scroll {
        spans.push(text(&scroll, Style::new().fg(colors().warn)));
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
        (true, true) => Style::new()
            .fg(colors().accent)
            .add_modifier(Modifier::BOLD),
        (true, false) => Style::new().fg(colors().accent),
        (false, true) => Style::new().fg(colors().strong),
        (false, false) => dim(),
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title(pane_title(p, st))
        .render(p.outer, buf);
    draw_header_buttons(buf, p, st);
    if let (Some((text, on)), Some(r)) = (
        super::broadcast::marker(p, st),
        super::broadcast::marker_rect(p, st),
    ) {
        let style = if on {
            Style::new()
                .fg(colors().warn)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            dim()
        };
        put(buf, r.x, r.y, &text, style, r.width);
    }
    let inner = p.content();
    if inner.is_empty() {
        return;
    }
    Clear.render(inner, buf);
    match screens.snapshot(&p.session) {
        Some(snap) => {
            let mark = st
                .goto
                .as_ref()
                .filter(|g| st.scroll.is_some() && g.session == p.session);
            draw_snapshot(buf, inner, snap, mark);
            if st.scroll.is_some() && st.focused_session() == Some(p.session.as_str()) {
                super::find::draw_overlay(buf, p.outer, inner, snap, st, &p.session);
            }
        }
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

fn draw_snapshot(buf: &mut Buffer, inner: Rect, snap: &Snapshot, mark: Option<&GotoMark>) {
    for (r, line) in snap.lines.iter().take(inner.height as usize).enumerate() {
        let y = inner.y + r as u16;
        let mut x = inner.x;
        for span in &line.spans {
            if x >= inner.right() {
                break;
            }
            x = put(buf, x, y, &span.t, attr_style(&span.a), inner.right() - x);
        }
        if let Some(m) = mark {
            let from_bottom = snap.lines.len().saturating_sub(1 + r) + snap.scroll_offset;
            if from_bottom <= m.bottom && from_bottom + m.rows > m.bottom {
                buf.set_style(
                    Rect::new(inner.x, y, inner.width, 1),
                    Style::new().add_modifier(Modifier::REVERSED),
                );
            }
        }
    }
}

pub fn cursor(st: &ViewState, lay: &Layout, screens: &dyn ScreenSource) -> Option<(u16, u16)> {
    if let Some(ns) = &st.new_session {
        let area = Rect::new(lay.status.x, 0, lay.status.width, lay.status.bottom());
        return new_session_cursor(ns, area);
    }
    if let Some(p) = &st.palette {
        let area = Rect::new(lay.status.x, 0, lay.status.width, lay.status.bottom());
        let rect = palette_rect(area, st.entries().len().max(1));
        let inner = palette_inner(rect);
        if rect.is_empty() || inner.is_empty() {
            return None;
        }
        if let PaletteMode::Form(f) = &p.mode {
            let value = if f.field == 0 { &f.name } else { &f.text };
            let w = query_tail(value, usize::from(inner.width).saturating_sub(7)).width() as u16;
            return Some((
                (inner.x + 6 + w).min(inner.right() - 1),
                inner.y + f.field as u16,
            ));
        }
        if matches!(p.mode, PaletteMode::Delete(_)) {
            return None;
        }
        let w = query_tail(&p.query, query_avail(inner)).width() as u16;
        return Some(((inner.x + 2 + w).min(inner.right() - 1), inner.y));
    }
    if st.scroll.is_some()
        || st.help
        || st.sidebar_focus.is_some()
        || st.qmenu.is_some()
        || st.filter_input()
    {
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
            colors().working,
            n(&|r| r.state.kind == StateKind::Working),
        ),
        (
            "◌",
            colors().waiting,
            n(&|r| r.state.kind == StateKind::Waiting),
        ),
        (
            "◆",
            colors().accent,
            n(&|r| r.state.kind == StateKind::Blocked),
        ),
        (
            "✓",
            colors().ok,
            n(&|r| r.state.kind == StateKind::Exited && r.state.exit_code == Some(0)),
        ),
    ];
    let failed = n(&|r| r.state.kind == StateKind::Exited && r.state.exit_code != Some(0));
    if failed > 0 {
        items.push(("✗", colors().bad, failed));
    }
    let mut spans = Vec::new();
    for (i, (glyph, color, count)) in items.into_iter().enumerate() {
        if i > 0 {
            spans.push(text(" ", Style::new()));
        }
        spans.push(text(&format!("{glyph} {count}"), Style::new().fg(color)));
    }
    let asked = st.open_total();
    if asked > 0 {
        spans.push(text(" ", Style::new()));
        spans.push(text(&badge_text(asked), Style::new().fg(colors().warn)));
    }
    spans
}

fn draw_status(buf: &mut Buffer, area: Rect, st: &ViewState) {
    if area.is_empty() {
        return;
    }
    let note = match (&st.rename, &st.confirm) {
        (Some(r), _) => Some(format!(
            "{}: {}▏  Enter saves, Esc cancels",
            if r.tab { "rename tab" } else { "rename" },
            r.text
        )),
        (None, Some(c)) if c.remove => Some(format!("stop and remove {}? y/n", c.name)),
        (None, Some(c)) => Some(format!("stop {}? y/n", c.name)),
        _ if st.layout_offer.is_some() => st.layout_offer.as_ref().map(|o| {
            let mut parts = Vec::new();
            if !o.resume.is_empty() {
                parts.push(format!("resume {} exited", o.resume.len()));
            }
            if !o.start.is_empty() {
                parts.push(format!("start {} missing", o.start.len()));
            }
            format!("layout {}: {}? y/n", o.name, parts.join(", "))
        }),
        _ if st.sidebar_focus.is_some() => {
            Some("sidebar: Up/Down move, Enter opens, Esc/Right back".into())
        }
        _ => st.notice.clone(),
    };
    let focused = match st.focused_row() {
        Some(r) => format!("{} · {}", r.name, state_label(r)),
        None => "no sessions".to_string(),
    };
    let sep = || text(" · ", dim());
    let mut spans = vec![text(" ", Style::new())];
    if area.width < WIDE_COLS {
        match &note {
            Some(n) => spans.push(text(n, Style::new().fg(colors().warn))),
            None => spans.push(text(&focused, Style::new())),
        }
        if st.open_total() > 0 {
            spans.push(sep());
            spans.push(text(
                &badge_text(st.open_total()),
                Style::new().fg(colors().warn),
            ));
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
    match &note {
        Some(n) => spans.push(text(n, Style::new().fg(colors().warn))),
        None => {
            spans.push(text(&focused, Style::new()));
            spans.push(sep());
            spans.extend(counts(st));
        }
    }
    let hint = "Ctrl-] ? help";
    let hint_w = hint.width() as u16 + 1;
    let used: usize = spans.iter().map(|s| s.content.width()).sum();
    let usage = st.usage_parts();
    let usage_w = if usage.is_empty() {
        0
    } else {
        parts_width(&usage) + 2
    };
    let usage_x = usize::from(area.width.saturating_sub(hint_w)).checked_sub(usage_w);
    if let Some(ux) = usage_x.filter(|ux| !usage.is_empty() && *ux > used + 2) {
        let line = Line::from(part_spans(&usage, Style::new()));
        buf.set_line(area.x + ux as u16, area.y, &line, usage_w as u16);
    }
    buf.set_line(
        area.x,
        area.y,
        &Line::from(spans),
        area.width.saturating_sub(hint_w),
    );
    put(buf, area.right() - hint_w, area.y, hint, dim(), hint_w - 1);
}

const MOUSE: &[(&str, &str)] = &[
    ("click", "pick a tab, a session or a pane"),
    ("click +", "new session (sidebar, last row)"),
    ("wheel", "scroll the history of the pane"),
    ("buttons", "header: pause/resume, stop, close"),
    ("drop", "a dropped file is pasted there"),
    (
        "drag",
        "a line resizes; Shift-drag selects (iTerm2: Option)",
    ),
];

pub(crate) enum HelpKey {
    Cmds(&'static [Cmd]),
    Lit(&'static str),
}

pub(crate) const HELP: &[(HelpKey, &str)] = &[
    (
        HelpKey::Cmds(&[Cmd::Palette]),
        "command palette: actions and sessions",
    ),
    (
        HelpKey::Cmds(&[Cmd::Detach]),
        "detach, sessions keep running",
    ),
    (
        HelpKey::Cmds(&[Cmd::NextSession, Cmd::PrevSession, Cmd::NeedsYou]),
        "next / previous / needs-you session",
    ),
    (HelpKey::Cmds(&[Cmd::Picker]), "session picker"),
    (
        HelpKey::Cmds(&[Cmd::Split]),
        "two panes, or collapse to one",
    ),
    (
        HelpKey::Cmds(&[Cmd::SplitRight, Cmd::SplitDown]),
        "split right / split down",
    ),
    (
        HelpKey::Cmds(&[Cmd::NewSession, Cmd::Prompts, Cmd::Layouts]),
        "new session / saved prompts / named layouts",
    ),
    (
        HelpKey::Cmds(&[Cmd::FocusOther]),
        "focus next pane (Shift-Tab previous)",
    ),
    (
        HelpKey::Cmds(&[Cmd::Focus(Nav::Left)]),
        "move focus (Shift+arrows: resize)",
    ),
    (
        HelpKey::Lit("Left at edge"),
        "focus the sidebar (Up/Down, Enter, Esc)",
    ),
    (
        HelpKey::Cmds(&[Cmd::Zoom, Cmd::Sidebar, Cmd::Filter]),
        "zoom / show or hide the sidebar / filter",
    ),
    (
        HelpKey::Cmds(&[Cmd::NextTab, Cmd::PrevTab, Cmd::RenameTab, Cmd::CloseTab]),
        "tab: next / previous / rename / close",
    ),
    (
        HelpKey::Cmds(&[
            Cmd::Group,
            Cmd::Filter,
            Cmd::SidebarNarrower,
            Cmd::SidebarWider,
        ]),
        "group by tab / repo, filter, sidebar width",
    ),
    (
        HelpKey::Cmds(&[Cmd::Rename, Cmd::OpenTask, Cmd::Git, Cmd::Summary]),
        "rename / its task / git panel / summary",
    ),
    (
        HelpKey::Cmds(&[Cmd::Broadcast, Cmd::BroadcastMark]),
        "broadcast typing to several panes / pick panes",
    ),
    (
        HelpKey::Cmds(&[Cmd::Search, Cmd::CopyMode]),
        "search scrollback (n older / N newer) / copy mode (v select, y copy)",
    ),
    (HelpKey::Cmds(&[Cmd::Resume]), "resume an exited session"),
    (
        HelpKey::Cmds(&[Cmd::Stop, Cmd::Remove]),
        "stop (asks first) / remove if exited",
    ),
    (
        HelpKey::Cmds(&[Cmd::ScrollMode]),
        "scroll back through history",
    ),
    (
        HelpKey::Cmds(&[Cmd::Questions, Cmd::Subagents, Cmd::Help]),
        "agent questions / subagents / this help",
    ),
    (
        HelpKey::Lit("Ctrl-]"),
        "send a literal Ctrl-] to the session",
    ),
];

const USAGE_LEGEND: (&str, &str) = ("Status bar", "5h/wk Claude, cx Codex, $ cost; ~ old, ? n/a");

fn help_key(key: &HelpKey) -> String {
    match key {
        HelpKey::Lit(s) => (*s).to_string(),
        HelpKey::Cmds(cmds) => cmds
            .iter()
            .filter_map(|c| find(*c).map(|a| a.hotkey))
            .collect::<Vec<_>>()
            .join(" / "),
    }
}

fn draw_peek(buf: &mut Buffer, area: Rect, peek: &Peek) {
    let w = area.width.min(100);
    let h = area.height.saturating_sub(2).min(30);
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
    let title = format!(
        " Subagent · {} · read-only ",
        clip(&peek.title, usize::from(w).saturating_sub(24))
    );
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(title)
        .render(r, buf);
    let inner = r.inner(ratatui::layout::Margin::new(1, 1));
    if inner.height < 2 {
        return;
    }
    let body = usize::from(inner.height - 1);
    if peek.lines.is_empty() {
        put(
            buf,
            inner.x,
            inner.y,
            "no transcript yet",
            dim(),
            inner.width,
        );
    } else {
        let end = peek
            .lines
            .len()
            .saturating_sub(peek.back)
            .max(body.min(peek.lines.len()));
        let start = end.saturating_sub(body);
        for (i, line) in peek.lines[start..end].iter().enumerate() {
            let text = clip(line, usize::from(inner.width));
            put(
                buf,
                inner.x,
                inner.y + i as u16,
                &text,
                Style::new(),
                inner.width,
            );
        }
    }
    let hint = if peek.back > 0 {
        format!("Esc closes · Up/Down scroll · G follows · -{}", peek.back)
    } else {
        "Esc closes · Up/Down scroll · g top".to_string()
    };
    put(buf, inner.x, inner.bottom() - 1, &hint, dim(), inner.width);
}

fn draw_help(buf: &mut Buffer, area: Rect) {
    let w = 64.min(area.width);
    let h = ((HELP.len() + MOUSE.len()) as u16 + 9).min(area.height);
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
    let mouse_fits = inner.y + 4 + ((HELP.len() + MOUSE.len()) as u16) < inner.bottom();
    let rows = HELP
        .iter()
        .map(|(k, what)| ((help_key(k), *what), false))
        .chain(
            MOUSE
                .iter()
                .filter(|_| mouse_fits)
                .map(|(k, what)| (((*k).to_string(), *what), true)),
        );
    for (i, ((key, what), mouse)) in rows.enumerate() {
        let y = inner.y + 2 + i as u16 + u16::from(mouse);
        if y >= inner.bottom() {
            return;
        }
        put(
            buf,
            inner.x,
            y,
            &key,
            Style::new().add_modifier(Modifier::BOLD),
            inner.width,
        );
        let x = inner.x + 14;
        if x < inner.right() {
            put(buf, x, y, what, Style::new(), inner.right() - x);
        }
    }
    let mut shown = HELP.len() + if mouse_fits { MOUSE.len() + 1 } else { 0 };
    let legend_y = inner.y + 3 + shown as u16;
    if mouse_fits && legend_y + 2 <= inner.bottom() {
        let (key, what) = USAGE_LEGEND;
        put(
            buf,
            inner.x,
            legend_y,
            key,
            Style::new().add_modifier(Modifier::BOLD),
            inner.width,
        );
        put(
            buf,
            inner.x + 14,
            legend_y,
            what,
            Style::new(),
            inner.width - 14,
        );
        shown += 1;
    }
    let y = inner.y + 3 + shown as u16;
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

fn draw_palette(buf: &mut Buffer, area: Rect, st: &ViewState) {
    let Some(p) = &st.palette else {
        return;
    };
    let rows = st.entries();
    let rect = palette_rect(area, rows.len().max(1));
    if rect.is_empty() {
        return;
    }
    Clear.render(rect, buf);
    let title = match &p.mode {
        PaletteMode::Commands => " Command palette ",
        PaletteMode::Prompts => " Saved prompts ",
        PaletteMode::Form(f) if f.index.is_some() => " Edit prompt ",
        PaletteMode::Form(_) => " Add prompt ",
        PaletteMode::Delete(_) => " Delete prompt ",
        PaletteMode::Layouts => " Layouts ",
        PaletteMode::DeleteLayout(_) => " Delete layout ",
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(title)
        .render(rect, buf);
    let inner = palette_inner(rect);
    if inner.is_empty() {
        return;
    }
    match &p.mode {
        PaletteMode::Form(f) => {
            let w = usize::from(inner.width).saturating_sub(7);
            for (i, (label, value)) in [("Name", &f.name), ("Text", &f.text)]
                .into_iter()
                .enumerate()
            {
                let y = inner.y + i as u16;
                let x = put(buf, inner.x, y, &format!("{label}  "), dim(), inner.width);
                let style = if f.field == i { Style::new() } else { dim() };
                put(buf, x, y, query_tail(value, w), style, inner.right() - x);
            }
            let list = palette_list(rect);
            put(
                buf,
                list.x,
                list.y,
                "Tab switches field, Enter saves, Esc cancels",
                dim(),
                list.width,
            );
            return;
        }
        PaletteMode::Delete(_) | PaletteMode::DeleteLayout(_) => {
            let name = match &p.mode {
                PaletteMode::DeleteLayout(idx) => {
                    st.layouts.get(*idx).map_or("", |l| l.name.as_str())
                }
                PaletteMode::Delete(idx) => st.prompts.get(*idx).map_or("", |p| p.name.as_str()),
                _ => "",
            };
            put(
                buf,
                inner.x,
                inner.y,
                &format!(
                    "Delete \"{}\"?",
                    fit(name, None, usize::from(inner.width).saturating_sub(10))
                ),
                Style::new().fg(colors().warn),
                inner.width,
            );
            let list = palette_list(rect);
            put(
                buf,
                list.x,
                list.y,
                "y deletes, any other key cancels",
                dim(),
                list.width,
            );
            return;
        }
        _ => {}
    }
    let x = put(buf, inner.x, inner.y, "> ", dim(), inner.width);
    let tail = query_tail(&p.query, query_avail(inner));
    put(buf, x, inner.y, tail, Style::new(), inner.right() - x);
    if matches!(p.mode, PaletteMode::Prompts | PaletteMode::Layouts) && p.query.is_empty() {
        let hint = if p.mode == PaletteMode::Layouts {
            "type a name to save · Ctrl-D delete"
        } else {
            "Ctrl-E edit · Ctrl-D delete"
        };
        let hw = hint.width() as u16;
        if x + hw + 1 < inner.right() {
            put(buf, inner.right() - hw, inner.y, hint, dim(), hw);
        }
    }
    let list = palette_list(rect);
    if list.is_empty() {
        return;
    }
    if rows.is_empty() {
        put(buf, list.x, list.y, "no match", dim(), list.width);
        return;
    }
    let sel = p.sel.min(rows.len() - 1);
    let top = palette_top(sel, usize::from(list.height));
    for (i, row) in rows
        .iter()
        .skip(top)
        .take(usize::from(list.height))
        .enumerate()
    {
        let y = list.y + i as u16;
        let selected = top + i == sel;
        if selected {
            buf.set_style(
                Rect::new(list.x, y, list.width, 1),
                Style::new().add_modifier(Modifier::REVERSED),
            );
        }
        draw_palette_row(buf, list, y, row);
    }
}

fn draw_palette_row(buf: &mut Buffer, list: Rect, y: u16, row: &RowView) {
    let right_w = row.right.width() as u16;
    let show_right = right_w + 2 < list.width;
    let label_max = if show_right {
        list.width - right_w - 1
    } else {
        list.width
    };
    let mut x = list.x;
    let end = list.x + label_max;
    for (idx, c) in row.label.chars().enumerate() {
        let w = c.width().unwrap_or(0) as u16;
        if x + w > end {
            break;
        }
        let style = if row.hits.contains(&idx) {
            Style::new().add_modifier(Modifier::BOLD)
        } else {
            Style::new()
        };
        put(buf, x, y, c.encode_utf8(&mut [0; 4]), style, end - x);
        x += w;
    }
    if show_right {
        put(buf, list.right() - right_w, y, &row.right, dim(), right_w);
    }
}
