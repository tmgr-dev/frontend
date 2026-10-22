use super::title::{clean_title, clip, contains, open_under, LINE_CAP, PASS_CAP};
use super::view::ViewState;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use tmgr_ipc::api::StateKind;

const TAIL_BYTES: u64 = 1 << 20;
const MAX_PASSES: usize = 4;
const MAX_PENDING: usize = 32;
const ACTIVITY_MAX: usize = 80;
const SECRET_WORDS: [&str; 5] = ["KEY", "TOKEN", "SECRET", "PASSWORD", "PASSWD"];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionStatus {
    pub activity: Option<String>,
    pub todo: Option<(u32, u32)>,
    pub subs: Vec<SubAgent>,
    pub ask: Option<String>,
    pub model: Option<String>,
    pub ctx: Option<u8>,
    pub mode: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubAgent {
    pub id: String,
    pub label: String,
    pub started_s: u64,
    pub agent: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peek {
    pub session: String,
    pub sub: String,
    pub agent: Option<String>,
    pub title: String,
    pub lines: Vec<String>,
    pub back: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PeekEdit {
    Keep,
    Close,
}

const PEEK_TAIL: u64 = 256 * 1024;
const PEEK_LINES: usize = 400;
const PEEK_WIDTH: usize = 240;
const MAX_SUBS: usize = 16;
const STALE_S: u64 = 600;
const META_CAP: usize = 4096;

impl SessionStatus {
    pub fn is_empty(&self) -> bool {
        self.activity.is_none()
            && self.todo.is_none()
            && self.subs.is_empty()
            && self.ask.is_none()
            && self.model.is_none()
            && self.ctx.is_none()
            && self.mode.is_none()
    }

    pub fn todo_label(&self) -> Option<String> {
        self.todo
            .filter(|(done, total)| *total > 0 && done < total)
            .map(|(done, total)| format!("{done}/{total}"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusLine {
    pub text: String,
    pub clock: Option<String>,
}

impl ViewState {
    pub fn set_status(&mut self, mut found: HashMap<String, SessionStatus>) -> bool {
        found.retain(|id, _| self.sessions.iter().any(|r| &r.id == id));
        if self
            .peek
            .as_ref()
            .is_some_and(|p| !self.sessions.iter().any(|r| r.id == p.session))
        {
            self.peek = None;
        }
        if found == self.status {
            return false;
        }
        self.subs_open
            .retain(|id| found.get(id).is_some_and(|s| !s.subs.is_empty()));
        self.status = found;
        true
    }

    pub fn subs_of(&self, id: &str) -> Vec<&SubAgent> {
        let working = self
            .sessions
            .iter()
            .any(|r| r.id == id && r.state.kind == StateKind::Working);
        self.status.get(id).map_or_else(Vec::new, |s| {
            s.subs
                .iter()
                .filter(|sub| sub.agent.is_some() || working)
                .collect()
        })
    }

    pub fn toggle_subs(&mut self, id: &str) {
        if self.subs_of(id).is_empty() {
            self.notice = Some("no subagents running".into());
            return;
        }
        if !self.subs_open.remove(id) {
            self.subs_open.insert(id.to_string());
        }
    }

    pub fn sub_elapsed(&self, sub: &SubAgent) -> String {
        if self.now_s == 0 || sub.started_s == 0 {
            return String::new();
        }
        let secs = self.now_s.saturating_sub(sub.started_s);
        if secs < 3600 {
            format!("{}:{:02}", secs / 60, secs % 60)
        } else {
            format!("{}h", secs / 3600)
        }
    }

    pub fn open_peek(&mut self, session: &str, sub: &str) {
        let subs = self.subs_of(session);
        let Some(found) = subs.iter().find(|s| s.id == sub) else {
            return;
        };
        let (agent, title) = (found.agent.clone(), found.label.clone());
        self.peek = Some(Peek {
            session: session.to_string(),
            sub: sub.to_string(),
            agent,
            title,
            lines: Vec::new(),
            back: 0,
        });
    }

    pub fn set_peek(&mut self, session: &str, sub: &str, lines: Vec<String>) -> bool {
        match self.peek.as_mut() {
            Some(p) if p.session == session && p.sub == sub && p.lines != lines => {
                p.lines = lines;
                true
            }
            _ => false,
        }
    }

    pub fn edit_peek(&mut self, data: &[u8]) -> PeekEdit {
        let Some(p) = self.peek.as_mut() else {
            return PeekEdit::Close;
        };
        let top = p.lines.len().saturating_sub(1);
        let mut i = 0;
        while i < data.len() {
            let rest = &data[i..];
            let (used, delta): (usize, i64) =
                if rest.starts_with(b"\x1b[A") || rest.starts_with(b"\x1bOA") {
                    (3, 1)
                } else if rest.starts_with(b"\x1b[B") || rest.starts_with(b"\x1bOB") {
                    (3, -1)
                } else if rest.starts_with(b"\x1b[5~") {
                    (4, 10)
                } else if rest.starts_with(b"\x1b[6~") {
                    (4, -10)
                } else if data.iter().all(|b| *b == 0x1b) {
                    self.peek = None;
                    return PeekEdit::Close;
                } else {
                    let b = rest[0];
                    match b {
                        b'q' => {
                            self.peek = None;
                            return PeekEdit::Close;
                        }
                        b'k' | 0x10 => (1, 1),
                        b'j' | 0x0e | b'\r' => (1, -1),
                        b' ' => (1, -10),
                        b'g' => (1, i64::MAX / 2),
                        b'G' => (1, i64::MIN / 2),
                        _ => (1, 0),
                    }
                };
            let next = (p.back as i64).saturating_add(delta).clamp(0, top as i64);
            p.back = next as usize;
            i += used;
        }
        PeekEdit::Keep
    }

    pub fn status_line(&self, id: &str) -> Option<StatusLine> {
        let row = self.sessions.iter().find(|r| r.id == id)?;
        let st = self.status.get(id)?;
        let working = row.state.kind == StateKind::Working;
        let activity = if working {
            Some(st.activity.clone().unwrap_or_else(|| "thinking".into()))
        } else {
            None
        };
        let text = match (st.todo_label(), activity) {
            (Some(todo), Some(a)) => format!("{todo} · {a}"),
            (Some(todo), None) => todo,
            (None, Some(a)) => a,
            (None, None) if !self.subs_of(id).is_empty() => "subagents running".to_string(),
            (None, None) => return None,
        };
        let clock = (working && row.state.since_ms > 0 && self.now_s > 0)
            .then(|| clock_label(self.now_s.saturating_sub(row.state.since_ms / 1000)));
        Some(StatusLine { text, clock })
    }
}

pub(crate) fn model_label(id: &str) -> String {
    let lower = id.to_ascii_lowercase();
    for family in ["opus", "sonnet", "haiku"] {
        if lower.contains(family) {
            return family.to_string();
        }
    }
    let short: String = lower
        .trim_start_matches("claude-")
        .chars()
        .take_while(|c| c.is_ascii_alphabetic() || *c == '-')
        .collect();
    let short = short.trim_end_matches('-');
    if short.is_empty() {
        "?".into()
    } else {
        short.chars().take(10).collect()
    }
}

pub(crate) fn mode_label(mode: &str) -> String {
    match mode {
        "bypassPermissions" => "bypass".into(),
        "acceptEdits" => "edits".into(),
        "default" => "ask".into(),
        other => other
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(10)
            .collect::<String>()
            .to_ascii_lowercase(),
    }
}

fn ctx_percent(tokens: u64) -> u8 {
    const SMALL: u64 = 200_000;
    const LARGE: u64 = 1_000_000;
    let window = if tokens > SMALL { LARGE } else { SMALL };
    ((tokens * 100 + window / 2) / window).min(100) as u8
}

fn clock_label(secs: u64) -> String {
    match secs {
        0..=3599 => format!("{}:{:02}", secs / 60, secs % 60),
        _ => format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60),
    }
}

#[derive(Debug)]
pub struct StatusScan {
    root: PathBuf,
    path: PathBuf,
    ident: Option<(u64, u64, i64, i64)>,
    windowed: bool,
    last_message: Option<String>,
    created: u32,
    completed: HashSet<String>,
    offset: u64,
    carry: Vec<u8>,
    skipping: bool,
    pending: Vec<(String, String)>,
    todo: Option<(u32, u32)>,
    subs: Vec<Sub>,
    asking: Option<(String, String)>,
    model: Option<String>,
    ctx: Option<u8>,
    mode: Option<String>,
}

#[derive(Debug)]
struct Sub {
    id: String,
    label: String,
    started_s: u64,
    agent: Option<String>,
}

impl StatusScan {
    pub fn new(root: PathBuf, path: PathBuf) -> StatusScan {
        StatusScan {
            root,
            path,
            ident: None,
            windowed: false,
            last_message: None,
            created: 0,
            completed: HashSet::new(),
            offset: 0,
            carry: Vec::new(),
            skipping: false,
            pending: Vec::new(),
            todo: None,
            subs: Vec::new(),
            asking: None,
            model: None,
            ctx: None,
            mode: None,
        }
    }

    pub fn is_for(&self, path: &Path) -> bool {
        self.path == path
    }

    pub fn status(&self) -> SessionStatus {
        SessionStatus {
            activity: self.pending.last().map(|(_, label)| label.clone()),
            todo: self.todo,
            ask: self.asking.as_ref().map(|(_, q)| q.clone()),
            model: self.model.clone(),
            ctx: self.ctx,
            mode: self.mode.clone(),
            subs: self
                .subs
                .iter()
                .filter(|s| !self.stale(s))
                .map(|s| SubAgent {
                    id: s.id.clone(),
                    label: s.label.clone(),
                    started_s: s.started_s,
                    agent: s.agent.clone(),
                })
                .collect(),
        }
    }

    fn stale(&self, sub: &Sub) -> bool {
        let Some(agent) = sub.agent.as_deref() else {
            return false;
        };
        let file = self
            .path
            .with_extension("")
            .join("subagents")
            .join(format!("agent-{agent}.jsonl"));
        let Ok(meta) = std::fs::symlink_metadata(&file) else {
            return sub.started_s > 0 && now_secs().saturating_sub(sub.started_s) > STALE_S;
        };
        meta.modified()
            .ok()
            .and_then(|m| m.elapsed().ok())
            .is_some_and(|age| age.as_secs() > STALE_S)
    }

    pub fn poll(&mut self) {
        for _ in 0..MAX_PASSES {
            if !self.pass() {
                return;
            }
        }
    }

    fn reset(&mut self) {
        self.offset = 0;
        self.carry.clear();
        self.skipping = false;
        self.pending.clear();
        self.todo = None;
        self.windowed = false;
        self.asking = None;
        self.subs.clear();
        self.model = None;
        self.ctx = None;
        self.mode = None;
        self.last_message = None;
        self.created = 0;
        self.completed.clear();
    }

    fn pass(&mut self) -> bool {
        let Some((mut file, meta)) = open_under(&self.root, &self.path) else {
            return false;
        };
        let ident = (meta.dev(), meta.ino(), meta.mtime(), meta.mtime_nsec());
        let rewritten = self.ident.is_some_and(|i| (i.0, i.1) != (ident.0, ident.1))
            || meta.len() < self.offset
            || (meta.len() == self.offset && self.ident.is_some_and(|i| i != ident));
        if rewritten {
            self.reset();
        }
        self.ident = Some(ident);
        if self.offset == 0 && meta.len() > TAIL_BYTES {
            self.offset = meta.len() - TAIL_BYTES;
            self.skipping = true;
            self.windowed = true;
        }
        if meta.len() == self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return false;
        }
        let mut buf = Vec::new();
        if file.take(PASS_CAP).read_to_end(&mut buf).is_err() {
            return false;
        }
        self.offset += buf.len() as u64;
        self.feed(&buf);
        buf.len() as u64 == PASS_CAP
    }

    fn feed(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        while let Some(nl) = rest.iter().position(|b| *b == b'\n') {
            let (head, tail) = rest.split_at(nl);
            rest = &tail[1..];
            if self.skipping {
                self.skipping = false;
                self.carry.clear();
                continue;
            }
            if self.carry.len() + head.len() <= LINE_CAP {
                let mut line = std::mem::take(&mut self.carry);
                line.extend_from_slice(head);
                self.line(&line);
            }
            self.carry.clear();
        }
        if self.skipping {
            return;
        }
        if self.carry.len() + rest.len() > LINE_CAP {
            self.skipping = true;
            self.carry.clear();
        } else {
            self.carry.extend_from_slice(rest);
        }
    }

    fn notified(&mut self, line: &[u8]) {
        let text = String::from_utf8_lossy(line);
        for tag in ["tool-use-id", "task-id"] {
            let open = format!("<{tag}>");
            let close = format!("</{tag}>");
            for part in text.split(&open).skip(1) {
                if let Some(id) = part.split(&close).next() {
                    self.subs
                        .retain(|s| s.id != id && s.agent.as_deref() != Some(id));
                }
            }
        }
    }

    fn line(&mut self, line: &[u8]) {
        if !self.subs.is_empty() && contains(line, b"<task-notification>") {
            self.notified(line);
        }
        if contains(line, b"\"type\":\"permission-mode\"") && self.permission(line) {
            return;
        }
        if !contains(line, b"\"type\":\"assistant\"") && !contains(line, b"\"type\":\"user\"") {
            return;
        }
        let Ok(v) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        if v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            return;
        }
        let Some(content) = v.get("message").and_then(|m| m.get("content")) else {
            return;
        };
        match v.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                let message = v
                    .get("message")
                    .and_then(|m| m.get("id"))
                    .and_then(Value::as_str);
                let at = v
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .and_then(iso_secs);
                self.meter(v.get("message"));
                self.assistant(message, at.unwrap_or(0), content);
            }
            Some("user") => self.user(&v, content),
            _ => {}
        }
    }

    fn permission(&mut self, line: &[u8]) -> bool {
        let Ok(v) = serde_json::from_slice::<Value>(line) else {
            return false;
        };
        if v.get("type").and_then(Value::as_str) != Some("permission-mode") {
            return false;
        }
        if let Some(mode) = v.get("permissionMode").and_then(Value::as_str) {
            self.mode = Some(mode_label(mode));
        }
        true
    }

    fn meter(&mut self, message: Option<&Value>) {
        let Some(message) = message else {
            return;
        };
        let model = message.get("model").and_then(Value::as_str).unwrap_or("");
        if model.is_empty() || model.starts_with('<') {
            return;
        }
        self.model = Some(model_label(model));
        let Some(usage) = message.get("usage") else {
            return;
        };
        let tokens: u64 = [
            "input_tokens",
            "cache_read_input_tokens",
            "cache_creation_input_tokens",
        ]
        .iter()
        .filter_map(|k| usage.get(k).and_then(Value::as_u64))
        .sum();
        if tokens > 0 {
            self.ctx = Some(ctx_percent(tokens));
        }
    }

    fn assistant(&mut self, message: Option<&str>, at: u64, content: &Value) {
        let Some(blocks) = content.as_array() else {
            return;
        };
        if message.is_some() && self.last_message.as_deref() != message {
            self.pending.clear();
            self.last_message = message.map(str::to_string);
        }
        for b in blocks {
            if b.get("type").and_then(Value::as_str) != Some("tool_use") {
                continue;
            }
            let (Some(id), Some(name)) = (
                b.get("id").and_then(Value::as_str),
                b.get("name").and_then(Value::as_str),
            ) else {
                continue;
            };
            let input = b.get("input").unwrap_or(&Value::Null);
            if name == "AskUserQuestion" {
                let question = input
                    .get("questions")
                    .and_then(Value::as_array)
                    .and_then(|q| q.first())
                    .and_then(|q| q.get("question"))
                    .and_then(Value::as_str)
                    .map(clean_title)
                    .filter(|q| !q.is_empty())
                    .unwrap_or_else(|| "asked a question".to_string());
                self.asking = Some((id.to_string(), question));
            }
            if matches!(name, "Agent" | "Task") {
                self.subs.push(Sub {
                    id: id.to_string(),
                    label: sub_label(input),
                    started_s: at,
                    agent: None,
                });
                if self.subs.len() > MAX_SUBS {
                    self.subs.remove(0);
                }
            }
            match name {
                "TodoWrite" => self.todo = todo_counts(input),
                "TaskCreate" if !self.windowed => {
                    self.created += 1;
                    self.todo = Some((self.completed.len() as u32, self.created));
                }
                "TaskUpdate" if !self.windowed => self.task_update(input),
                _ => {}
            }
            self.pending
                .push((id.to_string(), activity_label(name, input)));
            if self.pending.len() > MAX_PENDING {
                self.pending.remove(0);
            }
        }
    }

    fn human(&mut self) {
        self.pending.clear();
        self.asking = None;
        self.subs.retain(|s| s.agent.is_some());
    }

    fn task_update(&mut self, input: &Value) {
        let Some(id) = input.get("taskId").map(|i| match i {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }) else {
            return;
        };
        match input.get("status").and_then(Value::as_str) {
            Some("completed") => {
                self.completed.insert(id);
            }
            Some("deleted") => {
                self.completed.remove(&id);
                self.created = self.created.saturating_sub(1);
            }
            _ => return,
        }
        self.todo = Some((self.completed.len() as u32, self.created));
    }

    fn user(&mut self, v: &Value, content: &Value) {
        if v.get("isMeta").and_then(Value::as_bool) == Some(true) {
            return;
        }
        let Some(blocks) = content.as_array() else {
            self.human();
            return;
        };
        let results: Vec<&str> = blocks
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
            .filter_map(|b| b.get("tool_use_id").and_then(Value::as_str))
            .collect();
        if results.is_empty() {
            self.human();
            return;
        }
        self.pending
            .retain(|(id, _)| !results.contains(&id.as_str()));
        if self
            .asking
            .as_ref()
            .is_some_and(|(id, _)| results.contains(&id.as_str()))
        {
            self.asking = None;
        }
        let launched = v
            .get("toolUseResult")
            .filter(|r| r.get("status").and_then(Value::as_str) == Some("async_launched"))
            .and_then(|r| r.get("agentId").and_then(Value::as_str));
        self.subs.retain_mut(|s| {
            if !results.contains(&s.id.as_str()) {
                return true;
            }
            s.agent = launched.map(str::to_string);
            launched.is_some()
        });
    }
}

fn todo_counts(input: &Value) -> Option<(u32, u32)> {
    let todos = input.get("todos")?.as_array()?;
    let done = todos
        .iter()
        .filter(|t| t.get("status").and_then(Value::as_str) == Some("completed"))
        .count();
    Some((done as u32, todos.len() as u32))
}

fn str_of<'a>(input: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|k| input.get(*k).and_then(Value::as_str))
        .filter(|s| !s.trim().is_empty())
}

fn secret_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    SECRET_WORDS.iter().any(|w| upper.contains(w))
}

