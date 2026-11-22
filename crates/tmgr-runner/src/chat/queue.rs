use super::{lock, Chat, Delivery};
use std::collections::{HashMap, VecDeque};
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
    enq_ms: u64,
    ttl_ms: u64,
}

#[derive(Default)]
pub(super) struct Queue {
    rooms: HashMap<RoomRef, VecDeque<Entry>>,
}

impl Chat {
    pub fn submit(
        &self,
        room: &RoomRef,
        id: &str,
        body: &str,
        client: &str,
        device: Option<&str>,
        now_ms: u64,
        mut deliver: impl FnMut(&str, &str, Option<&str>) -> Delivery,
    ) -> Submit {
        let mut q = lock(&self.queue);
        let backlog = q.rooms.get(room).map_or(0, VecDeque::len);
        if backlog == 0 {
            match deliver(body, client, device) {
                Delivery::Delivered => {
                    self.remember_delivered(room, body);
                    return Submit::Delivered;
                }
                Delivery::Typed(typed) => {
                    self.remember_delivered(room, &typed);
                    return Submit::Delivered;
                }
                Delivery::Gone(r) => return Submit::Gone(r),
                Delivery::Refused(why) => return Submit::Refused(why),
                Delivery::Busy => {}
            }
        }
        if backlog >= QUEUE_CAP {
            return Submit::Full;
        }
        q.rooms.entry(room.clone()).or_default().push_back(Entry {
            id: id.into(),
            body_md: body.into(),
            client: client.into(),
            device: device.map(String::from),
            enq_ms: now_ms,
            ttl_ms: DEFAULT_TTL_MS,
        });
        Submit::Queued
    }

    pub fn drain_queue(
        &self,
        now_ms: u64,
        mut deliver: impl FnMut(&RoomRef, &str, &str, Option<&str>) -> Delivery,
    ) {
        let mut notes: Vec<(RoomRef, String)> = Vec::new();
        {
            let mut q = lock(&self.queue);
            for (room, entries) in q.rooms.iter_mut() {
                while let Some(front) = entries.front() {
                    if now_ms.saturating_sub(front.enq_ms) >= front.ttl_ms {
                        let mins = front.ttl_ms / 60_000;
                        notes.push((
                            room.clone(),
                            format!("Not delivered: expired after {mins} minutes in the queue."),
                        ));
                        entries.pop_front();
                        continue;
                    }
                    match deliver(room, &front.body_md, &front.client, front.device.as_deref()) {
                        Delivery::Delivered => {
                            self.remember_delivered(room, &front.body_md);
                            entries.pop_front();
                        }
                        Delivery::Typed(typed) => {
                            self.remember_delivered(room, &typed);
                            entries.pop_front();
                        }
                        Delivery::Refused(why) => {
                            notes.push((room.clone(), why.to_string()));
                            entries.pop_front();
                        }
                        Delivery::Busy => break,
                        Delivery::Gone(r) => {
                            notes.push((room.clone(), format!("Not delivered: {r}.")));
                            entries.pop_front();
                        }
                    }
                }
            }
            q.rooms.retain(|_, e| !e.is_empty());
        }
        for (room, text) in notes {
            self.note(&room, &text);
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
            entries.remove(pos);
            if entries.is_empty() {
                q.rooms.remove(room);
            }
            true
        };
        if removed {
            self.note(room, "Not delivered: cancelled.");
        }
        removed
    }

    pub fn note_undelivered(&self, room: &RoomRef, outcome: &Submit) {
        match outcome {
            Submit::Gone(r) => self.note(room, &format!("Not delivered: {r}.")),
            Submit::Refused(why) => self.note(room, why),
            Submit::Full => self.note(
                room,
                &format!(
                    "Not delivered: the queue for this session is full ({QUEUE_CAP} messages)."
                ),
            ),
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
    use std::cell::RefCell;

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

    fn busy(_: &str, _: &str, _: Option<&str>) -> Delivery {
        Delivery::Busy
    }

    fn drain_with(c: &Chat, now: u64, d: Delivery, got: &RefCell<Vec<String>>) {
        c.drain_queue(now, |_, body, _, _| {
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
        c.submit(&room, "m1", "hi", "relay", Some("d_phone1"), T0, busy);
        let seen = RefCell::new(None);
        c.drain_queue(T0 + 1, |_, _, client, device| {
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
    fn a_policy_refusal_at_drain_time_notes_its_text_and_drops_the_entry() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        c.submit(&room, "m1", "hi", "relay", None, T0, busy);
        c.drain_queue(T0 + 1, |_, _, _, _| {
            Delivery::Refused("Not delivered: nope.")
        });
        assert!(c.queued_ids(&room).is_empty());
        assert_eq!(systems(&c, &room), ["Not delivered: nope."]);
    }

    #[test]
    fn a_ready_target_delivers_at_once_and_remembers_the_text() {
        let (_t, c) = chat();
        let room = c.session_room("s_abcdefghij");
        let r = c.submit(&room, "m1", "hi", "relay", None, T0, |_, _, _| {
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
            c.submit(&room, "m1", "hi", "relay", None, T0, busy),
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
        c.submit(&room, "m1", "hi", "desktop", None, T0, busy);
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
        c.submit(&room, "m1", "one", "relay", None, T0, busy);
        let r = c.submit(&room, "m2", "two", "relay", None, T0, |_, _, _| {
            panic!("must not jump the queue")
        });
        assert_eq!(r, Submit::Queued);
        let order = RefCell::new(Vec::new());
        c.drain_queue(T0 + 1, |_, body, _, _| {
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
            c.submit(&room, &format!("m{i}"), b, "relay", None, T0, busy);
        }
        let n = RefCell::new(0);
        c.drain_queue(T0 + 1, |_, _, _, _| {
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
        c.submit(&room, "m1", "one", "relay", None, T0, busy);
        c.submit(&room, "m2", "two", "relay", None, T0, busy);
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
            let r = c.submit(&room, &format!("m{i}"), "x", "relay", None, T0, busy);
            assert_eq!(r, Submit::Queued);
        }
        let r = c.submit(&room, "m_over", "x", "relay", None, T0, busy);
        assert_eq!(r, Submit::Full);
        c.note_undelivered(&room, &r);
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
        c.submit(&room, "m1", "old", "relay", None, T0, busy);
        c.submit(
            &room,
            "m2",
            "new",
            "relay",
            None,
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
        let r = c.submit(&room, "m1", "x", "relay", None, T0, |_, _, _| {
            Delivery::Gone("the session has exited".into())
        });
        assert_eq!(r, Submit::Gone("the session has exited".into()));
        c.note_undelivered(&room, &r);
        c.submit(&room, "m2", "y", "relay", None, T0, busy);
        c.drain_queue(T0 + 1, |_, _, _, _| {
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
        c.submit(&a, "m1", "x", "relay", None, T0, busy);
        let r = c.submit(&b, "m2", "y", "relay", None, T0, |_, _, _| {
            Delivery::Delivered
        });
        assert_eq!(r, Submit::Delivered);
        assert_eq!(c.queued_ids(&a), ["m1"]);
    }
}
