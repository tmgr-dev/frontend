use std::path::Path;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tmgr_ipc::decisions::{CardIn, Event, Inbox, Opt, Secret, Source};
use tmgr_phone::{AckBody, AckResult, Action, AnswerBody, Dir, HelloBody, HelloCtx, Kind, Pairing, SnapshotBody};

use super::super::hub::Hub;
use super::super::push::{route_with, OpencodeSender, CHANGED, PushErr, Queue, SocketSender};
use super::super::store::{AnswerIn, Store};
use super::audit::Audit;
use super::pairing::PairState;
use super::pins::{MemStore, Pin, PinStore, Pins};
use super::relay::fake::FakeRelay;
use super::relay::{Conn, RelayMsg};
use super::*;

const T0: i64 = 1_700_000_000_000;
const HOUR: i64 = 3_600_000;
const BASE: &str = "http://127.0.0.1:9/api/";
const MBX: &str = "3f2b8c1e-5d4a-4b7e-9c3d-1a2b3c4d5e6f";

#[derive(Default)]
struct FakeHost {
  notes: Mutex<Vec<String>>,
  delivered: Mutex<Vec<(String, String, String)>>,
  hub: Mutex<Option<Arc<Hub>>>,
  before_route: Mutex<Option<Box<dyn FnOnce() + Send>>>,
  outcomes: Mutex<Vec<RouteOutcome>>,
  changed: AtomicUsize,
}

struct NoSocket;

impl SocketSender for NoSocket {
  fn send(&self, _: &str, _: &str, _: &str) -> Result<(), PushErr> {
    panic!("nothing may be typed into a session");
  }
}

struct NoOpencode;

impl OpencodeSender for NoOpencode {
  fn send(&self, _: &str, _: Option<&str>, _: &str, _: &str) -> Result<(), PushErr> {
    panic!("nothing may be sent to opencode");
  }
}

impl Host for FakeHost {
  fn notify(&self, _: &str, body: &str) {
    self.notes.lock().unwrap().push(body.to_owned());
  }

  fn route(&self, card_id: &str, event: &Event, bound: Option<&str>) -> RouteOutcome {
    if let Some(f) = self.before_route.lock().unwrap().take() {
      f();
    }
    self.delivered.lock().unwrap().push((
      card_id.to_owned(),
      event.free_text.clone().or_else(|| event.text.clone()).unwrap_or_default(),
      bound.unwrap_or_default().to_owned(),
    ));
    let hub = self.hub.lock().unwrap().clone().expect("hub");
    let out = route_with(&hub, &Queue::default(), &NoSocket, &NoOpencode, card_id, event, bound);
    self.outcomes.lock().unwrap().push(out);
    out
  }

  fn changed(&self) {
    self.changed.fetch_add(1, Ordering::SeqCst);
  }

  fn open_window(&self) {}
}

struct Phone {
  pin: Pin,
  sign: Vec<u8>,
  kex: Vec<u8>,
  d_sign_pub: Vec<u8>,
  d_kex_pub: Vec<u8>,
}

impl Phone {
  fn pairing(&self) -> Pairing<'_> {
    Pairing { mailbox: &self.pin.mailbox, desktop_fp: &self.pin.desktop_fp, phone_fp: &self.pin.phone_fp }
  }
}

struct Rig {
  phones: Arc<Phones>,
  hub: Arc<Hub>,
  relay: Arc<FakeRelay>,
  host: Arc<FakeHost>,
  clock: Arc<AtomicI64>,
  store: Arc<MemStore>,
  dir: tempfile::TempDir,
  counter: AtomicUsize,
}

fn rig() -> Rig {
  let store = Arc::new(MemStore::default());
  rig_with(store.clone(), store)
}

fn rig_with(store: Arc<MemStore>, pins: Arc<dyn PinStore>) -> Rig {
  let clock = Arc::new(AtomicI64::new(T0));
  let c = clock.clone();
  let dir = tempfile::tempdir().unwrap();
  let hub = Arc::new(Hub::new(Store::open(Path::new(":memory:")).unwrap(), Box::new(|_, _| {})));
  let relay = Arc::new(FakeRelay::default());
  let host = Arc::new(FakeHost::default());
  *host.hub.lock().unwrap() = Some(hub.clone());
  let phones = Phones::new(
    pins,
    relay.clone(),
    hub.clone(),
    host.clone(),
    Audit::new(dir.path().join("audit.jsonl")),
    Box::new(move || c.load(Ordering::SeqCst)),
  );
  Rig { phones, hub, relay, host, clock, store, dir, counter: AtomicUsize::new(1) }
}

fn key(seed: u8) -> Vec<u8> {
  vec![seed; 32]
}

impl Rig {
  fn now(&self) -> i64 {
    self.clock.load(Ordering::SeqCst)
  }

  fn pair(&self, mailbox: &str, name: &str, seed: u8) -> Phone {
    let (ds, dk, ps, pk) = (key(seed), key(seed + 1), key(seed + 2), key(seed + 3));
    let (d_sign_pub, d_kex_pub) = (tmgr_phone::public_key(&ds).unwrap(), tmgr_phone::public_key(&dk).unwrap());
    let (p_sign_pub, p_kex_pub) = (tmgr_phone::public_key(&ps).unwrap(), tmgr_phone::public_key(&pk).unwrap());
    let pin = Pin {
      mailbox: mailbox.into(),
      api_base: BASE.into(),
      user_id: 7,
      desktop_sign_secret: tmgr_phone::b64(&ds),
      desktop_kex_secret: tmgr_phone::b64(&dk),
      desktop_fp: tmgr_phone::fingerprint(&d_sign_pub, &d_kex_pub),
      desktop_token: "DESKTOP-TOKEN-VALUE".into(),
      desktop_name: "Studio Mac".into(),
      phone_sign_pub: tmgr_phone::b64(&p_sign_pub),
      phone_kex_pub: tmgr_phone::b64(&p_kex_pub),
      phone_fp: tmgr_phone::fingerprint(&p_sign_pub, &p_kex_pub),
      phone_name: name.into(),
      last_seq: 0,
      paired_ms: self.now(),
      last_used_ms: 0,
    };
    self.phones.pins.put(&pin).unwrap();
    Phone { pin, sign: ps, kex: pk, d_sign_pub, d_kex_pub }
  }

  fn card(&self, title: &str) -> String {
    let card = CardIn {
      title: title.into(),
      kind: "decision".into(),
      source: Source { agent: "claude".into(), session: Some("s1".into()), ..Default::default() },
      options: vec![
        Opt { id: "a".into(), label: "Ship".into(), recommended: true },
        Opt { id: "b".into(), label: "Hold".into(), recommended: false },
      ],
      ..Default::default()
    };
    self.hub.create(card).unwrap().id
  }

  fn hook_card(&self, title: &str, runner: &str) -> String {
    let card = CardIn {
      title: title.into(),
      kind: "stopped".into(),
      reply_via: "hook".into(),
      source: Source {
        agent: "claude".into(),
        session: Some("s2".into()),
        runner_session: Some(runner.into()),
        ..Default::default()
      },
      ..Default::default()
    };
    self.hub.create(card).unwrap().id
  }

  fn snapshot(&self, p: &Phone) -> SnapshotBody {
    self.phones.refresh_snapshots(true);
    let all = self.relay.0.lock().unwrap().snapshots.clone();
    all
      .iter()
      .rev()
      .find_map(|(sealed, _)| tmgr_phone::open_snapshot(&p.kex, &p.pairing(), &p.d_sign_pub, sealed).ok())
      .unwrap()
  }

  fn bind(&self, p: &Phone, card_id: &str) -> (String, String) {
    let snap = self.snapshot(p);
    let c = snap.cards.iter().find(|c| c.id == card_id).unwrap();
    let v: serde_json::Value = serde_json::from_str(&c.projection).unwrap();
    (c.digest.clone(), v["session"].as_str().unwrap().to_owned())
  }

  fn next_id(&self) -> String {
    format!("{:032x}", self.counter.fetch_add(1, Ordering::SeqCst))
  }

  fn body(
    &self,
    card_id: &str,
    digest: &str,
    session: &str,
    action: Action,
    choice: Option<&str>,
    text: Option<&str>,
  ) -> AnswerBody {
    AnswerBody {
      card_id: card_id.into(),
      card_digest: digest.into(),
      session: session.into(),
      answer_id: self.next_id(),
      issued_at: self.now(),
      action,
      choice: choice.map(Into::into),
      text: text.map(Into::into),
    }
  }

