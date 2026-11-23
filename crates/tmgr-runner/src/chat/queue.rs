use super::mention::QueueEnd;
use super::{lock, Chat, Delivery, NOT_DELIVERED};
use std::collections::{HashMap, VecDeque};
use tmgr_chat_relay::engine::ReceiptState;
use tmgr_ipc::chat::RoomRef;

pub const QUEUE_CAP: usize = 20;
pub const DEFAULT_TTL_MS: u64 = 30 * 60 * 1000;

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
}

#[derive(Default)]
pub(super) struct Queue {
    rooms: HashMap<RoomRef, VecDeque<Entry>>,
}

struct Note {
    room: RoomRef,
    text: String,
    id: String,
    client: String,
    device: Option<String>,
}

impl Note {
    fn of(room: &RoomRef, e: &Entry, text: String) -> Self {
        Self {
            room: room.clone(),
            text,
            id: e.id.clone(),
            client: e.client.clone(),
            device: e.device.clone(),
        }
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
        mut deliver: impl FnMut(&Item) -> Delivery,
        mut on_delivered: impl FnMut(&Item, &str),
    ) -> Submit {
        let mut q = lock(&self.queue);
        let backlog = q.rooms.get(room).map_or(0, VecDeque::len);
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
                    self.receipt(client, device, id, ReceiptState::Delivered, None);
                    return Submit::Delivered;
                }
                Delivery::Typed(typed) => {
                    self.remember_delivered(room, &typed);
                    on_delivered(&item, &typed);
                    self.receipt(client, device, id, ReceiptState::Delivered, None);
                    return Submit::Delivered;
                }
                Delivery::Gone(r) => {
                    self.receipt(
                        client,
                        device,
                        id,
                        ReceiptState::Failed,
                        Some(&format!("{r}.")),
                    );
                    return Submit::Gone(r);
                }
                Delivery::Refused(why) => {
                    self.receipt(client, device, id, ReceiptState::Failed, Some(why));
                    return Submit::Refused(why);
                }
                Delivery::Busy => {}
            }
        }
        if backlog >= QUEUE_CAP {
            self.receipt(
                client,
                device,
                id,
                ReceiptState::Failed,
                Some(&full_reason()),
            );
            return Submit::Full;
        }
        q.rooms.entry(room.clone()).or_default().push_back(Entry {
            id: id.into(),
            body_md: body.into(),
            client: client.into(),
            device: device.map(String::from),
            owner_device,
            enq_ms: now_ms,
            ttl_ms,
        });
        self.receipt(client, device, id, ReceiptState::Queued, None);
        Submit::Queued
    }

    fn receipt_for(&self, it: &Item, state: ReceiptState) {
        self.receipt(it.client, it.device, it.id, state, None);
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
        {
            let mut q = lock(&self.queue);
            for (room, entries) in q.rooms.iter_mut() {
                while let Some(front) = entries.front() {
                    if now_ms.saturating_sub(front.enq_ms) >= front.ttl_ms {
                        let mins = front.ttl_ms / 60_000;
                        notes.push(Note::of(
                            room,
                            front,
                            format!("Not delivered: expired after {mins} minutes in the queue."),
                        ));
                        ends.push((front.id.clone(), QueueEnd::Expired));
                        entries.pop_front();
                        continue;
                    }
                    let item = Item {
                        room,
                        id: &front.id,
                        body: &front.body_md,
                        client: &front.client,
                        device: front.device.as_deref(),
                        owner_device: front.owner_device
                            && now_ms.saturating_sub(front.enq_ms) < super::FRESH_PAST_MS
                            && front.device.as_ref().is_some_and(|d| owners.contains(d)),
                    };
                    match deliver(&item) {
                        Delivery::Delivered => {
                            self.remember_delivered(room, item.body);
                            on_delivered(&item, item.body);
                            self.receipt_for(&item, ReceiptState::Delivered);
                            entries.pop_front();
                        }
                        Delivery::Typed(typed) => {
                            self.remember_delivered(room, &typed);
                            on_delivered(&item, &typed);
                            self.receipt_for(&item, ReceiptState::Delivered);
                            entries.pop_front();
                        }
                        Delivery::Refused(why) => {
                            notes.push(Note::of(room, front, why.to_string()));
                            ends.push((front.id.clone(), QueueEnd::Refused(why.to_string())));
                            entries.pop_front();
                        }
                        Delivery::Busy => break,
                        Delivery::Gone(r) => {
                            notes.push(Note::of(room, front, format!("Not delivered: {r}.")));
                            ends.push((front.id.clone(), QueueEnd::Gone(r)));
                            entries.pop_front();
                        }
                    }
                }
            }
            q.rooms.retain(|_, e| !e.is_empty());
        }
        for n in notes {
            self.note_for(&n.room, &n.text, &n.id);
            self.receipt(
                &n.client,
                n.device.as_deref(),
                &n.id,
                ReceiptState::Failed,
                Some(&n.text),
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
        Delivery::Busy
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
        assert_eq!(*seen.borrow(), [false, false, false]);
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
        drain_with(&c, T0 + 1, Delivery::Busy, &got);
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
        assert_eq!(*order.borrow(), ["one", "two"]);
    }

    #[test]
    fn drain_stops_a_room_at_the_first_busy_entry_and_keeps_order() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        for (i, b) in ["a", "b", "c"].iter().enumerate() {
            c.submit(&room, &format!("m{i}"), b, "relay", None, false, T0, busy);
        }
        let n = RefCell::new(0);
        c.drain_queue(T0 + 1, |_, _, _, _, _| {
            *n.borrow_mut() += 1;
            if *n.borrow() == 1 {
                Delivery::Delivered
            } else {
                Delivery::Busy
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
        let item = |_: &Item| Delivery::Busy;
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
            |_| Delivery::Busy,
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
            |_| Delivery::Busy,
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
            |_| Delivery::Busy,
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
        submit_as(&c, &room, &mid(5), "relay", dev, Delivery::Busy);
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
            submit_as(&c, &room, &mid(100 + i as u64), "cli", None, Delivery::Busy);
        }
        let r = rec(&c);
        submit_as(
            &c,
            &room,
            &mid(1),
            "relay",
            Some("d_phone1"),
            Delivery::Busy,
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
        submit_as(&c, &room, &mid(3), "desktop", None, Delivery::Busy);
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
            Delivery::Busy,
        );
        c.drain_queue(T0 + 1, |_, _, _, _, _| Delivery::Busy);
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
        let dev = Some("d_phone1");
        for n in 1..=4 {
            submit_as(&c, &room, &mid(n), "relay", dev, Delivery::Busy);
        }
        let r = rec(&c);
        let mut n = 0;
        c.drain_queue(T0 + 1, |_, _, _, _, _| {
            n += 1;
            match n {
                1 => Delivery::Refused("Not delivered: nope."),
                2 => Delivery::Gone("the session no longer exists".into()),
                _ => Delivery::Busy,
            }
        });
        assert!(c.cancel(&room, &mid(3)));
        c.drain_queue(T0 + DEFAULT_TTL_MS, |_, _, _, _, _| Delivery::Busy);
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
}
