use super::app::{App, Done, Focus, Form, Hit, Palette, Picker, Screen};
use super::cmdline::Value;
use super::fuzzy;
use super::guard;
use super::model::Kind;
use super::picker;
use crate::cmd::board::theme::ColorLevel;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

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

    fn good(&self) -> Style {
        self.color(Color::Green)
    }

    fn warn(&self) -> Style {
        self.color(Color::Yellow)
    }

    fn bad(&self) -> Style {
        self.color(Color::Red)
    }

    fn dim(&self) -> Style {
        Style::new().add_modifier(Modifier::DIM)
    }

    fn selected(&self) -> Style {
        if self.level == ColorLevel::None {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        }
    }

    fn block<'a>(&self, title: &'a str) -> Block<'a> {
        Block::default()
            .borders(Borders::ALL)
            .border_style(self.dim())
            .title(Span::styled(format!(" {title} "), self.accent()))
    }
}

pub fn draw(f: &mut Frame, app: &App, look: Look) {
    let area = f.area();
    match &app.screen {
        Screen::Home => home(f, area, app, look),
        Screen::Form(form) => form_view(f, area, form, look),
        Screen::Done(done) => done_view(f, area, done, look),
    }
    if let Some(p) = &app.picker {
        picker_view(f, area, p, look);
    }
    if let Some(p) = &app.palette {
        palette(f, area, app, p, look);
    }
    if app.help {
        help(f, area, look);
    }
    if let Some(n) = &app.notice {
        let line = Rect {
            y: area.y + area.height.saturating_sub(1),
            height: 1,
            ..area
        };
        f.render_widget(Clear, line);
        f.render_widget(Paragraph::new(Span::styled(n.clone(), look.warn())), line);
    }
}

fn keys(look: Look, pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (k, what) in pairs {
        spans.push(Span::styled(format!(" {k} "), look.accent()));
        spans.push(Span::styled(format!("{what}  "), look.dim()));
    }
    Line::from(spans)
}

fn home(f: &mut Frame, area: Rect, app: &App, look: Look) {
    let [head, body, foot] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .areas(area);
    f.render_widget(
        Paragraph::new(Span::styled(app.header.clone(), look.accent())),
        head,
    );
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).areas(body);
    let mut lines = vec![
        Line::from(Span::styled("/ search commands", look.dim())),
        Line::raw(""),
    ];
    for (i, g) in app.groups.iter().enumerate() {
        let sel = i == app.group;
        let style = if sel && app.focus == Focus::Groups {
            look.selected()
        } else if sel {
            look.accent()
        } else {
            Style::new()
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} {:<9}", if sel { ">" } else { " " }, g.label),
                style,
            ),
            Span::styled(format!(" {}", g.summary), look.dim()),
        ]));
    }
    f.render_widget(Paragraph::new(lines).block(look.block("Commands")), left);
    let mut lines = Vec::new();
    if let Some(g) = app.groups.get(app.group) {
        for (i, e) in g.entries.iter().enumerate() {
            let sel = i == app.entry && app.focus == Focus::Entries;
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} {:<22}", if sel { ">" } else { " " }, e.label),
                    if sel { look.selected() } else { look.good() },
                ),
                Span::raw(format!(" {}", e.about)),
            ]));
        }
    }
    let title = app.groups.get(app.group).map_or("", |g| g.label.as_str());
    f.render_widget(Paragraph::new(lines).block(look.block(title)), right);
    f.render_widget(
        Paragraph::new(keys(
            look,
            &[
                ("\u{2191}\u{2193}", "move"),
                ("enter", "open"),
                ("/", "search"),
                ("?", "help"),
                ("esc", "back"),
                ("q", "quit"),
            ],
        )),
        foot,
    );
}

fn select_line(look: Look, options: &[String], current: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut all = vec![String::new()];
    all.extend(options.iter().cloned());
    for o in all {
        let on = o == current;
        let name = if o.is_empty() {
            "default".to_string()
        } else {
            o
        };
        spans.push(Span::styled(
            format!("({}) {}  ", if on { "\u{25cf}" } else { " " }, name),
            if on { look.accent() } else { Style::new() },
        ));
    }
    spans
}

