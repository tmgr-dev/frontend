use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use super::*;
use crate::chat::{Chat, Delivery};
use tmgr_chat_relay::engine::ReceiptState;
use tmgr_ipc::chat::{new_message_id, Author, MessageKind, RoomRef, CHAT_V};

const MACHINE: &str = "mx_0123456789abcdef";

fn msg(kind: AuthorKind, machine: &str, device: Option<&str>) -> ChatMessage {
    ChatMessage {
        v: CHAT_V,
        id: new_message_id(),
        room: RoomRef::manager(machine),
        author: Author {
            kind,
            id: "owner".into(),
            machine: None,
            device: device.map(str::to_owned),
            agent: None,
        },
        ts_ms: 1_800_000_000_000,
        kind: MessageKind::Text,
        body_md: "hi".into(),
        attachments: Vec::new(),
        reply_to: None,
        decision: None,
        session_meta: None,
        user_turn: false,
        mention: None,
    }
}

fn recording_sink(
    result: impl Fn() -> Result<Ingested, IngestError> + Send + Sync + 'static,
) -> (RelaySink, Arc<Mutex<Vec<Incoming>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let sink = RelaySink::new(
        MACHINE.into(),
        Box::new(move |i, _| {
            log.lock().unwrap().push(i);
            result()
        }),
    );
    (sink, seen)
}

fn stored() -> Result<Ingested, IngestError> {
    Ok(Ingested {
        message: msg(AuthorKind::Owner, MACHINE, None),
        delivered: true,
        duplicate: false,
    })
}

#[test]
fn owner_message_for_this_machine_is_ingested_with_the_sender_device() {
    let (sink, seen) = recording_sink(stored);
    let m = msg(AuthorKind::Owner, MACHINE, None);
    let id = m.id.clone();
    sink.deliver("dev-1", Some("mobile"), m).unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].id, id);
    assert_eq!(seen[0].device.as_deref(), Some("dev-1"));
    assert!(seen[0].owner_device);
}

#[test]
fn owner_rows_from_other_kinds_are_dropped_and_desktop_or_mobile_reach_ingest() {
    let (sink, seen) = recording_sink(stored);
    for kind in [Some("runner"), None, Some("weird")] {
        sink.deliver("dev-1", kind, msg(AuthorKind::Owner, MACHINE, None))
            .unwrap();
    }
    assert!(seen.lock().unwrap().is_empty());
    for kind in ["desktop", "mobile"] {
        sink.deliver("dev-1", Some(kind), msg(AuthorKind::Owner, MACHINE, None))
            .unwrap();
    }
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert!(seen.iter().all(|i| i.owner_device));
}

