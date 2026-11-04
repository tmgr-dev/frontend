use super::index::Piece;
use super::{
    clean_text, json_lines, open_regular, parse_ts, read_head, subdirs_and_files, Agent, Entry,
    Live, HEAD_CAP,
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

pub(super) fn live_sessions(root: &Path) -> HashMap<String, Option<String>> {
    let mut out = HashMap::new();
    for (path, meta) in subdirs_and_files(&root.join("sessions")) {
        if !meta.is_file() || path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let mut buf = Vec::new();
        let Ok(f) = open_regular(&path) else { continue };
        if f.take(SESSION_FILE_CAP).read_to_end(&mut buf).is_err() {
            continue;
        }
        let Ok(v) = serde_json::from_slice::<Value>(&buf) else {
            continue;
        };
        let (Some(pid), Some(sid)) = (v["pid"].as_i64(), v["sessionId"].as_str()) else {
            continue;
        };
        if valid_resume_id(sid) && pid_alive(pid) {
            out.insert(sid.to_string(), v["name"].as_str().and_then(clean_text));
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

pub(super) fn fill(e: &mut Entry, live: &HashMap<String, Option<String>>) -> bool {
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
    if let Some(name) = live.get(&e.id) {
        e.live = Live::Yes;
        e.title = name.clone();
    }
    true
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
