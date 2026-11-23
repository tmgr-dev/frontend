use super::mention::{Mentions, Probe, QueueEnd, Spec, Transition, Via};
use super::{lock, now_ms, Chat};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use tmgr_ipc::chat::{
    limits, Author, AuthorKind, ChatMessage, MentionRef, MentionState, MessageKind, RoomKind,
    RoomRef, CHAT_V,
};

const OPEN_FILE: &str = "mentions-open.json";

#[derive(Serialize, Deserialize)]
struct OpenRow {
    id: String,
    origin: RoomRef,
    target: RoomRef,
    title: String,
    hop: u8,
}

pub fn mention_ref(spec: &Spec, state: MentionState) -> MentionRef {
    MentionRef {
        id: spec.id.clone(),
        target: spec.target.clone(),
        target_title: spec.title.clone(),
        state,
        hop: spec.hop,
    }
}

fn state_text(t: &Transition) -> String {
    let who = &t.spec.title;
    let default = match t.state {
        MentionState::Queued => format!("Queued, waiting for @{who} to be free"),
        MentionState::Sent => format!("Sent to @{who}"),
        MentionState::Answering => format!("@{who} is answering"),
        MentionState::Answered => format!("Reply from @{who}"),
        MentionState::Busy => format!("@{who} is busy, try again later"),
        MentionState::Offline => format!("@{who} is offline"),
        MentionState::Timeout => format!("No answer from @{who} in 10 minutes"),
        MentionState::Interrupted => {
            format!("Interrupted: someone else typed into @{who}")
        }
        MentionState::Expired => "Expired".to_string(),
        MentionState::Refused => format!("@{who} cannot be asked"),
        MentionState::RateLimited => "Too many mentions, wait a minute".to_string(),
        MentionState::Cancelled => "Cancelled".to_string(),
        MentionState::Other => String::new(),
    };
    match &t.reason {
        Some(r) => format!("{default}: {r}"),
        None => default,
    }
}

impl Chat {
    fn open_path(&self) -> Option<PathBuf> {
        self.store.as_ref().map(|s| s.dir().join(OPEN_FILE))
    }

