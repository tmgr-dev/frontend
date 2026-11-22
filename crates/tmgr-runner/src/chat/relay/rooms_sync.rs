use std::collections::BTreeSet;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use serde_json::Value;
use tmgr_chat_relay::engine::{ChatSink, KeyStore, RelayEngine, Transport};
use tmgr_chat_relay::rooms::{Role, RoomEntry, RoomsIndex, MAX_ROOMS, ROOMS_V};
use tmgr_chat_relay::wire::random_msg_id;
use tmgr_ipc::chat::{AuthorKind, ChatMessage, RoomInfo, RoomRef};

use crate::mgr_rpc::files::write_private_atomic;

pub const BACKFILL_PER_ROOM: u32 = 20;
pub const BACKFILL_CAP_BYTES: usize = 200 * 1024;
const BACKFILLED_FILE: &str = "rooms-backfilled.json";
const MAX_BACKFILLED_BYTES: u64 = 64 * 1024;
const MAX_LABEL: usize = 64;
const FALLBACK_LABEL: &str = "This machine";

pub struct Snapshot {
    pub machine: String,
    pub label: String,
    pub top: bool,
    pub followers: Vec<String>,
    pub rooms: Vec<RoomInfo>,
}

pub trait OwnerSink {
    fn owners(&self) -> Result<Vec<String>, String>;
    fn send_index(&self, id: &str, payload: &Value, only: Option<&[String]>) -> Result<(), String>;
    fn send_chat(&self, msg: &ChatMessage, only: &[String]) -> Result<(), String>;
}

impl<T: Transport, K: KeyStore, S: ChatSink> OwnerSink for RelayEngine<T, K, S> {
    fn owners(&self) -> Result<Vec<String>, String> {
        self.owner_device_ids()
    }

    fn send_index(&self, id: &str, payload: &Value, only: Option<&[String]>) -> Result<(), String> {
        self.send_sealed_to_owners("rooms", id, payload, only)
    }

    fn send_chat(&self, msg: &ChatMessage, only: &[String]) -> Result<(), String> {
        self.send_chat_to_owners(msg, only)
    }
}

pub fn clean_label(raw: &str) -> String {
    let name = raw.trim().strip_suffix(".local").unwrap_or(raw.trim());
    let cleaned: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_LABEL)
        .collect();
    let cleaned = cleaned.trim().to_owned();
    if cleaned.is_empty() {
        FALLBACK_LABEL.to_owned()
    } else {
        cleaned
    }
}

pub fn host_label() -> String {
    let mut buf = [0u8; 256];
    let ok = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len() - 1) } == 0;
    if !ok {
        return FALLBACK_LABEL.to_owned();
    }
    let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    clean_label(&String::from_utf8_lossy(&buf[..end]))
}

pub fn build_index(s: &Snapshot) -> RoomsIndex {
    let rooms = s
        .rooms
        .iter()
        .take(MAX_ROOMS)
        .map(|r| RoomEntry {
            room: r.room.clone(),
            title: r.title.clone(),
            state: r.state.clone(),
            context_pct: r.context_pct,
            last_ts: r.last.as_ref().map(|m| m.ts_ms),
        })
        .collect();
    RoomsIndex {
        v: ROOMS_V,
        machine: s.machine.clone(),
        machine_label: s.label.clone(),
        role: if s.top { Role::Top } else { Role::Sub },
        followers: s.top.then(|| s.followers.clone()),
        rooms,
    }
}

pub fn new_devices(owners: &[String], done: &BTreeSet<String>) -> Vec<String> {
    owners
        .iter()
        .filter(|d| !done.contains(*d))
        .cloned()
        .collect()
}

fn relayable(m: &ChatMessage) -> bool {
    m.author.device.is_none() && m.author.kind != AuthorKind::Owner
}

fn size_of(m: &ChatMessage) -> usize {
    serde_json::to_vec(m).map_or(usize::MAX, |v| v.len())
}

