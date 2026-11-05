use super::mouse::MouseEvent;
use super::palette::{esc_key, next_char, query_tail, Key};
use super::render::{dim, put};
use super::theme::colors;
use super::view::{contains, MouseOut, ViewState};
use crate::sanitize::sanitize_text;
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, BorderType, Clear, Widget};
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Place {
    #[default]
    Right,
    Below,
    Replace,
    Tab,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpawnKind {
    Agent { profile: String },
    Shell,
    Custom { line: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindOpt {
    pub id: String,
    pub label: String,
    pub spawn: SpawnKind,
    pub bypass: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskOpt {
    pub key: String,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewRequest {
    pub kind: SpawnKind,
    pub folder: String,
    pub name: Option<String>,
    pub place: Place,
    pub task: Option<String>,
    pub worktree: bool,
    pub checkpoints: Option<bool>,
    pub permission_cards: bool,
    pub bypass: bool,
    pub no_sandbox: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivAsk {
    pub word: &'static str,
    pub prompt: String,
    pub warnings: Vec<&'static str>,
}

pub type PrivGate = fn(bool, bool) -> Result<PrivAsk, (String, Option<String>)>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Remembered {
    pub agent: String,
    pub bypass: bool,
    pub no_sandbox: bool,
    pub worktree: Option<bool>,
    pub permission_cards: Option<bool>,
    pub checkpoints: Option<bool>,
}

#[derive(Clone, Debug)]
struct PrivPrompt {
    req: NewRequest,
    ask: PrivAsk,
    typed: String,
    cleared: bool,
    seen: Cell<Option<Instant>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewEdit {
    Editing,
    Submit(NewRequest),
    Approve,
    Cancel,
}

const MAX_FOLDER: usize = 512;
const MAX_COMMAND: usize = 512;
const MAX_NAME: usize = 64;
const MAX_TASK: usize = 128;
const MAX_TASK_OPTS: usize = 200;
const MAX_ENTRIES: usize = 500;
const MAX_WIDTH: u16 = 72;
const LABEL_W: u16 = 12;
const MAX_TYPED: usize = 32;
const PROMPT_DELAY: Duration = Duration::from_millis(300);
const START_LABEL: &str = "[ Start ]";

impl Place {
    const ALL: [Place; 4] = [Place::Tab, Place::Right, Place::Below, Place::Replace];

    fn label(self) -> &'static str {
        match self {
            Place::Right => "Right",
            Place::Below => "Below",
            Place::Replace => "Replace",
            Place::Tab => "New tab",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewField {
    Kind,
    Task,
    Command,
    Folder,
    Name,
    Worktree,
    PermissionCards,
    Bypass,
    NoSandbox,
    Where,
    Checkpoints,
    Start,
}

#[derive(Clone, Debug)]
pub struct NewSession {
    pub kinds: Vec<KindOpt>,
    pub kind: usize,
    pub folder: String,
    pub command: String,
    pub name: String,
    pub place: Place,
    pub worktree: bool,
    pub permission_cards: bool,
    pub bypass: bool,
    pub no_sandbox: bool,
    remembered: bool,
    pub priv_gate: Option<PrivGate>,
    pub priv_lock: Option<String>,
    priv_prompt: Option<PrivPrompt>,
    pub error: Option<String>,
    pub error_hint: Option<String>,
    pub focus: NewField,
    pub taken_names: Vec<String>,
    pub task: String,
    pub tasks: Vec<TaskOpt>,
    pub task_at: usize,
    pub task_note: Option<String>,
    pub checkpoints: bool,
    pub review: Option<Vec<String>>,
    pub home: PathBuf,
    pub list_dir: fn(&Path) -> Vec<String>,
}

fn strip_control(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

fn list_dirs(path: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    rd.take(MAX_ENTRIES)
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| strip_control(&e.file_name().to_string_lossy()))
        .collect()
}

fn looks_like_ref(s: &str) -> bool {
    let digits =
        |t: &str, max: usize| (1..=max).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_digit());
    if let Some(rest) = s.strip_prefix('#') {
        return digits(rest, 9);
    }
    match s.split_once('-') {
        Some((code, seq)) => {
            code.len() <= 10
                && code.bytes().next().is_some_and(|b| b.is_ascii_uppercase())
                && code
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                && digits(seq, 7)
        }
        None => digits(s, 9),
    }
}

fn expand_home(s: &str, home: &Path) -> PathBuf {
    if s == "~" {
        home.to_path_buf()
    } else if let Some(rest) = s.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(s)
    }
}

fn is_reserved_shell_name(name: &str) -> bool {
    name.strip_prefix("sh-")
        .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
}

fn common_prefix(names: &[&String]) -> String {
    let Some((first, rest)) = names.split_first() else {
        return String::new();
    };
    let mut prefix: Vec<char> = first.chars().collect();
    for n in rest {
        let keep = prefix
            .iter()
            .zip(n.chars())
            .take_while(|(a, b)| **a == *b)
            .count();
        prefix.truncate(keep);
    }
    prefix.into_iter().collect()
}

impl NewSession {
    pub fn with_error(mut self, msg: impl Into<String>, hint: Option<String>) -> Self {
        self.error = Some(msg.into());
        self.error_hint = hint;
        self
    }

    fn is_custom(&self) -> bool {
        self.kinds
            .get(self.kind)
            .is_some_and(|k| matches!(k.spawn, SpawnKind::Custom { .. }))
    }

    fn is_agent(&self) -> bool {
        self.kinds
            .get(self.kind)
            .is_some_and(|k| matches!(k.spawn, SpawnKind::Agent { .. }))
    }

    fn kind_bypass(&self) -> bool {
        self.kinds.get(self.kind).is_some_and(|k| k.bypass)
    }

    fn lock_of(&self, f: NewField) -> Option<String> {
        if let Some(reason) = &self.priv_lock {
            return Some(reason.clone());
        }
        if self.priv_gate.is_none() {
            return Some("unavailable".into());
        }
        (f == NewField::Bypass && !self.kind_bypass()).then(|| "no bypass mode".into())
    }

    fn remembered_line(&self) -> Option<String> {
        if !self.remembered {
            return None;
        }
        let what = match (self.bypass, self.no_sandbox) {
            (true, true) => "Bypass and no sandbox are",
            (true, false) => "Bypass is",
            (false, true) => "No sandbox is",
            (false, false) => return None,
        };
        Some(format!("{what} on, remembered from your last session"))
    }

    pub fn clear_privileges(&mut self) {
        self.bypass = false;
        self.no_sandbox = false;
        self.remembered = false;
        self.priv_prompt = None;
    }

    pub fn priv_prompt_open(&self) -> bool {
        self.priv_prompt.is_some()
    }

    pub fn task_matches(&self) -> Vec<&TaskOpt> {
        let query = self.task.trim();
        if query.is_empty() {
            return Vec::new();
        }
        let mut hits = tmgr_fuzzy::rank(query, self.tasks.iter().collect(), |t| {
            format!("{} {}", t.key, t.title)
        });
        if let Some(at) = hits.iter().position(|t| t.key.eq_ignore_ascii_case(query)) {
            let exact = hits.remove(at);
            hits.insert(0, exact);
        }
        hits
    }

    fn chosen_task(&self) -> Result<Option<String>, String> {
        let query = self.task.trim();
        if query.is_empty() || !self.is_agent() {
            return Ok(None);
        }
        let hits = self.task_matches();
        let exact = hits
            .first()
            .is_some_and(|t| t.key.eq_ignore_ascii_case(query));
        if looks_like_ref(query) && !exact {
            return Ok(Some(query.to_string()));
        }
        if let Some(t) = hits.get(self.task_at.min(hits.len().saturating_sub(1))) {
            return Ok(Some(t.key.clone()));
        }
        if looks_like_ref(query) {
            return Ok(Some(query.to_string()));
        }
        Err("no such task, type a key like TM-12 or #12".into())
    }

    fn fields(&self) -> Vec<NewField> {
        let mut f = vec![NewField::Kind];
        if self.is_custom() {
            f.push(NewField::Command);
        }
        f.push(NewField::Folder);
        if self.is_agent() {
            f.push(NewField::Task);
        }
        f.push(NewField::Name);
        if self.is_agent() {
            f.push(NewField::Worktree);
            f.push(NewField::PermissionCards);
            f.push(NewField::Bypass);
            f.push(NewField::NoSandbox);
        }
        f.extend([NewField::Where, NewField::Checkpoints, NewField::Start]);
        f
    }

    fn clear_error(&mut self) {
        self.task_at = 0;
        self.error = None;
        self.error_hint = None;
    }

    fn text_field(&mut self) -> Option<(&mut String, usize)> {
        match self.focus {
            NewField::Task => Some((&mut self.task, MAX_TASK)),
            NewField::Command => Some((&mut self.command, MAX_COMMAND)),
            NewField::Folder => Some((&mut self.folder, MAX_FOLDER)),
            NewField::Name => Some((&mut self.name, MAX_NAME)),
            _ => None,
        }
    }

    fn push_char(&mut self, c: char) {
        if let Some((s, cap)) = self.text_field() {
            if s.chars().count() < cap {
                s.push(c);
                self.clear_error();
            }
        }
    }

    fn step(&mut self, down: bool) {
        let fields = self.fields();
        let at = fields.iter().position(|f| *f == self.focus).unwrap_or(0);
        let n = fields.len();
        let next = if down { (at + 1) % n } else { (at + n - 1) % n };
        self.focus = fields[next];
    }

    fn cycle(&mut self, forward: bool) {
        match self.focus {
            NewField::Kind => {
                let n = self.kinds.len();
                if n > 0 {
                    self.kind = if forward {
                        (self.kind + 1) % n
                    } else {
                        (self.kind + n - 1) % n
                    };
                    self.bypass = false;
                    self.no_sandbox = false;
                    self.remembered = false;
                    self.clear_error();
                }
            }
            NewField::Task => {
                let n = self.task_matches().len();
                if n > 0 {
                    let at = self.task_at.min(n - 1);
                    self.clear_error();
                    self.task_at = if forward {
                        (at + 1) % n
                    } else {
                        (at + n - 1) % n
                    };
                }
            }
            NewField::Worktree => {
                self.worktree = !self.worktree;
                self.clear_error();
            }
            NewField::Checkpoints => {
                self.checkpoints = !self.checkpoints;
                self.clear_error();
            }
            NewField::PermissionCards => {
                self.permission_cards = !self.permission_cards;
                self.clear_error();
            }
            NewField::Bypass if self.lock_of(NewField::Bypass).is_none() => {
                self.bypass = !self.bypass;
                self.remembered = false;
                self.clear_error();
            }
            NewField::NoSandbox if self.lock_of(NewField::NoSandbox).is_none() => {
                self.no_sandbox = !self.no_sandbox;
                self.remembered = false;
                self.clear_error();
            }
            NewField::Where => {
                let at = Place::ALL
                    .iter()
                    .position(|p| *p == self.place)
                    .unwrap_or(0);
                let n = Place::ALL.len();
                let next = if forward {
                    (at + 1) % n
                } else {
                    (at + n - 1) % n
                };
                self.place = Place::ALL[next];
                self.clear_error();
            }
            _ => {}
        }
    }

    fn complete_folder(&mut self) -> bool {
        let Some(slash) = self.folder.rfind('/') else {
            if self.folder == "~" {
                self.folder.push('/');
                return true;
            }
            return false;
        };
        let (dir_part, prefix) = self.folder.split_at(slash + 1);
        let dir = expand_home(dir_part, &self.home);
        if !dir.is_absolute() {
            return false;
        }
        let names: Vec<String> = (self.list_dir)(&dir)
            .iter()
            .map(|n| strip_control(n))
            .filter(|n| !n.is_empty() && n.starts_with(prefix))
            .filter(|n| prefix.starts_with('.') || !n.starts_with('.'))
            .collect();
        let refs: Vec<&String> = names.iter().collect();
        let done = match names.len() {
            0 => return false,
            1 => format!("{dir_part}{}/", names[0]),
            _ => format!("{dir_part}{}", common_prefix(&refs)),
        };
        if done.chars().count() <= MAX_FOLDER && done.len() > self.folder.len() {
            self.folder = done;
            self.clear_error();
            return true;
        }
        false
    }

    fn submit(&mut self) -> Result<NewRequest, ()> {
        match self.validate() {
            Ok(req) => Ok(req),
            Err((field, msg)) => {
                self.focus = field;
                self.error = Some(msg.to_string());
                self.error_hint = None;
                Err(())
            }
        }
    }

    fn try_submit(&mut self) -> Option<NewRequest> {
        let req = self.submit().ok()?;
        if !req.bypass && !req.no_sandbox {
            return Some(req);
        }
        let asked = match self.priv_gate {
            Some(gate) => gate(req.bypass, req.no_sandbox),
            None => Err(("bypass and no sandbox are unavailable here".into(), None)),
        };
        match asked {
            Ok(ask) => {
                self.error = None;
                self.error_hint = None;
                self.priv_prompt = Some(PrivPrompt {
                    req,
                    ask,
                    typed: String::new(),
                    cleared: false,
                    seen: Cell::new(None),
                });
            }
            Err((msg, hint)) => {
                self.error = Some(msg);
                self.error_hint = hint;
            }
        }
        None
    }

    fn validate(&self) -> Result<NewRequest, (NewField, String)> {
        let folder = self.folder.trim();
        if folder.is_empty() {
            return Err((NewField::Folder, "folder is required".into()));
        }
        let path = expand_home(folder, &self.home);
        if !path.is_absolute() {
            return Err((
                NewField::Folder,
                "folder must be an absolute path or start with ~".into(),
            ));
        }
        if !path.is_dir() {
            return Err((NewField::Folder, "no such folder".into()));
        }
        let mut folder = path.to_string_lossy().into_owned();
        while folder.len() > 1 && folder.ends_with('/') {
            folder.pop();
        }
        let kind = self
            .kinds
            .get(self.kind)
            .ok_or((NewField::Kind, "no session kind available".to_string()))?;
        let name = self.name.trim();
        if name.chars().count() > MAX_NAME {
            return Err((
                NewField::Name,
                format!("name is longer than {MAX_NAME} characters"),
            ));
        }
        if name.chars().any(char::is_control) {
            return Err((NewField::Name, "name has control characters".into()));
        }
        if !name.is_empty() {
            if self.taken_names.iter().any(|n| n == name) {
                return Err((
                    NewField::Name,
                    format!("a session is already named \"{name}\""),
                ));
            }
            if is_reserved_shell_name(name) && !matches!(kind.spawn, SpawnKind::Shell) {
                return Err((NewField::Name, "sh-N names are reserved for shells".into()));
            }
        }
        let task = self.chosen_task().map_err(|m| (NewField::Task, m))?;
        let spawn = match &kind.spawn {
            SpawnKind::Custom { .. } => {
                let line = self.command.trim();
                if line.is_empty() {
                    return Err((NewField::Command, "command is required".into()));
                }
                match shell_words::split(line) {
                    Ok(words) if !words.is_empty() => SpawnKind::Custom {
                        line: line.to_string(),
                    },
                    _ => return Err((NewField::Command, "command has unbalanced quotes".into())),
                }
            }
            other => other.clone(),
        };
        let bypass = self.is_agent() && self.bypass && self.lock_of(NewField::Bypass).is_none();
        let no_sandbox =
            self.is_agent() && self.no_sandbox && self.lock_of(NewField::NoSandbox).is_none();
        if task.is_some() && (bypass || no_sandbox) {
            return Err((
                NewField::Task,
                "a task start cannot use bypass or no sandbox; turn them off".into(),
            ));
        }
        Ok(NewRequest {
            kind: spawn,
            folder,
            name: (!name.is_empty()).then(|| name.to_string()),
            place: self.place,
            worktree: self.worktree && self.is_agent() && task.is_none(),
            permission_cards: self.permission_cards && self.is_agent() && task.is_none(),
            bypass,
            no_sandbox,
            task,
            checkpoints: Some(self.checkpoints),
        })
    }
}

struct Notes {
    remembered: Vec<String>,
    error: Vec<String>,
    hint: Vec<String>,
}

impl Notes {
    fn error_h(&self) -> usize {
        self.error.len().max(1)
    }

    fn hint_h(&self) -> usize {
        self.hint.len().max(1)
    }
}

fn wrap(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cw = 0usize;
    for word in text.split(' ') {
        let ww = word.width();
        if cw + usize::from(!cur.is_empty()) + ww <= width {
            if !cur.is_empty() {
                cur.push(' ');
                cw += 1;
            }
            cur.push_str(word);
            cw += ww;
            continue;
        }
        if !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
        }
        cw = 0;
        for c in word.chars() {
            let w = c.width().unwrap_or(0);
            if cw + w > width {
                lines.push(std::mem::take(&mut cur));
                cw = 0;
            }
            cur.push(c);
            cw += w;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            while last.width() + 1 > width {
                last.pop();
            }
            last.push('\u{2026}');
        }
    }
    lines
}

fn notes(ns: &NewSession, area_width: u16) -> Notes {
    let width = usize::from(MAX_WIDTH.min(area_width))
        .saturating_sub(4)
        .max(1);
    let clean = |s: &Option<String>, max| {
        s.as_deref()
            .map(|t| wrap(&strip_control(&sanitize_text(t)), width, max))
            .unwrap_or_default()
    };
    Notes {
        remembered: ns
            .remembered_line()
            .map(|t| wrap(&t, width, 2))
            .unwrap_or_default(),
        error: clean(&ns.error, 2),
        hint: clean(&ns.error_hint, 3),
    }
}

fn prompt_lines(p: &PrivPrompt, area_width: u16) -> Vec<String> {
    let width = usize::from(MAX_WIDTH.min(area_width))
        .saturating_sub(4)
        .max(1);
    let mut out: Vec<String> = p
        .ask
        .warnings
        .iter()
        .flat_map(|w| wrap(w, width, usize::MAX))
        .collect();
    out.push(String::new());
    out.extend(wrap(&p.ask.prompt, width, 3));
    out
}

fn review_wrapped(lines: &[String], width: u16) -> Vec<String> {
    let width = usize::from(MAX_WIDTH.min(width)).saturating_sub(4).max(1);
    lines
        .iter()
        .flat_map(|l| wrap(&strip_control(&sanitize_text(l)), width, 3))
        .collect()
}

pub(crate) fn new_session_rect(area: Rect, ns: &NewSession) -> Rect {
    let w = MAX_WIDTH.min(area.width);
    let n = notes(ns, area.width);
    let body = match (&ns.review, &ns.priv_prompt) {
        (Some(lines), _) => review_wrapped(lines, area.width).len() + 4,
        (None, Some(p)) => prompt_lines(p, area.width).len() + 4,
        (None, None) => ns.fields().len() + n.remembered.len() + n.error_h() + n.hint_h() + 6,
    };
    let h = u16::try_from(body).unwrap_or(u16::MAX).min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

fn inner(rect: Rect) -> Rect {
    rect.inner(Margin::new(2, 1))
}

fn field_rows(ns: &NewSession, inner: Rect) -> Vec<(NewField, Rect)> {
    if ns.review.is_some() || ns.priv_prompt.is_some() {
        return Vec::new();
    }
    ns.fields()
        .into_iter()
        .zip(0u16..)
        .map(|(f, k)| {
            (
                f,
                Rect::new(inner.x, inner.y.saturating_add(k), inner.width, 1),
            )
        })
        .filter(|(_, r)| r.y < inner.bottom())
        .collect()
}

fn value_x(row: Rect) -> u16 {
    row.x.saturating_add(LABEL_W)
}

fn value_avail(row: Rect) -> usize {
    usize::from(row.width.saturating_sub(LABEL_W))
}

fn option_spans(labels: &[String], sel: usize, avail: usize) -> Vec<(usize, u16, u16)> {
    let widths: Vec<usize> = labels.iter().map(|l| l.width() + 2).collect();
    let mut start = sel.min(labels.len().saturating_sub(1));
    while start > 0 && widths[start - 1..=sel].iter().sum::<usize>() + (sel - start + 1) <= avail {
        start -= 1;
    }
    let mut out = Vec::new();
    let mut x = 0usize;
    for (i, w) in widths.iter().enumerate().skip(start) {
        if x + w > avail {
            break;
        }
        out.push((i, x as u16, *w as u16));
        x += w + 1;
    }
    out
}

fn option_labels(ns: &NewSession, field: NewField) -> (Vec<String>, usize) {
    match field {
        NewField::Kind => (
            ns.kinds.iter().map(|k| strip_control(&k.label)).collect(),
            ns.kind,
        ),
        NewField::Worktree => (
            vec!["Off".to_string(), "On".to_string()],
            usize::from(ns.worktree),
        ),
        NewField::Checkpoints => (
            vec!["Off".to_string(), "On".to_string()],
            usize::from(ns.checkpoints),
        ),
        NewField::PermissionCards => (
            vec!["Off".to_string(), "On".to_string()],
            usize::from(ns.permission_cards),
        ),
        NewField::Bypass => (
            vec!["Off".to_string(), "On".to_string()],
            usize::from(ns.bypass),
        ),
        NewField::NoSandbox => (
            vec!["Off".to_string(), "On".to_string()],
            usize::from(ns.no_sandbox),
        ),
        _ => (
            Place::ALL.iter().map(|p| p.label().to_string()).collect(),
            Place::ALL.iter().position(|p| *p == ns.place).unwrap_or(0),
        ),
    }
}

fn field_label(f: NewField) -> &'static str {
    match f {
        NewField::Kind => "Kind",
        NewField::Task => "Task",
        NewField::Command => "Command",
        NewField::Folder => "Folder",
        NewField::Name => "Name",
        NewField::Where => "Where",
        NewField::Worktree => "Worktree",
        NewField::Checkpoints => "Rewind",
        NewField::PermissionCards => "Approvals",
        NewField::Bypass => "Bypass",
        NewField::NoSandbox => "No sandbox",
        NewField::Start => "",
    }
}

fn field_text(ns: &NewSession, f: NewField) -> Option<&str> {
    match f {
        NewField::Task => Some(&ns.task),
        NewField::Command => Some(&ns.command),
        NewField::Folder => Some(&ns.folder),
        NewField::Name => Some(&ns.name),
        _ => None,
    }
}

fn task_hint(ns: &NewSession) -> String {
    let hits = ns.task_matches();
    if ns.task.trim().is_empty() {
        return match &ns.task_note {
            Some(n) => strip_control(&sanitize_text(n)),
            None => "optional, type a key or words".into(),
        };
    }
    match hits.get(ns.task_at.min(hits.len().saturating_sub(1))) {
        Some(t) => format!(
            "  -> {} {} ({}/{})",
            strip_control(&sanitize_text(&t.key)),
            strip_control(&sanitize_text(&t.title)),
            ns.task_at.min(hits.len() - 1) + 1,
            hits.len()
        ),
        None => "  (no match)".into(),
    }
}

pub(crate) fn draw_new_session(buf: &mut Buffer, area: Rect, st: &ViewState) {
    let Some(ns) = &st.new_session else {
        return;
    };
    let rect = new_session_rect(area, ns);
    if rect.is_empty() {
        return;
    }
    Clear.render(rect, buf);
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(if ns.review.is_some() {
            " Start this run? "
        } else if ns.priv_prompt.is_some() {
            " Confirm "
        } else {
            " New session "
        })
        .render(rect, buf);
    let inner = inner(rect);
    if inner.is_empty() {
        return;
    }
    if let Some(lines) = &ns.review {
        let lines = review_wrapped(lines, area.width);
        for (i, text) in lines.iter().enumerate() {
            let y = inner.y.saturating_add(u16::try_from(i).unwrap_or(u16::MAX));
            if y < inner.bottom() {
                put(buf, inner.x, y, text, Style::new(), inner.width);
            }
        }
        let y = inner
            .y
            .saturating_add(u16::try_from(lines.len() + 1).unwrap_or(u16::MAX));
        if y < inner.bottom() {
            put(
                buf,
                inner.x,
                y,
                "y starts the run, Esc goes back",
                dim(),
                inner.width,
            );
        }
        return;
    }
    if let Some(p) = &ns.priv_prompt {
        if p.seen.get().is_none() {
            p.seen.set(Some(Instant::now()));
        }
        let lines = prompt_lines(p, area.width);
        for (i, text) in lines.iter().enumerate() {
            let y = inner.y.saturating_add(u16::try_from(i).unwrap_or(u16::MAX));
            if y < inner.bottom() {
                put(buf, inner.x, y, text, Style::new(), inner.width);
            }
        }
        let y = inner
            .y
            .saturating_add(u16::try_from(lines.len()).unwrap_or(u16::MAX));
        if y < inner.bottom() {
            put(
                buf,
                inner.x,
                y,
                &format!("> {}", strip_control(&p.typed)),
                Style::new(),
                inner.width,
            );
        }
        if y.saturating_add(1) < inner.bottom() {
            let (text, style) = if p.cleared {
                (
                    "Type one key at a time; input was cleared.",
                    Style::new().fg(colors().bad),
                )
            } else {
                ("Enter confirms, Esc goes back", dim())
            };
            put(buf, inner.x, y + 1, text, style, inner.width);
        }
        return;
    }
    let rows = field_rows(ns, inner);
    for (f, row) in &rows {
        let focused = *f == ns.focus;
        let marker = if focused { "> " } else { "  " };
        let label_style = if focused {
            Style::new().add_modifier(Modifier::BOLD)
        } else {
            dim()
        };
        put(buf, row.x, row.y, marker, label_style, row.width);
        put(
            buf,
            row.x + 2,
            row.y,
            field_label(*f),
            label_style,
            row.width.saturating_sub(2),
        );
        let x = value_x(*row);
        let avail = value_avail(*row);
        match f {
            NewField::Bypass | NewField::NoSandbox if ns.lock_of(*f).is_some() => {
                let reason = ns.lock_of(*f).unwrap_or_default();
                put(
                    buf,
                    x,
                    row.y,
                    &format!(" locked: {}", strip_control(&sanitize_text(&reason))),
                    dim(),
                    row.right().saturating_sub(x),
                );
            }
            NewField::Kind
            | NewField::Where
            | NewField::Worktree
            | NewField::Checkpoints
            | NewField::PermissionCards
            | NewField::Bypass
            | NewField::NoSandbox => {
                let (labels, sel) = option_labels(ns, *f);
                for (i, off, w) in option_spans(&labels, sel, avail) {
                    let style = if i == sel {
                        let on = Style::new().add_modifier(Modifier::REVERSED);
                        if sel == 1 && matches!(f, NewField::Bypass | NewField::NoSandbox) {
                            on.fg(colors().bad)
                        } else {
                            on
                        }
                    } else {
                        Style::new()
                    };
                    put(buf, x + off, row.y, &format!(" {} ", labels[i]), style, w);
                }
            }
            NewField::Start => {
                let style = if focused {
                    Style::new().add_modifier(Modifier::REVERSED)
                } else {
                    Style::new()
                };
                put(
                    buf,
                    x,
                    row.y,
                    START_LABEL,
                    style,
                    row.right().saturating_sub(x),
                );
            }
            _ => {
                let text = strip_control(field_text(ns, *f).unwrap_or(""));
                let tail = query_tail(&text, avail.saturating_sub(1));
                put(
                    buf,
                    x,
                    row.y,
                    tail,
                    Style::new(),
                    row.right().saturating_sub(x),
                );
                if *f == NewField::Task {
                    let used = tail.width() as u16;
                    let hx = x.saturating_add(used);
                    put(
                        buf,
                        hx,
                        row.y,
                        &task_hint(ns),
                        dim(),
                        row.right().saturating_sub(hx),
                    );
                }
            }
        }
    }
    let base = u16::try_from(ns.fields().len()).unwrap_or(u16::MAX);
    let line = |k: usize| {
        inner
            .y
            .saturating_add(base)
            .saturating_add(u16::try_from(k).unwrap_or(u16::MAX))
    };
    let bottom = inner.bottom();
    let n = notes(ns, area.width);
    for (i, text) in n.remembered.iter().enumerate() {
        if line(1 + i) < bottom {
            put(
                buf,
                inner.x,
                line(1 + i),
                text,
                Style::new().fg(colors().bad),
                inner.width,
            );
        }
    }
    let error_at = 1 + n.remembered.len();
    for (i, text) in n.error.iter().enumerate() {
        if line(error_at + i) < bottom {
            put(
                buf,
                inner.x,
                line(error_at + i),
                text,
                Style::new().fg(colors().bad),
                inner.width,
            );
        }
    }
    let hint_at = error_at + n.error_h();
    for (i, text) in n.hint.iter().enumerate() {
        if line(hint_at + i) < bottom {
            put(buf, inner.x, line(hint_at + i), text, dim(), inner.width);
        }
    }
    let help_at = hint_at + n.hint_h();
    if line(help_at) < bottom {
        put(
            buf,
            inner.x,
            line(help_at),
            if ns.task.trim().is_empty() {
                "Enter starts, Esc cancels"
            } else {
                "Enter shows the run to confirm, Esc cancels"
            },
            dim(),
            inner.width,
        );
    }
}

pub(crate) fn new_session_cursor(ns: &NewSession, area: Rect) -> Option<(u16, u16)> {
    let rect = new_session_rect(area, ns);
    let inner = inner(rect);
    if rect.is_empty() || inner.is_empty() || ns.review.is_some() {
        return None;
    }
    if let Some(p) = &ns.priv_prompt {
        let y = inner
            .y
            .saturating_add(u16::try_from(prompt_lines(p, area.width).len()).unwrap_or(u16::MAX));
        let x = inner.x.saturating_add(2 + p.typed.width() as u16);
        return Some((
            x.min(inner.right().saturating_sub(1)),
            y.min(inner.bottom().saturating_sub(1)),
        ));
    }
    let (_, row) = field_rows(ns, inner)
        .into_iter()
        .find(|(f, _)| *f == ns.focus)?;
    let text = strip_control(field_text(ns, ns.focus)?);
    let avail = value_avail(row);
    let w = query_tail(&text, avail.saturating_sub(1)).width();
    let x = value_x(row).saturating_add(w as u16);
    Some((x.min(row.right().saturating_sub(1)), row.y))
}

impl ViewState {
    pub fn open_new_session(&mut self, folder: String, kinds: Vec<KindOpt>, home: PathBuf) {
        let kind = self
            .new_kind
            .as_ref()
            .and_then(|id| kinds.iter().position(|k| &k.id == id))
            .unwrap_or(0);
        self.palette = None;
        self.help = false;
        self.confirm = None;
        self.rename = None;
        self.sidebar_focus = None;
        self.new_session = Some(NewSession {
            kinds,
            kind,
            folder,
            command: String::new(),
            name: String::new(),
            place: self.new_place.unwrap_or_default(),
            worktree: self.worktree_default,
            permission_cards: self.permission_cards_default,
            bypass: false,
            no_sandbox: false,
            remembered: false,
            priv_gate: self.priv_gate,
            priv_lock: self.privileged_lock.clone(),
            priv_prompt: None,
            error: None,
            error_hint: None,
            focus: NewField::Kind,
            taken_names: self.sessions.iter().map(|s| s.name.clone()).collect(),
            task: String::new(),
            tasks: self.new_tasks.clone(),
            task_at: 0,
            task_note: self.new_tasks_note.clone(),
            checkpoints: self.cp.form_default,
            review: None,
            home,
            list_dir: list_dirs,
        });
    }

    pub fn apply_remembered(&mut self, r: &Remembered) {
        let Some(ns) = self.new_session.as_mut() else {
            return;
        };
        if let Some(v) = r.worktree {
            ns.worktree = v;
        }
        if let Some(v) = r.permission_cards {
            ns.permission_cards = v;
        }
        if let Some(v) = r.checkpoints {
            ns.checkpoints = v;
        }
        if !r.bypass && !r.no_sandbox {
            return;
        }
        let clean = |t: &str| strip_control(&sanitize_text(t));
        let agent = clean(&r.agent);
        let selected = ns
            .kinds
            .get(ns.kind)
            .filter(|_| ns.is_agent())
            .map(|k| k.id.as_str());
        let mut off = Vec::new();
        let mut reason = String::new();
        let wrong_agent = selected != Some(r.agent.as_str());
        if wrong_agent {
            off.extend([
                r.bypass.then_some("bypass"),
                r.no_sandbox.then_some("no sandbox"),
            ]);
        } else {
            for (wanted, field, name) in [
                (r.bypass, NewField::Bypass, "bypass"),
                (r.no_sandbox, NewField::NoSandbox, "no sandbox"),
            ] {
                if !wanted {
                    continue;
                }
                match ns.lock_of(field) {
                    None => {
                        if field == NewField::Bypass {
                            ns.bypass = true;
                        } else {
                            ns.no_sandbox = true;
                        }
                        ns.remembered = true;
                    }
                    Some(lock) => {
                        off.push(Some(name));
                        reason = if lock == "no bypass mode" {
                            "this agent has no bypass mode".into()
                        } else {
                            format!("locked: {}", clean(&lock))
                        };
                    }
                }
            }
        }
        let off: Vec<&str> = off.into_iter().flatten().collect();
        if off.is_empty() {
            return;
        }
        let (names, verb, pronoun) = match off.as_slice() {
            [one] => (one.to_string(), "is", "it was"),
            _ => (off.join(" and "), "are", "they were"),
        };
        if wrong_agent {
            reason = if agent.is_empty() {
                format!("{pronoun} set for a different agent")
            } else {
                format!("{pronoun} set for {agent}")
            };
        }
        ns.error_hint = Some(format!("Remembered {names} {verb} off: {reason}"));
    }

    pub fn set_new_tasks(&mut self, mut tasks: Vec<TaskOpt>, note: Option<String>) {
        tasks.truncate(MAX_TASK_OPTS);
        self.new_tasks = tasks;
        self.new_tasks_note = note;
        if let Some(ns) = self.new_session.as_mut() {
            ns.tasks = self.new_tasks.clone();
            ns.task_note = self.new_tasks_note.clone();
        }
    }

    pub fn review_fits(&self, area: Rect) -> bool {
        self.new_session
            .as_ref()
            .and_then(|ns| ns.review.as_ref())
            .is_some_and(|lines| {
                review_wrapped(lines, area.width).len() + 4 <= usize::from(area.height)
            })
    }

    pub fn refuse_priv_batch(&mut self, forwards: usize) -> bool {
        let Some(p) = self
            .new_session
            .as_mut()
            .and_then(|n| n.priv_prompt.as_mut())
            .filter(|_| forwards > 1)
        else {
            return false;
        };
        p.typed.clear();
        p.cleared = true;
        true
    }

    pub fn priv_prompt_fits(&self, area: Rect) -> bool {
        self.new_session
            .as_ref()
            .and_then(|ns| ns.priv_prompt.as_ref())
            .is_some_and(|p| prompt_lines(p, area.width).len() + 4 <= usize::from(area.height))
    }

    pub fn review_new_session(&mut self, mut ns: NewSession, lines: Vec<String>) {
        ns.review = Some(lines);
        self.new_session = Some(ns);
    }

    pub fn reopen_new_session(&mut self, ns: NewSession) {
        self.new_session = Some(ns);
    }

    pub fn remember_new_session(&mut self, req: &NewRequest, kind_id: &str) {
        self.new_kind = Some(kind_id.to_string());
        self.new_place = Some(req.place);
    }

    pub fn edit_new_session(&mut self, data: &[u8]) -> NewEdit {
        let Some(ns) = self.new_session.as_mut() else {
            return NewEdit::Cancel;
        };
        if ns.review.is_some() {
            if !data.is_empty() && data.iter().all(|&b| b == 0x1b) {
                ns.review = None;
            } else if matches!(data, b"y" | b"Y") {
                return NewEdit::Approve;
            }
            return NewEdit::Editing;
        }
        if ns.priv_prompt.is_some() {
            return self.edit_priv_prompt(data);
        }
        if !data.is_empty() && data.iter().all(|&b| b == 0x1b) {
            self.new_session = None;
            return NewEdit::Cancel;
        }
        let mut i = 0;
        while i < data.len() {
            let Some(ns) = self.new_session.as_mut() else {
                break;
            };
            match data[i] {
                0x1b => {
                    let (key, used) = esc_key(&data[i..]);
                    let seq = &data[i..i + used];
                    i += used;
                    match key {
                        Key::Up => ns.step(false),
                        Key::Down => ns.step(true),
                        Key::Other => match seq {
                            b"\x1b[C" | b"\x1bOC" => ns.cycle(true),
                            b"\x1b[D" | b"\x1bOD" => ns.cycle(false),
                            b"\x1b[Z" => ns.step(false),
                            _ => {}
                        },
                    }
                }
                b'\t' => {
                    i += 1;
                    if ns.focus != NewField::Folder || !ns.complete_folder() {
                        ns.step(true);
                    }
                }
                b'\r' | b'\n' => {
                    i += 1;
                    if let Some(req) = ns.try_submit() {
                        self.new_session = None;
                        return NewEdit::Submit(req);
                    }
                    if ns.priv_prompt.is_some() {
                        break;
                    }
                }
                0x7f | 0x08 => {
                    i += 1;
                    if let Some((s, _)) = ns.text_field() {
                        s.pop();
                        ns.clear_error();
                    }
                }
                0x15 => {
                    i += 1;
                    if let Some((s, _)) = ns.text_field() {
                        s.clear();
                        ns.clear_error();
                    }
                }
                _ => {
                    let (c, used) = next_char(&data[i..]);
                    i += used;
                    if let Some(c) = c.filter(|c| !c.is_control()) {
                        ns.push_char(c);
                    }
                }
            }
        }
        if self.new_session.is_some() {
            NewEdit::Editing
        } else {
            NewEdit::Cancel
        }
    }

    fn edit_priv_prompt(&mut self, data: &[u8]) -> NewEdit {
        let Some(ns) = self.new_session.as_mut() else {
            return NewEdit::Cancel;
        };
        let Some(p) = ns.priv_prompt.as_mut() else {
            return NewEdit::Editing;
        };
        if !data.is_empty() && data.iter().all(|&b| b == 0x1b) {
            ns.priv_prompt = None;
            return NewEdit::Editing;
        }
        if !p.seen.get().is_some_and(|t| t.elapsed() >= PROMPT_DELAY) {
            return NewEdit::Editing;
        }
        let (c, used) = next_char(data);
        if data.is_empty() || used != data.len() {
            p.typed.clear();
            p.cleared = true;
            return NewEdit::Editing;
        }
        p.cleared = false;
        match data[0] {
            b'\r' | b'\n' => {
                let Some(p) = ns.priv_prompt.take() else {
                    return NewEdit::Editing;
                };
                if p.typed == p.ask.word {
                    self.new_session = None;
                    return NewEdit::Submit(p.req);
                }
                ns.bypass = false;
                ns.no_sandbox = false;
                ns.error = Some("confirmation did not match, nothing was started".into());
                ns.error_hint = None;
            }
            0x7f | 0x08 => {
                p.typed.pop();
            }
            0x15 => p.typed.clear(),
            _ => {
                if let Some(c) = c.filter(|c| !c.is_control()) {
                    if p.typed.chars().count() < MAX_TYPED {
                        p.typed.push(c);
                    }
                }
            }
        }
        NewEdit::Editing
    }

    pub fn insert_new_session_text(&mut self, data: &[u8]) {
        let Some(ns) = self
            .new_session
            .as_mut()
            .filter(|n| n.review.is_none() && n.priv_prompt.is_none())
        else {
            return;
        };
        let text = String::from_utf8_lossy(data)
            .replace("\x1b[200~", "")
            .replace("\x1b[201~", "");
        for c in text.chars().filter(|c| !c.is_control()) {
            ns.push_char(c);
        }
    }

    pub(crate) fn new_session_mouse(
        &mut self,
        area: Rect,
        ev: MouseEvent,
        mut out: MouseOut,
    ) -> MouseOut {
        let Some(ns) = self.new_session.as_mut() else {
            return out;
        };
        if !ev.is_left_press() {
            return out;
        }
        out.redraw = true;
        let rect = new_session_rect(area, ns);
        if !contains(rect, ev.col, ev.row) {
            self.new_session = None;
            out.new_edit = Some(NewEdit::Cancel);
            return out;
        }
        if ns.review.is_some() || ns.priv_prompt.is_some() {
            return out;
        }
        let Some((field, row)) = field_rows(ns, inner(rect))
            .into_iter()
            .find(|(_, r)| r.y == ev.row)
        else {
            return out;
        };
        ns.focus = field;
        let x = value_x(row);
        let avail = value_avail(row);
        match field {
            NewField::Kind
            | NewField::Where
            | NewField::Worktree
            | NewField::Checkpoints
            | NewField::PermissionCards
            | NewField::Bypass
            | NewField::NoSandbox => {
                let (labels, sel) = option_labels(ns, field);
                let hit = option_spans(&labels, sel, avail)
                    .into_iter()
                    .find(|(_, off, w)| ev.col >= x + off && ev.col < x + off + w);
                if let Some((i, _, _)) = hit {
                    if field == NewField::Kind {
                        ns.kind = i;
                        ns.bypass = false;
                        ns.no_sandbox = false;
                        ns.remembered = false;
                    } else if field == NewField::Bypass {
                        ns.remembered = false;
                        ns.bypass = i == 1 && ns.lock_of(field).is_none();
                    } else if field == NewField::NoSandbox {
                        ns.remembered = false;
                        ns.no_sandbox = i == 1 && ns.lock_of(field).is_none();
                    } else if field == NewField::Worktree {
                        ns.worktree = i == 1;
                    } else if field == NewField::Checkpoints {
                        ns.checkpoints = i == 1;
                    } else if field == NewField::PermissionCards {
                        ns.permission_cards = i == 1;
                    } else {
                        ns.place = Place::ALL[i];
                    }
                    ns.clear_error();
                }
            }
            NewField::Start => {
                let w = START_LABEL.width() as u16;
                if ev.col >= x && ev.col < x + w {
                    out.new_edit = Some(match ns.try_submit() {
                        Some(req) => {
                            self.new_session = None;
                            NewEdit::Submit(req)
                        }
                        None => NewEdit::Editing,
                    });
                }
            }
            _ => {}
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attach::view::SessionRow;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tmgr_ipc::api::{State, StateKind};

    fn kinds() -> Vec<KindOpt> {
        let k = |id: &str, spawn| KindOpt {
            id: id.into(),
            label: id.into(),
            spawn,
            bypass: id == "claude",
        };
        vec![
            k(
                "claude",
                SpawnKind::Agent {
                    profile: "claude".into(),
                },
            ),
            k(
                "codex",
                SpawnKind::Agent {
                    profile: "codex".into(),
                },
            ),
            k("shell", SpawnKind::Shell),
            k(
                "custom",
                SpawnKind::Custom {
                    line: String::new(),
                },
            ),
        ]
    }

    fn test_gate(bypass: bool, no_sandbox: bool) -> Result<PrivAsk, (String, Option<String>)> {
        let word = if no_sandbox { "no-sandbox" } else { "bypass" };
        let mut warnings = Vec::new();
        if bypass {
            warnings.push(
                "warning: bypass mode: the agent runs every command and edit without asking.",
            );
        }
        if no_sandbox {
            warnings.push("warning: no sandbox: the agent runs with your real home folder, your SSH keys, credentials and environment, with no sandbox and no network limits. Only the runner's control files are protected (macOS).");
        }
        Ok(PrivAsk {
            word,
            prompt: format!("Type {word} to continue:"),
            warnings,
        })
    }

    fn open(folder: &str, home: &Path) -> ViewState {
        let mut v = ViewState::new(Vec::new(), "r");
        v.priv_gate = Some(test_gate);
        v.open_new_session(folder.into(), kinds(), home.to_path_buf());
        v
    }

    fn ns(v: &mut ViewState) -> &mut NewSession {
        v.new_session.as_mut().unwrap()
    }

    fn submit_err(v: &mut ViewState) -> String {
        assert_eq!(v.edit_new_session(b"\r"), NewEdit::Editing);
        v.new_session.as_ref().unwrap().error.clone().unwrap()
    }

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn press(col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            cb: 0,
            col,
            row,
            release: false,
        }
    }

    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 30,
    };

    fn click(v: &mut ViewState, col: u16, row: u16) -> MouseOut {
        v.on_mouse(AREA, press(col, row), &|_| Default::default())
    }

    fn row_of(v: &mut ViewState, f: NewField) -> Rect {
        let ns = v.new_session.as_ref().unwrap();
        let rect = new_session_rect(AREA, ns);
        field_rows(ns, inner(rect))
            .into_iter()
            .find(|(g, _)| *g == f)
            .unwrap()
            .1
    }

    fn draw(v: &ViewState, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| {
            let area = f.area();
            draw_new_session(f.buffer_mut(), area, v)
        })
        .unwrap();
        let buf = t.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn defaults_and_remembered_choices() {
        let h = tmp();
        let mut v = open("~/x", h.path());
        assert_eq!(ns(&mut v).kind, 0);
        assert_eq!(ns(&mut v).place, Place::Right);
        assert_eq!(ns(&mut v).folder, "~/x");
        let req = NewRequest {
            kind: SpawnKind::Shell,
            folder: "/".into(),
            name: None,
            place: Place::Below,
            task: None,
            worktree: false,
            checkpoints: None,
            permission_cards: false,
            bypass: false,
            no_sandbox: false,
        };
        v.remember_new_session(&req, "shell");
        let p = v.prefs();
        assert_eq!(p.new_kind.as_deref(), Some("shell"));
        let mut w = ViewState::new(Vec::new(), "r");
        w.apply_prefs(&p);
        w.open_new_session("/".into(), kinds(), h.path().to_path_buf());
        assert_eq!(ns(&mut w).kind, 2);
        assert_eq!(ns(&mut w).place, Place::Below);
        w.new_kind = Some("gone".into());
        w.open_new_session("/".into(), kinds(), h.path().to_path_buf());
        assert_eq!(ns(&mut w).kind, 0);
    }

    #[test]
    fn taken_names_come_from_sessions_and_dialog_clears_other_modals() {
        let mut v = ViewState::new(
            vec![SessionRow {
                id: "a".into(),
                name: "alpha".into(),
                state: State {
                    kind: StateKind::Working,
                    since_ms: 0,
                    detail: None,
                    exit_code: None,
                },
                age: String::new(),
                you: None,
                esc: false,
                resumed_from: None,
            }],
            "r",
        );
        v.help = true;
        v.palette = Some(Default::default());
        v.open_new_session("/".into(), kinds(), PathBuf::from("/"));
        assert!(!v.help && v.palette.is_none());
        assert_eq!(ns(&mut v).taken_names, vec!["alpha".to_string()]);
    }

    #[test]
    fn kind_cycling_shows_command_only_for_custom() {
        let h = tmp();
        let mut v = open("/", h.path());
        assert!(!ns(&mut v).fields().contains(&NewField::Command));
        v.edit_new_session(b"\x1b[D");
        assert_eq!(ns(&mut v).kind, 3);
        assert!(ns(&mut v).fields().contains(&NewField::Command));
        v.edit_new_session(b"\x1b[B");
        assert_eq!(ns(&mut v).focus, NewField::Command);
        v.edit_new_session(b"\x1b[A\x1b[C");
        assert_eq!(ns(&mut v).kind, 0);
        v.edit_new_session(b"\x1b[B");
        assert_eq!(ns(&mut v).focus, NewField::Folder);
    }

    #[test]
    fn where_cycles_and_tab_moves_between_fields() {
        let h = tmp();
        let mut v = open("/", h.path());
        v.edit_new_session(b"\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B");
        assert_eq!(ns(&mut v).focus, NewField::Where);
        v.edit_new_session(b"\x1b[C");
        assert_eq!(ns(&mut v).place, Place::Below);
        v.edit_new_session(b"\x1b[D\x1b[D");
        assert_eq!(ns(&mut v).place, Place::Tab);
        v.edit_new_session(b"\x1b[D");
        assert_eq!(ns(&mut v).place, Place::Replace);
        v.edit_new_session(b"\t");
        assert_eq!(ns(&mut v).focus, NewField::Checkpoints);
        v.edit_new_session(b"\t");
        assert_eq!(ns(&mut v).focus, NewField::Start);
        v.edit_new_session(b"\x1b[Z\x1b[Z");
        assert_eq!(ns(&mut v).focus, NewField::Where);
    }

    #[test]
    fn folder_validation() {
        let h = tmp();
        let file = h.path().join("f");
        std::fs::write(&file, "x").unwrap();
        let mut v = open("", h.path());
        assert_eq!(submit_err(&mut v), "folder is required");
        assert_eq!(ns(&mut v).focus, NewField::Folder);
        ns(&mut v).folder = "relative/dir".into();
        assert_eq!(
            submit_err(&mut v),
            "folder must be an absolute path or start with ~"
        );
        ns(&mut v).folder = h.path().join("absent").to_string_lossy().into_owned();
        assert_eq!(submit_err(&mut v), "no such folder");
        ns(&mut v).folder = file.to_string_lossy().into_owned();
        assert_eq!(submit_err(&mut v), "no such folder");
    }

    #[test]
    fn tilde_expands_with_injected_home_and_submit_closes() {
        let h = tmp();
        std::fs::create_dir(h.path().join("proj")).unwrap();
        let mut v = open("~/proj/", h.path());
        ns(&mut v).name = "  work ".into();
        v.edit_new_session(b"\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\x1b[C");
        let NewEdit::Submit(req) = v.edit_new_session(b"\r") else {
            panic!("expected submit");
        };
        assert_eq!(req.folder, h.path().join("proj").to_string_lossy());
        assert_eq!(req.name.as_deref(), Some("work"));
        assert_eq!(req.place, Place::Below);
        assert_eq!(
            req.kind,
            SpawnKind::Agent {
                profile: "claude".into()
            }
        );
        assert!(v.new_session.is_none());
        let mut v = open("~", h.path());
        let NewEdit::Submit(req) = v.edit_new_session(b"\r") else {
            panic!("expected submit");
        };
        assert_eq!(req.folder, h.path().to_string_lossy());
        assert_eq!(req.name, None);
    }

    #[test]
    fn the_worktree_row_is_off_by_default_and_for_agents_only() {
        let h = tmp();
        let mut v = open("/", h.path());
        assert!(ns(&mut v).fields().contains(&NewField::Worktree));
        assert!(!ns(&mut v).worktree);
        v.edit_new_session(b"\x1b[B\x1b[B\x1b[B\x1b[B");
        assert_eq!(ns(&mut v).focus, NewField::Worktree);
        v.edit_new_session(b"\x1b[C");
        assert!(ns(&mut v).worktree);
        let NewEdit::Submit(req) = v.edit_new_session(b"\r") else {
            panic!("expected submit");
        };
        assert!(req.worktree);
        let mut v = open("/", h.path());
        v.edit_new_session(b"\x1b[B\x1b[B\x1b[B\x1b[B\x1b[C");
        v.edit_new_session(b"\x1b[A\x1b[A\x1b[A\x1b[A\x1b[D");
        assert_eq!(ns(&mut v).kind, 3);
        assert!(!ns(&mut v).fields().contains(&NewField::Worktree));
        ns(&mut v).command = "echo".into();
        let NewEdit::Submit(req) = v.edit_new_session(b"\r") else {
            panic!("expected submit");
        };
        assert!(!req.worktree);
    }

    #[test]
    fn the_worktree_row_starts_from_the_configured_default() {
        let h = tmp();
        let mut v = ViewState::new(Vec::new(), "r");
        v.worktree_default = true;
        v.open_new_session("/".into(), kinds(), h.path().to_path_buf());
        assert!(ns(&mut v).worktree);
        v.edit_new_session(b"\x1b[B\x1b[B\x1b[B\x1b[B\x1b[D");
        assert!(!ns(&mut v).worktree);
    }

    #[test]
    fn the_permission_cards_row_is_off_by_default_and_for_agents_only() {
        let h = tmp();
        let mut v = open("/", h.path());
        assert!(ns(&mut v).fields().contains(&NewField::PermissionCards));
        assert!(!ns(&mut v).permission_cards);
        v.edit_new_session(b"\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B");
        assert_eq!(ns(&mut v).focus, NewField::PermissionCards);
        v.edit_new_session(b"\x1b[C");
        assert!(ns(&mut v).permission_cards);
        let NewEdit::Submit(req) = v.edit_new_session(b"\r") else {
            panic!("expected submit");
        };
        assert!(req.permission_cards);
        let mut v = open("/", h.path());
        ns(&mut v).permission_cards = true;
        v.edit_new_session(b"\x1b[C\x1b[C\x1b[C");
        assert!(!ns(&mut v).fields().contains(&NewField::PermissionCards));
        ns(&mut v).command = "echo".into();
        let NewEdit::Submit(req) = v.edit_new_session(b"\r") else {
            panic!("expected submit");
        };
        assert!(!req.permission_cards);
    }

    #[test]
    fn the_permission_cards_row_starts_from_the_configured_default() {
        let h = tmp();
        let mut v = ViewState::new(Vec::new(), "r");
        v.permission_cards_default = true;
        v.open_new_session("/".into(), kinds(), h.path().to_path_buf());
        assert!(ns(&mut v).permission_cards);
    }

    #[test]
    fn a_task_start_makes_its_own_clone_so_the_worktree_flag_is_dropped() {
        let h = tmp();
        let mut v = open("/", h.path());
        with_tasks(&mut v);
        ns(&mut v).worktree = true;
        ns(&mut v).task = "TM-12".into();
        let NewEdit::Submit(req) = v.edit_new_session(b"\r") else {
            panic!("expected submit");
        };
        assert!(req.task.is_some() && !req.worktree);
    }

    #[test]
    fn name_validation() {
        let h = tmp();
        let mut v = open("~", h.path());
        ns(&mut v).taken_names = vec!["dup".into()];
        ns(&mut v).name = "x".repeat(65);
        assert!(submit_err(&mut v).contains("longer than 64"));
        ns(&mut v).name = "dup".into();
        assert_eq!(submit_err(&mut v), "a session is already named \"dup\"");
        ns(&mut v).name = "sh-12".into();
        assert_eq!(submit_err(&mut v), "sh-N names are reserved for shells");
        ns(&mut v).name = "sh-x".into();
        assert!(matches!(v.edit_new_session(b"\r"), NewEdit::Submit(_)));
        let mut v = open("~", h.path());
        ns(&mut v).kind = 2;
        ns(&mut v).name = "sh-12".into();
        assert!(matches!(v.edit_new_session(b"\r"), NewEdit::Submit(_)));
    }

    #[test]
    fn custom_command_validation() {
        let h = tmp();
        let mut v = open("~", h.path());
        ns(&mut v).kind = 3;
        assert_eq!(submit_err(&mut v), "command is required");
        assert_eq!(ns(&mut v).focus, NewField::Command);
        ns(&mut v).command = "echo 'oops".into();
        assert_eq!(submit_err(&mut v), "command has unbalanced quotes");
        ns(&mut v).command = " htop -d 5 ".into();
        let NewEdit::Submit(req) = v.edit_new_session(b"\r") else {
            panic!("expected submit");
        };
        assert_eq!(
            req.kind,
            SpawnKind::Custom {
                line: "htop -d 5".into()
            }
        );
    }

    #[test]
    fn typing_editing_caps_and_error_reset() {
        let h = tmp();
        let mut v = open("", h.path());
        submit_err(&mut v);
        v.edit_new_session("é\u{7f}ab".as_bytes());
        assert_eq!(ns(&mut v).folder, "ab");
        assert!(ns(&mut v).error.is_none());
        v.edit_new_session(b"\x15");
        assert_eq!(ns(&mut v).folder, "");
        v.edit_new_session("я\u{8}я\x07".as_bytes());
        assert_eq!(ns(&mut v).folder, "я");
        v.edit_new_session("z".repeat(600).as_bytes());
        assert_eq!(ns(&mut v).folder.chars().count(), MAX_FOLDER);
        ns(&mut v).focus = NewField::Name;
        v.edit_new_session("n".repeat(100).as_bytes());
        assert_eq!(ns(&mut v).name.chars().count(), MAX_NAME);
        ns(&mut v).focus = NewField::Kind;
        v.edit_new_session(b"qq");
        assert_eq!(ns(&mut v).kind, 0);
    }

    #[test]
    fn several_keys_in_one_chunk_and_escape_handling() {
        let h = tmp();
        let mut v = open("~", h.path());
        let out = v.edit_new_session(b"\x1b[B\x1b[B\x1b[B\x1b[B\r");
        assert!(matches!(out, NewEdit::Submit(_)));
        let mut v = open("~", h.path());
        v.edit_new_session(b"\x1b[B\x1b[B\x1bOAx");
        assert_eq!(ns(&mut v).focus, NewField::Folder);
        assert_eq!(ns(&mut v).folder, "~x");
        let mut v = open("~", h.path());
        v.edit_new_session(b"\x1bOA");
        assert_eq!(ns(&mut v).folder, "~");
        assert_eq!(v.edit_new_session(b"\x1b"), NewEdit::Cancel);
        assert!(v.new_session.is_none());
        let mut v = open("~", h.path());
        assert_eq!(v.edit_new_session(b"\x1b\x1b"), NewEdit::Cancel);
    }

    #[test]
    fn paste_never_submits_and_strips_controls() {
        let h = tmp();
        let mut v = open("", h.path());
        ns(&mut v).focus = NewField::Folder;
        v.insert_new_session_text(b"\x1b[200~/a/b\r\nc\x07d\x1b[201~");
        assert_eq!(ns(&mut v).folder, "/a/bcd");
        assert!(v.new_session.is_some());
        ns(&mut v).focus = NewField::Where;
        v.insert_new_session_text(b"zzz");
        assert_eq!(ns(&mut v).folder, "/a/bcd");
    }

    fn fake_dirs(_: &Path) -> Vec<String> {
        vec![
            "alpha".into(),
            "alps".into(),
            "beta".into(),
            ".hidden".into(),
            "x\u{7}y".into(),
        ]
    }

    fn tab_in_folder(v: &mut ViewState, folder: &str) {
        ns(v).folder = folder.into();
        ns(v).focus = NewField::Folder;
        v.edit_new_session(b"\t");
    }

    #[test]
    fn tab_completion_single_multiple_none() {
        let h = tmp();
        let mut v = open("/r/b", h.path());
        ns(&mut v).list_dir = fake_dirs;
        tab_in_folder(&mut v, "/r/b");
        assert_eq!(ns(&mut v).folder, "/r/beta/");
        assert_eq!(ns(&mut v).focus, NewField::Folder);
        tab_in_folder(&mut v, "/r/a");
        assert_eq!(ns(&mut v).folder, "/r/alp");
        assert_eq!(ns(&mut v).focus, NewField::Folder);
        tab_in_folder(&mut v, "/r/.h");
        assert_eq!(ns(&mut v).folder, "/r/.hidden/");
        tab_in_folder(&mut v, "/r/x");
        assert_eq!(ns(&mut v).folder, "/r/xy/");
        tab_in_folder(&mut v, "~");
        assert_eq!(ns(&mut v).folder, "~/");
        assert_eq!(ns(&mut v).focus, NewField::Folder);
    }

    #[test]
    fn tab_moves_on_when_completion_changes_nothing() {
        let h = tmp();
        let mut v = open("/r", h.path());
        ns(&mut v).list_dir = fake_dirs;
        tab_in_folder(&mut v, "/r/zz");
        assert_eq!(ns(&mut v).folder, "/r/zz");
        assert_eq!(ns(&mut v).focus, NewField::Task);
        tab_in_folder(&mut v, "/r/alp");
        assert_eq!(ns(&mut v).focus, NewField::Task);
        tab_in_folder(&mut v, "relative");
        assert_eq!(ns(&mut v).focus, NewField::Task);
        tab_in_folder(&mut v, "");
        assert_eq!(ns(&mut v).focus, NewField::Task);
    }

    #[test]
    fn tab_completion_expands_home_and_lists_real_dirs() {
        let h = tmp();
        std::fs::create_dir(h.path().join("projects")).unwrap();
        std::fs::write(h.path().join("pfile"), "x").unwrap();
        let mut v = open("~/p", h.path());
        ns(&mut v).focus = NewField::Folder;
        v.edit_new_session(b"\t");
        assert_eq!(ns(&mut v).folder, "~/projects/");
    }

    #[test]
    fn mouse_focus_select_start_and_outside() {
        let h = tmp();
        let mut v = open("~", h.path());
        let kind = row_of(&mut v, NewField::Kind);
        let (labels, sel) = option_labels(v.new_session.as_ref().unwrap(), NewField::Kind);
        let spans = option_spans(&labels, sel, value_avail(kind));
        let (_, off, _) = spans[2];
        let out = click(&mut v, value_x(kind) + off + 1, kind.y);
        assert!(out.redraw && out.new_edit.is_none());
        assert_eq!(ns(&mut v).kind, 2);
        assert_eq!(ns(&mut v).focus, NewField::Kind);
        let folder = row_of(&mut v, NewField::Folder);
        click(&mut v, folder.x + 15, folder.y);
        assert_eq!(ns(&mut v).focus, NewField::Folder);
        let wh = row_of(&mut v, NewField::Where);
        let (labels, sel) = option_labels(v.new_session.as_ref().unwrap(), NewField::Where);
        let (_, off, _) = option_spans(&labels, sel, value_avail(wh))[2];
        click(&mut v, value_x(wh) + off + 1, wh.y);
        assert_eq!(ns(&mut v).place, Place::Below);
        let start = row_of(&mut v, NewField::Start);
        let out = click(&mut v, value_x(start) + 2, start.y);
        let Some(NewEdit::Submit(req)) = out.new_edit else {
            panic!("expected submit");
        };
        assert_eq!(req.kind, SpawnKind::Shell);
        assert_eq!(req.place, Place::Below);
        assert!(v.new_session.is_none());
    }

    #[test]
    fn mouse_start_with_error_stays_and_outside_cancels() {
        let h = tmp();
        let mut v = open("", h.path());
        let start = row_of(&mut v, NewField::Start);
        let out = click(&mut v, value_x(start) + 1, start.y);
        assert_eq!(out.new_edit, Some(NewEdit::Editing));
        assert!(ns(&mut v).error.is_some());
        let rect = new_session_rect(AREA, v.new_session.as_ref().unwrap());
        let out = click(&mut v, rect.x + 1, rect.y + 1);
        assert!(out.new_edit.is_none() && out.forward.is_none() && out.control.is_none());
        assert!(v.new_session.is_some());
        let wheel = MouseEvent {
            cb: 64,
            col: 0,
            row: 0,
            release: false,
        };
        let out = v.on_mouse(AREA, wheel, &|_| Default::default());
        assert!(out.new_edit.is_none() && v.new_session.is_some());
        let out = click(&mut v, 0, 0);
        assert_eq!(out.new_edit, Some(NewEdit::Cancel));
        assert!(v.new_session.is_none());
    }

    #[test]
    fn render_shows_fields_error_and_hint() {
        let h = tmp();
        let mut v = open("~/code", h.path());
        let s = draw(&v, 100, 30);
        for want in [
            "New session",
            "Kind",
            "claude",
            "Folder",
            "~/code",
            "Name",
            "Where",
            "Right",
            "[ Start ]",
            "Enter starts, Esc cancels",
        ] {
            assert!(s.contains(want), "missing {want}\n{s}");
        }
        assert!(!s.contains("Command"));
        ns(&mut v).kind = 3;
        let s = draw(&v, 100, 30);
        assert!(s.contains("Command"));
        let ns0 = v.new_session.take().unwrap();
        v.reopen_new_session(ns0.with_error("boom", Some("Run it yourself: tmgr run x".into())));
        let s = draw(&v, 100, 30);
        assert!(s.contains("boom") && s.contains("Run it yourself: tmgr run x"));
        let s = draw(&v, 30, 8);
        assert!(s.contains("New session"));
    }

    #[test]
    fn cursor_follows_the_active_text_field() {
        let h = tmp();
        let mut v = open("~/ab", h.path());
        assert_eq!(
            new_session_cursor(v.new_session.as_ref().unwrap(), AREA),
            None
        );
        ns(&mut v).focus = NewField::Folder;
        let row = row_of(&mut v, NewField::Folder);
        assert_eq!(
            new_session_cursor(v.new_session.as_ref().unwrap(), AREA),
            Some((value_x(row) + 4, row.y))
        );
        ns(&mut v).folder = "x".repeat(300);
        let (x, _) = new_session_cursor(v.new_session.as_ref().unwrap(), AREA).unwrap();
        assert!(x < row.right());
    }

    fn long_hint() -> String {
        "tmgr run --agent-cmd 'some-tool --flag value' --cwd '/very/long/path/to/some/project/folder' --name 'my agent'".into()
    }

    #[test]
    fn wrap_breaks_at_spaces_hard_splits_and_truncates() {
        assert_eq!(wrap("aa bb cc", 5, 3), ["aa bb", "cc"]);
        assert_eq!(wrap("abcdefgh", 3, 5), ["abc", "def", "gh"]);
        let cut = wrap("aaaa bbbb cccc dddd", 4, 2);
        assert_eq!(cut.len(), 2);
        assert!(cut[1].ends_with('\u{2026}') && cut[1].width() <= 4);
        assert!(wrap("", 4, 2).is_empty());
        for l in wrap(&long_hint(), 68, 3) {
            assert!(l.width() <= 68);
        }
    }

    #[test]
    fn a_long_hint_wraps_over_lines_and_stays_exact() {
        let h = tmp();
        let mut v = open("~/code", h.path());
        let ns0 = v.new_session.take().unwrap();
        v.reopen_new_session(ns0.with_error("boom", Some(long_hint())));
        let n = notes(v.new_session.as_ref().unwrap(), 100);
        assert!(n.hint.len() >= 2 && n.hint.len() <= 3);
        assert_eq!(n.hint.join(" "), long_hint());
        let s = draw(&v, 100, 30);
        assert!(s.contains("Enter starts, Esc cancels"));
        for l in &n.hint {
            assert!(s.contains(l.as_str()), "missing {l}\n{s}");
        }
    }

    #[test]
    fn field_rows_and_mouse_hits_stay_put_with_a_long_hint() {
        let h = tmp();
        let mut v = open("~/code", h.path());
        let (before_rect, before) = {
            let ns = v.new_session.as_ref().unwrap();
            let rect = new_session_rect(AREA, ns);
            (rect, field_rows(ns, inner(rect)))
        };
        let ns0 = v.new_session.take().unwrap();
        v.reopen_new_session(ns0.with_error("e ".repeat(80), Some(long_hint())));
        let ns = v.new_session.as_ref().unwrap();
        let rect = new_session_rect(AREA, ns);
        let after = field_rows(ns, inner(rect));
        assert_eq!(after.len(), before.len());
        assert!(rect.height > 12);
        for ((f, a), (g, b)) in after.iter().zip(&before) {
            assert_eq!(f, g);
            assert_eq!(a.width, b.width);
            assert_eq!(a.y - rect.y, b.y - before_rect.y);
        }
        let folder = row_of(&mut v, NewField::Folder);
        click(&mut v, folder.x + 15, folder.y);
        assert_eq!(ns_of(&v).focus, NewField::Folder);
        let start = row_of(&mut v, NewField::Start);
        let out = click(&mut v, value_x(start) + 2, start.y);
        assert!(matches!(out.new_edit, Some(NewEdit::Submit(_))) || v.new_session.is_some());
        let below = rect.bottom() - 2;
        let out = click(&mut v, rect.x + 5, below);
        assert!(out.new_edit.is_none() && v.new_session.is_some());
    }

    fn ns_of(v: &ViewState) -> &NewSession {
        v.new_session.as_ref().unwrap()
    }

    #[test]
    fn tiny_sizes_never_panic() {
        let h = tmp();
        let mut v = open("~/code", h.path());
        let ns0 = v.new_session.take().unwrap();
        v.reopen_new_session(ns0.with_error("boom\x1b[31m", Some(long_hint())));
        for (w, hh) in [(1u16, 1u16), (10, 3), (12, 4), (30, 5)] {
            let area = Rect::new(0, 0, w, hh);
            draw(&v, w, hh);
            let _ = new_session_cursor(ns_of(&v), area);
            for (c, r) in [
                (0, 0),
                (w / 2, hh / 2),
                (w.saturating_sub(1), hh.saturating_sub(1)),
            ] {
                let mut vv = v.clone();
                vv.on_mouse(area, press(c, r), &|_| Default::default());
            }
        }
    }

    fn task(key: &str, title: &str) -> TaskOpt {
        TaskOpt {
            key: key.into(),
            title: title.into(),
        }
    }

    fn with_tasks(v: &mut ViewState) {
        v.set_new_tasks(
            vec![
                task("TM-12", "Fix the login redirect"),
                task("TM-30", "Login page copy"),
                task("TM-7", "Rename the exporter"),
            ],
            None,
        );
    }

    fn snapshot(name: &str, actual: &str) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/attach/snapshots")
            .join(format!("{name}.txt"));
        if std::env::var_os("TMGR_UPDATE_SNAPSHOTS").is_some() {
            std::fs::write(&path, format!("{actual}\n")).unwrap();
        }
        let expected = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(expected.trim_end_matches('\n'), actual, "snapshot {name}");
    }

    #[test]
    fn the_task_row_exists_for_agents_only() {
        let h = tmp();
        let mut v = open("/", h.path());
        assert!(ns(&mut v).fields().contains(&NewField::Task));
        v.edit_new_session(b"\x1b[D");
        assert_eq!(ns(&mut v).kind, 3);
        assert!(!ns(&mut v).fields().contains(&NewField::Task));
        v.edit_new_session(b"\x1b[D");
        assert_eq!(ns(&mut v).kind, 2);
        assert!(!ns(&mut v).fields().contains(&NewField::Task));
    }

    #[test]
    fn typing_words_picks_a_task_and_arrows_cycle_the_matches() {
        let h = tmp();
        let mut v = open("/", h.path());
        with_tasks(&mut v);
        v.edit_new_session(b"\x1b[B\x1b[B");
        assert_eq!(ns(&mut v).focus, NewField::Task);
        v.edit_new_session(b"login");
        let hits: Vec<String> = ns(&mut v)
            .task_matches()
            .iter()
            .map(|t| t.key.clone())
            .collect();
        assert_eq!(hits.len(), 2);
        assert!(hits.contains(&"TM-12".to_string()) && hits.contains(&"TM-30".to_string()));
        let first = hits[0].clone();
        v.edit_new_session(b"\x1b[C");
        assert_eq!(ns(&mut v).task_at, 1);
        v.edit_new_session(b"\x1b[C");
        assert_eq!(ns(&mut v).task_at, 0);
        v.edit_new_session(b"\x1b[D");
        let last = hits[1].clone();
        assert_eq!(ns(&mut v).task_at, 1);
        let NewEdit::Submit(req) = v.edit_new_session(b"\r") else {
            panic!("expected a submit");
        };
        assert_eq!(req.task.as_deref(), Some(last.as_str()));
        assert_ne!(last, first);
    }

    #[test]
    fn an_exact_key_wins_over_fuzzy_neighbours() {
        let h = tmp();
        let mut v = open("/", h.path());
        with_tasks(&mut v);
        v.edit_new_session(b"\x1b[B\x1b[B");
        v.edit_new_session(b"tm-7");
        let hits = ns(&mut v).task_matches();
        assert_eq!(hits[0].key, "TM-7");
        let NewEdit::Submit(req) = v.edit_new_session(b"\r") else {
            panic!("expected a submit");
        };
        assert_eq!(req.task.as_deref(), Some("TM-7"));
    }

    #[test]
    fn a_typed_reference_outside_the_list_is_kept_and_junk_is_refused() {
        let h = tmp();
        let mut v = open("/", h.path());
        with_tasks(&mut v);
        v.edit_new_session(b"\x1b[B\x1b[B");
        v.edit_new_session(b"ZZ-99");
        let NewEdit::Submit(req) = v.edit_new_session(b"\r") else {
            panic!("expected a submit");
        };
        assert_eq!(req.task.as_deref(), Some("ZZ-99"));

        let mut v = open("/", h.path());
        v.edit_new_session(b"\x1b[B\x1b[B");
        v.edit_new_session(b"nothing like it");
        assert!(submit_err(&mut v).contains("no such task"));
        assert_eq!(ns(&mut v).focus, NewField::Task);
        v.edit_new_session(b"\x15");
        assert!(matches!(v.edit_new_session(b"\r"), NewEdit::Submit(r) if r.task.is_none()));
    }

    #[test]
    fn a_task_is_dropped_when_the_kind_is_not_an_agent() {
        let h = tmp();
        let mut v = open("/", h.path());
        with_tasks(&mut v);
        v.edit_new_session(b"\x1b[B\x1b[B");
        v.edit_new_session(b"TM-7");
        v.edit_new_session(b"\x1b[A\x1b[A\x1b[D\x1b[D");
        assert_eq!(ns(&mut v).kind, 2);
        let NewEdit::Submit(req) = v.edit_new_session(b"\r") else {
            panic!("expected a submit");
        };
        assert_eq!(req.task, None);
    }

    #[test]
    fn late_tasks_reach_the_open_form_and_the_next_one() {
        let h = tmp();
        let mut v = open("/", h.path());
        assert!(ns(&mut v).tasks.is_empty());
        with_tasks(&mut v);
        assert_eq!(ns(&mut v).tasks.len(), 3);
        v.set_new_tasks(Vec::new(), Some("local workspaces only".into()));
        assert_eq!(
            ns(&mut v).task_note.as_deref(),
            Some("local workspaces only")
        );
        v.open_new_session("/".into(), kinds(), h.path().to_path_buf());
        assert_eq!(
            ns(&mut v).task_note.as_deref(),
            Some("local workspaces only")
        );
    }

    #[test]
    fn the_review_waits_for_y_and_esc_goes_back_to_the_form() {
        let h = tmp();
        let mut v = open("/", h.path());
        let dialog = v.new_session.take().unwrap();
        v.review_new_session(dialog, vec!["Task:    TM-7 x".into()]);
        assert_eq!(v.edit_new_session(b"abc\x1b[B"), NewEdit::Editing);
        assert!(ns(&mut v).review.is_some() && ns(&mut v).name.is_empty());
        assert_eq!(v.edit_new_session(b"\r"), NewEdit::Editing);
        assert_eq!(v.edit_new_session(b"y"), NewEdit::Approve);
        assert_eq!(v.edit_new_session(b"\x1b"), NewEdit::Editing);
        assert!(ns(&mut v).review.is_none());
        assert_eq!(v.edit_new_session(b"\x1b"), NewEdit::Cancel);
    }

    #[test]
    fn a_click_inside_the_review_does_nothing_and_outside_cancels() {
        let h = tmp();
        let mut v = open("/", h.path());
        let dialog = v.new_session.take().unwrap();
        v.review_new_session(dialog, vec!["Task:    TM-7 x".into()]);
        let rect = new_session_rect(AREA, ns_of(&v));
        let out = click(&mut v, rect.x + 3, rect.y + 1);
        assert!(out.new_edit.is_none() && ns_of(&v).review.is_some());
        let out = click(&mut v, 0, 0);
        assert_eq!(out.new_edit, Some(NewEdit::Cancel));
    }

    #[test]
    fn hostile_task_text_cannot_reach_the_screen_as_control_bytes() {
        let h = tmp();
        let mut v = open("/", h.path());
        v.set_new_tasks(vec![task("TM-1", "evil\x1b[2Jtitle\x07")], None);
        v.edit_new_session(b"\x1b[B\x1b[B");
        v.edit_new_session(b"evil");
        let shown = draw(&v, 100, 20);
        assert!(!shown.contains('\x1b') && !shown.contains('\x07'));
        let dialog = v.new_session.take().unwrap();
        v.review_new_session(dialog, vec!["Task:    TM-1 evil\x1b[2J".into()]);
        let shown = draw(&v, 100, 20);
        assert!(!shown.contains('\x1b'));
    }

    #[test]
    fn snapshot_task_row() {
        let h = tmp();
        let mut v = open("~/code", h.path());
        with_tasks(&mut v);
        v.edit_new_session(b"\x1b[B\x1b[B");
        v.edit_new_session(b"login");
        snapshot("new_session_task", &draw(&v, 80, 14));
    }

    #[test]
    fn snapshot_task_row_without_tasks() {
        let h = tmp();
        let mut v = open("~/code", h.path());
        v.set_new_tasks(
            Vec::new(),
            Some("local workspaces only, see `tmgr run`".into()),
        );
        snapshot("new_session_task_note", &draw(&v, 80, 14));
    }

    #[test]
    fn snapshot_review() {
        let h = tmp();
        let mut v = open("~/code", h.path());
        let dialog = v.new_session.take().unwrap();
        v.review_new_session(
            dialog,
            vec![
                "Task:    TM-12 Fix the login redirect".into(),
                "Author:  you".into(),
                "Repo:    ~/code/app".into(),
                "Clone:   ~/code/app-worktrees/run/tm-12".into(),
                "Branch:  tm-12-fix-the-login-redirect".into(),
                "Agent:   claude-code in ask mode".into(),
            ],
        );
        snapshot("new_session_review", &draw(&v, 80, 16));
    }

    #[test]
    fn a_literal_key_beats_a_fuzzy_neighbour() {
        let h = tmp();
        let mut v = open("/", h.path());
        v.set_new_tasks(vec![task("TM-170", "other"), task("TM-7", "seven")], None);
        v.edit_new_session(b"\x1b[B\x1b[B");
        v.edit_new_session(b"TM-70");
        let NewEdit::Submit(req) = v.edit_new_session(b"\r") else {
            panic!("expected a submit");
        };
        assert_eq!(req.task.as_deref(), Some("TM-70"));
    }

    #[test]
    fn paste_during_the_review_is_ignored_and_a_short_screen_cannot_approve() {
        let h = tmp();
        let mut v = open("/", h.path());
        let dialog = v.new_session.take().unwrap();
        v.review_new_session(dialog, vec!["Clone:   ".to_string() + &"x".repeat(150)]);
        v.insert_new_session_text(b"junk");
        assert!(ns(&mut v).task.is_empty() && ns(&mut v).folder == "/");
        assert!(v.review_fits(Rect::new(0, 0, 80, 20)));
        assert!(!v.review_fits(Rect::new(0, 0, 80, 5)));
    }

    #[test]
    fn a_long_review_line_wraps_instead_of_being_cut() {
        let h = tmp();
        let mut v = open("/", h.path());
        let dialog = v.new_session.take().unwrap();
        v.review_new_session(
            dialog,
            vec!["Clone:   /a/b/c/".to_string() + &"z".repeat(120)],
        );
        let shown = draw(&v, 80, 20);
        assert!(shown.matches('z').count() >= 120);
    }

    fn ready(v: &ViewState) {
        if let Some(p) = v.new_session.as_ref().and_then(|n| n.priv_prompt.as_ref()) {
            p.seen
                .set(Instant::now().checked_sub(Duration::from_secs(1)));
        }
    }

    fn keys(v: &mut ViewState, text: &str) -> NewEdit {
        ready(v);
        let mut last = NewEdit::Editing;
        for c in text.chars() {
            let mut b = [0u8; 4];
            last = v.edit_new_session(c.encode_utf8(&mut b).as_bytes());
            if last != NewEdit::Editing {
                return last;
            }
        }
        last
    }

    fn flip(v: &mut ViewState, f: NewField) {
        ns(v).focus = f;
        v.edit_new_session(b"\x1b[C");
    }

    fn privileged(v: &mut ViewState, bypass: bool, no_sandbox: bool) {
        if bypass {
            flip(v, NewField::Bypass);
        }
        if no_sandbox {
            flip(v, NewField::NoSandbox);
        }
    }

    #[test]
    fn the_privilege_rows_start_off_for_agents_only() {
        let h = tmp();
        let mut v = open("/", h.path());
        assert!(!ns(&mut v).bypass && !ns(&mut v).no_sandbox);
        let fields = ns(&mut v).fields();
        assert!(fields.contains(&NewField::Bypass) && fields.contains(&NewField::NoSandbox));
        ns(&mut v).kind = 2;
        let fields = ns(&mut v).fields();
        assert!(!fields.contains(&NewField::Bypass) && !fields.contains(&NewField::NoSandbox));
        let req = ns(&mut v).validate().unwrap();
        assert!(!req.bypass && !req.no_sandbox);
    }

    #[test]
    fn an_unprivileged_submit_never_asks_anything() {
        let h = tmp();
        let mut v = open("/", h.path());
        let NewEdit::Submit(req) = v.edit_new_session(b"\r") else {
            panic!("expected submit");
        };
        assert!(!req.bypass && !req.no_sandbox);
    }

    #[test]
    fn a_privileged_submit_keeps_the_form_open_until_the_word_is_typed() {
        let h = tmp();
        let mut v = open("/", h.path());
        privileged(&mut v, true, false);
        assert!(ns(&mut v).bypass);
        assert_eq!(v.edit_new_session(b"\r"), NewEdit::Editing);
        assert!(ns(&mut v).priv_prompt_open());
        assert_eq!(keys(&mut v, "bypas"), NewEdit::Editing);
        let NewEdit::Submit(req) = keys(&mut v, "s\r") else {
            panic!("expected submit");
        };
        assert!(req.bypass && !req.no_sandbox);
        assert!(v.new_session.is_none());
    }

    #[test]
    fn no_sandbox_asks_for_its_own_word_even_with_bypass() {
        let h = tmp();
        let mut v = open("/", h.path());
        privileged(&mut v, true, true);
        assert_eq!(v.edit_new_session(b"\r"), NewEdit::Editing);
        assert_eq!(keys(&mut v, "bypass\r"), NewEdit::Editing);
        assert!(v.new_session.is_some() && !ns(&mut v).priv_prompt_open());
        privileged(&mut v, true, true);
        v.edit_new_session(b"\r");
        let NewEdit::Submit(req) = keys(&mut v, "no-sandbox\r") else {
            panic!("expected submit");
        };
        assert!(req.bypass && req.no_sandbox);
    }

    #[test]
    fn a_wrong_word_cancels_keeps_the_form_and_turns_the_toggles_off() {
        let h = tmp();
        let mut v = open("/", h.path());
        privileged(&mut v, true, true);
        v.edit_new_session(b"\r");
        assert_eq!(keys(&mut v, "yes\r"), NewEdit::Editing);
        let n = ns(&mut v);
        assert!(!n.priv_prompt_open() && !n.bypass && !n.no_sandbox);
        assert!(n.error.as_deref().unwrap().contains("nothing was started"));
    }

    #[test]
    fn the_word_must_match_exactly() {
        let h = tmp();
        for typed in ["Bypass\r", " bypass\r", "bypass \r", "bypas\r", "\r"] {
            let mut v = open("/", h.path());
            privileged(&mut v, true, false);
            v.edit_new_session(b"\r");
            assert_eq!(keys(&mut v, typed), NewEdit::Editing, "{typed:?}");
            assert!(v.new_session.is_some());
        }
    }

    #[test]
    fn escape_in_the_prompt_goes_back_to_the_form() {
        let h = tmp();
        let mut v = open("/", h.path());
        privileged(&mut v, true, false);
        v.edit_new_session(b"\r");
        assert_eq!(v.edit_new_session(b"\x1b"), NewEdit::Editing);
        assert!(v.new_session.is_some() && !ns(&mut v).priv_prompt_open());
    }

    #[test]
    fn paste_arrows_and_trailing_bytes_cannot_confirm() {
        let h = tmp();
        let mut v = open("/", h.path());
        privileged(&mut v, true, false);
        v.edit_new_session(b"\r");
        ready(&v);
        v.insert_new_session_text(b"bypass");
        assert_eq!(v.edit_new_session(b"\x1b[A\x1b[C"), NewEdit::Editing);
        assert!(ns(&mut v).priv_prompt_open());
        assert_eq!(v.edit_new_session(b"\r"), NewEdit::Editing);
        assert!(v.new_session.is_some());
    }

    #[test]
    fn bytes_after_the_enter_that_opens_the_prompt_are_dropped() {
        let h = tmp();
        let mut v = open("/", h.path());
        privileged(&mut v, true, false);
        assert_eq!(v.edit_new_session(b"\rbypass\r"), NewEdit::Editing);
        assert!(ns(&mut v).priv_prompt_open());
        ready(&v);
        assert_eq!(v.edit_new_session(b"bypass\r"), NewEdit::Editing);
        assert!(ns(&mut v).priv_prompt_open());
    }

    #[test]
    fn a_whole_word_with_enter_in_one_chunk_never_confirms_and_clears_the_line() {
        let h = tmp();
        for raw in ["no-sandbox\r", "\x1b[201~no-sandbox\r", "no-sandbox\n"] {
            let mut v = prompt_view(h.path(), 0, true);
            ready(&v);
            assert_eq!(v.edit_new_session(b"no-"), NewEdit::Editing);
            assert_eq!(
                v.edit_new_session(raw.as_bytes()),
                NewEdit::Editing,
                "{raw:?}"
            );
            assert!(v.new_session.is_some() && ns(&mut v).priv_prompt_open());
            assert_eq!(v.edit_new_session(b"\r"), NewEdit::Editing);
            assert!(!ns(&mut v).priv_prompt_open() && v.new_session.is_some());
        }
    }

    #[test]
    fn utf8_characters_and_backspace_still_edit_the_line() {
        let h = tmp();
        let mut v = prompt_view(h.path(), 0, true);
        keys(&mut v, "\u{e9}x");
        v.edit_new_session(b"\x7f");
        assert!(draw(&v, 76, 14).contains("> \u{e9}"));
        assert!(!draw(&v, 76, 14).contains("> \u{e9}x"));
    }

    #[test]
    fn nothing_typed_before_the_prompt_was_shown_for_a_moment_counts() {
        let h = tmp();
        let mut v = prompt_view(h.path(), 0, true);
        for c in "no-sandbox".chars() {
            let mut b = [0u8; 4];
            v.edit_new_session(c.encode_utf8(&mut b).as_bytes());
        }
        assert_eq!(v.edit_new_session(b"\r"), NewEdit::Editing);
        assert!(ns(&mut v).priv_prompt_open());
        let mut v = prompt_view(h.path(), 0, true);
        draw(&v, 76, 14);
        for c in "no-sandbox".chars() {
            let mut b = [0u8; 4];
            v.edit_new_session(c.encode_utf8(&mut b).as_bytes());
        }
        assert_eq!(v.edit_new_session(b"\r"), NewEdit::Editing);
        assert!(ns(&mut v).priv_prompt_open());
    }

    fn replay(v: &mut ViewState, reads: &[&[u8]]) -> Vec<NewEdit> {
        let mut split = crate::attach::InputSplitter::new();
        let mut out = Vec::new();
        for read in reads {
            let actions = split.feed(read);
            let forwards = actions
                .iter()
                .filter(|a| matches!(a, crate::attach::Action::Forward(_)))
                .count();
            if v.refuse_priv_batch(forwards) {
                continue;
            }
            for action in actions {
                if let crate::attach::Action::Forward(d) = action {
                    if v.new_session.is_some() {
                        out.push(v.edit_new_session(&d));
                    }
                }
            }
        }
        out
    }

    #[test]
    fn after_the_delay_one_read_split_by_events_is_refused_and_says_so() {
        let h = tmp();
        let one_read: &[u8] =
            b"n\x1b[Io\x1b[I-\x1b[Is\x1b[Ia\x1b[In\x1b[Id\x1b[Ib\x1b[Io\x1b[Ix\x1b[I\r";
        let mut v = prompt_view(h.path(), 0, true);
        ready(&v);
        let out = replay(&mut v, &[b"n", one_read]);
        assert!(out.iter().all(|e| !matches!(e, NewEdit::Submit(_))));
        assert!(ns(&mut v).priv_prompt_open());
        let shown = draw(&v, 76, 14);
        assert!(shown.contains("input was cleared"));
        assert!(!shown.contains("> n"));
        let out = replay(&mut v, &[b"x"]);
        assert!(out.iter().all(|e| !matches!(e, NewEdit::Submit(_))));
        assert!(!draw(&v, 76, 14).contains("input was cleared"));
    }

    #[test]
    fn a_coalesced_chunk_clears_the_line_with_a_visible_note() {
        let h = tmp();
        let mut v = prompt_view(h.path(), 0, false);
        keys(&mut v, "byp");
        assert_eq!(v.edit_new_session(b"s\r"), NewEdit::Editing);
        assert!(ns(&mut v).priv_prompt_open() && ns(&mut v).bypass);
        assert!(draw(&v, 76, 14).contains("Type one key at a time; input was cleared."));
    }

    #[test]
    fn queued_input_split_by_events_or_reads_cannot_confirm() {
        let h = tmp();
        let cases: [&[&[u8]]; 5] = [
            &[b"\r\x1b[Ibypass\r"],
            &[b"\r\x1b[<35;5;5Mbypass\r"],
            &[b"\r", b"bypass\r"],
            &[b"\r\x1b[Ib\x1b[Iy\x1b[Ip\x1b[Ia\x1b[Is\x1b[Is\x1b[I\r"],
            &[b"\r\x1b[Ib", b"y", b"p", b"a", b"s", b"s", b"\r"],
        ];
        for reads in cases {
            let mut v = open("/", h.path());
            privileged(&mut v, true, false);
            let out = replay(&mut v, reads);
            assert!(
                out.iter().all(|e| !matches!(e, NewEdit::Submit(_))),
                "{reads:?}"
            );
            assert!(v.new_session.is_some());
        }
    }

    #[test]
    fn a_click_on_start_followed_by_queued_bytes_cannot_confirm() {
        let h = tmp();
        let mut v = open("/", h.path());
        privileged(&mut v, false, true);
        let row = row_of(&mut v, NewField::Start);
        click(&mut v, row.x + LABEL_W + 2, row.y);
        assert!(ns(&mut v).priv_prompt_open());
        let out = replay(&mut v, &[b"n", b"o", b"-", b"\r"]);
        assert!(out.iter().all(|e| !matches!(e, NewEdit::Submit(_))));
    }

    #[test]
    fn the_start_button_goes_through_the_same_prompt() {
        let h = tmp();
        let mut v = open("/", h.path());
        privileged(&mut v, false, true);
        let row = row_of(&mut v, NewField::Start);
        let out = click(&mut v, row.x + LABEL_W + 2, row.y);
        assert_eq!(out.new_edit, Some(NewEdit::Editing));
        assert!(ns(&mut v).priv_prompt_open());
        let out = click(&mut v, 50, 15);
        assert!(out.new_edit.is_none() && ns(&mut v).priv_prompt_open());
        let out = click(&mut v, 0, 0);
        assert_eq!(out.new_edit, Some(NewEdit::Cancel));
        assert!(v.new_session.is_none());
    }

    #[test]
    fn clicking_the_toggles_flips_them_and_changing_the_kind_clears_them() {
        let h = tmp();
        let mut v = open("/", h.path());
        let row = row_of(&mut v, NewField::Bypass);
        click(&mut v, row.x + LABEL_W + 6, row.y);
        assert!(ns(&mut v).bypass);
        let row = row_of(&mut v, NewField::NoSandbox);
        click(&mut v, row.x + LABEL_W + 6, row.y);
        assert!(ns(&mut v).no_sandbox);
        ns(&mut v).focus = NewField::Kind;
        v.edit_new_session(b"\x1b[C");
        assert!(!ns(&mut v).bypass && !ns(&mut v).no_sandbox);
        flip(&mut v, NewField::NoSandbox);
        let row = row_of(&mut v, NewField::Kind);
        click(&mut v, row.x + LABEL_W + 2, row.y);
        assert!(!ns(&mut v).no_sandbox);
    }

    #[test]
    fn a_profile_without_a_bypass_mode_locks_only_that_toggle() {
        let h = tmp();
        let mut v = open("/", h.path());
        ns(&mut v).kind = 1;
        flip(&mut v, NewField::Bypass);
        assert!(!ns(&mut v).bypass);
        assert!(ns(&mut v)
            .lock_of(NewField::Bypass)
            .unwrap()
            .contains("no bypass"));
        flip(&mut v, NewField::NoSandbox);
        assert!(ns(&mut v).no_sandbox);
        let shown = draw(&v, 100, 30);
        assert!(shown.contains("locked: no bypass mode"));
    }

    #[test]
    fn a_lock_from_the_caller_blocks_both_toggles_and_the_request() {
        let h = tmp();
        let mut v = open("/", h.path());
        v.new_session = None;
        v.privileged_lock = Some("agent session".into());
        v.open_new_session("/".into(), kinds(), h.path().to_path_buf());
        privileged(&mut v, true, true);
        assert!(!ns(&mut v).bypass && !ns(&mut v).no_sandbox);
        ns(&mut v).bypass = true;
        ns(&mut v).no_sandbox = true;
        let req = ns(&mut v).validate().unwrap();
        assert!(!req.bypass && !req.no_sandbox);
        let shown = draw(&v, 100, 30);
        assert_eq!(shown.matches("locked: agent session").count(), 2);
    }

    #[test]
    fn without_a_gate_the_toggles_are_locked() {
        let h = tmp();
        let mut v = ViewState::new(Vec::new(), "r");
        v.open_new_session("/".into(), kinds(), h.path().to_path_buf());
        flip(&mut v, NewField::NoSandbox);
        assert!(!ns(&mut v).no_sandbox);
    }

    #[test]
    fn a_refusing_gate_shows_its_error_and_never_prompts() {
        fn refuse(_: bool, _: bool) -> Result<PrivAsk, (String, Option<String>)> {
            Err(("refused here".into(), Some("run it yourself".into())))
        }
        let h = tmp();
        let mut v = open("/", h.path());
        v.priv_gate = None;
        v.new_session = None;
        v.priv_gate = Some(refuse);
        v.open_new_session("/".into(), kinds(), h.path().to_path_buf());
        privileged(&mut v, true, false);
        assert_eq!(v.edit_new_session(b"\r"), NewEdit::Editing);
        let n = ns(&mut v);
        assert!(!n.priv_prompt_open());
        assert_eq!(n.error.as_deref(), Some("refused here"));
        assert_eq!(n.error_hint.as_deref(), Some("run it yourself"));
    }

    #[test]
    fn a_task_cannot_be_combined_with_the_toggles() {
        let h = tmp();
        let mut v = open("/", h.path());
        with_tasks(&mut v);
        ns(&mut v).task = "TM-12".into();
        privileged(&mut v, true, false);
        assert_eq!(v.edit_new_session(b"\r"), NewEdit::Editing);
        let n = ns(&mut v);
        assert!(!n.priv_prompt_open());
        assert!(n.error.as_deref().unwrap().contains("task"));
        assert_eq!(n.focus, NewField::Task);
    }

    fn prompt_view(h: &Path, kind: usize, both: bool) -> ViewState {
        let mut v = open("/", h);
        ns(&mut v).kind = kind;
        privileged(&mut v, kind == 0 && !both, both);
        if both {
            flip(&mut v, NewField::Bypass);
        }
        v.edit_new_session(b"\r");
        v
    }

    #[test]
    fn the_prompt_wraps_the_whole_warning_at_narrow_widths_and_needs_room() {
        let h = tmp();
        let mut v = prompt_view(h.path(), 0, true);
        let shown = draw(&v, 30, 40);
        assert!(!shown.contains('\u{2026}'));
        for part in ["asking.", "SSH", "network", "(macOS).", "Type no-sandbox"] {
            assert!(shown.contains(part), "{part}");
        }
        assert!(v.priv_prompt_fits(Rect::new(0, 0, 30, 40)));
        assert!(!v.priv_prompt_fits(Rect::new(0, 0, 30, 6)));
        keys(&mut v, "no-sa");
        assert!(draw(&v, 30, 40).contains("> no-sa"));
    }

    #[test]
    fn hostile_bytes_in_the_prompt_cannot_reach_the_screen() {
        let h = tmp();
        let mut v = prompt_view(h.path(), 0, false);
        keys(&mut v, "a\x07b\x1b]0;x\x07c");
        let shown = draw(&v, 60, 20);
        assert!(!shown.chars().any(|c| c.is_control() && c != '\n'));
    }

    #[test]
    fn snapshot_privilege_prompt() {
        let h = tmp();
        let mut v = prompt_view(h.path(), 0, true);
        keys(&mut v, "no-s");
        snapshot("new_session_privileged", &draw(&v, 76, 14));
    }

    #[test]
    fn snapshot_privilege_prompt_narrow() {
        let h = tmp();
        let v = prompt_view(h.path(), 0, true);
        snapshot("new_session_privileged_narrow", &draw(&v, 34, 20));
    }

    #[test]
    fn snapshot_locked_privilege_rows_narrow() {
        let h = tmp();
        let mut v = open("/", h.path());
        ns(&mut v).kind = 1;
        v.new_session.as_mut().unwrap().priv_lock = None;
        snapshot("new_session_locked_narrow", &draw(&v, 40, 16));
    }

    fn rem(agent: &str, bypass: bool, no_sandbox: bool) -> Remembered {
        Remembered {
            agent: agent.into(),
            bypass,
            no_sandbox,
            ..Remembered::default()
        }
    }

    fn open_remembered(kind: &str, r: &Remembered, h: &Path) -> ViewState {
        let mut v = ViewState::new(Vec::new(), "r");
        v.priv_gate = Some(test_gate);
        v.new_kind = Some(kind.into());
        v.open_new_session("/".into(), kinds(), h.to_path_buf());
        v.apply_remembered(r);
        v
    }

    fn hint(v: &ViewState) -> String {
        v.new_session
            .as_ref()
            .unwrap()
            .error_hint
            .clone()
            .unwrap_or_default()
    }

    fn on_cells_in_alert(v: &ViewState, f: NewField) -> bool {
        let ns = v.new_session.as_ref().unwrap();
        let rect = new_session_rect(AREA, ns);
        let row = field_rows(ns, inner(rect))
            .into_iter()
            .find(|(g, _)| *g == f)
            .unwrap()
            .1;
        let mut t = Terminal::new(TestBackend::new(AREA.width, AREA.height)).unwrap();
        t.draw(|fr| {
            let area = fr.area();
            draw_new_session(fr.buffer_mut(), area, v)
        })
        .unwrap();
        let buf = t.backend().buffer();
        (row.x..row.right()).any(|x| {
            buf[(x, row.y)].symbol() == "O"
                && buf[(x + 1, row.y)].symbol() == "n"
                && buf[(x, row.y)].fg == colors().bad
        })
    }

    #[test]
    fn remembered_flags_for_the_selected_agent_are_on_visible_and_still_ask() {
        let h = tmp();
        let mut v = open_remembered("claude", &rem("claude", true, true), h.path());
        assert!(ns(&mut v).bypass && ns(&mut v).no_sandbox);
        assert_eq!(hint(&v), "");
        let shown = draw(&v, 100, 30);
        assert!(shown.contains("Bypass and no sandbox are on, remembered from your last session"));
        assert!(on_cells_in_alert(&v, NewField::Bypass));
        assert!(on_cells_in_alert(&v, NewField::NoSandbox));
        assert!(!on_cells_in_alert(&v, NewField::Worktree));
        assert_eq!(v.edit_new_session(b"\r"), NewEdit::Editing);
        assert!(ns(&mut v).priv_prompt_open());
    }

    #[test]
    fn the_remembered_line_counts_in_the_dialog_height_and_keeps_errors_visible() {
        let h = tmp();
        let plain = open("/", h.path());
        let v = open_remembered("claude", &rem("claude", true, false), h.path());
        let a = new_session_rect(AREA, plain.new_session.as_ref().unwrap());
        let b = new_session_rect(AREA, v.new_session.as_ref().unwrap());
        assert_eq!(b.height, a.height + 1);
        let mut v = v;
        ns(&mut v).error = Some("folder is required".into());
        let shown = draw(&v, 100, 30);
        assert!(shown.contains("Bypass is on, remembered from your last session"));
        assert!(shown.contains("folder is required"));
        assert!(shown.contains("Enter starts, Esc cancels"));
    }

    #[test]
    fn remembered_flags_for_another_agent_stay_off_with_a_note() {
        let h = tmp();
        let mut v = open_remembered("codex", &rem("claude", true, true), h.path());
        assert!(!ns(&mut v).bypass && !ns(&mut v).no_sandbox);
        assert_eq!(
            hint(&v),
            "Remembered bypass and no sandbox are off: they were set for claude"
        );
        let shown = draw(&v, 100, 30);
        assert!(shown.contains("were set for claude"));
        assert!(!shown.contains("remembered from your last session"));
        let mut v = open_remembered("codex", &rem("claude", false, true), h.path());
        assert!(!ns(&mut v).no_sandbox);
        assert_eq!(
            hint(&v),
            "Remembered no sandbox is off: it was set for claude"
        );
    }

    #[test]
    fn a_kind_without_a_bypass_mode_drops_only_the_bypass() {
        let h = tmp();
        let mut v = open_remembered("codex", &rem("codex", true, true), h.path());
        assert!(!ns(&mut v).bypass && ns(&mut v).no_sandbox);
        assert_eq!(
            hint(&v),
            "Remembered bypass is off: this agent has no bypass mode"
        );
        let shown = draw(&v, 100, 30);
        assert!(shown.contains("No sandbox is on, remembered from your last session"));
        assert!(on_cells_in_alert(&v, NewField::NoSandbox));
    }

    #[test]
    fn a_caller_lock_or_a_missing_gate_keeps_both_off_with_a_note() {
        let h = tmp();
        let mut v = ViewState::new(Vec::new(), "r");
        v.priv_gate = Some(test_gate);
        v.privileged_lock = Some("agent session".into());
        v.open_new_session("/".into(), kinds(), h.path().to_path_buf());
        v.apply_remembered(&rem("claude", true, true));
        assert!(!ns(&mut v).bypass && !ns(&mut v).no_sandbox);
        assert_eq!(
            hint(&v),
            "Remembered bypass and no sandbox are off: locked: agent session"
        );
        let mut v = ViewState::new(Vec::new(), "r");
        v.open_new_session("/".into(), kinds(), h.path().to_path_buf());
        v.apply_remembered(&rem("claude", true, false));
        assert!(!ns(&mut v).bypass);
        assert_eq!(hint(&v), "Remembered bypass is off: locked: unavailable");
    }

    #[test]
    fn nothing_remembered_means_no_note() {
        let h = tmp();
        for r in [rem("", false, false), rem("gone", false, false)] {
            let mut v = open_remembered("claude", &r, h.path());
            assert_eq!(hint(&v), "");
            assert!(!ns(&mut v).bypass && !ns(&mut v).no_sandbox);
            assert!(!draw(&v, 100, 30).contains("emembered"));
        }
    }

    #[test]
    fn changing_the_kind_or_a_toggle_clears_the_flags_and_the_remembered_line() {
        let h = tmp();
        let line = "remembered from your last session";
        let mut v = open_remembered("claude", &rem("claude", true, true), h.path());
        ns(&mut v).focus = NewField::Kind;
        v.edit_new_session(b"\x1b[C");
        assert!(!ns(&mut v).bypass && !ns(&mut v).no_sandbox);
        assert!(!draw(&v, 100, 30).contains(line));
        v.edit_new_session(b"\x1b[D");
        assert!(!draw(&v, 100, 30).contains(line));
        flip(&mut v, NewField::Bypass);
        assert!(!draw(&v, 100, 30).contains(line));

        let mut v = open_remembered("claude", &rem("claude", true, true), h.path());
        flip(&mut v, NewField::Bypass);
        assert!(!ns(&mut v).bypass && ns(&mut v).no_sandbox);
        assert!(!draw(&v, 100, 30).contains(line));
        assert!(!on_cells_in_alert(&v, NewField::NoSandbox) || ns(&mut v).no_sandbox);

        let mut v = open_remembered("claude", &rem("claude", true, true), h.path());
        let row = row_of(&mut v, NewField::Kind);
        click(&mut v, row.x + LABEL_W + 2, row.y);
        assert!(!ns(&mut v).bypass && !draw(&v, 100, 30).contains(line));

        let mut v = open_remembered("claude", &rem("claude", true, false), h.path());
        let row = row_of(&mut v, NewField::Bypass);
        click(&mut v, row.x + LABEL_W + 2, row.y);
        assert!(!ns(&mut v).bypass && !draw(&v, 100, 30).contains(line));
    }

    #[test]
    fn the_note_goes_away_with_the_next_edit() {
        let h = tmp();
        let mut v = open_remembered("codex", &rem("claude", true, false), h.path());
        assert!(!hint(&v).is_empty());
        flip(&mut v, NewField::Worktree);
        assert_eq!(hint(&v), "");
    }

    #[test]
    fn remembered_worktree_cards_and_rewind_are_applied_only_when_set() {
        let h = tmp();
        let r = Remembered {
            worktree: Some(true),
            permission_cards: Some(true),
            checkpoints: Some(false),
            ..Remembered::default()
        };
        let mut v = ViewState::new(Vec::new(), "r");
        v.priv_gate = Some(test_gate);
        v.open_new_session("/".into(), kinds(), h.path().to_path_buf());
        v.apply_remembered(&r);
        assert!(ns(&mut v).worktree && ns(&mut v).permission_cards && !ns(&mut v).checkpoints);

        let mut v = ViewState::new(Vec::new(), "r");
        v.worktree_default = true;
        v.permission_cards_default = true;
        v.cp.form_default = true;
        v.open_new_session("/".into(), kinds(), h.path().to_path_buf());
        v.apply_remembered(&Remembered::default());
        assert!(ns(&mut v).worktree && ns(&mut v).permission_cards && ns(&mut v).checkpoints);
        v.apply_remembered(&Remembered {
            worktree: Some(false),
            permission_cards: Some(false),
            checkpoints: Some(false),
            ..Remembered::default()
        });
        assert!(!ns(&mut v).worktree && !ns(&mut v).permission_cards && !ns(&mut v).checkpoints);
    }

    #[test]
    fn a_hostile_agent_id_in_the_note_is_cleaned() {
        let h = tmp();
        let v = open_remembered("codex", &rem("cla\x1b[31mude\u{7}", true, false), h.path());
        let note = hint(&v);
        assert!(!note.chars().any(|c| c.is_control()), "{note:?}");
        assert!(note.starts_with("Remembered bypass is off: it was set for"));
        let shown = draw(&v, 100, 30);
        assert!(!shown.contains('\x1b'));
    }

    #[test]
    fn a_failed_start_reopens_with_the_flags_off_and_no_remembered_line() {
        let h = tmp();
        let v = open_remembered("claude", &rem("claude", true, true), h.path());
        let mut saved = v.new_session.clone().unwrap();
        saved.clear_privileges();
        let mut v = v;
        v.reopen_new_session(saved);
        assert!(!ns(&mut v).bypass && !ns(&mut v).no_sandbox);
        assert!(!draw(&v, 100, 30).contains("remembered from your last session"));
    }
}
