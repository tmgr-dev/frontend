use super::mouse::{Kind, MouseEvent};
use super::prompts::clean;
use super::render::{dim, put};
use super::theme::colors;
use super::view::{contains, MouseOut, ViewState};
use crate::checkpoint::RESTORE_WARNING;
use chrono::TimeZone;
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, BorderType, Clear, Widget};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tmgr_ipc::api::{
    CheckpointDiff, CheckpointItem, CheckpointKind, CheckpointList, CheckpointRestored,
    CheckpointStatus,
};
use unicode_width::UnicodeWidthStr;

pub const OFF_MESSAGE: &str = "Checkpoints are off for this session. Start one with --checkpoints or set attach.checkpoints in tmgr config.";
pub const BUSY_MESSAGE: &str = "The agent is working. Try when it stops.";
const UNAVAILABLE: &str = "Checkpoints are not available for this session.";
const OLD_RUNNER: &str = "The running runner is older than this tmgr. Restart it.";
const DEBOUNCE: Duration = Duration::from_millis(200);
const PANEL_W: u16 = 100;
const PANEL_H: u16 = 24;
const STATUS_ROWS: u16 = 4;
const MAX_TEXT: usize = 400;

pub fn friendly_error(raw: &str) -> String {
    if raw.starts_with("busy:") {
        return BUSY_MESSAGE.into();
    }
    if raw.starts_with("unknown_method") {
        return OLD_RUNNER.into();
    }
    let text = match raw.split_once(": ") {
        Some((code, rest)) if code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') => rest,
        _ => raw,
    };
    clean(text, 200)
}

pub fn badge(status: &CheckpointStatus, room: usize) -> Option<String> {
    let text = if status.available {
        format!(" · ↶ {}", status.count)
    } else {
        " · ↶ no git".to_string()
    };
    (text.width() <= room).then_some(text)
}

fn clock(ms: u64) -> String {
    i64::try_from(ms)
        .ok()
        .and_then(|m| chrono::Local.timestamp_millis_opt(m).single())
        .map_or_else(|| "--:--".into(), |t| t.format("%H:%M").to_string())
}

fn plural(n: u32, one: &'static str, many: &'static str) -> &'static str {
    if n == 1 {
        one
    } else {
        many
    }
}

