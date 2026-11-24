use super::*;

const STEP: u64 = 5_000;

struct Rig {
    fake: Fake,
    sender_clock: Clock,
    runner_clock: Clock,
    desk: Engine,
    runner: Engine,
}

fn rig() -> Rig {
    let fake = Fake::default();
    let sender_clock = Clock::new(T0);
    let runner_clock = Clock::new(T0);
    let desk = engine_with(&fake, &sender_clock, "Desktop A");
    let runner = engine_with(&fake, &runner_clock, "Runner")
        .with_kind("runner")
        .with_room_scope(RoomScope::Local);
    runner
        .join_with_code(&desk.open_join_code().unwrap())
        .unwrap();
    desk.poll_once().unwrap();
    desk.decide_join(&my_id(&runner), true).unwrap();
    runner.poll_once().unwrap();
    desk.poll_once().unwrap();
    runner.poll_once().unwrap();
    Rig {
        fake,
        sender_clock,
        runner_clock,
        desk,
        runner,
    }
}

fn chat_in(machine: &str, text: &str) -> ChatMessage {
    let mut m = text_msg(new_message_id(), text);
    m.room = RoomRef::manager(machine);
    m
}

fn chat(text: &str) -> ChatMessage {
    chat_in("mx_local", text)
}

impl Rig {
    fn send(&self, m: &ChatMessage) -> Result<(), String> {
        self.desk.send_chat(m, true)
    }

    fn runner_queue(&self) -> usize {
        self.fake.queue_len(&my_id(&self.runner))
    }

    fn seq_in(&self) -> Option<u64> {
        self.runner
            .lock()
            .st
            .seq_in
            .get(&my_id(&self.desk))
            .copied()
    }

    fn prime(&self) {
        self.send(&chat("m0")).unwrap();
        assert_eq!(self.runner.poll_once().unwrap().len(), 1);
        assert_eq!(history(&self.runner), ["m0"]);
        self.settle();
        assert_eq!(self.runner_queue(), 0);
    }

    fn now(&self) -> u64 {
        self.runner_clock.0.load(Ordering::SeqCst)
    }

    fn settle(&self) {
        let t = self.now() + 31_000;
        self.runner_clock.set(t);
        self.sender_clock.set(t);
        self.runner.poll_once().unwrap();
    }

    fn poll_until(&self, want: usize) -> (u64, usize) {
        let start = self.runner_clock.0.load(Ordering::SeqCst);
        for polls in 1..=40 {
            let _ = self.runner.poll_once();
            if history(&self.runner).len() >= want {
                return (self.runner_clock.0.load(Ordering::SeqCst) - start, polls);
            }
            self.runner_clock
                .set(self.runner_clock.0.load(Ordering::SeqCst) + STEP);
        }
        panic!("never delivered: {:?}", history(&self.runner));
    }
}

#[test]
fn a_chat_reaches_the_runner_sink_on_the_next_poll() {
    let r = rig();
    r.runner.poll_once().unwrap();
    let polled_at = r.now();
    r.runner_clock.set(polled_at + STEP - 1);
    r.sender_clock.set(polled_at + STEP - 1);
    r.send(&chat("hello")).unwrap();
    let (waited, polls) = r.poll_until(1);
    assert_eq!(
        (waited, polls),
        (0, 1),
        "delivered by the first poll after the send; worst case is one {STEP} ms cadence"
    );
}

fn first_unverifiable_second_delayed(break_first: impl FnOnce(&Rig, &ChatMessage)) {
    let r = rig();
    r.prime();
    let seq_before = r.seq_in().unwrap();
    let m1 = chat("m1");
    break_first(&r, &m1);
    r.sender_clock.set(r.now());
    r.send(&chat("m2")).unwrap();
    assert_eq!(r.runner_queue(), 2);

    assert!(r.runner.poll_once().unwrap().is_empty());
    assert_eq!(
        r.runner_queue(),
        2,
        "m1 stays on the server until the cursor past it is stored"
    );
    assert_eq!(r.seq_in(), Some(seq_before), "m1 did not advance seq_in");

    let (delay, polls) = r.poll_until(2);
    assert_eq!(
        (delay, polls),
        (HOLD_MS, 4),
        "m2 waited HOLD_MS after the first poll that saw it ({} polls at {STEP} ms)",
        polls
    );
    assert_eq!(HOLD_MS, 15_000);
    assert_eq!(history(&r.runner), ["m0", "m2"]);
    assert!(alert_kinds(&r.runner).is_empty());
    let gaps = r.runner.trust_state().unwrap().seq_gaps;
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].got, gaps[0].expected + 1);
}