pub fn backfill_plan(
    rooms: &[RoomInfo],
    history: &dyn Fn(&RoomRef, u32) -> Vec<ChatMessage>,
    cap: usize,
) -> Vec<ChatMessage> {
    let mut ordered: Vec<&RoomInfo> = rooms.iter().collect();
    ordered.sort_by_key(|r| std::cmp::Reverse(r.last.as_ref().map_or(0, |m| m.ts_ms)));
    let mut groups: Vec<Vec<ChatMessage>> = ordered
        .iter()
        .map(|r| {
            let mut msgs: Vec<ChatMessage> = history(&r.room, BACKFILL_PER_ROOM)
                .into_iter()
                .filter(relayable)
                .filter(|m| m.validate().is_ok())
                .collect();
            msgs.sort_by_key(|m| m.ts_ms);
            let skip = msgs.len().saturating_sub(BACKFILL_PER_ROOM as usize);
            msgs.drain(..skip);
            msgs
        })
        .collect();
    let mut total: usize = groups.iter().flatten().map(size_of).sum();
    while total > cap {
        let oldest = groups
            .iter()
            .enumerate()
            .filter_map(|(i, g)| g.first().map(|m| (m.ts_ms, std::cmp::Reverse(i), i)))
            .min();
        let Some((_, _, i)) = oldest else { break };
        let dropped = groups[i].remove(0);
        total -= size_of(&dropped);
    }
    groups.into_iter().flatten().collect()
}

