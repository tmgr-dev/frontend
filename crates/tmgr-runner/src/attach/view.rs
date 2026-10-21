use super::keys::{Cmd, ScrollKey};
use super::mouse::{encode, wants, Kind, MouseEvent};
use super::newsession::{NewEdit, NewSession, Place};
use super::palette::{esc_key, Key, Palette, PaletteEntry};
use super::place::Loc;
use super::qmenu::{QEdit, QMenu};
use super::theme::ThemeKind;
use super::title::AgentTitle;
use super::tree::{neighbour, Border, Nav, Node, SplitDir, MAX_LEAVES, RESIZE_STEP};
use crate::questions::Entry;
use ratatui::layout::{Margin, Rect};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tmgr_ipc::api::{MouseMode, State, StateKind};

pub const SIDEBAR_WIDTH: u16 = 24;
pub const MIN_SIDEBAR_WIDTH: u16 = 16;
pub const MAX_SIDEBAR_WIDTH: u16 = 60;
const SIDEBAR_STEP: u16 = 2;
pub const WIDE_COLS: u16 = 100;
const MIN_PANE_COLS: u16 = 10;
const MAX_GROUP_KEYS: usize = 64;
const MAX_TABS: usize = 32;
const MAX_TAB_NAME: usize = 64;
pub const WHEEL_LINES: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRow {
    pub id: String,
    pub name: String,
    pub state: State,
    pub age: String,
    pub you: Option<u64>,
    pub esc: bool,
    pub resumed_from: Option<String>,
}

impl SessionRow {
    pub fn can_interrupt(&self) -> bool {
        self.state.kind == StateKind::Working
    }

    pub fn exited(&self) -> bool {
        self.state.kind == StateKind::Exited
    }

