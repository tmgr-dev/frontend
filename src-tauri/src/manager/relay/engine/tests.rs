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
  revocations: Vec<(String, String, String)>,
  next: i64,
  post_error: Option<String>,
  dir_error: bool,
  posts: usize,
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

  fn posts(&self) -> usize {
    self.0.lock().unwrap().posts
  }

  fn set_post_error(&self, e: Option<&str>) {
    self.0.lock().unwrap().post_error = e.map(str::to_owned);
  }

  fn set_dir_error(&self, on: bool) {
    self.0.lock().unwrap().dir_error = on;
  }

  fn add_revocation(&self, fp: &str, by: &str, statement: &str) {
    self.0.lock().unwrap().revocations.push((fp.into(), by.into(), statement.into()));
  }

  fn reverse_queue(&self, to: &str) {
    let mut s = self.0.lock().unwrap();
    let mut cursors: Vec<i64> = s.msgs.iter().filter(|m| m.to == to).map(|m| m.cursor).collect();
    cursors.reverse();
    for (m, c) in s.msgs.iter_mut().filter(|m| m.to == to).zip(cursors) {
      m.cursor = c;
    }
    s.msgs.sort_by_key(|m| m.cursor);
  }

  fn drop_nth(&self, to: &str, n: usize) {
    let mut s = self.0.lock().unwrap();
    let idx = s.msgs.iter().enumerate().filter(|(_, m)| m.to == to).map(|(i, _)| i).nth(n).unwrap();
    s.msgs.remove(idx);
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
        if s.dir_error {
          return Err("directory down".into());
        }
        let devices: Vec<Value> = s
          .devices
          .iter()
          .map(|d| json!({ "device_id": d.id, "kind": d.kind, "label": d.label, "sign_pub": d.sign_pub, "kex_pub": d.kex_pub, "intros": d.intros.iter().map(|i| i.1.clone()).collect::<Vec<_>>() }))
          .collect();
        let revs: Vec<Value> = s.revocations.iter().map(|r| json!({ "target_fp": r.0, "statement": r.2 })).collect();
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
        s.posts += 1;
        if let Some(e) = s.post_error.clone() {
          return Err(e);
        }
        let msg_id = body["msg_id"].as_str().unwrap().to_owned();
        let items = body["items"].as_array().unwrap().clone();
        if items.iter().any(|i| !s.devices.iter().any(|d| d.id == i["to"].as_str().unwrap())) {
          return Err(r#"{"error":"recipient_unknown"}"#.into());
        }
        let (mut stored, mut duplicates) = (0, 0);
        for i in items {
          let to = i["to"].as_str().unwrap().to_owned();
          if s.msgs.iter().any(|m| m.to == to && m.from == from && m.msg_id == msg_id) {
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
        let by = caller.ok_or("401")?;
        let fp = body["target_fp"].as_str().unwrap().to_owned();
        if !s.revocations.iter().any(|r| r.0 == fp && r.1 == by) {
          s.revocations.push((fp, by, body["statement"].as_str().unwrap().to_owned()));
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

fn send<K: KeyStore>(e: &RelayEngine<Fake, K>, text: &str) -> Result<ChatMessage, String> {
  e.send(SendParams { room: room(), body_md: text.into(), reply_to: None, attachments: vec![], id: None, ts_ms: None })
}

fn history<K: KeyStore>(e: &RelayEngine<Fake, K>) -> Vec<String> {
  e.history(&HistoryParams { room: room(), before: None, limit: None }).unwrap().into_iter().map(|m| m.body_md).collect()
}

fn my_id<K: KeyStore>(e: &RelayEngine<Fake, K>) -> String {
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
  assert!(b.poll_once().unwrap().is_empty());
  clock.set(T0 + 62_000);
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
  assert_eq!(b.trust_state().unwrap().key_mismatch, std::slice::from_ref(&aid));
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

fn trio(fake: &Fake, clock: &Clock) -> (Engine, Engine, Engine) {
  let (a, b) = pair(fake, clock);
  let c = engine_with(fake, clock, "Desktop C");
  let code = a.open_join_code().unwrap();
  c.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  a.decide_join(&my_id(&c), true).unwrap();
  c.poll_once().unwrap();
  b.poll_once().unwrap();
  (a, b, c)
}

fn assert_der_low_s(sig: &str) {
  let raw = tmgr_phone::b64_decode(sig).unwrap();
  assert_eq!(tmgr_phone::b64(&raw), sig);
  assert_ne!(raw.len(), 64);
  let parsed = p256::ecdsa::Signature::from_der(&raw).unwrap();
  assert!(parsed.normalize_s().is_none());
}

#[test]
fn emitted_signatures_are_padded_base64_der_low_s() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, _b, c) = trio(&fake, &clock);
  a.revoke(&my_id(&c)).unwrap();
  let s = fake.0.lock().unwrap();
  assert!(!s.revocations.is_empty());
  for (_, _, json) in &s.revocations {
    assert_der_low_s(serde_json::from_str::<Value>(json).unwrap()["sig"].as_str().unwrap());
  }
  let intros: Vec<&String> = s.devices.iter().flat_map(|d| d.intros.iter().map(|i| &i.1)).collect();
  assert!(!intros.is_empty());
  for i in intros {
    assert_der_low_s(serde_json::from_str::<Value>(i).unwrap()["sig"].as_str().unwrap());
  }
}

#[test]
fn directory_revocations_are_verified_one_by_one() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b, c) = trio(&fake, &clock);
  let cid = my_id(&c);
  let c_fp = c.lock().keys.as_ref().unwrap().fingerprint();
  let (aid, bid) = (my_id(&a), my_id(&b));
  fake.add_revocation(&c_fp, "00000000-0000-4000-8000-0000000000ff", "junk");
  a.revoke(&cid).unwrap();
  let real = fake.0.lock().unwrap().revocations.last().unwrap().2.clone();
  fake.add_revocation(&tmgr_phone::h("other"), &aid, &real);
  fake.0.lock().unwrap().msgs.retain(|m| m.to != bid);
  b.poll_once().unwrap();
  let ts = b.trust_state().unwrap();
  assert_eq!(ts.pending_revokes.len(), 1);
  assert_eq!(ts.pending_revokes[0].target_id, cid);
}

#[test]
fn at_most_fifty_directory_revocations_per_revoking_device_are_considered() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let aid = my_id(&a);
  {
    let g = a.lock();
    let keys = g.keys.as_ref().unwrap();
    for i in 0..51 {
      let fp = tmgr_phone::h(&format!("target {i}"));
      let st = RevokeStatement {
        user_id: USER,
        revoker_fp: keys.fingerprint(),
        target_id: format!("00000000-0000-4000-8000-{i:012x}"),
        target_fp: fp.clone(),
        issued_at_ms: T0,
      };
      fake.add_revocation(&fp, &aid, &sign_revoke(keys, &st).unwrap());
    }
  }
  b.poll_once().unwrap();
  assert_eq!(b.trust_state().unwrap().pending_revokes.len(), 50);
}

fn same_id_msg() -> ChatMessage {
  ChatMessage {
    v: 1,
    id: format!("m_{}", "1".repeat(32)),
    room: room(),
    author: Author { kind: "owner".into(), id: "owner".into(), machine: None, agent: None },
    ts_ms: T0,
    kind: "text".into(),
    body_md: "same id".into(),
    attachments: vec![],
    reply_to: None,
    decision: None,
  }
}

#[test]
fn bad_recipient_id_is_rejected_before_any_request() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, _b) = pair(&fake, &clock);
  {
    let mut g = a.lock();
    let mut bad = g.st.trusted.iter().find(|d| d.id != my_id_of(&g)).unwrap().clone();
    bad.id = "not-a-uuid".into();
    g.st.trusted.push(bad);
  }
  let posts = fake.posts();
  assert_eq!(send(&a, "x").unwrap_err(), "invalid recipient device id");
  assert_eq!(fake.posts(), posts);
  assert!(history(&a).is_empty());
}

