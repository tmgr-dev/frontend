use super::app::{App, Card, Chip, Detail, Hits, Mode};
use super::theme::Theme;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;
use tmgr_core::text::format_duration;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const WIDE_MIN: u16 = 100;
const COLUMN_MIN: u16 = 18;
const CARD_STRIDE: usize = 3;

pub fn fit(s: &str, width: usize, ell: &str) -> String {
    if s.width() <= width {
        return s.to_string();
    }
    let ell_w = ell.width();
    if width <= ell_w {
        return ell.chars().take(width).collect();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > width - ell_w {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push_str(ell);
    out
}

fn pad_between(left: Vec<Span<'static>>, right: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let lw: usize = left.iter().map(|s| s.content.width()).sum();
    let rw: usize = right.iter().map(|s| s.content.width()).sum();
    let mut spans = left;
    if lw + rw < width {
        spans.push(Span::raw(" ".repeat(width - lw - rw)));
        spans.extend(right);
    }
    Line::from(spans)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Wide,
    Tabs,
    List,
}

fn shape(app: &App, width: u16) -> Shape {
    if width < 60 {
        Shape::List
    } else if width < WIDE_MIN || (width as usize) / app.columns.len().max(1) < COLUMN_MIN as usize
    {
        Shape::Tabs
    } else {
        Shape::Wide
    }
}

pub fn draw(f: &mut Frame, app: &App, theme: &Theme) {
    let area = f.area();
    let mut hits = Hits::default();
    if area.width < 20 || area.height < 6 {
        f.render_widget(Paragraph::new("terminal too small"), area);
        app.view.borrow_mut().hits = hits;
        return;
    }
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);
    f.render_widget(
        Paragraph::new(header_line(app, theme, header, &mut hits)),
        header,
    );

    let (board, details) = if app.details_open {
        if body.height >= 16 {
            let h = (body.height / 2).clamp(9, 16);
            let [b, d] = Layout::vertical([Constraint::Fill(1), Constraint::Length(h)]).areas(body);
            (Some(b), Some(d))
        } else {
            (None, Some(body))
        }
    } else {
        (Some(body), None)
    };
    if let Some(b) = board {
        match shape(app, area.width) {
            Shape::Wide => draw_columns(f, app, theme, b, &mut hits),
            Shape::Tabs => draw_tabs(f, app, theme, b, &mut hits),
            Shape::List => draw_list(f, app, theme, b, &mut hits),
        }
    }
    if let Some(d) = details {
        draw_details(f, app, theme, d);
    }
    f.render_widget(
        Paragraph::new(footer_line(app, theme, footer.width as usize)),
        footer,
    );
    if app.mode == Mode::Help {
        draw_help(f, theme, area);
    }
    if app.mode == Mode::Picker {
        draw_picker(f, app, theme, area, &mut hits);
    }
    app.view.borrow_mut().hits = hits;
}

fn caret(theme: &Theme) -> &'static str {
    if theme.is_ascii() {
        "v"
    } else {
        "\u{25be}"
    }
}

struct Seg {
    text: String,
    style: Style,
    chip: Option<Chip>,
}

fn seg(text: impl Into<String>, style: Style, chip: Option<Chip>) -> Seg {
    Seg {
        text: text.into(),
        style,
        chip,
    }
}

