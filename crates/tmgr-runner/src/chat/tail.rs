use super::{lock, Chat};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tmgr_ipc::api::{SessionInfo, StateKind};
use tmgr_ipc::chat::{
    limits, ActivityState, Author, AuthorKind, ChatActivity, ChatMessage, MessageKind, RoomRef,
    SessionMeta, CHAT_V,
};
use tmgr_session::{switch, tail, Agent};

use crate::attach::{Format, StatusScan, TitleScan};

const RESOLVE_EVERY: Duration = Duration::from_secs(30);
const BACKFILL_BYTES: u64 = 4 << 20;
const BACKFILL_TURNS: usize = 30;
const PID_FILE_SLACK_MS: u64 = 2000;
const BACKFILL_TRIES: u32 = 3;
const NO_LOG_WARN_MS: u64 = 120_000;

fn tried() -> &'static Mutex<HashMap<String, u32>> {
    static T: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    T.get_or_init(Mutex::default)
}

fn warn_once(key: String, msg: &str) {
    static W: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    if lock(W.get_or_init(Mutex::default)).insert(key) {
        crate::daemon::log(msg);
    }
}

pub struct Tail {
    path: PathBuf,
    offset: u64,
    resolve_at: Instant,
    pub(super) meta: SessionMeta,
    title: Option<TitleScan>,
    status: Option<StatusScan>,
    sent: Option<ChatActivity>,
}

#[derive(Serialize, Deserialize)]
struct Saved {
    path: PathBuf,
    offset: u64,
    #[serde(default)]
    backfilled: bool,
}

impl Tail {
    pub(super) fn asking(&self) -> bool {
        self.status.as_ref().is_some_and(StatusScan::is_asking)
    }
}

pub(crate) fn agent_named(profile: Option<&str>, argv0: &str) -> Option<Agent> {
    match profile {
        Some("claude-code") => Some(Agent::Claude),
        Some("codex") => Some(Agent::Codex),
        Some(_) => None,
        None => match Path::new(argv0).file_name()?.to_str()? {
            "claude" => Some(Agent::Claude),
            "codex" => Some(Agent::Codex),
            _ => None,
        },
    }
}

fn agent_of(s: &SessionInfo) -> Option<Agent> {
    agent_named(
        s.profile.as_deref(),
        s.argv.first().map_or("", String::as_str),
    )
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

fn room_of(chat: &Chat, s: &SessionInfo) -> RoomRef {
    if s.manager {
        chat.manager_room()
    } else {
        chat.session_room(&s.id)
    }
}

const MANAGER_DEDUPE: u32 = 50;

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
    if s.manager {
        if turn.user {
            return Ok(());
        }
        let said = super::normalize(&text);
        let posted = chat
            .history(room, None, Some(MANAGER_DEDUPE))
            .unwrap_or_default()
            .iter()
            .rev()
            .take_while(|m| m.author.kind != AuthorKind::Session)
            .any(|m| m.author.kind == AuthorKind::Manager && super::normalize(&m.body_md) == said);
        if posted {
            return Ok(());
        }
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
        mention: None,
    })
    .map(|_| ())
}

fn backfill(
    chat: &Chat,
    s: &SessionInfo,
    agent: Agent,
    t: &mut Tail,
    end: u64,
    legacy: bool,
) -> bool {
    let room = room_of(chat, s);
    let Ok(start) = tail::backfill_start(&t.path, end, BACKFILL_BYTES) else {
        return false;
    };
    if start >= end {
        return true;
    }
    let window = usize::try_from(end - start).unwrap_or(usize::MAX);
    let Ok(batch) = tail::read_new(&t.path, agent, start, window) else {
        return false;
    };
    update_meta(&mut t.meta, s.profile.as_deref(), &batch.usage);
    let known: HashSet<String> = chat
        .history(&room, None, Some(200))
        .unwrap_or_default()
        .into_iter()
        .filter(|m| matches!(m.author.kind, AuthorKind::Session | AuthorKind::Owner))
        .map(|m| super::normalize(&m.body_md))
        .collect();
    let skip = batch.turns.len().saturating_sub(BACKFILL_TURNS);
    for turn in batch.turns.into_iter().skip(skip) {
        if known.contains(&super::normalize(&clip(turn.text.clone()))) {
            continue;
        }
        if post(chat, &room, s, &t.meta, turn).is_err() {
            return false;
        }
    }
    t.offset = if legacy {
        end.max(batch.offset)
    } else {
        batch.offset
    };
    true
}