fn my_id_of(c: &Core) -> String {
  c.identity.as_ref().unwrap().device_id.clone()
}

#[test]
fn full_user_storage_stops_with_a_clear_error() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  fake.set_post_error(Some(r#"{"error":"user_full"}"#));
  let posts = fake.posts();
  assert_eq!(send(&a, "x").unwrap_err(), "relay storage is full");
  assert_eq!(fake.posts(), posts + 1);
  assert!(history(&a).is_empty());
  assert!(a.lock().st.seq_out.get(&my_id(&b)).is_none_or(|s| *s == 1));
}

#[test]
fn rate_limited_post_reports_retry_after_without_looping() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, _b) = pair(&fake, &clock);
  fake.set_post_error(Some(r#"{"error":"rate_limited","retry_after":7}"#));
  let posts = fake.posts();
  let e = send(&a, "x").unwrap_err();
  assert_eq!(retry_after_secs(&e), Some(7));
  assert_eq!(fake.posts(), posts + 1);
  assert!(history(&a).is_empty());
  fake.set_post_error(None);
  send(&a, "x").unwrap();
  assert_eq!(history(&a), ["x"]);
}

#[test]
fn dedupe_is_per_sender_and_message_id() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b, c) = trio(&fake, &clock);
  let bid = my_id(&b);
  let msg = same_id_msg();
  let payload = serde_json::to_value(&msg).unwrap();
  for e in [&a, &c] {
    let mut g = e.lock();
    let target: Vec<Dev> = g.st.trusted.iter().filter(|d| d.id == bid).cloned().collect();
    e.seal_and_post(&mut g, "chat", &msg.id, &payload, &target, false).unwrap();
  }
  assert_eq!(fake.queue_len(&bid), 2);
  b.poll_once().unwrap();
  assert_eq!(b.lock().st.seen.iter().filter(|s| s.id == msg.id).count(), 2);
  assert_eq!(b.lock().log.iter().filter(|e| e.msg.id == msg.id).count(), 2);
  assert_eq!(history(&b), ["same id", "same id"]);
  fake.reverse_queue(&bid);
  for e in [&a, &c] {
    let mut g = e.lock();
    let target: Vec<Dev> = g.st.trusted.iter().filter(|d| d.id == bid).cloned().collect();
    e.seal_and_post(&mut g, "chat", &msg.id, &payload, &target, false).unwrap();
  }
  b.poll_once().unwrap();
  assert_eq!(history(&b).len(), 2);
}