fn header_line(app: &App, theme: &Theme, area: Rect, hits: &mut Hits) -> Line<'static> {
    let width = area.width as usize;
    let ell = theme.ellipsis();
    let down = caret(theme);
    let mut left: Vec<Seg> = Vec::new();
    let f = &app.filters;
    let value_style = |active: bool| if active { theme.accent() } else { Style::new() };
    let narrow = |left: &mut Vec<Seg>| {
        let n = f.active();
        let label = match n {
            0 => "no filters".to_string(),
            1 => "1 filter".to_string(),
            n => format!("{n} filters"),
        };
        left.push(seg(
            fit(&app.ws.name, 24, ell),
            theme.bold(),
            Some(Chip::Menu),
        ));
        left.push(seg(
            format!("{}{label}", theme.sep()),
            if n > 0 { theme.accent() } else { theme.muted() },
            Some(Chip::Menu),
        ));
    };
    let mut wide: Vec<Seg> = Vec::new();
    if area.width >= WIDE_MIN {
        let left = &mut wide;
        left.push(seg(
            format!("[{} {down}]", fit(&app.ws.name, 22, ell)),
            theme.key(),
            Some(Chip::Workspace),
        ));
        left.push(seg("  ", Style::new(), None));
        left.push(seg("Project: ", theme.muted(), Some(Chip::Project)));
        left.push(seg(
            format!("{} {down}", fit(&app.project_label(), 12, ell)),
            value_style(f.project.is_some()),
            Some(Chip::Project),
        ));
        left.push(seg("  ", Style::new(), None));
        left.push(seg("Assignee: ", theme.muted(), Some(Chip::Assignee)));
        left.push(seg(
            format!("{} {down}", fit(&app.assignee_label(), 14, ell)),
            value_style(f.assignee != super::filters::Assignee::All),
            Some(Chip::Assignee),
        ));
        left.push(seg("  ", Style::new(), None));
        left.push(seg("Persona: ", theme.muted(), Some(Chip::Persona)));
        left.push(seg(
            format!("{} {down}", fit(&app.persona_label(), 14, ell)),
            value_style(f.persona != super::filters::PersonaSel::Any),
            Some(Chip::Persona),
        ));
        left.push(seg("  ", Style::new(), None));
        if f.search.trim().is_empty() {
            left.push(seg(
                format!("/search{ell}"),
                theme.muted(),
                Some(Chip::Search),
            ));
        } else {
            left.push(seg(
                format!("/{}", fit(f.search.trim(), 20, ell)),
                theme.accent(),
                Some(Chip::Search),
            ));
        }
    }
    let wide_w: usize = wide.iter().map(|s| s.text.width()).sum();
    if wide.is_empty() || wide_w + 14 > width {
        narrow(&mut left);
    } else {
        left = wide;
    }
    let sep = theme.sep();
    let mut right = Vec::new();
    if app.offline {
        right.push(Span::styled("offline".to_string(), theme.accent()));
        right.push(Span::styled(sep.to_string(), theme.muted()));
    } else if app.loading {
        right.push(Span::styled("refreshing".to_string(), theme.muted()));
        right.push(Span::styled(sep.to_string(), theme.muted()));
    }
    let more = if app.more { "+" } else { "" };
    right.push(Span::styled(
        format!("{}{more} tasks", app.visible_count()),
        theme.muted(),
    ));
    let mut x = area.x;
    let mut spans = Vec::new();
    for s in left {
        let w = s.text.width() as u16;
        if let Some(chip) = s.chip {
            let w = w.min(area.x + area.width - x.min(area.x + area.width));
            hits.chips.push((Rect::new(x, area.y, w, 1), chip));
        }
        x += w;
        spans.push(Span::styled(s.text, s.style));
    }
    pad_between(spans, right, width)
}

fn footer_line(app: &App, theme: &Theme, width: usize) -> Line<'static> {
    match app.mode {
        Mode::Filter => {
            return Line::from(vec![
                Span::styled("/ ", theme.key()),
                Span::raw(format!("{}{}", app.input, theme.cursor())),
            ])
        }
        Mode::Comment => {
            return Line::from(vec![
                Span::styled("comment ", theme.key()),
                Span::raw(format!(
                    "{}{}",
                    tail(&app.input, width.saturating_sub(10)),
                    theme.cursor()
                )),
            ])
        }
        _ => {}
    }
    if let Some(n) = &app.notice {
        let style = if n.error {
            theme.accent()
        } else {
            theme.good()
        };
        return Line::from(Span::styled(fit(&n.text, width, theme.ellipsis()), style));
    }
    if app.mode == Mode::Picker {
        return Line::from(vec![
            Span::styled("j k", theme.key()),
            Span::styled(" move  ", theme.muted()),
            Span::styled("enter", theme.key()),
            Span::styled(" choose  ", theme.muted()),
            Span::styled("esc", theme.key()),
            Span::styled(" close", theme.muted()),
        ]);
    }
    let hints: [(&str, &str); 13] = [
        ("h l", "column"),
        ("j k", "card"),
        ("H L", "move"),
        ("enter", "details"),
        ("s S", "timer"),
        ("c", "comment"),
        ("/", "search"),
        ("w", "workspace"),
        ("p a P", "filters"),
        ("x", "reset"),
        ("f", "menu"),
        ("?", "help"),
        ("q", "quit"),
    ];
    let mut spans = Vec::new();
    let mut used = 0;
    for (k, label) in hints {
        let w = k.width() + label.width() + 3;
        if used + w > width {
            break;
        }
        spans.push(Span::styled(k.to_string(), theme.key()));
        spans.push(Span::styled(format!(" {label}  "), theme.muted()));
        used += w;
    }
    Line::from(spans)
}

