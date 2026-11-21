use super::{lock, Chat};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tmgr_ipc::api::{SessionInfo, StateKind};
use tmgr_ipc::chat::{
    limits, Author, AuthorKind, ChatMessage, MessageKind, RoomRef, SessionMeta, CHAT_V,
};
use tmgr_session::{switch, tail, Agent};

const RESOLVE_EVERY: Duration = Duration::from_secs(30);
const BACKFILL_BYTES: u64 = 4 << 20;
const BACKFILL_TURNS: usize = 30;

pub struct Tail {
    path: PathBuf,
    offset: u64,
    resolve_at: Instant,
    pub(super) meta: SessionMeta,
}

#[derive(Serialize, Deserialize)]
struct Saved {
    path: PathBuf,
    offset: u64,
    #[serde(default)]
    backfilled: bool,
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
        backfilled: true,
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

fn update_meta(meta: &mut SessionMeta, profile: Option<&str>, usage: &tail::Usage) {
    meta.agent = profile.map(str::to_string);
    if let Some(model) = usage
        .model
        .as_deref()
        .filter(|m| m.len() <= limits::MAX_MODEL && m.bytes().all(|b| b.is_ascii_graphic()))
    {
        meta.model = Some(model.to_string());
    }
    if let Some(tokens) = usage.context_tokens {
        meta.context_tokens = Some(tokens);
        meta.context_pct = Some(match usage.context_window {
            Some(window) => ((tokens * 100 + window / 2) / window).min(100) as u8,
            None => crate::attach::status::ctx_percent(tokens),
        });
    }
}

fn post(
    chat: &Chat,
    room: &RoomRef,
    s: &SessionInfo,
    meta: &SessionMeta,
    turn: tail::Turn,
) -> Result<(), String> {
    let text = clip(turn.text);
    if turn.user && chat.take_sent(&s.id, &text) {
        return Ok(());
    }
    let ts_ms = turn
        .ts
        .as_deref()
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .and_then(|t| u64::try_from(t.timestamp_millis()).ok())
        .unwrap_or_else(super::now_ms);
    chat.append(ChatMessage {
        v: CHAT_V,
        id: chat.mint_id(),
        room: room.clone(),
        author: Author {
            kind: AuthorKind::Session,
            id: s.id.clone(),
            machine: Some(chat.machine().to_string()),
            device: None,
            agent: s.profile.clone(),
        },
        ts_ms,
        kind: MessageKind::Text,
        body_md: text,
        attachments: Vec::new(),
        reply_to: None,
        decision: None,
        session_meta: Some(meta.clone()),
        user_turn: turn.user,
    })
    .map(|_| ())
}

fn backfill(chat: &Chat, s: &SessionInfo, agent: Agent, t: &mut Tail, end: u64, legacy: bool) {
    let room = chat.session_room(&s.id);
    let Ok(start) = tail::backfill_start(&t.path, end, BACKFILL_BYTES) else {
        return;
    };
    let window = usize::try_from(end - start).unwrap_or(usize::MAX).max(1);
    let Ok(batch) = tail::read_new(&t.path, agent, start, window) else {
        return;
    };
    update_meta(&mut t.meta, s.profile.as_deref(), &batch.usage);
    let known: HashSet<String> = chat
        .history(&room, None, Some(200))
        .unwrap_or_default()
        .into_iter()
        .filter(|m| m.author.kind == AuthorKind::Session)
        .map(|m| m.body_md)
        .collect();
    let skip = batch.turns.len().saturating_sub(BACKFILL_TURNS);
    for turn in batch.turns.into_iter().skip(skip) {
        if known.contains(&clip(turn.text.clone())) {
            continue;
        }
        if post(chat, &room, s, &t.meta, turn).is_err() {
            return;
        }
    }
    t.offset = if legacy { end } else { batch.offset };
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
                    let (offset, backfill_from) = match load(dir, &s.id) {
                        Some(saved) if saved.path == path && saved.backfilled => {
                            (saved.offset, None)
                        }
                        Some(saved) if saved.path == path => {
                            (saved.offset, Some((saved.offset, true)))
                        }
                        _ => {
                            let end = tail::len(&path).unwrap_or(0);
                            (end, Some((end, false)))
                        }
                    };
                    let mut t = Tail {
                        path,
                        offset,
                        resolve_at: Instant::now() + RESOLVE_EVERY,
                        meta: SessionMeta::default(),
                    };
                    if let Some((end, legacy)) = backfill_from {
                        backfill(chat, s, agent, &mut t, end, legacy);
                    }
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
        update_meta(&mut t.meta, s.profile.as_deref(), &batch.usage);
        let mut failed = false;
        for turn in batch.turns {
            if post(chat, &room, s, &t.meta, turn).is_err() {
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
    fn session_messages_and_rooms_carry_the_session_meta() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let log = t.path().join("log.jsonl");
        let turn = |text: &str, input: u64| {
            json!({"type": "assistant", "message": {"model": "claude-opus-4-1", "usage": {"input_tokens": input},
                "content": [{"type": "text", "text": text}]}})
            .to_string()
                + "\n"
        };
        append(&log, &turn("first", 40_000));
        let s = [info(SID, "claude-code", "working")];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        capture_with(&chat, &s, &resolve);
        let want = SessionMeta {
            context_pct: Some(20),
            context_tokens: Some(40_000),
            agent: Some("claude-code".into()),
            model: Some("claude-opus-4-1".into()),
        };
        let m = chat
            .history(&chat.session_room(SID), None, None)
            .unwrap()
            .remove(0);
        assert_eq!(m.session_meta.as_ref(), Some(&want));
        m.validate().unwrap();
        let room = chat.rooms(&s).remove(1);
        assert_eq!(room.context_pct, Some(20));
        assert_eq!(room.context_tokens, Some(40_000));
    }

    #[test]
    fn codex_context_uses_the_reported_window() {
        let mut meta = SessionMeta::default();
        let usage = tail::Usage {
            model: Some("gpt-x".into()),
            context_tokens: Some(64_600),
            context_window: Some(258_400),
        };
        update_meta(&mut meta, Some("codex"), &usage);
        assert_eq!(meta.context_pct, Some(25));
        update_meta(&mut meta, Some("codex"), &tail::Usage::default());
        assert_eq!(meta.context_pct, Some(25));
        assert_eq!(meta.model.as_deref(), Some("gpt-x"));
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

    fn user_line(text: &str) -> String {
        json!({"type": "user", "message": {"role": "user", "content": text}}).to_string() + "\n"
    }

    fn kinds(chat: &Chat, id: &str) -> Vec<(bool, String)> {
        chat.history(&chat.session_room(id), None, None)
            .unwrap()
            .into_iter()
            .map(|m| (m.user_turn, m.body_md))
            .collect()
    }

    #[test]
    fn user_prompts_are_captured_as_user_turns_in_order() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let log = t.path().join("log.jsonl");
        append(&log, &(user_line("question") + &line("answer")));
        let s = [info(SID, "claude-code", "working")];
        capture_with(&chat, &s, &|_, _| Some(log.clone()));
        assert_eq!(
            kinds(&chat, SID),
            [
                (true, "question".to_string()),
                (false, "answer".to_string())
            ]
        );
        let m = chat
            .history(&chat.session_room(SID), None, None)
            .unwrap()
            .remove(0);
        assert_eq!(m.author.kind, AuthorKind::Session);
        m.validate().unwrap();
    }

    #[test]
    fn a_big_existing_log_backfills_only_the_last_turns() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let log = t.path().join("log.jsonl");
        for _ in 0..16 {
            append(&log, &line(&"old ".repeat(80_000)));
        }
        for i in 0..40 {
            append(
                &log,
                &(user_line(&format!("q{i}")) + &line(&format!("a{i}"))),
            );
        }
        let s = [info(SID, "claude-code", "working")];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        capture_with(&chat, &s, &resolve);
        let got = kinds(&chat, SID);
        assert_eq!(got.len(), BACKFILL_TURNS);
        assert_eq!(got.last().unwrap().1, "a39");
        assert_eq!(got[0].1, "q25");
        assert!(got.iter().all(|(_, b)| !b.starts_with("old")));
        capture_with(&chat, &s, &resolve);
        assert_eq!(kinds(&chat, SID).len(), BACKFILL_TURNS);
        append(&log, &line("fresh"));
        capture_with(&chat, &s, &resolve);
        assert_eq!(bodies(&chat, SID).last().unwrap(), "fresh");
    }

    #[test]
    fn a_log_over_the_window_starts_at_a_line_boundary() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let log = t.path().join("log.jsonl");
        append(&log, &line(&"x".repeat((BACKFILL_BYTES as usize) + 10)));
        append(&log, &(user_line("recent q") + &line("recent a")));
        capture_with(&chat, &[info(SID, "codex", "working")], &|_, _| {
            Some(log.clone())
        });
        assert!(bodies(&chat, SID).is_empty());
        let codex_user =
            json!({"type": "response_item", "payload": {"type": "message", "role": "user",
            "content": [{"type": "input_text", "text": "codex q"}]}})
            .to_string()
                + "\n";
        let codex_log = t.path().join("codex.jsonl");
        append(&codex_log, &codex_user);
        let other = "s_zyxwvutsrq";
        capture_with(&chat, &[info(other, "codex", "working")], &|_, _| {
            Some(codex_log.clone())
        });
        assert_eq!(kinds(&chat, other), [(true, "codex q".to_string())]);
    }