  fn send(&self, p: &Phone, body: &AnswerBody) -> String {
    let sealed = tmgr_phone::seal_answer(&p.d_kex_pub, &p.pairing(), &p.sign, body).unwrap();
    self.relay.push("answer", &body.answer_id, &sealed)
  }

  fn acks(&self, p: &Phone) -> Vec<AckBody> {
    let posted = self.relay.0.lock().unwrap().posted.clone();
    posted
      .iter()
      .filter(|(k, _, _)| k == "ack")
      .filter_map(|(_, _, b)| tmgr_phone::open_ack(&p.kex, &p.pairing(), &p.d_sign_pub, b).ok())
      .collect()
  }

  fn answer_ack(&self, p: &Phone, body: &AnswerBody) -> AckBody {
    self.send(p, body);
    self.phones.poll_all();
    self.acks(p).into_iter().rev().find(|a| a.answer_id == body.answer_id).expect("an ack")
  }

  fn state(&self, id: &str) -> String {
    self.hub.get(id).unwrap().state
  }

  fn stored_ack(&self, answer_id: &str) -> String {
    let ack = self.hub.stored_phone_ack(answer_id).expect("a stored ack");
    serde_json::from_str::<serde_json::Value>(&ack).unwrap()["result"].as_str().unwrap().to_owned()
  }

  fn typed(&self) -> usize {
    self.host.delivered.lock().unwrap().len()
  }
}

#[test]
fn applied_for_an_mcp_card() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  let body = r.body(&id, &digest, &session, Action::Answer, Some("a"), None);
  let ack = r.answer_ack(&p, &body);
  assert_eq!(ack.result, AckResult::Applied);
  assert_eq!(ack.label.as_deref(), Some("Ship"));
  let view = r.hub.get(&id).unwrap();
  assert_eq!(view.state, "answered");
  let ev = view.answer.unwrap();
  assert_eq!(ev.choice.as_deref(), Some("a"));
  assert_eq!(ev.via.as_deref(), Some("phone"));
  assert_eq!(ev.via_fp.as_deref(), Some(&p.pin.phone_fp[..8]));
  assert_eq!(r.host.notes.lock().unwrap().as_slice(), ["Answered from iPhone: Ship"]);
  assert_eq!(r.typed(), 0);
  assert!(r.relay.0.lock().unwrap().inbox.is_empty());
  assert!(r.phones.pins.get("mb1").unwrap().unwrap().last_used_ms > 0);
}

#[test]
fn applied_for_a_hook_card_types_into_its_bound_session_only() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.hook_card("Agent stopped", "run-1");
  let (digest, session) = r.bind(&p, &id);
  assert_eq!(session, "runner:run-1");
  let body = r.body(&id, &digest, &session, Action::Answer, None, Some("go on"));
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Applied);
  assert_eq!(
    r.host.delivered.lock().unwrap().as_slice(),
    [(id.clone(), "go on".to_owned(), "runner:run-1".to_owned())]
  );
  assert_eq!(r.host.outcomes.lock().unwrap().as_slice(), [RouteOutcome::Queued]);
  assert_eq!(r.host.notes.lock().unwrap().as_slice(), ["Answered from iPhone: Text reply"]);
}

#[test]
fn replay_returns_duplicate_and_applies_once() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  let body = r.body(&id, &digest, &session, Action::Answer, Some("a"), None);
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Applied);
  let again = r.answer_ack(&p, &body);
  assert_eq!(again.result, AckResult::Duplicate);
  assert_eq!(again.label.as_deref(), Some("Ship"));
  assert_eq!(r.host.notes.lock().unwrap().len(), 1);
  assert_eq!(r.hub.get(&id).unwrap().answer.unwrap().seq, 1);
}

fn assert_failed_verification(r: &Rig, id: &str, p: &Phone) {
  assert_eq!(r.state(id), "open");
  assert!(r.acks(p).is_empty());
  assert!(r.relay.0.lock().unwrap().inbox.is_empty());
  assert_eq!(r.phones.verification_failures(), 1);
  assert_eq!(r.host.notes.lock().unwrap().as_slice(), ["A message from a paired iPhone failed verification"]);
}

#[test]
fn a_wrong_phone_key_applies_nothing_and_gets_no_ack() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  let body = r.body(&id, &digest, &session, Action::Answer, Some("a"), None);
  let sealed = tmgr_phone::seal_answer(&p.d_kex_pub, &p.pairing(), &key(99), &body).unwrap();
  r.relay.push("answer", &body.answer_id, &sealed);
  r.phones.poll_all();
  assert_failed_verification(&r, &id, &p);
}

#[test]
fn tampered_ciphertext_applies_nothing_and_gets_no_ack() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  let body = r.body(&id, &digest, &session, Action::Answer, Some("a"), None);
  let sealed = tmgr_phone::seal_answer(&p.d_kex_pub, &p.pairing(), &p.sign, &body).unwrap();
  let mut v: serde_json::Value = serde_json::from_str(&sealed).unwrap();
  let mut ct = tmgr_phone::b64_decode(v["ct"].as_str().unwrap()).unwrap();
  ct[3] ^= 1;
  v["ct"] = tmgr_phone::b64(&ct).into();
  r.relay.push("answer", &body.answer_id, &v.to_string());
  r.phones.poll_all();
  assert_failed_verification(&r, &id, &p);
}

#[test]
fn verification_alerts_are_rate_limited() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  for _ in 0..3 {
    r.relay.push("answer", &r.next_id(), "{}");
  }
  r.phones.poll_all();
  assert_eq!(r.phones.verification_failures(), 3);
  assert_eq!(r.host.notes.lock().unwrap().len(), 1);
  r.clock.fetch_add(11 * 60 * 1000, Ordering::SeqCst);
  r.relay.push("answer", &r.next_id(), "{}");
  r.phones.poll_all();
  assert_eq!(r.host.notes.lock().unwrap().len(), 2);
  assert!(r.acks(&p).is_empty());
}

#[test]
fn a_swapped_card_id_is_changed_and_touches_nothing() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let a = r.card("First");
  let b = r.card("Second");
  let (digest_a, session) = r.bind(&p, &a);
  let body = r.body(&b, &digest_a, &session, Action::Answer, Some("a"), None);
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Changed);
  assert_eq!(r.state(&a), "open");
  assert_eq!(r.state(&b), "open");
}

#[test]
fn a_new_agent_message_makes_the_digest_stale() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  r.hub.reply(&id, Some("actually, wait".into()), vec![]).unwrap();
  let body = r.body(&id, &digest, &session, Action::Answer, Some("a"), None);
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Changed);
  assert_eq!(r.state(&id), "open");
  assert_eq!(r.typed(), 0);
  assert_eq!(r.stored_ack(&body.answer_id), "changed");
}

#[test]
fn a_changed_patch_makes_the_digest_stale() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let patch = |text: &str| tmgr_ipc::decisions::PatchIn { label: "fix".into(), text: text.into() };
  let make = |text: &str, replaces: Option<String>| CardIn {
    title: "Apply?".into(),
    kind: "decision".into(),
    replaces,
    patches: vec![patch(text)],
    source: Source { agent: "claude".into(), session: Some("s1".into()), ..Default::default() },
    options: vec![Opt { id: "a".into(), label: "Ship".into(), recommended: true }],
    ..Default::default()
  };
  let id = r.hub.create(make("+safe\n", None)).unwrap().id;
  let (digest, session) = r.bind(&p, &id);
  r.hub.create(make("+evil\n", Some(id.clone()))).unwrap();
  let body = r.body(&id, &digest, &session, Action::Answer, Some("a"), None);
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Changed);
  assert_eq!(r.state(&id), "open");
}

#[test]
fn an_answer_signed_before_the_session_ended_is_changed() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let card = CardIn {
    title: "Ship?".into(),
    kind: "decision".into(),
    source: Source { agent: "claude".into(), session: Some("s1".into()), pid: Some(4242), ..Default::default() },
    options: vec![Opt { id: "a".into(), label: "Ship".into(), recommended: true }],
    ..Default::default()
  };
  let id = r.hub.create(card).unwrap().id;
  let (digest, session) = r.bind(&p, &id);
  r.hub.check_liveness(|_| false);
  assert!(r.hub.get(&id).unwrap().session_gone);
  let body = r.body(&id, &digest, &session, Action::Answer, Some("a"), None);
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Changed);
  assert_eq!(r.state(&id), "open");
}