fn tail(s: &str, width: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut start = chars.len();
    let mut used = 0;
    while start > 0 {
        let w = chars[start - 1].width().unwrap_or(0);
        if used + w > width {
            break;
        }
        used += w;
        start -= 1;
    }
    chars[start..].iter().collect()
}

fn card_lines(
    app: &App,
    theme: &Theme,
    card: &Card,
    width: usize,
    selected: bool,
) -> Vec<Line<'static>> {
    let key_w = card.key.width();
    let title = fit(
        &card.title,
        width.saturating_sub(key_w + 1),
        theme.ellipsis(),
    );
    let mut first = vec![
        Span::styled(card.key.clone(), theme.key()),
        Span::raw(format!(" {title}")),
    ];
    let who = if card.assignees.is_empty() {
        "unassigned".to_string()
    } else {
        card.assignees.join(", ")
    };
    let mut meta: Vec<Span<'static>> = Vec::new();
    let who = fit(&who, width.saturating_sub(14), theme.ellipsis());
    meta.push(Span::styled(who, theme.muted()));
    if card.running {
        let elapsed = format_duration((app.now - card.start_time).max(0));
        let text = format!("{}{} {}", theme.sep(), theme.dot(), elapsed);
        meta.push(Span::styled(text, theme.warn()));
    } else if card.tracked > 0 {
        let text = format!("{}{}", theme.sep(), format_duration(card.tracked));
        meta.push(Span::styled(text, theme.muted()));
    }
    let mut lines = vec![Line::from(std::mem::take(&mut first)), Line::from(meta)];
    if selected {
        for l in &mut lines {
            *l = l.clone().style(theme.selected());
        }
    }
    lines
}

fn keep_visible(scroll: &mut usize, selected: usize, visible: usize, total: usize) {
    if visible == 0 {
        *scroll = 0;
        return;
    }
    if selected < *scroll {
        *scroll = selected;
    } else if selected >= *scroll + visible {
        *scroll = selected + 1 - visible;
    }
    *scroll = (*scroll).min(total.saturating_sub(visible));
}

fn draw_cards(
    f: &mut Frame,
    app: &App,
    theme: &Theme,
    col: usize,
    inner: Rect,
    hits: &mut Hits,
    active: bool,
) {
    let inner = Rect::new(
        inner.x + 1.min(inner.width / 2),
        inner.y,
        inner.width.saturating_sub(2),
        inner.height,
    );
    let cards = app.visible(col);
    let visible = (inner.height as usize + 1) / CARD_STRIDE;
    let sel = app.row(col);
    let mut view = app.view.borrow_mut();
    if view.col_scroll.len() < app.columns.len() {
        view.col_scroll.resize(app.columns.len(), 0);
    }
    keep_visible(&mut view.col_scroll[col], sel, visible, cards.len());
    let top = view.col_scroll[col];
    drop(view);
    for (i, card) in cards.iter().enumerate().skip(top).take(visible) {
        let y = inner.y + ((i - top) * CARD_STRIDE) as u16;
        let rect = Rect::new(inner.x, y, inner.width, 2.min(inner.y + inner.height - y));
        let selected = active && i == sel;
        f.render_widget(
            Paragraph::new(card_lines(app, theme, card, inner.width as usize, selected)),
            rect,
        );
        hits.cards.push((rect, col, i));
    }
}

