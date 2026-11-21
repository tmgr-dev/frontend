use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tmgr_chat_relay::api::{Method, Request};

use super::*;
use crate::manager::backend::Author;
use crate::manager::relay::ports::MemKeyStore;

const T0: u64 = 1_800_000_000_000;
const USER: u64 = 42;

struct FDevice {
  id: String,
  token: String,
  kind: String,
  label: String,
  sign_pub: String,
  kex_pub: String,
  intros: Vec<(String, String)>,
}

struct FMsg {
  cursor: i64,
  msg_id: String,
  from: String,
  to: String,
  body: String,
}

#[derive(Default)]
struct Server {
  devices: Vec<FDevice>,
  msgs: Vec<FMsg>,
  revocations: Vec<(String, String)>,
  next: i64,
}

#[derive(Clone, Default)]
struct Fake(Arc<Mutex<Server>>);

impl Fake {
  fn queue_len(&self, to: &str) -> usize {
    self.0.lock().unwrap().msgs.iter().filter(|m| m.to == to).count()
  }

  fn first_body(&self, to: &str) -> (String, String, String) {
    let s = self.0.lock().unwrap();
    let m = s.msgs.iter().find(|m| m.to == to).unwrap();
    (m.msg_id.clone(), m.from.clone(), m.body.clone())
  }

  fn inject(&self, to: &str, msg_id: &str, from: &str, body: &str) {
    let mut s = self.0.lock().unwrap();
    s.next += 1;
    let cursor = s.next;
    s.msgs.push(FMsg { cursor, msg_id: msg_id.into(), from: from.into(), to: to.into(), body: body.into() });
  }
}

fn wrap(v: Value) -> Result<String, String> {
  Ok(json!({ "data": v }).to_string())
}

