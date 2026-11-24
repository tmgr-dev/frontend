use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};
use tmgr_chat_relay::engine::{ChatSink, RoomsSink};
use tmgr_chat_relay::rooms::RoomsIndex;
use tmgr_ipc::chat::{ChatMessage, RoomInfo, RoomKind, RoomRef};
use tmgr_runner::chat::relay::store::RelayDir;

const FILE: &str = "cache.json";
const MAX_PER_ROOM: usize = 200;
const MAX_TOTAL: usize = 2000;

#[derive(Default, Serialize, Deserialize)]
struct Data {
    #[serde(default)]
    indexes: BTreeMap<String, RoomsIndex>,
    #[serde(default)]
    messages: Vec<ChatMessage>,
    #[serde(default)]
    runners: BTreeMap<String, RunnerInfo>,
    #[serde(default)]
    sent: Vec<(String, String)>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct RunnerInfo {
    pub machine: String,
    pub machine_label: String,
    pub terminal_input: bool,
}

pub struct RoomCache {
    data: Mutex<Data>,
    file: Option<RelayDir>,
}

pub struct CacheSink(pub Arc<RoomCache>);

impl RoomCache {
    #[cfg(test)]
    pub fn memory() -> Arc<RoomCache> {
        Arc::new(RoomCache {
            data: Mutex::default(),
            file: None,
        })
    }