#[test]
fn non_owner_and_foreign_machine_messages_are_dropped() {
    let (sink, seen) = recording_sink(stored);
    for kind in [AuthorKind::Manager, AuthorKind::Session, AuthorKind::System] {
        sink.deliver("dev-1", Some("mobile"), msg(kind, MACHINE, None))
            .unwrap();
    }
    sink.deliver(
        "dev-1",
        Some("mobile"),
        msg(AuthorKind::Owner, "mx_ffffffffffffffff", None),
    )
    .unwrap();
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn not_found_and_conflict_are_acked_but_invalid_is_dropped_at_once() {
    let (sink, _) = recording_sink(|| Err(IngestError::NotFound("no room".into())));
    sink.deliver("d", Some("mobile"), msg(AuthorKind::Owner, MACHINE, None))
        .unwrap();
    let (sink, _) = recording_sink(|| Err(IngestError::Conflict("differs".into())));
    sink.deliver("d", Some("mobile"), msg(AuthorKind::Owner, MACHINE, None))
        .unwrap();
    let (sink, seen) = recording_sink(|| Err(IngestError::Invalid("bad".into())));
    assert_eq!(
        sink.deliver("d", Some("mobile"), msg(AuthorKind::Owner, MACHINE, None)),
        Ok(())
    );
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[test]
fn storage_errors_are_retried_forever_and_never_dropped() {
    let (sink, seen) = recording_sink(|| Err(IngestError::Storage("disk full".into())));
    let m = msg(AuthorKind::Owner, MACHINE, None);
    for _ in 0..10 {
        assert_eq!(
            sink.deliver("d", Some("mobile"), m.clone()),
            Err("disk full".into())
        );
    }
    assert_eq!(seen.lock().unwrap().len(), 10);
}

type Told = Arc<Mutex<Vec<(String, String)>>>;

fn told_sink(kind_result: IngestError) -> (RelaySink, Told) {
    let (sink, _) = recording_sink(move || Err(kind_result.clone()));
    let told = Arc::new(Mutex::new(Vec::new()));
    let log = told.clone();
    let sink = sink.with_give_up(Box::new(move |device, id| {
        log.lock().unwrap().push((device.to_owned(), id.to_owned()))
    }));
    (sink, told)
}

#[test]
fn giving_up_on_an_invalid_message_from_a_desktop_tells_it_once() {
    let (sink, told) = told_sink(IngestError::Invalid("bad".into()));
    let m = msg(AuthorKind::Owner, MACHINE, None);
    assert_eq!(sink.deliver("dev-9", Some("desktop"), m.clone()), Ok(()));
    assert_eq!(*told.lock().unwrap(), [("dev-9".to_owned(), m.id)]);
}

#[test]
fn an_invalid_message_from_a_phone_is_dropped_without_a_failed_receipt() {
    let (sink, told) = told_sink(IngestError::Invalid("bad".into()));
    let m = msg(AuthorKind::Owner, MACHINE, None);
    assert_eq!(sink.deliver("dev-9", Some("mobile"), m), Ok(()));
    assert!(told.lock().unwrap().is_empty());
}

fn receipts_after(kind: &str, ts_ms: u64) -> (usize, Vec<Out>) {
    let dir = tempfile::tempdir().unwrap();
    let chat = Arc::new(Chat::open(dir.path()));
    let machine = chat.machine().to_owned();
    let (tx, rx) = mpsc::sync_channel::<Out>(8);
    chat.set_receipt_sink(Some(receipt_queue(tx)));
    let c = chat.clone();
    let sink = RelaySink::new(
        machine.clone(),
        Box::new(move |i, _| c.ingest(i, 1_800_000_000_000, |_| false, |_| Delivery::Busy(None))),
    );
    let mut m = msg(AuthorKind::Owner, &machine, None);
    m.ts_ms = ts_ms;
    sink.deliver("dev-1", Some(kind), m).unwrap();
    let out: Vec<Out> = rx.try_iter().collect();
    (out.len(), out)
}

#[test]
fn a_message_from_a_phone_produces_no_receipt_and_never_enters_the_outbound_queue() {
    let (n, _) = receipts_after("mobile", 1_800_000_000_000);
    assert_eq!(n, 0);
    let (n, _) = receipts_after("mobile", 1_000);
    assert_eq!(n, 0);
}

#[test]
fn a_message_from_a_desktop_still_produces_receipts() {
    let (n, _) = receipts_after("desktop", 1_800_000_000_000);
    assert!(n > 0);
}

#[test]
fn redelivery_is_deduped_by_the_chat_store() {
    let dir = tempfile::tempdir().unwrap();
    let chat = Arc::new(Chat::open(dir.path()));
    let machine = chat.machine().to_owned();
    let delivered = Arc::new(AtomicUsize::new(0));
    let (c, n) = (chat.clone(), delivered.clone());
    let sink = RelaySink::new(
        machine.clone(),
        Box::new(move |i, _| {
            let n = n.clone();
            c.ingest(
                i,
                1_800_000_000_000,
                |_| false,
                move |_| {
                    n.fetch_add(1, Ordering::SeqCst);
                    Delivery::Delivered
                },
            )
        }),
    );
    let m = msg(AuthorKind::Owner, &machine, None);
    sink.deliver("dev-1", Some("mobile"), m.clone()).unwrap();
    sink.deliver("dev-1", Some("mobile"), m.clone()).unwrap();
    assert_eq!(delivered.load(Ordering::SeqCst), 1);
    let history = chat.history(&m.room, None, None).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].author.device.as_deref(), Some("dev-1"));
}

struct ScriptedRelay {
    polls: Mutex<Vec<Result<(), String>>>,
    sends: Mutex<Vec<Result<(), String>>>,
    sent: Mutex<Vec<String>>,
    receipts: Mutex<Vec<Result<(), String>>>,
    receipts_sent: Mutex<Vec<(String, ReceiptState)>>,
    gate: Mutex<()>,
}

