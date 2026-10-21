use super::rules::{clip_head, clip_tail};
use crate::attach::title::{contains, open_under, Format, LINE_CAP, PASS_CAP};
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

const MAX_PASSES: usize = 4;
const TAIL_BYTES: u64 = 1 << 20;
pub const USER_CAP: usize = 1500;
pub const REPLY_CAP: usize = 8000;
const DIR_CAP: usize = 512;
const ROLLOUT_CHECK: usize = 24;
const WALK_BUDGET: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub start: u64,
    pub user: String,
    pub text: String,
    pub asked: Vec<String>,
}

#[derive(Debug)]
struct Draft {
    start: u64,
    text: String,
    asked: Vec<(String, String)>,
    pending: Vec<String>,
}

#[derive(Debug)]
pub struct Tailer {
    format: Format,
    root: PathBuf,
    path: PathBuf,
    ident: Option<(u64, u64)>,
    consumed: u64,
    skipping: bool,
    windowed: bool,
    user: String,
    user_at: Option<u64>,
    draft: Option<Draft>,
}

enum Line {
    Human(String),
    Agent {
        text: String,
        asked: Vec<(String, String)>,
        tools: Vec<String>,
    },
    Results(Vec<String>),
}

impl Tailer {
    pub fn new(format: Format, root: PathBuf, path: PathBuf, resume: Option<u64>) -> Tailer {
        Tailer {
            format,
            root,
            path,
            ident: None,
            consumed: resume.unwrap_or(0),
            skipping: false,
            windowed: resume.is_none(),
            user: String::new(),
            user_at: None,
            draft: None,
        }
    }