pub fn item_label(it: &CheckpointItem) -> String {
    let mut parts = vec![match it.kind {
        CheckpointKind::Start => "session start".to_string(),
        CheckpointKind::Turn => format!("after turn {}", it.turn),
        CheckpointKind::BeforeRestore => "before restore".to_string(),
        CheckpointKind::Restored => match it.restored_from {
            Some(n) => format!("restored #{n}"),
            None => "restored".to_string(),
        },
    }];
    if it.kind == CheckpointKind::Turn {
        parts.push(format!(
            "{} {}",
            it.files,
            plural(it.files, "file", "files")
        ));
    }
    if it.skipped > 0 {
        parts.push(format!("{} skipped", it.skipped));
    }
    parts.push(clock(it.at_ms));
    format!("#{} {}", it.n, parts.join(" · "))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CpAction {
    Check(String, u64),
    Restore(String, u64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CpFetch {
    List(String),
    Diff(String, u64),
}

#[derive(Debug)]
pub enum CpMsg {
    List(String, Result<CheckpointList, String>),
    Diff(String, u64, Result<CheckpointDiff, String>),
    Check(String, u64, Result<CheckpointDiff, String>),
    Restored(String, u64, Result<CheckpointRestored, String>),
}

#[derive(Clone, Debug)]
enum Preview {
    Loading,
    Ready(CheckpointDiff),
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Mode {
    Browse,
    Checking(u64),
    Confirm { n: u64, change: u32, removed: u32 },
    Restoring(u64),
}

#[derive(Clone, Debug)]
struct Panel {
    session: String,
    message: Option<String>,
    items: Vec<CheckpointItem>,
    list_err: Option<String>,
    want_list: bool,
    list_inflight: bool,
    sel: usize,
    scroll: usize,
    previews: HashMap<u64, Preview>,
    settle: Instant,
    mode: Mode,
    result: Vec<String>,
    left: Vec<String>,
    show_paths: bool,
}

impl Panel {
    fn new(session: &str, status: Option<&CheckpointStatus>) -> Self {
        let mut p = Panel {
            session: session.to_string(),
            message: None,
            items: Vec::new(),
            list_err: None,
            want_list: true,
            list_inflight: false,
            sel: 0,
            scroll: 0,
            previews: HashMap::new(),
            settle: Instant::now(),
            mode: Mode::Browse,
            result: Vec::new(),
            left: Vec::new(),
            show_paths: false,
        };
        p.sync(status);
        p
    }

    fn sync(&mut self, status: Option<&CheckpointStatus>) {
        self.message = match status {
            None => Some(OFF_MESSAGE.to_string()),
            Some(s) if !s.available => Some(
                s.note
                    .as_deref()
                    .map(|n| clean(n, MAX_TEXT))
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| UNAVAILABLE.to_string()),
            ),
            Some(_) => None,
        };
    }

    fn selected(&self) -> Option<&CheckpointItem> {
        self.items.get(self.sel)
    }

    fn select(&mut self, at: usize, now: Instant) {
        let at = at.min(self.items.len().saturating_sub(1));
        if at != self.sel {
            self.sel = at;
            self.scroll = 0;
            self.show_paths = false;
            self.settle = now + DEBOUNCE;
        }
    }

    fn lines(&self) -> Vec<(String, Style)> {
        if self.show_paths {
            let mut out = vec![("Left as is (changed since):".to_string(), Style::new())];
            out.extend(self.left.iter().map(|p| (clean(p, MAX_TEXT), Style::new())));
            return out;
        }
        let Some(item) = self.selected() else {
            return Vec::new();
        };
        match self.previews.get(&item.n) {
            None | Some(Preview::Loading) => vec![("Loading...".to_string(), dim())],
            Some(Preview::Failed(e)) => vec![(e.clone(), Style::new().fg(colors().bad))],
            Some(Preview::Ready(d)) => {
                let mut out: Vec<(String, Style)> = d
                    .text
                    .lines()
                    .map(|l| {
                        let style = if l.starts_with("+++") || l.starts_with("---") {
                            Style::new().add_modifier(Modifier::BOLD)
                        } else if l.starts_with('+') {
                            Style::new().fg(colors().ok)
                        } else if l.starts_with('-') {
                            Style::new().fg(colors().bad)
                        } else {
                            Style::new()
                        };
                        (clean(l, MAX_TEXT), style)
                    })
                    .collect();
                if out.is_empty() {
                    out.push(("No changes.".to_string(), dim()));
                }
                if d.truncated {
                    out.push(("… diff truncated".to_string(), dim()));
                }
                out
            }
        }
    }

    fn status_lines(&self) -> Vec<String> {
        match &self.mode {
            Mode::Checking(_) => vec!["Checking what a restore would change...".into()],
            Mode::Confirm { n, change, removed } => vec![
                RESTORE_WARNING.to_string(),
                format!(
                    "Restore to #{n}? {change} {}, {removed} {}. Current state is saved first. y/n",
                    plural(*change, "file changes", "files change"),
                    plural(*removed, "is removed", "are removed"),
                ),
            ],
            Mode::Restoring(_) => vec!["Restoring...".into()],
            Mode::Browse => self.result.clone(),
        }
    }

    fn hint(&self) -> String {
        match &self.mode {
            Mode::Browse if self.message.is_some() => "Esc  Close".into(),
            Mode::Browse => {
                let paths = if self.left.is_empty() {
                    ""
                } else if self.show_paths {
                    "  ·  p  diff"
                } else {
                    "  ·  p  paths"
                };
                format!("↑/↓  Select  ·  PgUp/PgDn  Scroll  ·  r  Restore{paths}  ·  Esc  Close")
            }
            Mode::Confirm { .. } => "y  Restore  ·  any other key  Cancel".into(),
            Mode::Checking(_) | Mode::Restoring(_) => String::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CpState {
    pub form_default: bool,
    pub status: HashMap<String, CheckpointStatus>,
    panel: Option<Panel>,
}

impl CpState {
    pub fn is_open(&self) -> bool {
        self.panel.is_some()
    }

    pub fn close(&mut self) {
        self.panel = None;
    }

    pub fn selected_n(&self) -> Option<u64> {
        self.panel.as_ref()?.selected().map(|i| i.n)
    }

    pub fn session(&self) -> Option<&str> {
        self.panel.as_ref().map(|p| p.session.as_str())
    }
}

struct Geo {
    rect: Rect,
    list: Rect,
    preview: Rect,
    status: Rect,
}

fn geometry(area: Rect) -> Option<Geo> {
    let w = PANEL_W.min(area.width);
    let h = PANEL_H.min(area.height);
    let rect = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    let inner = rect.inner(Margin::new(2, 1));
    if inner.width < 8 || inner.height <= STATUS_ROWS + 1 {
        return None;
    }
    let body_h = inner.height - STATUS_ROWS;
    let list_w = (inner.width / 2).clamp(20, 52).min(inner.width);
    let list = Rect::new(inner.x, inner.y, list_w, body_h);
    let preview_x = inner.x + list_w + 1;
    let preview = Rect::new(
        preview_x,
        inner.y,
        inner.right().saturating_sub(preview_x),
        body_h,
    );
    let status = Rect::new(inner.x, inner.y + body_h, inner.width, STATUS_ROWS);
    Some(Geo {
        rect,
        list,
        preview,
        status,
    })
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match out.last_mut() {
            Some(line) if line.width() + 1 + word.width() <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => out.push(word.to_string()),
        }
    }
    out
}

fn list_top(sel: usize, rows: usize) -> usize {
    if rows == 0 || sel < rows {
        0
    } else {
        sel + 1 - rows
    }
}

impl ViewState {
    pub fn set_checkpoints(&mut self, found: HashMap<String, CheckpointStatus>) -> bool {
        if found == self.cp.status {
            return false;
        }
        let ids: Vec<&str> = self.sessions.iter().map(|s| s.id.as_str()).collect();
        if let Some(p) = &mut self.cp.panel {
            if !ids.contains(&p.session.as_str()) {
                self.cp.panel = None;
            } else {
                let now = found.get(&p.session);
                let was_message = p.message.is_some();
                let changed = now != self.cp.status.get(&p.session);
                p.sync(now);
                if changed && p.message.is_none() && (was_message || !p.list_inflight) {
                    p.want_list = true;
                }
            }
        }
        self.cp.status = found;
        true
    }

    pub fn toggle_checkpoints(&mut self) {
        if self.cp.panel.take().is_some() {
            return;
        }
        let Some(id) = self.focused_session().map(str::to_string) else {
            return;
        };
        self.cp.panel = Some(Panel::new(&id, self.cp.status.get(&id)));
        self.help = false;
        self.grab = None;
    }

    pub fn edit_cp(&mut self, data: &[u8]) -> Option<CpAction> {
        let panel = self.cp.panel.as_mut()?;
        match panel.mode.clone() {
            Mode::Confirm { n, .. } => {
                if data == b"y" {
                    panel.mode = Mode::Restoring(n);
                    panel.result.clear();
                    panel.left.clear();
                    panel.show_paths = false;
                    return Some(CpAction::Restore(panel.session.clone(), n));
                }
                panel.mode = Mode::Browse;
                return None;
            }
            Mode::Checking(_) => {
                panel.mode = Mode::Browse;
                return None;
            }
            Mode::Restoring(_) => return None,
            Mode::Browse => {}
        }
        if matches!(data, b"\x1b" | b"q" | b"C") {
            self.cp.panel = None;
            return None;
        }
        if panel.message.is_some() {
            return None;
        }
        let now = Instant::now();
        let last = panel.items.len().saturating_sub(1);
        match data {
            b"\x1b[A" | b"k" => panel.select(panel.sel.saturating_sub(1), now),
            b"\x1b[B" | b"j" => panel.select((panel.sel + 1).min(last), now),
            b"\x1b[H" | b"g" => panel.select(0, now),
            b"\x1b[F" | b"G" => panel.select(last, now),
            b"\x1b[5~" => panel.scroll = panel.scroll.saturating_sub(10),
            b"\x1b[6~" => {
                panel.scroll = (panel.scroll + 10).min(panel.lines().len().saturating_sub(1))
            }
            b"p" if !panel.left.is_empty() => {
                panel.show_paths = !panel.show_paths;
                panel.scroll = 0;
            }
            b"r" => {
                let n = panel.selected()?.n;
                panel.mode = Mode::Checking(n);
                return Some(CpAction::Check(panel.session.clone(), n));
            }
            _ => {}
        }
        None
    }

    pub fn cp_due(&mut self, now: Instant) -> Vec<CpFetch> {
        let mut out = Vec::new();
        let Some(p) = &mut self.cp.panel else {
            return out;
        };
        if p.message.is_some() {
            return out;
        }
        if p.want_list && !p.list_inflight {
            p.want_list = false;
            p.list_inflight = true;
            out.push(CpFetch::List(p.session.clone()));
        }
        if now >= p.settle && !p.previews.values().any(|v| matches!(v, Preview::Loading)) {
            if let Some(n) = p.selected().map(|i| i.n) {
                if let std::collections::hash_map::Entry::Vacant(slot) = p.previews.entry(n) {
                    slot.insert(Preview::Loading);
                    out.push(CpFetch::Diff(p.session.clone(), n));
                }
            }
        }
        out
    }

    pub fn cp_msg(&mut self, msg: CpMsg) -> bool {
        let Some(p) = &mut self.cp.panel else {
            return false;
        };
        match msg {
            CpMsg::List(session, res) if session == p.session => {
                p.list_inflight = false;
                match res {
                    Ok(list) => {
                        let keep = p.selected().map(|i| i.n);
                        let mut items = list.items;
                        items.sort_by_key(|i| std::cmp::Reverse(i.n));
                        p.items = items;
                        p.sel = keep
                            .and_then(|n| p.items.iter().position(|i| i.n == n))
                            .unwrap_or(0);
                        p.list_err = None;
                        p.settle = Instant::now();
                        p.sync(Some(&list.status));
                        self.cp.status.insert(session, list.status);
                    }
                    Err(e) => p.list_err = Some(e),
                }
            }
            CpMsg::Diff(session, n, res) if session == p.session => {
                let preview = match res {
                    Ok(d) => Preview::Ready(d),
                    Err(e) => Preview::Failed(e),
                };
                p.previews.insert(n, preview);
            }
            CpMsg::Check(session, n, res)
                if session == p.session && p.mode == Mode::Checking(n) =>
            {
                match res {
                    Ok(d) => {
                        p.mode = Mode::Confirm {
                            n,
                            change: d.modified + d.deleted,
                            removed: d.added,
                        }
                    }
                    Err(e) => {
                        p.mode = Mode::Browse;
                        p.result = wrap(&e, 80);
                    }
                }
            }
            CpMsg::Restored(session, n, res)
                if session == p.session && p.mode == Mode::Restoring(n) =>
            {
                p.mode = Mode::Browse;
                match res {
                    Ok(r) => {
                        p.result = vec![format!(
                            "Restored #{}. {} written, {} removed. Undo: pick #{}.",
                            r.n, r.written, r.deleted, r.before_n
                        )];
                        if !r.left_as_is.is_empty() {
                            p.result.push(format!(
                                "{} left as is (changed since). Press p to see the paths.",
                                r.left_as_is.len()
                            ));
                        }
                        p.left = r.left_as_is;
                        p.sel = 0;
                        p.scroll = 0;
                        p.want_list = true;
                    }
                    Err(e) => p.result = vec![e],
                }
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn cp_mouse(&mut self, area: Rect, ev: MouseEvent, mut out: MouseOut) -> MouseOut {
        let Some(geo) = geometry(area) else {
            if ev.is_left_press() {
                self.cp.panel = None;
                out.redraw = true;
            }
            return out;
        };
        let Some(p) = &mut self.cp.panel else {
            return out;
        };
        match ev.kind() {
            Kind::Press => {
                self.grab = Some(String::new());
                if !ev.is_left_press() {
                    return out;
                }
                out.redraw = true;
                if p.message.is_none()
                    && matches!(p.mode, Mode::Browse)
                    && contains(geo.list, ev.col, ev.row)
                {
                    let top = list_top(p.sel, usize::from(geo.list.height));
                    let at = top + usize::from(ev.row - geo.list.y);
                    if at < p.items.len() {
                        p.select(at, Instant::now());
                    }
                } else if !contains(geo.rect, ev.col, ev.row) {
                    self.cp.panel = None;
                }
            }
            Kind::Release if self.grab.as_deref() == Some("") => self.grab = None,
            Kind::WheelUp | Kind::WheelDown if contains(geo.rect, ev.col, ev.row) => {
                let max = p.lines().len().saturating_sub(1);
                p.scroll = if ev.kind() == Kind::WheelUp {
                    p.scroll.saturating_sub(3)
                } else {
                    (p.scroll + 3).min(max)
                };
                out.redraw = true;
            }
            _ => {}
        }
        out
    }
}

pub(crate) fn draw_checkpoints(buf: &mut Buffer, area: Rect, st: &ViewState) {
    let Some(p) = &st.cp.panel else {
        return;
    };
    let Some(geo) = geometry(area) else {
        return;
    };
    Clear.render(geo.rect, buf);
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" Checkpoints ")
        .render(geo.rect, buf);
    if let Some(msg) = &p.message {
        let inner = geo.list;
        let width = usize::from(geo.status.right().saturating_sub(inner.x));
        for (i, line) in wrap(msg, width).iter().enumerate() {
            let y = inner.y + i as u16;
            if y < geo.status.y {
                put(buf, inner.x, y, line, Style::new(), width as u16);
            }
        }
        put(
            buf,
            geo.status.x,
            geo.status.bottom() - 1,
            &p.hint(),
            dim(),
            geo.status.width,
        );
        return;
    }
    let rows = usize::from(geo.list.height);
    if p.items.is_empty() {
        let text = match &p.list_err {
            Some(e) => e.as_str(),
            None => "Loading...",
        };
        put(buf, geo.list.x, geo.list.y, text, dim(), geo.list.width);
    }
    let top = list_top(p.sel, rows);
    for (row, item) in p.items.iter().skip(top).take(rows).enumerate() {
        let y = geo.list.y + row as u16;
        let selected = row + top == p.sel;
        let style = if selected {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        let line = format!("{} {}", if selected { ">" } else { " " }, item_label(item));
        put(buf, geo.list.x, y, &line, style, geo.list.width);
    }
    for y in geo.list.y..geo.list.bottom() {
        put(buf, geo.list.right(), y, "│", dim(), 1);
    }
    if geo.preview.width > 4 {
        let lines = p.lines();
        let top = p.scroll.min(lines.len().saturating_sub(1));
        for (row, (text, style)) in lines
            .iter()
            .skip(top)
            .take(usize::from(geo.preview.height))
            .enumerate()
        {
            put(
                buf,
                geo.preview.x,
                geo.preview.y + row as u16,
                text,
                *style,
                geo.preview.width,
            );
        }
    }
    let width = usize::from(geo.status.width);
    for (i, line) in p
        .status_lines()
        .iter()
        .flat_map(|l| wrap(l, width))
        .take(usize::from(STATUS_ROWS) - 1)
        .enumerate()
    {
        put(
            buf,
            geo.status.x,
            geo.status.y + i as u16,
            &line,
            Style::new().add_modifier(Modifier::BOLD),
            geo.status.width,
        );
    }
    put(
        buf,
        geo.status.x,
        geo.status.bottom() - 1,
        &p.hint(),
        dim(),
        geo.status.width,
    );
}