#[test]
fn message_with_skewed_sent_at_is_reported_consumes_its_seq_and_does_not_delay_the_next() {
    let r = rig();
    r.prime();
    let seq_before = r.seq_in().unwrap();
    r.sender_clock.set(r.now() + 32 * 24 * 3_600_000);
    r.send(&chat("m1")).unwrap();
    r.sender_clock.set(r.now());
    r.send(&chat("m2")).unwrap();
    assert_eq!(r.runner.poll_once().unwrap().len(), 1);
    assert_eq!(history(&r.runner), ["m0", "m2"]);
    assert_eq!(r.seq_in(), Some(seq_before + 2));
    assert_eq!(r.runner.s.drops(), ["outside the time window"]);
    r.settle();
    assert_eq!(r.runner_queue(), 0);
    assert!(r.runner.trust_state().unwrap().seq_gaps.is_empty());
    assert_eq!(
        r.runner.s.drops().len(),
        1,
        "a message is reported once as the cursor passes it"
    );
}

#[test]
fn unopenable_first_message_with_corrupted_body_loses_it_and_delays_the_next_by_hold() {
    first_unverifiable_second_delayed(|r, m1| {
        r.send(m1).unwrap();
        let mut s = r.fake.0.lock().unwrap();
        s.msgs.iter_mut().find(|m| m.msg_id == m1.id).unwrap().body = "garbage".into();
    });
}

#[test]
fn unverifiable_rejections_are_reported_once_with_a_fixed_reason() {
    let r = rig();
    r.prime();
    let m1 = chat("m1");
    r.send(&m1).unwrap();
    {
        let mut s = r.fake.0.lock().unwrap();
        s.msgs.iter_mut().find(|m| m.msg_id == m1.id).unwrap().body = "garbage".into();
    }
    r.send(&chat("m2")).unwrap();
    for _ in 0..3 {
        let _ = r.runner.poll_once();
        r.runner_clock.set(r.now() + STEP);
    }
    assert_eq!(r.runner.s.drops(), ["undecryptable or malformed"]);
}

fn first_dropped_second_immediate(first: ChatMessage, reason: &str) {
    let r = rig();
    r.prime();
    let seq_before = r.seq_in().unwrap();
    r.send(&first).unwrap();
    r.send(&chat("m2")).unwrap();
    let got = r.runner.poll_once().unwrap();
    assert_eq!(history(&r.runner), ["m0", "m2"]);
    assert_eq!(got.len(), 1);
    assert_eq!(r.runner.s.drops(), [reason]);
    assert_eq!(
        r.seq_in(),
        Some(seq_before + 2),
        "seq_in advanced past the dropped message"
    );
    r.settle();
    assert_eq!(r.runner_queue(), 0, "dropped message is acked");
    assert!(r.runner.trust_state().unwrap().seq_gaps.is_empty());
}

#[test]
fn chat_rejected_by_authorized_author_is_acked_and_does_not_delay_the_next() {
    let mut m1 = chat("m1");
    m1.author.kind = AuthorKind::Manager;
    first_dropped_second_immediate(m1, "unauthorized author");
}

#[test]
fn chat_for_a_room_the_runner_does_not_own_is_acked_and_does_not_delay_the_next() {
    first_dropped_second_immediate(chat_in("mx_other", "m1"), "room not owned by this machine");
}

#[test]
fn chat_from_a_sender_without_a_verified_kind_is_acked_and_does_not_delay_the_next() {
    let r = rig();
    r.prime();
    let seq_before = r.seq_in().unwrap();
    let kinds = r.runner.lock().st.kinds.clone();
    drop_pin(&r.runner, &my_id(&r.desk));
    r.send(&chat("m1")).unwrap();
    assert!(r.runner.poll_once().unwrap().is_empty());
    assert_eq!(r.runner.s.drops(), ["unauthorized author"]);
    r.runner.lock().st.kinds = kinds;
    r.send(&chat("m2")).unwrap();
    assert_eq!(r.runner.poll_once().unwrap().len(), 1);
    assert_eq!(history(&r.runner), ["m0", "m2"]);
    assert_eq!(r.seq_in(), Some(seq_before + 2));
    assert_eq!(r.runner_queue(), 0);
}

