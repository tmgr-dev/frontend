use super::{lock, Chat};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tmgr_ipc::api::{SessionInfo, StateKind};
use tmgr_ipc::chat::{limits, Author, AuthorKind};
use tmgr_session::{switch, tail, Agent};

const RESOLVE_EVERY: Duration = Duration::from_secs(30);
const BACKFILL_MAX: u64 = 256 << 10;

pub struct Tail {
    path: PathBuf,
    offset: u64,
    resolve_at: Instant,
}

#[derive(Serialize, Deserialize)]
struct Saved {
    path: PathBuf,
    offset: u64,
}

fn agent_of(profile: Option<&str>) -> Option<Agent> {
    match profile? {
        "claude-code" => Some(Agent::Claude),
        "codex" => Some(Agent::Codex),
        _ => None,
    }
}

fn state_path(dir: &Path, session: &str) -> PathBuf {
    dir.join(format!("tail-{session}.json"))
}

fn load(dir: &Path, session: &str) -> Option<Saved> {
    serde_json::from_slice(&fs::read(state_path(dir, session)).ok()?).ok()
}

fn save(dir: &Path, session: &str, t: &Tail) {
    let saved = Saved {
        path: t.path.clone(),
        offset: t.offset,
    };
    let path = state_path(dir, session);
    let tmp = path.with_extension("json.tmp");
    let write = || -> std::io::Result<()> {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp)?;
        f.write_all(&serde_json::to_vec(&saved).unwrap_or_default())?;
        fs::rename(&tmp, &path)
    };
    if let Err(e) = write() {
        crate::daemon::log(&format!("chat: tail state of {session} not saved: {e}"));
    }
}

