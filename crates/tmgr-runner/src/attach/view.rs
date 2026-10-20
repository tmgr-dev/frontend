use super::keys::{Cmd, ScrollKey};
use super::mouse::{encode, wants, Kind, MouseEvent};
use ratatui::layout::{Margin, Rect};
use serde::{Deserialize, Serialize};
use tmgr_ipc::api::{MouseMode, State, StateKind};

pub const SIDEBAR_WIDTH: u16 = 24;
pub const WIDE_COLS: u16 = 100;
const MIN_PANE_COLS: u16 = 10;
pub const WHEEL_LINES: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRow {
    pub id: String,
    pub name: String,
    pub state: State,
    pub age: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollState {
    pub offset: usize,
    pub max: usize,
}

#[derive(Clone, Debug)]
pub struct ViewState {
    pub sessions: Vec<SessionRow>,
    pub panes: Vec<String>,
    pub focus: usize,
    pub zoom: bool,
    pub sidebar: Option<bool>,
    pub help: bool,
    pub scroll: Option<ScrollState>,
    pub notice: Option<String>,
    pub runner_label: String,
    pub width: u16,
    pub pointer: Option<(u16, u16)>,
    pub grab: Option<String>,
    pub confirm: Option<Confirm>,
    pub rename: Option<Rename>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rename {
    pub session: String,
    pub text: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RenameEdit {
    Editing,
    Submit(String, String),
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirm {
    pub session: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutPrefs {
    pub panes: Vec<String>,
    pub focus: usize,
    pub zoom: bool,
    pub sidebar: Option<bool>,
}

impl ViewState {
    pub fn new(sessions: Vec<SessionRow>, runner_label: &str) -> Self {
        let panes = sessions.first().map(|s| s.id.clone()).into_iter().collect();
        Self {
            sessions,
            panes,
            focus: 0,
            zoom: false,
            sidebar: None,
            help: false,
            scroll: None,
            notice: None,
            runner_label: runner_label.to_string(),
            width: WIDE_COLS,
            pointer: None,
            grab: None,
            confirm: None,
            rename: None,
        }
    }

    pub fn sidebar_rows(&self) -> Vec<&SessionRow> {
        let mut rows: Vec<&SessionRow> = self.sessions.iter().collect();
        rows.sort_by_key(|r| r.state.kind != StateKind::Blocked);
        rows
    }

    pub fn focused_session(&self) -> Option<&str> {
        self.panes.get(self.focus).map(String::as_str)
    }

    pub fn focused_row(&self) -> Option<&SessionRow> {
        let id = self.focused_session()?;
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn sidebar_shown(&self) -> bool {
        !self.zoom && self.sidebar.unwrap_or(self.width >= WIDE_COLS)
    }

    pub fn apply(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::NextSession => self.step(true),
            Cmd::PrevSession => self.step(false),
            Cmd::Split => self.split(),
            Cmd::FocusOther => {
                if self.panes.len() == 2 {
                    self.focus ^= 1;
                    self.scroll = None;
                }
            }
            Cmd::Zoom => self.zoom = !self.zoom,
            Cmd::Sidebar => {
                let shown = self.sidebar.unwrap_or(self.width >= WIDE_COLS);
                self.sidebar = Some(!shown);
            }
            Cmd::Help => self.help = !self.help,
            Cmd::ScrollMode => {
                if self.focused_session().is_some() {
                    self.scroll = Some(ScrollState::default());
                }
            }
            Cmd::Stop => match self.focused_row() {
                Some(r) if r.state.kind == StateKind::Exited => {
                    self.notice = Some("already exited, Ctrl-] X removes it".into());
                }
                Some(r) => {
                    self.confirm = Some(Confirm {
                        session: r.id.clone(),
                        name: r.name.clone(),
                    });
                }
                None => {}
            },
            Cmd::Remove => match self.focused_row() {
                Some(r) if r.state.kind != StateKind::Exited => {
                    self.notice = Some("still running, stop it first with Ctrl-] x".into());
                }
                Some(r) => {
                    let id = r.id.clone();
                    self.remove_session(&id);
                }
                None => {}
            },
            Cmd::Rename => {
                if let Some(r) = self.focused_row() {
                    self.rename = Some(Rename {
                        session: r.id.clone(),
                        text: r.name.clone(),
                    });
                }
            }
            Cmd::Detach | Cmd::Picker => {}
        }
    }

    pub fn edit_rename(&mut self, data: &[u8]) -> RenameEdit {
        let Some(r) = self.rename.as_mut() else {
            return RenameEdit::Cancel;
        };
        let text = String::from_utf8_lossy(data);
        for ch in text.chars() {
            match ch {
                '\r' | '\n' => {
                    let done = self.rename.take();
                    return match done {
                        Some(r) => RenameEdit::Submit(r.session, r.text.trim().to_string()),
                        None => RenameEdit::Cancel,
                    };
                }
                '\x1b' => {
                    self.rename = None;
                    return RenameEdit::Cancel;
                }
                '\x7f' | '\x08' => {
                    r.text.pop();
                }
                c if !c.is_control() && r.text.chars().count() < 64 => r.text.push(c),
                _ => {}
            }
        }
        RenameEdit::Editing
    }

    pub fn rename_done(&mut self, id: &str, name: &str) {
        if let Some(row) = self.sessions.iter_mut().find(|s| s.id == id) {
            row.name = name.to_string();
        }
    }

    pub fn remove_session(&mut self, id: &str) {
        let Some(pos) = self.sessions.iter().position(|s| s.id == id) else {
            return;
        };
        self.sessions.remove(pos);
        if let Some(i) = self.panes.iter().position(|p| p == id) {
            let neighbour = self.sessions[pos..]
                .iter()
                .chain(self.sessions[..pos].iter().rev())
                .find(|s| !self.panes.contains(&s.id))
                .map(|s| s.id.clone());
            match neighbour {
                Some(n) => self.panes[i] = n,
                None => {
                    self.panes.remove(i);
                    self.focus = self.focus.min(self.panes.len().saturating_sub(1));
                    self.zoom = false;
                }
            }
            self.scroll = None;
        }
    }

    pub fn select(&mut self, id: &str) {
        if let Some(i) = self.panes.iter().position(|p| p == id) {
            if self.focus != i {
                self.focus = i;
                self.scroll = None;
            }
            return;
        }
        match self.panes.get_mut(self.focus) {
            Some(slot) => *slot = id.to_string(),
            None => self.panes.push(id.to_string()),
        }
        self.scroll = None;
    }

    pub fn hit(&self, area: Rect, col: u16, row: u16) -> Hit {
        let lay = layout(area, self);
        if let Some(sb) = lay.sidebar {
            if contains(sb, col, row) {
                let idx = usize::from(row - sb.y);
                return match idx
                    .checked_sub(1)
                    .and_then(|i| self.sidebar_rows().get(i).copied())
                {
                    Some(r) => Hit::Session(r.id.clone()),
                    None => Hit::None,
                };
            }
        }
        for p in &lay.panes {
            if contains(p.outer, col, row) {
                let Some(idx) = self.panes.iter().position(|id| *id == p.session) else {
                    return Hit::None;
                };
                let at = contains(p.inner, col, row).then(|| (col - p.inner.x, row - p.inner.y));
                return Hit::Pane { idx, at };
            }
        }
        Hit::None
    }

    pub fn paste_target(&self, area: Rect) -> usize {
        match self.pointer.map(|(c, r)| self.hit(area, c, r)) {
            Some(Hit::Pane { idx, .. }) => idx,
            _ => self.focus,
        }
    }

    pub fn focus_pane(&mut self, idx: usize) {
        if idx < self.panes.len() && idx != self.focus {
            self.focus = idx;
            self.scroll = None;
        }
    }

    pub fn on_mouse(
        &mut self,
        area: Rect,
        ev: MouseEvent,
        info: &dyn Fn(&str) -> PaneInfo,
    ) -> MouseOut {
        let mut out = MouseOut::default();
        let kind = ev.kind();
        self.pointer = Some((ev.col, ev.row));
        if self.help {
            if ev.is_left_press() {
                self.help = false;
                out.redraw = true;
            }
            return out;
        }
        if let Some(g) = self.grab.clone() {
            if matches!(kind, Kind::Drag | Kind::Release) {
                let lay = layout(area, self);
                if let Some(p) = lay
                    .panes
                    .iter()
                    .find(|p| p.session == g && !p.inner.is_empty())
                {
                    let pane = info(&g);
                    let c = ev.col.clamp(p.inner.x, p.inner.right() - 1) - p.inner.x;
                    let r = ev.row.clamp(p.inner.y, p.inner.bottom() - 1) - p.inner.y;
                    if wants(pane.mouse, kind) {
                        out.forward = Some((g, encode(&ev, c, r, pane.mouse.encoding)));
                    }
                }
                if kind == Kind::Release {
                    self.grab = None;
                }
                return out;
            }
        }
        match self.hit(area, ev.col, ev.row) {
            Hit::Session(id) => {
                if ev.is_left_press() {
                    self.select(&id);
                    out.redraw = true;
                }
            }
            Hit::Pane { idx, at } => {
                let session = self.panes[idx].clone();
                let pane = info(&session);
                let local = self.scroll.is_some() && idx == self.focus;
                if matches!(kind, Kind::WheelUp | Kind::WheelDown) {
                    if wants(pane.mouse, kind) && !local {
                        if let Some((c, r)) = at {
                            out.forward = Some((session, encode(&ev, c, r, pane.mouse.encoding)));
                        }
                    } else if pane.alt_screen && pane.scroll_max == 0 {
                        let key: &[u8] = match (kind == Kind::WheelUp, pane.app_cursor) {
                            (true, true) => b"\x1bOA",
                            (true, false) => b"\x1b[A",
                            (false, true) => b"\x1bOB",
                            (false, false) => b"\x1b[B",
                        };
                        out.forward = Some((session, key.repeat(WHEEL_LINES)));
                    } else {
                        out.redraw = self.wheel(idx, kind == Kind::WheelUp, pane.scroll_max);
                    }
                    return out;
                }
                if ev.is_left_press() && idx != self.focus {
                    self.focus_pane(idx);
                    out.redraw = true;
                }
                if !local && wants(pane.mouse, kind) {
                    if let Some((c, r)) = at {
                        if kind == Kind::Press {
                            self.grab = Some(session.clone());
                        }
                        out.forward = Some((session, encode(&ev, c, r, pane.mouse.encoding)));
                    }
                }
            }
            Hit::None => {}
        }
        out
    }

    fn wheel(&mut self, idx: usize, up: bool, scroll_max: usize) -> bool {
        self.focus_pane(idx);
        if up {
            if self.scroll.is_none() {
                if scroll_max == 0 {
                    return false;
                }
                self.scroll = Some(ScrollState::default());
            }
            if let Some(s) = self.scroll.as_mut() {
                s.max = scroll_max;
                s.offset = s.offset.saturating_add(WHEEL_LINES).min(s.max);
            }
            return true;
        }
        let Some(s) = self.scroll.as_mut() else {
            return false;
        };
        s.max = scroll_max;
        s.offset = s.offset.saturating_sub(WHEEL_LINES);
        if s.offset == 0 {
            self.scroll = None;
        }
        true
    }

    pub fn scroll_key(&mut self, key: ScrollKey, page: usize) {
        let Some(s) = self.scroll.as_mut() else {
            return;
        };
        match key {
            ScrollKey::PageUp => s.offset = s.offset.saturating_add(page),
            ScrollKey::PageDown => s.offset = s.offset.saturating_sub(page),
            ScrollKey::Up => s.offset = s.offset.saturating_add(1),
            ScrollKey::Down => s.offset = s.offset.saturating_sub(1),
            ScrollKey::Top => s.offset = s.max,
            ScrollKey::Bottom => s.offset = 0,
            ScrollKey::Exit => {
                self.scroll = None;
                return;
            }
        }
        s.offset = s.offset.min(s.max);
    }

    fn other_pane(&self) -> Option<&str> {
        (self.panes.len() == 2).then(|| self.panes[self.focus ^ 1].as_str())
    }

    fn next_free(&self, from: Option<usize>, forward: bool, skip: Option<&str>) -> Option<String> {
        let n = self.sessions.len();
        if n == 0 {
            return None;
        }
        let start = from.unwrap_or(if forward { n - 1 } else { 0 });
        (1..=n)
            .map(|k| {
                if forward {
                    (start + k) % n
                } else {
                    (start + n - k % n) % n
                }
            })
            .map(|i| &self.sessions[i])
            .find(|s| Some(s.id.as_str()) != skip)
            .map(|s| s.id.clone())
    }

    fn step(&mut self, forward: bool) {
        if self.sessions.is_empty() {
            return;
        }
        let current = self.focused_session().map(str::to_string);
        let from = current
            .as_deref()
            .and_then(|id| self.sessions.iter().position(|s| s.id == id));
        let skip = self.other_pane().map(str::to_string);
        let Some(next) = self.next_free(from, forward, skip.as_deref()) else {
            return;
        };
        if Some(&next) == current.as_ref() {
            return;
        }
        match self.panes.get_mut(self.focus) {
            Some(slot) => *slot = next,
            None => self.panes.push(next),
        }
        self.scroll = None;
    }

    fn split(&mut self) {
        self.scroll = None;
        if self.panes.len() == 2 {
            let keep = self.panes.swap_remove(self.focus);
            self.panes = vec![keep];
            self.focus = 0;
            self.zoom = false;
            return;
        }
        let from = self
            .focused_session()
            .and_then(|id| self.sessions.iter().position(|s| s.id == id));
        let skip = self.focused_session().map(str::to_string);
        match self.next_free(from, true, skip.as_deref()) {
            Some(id) if self.sessions.len() > 1 => {
                self.panes.push(id);
                self.focus = 1;
                self.zoom = false;
            }
            _ => self.notice = Some("only one session, nothing to split".into()),
        }
    }

    pub fn sessions_changed(&mut self, rows: Vec<SessionRow>) {
        self.sessions = rows;
        let focused = self.focused_session().map(str::to_string);
        let exists = |id: &String| self.sessions.iter().any(|s| &s.id == id);
        let kept: Vec<String> = self.panes.iter().filter(|id| exists(id)).cloned().collect();
        let focus = focused
            .and_then(|f| kept.iter().position(|id| *id == f))
            .unwrap_or(0);
        let before = self.panes.clone();
        self.panes = kept;
        self.focus = focus;
        if self.panes.is_empty() {
            self.panes = self
                .sessions
                .first()
                .map(|s| s.id.clone())
                .into_iter()
                .collect();
        }
        if self.panes != before {
            self.scroll = None;
        }
    }

    pub fn prefs(&self) -> LayoutPrefs {
        LayoutPrefs {
            panes: self.panes.clone(),
            focus: self.focus,
            zoom: self.zoom,
            sidebar: self.sidebar,
        }
    }

    pub fn apply_prefs(&mut self, prefs: &LayoutPrefs) {
        let mut panes: Vec<String> = Vec::new();
        for id in &prefs.panes {
            if self.sessions.iter().any(|s| &s.id == id) && !panes.contains(id) && panes.len() < 2 {
                panes.push(id.clone());
            }
        }
        if !panes.is_empty() {
            self.panes = panes;
        }
        self.focus = prefs.focus.min(self.panes.len().saturating_sub(1));
        self.zoom = prefs.zoom;
        self.sidebar = prefs.sidebar;
        self.scroll = None;
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PaneInfo {
    pub mouse: MouseMode,
    pub scroll_max: usize,
    pub alt_screen: bool,
    pub app_cursor: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct MouseOut {
    pub redraw: bool,
    pub forward: Option<(String, Vec<u8>)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hit {
    Session(String),
    Pane { idx: usize, at: Option<(u16, u16)> },
    None,
}

fn contains(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.right() && row >= r.y && row < r.bottom()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneRect {
    pub session: String,
    pub outer: Rect,
    pub inner: Rect,
}

impl PaneRect {
    pub fn content(&self) -> Rect {
        self.outer.inner(Margin::new(1, 1))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub sidebar: Option<Rect>,
    pub panes: Vec<PaneRect>,
    pub status: Rect,
}

fn pane(session: &str, outer: Rect) -> PaneRect {
    let c = outer.inner(Margin::new(1, 1));
    PaneRect {
        session: session.to_string(),
        outer,
        inner: Rect::new(
            outer.x.saturating_add(1),
            outer.y.saturating_add(1),
            c.width.max(1),
            c.height.max(1),
        ),
    }
}

pub fn layout(area: Rect, st: &ViewState) -> Layout {
    let status = Rect::new(
        area.x,
        area.bottom().saturating_sub(1).max(area.y),
        area.width,
        area.height.min(1),
    );
    let mut body = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
    let mut sidebar = None;
    if st.sidebar_shown() {
        let w = SIDEBAR_WIDTH.min(body.width.saturating_sub(MIN_PANE_COLS));
        if w > 0 {
            sidebar = Some(Rect::new(body.x, body.y, w, body.height));
            body = Rect::new(body.x + w, body.y, body.width - w, body.height);
        }
    }
    let shown: Vec<&String> = if st.zoom {
        st.panes.get(st.focus).into_iter().collect()
    } else {
        st.panes.iter().collect()
    };
    let panes = match shown.as_slice() {
        [] => vec![],
        [one] => vec![pane(one, body)],
        [a, b] if body.width >= WIDE_COLS => {
            let left = body.width / 2;
            vec![
                pane(a, Rect::new(body.x, body.y, left, body.height)),
                pane(
                    b,
                    Rect::new(body.x + left, body.y, body.width - left, body.height),
                ),
            ]
        }
        [a, b] => {
            let top = body.height / 2;
            vec![
                pane(a, Rect::new(body.x, body.y, body.width, top)),
                pane(
                    b,
                    Rect::new(body.x, body.y + top, body.width, body.height - top),
                ),
            ]
        }
        _ => vec![],
    };
    Layout {
        sidebar,
        panes,
        status,
    }
}