fn column_block(app: &App, theme: &Theme, col: usize) -> Block<'static> {
    let active = col == app.col;
    let style = if active {
        theme.accent()
    } else {
        theme.muted()
    };
    let count = app.visible(col).len();
    Block::bordered()
        .border_set(theme.border_set())
        .border_style(style)
        .title(Line::from(vec![
            Span::styled(format!(" {}", app.columns[col].name), theme.bold()),
            Span::styled(format!(" {count} "), theme.muted()),
        ]))
}

fn draw_columns(f: &mut Frame, app: &App, theme: &Theme, area: Rect, hits: &mut Hits) {
    if app.columns.is_empty() {
        f.render_widget(Paragraph::new("no statuses"), area);
        return;
    }
    let parts = Layout::horizontal(vec![Constraint::Fill(1); app.columns.len()]).split(area);
    for (col, rect) in parts.iter().enumerate() {
        let block = column_block(app, theme, col);
        let inner = block.inner(*rect);
        f.render_widget(block, *rect);
        draw_cards(f, app, theme, col, inner, hits, col == app.col);
    }
}

fn tab_label(app: &App, col: usize) -> String {
    format!("{} {}", app.columns[col].name, app.visible(col).len())
}

fn draw_tabs(f: &mut Frame, app: &App, theme: &Theme, area: Rect, hits: &mut Hits) {
    if app.columns.is_empty() {
        f.render_widget(Paragraph::new("no statuses"), area);
        return;
    }
    let [strip, rest] = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);
    let width = strip.width as usize;
    let labels: Vec<String> = (0..app.columns.len()).map(|c| tab_label(app, c)).collect();
    let (mut lo, mut hi) = (app.col, app.col);
    let span = |lo: usize, hi: usize| -> usize {
        labels[lo..=hi].iter().map(|l| l.width() + 2).sum::<usize>()
    };
    loop {
        if hi + 1 < labels.len() && span(lo, hi + 1) <= width {
            hi += 1;
        } else if lo > 0 && span(lo - 1, hi) <= width {
            lo -= 1;
        } else {
            break;
        }
    }
    let mut x = strip.x;
    let mut spans = Vec::new();
    for (i, label) in labels.iter().enumerate().take(hi + 1).skip(lo) {
        let text = fit(&format!(" {label} "), width, theme.ellipsis());
        let w = text.width() as u16;
        let style = if i == app.col {
            theme
                .accent()
                .add_modifier(ratatui::style::Modifier::BOLD | ratatui::style::Modifier::REVERSED)
        } else {
            theme.muted()
        };
        hits.tabs.push((
            Rect::new(x, strip.y, w.min(strip.x + strip.width - x), 1),
            i,
        ));
        x += w;
        spans.push(Span::styled(text, style));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), strip);
    let block = Block::bordered()
        .border_set(theme.border_set())
        .border_style(theme.accent());
    let inner = block.inner(rest);
    f.render_widget(block, rest);
    draw_cards(f, app, theme, app.col, inner, hits, true);
}