fn engine_on(fake: &Fake, clock: &Clock, store: Arc<MemKeyStore>, local: &str) -> Engine {
  let c = clock.0.clone();
  RelayEngine::new(fake.clone(), store, EngineConfig { user_id: USER, label: "Desktop A".into(), local_machine: local.into() })
    .unwrap()
    .with_clock(move || c.load(Ordering::SeqCst))
}

fn fp_of(e: &Engine) -> String {
  e.lock().keys.as_ref().unwrap().fingerprint()
}

fn post_chat(from: &Engine, to_id: &str, msg: &ChatMessage) {
  let payload = serde_json::to_value(msg).unwrap();
  let mut g = from.lock();
  let target: Vec<Dev> = g.st.trusted.iter().filter(|d| d.id == to_id).cloned().collect();
  from.seal_and_post(&mut g, "chat", &msg.id, &payload, &target, false).unwrap();
}

#[test]
fn revoke_after_restart_without_directory_keeps_devices_trusted_through_intros() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let store = Arc::new(MemKeyStore::default());
  let a = engine_on(&fake, &clock, store.clone(), "mx_local");
  let b = engine_with(&fake, &clock, "Desktop B");
  let code = a.open_join_code().unwrap();
  b.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  a.decide_join(&my_id(&b), true).unwrap();
  b.poll_once().unwrap();
  let c = engine_with(&fake, &clock, "Desktop C");
  let code = b.open_join_code().unwrap();
  c.join_with_code(&code).unwrap();
  b.poll_once().unwrap();
  b.decide_join(&my_id(&c), true).unwrap();
  c.poll_once().unwrap();
  a.poll_once().unwrap();
  let cid = my_id(&c);
  assert!(a.trust_state().unwrap().devices.iter().any(|d| d.id == cid && d.state == DeviceState::Trusted));
  let intros_before = a.lock().st.intros.len();
  assert!(intros_before > 0);

  let a2 = engine_on(&fake, &clock, store, "mx_local");
  {
    let mut g = a2.lock();
    assert!(g.dir.is_none());
    apply_revoke(&mut g, USER, "00000000-0000-4000-8000-0000000000dd", &tmgr_phone::h("device d"), "D");
  }
  let ts = a2.trust_state().unwrap();
  assert!(ts.devices.iter().any(|d| d.id == cid && d.state == DeviceState::Trusted));
  assert_eq!(a2.lock().st.intros.len(), intros_before);
}

#[test]
fn revoked_fingerprint_cannot_join_again() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let b = engine_with(&fake, &clock, "Desktop B");
  let code = a.open_join_code().unwrap();
  b.join_with_code(&code).unwrap();
  a.lock().st.revoked.push(fp_of(&b));
  a.poll_once().unwrap();
  assert!(a.trust_state().unwrap().pending_joins.is_empty());

  let c = engine_with(&fake, &clock, "Desktop C");
  let code = a.open_join_code().unwrap();
  c.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  let cid = my_id(&c);
  assert_eq!(a.trust_state().unwrap().pending_joins.len(), 1);
  a.lock().st.revoked.push(fp_of(&c));
  assert!(a.decide_join(&cid, true).is_err());
  assert!(a.trust_state().unwrap().devices.iter().all(|d| d.id != cid || d.state != DeviceState::Trusted));
}

#[test]
fn joiner_ignores_chat_before_join_ok_and_honours_its_revoked_list() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let bid = my_id(&b);
  let c = engine_with(&fake, &clock, "Desktop C");
  let code = a.open_join_code().unwrap();
  c.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  {
    let mut g = a.lock();
    apply_revoke(&mut g, USER, &bid, &fp_of(&b), "Desktop B");
  }
  let cid = my_id(&c);
  b.lock().st.trusted.push(c.lock().st.trusted.iter().find(|d| d.id == cid).unwrap().clone());
  post_chat(&b, &cid, &same_id_msg());
  a.decide_join(&cid, true).unwrap();
  assert!(c.poll_once().unwrap().is_empty());
  assert!(history(&c).is_empty());
  assert!(c.trust_state().unwrap().devices.iter().all(|d| d.id != bid || d.state != DeviceState::Trusted));
}