impl ScriptedRelay {
    fn new(polls: Vec<Result<(), String>>, sends: Vec<Result<(), String>>) -> Self {
        Self {
            polls: Mutex::new(polls),
            sends: Mutex::new(sends),
            sent: Mutex::new(Vec::new()),
            receipts: Mutex::new(Vec::new()),
            receipts_sent: Mutex::new(Vec::new()),
            gate: Mutex::new(()),
        }
    }
}

impl Relay for ScriptedRelay {
    fn poll(&self) -> Result<(), String> {
        self.polls.lock().unwrap().remove(0)
    }

    fn send(&self, m: &ChatMessage) -> Result<(), String> {
        self.sent.lock().unwrap().push(m.id.clone());
        self.sends.lock().unwrap().remove(0)
    }

    fn send_receipt(&self, r: &Receipt) -> Result<(), String> {
        let _engine = self.gate.lock().unwrap();
        self.receipts_sent
            .lock()
            .unwrap()
            .push((r.id.clone(), r.state));
        let mut scripted = self.receipts.lock().unwrap();
        if scripted.is_empty() {
            return Ok(());
        }
        scripted.remove(0)
    }
}

fn receipt(state: ReceiptState) -> Receipt {
    Receipt {
        device: "dev-1".into(),
        id: new_message_id(),
        state,
        reason: None,
    }
}

#[test]
fn a_receipt_is_retried_with_backoff_and_dropped_without_retry_when_permanent() {
    let relay = ScriptedRelay::new(vec![], vec![]);
    *relay.receipts.lock().unwrap() = vec![Err("relay unreachable".into()), Ok(())];
    let sleeps = Mutex::new(Vec::new());
    send_receipt_one(&relay, &receipt(ReceiptState::Queued), &|| false, &|d| {
        sleeps.lock().unwrap().push(d.as_secs())
    });
    assert_eq!(*sleeps.lock().unwrap(), [5]);
    assert_eq!(relay.receipts_sent.lock().unwrap().len(), 2);

    let relay = ScriptedRelay::new(vec![], vec![]);
    *relay.receipts.lock().unwrap() = vec![Err(RECIPIENT_FULL.into())];
    send_receipt_one(
        &relay,
        &receipt(ReceiptState::Delivered),
        &|| false,
        &|_| panic!("must not wait"),
    );
    assert_eq!(relay.receipts_sent.lock().unwrap().len(), 1);

    let relay = ScriptedRelay::new(vec![], vec![]);
    *relay.receipts.lock().unwrap() = vec![Err("relay unreachable".into()); 10];
    send_receipt_one(
        &relay,
        &receipt(ReceiptState::Delivered),
        &|| false,
        &|_| {},
    );
    assert_eq!(
        relay.receipts_sent.lock().unwrap().len(),
        SEND_ATTEMPTS as usize
    );
}

#[test]
fn run_send_carries_chat_and_receipts_in_order_on_one_thread() {
    let (tx, rx) = mpsc::sync_channel::<Out>(8);
    let m = msg(AuthorKind::Owner, MACHINE, None);
    let r = receipt(ReceiptState::Delivered);
    tx.send(Out::Receipt(r.clone())).unwrap();
    tx.send(Out::Chat(Box::new(m.clone()))).unwrap();
    drop(tx);
    let relay = ScriptedRelay::new(vec![], vec![Ok(())]);
    run_send(&relay, &rx, &|| false, &|_| {});
    assert_eq!(
        *relay.receipts_sent.lock().unwrap(),
        [(r.id, ReceiptState::Delivered)]
    );
    assert_eq!(*relay.sent.lock().unwrap(), [m.id]);
}

#[test]
fn a_full_outbound_queue_drops_the_receipt_without_blocking() {
    let (tx, rx) = mpsc::sync_channel::<Out>(1);
    let sink = receipt_queue(tx);
    assert!(sink(receipt(ReceiptState::Queued)));
    assert!(!sink(receipt(ReceiptState::Delivered)));
    assert!(matches!(rx.try_recv(), Ok(Out::Receipt(r)) if r.state == ReceiptState::Queued));
    assert!(rx.try_recv().is_err());
}