    pub fn interrupt_bytes(&self) -> &'static [u8] {
        if self.esc {
            b"\x1b"
        } else {
            b"\x03"
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Resume,
    Interrupt,
    Stop,
    Close,
}

pub const HEADER_MIN_WIDTH: u16 = 40;

pub fn header_button(outer: Rect, ctl: Control) -> Option<u16> {
    if outer.width < HEADER_MIN_WIDTH {
        return None;
    }
    let base = outer.right().checked_sub(8)?;
    Some(match ctl {
        Control::Interrupt | Control::Resume => base + 1,
        Control::Stop => base + 3,
        Control::Close => base + 5,
    })
}

pub fn sidebar_button(area: Rect, ctl: Control) -> Option<u16> {
    if area.width < 12 {
        return None;
    }
    match ctl {
        Control::Interrupt => None,
        Control::Stop | Control::Resume => area.right().checked_sub(5),
        Control::Close => area.right().checked_sub(3),
    }
}

pub fn slot_control(row: &SessionRow, slot: Control, sidebar: bool) -> Control {
    let play_slot = if sidebar {
        Control::Stop
    } else {
        Control::Interrupt
    };
    if row.exited() && slot == play_slot {
        Control::Resume
    } else {
        slot
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollState {
    pub offset: usize,
    pub max: usize,
}

#[derive(Clone, Debug)]
pub struct ViewState {
    pub sessions: Vec<SessionRow>,
    tree: Option<Node>,
    panes: Vec<String>,
    focus_id: Option<String>,
    pub shells: Vec<String>,
    pub zoom: bool,
    pub sidebar: Option<bool>,
    pub sidebar_w: Option<u16>,
    pub sidebar_default: u16,
    pub theme: ThemeKind,
    border_drag: Option<Option<u16>>,
    split_drag: Option<(Border, u16, Option<Node>)>,
    pub help: bool,
    pub scroll: Option<ScrollState>,
    pub notice: Option<String>,
    pub runner_label: String,
    pub width: u16,
    pub pointer: Option<(u16, u16)>,
    pub grab: Option<String>,
    pub confirm: Option<Confirm>,
    pub rename: Option<Rename>,
    pub palette: Option<Palette>,
    pub new_session: Option<NewSession>,
    pub new_kind: Option<String>,
    pub new_place: Option<Place>,
    pub sidebar_focus: Option<SidebarItem>,
    pub allow_new: bool,
    pub locs: HashMap<String, Loc>,
    pub titles: HashMap<String, AgentTitle>,
    pub questions: Vec<Entry>,
    pub qmenu: Option<QMenu>,
    pub now_s: u64,
    pub grouped: bool,
    pub(crate) collapsed: HashSet<String>,
    group_order: Vec<String>,
    tabs: Vec<Tab>,
    tab_id: u64,
    tab_name: Option<String>,
    next_tab: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Tab {
    id: u64,
    name: Option<String>,
    tree: Node,
    focus: String,
}

impl Tab {
    fn strip(&mut self, ids: &[String]) -> bool {
        let mut tree = Some(self.tree.clone());
        for id in ids {
            Node::remove(&mut tree, id);
        }
        match tree {
            Some(t) if t.leaves().len() >= 2 => {
                if !t.contains(&self.focus) {
                    self.focus = t.leaves()[0].to_string();
                }
                self.tree = t;
                true
            }
            _ => false,
        }
    }

    fn rename(&mut self, old: &str, new: &str) {
        if self.tree.rename_leaf(old, new) && self.focus == old {
            self.focus = new.to_string();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TabItem {
    Tab(u64),
    Session(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarItem {
    Group(String),
    Tab(u64),
    Session(String),
    New,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarLine {
    Header {
        key: String,
        count: usize,
        collapsed: bool,
    },
    Tab {
        id: u64,
        count: usize,
    },
    Session {
        id: String,
        indent: bool,
    },
    Detail(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidebarLayout {
    pub lines: Vec<(SidebarLine, u16)>,
    pub new_row: Option<u16>,
}

impl SidebarLayout {
    pub fn rows(&self) -> Vec<(String, u16)> {
        self.lines
            .iter()
            .filter_map(|(l, y)| match l {
                SidebarLine::Session { id, .. } => Some((id.clone(), *y)),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SidebarEdit {
    Editing,
    Select(String),
    New,
    Leave,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rename {
    pub session: String,
    pub text: String,
    pub tab: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RenameEdit {
    Editing,
    Submit(String, String),
    Done,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirm {
    pub session: String,
    pub name: String,
    pub remove: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutPrefs {
    pub panes: Vec<String>,
    pub focus: usize,
    pub zoom: bool,
    pub sidebar: Option<bool>,
    #[serde(default)]
    pub sidebar_width: Option<u16>,
    #[serde(default)]
    pub tree: Option<Node>,
    #[serde(default)]
    pub shells: Vec<String>,
    #[serde(default)]
    pub new_kind: Option<String>,
    #[serde(default)]
    pub new_place: Option<Place>,
    #[serde(default)]
    pub tab_name: Option<String>,
    #[serde(default, deserialize_with = "lenient_tabs")]
    pub tabs: Vec<TabPrefs>,
}

fn lenient_tabs<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<TabPrefs>, D::Error> {
    let raw = Vec::<serde_json::Value>::deserialize(d).unwrap_or_default();
    Ok(raw
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabPrefs {
    pub tree: Node,
    #[serde(default)]
    pub focus: String,
    #[serde(default)]
    pub name: Option<String>,
}

fn clean_tab_name(name: Option<&str>) -> Option<String> {
    let name: String = name?
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_TAB_NAME)
        .collect();
    (!name.is_empty()).then_some(name)
}

impl ViewState {
    pub fn new(sessions: Vec<SessionRow>, runner_label: &str) -> Self {
        let panes: Vec<String> = sessions.first().map(|s| s.id.clone()).into_iter().collect();
        Self {
            tree: panes.first().map(|id| Node::leaf(id)),
            focus_id: panes.first().cloned(),
            sessions,
            panes,
            shells: Vec::new(),
            zoom: false,
            sidebar: None,
            sidebar_w: None,
            sidebar_default: SIDEBAR_WIDTH,
            theme: ThemeKind::default(),
            border_drag: None,
            split_drag: None,
            help: false,
            scroll: None,
            notice: None,
            runner_label: runner_label.to_string(),
            width: WIDE_COLS,
            pointer: None,
            grab: None,
            confirm: None,
            rename: None,
            palette: None,
            new_session: None,
            new_kind: None,
            new_place: None,
            sidebar_focus: None,
            allow_new: true,
            locs: HashMap::new(),
            titles: HashMap::new(),
            questions: Vec::new(),
            qmenu: None,
            now_s: 0,
            grouped: false,
            collapsed: HashSet::new(),
            group_order: Vec::new(),
            tabs: Vec::new(),
            tab_id: 0,
            tab_name: None,
            next_tab: 1,
        }
    }

    pub fn sidebar_rows(&self) -> Vec<&SessionRow> {
        let mut rows: Vec<&SessionRow> = self.sessions.iter().collect();
        rows.sort_by_key(|r| r.state.kind != StateKind::Blocked);
        rows
    }

    pub fn panes(&self) -> &[String] {
        &self.panes
    }

    pub fn focus(&self) -> usize {
        self.focus_id
            .as_ref()
            .and_then(|f| self.panes.iter().position(|p| p == f))
            .unwrap_or(0)
    }

    pub fn focused_session(&self) -> Option<&str> {
        self.focus_id.as_deref()
    }

    fn commit(&mut self, tree: Option<Node>, focus_id: Option<String>) {
        let before = self.focus_id.take();
        self.panes = tree
            .as_ref()
            .map(|t| t.leaves().into_iter().map(str::to_string).collect())
            .unwrap_or_default();
        self.tree = tree;
        self.focus_id = focus_id
            .filter(|f| self.panes.contains(f))
            .or_else(|| self.panes.first().cloned());
        if self.focus_id != before {
            self.scroll = None;
        }
        if let Some(g) = &self.grab {
            if !g.is_empty() && !self.panes.contains(g) {
                self.grab = None;
            }
        }
        if self.panes.len() < 2 {
            self.tab_name = None;
        }
        let claimed = self.panes.clone();
        self.tabs.retain_mut(|t| t.strip(&claimed));
    }

    fn auto_dir(&self) -> SplitDir {
        if self.width >= WIDE_COLS {
            SplitDir::Right
        } else {
            SplitDir::Down
        }
    }

    pub fn set_initial(&mut self, id: &str) {
        self.open(id);
    }

    pub fn active_tab(&self) -> Option<u64> {
        (self.panes.len() >= 2).then_some(self.tab_id)
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len() + usize::from(self.active_tab().is_some())
    }

    pub fn in_tab(&self, id: &str) -> bool {
        self.panes.iter().any(|p| p == id) || self.tabs.iter().any(|t| t.tree.contains(id))
    }

    pub fn item_of(&self, id: &str) -> TabItem {
        if self.panes.len() >= 2 && self.panes.iter().any(|p| p == id) {
            return TabItem::Tab(self.tab_id);
        }
        match self.tabs.iter().find(|t| t.tree.contains(id)) {
            Some(t) => TabItem::Tab(t.id),
            None => TabItem::Session(id.to_string()),
        }
    }

    pub fn tab_members(&self, tab: u64) -> Vec<String> {
        if self.active_tab() == Some(tab) {
            return self.panes.clone();
        }
        self.tabs
            .iter()
            .find(|t| t.id == tab)
            .map(|t| t.tree.leaves().into_iter().map(str::to_string).collect())
            .unwrap_or_default()
    }

    pub fn tab_title(&self, tab: u64) -> String {
        let name = if self.active_tab() == Some(tab) {
            self.tab_name.clone()
        } else {
            self.tabs
                .iter()
                .find(|t| t.id == tab)
                .and_then(|t| t.name.clone())
        };
        if let Some(n) = name {
            return n;
        }
        let Some(first) = self.tab_members(tab).into_iter().next() else {
            return "tab".into();
        };
        if let Some(loc) = self.locs.get(&first) {
            let root = loc.root.trim_end_matches('/');
            if let Some(last) = root.rsplit('/').next().filter(|s| !s.is_empty()) {
                return last.to_string();
            }
        }
        self.sessions
            .iter()
            .find(|r| r.id == first)
            .map_or(first, |r| r.name.clone())
    }

    pub fn tab_blocked(&self, tab: u64) -> bool {
        let members = self.tab_members(tab);
        self.sessions
            .iter()
            .any(|r| r.state.kind == StateKind::Blocked && members.contains(&r.id))
    }

    pub fn tab_order(&self) -> Vec<(TabItem, Vec<String>)> {
        let mut out: Vec<(TabItem, Vec<String>)> = Vec::new();
        for r in self.sidebar_rows() {
            let item = self.item_of(&r.id);
            if out.iter().any(|(i, _)| *i == item) {
                continue;
            }
            let members = match &item {
                TabItem::Tab(t) => self.tab_members(*t),
                TabItem::Session(s) => vec![s.clone()],
            };
            out.push((item, members));
        }
        out
    }

    fn fresh_tab(&mut self) -> u64 {
        let id = self.next_tab;
        self.next_tab += 1;
        id
    }

    fn stash_active(&mut self) {
        let name = self.tab_name.take();
        if let Some(tree) = self.tree.clone().filter(|t| t.leaves().len() >= 2) {
            self.tabs.push(Tab {
                id: self.tab_id,
                name,
                tree,
                focus: self.focus_id.clone().unwrap_or_default(),
            });
        }
    }

    pub fn switch_tab(&mut self, item: &TabItem, focus: Option<&str>) {
        let current = match item {
            TabItem::Tab(t) => self.active_tab() == Some(*t),
            TabItem::Session(s) => self.panes == [s.clone()],
        };
        if current {
            if let Some(i) = focus.and_then(|f| self.panes.iter().position(|p| p == f)) {
                self.focus_pane(i);
            }
            return;
        }
        let (tree, f) = match item {
            TabItem::Tab(t) => {
                let Some(at) = self.tabs.iter().position(|x| x.id == *t) else {
                    return;
                };
                let tab = self.tabs.remove(at);
                self.stash_active();
                self.tab_id = tab.id;
                self.tab_name = tab.name;
                let f = focus
                    .filter(|f| tab.tree.contains(f))
                    .map_or(tab.focus, str::to_string);
                (tab.tree, f)
            }
            TabItem::Session(s) => {
                self.stash_active();
                self.tab_id = self.fresh_tab();
                (Node::leaf(s), s.clone())
            }
        };
        if self.rename.as_ref().is_some_and(|r| r.tab) {
            self.rename = None;
        }
        self.zoom = false;
        self.commit(Some(tree), Some(f));
        self.scroll = None;
    }

    pub fn open(&mut self, id: &str) {
        let item = self.item_of(id);
        self.switch_tab(&item, Some(id));
    }

    fn step_tab(&mut self, forward: bool) {
        let order: Vec<TabItem> = self.tab_order().into_iter().map(|(i, _)| i).collect();
        let n = order.len();
        if n < 2 {
            return;
        }
        let cur = self.focused_session().map(|f| self.item_of(f));
        let at = cur
            .and_then(|c| order.iter().position(|i| *i == c))
            .unwrap_or(0);
        let next = if forward {
            (at + 1) % n
        } else {
            (at + n - 1) % n
        };
        self.switch_tab(&order[next], None);
    }

    fn close_tab(&mut self) {
        if self.panes.len() < 2 {
            self.notice = Some("a one-pane tab is just its session".into());
            return;
        }
        let keep = self.focus_id.clone();
        self.tab_name = None;
        self.tab_id = self.fresh_tab();
        self.zoom = false;
        self.commit(keep.as_deref().map(Node::leaf), keep);
        self.notice = Some("tab closed, its sessions keep running".into());
    }

    fn rename_tab(&mut self) {
        if self.panes.len() < 2 {
            self.notice = Some("split the pane first, a one-pane tab is just its session".into());
            return;
        }
        let text = self.tab_title(self.tab_id);
        self.rename = Some(Rename {
            session: self.focus_id.clone().unwrap_or_default(),
            text,
            tab: true,
        });
    }

    pub fn split_focused(&mut self, dir: SplitDir, new_id: &str) -> bool {
        let Some(f) = self.focus_id.clone() else {
            return false;
        };
        self.split_at(&f, dir, new_id, true)
    }

    pub fn split_at(&mut self, target: &str, dir: SplitDir, new_id: &str, focus_new: bool) -> bool {
        if self.panes.len() >= MAX_LEAVES {
            return false;
        }
        let Some(mut tree) = self.tree.clone() else {
            return false;
        };
        if !tree.split_leaf(target, dir, new_id) {
            return false;
        }
        let focus = if focus_new {
            Some(new_id.to_string())
        } else {
            self.focus_id.clone()
        };
        self.zoom = false;
        self.commit(Some(tree), focus);
        true
    }

    pub fn close_pane(&mut self, id: &str) {
        if !self.panes.iter().any(|p| p == id) {
            return;
        }
        if self.panes.len() < 2 {
            self.remove_session(id);
            return;
        }
        self.drop_leaf(id);
    }

    fn drop_leaf(&mut self, id: &str) {
        let mut tree = self.tree.take();
        let mut focus = self.focus_id.clone();
        if focus.as_deref() == Some(id) {
            focus = tree.as_ref().and_then(|t| t.sibling_leaf(id));
        }
        Node::remove(&mut tree, id);
        self.shells.retain(|s| s != id);
        self.zoom = false;
        self.commit(tree, focus);
    }

    pub fn focus_nav(&mut self, area: Rect, nav: Nav) -> bool {
        let Some(f) = self.focus_id.clone() else {
            return false;
        };
        let rects: Vec<(String, Rect)> = layout(area, self)
            .panes
            .into_iter()
            .map(|p| (p.session, p.outer))
            .collect();
        if let Some(next) = neighbour(&rects, &f, nav) {
            if let Some(idx) = self.panes.iter().position(|p| *p == next) {
                let moved = idx != self.focus();
                self.focus_pane(idx);
                return moved;
            }
        }
        if nav == Nav::Left {
            self.enter_sidebar(area);
        }
        false
    }

    pub fn enter_sidebar(&mut self, area: Rect) -> bool {
        if !self.sidebar_shown() {
            return false;
        }
        let items = self.sidebar_items(area);
        let start = self
            .focused_session()
            .map(|id| SidebarItem::Session(id.to_string()))
            .filter(|i| items.contains(i))
            .or_else(|| items.first().cloned());
        self.sidebar_focus = start;
        self.sidebar_focus.is_some()
    }

    pub fn sidebar_items(&self, area: Rect) -> Vec<SidebarItem> {
        let Some(rect) = layout(area, self).sidebar else {
            return Vec::new();
        };
        let sl = self.sidebar_layout(rect);
        let mut items: Vec<SidebarItem> = sl
            .lines
            .into_iter()
            .filter_map(|(line, _)| match line {
                SidebarLine::Header { key, .. } => Some(SidebarItem::Group(key)),
                SidebarLine::Tab { id, .. } => Some(SidebarItem::Tab(id)),
                SidebarLine::Session { id, .. } => Some(SidebarItem::Session(id)),
                SidebarLine::Detail(_) => None,
            })
            .collect();
        if sl.new_row.is_some() {
            items.push(SidebarItem::New);
        }
        items
    }

    pub fn sidebar_layout(&self, rect: Rect) -> SidebarLayout {
        if rect.is_empty() {
            return SidebarLayout::default();
        }
        let new_row = (self.allow_new && rect.height >= 4).then(|| rect.bottom() - 1);
        let last = new_row.unwrap_or(rect.bottom());
        let room = usize::from(last.saturating_sub(rect.y + 1));
        let mut plan = self.sidebar_plan();
        if plan.len() > room {
            plan.retain(|l| !matches!(l, SidebarLine::Detail(_)));
        }
        let lines = plan.into_iter().zip(rect.y + 1..last).collect();
        SidebarLayout { lines, new_row }
    }

    fn group_key(&self, id: &str) -> String {
        self.locs
            .get(id)
            .map_or_else(|| format!("\0{id}"), |l| l.root.clone())
    }

    fn groups(&self) -> Vec<(String, Vec<&SessionRow>)> {
        let mut out: Vec<(String, Vec<&SessionRow>)> = self
            .group_order
            .iter()
            .map(|k| (k.clone(), Vec::new()))
            .collect();
        for r in &self.sessions {
            let key = self.group_key(&r.id);
            match out.iter_mut().find(|(k, _)| *k == key) {
                Some((_, m)) => m.push(r),
                None => out.push((key, vec![r])),
            }
        }
        out.retain(|(_, m)| !m.is_empty());
        out
    }

    fn sidebar_plan(&self) -> Vec<SidebarLine> {
        let push = |r: &SessionRow, out: &mut Vec<SidebarLine>, indent: bool| {
            out.push(SidebarLine::Session {
                id: r.id.clone(),
                indent,
            });
            if !indent && self.locs.contains_key(&r.id) {
                out.push(SidebarLine::Detail(r.id.clone()));
            }
        };
        let mut out = Vec::new();
        if !self.grouped {
            for (item, members) in self.tab_order() {
                match item {
                    TabItem::Tab(id) => {
                        out.push(SidebarLine::Tab {
                            id,
                            count: members.len(),
                        });
                        for m in members {
                            out.push(SidebarLine::Session {
                                id: m.clone(),
                                indent: true,
                            });
                            if self.locs.contains_key(&m) {
                                out.push(SidebarLine::Detail(m));
                            }
                        }
                    }
                    TabItem::Session(id) => {
                        if let Some(r) = self.sessions.iter().find(|r| r.id == id) {
                            push(r, &mut out, false);
                        }
                    }
                }
            }
            return out;
        }
        for (key, members) in self.groups() {
            if let [only] = members.as_slice() {
                push(only, &mut out, false);
                continue;
            }
            let collapsed = self.collapsed.contains(&key);
            out.push(SidebarLine::Header {
                key,
                count: members.len(),
                collapsed,
            });
            if !collapsed {
                for r in members {
                    push(r, &mut out, true);
                }
            }
        }
        out
    }

    pub fn group_title(&self, key: &str) -> Option<(String, Option<String>)> {
        let first = self
            .sessions
            .iter()
            .find(|r| self.group_key(&r.id) == key)?;
        let loc = self.locs.get(&first.id)?;
        Some((super::place::tail2(&loc.root), loc.branch.clone()))
    }

    pub fn group_blocked(&self, key: &str) -> bool {
        self.sessions
            .iter()
            .any(|r| r.state.kind == StateKind::Blocked && self.group_key(&r.id) == key)
    }

    pub fn group_holds(&self, key: &str, session: &str) -> bool {
        self.group_key(session) == key
    }

    pub fn is_collapsed(&self, key: &str) -> bool {
        self.collapsed.contains(key)
    }

    fn toggle_group(&mut self, key: &str) {
        if !self.collapsed.remove(key) {
            self.collapsed.insert(key.to_string());
        }
    }

    fn refresh_groups(&mut self) {
        let ids: HashSet<&str> = self.sessions.iter().map(|r| r.id.as_str()).collect();
        self.locs.retain(|id, _| ids.contains(id.as_str()));
        self.titles.retain(|id, _| ids.contains(id.as_str()));
        let keys: Vec<String> = self
            .sessions
            .iter()
            .map(|r| self.group_key(&r.id))
            .collect();
        self.group_order
            .retain(|k| !k.starts_with('\0') || keys.contains(k));
        for k in &keys {
            if !self.group_order.contains(k) {
                self.group_order.push(k.clone());
            }
        }
        self.collapsed.retain(|k| keys.contains(k));
        let mut extra = self.group_order.len().saturating_sub(MAX_GROUP_KEYS);
        self.group_order.retain(|k| {
            let drop = extra > 0 && !keys.contains(k);
            extra -= usize::from(drop);
            !drop
        });
    }

    pub fn set_locs(&mut self, area: Rect, updates: Vec<(String, Loc)>) -> bool {
        let before = self.sidebar_items(area);
        let mut changed = false;
        for (id, loc) in updates {
            if !self.sessions.iter().any(|r| r.id == id) {
                continue;
            }
            if self.locs.get(&id) != Some(&loc) {
                self.locs.insert(id, loc);
                changed = true;
            }
        }
        if changed {
            self.refresh_groups();
            self.keep_sidebar_focus(area, &before);
        }
        changed
    }

    pub fn set_titles(&mut self, updates: Vec<(String, Option<AgentTitle>)>) -> bool {
        let mut changed = false;
        for (id, title) in updates {
            if !self.sessions.iter().any(|r| r.id == id) {
                continue;
            }
            match title {
                Some(t) if self.titles.get(&id) != Some(&t) => {
                    self.titles.insert(id, t);
                    changed = true;
                }
                None => changed |= self.titles.remove(&id).is_some(),
                _ => {}
            }
        }
        changed
    }

    pub fn agent_title(&self, row: &SessionRow) -> Option<&AgentTitle> {
        self.titles.get(&row.id).filter(|t| t.base == row.name)
    }

    pub fn visible_sessions(&self, area: Rect) -> Vec<String> {
        let Some(rect) = layout(area, self).sidebar else {
            return Vec::new();
        };
        self.sidebar_layout(rect)
            .rows()
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }

    fn nav_order(&self) -> Vec<&str> {
        if !self.grouped {
            return self.sessions.iter().map(|r| r.id.as_str()).collect();
        }
        self.groups()
            .into_iter()
            .flat_map(|(_, m)| m)
            .map(|r| r.id.as_str())
            .collect()
    }

    fn folded(&self) -> HashSet<&str> {
        if !self.grouped {
            return HashSet::new();
        }
        self.groups()
            .into_iter()
            .filter(|(k, m)| m.len() > 1 && self.collapsed.contains(k))
            .flat_map(|(_, m)| m)
            .map(|r| r.id.as_str())
            .collect()
    }

    pub fn edit_sidebar(&mut self, area: Rect, data: &[u8]) -> SidebarEdit {
        if self.sidebar_focus.is_none() {
            return SidebarEdit::Leave;
        }
        if !data.is_empty() && data.iter().all(|&b| b == 0x1b) {
            self.sidebar_focus = None;
            return SidebarEdit::Leave;
        }
        let mut i = 0;
        while i < data.len() {
            let step = match data[i] {
                0x1b => {
                    let rest = &data[i..];
                    if rest.starts_with(b"\x1b[C") || rest.starts_with(b"\x1bOC") {
                        self.sidebar_focus = None;
                        return SidebarEdit::Leave;
                    }
                    let (key, used) = esc_key(rest);
                    match key {
                        Key::Up => self.move_sidebar_focus(area, false),
                        Key::Down => self.move_sidebar_focus(area, true),
                        Key::Other => {}
                    }
                    used
                }
                0x10 | b'k' => {
                    self.move_sidebar_focus(area, false);
                    1
                }
                0x0e | b'j' => {
                    self.move_sidebar_focus(area, true);
                    1
                }
                b'\n' if i > 0 && data[i - 1] == b'\r' => 1,
                b'\r' | b'\n' => {
                    let items = self.sidebar_items(area);
                    if let Some(SidebarItem::Group(key)) =
                        self.sidebar_focus.clone().filter(|f| items.contains(f))
                    {
                        self.toggle_group(&key);
                        i += 1;
                        continue;
                    }
                    return match self.sidebar_focus.take().filter(|f| items.contains(f)) {
                        Some(SidebarItem::Session(id))
                            if self.sessions.iter().any(|s| s.id == id) =>
                        {
                            self.open(&id);
                            SidebarEdit::Select(id)
                        }
                        Some(SidebarItem::Tab(t)) => {
                            self.switch_tab(&TabItem::Tab(t), None);
                            match self.focused_session() {
                                Some(f) => SidebarEdit::Select(f.to_string()),
                                None => SidebarEdit::Leave,
                            }
                        }
                        Some(SidebarItem::New) if self.allow_new => SidebarEdit::New,
                        _ => SidebarEdit::Leave,
                    };
                }
                _ => 1,
            };
            i += step;
        }
        SidebarEdit::Editing
    }

    fn move_sidebar_focus(&mut self, area: Rect, down: bool) {
        let items = self.sidebar_items(area);
        let Some(cur) = self
            .sidebar_focus
            .as_ref()
            .and_then(|f| items.iter().position(|i| i == f))
        else {
            self.sidebar_focus = items.first().cloned();
            return;
        };
        let next = if down {
            (cur + 1).min(items.len() - 1)
        } else {
            cur.saturating_sub(1)
        };
        self.sidebar_focus = Some(items[next].clone());
    }

    fn keep_sidebar_focus(&mut self, area: Rect, before: &[SidebarItem]) {
        if !self.sidebar_shown() {
            self.sidebar_focus = None;
            return;
        }
        let Some(cur) = self.sidebar_focus.clone() else {
            return;
        };
        let items = self.sidebar_items(area);
        if items.contains(&cur) {
            return;
        }
        let at = before.iter().position(|i| *i == cur).unwrap_or(0);
        self.sidebar_focus = before[at..]
            .iter()
            .chain(before[..at].iter().rev())
            .find(|i| items.contains(i))
            .cloned()
            .or_else(|| items.first().cloned());
    }

    pub fn resize_focused(&mut self, area: Rect, nav: Nav) {
        let (Some(mut tree), Some(f)) = (self.tree.clone(), self.focus_id.clone()) else {
            return;
        };
        if tree.resize(body_rect(area, self), &f, nav, RESIZE_STEP) {
            self.tree = Some(tree);
        }
    }

    fn cycle_focus(&mut self, forward: bool) {
        let n = self.panes.len();
        if n < 2 {
            return;
        }
        let at = self.focus();
        self.focus_pane(if forward {
            (at + 1) % n
        } else {
            (at + n - 1) % n
        });
    }

    pub fn focused_row(&self) -> Option<&SessionRow> {
        let id = self.focused_session()?;
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn sidebar_shown(&self) -> bool {
        !self.zoom && self.sidebar.unwrap_or(self.width >= WIDE_COLS)
    }

    pub fn apply(&mut self, cmd: Cmd) {
        self.sidebar_focus = None;
        match cmd {
            Cmd::NextSession => self.step(true),
            Cmd::PrevSession => self.step(false),
            Cmd::Split => self.split(),
            Cmd::FocusOther => self.cycle_focus(true),
            Cmd::FocusPrev => self.cycle_focus(false),
            Cmd::SplitRight | Cmd::SplitDown | Cmd::Focus(_) | Cmd::Resize(_) => {}
            Cmd::NextTab => self.step_tab(true),
            Cmd::PrevTab => self.step_tab(false),
            Cmd::RenameTab => self.rename_tab(),
            Cmd::CloseTab => self.close_tab(),
            Cmd::Zoom => self.zoom = !self.zoom,
            Cmd::Group => self.grouped = !self.grouped,
            Cmd::Sidebar => {
                let shown = self.sidebar.unwrap_or(self.width >= WIDE_COLS);
                self.sidebar = Some(!shown);
            }
            Cmd::SidebarNarrower => self.resize_sidebar(false),
            Cmd::SidebarWider => self.resize_sidebar(true),
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
                        remove: false,
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
                        tab: false,
                    });
                }
            }
            Cmd::Palette => {
                if self.palette.take().is_none() {
                    self.palette = Some(Palette::default());
                    self.help = false;
                    self.grab = None;
                }
            }
            Cmd::Questions => self.toggle_questions(),
            Cmd::Detach | Cmd::Picker | Cmd::Resume | Cmd::NewSession => {}
        }
    }

    pub fn replace_session(&mut self, old: &str, new: &str) {
        if old == new {
            return;
        }
        self.sessions.retain(|s| s.id != old);
        if self.shells.iter().any(|s| s == old) {
            self.shells.retain(|s| s != old);
            if !self.shells.iter().any(|s| s == new) {
                self.shells.push(new.to_string());
            }
        }
        if !self.panes.iter().any(|p| p == old) {
            for t in &mut self.tabs {
                t.rename(old, new);
            }
            return;
        }
        let mut tree = self.tree.take();
        if let Some(t) = tree.as_mut() {
            t.rename_leaf(old, new);
        }
        let focus = if self.focus_id.as_deref() == Some(old) {
            Some(new.to_string())
        } else {
            self.focus_id.clone()
        };
        self.commit(tree, focus);
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
                        Some(r) if r.tab => {
                            self.tab_name = clean_tab_name(Some(&r.text));
                            RenameEdit::Done
                        }
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
        let was_shell = self.shells.iter().any(|s| s == id);
        if !self.panes.iter().any(|p| p == id) {
            self.shells.retain(|s| s != id);
            let gone = [id.to_string()];
            self.tabs.retain_mut(|t| t.strip(&gone));
            return;
        }
        if was_shell && self.panes.len() > 1 {
            self.drop_leaf(id);
            return;
        }
        self.shells.retain(|s| s != id);
        let neighbour = self.sessions[pos..]
            .iter()
            .chain(self.sessions[..pos].iter().rev())
            .find(|s| !self.in_tab(&s.id))
            .map(|s| s.id.clone());
        match neighbour {
            Some(n) => {
                let mut tree = self.tree.take();
                if let Some(t) = tree.as_mut() {
                    t.rename_leaf(id, &n);
                }
                let focus = if self.focus_id.as_deref() == Some(id) {
                    Some(n)
                } else {
                    self.focus_id.clone()
                };
                self.commit(tree, focus);
                self.scroll = None;
            }
            None => {
                self.drop_leaf(id);
                self.scroll = None;
                if self.tree.is_none() {
                    self.show_first_tab();
                }
            }
        }
    }

    pub fn select(&mut self, id: &str) {
        if let Some(i) = self.panes.iter().position(|p| p == id) {
            self.focus_pane(i);
            return;
        }
        self.replace_focused(id.to_string());
    }

    fn replace_focused(&mut self, id: String) {
        let mut tree = self.tree.take();
        let swapped = match (tree.as_mut(), self.focus_id.as_deref()) {
            (Some(t), Some(f)) => t.rename_leaf(f, &id),
            _ => false,
        };
        if !swapped {
            tree = Some(Node::leaf(&id));
        }
        self.commit(tree, Some(id));
        self.scroll = None;
    }

    pub fn sidebar_width(&self) -> u16 {
        clamp_sidebar(self.sidebar_w.unwrap_or(self.sidebar_default))
    }

    fn resize_sidebar(&mut self, wider: bool) {
        let w = self.sidebar_width().min(sidebar_cap(self.width));
        self.sidebar_w = Some(clamp_sidebar(if wider {
            w.saturating_add(SIDEBAR_STEP)
        } else {
            w.saturating_sub(SIDEBAR_STEP)
        }));
    }

    fn on_sidebar_border(&self, area: Rect, col: u16, row: u16) -> bool {
        let Some(sb) = layout(area, self).sidebar else {
            return false;
        };
        row >= sb.y && row < sb.bottom() && (col == sb.right() || col + 1 == sb.right())
    }

    fn split_border_mouse(&mut self, area: Rect, ev: MouseEvent, out: &mut MouseOut) -> bool {
        if ev.kind() == Kind::Press {
            self.split_drag = None;
        }
        if let Some((border, delta, _)) = self.split_drag.clone() {
            let body = body_rect(area, self);
            let live = !self.zoom
                && self.tree.as_ref().is_some_and(|t| {
                    t.borders(body).iter().any(|b| {
                        b.path == border.path && b.dir == border.dir && b.rect == border.rect
                    })
                });
            if !live {
                self.split_drag = None;
                out.redraw = true;
                return true;
            }
            match ev.kind() {
                Kind::Drag => {
                    let at = match border.dir {
                        SplitDir::Right => ev.col,
                        SplitDir::Down => ev.row,
                    };
                    let first = (at + delta).saturating_sub(border.start());
                    if let Some(tree) = self.tree.as_mut() {
                        out.redraw |= tree.drag_border(&border, first);
                    }
                }
                Kind::Release => {
                    self.split_drag = None;
                    out.redraw = true;
                }
                _ => {}
            }
            return true;
        }
        if !ev.is_left_press() || self.zoom {
            return false;
        }
        if matches!(self.hit(area, ev.col, ev.row), Hit::Button(..))
            || self.on_sidebar_border(area, ev.col, ev.row)
        {
            return false;
        }
        let Some(tree) = self.tree.as_ref() else {
            return false;
        };
        let Some((border, first)) = tree.border_at(body_rect(area, self), ev.col, ev.row) else {
            return false;
        };
        if let Hit::Pane { idx, .. } = self.hit(area, ev.col, ev.row) {
            self.focus_pane(idx);
        }
        self.split_drag = Some((border, u16::from(first), self.tree.clone()));
        out.redraw = true;
        true
    }

    fn border_mouse(&mut self, area: Rect, ev: MouseEvent, out: &mut MouseOut) -> bool {
        if self.split_border_mouse(area, ev, out) {
            return true;
        }
        if self.border_drag.is_some() {
            match ev.kind() {
                Kind::Drag => {
                    let w = (ev.col.saturating_sub(area.x) + 1).min(sidebar_cap(area.width));
                    self.sidebar_w = Some(clamp_sidebar(w));
                    out.redraw = true;
                }
                Kind::Release => {
                    self.border_drag = None;
                    out.redraw = true;
                }
                _ => {}
            }
            return true;
        }
        if ev.kind() == Kind::Press {
            self.border_drag = None;
        }
        if ev.is_left_press() && self.on_sidebar_border(area, ev.col, ev.row) {
            self.border_drag = Some(self.sidebar_w);
            return true;
        }
        false
    }

    pub fn hit(&self, area: Rect, col: u16, row: u16) -> Hit {
        let lay = layout(area, self);
        if let Some(sb) = lay.sidebar {
            if contains(sb, col, row) {
                let sl = self.sidebar_layout(sb);
                if sl.new_row == Some(row) {
                    return Hit::NewSession;
                }
                let Some((line, _)) = sl.lines.iter().find(|(_, y)| *y == row) else {
                    return Hit::None;
                };
                let id = match line {
                    SidebarLine::Header { key, .. } => return Hit::Group(key.clone()),
                    SidebarLine::Tab { id, .. } => return Hit::Tab(*id),
                    SidebarLine::Detail(id) => return Hit::Session(id.clone()),
                    SidebarLine::Session { id, .. } => id,
                };
                let Some(r) = self.sessions.iter().find(|s| s.id == *id) else {
                    return Hit::None;
                };
                for slot in [Control::Stop, Control::Close] {
                    if sidebar_button(sb, slot) == Some(col) {
                        return Hit::Button(r.id.clone(), slot_control(r, slot, true));
                    }
                }
                let off = u16::from(matches!(line, SidebarLine::Session { indent: true, .. })) * 2;
                if let Some((x, w)) = self.sidebar_badge(sb, &r.id, off) {
                    if col >= x && col < x + w {
                        return Hit::Questions(r.id.clone());
                    }
                }
                return Hit::Session(r.id.clone());
            }
        }
        for p in &lay.panes {
            if contains(p.outer, col, row) {
                if row == p.outer.y {
                    for slot in [Control::Interrupt, Control::Stop, Control::Close] {
                        if header_button(p.outer, slot) == Some(col) {
                            let ctl = self
                                .sessions
                                .iter()
                                .find(|s| s.id == p.session)
                                .map_or(slot, |r| slot_control(r, slot, false));
                            return Hit::Button(p.session.clone(), ctl);
                        }
                    }
                }
                if row == p.outer.y {
                    if let Some((x, w)) = self.header_badge(p.outer, &p.session) {
                        if col >= x && col < x + w {
                            return Hit::Questions(p.session.clone());
                        }
                    }
                }
                let Some(idx) = self.panes.iter().position(|id| *id == p.session) else {
                    return Hit::None;
                };
                let at = contains(p.inner, col, row).then(|| (col - p.inner.x, row - p.inner.y));
                return Hit::Pane { idx, at };
            }
        }
        Hit::None
    }

    fn hover_key(&self, area: Rect) -> Option<Hit> {
        let (c, r) = self.pointer?;
        match self.hit(area, c, r) {
            h @ (Hit::Button(..)
            | Hit::Session(_)
            | Hit::Group(_)
            | Hit::Tab(_)
            | Hit::Questions(_)
            | Hit::NewSession) => Some(h),
            _ => None,
        }
    }

    pub fn paste_target(&self, area: Rect) -> usize {
        match self.pointer.map(|(c, r)| self.hit(area, c, r)) {
            Some(Hit::Pane { idx, .. }) => idx,
            _ => self.focus(),
        }
    }

    pub fn focus_pane(&mut self, idx: usize) {
        if idx < self.panes.len() && idx != self.focus() {
            self.focus_id = Some(self.panes[idx].clone());
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
        let before = self.hover_key(area);
        self.pointer = Some((ev.col, ev.row));
        if self.hover_key(area) != before {
            out.redraw = true;
        }
        if kind == Kind::Move
            || self.new_session.is_some()
            || self.palette.is_some()
            || self.qmenu.is_some()
            || self.help
        {
            self.border_drag = None;
            self.split_drag = None;
        }
        if self.new_session.is_some() {
            return self.new_session_mouse(area, ev, out);
        }
        if self.palette.is_some() {
            return self.palette_mouse(area, ev, out);
        }
        if self.qmenu.is_some() {
            return self.qmenu_mouse(area, ev, out);
        }
        if ev.is_left_press() && self.sidebar_focus.take().is_some() {
            out.redraw = true;
        }
        if ev.is_left_press() && !matches!(self.hit(area, ev.col, ev.row), Hit::Button(..)) {
            self.confirm = None;
        }
        if self.help {
            if ev.is_left_press() {
                self.help = false;
                out.redraw = true;
            }
            return out;
        }
        if self.border_mouse(area, ev, &mut out) {
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
            Hit::Button(id, ctl) => {
                if ev.is_left_press() {
                    self.grab = Some(String::new());
                    out.control = Some((id, ctl));
                }
            }
            Hit::Session(id) => {
                if ev.is_left_press() {
                    self.open(&id);
                    out.redraw = true;
                }
            }
            Hit::Tab(t) => {
                if ev.is_left_press() {
                    self.switch_tab(&TabItem::Tab(t), None);
                    out.redraw = true;
                }
            }
            Hit::Pane { idx, at } => {
                let session = self.panes[idx].clone();
                let pane = info(&session);
                let local = self.scroll.is_some() && idx == self.focus();
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
                if ev.is_left_press() && idx != self.focus() {
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
            Hit::Group(key) => {
                if ev.is_left_press() {
                    let before = self.sidebar_items(area);
                    self.toggle_group(&key);
                    self.keep_sidebar_focus(area, &before);
                    out.redraw = true;
                }
            }
            Hit::Questions(id) => {
                if ev.is_left_press() {
                    self.open(&id);
                    self.open_questions(Some(id));
                    out.redraw = true;
                }
            }
            Hit::NewSession => {
                if ev.is_left_press() {
                    out.new_session = true;
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

    fn next_free(&self, from: Option<&str>, forward: bool) -> Option<String> {
        let order = self.nav_order();
        let n = order.len();
        if n == 0 {
            return None;
        }
        let folded = self.folded();
        let start = from
            .and_then(|f| order.iter().position(|id| *id == f))
            .unwrap_or(if forward { n - 1 } else { 0 });
        let pick = |skip_folded: bool| {
            (1..=n)
                .map(|k| {
                    if forward {
                        (start + k) % n
                    } else {
                        (start + n - k % n) % n
                    }
                })
                .map(|i| order[i])
                .find(|id| !(self.in_tab(id) || skip_folded && folded.contains(id)))
        };
        pick(true).or_else(|| pick(false)).map(str::to_string)
    }

    fn step(&mut self, forward: bool) {
        let from = self.focused_session().map(str::to_string);
        let Some(next) = self.next_free(from.as_deref(), forward) else {
            return;
        };
        self.replace_focused(next);
    }

    fn split(&mut self) {
        self.scroll = None;
        match self.panes.len() {
            0 => self.show_first_tab(),
            1 => {
                let from = self.focused_session().map(str::to_string);
                match self.next_free(from.as_deref(), true) {
                    Some(id) => {
                        let dir = self.auto_dir();
                        self.split_focused(dir, &id);
                    }
                    None if self.sessions.len() > 1 => {
                        self.notice =
                            Some("the other sessions are in tabs, Ctrl-] c starts one".into())
                    }
                    None => self.notice = Some("only one session, nothing to split".into()),
                }
            }
            _ => {
                let hidden = self
                    .shells
                    .iter()
                    .any(|s| self.panes.contains(s) && self.focus_id.as_ref() != Some(s));
                let keep = self.focus_id.clone();
                self.commit(keep.as_deref().map(Node::leaf), keep);
                self.zoom = false;
                if hidden {
                    self.notice = Some("hidden shells keep running, see the sidebar".into());
                }
            }
        }
    }

    pub fn sessions_changed(&mut self, area: Rect, rows: Vec<SessionRow>) {
        let before = self.sidebar_items(area);
        self.apply_sessions(rows);
        self.keep_sidebar_focus(area, &before);
    }

    fn apply_sessions(&mut self, rows: Vec<SessionRow>) {
        self.sessions = rows;
        if let Some(p) = self.palette.as_mut() {
            p.sel = 0;
        }
        let renames: Vec<(String, String)> = self
            .sessions
            .iter()
            .filter_map(|r| {
                let old = r.resumed_from.as_ref()?;
                let vanished = !self.sessions.iter().any(|s| &s.id == old);
                (vanished && self.in_tab(old) && !self.in_tab(&r.id))
                    .then(|| (old.clone(), r.id.clone()))
            })
            .collect();
        for (old, new) in renames {
            if let Some(loc) = self.locs.get(&old).cloned() {
                self.locs.entry(new.clone()).or_insert(loc);
            }
            if let Some(title) = self.titles.get(&old).cloned() {
                self.titles.entry(new.clone()).or_insert(title);
            }
            self.replace_session(&old, &new);
        }
        self.refresh_groups();
        let ids: HashSet<&str> = self.sessions.iter().map(|r| r.id.as_str()).collect();
        let stale: Vec<String> = self
            .tabs
            .iter()
            .flat_map(|t| t.tree.leaves())
            .filter(|id| !ids.contains(id))
            .map(str::to_string)
            .collect();
        self.tabs.retain_mut(|t| t.strip(&stale));
        let gone: Vec<String> = self
            .panes
            .iter()
            .filter(|id| !self.sessions.iter().any(|s| &s.id == *id))
            .cloned()
            .collect();
        self.shells
            .retain(|id| self.sessions.iter().any(|s| &s.id == id));
        if gone.is_empty() {
            if self.tree.is_none() {
                self.show_first_tab();
            }
            return;
        }
        let mut tree = self.tree.take();
        let mut focus = self.focus_id.clone();
        for id in &gone {
            if focus.as_deref() == Some(id.as_str()) {
                focus = tree.as_ref().and_then(|t| t.sibling_leaf(id));
            }
            Node::remove(&mut tree, id);
        }
        self.commit(tree, focus);
        if self.tree.is_none() {
            self.show_first_tab();
        }
    }

    fn show_first_tab(&mut self) {
        self.tab_name = None;
        if let Some(item) = self.sessions.first().map(|r| self.item_of(&r.id)) {
            self.switch_tab(&item, None);
        }
    }

    pub fn prefs(&self) -> LayoutPrefs {
        LayoutPrefs {
            panes: self.panes.clone(),
            focus: self.focus(),
            zoom: self.zoom,
            sidebar: self.sidebar,
            sidebar_width: self.border_drag.unwrap_or(self.sidebar_w),
            tree: match &self.split_drag {
                Some((_, _, before)) => before.clone(),
                None => self.tree.clone(),
            },
            shells: self.shells.clone(),
            new_kind: self.new_kind.clone(),
            new_place: self.new_place.filter(|p| *p != Place::Tab),
            tab_name: self.active_tab().and(self.tab_name.clone()),
            tabs: self
                .tabs
                .iter()
                .take(MAX_TABS)
                .map(|t| TabPrefs {
                    tree: t.tree.clone(),
                    focus: t.focus.clone(),
                    name: t.name.clone(),
                })
                .collect(),
        }
    }

    fn tree_from_panes(&self, panes: &[String]) -> Option<Node> {
        let dir = self.auto_dir();
        let n = panes.len();
        let mut node = Node::leaf(panes.last()?);
        for k in (0..n - 1).rev() {
            node = Node::Split {
                dir,
                ratio: 1000 / (n - k) as u16,
                a: Box::new(Node::leaf(&panes[k])),
                b: Box::new(node),
            };
        }
        Some(node)
    }

    pub fn apply_prefs(&mut self, prefs: &LayoutPrefs) {
        let known = |id: &str| self.sessions.iter().any(|s| s.id == id);
        let mut tree = prefs.tree.clone().filter(Node::is_valid);
        if let Some(t) = tree.as_mut() {
            t.clamp_ratios();
        }
        let missing: Vec<String> = tree
            .iter()
            .flat_map(Node::leaves)
            .filter(|id| !known(id))
            .map(str::to_string)
            .collect();
        for id in &missing {
            Node::remove(&mut tree, id);
        }
        if tree.is_none() {
            let mut panes: Vec<String> = Vec::new();
            for id in &prefs.panes {
                if known(id) && !panes.contains(id) && panes.len() < MAX_LEAVES {
                    panes.push(id.clone());
                }
            }
            tree = self.tree_from_panes(&panes);
        }
        let shells: Vec<String> = prefs
            .shells
            .iter()
            .filter(|id| known(id))
            .cloned()
            .collect();
        let tree = tree.or_else(|| self.tree.clone());
        let leaves: Vec<String> = tree
            .iter()
            .flat_map(Node::leaves)
            .map(str::to_string)
            .collect();
        let focus = leaves.get(prefs.focus).or_else(|| leaves.last()).cloned();
        self.tabs.clear();
        self.tab_id = self.fresh_tab();
        self.commit(tree, focus);
        self.tab_name = clean_tab_name(prefs.tab_name.as_deref());
        self.load_tabs(&prefs.tabs);
        self.shells = shells;
        self.zoom = prefs.zoom;
        self.sidebar = prefs.sidebar;
        if prefs.sidebar_width.is_some() {
            self.sidebar_w = prefs.sidebar_width;
        }
        self.new_kind = prefs.new_kind.clone();
        self.new_place = prefs.new_place;
        self.scroll = None;
    }
}

impl ViewState {
    fn load_tabs(&mut self, saved: &[TabPrefs]) {
        let mut claimed: Vec<String> = self.panes.clone();
        for tp in saved {
            if self.tabs.len() >= MAX_TABS {
                break;
            }
            let mut tree = Some(tp.tree.clone()).filter(Node::is_valid);
            if let Some(t) = tree.as_mut() {
                t.clamp_ratios();
            }
            let drop: Vec<String> = tree
                .iter()
                .flat_map(Node::leaves)
                .filter(|id| {
                    claimed.iter().any(|c| c == id) || !self.sessions.iter().any(|s| s.id == *id)
                })
                .map(str::to_string)
                .collect();
            for id in &drop {
                Node::remove(&mut tree, id);
            }
            let Some(tree) = tree.filter(|t| t.leaves().len() >= 2) else {
                continue;
            };
            let leaves: Vec<String> = tree.leaves().into_iter().map(str::to_string).collect();
            let focus = if tree.contains(&tp.focus) {
                tp.focus.clone()
            } else {
                leaves[0].clone()
            };
            claimed.extend(leaves);
            let id = self.fresh_tab();
            self.tabs.push(Tab {
                id,
                name: clean_tab_name(tp.name.as_deref()),
                tree,
                focus,
            });
        }
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
    pub control: Option<(String, Control)>,
    pub palette: Option<PaletteEntry>,
    pub new_session: bool,
    pub new_edit: Option<NewEdit>,
    pub question: Option<QEdit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hit {
    Button(String, Control),
    Session(String),
    Group(String),
    Tab(u64),
    Questions(String),
    NewSession,
    Pane { idx: usize, at: Option<(u16, u16)> },
    None,
}

fn sidebar_cap(cols: u16) -> u16 {
    cols.saturating_sub(MIN_PANE_COLS)
}

fn clamp_sidebar(w: u16) -> u16 {
    w.clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH)
}

pub(crate) fn contains(r: Rect, col: u16, row: u16) -> bool {
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

fn body_rect(area: Rect, st: &ViewState) -> Rect {
    let mut body = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
    if st.sidebar_shown() {
        let w = st
            .sidebar_width()
            .min(body.width.saturating_sub(MIN_PANE_COLS));
        if w > 0 {
            body = Rect::new(body.x + w, body.y, body.width - w, body.height);
        }
    }
    body
}

pub fn layout(area: Rect, st: &ViewState) -> Layout {
    let status = Rect::new(
        area.x,
        area.bottom().saturating_sub(1).max(area.y),
        area.width,
        area.height.min(1),
    );
    let body = body_rect(area, st);
    let sidebar = (area.width > body.width)
        .then(|| Rect::new(area.x, area.y, area.width - body.width, body.height));
    let focused = st.focused_session();
    let full = || focused.map(|id| vec![pane(id, body)]).unwrap_or_default();
    let panes = if st.zoom {
        full()
    } else {
        match &st.tree {
            None => vec![],
            Some(t) => match t.layout(body) {
                Some(rects) => rects.iter().map(|(id, r)| pane(id, *r)).collect(),
                None => full(),
            },
        }
    };
    Layout {
        sidebar,
        panes,
        status,
    }
}
