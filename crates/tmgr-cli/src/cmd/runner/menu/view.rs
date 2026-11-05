use super::memory::Kind;
use super::state::{
    Availability, Field, RowKind, Screen, Smart, State, CONFIRM_NEXT, FORK_KEEPS_SETTINGS,
    LIVE_REASON, READING_OFF,
};
use crate::cmd::board::theme::ColorLevel;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use tmgr_runner::sanitize::sanitize_text;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy)]
pub struct Look {
    pub level: ColorLevel,
}

impl Look {
    fn color(&self, c: Color) -> Style {
        if self.level == ColorLevel::None {
            Style::new()
        } else {
            Style::new().fg(c)
        }
    }

    fn accent(&self) -> Style {
        self.color(Color::Cyan).add_modifier(Modifier::BOLD)
    }

    fn warn(&self) -> Style {
        self.color(Color::Yellow)
    }

    fn dim(&self) -> Style {
        Style::new().add_modifier(Modifier::DIM)
    }

    fn selected(&self) -> Style {
        Style::new().add_modifier(Modifier::REVERSED)
    }
}

pub fn fit(s: &str, width: usize) -> String {
    let clean = sanitize_text(s);
    if clean.width() <= width {
        return clean.into_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in clean.chars() {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w + 1 > width {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('\u{2026}');
    out
}

fn line(text: &str, width: usize, style: Style) -> Line<'static> {
    Line::from(Span::styled(fit(text, width), style))
}

fn marker(on: bool) -> &'static str {
    if on {
        "\u{25b8} "
    } else {
        "  "
    }
}

fn keys(look: Look, text: &str, width: usize) -> Line<'static> {
    line(text, width, look.dim())
}

pub fn draw(f: &mut Frame, s: &State, look: Look) {
    let area = f.area();
    let title = match s.screen {
        Screen::Agent => " tmgr run \u{b7} agent ",
        Screen::Session => " tmgr run \u{b7} session ",
        Screen::Flags => " tmgr run \u{b7} start ",
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(title, look.accent()));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let width = inner.width as usize;
    let height = inner.height as usize;
    let mut lines = match s.screen {
        Screen::Agent => agent_lines(s, look, width),
        Screen::Session => session_lines(s, look, width, height),
        Screen::Flags => flags_lines(s, look, width),
    };
    lines.truncate(height);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn agent_lines(s: &State, look: Look, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![line("Which agent?", width, Style::new()), Line::raw("")];
    for (i, a) in s.agents.iter().enumerate() {
        let on = i == s.agent_cursor && a.installed;
        let text = if a.installed {
            format!("{}{}  ({})", marker(on), a.name, a.id)
        } else {
            format!("{}{}  ({}) not installed", marker(false), a.name, a.id)
        };
        let style = if !a.installed {
            look.dim()
        } else if on {
            look.selected()
        } else {
            Style::new()
        };
        lines.push(line(&text, width, style));
    }
    if s.agents.iter().all(|a| !a.installed) {
        lines.push(Line::raw(""));
        lines.push(line("No supported agent is installed.", width, look.warn()));
    }
    lines.push(Line::raw(""));
    lines.push(keys(
        look,
        "Up/Down move \u{b7} Enter select \u{b7} q quit",
        width,
    ));
    lines
}

fn session_lines(s: &State, look: Look, width: usize, height: usize) -> Vec<Line<'static>> {
    let mut head = vec![
        line(
            &format!(
                "{} sessions{}",
                s.agent().name,
                if s.loading { " (loading...)" } else { "" }
            ),
            width,
            Style::new(),
        ),
        match (&s.smart, &s.found) {
            (Smart::Asking(q), _) => line(&format!("Ask: {q}_"), width, look.accent()),
            (Smart::Running(q), _) => line(
                &format!(
                    "Asking {} about: {q} (Esc cancels)",
                    s.smart_agent().unwrap_or_default()
                ),
                width,
                look.accent(),
            ),
            (_, Some((q, _))) => line(
                &format!("Smart search: {q} (Esc clears)"),
                width,
                look.accent(),
            ),
            _ => line(&format!("Filter: {}_", s.filter), width, look.accent()),
        },
    ];
    let mut foot = Vec::new();
    if let Some(reason) = s.reason() {
        foot.push(line(reason, width, look.warn()));
    }
    for text in s.confirm_lines() {
        foot.push(line(&text, width, look.warn()));
    }
    if s.reading_hint() {
        foot.push(line(READING_OFF, width, look.warn()));
    }
    if let Some(n) = &s.notice {
        foot.push(line(n, width, look.warn()));
    }
    if let Some(status) = s.status() {
        foot.push(line(&status, width, look.dim()));
    }
    foot.push(match s.choice {
        Some(kind) => choice_line(kind, s, look, width),
        None => keys(
            look,
            &format!(
                "Up/Down move \u{b7} type to filter \u{b7} {}Enter select \u{b7} Esc back \u{b7} Ctrl+C quit",
                if s.smart_agent().is_some() { "Ctrl+A ask the agent \u{b7} " } else { "" }
            ),
            width,
        ),
    });
    let visible = s.visible();
    let room = height.saturating_sub(head.len() + foot.len()).max(1);
    let total = visible.len() + 1;
    let start = (s.cursor + 1)
        .saturating_sub(room)
        .min(total.saturating_sub(room));
    let mut list = Vec::new();
    for pos in start..(start + room).min(total) {
        let on = pos == s.cursor;
        if pos == 0 {
            let style = if on { look.selected() } else { look.accent() };
            list.push(line(&format!("{}New session", marker(on)), width, style));
            continue;
        }
        let row = &s.rows[visible[pos - 1]];
        let avail = State::availability(row);
        let tag = match row.kind {
            RowKind::Tmgr => "tmgr",
            RowKind::External => "ext ",
        };
        let mut text = format!(
            "{}{} {:<8} {} {}",
            marker(on),
            tag,
            row.when,
            row.project,
            row.text
        );
        match avail {
            Availability::Live => text.push_str(" (live)"),
            Availability::Ok if row.maybe_live => text.push_str(" (may still be running)"),
            Availability::FolderGone => text.push_str(" (folder gone)"),
            Availability::Unreadable => text.push_str(" (not readable)"),
            _ => {}
        }
        if let Some(why) = s.reason_for(&row.id) {
            text.push_str(&format!(" \u{2014} {why}"));
        }
        let style = if on {
            look.selected()
        } else if matches!(avail, Availability::Ok | Availability::Live) {
            Style::new()
        } else {
            look.dim()
        };
        list.push(line(&text, width, style));
    }
    head.extend(list);
    head.extend(foot);
    head
}

