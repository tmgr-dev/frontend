use super::keys::{Cmd, ScrollKey};
use ratatui::layout::{Margin, Rect};
use serde::{Deserialize, Serialize};
use tmgr_ipc::api::State;

pub const SIDEBAR_WIDTH: u16 = 24;
pub const WIDE_COLS: u16 = 100;
const MIN_PANE_COLS: u16 = 10;

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
        }
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
            Cmd::Detach | Cmd::Picker => {}
        }
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
