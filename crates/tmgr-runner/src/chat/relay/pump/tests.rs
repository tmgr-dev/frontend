use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use super::*;
use crate::chat::{Chat, Delivery};
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
    sink.deliver("dev-1", m).unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].id, id);
    assert_eq!(seen[0].device.as_deref(), Some("dev-1"));
}

#[test]
fn non_owner_and_foreign_machine_messages_are_dropped() {
    let (sink, seen) = recording_sink(stored);
    for kind in [AuthorKind::Manager, AuthorKind::Session, AuthorKind::System] {
        sink.deliver("dev-1", msg(kind, MACHINE, None)).unwrap();
    }
    sink.deliver("dev-1", msg(AuthorKind::Owner, "mx_ffffffffffffffff", None))
        .unwrap();
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn not_found_and_conflict_are_acked_but_invalid_is_retried() {
    let (sink, _) = recording_sink(|| Err(IngestError::NotFound("no room".into())));
    sink.deliver("d", msg(AuthorKind::Owner, MACHINE, None))
        .unwrap();
    let (sink, _) = recording_sink(|| Err(IngestError::Conflict("differs".into())));
    sink.deliver("d", msg(AuthorKind::Owner, MACHINE, None))
        .unwrap();
    let (sink, _) = recording_sink(|| Err(IngestError::Invalid("store down".into())));
    assert_eq!(
        sink.deliver("d", msg(AuthorKind::Owner, MACHINE, None)),
        Err("store down".into())
    );
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
    sink.deliver("dev-1", m.clone()).unwrap();
    sink.deliver("dev-1", m.clone()).unwrap();
    assert_eq!(delivered.load(Ordering::SeqCst), 1);
    let history = chat.history(&m.room, None, None).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].author.device.as_deref(), Some("dev-1"));
}

struct ScriptedRelay {
    polls: Mutex<Vec<Result<(), String>>>,
    sends: Mutex<Vec<Result<(), String>>>,
    sent: Mutex<Vec<String>>,
}

impl ScriptedRelay {
    fn new(polls: Vec<Result<(), String>>, sends: Vec<Result<(), String>>) -> Self {
        Self {
            polls: Mutex::new(polls),
            sends: Mutex::new(sends),
            sent: Mutex::new(Vec::new()),
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
    run_poll(&relay, &|| relay.polls.lock().unwrap().is_empty(), &|d| {
        sleeps.lock().unwrap().push(d.as_secs())
    });
    assert_eq!(*sleeps.lock().unwrap(), [120, 10, 20, 5, 5]);
}

#[test]
fn poll_backoff_is_capped() {
    let relay = ScriptedRelay::new(vec![Err("x".into()); 12], vec![]);
    let sleeps = Mutex::new(Vec::new());
    run_poll(&relay, &|| relay.polls.lock().unwrap().is_empty(), &|d| {
        sleeps.lock().unwrap().push(d.as_secs())
    });
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
    let (queue_tx, queue_rx) = mpsc::sync_channel(1);
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
    };
    let (queue_tx, queue_rx) = mpsc::sync_channel(8);
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
    sink.deliver("dev-1", m).unwrap();
    sink.deliver("dev-1", msg(AuthorKind::Owner, MACHINE, None))
        .unwrap();
    assert_eq!(*targets.lock().unwrap(), [Some(target), None]);
}