    #[test]
    fn an_old_saved_state_without_backfill_gets_history_once_without_duplicates() {
        let t = tempfile::tempdir().unwrap();
        let run = t.path().join("run");
        let log = t.path().join("log.jsonl");
        append(&log, &(user_line("q1") + &line("a1")));
        let end = tail::len(&log).unwrap();
        append(&log, &line("a2"));
        let chat = Chat::open(&run);
        let dir = chat.store.as_ref().unwrap().dir().to_path_buf();
        fs::write(
            state_path(&dir, SID),
            serde_json::to_vec(&json!({"path": log, "offset": end})).unwrap(),
        )
        .unwrap();
        let s = [info(SID, "claude-code", "working")];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        capture_with(&chat, &s, &resolve);
        assert_eq!(
            kinds(&chat, SID),
            [
                (true, "q1".to_string()),
                (false, "a1".to_string()),
                (false, "a2".to_string())
            ]
        );
        let again = Chat::open(&run);
        capture_with(&again, &s, &resolve);
        assert_eq!(kinds(&again, SID).len(), 3);
    }

    #[test]
    fn a_changed_transcript_path_backfills_instead_of_replaying_from_zero() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let first = t.path().join("first.jsonl");
        let second = t.path().join("second.jsonl");
        append(&first, &line("one"));
        append(&second, &line(&"old ".repeat(2_000_000)));
        append(&second, &line("two"));
        let s = [info(SID, "claude-code", "working")];
        capture_with(&chat, &s, &|_, _| Some(first.clone()));
        let tails_path = chat.store.as_ref().unwrap().dir().to_path_buf();
        lock(&chat.tails).get_mut(SID).unwrap().resolve_at = Instant::now();
        capture_with(&chat, &s, &|_, _| Some(second.clone()));
        assert_eq!(bodies(&chat, SID), ["one", "two"]);
        assert!(load(&tails_path, SID).unwrap().path == second);
    }

    #[test]
    fn a_prompt_sent_from_chat_is_not_echoed_as_a_second_message() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let log = t.path().join("log.jsonl");
        let s = [info(SID, "claude-code", "working")];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        capture_with(&chat, &s, &resolve);
        chat.remember_sent(SID, "run it\n");
        append(&log, &(user_line("run it") + &user_line("run it")));
        capture_with(&chat, &s, &resolve);
        assert_eq!(kinds(&chat, SID), [(true, "run it".to_string())]);
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