#[test]
fn failed_join_post_can_be_retried() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let b = engine_with(&fake, &clock, "Desktop B");
  let code = a.open_join_code().unwrap();
  fake.set_post_error(Some(r#"{"error":"recipient_full"}"#));
  assert!(b.join_with_code(&code).is_err());
  assert_eq!(b.trust_state().unwrap().devices.len(), 1);
  fake.set_post_error(None);
  b.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  assert_eq!(a.trust_state().unwrap().pending_joins.len(), 1);
}

fn set_kind(fake: &Fake, id: &str, kind: &str) {
  fake.0.lock().unwrap().devices.iter_mut().find(|d| d.id == id).unwrap().kind = kind.into();
}

fn msg_n(n: u8, kind: &str) -> ChatMessage {
  let mut m = same_id_msg();
  m.id = format!("m_{}", n.to_string().repeat(32).chars().take(32).collect::<String>());
  m.author.kind = kind.into();
  m
}

fn join_as(fake: &Fake, clock: &Clock, a: &Engine, label: &str, kind: &str) -> Engine {
  let n = engine_with(fake, clock, label).with_kind(kind);
  let code = a.open_join_code().unwrap();
  n.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  let id = my_id(&n);
  a.decide_join(&id, true).unwrap();
  n.poll_once().unwrap();
  a.poll_once().unwrap();
  n
}

fn alert_kinds(e: &Engine) -> Vec<String> {
  e.trust_state().unwrap().alerts.into_iter().map(|a| a.kind).collect()
}

#[test]
fn non_runner_cannot_author_manager_session_or_system_messages() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let bid = my_id(&b);
  for (n, k) in [(2, "manager"), (3, "session"), (4, "system")] {
    post_chat(&a, &bid, &msg_n(n, k));
  }
  assert!(b.poll_once().unwrap().is_empty());
  post_chat(&a, &bid, &msg_n(5, "owner"));
  assert_eq!(b.poll_once().unwrap().len(), 1);
}

#[test]
fn runner_cannot_author_owner_messages_but_may_author_manager_messages() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let r = join_as(&fake, &clock, &a, "Runner", "runner");
  let aid = my_id(&a);
  post_chat(&r, &aid, &msg_n(2, "owner"));
  assert!(a.poll_once().unwrap().is_empty());
  post_chat(&r, &aid, &msg_n(3, "manager"));
  assert_eq!(a.poll_once().unwrap().len(), 1);
}

#[test]
fn second_runner_claiming_a_bound_machine_is_dropped_and_alerted() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let r1 = join_as(&fake, &clock, &a, "Runner 1", "runner");
  let r2 = join_as(&fake, &clock, &a, "Runner 2", "runner");
  let aid = my_id(&a);
  post_chat(&r1, &aid, &msg_n(2, "manager"));
  assert_eq!(a.poll_once().unwrap().len(), 1);
  post_chat(&r2, &aid, &msg_n(3, "manager"));
  assert!(a.poll_once().unwrap().is_empty());
  assert_eq!(alert_kinds(&a), vec!["room_conflict".to_owned()]);
  post_chat(&r1, &aid, &msg_n(4, "session"));
  assert_eq!(a.poll_once().unwrap().len(), 1);
}

#[test]
fn runner_machine_binding_survives_a_restart() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let store = Arc::new(MemKeyStore::default());
  let a = engine_on(&fake, &clock, store.clone(), "mx_local");
  let r1 = join_as(&fake, &clock, &a, "Runner 1", "runner");
  let r2 = join_as(&fake, &clock, &a, "Runner 2", "runner");
  let aid = my_id(&a);
  post_chat(&r1, &aid, &msg_n(2, "manager"));
  assert_eq!(a.poll_once().unwrap().len(), 1);
  clock.set(T0 + 31_000);
  a.poll_once().unwrap();
  let again = engine_on(&fake, &clock, store, "mx_local");
  post_chat(&r2, &aid, &msg_n(3, "manager"));
  assert!(again.poll_once().unwrap().is_empty());
  assert_eq!(alert_kinds(&again), vec!["room_conflict".to_owned()]);
}

#[test]
fn owns_is_false_for_the_local_machine_and_when_it_is_unknown() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  assert!(a.owns(&room()));
  assert!(!a.owns(&RoomRef { kind: RoomKind::Manager, machine: "mx_local".into(), session: None }));
  let blank = engine_on(&fake, &clock, Arc::new(MemKeyStore::default()), "");
  assert!(!blank.owns(&room()));
  assert!(!blank.owns(&RoomRef { kind: RoomKind::Manager, machine: String::new(), session: None }));
  assert_eq!(send(&blank, "x").unwrap_err(), "unknown room");
}

#[test]
fn transient_errors_keep_the_pending_join_and_revoke() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let b = engine_with(&fake, &clock, "Desktop B");
  let code = a.open_join_code().unwrap();
  b.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  let bid = my_id(&b);
  assert_eq!(a.trust_state().unwrap().pending_joins[0].kind.as_deref(), Some("desktop"));
  fake.set_post_error(Some(r#"{"error":"rate_limited","retry_after":3}"#));
  assert!(a.decide_join(&bid, true).is_err());
  assert_eq!(a.trust_state().unwrap().pending_joins.len(), 1);
  fake.set_post_error(None);
  a.decide_join(&bid, true).unwrap();
  assert!(a.trust_state().unwrap().pending_joins.is_empty());
  b.poll_once().unwrap();
  assert!(b.trust_state().unwrap().devices.iter().any(|d| d.id == bid));

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
  fake.set_dir_error(true);
  assert!(b.decide_revoke(&sid, true).is_err());
  assert_eq!(b.trust_state().unwrap().pending_revokes.len(), 1);
  fake.set_dir_error(false);
  b.decide_revoke(&sid, true).unwrap();
  assert!(b.trust_state().unwrap().pending_revokes.is_empty());
}

fn send_with(e: &Engine, id: &str, ts: u64, text: &str) -> Result<ChatMessage, String> {
  e.send(SendParams { room: room(), body_md: text.into(), reply_to: None, attachments: vec![], id: Some(id.into()), ts_ms: Some(ts) })
}

#[test]
fn send_with_an_id_is_idempotent_and_conflicts_on_different_content() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let id = format!("m_{}", "7".repeat(32));
  let first = send_with(&a, &id, T0 + 5, "once").unwrap();
  assert_eq!((first.id.as_str(), first.ts_ms), (id.as_str(), T0 + 5));
  let posts = fake.posts();
  let again = send_with(&a, &id, T0 + 99, "once").unwrap();
  assert_eq!((again.id, again.ts_ms), (first.id, first.ts_ms));
  assert_eq!(fake.posts(), posts);
  assert_eq!(send_with(&a, &id, T0 + 5, "other").unwrap_err(), "conflict");
  assert_eq!(history(&a), ["once"]);
  assert_eq!(b.poll_once().unwrap().len(), 1);
}