    fn persist_open(&self) -> std::io::Result<()> {
        let Some(path) = self.open_path() else {
            return Ok(());
        };
        let _serial = lock(&self.persist_lock);
        let rows: Vec<OpenRow> = lock(&self.mentions)
            .open_specs()
            .into_iter()
            .map(|s| OpenRow {
                id: s.id,
                origin: s.origin,
                target: s.target,
                title: s.title,
                hop: s.hop,
            })
            .collect();
        if rows.is_empty() {
            let _ = fs::remove_file(&path);
            return Ok(());
        }
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let tmp = path.with_extension(format!(
            "json.{}.{}.tmp",
            std::process::id(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let write = || -> std::io::Result<()> {
            let mut f = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&tmp)?;
            f.write_all(&serde_json::to_vec(&rows).unwrap_or_default())?;
            fs::rename(&tmp, &path)
        };
        let r = write();
        if let Err(e) = &r {
            crate::daemon::log(&format!("chat: open mentions not saved: {e}"));
        }
        r
    }

    pub(super) fn expire_open_mentions(&self) {
        let Some(path) = self.open_path() else { return };
        let Ok(bytes) = fs::read(&path) else { return };
        let rows: Vec<OpenRow> = serde_json::from_slice(&bytes).unwrap_or_default();
        for r in rows {
            let spec = Spec {
                id: r.id,
                origin: r.origin,
                origin_session: None,
                target: r.target,
                title: r.title,
                hop: r.hop,
                question: String::new(),
                via: Via::Local {
                    client: String::new(),
                },
                confirmed_risky: false,
            };
            self.emit_mention(&Transition {
                spec,
                state: MentionState::Expired,
                reason: None,
                reply: None,
            });
        }
        let _ = fs::remove_file(&path);
    }

    fn emit_mention(&self, t: &Transition) {
        let mut body = state_text(t);
        let (kind, author) = match (&t.state, &t.reply) {
            (MentionState::Answered, Some(reply)) => {
                body = format!("↩ @{}: {reply}", t.spec.title);
                let session = t.spec.target.session.clone();
                (
                    MessageKind::Text,
                    Author {
                        kind: AuthorKind::Session,
                        id: session.unwrap_or_else(|| "manager".into()),
                        machine: Some(self.machine.clone()),
                        device: None,
                        agent: None,
                    },
                )
            }
            _ => (
                MessageKind::System,
                Author {
                    kind: AuthorKind::System,
                    id: "runner".into(),
                    machine: None,
                    device: None,
                    agent: None,
                },
            ),
        };
        let body = super::mention::clip_to(&body, limits::MAX_BODY);
        let msg = ChatMessage {
            v: CHAT_V,
            id: self.mint_id(),
            room: t.spec.origin.clone(),
            author,
            ts_ms: now_ms(),
            kind,
            body_md: body,
            attachments: Vec::new(),
            reply_to: Some(t.spec.id.clone()),
            decision: None,
            session_meta: None,
            user_turn: false,
            mention: Some(mention_ref(&t.spec, t.state)),
        };
        if let Err(e) = self.append(msg) {
            crate::daemon::log(&format!("chat: mention state not stored: {e}"));
        }
    }

    fn apply_mentions(&self, ts: Vec<Transition>) {
        if ts.is_empty() {
            return;
        }
        for t in &ts {
            self.emit_mention(t);
        }
        let _ = self.persist_open();
    }

    pub fn mention_admit(&self, now: u64, origin: &RoomRef) -> Result<(), MentionState> {
        lock(&self.mentions).admit(now, origin)
    }

    pub fn mention_target_busy(&self, target: &RoomRef) -> bool {
        lock(&self.mentions).target_busy(target)
    }

    pub fn mention_open(&self, spec: Spec) -> bool {
        let fresh = lock(&self.mentions).open(spec.clone());
        if !fresh {
            self.emit_mention(&Transition {
                spec,
                state: MentionState::Refused,
                reason: Some("that message id is already in use".into()),
                reply: None,
            });
            return false;
        }
        let _ = self.persist_open();
        true
    }

    pub fn mention_finish(&self, spec_id: &str, state: MentionState, reason: Option<String>) {
        let t = lock(&self.mentions).finish(spec_id, state, reason);
        self.apply_mentions(t.into_iter().collect());
    }

    pub(super) fn mention_queue_end(&self, id: &str, end: QueueEnd) {
        let t = lock(&self.mentions).queue_end(id, end);
        self.apply_mentions(t.into_iter().collect());
    }

    pub fn is_mention_open(&self, id: &str) -> bool {
        lock(&self.mentions).is_open(id)
    }

    pub fn mention_spec(&self, id: &str) -> Option<Spec> {
        lock(&self.mentions).get(id).map(|e| e.spec.clone())
    }

    pub fn mention_prepare(&self, id: &str, work_base: u64, anchor_pos: u64) {
        lock(&self.mentions).prepare(id, work_base, anchor_pos);
    }

    pub fn mention_delivered(&self, id: &str, typed: &str, now: u64) {
        let t = lock(&self.mentions).delivered(id, typed, now);
        self.apply_mentions(t.into_iter().collect());
    }

    pub fn mention_set_gen(&self, id: &str, gen: u64) {
        lock(&self.mentions).set_gen(id, gen);
    }

    pub fn mention_observe(&self, room: &RoomRef, user: bool, text: &str, end: Option<u64>) {
        lock(&self.mentions).observe(room, user, text, end);
    }

    pub fn mention_pass(&self, room: &RoomRef, caught_up: bool) {
        let ts = lock(&self.mentions).pass(room, caught_up);
        self.apply_mentions(ts);
    }

    pub(super) fn mention_observe_manager_row(&self, msg: &ChatMessage) {
        if msg.room.kind == RoomKind::Manager && msg.author.kind == AuthorKind::Manager {
            self.mention_observe(&msg.room, false, &msg.body_md, None);
        }
    }

    pub fn mention_tick(&self, now: u64, probe: &dyn Fn(&RoomRef) -> Option<Probe>) {
        let ts = lock(&self.mentions).tick(now, probe);
        self.apply_mentions(ts);
    }

    pub fn mention_reply(&self, id: &str, origin: &RoomRef) -> Option<(Spec, String)> {
        lock(&self.mentions)
            .reply(id, origin)
            .map(|(s, t)| (s.clone(), t.to_string()))
    }

    pub fn tail_len(&self, session: &str) -> u64 {
        lock(&self.tails)
            .get(session)
            .and_then(|t| t.file_len())
            .unwrap_or(0)
    }

    pub fn has_tail(&self, session: &str) -> bool {
        lock(&self.tails).contains_key(session)
    }

    #[cfg(test)]
    pub(crate) fn add_placeholder_tail(&self, session: &str) {
        lock(&self.tails).insert(session.into(), super::tail::Tail::placeholder());
    }
}

pub(super) fn new_mentions() -> std::sync::Mutex<Mentions> {
    std::sync::Mutex::new(Mentions::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(title: &str) -> Spec {
        let room = |s: &str| RoomRef::session("mx_0000000000000001", s);
        Spec {
            id: "m_0190a1b2c3d4aabbccddeeff00112233".into(),
            origin: room("s_originaaaa"),
            origin_session: None,
            target: room("s_targetaaaa"),
            title: title.into(),
            hop: 1,
            question: "q".into(),
            via: Via::Local {
                client: "cli".into(),
            },
            confirmed_risky: false,
        }
    }

    #[test]
    fn a_long_multibyte_reason_is_truncated_on_a_char_boundary() {
        for title in ["a", "ab"] {
            let t = tempfile::tempdir().unwrap();
            let c = Chat::open(t.path());
            let mut sp = spec(title);
            sp.origin = c.session_room("s_originaaaa");
            sp.target = c.session_room("s_targetaaaa");
            c.mention_open(sp.clone());
            c.mention_finish(&sp.id, MentionState::Refused, Some("é".repeat(40_000)));
            let rows = c.history(&sp.origin, None, None).unwrap();
            assert!(rows[0].body_md.len() <= limits::MAX_BODY);
        }
    }

    #[test]
    fn concurrent_open_mention_saves_never_fail_and_leave_no_temp_file() {
        let t = tempfile::tempdir().unwrap();
        let c = std::sync::Arc::new(Chat::open(t.path()));
        let mut hs = Vec::new();
        for n in 0..8u8 {
            let c = c.clone();
            hs.push(std::thread::spawn(move || {
                let mut ok = true;
                for k in 0..40u8 {
                    let mut sp = spec("x");
                    sp.id = format!("m_0190a1b2c3d4aabbccddeeff0011{n:02x}{k:02x}");
                    sp.origin = c.session_room("s_originaaaa");
                    sp.target = c.session_room("s_targetaaaa");
                    c.mention_open(sp);
                    ok &= c.persist_open().is_ok();
                }
                ok
            }));
        }
        for h in hs {
            assert!(h.join().unwrap(), "a save failed under contention");
        }
        let dir = c.store.as_ref().unwrap().dir().to_path_buf();
        let names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|n| !n.ends_with(".tmp")), "{names:?}");
        let rows: Vec<serde_json::Value> =
            serde_json::from_slice(&std::fs::read(c.open_path().unwrap()).unwrap()).unwrap();
        assert_eq!(rows.len(), 8 * 40);
    }
}