fn clip(text: String) -> String {
    if text.len() <= limits::MAX_BODY {
        return text;
    }
    let mut end = limits::MAX_BODY;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

pub fn capture(chat: &Chat, sessions: &[SessionInfo]) {
    capture_with(chat, sessions, &|s, a| switch::log_of(s, a).ok());
}

pub fn capture_with(
    chat: &Chat,
    sessions: &[SessionInfo],
    resolve: &dyn Fn(&SessionInfo, Agent) -> Option<PathBuf>,
) {
    let Some(store) = chat.store.as_ref() else {
        return;
    };
    let dir = store.dir();
    let mut tails = lock(&chat.tails);
    for s in sessions {
        let Some(agent) = agent_of(s.profile.as_deref()) else {
            continue;
        };
        let exited = s.state.kind == StateKind::Exited;
        if exited && !tails.contains_key(&s.id) {
            continue;
        }
        let room = chat.session_room(&s.id);
        if room.validate().is_err() {
            continue;
        }
        let due = tails
            .get(&s.id)
            .is_none_or(|t| t.resolve_at <= Instant::now());
        if due && !exited {
            if let Some(path) = resolve(s, agent) {
                let changed = tails.get(&s.id).is_none_or(|t| t.path != path);
                if changed {
                    let offset = match load(dir, &s.id) {
                        Some(saved) if saved.path == path => saved.offset,
                        Some(_) => 0,
                        None => match tail::len(&path) {
                            Ok(n) if n > BACKFILL_MAX => n,
                            _ => 0,
                        },
                    };
                    let t = Tail {
                        path,
                        offset,
                        resolve_at: Instant::now() + RESOLVE_EVERY,
                    };
                    save(dir, &s.id, &t);
                    tails.insert(s.id.clone(), t);
                } else if let Some(t) = tails.get_mut(&s.id) {
                    t.resolve_at = Instant::now() + RESOLVE_EVERY;
                }
            } else if let Some(t) = tails.get_mut(&s.id) {
                t.resolve_at = Instant::now() + RESOLVE_EVERY;
            }
        }
        let Some(t) = tails.get_mut(&s.id) else {
            continue;
        };
        let Ok(batch) = tail::read_new(&t.path, agent, t.offset, tail::WINDOW) else {
            continue;
        };
        let author = Author {
            kind: AuthorKind::Session,
            id: s.id.clone(),
            machine: Some(chat.machine().to_string()),
            device: None,
            agent: s.profile.clone(),
        };
        let mut failed = false;
        for text in batch.texts {
            if chat
                .post(room.clone(), author.clone(), clip(text), None, Vec::new())
                .is_err()
            {
                failed = true;
                break;
            }
        }
        if !failed && batch.offset != t.offset {
            t.offset = batch.offset;
            save(dir, &s.id, t);
        }
    }
    tails.retain(|id, _| {
        sessions
            .iter()
            .any(|s| s.id == *id && s.state.kind != StateKind::Exited)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::Cell;

    fn info(id: &str, profile: &str, kind: &str) -> SessionInfo {
        serde_json::from_value(json!({
            "id": id, "name": "work", "argv": [profile], "cwd": "/x", "profile": profile,
            "state": {"kind": kind, "since_ms": 0, "detail": null},
            "created_ms": 1, "cols": 80, "rows": 24, "shim_pid": 1, "child_pid": 2,
        }))
        .unwrap()
    }

    fn line(text: &str) -> String {
        json!({"type": "assistant", "message": {"content": [{"type": "text", "text": text}]}})
            .to_string()
            + "\n"
    }

    fn append(p: &Path, s: &str) {
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
            .unwrap();
        f.write_all(s.as_bytes()).unwrap();
    }

    fn bodies(chat: &Chat, id: &str) -> Vec<String> {
        chat.history(&chat.session_room(id), None, None)
            .unwrap()
            .into_iter()
            .map(|m| m.body_md)
            .collect()
    }

    const SID: &str = "s_abcdefghij";

    #[test]
    fn new_assistant_text_becomes_session_messages_once() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let log = t.path().join("log.jsonl");
        append(&log, &line("first"));
        let s = [info(SID, "claude-code", "working")];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        capture_with(&chat, &s, &resolve);
        capture_with(&chat, &s, &resolve);
        assert_eq!(bodies(&chat, SID), ["first"]);
        append(&log, &line("second"));
        capture_with(&chat, &s, &resolve);
        assert_eq!(bodies(&chat, SID), ["first", "second"]);
        let m = chat
            .history(&chat.session_room(SID), None, None)
            .unwrap()
            .remove(0);
        assert_eq!(m.author.kind, AuthorKind::Session);
        assert_eq!(m.author.id, SID);
        assert_eq!(m.author.machine.as_deref(), Some(chat.machine()));
        assert_eq!(m.author.agent.as_deref(), Some("claude-code"));
        m.validate().unwrap();
    }

    #[test]
    fn a_restart_resumes_at_the_saved_offset_without_duplicates() {
        let t = tempfile::tempdir().unwrap();
        let run = t.path().join("run");
        let log = t.path().join("log.jsonl");
        append(&log, &line("one"));
        let s = [info(SID, "claude-code", "working")];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        capture_with(&Chat::open(&run), &s, &resolve);
        append(&log, &line("two"));
        let again = Chat::open(&run);
        capture_with(&again, &s, &resolve);
        assert_eq!(bodies(&again, SID), ["one", "two"]);
    }

    #[test]
    fn a_big_existing_log_is_not_replayed_but_new_turns_are() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let log = t.path().join("log.jsonl");
        append(&log, &line(&"old ".repeat(80_000)));
        let s = [info(SID, "codex", "working")];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        capture_with(&chat, &s, &resolve);
        assert!(bodies(&chat, SID).is_empty());
        let codex =
            json!({"type": "response_item", "payload": {"type": "message", "role": "assistant",
            "content": [{"type": "output_text", "text": "fresh"}]}})
            .to_string()
                + "\n";
        append(&log, &codex);
        capture_with(&chat, &s, &resolve);
        assert_eq!(bodies(&chat, SID), ["fresh"]);
    }

    #[test]
    fn other_profiles_and_unresolvable_logs_are_skipped_and_the_log_is_cached() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let calls = Cell::new(0);
        let resolve = |_: &SessionInfo, _: Agent| {
            calls.set(calls.get() + 1);
            None
        };
        let s = [
            info(SID, "claude-code", "working"),
            info("s_bcdefghijk", "opencode", "working"),
        ];
        capture_with(&chat, &s, &resolve);
        capture_with(&chat, &s, &resolve);
        assert_eq!(calls.get(), 2);
        assert!(bodies(&chat, "s_bcdefghijk").is_empty());

        let log = t.path().join("log.jsonl");
        append(&log, &line("hi"));
        let hit = Cell::new(0);
        let resolve = |_: &SessionInfo, _: Agent| {
            hit.set(hit.get() + 1);
            Some(log.clone())
        };
        let s = [info("s_cdefghijkl", "claude-code", "working")];
        for _ in 0..3 {
            capture_with(&chat, &s, &resolve);
        }
        assert_eq!(hit.get(), 1);
    }

    #[test]
    fn an_exited_session_is_drained_once_then_forgotten() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let log = t.path().join("log.jsonl");
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        append(&log, &line("a"));
        capture_with(&chat, &[info(SID, "claude-code", "working")], &resolve);
        append(&log, &line("last words"));
        let gone = [info(SID, "claude-code", "exited")];
        capture_with(&chat, &gone, &resolve);
        assert_eq!(bodies(&chat, SID), ["a", "last words"]);
        assert!(lock(&chat.tails).is_empty());
        append(&log, &line("ghost"));
        capture_with(&chat, &gone, &resolve);
        assert_eq!(bodies(&chat, SID).len(), 2);
    }

    #[test]
    fn an_oversized_reply_is_clipped_to_the_body_limit() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let log = t.path().join("log.jsonl");
        append(&log, &line(&"é".repeat(limits::MAX_BODY)));
        capture_with(&chat, &[info(SID, "claude-code", "working")], &|_, _| {
            Some(log.clone())
        });
        let b = bodies(&chat, SID);
        assert_eq!(b.len(), 1);
        assert!(b[0].len() <= limits::MAX_BODY);
    }
}