#[test]
fn a_moved_session_target_is_changed_and_nothing_is_typed() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.hook_card("Agent stopped", "run-1");
  let (digest, _) = r.bind(&p, &id);
  let body = r.body(&id, &digest, "runner:run-2", Action::Answer, None, Some("rm -rf /"));
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Changed);
  assert_eq!(r.typed(), 0);
  assert_eq!(r.state(&id), "open");
}

#[test]
fn a_target_change_between_the_check_and_delivery_types_nothing_and_acks_changed() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let card = CardIn {
    title: "Agent stopped".into(),
    kind: "stopped".into(),
    reply_via: "hook".into(),
    source: Source {
      agent: "claude".into(),
      session: Some("s2".into()),
      runner_session: Some("run-1".into()),
      ..Default::default()
    },
    ..Default::default()
  };
  let endpoint = "/tmp/cc-socks/77.sock";
  let inbox = Inbox {
    kind: "claude".into(),
    endpoint: endpoint.into(),
    token: Some(Secret::new("tok")),
    session: Some("sess-1".into()),
  };
  let id = r.hub.create_with(card, Some(inbox)).unwrap().id;
  let (digest, session) = r.bind(&p, &id);
  assert!(session.starts_with("claude:sess-1@") && session.ends_with("|runner:run-1"));
  let hub = r.hub.clone();
  let moved = id.clone();
  *r.host.before_route.lock().unwrap() = Some(Box::new(move || hub.drop_target(&moved, endpoint)));
  let body = r.body(&id, &digest, &session, Action::Answer, None, Some("go on"));
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Changed);
  assert_eq!(r.host.outcomes.lock().unwrap().as_slice(), [RouteOutcome::TargetChanged]);
  let d = r.hub.get(&id).unwrap().delivery.unwrap();
  assert_eq!((d.state.as_str(), d.detail.as_str()), ("undelivered", CHANGED));
  assert_eq!(r.stored_ack(&body.answer_id), "changed");
}

#[test]
fn a_replay_after_a_route_time_target_change_returns_the_changed_ack() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let card = CardIn {
    title: "Agent stopped".into(),
    kind: "stopped".into(),
    reply_via: "hook".into(),
    source: Source { agent: "claude".into(), session: Some("s2".into()), ..Default::default() },
    ..Default::default()
  };
  let endpoint = "/tmp/cc-socks/77.sock";
  let inbox = Inbox {
    kind: "claude".into(),
    endpoint: endpoint.into(),
    token: Some(Secret::new("tok")),
    session: Some("sess-1".into()),
  };
  let id = r.hub.create_with(card, Some(inbox)).unwrap().id;
  let (digest, session) = r.bind(&p, &id);
  let hub = r.hub.clone();
  let moved = id.clone();
  *r.host.before_route.lock().unwrap() = Some(Box::new(move || hub.drop_target(&moved, endpoint)));
  let body = r.body(&id, &digest, &session, Action::Answer, None, Some("go on"));
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Changed);
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Changed);
  assert_eq!(r.typed(), 1);
}

#[test]
fn a_session_label_re_registered_with_another_endpoint_is_changed_and_nothing_is_sent() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let card = CardIn {
    title: "Agent stopped".into(),
    kind: "stopped".into(),
    reply_via: "hook".into(),
    source: Source { agent: "claude".into(), session: Some("s2".into()), ..Default::default() },
    ..Default::default()
  };
  let inbox = |endpoint: &str| Inbox {
    kind: "claude".into(),
    endpoint: endpoint.into(),
    token: Some(Secret::new("tok")),
    session: Some("sess-1".into()),
  };
  let id = r.hub.create_with(card.clone(), Some(inbox("/tmp/cc-socks/77.sock"))).unwrap().id;
  let (digest, session) = r.bind(&p, &id);
  r.hub.drop_target(&id, "/tmp/cc-socks/77.sock");
  r.hub.create_with(CardIn { id: Some(id.clone()), ..card }, Some(inbox("/tmp/cc-socks/88.sock"))).unwrap();
  let body = r.body(&id, &digest, &session, Action::Answer, None, Some("secret"));
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Changed);
  assert_eq!(r.state(&id), "open");
}

#[test]
fn expired_and_future_issued_at_are_refused() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  let mut old = r.body(&id, &digest, &session, Action::Answer, Some("a"), None);
  old.issued_at = r.now() - 25 * HOUR;
  assert_eq!(r.answer_ack(&p, &old).result, AckResult::Expired);
  let mut future = r.body(&id, &digest, &session, Action::Answer, Some("a"), None);
  future.issued_at = r.now() + 10 * 60 * 1000;
  assert_eq!(r.answer_ack(&p, &future).result, AckResult::Expired);
  assert_eq!(r.state(&id), "open");
}

#[test]
fn a_replay_after_the_window_is_expired_not_duplicate() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  let body = r.body(&id, &digest, &session, Action::Answer, Some("a"), None);
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Applied);
  r.clock.fetch_add(25 * HOUR, Ordering::SeqCst);
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Expired);
}

#[test]
fn only_an_applied_answer_extends_the_pairing() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  let body = r.body(&id, &digest, &session, Action::Answer, Some("a"), None);
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Applied);
  let used = r.phones.pins.get("mb1").unwrap().unwrap().last_used_ms;
  r.clock.fetch_add(HOUR, Ordering::SeqCst);
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Duplicate);
  let again = r.body(&id, &digest, &session, Action::Answer, Some("b"), None);
  assert_eq!(r.answer_ack(&p, &again).result, AckResult::Closed);
  let mut stale = r.body(&id, &digest, &session, Action::Answer, Some("a"), None);
  stale.issued_at = r.now() - 25 * HOUR;
  assert_eq!(r.answer_ack(&p, &stale).result, AckResult::Expired);
  assert_eq!(r.phones.pins.get("mb1").unwrap().unwrap().last_used_ms, used);
}

#[test]
fn a_revoked_pairing_gets_ack_revoked_and_applies_nothing() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  let body = r.body(&id, &digest, &session, Action::Answer, Some("a"), None);
  let mid = r.send(&p, &body);
  r.phones.pins.delete("mb1").unwrap();
  let msg = RelayMsg { id: mid, kind: "answer".into(), reference: body.answer_id.clone(), body: String::new() };
  r.phones.handle_answer(&p.pin, &msg).unwrap();
  assert_eq!(r.acks(&p)[0].result, AckResult::Revoked);
  assert_eq!(r.state(&id), "open");
}

struct ChangedAfterFirstRead {
  inner: Arc<MemStore>,
  armed: std::sync::atomic::AtomicBool,
  replacement: Option<String>,
}

impl PinStore for ChangedAfterFirstRead {
  fn get(&self, account: &str) -> Result<Option<String>, String> {
    let v = self.inner.get(account)?;
    if account != "index" && self.armed.swap(false, Ordering::SeqCst) {
      match &self.replacement {
        Some(r) => self.inner.set(account, r)?,
        None => self.inner.delete(account)?,
      }
    }
    Ok(v)
  }

  fn set(&self, account: &str, value: &str) -> Result<(), String> {
    self.inner.set(account, value)
  }

  fn delete(&self, account: &str) -> Result<(), String> {
    self.inner.delete(account)
  }
}

fn answer_after_pin_change(replace: impl FnOnce(&Pin) -> Option<Pin>) -> (Rig, Phone, String, String) {
  let mem = Arc::new(MemStore::default());
  let probe = rig();
  let p = probe.pair("mb1", "Pocket", 10);
  let replacement = replace(&p.pin).map(|pin| serde_json::to_string(&pin).unwrap());
  let flaky = Arc::new(ChangedAfterFirstRead { inner: mem.clone(), armed: Default::default(), replacement });
  let r = rig_with(mem, flaky.clone());
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  let body = r.body(&id, &digest, &session, Action::Answer, Some("a"), None);
  let mid = r.send(&p, &body);
  let sealed = r.relay.0.lock().unwrap().inbox.iter().find(|m| m.id == mid).unwrap().body.clone();
  let msg = RelayMsg { id: mid.clone(), kind: "answer".into(), reference: body.answer_id.clone(), body: sealed };
  flaky.armed.store(true, Ordering::SeqCst);
  r.phones.handle_answer(&p.pin, &msg).unwrap();
  (r, p, id, body.answer_id)
}

