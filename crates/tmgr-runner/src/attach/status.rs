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
}

impl SessionStatus {
    pub fn is_empty(&self) -> bool {
        self.activity.is_none() && self.todo.is_none()
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
        if found == self.status {
            return false;
        }
        self.status = found;
        true
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
            (None, None) => return None,
        };
        let clock = (working && row.state.since_ms > 0 && self.now_s > 0)
            .then(|| clock_label(self.now_s.saturating_sub(row.state.since_ms / 1000)));
        Some(StatusLine { text, clock })
    }
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
        }
    }

    pub fn is_for(&self, path: &Path) -> bool {
        self.path == path
    }

    pub fn status(&self) -> SessionStatus {
        SessionStatus {
            activity: self.pending.last().map(|(_, label)| label.clone()),
            todo: self.todo,
        }
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

    fn line(&mut self, line: &[u8]) {
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
                self.assistant(message, content);
            }
            Some("user") => self.user(&v, content),
            _ => {}
        }
    }

    fn assistant(&mut self, message: Option<&str>, content: &Value) {
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
            self.pending.clear();
            return;
        };
        let results: Vec<&str> = blocks
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
            .filter_map(|b| b.get("tool_use_id").and_then(Value::as_str))
            .collect();
        if results.is_empty() {
            self.pending.clear();
        } else {
            self.pending
                .retain(|(id, _)| !results.contains(&id.as_str()));
        }
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

fn mask_secrets(text: &str) -> String {
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

    #[test]
    fn a_finished_todo_list_shows_nothing() {
        let st = SessionStatus {
            activity: None,
            todo: Some((3, 3)),
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