#[test]
fn an_ingest_inside_the_engine_lock_queues_the_receipt_and_the_send_thread_sends_it_later() {
    let dir = tempfile::tempdir().unwrap();
    let chat = Arc::new(Chat::open(dir.path()));
    let machine = chat.machine().to_owned();
    let (tx, rx) = mpsc::sync_channel::<Out>(8);
    chat.set_receipt_sink(Some(receipt_queue(tx.clone())));
    let c = chat.clone();
    let sink = RelaySink::new(
        machine.clone(),
        Box::new(move |i, _| c.ingest(i, 1_800_000_000_000, |_| false, |_| Delivery::Delivered)),
    );
    let relay = ScriptedRelay::new(vec![], vec![]);
    let m = msg(AuthorKind::Owner, &machine, None);
    {
        let _engine_lock = relay.gate.lock().unwrap();
        sink.deliver("dev-1", Some("desktop"), m.clone()).unwrap();
        assert!(relay.receipts_sent.lock().unwrap().is_empty());
        assert!(matches!(rx.try_recv(), Ok(Out::Receipt(_))));
        sink.deliver(
            "dev-1",
            Some("desktop"),
            msg(AuthorKind::Owner, &machine, None),
        )
        .unwrap();
    }
    drop(tx);
    chat.set_receipt_sink(None);
    run_send(&relay, &rx, &|| false, &|_| {});
    assert_eq!(relay.receipts_sent.lock().unwrap().len(), 1);
    assert_eq!(
        relay.receipts_sent.lock().unwrap()[0].1,
        ReceiptState::Delivered
    );
}