#[test]
fn a_revoke_after_the_first_pairing_check_still_rejects_the_answer() {
  let (r, p, id, answer_id) = answer_after_pin_change(|_| None);
  assert_eq!(r.acks(&p).last().unwrap().result, AckResult::Revoked);
  assert_eq!(r.state(&id), "open");
  assert!(r.hub.stored_phone_ack(&answer_id).is_none());
  assert!(r.relay.0.lock().unwrap().inbox.is_empty());
}

#[test]
fn a_re_pairing_after_the_first_pairing_check_rejects_the_answer() {
  let (r, _, id, answer_id) = answer_after_pin_change(|pin| {
    let mut fresh = pin.clone();
    fresh.phone_sign_pub = tmgr_phone::b64(&tmgr_phone::public_key(&key(90)).unwrap());
    Some(fresh)
  });
  assert_eq!(r.state(&id), "open");
  assert!(r.hub.stored_phone_ack(&answer_id).is_none());
}

#[test]
fn an_unused_pairing_expires_after_thirty_days() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  let body = r.body(&id, &digest, &session, Action::Answer, Some("a"), None);
  r.send(&p, &body);
  r.clock.fetch_add(31 * 24 * HOUR, Ordering::SeqCst);
  assert!(r.phones.list().unwrap().is_empty());
  r.phones.poll_all();
  assert_eq!(r.acks(&p)[0].result, AckResult::Revoked);
  assert!(r.relay.0.lock().unwrap().mailbox_deleted);
  assert!(r.phones.pins.get("mb1").unwrap().is_none());
  assert_eq!(r.state(&id), "open");
}

#[test]
fn use_extends_the_thirty_days() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  r.clock.fetch_add(20 * 24 * HOUR, Ordering::SeqCst);
  let (digest, session) = r.bind(&p, &id);
  let body = r.body(&id, &digest, &session, Action::Defer, None, None);
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Applied);
  r.clock.fetch_add(20 * 24 * HOUR, Ordering::SeqCst);
  assert_eq!(r.phones.list().unwrap().len(), 1);
}

#[test]
fn explicit_revoke_deletes_the_pin_first_and_cleans_the_relay() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let aid = r.next_id();
  r.relay.push("answer", &aid, "x");
  let pin = r.phones.revoke("mb1").unwrap();
  assert!(r.phones.pins.get("mb1").unwrap().is_none());
  assert!(!r.relay.0.lock().unwrap().mailbox_deleted);
  r.phones.cleanup_revoked(&pin);
  assert!(r.relay.0.lock().unwrap().mailbox_deleted);
  assert_eq!(r.acks(&p)[0].result, AckResult::Revoked);
  assert!(r.phones.revoke("mb1").is_err());
}

#[test]
fn a_closed_card_reports_the_winner_and_label() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  r.hub.answer(&id, AnswerIn { choice: Some("b".into()), ..Default::default() }).unwrap();
  let body = r.body(&id, &digest, &session, Action::Answer, Some("a"), None);
  let ack = r.answer_ack(&p, &body);
  assert_eq!(ack.result, AckResult::Closed);
  assert_eq!(ack.winner.as_deref(), Some("Studio Mac"));
  assert_eq!(ack.label.as_deref(), Some("Hold"));
}

#[test]
fn the_first_phone_wins_and_the_second_sees_its_name() {
  let r = rig();
  let a = r.pair("mb1", "Pocket", 10);
  let b = r.pair("mb2", "Tablet", 30);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&a, &id);
  let first = r.body(&id, &digest, &session, Action::Answer, Some("a"), None);
  let second = r.body(&id, &digest, &session, Action::Answer, Some("b"), None);
  for (phone, body) in [(&a, &first), (&b, &second)] {
    let sealed = tmgr_phone::seal_answer(&phone.d_kex_pub, &phone.pairing(), &phone.sign, body).unwrap();
    let msg = RelayMsg { id: "m".into(), kind: "answer".into(), reference: body.answer_id.clone(), body: sealed };
    r.phones.handle_answer(&phone.pin, &msg).unwrap();
  }
  let ack = r.acks(&b).into_iter().find(|x| x.answer_id == second.answer_id).unwrap();
  assert_eq!(ack.result, AckResult::Closed);
  assert_eq!(ack.winner.as_deref(), Some("Pocket"));
  assert_eq!(ack.label.as_deref(), Some("Ship"));
}

#[test]
fn defer_and_dismiss_map_to_the_card_states() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Later?");
  let (digest, session) = r.bind(&p, &id);
  let defer = r.body(&id, &digest, &session, Action::Defer, None, None);
  let ack = r.answer_ack(&p, &defer);
  assert_eq!((ack.result, ack.label.as_deref()), (AckResult::Applied, Some("Deferred")));
  assert_eq!(r.state(&id), "deferred");
  let (digest, session) = r.bind(&p, &id);
  let dismiss = r.body(&id, &digest, &session, Action::Dismiss, None, None);
  assert_eq!(r.answer_ack(&p, &dismiss).label.as_deref(), Some("Dismissed"));
  assert_eq!(r.state(&id), "dismissed");
}

#[test]
fn invalid_answers_are_recorded_and_never_apply_later() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  let long = "x".repeat(1001);
  let cases = [
    r.body(&id, &digest, &session, Action::Answer, Some("zzz"), None),
    r.body(&id, &digest, &session, Action::Answer, None, Some(&long)),
    r.body(&id, &digest, &session, Action::Answer, None, None),
    r.body(&id, &digest, &session, Action::Defer, Some("a"), None),
  ];
  for body in &cases {
    assert_eq!(r.answer_ack(&p, body).result, AckResult::Invalid);
  }
  assert_eq!(r.state(&id), "open");
  assert_eq!(r.answer_ack(&p, &cases[0]).result, AckResult::Duplicate);
  let ok = r.body(&id, &digest, &session, Action::Answer, None, Some(&"y".repeat(1000)));
  assert_eq!(r.answer_ack(&p, &ok).result, AckResult::Applied);
}

#[test]
fn a_rejected_answer_id_can_never_apply_later() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  let mut early = r.body(&id, &digest, &session, Action::Answer, Some("a"), None);
  early.card_digest = "0".repeat(64);
  assert_eq!(r.answer_ack(&p, &early).result, AckResult::Changed);
  early.card_digest = digest;
  assert_eq!(r.answer_ack(&p, &early).result, AckResult::Duplicate);
  assert_eq!(r.state(&id), "open");
}

#[test]
fn ineligible_views_are_refused() {
  let r = rig();
  let id = r.card("Ship it?");
  let mut view = r.hub.get(&id).unwrap();
  assert!(eligible(&view));
  view.reply_via = "other".into();
  assert!(!eligible(&view));
  view.reply_via = "hook".into();
  assert!(eligible(&view));
  view.state = "answered".into();
  assert!(!eligible(&view));
}

#[test]
fn snapshot_seq_never_decreases() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  r.card("One");
  let mut seqs = Vec::new();
  for step in [0, 0, -5000, 0] {
    r.clock.fetch_add(step, Ordering::SeqCst);
    seqs.push(r.snapshot(&p).seq);
  }
  assert!(seqs.windows(2).all(|w| w[1] > w[0]), "{seqs:?}");
  let mut pin = r.phones.pins.get("mb1").unwrap().unwrap();
  pin.last_seq = 0;
  r.phones.pins.put(&pin).unwrap();
  r.clock.fetch_add(10_000, Ordering::SeqCst);
  assert!(r.snapshot(&p).seq > *seqs.last().unwrap());
}

#[test]
fn the_seq_is_persisted_before_the_upload() {
  let r = rig();
  r.pair("mb1", "Pocket", 10);
  r.card("One");
  r.relay.0.lock().unwrap().fail_snapshot = true;
  r.phones.refresh_snapshots(true);
  assert_eq!(r.phones.pins.get("mb1").unwrap().unwrap().last_seq, T0);
}

