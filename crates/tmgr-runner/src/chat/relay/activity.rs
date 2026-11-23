use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tmgr_chat_relay::engine::{ActivitySubSink, ChatSink, KeyStore, RelayEngine, Transport};
use tmgr_ipc::chat::{ActivityState, ChatActivity, ToolCall};

use super::pump::FailureLog;
use crate::attach::redact;
use crate::chat::ActivityRx;

pub const FLUSH_EVERY: Duration = Duration::from_secs(3);
const LEASE_MS: u64 = 150_000;
const MAX_LEASES: usize = 20;
const KEEPALIVE_MS: u64 = 30_000;
const MAX_ROOMS: usize = 256;
const MAX_CALLS: usize = 10;
const MAX_USED: usize = 50;
const MAX_ENVELOPE: usize = 48 * 1024;
const V: u32 = 1;

pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

pub struct Leases {
    clock: Clock,
    map: Mutex<BTreeMap<String, u64>>,
}

impl Leases {
    pub fn new(clock: Clock) -> Self {
        Self {
            clock,
            map: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn active(&self) -> Vec<String> {
        let now = (self.clock)();
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, exp| *exp > now);
        map.keys().cloned().collect()
    }
}

impl ActivitySubSink for Leases {
    fn deliver_activity_on(&self, from_device: &str, on: bool) {
        let now = (self.clock)();
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        if !on {
            map.remove(from_device);
            return;
        }
        map.retain(|_, exp| *exp > now);
        if map.len() >= MAX_LEASES && !map.contains_key(from_device) {
            return;
        }
        map.insert(from_device.to_owned(), now + LEASE_MS);
    }
}

pub trait ActivitySend: Send + Sync {
    fn send(&self, payload: &Value, only: &[String]) -> Result<(), String>;
}

impl<T: Transport, K: KeyStore, S: ChatSink> ActivitySend for RelayEngine<T, K, S> {
    fn send(&self, payload: &Value, only: &[String]) -> Result<(), String> {
        let id = tmgr_chat_relay::wire::random_msg_id("activity").map_err(|e| e.to_string())?;
        self.send_activity(&id, payload, only)
    }
}

fn blank(e: &ChatActivity) -> bool {
    e.state == ActivityState::Idle && e.current.is_none() && e.used.is_empty() && e.calls.is_empty()
}

fn working(e: &ChatActivity) -> bool {
    e.state == ActivityState::Working
}

fn sanitize(e: &ChatActivity) -> ChatActivity {
    let skip = e.used.len().saturating_sub(MAX_USED);
    let from = e.calls.len().saturating_sub(MAX_CALLS);
    ChatActivity {
        v: V,
        room: e.room.clone(),
        state: e.state,
        current: e.current.as_deref().map(redact::summary),
        used: e.used[skip..].iter().map(|u| redact::summary(u)).collect(),
        calls: e.calls[from..]
            .iter()
            .map(|c| ToolCall {
                n: c.n,
                name: redact::summary(&c.name),
                summary: redact::summary(&c.summary),
                result: c.result.as_deref().and_then(redact::excerpt),
                running: c.running,
                error: c.error,
            })
            .collect(),
    }
}

fn room_json(e: &ChatActivity) -> Value {
    let mut v = serde_json::to_value(e).unwrap_or(Value::Null);
    if let Some(o) = v.as_object_mut() {
        o.remove("v");
    }
    v
}

fn payload(machine: &str, snapshot: bool, rooms: &[Value]) -> Value {
    json!({"v": V, "machine": machine, "snapshot": snapshot, "rooms": rooms})
}

fn size(machine: &str, snapshot: bool, rooms: &[Value]) -> usize {
    serde_json::to_vec(&payload(machine, snapshot, rooms)).map_or(usize::MAX, |b| b.len())
}

fn degrade(rooms: &[&ChatActivity], level: u8) -> Vec<Value> {
    rooms
        .iter()
        .map(|e| {
            let mut e = (*e).clone();
            if level >= 1 {
                for c in &mut e.calls {
                    c.result = None;
                }
            }
            if level >= 2 {
                e.calls.clear();
            }
            room_json(&e)
        })
        .collect()
}

fn envelopes(machine: &str, snapshot: bool, rooms: &[&ChatActivity]) -> Vec<Value> {
    for level in 0..=2 {
        let r = degrade(rooms, level);
        if size(machine, snapshot, &r) <= MAX_ENVELOPE {
            return vec![payload(machine, snapshot, &r)];
        }
    }
    let r = degrade(rooms, 2);
    let mut parts: Vec<Vec<Value>> = Vec::new();
    for room in r {
        let mut cur = parts.pop().unwrap_or_default();
        cur.push(room);
        if cur.len() > 1 && size(machine, false, &cur) > MAX_ENVELOPE {
            let last = cur.pop().expect("pushed above");
            parts.push(cur);
            cur = vec![last];
        }
        parts.push(cur);
    }
    parts
        .iter()
        .enumerate()
        .map(|(i, p)| payload(machine, snapshot && i == 0, p))
        .collect()
}

struct Entry {
    event: ChatActivity,
    seq: u64,
}

struct Peer {
    seen: u64,
    snapshot_at: u64,
}

pub struct Flusher {
    machine: String,
    rooms: Vec<Entry>,
    seq: u64,
    peers: BTreeMap<String, Peer>,
}

impl Flusher {
    pub fn new(machine: String) -> Self {
        Self {
            machine,
            rooms: Vec::new(),
            seq: 0,
            peers: BTreeMap::new(),
        }
    }

