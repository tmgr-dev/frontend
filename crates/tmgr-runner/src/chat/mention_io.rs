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
        MentionState::NeedsConfirmation => format!("Waiting for confirmation to ask @{who}"),
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

    fn persist_open(&self) {
        let Some(path) = self.open_path() else { return };
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
            return;
        }
        let tmp = path.with_extension("json.tmp");
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
        if let Err(e) = write() {
            crate::daemon::log(&format!("chat: open mentions not saved: {e}"));
        }
    }

    pub(super) fn expire_open_mentions(&self) {
        let Some(path) = self.open_path() else { return };
        let Ok(bytes) = fs::read(&path) else { return };
        let rows: Vec<OpenRow> = serde_json::from_slice(&bytes).unwrap_or_default();
        let _ = fs::remove_file(&path);
        for r in rows {
            let spec = Spec {
                id: r.id,
                origin: r.origin,
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
        if body.len() > limits::MAX_BODY {
            body.truncate(limits::MAX_BODY);
        }
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
        self.persist_open();
    }

    pub fn mention_admit(&self, now: u64, origin: &RoomRef) -> Result<(), MentionState> {
        lock(&self.mentions).admit(now, origin)
    }

    pub fn mention_target_busy(&self, target: &RoomRef) -> bool {
        lock(&self.mentions).target_busy(target)
    }

    pub fn mention_open(&self, spec: Spec) {
        lock(&self.mentions).open(spec);
        self.persist_open();
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

    pub fn mention_prepare(&self, id: &str, work_base: u64) {
        lock(&self.mentions).prepare(id, work_base);
    }

    pub fn mention_delivered(&self, id: &str, typed: &str, now: u64) {
        let t = lock(&self.mentions).delivered(id, typed, now);
        self.apply_mentions(t.into_iter().collect());
    }

    pub fn mention_set_gen(&self, id: &str, gen: u64) {
        lock(&self.mentions).set_gen(id, gen);
    }

    pub fn mention_observe(&self, room: &RoomRef, user: bool, text: &str) {
        lock(&self.mentions).observe(room, user, text);
    }

    pub fn mention_pass(&self, room: &RoomRef, caught_up: bool) {
        let ts = lock(&self.mentions).pass(room, caught_up);
        self.apply_mentions(ts);
    }

    pub(super) fn mention_observe_manager_row(&self, msg: &ChatMessage) {
        if msg.room.kind == RoomKind::Manager && msg.author.kind == AuthorKind::Manager {
            self.mention_observe(&msg.room, false, &msg.body_md);
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