fn resolve_log(s: &SessionInfo, agent: Agent, home: &Path) -> Option<PathBuf> {
    switch::log_of(s, agent).ok().or_else(|| {
        if !s.no_sandbox || agent != Agent::Claude {
            return None;
        }
        crate::attach::locate_unsandboxed_claude(
            home,
            &s.cwd,
            s.child_pid,
            s.created_ms.saturating_sub(PID_FILE_SLACK_MS),
        )
        .map(|(path, _)| path)
    })
}

pub fn capture(chat: &Chat, sessions: &[SessionInfo]) -> Vec<(String, String)> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let consent = tmgr_usage::consent::consent_read_only(&tmgr_usage::paths::Env::from_process());
    capture_consenting(
        chat,
        sessions,
        &|s, a| resolve_log(s, a, home.as_deref()?),
        &|agent| {
            let tool = match agent {
                Agent::Claude => tmgr_usage::Tool::Claude,
                Agent::Codex => tmgr_usage::Tool::Codex,
            };
            consent.as_ref().is_some_and(|c| c.allows(tool, false))
        },
    )
}

fn activity_of(room: &RoomRef, s: &SessionInfo, scan: Option<&StatusScan>) -> ChatActivity {
    let working = s.state.kind == StateKind::Working;
    let short = |label: &str| crate::attach::status::mask_secrets(label);
    let current = scan
        .filter(|_| working)
        .and_then(|sc| sc.current_event().map(str::to_string))
        .map(|l| short(&l));
    ChatActivity {
        v: 1,
        room: room.clone(),
        state: if working {
            ActivityState::Working
        } else {
            ActivityState::Idle
        },
        current,
        used: scan.map_or_else(Vec::new, |sc| sc.used().iter().map(|l| short(l)).collect()),
    }
}

fn title_scan(agent: Agent, path: &Path) -> Option<TitleScan> {
    if agent != Agent::Claude {
        return None;
    }
    let root = path.parent()?.to_path_buf();
    Some(TitleScan::new(
        Format::Claude,
        root,
        path.to_path_buf(),
        None,
    ))
}

pub fn capture_with(
    chat: &Chat,
    sessions: &[SessionInfo],
    resolve: &dyn Fn(&SessionInfo, Agent) -> Option<PathBuf>,
) -> Vec<(String, String)> {
    capture_consenting(chat, sessions, resolve, &|_| false)
}

