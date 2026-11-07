use super::index::Piece;
use super::{
    clean_text, json_lines, open_regular, parse_ts, read_head, read_tail, subdirs_and_files, Agent,
    Entry, Live, HEAD_CAP,
};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use tmgr_runner::resume::valid_resume_id;

const SESSION_FILE_CAP: u64 = 64 * 1024;
const WRAPPERS: &[&str] = &[
    "<command-name>",
    "<command-message>",
    "<command-args>",
    "<local-command-",
    "<system-reminder>",
    "<user-prompt-submit-hook>",
    "<bash-input>",
    "<bash-stdout>",
    "<bash-stderr>",
    "Caveat:",
    "[Request interrupted",
];

pub(super) fn scan(root: &Path, readable: bool) -> Vec<Entry> {
    let mut out = Vec::new();
    for (dir, meta) in subdirs_and_files(&root.join("projects")) {
        if !meta.is_dir() {
            continue;
        }
        let project = dir
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(clean_text);
        for (path, meta) in subdirs_and_files(&dir) {
            if !meta.is_file() || path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(id) = super::uuid_stem(&path).filter(|s| valid_resume_id(s)) else {
                continue;
            };
            let mut e = Entry::light(Agent::Claude, id.to_string(), path.clone(), &meta, readable);
            if !readable {
                e.project = project.clone();
            }
            out.push(e);
        }
    }
    out
}

#[derive(Default)]
pub(super) struct Running {
    pub by_id: HashMap<String, (Option<String>, Option<String>)>,
    pub unknown: bool,
}

pub(super) fn live_sessions(root: &Path) -> Running {
    let mut out = Running::default();
    for (path, meta) in subdirs_and_files(&root.join("sessions")) {
        if !meta.is_file() || path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let stem_alive = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<i64>().ok())
            .is_some_and(pid_alive);
        let mut buf = Vec::new();
        let read = open_regular(&path).and_then(|f| f.take(SESSION_FILE_CAP).read_to_end(&mut buf));
        let parsed = read
            .ok()
            .and_then(|_| serde_json::from_slice::<Value>(&buf).ok());
        let Some(v) = parsed else {
            out.unknown |= stem_alive;
            continue;
        };
        let alive = v["pid"].as_i64().is_some_and(pid_alive);
        match v["sessionId"].as_str().filter(|s| valid_resume_id(s)) {
            Some(sid) if alive => {
                out.by_id.insert(
                    sid.to_string(),
                    (
                        v["name"].as_str().and_then(clean_text),
                        v["cwd"].as_str().filter(|c| !c.is_empty()).map(Into::into),
                    ),
                );
            }
            Some(_) => {}
            None => out.unknown |= alive || stem_alive,
        }
    }
    out
}

fn pid_alive(pid: i64) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    if pid <= 0 {
        return false;
    }
    // SAFETY: signal 0 only probes for existence.
    let ok = unsafe { libc::kill(pid, 0) } == 0;
    ok || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

pub(super) fn fill(e: &mut Entry, live: &Running, orphan_cwds: &[String]) -> bool {
    let Ok(head) = read_head(&e.path, HEAD_CAP) else {
        e.readable = false;
        return true;
    };
    for v in json_lines(&head) {
        if e.started_at.is_none() {
            e.started_at = parse_ts(&v["timestamp"]);
        }
        if e.cwd.is_none() {
            e.cwd = v["cwd"].as_str().filter(|c| !c.is_empty()).map(Into::into);
        }
        if e.git_branch.is_none() {
            e.git_branch = v["gitBranch"].as_str().and_then(clean_text);
        }
        if e.first_message.is_none()
            && v["type"] == "user"
            && !flag(&v, "isMeta")
            && !flag(&v, "isSidechain")
        {
            e.first_message = message_text(&v)
                .filter(|t| !is_wrapper(t))
                .and_then(|t| clean_text(&t));
        }
        if e.cwd.is_some() && e.first_message.is_some() && e.git_branch.is_some() {
            break;
        }
    }
    let log_title = log_title(
        &head,
        read_tail(&e.path, HEAD_CAP).unwrap_or_default().as_slice(),
    );
    if let Some((name, _)) = live.by_id.get(&e.id) {
        e.live = Live::Yes;
        e.title = name.clone().or(log_title);
    } else {
        if live.unknown
            || e.cwd
                .as_deref()
                .is_some_and(|c| orphan_cwds.iter().any(|o| o == c))
        {
            e.live = Live::Recent;
        }
        e.title = log_title;
    }
    true
}

#[derive(Default)]
struct Titles {
    custom: Option<String>,
    ai: Option<String>,
}

fn titles(buf: &[u8]) -> Titles {
    let mut t = Titles::default();
    let lines = buf
        .split(|b| *b == b'\n')
        .filter(|l| l.windows(7).any(|w| w == b"-title\""));
    for l in lines {
        let Ok(v) = serde_json::from_slice::<Value>(l) else {
            continue;
        };
        let (slot, key) = match v["type"].as_str() {
            Some("custom-title") => (&mut t.custom, "customTitle"),
            Some("ai-title") => (&mut t.ai, "aiTitle"),
            _ => continue,
        };
        if let Some(name) = v[key].as_str().and_then(clean_text) {
            *slot = Some(name);
        }
    }
    t
}

fn log_title(head: &[u8], tail: &[u8]) -> Option<String> {
    let (h, t) = (titles(head), titles(tail));
    t.custom.or(h.custom).or(t.ai).or(h.ai)
}

fn flag(v: &Value, key: &str) -> bool {
    v[key].as_bool().unwrap_or(false)
}

fn is_wrapper(t: &str) -> bool {
    let t = t.trim_start();
    t.is_empty() || WRAPPERS.iter().any(|w| t.starts_with(w))
}

pub(super) fn message_text(v: &Value) -> Option<String> {
    match &v["message"]["content"] {
        Value::String(s) => Some(s.clone()),
        Value::Array(parts) => {
            let texts: Vec<&str> = parts
                .iter()
                .filter(|p| p["type"] == "text")
                .filter_map(|p| p["text"].as_str())
                .collect();
            (!texts.is_empty()).then(|| texts.join("\n"))
        }
        _ => None,
    }
}

pub(super) fn pieces(v: &Value, out: &mut Vec<Piece>) {
    if !matches!(v["type"].as_str(), Some("user" | "assistant")) || flag(v, "isMeta") {
        return;
    }
    match &v["message"]["content"] {
        Value::String(s) => out.push(Piece::text(s)),
        Value::Array(parts) => {
            for p in parts {
                match p["type"].as_str() {
                    Some("text") => {
                        if let Some(t) = p["text"].as_str() {
                            out.push(Piece::text(t));
                        }
                    }
                    Some("tool_result") => match &p["content"] {
                        Value::String(s) => out.push(Piece::tool(s)),
                        Value::Array(items) => {
                            for t in items.iter().filter(|i| i["type"] == "text") {
                                if let Some(t) = t["text"].as_str() {
                                    out.push(Piece::tool(t));
                                }
                            }
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
        _ => {}
    }
}