fn counted(fake: &Fake, clock: &Clock, label: &str) -> (RelayEngine<Fake, Arc<CountingStore>>, Arc<CountingStore>) {
  let store = Arc::new(CountingStore::default());
  let c = clock.0.clone();
  let e = RelayEngine::new(fake.clone(), store.clone(), EngineConfig { user_id: USER, label: label.into(), local_machine: "mx_local".into() })
    .unwrap()
    .with_clock(move || c.load(Ordering::SeqCst));
  (e, store)
}

#[derive(Default)]
struct CountingStore {
  inner: MemKeyStore,
  saves: Mutex<Vec<Slot>>,
}

impl CountingStore {
  fn count(&self, slot: Slot) -> usize {
    self.saves.lock().unwrap().iter().filter(|s| **s == slot).count()
  }
}

impl KeyStore for Arc<CountingStore> {
  fn load(&self, slot: Slot) -> Result<Option<String>, String> {
    self.inner.load(slot)
  }
  fn save(&self, slot: Slot, value: &str) -> Result<(), String> {
    self.saves.lock().unwrap().push(slot);
    self.inner.save(slot, value)
  }
  fn clear(&self) -> Result<(), String> {
    self.inner.clear()
  }
}

fn raw_sig(der_b64: &str) -> String {
  let der = tmgr_phone::b64_decode(der_b64).unwrap();
  tmgr_phone::b64(&p256::ecdsa::Signature::from_der(&der).unwrap().to_bytes())
}

fn with_sig(json: &str, sig: &str) -> String {
  let mut v: Value = serde_json::from_str(json).unwrap();
  v["sig"] = json!(sig);
  v.to_string()
}

#[test]
fn raw_r_s_signature_on_a_directory_revoke_is_rejected() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b, c) = trio(&fake, &clock);
  let (aid, cid) = (my_id(&a), my_id(&c));
  let st = {
    let g = a.lock();
    let target = g.st.trusted.iter().find(|d| d.id == cid).unwrap().clone();
    RevokeStatement { user_id: USER, revoker_fp: g.keys.as_ref().unwrap().fingerprint(), target_id: cid.clone(), target_fp: target.fp, issued_at_ms: T0 }
  };
  let der = sign_revoke(a.lock().keys.as_ref().unwrap(), &st).unwrap();
  let sig = serde_json::from_str::<Value>(&der).unwrap()["sig"].as_str().unwrap().to_owned();
  fake.add_revocation(&st.target_fp, &aid, &with_sig(&der, &raw_sig(&sig)));
  b.poll_once().unwrap();
  assert!(b.trust_state().unwrap().pending_revokes.is_empty());
  fake.0.lock().unwrap().revocations.clear();
  fake.add_revocation(&st.target_fp, &aid, &der);
  b.poll_once().unwrap();
  assert_eq!(b.trust_state().unwrap().pending_revokes.len(), 1);
}

#[test]
fn raw_r_s_signature_on_an_intro_is_rejected() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let c = engine_with(&fake, &clock, "Desktop C");
  c.register().unwrap();
  let cid = my_id(&c);
  let (aid, c_dev) = (my_id(&a), c.lock().st.trusted[0].clone());
  let st = IntroStatement {
    user_id: USER,
    introducer_fp: fp_of(&a),
    target_id: cid.clone(),
    target_fp: c_dev.fp.clone(),
    target_kind: "desktop".into(),
    issued_at_ms: T0,
  };
  let der = sign_intro(a.lock().keys.as_ref().unwrap(), &aid, &st).unwrap();
  let sig = serde_json::from_str::<Value>(&der).unwrap()["sig"].as_str().unwrap().to_owned();
  let put = |intro: &str| {
    let mut s = fake.0.lock().unwrap();
    let d = s.devices.iter_mut().find(|d| d.id == cid).unwrap();
    d.intros = vec![(aid.clone(), intro.to_owned())];
  };
  put(&with_sig(&der, &raw_sig(&sig)));
  b.poll_once().unwrap();
  assert!(b.trust_state().unwrap().devices.iter().all(|d| d.id != cid));
  put(&der);
  b.poll_once().unwrap();
  assert!(b.trust_state().unwrap().devices.iter().any(|d| d.id == cid && d.state == DeviceState::Trusted));
}