impl Transport for Fake {
  fn call(&self, req: &Request, token: Option<&str>) -> Result<String, String> {
    let mut s = self.0.lock().unwrap();
    let path = req.path.trim_start_matches("/api/chat-relay").to_owned();
    let caller = token.and_then(|t| s.devices.iter().find(|d| d.token == t).map(|d| d.id.clone()));
    let body = req.body.clone().unwrap_or(Value::Null);
    let seg: Vec<&str> = path.split('?').next().unwrap().trim_matches('/').split('/').collect();
    match (req.method, seg.as_slice()) {
      (Method::Post, ["devices"]) => {
        let id = format!("00000000-0000-4000-8000-{:012x}", rand_n());
        let token = format!("tok-{id}");
        let g = |k: &str| body[k].as_str().unwrap().to_owned();
        s.devices.push(FDevice { id: id.clone(), token: token.clone(), kind: g("kind"), label: g("label"), sign_pub: g("sign_pub"), kex_pub: g("kex_pub"), intros: vec![] });
        wrap(json!({ "device_id": id, "device_token": token }))
      }
      (Method::Get, ["devices"]) => {
        caller.ok_or("401")?;
        let devices: Vec<Value> = s
          .devices
          .iter()
          .map(|d| json!({ "device_id": d.id, "kind": d.kind, "label": d.label, "sign_pub": d.sign_pub, "kex_pub": d.kex_pub, "intros": d.intros.iter().map(|i| i.1.clone()).collect::<Vec<_>>() }))
          .collect();
        let revs: Vec<Value> = s.revocations.iter().map(|r| json!({ "target_fp": r.0, "statement": r.1 })).collect();
        wrap(json!({ "devices": devices, "revocations": revs }))
      }
      (Method::Put, ["devices", target, "intros", "self"]) => {
        let by = caller.ok_or("401")?;
        let intro = body["intro"].as_str().unwrap().to_owned();
        let d = s.devices.iter_mut().find(|d| d.id == *target).ok_or("404")?;
        d.intros.retain(|i| i.0 != by);
        d.intros.push((by, intro));
        Ok(String::new())
      }
      (Method::Delete, ["devices", target]) => {
        s.devices.retain(|d| d.id != *target);
        s.msgs.retain(|m| m.to != *target && m.from != *target);
        Ok(String::new())
      }
      (Method::Post, ["messages"]) => {
        let from = caller.ok_or("401")?;
        let msg_id = body["msg_id"].as_str().unwrap().to_owned();
        let items = body["items"].as_array().unwrap().clone();
        if items.iter().any(|i| !s.devices.iter().any(|d| d.id == i["to"].as_str().unwrap())) {
          return Err(r#"{"error":"recipient_unknown"}"#.into());
        }
        let (mut stored, mut duplicates) = (0, 0);
        for i in items {
          let to = i["to"].as_str().unwrap().to_owned();
          if s.msgs.iter().any(|m| m.to == to && m.msg_id == msg_id) {
            duplicates += 1;
            continue;
          }
          s.next += 1;
          let cursor = s.next;
          s.msgs.push(FMsg { cursor, msg_id: msg_id.clone(), from: from.clone(), to, body: i["body"].as_str().unwrap().into() });
          stored += 1;
        }
        wrap(json!({ "stored": stored, "duplicates": duplicates }))
      }
      (Method::Get, ["messages"]) => {
        let me = caller.ok_or("401")?;
        let after: i64 = path.split("after=").nth(1).unwrap().split('&').next().unwrap().parse().unwrap();
        let mine: Vec<Value> = s
          .msgs
          .iter()
          .filter(|m| m.to == me && m.cursor > after)
          .map(|m| json!({ "cursor": m.cursor, "msg_id": m.msg_id, "from": m.from, "body": m.body }))
          .collect();
        wrap(json!({ "messages": mine, "more": false }))
      }
      (Method::Post, ["messages", "ack"]) => {
        let me = caller.ok_or("401")?;
        let through = body["through"].as_i64().unwrap();
        s.msgs.retain(|m| !(m.to == me && m.cursor <= through));
        wrap(json!({ "deleted": 0 }))
      }
      (Method::Post, ["revocations"]) => {
        caller.ok_or("401")?;
        let fp = body["target_fp"].as_str().unwrap().to_owned();
        if !s.revocations.iter().any(|r| r.0 == fp) {
          s.revocations.push((fp, body["statement"].as_str().unwrap().to_owned()));
        }
        Ok(String::new())
      }
      _ => Err(format!("fake: unhandled {} {}", req.method.as_str(), req.path)),
    }
  }
}

fn rand_n() -> u64 {
  static N: AtomicU64 = AtomicU64::new(1);
  N.fetch_add(1, Ordering::SeqCst)
}

struct Clock(Arc<AtomicU64>);

impl Clock {
  fn new(t: u64) -> Clock {
    Clock(Arc::new(AtomicU64::new(t)))
  }
  fn set(&self, t: u64) {
    self.0.store(t, Ordering::SeqCst);
  }
}

type Engine = RelayEngine<Fake, Arc<MemKeyStore>>;

impl KeyStore for Arc<MemKeyStore> {
  fn load(&self, slot: Slot) -> Result<Option<String>, String> {
    (**self).load(slot)
  }
  fn save(&self, slot: Slot, value: &str) -> Result<(), String> {
    (**self).save(slot, value)
  }
  fn clear(&self) -> Result<(), String> {
    (**self).clear()
  }
}

fn engine_with(fake: &Fake, clock: &Clock, label: &str) -> Engine {
  let c = clock.0.clone();
  RelayEngine::new(fake.clone(), Arc::new(MemKeyStore::default()), EngineConfig { user_id: USER, label: label.into(), local_machine: "mx_local".into() })
    .unwrap()
    .with_clock(move || c.load(Ordering::SeqCst))
}

fn room() -> RoomRef {
  RoomRef { kind: RoomKind::Manager, machine: "mx_remote".into(), session: None }
}

fn send(e: &Engine, text: &str) -> Result<ChatMessage, String> {
  e.send(SendParams { room: room(), body_md: text.into(), reply_to: None, attachments: vec![] })
}

fn history(e: &Engine) -> Vec<String> {
  e.history(&HistoryParams { room: room(), before: None, limit: None }).unwrap().into_iter().map(|m| m.body_md).collect()
}

fn my_id(e: &Engine) -> String {
  e.lock().identity.as_ref().unwrap().device_id.clone()
}

fn pair(fake: &Fake, clock: &Clock) -> (Engine, Engine) {
  let a = engine_with(fake, clock, "Desktop A");
  let b = engine_with(fake, clock, "Desktop B");
  let code = a.open_join_code().unwrap();
  b.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  let bid = my_id(&b);
  a.decide_join(&bid, true).unwrap();
  b.poll_once().unwrap();
  (a, b)
}

#[test]
fn message_round_trip_between_two_engines() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let sent = send(&a, "hello from A").unwrap();
  let got = b.poll_once().unwrap();
  assert_eq!(got.len(), 1);
  assert_eq!(got[0].id, sent.id);
  assert_eq!(history(&b), ["hello from A"]);
  assert_eq!(history(&a), ["hello from A"]);
  clock.set(T0 + 1000);
  send(&b, "reply from B").unwrap();
  assert_eq!(a.poll_once().unwrap().len(), 1);
  assert_eq!(history(&a), ["hello from A", "reply from B"]);
  assert_eq!(b.search(&SearchParams { query: "HELLO".into(), room: None, limit: None }).unwrap().len(), 1);
  assert_eq!(b.rooms().unwrap().len(), 1);
  assert!(b.poll_once().unwrap().is_empty());
}