pub fn capture_consenting(
    chat: &Chat,
    sessions: &[SessionInfo],
    resolve: &dyn Fn(&SessionInfo, Agent) -> Option<PathBuf>,
    reading_allowed: &dyn Fn(Agent) -> bool,
) -> Vec<(String, String)> {
    chat.set_activity_consent(reading_allowed(Agent::Claude) || reading_allowed(Agent::Codex));
    let mut titles = Vec::new();
    let Some(store) = chat.store.as_ref() else {
        return titles;
    };
    let dir = store.dir();
    let mut tails = lock(&chat.tails);
    for s in sessions {
        let Some(agent) = agent_of(s) else {
            continue;
        };
        let exited = s.state.kind == StateKind::Exited;
        if exited && !tails.contains_key(&s.id) {
            continue;
        }
        let room = room_of(chat, s);
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
                        title: title_scan(agent, &path),
                        status: None,
                        sent: None,
                        path,
                        offset,
                        resolve_at: Instant::now() + RESOLVE_EVERY,
                        meta: SessionMeta::default(),
                    };
                    if let Some((end, legacy)) = backfill_from {
                        if backfill(chat, s, agent, &mut t, end, legacy) {
                            lock(tried()).remove(&s.id);
                        } else {
                            let n = {
                                let mut m = lock(tried());
                                let n = m.entry(s.id.clone()).or_insert(0);
                                *n += 1;
                                *n
                            };
                            warn_once(
                                format!("backfill-{}", s.id),
                                &format!("chat: history of session {} not read, will retry", s.id),
                            );
                            if n < BACKFILL_TRIES {
                                continue;
                            }
                            lock(tried()).remove(&s.id);
                        }
                    }
                    save(dir, &s.id, &t);
                    tails.insert(s.id.clone(), t);
                } else if let Some(t) = tails.get_mut(&s.id) {
                    t.resolve_at = Instant::now() + RESOLVE_EVERY;
                }
            } else if let Some(t) = tails.get_mut(&s.id) {
                t.resolve_at = Instant::now() + RESOLVE_EVERY;
            } else if super::now_ms().saturating_sub(s.created_ms) > NO_LOG_WARN_MS {
                warn_once(
                    format!("nolog-{}", s.id),
                    &format!("chat: no conversation file found for session {}", s.id),
                );
            }
        }
        let Some(t) = tails.get_mut(&s.id) else {
            continue;
        };
        if let Some(scan) = t.title.as_mut() {
            scan.poll();
            if let Some(found) = scan.title("").filter(|_| scan.caught_up()) {
                titles.push((s.id.clone(), found.text));
            }
        }
        if reading_allowed(agent) {
            if t.status.is_none() && agent == Agent::Claude {
                t.status = t.path.parent().map(|root| {
                    let mut scan = StatusScan::new(root.to_path_buf(), t.path.clone());
                    scan.poll();
                    scan.take_finished();
                    scan
                });
            }
            let finished = t.status.as_mut().and_then(|scan| {
                scan.poll();
                scan.take_finished()
            });
            let event = match finished {
                Some(done) => ChatActivity {
                    v: 1,
                    room: room.clone(),
                    state: ActivityState::Idle,
                    current: None,
                    used: done
                        .iter()
                        .map(|l| crate::attach::status::mask_secrets(l))
                        .collect(),
                },
                None => activity_of(&room, s, t.status.as_ref()),
            };
            if t.sent.as_ref() != Some(&event) {
                chat.publish_activity(&event);
                t.sent = Some(event);
            }
        } else {
            t.status = None;
            if t.sent
                .take()
                .is_some_and(|prev| prev != super::activity::blank_idle(&room))
            {
                chat.retire_activity(&room);
            }
        }
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
    titles
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
    fn the_agent_title_follows_the_transcript_and_a_custom_one_wins() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let log = t.path().join("log.jsonl");
        append(&log, &line("hi"));
        let s = [info(SID, "claude-code", "working")];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        assert!(capture_with(&chat, &s, &resolve).is_empty());
        append(
            &log,
            "{\"type\":\"ai-title\",\"aiTitle\":\"Fix the build\"}\n",
        );
        assert_eq!(
            capture_with(&chat, &s, &resolve),
            [(SID.to_string(), "Fix the build".to_string())]
        );
        append(
            &log,
            "{\"type\":\"custom-title\",\"customTitle\":\"Mine\"}\n",
        );
        assert_eq!(
            capture_with(&chat, &s, &resolve),
            [(SID.to_string(), "Mine".to_string())]
        );
    }

    fn tool_line(id: &str, name: &str, input: serde_json::Value) -> String {
        json!({"type": "assistant", "message": {"id": format!("m{id}"), "content": [
            {"type": "tool_use", "id": id, "name": name, "input": input}]}})
        .to_string()
            + "\n"
    }

    fn drain(rx: &super::super::ActivityRx) -> Vec<ChatActivity> {
        rx.take(Duration::ZERO)
    }

    #[test]
    fn activity_needs_consent_and_a_revoked_one_drops_the_scan() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let rx = chat.subscribe_activity();
        let log = t.path().join("log.jsonl");
        append(&log, &line("hi"));
        let s = [info(SID, "claude-code", "working")];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        let allowed = Cell::new(false);
        let consent = |_: Agent| allowed.get();
        capture_consenting(&chat, &s, &resolve, &consent);
        append(&log, &tool_line("a", "Bash", json!({"command": "ls"})));
        capture_consenting(&chat, &s, &resolve, &consent);
        assert!(drain(&rx).is_empty());
        assert!(lock(&chat.tails).get(SID).unwrap().status.is_none());

        allowed.set(true);
        capture_consenting(&chat, &s, &resolve, &consent);
        let got = drain(&rx);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].room, chat.session_room(SID));
        assert_eq!(got[0].state, ActivityState::Working);
        assert_eq!(got[0].current.as_deref(), Some("Bash: ls"));
        assert_eq!(got[0].used, ["Bash: ls"]);
        capture_consenting(&chat, &s, &resolve, &consent);
        assert!(drain(&rx).is_empty());

        allowed.set(false);
        capture_consenting(&chat, &s, &resolve, &consent);
        assert!(lock(&chat.tails).get(SID).unwrap().status.is_none());
        let last = drain(&rx);
        assert_eq!(last.len(), 1);
        assert!(last[0].current.is_none() && last[0].used.is_empty());
        append(&log, &tool_line("b", "Bash", json!({"command": "rm"})));
        capture_consenting(&chat, &s, &resolve, &consent);
        assert!(drain(&rx).is_empty());
    }

    #[test]
    fn activity_is_never_a_chat_message_and_never_reaches_chat_subscribers() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let pushes = chat.subscribe();
        let log = t.path().join("log.jsonl");
        append(&log, &line("hi"));
        let s = [info(SID, "claude-code", "working")];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        capture_consenting(&chat, &s, &resolve, &|_| true);
        let before = bodies(&chat, SID);
        let rx = chat.subscribe_activity();
        append(&log, &tool_line("a", "Bash", json!({"command": "ls"})));
        capture_consenting(&chat, &s, &resolve, &|_| true);
        assert_eq!(drain(&rx).len(), 1);
        assert_eq!(bodies(&chat, SID), before);
        assert!(pushes.try_iter().all(|p| p.message.body_md != "Bash: ls"));
        assert_eq!(pushes.try_iter().count(), 0);
    }

    #[test]
    fn the_idle_state_has_no_current_tool_and_a_codex_session_gets_state_only() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let rx = chat.subscribe_activity();
        let log = t.path().join("log.jsonl");
        append(&log, &line("hi"));
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        let claude = [info(SID, "claude-code", "working")];
        append(&log, &tool_line("a", "Bash", json!({"command": "ls"})));
        capture_consenting(&chat, &claude, &resolve, &|_| true);
        drain(&rx);
        let waiting = [info(SID, "claude-code", "waiting")];
        capture_consenting(&chat, &waiting, &resolve, &|_| true);
        let got = drain(&rx);
        assert_eq!(got[0].state, ActivityState::Idle);
        assert_eq!(got[0].current, None);

        let codex_id = "s_codexcodex";
        let codex = [info(codex_id, "codex", "working")];
        capture_consenting(&chat, &codex, &resolve, &|_| true);
        let got = drain(&rx);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].state, ActivityState::Working);
        assert_eq!(got[0].current, None);
        assert!(got[0].used.is_empty());
        assert!(lock(&chat.tails).get(codex_id).unwrap().status.is_none());
    }

    #[test]
    fn activity_labels_are_short_masked_and_carry_no_contents_or_output() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let rx = chat.subscribe_activity();
        let log = t.path().join("log.jsonl");
        append(&log, &line("hi"));
        let s = [info(SID, "claude-code", "working")];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        capture_consenting(&chat, &s, &resolve, &|_| true);
        let long = "x".repeat(500);
        append(
            &log,
            &tool_line(
                "a",
                "Bash",
                json!({"command": format!("curl -H 'Authorization: Bearer abc123' {long}")}),
            ),
        );
        append(
            &log,
            &tool_line(
                "b",
                "Write",
                json!({"file_path": "/x/notes.txt", "content": "FILECONTENTS"}),
            ),
        );
        append(
            &log,
            &(json!({"type": "user", "message": {"content": [
                {"type": "tool_result", "tool_use_id": "b", "content": "TOOLOUTPUT"}]}})
            .to_string()
                + "\n"),
        );
        capture_consenting(&chat, &s, &resolve, &|_| true);
        let got = drain(&rx);
        let wire = serde_json::to_string(&got).unwrap();
        for leaked in ["abc123", "FILECONTENTS", "TOOLOUTPUT"] {
            assert!(!wire.contains(leaked), "{leaked}");
        }
        assert!(got[0].used.len() == 2 && got[0].used[1] == "Write: notes.txt");
        assert!(got[0]
            .used
            .iter()
            .all(|l| l.chars().count() <= crate::attach::status::ACTIVITY_MAX));
    }

    #[test]
    fn revoking_consent_sends_one_blank_idle_event_and_nothing_else() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let rx = chat.subscribe_activity();
        let log = t.path().join("log.jsonl");
        append(&log, &line("hi"));
        let s = [info(SID, "claude-code", "working")];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        let allowed = Cell::new(true);
        let consent = |_: Agent| allowed.get();
        capture_consenting(&chat, &s, &resolve, &consent);
        append(&log, &tool_line("a", "Bash", json!({"command": "ls"})));
        capture_consenting(&chat, &s, &resolve, &consent);
        allowed.set(false);
        capture_consenting(&chat, &s, &resolve, &consent);
        let got = drain(&rx);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].room, chat.session_room(SID));
        assert_eq!(got[0].state, ActivityState::Idle);
        assert_eq!(got[0].current, None);
        assert!(got[0].used.is_empty());
        capture_consenting(&chat, &s, &resolve, &consent);
        assert!(drain(&rx).is_empty());
    }

    #[test]
    fn tools_and_the_final_answer_in_one_window_still_report_the_finished_turns_tools() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let rx = chat.subscribe_activity();
        let log = t.path().join("log.jsonl");
        append(&log, &line("hi"));
        let s = [info(SID, "claude-code", "working")];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        capture_consenting(&chat, &s, &resolve, &|_| true);
        drain(&rx);
        let mut chunk = tool_line("a", "Bash", json!({"command": "cargo test -p x"}));
        chunk += &(json!({"type": "user", "message": {"content": [
            {"type": "tool_result", "tool_use_id": "a", "content": "ok"}]}})
        .to_string()
            + "\n");
        chunk += &(json!({"type": "assistant", "message": {"id": "fin", "stop_reason": "end_turn",
            "content": [{"type": "text", "text": "done"}]}})
        .to_string()
            + "\n");
        append(&log, &chunk);
        capture_consenting(&chat, &s, &resolve, &|_| true);
        let got = drain(&rx);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].state, ActivityState::Idle);
        assert_eq!(got[0].current, None);
        assert_eq!(got[0].used, ["Bash: cargo test"]);
        capture_consenting(&chat, &s, &resolve, &|_| true);
        let after = drain(&rx);
        assert!(after.iter().all(|e| e.used.is_empty()));
    }

    #[test]
    fn a_finished_turn_already_in_the_log_when_the_scan_starts_is_not_replayed() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let rx = chat.subscribe_activity();
        let log = t.path().join("log.jsonl");
        append(&log, &line("hi"));
        append(&log, &tool_line("a", "Bash", json!({"command": "ls"})));
        append(
            &log,
            &(json!({"type": "assistant", "message": {"id": "fin", "stop_reason": "end_turn",
                "content": [{"type": "text", "text": "done"}]}})
            .to_string()
                + "\n"),
        );
        let s = [info(SID, "claude-code", "waiting")];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        capture_consenting(&chat, &s, &resolve, &|_| true);
        assert!(drain(&rx).iter().all(|e| e.used.is_empty()));
    }

    fn manager_info() -> SessionInfo {
        let mut s = info(SID, "claude-code", "working");
        s.manager = true;
        s
    }

    fn manager_bodies(chat: &Chat) -> Vec<String> {
        chat.history(&chat.manager_room(), None, None)
            .unwrap()
            .into_iter()
            .map(|m| m.body_md)
            .collect()
    }

    #[test]
    fn a_manager_sessions_replies_land_in_the_manager_room_without_user_turns() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let log = t.path().join("log.jsonl");
        append(&log, &line("old"));
        let s = [manager_info()];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        capture_with(&chat, &s, &resolve);
        append(
            &log,
            &(json!({"type": "user", "message": {"content": "do it"}}).to_string() + "\n"),
        );
        append(&log, &line("done"));
        capture_with(&chat, &s, &resolve);
        assert_eq!(manager_bodies(&chat), ["old", "done"]);
        assert!(bodies(&chat, SID).is_empty());
    }

    #[test]
    fn a_manager_reply_already_posted_with_post_message_is_not_duplicated() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let log = t.path().join("log.jsonl");
        append(&log, &line("old"));
        let s = [manager_info()];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        capture_with(&chat, &s, &resolve);
        chat.post(
            chat.manager_room(),
            Author {
                kind: AuthorKind::Manager,
                id: "manager".into(),
                machine: None,
                device: None,
                agent: None,
            },
            "All sessions are idle".into(),
            None,
            Vec::new(),
        )
        .unwrap();
        append(&log, &line("All sessions are idle"));
        capture_with(&chat, &s, &resolve);
        assert_eq!(manager_bodies(&chat), ["old", "All sessions are idle"]);
    }

    #[test]
    fn a_repeated_manager_reply_is_kept() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let log = t.path().join("log.jsonl");
        append(&log, &line("old"));
        let s = [manager_info()];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        capture_with(&chat, &s, &resolve);
        append(&log, &line("Done."));
        capture_with(&chat, &s, &resolve);
        append(&log, &line("Done."));
        capture_with(&chat, &s, &resolve);
        assert_eq!(manager_bodies(&chat), ["old", "Done.", "Done."]);
    }

    #[test]
    fn a_raw_claude_command_without_a_profile_still_gets_its_history() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let log = t.path().join("log.jsonl");
        append(&log, &line("first"));
        let mut s = info(SID, "claude-code", "working");
        s.profile = None;
        s.argv = vec![
            "/usr/local/bin/claude".into(),
            "--permission-mode".into(),
            "bypassPermissions".into(),
        ];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        capture_with(&chat, &[s], &resolve);
        assert_eq!(bodies(&chat, SID), ["first"]);
    }

    #[test]
    fn a_session_without_the_sandbox_is_read_through_its_pid_file() {
        let t = tempfile::tempdir().unwrap();
        let home = t.path().join("home");
        let id = "2f0e1c3a-5b7d-4e8f-9a1b-3c4d5e6f7a8b";
        let cwd = "/work/app";
        let proj = home.join(".claude/projects").join("-work-app");
        fs::create_dir_all(&proj).unwrap();
        fs::create_dir_all(home.join(".claude/sessions")).unwrap();
        fs::write(
            home.join(".claude/sessions/2.json"),
            json!({"pid": 2, "sessionId": id}).to_string(),
        )
        .unwrap();
        append(&proj.join(format!("{id}.jsonl")), &line("hello"));
        let mut s = info(SID, "claude-code", "working");
        s.no_sandbox = true;
        s.cwd = cwd.into();
        let chat = Chat::open(&t.path().join("run"));
        capture_with(&chat, &[s.clone()], &|s, a| resolve_log(s, a, &home));
        append(&proj.join(format!("{id}.jsonl")), &line("more"));
        capture_with(&chat, &[s], &|s, a| resolve_log(s, a, &home));
        assert_eq!(bodies(&chat, SID), ["hello", "more"]);
    }

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
        let room = chat.rooms(&s, None).remove(1);
        assert_eq!(room.context_pct, Some(20));
        assert_eq!(room.context_tokens, Some(40_000));
        assert_eq!(room.model.as_deref(), Some("claude-opus-4-1"));
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
    fn a_log_over_the_window_starts_at_a_line_boundary_and_codex_user_turns_count() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let log = t.path().join("log.jsonl");
        append(&log, &line(&"x".repeat((BACKFILL_BYTES as usize) + 10)));
        append(&log, &(user_line("recent q") + &line("recent a")));
        capture_with(&chat, &[info(SID, "claude-code", "working")], &|_, _| {
            Some(log.clone())
        });
        assert_eq!(
            kinds(&chat, SID),
            [
                (true, "recent q".to_string()),
                (false, "recent a".to_string())
            ]
        );
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
    fn an_old_saved_state_at_offset_zero_does_not_read_twice() {
        let t = tempfile::tempdir().unwrap();
        let log = t.path().join("log.jsonl");
        append(&log, &(user_line("q1") + &line("a1")));
        let chat = Chat::open(&t.path().join("run"));
        let dir = chat.store.as_ref().unwrap().dir().to_path_buf();
        fs::write(
            state_path(&dir, SID),
            serde_json::to_vec(&json!({"path": log, "offset": 0})).unwrap(),
        )
        .unwrap();
        let s = [info(SID, "claude-code", "working")];
        capture_with(&chat, &s, &|_, _| Some(log.clone()));
        assert_eq!(kinds(&chat, SID).len(), 2);
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
    fn a_relayed_prompt_is_not_echoed_as_a_second_message() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(&t.path().join("run"));
        let log = t.path().join("log.jsonl");
        let s = [info(SID, "claude-code", "working")];
        let resolve = |_: &SessionInfo, _: Agent| Some(log.clone());
        capture_with(&chat, &s, &resolve);
        let typed = tmgr_ipc::manager::wrap_untrusted("a phone", "d_phone1", "run it");
        let m = crate::chat::Incoming {
            room: chat.session_room(SID),
            id: tmgr_ipc::chat::message_id_at(1, [7; 10]),
            ts_ms: 5,
            body_md: "run it".into(),
            reply_to: None,
            attachments: Vec::new(),
            device: Some("d_phone1".into()),
            client: "relay".into(),
        };
        let typed_back = typed.clone();
        chat.ingest(
            m,
            5,
            |_| true,
            move |_| crate::chat::Delivery::Typed(typed_back),
        )
        .unwrap();
        append(&log, &user_line(&typed));
        capture_with(&chat, &s, &resolve);
        assert_eq!(kinds(&chat, SID), [(false, "run it".to_string())]);
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