fn forge_chat(from: &Engine, to: &Engine, msg: &ChatMessage, raw: bool) -> (String, String) {
  let (fid, tid) = (my_id(from), my_id(to));
  let g = from.lock();
  let keys = g.keys.as_ref().unwrap();
  let to_dev = g.st.trusted.iter().find(|d| d.id == tid).unwrap().clone();
  let inner = json!({ "v": 1, "kind": "chat", "id": msg.id, "sent_at": T0, "seq": 2, "payload": serde_json::to_value(msg).unwrap() }).to_string();
  let signed = tmgr_chat_relay::wire::signed_string(USER, &keys.fingerprint(), &to_dev.fp, &fid, &tid, "chat", &msg.id, T0, &inner).unwrap();
  let mut sig = tmgr_phone::sign(keys.sign_secret(), &signed).unwrap();
  if raw {
    sig = raw_sig(&sig);
  }
  let plain = json!({ "body": inner, "sig": sig }).to_string();
  let ctx = tmgr_chat_relay::wire::chat_ctx(USER, &fid, &tid, "chat", &msg.id).unwrap();
  let ent = tmgr_phone::Entropy { eph_secret: [9u8; 32], nonce: [3u8; 12] };
  let s = tmgr_phone::seal_ctx_with(&tmgr_phone::b64_decode(&to_dev.kex_pub).unwrap(), &ctx, "chat", &msg.id, plain.as_bytes(), &ent).unwrap();
  let body = json!({ "v": 1, "kind": s.kind, "id": s.id, "from": fid, "eph": s.eph, "nonce": s.nonce, "ct": s.ct }).to_string();
  (fid, body)
}

#[test]
fn raw_r_s_signature_on_a_message_is_rejected() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let bid = my_id(&b);
  let msg = same_id_msg();
  let (from, body) = forge_chat(&a, &b, &msg, true);
  fake.inject(&bid, &msg.id, &from, &body);
  assert!(b.poll_once().unwrap().is_empty());
  assert!(history(&b).is_empty());
  let mut ok = msg.clone();
  ok.id = format!("m_{}", "2".repeat(32));
  let (from, body) = forge_chat(&a, &b, &ok, false);
  fake.inject(&bid, &ok.id, &from, &body);
  assert_eq!(b.poll_once().unwrap().len(), 1);
}

fn three_chats(fake: &Fake, a: &Engine, b: &Engine) -> String {
  let bid = my_id(b);
  for t in ["one", "two", "three"] {
    send(a, t).unwrap();
  }
  assert_eq!(fake.queue_len(&bid), 3);
  bid
}

#[test]
fn out_of_order_messages_are_processed_in_seq_order_without_gaps() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let bid = three_chats(&fake, &a, &b);
  fake.reverse_queue(&bid);
  assert_eq!(b.poll_once().unwrap().len(), 3);
  assert!(b.trust_state().unwrap().seq_gaps.is_empty());
  let seqs: Vec<u64> = b.lock().st.seq_in.values().copied().collect();
  assert_eq!(seqs, [4]);
}

#[test]
fn a_missing_message_is_held_briefly_then_reported_as_a_gap() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let bid = three_chats(&fake, &a, &b);
  fake.drop_nth(&bid, 1);
  assert_eq!(b.poll_once().unwrap().len(), 1);
  assert!(b.trust_state().unwrap().seq_gaps.is_empty());
  assert_eq!(fake.queue_len(&bid), 2);
  clock.set(T0 + 30_000);
  assert!(b.poll_once().unwrap().is_empty());
  clock.set(T0 + 61_000);
  assert_eq!(b.poll_once().unwrap().len(), 1);
  let mut shown = history(&b);
  shown.sort();
  assert_eq!(shown, ["one", "three"]);
  let gaps = b.trust_state().unwrap().seq_gaps;
  assert_eq!(gaps, [SeqGap { from_device: my_id(&a), expected: 3, got: 4 }]);
  assert_eq!(fake.queue_len(&bid), 0);
}

#[test]
fn reset_identity_tells_trusted_devices_to_drop_the_old_identity() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let (old_id, old_fp) = (my_id(&a), fp_of(&a));
  let posts = fake.posts();
  a.reset_identity().unwrap();
  assert_eq!(fake.posts(), posts + 1);
  b.poll_once().unwrap();
  let ts = b.trust_state().unwrap();
  assert!(ts.pending_revokes.is_empty());
  assert!(ts.devices.iter().any(|d| d.id == old_id && d.state == DeviceState::Revoked));
  assert!(b.lock().st.revoked.contains(&old_fp));
}

#[test]
fn reset_identity_wipes_even_when_the_revoke_cannot_be_sent() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, _b) = pair(&fake, &clock);
  let old = my_id(&a);
  fake.set_post_error(Some("boom"));
  let e = a.reset_identity().unwrap_err();
  assert!(e.contains("not told"), "{e}");
  assert_ne!(my_id(&a), old);
  assert_eq!(a.trust_state().unwrap().devices.len(), 1);
}