#[test]
fn a_post_stored_by_the_server_but_seen_as_an_error_makes_the_next_message_reuse_the_seq_without_loss(
) {
    let r = rig();
    r.prime();
    let seq_before = r.seq_in().unwrap();
    r.fake.0.lock().unwrap().fail_after_store = true;
    assert!(r.send(&chat("m1")).is_err());
    r.fake.0.lock().unwrap().fail_after_store = false;
    r.send(&chat("m2")).unwrap();
    assert_eq!(r.runner_queue(), 2);
    assert_eq!(r.runner.poll_once().unwrap().len(), 2);
    assert_eq!(history(&r.runner), ["m0", "m1", "m2"]);
    assert_eq!(
        r.seq_in(),
        Some(seq_before + 1),
        "both messages carried the same seq"
    );
    assert!(r.runner.trust_state().unwrap().seq_gaps.is_empty());
}

#[test]
fn retrying_a_stored_but_errored_post_with_the_same_id_is_deduped_by_the_server() {
    let r = rig();
    r.prime();
    let seq_before = r.seq_in().unwrap();
    let m1 = chat("m1");
    r.fake.0.lock().unwrap().fail_after_store = true;
    assert!(r.send(&m1).is_err());
    r.fake.0.lock().unwrap().fail_after_store = false;
    r.send(&m1).unwrap();
    r.send(&chat("m2")).unwrap();
    assert_eq!(r.runner_queue(), 2, "the retry was a server-side duplicate");
    assert_eq!(r.runner.poll_once().unwrap().len(), 2);
    assert_eq!(history(&r.runner), ["m0", "m1", "m2"]);
    assert_eq!(r.seq_in(), Some(seq_before + 2));
    assert!(r.runner.trust_state().unwrap().seq_gaps.is_empty());
}

#[test]
fn a_failing_sink_holds_the_cursor_retries_every_poll_and_blocks_later_messages_from_that_sender() {
    let r = rig();
    r.prime();
    let seq_before = r.seq_in().unwrap();
    let cursor_before = r.runner.lock().st.cursor;
    r.send(&chat("m1")).unwrap();
    r.send(&chat("m2")).unwrap();
    r.runner.s.set_failing(true);
    let base = r.now();
    for i in 1..=3 {
        r.runner_clock.set(base + i * STEP);
        assert_eq!(r.runner.poll_once().unwrap_err(), "sink is down");
        assert_eq!(r.runner_queue(), 2, "poll {i}: nothing acked");
        assert_eq!(r.runner.lock().st.cursor, cursor_before, "poll {i}");
        assert_eq!(r.seq_in(), Some(seq_before), "poll {i}: seq_in rolled back");
    }
    assert_eq!(history(&r.runner), ["m0"]);
    r.runner.s.set_failing(false);
    r.runner_clock.set(base + 4 * STEP);
    assert_eq!(r.runner.poll_once().unwrap().len(), 2);
    assert_eq!(history(&r.runner), ["m0", "m1", "m2"]);
}

fn slow_poll_blocks_send(queued_in_desk: bool) -> usize {
    let r = rig();
    if queued_in_desk {
        let mut m = chat("reply");
        m.author.kind = AuthorKind::Manager;
        r.runner.send_chat(&m, true).unwrap();
        r.sender_clock.set(T0 + WRITE_EVERY_MS);
    }
    r.fake.0.lock().unwrap().slow_ms = 10;
    std::thread::scope(|s| {
        let poll = s.spawn(|| r.desk.poll_once().unwrap());
        while r.fake.0.lock().unwrap().slow_started == 0 {
            std::thread::yield_now();
        }
        r.desk.send_chat(&chat("outbound"), true).unwrap();
        poll.join().unwrap();
    });
    let calls = r.fake.0.lock().unwrap().slow_done_at_post;
    calls.unwrap()
}

#[test]
fn send_waits_for_an_in_flight_poll_and_the_wait_scales_with_its_http_calls() {
    let idle = slow_poll_blocks_send(false);
    let busy = slow_poll_blocks_send(true);
    assert_eq!(
        idle, 2,
        "idle poll (devices + messages = 2 calls) finished before send posted"
    );
    assert_eq!(
        busy, 3,
        "poll with one message (devices + messages + ack = 3 calls) finished before send posted"
    );
}