#[test]
fn nudge_only_when_a_new_open_card_appears() {
  let r = rig();
  r.pair("mb1", "Pocket", 10);
  r.card("One");
  r.phones.refresh_snapshots(false);
  r.card("Two");
  r.clock.fetch_add(10, Ordering::SeqCst);
  r.phones.refresh_snapshots(false);
  r.phones.refresh_snapshots(false);
  let id = r.hub.list().unwrap()[0].id.clone();
  r.hub.reply(&id, Some("note".into()), vec![]).unwrap();
  r.phones.refresh_snapshots(false);
  let nudges: Vec<bool> = r.relay.0.lock().unwrap().snapshots.iter().map(|s| s.1).collect();
  assert_eq!(nudges, [true, true, false]);
}

fn pendings(r: &Rig) -> Vec<u32> {
  r.relay.0.lock().unwrap().pendings.clone()
}

fn nudges(r: &Rig) -> Vec<bool> {
  r.relay.0.lock().unwrap().snapshots.iter().map(|s| s.1).collect()
}

#[test]
fn first_upload_nudges_only_with_an_open_card() {
  let r = rig();
  r.pair("mb1", "Pocket", 10);
  r.phones.refresh_snapshots(true);
  let id = r.card("One");
  r.hub.answer(&id, AnswerIn { deferred: true, ..Default::default() }).unwrap();
  r.phones.synced.lock().unwrap().clear();
  r.phones.refresh_snapshots(true);
  assert_eq!(nudges(&r), [false, false]);
  r.card("Two");
  r.phones.synced.lock().unwrap().clear();
  r.phones.refresh_snapshots(true);
  assert_eq!(nudges(&r), [false, false, true]);
}

#[test]
fn pending_counts_open_cards_beyond_the_cap_and_not_deferred() {
  let r = rig();
  r.pair("mb1", "Pocket", 10);
  r.phones.refresh_snapshots(true);
  for i in 0..32 {
    r.card(&format!("Card {i}"));
  }
  let kept = r.card("Kept");
  r.hub.answer(&kept, AnswerIn { deferred: true, ..Default::default() }).unwrap();
  r.phones.refresh_snapshots(true);
  assert_eq!(pendings(&r), [0, 32]);
}

#[test]
fn pending_is_clamped_to_999() {
  let r = rig();
  r.pair("mb1", "Pocket", 10);
  for i in 0..1005 {
    r.card(&format!("Card {i}"));
  }
  r.phones.refresh_snapshots(true);
  assert_eq!(pendings(&r), [999]);
}

#[test]
fn snapshots_hold_only_eligible_cards_capped_with_more() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  for i in 0..35 {
    r.card(&format!("Card {i}"));
  }
  let done = r.card("Done");
  r.hub.answer(&done, AnswerIn { choice: Some("a".into()), ..Default::default() }).unwrap();
  let snap = r.snapshot(&p);
  assert_eq!((snap.cards.len(), snap.more), (30, 5));
  assert!(snap.cards.iter().all(|c| c.id != done));
  assert_eq!(snap.desktop_name, "Studio Mac");
}

#[test]
fn projection_is_deterministic_and_capped() {
  let r = rig();
  let id = r.card("Big");
  r.hub.reply(&id, Some("m".repeat(1900)), vec![]).unwrap();
  let big = CardIn {
    title: "Huge".into(),
    context: Some("c".repeat(20_000)),
    source: Source { agent: "claude".into(), ..Default::default() },
    ..Default::default()
  };
  let huge = r.hub.create(big).unwrap().id;
  let (v1, v2) = (r.hub.get(&id).unwrap(), r.hub.get(&id).unwrap());
  let a = project(&v1, "").unwrap();
  assert_eq!(a.json, project(&v2, "").unwrap().json);
  assert_eq!(a.digest, tmgr_phone::digest(&a.json));
  assert!(a.json.starts_with(r#"{"p":2,"kind":"decision","reply_via":"mcp","agent":"claude""#));
  let h = project(&r.hub.get(&huge).unwrap(), "").unwrap();
  assert!(h.json.len() <= 16 * 1024);
  let parsed: serde_json::Value = serde_json::from_str(&h.json).unwrap();
  assert_eq!(parsed["context_cut"], true);
  assert!(parsed["context"].as_str().unwrap().chars().count() <= 8000);
}

fn signature_of(p: &Phone, kind: Kind, sealed: &str) -> p256::ecdsa::Signature {
  let opened = tmgr_phone::open(&p.kex, &p.pin.mailbox, Dir::D2p, sealed, &[kind]).unwrap();
  let inner: serde_json::Value = serde_json::from_slice(&opened.plaintext).unwrap();
  let raw = tmgr_phone::b64_decode(inner["sig"].as_str().unwrap()).unwrap();
  p256::ecdsa::Signature::from_der(&raw).unwrap()
}

#[test]
fn desktop_signatures_are_low_s() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("One");
  for _ in 0..8 {
    r.clock.fetch_add(1, Ordering::SeqCst);
    r.snapshot(&p);
  }
  let snapshots = r.relay.0.lock().unwrap().snapshots.clone();
  for (sealed, _) in &snapshots {
    assert!(signature_of(&p, Kind::Snapshot, sealed).normalize_s().is_none());
  }
  let (digest, session) = r.bind(&p, &id);
  let body = r.body(&id, &digest, &session, Action::Defer, None, None);
  r.answer_ack(&p, &body);
  let ack = r.relay.0.lock().unwrap().posted.iter().find(|m| m.0 == "ack").unwrap().2.clone();
  assert!(signature_of(&p, Kind::Ack, &ack).normalize_s().is_none());
}

struct Pairer {
  r: Rig,
  ps: Vec<u8>,
  pk: Vec<u8>,
}

fn begin(r: Rig) -> Pairer {
  r.phones.begin_pairing(BASE, 7, MBX, "dtok", "ptok", "Studio Mac").unwrap();
  Pairer { r, ps: key(50), pk: key(51) }
}

impl Pairer {
  fn hello(&self, user: i64, name: &str, secret_override: Option<Vec<u8>>) -> String {
    let (d_kex, d_fp, secret) = {
      let g = self.r.phones.pairing.lock().unwrap();
      let p = g.as_ref().unwrap();
      (tmgr_phone::public_key(&p.kex_secret).unwrap(), p.desktop_fp.clone(), p.secret.clone())
    };
    let ctx = HelloCtx { host: BASE, user_id: user, mailbox: MBX, desktop_fp: &d_fp };
    let body = HelloBody::build(
      &ctx,
      &secret_override.unwrap_or(secret),
      &tmgr_phone::public_key(&self.ps).unwrap(),
      &tmgr_phone::public_key(&self.pk).unwrap(),
      name,
      "NEXT-PHONE-TOKEN",
    )
    .unwrap();
    tmgr_phone::seal_hello(&d_kex, &ctx, &self.ps, &body).unwrap()
  }

  fn code(&self) -> String {
    self.r.phones.pairing.lock().unwrap().as_ref().unwrap().hello.as_ref().unwrap().0.code.clone()
  }

  fn state(&self) -> PairState {
    self.r.phones.pairing_view().unwrap().state
  }
}

#[test]
fn pairing_happy_path_pins_the_phone_and_uploads_a_snapshot() {
  let p = begin(rig());
  let view = p.r.phones.pairing_view().unwrap();
  assert_eq!(view.state, PairState::Waiting);
  assert!(view.qr_svg_data_url.starts_with("data:image/svg+xml;base64,"));
  p.r.relay.push("hello", "hello", &p.hello(7, "My iPhone", None));
  p.r.phones.poll_pairing();
  assert_eq!(p.state(), PairState::Confirm);
  assert_eq!(p.r.phones.pairing_view().unwrap().phone_name.as_deref(), Some("My iPhone"));
  p.r.card("Q");
  p.r.phones.confirm(&p.code()).unwrap();
  assert_eq!(p.state(), PairState::Done);
  let st = p.r.relay.0.lock().unwrap();
  assert_eq!(st.phone_token.as_deref(), Some("NEXT-PHONE-TOKEN"));
  assert_eq!(st.posted[0].0, "pair_ok");
  assert_eq!(st.snapshots.len(), 1);
  drop(st);
  let list = p.r.phones.list().unwrap();
  assert_eq!((list.len(), list[0].name.as_str(), list[0].last_used_ms), (1, "My iPhone", 0));
  let pin = p.r.phones.pins.get(MBX).unwrap().unwrap();
  let (sign_pub, kex_pub) = (tmgr_phone::public_key(&p.ps).unwrap(), tmgr_phone::public_key(&p.pk).unwrap());
  assert_eq!(pin.phone_fp, tmgr_phone::fingerprint(&sign_pub, &kex_pub));
  let pairing = Pairing { mailbox: MBX, desktop_fp: &pin.desktop_fp, phone_fp: &pin.phone_fp };
  let d_sign_pub = tmgr_phone::public_key(&pin.keys().unwrap().sign_secret).unwrap();
  let sealed = p.r.relay.0.lock().unwrap().posted[0].2.clone();
  let ok = tmgr_phone::open_pair_ok(&p.pk, &pairing, &d_sign_pub, &sealed).unwrap();
  assert_eq!(ok.desktop_name, "Studio Mac");
  assert!(p.r.dir.path().join("audit.jsonl").exists());
}

#[test]
fn a_second_different_hello_aborts_the_pairing() {
  let mut p = begin(rig());
  p.r.relay.push("hello", "hello", &p.hello(7, "My iPhone", None));
  p.r.phones.poll_pairing();
  assert_eq!(p.state(), PairState::Confirm);
  p.ps = key(60);
  p.pk = key(61);
  p.r.relay.push("hello", "hello", &p.hello(7, "Intruder", None));
  p.r.phones.poll_pairing();
  assert_eq!(p.state(), PairState::Failed);
  assert!(p.r.relay.0.lock().unwrap().mailbox_deleted);
  assert!(p.r.phones.confirm("123456").is_err());
}

#[test]
fn hellos_for_another_user_or_secret_are_dropped() {
  let p = begin(rig());
  p.r.relay.push("hello", "hello", &p.hello(8, "Other user", None));
  p.r.relay.push("hello", "hello", &p.hello(7, "No secret", Some(key(1))));
  p.r.phones.poll_pairing();
  assert_eq!(p.state(), PairState::Waiting);
  assert!(p.r.relay.0.lock().unwrap().inbox.is_empty());
}

#[test]
fn three_wrong_codes_fail_the_pairing_and_delete_the_mailbox() {
  let p = begin(rig());
  p.r.relay.push("hello", "hello", &p.hello(7, "My iPhone", None));
  p.r.phones.poll_pairing();
  let good = p.code();
  let wrong = if good == "000000" { "000001" } else { "000000" };
  p.r.phones.confirm(wrong).unwrap();
  assert_eq!(p.r.phones.pairing_view().unwrap().attempts_left, 2);
  p.r.phones.confirm(wrong).unwrap();
  p.r.phones.confirm(wrong).unwrap();
  assert_eq!(p.state(), PairState::Failed);
  assert!(p.r.relay.0.lock().unwrap().mailbox_deleted);
  assert!(p.r.phones.confirm(&good).is_err());
  assert!(p.r.phones.list().unwrap().is_empty());
}

#[test]
fn the_qr_expires_after_five_minutes() {
  let p = begin(rig());
  p.r.clock.fetch_add(5 * 60 * 1000, Ordering::SeqCst);
  assert_eq!(p.state(), PairState::Failed);
  assert!(p.r.relay.0.lock().unwrap().mailbox_deleted);
  p.r.phones.cancel();
  assert!(p.r.phones.pairing_view().is_none());
}

#[test]
fn at_most_three_phones_and_expired_ones_do_not_count() {
  let r = rig();
  for (i, n) in ["a", "b", "c"].iter().enumerate() {
    r.pair(&format!("mb{n}"), n, 10 + 10 * i as u8);
  }
  assert!(r.phones.begin_pairing(BASE, 7, MBX, "dtok", "ptok", "Mac").is_err());
  r.clock.fetch_add(31 * 24 * HOUR, Ordering::SeqCst);
  assert!(r.phones.begin_pairing(BASE, 7, MBX, "dtok", "ptok", "Mac").is_ok());
}

#[test]
fn untrusted_api_bases_are_refused() {
  let r = rig();
  assert!(r.phones.begin_pairing("https://evil.example/api/", 7, MBX, "dtok", "ptok", "Mac").is_err());
  assert!(r.phones.begin_pairing(BASE, 7, "../x", "dtok", "ptok", "Mac").is_err());
}

#[test]
fn debug_output_never_shows_tokens_or_keys() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let conn: Conn = p.pin.conn();
  let msg = RelayMsg { id: "1".into(), kind: "answer".into(), reference: "r".into(), body: "SEALED-BODY".into() };
  let shown = format!("{:?} {:?} {:?}", p.pin, conn, msg);
  for secret in ["DESKTOP-TOKEN-VALUE", &p.pin.desktop_sign_secret, &p.pin.desktop_kex_secret, "SEALED-BODY"] {
    assert!(!shown.contains(secret), "{shown}");
  }
  let pairer = begin(rig());
  let shown = format!("{:?}", pairer.r.phones.pairing.lock().unwrap().as_ref().unwrap());
  assert!(!shown.contains("dtok") && !shown.contains("ptok"), "{shown}");
}

