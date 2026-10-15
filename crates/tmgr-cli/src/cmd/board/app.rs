use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use std::cell::RefCell;
use std::collections::HashMap;
use tmgr_core::error::Kind;
use tmgr_core::models::{ApiTask, Status};
use tmgr_core::text::sanitize_line;
use tmgr_core::view::{display_title, task_ref};

use super::filters::{self, Assignee, Filters, PersonaSel, Saved};

pub const POLL_SECS: i64 = 30;
pub const MAX_INPUT_CHARS: usize = 2000;
const HIDDEN_KINDS: [&str; 2] = ["hidden", "archived"];

#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub id: u64,
    pub key: String,
    pub title: String,
    pub status_id: Option<u64>,
    pub running: bool,
    pub start_time: i64,
    pub tracked: i64,
    pub assignees: Vec<String>,
    pub people: Vec<(i64, String)>,
    pub personas: Vec<PersonaRef>,
    pub description: Option<String>,
}

impl Card {
    pub fn from_task(t: &ApiTask) -> Card {
        let start = t.start_time.unwrap_or(0);
        Card {
            id: t.id,
            key: sanitize_line(&task_ref(t)),
            title: display_title(&t.title),
            status_id: t.status_id,
            running: start > 0,
            start_time: start,
            tracked: t.common_time.unwrap_or(0),
            assignees: t
                .assignees
                .iter()
                .filter_map(|p| p.name.as_deref())
                .map(sanitize_line)
                .collect(),
            people: t
                .assignees
                .iter()
                .filter_map(|p| {
                    Some((p.id?, sanitize_line(p.name.as_deref().unwrap_or("unknown"))))
                })
                .collect(),
            personas: t
                .persona_assignees
                .iter()
                .filter_map(PersonaRef::parse)
                .collect(),
            description: t
                .description
                .as_deref()
                .filter(|d| !d.trim().is_empty())
                .map(plain_text),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PersonaRef {
    pub id: String,
    pub name: String,
    pub owner_id: Option<i64>,
}

impl PersonaRef {
    fn parse(v: &serde_json::Value) -> Option<PersonaRef> {
        if let Some(id) = v.as_str() {
            return Some(PersonaRef {
                id: sanitize_line(id),
                name: sanitize_line(id),
                owner_id: None,
            });
        }
        let id = v.get("id").and_then(serde_json::Value::as_str)?;
        Some(PersonaRef {
            id: sanitize_line(id),
            name: sanitize_line(
                v.get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(id),
            ),
            owner_id: v
                .get("owner")
                .and_then(|o| o.get("id"))
                .and_then(serde_json::Value::as_i64),
        })
    }
}

pub fn plain_text(markdown: &str) -> String {
    let (capped, _) = tmgr_core::text::truncate_bytes(markdown, 64 * 1024);
    tmgr_core::text::markdown_to_text(&capped)
        .lines()
        .map(sanitize_line)
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ws {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub local: bool,
}

impl Ws {
    pub fn from_workspace(w: &tmgr_core::models::Workspace) -> Ws {
        Ws {
            id: w.id,
            code: sanitize_line(&w.code),
            name: sanitize_line(&w.name),
            local: w.local,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub code: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct Opened {
    pub ws: Ws,
    pub category: Option<u64>,
    pub project: Option<String>,
    pub projects: Vec<Project>,
    pub me: Option<i64>,
    pub snapshot: Snapshot,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub statuses: Vec<Status>,
    pub tasks: Vec<ApiTask>,
    pub more: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommentLine {
    pub author: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Run {
    pub agent: String,
    pub model: String,
    pub status: String,
    pub seconds: i64,
    pub branch: String,
    pub pr: String,
    pub passed: Option<i64>,
    pub failed: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Detail {
    pub description: Option<String>,
    pub comments: Vec<CommentLine>,
    pub runs: Vec<Run>,
    pub runs_available: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Failure {
    pub kind: Kind,
    pub message: String,
}

impl Failure {
    pub fn new(kind: Kind, message: impl AsRef<str>) -> Failure {
        Failure {
            kind,
            message: sanitize_line(message.as_ref()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Refresh,
    LoadDetail(u64),
    Move { id: u64, to: u64 },
    Timer { id: u64, start: bool },
    Comment { id: u64, text: String },
    Open { ws: Ws, project: Option<String> },
    Workspaces,
    Save { ws: String, filters: Filters },
}

pub enum Msg {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16),
    Tick(i64),
    Loaded(Result<Snapshot, Failure>),
    Detail(u64, Box<Result<Detail, Failure>>),
    MoveDone(u64, Result<(), Failure>),
    TimerDone(u64, bool, Result<(), Failure>),
    CommentDone(u64, Result<(), Failure>),
    Opened(Box<Result<Opened, Failure>>),
    Workspaces(Result<Vec<Ws>, Failure>),
    SaveFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Filter,
    Comment,
    Help,
    Picker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chip {
    Workspace,
    Project,
    Assignee,
    Persona,
    Search,
    Menu,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Choice {
    Workspace(Ws),
    Project(Option<String>),
    Assignee(Assignee),
    Persona(PersonaSel),
    Chip(Chip),
    Reset,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PickItem {
    pub label: String,
    pub choice: Choice,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Picker {
    pub chip: Chip,
    pub items: Vec<PickItem>,
    pub sel: usize,
    pub loading: bool,
}

#[derive(Debug, Clone)]
struct Switch {
    ws: Ws,
    filters: Filters,
    save: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub text: String,
    pub error: bool,
}

#[derive(Default)]
pub struct Hits {
    pub cards: Vec<(Rect, usize, usize)>,
    pub tabs: Vec<(Rect, usize)>,
    pub chips: Vec<(Rect, Chip)>,
    pub picker: Vec<(Rect, usize)>,
    pub picker_box: Option<Rect>,
}

#[derive(Default)]
pub struct ViewState {
    pub col_scroll: Vec<usize>,
    pub list_scroll: usize,
    pub picker_scroll: usize,
    pub hits: Hits,
}

pub struct App {
    pub ws: Ws,
    pub projects: Vec<Project>,
    pub filters: Filters,
    pub saved: Saved,
    pub workspaces: Option<Vec<Ws>>,
    pub picker: Option<Picker>,
    switching: Option<Switch>,
    pub me: Option<i64>,
    pub columns: Vec<Column>,
    pub cards: Vec<Card>,
    pub more: bool,
    pub col: usize,
    rows: Vec<usize>,
    pub mode: Mode,
    pub input: String,
    pub details_open: bool,
    pub details: HashMap<u64, Detail>,
    pub notice: Option<Notice>,
    pub offline: bool,
    pub loading: bool,
    pub now: i64,
    pub flat: bool,
    pub quit: bool,
    pub fatal: Option<Failure>,
    last_refresh: i64,
    pending_status: HashMap<u64, u64>,
    origin: HashMap<u64, Option<u64>>,
    pending_timer: HashMap<u64, bool>,
    pub view: RefCell<ViewState>,
}

impl App {
    pub fn new(ws: Ws, filters: Filters, saved: Saved, me: Option<i64>, now: i64) -> App {
        App {
            ws,
            projects: Vec::new(),
            filters,
            saved,
            workspaces: None,
            picker: None,
            switching: None,
            me,
            columns: Vec::new(),
            cards: Vec::new(),
            more: false,
            col: 0,
            rows: Vec::new(),
            mode: Mode::Normal,
            input: String::new(),
            details_open: false,
            details: HashMap::new(),
            notice: None,
            offline: false,
            loading: false,
            now,
            flat: false,
            quit: false,
            fatal: None,
            last_refresh: now,
            pending_status: HashMap::new(),
            origin: HashMap::new(),
            pending_timer: HashMap::new(),
            view: RefCell::new(ViewState::default()),
        }
    }

    pub fn apply_snapshot(&mut self, snap: Snapshot) {
        let keep = self.selected().map(|c| c.id);
        let keep_status = self.columns.get(self.col).map(|c| c.id);
        self.columns = snap
            .statuses
            .iter()
            .filter(|s| !HIDDEN_KINDS.contains(&s.kind.as_str()))
            .map(|s| Column {
                id: s.id,
                name: sanitize_line(&s.name),
            })
            .collect();
        self.cards = snap.tasks.iter().map(Card::from_task).collect();
        for card in &mut self.cards {
            if let Some(to) = self.pending_status.get(&card.id) {
                card.status_id = Some(*to);
            }
            if let Some(running) = self.pending_timer.get(&card.id) {
                card.running = *running;
            }
        }
        self.more = snap.more;
        self.rows.resize(self.columns.len(), 0);
        self.col = keep_status
            .and_then(|id| self.columns.iter().position(|c| c.id == id))
            .unwrap_or(0)
            .min(self.columns.len().saturating_sub(1));
        if let Some(id) = keep {
            self.select_task(id);
        }
        self.clamp();
    }

    fn col_of(&self, card: &Card) -> Option<usize> {
        match card.status_id {
            Some(id) => self.columns.iter().position(|c| c.id == id),
            None if self.columns.is_empty() => None,
            None => Some(0),
        }
    }

    fn matches_filter(&self, card: &Card) -> bool {
        let assignee = match &self.filters.assignee {
            Assignee::All => true,
            Assignee::Me => self
                .me
                .is_some_and(|m| card.people.iter().any(|(id, _)| *id == m)),
            Assignee::User { id, .. } => card.people.iter().any(|(p, _)| p == id),
        };
        let persona = match &self.filters.persona {
            PersonaSel::Any => true,
            PersonaSel::Mine => self
                .me
                .is_some_and(|m| card.personas.iter().any(|p| p.owner_id == Some(m))),
            PersonaSel::One { id, .. } => card.personas.iter().any(|p| &p.id == id),
        };
        if !assignee || !persona {
            return false;
        }
        let needle = self.filters.search.trim().to_lowercase();
        if needle.is_empty() {
            return true;
        }
        let hay =
            format!("{} {} {}", card.key, card.title, card.assignees.join(" ")).to_lowercase();
        hay.contains(&needle)
    }

    pub fn visible(&self, col: usize) -> Vec<&Card> {
        self.cards
            .iter()
            .filter(|c| self.col_of(c) == Some(col) && self.matches_filter(c))
            .collect()
    }

    pub fn visible_count(&self) -> usize {
        (0..self.columns.len()).map(|c| self.visible(c).len()).sum()
    }

    pub fn row(&self, col: usize) -> usize {
        let len = self.visible(col).len();
        self.rows
            .get(col)
            .copied()
            .unwrap_or(0)
            .min(len.saturating_sub(1))
    }

    pub fn selected(&self) -> Option<&Card> {
        self.visible(self.col).get(self.row(self.col)).copied()
    }

    pub fn selected_id(&self) -> Option<u64> {
        self.selected().map(|c| c.id)
    }

    fn clamp(&mut self) {
        for col in 0..self.columns.len() {
            let r = self.row(col);
            self.rows[col] = r;
        }
    }

    pub fn select_task(&mut self, id: u64) {
        for col in 0..self.columns.len() {
            if let Some(row) = self.visible(col).iter().position(|c| c.id == id) {
                self.col = col;
                self.rows[col] = row;
                return;
            }
        }
    }

    fn select_at(&mut self, col: usize, row: usize) {
        if col < self.columns.len() {
            self.col = col;
            self.rows[col] = row;
            self.clamp();
        }
    }

    fn move_col(&mut self, delta: isize) {
        let next = self.col as isize + delta;
        if next >= 0 && (next as usize) < self.columns.len() {
            self.col = next as usize;
        }
    }

    fn move_row(&mut self, delta: isize) {
        let len = self.visible(self.col).len();
        let cur = self.row(self.col);
        if delta > 0 && cur + 1 >= len {
            if self.flat {
                if let Some(next) =
                    (self.col + 1..self.columns.len()).find(|c| !self.visible(*c).is_empty())
                {
                    self.col = next;
                    self.rows[next] = 0;
                }
            }
            return;
        }
        if delta < 0 && cur == 0 {
            if self.flat {
                if let Some(prev) = (0..self.col).rev().find(|c| !self.visible(*c).is_empty()) {
                    self.col = prev;
                    self.rows[prev] = self.visible(prev).len() - 1;
                }
            }
            return;
        }
        self.rows[self.col] = (cur as isize + delta) as usize;
    }

    fn detail_effect(&self) -> Vec<Effect> {
        match (self.details_open, self.selected_id()) {
            (true, Some(id)) if !self.details.contains_key(&id) => vec![Effect::LoadDetail(id)],
            _ => Vec::new(),
        }
    }

    fn refresh_effects(&mut self) -> Vec<Effect> {
        self.loading = true;
        self.last_refresh = self.now;
        let mut out = vec![Effect::Refresh];
        if let (true, Some(id)) = (self.details_open, self.selected_id()) {
            out.push(Effect::LoadDetail(id));
        }
        out
    }

    fn notify(&mut self, text: impl Into<String>, error: bool) {
        self.notice = Some(Notice {
            text: sanitize_line(&text.into()),
            error,
        });
    }

    fn fail(&mut self, f: &Failure, what: &str) {
        if f.kind == Kind::Auth {
            self.fatal = Some(f.clone());
            self.quit = true;
            return;
        }
        if f.kind == Kind::Offline {
            self.offline = true;
        }
        self.notify(format!("{what}: {}", f.message), true);
    }

    pub fn update(&mut self, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Key(key) => self.on_key(key),
            Msg::Opened(result) => self.on_opened(*result),
            Msg::Workspaces(result) => self.on_workspaces(result),
            Msg::SaveFailed => {
                self.notify("could not remember the filters", true);
                Vec::new()
            }
            Msg::Loaded(_)
            | Msg::Detail(..)
            | Msg::MoveDone(..)
            | Msg::TimerDone(..)
            | Msg::CommentDone(..)
                if self.switching.is_some() =>
            {
                Vec::new()
            }
            Msg::Mouse(m) => self.on_mouse(m),
            Msg::Resize(width) => {
                self.flat = width < 60;
                Vec::new()
            }
            Msg::Tick(now) => {
                self.now = now;
                if !self.loading && self.switching.is_none() && now - self.last_refresh >= POLL_SECS
                {
                    return self.refresh_effects();
                }
                Vec::new()
            }
            Msg::Loaded(result) => {
                self.loading = false;
                self.last_refresh = self.now;
                match result {
                    Ok(snap) => {
                        self.offline = false;
                        self.apply_snapshot(snap);
                        self.detail_effect()
                    }
                    Err(f) => {
                        self.fail(&f, "refresh failed");
                        Vec::new()
                    }
                }
            }
            Msg::Detail(id, result) => {
                match *result {
                    Ok(d) => {
                        self.details.insert(id, d);
                    }
                    Err(f) => {
                        self.details.insert(
                            id,
                            Detail {
                                error: Some(f.message.clone()),
                                ..Detail::default()
                            },
                        );
                        if f.kind == Kind::Auth {
                            self.fail(&f, "details");
                        }
                    }
                }
                Vec::new()
            }
            Msg::MoveDone(id, result) => {
                let to = self.pending_status.remove(&id);
                match result {
                    Ok(()) => self.refresh_effects(),
                    Err(f) => {
                        self.rollback_move(id, to);
                        self.fail(&f, "move failed");
                        Vec::new()
                    }
                }
            }
            Msg::TimerDone(id, start, result) => {
                self.pending_timer.remove(&id);
                match result {
                    Ok(()) => self.refresh_effects(),
                    Err(f) => {
                        if let Some(c) = self.cards.iter_mut().find(|c| c.id == id) {
                            c.running = !start;
                            c.start_time = 0;
                        }
                        self.fail(&f, "timer failed");
                        Vec::new()
                    }
                }
            }
            Msg::CommentDone(id, result) => match result {
                Ok(()) => {
                    self.details.remove(&id);
                    self.notify("comment added", false);
                    match (self.details_open, self.selected_id()) {
                        (true, Some(sel)) if sel == id => vec![Effect::LoadDetail(id)],
                        _ => Vec::new(),
                    }
                }
                Err(f) => {
                    self.fail(&f, "comment failed");
                    Vec::new()
                }
            },
        }
    }

    fn open_chip(&mut self, chip: Chip) -> Vec<Effect> {
        if chip == Chip::Search {
            self.input = self.filters.search.clone();
            self.mode = Mode::Filter;
            return Vec::new();
        }
        self.open_picker(chip)
    }

    fn save_effect(&mut self) -> Effect {
        let key = filters::key(&self.ws.code);
        let mut f = self.filters.clone();
        f.search.clear();
        if f == Filters::default() {
            self.saved.workspaces.remove(&key);
        } else {
            self.saved.workspaces.insert(key, f);
        }
        Effect::Save {
            ws: self.ws.code.clone(),
            filters: self.filters.clone(),
        }
    }

    fn people_options(&self) -> Vec<(i64, String)> {
        let mut out: Vec<(i64, String)> = Vec::new();
        if let Assignee::User { id, name } = &self.filters.assignee {
            out.push((*id, name.clone()));
        }
        for c in &self.cards {
            for p in &c.people {
                if Some(p.0) != self.me && !out.iter().any(|o| o.0 == p.0) {
                    out.push(p.clone());
                }
            }
        }
        out.sort_by_key(|p| p.1.to_lowercase());
        out
    }

    fn persona_options(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        if let PersonaSel::One { id, name } = &self.filters.persona {
            out.push((id.clone(), name.clone()));
        }
        for c in &self.cards {
            for p in &c.personas {
                if !out.iter().any(|o| o.0 == p.id) {
                    out.push((p.id.clone(), p.name.clone()));
                }
            }
        }
        out.sort_by_key(|p| p.1.to_lowercase());
        out
    }

    pub fn assignee_label(&self) -> String {
        match &self.filters.assignee {
            Assignee::All => "all".into(),
            Assignee::Me => "me".into(),
            Assignee::User { name, .. } => name.clone(),
        }
    }

    pub fn persona_label(&self) -> String {
        match &self.filters.persona {
            PersonaSel::Any => "-".into(),
            PersonaSel::Mine => "mine".into(),
            PersonaSel::One { name, .. } => name.clone(),
        }
    }

    pub fn project_label(&self) -> String {
        self.filters.project.clone().unwrap_or_else(|| "all".into())
    }

    fn item(label: impl AsRef<str>, choice: Choice, current: bool) -> PickItem {
        PickItem {
            label: sanitize_line(label.as_ref()),
            choice,
            current,
        }
    }

    fn picker_items(&self, chip: Chip) -> Vec<PickItem> {
        match chip {
            Chip::Workspace => self
                .workspaces
                .iter()
                .flatten()
                .map(|w| {
                    let label = if w.local {
                        format!("{} (local)", w.name)
                    } else {
                        w.name.clone()
                    };
                    Self::item(label, Choice::Workspace(w.clone()), w.code == self.ws.code)
                })
                .collect(),
            Chip::Project => {
                let mut items = vec![Self::item(
                    "All projects",
                    Choice::Project(None),
                    self.filters.project.is_none(),
                )];
                items.extend(self.projects.iter().map(|p| {
                    Self::item(
                        format!("{} {}", p.code, p.title),
                        Choice::Project(Some(p.code.clone())),
                        self.filters.project.as_deref() == Some(p.code.as_str()),
                    )
                }));
                items
            }
            Chip::Assignee => {
                let a = &self.filters.assignee;
                let mut items = vec![
                    Self::item(
                        "Everyone",
                        Choice::Assignee(Assignee::All),
                        *a == Assignee::All,
                    ),
                    Self::item("Me", Choice::Assignee(Assignee::Me), *a == Assignee::Me),
                ];
                items.extend(self.people_options().into_iter().map(|(id, name)| {
                    let choice = Assignee::User {
                        id,
                        name: name.clone(),
                    };
                    let current = *a == choice;
                    Self::item(name, Choice::Assignee(choice), current)
                }));
                items
            }
            Chip::Persona => {
                let p = &self.filters.persona;
                let mut items = vec![
                    Self::item(
                        "Any",
                        Choice::Persona(PersonaSel::Any),
                        *p == PersonaSel::Any,
                    ),
                    Self::item(
                        "My personas",
                        Choice::Persona(PersonaSel::Mine),
                        *p == PersonaSel::Mine,
                    ),
                ];
                items.extend(self.persona_options().into_iter().map(|(id, name)| {
                    let choice = PersonaSel::One {
                        id,
                        name: name.clone(),
                    };
                    let current = *p == choice;
                    Self::item(name, Choice::Persona(choice), current)
                }));
                items
            }
            Chip::Search | Chip::Menu => vec![
                Self::item(
                    format!("Workspace: {}", self.ws.name),
                    Choice::Chip(Chip::Workspace),
                    false,
                ),
                Self::item(
                    format!("Project: {}", self.project_label()),
                    Choice::Chip(Chip::Project),
                    false,
                ),
                Self::item(
                    format!("Assignee: {}", self.assignee_label()),
                    Choice::Chip(Chip::Assignee),
                    false,
                ),
                Self::item(
                    format!("Persona: {}", self.persona_label()),
                    Choice::Chip(Chip::Persona),
                    false,
                ),
                Self::item("Search", Choice::Chip(Chip::Search), false),
                Self::item("Reset all filters", Choice::Reset, false),
            ],
        }
    }

    fn open_picker(&mut self, chip: Chip) -> Vec<Effect> {
        let items = self.picker_items(chip);
        let sel = items.iter().position(|i| i.current).unwrap_or(0);
        let wait = chip == Chip::Workspace;
        self.picker = Some(Picker {
            chip,
            items,
            sel,
            loading: wait,
        });
        self.view.borrow_mut().picker_scroll = 0;
        self.mode = Mode::Picker;
        if wait {
            vec![Effect::Workspaces]
        } else {
            Vec::new()
        }
    }

    fn close_picker(&mut self) {
        self.picker = None;
        self.mode = Mode::Normal;
    }

    fn on_workspaces(&mut self, result: Result<Vec<Ws>, Failure>) -> Vec<Effect> {
        match result {
            Ok(list) => {
                self.workspaces = Some(list);
                if let Some(p) = &self.picker {
                    if p.chip == Chip::Workspace {
                        let items = self.picker_items(Chip::Workspace);
                        let sel = items.iter().position(|i| i.current).unwrap_or(0);
                        self.picker = Some(Picker {
                            chip: Chip::Workspace,
                            items,
                            sel,
                            loading: false,
                        });
                    }
                }
            }
            Err(f) => {
                if f.kind == Kind::Auth && self.workspaces.is_none() {
                    self.workspaces = Some(vec![self.ws.clone()]);
                }
                if matches!(&self.picker, Some(p) if p.chip == Chip::Workspace) {
                    self.close_picker();
                }
                self.notify(format!("workspaces: {}", f.message), true);
            }
        }
        Vec::new()
    }

    fn on_picker_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        let Some(picker) = self.picker.as_mut() else {
            self.mode = Mode::Normal;
            return Vec::new();
        };
        let len = picker.items.len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.close_picker(),
            KeyCode::Char('j') | KeyCode::Down => {
                picker.sel = (picker.sel + 1).min(len.saturating_sub(1));
            }
            KeyCode::Char('k') | KeyCode::Up => picker.sel = picker.sel.saturating_sub(1),
            KeyCode::Enter => {
                if let Some(item) = picker.items.get(picker.sel).cloned() {
                    self.close_picker();
                    return self.choose(item.choice);
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn on_picker_mouse(&mut self, m: MouseEvent) -> Vec<Effect> {
        match m.kind {
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                if let Some(p) = self.picker.as_mut() {
                    let last = p.items.len().saturating_sub(1);
                    p.sel = if m.kind == MouseEventKind::ScrollDown {
                        (p.sel + 1).min(last)
                    } else {
                        p.sel.saturating_sub(1)
                    };
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let at = (m.column, m.row);
                let (item, inside) = {
                    let view = self.view.borrow();
                    (
                        view.hits
                            .picker
                            .iter()
                            .find(|(r, _)| contains(*r, at))
                            .map(|(_, i)| *i),
                        view.hits.picker_box.is_some_and(|r| contains(r, at)),
                    )
                };
                match item {
                    Some(i) => {
                        let choice = self
                            .picker
                            .as_ref()
                            .and_then(|p| p.items.get(i))
                            .map(|it| it.choice.clone());
                        self.close_picker();
                        if let Some(c) = choice {
                            return self.choose(c);
                        }
                    }
                    None if !inside => self.close_picker(),
                    None => {}
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn begin_switch(&mut self, ws: Ws, filters: Filters, save: bool) -> Vec<Effect> {
        let project = filters.project.clone();
        self.switching = Some(Switch {
            ws: ws.clone(),
            filters,
            save,
        });
        self.loading = true;
        vec![Effect::Open { ws, project }]
    }

    fn choose(&mut self, choice: Choice) -> Vec<Effect> {
        match choice {
            Choice::Chip(chip) => self.open_chip(chip),
            Choice::Workspace(ws) => {
                if ws.code == self.ws.code {
                    return Vec::new();
                }
                let filters = self
                    .saved
                    .workspaces
                    .get(&filters::key(&ws.code))
                    .cloned()
                    .unwrap_or_default();
                self.begin_switch(ws, filters, false)
            }
            Choice::Project(project) => {
                if project == self.filters.project {
                    return Vec::new();
                }
                let filters = Filters {
                    project,
                    ..self.filters.clone()
                };
                self.begin_switch(self.ws.clone(), filters, true)
            }
            Choice::Assignee(a) => {
                self.filters.assignee = a;
                self.after_filter_change()
            }
            Choice::Persona(p) => {
                self.filters.persona = p;
                self.after_filter_change()
            }
            Choice::Reset => {
                if self.filters == Filters::default() {
                    return Vec::new();
                }
                if self.filters.project.is_some() {
                    return self.begin_switch(self.ws.clone(), Filters::default(), true);
                }
                self.filters = Filters::default();
                self.after_filter_change()
            }
        }
    }

    fn after_filter_change(&mut self) -> Vec<Effect> {
        let mut out = vec![self.save_effect()];
        if self.visible(self.col).is_empty() {
            if let Some(c) = (0..self.columns.len()).find(|c| !self.visible(*c).is_empty()) {
                self.col = c;
            }
        }
        self.clamp();
        out.extend(self.detail_effect());
        out
    }

    fn on_opened(&mut self, result: Result<Opened, Failure>) -> Vec<Effect> {
        let Some(switch) = self.switching.take() else {
            return Vec::new();
        };
        self.loading = false;
        self.last_refresh = self.now;
        let opened = match result {
            Ok(o) => o,
            Err(f) => {
                if f.kind == Kind::Offline {
                    self.offline = true;
                }
                self.pending_status.clear();
                self.pending_timer.clear();
                self.origin.clear();
                self.notify(
                    format!("could not open {}: {}", switch.ws.name, f.message),
                    true,
                );
                return self.refresh_effects();
            }
        };
        self.offline = false;
        self.ws = opened.ws;
        self.filters = Filters {
            project: opened.project,
            ..switch.filters
        };
        self.projects = opened.projects;
        self.me = opened.me;
        self.details.clear();
        self.pending_status.clear();
        self.pending_timer.clear();
        self.origin.clear();
        self.rows.clear();
        self.col = 0;
        self.columns.clear();
        self.cards.clear();
        {
            let mut v = self.view.borrow_mut();
            v.col_scroll.clear();
            v.list_scroll = 0;
        }
        self.apply_snapshot(opened.snapshot);
        let mut out = Vec::new();
        if switch.save {
            out.push(self.save_effect());
        }
        out.extend(self.detail_effect());
        out
    }

    fn rollback_move(&mut self, id: u64, to: Option<u64>) {
        let origin = self.origin.remove(&id);
        let Some(to) = to else { return };
        if let Some(c) = self.cards.iter_mut().find(|c| c.id == id) {
            if c.status_id == Some(to) {
                c.status_id = origin.flatten();
            }
        }
        self.select_task(id);
    }

    fn on_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if key.kind != ratatui::crossterm::event::KeyEventKind::Press {
            return Vec::new();
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return Vec::new();
        }
        match self.mode {
            Mode::Help => {
                self.mode = Mode::Normal;
                Vec::new()
            }
            Mode::Picker => self.on_picker_key(key),
            Mode::Filter => self.on_filter_key(key),
            Mode::Comment => self.on_comment_key(key),
            Mode::Normal => self.on_normal_key(key),
        }
    }

    fn edit_input(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.clear();
                true
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL) && !c.is_control() =>
            {
                if self.input.chars().count() < MAX_INPUT_CHARS {
                    self.input.push(c);
                }
                true
            }
            KeyCode::Backspace => {
                self.input.pop();
                true
            }
            _ => false,
        }
    }

    fn on_filter_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Esc => {
                self.filters.search.clear();
                self.input.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                self.mode = Mode::Normal;
            }
            _ => {
                if self.edit_input(key) {
                    self.filters.search = self.input.clone();
                }
            }
        }
        if self.visible(self.col).is_empty() {
            if let Some(c) = (0..self.columns.len()).find(|c| !self.visible(*c).is_empty()) {
                self.col = c;
            }
        }
        self.clamp();
        self.detail_effect()
    }

    fn on_comment_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Esc => {
                self.input.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                let text = self.input.trim().to_string();
                self.input.clear();
                self.mode = Mode::Normal;
                if let (false, Some(id)) = (text.is_empty(), self.selected_id()) {
                    self.notify("sending comment...", false);
                    return vec![Effect::Comment { id, text }];
                }
            }
            _ => {
                self.edit_input(key);
            }
        }
        Vec::new()
    }

    fn on_normal_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        self.notice = None;
        if self.switching.is_some() && !matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
            return Vec::new();
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('h') | KeyCode::Left => self.move_col(-1),
            KeyCode::Char('l') | KeyCode::Right => self.move_col(1),
            KeyCode::Char('j') | KeyCode::Down => self.move_row(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_row(-1),
            KeyCode::Char('H') => return self.move_card(-1),
            KeyCode::Char('L') => return self.move_card(1),
            KeyCode::Enter => {
                self.details_open = !self.details_open;
            }
            KeyCode::Char('s') => return self.timer(true),
            KeyCode::Char('S') => return self.timer(false),
            KeyCode::Char('c') => {
                if self.selected().is_some() {
                    self.input.clear();
                    self.mode = Mode::Comment;
                }
            }
            KeyCode::Char('/') => {
                self.input = self.filters.search.clone();
                self.mode = Mode::Filter;
            }
            KeyCode::Char('g') => return self.refresh_effects(),
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Char('w') => return self.open_picker(Chip::Workspace),
            KeyCode::Char('p') => return self.open_picker(Chip::Project),
            KeyCode::Char('a') => return self.open_picker(Chip::Assignee),
            KeyCode::Char('P') => return self.open_picker(Chip::Persona),
            KeyCode::Char('f') => return self.open_picker(Chip::Menu),
            KeyCode::Char('x') => return self.choose(Choice::Reset),
            _ => {}
        }
        self.detail_effect()
    }

    fn move_card(&mut self, delta: isize) -> Vec<Effect> {
        let target = self.col as isize + delta;
        let Some(card) = self.selected().cloned() else {
            return Vec::new();
        };
        if target < 0 || target as usize >= self.columns.len() {
            return Vec::new();
        }
        let to = self.columns[target as usize].id;
        if card.status_id == Some(to) {
            return Vec::new();
        }
        self.origin.insert(card.id, card.status_id);
        self.pending_status.insert(card.id, to);
        if let Some(c) = self.cards.iter_mut().find(|c| c.id == card.id) {
            c.status_id = Some(to);
        }
        self.select_task(card.id);
        let mut out = vec![Effect::Move { id: card.id, to }];
        out.extend(self.detail_effect());
        out
    }

    fn timer(&mut self, start: bool) -> Vec<Effect> {
        let Some(card) = self.selected().cloned() else {
            return Vec::new();
        };
        if card.running == start {
            self.notify(
                if start {
                    "timer is already running"
                } else {
                    "timer is not running"
                },
                false,
            );
            return Vec::new();
        }
        self.pending_timer.insert(card.id, start);
        let now = self.now;
        if let Some(c) = self.cards.iter_mut().find(|c| c.id == card.id) {
            c.running = start;
            c.start_time = if start { now } else { 0 };
        }
        vec![Effect::Timer { id: card.id, start }]
    }

    fn on_mouse(&mut self, m: MouseEvent) -> Vec<Effect> {
        if self.mode == Mode::Picker {
            return self.on_picker_mouse(m);
        }
        if self.mode == Mode::Help {
            if matches!(m.kind, MouseEventKind::Down(_)) {
                self.mode = Mode::Normal;
            }
            return Vec::new();
        }
        if self.mode != Mode::Normal {
            return Vec::new();
        }
        match m.kind {
            MouseEventKind::ScrollDown => self.move_row(1),
            MouseEventKind::ScrollUp => self.move_row(-1),
            MouseEventKind::Down(MouseButton::Left) => {
                let at = (m.column, m.row);
                let chip = self
                    .view
                    .borrow()
                    .hits
                    .chips
                    .iter()
                    .find(|(r, _)| contains(*r, at))
                    .map(|(_, c)| *c);
                if let Some(chip) = chip {
                    if self.switching.is_some() {
                        return Vec::new();
                    }
                    return self.open_chip(chip);
                }
                let hit = {
                    let view = self.view.borrow();
                    let card = view
                        .hits
                        .cards
                        .iter()
                        .find(|(r, _, _)| contains(*r, at))
                        .map(|(_, c, r)| (*c, Some(*r)));
                    card.or_else(|| {
                        view.hits
                            .tabs
                            .iter()
                            .find(|(r, _)| contains(*r, at))
                            .map(|(_, c)| (*c, None))
                    })
                };
                match hit {
                    Some((col, Some(row))) => self.select_at(col, row),
                    Some((col, None)) => self.move_col(col as isize - self.col as isize),
                    None => {}
                }
            }
            _ => {}
        }
        self.detail_effect()
    }
}

fn contains(r: Rect, (x, y): (u16, u16)) -> bool {
    x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}
