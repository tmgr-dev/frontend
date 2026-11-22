mod activity;
pub mod queue;
pub mod relay;
pub mod store;
pub mod tail;

pub use activity::ActivityRx;
pub use queue::Submit;
use std::collections::{HashMap, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use store::Store;
use tmgr_ipc::api::{SessionInfo, StateKind};
use tmgr_ipc::chat::{
    message_id_at, valid_message_id, Attachment, Author, AuthorKind, ChatActivity, ChatMessage,
    ChatPush, MessageKind, RelaySelf, RoomInfo, RoomRef, CHAT_V,
};

use crate::mgr_rpc::relay::RpcClient;

const MACHINE_FILE: &str = "machine.id";
const SUBSCRIBER_QUEUE: usize = 256;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn valid_machine_id(s: &str) -> bool {
    s.strip_prefix("mx_")
        .is_some_and(|r| r.len() == 16 && r.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')))
}

fn random_machine_id() -> String {
    let mut b = [0u8; 8];
    getrandom::fill(&mut b).expect("system random source");
    format!(
        "mx_{}",
        b.iter().map(|x| format!("{x:02x}")).collect::<String>()
    )
}

pub fn machine_id(runner_dir: &Path) -> io::Result<String> {
    let path = runner_dir.join(MACHINE_FILE);
    loop {
        match fs::read_to_string(&path) {
            Ok(text) => {
                let id = text.trim();
                return if valid_machine_id(id) {
                    Ok(id.to_string())
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "machine.id is malformed",
                    ))
                };
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let id = random_machine_id();
        let tmp = runner_dir.join(format!("{MACHINE_FILE}.{}.tmp", std::process::id()));
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp)?;
        f.write_all(id.as_bytes())?;
        f.sync_all()?;
        drop(f);
        let linked = fs::hard_link(&tmp, &path);
        let _ = fs::remove_file(&tmp);
        match linked {
            Ok(()) => return Ok(id),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

pub const FRESH_PAST_MS: u64 = 10 * 60 * 1000;
pub const FRESH_FUTURE_MS: u64 = 5 * 60 * 1000;

pub struct Incoming {
    pub room: RoomRef,
    pub id: String,
    pub ts_ms: u64,
    pub body_md: String,
    pub reply_to: Option<String>,
    pub attachments: Vec<Attachment>,
    pub device: Option<String>,
    pub client: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    Delivered,
    Typed(String),
    Busy,
    Refused(&'static str),
    Gone(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestError {
    Invalid(String),
    NotFound(String),
    Conflict(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ingested {
    pub message: ChatMessage,
    pub delivered: bool,
    pub duplicate: bool,
}

const SENT_TTL: Duration = Duration::from_secs(600);
const SENT_MAX: usize = 32;

pub(super) fn normalize(text: &str) -> String {
    text.replace('\r', "").trim().to_string()
}

pub struct ChatSub {
    pub rx: Receiver<ChatPush>,
    pub activity: Option<ActivityRx>,
}

pub struct Chat {
    machine: String,
    store: Option<Store>,
    write: Mutex<()>,
    subs: Mutex<Vec<SyncSender<ChatPush>>>,
    activity_subs: Mutex<Vec<Arc<activity::Slot>>>,
    activity_on: Arc<AtomicBool>,
    seq: AtomicU16,
    tails: Mutex<HashMap<String, tail::Tail>>,
    sent: Mutex<VecDeque<(String, String, Instant)>>,
    queue: Mutex<queue::Queue>,
    rpc_client: Mutex<Option<Arc<RpcClient>>>,
    relay_device: Mutex<Option<(String, String)>>,
}

impl Chat {
    pub fn open(runner_dir: &Path) -> Chat {
        let machine = machine_id(runner_dir).unwrap_or_else(|e| {
            crate::daemon::log(&format!(
                "chat: machine id unavailable ({e}); using a temporary one"
            ));
            random_machine_id()
        });
        let store = Store::open(runner_dir)
            .map_err(|e| crate::daemon::log(&format!("chat: store unavailable: {e}")))
            .ok();
        Chat {
            machine,
            store,
            write: Mutex::new(()),
            subs: Mutex::new(Vec::new()),
            activity_subs: Mutex::new(Vec::new()),
            activity_on: Arc::new(AtomicBool::new(false)),
            seq: AtomicU16::new(0),
            tails: Mutex::new(HashMap::new()),
            sent: Mutex::new(VecDeque::new()),
            queue: Mutex::new(queue::Queue::default()),
            rpc_client: Mutex::new(None),
            relay_device: Mutex::new(None),
        }
    }

    pub fn machine(&self) -> &str {
        &self.machine
    }

    pub fn set_rpc_client(&self, client: Option<Arc<RpcClient>>) {
        *lock(&self.rpc_client) = client;
    }

    pub fn rpc_client(&self) -> Option<Arc<RpcClient>> {
        lock(&self.rpc_client).clone()
    }

    pub fn set_relay_device(&self, device: Option<(String, String)>) {
        *lock(&self.relay_device) = device;
    }

    pub fn relay_self(&self) -> Option<RelaySelf> {
        lock(&self.relay_device)
            .clone()
            .map(|(device_id, fp)| RelaySelf {
                id: self.machine.clone(),
                device_id,
                fp,
            })
    }

    pub fn manager_room(&self) -> RoomRef {
        RoomRef::manager(&self.machine)
    }

    pub fn session_room(&self, session: &str) -> RoomRef {
        RoomRef::session(&self.machine, session)
    }

    fn store(&self) -> Result<&Store, String> {
        self.store
            .as_ref()
            .ok_or_else(|| "the chat store is unavailable".to_string())
    }

    fn own(&self, room: &RoomRef) -> Result<(), String> {
        room.validate()?;
        if room.machine != self.machine {
            return Err("the room belongs to another machine".into());
        }
        Ok(())
    }

    pub fn mint_id(&self) -> String {
        let mut r = [0u8; 10];
        getrandom::fill(&mut r).expect("system random source");
        let n = self.seq.fetch_add(1, Ordering::SeqCst).to_be_bytes();
        r[..2].copy_from_slice(&n);
        message_id_at(now_ms(), r)
    }

    pub fn post(
        &self,
        room: RoomRef,
        author: Author,
        body_md: String,
        reply_to: Option<String>,
        attachments: Vec<Attachment>,
    ) -> Result<ChatMessage, String> {
        self.own(&room)?;
        let msg = ChatMessage {
            v: CHAT_V,
            id: self.mint_id(),
            room,
            author,
            ts_ms: now_ms(),
            kind: MessageKind::Text,
            body_md,
            attachments,
            reply_to,
            decision: None,
            session_meta: None,
            user_turn: false,
        };
        self.append(msg)
    }

    pub fn append(&self, msg: ChatMessage) -> Result<ChatMessage, String> {
        self.own(&msg.room)?;
        {
            let _g = lock(&self.write);
            self.store()?.append(&msg)?;
        }
        self.publish(&msg);
        Ok(msg)
    }

    fn publish(&self, msg: &ChatMessage) {
        let push = ChatPush {
            message: msg.clone(),
        };
        lock(&self.subs).retain(|s| s.try_send(push.clone()).is_ok());
    }

    fn note(&self, room: &RoomRef, text: &str) {
        let msg = system_message(room, self.mint_id(), now_ms(), text);
        if let Err(e) = self.append(msg) {
            crate::daemon::log(&format!("chat: note not stored: {e}"));
        }
    }

    pub fn reply_system(
        &self,
        room: &RoomRef,
        reply_to: Option<&str>,
        text: &str,
    ) -> Result<ChatMessage, String> {
        let mut msg = system_message(room, self.mint_id(), now_ms(), text);
        msg.reply_to = reply_to.map(String::from);
        self.append(msg)
    }

    pub fn system_once(
        &self,
        room: &RoomRef,
        id: &str,
        ts_ms: u64,
        text: &str,
    ) -> Result<bool, String> {
        if !valid_message_id(id) {
            return Err("invalid message id".into());
        }
        self.own(room)?;
        let store = self.store()?;
        let msg = system_message(room, id.into(), ts_ms, text);
        {
            let _g = lock(&self.write);
            if store.get(room, id)?.is_some() {
                return Ok(false);
            }
            store.append(&msg)?;
        }
        self.publish(&msg);
        Ok(true)
    }

    pub fn ingest(
        &self,
        m: Incoming,
        now_ms: u64,
        known_session: impl Fn(&str) -> bool,
        deliver: impl FnOnce(&str) -> Delivery,
    ) -> Result<Ingested, IngestError> {
        use IngestError::{Conflict, Invalid, NotFound};
        if !valid_message_id(&m.id) {
            return Err(Invalid("invalid message id".into()));
        }
        self.own(&m.room).map_err(Invalid)?;
        let store = self.store().map_err(Invalid)?;
        let exists = match m.room.session.as_deref() {
            None => true,
            Some(s) => known_session(s) || store.exists(&m.room),
        };
        if !exists {
            return Err(NotFound("no such room".into()));
        }
        let (client, device) = (m.client, m.device.clone());
        let msg = ChatMessage {
            v: CHAT_V,
            id: m.id,
            room: m.room,
            author: Author {
                kind: AuthorKind::Owner,
                id: "owner".into(),
                machine: None,
                device: m.device,
                agent: None,
            },
            ts_ms: m.ts_ms,
            kind: MessageKind::Text,
            body_md: m.body_md,
            attachments: m.attachments,
            reply_to: m.reply_to,
            decision: None,
            session_meta: None,
            user_turn: false,
        };
        msg.validate().map_err(Invalid)?;
        {
            let _g = lock(&self.write);
            match store.get(&msg.room, &msg.id).map_err(Invalid)? {
                Some(old) if old == msg => {
                    return Ok(Ingested {
                        message: old,
                        delivered: false,
                        duplicate: true,
                    });
                }
                Some(_) => return Err(Conflict("that message id holds different content".into())),
                None => store.append(&msg).map_err(Invalid)?,
            }
        }
        self.publish(&msg);
        if msg.ts_ms < now_ms && now_ms - msg.ts_ms > FRESH_PAST_MS {
            self.note(&msg.room, "Not delivered: older than 10 minutes.");
            return Ok(Ingested {
                message: msg,
                delivered: false,
                duplicate: false,
            });
        }
        if msg.ts_ms > now_ms + FRESH_FUTURE_MS {
            self.note(
                &msg.room,
                "Not delivered: its time is too far ahead of this machine's clock.",
            );
            return Ok(Ingested {
                message: msg,
                delivered: false,
                duplicate: false,
            });
        }
        let mut deliver = Some(deliver);
        let outcome = self.submit(
            &msg.room,
            &msg.id,
            &msg.body_md,
            &client,
            device.as_deref(),
            now_ms,
            |body, _, _| deliver.take().map_or(Delivery::Busy, |d| d(body)),
        );
        self.note_undelivered(&msg.room, &outcome);
        let delivered = outcome == Submit::Delivered;
        Ok(Ingested {
            message: msg,
            delivered,
            duplicate: false,
        })
    }

    pub(crate) fn remember_sent(&self, session: &str, body: &str) {
        let mut sent = lock(&self.sent);
        let now = Instant::now();
        sent.retain(|(_, _, at)| now.duration_since(*at) < SENT_TTL);
        if sent.len() >= SENT_MAX {
            sent.pop_front();
        }
        sent.push_back((session.to_string(), normalize(body), now));
    }

    pub(super) fn take_sent(&self, session: &str, text: &str) -> bool {
        let text = normalize(text);
        let mut sent = lock(&self.sent);
        let now = Instant::now();
        sent.retain(|(_, _, at)| now.duration_since(*at) < SENT_TTL);
        match sent.iter().position(|(s, t, _)| s == session && *t == text) {
            Some(i) => {
                sent.remove(i);
                true
            }
            None => false,
        }
    }

    pub fn owner() -> Author {
        Author {
            kind: AuthorKind::Owner,
            id: "owner".into(),
            machine: None,
            device: None,
            agent: None,
        }
    }

    pub fn subscribe(&self) -> Receiver<ChatPush> {
        let (tx, rx) = mpsc::sync_channel(SUBSCRIBER_QUEUE);
        lock(&self.subs).push(tx);
        rx
    }

    pub fn subscribe_activity(&self) -> ActivityRx {
        let slot = Arc::new(activity::Slot::new(self.activity_on.clone()));
        let mut subs = lock(&self.activity_subs);
        subs.retain(|s| Arc::strong_count(s) > 1);
        subs.push(slot.clone());
        ActivityRx(slot)
    }

    pub(crate) fn set_activity_consent(&self, on: bool) {
        self.activity_on.store(on, Ordering::SeqCst);
        if !on {
            for s in lock(&self.activity_subs).iter() {
                s.clear();
            }
        }
    }

    pub(crate) fn retire_activity(&self, room: &RoomRef) {
        for s in lock(&self.activity_subs).iter() {
            s.forget(room);
            s.put_blank_idle(room);
        }
    }

    #[cfg(test)]
    pub(crate) fn activity_subscribers(&self) -> usize {
        lock(&self.activity_subs).len()
    }

    pub(crate) fn publish_activity(&self, event: &ChatActivity) {
        if !self.activity_on.load(Ordering::SeqCst) {
            return;
        }
        let mut subs = lock(&self.activity_subs);
        subs.retain(|s| Arc::strong_count(s) > 1);
        for s in subs.iter() {
            s.put(event);
        }
    }

    pub fn history(
        &self,
        room: &RoomRef,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<ChatMessage>, String> {
        self.own(room)?;
        self.store()?.history(room, before, limit)
    }

    pub fn search(
        &self,
        query: &str,
        room: Option<&RoomRef>,
        limit: Option<u32>,
    ) -> Result<Vec<ChatMessage>, String> {
        if let Some(r) = room {
            self.own(r)?;
        }
        self.store()?.search(query, room, limit)
    }

    pub fn rooms(&self, sessions: &[SessionInfo], unrestricted: Option<&str>) -> Vec<RoomInfo> {
        let last = |room: &RoomRef| self.store.as_ref().and_then(|s| s.last(room));
        let manager = self.manager_room();
        let live = sessions
            .iter()
            .find(|s| s.manager && s.state.kind != StateKind::Exited);
        let live_meta = live.and_then(|s| lock(&self.tails).get(&s.id).map(|t| t.meta.clone()));
        let mut out = vec![RoomInfo {
            last: last(&manager),
            room: manager,
            title: "Manager".into(),
            agent: None,
            state: live.map(|s| state_name(s.state.kind).into()),
            context_pct: live_meta.as_ref().and_then(|m| m.context_pct),
            context_tokens: live_meta.and_then(|m| m.context_tokens),
            model: None,
            unrestricted: unrestricted.is_some(),
        }];
        let live_id = live.map(|s| s.id.as_str());
        let mut sessions: Vec<&SessionInfo> = sessions
            .iter()
            .filter(|s| live_id != Some(s.id.as_str()))
            .collect();
        sessions.sort_by(|a, b| {
            a.created_ms
                .cmp(&b.created_ms)
                .then_with(|| a.id.cmp(&b.id))
        });
        for s in sessions {
            let room = self.session_room(&s.id);
            if room.validate().is_err() {
                continue;
            }
            let meta = lock(&self.tails).get(&s.id).map(|t| t.meta.clone());
            out.push(RoomInfo {
                last: last(&room),
                room,
                title: s.name.clone(),
                agent: s.profile.clone(),
                state: Some(state_name(s.state.kind).into()),
                context_pct: meta.as_ref().and_then(|m| m.context_pct),
                context_tokens: meta.as_ref().and_then(|m| m.context_tokens),
                model: meta.and_then(|m| m.model),
                unrestricted: unrestricted == Some(s.id.as_str()),
            });
        }
        out
    }
}

fn state_name(kind: StateKind) -> &'static str {
    match kind {
        StateKind::Starting => "starting",
        StateKind::Working => "working",
        StateKind::Waiting => "waiting",
        StateKind::Blocked => "blocked",
        StateKind::Exited => "exited",
    }
}

impl crate::manager::ChatPort for std::sync::Arc<Chat> {
    fn post(&self, msg: ChatMessage) -> Result<ChatMessage, String> {
        self.append(msg)
    }

    fn history(&self, room: &RoomRef, limit: u32) -> Result<Vec<ChatMessage>, String> {
        Chat::history(self, room, None, Some(limit))
    }
}

fn system_message(room: &RoomRef, id: String, ts_ms: u64, text: &str) -> ChatMessage {
    ChatMessage {
        v: CHAT_V,
        id,
        room: room.clone(),
        author: Author {
            kind: AuthorKind::System,
            id: "runner".into(),
            machine: None,
            device: None,
            agent: None,
        },
        ts_ms,
        kind: MessageKind::System,
        body_md: text.into(),
        attachments: Vec::new(),
        reply_to: None,
        decision: None,
        session_meta: None,
        user_turn: false,
    }
}

pub fn prompt_bytes(body: &str) -> Vec<u8> {
    let clean: String = body
        .chars()
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .collect();
    format!("\x1b[200~{clean}\x1b[201~\r").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn client_for(dir: &Path) -> std::sync::Arc<crate::mgr_rpc::relay::RpcClient> {
        use crate::mgr_rpc::relay::{MachinesCache, RpcClient, RpcInbox};
        std::sync::Arc::new(RpcClient::new(
            std::sync::Arc::new(RpcInbox::new()),
            std::sync::Arc::new(MachinesCache::new(dir)),
            std::sync::Arc::new(|_, _, _, _| Ok(())),
            std::sync::Arc::new(|| 0),
        ))
    }

    #[test]
    fn the_rpc_client_slot_starts_empty_and_can_be_set_and_cleared() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(t.path());
        assert!(chat.rpc_client().is_none());
        let c = client_for(t.path());
        chat.set_rpc_client(Some(c.clone()));
        assert!(std::sync::Arc::ptr_eq(&chat.rpc_client().unwrap(), &c));
        chat.set_rpc_client(None);
        assert!(chat.rpc_client().is_none());
    }

    #[test]
    fn relay_self_is_none_until_published_and_carries_the_machine_id() {
        let t = tempfile::tempdir().unwrap();
        let chat = Chat::open(t.path());
        assert!(chat.relay_self().is_none());
        chat.set_relay_device(Some(("dev".into(), "fp".into())));
        let me = chat.relay_self().unwrap();
        assert_eq!(me.id, chat.machine());
        assert_eq!((me.device_id.as_str(), me.fp.as_str()), ("dev", "fp"));
        chat.set_relay_device(None);
        assert!(chat.relay_self().is_none());
    }

    #[test]
    fn the_machine_id_is_created_once_private_and_stable() {
        let t = tempfile::tempdir().unwrap();
        let a = machine_id(t.path()).unwrap();
        assert!(valid_machine_id(&a), "{a}");
        assert_eq!(machine_id(t.path()).unwrap(), a);
        let mode = fs::metadata(t.path().join("machine.id"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        let other = tempfile::tempdir().unwrap();
        assert_ne!(machine_id(other.path()).unwrap(), a);
        assert_eq!(fs::read_dir(t.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_malformed_machine_id_is_an_error_not_a_silent_rewrite() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("machine.id"), "my-laptop").unwrap();
        assert!(machine_id(t.path()).is_err());
        assert_eq!(
            fs::read_to_string(t.path().join("machine.id")).unwrap(),
            "my-laptop"
        );
    }

    #[test]
    fn a_restarted_chat_keeps_its_machine_and_history() {
        let t = tempfile::tempdir().unwrap();
        let a = Chat::open(t.path());
        let room = a.manager_room();
        a.post(room.clone(), Chat::owner(), "hello".into(), None, vec![])
            .unwrap();
        let b = Chat::open(t.path());
        assert_eq!(b.machine(), a.machine());
        assert_eq!(b.history(&room, None, None).unwrap()[0].body_md, "hello");
    }

    fn session_info(id: &str) -> SessionInfo {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": id, "argv": [], "cwd": "/w", "profile": "claude-code",
            "state": {"kind": "waiting", "since_ms": 1, "detail": null, "exit_code": null},
            "created_ms": 5, "cols": 80, "rows": 24, "shim_pid": 1, "child_pid": 2
        }))
        .unwrap()
    }

    #[test]
    fn the_live_manager_session_has_no_room_of_its_own_and_lends_its_state_to_the_manager_room() {
        let t = tempfile::tempdir().unwrap();
        let c = Chat::open(t.path());
        let mut live = session_info("s_aaaaaaaaaa");
        live.manager = true;
        let other = session_info("s_bbbbbbbbbb");
        let rooms = c.rooms(&[live.clone(), other.clone()], Some("s_aaaaaaaaaa"));
        let sessions: Vec<_> = rooms
            .iter()
            .filter_map(|r| r.room.session.clone())
            .collect();
        assert_eq!(sessions, ["s_bbbbbbbbbb"]);
        assert_eq!(rooms[0].state.as_deref(), Some("waiting"));
        assert!(rooms[0].unrestricted);
        live.state.kind = StateKind::Exited;
        let rooms = c.rooms(&[live, other], None);
        assert_eq!(rooms.len(), 3);
        assert_eq!(rooms[0].state, None);
    }

    #[test]
    fn only_the_unrestricted_managers_rooms_carry_the_flag() {
        let t = tempfile::tempdir().unwrap();
        let c = Chat::open(t.path());
        let sessions = [session_info("s_aaaaaaaaaa"), session_info("s_bbbbbbbbbb")];
        let flags = |rooms: Vec<RoomInfo>| rooms.iter().map(|r| r.unrestricted).collect::<Vec<_>>();
        assert_eq!(flags(c.rooms(&sessions, None)), [false, false, false]);
        assert_eq!(
            flags(c.rooms(&sessions, Some("s_bbbbbbbbbb"))),
            [true, false, true]
        );
        let plain = serde_json::to_value(&c.rooms(&sessions, None)[0]).unwrap();
        assert!(plain.get("unrestricted").is_none());
    }

    #[test]
    fn rooms_of_another_machine_are_refused() {
        let t = tempfile::tempdir().unwrap();
        let c = Chat::open(t.path());
        let foreign = RoomRef::manager("mx_ffffffffffffffff");
        assert!(c
            .post(foreign.clone(), Chat::owner(), "x".into(), None, vec![])
            .is_err());
        assert!(c.history(&foreign, None, None).is_err());
    }

    #[test]
    fn minted_ids_sort_in_creation_order_and_subscribers_get_every_message() {
        let t = tempfile::tempdir().unwrap();
        let c = Chat::open(t.path());
        let rx = c.subscribe();
        let room = c.session_room("s_abcdefghij");
        let ids: Vec<String> = (0..50)
            .map(|i| {
                c.post(room.clone(), Chat::owner(), format!("m{i}"), None, vec![])
                    .unwrap()
                    .id
            })
            .collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
        let pushed: Vec<String> = rx.try_iter().map(|p| p.message.id).collect();
        assert_eq!(pushed, ids);
    }

    #[test]
    fn a_slow_activity_subscriber_is_never_evicted_and_the_latest_event_per_room_wins() {
        let (_t, c) = chat();
        c.set_activity_consent(true);
        let rx = c.subscribe_activity();
        let rooms = [c.manager_room(), c.session_room("s_abcdefghij")];
        for i in 0..5000 {
            for room in &rooms {
                c.publish_activity(&ChatActivity {
                    v: 1,
                    room: room.clone(),
                    state: tmgr_ipc::chat::ActivityState::Working,
                    current: Some(format!("Bash: step {i}")),
                    used: Vec::new(),
                });
            }
        }
        assert_eq!(lock(&c.activity_subs).len(), 1);
        let got = rx.take(Duration::ZERO);
        assert_eq!(got.len(), 2);
        assert!(got
            .iter()
            .all(|e| e.current.as_deref() == Some("Bash: step 4999")));
        assert!(rx.take(Duration::ZERO).is_empty());
        drop(rx);
        c.publish_activity(&got[0]);
        assert!(lock(&c.activity_subs).is_empty());
    }

    fn activity_in(room: &RoomRef, current: &str) -> ChatActivity {
        ChatActivity {
            v: 1,
            room: room.clone(),
            state: tmgr_ipc::chat::ActivityState::Working,
            current: Some(current.into()),
            used: Vec::new(),
        }
    }

    #[test]
    fn revoked_consent_drops_queued_activity_and_silences_publishing_until_it_returns() {
        let (_t, c) = chat();
        let room = c.manager_room();
        c.set_activity_consent(true);
        let rx = c.subscribe_activity();
        c.publish_activity(&activity_in(&room, "Bash: ls"));
        c.set_activity_consent(false);
        c.publish_activity(&activity_in(&room, "Bash: rm"));
        assert!(rx.take(Duration::ZERO).is_empty());
        c.set_activity_consent(true);
        c.publish_activity(&activity_in(&room, "Bash: cargo"));
        let got = rx.take(Duration::ZERO);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].current.as_deref(), Some("Bash: cargo"));
    }

    #[test]
    fn an_event_queued_before_revocation_is_not_handed_out_after_it() {
        let (_t, c) = chat();
        c.set_activity_consent(true);
        let rx = c.subscribe_activity();
        c.publish_activity(&activity_in(&c.manager_room(), "Bash: ls"));
        c.activity_on.store(false, Ordering::SeqCst);
        assert!(rx.take(Duration::ZERO).is_empty());
    }

    #[test]
    fn subscribing_prunes_dead_slots_without_any_event() {
        let (_t, c) = chat();
        for _ in 0..1000 {
            drop(c.subscribe_activity());
        }
        assert!(c.activity_subscribers() <= 1);
    }

    #[test]
    fn past_the_room_limit_the_oldest_room_is_evicted_and_the_newest_kept() {
        let (_t, c) = chat();
        c.set_activity_consent(true);
        let rx = c.subscribe_activity();
        let room = |i: usize| c.session_room(&format!("s_{i:010}"));
        for i in 0..300 {
            c.publish_activity(&activity_in(&room(i), "Bash: ls"));
        }
        c.publish_activity(&activity_in(&room(299), "Bash: cargo"));
        let got = rx.take(Duration::ZERO);
        assert_eq!(got.len(), 256);
        assert_eq!(got[0].room, room(44));
        assert_eq!(got[255].room, room(299));
        assert_eq!(got[255].current.as_deref(), Some("Bash: cargo"));
    }

    #[test]
    fn a_dropped_subscriber_is_forgotten() {
        let t = tempfile::tempdir().unwrap();
        let c = Chat::open(t.path());
        drop(c.subscribe());
        c.post(c.manager_room(), Chat::owner(), "x".into(), None, vec![])
            .unwrap();
        assert!(lock(&c.subs).is_empty());
    }

    #[test]
    fn prompt_bytes_wrap_the_body_in_a_paste_and_drop_escapes() {
        let b = prompt_bytes("a\nb\x1b[201~c\x07");
        assert_eq!(b, b"\x1b[200~a\nb[201~c\x1b[201~\r");
    }

    const NOW: u64 = 1_800_000_000_000;

    fn incoming(room: RoomRef, n: u64, ts: u64, body: &str) -> Incoming {
        Incoming {
            room,
            id: message_id_at(n, [7; 10]),
            ts_ms: ts,
            body_md: body.into(),
            reply_to: None,
            attachments: Vec::new(),
            device: Some("d_phone1".into()),
            client: "relay".into(),
        }
    }

    fn chat() -> (tempfile::TempDir, Chat) {
        let t = tempfile::tempdir().unwrap();
        let c = Chat::open(t.path());
        (t, c)
    }

    fn texts(c: &Chat, room: &RoomRef) -> Vec<(AuthorKind, String)> {
        c.history(room, None, None)
            .unwrap()
            .into_iter()
            .map(|m| (m.author.kind, m.body_md))
            .collect()
    }

    fn no_session(_: &str) -> bool {
        false
    }

    #[test]
    fn a_fresh_ingest_is_stored_with_the_device_and_delivered_once() {
        let (_t, c) = chat();
        let room = c.manager_room();
        let calls = std::cell::Cell::new(0);
        let deliver = |body: &str| {
            assert_eq!(body, "run it");
            calls.set(calls.get() + 1);
            Delivery::Delivered
        };
        let r = c
            .ingest(
                incoming(room.clone(), 1, NOW - 1000, "run it"),
                NOW,
                no_session,
                deliver,
            )
            .unwrap();
        assert!(r.delivered && !r.duplicate);
        assert_eq!(r.message.author.kind, AuthorKind::Owner);
        assert_eq!(r.message.author.device.as_deref(), Some("d_phone1"));
        assert_eq!(r.message.ts_ms, NOW - 1000);
        assert_eq!(calls.get(), 1);
        assert_eq!(
            texts(&c, &room),
            [(AuthorKind::Owner, "run it".to_string())]
        );
    }

    #[test]
    fn a_fresh_ingest_without_a_target_is_stored_with_the_real_reason() {
        let (_t, c) = chat();
        let room = c.manager_room();
        let r = c
            .ingest(
                incoming(room.clone(), 1, NOW, "hi"),
                NOW,
                no_session,
                |_| Delivery::Gone("no manager is running".into()),
            )
            .unwrap();
        assert!(!r.delivered);
        assert_eq!(
            texts(&c, &room)[1],
            (
                AuthorKind::System,
                "Not delivered: no manager is running.".to_string()
            )
        );
        assert!(c.queued_ids(&room).is_empty());
    }

    #[test]
    fn a_stale_or_future_message_is_stored_with_a_note_and_never_delivered() {
        let (_t, c) = chat();
        let room = c.manager_room();
        let never = |_: &str| -> Delivery { panic!("must not deliver") };
        let r = c
            .ingest(
                incoming(room.clone(), 1, NOW - FRESH_PAST_MS - 1, "old"),
                NOW,
                no_session,
                never,
            )
            .unwrap();
        assert!(!r.delivered);
        let r = c
            .ingest(
                incoming(room.clone(), 2, NOW + FRESH_FUTURE_MS + 1, "future"),
                NOW,
                no_session,
                never,
            )
            .unwrap();
        assert!(!r.delivered);
        let h = texts(&c, &room);
        assert_eq!(h.len(), 4);
        assert_eq!(
            h[1],
            (
                AuthorKind::System,
                "Not delivered: older than 10 minutes.".to_string()
            )
        );
        assert_eq!(h[3].0, AuthorKind::System);
        let edge = c
            .ingest(
                incoming(room.clone(), 3, NOW - FRESH_PAST_MS, "edge"),
                NOW,
                no_session,
                |_| Delivery::Delivered,
            )
            .unwrap();
        assert!(edge.delivered);
        let edge = c
            .ingest(
                incoming(room, 4, NOW + FRESH_FUTURE_MS, "edge2"),
                NOW,
                no_session,
                |_| Delivery::Delivered,
            )
            .unwrap();
        assert!(edge.delivered);
    }

    #[test]
    fn a_busy_target_queues_the_message_without_a_note_until_drain_delivers_it() {
        let (_t, c) = chat();
        let room = c.manager_room();
        let r = c
            .ingest(
                incoming(room.clone(), 1, NOW, "hello"),
                NOW,
                no_session,
                |_| Delivery::Busy,
            )
            .unwrap();
        assert!(!r.delivered);
        assert_eq!(c.queued_ids(&room), std::slice::from_ref(&r.message.id));
        assert_eq!(texts(&c, &room).len(), 1);
        let got = std::cell::RefCell::new(Vec::new());
        c.drain_queue(NOW + 1, |_, body, client, _| {
            got.borrow_mut()
                .push((body.to_string(), client.to_string()));
            Delivery::Delivered
        });
        assert_eq!(*got.borrow(), [("hello".to_string(), "relay".to_string())]);
        assert!(c.queued_ids(&room).is_empty());
        assert_eq!(texts(&c, &room).len(), 1);
    }

    #[test]
    fn a_duplicate_id_is_not_queued_twice() {
        let (_t, c) = chat();
        let room = c.manager_room();
        for _ in 0..2 {
            c.ingest(
                incoming(room.clone(), 1, NOW, "hello"),
                NOW,
                no_session,
                |_| Delivery::Busy,
            )
            .unwrap();
        }
        assert_eq!(c.queued_ids(&room).len(), 1);
    }

    #[test]
    fn a_full_queue_notes_it_and_a_gone_target_notes_its_reason_never_busy() {
        let (_t, c) = chat();
        let room = c.manager_room();
        for n in 1..=21 {
            c.ingest(incoming(room.clone(), n, NOW, "x"), NOW, no_session, |_| {
                Delivery::Busy
            })
            .unwrap();
        }
        let h = texts(&c, &room);
        assert_eq!(
            h.last().unwrap().1,
            "Not delivered: the queue for this session is full (20 messages)."
        );
        assert!(h.iter().all(|(_, t)| !t.contains("busy")));
    }

    #[test]
    fn a_refused_delivery_stores_the_message_and_posts_the_reason() {
        let (_t, c) = chat();
        let room = c.manager_room();
        let r = c
            .ingest(
                incoming(room.clone(), 1, NOW, "hello"),
                NOW,
                no_session,
                |_| Delivery::Refused("Not delivered: nope."),
            )
            .unwrap();
        assert!(!r.delivered);
        let h = texts(&c, &room);
        assert_eq!(h[0].1, "hello");
        assert_eq!(
            h[1],
            (AuthorKind::System, "Not delivered: nope.".to_string())
        );
    }

    #[test]
    fn a_typed_delivery_remembers_what_was_typed_not_the_stored_body() {
        let (_t, c) = chat();
        let session = "s_abcdefghij";
        let room = c.session_room(session);
        let r = c
            .ingest(
                incoming(room.clone(), 1, NOW, "hello"),
                NOW,
                |_| true,
                |_| Delivery::Typed("[marker]\n\nhello".into()),
            )
            .unwrap();
        assert!(r.delivered);
        assert_eq!(r.message.body_md, "hello");
        assert!(!c.take_sent(session, "hello"));
        assert!(c.take_sent(session, "[marker]\n\nhello\n"));
    }

    #[test]
    fn the_same_id_and_content_is_a_no_op_and_different_content_conflicts() {
        let (_t, c) = chat();
        let room = c.manager_room();
        let first = c
            .ingest(
                incoming(room.clone(), 1, NOW, "same"),
                NOW,
                no_session,
                |_| Delivery::Delivered,
            )
            .unwrap();
        let rx = c.subscribe();
        let again = c
            .ingest(
                incoming(room.clone(), 1, NOW, "same"),
                NOW,
                no_session,
                |_| -> Delivery { panic!("no second delivery") },
            )
            .unwrap();
        assert!(again.duplicate && !again.delivered);
        assert_eq!(again.message, first.message);
        assert!(rx.try_recv().is_err());
        let e = c
            .ingest(
                incoming(room.clone(), 1, NOW, "other"),
                NOW,
                no_session,
                |_| Delivery::Delivered,
            )
            .unwrap_err();
        assert!(matches!(e, IngestError::Conflict(_)));
        let e = c
            .ingest(
                incoming(room.clone(), 1, NOW - 1, "same"),
                NOW,
                no_session,
                |_| Delivery::Delivered,
            )
            .unwrap_err();
        assert!(matches!(e, IngestError::Conflict(_)));
        assert_eq!(texts(&c, &room), [(AuthorKind::Owner, "same".to_string())]);
    }

    #[test]
    fn a_duplicate_of_a_stale_message_adds_no_second_note() {
        let (_t, c) = chat();
        let room = c.manager_room();
        let old = NOW - FRESH_PAST_MS - 5;
        c.ingest(
            incoming(room.clone(), 1, old, "old"),
            NOW,
            no_session,
            |_| Delivery::Delivered,
        )
        .unwrap();
        c.ingest(
            incoming(room.clone(), 1, old, "old"),
            NOW,
            no_session,
            |_| Delivery::Delivered,
        )
        .unwrap();
        assert_eq!(texts(&c, &room).len(), 2);
    }

    #[test]
    fn bad_ids_foreign_machines_and_unknown_rooms_are_refused() {
        let (_t, c) = chat();
        let mut m = incoming(c.manager_room(), 1, NOW, "x");
        m.id = "b_0123456789abcdef0123456789abcdef".into();
        assert!(matches!(
            c.ingest(m, NOW, no_session, |_| Delivery::Delivered),
            Err(IngestError::Invalid(_))
        ));
        let foreign = RoomRef::manager("mx_ffffffffffffffff");
        let m = incoming(foreign, 1, NOW, "x");
        assert!(matches!(
            c.ingest(m, NOW, no_session, |_| Delivery::Delivered),
            Err(IngestError::Invalid(_))
        ));
        let mut m = incoming(c.manager_room(), 1, NOW, "x");
        m.device = Some("bad device".into());
        assert!(matches!(
            c.ingest(m, NOW, no_session, |_| Delivery::Delivered),
            Err(IngestError::Invalid(_))
        ));
        let ghost = c.session_room("s_abcdefghij");
        let m = incoming(ghost.clone(), 1, NOW, "x");
        assert!(matches!(
            c.ingest(m, NOW, no_session, |_| Delivery::Delivered),
            Err(IngestError::NotFound(_))
        ));
        assert!(c.history(&ghost, None, None).unwrap().is_empty());
    }

    #[test]
    fn a_session_room_exists_when_the_session_is_known_or_the_store_has_it() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        let known = |s: &str| s == "s_abcdefghij";
        c.ingest(incoming(room.clone(), 1, NOW, "a"), NOW, known, |_| {
            Delivery::Delivered
        })
        .unwrap();
        let r = c
            .ingest(incoming(room.clone(), 2, NOW, "b"), NOW, no_session, |_| {
                Delivery::Delivered
            })
            .unwrap();
        assert!(!r.duplicate);
        assert_eq!(texts(&c, &room).len(), 2);
    }

    #[test]
    fn system_once_writes_a_message_once_and_history_shows_it() {
        let (_t, c) = chat();
        let room = c.manager_room();
        let id = message_id_at(7, [3; 10]);
        assert!(c.system_once(&room, &id, NOW, "handed off").unwrap());
        assert!(!c.system_once(&room, &id, NOW, "handed off").unwrap());
        let all = c.history(&room, None, None).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, id);
        assert_eq!(all[0].kind, MessageKind::System);
        assert_eq!(all[0].body_md, "handed off");
    }

    #[test]
    fn concurrent_system_once_calls_write_one_message() {
        use std::sync::{Arc, Barrier};
        let t = tempfile::tempdir().unwrap();
        let c = Arc::new(Chat::open(t.path()));
        let room = c.manager_room();
        let id = message_id_at(9, [4; 10]);
        let gate = Arc::new(Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let (c, room, gate, id) = (c.clone(), room.clone(), gate.clone(), id.clone());
                std::thread::spawn(move || {
                    gate.wait();
                    c.system_once(&room, &id, NOW, "once").unwrap()
                })
            })
            .collect();
        let wrote = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|w| *w)
            .count();
        assert_eq!(wrote, 1);
        assert_eq!(c.history(&room, None, None).unwrap().len(), 1);
    }

    #[test]
    fn system_once_refuses_a_malformed_id() {
        let (_t, c) = chat();
        let room = c.manager_room();
        assert!(c.system_once(&room, "m_short", NOW, "x").is_err());
        assert!(c.history(&room, None, None).unwrap().is_empty());
    }
}
