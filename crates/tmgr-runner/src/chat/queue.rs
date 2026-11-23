use super::mention::QueueEnd;
use super::{lock, Chat, Delivery, NOT_DELIVERED};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use tmgr_chat_relay::engine::ReceiptState;
use tmgr_ipc::chat::RoomRef;

pub const QUEUE_CAP: usize = 20;
pub const DEFAULT_TTL_MS: u64 = 30 * 60 * 1000;
const FINISHED_REMEMBERED: usize = 512;
const QUEUE_FILE: &str = "queue.json";
const BATCH_MAX: usize = QUEUE_CAP;
const BATCH_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submit {
    Delivered,
    Queued,
    Gone(String),
    Refused(&'static str),
    Full,
}

pub(super) struct Entry {
    id: String,
    body_md: String,
    client: String,
    device: Option<String>,
    owner_device: bool,
    enq_ms: u64,
    ttl_ms: u64,
    why: Option<&'static str>,
    durable: bool,
}

#[derive(Serialize, Deserialize)]
struct Row {
    id: String,
    room: RoomRef,
    body_md: String,
    client: String,
    device: Option<String>,
    owner_device: bool,
    enq_ms: u64,
    ttl_ms: u64,
}

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    #[serde(default)]
    queue: Vec<Row>,
    #[serde(default)]
    finished: Vec<String>,
}

#[derive(Default)]
pub(super) struct Queue {
    rooms: HashMap<RoomRef, VecDeque<Entry>>,
    finished: VecDeque<String>,
    path: Option<PathBuf>,
}