fn choice_line(kind: Kind, s: &State, look: Look, width: usize) -> Line<'static> {
    let live = s
        .focus_row()
        .is_some_and(|r| State::availability(r) == Availability::Live);
    let mark = |k: Kind, label: &str| {
        if k == kind {
            format!("[{label}]")
        } else {
            format!(" {label} ")
        }
    };
    let resume = if live {
        format!("({LIVE_REASON})")
    } else {
        mark(Kind::Resume, "r Resume")
    };
    let fork = match s.focus_row().and_then(|r| s.fork_blocked(r)) {
        Some(_) => "(fork unavailable)".to_string(),
        None => mark(Kind::Fork, "f Fork"),
    };
    let text = format!("{resume}  {fork}   Tab switch \u{b7} Enter confirm \u{b7} Esc cancel");
    line(&text, width, look.accent())
}

fn toggle(on: bool) -> &'static str {
    if on {
        "[x]"
    } else {
        "[ ]"
    }
}

fn flags_lines(s: &State, look: Look, width: usize) -> Vec<Line<'static>> {
    let what = match s.kind() {
        Kind::New => "New session".to_string(),
        Kind::Resume => "Resume".to_string(),
        Kind::Fork => "Fork".to_string(),
    };
    let mut lines = vec![
        line(
            &format!("{} \u{b7} {what}", s.agent().name),
            width,
            Style::new(),
        ),
        Line::raw(""),
    ];
    let lock = s.no_sandbox_lock();
    if s.fields().is_empty() {
        lines.push(line(FORK_KEEPS_SETTINGS, width, look.warn()));
    }
    for (i, field) in s.fields().into_iter().enumerate() {
        let on = i == s.flag_cursor;
        let text = match field {
            Field::Folder => format!(
                "{}Folder: {}",
                marker(on),
                if s.folder.is_empty() {
                    "(current folder)"
                } else {
                    &s.folder
                }
            ),
            Field::Bypass => format!(
                "{}{} Bypass permissions{}",
                marker(on),
                toggle(s.bypass()),
                if s.bypass_lock().is_some() {
                    " (locked)"
                } else {
                    ""
                }
            ),
            Field::NoSandbox => format!(
                "{}{} No sandbox{}",
                marker(on),
                toggle(s.no_sandbox()),
                if lock.is_some() { " (locked)" } else { "" }
            ),
        };
        let style = if on { look.selected() } else { Style::new() };
        lines.push(line(&text, width, style));
        if field == Field::Bypass {
            if let Some((_, why)) = s.bypass_lock() {
                lines.push(line(&format!("    {why}"), width, look.warn()));
            }
        }
        if field == Field::NoSandbox {
            if let Some((_, why)) = lock {
                lines.push(line(&format!("    {why}"), width, look.warn()));
            }
        }
    }
    if s.needs_confirmation() {
        lines.push(Line::raw(""));
        lines.push(line(CONFIRM_NEXT, width, look.warn()));
    }
    lines.push(Line::raw(""));
    lines.push(line("Command", width, look.dim()));
    lines.push(Line::from(Span::styled(
        sanitize_text(&s.command()).into_owned(),
        look.accent(),
    )));
    lines.push(Line::raw(""));
    lines.push(keys(
        look,
        "Up/Down move \u{b7} Space toggle \u{b7} Enter start \u{b7} Esc back \u{b7} Ctrl+C quit",
        width,
    ));
    lines
}