    pub fn ingest(&mut self, events: Vec<ChatActivity>) {
        for raw in events {
            if raw.room.machine != self.machine {
                continue;
            }
            let event = sanitize(&raw);
            let at = self.rooms.iter().position(|e| e.event.room == event.room);
            if let Some(i) = at {
                if self.rooms[i].event == event {
                    continue;
                }
                self.rooms.remove(i);
            } else if self.rooms.len() >= MAX_ROOMS {
                self.rooms.remove(0);
            }
            self.seq += 1;
            self.rooms.push(Entry {
                event,
                seq: self.seq,
            });
        }
    }

    pub fn blank_all(&mut self) {
        let blanks = self
            .rooms
            .iter()
            .filter(|e| !blank(&e.event))
            .map(|e| crate::chat::activity::blank_idle(&e.event.room))
            .collect();
        self.ingest(blanks);
    }

    pub fn flush(
        &mut self,
        leased: &[String],
        now: u64,
        send: &mut dyn FnMut(&str, &Value) -> Result<(), String>,
    ) -> Option<String> {
        self.peers.retain(|d, _| leased.iter().any(|l| l == d));
        let busy = self.rooms.iter().any(|e| working(&e.event));
        let mut first_err = None;
        let mut failed = Vec::new();
        for device in leased {
            let new = !self.peers.contains_key(device);
            let peer = self.peers.entry(device.clone()).or_insert(Peer {
                seen: 0,
                snapshot_at: now,
            });
            let snapshot = new || (busy && now.saturating_sub(peer.snapshot_at) >= KEEPALIVE_MS);
            let rooms: Vec<&ChatActivity> = self
                .rooms
                .iter()
                .filter(|e| {
                    if snapshot {
                        !blank(&e.event)
                    } else {
                        e.seq > peer.seen
                    }
                })
                .map(|e| &e.event)
                .collect();
            peer.seen = self.seq;
            if snapshot {
                peer.snapshot_at = now;
            }
            if rooms.is_empty() && !snapshot {
                continue;
            }
            for p in envelopes(&self.machine, snapshot, &rooms) {
                if let Err(e) = send(device, &p) {
                    first_err.get_or_insert(e);
                    failed.push(device.clone());
                    break;
                }
            }
        }
        for device in failed {
            self.peers.remove(&device);
        }
        self.rooms.retain(|e| !blank(&e.event));
        first_err
    }
}

pub(super) fn tick(
    flusher: &mut Flusher,
    rx: &ActivityRx,
    leases: &Leases,
    clock: &dyn Fn() -> u64,
    sender: &dyn ActivitySend,
    failures: &mut FailureLog,
) {
    flusher.ingest(rx.take(Duration::ZERO));
    if !rx.consented() {
        flusher.blank_all();
    }
    let leased = leases.active();
    let err = flusher.flush(&leased, clock(), &mut |device, p| {
        sender.send(p, std::slice::from_ref(&device.to_owned()))
    });
    match err {
        Some(e) => {
            if let Some(class) = failures.note(&e) {
                crate::daemon::log(&format!("chat relay: activity send failed ({class})"));
            }
        }
        None => failures.clear(),
    }
}

pub(super) fn run_activity(
    machine: &str,
    rx: &ActivityRx,
    leases: &Leases,
    clock: &dyn Fn() -> u64,
    sender: &dyn ActivitySend,
    stop: &dyn Fn() -> bool,
    sleep: &dyn Fn(Duration),
) {
    let mut flusher = Flusher::new(machine.to_owned());
    let mut failures = FailureLog::default();
    while !stop() {
        sleep(FLUSH_EVERY);
        if stop() {
            return;
        }
        tick(&mut flusher, rx, leases, clock, sender, &mut failures);
    }
}

#[cfg(test)]
mod tests;