fn secret_literal(word: &str) -> bool {
    let w = word.trim_matches(|c| c == '\'' || c == '"');
    ["sk-", "ghp_", "gho_", "ghs_", "github_pat_", "xox", "AKIA"]
        .iter()
        .any(|p| w.starts_with(p))
}

fn mask_userinfo(word: &str) -> Option<String> {
    let (scheme, rest) = word.split_once("://")?;
    let (userinfo, host) = rest.split_once('@')?;
    (!userinfo.contains('/')).then(|| format!("{scheme}://…@{host}"))
}

pub fn mask_secrets(text: &str) -> String {
    let mut out = Vec::new();
    let mut mask_next = 0;
    for word in text.split(' ') {
        if mask_next > 0 && !word.is_empty() {
            mask_next -= 1;
            out.push("…".to_string());
            continue;
        }
        let lower = word.to_ascii_lowercase();
        if lower == "authorization:" {
            mask_next = 2;
        } else if lower.ends_with("key:")
            || lower.ends_with("token:")
            || matches!(lower.as_str(), "-u" | "--user" | "-p" | "--pass")
            || (lower.starts_with("--") && secret_name(&lower) && !lower.contains('='))
        {
            mask_next = 1;
        }
        out.push(match word.split_once('=') {
            Some((name, _)) if !name.is_empty() && secret_name(name) => format!("{name}=…"),
            _ if secret_literal(word) => "…".to_string(),
            _ => mask_userinfo(word).unwrap_or_else(|| word.to_string()),
        });
    }
    out.join(" ")
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn sub_label(input: &Value) -> String {
    let kind = str_of(input, &["subagent_type"]).unwrap_or("agent");
    let what = str_of(input, &["description"]).unwrap_or("");
    let line = format!(
        "{}: {}",
        clean_title(kind),
        mask_secrets(&clean_title(what))
    );
    clip(line.trim_end_matches([':', ' ']), ACTIVITY_MAX)
}

fn iso_secs(ts: &str) -> Option<u64> {
    let b = ts.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[10] != b'T' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| ts.get(r)?.parse::<i64>().ok();
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let (hh, mm, ss) = (n(11..13)?, n(14..16)?, n(17..19)?);
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + hh * 3600 + mm * 60 + ss).ok()
}