fn draw_list(f: &mut Frame, app: &App, theme: &Theme, area: Rect, hits: &mut Hits) {
    struct Row {
        line: Line<'static>,
        card: Option<(usize, usize)>,
    }
    let mut rows: Vec<Row> = Vec::new();
    let mut sel_line = 0;
    for (col, column) in app.columns.iter().enumerate() {
        let cards = app.visible(col);
        rows.push(Row {
            line: Line::from(vec![
                Span::styled(column.name.clone(), theme.bold()),
                Span::styled(format!(" {}", cards.len()), theme.muted()),
            ]),
            card: None,
        });
        for (i, card) in cards.iter().enumerate() {
            let selected = col == app.col && i == app.row(col);
            if selected {
                sel_line = rows.len();
            }
            let mut spans = vec![
                Span::styled(format!(" {}", card.key), theme.key()),
                Span::raw(format!(
                    " {}",
                    fit(
                        &card.title,
                        (area.width as usize).saturating_sub(card.key.width() + 4),
                        theme.ellipsis()
                    )
                )),
            ];
            if card.running {
                spans.push(Span::styled(format!(" {}", theme.dot()), theme.warn()));
            }
            let mut line = Line::from(spans);
            if selected {
                line = line.style(theme.selected());
            }
            rows.push(Row {
                line,
                card: Some((col, i)),
            });
        }
    }
    let visible = area.height as usize;
    let mut view = app.view.borrow_mut();
    keep_visible(&mut view.list_scroll, sel_line, visible, rows.len());
    let top = view.list_scroll;
    drop(view);
    for (n, row) in rows.into_iter().skip(top).take(visible).enumerate() {
        let rect = Rect::new(area.x, area.y + n as u16, area.width, 1);
        if let Some((c, i)) = row.card {
            hits.cards.push((rect, c, i));
        }
        f.render_widget(Paragraph::new(row.line), rect);
    }
}

fn detail_lines(app: &App, theme: &Theme, card: &Card, width: usize) -> Vec<Line<'static>> {
    let ell = theme.ellipsis();
    let status = app
        .columns
        .iter()
        .find(|c| Some(c.id) == card.status_id)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let mut lines = vec![Line::from(vec![
        Span::styled(card.key.clone(), theme.key()),
        Span::styled(
            format!(
                " {}",
                fit(
                    &card.title,
                    width.saturating_sub(card.key.width() + status.width() + 6),
                    ell
                )
            ),
            theme.bold(),
        ),
        Span::styled(format!(" [{status}]"), theme.good()),
    ])];
    let mut meta = vec![format!(
        "assignee {}",
        if card.assignees.is_empty() {
            "none".to_string()
        } else {
            card.assignees.join(", ")
        }
    )];
    if card.tracked > 0 {
        meta.push(format!("tracked {}", format_duration(card.tracked)));
    }
    if card.running {
        meta.push(format!(
            "timer {} {}",
            theme.dot(),
            format_duration((app.now - card.start_time).max(0))
        ));
    }
    lines.push(Line::from(Span::styled(
        meta.join(theme.sep()),
        theme.muted(),
    )));
    let detail: Option<&Detail> = app.details.get(&card.id);
    let Some(d) = detail else {
        let text = card.description.clone().unwrap_or_default();
        push_description(&mut lines, &text, width, ell);
        lines.push(Line::from(Span::styled("loading...", theme.muted())));
        return lines;
    };
    if let Some(e) = &d.error {
        lines.push(Line::from(Span::styled(
            fit(&format!("details unavailable: {e}"), width, ell),
            theme.accent(),
        )));
    }
    let desc = d.description.clone().or_else(|| card.description.clone());
    push_description(&mut lines, &desc.unwrap_or_default(), width, ell);
    if !d.comments.is_empty() {
        lines.push(Line::from(Span::styled("Comments", theme.bold())));
        for c in &d.comments {
            let head = format!("{}: ", c.author);
            lines.push(Line::from(vec![
                Span::styled(head.clone(), theme.muted()),
                Span::raw(fit(
                    c.text.lines().next().unwrap_or(""),
                    width.saturating_sub(head.width()),
                    ell,
                )),
            ]));
        }
    }
    if d.runs_available && !d.runs.is_empty() {
        lines.push(Line::from(Span::styled("Agent work", theme.bold())));
        for r in &d.runs {
            let mut text = format!(
                "{} {} {} {}",
                r.agent,
                r.model,
                r.status,
                format_duration(r.seconds)
            );
            if !r.branch.is_empty() {
                text.push_str(&format!("{}{}", theme.sep(), r.branch));
            }
            if let (Some(p), Some(fl)) = (r.passed, r.failed) {
                text.push_str(&format!("{}tests {p} passed {fl} failed", theme.sep()));
            }
            if !r.pr.is_empty() {
                text.push_str(&format!("{}{}", theme.sep(), r.pr));
            }
            lines.push(Line::from(fit(&text, width, ell)));
        }
    }
    lines
}