#[test]
fn polls_do_not_rewrite_the_log_more_than_once_per_30_seconds() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, _) = pair(&fake, &clock);
  let (b, store) = counted(&fake, &clock, "Desktop B");
  let code = a.open_join_code().unwrap();
  b.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  a.decide_join(&my_id(&b), true).unwrap();
  b.poll_once().unwrap();
  let (logs, states) = (store.count(Slot::Log), store.count(Slot::State));
  let bid = my_id(&b);
  for i in 1..=5u64 {
    clock.set(T0 + i * 1000);
    send(&a, &format!("m{i}")).unwrap();
    assert_eq!(b.poll_once().unwrap().len(), 1);
  }
  assert_eq!(store.count(Slot::Log), logs);
  assert_eq!(store.count(Slot::State), states);
  assert_eq!(fake.queue_len(&bid), 5);
  clock.set(T0 + 31_000);
  b.poll_once().unwrap();
  assert_eq!(store.count(Slot::Log), logs + 1);
  assert_eq!(fake.queue_len(&bid), 0);
  b.poll_once().unwrap();
  assert_eq!(store.count(Slot::Log), logs + 1);
}

#[test]
fn unwritten_messages_stay_on_the_server_until_stored() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, _) = pair(&fake, &clock);
  let (b, store) = counted(&fake, &clock, "Desktop B");
  let code = a.open_join_code().unwrap();
  b.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  a.decide_join(&my_id(&b), true).unwrap();
  b.poll_once().unwrap();
  clock.set(T0 + 1000);
  send(&a, "pending write").unwrap();
  assert_eq!(b.poll_once().unwrap().len(), 1);
  let restarted = RelayEngine::new(fake.clone(), store, EngineConfig { user_id: USER, label: "Desktop B".into(), local_machine: "mx_local".into() })
    .unwrap()
    .with_clock({
      let c = clock.0.clone();
      move || c.load(Ordering::SeqCst)
    });
  assert_eq!(restarted.poll_once().unwrap().len(), 1);
  assert_eq!(history(&restarted), ["pending write"]);
}

#[test]
fn seen_is_capped_at_5000_dropping_the_oldest() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  {
    let mut g = b.lock();
    for i in 0..5100 {
      g.st.seen.push(Seen { from: "x".into(), id: format!("old{i}"), at: T0 });
    }
  }
  send(&a, "fresh").unwrap();
  b.poll_once().unwrap();
  let g = b.lock();
  assert_eq!(g.st.seen.len(), 5000);
  assert!(g.st.seen.last().unwrap().id.starts_with("m_"));
  assert!(g.st.seen.iter().all(|s| s.id != "old0"));
}

const DAY: u64 = 24 * 3600 * 1000;

fn silence_setup() -> (Fake, Clock, Engine, Engine, Engine) {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b, c) = trio(&fake, &clock);
  (fake, clock, a, b, c)
}

fn is_revoked(e: &Engine, id: &str) -> bool {
  e.trust_state().unwrap().devices.iter().any(|d| d.id == id && d.state == DeviceState::Revoked)
}

#[test]
fn revoke_of_a_device_unseen_for_31_days_applies_automatically() {
  let (_f, clock, a, b, c) = silence_setup();
  let cid = my_id(&c);
  send(&c, "hello").unwrap();
  b.poll_once().unwrap();
  clock.set(T0 + 31 * DAY);
  a.revoke(&cid).unwrap();
  b.poll_once().unwrap();
  assert!(is_revoked(&b, &cid));
  assert!(b.trust_state().unwrap().pending_revokes.is_empty());
}

#[test]
fn revoke_of_a_device_seen_29_days_ago_stays_pending() {
  let (_f, clock, a, b, c) = silence_setup();
  let cid = my_id(&c);
  send(&c, "hello").unwrap();
  b.poll_once().unwrap();
  clock.set(T0 + 29 * DAY);
  a.revoke(&cid).unwrap();
  b.poll_once().unwrap();
  let ts = b.trust_state().unwrap();
  assert_eq!(ts.pending_revokes.len(), 1);
  assert!(!is_revoked(&b, &cid));
}

#[test]
fn revoke_of_a_never_seen_device_pinned_31_days_ago_applies_automatically() {
  let (_f, clock, a, b, c) = silence_setup();
  let cid = my_id(&c);
  clock.set(T0 + 31 * DAY);
  a.revoke(&cid).unwrap();
  b.poll_once().unwrap();
  assert!(is_revoked(&b, &cid));
}

#[test]
fn self_revoke_still_applies_automatically() {
  let (_f, _clock, a, b, c) = silence_setup();
  let cid = my_id(&c);
  c.reset_identity().unwrap();
  a.poll_once().unwrap();
  b.poll_once().unwrap();
  assert!(!a.trust_state().unwrap().devices.iter().any(|d| d.id == cid && d.state == DeviceState::Trusted));
  assert!(a.trust_state().unwrap().pending_revokes.is_empty());
}

#[test]
fn last_seen_survives_a_restart() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let store = Arc::new(MemKeyStore::default());
  let a = engine_on(&fake, &clock, store.clone(), "mx_local");
  let b = engine_with(&fake, &clock, "Desktop B");
  let code = a.open_join_code().unwrap();
  b.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  a.decide_join(&my_id(&b), true).unwrap();
  b.poll_once().unwrap();
  send(&b, "hi").unwrap();
  a.poll_once().unwrap();
  let bid = my_id(&b);
  assert_eq!(a.lock().st.last_seen.get(&bid), Some(&T0));
  clock.set(T0 + 31_000);
  a.poll_once().unwrap();
  let again = engine_on(&fake, &clock, store, "mx_local");
  assert_eq!(again.lock().st.last_seen.get(&bid), Some(&T0));
}