fn form_view(f: &mut Frame, area: Rect, form: &Form, look: Look) {
    let [crumb, body, cmd, foot] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(4),
        Constraint::Length(4),
        Constraint::Length(1),
    ])
    .areas(area);
    let mut path = vec![Span::styled("tmgr", look.accent())];
    for p in &form.node.path {
        path.push(Span::styled(" \u{203a} ", look.dim()));
        path.push(Span::styled(p.clone(), look.accent()));
    }
    f.render_widget(
        Paragraph::new(vec![
            Line::from(path),
            Line::from(Span::styled(form.node.about.clone(), look.dim())),
        ]),
        crumb,
    );
    let mut lines = Vec::new();
    if form.node.fields.is_empty() {
        lines.push(Line::from(Span::styled(
            "No options. Press enter to run.",
            look.dim(),
        )));
    }
    for (i, field) in form.node.fields.iter().enumerate() {
        let focused = i == form.cursor;
        let mark = if field.required { "*" } else { " " };
        let label = format!(
            "{} {}{:<16}",
            if focused { ">" } else { " " },
            mark,
            field.label()
        );
        let mut spans = vec![Span::styled(
            label,
            if focused { look.accent() } else { Style::new() },
        )];
        match &field.kind {
            Kind::Select(options) => {
                let cur = match form.values.get(&field.id) {
                    Some(Value::Text(t)) => t.clone(),
                    _ => String::new(),
                };
                spans.extend(select_line(look, options, &cur));
            }
            _ => {
                let mut v = form.shown_value(i);
                if v.is_empty() {
                    if let Some(d) = &field.default {
                        spans.push(Span::styled(format!("({d}) "), look.dim()));
                    } else if !focused {
                        spans.push(Span::styled(field.help.clone(), look.dim()));
                    }
                }
                if focused && matches!(field.kind, Kind::Text | Kind::Path | Kind::Multi) {
                    v.push('_');
                }
                spans.push(Span::raw(v));
            }
        }
        if focused && picker::source_for(&form.node.path, field).is_some() {
            spans.push(Span::styled("   ctrl+o pick", look.dim()));
        }
        lines.push(Line::from(spans));
        if focused && !field.help.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("    {}", field.help),
                look.dim(),
            )));
        }
    }
    if let Some(e) = &form.error {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(e.clone(), look.bad())));
    }
    if form.review {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "This command asks for a typed confirmation. The menu never types it for you:",
            look.warn(),
        )));
        lines.push(Line::from(Span::styled(
            "after enter you answer it in the terminal, exactly as without the menu.",
            look.warn(),
        )));
        lines.push(Line::from(Span::styled(
            "enter: continue    any other key: back to the form",
            look.dim(),
        )));
    } else if guard::is_guarded(&form.node, &form.values) {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "Guarded: asks for a typed confirmation in the terminal.",
            look.warn(),
        )));
    }
    f.render_widget(Paragraph::new(lines).block(look.block("Options")), body);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("$ ", look.good()),
            Span::styled(form.display(), look.good()),
        ]))
        .wrap(Wrap { trim: false })
        .block(look.block("Command")),
        cmd,
    );
    f.render_widget(
        Paragraph::new(keys(
            look,
            &[
                ("tab", "next field"),
                ("space", "select"),
                ("enter", "run"),
                ("ctrl+y", "copy command"),
                ("esc", "back"),
            ],
        )),
        foot,
    );
}

fn done_view(f: &mut Frame, area: Rect, done: &Done, look: Look) {
    let [body, foot] = Layout::vertical([Constraint::Min(4), Constraint::Length(1)]).areas(area);
    let (text, style) = match done.code {
        Some(0) => ("Finished".to_string(), look.good()),
        Some(c) => (format!("Exited with code {c}"), look.bad()),
        None => ("Could not start the command".to_string(), look.bad()),
    };
    let lines = vec![
        Line::from(Span::styled(text, style)),
        Line::raw(""),
        Line::from(Span::styled("Same command next time:", look.accent())),
        Line::from(vec![
            Span::styled("$ ", look.good()),
            Span::styled(done.command.clone(), look.good()),
        ]),
    ];
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(look.block(&done.node.path.join(" "))),
        body,
    );
    f.render_widget(
        Paragraph::new(keys(
            look,
            &[
                ("enter", "menu"),
                ("e", "edit and run again"),
                ("ctrl+y", "copy command"),
                ("q", "quit"),
            ],
        )),
        foot,
    );
}