fn push_description(lines: &mut Vec<Line<'static>>, text: &str, width: usize, ell: &str) {
    for l in text.lines().filter(|l| !l.trim().is_empty()).take(3) {
        lines.push(Line::from(fit(l.trim_end(), width, ell)));
    }
}

fn draw_details(f: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let block = Block::bordered()
        .border_set(theme.border_set())
        .border_style(theme.accent());
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines = match app.selected() {
        Some(card) => detail_lines(app, theme, card, inner.width as usize),
        None => vec![Line::from(Span::styled("no task selected", theme.muted()))],
    };
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_help(f: &mut Frame, theme: &Theme, area: Rect) {
    let rows = [
        ("h l / arrows", "previous / next column"),
        ("j k / arrows", "previous / next card"),
        ("H L", "move card to previous / next status"),
        ("enter", "toggle details"),
        ("s", "start timer"),
        ("S", "stop timer"),
        ("c", "comment on the task"),
        ("/", "search (enter keeps, esc clears)"),
        ("w", "switch workspace (cloud and local)"),
        ("p", "filter by project"),
        ("a", "filter by assignee"),
        ("P", "filter by persona"),
        ("x", "reset all filters"),
        ("f", "filters menu"),
        ("g", "refresh"),
        ("mouse", "click selects, wheel moves"),
        ("q / esc", "quit"),
    ];
    let w = 56.min(area.width);
    let h = (rows.len() as u16 + 4).min(area.height);
    let rect = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    f.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_set(theme.border_set())
        .border_style(theme.accent())
        .title(" keys ");
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let mut lines: Vec<Line<'static>> = rows
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(format!("{k:<14}"), theme.key()),
                Span::styled((*d).to_string(), Style::new()),
            ])
        })
        .collect();
    lines.push(Line::from(Span::styled("any key closes", theme.muted())));
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_picker(f: &mut Frame, app: &App, theme: &Theme, area: Rect, hits: &mut Hits) {
    let Some(picker) = &app.picker else { return };
    let title = match picker.chip {
        Chip::Workspace => " workspace ",
        Chip::Project => " project ",
        Chip::Assignee => " assignee ",
        Chip::Persona => " persona ",
        Chip::Search | Chip::Menu => " filters ",
    };
    let want = picker
        .items
        .iter()
        .map(|i| i.label.width() + 4)
        .max()
        .unwrap_or(0)
        .max(title.width() + 2)
        .max(24)
        + 2;
    let w = (want as u16).min(area.width).min(52);
    let body = picker.items.len().max(1) as u16;
    let h = (body + 2).min(area.height.saturating_sub(2)).max(3);
    let rect = Rect::new(area.x, area.y + 1, w, h.min(area.height.saturating_sub(1)));
    f.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_set(theme.border_set())
        .border_style(theme.accent())
        .title(title);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    hits.picker_box = Some(rect);
    if picker.loading {
        f.render_widget(
            Paragraph::new(Span::styled("loading...", theme.muted())),
            inner,
        );
        return;
    }
    if picker.items.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled("nothing to choose from", theme.muted())),
            inner,
        );
        return;
    }
    let visible = inner.height as usize;
    let mut view = app.view.borrow_mut();
    keep_visible(
        &mut view.picker_scroll,
        picker.sel,
        visible,
        picker.items.len(),
    );
    let top = view.picker_scroll;
    drop(view);
    for (n, item) in picker.items.iter().enumerate().skip(top).take(visible) {
        let rect = Rect::new(inner.x, inner.y + (n - top) as u16, inner.width, 1);
        let mark = if item.current { theme.dot() } else { " " };
        let text = fit(
            &format!("{mark} {}", item.label),
            inner.width as usize,
            theme.ellipsis(),
        );
        let mut line = Line::from(text);
        if n == picker.sel {
            line = line.style(theme.selected());
        }
        f.render_widget(Paragraph::new(line), rect);
        hits.picker.push((rect, n));
    }
}