#[test]
fn pins_use_an_index_and_survive_a_new_pins_handle() {
  let r = rig();
  r.pair("mb1", "Pocket", 10);
  r.pair("mb2", "Tablet", 30);
  let again = Pins::new(r.store.clone() as Arc<dyn PinStore>);
  assert_eq!(again.list().unwrap().len(), 2);
  again.delete("mb1").unwrap();
  assert_eq!(again.list().unwrap().len(), 1);
}

#[test]
fn an_update_after_a_revoke_does_not_resurrect_the_pin() {
  let r = rig();
  r.pair("mb1", "Pocket", 10);
  let held = r.phones.pins.get("mb1").unwrap().unwrap();
  r.phones.revoke("mb1").unwrap();
  let out = r.phones.pins.update("mb1", |p| p.last_seq = held.last_seq + 1).unwrap();
  assert!(out.is_none());
  assert!(r.phones.pins.get("mb1").unwrap().is_none());
  assert!(r.phones.pins.list().unwrap().is_empty());
}

#[test]
fn update_changes_a_live_pin_in_place() {
  let r = rig();
  r.pair("mb1", "Pocket", 10);
  let out = r.phones.pins.update("mb1", |p| p.last_seq = 9).unwrap().unwrap();
  assert_eq!(out.last_seq, 9);
  assert_eq!(r.phones.pins.get("mb1").unwrap().unwrap().last_seq, 9);
  assert_eq!(r.phones.pins.list().unwrap().len(), 1);
}

#[test]
fn a_non_uuid_mailbox_is_refused_and_cannot_collide_with_the_index() {
  let r = rig();
  for bad in ["index", "mbx-1", "3f2b8c1e-5d4a-4b7e-9c3d-1a2b3c4d5e6", "3f2b8c1e-5d4a-4b7e-9c3d-1a2b3c4d5e6g"] {
    assert!(r.phones.begin_pairing(BASE, 7, bad, "dtok", "ptok", "Mac").is_err(), "{bad}");
  }
  assert!(r.phones.begin_pairing(BASE, 7, MBX, "dtok", "ptok", "Mac").is_ok());
}

#[test]
fn the_phone_name_in_the_pairing_view_is_cleaned() {
  let p = begin(rig());
  p.r.relay.push("hello", "hello", &p.hello(7, "  My\u{7}iPhone\n ", None));
  p.r.phones.poll_pairing();
  assert_eq!(p.r.phones.pairing_view().unwrap().phone_name.as_deref(), Some("MyiPhone"));
}

#[test]
fn at_most_a_hundred_messages_are_processed_per_poll() {
  let r = rig();
  r.pair("mb1", "Pocket", 10);
  for _ in 0..150 {
    r.relay.push("answer", &r.next_id(), "{}");
  }
  r.phones.poll_all();
  assert_eq!(r.phones.verification_failures(), 100);
  assert_eq!(r.relay.0.lock().unwrap().inbox.len(), 50);
}

#[test]
fn messages_with_unsafe_ids_are_dropped_unprocessed() {
  let r = rig();
  r.pair("mb1", "Pocket", 10);
  for id in ["../x", "a/b", "", &"a".repeat(65)] {
    r.relay.push_with_id(id, "answer", &r.next_id(), "{}");
  }
  r.phones.poll_all();
  assert_eq!(r.phones.verification_failures(), 0);
  assert!(r.relay.0.lock().unwrap().deleted.is_empty());
  assert!(!r.dir.path().join("audit.jsonl").exists());
}

