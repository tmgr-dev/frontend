use super::index::Piece;
use super::{
    clean_text, json_lines, open_regular, parse_ts, read_head, subdirs_and_files, Agent, Entry,
    Live, HEAD_CAP,
};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::path::Path;
use tmgr_runner::resume::valid_resume_id;

const INDEX_CAP: u64 = 8 * 1024 * 1024;
const PREAMBLES: &[&str] = &[
    "<environment_context>",
    "<user_instructions>",
    "<user_action>",
    "<INSTRUCTIONS>",
    "# AGENTS.md",
];

fn digits(p: &Path, len: usize) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.len() == len && n.bytes().all(|b| b.is_ascii_digit()))
}

pub(super) fn scan(root: &Path, readable: bool) -> Vec<Entry> {
    let mut out = Vec::new();
    for (y, ym) in subdirs_and_files(&root.join("sessions")) {
        if !ym.is_dir() || !digits(&y, 4) {
            continue;
        }
        for (m, mm) in subdirs_and_files(&y) {
            if !mm.is_dir() || !digits(&m, 2) {
                continue;
            }
            for (d, dm) in subdirs_and_files(&m) {
                if !dm.is_dir() || !digits(&d, 2) {
                    continue;
                }
                for (path, meta) in subdirs_and_files(&d) {
                    if !meta.is_file() {
                        continue;
                    }
                    if let Some(id) = rollout_id(&path) {
                        out.push(Entry::light(Agent::Codex, id, path, &meta, readable));
                    }
                }
            }
        }
    }
    out
}

fn rollout_id(path: &Path) -> Option<String> {
    if path.extension()?.to_str()? != "jsonl" {
        return None;
    }
    let stem = super::uuid_stem(path)?.strip_prefix("rollout-")?;
    let id = stem.get(stem.len().checked_sub(36)?..)?;
    valid_resume_id(id).then(|| id.to_string())
}

pub(super) fn titles(root: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut buf = Vec::new();
    let Ok(f) = open_regular(&root.join("session_index.jsonl")) else {
        return out;
    };
    if f.take(INDEX_CAP).read_to_end(&mut buf).is_err() {
        return out;
    }
    for v in json_lines(&buf) {
        if let (Some(id), Some(name)) = (
            v["id"].as_str(),
            v["thread_name"].as_str().and_then(clean_text),
        ) {
            out.insert(id.to_string(), name);
        }
    }
    out
}

pub(super) fn is_subagent(meta: &Value) -> bool {
    meta["payload"]["source"]
        .as_object()
        .is_some_and(|o| o.contains_key("subagent"))
}

pub(super) fn fill(e: &mut Entry, titles: &HashMap<String, String>, root: &Path) -> bool {
    let Ok(head) = read_head(&e.path, HEAD_CAP) else {
        e.readable = false;
        return true;
    };
    let mut lines = json_lines(&head);
    if let Some(first) = lines.next().filter(|v| v["type"] == "session_meta") {
        if is_subagent(&first) {
            return false;
        }
        let p = &first["payload"];
        if let Some(id) = p["id"].as_str() {
            if id != e.id {
                return false;
            }
        }
        e.started_at = parse_ts(&p["timestamp"]);
        e.cwd = p["cwd"].as_str().filter(|c| !c.is_empty()).map(Into::into);
        e.git_branch = p["git"]["branch"].as_str().and_then(clean_text);
    }
    for v in lines {
        if v["type"] == "event_msg" && v["payload"]["type"] == "user_message" {
            if let Some(t) = v["payload"]["message"].as_str() {
                if !is_preamble(t) {
                    e.first_message = clean_text(t);
                    break;
                }
            }
        }
    }
    e.title = titles.get(&e.id).cloned();
    match lock_state(root, &e.id) {
        Lock::Held => e.live = Live::Yes,
        Lock::Unknown => e.live = Live::Recent,
        Lock::Free => {}
    }
    true
}

fn is_preamble(t: &str) -> bool {
    let t = t.trim_start();
    t.is_empty() || PREAMBLES.iter().any(|p| t.starts_with(p))
}

enum Lock {
    Held,
    Free,
    Unknown,
}

fn lock_state(root: &Path, id: &str) -> Lock {
    let f = match open_regular(&root.join("thread-writer-locks").join(format!("{id}.lock"))) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Lock::Free,
        Err(_) => return Lock::Unknown,
    };
    let fd = f.as_raw_fd();
    // SAFETY: fd belongs to `f`, which outlives both calls.
    unsafe {
        if libc::flock(fd, libc::LOCK_SH | libc::LOCK_NB) == 0 {
            libc::flock(fd, libc::LOCK_UN);
            return Lock::Free;
        }
    }
    if std::io::Error::last_os_error().raw_os_error() == Some(libc::EWOULDBLOCK) {
        Lock::Held
    } else {
        Lock::Unknown
    }
}

pub(super) fn pieces(v: &Value, out: &mut Vec<Piece>) {
    let p = &v["payload"];
    match (v["type"].as_str(), p["type"].as_str()) {
        (Some("event_msg"), Some("user_message" | "agent_message")) => {
            if let Some(t) = p["message"].as_str() {
                if !is_preamble(t) {
                    out.push(Piece::text(t));
                }
            }
        }
        (Some("response_item"), Some("function_call_output" | "custom_tool_call_output")) => {
            tool_output(&p["output"], out);
        }
        _ => {}
    }
}

fn tool_output(v: &Value, out: &mut Vec<Piece>) {
    match v {
        Value::String(s) => match serde_json::from_str::<Value>(s) {
            Ok(inner @ Value::Object(_)) => tool_output(&inner, out),
            _ => out.push(Piece::tool(s)),
        },
        Value::Object(o) => {
            for key in ["output", "content", "text"] {
                if let Some(x) = o.get(key) {
                    tool_output(x, out);
                    return;
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|i| tool_output(i, out)),
        _ => {}
    }
}