impl Queue {
    pub(super) fn open(dir: Option<&Path>) -> Self {
        let mut q = Queue {
            path: dir.map(|d| d.join(QUEUE_FILE)),
            ..Queue::default()
        };
        let Some(path) = q.path.clone() else { return q };
        let read = || -> io::Result<Option<Saved>> {
            let mut f = match OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&path)
            {
                Ok(f) => f,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e),
            };
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            serde_json::from_slice(&buf)
                .map(Some)
                .map_err(io::Error::other)
        };
        match read() {
            Ok(Some(saved)) => {
                q.finished = saved.finished.into_iter().collect();
                for r in saved.queue {
                    q.rooms.entry(r.room).or_default().push_back(Entry {
                        id: r.id,
                        body_md: r.body_md,
                        client: r.client,
                        device: r.device,
                        owner_device: r.owner_device,
                        enq_ms: r.enq_ms,
                        ttl_ms: r.ttl_ms,
                        why: None,
                        durable: true,
                    });
                }
            }
            Ok(None) => {}
            Err(e) => crate::daemon::log(&format!("chat: queue not restored: {e}")),
        }
        q
    }

    fn queued(&self, room: &RoomRef, id: &str) -> bool {
        self.rooms
            .get(room)
            .is_some_and(|e| e.iter().any(|e| e.id == id))
    }

    fn mark_finished(&mut self, id: &str) {
        if self.finished.iter().any(|i| i == id) {
            return;
        }
        if self.finished.len() >= FINISHED_REMEMBERED {
            self.finished.pop_front();
        }
        self.finished.push_back(id.to_owned());
    }

    fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let queue: Vec<Row> = self
            .rooms
            .iter()
            .flat_map(|(room, entries)| {
                entries.iter().filter(|e| e.durable).map(move |e| Row {
                    id: e.id.clone(),
                    room: room.clone(),
                    body_md: e.body_md.clone(),
                    client: e.client.clone(),
                    device: e.device.clone(),
                    owner_device: e.owner_device,
                    enq_ms: e.enq_ms,
                    ttl_ms: e.ttl_ms,
                })
            })
            .collect();
        if queue.is_empty() && self.finished.is_empty() {
            return match fs::remove_file(path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        let saved = Saved {
            queue,
            finished: self.finished.iter().cloned().collect(),
        };
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let tmp = path.with_extension(format!(
            "json.{}.{}.tmp",
            std::process::id(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let write = || -> io::Result<()> {
            let mut f = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&tmp)?;
            f.write_all(&serde_json::to_vec(&saved).map_err(io::Error::other)?)?;
            f.sync_all()?;
            fs::rename(&tmp, path)
        };
        let r = write();
        if r.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        r
    }

    fn save_logged(&self) {
        if let Err(e) = self.save() {
            crate::daemon::log(&format!("chat: queue not saved: {e}"));
        }
    }

    fn finish(&mut self, id: &str, durable: bool) {
        if durable {
            self.mark_finished(id);
            self.save_logged();
        }
    }

    fn pop(&mut self, room: &RoomRef) {
        let Some(entries) = self.rooms.get_mut(room) else {
            return;
        };
        let Some(e) = entries.pop_front() else { return };
        if entries.is_empty() {
            self.rooms.remove(room);
        }
        self.finish(&e.id, e.durable);
    }

    fn pop_n(&mut self, room: &RoomRef, n: usize) {
        let Some(entries) = self.rooms.get_mut(room) else {
            return;
        };
        let mut any = false;
        let mut ids = Vec::new();
        for _ in 0..n {
            let Some(e) = entries.pop_front() else { break };
            if e.durable {
                any = true;
                ids.push(e.id);
            }
        }
        if entries.is_empty() {
            self.rooms.remove(room);
        }
        for id in &ids {
            self.mark_finished(id);
        }
        if any {
            self.save_logged();
        }
    }
}

struct Note {
    room: RoomRef,
    text: String,
    reason: String,
    id: String,
    client: String,
    device: Option<String>,
}

impl Note {
    fn of(room: &RoomRef, e: &Entry, text: String) -> Self {
        Self {
            room: room.clone(),
            reason: text.clone(),
            text,
            id: e.id.clone(),
            client: e.client.clone(),
            device: e.device.clone(),
        }
    }
}

fn gone_reason(r: &str) -> String {
    const KNOWN: [&str; 3] = [
        "the session no longer exists",
        "no manager is running",
        "the session has exited",
    ];
    if KNOWN.contains(&r) {
        format!("{r}.")
    } else {
        "the message could not be typed into the session.".into()
    }
}

fn full_reason() -> String {
    format!("the queue for this session is full ({QUEUE_CAP} messages).")
}

pub struct Item<'a> {
    pub room: &'a RoomRef,
    pub id: &'a str,
    pub body: &'a str,
    pub client: &'a str,
    pub device: Option<&'a str>,
    pub owner_device: bool,
}

impl Chat {
    #[allow(clippy::too_many_arguments)]
    pub fn submit(
        &self,
        room: &RoomRef,
        id: &str,
        body: &str,
        client: &str,
        device: Option<&str>,
        owner_device: bool,
        now_ms: u64,
        mut deliver: impl FnMut(&str, &str, Option<&str>, bool) -> Delivery,
    ) -> Submit {
        self.submit_with(
            room,
            id,
            body,
            client,
            device,
            owner_device,
            now_ms,
            DEFAULT_TTL_MS,
            |it| deliver(it.body, it.client, it.device, it.owner_device),
            |_, _| {},
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn submit_with(
        &self,
        room: &RoomRef,
        id: &str,
        body: &str,
        client: &str,
        device: Option<&str>,
        owner_device: bool,
        now_ms: u64,
        ttl_ms: u64,
        deliver: impl FnMut(&Item) -> Delivery,
        on_delivered: impl FnMut(&Item, &str),
    ) -> Submit {
        self.submit_inner(
            room,
            id,
            body,
            client,
            device,
            owner_device,
            now_ms,
            ttl_ms,
            false,
            deliver,
            on_delivered,
        )
        .unwrap_or(Submit::Queued)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn submit_inner(
        &self,
        room: &RoomRef,
        id: &str,
        body: &str,
        client: &str,
        device: Option<&str>,
        owner_device: bool,
        now_ms: u64,
        ttl_ms: u64,
        strict: bool,
        mut deliver: impl FnMut(&Item) -> Delivery,
        mut on_delivered: impl FnMut(&Item, &str),
    ) -> Result<Submit, String> {
        let mut q = lock(&self.queue);
        if q.queued(room, id) {
            return Ok(Submit::Queued);
        }
        let durable = !self.is_mention_open(id);
        let backlog = q.rooms.get(room).map_or(0, VecDeque::len);
        let mut busy_why = q
            .rooms
            .get(room)
            .and_then(|e| e.front())
            .and_then(|e| e.why);
        if backlog == 0 {
            let item = Item {
                room,
                id,
                body,
                client,
                device,
                owner_device,
            };
            match deliver(&item) {
                Delivery::Delivered => {
                    self.remember_delivered(room, body);
                    on_delivered(&item, body);
                    q.finish(id, durable);
                    self.receipt(client, device, id, ReceiptState::Delivered, None);
                    return Ok(Submit::Delivered);
                }
                Delivery::Typed(typed) => {
                    self.remember_delivered(room, &typed);
                    on_delivered(&item, &typed);
                    q.finish(id, durable);
                    self.receipt(client, device, id, ReceiptState::Delivered, None);
                    return Ok(Submit::Delivered);
                }
                Delivery::Gone(r) => {
                    q.finish(id, durable);
                    self.receipt(
                        client,
                        device,
                        id,
                        ReceiptState::Failed,
                        Some(&gone_reason(&r)),
                    );
                    return Ok(Submit::Gone(r));
                }
                Delivery::Refused(why) => {
                    q.finish(id, durable);
                    self.receipt(client, device, id, ReceiptState::Failed, Some(why));
                    return Ok(Submit::Refused(why));
                }
                Delivery::Busy(why) => busy_why = why,
            }
        }
        if backlog >= QUEUE_CAP {
            q.finish(id, durable);
            self.receipt(
                client,
                device,
                id,
                ReceiptState::Failed,
                Some(&full_reason()),
            );
            return Ok(Submit::Full);
        }
        q.rooms.entry(room.clone()).or_default().push_back(Entry {
            id: id.into(),
            body_md: body.into(),
            client: client.into(),
            device: device.map(String::from),
            owner_device,
            enq_ms: now_ms,
            ttl_ms,
            why: busy_why,
            durable,
        });
        if durable {
            if let Err(e) = q.save() {
                if strict {
                    if let Some(entries) = q.rooms.get_mut(room) {
                        entries.pop_back();
                        if entries.is_empty() {
                            q.rooms.remove(room);
                        }
                    }
                    return Err(format!("queue not saved: {e}"));
                }
                crate::daemon::log(&format!("chat: queue not saved: {e}"));
            }
        }
        self.receipt(client, device, id, ReceiptState::Queued, busy_why);
        Ok(Submit::Queued)
    }

    pub(super) fn is_pending(&self, room: &RoomRef, id: &str) -> bool {
        let q = lock(&self.queue);
        q.queued(room, id) || q.finished.iter().any(|i| i == id)
    }

    pub(super) fn finish_id(&self, id: &str) {
        lock(&self.queue).finish(id, true);
    }

    pub fn drain_queue(
        &self,
        now_ms: u64,
        mut deliver: impl FnMut(&RoomRef, &str, &str, Option<&str>, bool) -> Delivery,
    ) {
        self.drain_queue_with(
            now_ms,
            |it| deliver(it.room, it.body, it.client, it.device, it.owner_device),
            |_, _| {},
        );
    }

    pub fn drain_queue_with(
        &self,
        now_ms: u64,
        mut deliver: impl FnMut(&Item) -> Delivery,
        mut on_delivered: impl FnMut(&Item, &str),
    ) {
        let mut notes: Vec<Note> = Vec::new();
        let mut ends: Vec<(String, QueueEnd)> = Vec::new();
        let owners = self.current_owners();
        let owners_ready = lock(&self.relay_owners).is_some();
        {
            let mut q = lock(&self.queue);
            let rooms: Vec<RoomRef> = q.rooms.keys().cloned().collect();
            for room in &rooms {
                let mut stalled = None;
                while let Some(front) = q.rooms.get(room).and_then(VecDeque::front) {
                    if now_ms.saturating_sub(front.enq_ms) >= front.ttl_ms {
                        let mins = front.ttl_ms / 60_000;
                        notes.push(Note::of(
                            room,
                            front,
                            format!("Not delivered: expired after {mins} minutes in the queue."),
                        ));
                        ends.push((front.id.clone(), QueueEnd::Expired));
                        q.pop(room);
                        continue;
                    }
                    if front.owner_device && !owners_ready {
                        break;
                    }
                    let effective_owner = |e: &Entry| {
                        e.owner_device
                            && now_ms.saturating_sub(e.enq_ms) < super::FRESH_PAST_MS
                            && e.device.as_ref().is_some_and(|d| owners.contains(d))
                    };
                    let solo = |e: &Entry| !e.durable || self.is_mention_open(&e.id);
                    let mut batch = 1;
                    let mut joined = String::new();
                    if !solo(front) {
                        let mut size = front.body_md.len();
                        let entries = q.rooms.get(room).into_iter().flatten().skip(1);
                        for e in entries.take(BATCH_MAX - 1) {
                            if e.client != front.client
                                || e.device != front.device
                                || e.owner_device != front.owner_device
                                || effective_owner(e) != effective_owner(front)
                                || solo(e)
                                || now_ms.saturating_sub(e.enq_ms) >= e.ttl_ms
                                || size + 2 + e.body_md.len() > BATCH_BYTES
                            {
                                break;
                            }
                            size += 2 + e.body_md.len();
                            batch += 1;
                        }
                        if batch > 1 {
                            joined = q
                                .rooms
                                .get(room)
                                .into_iter()
                                .flatten()
                                .take(batch)
                                .map(|e| e.body_md.as_str())
                                .collect::<Vec<_>>()
                                .join("\n\n");
                        }
                    }
                    let body = if batch > 1 {
                        joined.as_str()
                    } else {
                        front.body_md.as_str()
                    };
                    let item = Item {
                        room,
                        id: &front.id,
                        body,
                        client: &front.client,
                        device: front.device.as_deref(),
                        owner_device: effective_owner(front),
                    };
                    match deliver(&item) {
                        Delivery::Delivered => {
                            self.remember_delivered(room, item.body);
                            on_delivered(&item, item.body);
                            for e in q.rooms.get(room).into_iter().flatten().take(batch) {
                                self.receipt(
                                    &e.client,
                                    e.device.as_deref(),
                                    &e.id,
                                    ReceiptState::Delivered,
                                    None,
                                );
                            }
                            q.pop_n(room, batch);
                        }
                        Delivery::Typed(typed) => {
                            self.remember_delivered(room, &typed);
                            on_delivered(&item, &typed);
                            for e in q.rooms.get(room).into_iter().flatten().take(batch) {
                                self.receipt(
                                    &e.client,
                                    e.device.as_deref(),
                                    &e.id,
                                    ReceiptState::Delivered,
                                    None,
                                );
                            }
                            q.pop_n(room, batch);
                        }
                        Delivery::Refused(why) => {
                            for e in q.rooms.get(room).into_iter().flatten().take(batch) {
                                notes.push(Note::of(room, e, why.to_string()));
                                ends.push((e.id.clone(), QueueEnd::Refused(why.to_string())));
                            }
                            q.pop_n(room, batch);
                        }
                        Delivery::Busy(why) => {
                            stalled = Some(why);
                            break;
                        }
                        Delivery::Gone(r) => {
                            for e in q.rooms.get(room).into_iter().flatten().take(batch) {
                                let mut note = Note::of(room, e, format!("Not delivered: {r}."));
                                note.reason = gone_reason(&r);
                                notes.push(note);
                                ends.push((e.id.clone(), QueueEnd::Gone(r.clone())));
                            }
                            q.pop_n(room, batch);
                        }
                    }
                }
                if let Some(why) = stalled {
                    if let Some(entries) = q.rooms.get_mut(room) {
                        entries.iter_mut().for_each(|e| e.why = why);
                    }
                }
            }
        }
        for n in notes {
            self.note_for(&n.room, &n.text, &n.id);
            self.receipt(
                &n.client,
                n.device.as_deref(),
                &n.id,
                ReceiptState::Failed,
                Some(&n.reason),
            );
        }
        for (id, end) in ends {
            self.mention_queue_end(&id, end);
        }
    }

    fn current_owners(&self) -> Vec<String> {
        let queued = lock(&self.queue)
            .rooms
            .values()
            .flatten()
            .any(|e| e.owner_device);
        let check = lock(&self.relay_owners).clone();
        match check {
            Some(f) if queued => f(),
            _ => Vec::new(),
        }
    }

    pub fn queued_ids(&self, room: &RoomRef) -> Vec<String> {
        lock(&self.queue)
            .rooms
            .get(room)
            .map(|e| e.iter().map(|e| e.id.clone()).collect())
            .unwrap_or_default()
    }

    pub fn queued_why(&self, room: &RoomRef) -> HashMap<String, String> {
        lock(&self.queue)
            .rooms
            .get(room)
            .map(|e| {
                e.iter()
                    .filter_map(|e| Some((e.id.clone(), e.why?.to_owned())))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn cancel(&self, room: &RoomRef, id: &str) -> bool {
        let removed = {
            let mut q = lock(&self.queue);
            let Some(entries) = q.rooms.get_mut(room) else {
                return false;
            };
            let Some(pos) = entries.iter().position(|e| e.id == id) else {
                return false;
            };
            let entry = entries.remove(pos);
            if entries.is_empty() {
                q.rooms.remove(room);
            }
            if let Some(e) = &entry {
                q.finish(&e.id, e.durable);
            }
            entry
        };
        self.note_for(room, "Not delivered: cancelled.", id);
        if let Some(e) = &removed {
            self.receipt(
                &e.client,
                e.device.as_deref(),
                id,
                ReceiptState::Failed,
                Some("cancelled."),
            );
        }
        self.mention_queue_end(id, QueueEnd::Cancelled);
        true
    }

    pub fn note_undelivered(&self, room: &RoomRef, id: &str, outcome: &Submit) {
        match outcome {
            Submit::Gone(r) => self.note_for(room, &format!("Not delivered: {r}."), id),
            Submit::Refused(why) => self.note_for(room, why, id),
            Submit::Full => self.note_for(room, &format!("{NOT_DELIVERED}{}", full_reason()), id),
            Submit::Delivered | Submit::Queued => {}
        }
    }

    fn remember_delivered(&self, room: &RoomRef, body: &str) {
        if let Some(session) = room.session.as_deref() {
            self.remember_sent(session, body);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::FRESH_PAST_MS;
    use std::cell::RefCell;
    use tmgr_chat_relay::engine::ReceiptState;

    const T0: u64 = 1_800_000_000_000;

    fn chat() -> (tempfile::TempDir, Chat) {
        let t = tempfile::tempdir().unwrap();
        let c = Chat::open(t.path());
        (t, c)
    }

    fn systems(c: &Chat, room: &RoomRef) -> Vec<String> {
        c.history(room, None, None)
            .unwrap()
            .into_iter()
            .filter(|m| m.author.kind == tmgr_ipc::chat::AuthorKind::System)
            .map(|m| m.body_md)
            .collect()
    }

    fn mid(n: u64) -> String {
        tmgr_ipc::chat::message_id_at(n, [7; 10])
    }

    fn note_replies(c: &Chat, room: &RoomRef) -> Vec<Option<String>> {
        c.history(room, None, None)
            .unwrap()
            .into_iter()
            .filter(|m| m.author.kind == tmgr_ipc::chat::AuthorKind::System)
            .map(|m| m.reply_to)
            .collect()
    }

    fn busy(_: &str, _: &str, _: Option<&str>, _: bool) -> Delivery {
        Delivery::Busy(None)
    }

    fn drain_with(c: &Chat, now: u64, d: Delivery, got: &RefCell<Vec<String>>) {
        c.drain_queue(now, |_, body, _, _, _| {
            if d == Delivery::Delivered {
                got.borrow_mut().push(body.into());
            }
            d.clone()
        });
    }

    #[test]
    fn a_queued_relay_message_is_delivered_with_its_device_and_remembers_the_typed_text() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        c.submit(
            &room,
            "m1",
            "hi",
            "relay",
            Some("d_phone1"),
            false,
            T0,
            busy,
        );
        let seen = RefCell::new(None);
        c.drain_queue(T0 + 1, |_, _, client, device, _| {
            *seen.borrow_mut() = Some((client.to_string(), device.map(String::from)));
            Delivery::Typed("[marker]\n\nhi".into())
        });
        assert_eq!(
            *seen.borrow(),
            Some(("relay".to_string(), Some("d_phone1".to_string())))
        );
        assert!(c.take_sent("s_abcdefghij", "[marker]\n\nhi\n"));
        assert!(c.queued_ids(&room).is_empty());
    }

    #[test]
    fn the_owner_device_flag_counts_at_drain_only_within_ten_minutes_of_enqueue() {
        let (_t, c) = chat();
        c.set_relay_owners(Some(std::sync::Arc::new(|| vec!["d_phone1".to_owned()])));
        let room = c.session_room("s_abcdefghij");
        let seen = RefCell::new(Vec::new());
        let drain = |now: u64| {
            c.drain_queue(now, |_, _, _, _, owner| {
                seen.borrow_mut().push(owner);
                Delivery::Delivered
            });
        };
        c.submit(&room, "m1", "hi", "relay", Some("d_phone1"), true, T0, busy);
        drain(T0 + 1);
        c.submit(&room, "m2", "hi", "relay", Some("d_phone1"), true, T0, busy);
        drain(T0 + FRESH_PAST_MS - 1);
        c.submit(&room, "m3", "hi", "relay", Some("d_phone1"), true, T0, busy);
        drain(T0 + FRESH_PAST_MS);
        c.submit(
            &room,
            "m4",
            "hi",
            "relay",
            Some("d_phone1"),
            false,
            T0,
            busy,
        );
        drain(T0 + 1);
        assert_eq!(*seen.borrow(), [true, true, false, false]);
    }

    #[test]
    fn the_owner_device_flag_drops_at_drain_once_the_device_is_no_longer_an_owner() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        let seen = RefCell::new(Vec::new());
        let drain = || {
            c.drain_queue(T0 + 1, |_, _, _, _, owner| {
                seen.borrow_mut().push(owner);
                Delivery::Delivered
            });
        };
        c.submit(&room, "m1", "hi", "relay", Some("d_phone1"), true, T0, busy);
        drain();
        c.set_relay_owners(Some(std::sync::Arc::new(Vec::new)));
        c.submit(&room, "m2", "hi", "relay", Some("d_phone1"), true, T0, busy);
        drain();
        c.set_relay_owners(Some(std::sync::Arc::new(|| vec!["d_other".to_owned()])));
        c.submit(&room, "m3", "hi", "relay", Some("d_phone1"), true, T0, busy);
        drain();
        assert_eq!(*seen.borrow(), [false, false]);
    }

    #[test]
    fn a_policy_refusal_at_drain_time_notes_its_text_and_drops_the_entry() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        c.submit(&room, "m1", "hi", "relay", None, false, T0, busy);
        c.drain_queue(T0 + 1, |_, _, _, _, _| {
            Delivery::Refused("Not delivered: nope.")
        });
        assert!(c.queued_ids(&room).is_empty());
        assert_eq!(systems(&c, &room), ["Not delivered: nope."]);
    }

    #[test]
    fn a_ready_target_delivers_at_once_and_remembers_the_text() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        let r = c.submit(&room, "m1", "hi", "relay", None, false, T0, |_, _, _, _| {
            Delivery::Delivered
        });
        assert_eq!(r, Submit::Delivered);
        assert!(c.queued_ids(&room).is_empty());
        assert!(c.take_sent("s_abcdefghij", "hi"));
    }

    #[test]
    fn a_busy_target_queues_without_remembering_or_noting() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        assert_eq!(
            c.submit(&room, "m1", "hi", "relay", None, false, T0, busy),
            Submit::Queued
        );
        assert_eq!(c.queued_ids(&room), ["m1"]);
        assert!(!c.take_sent("s_abcdefghij", "hi"));
        assert!(systems(&c, &room).is_empty());
    }

    #[test]
    fn drain_delivers_when_the_target_becomes_ready() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        c.submit(&room, "m1", "hi", "desktop", None, false, T0, busy);
        let got = RefCell::new(Vec::new());
        drain_with(&c, T0 + 1, Delivery::Busy(None), &got);
        assert_eq!(c.queued_ids(&room), ["m1"]);
        drain_with(&c, T0 + 2, Delivery::Delivered, &got);
        assert_eq!(*got.borrow(), ["hi"]);
        assert!(c.queued_ids(&room).is_empty());
        assert!(c.take_sent("s_abcdefghij", "hi"));
    }

    #[test]
    fn a_new_message_queues_behind_older_ones_even_when_the_target_is_ready() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        c.submit(&room, "m1", "one", "relay", None, false, T0, busy);
        let r = c.submit(
            &room,
            "m2",
            "two",
            "relay",
            None,
            false,
            T0,
            |_, _, _, _| panic!("must not jump the queue"),
        );
        assert_eq!(r, Submit::Queued);
        let order = RefCell::new(Vec::new());
        c.drain_queue(T0 + 1, |_, body, _, _, _| {
            order.borrow_mut().push(body.to_string());
            Delivery::Delivered
        });
        assert_eq!(*order.borrow(), ["one\n\ntwo"]);
    }

    #[test]
    fn drain_stops_a_room_at_the_first_busy_entry_and_keeps_order() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        for (i, b) in ["a", "b", "c"].iter().enumerate() {
            let dev = format!("d_phone{i}");
            c.submit(
                &room,
                &format!("m{i}"),
                b,
                "relay",
                Some(&dev),
                false,
                T0,
                busy,
            );
        }
        let n = RefCell::new(0);
        c.drain_queue(T0 + 1, |_, _, _, _, _| {
            *n.borrow_mut() += 1;
            if *n.borrow() == 1 {
                Delivery::Delivered
            } else {
                Delivery::Busy(None)
            }
        });
        assert_eq!(*n.borrow(), 2);
        assert_eq!(c.queued_ids(&room), ["m1", "m2"]);
    }

    #[test]
    fn cancel_removes_a_queued_message_with_a_note_and_refuses_a_delivered_one() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        c.submit(&room, "m1", "one", "relay", None, false, T0, busy);
        c.submit(&room, "m2", "two", "relay", None, false, T0, busy);
        assert!(c.cancel(&room, "m1"));
        assert_eq!(systems(&c, &room), ["Not delivered: cancelled."]);
        assert_eq!(c.queued_ids(&room), ["m2"]);
        let got = RefCell::new(Vec::new());
        drain_with(&c, T0 + 1, Delivery::Delivered, &got);
        assert_eq!(*got.borrow(), ["two"]);
        assert!(!c.cancel(&room, "m2"));
        assert_eq!(systems(&c, &room).len(), 1);
    }

    #[test]
    fn the_twenty_first_message_is_full() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        for i in 0..QUEUE_CAP {
            let r = c.submit(&room, &format!("m{i}"), "x", "relay", None, false, T0, busy);
            assert_eq!(r, Submit::Queued);
        }
        let r = c.submit(&room, "m_over", "x", "relay", None, false, T0, busy);
        assert_eq!(r, Submit::Full);
        c.note_undelivered(&room, "m1", &r);
        assert_eq!(
            systems(&c, &room),
            ["Not delivered: the queue for this session is full (20 messages)."]
        );
        assert_eq!(c.queued_ids(&room).len(), QUEUE_CAP);
    }

    #[test]
    fn an_entry_past_its_ttl_is_dropped_with_a_note_and_the_next_one_goes() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        c.submit(&room, "m1", "old", "relay", None, false, T0, busy);
        c.submit(
            &room,
            "m2",
            "new",
            "relay",
            None,
            false,
            T0 + DEFAULT_TTL_MS - 1,
            busy,
        );
        let got = RefCell::new(Vec::new());
        drain_with(&c, T0 + DEFAULT_TTL_MS, Delivery::Delivered, &got);
        assert_eq!(*got.borrow(), ["new"]);
        assert_eq!(
            systems(&c, &room),
            ["Not delivered: expired after 30 minutes in the queue."]
        );
    }

    #[test]
    fn a_gone_target_reports_its_reason_at_submit_and_at_drain() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        let r = c.submit(&room, "m1", "x", "relay", None, false, T0, |_, _, _, _| {
            Delivery::Gone("the session has exited".into())
        });
        assert_eq!(r, Submit::Gone("the session has exited".into()));
        c.note_undelivered(&room, "m1", &r);
        c.submit(&room, "m2", "y", "relay", None, false, T0, busy);
        c.drain_queue(T0 + 1, |_, _, _, _, _| {
            Delivery::Gone("the session no longer exists".into())
        });
        assert_eq!(
            systems(&c, &room),
            [
                "Not delivered: the session has exited.",
                "Not delivered: the session no longer exists."
            ]
        );
        assert!(c.queued_ids(&room).is_empty());
    }

    #[test]
    fn rooms_queue_independently() {
        let (_t, c) = chat();
        let a = c.session_room("s_abcdefghij");
        let b = c.manager_room();
        c.submit(&a, "m1", "x", "relay", None, false, T0, busy);
        let r = c.submit(&b, "m2", "y", "relay", None, false, T0, |_, _, _, _| {
            Delivery::Delivered
        });
        assert_eq!(r, Submit::Delivered);
        assert_eq!(c.queued_ids(&a), ["m1"]);
    }

    const MID: &str = "m_0190a1b2c3d4aabbccddeeff00112233";

    fn mention_spec(c: &Chat, id: &str) -> super::super::mention::Spec {
        use super::super::mention::{Spec, Via};
        Spec {
            id: id.into(),
            origin: c.session_room("s_originaaaa"),
            origin_session: None,
            target: c.session_room("s_abcdefghij"),
            title: "build-fix".into(),
            hop: 1,
            question: "q".into(),
            via: Via::Local {
                client: "desktop".into(),
            },
            confirmed_risky: false,
        }
    }

    fn mention_states(c: &Chat) -> Vec<tmgr_ipc::chat::MentionState> {
        c.history(&c.session_room("s_originaaaa"), None, None)
            .unwrap()
            .into_iter()
            .filter_map(|m| m.mention.map(|r| r.state))
            .collect()
    }

    #[test]
    fn a_mention_entry_expires_after_its_own_ttl_not_the_default_one() {
        use tmgr_ipc::chat::MentionState;
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        c.mention_open(mention_spec(&c, MID));
        let item = |_: &Item| Delivery::Busy(None);
        assert_eq!(
            c.submit_with(
                &room,
                MID,
                "q",
                "desktop",
                None,
                false,
                T0,
                120_000,
                item,
                |_, _| {}
            ),
            Submit::Queued
        );
        c.drain_queue_with(T0 + 119_999, item, |_, _| {});
        assert_eq!(c.queued_ids(&room), [MID]);
        c.drain_queue_with(T0 + 120_000, item, |_, _| {});
        assert!(c.queued_ids(&room).is_empty());
        assert_eq!(mention_states(&c), [MentionState::Busy]);
        assert!(!c.is_mention_open(MID));
    }

    #[test]
    fn cancelling_a_queued_mention_ends_it_as_cancelled() {
        use tmgr_ipc::chat::MentionState;
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        c.mention_open(mention_spec(&c, MID));
        c.submit_with(
            &room,
            MID,
            "q",
            "desktop",
            None,
            false,
            T0,
            120_000,
            |_| Delivery::Busy(None),
            |_, _| {},
        );
        assert!(c.cancel(&room, MID));
        assert_eq!(mention_states(&c), [MentionState::Cancelled]);
    }

    #[test]
    fn the_delivered_callback_gets_the_typed_text_and_starts_the_mention_clock() {
        use tmgr_ipc::chat::MentionState;
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        c.mention_open(mention_spec(&c, MID));
        c.submit_with(
            &room,
            MID,
            "q",
            "desktop",
            None,
            false,
            T0,
            120_000,
            |_| Delivery::Busy(None),
            |_, _| {},
        );
        let typed = RefCell::new(None);
        c.drain_queue_with(
            T0 + 1,
            |_| Delivery::Typed("wrapped q".into()),
            |it, t| {
                *typed.borrow_mut() = Some((it.id.to_string(), t.to_string()));
                c.mention_delivered(it.id, t, T0 + 1);
            },
        );
        assert_eq!(
            *typed.borrow(),
            Some((MID.to_string(), "wrapped q".to_string()))
        );
        assert_eq!(mention_states(&c), [MentionState::Sent]);
        assert!(c.take_sent("s_abcdefghij", "wrapped q"));
    }

    #[test]
    fn a_refused_or_gone_queued_mention_ends_with_the_reason() {
        use tmgr_ipc::chat::MentionState;
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        c.mention_open(mention_spec(&c, MID));
        c.submit_with(
            &room,
            MID,
            "q",
            "desktop",
            None,
            false,
            T0,
            120_000,
            |_| Delivery::Busy(None),
            |_, _| {},
        );
        c.drain_queue_with(
            T0 + 1,
            |_| Delivery::Gone("the session has exited".into()),
            |_, _| {},
        );
        assert_eq!(mention_states(&c), [MentionState::Offline]);
    }

    #[test]
    fn submit_outcome_notes_point_at_the_message() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        let gone = c.submit(
            &room,
            &mid(1),
            "x",
            "relay",
            None,
            false,
            T0,
            |_, _, _, _| Delivery::Gone("the session has exited".into()),
        );
        c.note_undelivered(&room, &mid(1), &gone);
        let refused = c.submit(
            &room,
            &mid(2),
            "x",
            "relay",
            None,
            false,
            T0,
            |_, _, _, _| Delivery::Refused("Not delivered: nope."),
        );
        c.note_undelivered(&room, &mid(2), &refused);
        for i in 0..QUEUE_CAP {
            c.submit(
                &room,
                &mid(100 + i as u64),
                "x",
                "relay",
                None,
                false,
                T0,
                busy,
            );
        }
        let full = c.submit(&room, &mid(3), "x", "relay", None, false, T0, busy);
        assert_eq!(full, Submit::Full);
        c.note_undelivered(&room, &mid(3), &full);
        assert_eq!(
            note_replies(&c, &room),
            [Some(mid(1)), Some(mid(2)), Some(mid(3))]
        );
    }

    #[test]
    fn drain_and_cancel_notes_point_at_the_message() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        c.submit(&room, &mid(1), "old", "relay", None, false, T0, busy);
        c.submit(
            &room,
            &mid(2),
            "b",
            "relay",
            None,
            false,
            T0 + DEFAULT_TTL_MS,
            busy,
        );
        c.submit(
            &room,
            &mid(3),
            "c",
            "relay",
            None,
            false,
            T0 + DEFAULT_TTL_MS,
            busy,
        );
        c.submit(
            &room,
            &mid(4),
            "d",
            "relay",
            None,
            false,
            T0 + DEFAULT_TTL_MS,
            busy,
        );
        let mut n = 0;
        c.drain_queue(T0 + DEFAULT_TTL_MS, |_, _, _, _, _| {
            n += 1;
            if n == 1 {
                Delivery::Refused("Not delivered: nope.")
            } else {
                Delivery::Gone("the session no longer exists".into())
            }
        });
        c.submit(
            &room,
            &mid(5),
            "e",
            "relay",
            None,
            false,
            T0 + DEFAULT_TTL_MS,
            busy,
        );
        assert!(c.cancel(&room, &mid(5)));
        assert_eq!(
            note_replies(&c, &room),
            [
                Some(mid(1)),
                Some(mid(2)),
                Some(mid(3)),
                Some(mid(4)),
                Some(mid(5))
            ]
        );
    }
    fn rec(c: &Chat) -> std::sync::Arc<std::sync::Mutex<Vec<crate::chat::Receipt>>> {
        c.capture_receipts()
    }

    fn got(
        r: &std::sync::Arc<std::sync::Mutex<Vec<crate::chat::Receipt>>>,
    ) -> Vec<(String, ReceiptState, Option<String>)> {
        r.lock()
            .unwrap()
            .iter()
            .map(|r| (r.id.clone(), r.state, r.reason.clone()))
            .collect()
    }

    fn submit_as(
        c: &Chat,
        room: &RoomRef,
        id: &str,
        client: &str,
        device: Option<&str>,
        d: Delivery,
    ) -> Submit {
        c.submit(room, id, "x", client, device, false, T0, |_, _, _, _| {
            d.clone()
        })
    }

    #[test]
    fn submit_outcomes_map_to_receipts_for_relay_messages_with_a_device() {
        let (_t, c) = chat();
        let r = rec(&c);
        let room = c.session_room("s_abcdefghij");
        let dev = Some("d_phone1");
        submit_as(&c, &room, &mid(1), "relay", dev, Delivery::Delivered);
        submit_as(
            &c,
            &room,
            &mid(2),
            "relay",
            dev,
            Delivery::Typed("y".into()),
        );
        submit_as(
            &c,
            &room,
            &mid(3),
            "relay",
            dev,
            Delivery::Gone("the session has exited".into()),
        );
        submit_as(
            &c,
            &room,
            &mid(4),
            "relay",
            dev,
            Delivery::Refused("Not delivered: nope."),
        );
        submit_as(&c, &room, &mid(5), "relay", dev, Delivery::Busy(None));
        assert_eq!(
            got(&r),
            [
                (mid(1), ReceiptState::Delivered, None),
                (mid(2), ReceiptState::Delivered, None),
                (
                    mid(3),
                    ReceiptState::Failed,
                    Some("the session has exited.".into())
                ),
                (mid(4), ReceiptState::Failed, Some("nope.".into())),
                (mid(5), ReceiptState::Queued, None),
            ]
        );
        assert!(r.lock().unwrap().iter().all(|r| r.device == "d_phone1"));
    }

    #[test]
    fn a_full_queue_sends_a_failed_receipt_with_the_note_text() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        for i in 0..QUEUE_CAP {
            submit_as(
                &c,
                &room,
                &mid(100 + i as u64),
                "cli",
                None,
                Delivery::Busy(None),
            );
        }
        let r = rec(&c);
        submit_as(
            &c,
            &room,
            &mid(1),
            "relay",
            Some("d_phone1"),
            Delivery::Busy(None),
        );
        assert_eq!(
            got(&r),
            [(
                mid(1),
                ReceiptState::Failed,
                Some("the queue for this session is full (20 messages).".into())
            )]
        );
    }

    #[test]
    fn an_internal_error_text_stays_in_the_note_and_never_reaches_the_receipt() {
        let (_t, c) = chat();
        let r = rec(&c);
        let room = c.session_room("s_abcdefghij");
        let gone = submit_as(
            &c,
            &room,
            &mid(1),
            "relay",
            Some("d_phone1"),
            Delivery::Gone("pty write failed: /dev/ttys004: EIO".into()),
        );
        c.note_undelivered(&room, &mid(1), &gone);
        assert_eq!(
            got(&r),
            [(
                mid(1),
                ReceiptState::Failed,
                Some("the message could not be typed into the session.".into())
            )]
        );
        let notes = c.history(&room, None, None).unwrap();
        assert!(notes
            .iter()
            .any(|m| m.body_md.contains("pty write failed: /dev/ttys004: EIO")));
    }

    #[test]
    fn a_queued_message_that_hits_an_internal_error_gets_the_fixed_reason() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        submit_as(
            &c,
            &room,
            &mid(1),
            "relay",
            Some("d_phone1"),
            Delivery::Busy(None),
        );
        let r = rec(&c);
        c.drain_queue(T0 + 1, |_, _, _, _, _| {
            Delivery::Gone("pty write failed: EIO".into())
        });
        assert_eq!(
            got(&r),
            [(
                mid(1),
                ReceiptState::Failed,
                Some("the message could not be typed into the session.".into())
            )]
        );
    }

    #[test]
    fn messages_from_other_clients_or_without_a_device_get_no_receipt() {
        let (_t, c) = chat();
        let r = rec(&c);
        let room = c.session_room("s_abcdefghij");
        submit_as(
            &c,
            &room,
            &mid(1),
            "cli",
            Some("d_phone1"),
            Delivery::Delivered,
        );
        submit_as(&c, &room, &mid(2), "relay", None, Delivery::Delivered);
        submit_as(&c, &room, &mid(3), "desktop", None, Delivery::Busy(None));
        c.drain_queue(T0 + 1, |_, _, _, _, _| Delivery::Delivered);
        assert!(got(&r).is_empty());
    }

    #[test]
    fn a_queued_message_later_delivered_sends_delivered_once() {
        let (_t, c) = chat();
        let r = rec(&c);
        let room = c.session_room("s_abcdefghij");
        submit_as(
            &c,
            &room,
            &mid(1),
            "relay",
            Some("d_phone1"),
            Delivery::Busy(None),
        );
        c.drain_queue(T0 + 1, |_, _, _, _, _| Delivery::Busy(None));
        c.drain_queue(T0 + 2, |_, _, _, _, _| Delivery::Typed("x".into()));
        c.drain_queue(T0 + 3, |_, _, _, _, _| Delivery::Delivered);
        assert_eq!(
            got(&r),
            [
                (mid(1), ReceiptState::Queued, None),
                (mid(1), ReceiptState::Delivered, None)
            ]
        );
    }

    #[test]
    fn a_queued_message_that_ends_refused_gone_expired_or_cancelled_sends_failed() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        for n in 1..=4 {
            let dev = format!("d_phone{n}");
            submit_as(
                &c,
                &room,
                &mid(n),
                "relay",
                Some(&dev),
                Delivery::Busy(None),
            );
        }
        let r = rec(&c);
        let mut n = 0;
        c.drain_queue(T0 + 1, |_, _, _, _, _| {
            n += 1;
            match n {
                1 => Delivery::Refused("Not delivered: nope."),
                2 => Delivery::Gone("the session no longer exists".into()),
                _ => Delivery::Busy(None),
            }
        });
        assert!(c.cancel(&room, &mid(3)));
        c.drain_queue(T0 + DEFAULT_TTL_MS, |_, _, _, _, _| Delivery::Busy(None));
        assert_eq!(
            got(&r),
            [
                (mid(1), ReceiptState::Failed, Some("nope.".into())),
                (
                    mid(2),
                    ReceiptState::Failed,
                    Some("the session no longer exists.".into())
                ),
                (mid(3), ReceiptState::Failed, Some("cancelled.".into())),
                (
                    mid(4),
                    ReceiptState::Failed,
                    Some("expired after 30 minutes in the queue.".into())
                ),
            ]
        );
    }

    #[test]
    fn the_same_id_and_state_is_never_sent_twice() {
        let (_t, c) = chat();
        let r = rec(&c);
        let room = c.session_room("s_abcdefghij");
        for _ in 0..3 {
            submit_as(
                &c,
                &room,
                &mid(1),
                "relay",
                Some("d_phone1"),
                Delivery::Delivered,
            );
        }
        assert_eq!(got(&r).len(), 1);
    }

    #[test]
    fn a_queued_receipt_carries_the_busy_reason_and_later_entries_inherit_it() {
        let (_t, c) = chat();
        let room = c.manager_room();
        let r = rec(&c);
        let dev = Some("d_phone1");
        submit_as(
            &c,
            &room,
            &mid(1),
            "relay",
            dev,
            Delivery::Busy(Some("turn")),
        );
        submit_as(&c, &room, &mid(2), "relay", dev, Delivery::Busy(None));
        assert_eq!(
            got(&r),
            [
                (mid(1), ReceiptState::Queued, Some("turn".into())),
                (mid(2), ReceiptState::Queued, Some("turn".into())),
            ]
        );
        let why = c.queued_why(&room);
        assert_eq!(why.get(&mid(1)).map(String::as_str), Some("turn"));
        assert_eq!(why.get(&mid(2)).map(String::as_str), Some("turn"));
    }

    #[test]
    fn a_drain_that_stays_busy_refreshes_the_reason_of_the_queued_entries() {
        let (_t, c) = chat();
        let room = c.manager_room();
        submit_as(
            &c,
            &room,
            &mid(1),
            "cli",
            None,
            Delivery::Busy(Some("turn")),
        );
        c.drain_queue(T0 + 1, |_, _, _, _, _| Delivery::Busy(Some("dialog")));
        assert_eq!(
            c.queued_why(&room).get(&mid(1)).map(String::as_str),
            Some("dialog")
        );
        c.drain_queue(T0 + 2, |_, _, _, _, _| Delivery::Busy(None));
        assert!(c.queued_why(&room).is_empty());
    }

    fn queue_as(c: &Chat, room: &RoomRef, id: &str, body: &str, client: &str, dev: Option<&str>) {
        c.submit(room, id, body, client, dev, false, T0, busy);
    }

    fn drain_calls(c: &Chat, now: u64) -> Vec<String> {
        let calls = RefCell::new(Vec::new());
        c.drain_queue(now, |_, body, _, _, _| {
            calls.borrow_mut().push(body.to_string());
            Delivery::Delivered
        });
        calls.into_inner()
    }

    #[test]
    fn compatible_queued_entries_are_typed_in_one_call_with_a_receipt_each() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        for n in 1..=5 {
            queue_as(
                &c,
                &room,
                &mid(n),
                &format!("b{n}"),
                "relay",
                Some("d_phone1"),
            );
        }
        let r = rec(&c);
        assert_eq!(drain_calls(&c, T0 + 1), ["b1\n\nb2\n\nb3\n\nb4\n\nb5"]);
        assert!(c.queued_ids(&room).is_empty());
        assert_eq!(
            got(&r),
            (1..=5)
                .map(|n| (mid(n), ReceiptState::Delivered, None))
                .collect::<Vec<_>>()
        );
        assert!((1..=5).all(|n| c.is_pending(&room, &mid(n))));
    }

    #[test]
    fn a_batch_stops_at_a_different_device_client_or_owner_flag() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        queue_as(&c, &room, "m1", "a", "relay", Some("d_phone1"));
        queue_as(&c, &room, "m2", "b", "relay", Some("d_phone1"));
        queue_as(&c, &room, "m3", "c", "relay", Some("d_phone2"));
        queue_as(&c, &room, "m4", "d", "relay", Some("d_phone2"));
        queue_as(&c, &room, "m5", "e", "desktop", Some("d_phone2"));
        queue_as(&c, &room, "m6", "f", "desktop", Some("d_phone2"));
        assert_eq!(drain_calls(&c, T0 + 1), ["a\n\nb", "c\n\nd", "e\n\nf"]);

        c.set_relay_owners(Some(std::sync::Arc::new(Vec::new)));
        c.submit(&room, "m7", "g", "relay", Some("d_p"), true, T0 + 2, busy);
        c.submit(&room, "m8", "h", "relay", Some("d_p"), false, T0 + 2, busy);
        c.submit(&room, "m9", "i", "relay", Some("d_p"), false, T0 + 2, busy);
        assert_eq!(drain_calls(&c, T0 + 3), ["g", "h\n\ni"]);
    }

    #[test]
    fn a_mention_entry_is_never_merged_with_anything() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        c.mention_open(mention_spec(&c, MID));
        queue_as(&c, &room, "m1", "a", "desktop", None);
        queue_as(&c, &room, "m2", "b", "desktop", None);
        c.submit_with(
            &room,
            MID,
            "q",
            "desktop",
            None,
            false,
            T0,
            DEFAULT_TTL_MS,
            |_| Delivery::Busy(None),
            |_, _| {},
        );
        queue_as(&c, &room, "m3", "c", "desktop", None);
        queue_as(&c, &room, "m4", "d", "desktop", None);
        assert_eq!(drain_calls(&c, T0 + 1), ["a\n\nb", "q", "c\n\nd"]);
    }

    #[test]
    fn a_batch_takes_a_full_queue_and_holds_at_sixteen_kib() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        for n in 1..=QUEUE_CAP as u64 {
            queue_as(&c, &room, &mid(n), "x", "desktop", None);
        }
        let sizes: Vec<usize> = drain_calls(&c, T0 + 1)
            .iter()
            .map(|b| b.split("\n\n").count())
            .collect();
        assert_eq!(sizes, [QUEUE_CAP]);

        let long = "y".repeat(7000);
        for n in 1..=3 {
            queue_as(&c, &room, &mid(100 + n), &long, "desktop", None);
        }
        let lens: Vec<usize> = drain_calls(&c, T0 + 2).iter().map(String::len).collect();
        assert_eq!(lens, [14_002, 7000]);
    }

    #[test]
    fn a_busy_target_keeps_the_whole_batch_and_a_later_drain_types_it_once() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        for n in 1..=4 {
            queue_as(
                &c,
                &room,
                &mid(n),
                &format!("b{n}"),
                "relay",
                Some("d_phone1"),
            );
        }
        let r = rec(&c);
        c.drain_queue(T0 + 1, |_, _, _, _, _| Delivery::Busy(Some("settle")));
        assert_eq!(c.queued_ids(&room).len(), 4);
        assert!(got(&r).iter().all(|g| g.1 != ReceiptState::Delivered));
        assert_eq!(drain_calls(&c, T0 + 2), ["b1\n\nb2\n\nb3\n\nb4"]);
        assert!(c.queued_ids(&room).is_empty());
    }

    #[test]
    fn a_refused_batch_fails_every_entry() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        for n in 1..=3 {
            queue_as(&c, &room, &mid(n), "x", "relay", Some("d_phone1"));
        }
        let r = rec(&c);
        c.drain_queue(T0 + 1, |_, _, _, _, _| {
            Delivery::Refused("Not delivered: nope.")
        });
        assert!(c.queued_ids(&room).is_empty());
        assert_eq!(
            got(&r)
                .iter()
                .filter(|g| g.1 == ReceiptState::Failed)
                .count(),
            3
        );
    }

    #[test]
    fn the_remembered_sent_text_is_the_combined_typed_text() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        for (n, b) in [(1, "a"), (2, "b"), (3, "c")] {
            queue_as(&c, &room, &mid(n), b, "relay", Some("d_phone1"));
        }
        let seen = RefCell::new(String::new());
        c.drain_queue(T0 + 1, |_, body, _, _, _| {
            *seen.borrow_mut() = body.to_string();
            Delivery::Typed(format!("[wrapped]\n\n{body}"))
        });
        assert_eq!(*seen.borrow(), "a\n\nb\n\nc");
        assert!(!c.take_sent("s_abcdefghij", "a"));
        assert!(c.take_sent("s_abcdefghij", "[wrapped]\n\na\n\nb\n\nc\n"));
        assert!(!c.take_sent("s_abcdefghij", "[wrapped]\n\na\n\nb\n\nc\n"));
    }
}