pub fn load_backfilled(dir: &Path) -> BTreeSet<String> {
    let path = dir.join(BACKFILLED_FILE);
    let Ok(mut f) = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    else {
        return BTreeSet::new();
    };
    let mut text = String::new();
    if f.by_ref()
        .take(MAX_BACKFILLED_BYTES)
        .read_to_string(&mut text)
        .is_err()
    {
        return BTreeSet::new();
    }
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save_backfilled(dir: &Path, done: &BTreeSet<String>) -> std::io::Result<()> {
    let text = serde_json::to_vec(done).map_err(std::io::Error::other)?;
    write_private_atomic(dir, BACKFILLED_FILE, &text)
}

pub struct RoomsSync {
    last_sent: Option<String>,
    last_error: Option<String>,
}

impl RoomsSync {
    pub fn new() -> Self {
        Self {
            last_sent: None,
            last_error: None,
        }
    }

    fn note(&mut self, e: Option<String>) {
        if e != self.last_error {
            if let Some(e) = &e {
                crate::daemon::log(&format!("chat relay: rooms sync failed: {e}"));
            }
            self.last_error = e;
        }
    }

    pub fn tick(
        &mut self,
        sink: &dyn OwnerSink,
        dir: &Path,
        snapshot: &dyn Fn() -> Snapshot,
        history: &dyn Fn(&RoomRef, u32) -> Vec<ChatMessage>,
    ) {
        let result = self.tick_inner(sink, dir, snapshot, history);
        self.note(result.err());
    }

    fn tick_inner(
        &mut self,
        sink: &dyn OwnerSink,
        dir: &Path,
        snapshot: &dyn Fn() -> Snapshot,
        history: &dyn Fn(&RoomRef, u32) -> Vec<ChatMessage>,
    ) -> Result<(), String> {
        let owners = sink.owners()?;
        if owners.is_empty() {
            return Ok(());
        }
        let snap = snapshot();
        let index = build_index(&snap);
        index.validate()?;
        let payload = serde_json::to_value(&index).map_err(|e| e.to_string())?;
        let json = payload.to_string();
        let mut done = load_backfilled(dir);
        let fresh = new_devices(&owners, &done);
        let mut failure = None;
        for device in &fresh {
            let only = std::slice::from_ref(device);
            let sent = sink
                .send_index(
                    &random_msg_id("rooms").map_err(|e| e.to_string())?,
                    &payload,
                    Some(only),
                )
                .and_then(|()| {
                    for m in backfill_plan(&snap.rooms, history, BACKFILL_CAP_BYTES) {
                        sink.send_chat(&m, only)?;
                    }
                    Ok(())
                });
            match sent {
                Ok(()) => {
                    done.insert(device.clone());
                    save_backfilled(dir, &done).map_err(|e| e.to_string())?;
                }
                Err(e) => failure = Some(e),
            }
        }
        let handled: Vec<String> = fresh
            .iter()
            .filter(|d| done.contains(*d))
            .cloned()
            .collect();
        if self.last_sent.as_deref() != Some(json.as_str()) {
            let rest: Vec<String> = owners
                .iter()
                .filter(|d| !handled.contains(*d))
                .cloned()
                .collect();
            let all_ok = if rest.is_empty() {
                true
            } else {
                match sink.send_index(
                    &random_msg_id("rooms").map_err(|e| e.to_string())?,
                    &payload,
                    Some(&rest),
                ) {
                    Ok(()) => true,
                    Err(e) => {
                        failure.get_or_insert(e);
                        false
                    }
                }
            };
            if all_ok {
                self.last_sent = Some(json);
            }
        }
        failure.map_or(Ok(()), Err)
    }
}

impl Default for RoomsSync {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use tmgr_ipc::chat::{new_message_id, Author, MessageKind, CHAT_V};

    const MACHINE: &str = "mx_0123456789abcdef";

    fn chat_msg(room: &RoomRef, ts: u64, body: &str) -> ChatMessage {
        ChatMessage {
            v: CHAT_V,
            id: new_message_id(),
            room: room.clone(),
            author: Author {
                kind: AuthorKind::Manager,
                id: "s_manager01".into(),
                machine: None,
                device: None,
                agent: None,
            },
            ts_ms: ts,
            kind: MessageKind::Text,
            body_md: body.into(),
            attachments: Vec::new(),
            reply_to: None,
            decision: None,
            session_meta: None,
            user_turn: false,
        }
    }

    fn info(room: RoomRef, title: &str, last: Option<u64>) -> RoomInfo {
        let last = last.map(|ts| chat_msg(&room, ts, "last"));
        RoomInfo {
            room,
            title: title.into(),
            agent: None,
            state: Some("working".into()),
            context_pct: Some(5),
            context_tokens: None,
            last,
            unrestricted: false,
        }
    }

    fn two_rooms() -> Vec<RoomInfo> {
        vec![
            info(RoomRef::manager(MACHINE), "Manager", Some(10)),
            info(RoomRef::session(MACHINE, "s_abc"), "fix login", Some(50)),
        ]
    }

    fn snap(rooms: Vec<RoomInfo>) -> Snapshot {
        Snapshot {
            machine: MACHINE.into(),
            label: "Studio".into(),
            top: true,
            followers: vec!["mx_fedcba9876543210".into()],
            rooms,
        }
    }

    fn history_of(per_room: usize, body_len: usize) -> impl Fn(&RoomRef, u32) -> Vec<ChatMessage> {
        move |room, limit| {
            let base = if room.session.is_some() { 1000 } else { 100 };
            (0..per_room.min(limit as usize))
                .map(|i| chat_msg(room, base + i as u64, &"x".repeat(body_len)))
                .collect()
        }
    }

    #[derive(Default)]
    struct Sink {
        owners: RefCell<Vec<String>>,
        index_to: RefCell<Vec<Option<Vec<String>>>>,
        chats_to: RefCell<Vec<(String, Vec<String>)>>,
        fail: RefCell<bool>,
    }

    impl Sink {
        fn with_owners(owners: &[&str]) -> Sink {
            let s = Sink::default();
            *s.owners.borrow_mut() = owners.iter().map(|o| (*o).to_owned()).collect();
            s
        }
    }

    impl OwnerSink for Sink {
        fn owners(&self) -> Result<Vec<String>, String> {
            Ok(self.owners.borrow().clone())
        }
        fn send_index(&self, id: &str, _: &Value, only: Option<&[String]>) -> Result<(), String> {
            assert!(id.starts_with("i_"));
            if *self.fail.borrow() {
                return Err("down".into());
            }
            self.index_to
                .borrow_mut()
                .push(only.map(<[String]>::to_vec));
            Ok(())
        }
        fn send_chat(&self, m: &ChatMessage, only: &[String]) -> Result<(), String> {
            if *self.fail.borrow() {
                return Err("down".into());
            }
            self.chats_to
                .borrow_mut()
                .push((m.id.clone(), only.to_vec()));
            Ok(())
        }
    }

    #[test]
    fn the_index_follows_the_contract_for_top_and_sub() {
        let top = build_index(&snap(two_rooms()));
        assert_eq!(top.role, Role::Top);
        assert_eq!(
            top.followers.as_deref(),
            Some(&["mx_fedcba9876543210".to_owned()][..])
        );
        assert_eq!(top.rooms[1].last_ts, Some(50));
        assert!(top.validate().is_ok());
        let mut s = snap(two_rooms());
        s.top = false;
        let sub = build_index(&s);
        assert_eq!((sub.role, sub.followers), (Role::Sub, None));
    }

    #[test]
    fn the_index_is_capped_at_the_room_limit() {
        let rooms = (0..250)
            .map(|i| info(RoomRef::session(MACHINE, &format!("s{i}")), "t", None))
            .collect();
        let idx = build_index(&snap(rooms));
        assert_eq!(idx.rooms.len(), MAX_ROOMS);
        assert!(idx.validate().is_ok());
    }

    #[test]
    fn labels_are_cleaned_and_never_empty() {
        assert_eq!(clean_label("Studio.local\n"), "Studio");
        assert_eq!(clean_label("a\u{7}b"), "ab");
        assert_eq!(clean_label("  "), FALLBACK_LABEL);
        assert_eq!(clean_label(&"y".repeat(100)).chars().count(), 64);
    }

    #[test]
    fn new_devices_are_the_owners_not_yet_backfilled() {
        let done: BTreeSet<String> = ["a".to_owned()].into();
        assert_eq!(new_devices(&["a".into(), "b".into()], &done), ["b"]);
    }

    #[test]
    fn backfill_puts_newest_rooms_first_and_skips_owner_input() {
        let mut owner_msg = chat_msg(&RoomRef::manager(MACHINE), 120, "mine");
        owner_msg.author.kind = AuthorKind::Owner;
        let hist = move |room: &RoomRef, _: u32| {
            let mut v = history_of(2, 4)(room, 20);
            if room.session.is_none() {
                v.push(owner_msg.clone());
            }
            v
        };
        let plan = backfill_plan(&two_rooms(), &hist, BACKFILL_CAP_BYTES);
        assert_eq!(plan.len(), 4);
        assert!(plan[..2].iter().all(|m| m.room.session.is_some()));
        assert!(plan.iter().all(|m| m.author.kind != AuthorKind::Owner));
    }

    #[test]
    fn backfill_keeps_the_last_twenty_per_room() {
        let plan = backfill_plan(&two_rooms(), &history_of(30, 4), BACKFILL_CAP_BYTES);
        assert_eq!(plan.len(), 40);
    }

    #[test]
    fn backfill_over_the_cap_drops_the_oldest_messages_first() {
        let one = size_of(&chat_msg(&RoomRef::manager(MACHINE), 0, &"x".repeat(1000)));
        let plan = backfill_plan(&two_rooms(), &history_of(20, 1000), one * 10 + 5);
        assert!((9..=10).contains(&plan.len()));
        let min_kept = plan.iter().map(|m| m.ts_ms).min().unwrap();
        assert!(
            min_kept >= 1010,
            "manager history is older than the session's"
        );
        assert!(plan.iter().map(size_of).sum::<usize>() <= one * 10 + 5);
    }

    #[test]
    fn a_new_owner_gets_the_index_and_backfill_only_and_is_marked_once() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::with_owners(&["phone"]);
        let rooms = two_rooms();
        let snapshot = || snap(rooms.clone());
        let hist = history_of(3, 10);
        let mut sync = RoomsSync::new();
        sync.tick(&sink, dir.path(), &snapshot, &hist);
        assert_eq!(*sink.index_to.borrow(), [Some(vec!["phone".to_owned()])]);
        assert_eq!(sink.chats_to.borrow().len(), 6);
        assert!(sink.chats_to.borrow().iter().all(|c| c.1 == ["phone"]));
        assert!(load_backfilled(dir.path()).contains("phone"));
        sync.tick(&sink, dir.path(), &snapshot, &hist);
        assert_eq!(sink.index_to.borrow().len(), 1);
        assert_eq!(sink.chats_to.borrow().len(), 6);
    }

    #[test]
    fn a_restart_does_not_resend_the_backfill() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::with_owners(&["phone"]);
        let rooms = two_rooms();
        let snapshot = || snap(rooms.clone());
        let hist = history_of(3, 10);
        RoomsSync::new().tick(&sink, dir.path(), &snapshot, &hist);
        let chats = sink.chats_to.borrow().len();
        RoomsSync::new().tick(&sink, dir.path(), &snapshot, &hist);
        assert_eq!(sink.chats_to.borrow().len(), chats);
    }

    #[test]
    fn a_later_owner_is_backfilled_alone_and_a_changed_index_reaches_the_others() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::with_owners(&["phone"]);
        let rooms = RefCell::new(two_rooms());
        let snapshot = || snap(rooms.borrow().clone());
        let hist = history_of(1, 10);
        let mut sync = RoomsSync::new();
        sync.tick(&sink, dir.path(), &snapshot, &hist);
        sink.index_to.borrow_mut().clear();
        sink.chats_to.borrow_mut().clear();
        sink.owners.borrow_mut().push("desk".into());
        sync.tick(&sink, dir.path(), &snapshot, &hist);
        assert_eq!(*sink.index_to.borrow(), [Some(vec!["desk".to_owned()])]);
        assert!(sink.chats_to.borrow().iter().all(|c| c.1 == ["desk"]));
        assert_eq!(sink.chats_to.borrow().len(), 2);
        sink.index_to.borrow_mut().clear();
        rooms
            .borrow_mut()
            .push(info(RoomRef::session(MACHINE, "s_new"), "new", None));
        sync.tick(&sink, dir.path(), &snapshot, &hist);
        assert_eq!(
            *sink.index_to.borrow(),
            [Some(vec!["phone".to_owned(), "desk".to_owned()])]
        );
        assert_eq!(sink.chats_to.borrow().len(), 2);
    }

    #[test]
    fn a_failed_send_does_not_mark_the_device_and_is_retried() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::with_owners(&["phone"]);
        let rooms = two_rooms();
        let snapshot = || snap(rooms.clone());
        let hist = history_of(1, 10);
        let mut sync = RoomsSync::new();
        *sink.fail.borrow_mut() = true;
        sync.tick(&sink, dir.path(), &snapshot, &hist);
        assert!(load_backfilled(dir.path()).is_empty());
        *sink.fail.borrow_mut() = false;
        sync.tick(&sink, dir.path(), &snapshot, &hist);
        assert!(load_backfilled(dir.path()).contains("phone"));
        assert_eq!(sink.chats_to.borrow().len(), 2);
    }

    #[test]
    fn nothing_is_sent_without_owner_devices() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::with_owners(&[]);
        let rooms = two_rooms();
        RoomsSync::new().tick(
            &sink,
            dir.path(),
            &|| snap(rooms.clone()),
            &history_of(1, 1),
        );
        assert!(sink.index_to.borrow().is_empty() && sink.chats_to.borrow().is_empty());
    }

    #[test]
    fn the_backfilled_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        save_backfilled(dir.path(), &["a".to_owned()].into()).unwrap();
        let mode = std::fs::metadata(dir.path().join(BACKFILLED_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