#[test]
fn poll_honours_retry_after_then_doubles_and_resets_on_success() {
    let busy = |s: u64| Err(format!(r#"{{"error":"busy","retry_after":{s}}}"#));
    let relay = ScriptedRelay::new(
        vec![
            busy(120),
            Err("relay unreachable".into()),
            Err("relay unreachable".into()),
            Ok(()),
            Err("relay unreachable".into()),
        ],
        vec![],
    );
    let sleeps = Mutex::new(Vec::new());
    run_poll(
        &relay,
        &|| relay.polls.lock().unwrap().is_empty(),
        &|| false,
        &|d| sleeps.lock().unwrap().push(d.as_secs()),
    );
    assert_eq!(*sleeps.lock().unwrap(), [120, 10, 20, 1, 5]);
}

#[test]
fn poll_backoff_is_capped() {
    let relay = ScriptedRelay::new(vec![Err("x".into()); 12], vec![]);
    let sleeps = Mutex::new(Vec::new());
    run_poll(
        &relay,
        &|| relay.polls.lock().unwrap().is_empty(),
        &|| false,
        &|d| sleeps.lock().unwrap().push(d.as_secs()),
    );
    assert_eq!(sleeps.lock().unwrap().iter().max(), Some(&300));
    assert!(sleeps.lock().unwrap().iter().all(|s| *s >= 5));
}

#[test]
fn busy_send_waits_for_the_hint_and_retries_the_same_message() {
    let m = msg(AuthorKind::Owner, MACHINE, None);
    let relay = ScriptedRelay::new(
        vec![],
        vec![Err("relay is busy, retry in 30s".into()), Ok(())],
    );
    let sleeps = Mutex::new(Vec::new());
    send_one(&relay, &m, &|| false, &|d| {
        sleeps.lock().unwrap().push(d.as_secs())
    });
    assert_eq!(*sleeps.lock().unwrap(), [30]);
    assert_eq!(*relay.sent.lock().unwrap(), [m.id.clone(), m.id]);
}

#[test]
fn send_gives_up_after_bounded_attempts() {
    let m = msg(AuthorKind::Owner, MACHINE, None);
    let relay = ScriptedRelay::new(vec![], vec![Err("relay unreachable".into()); 10]);
    let sleeps = Mutex::new(Vec::new());
    send_one(&relay, &m, &|| false, &|d| {
        sleeps.lock().unwrap().push(d.as_secs())
    });
    assert_eq!(relay.sent.lock().unwrap().len(), SEND_ATTEMPTS as usize);
    assert_eq!(*sleeps.lock().unwrap(), [5, 10, 20, 40]);
}

#[test]
fn full_recipient_and_untrusted_states_drop_without_retry() {
    for e in [
        RECIPIENT_FULL,
        "there are no other trusted devices to send to",
        "this device is not a trusted member of the chat yet",
        "the target is not a trusted desktop",
    ] {
        let m = msg(AuthorKind::Owner, MACHINE, None);
        let relay = ScriptedRelay::new(vec![], vec![Err(e.into())]);
        send_one(&relay, &m, &|| false, &|_| panic!("must not wait"));
        assert_eq!(relay.sent.lock().unwrap().len(), 1);
    }
}

#[test]
fn forward_skips_relay_origin_and_survives_a_full_queue_and_a_closed_subscription() {
    let (tx_a, rx_a) = mpsc::sync_channel::<ChatPush>(8);
    let (tx_b, rx_b) = mpsc::sync_channel::<ChatPush>(8);
    let subs = Mutex::new(vec![rx_b, rx_a]);
    let subscribed = AtomicUsize::new(0);
    let push = |m: ChatMessage| ChatPush { message: m };
    let local = msg(AuthorKind::Session, MACHINE, None);
    let echoed = msg(AuthorKind::Owner, MACHINE, Some("dev-1"));
    let owner_typed = msg(AuthorKind::Owner, MACHINE, None);
    let second = msg(AuthorKind::Manager, MACHINE, None);
    tx_a.send(push(echoed)).unwrap();
    tx_a.send(push(owner_typed)).unwrap();
    tx_a.send(push(local.clone())).unwrap();
    drop(tx_a);
    tx_b.send(push(second.clone())).unwrap();
    let (queue_tx, queue_rx) = mpsc::sync_channel::<ChatMessage>(1);
    let polled = AtomicUsize::new(0);
    forward(
        &|| {
            subscribed.fetch_add(1, Ordering::SeqCst);
            subs.lock().unwrap().pop().unwrap()
        },
        &queue_tx,
        &|| polled.fetch_add(1, Ordering::SeqCst) >= 4,
    );
    assert_eq!(subscribed.load(Ordering::SeqCst), 2);
    assert_eq!(queue_rx.try_recv().unwrap().id, local.id);
    assert!(queue_rx.try_recv().is_err());
    drop(tx_b);
}

#[test]
fn forward_sends_a_system_note_that_replies_to_an_owner_message_and_it_validates() {
    let (tx, rx) = mpsc::sync_channel::<ChatPush>(8);
    let mut note = msg(AuthorKind::System, MACHINE, None);
    note.reply_to = Some(new_message_id());
    note.validate().unwrap();
    tx.send(ChatPush {
        message: note.clone(),
    })
    .unwrap();
    drop(tx);
    let (queue_tx, queue_rx) = mpsc::sync_channel::<ChatMessage>(1);
    let rx = Mutex::new(Some(rx));
    let polled = AtomicUsize::new(0);
    forward(
        &|| {
            rx.lock()
                .unwrap()
                .take()
                .unwrap_or_else(|| mpsc::sync_channel(1).1)
        },
        &queue_tx,
        &|| polled.fetch_add(1, Ordering::SeqCst) >= 2,
    );
    let sent = queue_rx.try_recv().unwrap();
    assert_eq!(sent.reply_to, note.reply_to);
}

#[test]
fn forward_receives_zero_activity_events() {
    let dir = tempfile::tempdir().unwrap();
    let chat = Chat::open(dir.path());
    chat.set_activity_consent(true);
    let activity = |i: u32| tmgr_ipc::chat::ChatActivity {
        v: 1,
        room: chat.session_room("s_abcdefghij"),
        state: tmgr_ipc::chat::ActivityState::Working,
        current: Some(format!("Bash: {i}")),
        used: vec!["Read: a".into()],
        calls: Vec::new(),
    };
    let (queue_tx, queue_rx) = mpsc::sync_channel::<ChatMessage>(8);
    let polled = AtomicUsize::new(0);
    forward(
        &|| {
            let rx = chat.subscribe();
            for i in 0..10 {
                chat.publish_activity(&activity(i));
            }
            rx
        },
        &queue_tx,
        &|| polled.fetch_add(1, Ordering::SeqCst) >= 2,
    );
    assert!(queue_rx.try_recv().is_err());
}

#[test]
fn send_wait_is_capped_at_five_minutes() {
    let m = msg(AuthorKind::Manager, MACHINE, None);
    let relay = ScriptedRelay::new(
        vec![],
        vec![Err("relay is busy, retry in 3600s".into()), Ok(())],
    );
    let sleeps = Mutex::new(Vec::new());
    send_one(&relay, &m, &|| false, &|d| {
        sleeps.lock().unwrap().push(d.as_secs())
    });
    assert_eq!(*sleeps.lock().unwrap(), [300]);
}

#[test]
fn poll_failures_are_logged_on_the_first_error_and_on_each_class_change() {
    let mut log = FailureLog::default();
    let busy = r#"{"error":"busy","retry_after":9}"#;
    assert_eq!(log.note("relay unreachable"), Some("failed"));
    assert_eq!(log.note("relay unreachable"), None);
    assert_eq!(log.note(busy), Some("rate limited"));
    assert_eq!(log.note(busy), None);
    assert_eq!(log.note("HTTP 401"), Some("unauthorized"));
    log.clear();
    assert_eq!(log.note("HTTP 401"), Some("unauthorized"));
}

#[test]
fn only_agent_decision_cards_push() {
    let mut m = msg(AuthorKind::Manager, MACHINE, None);
    assert!(!wants_push(&m));
    m.kind = MessageKind::Decision;
    assert!(wants_push(&m));
    m.author.kind = AuthorKind::Session;
    assert!(wants_push(&m));
    m.author.kind = AuthorKind::Owner;
    assert!(!wants_push(&m));
    m.author.kind = AuthorKind::System;
    assert!(!wants_push(&m));
}

#[test]
fn the_sink_hands_the_ingest_only_the_mention_target_never_the_forged_state() {
    let targets = Arc::new(Mutex::new(Vec::new()));
    let log = targets.clone();
    let sink = RelaySink::new(
        MACHINE.into(),
        Box::new(move |_, t| {
            log.lock().unwrap().push(t);
            stored()
        }),
    );
    let target = RoomRef::session(MACHINE, "s_target0001");
    let mut m = msg(AuthorKind::Owner, MACHINE, None);
    m.mention = Some(tmgr_ipc::chat::MentionRef {
        id: new_message_id(),
        target: target.clone(),
        target_title: "forged".into(),
        state: tmgr_ipc::chat::MentionState::Answered,
        hop: 9,
    });
    sink.deliver("dev-1", Some("mobile"), m).unwrap();
    sink.deliver(
        "dev-1",
        Some("mobile"),
        msg(AuthorKind::Owner, MACHINE, None),
    )
    .unwrap();
    assert_eq!(*targets.lock().unwrap(), [Some(target), None]);
}

#[test]
fn a_receipt_the_target_cannot_take_is_dropped_without_retry() {
    let relay = ScriptedRelay::new(vec![], vec![]);
    *relay.receipts.lock().unwrap() = vec![Err("the target is not a trusted desktop".into())];
    send_receipt_one(
        &relay,
        &receipt(ReceiptState::Delivered),
        &|| false,
        &|_| panic!("must not wait"),
    );
    assert_eq!(relay.receipts_sent.lock().unwrap().len(), 1);
}

#[test]
fn poll_cadence_is_one_second_with_live_sessions_and_five_after_two_idle_minutes() {
    let secs = Duration::from_secs;
    assert_eq!(cadence(true, secs(0)), secs(1));
    assert_eq!(cadence(true, secs(3600)), secs(1));
    assert_eq!(cadence(false, secs(119)), secs(1));
    assert_eq!(cadence(false, secs(120)), secs(5));
}

#[test]
fn the_pace_falls_back_after_two_minutes_without_sessions_and_speeds_up_again() {
    let t0 = Instant::now();
    let mut pace = Pace::new(t0);
    let at = |s: u64| t0 + Duration::from_secs(s);
    assert_eq!(pace.next(true, at(10)), Duration::from_secs(1));
    assert_eq!(pace.next(false, at(60)), Duration::from_secs(1));
    assert_eq!(pace.next(false, at(129)), Duration::from_secs(1));
    assert_eq!(pace.next(false, at(130)), Duration::from_secs(5));
    assert_eq!(pace.next(true, at(400)), Duration::from_secs(1));
    assert_eq!(pace.next(false, at(401)), Duration::from_secs(1));
}