#[test]
fn reused_join_code_raises_an_alert_without_a_pending_join() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let b = engine_with(&fake, &clock, "Desktop B");
  let c = engine_with(&fake, &clock, "Desktop C");
  let code = a.open_join_code().unwrap();
  b.join_with_code(&code).unwrap();
  c.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  let ts = a.trust_state().unwrap();
  assert_eq!(ts.pending_joins.len(), 1);
  assert_eq!(ts.alerts.len(), 1);
  assert_eq!(ts.alerts[0].kind, "join_code_reused");
  a.poll_once().unwrap();
  assert_eq!(a.trust_state().unwrap().alerts.len(), 1);
}

#[test]
fn expired_join_code_raises_an_alert_and_alerts_age_out_after_7_days() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let b = engine_with(&fake, &clock, "Desktop B");
  let code = a.open_join_code().unwrap();
  b.join_with_code(&code).unwrap();
  clock.set(T0 + 11 * 60 * 1000);
  a.poll_once().unwrap();
  let ts = a.trust_state().unwrap();
  assert!(ts.pending_joins.is_empty());
  assert_eq!(ts.alerts.len(), 1);
  assert_eq!(ts.alerts[0].device_id, my_id(&b));
  clock.set(T0 + 8 * DAY);
  assert!(a.trust_state().unwrap().alerts.is_empty());
}

#[test]
fn alerts_are_capped_at_fifty() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  a.open_join_code().unwrap();
  let mut g = a.lock();
  for i in 0..60 {
    record_alert(&mut g, "join_code_reused", &format!("d{i}"), T0);
  }
  assert_eq!(g.st.alerts.len(), 50);
  assert_eq!(g.st.alerts[0].device_id, "d10");
}

#[test]
fn signed_kind_wins_over_a_lying_directory() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let r = engine_with(&fake, &clock, "Runner").with_kind("runner");
  let code = a.open_join_code().unwrap();
  r.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  let rid = my_id(&r);
  set_kind(&fake, &rid, "desktop");
  a.decide_join(&rid, true).unwrap();
  r.poll_once().unwrap();
  a.poll_once().unwrap();
  assert!(a.trust_state().unwrap().devices.iter().any(|d| d.id == rid && d.kind == "runner"));
  let aid = my_id(&a);
  post_chat(&r, &aid, &msg_n(2, "owner"));
  assert!(a.poll_once().unwrap().is_empty());
  post_chat(&r, &aid, &msg_n(3, "manager"));
  assert_eq!(a.poll_once().unwrap().len(), 1);
}

#[test]
fn directory_only_kind_is_unknown_and_cannot_author_owner_messages() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let (aid, bid) = (my_id(&a), my_id(&b));
  a.lock().st.kinds.remove(&bid);
  a.lock().st.intros.clear();
  fake.0.lock().unwrap().devices.iter_mut().find(|d| d.id == bid).unwrap().intros.clear();
  post_chat(&b, &aid, &msg_n(2, "owner"));
  assert!(a.poll_once().unwrap().is_empty());
  a.lock().st.kinds.insert(bid.clone(), "desktop".into());
  post_chat(&b, &aid, &msg_n(3, "owner"));
  assert_eq!(a.poll_once().unwrap().len(), 1);
}

#[test]
fn a_conflicting_signed_kind_raises_an_alert_and_is_not_applied() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let bid = my_id(&b);
  {
    let mut g = a.lock();
    pin_kind(&mut g, &bid, "runner", T0);
    assert_eq!(g.st.kinds.get(&bid).map(String::as_str), Some("desktop"));
  }
  assert_eq!(alert_kinds(&a), vec!["kind_conflict".to_owned()]);
}

fn code_with_kind(a: &Engine, kind: Option<&str>) -> String {
  let code = a.open_join_code().unwrap();
  let (stripped, _) = split_code_kind(&code).unwrap();
  match kind {
    Some(k) => add_code_kind(&stripped, k).unwrap(),
    None => stripped,
  }
}

#[test]
fn join_code_kind_must_match_the_directory_and_is_pinned() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  a.register().unwrap();
  let aid = my_id(&a);
  set_kind(&fake, &aid, "runner");
  let b = engine_with(&fake, &clock, "Desktop B");
  assert!(b.join_with_code(&code_with_kind(&a, Some("mobile"))).is_err());
  assert_eq!(b.trust_state().unwrap().devices.len(), 1);
  assert!(!b.lock().st.kinds.contains_key(&aid));
  b.join_with_code(&code_with_kind(&a, Some("runner"))).unwrap();
  assert_eq!(b.lock().st.kinds.get(&aid).map(String::as_str), Some("runner"));
}

#[test]
fn v1_join_code_without_kind_still_joins_with_unknown_kind() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let b = engine_with(&fake, &clock, "Desktop B");
  b.join_with_code(&code_with_kind(&a, None)).unwrap();
  assert!(!b.lock().st.kinds.contains_key(&my_id(&a)));
}