    pub fn open(dir: RelayDir) -> Result<Arc<RoomCache>, String> {
        let data = match dir.read_file(FILE)? {
            Some(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            None => Data::default(),
        };
        Ok(Arc::new(RoomCache {
            data: Mutex::new(data),
            file: Some(dir),
        }))
    }

    fn lock(&self) -> MutexGuard<'_, Data> {
        self.data.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn save(&self, d: &Data) -> Result<(), String> {
        let Some(dir) = &self.file else {
            return Ok(());
        };
        let raw = serde_json::to_string(d).map_err(|e| e.to_string())?;
        dir.write_file(FILE, &raw)
    }

    pub fn wipe(&self) -> Result<(), String> {
        *self.lock() = Data::default();
        match &self.file {
            Some(dir) => dir.remove_file(FILE),
            None => Ok(()),
        }
    }

    pub fn add(&self, msg: ChatMessage) -> Result<(), String> {
        if msg.room.kind != RoomKind::Session {
            return Ok(());
        }
        let mut d = self.lock();
        if d.messages
            .iter()
            .any(|m| m.id == msg.id && m.room == msg.room)
        {
            return Ok(());
        }
        let room = msg.room.clone();
        d.messages.push(msg);
        let in_room = d.messages.iter().filter(|m| m.room == room).count();
        if in_room > MAX_PER_ROOM {
            trim_oldest(&mut d.messages, |m| m.room == room, in_room - MAX_PER_ROOM);
        }
        if d.messages.len() > MAX_TOTAL {
            let extra = d.messages.len() - MAX_TOTAL;
            trim_oldest(&mut d.messages, |_| true, extra);
        }
        self.save(&d)
    }

    pub fn set_index(&self, mut index: RoomsIndex) -> Result<(), String> {
        index.rooms.retain(|e| e.room.kind == RoomKind::Session);
        let mut d = self.lock();
        d.indexes.insert(index.machine.clone(), index);
        self.save(&d)
    }

    pub fn note_runner(&self, device: &str, index: &RoomsIndex) -> Result<(), String> {
        let mut d = self.lock();
        d.runners.insert(
            device.to_owned(),
            RunnerInfo {
                machine: index.machine.clone(),
                machine_label: index.machine_label.clone(),
                terminal_input: index.terminal_input,
            },
        );
        self.save(&d)
    }

    pub fn sent(&self) -> Vec<(String, String)> {
        self.lock().sent.clone()
    }

    pub fn set_sent(&self, log: Vec<(String, String)>) -> Result<(), String> {
        let mut d = self.lock();
        d.sent = log;
        self.save(&d)
    }

    pub fn version(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let d = self.lock();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_string(&d.indexes)
            .unwrap_or_default()
            .hash(&mut h);
        h.finish()
    }

    pub fn prune(&self, trusted_runners: &[String]) -> Result<(), String> {
        let mut d = self.lock();
        let before = (d.runners.len(), d.indexes.len());
        d.runners.retain(|id, _| trusted_runners.contains(id));
        let machines: Vec<String> = d.runners.values().map(|r| r.machine.clone()).collect();
        d.indexes.retain(|m, _| machines.contains(m));
        if before == (d.runners.len(), d.indexes.len()) {
            return Ok(());
        }
        self.save(&d)
    }

    pub fn runner(&self, device: &str) -> Option<RunnerInfo> {
        self.lock().runners.get(device).cloned()
    }

    pub fn machines(&self, local: &str) -> Vec<(String, String)> {
        let d = self.lock();
        let all: Vec<_> = d.indexes.values().filter(|i| i.machine != local).collect();
        all.iter()
            .map(|i| {
                let shared = all
                    .iter()
                    .any(|o| o.machine != i.machine && o.machine_label == i.machine_label);
                let label = if shared {
                    let n = i.machine.chars().count();
                    let tail: String = i.machine.chars().skip(n.saturating_sub(4)).collect();
                    format!("{} ({tail})", i.machine_label)
                } else {
                    i.machine_label.clone()
                };
                (i.machine.clone(), label)
            })
            .collect()
    }

    pub fn rooms(&self, local: &str) -> Vec<RoomInfo> {
        let d = self.lock();
        let last = |room: &RoomRef| {
            d.messages
                .iter()
                .filter(|m| m.room == *room)
                .max_by(|a, b| (a.ts_ms, &a.id).cmp(&(b.ts_ms, &b.id)))
                .cloned()
        };
        let info = |room: RoomRef, title: String, state, context_pct| RoomInfo {
            last: last(&room),
            room,
            title,
            agent: None,
            state,
            context_pct,
            context_tokens: None,
            model: None,
            unrestricted: false,
            mention: None,
        };
        let mut rooms: Vec<RoomInfo> = Vec::new();
        for e in d
            .indexes
            .values()
            .filter(|i| i.machine != local)
            .flat_map(|i| i.rooms.iter())
        {
            if !rooms.iter().any(|r| r.room == e.room) {
                rooms.push(info(
                    e.room.clone(),
                    e.title.clone(),
                    e.state.clone(),
                    e.context_pct,
                ));
            }
        }
        for m in d.messages.iter().filter(|m| m.room.machine != local) {
            if !rooms.iter().any(|r| r.room == m.room) {
                let title = m.room.session.clone().unwrap_or_default();
                rooms.push(info(m.room.clone(), title, None, None));
            }
        }
        rooms.sort_by(|a, b| {
            let ts = |r: &RoomInfo| r.last.as_ref().map(|m| m.ts_ms);
            ts(b).cmp(&ts(a)).then_with(|| a.title.cmp(&b.title))
        });
        rooms
    }

    pub fn history(&self, room: &RoomRef, limit: usize) -> Vec<ChatMessage> {
        let mut all: Vec<ChatMessage> = self
            .lock()
            .messages
            .iter()
            .filter(|m| m.room == *room)
            .cloned()
            .collect();
        all.sort_by(|a, b| (a.ts_ms, &a.id).cmp(&(b.ts_ms, &b.id)));
        all.drain(..all.len().saturating_sub(limit));
        all
    }
}

fn trim_oldest(msgs: &mut Vec<ChatMessage>, matches: impl Fn(&ChatMessage) -> bool, n: usize) {
    let mut order: Vec<(u64, String)> = msgs
        .iter()
        .filter(|m| matches(m))
        .map(|m| (m.ts_ms, m.id.clone()))
        .collect();
    order.sort();
    let drop: std::collections::HashSet<String> =
        order.into_iter().take(n).map(|(_, id)| id).collect();
    msgs.retain(|m| !(matches(m) && drop.contains(&m.id)));
}

impl ChatSink for CacheSink {
    fn deliver(&self, _: &str, _: Option<&str>, msg: ChatMessage) -> Result<(), String> {
        self.0.add(msg)
    }
}

impl RoomsSink for CacheSink {
    fn deliver_rooms(&self, from_device: &str, index: RoomsIndex) {
        let _ = self.0.note_runner(from_device, &index);
        let _ = self.0.set_index(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_chat_relay::rooms::{Role, RoomEntry, ROOMS_V};
    use tmgr_ipc::chat::{Author, AuthorKind, MessageKind, CHAT_V};

    const LOCAL: &str = "mx_00000000000000aa";
    const PEER: &str = "mx_00000000000000bb";

    fn msg(n: u32, room: RoomRef, ts: u64) -> ChatMessage {
        ChatMessage {
            v: CHAT_V,
            id: format!("m_{n:032x}"),
            room,
            author: Author {
                kind: AuthorKind::Owner,
                id: "owner".into(),
                machine: None,
                device: None,
                agent: None,
            },
            ts_ms: ts,
            kind: MessageKind::Text,
            body_md: format!("body {n}"),
            attachments: Vec::new(),
            reply_to: None,
            decision: None,
            session_meta: None,
            user_turn: false,
            mention: None,
        }
    }

    fn index(machine: &str, rooms: Vec<RoomRef>) -> RoomsIndex {
        RoomsIndex {
            v: ROOMS_V,
            machine: machine.into(),
            machine_label: "Laptop".into(),
            role: Role::Top,
            followers: None,
            rooms: rooms
                .into_iter()
                .map(|room| RoomEntry {
                    title: room.session.clone().unwrap_or_else(|| "Manager".into()),
                    room,
                    state: Some("idle".into()),
                    context_pct: Some(3),
                    last_ts: None,
                    mention_ok: false,
                    agent: None,
                    context_tokens: None,
                })
                .collect(),
            activity: false,
            blob: false,
            terminal_input: false,
        }
    }

    #[test]
    fn rooms_of_other_machines_come_from_the_index_and_from_messages_never_the_local_one() {
        let c = RoomCache::memory();
        let s1 = RoomRef::session(PEER, "s_1");
        c.set_index(index(PEER, vec![RoomRef::manager(PEER), s1.clone()]))
            .unwrap();
        c.set_index(index(LOCAL, vec![RoomRef::manager(LOCAL)]))
            .unwrap();
        c.add(msg(1, s1.clone(), 10)).unwrap();
        c.add(msg(2, RoomRef::session(PEER, "s_9"), 20)).unwrap();
        c.add(msg(3, RoomRef::manager(LOCAL), 30)).unwrap();
        c.add(msg(4, RoomRef::manager(PEER), 40)).unwrap();
        let rooms = c.rooms(LOCAL);
        let ids: Vec<_> = rooms.iter().map(|r| r.room.clone()).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.iter().all(|r| r.machine == PEER));
        assert!(ids.iter().all(|r| r.kind == RoomKind::Session));
        assert_eq!(rooms[0].room, RoomRef::session(PEER, "s_9"));
        assert_eq!(rooms[1].room, s1);
        assert_eq!(rooms[1].state.as_deref(), Some("idle"));
        assert_eq!(rooms[1].last.as_ref().unwrap().id, format!("m_{:032x}", 1));
        assert_eq!(c.machines(LOCAL), [(PEER.to_owned(), "Laptop".to_owned())]);
    }

    #[test]
    fn history_is_ordered_deduped_and_limited_to_the_room() {
        let c = RoomCache::memory();
        let room = RoomRef::session(PEER, "s_0");
        c.add(msg(2, room.clone(), 20)).unwrap();
        c.add(msg(1, room.clone(), 10)).unwrap();
        c.add(msg(1, room.clone(), 10)).unwrap();
        c.add(msg(3, RoomRef::session(PEER, "s_1"), 5)).unwrap();
        let all = c.history(&room, 10);
        assert_eq!(all.iter().map(|m| m.ts_ms).collect::<Vec<_>>(), [10, 20]);
        assert_eq!(c.history(&room, 1)[0].ts_ms, 20);
    }

    #[test]
    fn a_room_keeps_only_its_newest_messages() {
        let c = RoomCache::memory();
        let room = RoomRef::session(PEER, "s_0");
        let other = RoomRef::session(PEER, "s_1");
        c.add(msg(100_000, other.clone(), 1)).unwrap();
        for n in 0..(MAX_PER_ROOM as u32 + 5) {
            c.add(msg(n, room.clone(), 1000 + u64::from(n))).unwrap();
        }
        let h = c.history(&room, usize::MAX);
        assert_eq!(h.len(), MAX_PER_ROOM);
        assert_eq!(h[0].ts_ms, 1005);
        assert_eq!(c.history(&other, 10).len(), 1);
    }

    #[test]
    fn the_cache_survives_a_restart_in_a_private_file_and_wipe_removes_it() {
        use std::os::unix::fs::PermissionsExt;
        let t = tempfile::tempdir().unwrap();
        let room = RoomRef::session(PEER, "s_0");
        let c = RoomCache::open(RelayDir::open_named(t.path(), "attach-relay").unwrap()).unwrap();
        c.add(msg(1, room.clone(), 10)).unwrap();
        c.set_index(index(PEER, vec![room.clone()])).unwrap();
        let file = t.path().join("attach-relay").join(FILE);
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let again =
            RoomCache::open(RelayDir::open_named(t.path(), "attach-relay").unwrap()).unwrap();
        assert_eq!(again.history(&room, 10).len(), 1);
        assert_eq!(again.machines(LOCAL).len(), 1);
        again.wipe().unwrap();
        assert!(!file.exists());
        assert!(again.rooms(LOCAL).is_empty());
    }

    #[test]
    fn the_sink_stores_delivered_messages_and_indexes() {
        let c = RoomCache::memory();
        let sink = CacheSink(c.clone());
        let room = RoomRef::session(PEER, "s_0");
        let manager = RoomRef::manager(PEER);
        sink.deliver("d1", Some("runner"), msg(1, room.clone(), 10))
            .unwrap();
        sink.deliver("d1", Some("runner"), msg(2, manager.clone(), 11))
            .unwrap();
        sink.deliver_rooms("d1", index(PEER, vec![room.clone(), manager.clone()]));
        assert_eq!(c.history(&room, 10).len(), 1);
        assert!(c.history(&manager, 10).is_empty());
        assert_eq!(c.machines(LOCAL).len(), 1);
        assert_eq!(c.rooms(LOCAL).len(), 1);
    }

    #[test]
    fn pruning_drops_runners_and_the_machines_they_alone_vouched_for() {
        let c = RoomCache::memory();
        let sink = CacheSink(c.clone());
        let other = "mx_00000000000000cc";
        sink.deliver_rooms("d1", index(PEER, vec![RoomRef::session(PEER, "s_0")]));
        sink.deliver_rooms("d2", index(other, vec![RoomRef::session(other, "s_0")]));
        c.prune(&["d1".to_owned()]).unwrap();
        assert!(c.runner("d1").is_some() && c.runner("d2").is_none());
        let machines: Vec<_> = c.machines(LOCAL).into_iter().map(|m| m.0).collect();
        assert_eq!(machines, [PEER]);
        assert_eq!(c.rooms(LOCAL).len(), 1);
    }

    #[test]
    fn the_version_changes_with_room_titles_and_states_and_the_sent_log_persists() {
        let c = RoomCache::memory();
        let before = c.version();
        let mut idx = index(PEER, vec![RoomRef::session(PEER, "s_0")]);
        c.set_index(idx.clone()).unwrap();
        let first = c.version();
        assert_ne!(before, first);
        idx.rooms[0].state = Some("busy".into());
        c.set_index(idx.clone()).unwrap();
        assert_ne!(first, c.version());
        idx.rooms[0].title = "renamed".into();
        let second = c.version();
        c.set_index(idx).unwrap();
        assert_ne!(second, c.version());
        c.set_sent(vec![("m".into(), "r".into())]).unwrap();
        assert_eq!(c.sent(), [("m".to_owned(), "r".to_owned())]);
    }

    #[test]
    fn terminal_input_is_remembered_per_runner_device_even_on_one_machine() {
        let c = RoomCache::memory();
        let sink = CacheSink(c.clone());
        let mut on = index(PEER, vec![]);
        on.terminal_input = true;
        sink.deliver_rooms("old", index(PEER, vec![]));
        sink.deliver_rooms("new", on);
        let (old, new) = (c.runner("old").unwrap(), c.runner("new").unwrap());
        assert!(!old.terminal_input && new.terminal_input);
        assert_eq!(
            (new.machine.as_str(), new.machine_label.as_str()),
            (PEER, "Laptop")
        );
        assert!(c.runner("none").is_none());
        c.wipe().unwrap();
        assert!(c.runner("new").is_none());
    }
}