#[test]
fn a_replayed_message_id_is_audited_once() {
  let r = rig();
  r.pair("mb1", "Pocket", 10);
  for _ in 0..3 {
    r.relay.push_with_id("same", "answer", &r.next_id(), "{}");
    r.phones.poll_all();
  }
  let log = std::fs::read_to_string(r.dir.path().join("audit.jsonl")).unwrap();
  assert_eq!(log.lines().filter(|l| l.contains("verify_failed")).count(), 1);
  assert_eq!(r.phones.verification_failures(), 3);
}

fn verify_failed_lines(r: &Rig) -> usize {
  std::fs::read_to_string(r.dir.path().join("audit.jsonl")).unwrap().lines().filter(|l| l.contains("verify_failed")).count()
}

#[test]
fn a_reused_message_id_with_a_different_body_is_audited_again() {
  let r = rig();
  r.pair("mb1", "Pocket", 10);
  for body in ["{}", "{\"a\":1}", "{}"] {
    r.relay.push_with_id("same", "answer", &r.next_id(), body);
    r.phones.poll_all();
  }
  assert_eq!(verify_failed_lines(&r), 2);
}

#[test]
fn the_same_message_id_in_another_mailbox_is_audited() {
  let r = rig();
  let first = r.pair("mb1", "Pocket", 10).pin;
  let second = r.pair("mb2", "Tablet", 20).pin;
  let msg = RelayMsg { id: "same".into(), kind: "answer".into(), reference: String::new(), body: "{}".into() };
  r.phones.verification_failed(&first, &msg);
  r.phones.verification_failed(&second, &msg);
  r.phones.verification_failed(&second, &msg);
  assert_eq!(verify_failed_lines(&r), 2);
}

#[test]
fn the_failure_dedupe_evicts_one_entry_at_a_time() {
  let r = rig();
  let pin = r.pair("mb1", "Pocket", 10).pin;
  let msg = |id: usize| RelayMsg { id: format!("m{id}"), kind: "answer".into(), reference: String::new(), body: "{}".into() };
  for i in 0..1025 {
    r.phones.verification_failed(&pin, &msg(i));
  }
  assert_eq!(verify_failed_lines(&r), 1025);
  r.phones.verification_failed(&pin, &msg(1024));
  r.phones.verification_failed(&pin, &msg(1));
  assert_eq!(verify_failed_lines(&r), 1025);
  r.phones.verification_failed(&pin, &msg(0));
  assert_eq!(verify_failed_lines(&r), 1026);
}

#[test]
fn the_audit_file_stays_near_its_cap() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("audit.jsonl");
  let audit = Audit::new(path.clone());
  let big = "x".repeat(50_000);
  for i in 0..300 {
    audit.append(i, "verify_failed", &[("message", &big)]);
  }
  let size = std::fs::metadata(&path).unwrap().len();
  assert!(size <= audit::MAX_BYTES + 60_000, "{size}");
}

#[test]
fn pairing_polls_handle_a_hundred_hellos_and_drop_unsafe_ids() {
  let p = begin(rig());
  for _ in 0..150 {
    p.r.relay.push("hello", "hello", "{}");
  }
  p.r.relay.push_with_id("../x", "hello", "hello", "{}");
  p.r.phones.poll_pairing();
  assert_eq!(p.r.relay.0.lock().unwrap().inbox.len(), 51);
  assert!(p.r.phones.pairing.lock().unwrap().as_ref().unwrap().seen.len() <= 100);
}

fn plain_hook_card(r: &Rig, title: &str) -> String {
  let card = CardIn {
    title: title.into(),
    kind: "decision".into(),
    reply_via: "hook".into(),
    source: Source { agent: "claude".into(), session: Some("s3".into()), ..Default::default() },
    ..Default::default()
  };
  r.hub.create(card).unwrap().id
}

fn owner_texts(r: &Rig, id: &str) -> Vec<String> {
  r.hub.get(id).unwrap().messages.iter().filter(|m| m.author == "owner").map(|m| m.text.clone()).collect()
}

fn snapshots_sent(r: &Rig) -> usize {
  r.relay.0.lock().unwrap().snapshots.len()
}

fn hold_wait(r: &Rig, id: &str) -> std::thread::JoinHandle<()> {
  let (hub, id) = (r.hub.clone(), id.to_owned());
  let t = std::thread::spawn(move || {
    let _ = hub.wait(&id, 0, std::time::Duration::from_secs(20), false);
  });
  std::thread::sleep(std::time::Duration::from_millis(150));
  t
}

#[test]
fn chat_is_applied_keeps_the_card_open_and_adds_an_owner_message() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  let body = r.body(&id, &digest, &session, Action::Chat, None, Some("wait for CI"));
  let ack = r.answer_ack(&p, &body);
  assert_eq!((ack.result, ack.label.as_deref()), (AckResult::Applied, Some("Message")));
  let view = r.hub.get(&id).unwrap();
  assert_eq!(view.state, "open");
  assert!(view.answer.is_none());
  assert_eq!(owner_texts(&r, &id), ["wait for CI"]);
  let event = r.hub.wait(&id, 0, std::time::Duration::ZERO, false).unwrap().1.unwrap();
  assert_eq!((event.kind.as_str(), event.text.as_deref(), event.via.as_deref()), ("message", Some("wait for CI"), Some("phone")));
  assert_eq!(event.via_fp.as_deref(), Some(&p.pin.phone_fp[..8]));
  assert_eq!(r.host.notes.lock().unwrap().as_slice(), ["Message from iPhone"]);
  assert_eq!(r.host.outcomes.lock().unwrap().len(), 1);
  assert!(r.phones.pins.get("mb1").unwrap().unwrap().last_used_ms > 0);
}

#[test]
fn a_replayed_chat_is_a_duplicate_and_stores_one_message() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  let body = r.body(&id, &digest, &session, Action::Chat, None, Some("once"));
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Applied);
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Duplicate);
  assert_eq!(owner_texts(&r, &id), ["once"]);
  assert_eq!(r.host.notes.lock().unwrap().len(), 1);
}

#[test]
fn chat_for_a_stale_session_is_changed_and_stores_nothing() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.hook_card("Agent stopped", "run-1");
  let (digest, _) = r.bind(&p, &id);
  let chat = r.body(&id, &digest, "runner:run-0", Action::Chat, None, Some("old session"));
  assert_eq!(r.answer_ack(&p, &chat).result, AckResult::Changed);
  assert!(owner_texts(&r, &id).is_empty());
  assert_eq!(r.typed(), 0);
  assert_eq!(r.state(&id), "open");
}

#[test]
fn chat_after_new_messages_on_the_same_session_still_goes() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.hook_card("Agent stopped", "run-1");
  let (_, session) = r.bind(&p, &id);
  let chat = r.body(&id, &"0".repeat(64), &session, Action::Chat, None, Some("go on"));
  assert_eq!(r.answer_ack(&p, &chat).result, AckResult::Applied);
  assert_eq!(owner_texts(&r, &id), ["go on"]);
}

#[test]
fn chat_is_routed_bound_to_the_session_it_was_written_for() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.hook_card("Agent stopped", "run-1");
  let (digest, session) = r.bind(&p, &id);
  let chat = r.body(&id, &digest, &session, Action::Chat, None, Some("go on"));
  assert_eq!(r.answer_ack(&p, &chat).result, AckResult::Applied);
  assert_eq!(r.host.delivered.lock().unwrap().as_slice(), [(id.clone(), "go on".to_owned(), "runner:run-1".to_owned())]);
}