fn palette(f: &mut Frame, area: Rect, app: &App, p: &Palette, look: Look) {
    let w = area.width.saturating_sub(8).clamp(30, 90);
    let h = (p.hits.len().min(10) as u16 + 5).min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + area.height / 6,
        width: w.min(area.width),
        height: h,
    };
    f.render_widget(Clear, rect);
    let mut lines = vec![
        Line::from(vec![
            Span::styled("\u{203a} ", look.accent()),
            Span::raw(format!("{}_", p.query)),
        ]),
        Line::raw(""),
    ];
    for (i, hit) in p.hits.iter().take(10).enumerate() {
        let (label, about) = app.hit_label(hit);
        let sel = i == p.cursor;
        let hits = fuzzy::matched(&p.query, &label);
        let mut spans: Vec<Span> = label
            .chars()
            .enumerate()
            .map(|(ci, c)| {
                let base = if sel {
                    look.selected()
                } else if matches!(hit, Hit::Recent(_)) {
                    look.dim()
                } else {
                    Style::new()
                };
                let st = if hits.contains(&ci) && !sel {
                    base.add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else {
                    base
                };
                Span::styled(c.to_string(), st)
            })
            .collect();
        if !about.is_empty() {
            spans.push(Span::styled(format!("   {about}"), look.dim()));
        }
        lines.push(Line::from(spans));
    }
    if p.hits.is_empty() {
        lines.push(Line::from(Span::styled("No matching command", look.dim())));
    }
    lines.push(Line::raw(""));
    lines.push(keys(
        look,
        &[
            ("enter", "open"),
            ("tab", "fill flags"),
            ("ctrl+y", "copy as command"),
        ],
    ));
    f.render_widget(
        Paragraph::new(lines).block(look.block("Command palette")),
        rect,
    );
}

fn help(f: &mut Frame, area: Rect, look: Look) {
    let w = area.width.saturating_sub(8).clamp(30, 64);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + area.height / 6,
        width: w.min(area.width),
        height: 14.min(area.height),
    };
    f.render_widget(Clear, rect);
    let rows = [
        ("\u{2191}\u{2193} / j k", "move"),
        ("enter", "open or run"),
        ("/  or ctrl+p", "command palette"),
        ("tab, shift+tab", "next / previous field in a form"),
        ("space, \u{2190}\u{2192}", "toggle or choose a value"),
        ("ctrl+y", "copy the command line"),
        ("esc", "back"),
        ("q", "quit (in the menu)"),
        ("ctrl+c", "quit"),
    ];
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(format!("{k:<16}"), look.accent()),
                Span::raw(*d),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines).block(look.block("Keys")), rect);
}

fn picker_view(f: &mut Frame, area: Rect, p: &Picker, look: Look) {
    f.render_widget(Clear, area);
    let [search, body, foot] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(4),
        Constraint::Length(1),
    ])
    .areas(area);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("\u{203a} ", look.accent()),
            Span::raw(format!("{}_", p.query)),
        ]))
        .block(look.block(p.source.title())),
        search,
    );
    let [list, preview] =
        Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)]).areas(body);
    let mut lines: Vec<Line> = Vec::new();
    let mut side: Vec<Line> = Vec::new();
    match (&p.items, &p.error) {
        (_, Some(e)) => lines.push(Line::from(Span::styled(e.clone(), look.bad()))),
        (None, None) => lines.push(Line::from(Span::styled("Loading...", look.dim()))),
        (Some(items), None) => {
            let heads = p.source.heads();
            lines.push(Line::from(Span::styled(
                format!("  {}", heads.join("   ")),
                look.dim(),
            )));
            let rows = list.height.saturating_sub(3) as usize;
            let start = p.cursor.saturating_sub(rows.saturating_sub(1));
            for (n, idx) in p.shown.iter().enumerate().skip(start).take(rows) {
                let item = &items[*idx];
                let sel = n == p.cursor;
                let text = format!("{} {}", if sel { ">" } else { " " }, item.cols.join("   "));
                lines.push(Line::from(Span::styled(
                    text,
                    if sel { look.selected() } else { Style::new() },
                )));
            }
            if p.shown.is_empty() {
                lines.push(Line::from(Span::styled("Nothing matches", look.dim())));
            }
            if let Some(item) = p.shown.get(p.cursor).and_then(|i| items.get(*i)) {
                for (i, l) in item.preview.iter().enumerate() {
                    side.push(Line::from(Span::styled(
                        l.clone(),
                        if i == 0 { look.accent() } else { Style::new() },
                    )));
                }
            }
        }
    }
    f.render_widget(Paragraph::new(lines).block(look.block("Results")), list);
    f.render_widget(
        Paragraph::new(side)
            .wrap(Wrap { trim: false })
            .block(look.block("Preview")),
        preview,
    );
    f.render_widget(
        Paragraph::new(keys(
            look,
            &[
                ("type", "filter"),
                ("\u{2191}\u{2193}", "move"),
                ("enter", "use it"),
                ("esc", "back"),
            ],
        )),
        foot,
    );
}