    pub fn is_for(&self, path: &Path) -> bool {
        self.path == path
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn resume_offset(&self) -> u64 {
        match (&self.draft, self.user_at) {
            (Some(d), _) => d.start,
            (None, Some(at)) => at,
            _ => self.consumed,
        }
    }

    pub fn poll(&mut self, settled: bool) -> Option<Turn> {
        for _ in 0..MAX_PASSES {
            if self.pass() != Some(true) {
                break;
            }
        }
        if settled && self.at_end() && self.draft.as_ref().is_some_and(|d| d.pending.is_empty()) {
            return self.close();
        }
        None
    }

    fn at_end(&self) -> bool {
        open_under(&self.root, &self.path).is_some_and(|(_, m)| m.len() <= self.consumed)
    }

    fn reset(&mut self) {
        self.consumed = 0;
        self.skipping = false;
        self.windowed = true;
        self.user.clear();
        self.user_at = None;
        self.draft = None;
    }

    fn pass(&mut self) -> Option<bool> {
        let (mut file, meta) = open_under(&self.root, &self.path)?;
        let ident = (meta.dev(), meta.ino());
        if self.ident.is_some_and(|i| i != ident) || meta.len() < self.consumed {
            self.reset();
        }
        self.ident = Some(ident);
        if self.windowed && self.consumed == 0 && meta.len() > TAIL_BYTES {
            self.consumed = meta.len() - TAIL_BYTES;
            self.skipping = true;
        }
        self.windowed = false;
        if meta.len() <= self.consumed || file.seek(SeekFrom::Start(self.consumed)).is_err() {
            return Some(false);
        }
        let mut buf = Vec::new();
        file.take(PASS_CAP).read_to_end(&mut buf).ok()?;
        let full = buf.len() as u64 == PASS_CAP;
        let mut at = 0usize;
        while at < buf.len() {
            let Some(nl) = buf[at..].iter().position(|b| *b == b'\n') else {
                if buf.len() - at >= LINE_CAP {
                    self.skipping = true;
                    at = buf.len();
                }
                break;
            };
            let line = &buf[at..at + nl];
            let offset = self.consumed + at as u64;
            at += nl + 1;
            if self.skipping {
                self.skipping = false;
                continue;
            }
            if line.len() <= LINE_CAP {
                self.line(line, offset);
            } else if self.format == Format::Claude {
                let head = &line[..4096];
                if contains(head, b"\"type\":\"user\"") && !contains(head, b"tool_result") {
                    self.human(String::new(), offset);
                }
            }
        }
        self.consumed += at as u64;
        Some(full && at > 0)
    }

    fn line(&mut self, line: &[u8], offset: u64) {
        match parse(self.format, line) {
            Some(Line::Human(text)) => self.human(text, offset),
            Some(Line::Agent { text, asked, tools }) => {
                let d = self.draft.get_or_insert_with(|| Draft {
                    start: self.user_at.unwrap_or(offset),
                    text: String::new(),
                    asked: Vec::new(),
                    pending: Vec::new(),
                });
                if !text.is_empty() {
                    if !d.text.is_empty() {
                        d.text.push_str("\n\n");
                    }
                    d.text.push_str(&text);
                    if d.text.len() > REPLY_CAP * 8 {
                        d.text = clip_tail(&d.text, REPLY_CAP * 2);
                    }
                }
                d.asked.extend(asked);
                d.pending.extend(tools);
            }
            Some(Line::Results(ids)) => {
                if let Some(d) = self.draft.as_mut() {
                    d.pending.retain(|p| !ids.contains(p));
                    d.asked.retain(|(id, _)| !ids.contains(id));
                }
            }
            None => {}
        }
    }

    fn human(&mut self, text: String, offset: u64) {
        self.draft = None;
        self.user = clip_head(&text, USER_CAP);
        self.user_at = Some(offset);
    }

    fn close(&mut self) -> Option<Turn> {
        let d = self.draft.take()?;
        self.user_at = None;
        Some(Turn {
            start: d.start,
            user: self.user.clone(),
            text: clip_tail(&d.text, REPLY_CAP),
            asked: d.asked.into_iter().map(|(_, q)| q).collect(),
        })
    }
}

fn parse(format: Format, line: &[u8]) -> Option<Line> {
    match format {
        Format::Claude => {
            if !contains(line, b"\"type\":\"assistant\"") && !contains(line, b"\"type\":\"user\"") {
                return None;
            }
            claude_line(&serde_json::from_slice::<Value>(line).ok()?)
        }
        Format::Codex => {
            if !contains(line, b"\"message\"") && !contains(line, b"user_message") {
                return None;
            }
            codex_line(&serde_json::from_slice::<Value>(line).ok()?)
        }
    }
}

fn claude_line(v: &Value) -> Option<Line> {
    if v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let content = v.get("message")?.get("content")?;
    match v.get("type")?.as_str()? {
        "user" => {
            if v.get("isMeta").and_then(Value::as_bool) == Some(true) {
                return None;
            }
            match content {
                Value::String(s) => Some(Line::Human(s.clone())),
                Value::Array(blocks) => {
                    let results: Vec<String> = blocks
                        .iter()
                        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
                        .filter_map(|b| b.get("tool_use_id")?.as_str().map(str::to_string))
                        .collect();
                    if !results.is_empty() {
                        return Some(Line::Results(results));
                    }
                    let text = join_text(blocks, "text");
                    (!text.is_empty()).then_some(Line::Human(text))
                }
                _ => None,
            }
        }
        "assistant" => {
            let blocks = content.as_array()?;
            let mut asked = Vec::new();
            let mut tools = Vec::new();
            for b in blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
            {
                let id = b.get("id").and_then(Value::as_str).unwrap_or_default();
                if b.get("name").and_then(Value::as_str) != Some("AskUserQuestion") {
                    tools.push(id.to_string());
                    continue;
                }
                let questions = b
                    .get("input")
                    .and_then(|i| i.get("questions"))
                    .and_then(Value::as_array);
                for q in questions.into_iter().flatten() {
                    let text = q
                        .get("question")
                        .and_then(Value::as_str)
                        .map(super::rules::collapse)
                        .filter(|q| !q.is_empty());
                    if let Some(text) = text {
                        asked.push((id.to_string(), clip_tail(&text, super::rules::MAX_LEN)));
                    }
                }
            }
            Some(Line::Agent {
                text: join_text(blocks, "text"),
                asked,
                tools,
            })
        }
        _ => None,
    }
}

fn codex_line(v: &Value) -> Option<Line> {
    let payload = v.get("payload")?;
    match (v.get("type")?.as_str()?, payload.get("type")?.as_str()?) {
        ("event_msg", "user_message") => {
            Some(Line::Human(payload.get("message")?.as_str()?.to_string()))
        }
        ("response_item", "message") if payload.get("role")?.as_str()? == "assistant" => {
            Some(Line::Agent {
                text: join_text(payload.get("content")?.as_array()?, "output_text"),
                asked: Vec::new(),
                tools: Vec::new(),
            })
        }
        _ => None,
    }
}

fn join_text(blocks: &[Value], kind: &str) -> String {
    blocks
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some(kind))
        .filter_map(|b| b.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn locate_codex_rollout(home: &Path, id: Option<&str>, cwd: &str) -> Option<PathBuf> {
    let codex = home.join(".codex");
    let root = codex.join("sessions");
    for dir in [&codex, &root] {
        if !std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir()) {
            return None;
        }
    }
    let mut files = Vec::new();
    let mut budget = WALK_BUDGET;
    collect(&root, 0, &mut files, &mut budget);
    files.sort();
    files.reverse();
    match id {
        Some(id) => {
            let suffix = format!("-{id}.jsonl");
            files.into_iter().find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(&suffix))
            })
        }
        None => files
            .into_iter()
            .take(ROLLOUT_CHECK)
            .find(|p| rollout_cwd(&root, p).is_some_and(|c| c == cwd)),
    }
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>, budget: &mut usize) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.take(DIR_CAP).flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let path = entry.path();
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_dir() && depth < 3 {
            collect(&path, depth + 1, out, budget);
        } else if ft.is_file()
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
        {
            out.push(path);
        }
    }
}

fn rollout_cwd(root: &Path, path: &Path) -> Option<String> {
    let (file, _) = open_under(root, path)?;
    let mut buf = Vec::new();
    file.take(64 * 1024).read_to_end(&mut buf).ok()?;
    let first = buf.split(|b| *b == b'\n').next()?;
    let v: Value = serde_json::from_slice(first).ok()?;
    Some(v.get("payload")?.get("cwd")?.as_str()?.to_string())
}