#[test]
fn a_target_change_before_chat_delivery_is_acked_changed_and_types_nothing() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let card = CardIn {
    title: "Agent stopped".into(),
    kind: "stopped".into(),
    reply_via: "hook".into(),
    source: Source { agent: "claude".into(), session: Some("s2".into()), ..Default::default() },
    ..Default::default()
  };
  let endpoint = "/tmp/cc-socks/77.sock";
  let inbox = Inbox {
    kind: "claude".into(),
    endpoint: endpoint.into(),
    token: Some(Secret::new("tok")),
    session: Some("sess-1".into()),
  };
  let id = r.hub.create_with(card, Some(inbox)).unwrap().id;
  let (digest, session) = r.bind(&p, &id);
  let hub = r.hub.clone();
  let moved = id.clone();
  *r.host.before_route.lock().unwrap() = Some(Box::new(move || hub.drop_target(&moved, endpoint)));
  let chat = r.body(&id, &digest, &session, Action::Chat, None, Some("go on"));
  assert_eq!(r.answer_ack(&p, &chat).result, AckResult::Changed);
  assert_eq!(r.host.outcomes.lock().unwrap().as_slice(), [RouteOutcome::TargetChanged]);
  assert_eq!(owner_texts(&r, &id), ["go on"]);
}

#[test]
fn chat_on_a_closed_card_is_closed_and_stores_nothing() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  r.hub.answer(&id, AnswerIn { choice: Some("b".into()), ..Default::default() }).unwrap();
  let body = r.body(&id, &digest, &session, Action::Chat, None, Some("too late"));
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Closed);
  assert!(owner_texts(&r, &id).is_empty());
}

#[test]
fn chat_on_a_hook_card_that_is_not_stopped_is_not_eligible() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = plain_hook_card(&r, "Hook only");
  let (digest, session) = r.bind(&p, &id);
  let body = r.body(&id, &digest, &session, Action::Chat, None, Some("hello"));
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::NotEligible);
  assert!(owner_texts(&r, &id).is_empty());
  assert_eq!(r.typed(), 0);
}

#[test]
fn chat_with_empty_long_or_choice_input_is_invalid() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  let long = "x".repeat(1001);
  let cases = [
    r.body(&id, &digest, &session, Action::Chat, None, None),
    r.body(&id, &digest, &session, Action::Chat, None, Some("   ")),
    r.body(&id, &digest, &session, Action::Chat, None, Some(&long)),
    r.body(&id, &digest, &session, Action::Chat, Some("a"), Some("hi")),
  ];
  for body in &cases {
    assert_eq!(r.answer_ack(&p, body).result, AckResult::Invalid);
  }
  assert!(owner_texts(&r, &id).is_empty());
}

#[test]
fn the_projection_is_v2_with_message_seq_chat_and_task_flags() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  r.hub.message(&id, tmgr_ipc_message("hi")).unwrap();
  let hook = plain_hook_card(&r, "Hook only");
  let stopped = r.hook_card("Stopped", "run-1");
  let linked = r
    .hub
    .create(CardIn {
      title: "Linked".into(),
      source: Source { agent: "claude".into(), ..Default::default() },
      links: tmgr_ipc::decisions::Links { task: Some("https://tmgr.dev/ws/tasks/5".into()), ..Default::default() },
      ..Default::default()
    })
    .unwrap()
    .id;
  let snap = r.snapshot(&p);
  let proj = |id: &str| -> serde_json::Value {
    serde_json::from_str(&snap.cards.iter().find(|c| c.id == id).unwrap().projection).unwrap()
  };
  let a = proj(&id);
  assert_eq!((a["p"].as_u64(), a["chat"].as_bool(), a["task"].as_bool()), (Some(2), Some(true), Some(false)));
  assert_eq!(a["messages"][0]["seq"], 1);
  assert!(a["messages"][0]["created_ms"].as_i64().unwrap() > 0);
  assert_eq!(proj(&hook)["chat"], false);
  assert_eq!(proj(&stopped)["chat"], true);
  assert_eq!(proj(&linked)["task"], true);
}

fn tmgr_ipc_message(text: &str) -> crate::decisions::store::MessageIn {
  crate::decisions::store::MessageIn { text: text.into(), ..Default::default() }
}

#[test]
fn the_snapshot_reports_listening_for_an_mcp_card_with_a_blocked_wait_and_nothing_for_hook_cards() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let hook = plain_hook_card(&r, "Hook only");
  let live = |s: &SnapshotBody, id: &str| s.cards.iter().find(|c| c.id == id).unwrap().live.clone().unwrap();
  let before = r.snapshot(&p);
  assert_eq!(live(&before, &id).listening, Some(false));
  let waiter = hold_wait(&r, &id);
  let during = r.snapshot(&p);
  assert_eq!(live(&during, &id).listening, Some(true));
  assert_eq!(live(&during, &hook).listening, None);
  assert_eq!(during.cards.iter().find(|c| c.id == id).unwrap().digest, before.cards.iter().find(|c| c.id == id).unwrap().digest);
  r.hub.answer(&id, AnswerIn { dismissed: true, ..Default::default() }).unwrap();
  waiter.join().unwrap();
}

#[test]
fn delivered_ms_appears_once_the_agent_picks_the_message_up() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  let (digest, session) = r.bind(&p, &id);
  let body = r.body(&id, &digest, &session, Action::Chat, None, Some("ping"));
  assert_eq!(r.answer_ack(&p, &body).result, AckResult::Applied);
  let messages = |r: &Rig| r.snapshot(&p).cards.iter().find(|c| c.id == id).unwrap().live.clone().unwrap().messages;
  let first = messages(&r);
  assert_eq!((first.len(), first[0].seq, first[0].delivered_ms), (1, 1, None));
  assert!(r.hub.wait(&id, 0, std::time::Duration::ZERO, false).unwrap().1.is_some());
  let second = messages(&r);
  assert!(second[0].delivered_ms.is_some());
  assert_eq!(second[0].saved_ms, None);
}

#[test]
fn live_messages_cover_only_the_messages_the_projection_kept() {
  let r = rig();
  let p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  for i in 0..12 {
    r.hub.message(&id, tmgr_ipc_message(&format!("{i}{}", "m".repeat(1900)))).unwrap();
  }
  let card = r.snapshot(&p).cards.into_iter().find(|c| c.id == id).unwrap();
  let parsed: serde_json::Value = serde_json::from_str(&card.projection).unwrap();
  let kept: Vec<u64> = parsed["messages"].as_array().unwrap().iter().map(|m| m["seq"].as_u64().unwrap()).collect();
  assert!(!kept.is_empty() && kept.len() < 12);
  assert_eq!(card.live.unwrap().messages.iter().map(|m| m.seq).collect::<Vec<_>>(), kept);
}

#[test]
fn a_listening_card_refreshes_every_minute_and_an_idle_one_every_five() {
  let r = rig();
  let _p = r.pair("mb1", "Pocket", 10);
  let id = r.card("Ship it?");
  r.phones.refresh_snapshots(false);
  let idle = snapshots_sent(&r);
  r.clock.fetch_add(61_000, Ordering::SeqCst);
  r.phones.refresh_snapshots(false);
  assert_eq!(snapshots_sent(&r), idle);
  r.clock.fetch_add(5 * 60_000, Ordering::SeqCst);
  r.phones.refresh_snapshots(false);
  assert_eq!(snapshots_sent(&r), idle + 1);
  let waiter = hold_wait(&r, &id);
  r.phones.refresh_snapshots(false);
  let base = snapshots_sent(&r);
  r.clock.fetch_add(30_000, Ordering::SeqCst);
  r.phones.refresh_snapshots(false);
  assert_eq!(snapshots_sent(&r), base);
  r.clock.fetch_add(29_000, Ordering::SeqCst);
  r.phones.refresh_snapshots(false);
  assert_eq!(snapshots_sent(&r), base + 1);
  r.clock.fetch_add(44_000, Ordering::SeqCst);
  r.phones.refresh_snapshots(false);
  assert_eq!(snapshots_sent(&r), base + 1);
  r.clock.fetch_add(15_000, Ordering::SeqCst);
  r.phones.refresh_snapshots(false);
  assert_eq!(snapshots_sent(&r), base + 2);
  r.hub.answer(&id, AnswerIn { dismissed: true, ..Default::default() }).unwrap();
  waiter.join().unwrap();
}

#[test]
fn the_pairing_qr_is_a_tmgr_pair_link_around_the_raw_payload() {
  let link = super::pairing::pair_link(br#"{"v":1}"#);
  let raw = link.strip_prefix("tmgr://pair#").unwrap();
  let body = raw.strip_prefix("tmgr-pair:v1:").unwrap();
  use base64::Engine;
  assert_eq!(base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(body).unwrap(), br#"{"v":1}"#);
  assert!(!link.contains('?') && link.matches('#').count() == 1);
}