#[test]
fn untrusted_sender_is_ignored() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  a.register().unwrap();
  let stranger = engine_with(&fake, &clock, "Stranger");
  stranger.register().unwrap();
  assert!(send(&stranger, "hi").is_err());
  {
    let mut g = stranger.lock();
    let a_dev = a.lock().st.trusted[0].clone();
    g.st.trusted.push(a_dev);
  }
  send(&stranger, "let me in").unwrap();
  assert!(a.poll_once().unwrap().is_empty());
  assert!(history(&a).is_empty());
  assert_eq!(fake.queue_len(&my_id(&a)), 0);
}

#[test]
fn join_needs_confirmation_on_existing_device() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let b = engine_with(&fake, &clock, "Desktop B");
  let code = a.open_join_code().unwrap();
  b.join_with_code(&code).unwrap();
  assert!(send(&b, "too early").is_err());
  a.poll_once().unwrap();
  let ts = a.trust_state().unwrap();
  assert_eq!(ts.pending_joins.len(), 1);
  assert_eq!(ts.pending_joins[0].label, "Desktop B");
  assert_eq!(ts.devices.len(), 1);
  assert!(send(&a, "before confirm").is_err());
  assert!(b.poll_once().unwrap().is_empty());
  assert!(send(&b, "still sealed off").is_err());
  let bid = my_id(&b);
  a.decide_join(&bid, true).unwrap();
  assert_eq!(a.trust_state().unwrap().devices.len(), 2);
  b.poll_once().unwrap();
  assert!(b.trust_state().unwrap().devices.iter().any(|d| d.is_self) );
  send(&b, "now allowed").unwrap();
  assert_eq!(a.poll_once().unwrap().len(), 1);
}

#[test]
fn declined_join_stays_untrusted_and_code_is_single_use() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let b = engine_with(&fake, &clock, "Desktop B");
  let c = engine_with(&fake, &clock, "Desktop C");
  let code = a.open_join_code().unwrap();
  b.join_with_code(&code).unwrap();
  c.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  let pending = a.trust_state().unwrap().pending_joins;
  assert_eq!(pending.len(), 1);
  a.decide_join(&pending[0].device_id, false).unwrap();
  assert!(a.trust_state().unwrap().pending_joins.is_empty());
  assert_eq!(a.trust_state().unwrap().devices.len(), 1);
  assert!(a.decide_join(&pending[0].device_id, true).is_err());
}

#[test]
fn expired_join_code_is_rejected() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let b = engine_with(&fake, &clock, "Desktop B");
  let code = a.open_join_code().unwrap();
  b.join_with_code(&code).unwrap();
  clock.set(T0 + 11 * 60 * 1000);
  a.poll_once().unwrap();
  assert!(a.trust_state().unwrap().pending_joins.is_empty());
}

#[test]
fn revoke_of_another_device_needs_confirmation_on_receiver() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let c = engine_with(&fake, &clock, "Desktop C");
  let code = a.open_join_code().unwrap();
  c.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  let cid = my_id(&c);
  a.decide_join(&cid, true).unwrap();
  c.poll_once().unwrap();
  b.poll_once().unwrap();
  assert_eq!(b.trust_state().unwrap().devices.len(), 3);

  a.revoke(&cid).unwrap();
  assert!(a.trust_state().unwrap().devices.iter().any(|d| d.id == cid && d.state == DeviceState::Revoked));
  assert!(send(&a, "after revoke").is_ok());

  b.poll_once().unwrap();
  let ts = b.trust_state().unwrap();
  assert_eq!(ts.pending_revokes.len(), 1);
  assert_eq!(ts.pending_revokes[0].target_id, cid);
  assert_eq!(ts.pending_revokes[0].revoker_label, "Desktop A");
  assert!(ts.devices.iter().any(|d| d.id == cid && d.state == DeviceState::Trusted));
  assert!(b.poll_once().unwrap().is_empty());
  b.poll_once().unwrap();
  assert_eq!(b.trust_state().unwrap().pending_revokes.len(), 1);

  b.decide_revoke(&ts.pending_revokes[0].statement_id, true).unwrap();
  let ts = b.trust_state().unwrap();
  assert!(ts.pending_revokes.is_empty());
  assert!(ts.devices.iter().any(|d| d.id == cid && d.state == DeviceState::Revoked));
  b.poll_once().unwrap();
  assert!(b.trust_state().unwrap().pending_revokes.is_empty());
}