fn meta_agent(root: &Path, dir: &Path, tool_use_id: &str, agent: Option<&str>) -> Option<PathBuf> {
    let matches = |meta: &Path| -> bool {
        let Some((file, _)) = open_under(root, meta) else {
            return false;
        };
        let mut buf = Vec::new();
        if file.take(8192).read_to_end(&mut buf).is_err() {
            return false;
        }
        serde_json::from_slice::<Value>(&buf)
            .ok()
            .is_some_and(|v| v.get("toolUseId").and_then(Value::as_str) == Some(tool_use_id))
    };
    if let Some(agent) = agent {
        let known = dir.join(format!("agent-{agent}.jsonl"));
        return Some(known);
    }
    let rd = std::fs::read_dir(dir).ok()?;
    let names = rd
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".meta.json"))
        .take(META_CAP);
    for name in names {
        if matches(&dir.join(&name)) {
            let stem = name.trim_end_matches(".meta.json");
            return Some(dir.join(format!("{stem}.jsonl")));
        }
    }
    None
}

pub fn peek_lines(
    root: &Path,
    transcript: &Path,
    tool_use_id: &str,
    agent: Option<&str>,
) -> Vec<String> {
    let dir = transcript.with_extension("").join("subagents");
    let Some(path) = meta_agent(root, &dir, tool_use_id, agent) else {
        return Vec::new();
    };
    let Some((mut file, meta)) = open_under(root, &path) else {
        return Vec::new();
    };
    let start = meta.len().saturating_sub(PEEK_TAIL);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut buf = Vec::new();
    if file.take(PEEK_TAIL).read_to_end(&mut buf).is_err() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut rows = buf.split(|b| *b == b'\n');
    if start > 0 {
        rows.next();
    }
    for row in rows.filter(|r| r.len() <= LINE_CAP) {
        let Ok(v) = serde_json::from_slice::<Value>(row) else {
            continue;
        };
        peek_entry(&v, &mut out);
    }
    let from = out.len().saturating_sub(PEEK_LINES);
    out.split_off(from)
}

fn peek_entry(v: &Value, out: &mut Vec<String>) {
    let Some(content) = v.get("message").and_then(|m| m.get("content")) else {
        return;
    };
    let mut push = |prefix: &str, text: &str| {
        let line = mask_secrets(&clean_title(text));
        if !line.is_empty() {
            out.push(clip(&format!("{prefix}{line}"), PEEK_WIDTH));
        }
    };
    match (v.get("type").and_then(Value::as_str), content) {
        (Some("user"), Value::String(s)) => push("prompt: ", s),
        (Some("assistant"), Value::Array(blocks)) => {
            for b in blocks {
                match b.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        let text = b.get("text").and_then(Value::as_str).unwrap_or("");
                        for l in text.lines().take(12) {
                            push("", l);
                        }
                    }
                    Some("tool_use") => {
                        let name = b.get("name").and_then(Value::as_str).unwrap_or("tool");
                        let input = b.get("input").unwrap_or(&Value::Null);
                        push("> ", &activity_label(name, input));
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

pub fn activity_label(name: &str, input: &Value) -> String {
    let base = |p: &str| p.rsplit('/').next().unwrap_or(p).to_string();
    let (shown, arg) = match name {
        "Bash" => (
            "Bash",
            str_of(input, &["command"])
                .and_then(|c| c.lines().find(|l| !l.trim().is_empty()))
                .map(str::to_string),
        ),
        "Read" | "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => (
            name,
            str_of(input, &["file_path", "notebook_path"]).map(base),
        ),
        "Grep" | "Glob" => (name, str_of(input, &["pattern"]).map(str::to_string)),
        "WebFetch" => (name, str_of(input, &["url"]).map(str::to_string)),
        "WebSearch" => (name, str_of(input, &["query"]).map(str::to_string)),
        "Agent" | "Task" => ("Agent", str_of(input, &["description"]).map(str::to_string)),
        "TodoWrite" => ("Todos", None),
        other => (other.rsplit("__").next().unwrap_or(other), None),
    };
    let head = clean_title(shown);
    let line = match arg {
        Some(a) => format!("{head}: {}", mask_secrets(&clean_title(&a))),
        None => head,
    };
    clip(&line, ACTIVITY_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scan(dir: &Path, lines: &[&str]) -> StatusScan {
        let path = dir.join("t.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        let mut s = StatusScan::new(dir.to_path_buf(), path);
        s.poll();
        s
    }

    fn tool(id: &str, name: &str, input: &str) -> String {
        format!(
            r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"{id}","name":"{name}","input":{input}}}]}}}}"#
        )
    }

    fn result(id: &str) -> String {
        format!(
            r#"{{"type":"user","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"{id}","content":"ok"}}]}}}}"#
        )
    }

    const HUMAN: &str = r#"{"type":"user","message":{"role":"user","content":"go"}}"#;

    #[test]
    fn a_running_tool_is_the_activity() {
        let dir = tempfile::tempdir().unwrap();
        let s = scan(
            dir.path(),
            &[
                HUMAN,
                &tool("a", "Bash", r#"{"command":"cargo test\nsecond"}"#),
            ],
        );
        assert_eq!(s.status().activity.as_deref(), Some("Bash: cargo test"));
    }

    #[test]
    fn a_finished_tool_clears_the_activity() {
        let dir = tempfile::tempdir().unwrap();
        let s = scan(
            dir.path(),
            &[
                HUMAN,
                &tool("a", "Bash", r#"{"command":"ls"}"#),
                &result("a"),
            ],
        );
        assert_eq!(s.status().activity, None);
    }

    #[test]
    fn parallel_tools_show_the_latest_unfinished_one() {
        let dir = tempfile::tempdir().unwrap();
        let s = scan(
            dir.path(),
            &[
                HUMAN,
                &tool("a", "Read", r#"{"file_path":"/x/y/api.rs"}"#),
                &tool("b", "Edit", r#"{"file_path":"/x/y/lib.rs"}"#),
                &result("b"),
            ],
        );
        assert_eq!(s.status().activity.as_deref(), Some("Read: api.rs"));
    }

    #[test]
    fn a_new_human_message_clears_a_stale_tool() {
        let dir = tempfile::tempdir().unwrap();
        let s = scan(
            dir.path(),
            &[HUMAN, &tool("a", "Bash", r#"{"command":"sleep 9"}"#), HUMAN],
        );
        assert_eq!(s.status().activity, None);
    }

    #[test]
    fn sidechain_lines_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let side = tool("a", "Bash", r#"{"command":"ls"}"#).replace(
            r#"{"type":"assistant""#,
            r#"{"isSidechain":true,"type":"assistant""#,
        );
        let s = scan(dir.path(), &[HUMAN, &side]);
        assert_eq!(s.status().activity, None);
    }

    #[test]
    fn todo_write_gives_progress() {
        let dir = tempfile::tempdir().unwrap();
        let todos = r#"{"todos":[{"content":"a","status":"completed"},{"content":"b","status":"in_progress"},{"content":"c","status":"pending"}]}"#;
        let s = scan(
            dir.path(),
            &[HUMAN, &tool("t", "TodoWrite", todos), &result("t")],
        );
        let st = s.status();
        assert_eq!(st.todo, Some((1, 3)));
        assert_eq!(st.todo_label().as_deref(), Some("1/3"));
    }

    fn turn(model: &str, input: u64, read: u64, create: u64) -> String {
        format!(
            r#"{{"type":"assistant","message":{{"id":"m","role":"assistant","model":"{model}","content":[{{"type":"text","text":"ok"}}],"usage":{{"input_tokens":{input},"cache_read_input_tokens":{read},"cache_creation_input_tokens":{create}}}}}}}"#
        )
    }

    const BYPASS: &str =
        r#"{"type":"permission-mode","permissionMode":"bypassPermissions","sessionId":"x"}"#;

    #[test]
    fn model_context_and_mode_come_from_the_latest_lines() {
        let dir = tempfile::tempdir().unwrap();
        let s = scan(
            dir.path(),
            &[
                BYPASS,
                HUMAN,
                &turn("claude-opus-5-5", 2, 100_000, 34_000),
                r#"{"type":"permission-mode","permissionMode":"plan"}"#,
            ],
        );
        let st = s.status();
        assert_eq!(st.model.as_deref(), Some("opus"));
        assert_eq!(st.ctx, Some(67));
        assert_eq!(st.mode.as_deref(), Some("plan"));
        assert!(!st.is_empty());
    }

    #[test]
    fn a_context_over_200k_is_measured_against_a_million() {
        let dir = tempfile::tempdir().unwrap();
        let s = scan(dir.path(), &[&turn("claude-opus-5-5", 2, 295_000, 1_000)]);
        assert_eq!(s.status().ctx, Some(30));
    }

    #[test]
    fn synthetic_turns_and_sidechains_do_not_set_the_meter() {
        let dir = tempfile::tempdir().unwrap();
        let side = turn("claude-haiku-4-5", 1, 1, 1).replace(
            r#"{"type":"assistant""#,
            r#"{"isSidechain":true,"type":"assistant""#,
        );
        let s = scan(
            dir.path(),
            &[
                &turn("claude-sonnet-5", 1, 20_000, 0),
                &turn("<synthetic>", 0, 0, 0),
                &side,
            ],
        );
        let st = s.status();
        assert_eq!(st.model.as_deref(), Some("sonnet"));
        assert_eq!(st.ctx, Some(10));
    }

    #[test]
    fn labels_for_models_and_modes() {
        assert_eq!(model_label("claude-3-5-haiku-20241022"), "haiku");
        assert_eq!(model_label("claude-foo-9"), "foo");
        assert_eq!(model_label("???"), "?");
        assert_eq!(mode_label("bypassPermissions"), "bypass");
        assert_eq!(mode_label("acceptEdits"), "edits");
        assert_eq!(mode_label("default"), "ask");
        assert_eq!(mode_label("auto"), "auto");
    }

    #[test]
    fn a_finished_todo_list_shows_nothing() {
        let st = SessionStatus {
            activity: None,
            todo: Some((3, 3)),
            ..Default::default()
        };
        assert_eq!(st.todo_label(), None);
    }

    #[test]
    fn labels_by_tool() {
        let l = |n: &str, i: &str| activity_label(n, &serde_json::from_str(i).unwrap());
        assert_eq!(l("Grep", r#"{"pattern":"fn main"}"#), "Grep: fn main");
        assert_eq!(
            l("Agent", r#"{"description":"find auth routes"}"#),
            "Agent: find auth routes"
        );
        assert_eq!(l("mcp__tmgr__get_task", "{}"), "get_task");
        assert_eq!(l("Bash", "{}"), "Bash");
    }

    #[test]
    fn terminal_escapes_and_secret_assignments_are_removed() {
        let l = activity_label(
            "Bash",
            &serde_json::json!({"command": "\u{1b}[31mAPI_TOKEN=abc123 curl x"}),
        );
        assert_eq!(l, "Bash: API_TOKEN=… curl x");
        assert!(!l.contains('\u{1b}'));
    }

    #[test]
    fn common_secret_forms_are_masked() {
        let m = |c: &str| activity_label("Bash", &serde_json::json!({ "command": c }));
        assert_eq!(
            m("tool --password hunter2 run"),
            "Bash: tool --password … run"
        );
        assert_eq!(
            m("git clone https://me:pat123@host/x.git"),
            "Bash: git clone https://…@host/x.git"
        );
        assert_eq!(
            m("curl -H Authorization: Bearer abc x"),
            "Bash: curl -H Authorization: … … x"
        );
        assert_eq!(
            m("curl -H X-Api-Key: abc x"),
            "Bash: curl -H X-Api-Key: … x"
        );
        assert_eq!(m("echo ghp_abcdef"), "Bash: echo …");
        assert_eq!(m("curl --user a x"), "Bash: curl --user … x");
    }

    #[test]
    fn a_new_assistant_message_drops_tools_whose_result_never_arrived() {
        let dir = tempfile::tempdir().unwrap();
        let first = tool("a", "Bash", r#"{"command":"ls"}"#)
            .replace(r#""role":"assistant""#, r#""id":"m1","role":"assistant""#);
        let second = tool("b", "Read", r#"{"file_path":"/x/y.rs"}"#)
            .replace(r#""role":"assistant""#, r#""id":"m2","role":"assistant""#);
        let s = scan(dir.path(), &[HUMAN, &first, &second]);
        assert_eq!(s.status().activity.as_deref(), Some("Read: y.rs"));
        let s2 = scan(dir.path(), &[HUMAN, &first, &second, &result("b")]);
        assert_eq!(s2.status().activity, None);
    }

    #[test]
    fn task_tools_give_progress() {
        let dir = tempfile::tempdir().unwrap();
        let s = scan(
            dir.path(),
            &[
                HUMAN,
                &tool("1", "TaskCreate", r#"{"subject":"a"}"#),
                &tool("2", "TaskCreate", r#"{"subject":"b"}"#),
                &tool("3", "TaskCreate", r#"{"subject":"c"}"#),
                &tool("4", "TaskUpdate", r#"{"taskId":"1","status":"completed"}"#),
                &tool(
                    "5",
                    "TaskUpdate",
                    r#"{"taskId":"2","status":"in_progress"}"#,
                ),
            ],
        );
        assert_eq!(s.status().todo, Some((1, 3)));
    }

    fn agent_call(id: &str, kind: &str, what: &str, ts: &str) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"{id}","name":"Agent","input":{{"subagent_type":"{kind}","description":"{what}"}}}}]}}}}"#
        )
    }

    fn launched(id: &str, agent: &str) -> String {
        format!(
            r#"{{"type":"user","toolUseResult":{{"status":"async_launched","agentId":"{agent}"}},"message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"{id}","content":"started"}}]}}}}"#
        )
    }

    fn live_agent(dir: &Path, agent: &str) {
        let sub_dir = dir.join("t").join("subagents");
        std::fs::create_dir_all(&sub_dir).unwrap();
        std::fs::write(sub_dir.join(format!("agent-{agent}.jsonl")), "{}\n").unwrap();
    }

    #[test]
    fn a_foreground_agent_runs_until_its_result() {
        let dir = tempfile::tempdir().unwrap();
        let call = agent_call(
            "a",
            "Explore",
            "find auth routes",
            "2026-10-21T20:00:00.000Z",
        );
        let s = scan(dir.path(), &[HUMAN, &call]);
        let subs = s.status().subs;
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].label, "Explore: find auth routes");
        assert_eq!(subs[0].started_s, 1_792_612_800);
        let s = scan(dir.path(), &[HUMAN, &call, &result("a")]);
        assert!(s.status().subs.is_empty());
    }

    #[test]
    fn a_background_agent_runs_until_its_notification() {
        let dir = tempfile::tempdir().unwrap();
        let call = agent_call(
            "a",
            "general-purpose",
            "big job",
            "2026-10-21T20:00:00.000Z",
        );
        let note = r#"{"type":"attachment","attachment":{"prompt":"<task-notification>\n<task-id>ag1</task-id>\n<tool-use-id>a</tool-use-id>\n<status>completed</status>\n</task-notification>"}}"#;
        live_agent(dir.path(), "ag1");
        let s = scan(dir.path(), &[HUMAN, &call, &launched("a", "ag1"), HUMAN]);
        assert_eq!(s.status().subs.len(), 1);
        let s = scan(dir.path(), &[HUMAN, &call, &launched("a", "ag1"), note]);
        assert!(s.status().subs.is_empty());
    }

    #[test]
    fn a_new_human_message_drops_foreground_agents_only() {
        let dir = tempfile::tempdir().unwrap();
        let fg = agent_call("f", "Plan", "plan it", "2026-10-21T20:00:00.000Z");
        let bg = agent_call("b", "Explore", "scan", "2026-10-21T20:00:01.000Z");
        live_agent(dir.path(), "ag2");
        let s = scan(dir.path(), &[HUMAN, &fg, &bg, &launched("b", "ag2"), HUMAN]);
        let subs = s.status().subs;
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].id, "b");
    }

    #[test]
    fn a_silent_background_agent_expires() {
        let dir = tempfile::tempdir().unwrap();
        let call = agent_call("a", "Explore", "scan", "2026-10-21T20:00:00.000Z");
        let s = scan(dir.path(), &[HUMAN, &call, &launched("a", "ag3")]);
        let sub_dir = dir.path().join("t").join("subagents");
        std::fs::create_dir_all(&sub_dir).unwrap();
        let file = sub_dir.join("agent-ag3.jsonl");
        std::fs::write(&file, "{}\n").unwrap();
        assert_eq!(s.status().subs.len(), 1);
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(STALE_S + 60);
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(old)
            .unwrap();
        assert!(s.status().subs.is_empty());
    }

    #[test]
    fn iso_timestamps_parse_to_epoch_seconds() {
        assert_eq!(iso_secs("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(iso_secs("2024-02-29T12:00:01Z"), Some(1_709_208_001));
        assert_eq!(iso_secs("garbage"), None);
    }

    #[test]
    fn peek_reads_the_subagent_transcript_found_by_its_meta_file() {
        let dir = tempfile::tempdir().unwrap();
        let transcript = dir.path().join("t.jsonl");
        std::fs::write(&transcript, "").unwrap();
        let sub_dir = dir.path().join("t").join("subagents");
        std::fs::create_dir_all(&sub_dir).unwrap();
        std::fs::write(sub_dir.join("agent-x1.meta.json"), r#"{"toolUseId":"a"}"#).unwrap();
        std::fs::write(
            sub_dir.join("agent-x2.meta.json"),
            r#"{"toolUseId":"other"}"#,
        )
        .unwrap();
        let body = [
            r#"{"type":"user","message":{"role":"user","content":"find the \u001b[31mauth routes"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Looking.\nSecond line"},{"type":"tool_use","id":"t1","name":"Grep","input":{"pattern":"route"}}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"secret result"}]}}"#,
        ]
        .join("\n");
        std::fs::write(sub_dir.join("agent-x1.jsonl"), body).unwrap();
        let lines = peek_lines(dir.path(), &transcript, "a", None);
        assert_eq!(
            lines,
            vec![
                "prompt: find the auth routes".to_string(),
                "Looking.".into(),
                "Second line".into(),
                "> Grep: route".into(),
            ]
        );
        assert!(peek_lines(dir.path(), &transcript, "missing", None).is_empty());
    }

    #[test]
    fn peek_finds_the_file_by_agent_id_and_in_a_crowded_directory() {
        let dir = tempfile::tempdir().unwrap();
        let transcript = dir.path().join("t.jsonl");
        std::fs::write(&transcript, "").unwrap();
        let sub_dir = dir.path().join("t").join("subagents");
        std::fs::create_dir_all(&sub_dir).unwrap();
        for i in 0..600 {
            std::fs::write(
                sub_dir.join(format!("agent-n{i}.meta.json")),
                format!(r#"{{"toolUseId":"n{i}"}}"#),
            )
            .unwrap();
            std::fs::write(sub_dir.join(format!("agent-n{i}.jsonl")), "").unwrap();
        }
        let line = r#"{"type":"user","message":{"role":"user","content":"hello"}}"#;
        std::fs::write(sub_dir.join("agent-n599.jsonl"), line).unwrap();
        let by_scan = peek_lines(dir.path(), &transcript, "n599", None);
        let by_id = peek_lines(dir.path(), &transcript, "unused", Some("n599"));
        assert_eq!(by_scan, vec!["prompt: hello".to_string()]);
        assert_eq!(by_id, by_scan);
    }

    #[test]
    fn a_background_agent_without_a_file_expires_after_its_start() {
        let dir = tempfile::tempdir().unwrap();
        let old = agent_call("a", "Explore", "scan", "2020-01-01T00:00:00.000Z");
        let s = scan(dir.path(), &[HUMAN, &old, &launched("a", "ag9")]);
        assert!(s.status().subs.is_empty());
    }

    #[test]
    fn a_pending_ask_user_question_is_reported_until_answered() {
        let dir = tempfile::tempdir().unwrap();
        let ask = tool(
            "q",
            "AskUserQuestion",
            r#"{"questions":[{"question":"Which \u001b[31mdatabase?","header":"db"}]}"#,
        );
        let s = scan(dir.path(), &[HUMAN, &ask]);
        assert_eq!(s.status().ask.as_deref(), Some("Which database?"));
        let s = scan(dir.path(), &[HUMAN, &ask, &result("q")]);
        assert_eq!(s.status().ask, None);
        let s = scan(dir.path(), &[HUMAN, &ask, HUMAN]);
        assert_eq!(s.status().ask, None);
    }

    #[test]
    fn long_activity_is_clipped() {
        let l = activity_label("Bash", &serde_json::json!({"command": "x".repeat(300)}));
        assert!(l.chars().count() <= ACTIVITY_MAX);
    }

    #[test]
    fn new_lines_are_picked_up_incrementally() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = scan(dir.path(), &[HUMAN]);
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(dir.path().join("t.jsonl"))
            .unwrap();
        writeln!(f, "{}", tool("a", "Write", r#"{"file_path":"/q/z.md"}"#)).unwrap();
        s.poll();
        assert_eq!(s.status().activity.as_deref(), Some("Write: z.md"));
    }

    #[test]
    fn a_big_transcript_is_read_from_the_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        let filler = tool(
            "old",
            "Bash",
            &format!(r#"{{"command":"{}"}}"#, "y".repeat(900)),
        );
        for _ in 0..(TAIL_BYTES as usize / filler.len() + 50) {
            writeln!(f, "{filler}").unwrap();
        }
        writeln!(f, "{}", tool("new", "Edit", r#"{"file_path":"/a/b.rs"}"#)).unwrap();
        let mut s = StatusScan::new(dir.path().to_path_buf(), path);
        s.poll();
        assert_eq!(s.status().activity.as_deref(), Some("Edit: b.rs"));
    }
}