#[test]
fn declined_revoke_is_not_asked_again() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let bid = my_id(&b);
  let c = engine_with(&fake, &clock, "Desktop C");
  let code = a.open_join_code().unwrap();
  c.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  let cid = my_id(&c);
  a.decide_join(&cid, true).unwrap();
  b.poll_once().unwrap();
  a.revoke(&cid).unwrap();
  b.poll_once().unwrap();
  let sid = b.trust_state().unwrap().pending_revokes[0].statement_id.clone();
  b.decide_revoke(&sid, false).unwrap();
  b.poll_once().unwrap();
  let ts = b.trust_state().unwrap();
  assert!(ts.pending_revokes.is_empty());
  assert!(ts.devices.iter().any(|d| d.id == cid && d.state == DeviceState::Trusted));
  let _ = bid;
}

#[test]
fn reset_identity_makes_others_untrusted() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  send(&a, "before reset").unwrap();
  b.poll_once().unwrap();
  let old = my_id(&a);
  a.reset_identity().unwrap();
  assert_ne!(my_id(&a), old);
  let ts = a.trust_state().unwrap();
  assert_eq!(ts.devices.len(), 1);
  assert!(ts.devices[0].is_self);
  assert!(history(&a).is_empty());
  assert!(send(&a, "alone").is_err());

  assert!(b.poll_once().unwrap().is_empty());
  let bts = b.trust_state().unwrap();
  let new_a = my_id(&a);
  assert!(bts.devices.iter().all(|d| d.id != new_a));
  let bid = my_id(&b);
  let b_dev = b.lock().st.trusted.iter().find(|d| d.id == bid).unwrap().clone();
  a.lock().st.trusted.push(b_dev);
  send(&a, "new identity").unwrap();
  assert!(b.poll_once().unwrap().is_empty());
  assert_eq!(history(&b), ["before reset"]);
}

#[test]
fn stale_and_replayed_messages_are_dropped() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let bid = my_id(&b);
  send(&a, "old news").unwrap();
  let _ = fake.first_body(&bid);
  clock.set(T0 + 32 * 24 * 3600 * 1000);
  assert!(b.poll_once().unwrap().is_empty());
  assert!(history(&b).is_empty());

  clock.set(T0 + 1000);
  send(&a, "fresh").unwrap();
  assert_eq!(b.poll_once().unwrap().len(), 1);
  assert_eq!(history(&b), ["fresh"]);
}

#[test]
fn replay_of_accepted_message_is_deduped() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let bid = my_id(&b);
  send(&a, "once").unwrap();
  let (msg_id, from, body) = fake.first_body(&bid);
  assert_eq!(b.poll_once().unwrap().len(), 1);
  fake.inject(&bid, &msg_id, &from, &body);
  assert!(b.poll_once().unwrap().is_empty());
  assert_eq!(history(&b), ["once"]);
}

#[test]
fn directory_key_mismatch_is_reported_and_pinned_keys_are_kept() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let aid = my_id(&a);
  let evil = DeviceKeys::generate().unwrap();
  {
    let mut s = fake.0.lock().unwrap();
    let d = s.devices.iter_mut().find(|d| d.id == aid).unwrap();
    d.sign_pub = evil.sign_pub_b64();
    d.kex_pub = evil.kex_pub_b64();
  }
  b.poll_once().unwrap();
  assert_eq!(b.trust_state().unwrap().key_mismatch, [aid.clone()]);
  let pinned = b.lock().st.trusted.iter().find(|d| d.id == aid).unwrap().fp.clone();
  assert_eq!(pinned, a.trust_state().unwrap().self_fp.unwrap());
}

#[test]
fn state_survives_a_restart() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let store = Arc::new(MemKeyStore::default());
  let cfg = || EngineConfig { user_id: USER, label: "Desktop A".into(), local_machine: "mx_local".into() };
  let a = RelayEngine::new(fake.clone(), store.clone(), cfg()).unwrap();
  a.register().unwrap();
  let fp = a.trust_state().unwrap().self_fp;
  let again = RelayEngine::new(fake.clone(), store, cfg()).unwrap();
  let ts = again.trust_state().unwrap();
  assert!(ts.registered);
  assert_eq!(ts.self_fp, fp);
  let _ = clock;
}

#[test]
fn chat_validation_rejects_bad_ids_and_rooms() {
  let mut m = ChatMessage {
    v: 1,
    id: format!("m_{}", "a".repeat(32)),
    room: room(),
    author: Author { kind: "owner".into(), id: "o".into(), machine: None, agent: None },
    ts_ms: 1,
    kind: "text".into(),
    body_md: "x".into(),
    attachments: vec![],
    reply_to: None,
    decision: None,
  };
  assert!(validate_chat(&m).is_ok());
  m.id = "m_short".into();
  assert!(validate_chat(&m).is_err());
  m.id = format!("m_{}", "a".repeat(32));
  m.room.session = Some("s".into());
  assert!(validate_chat(&m).is_err());
}
