use std::collections::BTreeMap;
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
  last_seen: BTreeMap<String, u64>,
  puts: Vec<(String, String, String)>,
}

#[derive(Clone, Default)]
struct Fake(Arc<Mutex<Server>>);

impl Fake {
  fn puts_by(&self, by: &str, target: &str) -> Vec<u8> {
    self.0.lock().unwrap().puts.iter().filter(|p| p.0 == by && p.1 == target).map(|p| parse_intro(&p.2).unwrap().parsed.version).collect()
  }

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

  fn set_last_seen(&self, id: &str, ms: u64) {
    self.0.lock().unwrap().last_seen.insert(id.into(), ms);
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
          .map(|d| json!({ "device_id": d.id, "kind": d.kind, "label": d.label, "sign_pub": d.sign_pub, "kex_pub": d.kex_pub, "intros": d.intros.iter().map(|i| i.1.clone()).collect::<Vec<_>>(), "last_seen_at": s.last_seen.get(&d.id) }))
          .collect();
        let revs: Vec<Value> = s.revocations.iter().map(|r| json!({ "target_fp": r.0, "statement": r.2 })).collect();
        wrap(json!({ "devices": devices, "revocations": revs }))
      }
      (Method::Put, ["devices", target, "intros", "self"]) => {
        let by = caller.ok_or("401")?;
        let intro = body["intro"].as_str().unwrap().to_owned();
        s.puts.push((by.clone(), (*target).to_owned(), intro.clone()));
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
  e.send(SendParams { room: room(), body_md: text.into(), reply_to: None, attachments: vec![], id: None, ts_ms: None, mention: None })
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
    mention: None,
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
    mention: None,
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
    apply_revoke(&mut g, USER, T0, "00000000-0000-4000-8000-0000000000dd", &tmgr_phone::h("device d"), "D");
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
    apply_revoke(&mut g, USER, T0, &bid, &fp_of(&b), "Desktop B");
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

fn pin_fp(c: &Core, id: &str) -> Option<String> {
  c.st.trusted.iter().chain(c.st.base.iter()).find(|d| d.id == id).map(|d| d.fp.clone())
}

fn kind_pin(e: &Engine, id: &str) -> Option<String> {
  let g = e.lock();
  let fp = pin_fp(&g, id)?;
  g.st.kind_of(id, &fp).map(str::to_owned)
}

fn set_pin(e: &Engine, id: &str, kind: &str) {
  let mut g = e.lock();
  let fp = pin_fp(&g, id).unwrap();
  g.st.kinds.insert(kind_key(id, &fp), kind.to_owned());
}

fn drop_pin(e: &Engine, id: &str) {
  let mut g = e.lock();
  let prefix = kind_key(id, "");
  g.st.kinds.retain(|k, _| !k.starts_with(&prefix));
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
  e.send(SendParams { room: room(), body_md: text.into(), reply_to: None, attachments: vec![], id: Some(id.into()), ts_ms: Some(ts), mention: None })
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
    version: 2,
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
  drop_pin(&a, &bid);
  a.lock().st.intros.clear();
  fake.0.lock().unwrap().devices.iter_mut().find(|d| d.id == bid).unwrap().intros.clear();
  post_chat(&b, &aid, &msg_n(2, "owner"));
  assert!(a.poll_once().unwrap().is_empty());
  set_pin(&a, &bid, "desktop");
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
    let fp = pin_fp(&g, &bid).unwrap();
    assert!(!pin_kind(&mut g, &bid, &fp, "mobile", T0));
    assert_eq!(g.st.kind_of(&bid, &fp), Some("desktop"));
  }
  assert_eq!(alert_kinds(&a), vec!["kind_conflict".to_owned()]);
}

fn code_with_kind(a: &Engine, kind: Option<&str>) -> String {
  let mut code = decode_join_code(&a.open_join_code().unwrap()).unwrap();
  match kind {
    Some(k) => code.kind = Some(k.to_owned()),
    None => {
      code.version = 1;
      code.kind = None;
    }
  }
  encode_join_code(&code).unwrap()
}

#[test]
fn join_code_kind_must_match_the_directory_and_is_pinned() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  a.register().unwrap();
  let aid = my_id(&a);
  let b = engine_with(&fake, &clock, "Desktop B");
  assert!(b.join_with_code(&code_with_kind(&a, Some("mobile"))).is_err());
  assert_eq!(b.trust_state().unwrap().devices.len(), 1);
  assert!(!kind_pin(&b, &aid).is_some());
  b.join_with_code(&code_with_kind(&a, Some("desktop"))).unwrap();
  assert_eq!(kind_pin(&b, &aid).as_deref(), Some("desktop"));
}

#[test]
fn v1_join_code_without_kind_still_joins_with_unknown_kind() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let b = engine_with(&fake, &clock, "Desktop B");
  b.join_with_code(&code_with_kind(&a, None)).unwrap();
  assert!(!kind_pin(&b, &my_id(&a)).is_some());
}

fn directory_revoke(fake: &Fake, by: &Engine, target_id: &str, target_fp: &str) {
  let g = by.lock();
  let keys = g.keys.as_ref().unwrap();
  let st = RevokeStatement { user_id: USER, revoker_fp: keys.fingerprint(), target_id: target_id.into(), target_fp: target_fp.into(), issued_at_ms: T0 };
  fake.add_revocation(target_fp, &my_id_of(&g), &sign_revoke(keys, &st).unwrap());
}

#[test]
fn unknown_author_kind_is_dropped_and_alerted() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let r = join_as(&fake, &clock, &a, "Runner", "runner");
  let aid = my_id(&a);
  post_chat(&r, &aid, &msg_n(2, "bogus"));
  assert!(a.poll_once().unwrap().is_empty());
  assert!(alert_kinds(&a).contains(&"unknown_author_kind".to_owned()));
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  post_chat(&a, &my_id(&b), &msg_n(2, "bogus"));
  assert!(b.poll_once().unwrap().is_empty());
  assert!(alert_kinds(&b).contains(&"unknown_author_kind".to_owned()));
}

#[test]
fn revoke_aimed_at_the_receiving_device_stays_pending() {
  let (fake, clock, a, _b, c) = silence_setup();
  let cid = my_id(&c);
  clock.set(T0 + 31 * DAY);
  directory_revoke(&fake, &a, &cid, &fp_of(&c));
  c.poll_once().unwrap();
  let ts = c.trust_state().unwrap();
  assert_eq!(ts.pending_revokes.len(), 1);
  assert!(ts.devices.iter().any(|d| d.id == cid && d.state == DeviceState::Trusted));
}

#[test]
fn quiet_device_active_in_the_directory_stays_pending() {
  let (fake, clock, a, b, c) = silence_setup();
  let cid = my_id(&c);
  clock.set(T0 + 31 * DAY);
  fake.set_last_seen(&cid, T0 + 30 * DAY);
  directory_revoke(&fake, &a, &cid, &fp_of(&c));
  b.poll_once().unwrap();
  assert_eq!(b.trust_state().unwrap().pending_revokes.len(), 1);
  assert!(!is_revoked(&b, &cid));
}

#[test]
fn directory_last_seen_accepts_seconds_millis_and_rfc3339() {
  assert_eq!(parse_seen(&json!(1_800_000_000)), Some(1_800_000_000_000));
  assert_eq!(parse_seen(&json!(1_800_000_000_123u64)), Some(1_800_000_000_123));
  assert_eq!(parse_seen(&json!("2027-01-15T08:00:00Z")), Some(1800000000000));
  assert_eq!(parse_seen(&Value::Null), None);
}

fn publish_for(a: &Engine) {
  let mut g = a.lock();
  a.publish_intros(&mut g).unwrap();
}

fn intros_of(fake: &Fake, id: &str) -> Vec<String> {
  fake.0.lock().unwrap().devices.iter().find(|d| d.id == id).unwrap().intros.iter().map(|i| i.1.clone()).collect()
}

#[test]
fn no_intro_is_signed_for_a_device_without_a_signed_kind() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let bid = my_id(&b);
  drop_pin(&a, &bid);
  set_kind(&fake, &bid, "desktop");
  fake.0.lock().unwrap().devices.iter_mut().find(|d| d.id == bid).unwrap().intros.clear();
  publish_for(&a);
  assert!(intros_of(&fake, &bid).is_empty());
}

#[test]
fn directory_kind_never_ends_up_in_a_signed_intro() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let bid = my_id(&b);
  set_kind(&fake, &bid, "runner");
  a.lock().st.trusted.iter_mut().find(|d| d.id == bid).unwrap().kind = "runner".into();
  fake.0.lock().unwrap().devices.iter_mut().find(|d| d.id == bid).unwrap().intros.clear();
  publish_for(&a);
  let intros = intros_of(&fake, &bid);
  assert_eq!(intros.len(), 1);
  assert_eq!(parse_intro(&intros[0]).unwrap().parsed.target_kind, "desktop");
}

#[test]
fn trust_state_marks_only_signed_kinds_as_verified() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let bid = my_id(&b);
  let verified = |e: &Engine| e.trust_state().unwrap().devices.iter().find(|d| d.id == bid).unwrap().kind_verified;
  assert!(verified(&a));
  drop_pin(&a, &bid);
  assert!(!verified(&a));
}

#[test]
fn join_ok_is_persisted_before_it_is_acked() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let store = Arc::new(MemKeyStore::default());
  let b = engine_on(&fake, &clock, store.clone(), "mx_b");
  let code = a.open_join_code().unwrap();
  b.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  a.decide_join(&my_id(&b), true).unwrap();
  b.poll_once().unwrap();
  let again = engine_on(&fake, &clock, store, "mx_b");
  let g = again.lock();
  assert!(g.st.joined);
  assert_eq!(g.st.kind_of(&my_id(&a), &fp_of(&a)), Some("desktop"));
}

#[test]
fn repeated_revoke_retries_the_broadcast_and_the_directory_delete() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b, c) = trio(&fake, &clock);
  let (bid, cid) = (my_id(&b), my_id(&c));
  let before = fake.queue_len(&bid);
  fake.set_post_error(Some("boom"));
  assert!(a.revoke(&cid).is_err());
  assert!(is_revoked(&a, &cid));
  fake.set_post_error(None);
  a.revoke(&cid).unwrap();
  assert_eq!(fake.queue_len(&bid), before + 1);
  assert!(!fake.0.lock().unwrap().devices.iter().any(|d| d.id == cid));
  assert_eq!(a.trust_state().unwrap().devices.iter().filter(|d| d.id == cid).count(), 1);
}

fn put_foreign_intro(fake: &Fake, signer: &Engine, target_id: &str, target_fp: &str, kind: &str) {
  let st = IntroStatement { version: 2, user_id: USER, introducer_fp: fp_of(signer), target_id: target_id.to_owned(), target_fp: target_fp.to_owned(), target_kind: kind.into(), issued_at_ms: T0 };
  let sid = my_id(signer);
  let intro = sign_intro(signer.lock().keys.as_ref().unwrap(), &sid, &st).unwrap();
  let mut s = fake.0.lock().unwrap();
  let d = s.devices.iter_mut().find(|d| d.id == target_id).unwrap();
  d.intros.push((sid, intro));
}

#[test]
fn a_runner_signed_intro_adds_no_device_but_a_desktop_signed_one_does() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let r = join_as(&fake, &clock, &a, "Runner", "runner");
  let x = engine_with(&fake, &clock, "Desktop X");
  x.register().unwrap();
  let xid = my_id(&x);
  let x_fp = x.lock().st.trusted[0].fp.clone();
  put_foreign_intro(&fake, &r, &xid, &x_fp, "desktop");
  a.poll_once().unwrap();
  assert!(a.trust_state().unwrap().devices.iter().all(|d| d.id != xid));
  put_foreign_intro(&fake, &a, &xid, &x_fp, "desktop");
  a.poll_once().unwrap();
  assert!(a.trust_state().unwrap().devices.iter().any(|d| d.id == xid && d.state == DeviceState::Trusted));
}

fn put_raw_intro(fake: &Fake, by_id: &str, target_id: &str, intro: &str) {
  let mut s = fake.0.lock().unwrap();
  let d = s.devices.iter_mut().find(|d| d.id == target_id).unwrap();
  d.intros.push((by_id.to_owned(), intro.to_owned()));
}

fn signed_intro(signer: &Engine, target_id: &str, target_fp: &str, kind: &str) -> String {
  signed_intro_v(2, signer, target_id, target_fp, kind)
}

fn signed_intro_v(version: u8, signer: &Engine, target_id: &str, target_fp: &str, kind: &str) -> String {
  let st = IntroStatement { version, user_id: USER, introducer_fp: fp_of(signer), target_id: target_id.to_owned(), target_fp: target_fp.to_owned(), target_kind: kind.into(), issued_at_ms: T0 };
  { let sid = my_id(signer); sign_intro(signer.lock().keys.as_ref().unwrap(), &sid, &st).unwrap() }
}

fn clear_intros(fake: &Fake, e: &Engine) {
  for d in fake.0.lock().unwrap().devices.iter_mut() {
    d.intros.clear();
  }
  e.lock().st.intros.clear();
}

#[test]
fn forged_directory_intros_pin_no_kind() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let b = join_as(&fake, &clock, &a, "Desktop B", "desktop");
  let r = join_as(&fake, &clock, &a, "Runner", "runner");
  let (aid, bid, b_fp) = (my_id(&a), my_id(&b), fp_of(&b));
  clear_intros(&fake, &a);
  drop_pin(&a, &bid);
  let other_sig = serde_json::from_str::<Value>(&signed_intro(&a, &bid, &b_fp, "mobile")).unwrap()["sig"].clone();
  let mut v: Value = serde_json::from_str(&signed_intro(&a, &bid, &b_fp, "desktop")).unwrap();
  v["sig"] = other_sig;
  put_raw_intro(&fake, &aid, &bid, &v.to_string());
  let mut v: Value = serde_json::from_str(&signed_intro(&r, &bid, &b_fp, "desktop")).unwrap();
  v["by"] = json!(aid);
  put_raw_intro(&fake, &aid, &bid, &v.to_string());
  a.poll_once().unwrap();
  assert!(!kind_pin(&a, &bid).is_some());
}

#[test]
fn a_runner_intro_listed_first_pins_no_kind_and_the_desktop_one_does() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let r = join_as(&fake, &clock, &a, "Runner", "runner");
  let x = engine_with(&fake, &clock, "Desktop X");
  x.register().unwrap();
  let (xid, x_fp) = (my_id(&x), fp_of(&x));
  put_raw_intro(&fake, &my_id(&r), &xid, &signed_intro(&r, &xid, &x_fp, "desktop"));
  a.poll_once().unwrap();
  assert!(!kind_pin(&a, &xid).is_some());
  put_raw_intro(&fake, &my_id(&a), &xid, &signed_intro(&a, &xid, &x_fp, "mobile"));
  a.poll_once().unwrap();
  assert_eq!(kind_pin(&a, &xid).as_deref(), Some("mobile"));
}

#[test]
fn a_state_without_the_own_kind_keeps_introduced_devices_through_a_revoke() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let store = Arc::new(MemKeyStore::default());
  let a = engine_on(&fake, &clock, store.clone(), "mx_local");
  let b = engine_with(&fake, &clock, "Desktop B");
  b.join_with_code(&a.open_join_code().unwrap()).unwrap();
  a.poll_once().unwrap();
  a.decide_join(&my_id(&b), true).unwrap();
  b.poll_once().unwrap();
  let (bid, aid) = (my_id(&b), my_id(&a));
  {
    let mut g = a.lock();
    let prefix = kind_key(&aid, "");
    g.st.kinds.retain(|k, _| !k.starts_with(&prefix));
    store.save(Slot::State, &serde_json::to_string(&g.st).unwrap()).unwrap();
  }
  let a2 = engine_on(&fake, &clock, store, "mx_local");
  a2.register().unwrap();
  {
    let mut g = a2.lock();
    apply_revoke(&mut g, USER, T0, "00000000-0000-4000-8000-0000000000dd", &tmgr_phone::h("device d"), "D");
  }
  assert!(a2.trust_state().unwrap().devices.iter().any(|d| d.id == bid && d.state == DeviceState::Trusted));
}

#[test]
fn conflicting_kinds_from_two_desktops_leave_the_kind_unknown_and_alert_in_both_orders() {
  for flip in [false, true] {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let (_x, xid, x_fp) = registered(&fake, &clock, "Desktop X", "desktop");
    let by_a = (my_id(&a), signed_intro(&a, &xid, &x_fp, "desktop"));
    let by_b = (my_id(&b), signed_intro(&b, &xid, &x_fp, "mobile"));
    let (first, second) = if flip { (by_b, by_a) } else { (by_a, by_b) };
    put_raw_intro(&fake, &first.0, &xid, &first.1);
    put_raw_intro(&fake, &second.0, &xid, &second.1);
    a.poll_once().unwrap();
    assert_eq!(kind_pin(&a, &xid), None);
    assert!(alert_kinds(&a).contains(&"kind_conflict".to_owned()));
    assert!(alert_kinds(&a).contains(&"kind_unknown".to_owned()));
  }
}

fn kind_entries_of(e: &Engine, id: &str) -> Vec<String> {
  let prefix = kind_key(id, "");
  e.lock().st.kinds.keys().filter(|k| k.starts_with(&prefix)).cloned().collect()
}

fn rename_device(fake: &Fake, e: &Engine, to: &str) {
  let from = my_id(e);
  fake.0.lock().unwrap().devices.iter_mut().find(|d| d.id == from).unwrap().id = to.to_owned();
  e.lock().identity.as_mut().unwrap().device_id = to.to_owned();
}

fn registered(fake: &Fake, clock: &Clock, label: &str, kind: &str) -> (Engine, String, String) {
  let e = engine_with(fake, clock, label).with_kind(kind);
  e.register().unwrap();
  let (id, fp) = (my_id(&e), fp_of(&e));
  (e, id, fp)
}

#[test]
fn a_revoked_id_reused_by_a_runner_with_new_keys_inherits_no_kind_and_is_refused() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let bid = my_id(&b);
  assert_eq!(kind_pin(&a, &bid).as_deref(), Some("desktop"));
  a.revoke(&bid).unwrap();
  assert!(kind_entries_of(&a, &bid).is_empty());
  let r = engine_with(&fake, &clock, "Runner").with_kind("runner");
  r.register().unwrap();
  rename_device(&fake, &r, &bid);
  r.join_with_code(&a.open_join_code().unwrap()).unwrap();
  a.poll_once().unwrap();
  assert_eq!(a.trust_state().unwrap().pending_joins.len(), 1);
  assert!(a.decide_join(&bid, true).is_err());
  assert!(a.trust_state().unwrap().pending_joins.is_empty());
  assert!(a.trust_state().unwrap().devices.iter().all(|d| d.id != bid || d.state == DeviceState::Revoked));
  assert!(kind_entries_of(&a, &bid).is_empty());
  assert!(alert_kinds(&a).contains(&"kind_conflict".to_owned()));
}

#[test]
fn a_revoke_with_an_unknown_root_kind_keeps_the_pins_and_alerts() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let (_c, cid, c_fp) = registered(&fake, &clock, "Desktop C", "desktop");
  let aid = my_id(&a);
  put_raw_intro(&fake, &aid, &cid, &signed_intro(&a, &cid, &c_fp, "desktop"));
  b.poll_once().unwrap();
  assert!(b.trust_state().unwrap().devices.iter().any(|d| d.id == cid && d.state == DeviceState::Trusted));
  {
    let mut g = b.lock();
    let prefix = kind_key(&aid, "");
    g.st.kinds.retain(|k, _| !k.starts_with(&prefix));
    g.dir = None;
    g.st.intros.retain(|i| parse_intro(i).is_ok_and(|p| p.parsed.target_id == cid));
    apply_revoke(&mut g, USER, T0, "00000000-0000-4000-8000-0000000000dd", &tmgr_phone::h("device d"), "D");
  }
  assert!(alert_kinds(&b).contains(&"root_kind_unknown".to_owned()));
  let devices = b.trust_state().unwrap().devices;
  assert!(devices.iter().any(|d| d.id == cid && d.state == DeviceState::Trusted));
  assert!(devices.iter().any(|d| d.id == aid && d.state == DeviceState::Trusted));
}

#[test]
fn a_join_ok_with_an_unknown_kind_is_rejected() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let mut a = engine_with(&fake, &clock, "Desktop A");
  let b = engine_with(&fake, &clock, "Desktop B");
  b.join_with_code(&a.open_join_code().unwrap()).unwrap();
  a.poll_once().unwrap();
  a.kind = "bogus".into();
  a.decide_join(&my_id(&b), true).unwrap();
  b.poll_once().unwrap();
  assert!(!b.lock().st.joined);
  assert!(b.lock().st.kinds.values().all(|k| k != "bogus"));
}

#[test]
fn a_repeated_kind_conflict_does_not_touch_the_state_or_the_alert_time() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let (_x, xid, x_fp) = registered(&fake, &clock, "Desktop X", "desktop");
  put_raw_intro(&fake, &my_id(&a), &xid, &signed_intro(&a, &xid, &x_fp, "desktop"));
  put_raw_intro(&fake, &my_id(&b), &xid, &signed_intro(&b, &xid, &x_fp, "mobile"));
  a.poll_once().unwrap();
  assert!(alert_kinds(&a).contains(&"kind_conflict".to_owned()));
  let snap = |e: &Engine| {
    let g = e.lock();
    (trust_key(&g.st), serde_json::to_string(&g.st).unwrap())
  };
  let first = snap(&a);
  for n in 1..4 {
    clock.set(T0 + n * 1000);
    a.poll_once().unwrap();
    assert_eq!(snap(&a), first);
  }
}

#[test]
fn a_failed_join_keeps_a_kind_that_was_pinned_before_and_drops_one_it_added() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let b = engine_with(&fake, &clock, "Desktop B");
  let code = a.open_join_code().unwrap();
  let (aid, a_fp) = (my_id(&a), fp_of(&a));
  fake.set_post_error(Some(r#"{"error":"recipient_full"}"#));
  assert!(b.join_with_code(&code).is_err());
  assert!(kind_entries_of(&b, &aid).is_empty());
  b.lock().st.kinds.insert(kind_key(&aid, &a_fp), "desktop".into());
  assert!(b.join_with_code(&code).is_err());
  assert_eq!(b.lock().st.kinds.get(&kind_key(&aid, &a_fp)).map(String::as_str), Some("desktop"));
}

fn legacy_state_with_kinds(store: &Arc<MemKeyStore>, e: &Engine, kinds: Value, intros: Vec<String>) {
  let g = e.lock();
  let mut v: Value = serde_json::to_value(&g.st).unwrap();
  v["kinds"] = kinds;
  v.as_object_mut().unwrap().remove("kinds_v");
  v["intros"] = json!(intros);
  store.save(Slot::State, &v.to_string()).unwrap();
}

fn trusted_with_kind(e: &Engine, id: &str, kind: &str) -> bool {
  e.trust_state().unwrap().devices.iter().any(|d| d.id == id && d.state == DeviceState::Trusted && d.kind == kind && d.kind_verified)
}

#[test]
fn a_migrated_device_republishes_its_intros_once_with_fresh_timestamps() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let store = Arc::new(MemKeyStore::default());
  let a = engine_with(&fake, &clock, "Desktop A");
  let b = engine_on(&fake, &clock, store.clone(), "mx_local");
  b.join_with_code(&a.open_join_code().unwrap()).unwrap();
  a.poll_once().unwrap();
  a.decide_join(&my_id(&b), true).unwrap();
  b.poll_once().unwrap();
  let (aid, bid) = (my_id(&a), my_id(&b));
  let own_intros = b.lock().st.intros.clone();
  legacy_state_with_kinds(&store, &b, json!({ aid.clone(): "desktop", bid.clone(): "desktop" }), own_intros);
  let issued = |fake: &Fake| -> Vec<u64> { intros_of(fake, &aid).iter().map(|i| parse_intro(i).unwrap()).filter(|i| i.by == bid).map(|i| i.parsed.issued_at_ms).collect() };
  assert!(issued(&fake).contains(&T0));
  clock.set(T0 + 1000);
  let b2 = engine_on(&fake, &clock, store, "mx_local");
  b2.poll_once().unwrap();
  assert!(issued(&fake).iter().all(|t| *t >= T0 + 1000));
  assert!(!issued(&fake).is_empty());
}

fn kind_race(runner_first: bool) {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let (_x, xid, x_fp) = registered(&fake, &clock, "Desktop X", "desktop");
  let by_a = signed_intro(&a, &xid, &x_fp, "desktop");
  let by_b = signed_intro(&b, &xid, &x_fp, "runner");
  let (first, second) = if runner_first { ((my_id(&b), by_b), (my_id(&a), by_a)) } else { ((my_id(&a), by_a), (my_id(&b), by_b)) };
  put_raw_intro(&fake, &first.0, &xid, &first.1);
  put_raw_intro(&fake, &second.0, &xid, &second.1);
  a.poll_once().unwrap();
  assert_eq!(kind_pin(&a, &xid).as_deref(), Some("runner"));
  assert!(alert_kinds(&a).contains(&"kind_conflict".to_owned()));
}

#[test]
fn a_runner_intro_wins_a_kind_conflict_when_it_comes_second() {
  kind_race(false);
}

#[test]
fn a_runner_intro_wins_a_kind_conflict_when_it_comes_first() {
  kind_race(true);
}

#[test]
fn a_pin_kind_conflict_pins_runner_and_keeps_desktop_against_mobile() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let bid = my_id(&b);
  let b_fp = fp_of(&b);
  {
    let mut g = a.lock();
    assert!(!pin_kind(&mut g, &bid, &b_fp, "mobile", T0));
    assert_eq!(g.st.kind_of(&bid, &b_fp), Some("desktop"));
    pin_kind(&mut g, &bid, &b_fp, "runner", T0);
    assert_eq!(g.st.kind_of(&bid, &b_fp), Some("runner"));
    assert!(!pin_kind(&mut g, &bid, &b_fp, "desktop", T0));
    assert_eq!(g.st.kind_of(&bid, &b_fp), Some("runner"));
  }
  assert!(alert_kinds(&a).contains(&"kind_conflict".to_owned()));
}

#[test]
fn revoking_a_voucher_drops_its_devices_even_when_a_root_kind_is_unknown() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let (c_dev, cid, c_fp) = registered(&fake, &clock, "Desktop C", "desktop");
  let (_x, xid, x_fp) = registered(&fake, &clock, "Desktop X", "desktop");
  let aid = my_id(&a);
  put_raw_intro(&fake, &aid, &cid, &signed_intro(&a, &cid, &c_fp, "desktop"));
  put_raw_intro(&fake, &cid, &xid, &signed_intro(&c_dev, &xid, &x_fp, "desktop"));
  b.poll_once().unwrap();
  assert!(trusted_with_kind(&b, &cid, "desktop"));
  assert!(trusted_with_kind(&b, &xid, "desktop"));
  {
    let mut g = b.lock();
    let prefix = kind_key(&aid, "");
    g.st.kinds.retain(|k, _| !k.starts_with(&prefix));
    g.dir = None;
    g.st.intros.retain(|i| parse_intro(i).is_ok_and(|p| p.parsed.target_id == cid || p.parsed.target_id == xid));
    apply_revoke(&mut g, USER, T0, &cid, &c_fp, "C");
  }
  assert!(alert_kinds(&b).contains(&"root_kind_unknown".to_owned()));
  let devices = b.trust_state().unwrap().devices;
  assert!(devices.iter().all(|d| d.id != xid || d.state != DeviceState::Trusted));
  assert!(kind_entries_of(&b, &xid).is_empty());
  assert!(devices.iter().any(|d| d.id == aid && d.state == DeviceState::Trusted));
}

fn take_intros_by(fake: &Fake, row: &str, by: &str) -> Vec<(String, String)> {
  let mut s = fake.0.lock().unwrap();
  let d = s.devices.iter_mut().find(|d| d.id == row).unwrap();
  let (taken, kept) = d.intros.drain(..).partition(|i| i.0 == by);
  d.intros = kept;
  taken
}

fn restore_intros(fake: &Fake, row: &str, intros: Vec<(String, String)>) {
  let mut s = fake.0.lock().unwrap();
  s.devices.iter_mut().find(|d| d.id == row).unwrap().intros.extend(intros);
}

fn legacy_owner_fleet() -> (Fake, Clock, Arc<MemKeyStore>, Engine, Engine, Engine) {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let store = Arc::new(MemKeyStore::default());
  let a = engine_on(&fake, &clock, store.clone(), "mx_local");
  let x = join_as(&fake, &clock, &a, "Desktop X", "desktop");
  let y = join_as(&fake, &clock, &a, "Desktop Y", "desktop");
  (fake, clock, store, a, x, y)
}

#[test]
fn m01_a_v2_intro_round_trips_and_a_v1_parser_rejects_it() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let (bid, b_fp) = (my_id(&b), fp_of(&b));
  let v2 = signed_intro(&a, &bid, &b_fp, "desktop");
  let parsed = parse_intro(&v2).unwrap().parsed;
  assert_eq!(parsed.version, 2);
  assert!(parsed.to_statement().unwrap().starts_with("tmgr-chat-intro:v2:"));
  assert!(IntroStatement { version: 3, ..parsed.clone() }.to_statement().is_err());
  let mut forged: Value = serde_json::from_str(&v2).unwrap();
  forged["v"] = json!(1);
  assert!(parse_intro(&forged.to_string()).is_err());
}

#[test]
fn m02_a_v1_intro_pins_no_kind_and_trusts_only_a_target_with_a_known_kind() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let (_x, xid, x_fp) = registered(&fake, &clock, "Desktop X", "desktop");
  put_raw_intro(&fake, &my_id(&a), &xid, &signed_intro_v(1, &a, &xid, &x_fp, "desktop"));
  a.poll_once().unwrap();
  b.poll_once().unwrap();
  for e in [&a, &b] {
    assert!(e.trust_state().unwrap().devices.iter().all(|d| d.id != xid || d.state != DeviceState::Trusted));
    assert_eq!(kind_pin(e, &xid), None);
  }
}

#[test]
fn m03_poisoned_legacy_state_migrates() {
  let (fake, clock, store, a, x, y) = legacy_owner_fleet();
  let (aid, xid, yid, y_fp) = (my_id(&a), my_id(&x), my_id(&y), fp_of(&y));
  let own_v1 = signed_intro_v(1, &a, &xid, &fp_of(&x), "desktop");
  legacy_state_with_kinds(&store, &a, json!({ aid.clone(): "desktop", xid.clone(): "mobile", yid.clone(): "runner" }), vec![own_v1.clone()]);
  clear_intros(&fake, &a);
  put_raw_intro(&fake, &aid, &xid, &own_v1);
  put_raw_intro(&fake, &aid, &yid, &signed_intro_v(1, &a, &yid, &y_fp, "runner"));
  clock.set(T0 + 1000);
  let a2 = engine_on(&fake, &clock, store, "mx_local");
  {
    let g = a2.lock();
    assert_eq!(g.st.kinds_v, KINDS_V);
    assert_eq!(g.st.kind_of(&xid, &fp_of(&x)), Some("mobile"));
    assert_eq!(g.st.kind_of(&yid, &y_fp), None);
    assert!(g.st.legacy_unknown.contains(&kind_key(&yid, &y_fp)));
    assert!(g.st.intros.contains(&own_v1));
  }
  a2.poll_once().unwrap();
  assert!(alert_kinds(&a2).contains(&"kind_unknown".to_owned()));
  post_chat(&y, &aid, &msg_n(2, "owner"));
  assert!(a2.poll_once().unwrap().is_empty());
  let can_confirm = |e: &Engine| e.trust_state().unwrap().devices.iter().find(|d| d.id == yid).map(|d| (d.state, d.kind_verified, d.can_confirm_kind));
  assert_eq!(can_confirm(&a2), Some((DeviceState::Trusted, false, true)));
  assert!(a2.confirm_device_kind(&xid, "desktop").is_err());
  assert!(a2.confirm_device_kind(&yid, "runner").is_err());
  assert!(a2.confirm_device_kind("00000000-0000-4000-8000-0000000000aa", "desktop").is_err());
  a2.confirm_device_kind(&yid, "desktop").unwrap();
  assert_eq!(kind_pin(&a2, &yid).as_deref(), Some("desktop"));
  assert_eq!(can_confirm(&a2), Some((DeviceState::Trusted, true, false)));
  assert!(!alert_kinds(&a2).contains(&"kind_unknown".to_owned()));
  assert!(fake.puts_by(&aid, &yid).contains(&2));
  post_chat(&y, &aid, &msg_n(3, "owner"));
  assert_eq!(a2.poll_once().unwrap().len(), 1);
  assert!(a2.confirm_device_kind(&yid, "mobile").is_err());
}

#[test]
fn a_kind_unknown_device_that_was_not_trusted_at_migration_cannot_be_confirmed() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let (_x, xid, x_fp) = registered(&fake, &clock, "Desktop X", "desktop");
  put_raw_intro(&fake, &my_id(&a), &xid, &signed_intro(&a, &xid, &x_fp, "desktop"));
  put_raw_intro(&fake, &my_id(&b), &xid, &signed_intro(&b, &xid, &x_fp, "mobile"));
  a.poll_once().unwrap();
  assert_eq!(kind_pin(&a, &xid), None);
  assert!(a.confirm_device_kind(&xid, "desktop").is_err());
  assert_eq!(kind_pin(&a, &xid), None);
}

#[test]
fn m04_a_replayed_v1_intro_cannot_turn_a_runner_into_a_desktop() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let r = join_as(&fake, &clock, &a, "Runner", "runner");
  let (aid, bid, rid, r_fp) = (my_id(&a), my_id(&b), my_id(&r), fp_of(&r));
  b.poll_once().unwrap();
  assert_eq!(kind_pin(&b, &rid).as_deref(), Some("runner"));
  put_raw_intro(&fake, &aid, &rid, &signed_intro_v(1, &a, &rid, &r_fp, "desktop"));
  put_raw_intro(&fake, &bid, &rid, &signed_intro_v(1, &b, &rid, &r_fp, "desktop"));
  let (_x, xid, x_fp) = registered(&fake, &clock, "Desktop X", "desktop");
  put_foreign_intro(&fake, &r, &xid, &x_fp, "desktop");
  for e in [&a, &b] {
    e.poll_once().unwrap();
    assert_eq!(kind_pin(e, &rid).as_deref(), Some("runner"));
    assert!(e.trust_state().unwrap().devices.iter().all(|d| d.id != xid));
  }
  post_chat(&r, &aid, &msg_n(2, "owner"));
  assert!(a.poll_once().unwrap().is_empty());
}

#[test]
fn m05_a_replayed_intro_for_a_revoked_fp_or_a_reused_id_is_not_counted() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let (aid, bid) = (my_id(&a), my_id(&b));
  let old: Vec<String> = intros_of(&fake, &bid);
  assert!(!old.is_empty());
  a.revoke(&bid).unwrap();
  let r = engine_with(&fake, &clock, "Reused").with_kind("desktop");
  r.register().unwrap();
  rename_device(&fake, &r, &bid);
  for i in &old {
    put_raw_intro(&fake, &aid, &bid, i);
  }
  a.poll_once().unwrap();
  assert!(a.trust_state().unwrap().devices.iter().all(|d| d.id != bid || d.state == DeviceState::Revoked));
  assert!(kind_entries_of(&a, &bid).is_empty());
}

#[test]
fn m06_a_demoted_voucher_drops_the_devices_it_vouched_for() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let (x, xid, x_fp) = registered(&fake, &clock, "Desktop X", "desktop");
  let (_y, yid, y_fp) = registered(&fake, &clock, "Desktop Y", "desktop");
  put_raw_intro(&fake, &my_id(&a), &xid, &signed_intro(&a, &xid, &x_fp, "desktop"));
  put_raw_intro(&fake, &xid, &yid, &signed_intro(&x, &yid, &y_fp, "desktop"));
  put_raw_intro(&fake, &my_id(&b), &xid, &signed_intro(&b, &xid, &x_fp, "runner"));
  a.poll_once().unwrap();
  assert_eq!(kind_pin(&a, &xid).as_deref(), Some("runner"));
  assert!(a.trust_state().unwrap().devices.iter().all(|d| d.id != yid));
}

#[test]
fn m07_two_counted_intros_for_one_target_give_runner_and_leave_the_state_alone() {
  for runner_first in [true, false] {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let (_x, xid, x_fp) = registered(&fake, &clock, "Desktop X", "desktop");
    let by_a = (my_id(&a), signed_intro(&a, &xid, &x_fp, "desktop"));
    let by_b = (my_id(&b), signed_intro(&b, &xid, &x_fp, "runner"));
    let (first, second) = if runner_first { (by_b, by_a) } else { (by_a, by_b) };
    put_raw_intro(&fake, &first.0, &xid, &first.1);
    put_raw_intro(&fake, &second.0, &xid, &second.1);
    a.poll_once().unwrap();
    assert_eq!(kind_pin(&a, &xid).as_deref(), Some("runner"));
    let conflicts = |e: &Engine| alert_kinds(e).iter().filter(|k| *k == "kind_conflict").count();
    assert_eq!(conflicts(&a), 1);
    let snap = |e: &Engine| {
      let g = e.lock();
      (trust_key(&g.st), serde_json::to_string(&g.st).unwrap())
    };
    let first = snap(&a);
    for n in 1..4 {
      clock.set(T0 + n * 1000);
      a.poll_once().unwrap();
      assert_eq!(snap(&a), first);
    }
  }
}

#[test]
fn m09_a_phone_approved_before_the_upgrade_keeps_its_kind_and_gets_a_v2_intro() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let store = Arc::new(MemKeyStore::default());
  let a = engine_on(&fake, &clock, store.clone(), "mx_local");
  let m = join_as(&fake, &clock, &a, "Phone", "mobile");
  let (aid, mid) = (my_id(&a), my_id(&m));
  let legacy = signed_intro_v(1, &a, &mid, &fp_of(&m), "mobile");
  legacy_state_with_kinds(&store, &a, json!({ aid.clone(): "desktop", mid.clone(): "mobile" }), vec![legacy.clone()]);
  clear_intros(&fake, &a);
  put_raw_intro(&fake, &aid, &mid, &legacy);
  clock.set(T0 + 1000);
  let a2 = engine_on(&fake, &clock, store, "mx_local");
  assert_eq!(a2.lock().st.kind_of(&mid, &fp_of(&m)), Some("mobile"));
  a2.poll_once().unwrap();
  post_chat(&m, &aid, &msg_n(2, "owner"));
  assert_eq!(a2.poll_once().unwrap().len(), 1);
  let v2: Vec<_> = intros_of(&fake, &mid).iter().map(|i| parse_intro(i).unwrap()).filter(|i| i.by == aid && i.parsed.version == 2 && !i.parsed.is_self(&aid)).collect();
  assert_eq!(v2.len(), 1);
  assert_eq!(v2[0].parsed.target_kind, "mobile");
  assert!(v2[0].parsed.issued_at_ms >= T0 + 1000);
}

#[test]
fn m10_a_runner_join_waits_for_every_owner_device_to_show_a_v2_self_intro() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let bid = my_id(&b);
  let withheld = take_intros_by(&fake, &bid, &bid);
  assert_eq!(withheld.len(), 1);
  let r = engine_with(&fake, &clock, "Runner").with_kind("runner");
  r.join_with_code(&a.open_join_code().unwrap()).unwrap();
  a.poll_once().unwrap();
  let rid = my_id(&r);
  let e = a.decide_join(&rid, true).unwrap_err();
  assert!(e.contains("update these devices first") && e.contains("Desktop B"), "{e}");
  assert_eq!(a.trust_state().unwrap().pending_joins.len(), 1);
  assert!(a.trust_state().unwrap().devices.iter().all(|d| d.id != rid));
  restore_intros(&fake, &bid, withheld);
  a.decide_join(&rid, true).unwrap();
  r.poll_once().unwrap();
  assert!(r.lock().st.joined);
  assert_eq!(kind_pin(&a, &rid).as_deref(), Some("runner"));
}

#[test]
fn m11_a_v1_join_is_refused_once_a_runner_is_trusted() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let early = engine_with(&fake, &clock, "Desktop E");
  early.join_with_code(&code_with_kind(&a, None)).unwrap();
  a.poll_once().unwrap();
  a.decide_join(&my_id(&early), true).unwrap();
  early.poll_once().unwrap();
  a.poll_once().unwrap();
  let _r = join_as(&fake, &clock, &a, "Runner", "runner");
  let x = engine_with(&fake, &clock, "Desktop X");
  x.join_with_code(&code_with_kind(&a, None)).unwrap();
  a.poll_once().unwrap();
  let xid = my_id(&x);
  let e = a.decide_join(&xid, true).unwrap_err();
  assert!(e.contains("update the app"), "{e}");
  assert!(a.trust_state().unwrap().devices.iter().all(|d| d.id != xid));
  let y = engine_with(&fake, &clock, "Desktop Y");
  y.join_with_code(&a.open_join_code().unwrap()).unwrap();
  a.poll_once().unwrap();
  a.decide_join(&my_id(&y), true).unwrap();
}

#[test]
fn m13_withheld_self_intros_keep_the_gate_closed_and_change_no_trust() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let bid = my_id(&b);
  take_intros_by(&fake, &bid, &bid);
  let r = engine_with(&fake, &clock, "Runner").with_kind("runner");
  r.join_with_code(&a.open_join_code().unwrap()).unwrap();
  a.poll_once().unwrap();
  let ids = |e: &Engine| e.trust_state().unwrap().devices.iter().map(|d| d.id.clone()).collect::<Vec<_>>();
  let before = ids(&a);
  for n in 1..4 {
    clock.set(T0 + n * 1000);
    take_intros_by(&fake, &bid, &bid);
    a.poll_once().unwrap();
    assert!(a.decide_join(&my_id(&r), true).is_err());
  }
  assert_eq!(before, ids(&a));
}

#[test]
fn m16_no_v1_intro_is_signed_for_any_target_in_any_gate_state() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, m) = pair(&fake, &clock);
  let l = join_as(&fake, &clock, &a, "Desktop L", "desktop");
  let (lid, mid) = (my_id(&l), my_id(&m));
  let withheld = take_intros_by(&fake, &lid, &lid);
  let r = join_as(&fake, &clock, &a, "Runner", "runner");
  let rid = my_id(&r);
  m.poll_once().unwrap();
  assert_eq!(kind_pin(&m, &rid).as_deref(), Some("runner"));
  assert_eq!(kind_pin(&m, &lid).as_deref(), Some("desktop"));
  publish_for(&m);
  assert_eq!(fake.puts_by(&mid, &rid), [2]);
  assert!(fake.puts_by(&mid, &lid).iter().all(|v| *v == 2));
  for i in intros_of(&fake, &rid) {
    assert_eq!(parse_intro(&i).unwrap().parsed.version, 2);
  }
  restore_intros(&fake, &lid, withheld);
  publish_for(&m);
  let all = fake.0.lock().unwrap();
  assert!(all.puts.iter().all(|p| parse_intro(&p.2).unwrap().parsed.version == 2));
}

#[test]
fn m17_a_second_runner_joins_after_the_first_is_trusted() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, _b) = pair(&fake, &clock);
  let r1 = join_as(&fake, &clock, &a, "Runner 1", "runner");
  let r2 = join_as(&fake, &clock, &a, "Runner 2", "runner");
  for r in [&r1, &r2] {
    assert!(a.trust_state().unwrap().devices.iter().any(|d| d.id == my_id(r) && d.state == DeviceState::Trusted));
  }
}

#[test]
fn m18_a_cap_below_the_needed_passes_raises_trust_unstable_and_drops_the_moved_device() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, _b) = pair(&fake, &clock);
  let (_c, cid, c_fp) = registered(&fake, &clock, "Desktop C", "desktop");
  put_raw_intro(&fake, &my_id(&a), &cid, &signed_intro(&a, &cid, &c_fp, "desktop"));
  a.lock().pass_cap = Some(1);
  a.poll_once().unwrap();
  assert!(alert_kinds(&a).contains(&"trust_unstable".to_owned()));
  assert!(a.trust_state().unwrap().devices.iter().all(|d| d.id != cid));
  a.lock().pass_cap = None;
  a.poll_once().unwrap();
  assert!(trusted_with_kind(&a, &cid, "desktop"));
}

#[test]
fn a_legacy_state_without_a_runner_gate_can_still_join_runners_after_migration() {
  let (fake, clock, store, a, _x, _y) = legacy_owner_fleet();
  let aid = my_id(&a);
  let kinds: Value = {
    let g = a.lock();
    g.st.trusted.iter().map(|d| (d.id.clone(), json!("desktop"))).collect::<serde_json::Map<_, _>>().into()
  };
  legacy_state_with_kinds(&store, &a, kinds, vec![]);
  clock.set(T0 + 1000);
  let a2 = engine_on(&fake, &clock, store, "mx_local");
  a2.poll_once().unwrap();
  assert_eq!(a2.lock().st.kind_of(&aid, &fp_of(&a)), Some("desktop"));
  assert!(a2.lock().st.legacy_unknown.is_empty());
  let r = engine_with(&fake, &clock, "Runner").with_kind("runner");
  r.join_with_code(&a2.open_join_code().unwrap()).unwrap();
  a2.poll_once().unwrap();
  a2.decide_join(&my_id(&r), true).unwrap();
}

#[test]
fn m1_revoke_from_base_never_trusts_a_device_that_was_untrusted_before() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let (_c, cid, c_fp) = registered(&fake, &clock, "Desktop C", "desktop");
  let (_d, did, d_fp) = registered(&fake, &clock, "Desktop D", "desktop");
  let aid = my_id(&a);
  put_raw_intro(&fake, &aid, &cid, &signed_intro(&a, &cid, &c_fp, "desktop"));
  b.poll_once().unwrap();
  assert!(trusted_with_kind(&b, &cid, "desktop"));
  {
    let mut g = b.lock();
    let prefix = kind_key(&aid, "");
    g.st.kinds.retain(|k, _| !k.starts_with(&prefix));
    g.st.intros.retain(|i| parse_intro(i).is_ok_and(|p| p.parsed.target_id != aid));
  }
  fake.0.lock().unwrap().devices.iter_mut().find(|d| d.id == aid).unwrap().intros.clear();
  put_raw_intro(&fake, &aid, &did, &signed_intro(&a, &did, &d_fp, "desktop"));
  b.poll_once().unwrap();
  let trusted_ids = |e: &Engine| -> Vec<String> {
    e.trust_state().unwrap().devices.iter().filter(|d| d.state == DeviceState::Trusted).map(|d| d.id.clone()).collect()
  };
  let before = trusted_ids(&b);
  assert!(!before.contains(&did));
  {
    let mut g = b.lock();
    apply_revoke(&mut g, USER, T0, &cid, &c_fp, "C");
  }
  let after = trusted_ids(&b);
  assert!(after.iter().all(|id| before.contains(id)));
  assert!(!after.contains(&did));
  assert!(kind_entries_of(&b, &did).is_empty());
}

#[test]
fn m4_join_approval_refuses_a_kind_that_contradicts_a_known_one() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let n = engine_with(&fake, &clock, "Runner N").with_kind("runner");
  let code = a.open_join_code().unwrap();
  n.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  let (nid, n_fp) = (my_id(&n), fp_of(&n));
  a.lock().st.kinds.insert(kind_key(&nid, &n_fp), "desktop".to_owned());
  let intros_before = a.lock().st.intros.len();
  assert!(a.decide_join(&nid, true).is_err());
  assert_eq!(a.lock().st.intros.len(), intros_before);
  assert!(!trusted_with_kind(&a, &nid, "runner"));
  assert!(fake.0.lock().unwrap().devices.iter().find(|d| d.id == nid).unwrap().intros.is_empty());
}

#[test]
fn m3_a_device_admitted_only_by_a_demoted_signer_keeps_no_stored_kind() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let (x, xid, x_fp) = registered(&fake, &clock, "Desktop X", "desktop");
  let (_y, yid, y_fp) = registered(&fake, &clock, "Desktop Y", "desktop");
  let aid = my_id(&a);
  put_raw_intro(&fake, &aid, &xid, &signed_intro(&a, &xid, &x_fp, "desktop"));
  b.poll_once().unwrap();
  assert!(trusted_with_kind(&b, &xid, "desktop"));
  put_raw_intro(&fake, &xid, &yid, &signed_intro(&x, &yid, &y_fp, "desktop"));
  put_raw_intro(&fake, &aid, &xid, &signed_intro(&a, &xid, &x_fp, "runner"));
  b.poll_once().unwrap();
  assert!(trusted_with_kind(&b, &xid, "runner"));
  assert!(b.trust_state().unwrap().devices.iter().all(|d| d.id != yid || d.state != DeviceState::Trusted));
  assert!(kind_entries_of(&b, &yid).is_empty());
}

#[test]
fn join_code_view_carries_the_code_a_qr_and_the_ten_minute_expiry() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let view = crate::manager::join_code_view(&a).unwrap();
  assert!(view.code.starts_with("tmgr-chat-join:v2:"));
  assert!(view.qr.starts_with("data:image/svg+xml;base64,"));
  assert_eq!(view.expires_at_ms, T0 + 10 * 60 * 1000);
}

#[test]
fn a_code_from_the_view_pairs_a_second_device_through_the_port() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let b = engine_with(&fake, &clock, "Phone B");
  let view = crate::manager::join_code_view(&a).unwrap();
  RelayPort::join_with_code(&b, &format!("  {}\n", view.code)).unwrap();
  a.poll_once().unwrap();
  let bid = my_id(&b);
  a.decide_join(&bid, true).unwrap();
  b.poll_once().unwrap();
  assert_eq!(b.trust_state().unwrap().devices.iter().filter(|d| d.state == DeviceState::Trusted).count(), 2);
}

#[test]
fn join_with_a_bad_code_fails_without_echoing_it() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let b = engine_with(&fake, &clock, "Desktop B");
  let err = RelayPort::join_with_code(&b, "tmgr-chat-join:v2:SECRETSECRET").unwrap_err();
  assert_eq!(err, "the join code is not valid");
}

#[test]
fn without_a_relay_the_join_commands_say_chat_sync_is_not_set_up() {
  assert_eq!(crate::manager::relay::port::NoRelay.open_join_code().unwrap_err(), "chat sync is not set up");
  assert_eq!(crate::manager::relay::port::NoRelay.join_with_code("x").unwrap_err(), "chat sync is not set up");
}

#[test]
fn a_device_waiting_for_approval_shows_the_phone_as_pending_and_can_try_another_code() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a1 = engine_with(&fake, &clock, "Phone 1");
  let a2 = engine_with(&fake, &clock, "Phone 2");
  let b = engine_with(&fake, &clock, "Desktop B");
  let first = a1.open_join_code().unwrap();
  b.join_with_code(&first).unwrap();
  let waiting = b.trust_state().unwrap();
  assert!(waiting.joining && waiting.can_join_with_code);
  assert!(waiting.devices.iter().any(|d| !d.is_self && d.state == DeviceState::Pending));
  assert!(b.open_join_code().unwrap_err().contains("waiting"));
  let second = a2.open_join_code().unwrap();
  b.join_with_code(&second).unwrap();
  let state = b.trust_state().unwrap();
  assert_eq!(state.devices.len(), 2);
  assert!(state.devices.iter().any(|d| d.label == "Phone 2"));
  a2.poll_once().unwrap();
  let bid = my_id(&b);
  a2.decide_join(&bid, true).unwrap();
  b.poll_once().unwrap();
  let done = b.trust_state().unwrap();
  assert!(!done.joining && !done.can_join_with_code);
  assert!(done.devices.iter().all(|d| d.state == DeviceState::Trusted));
}

#[test]
fn a_device_refuses_its_own_join_code() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let code = a.open_join_code().unwrap();
  let err = a.join_with_code(&code).unwrap_err();
  assert!(err.contains("this device"));
  assert_eq!(a.trust_state().unwrap().devices.len(), 1);
}

#[test]
fn a_closed_join_code_cannot_be_used_and_raises_the_reuse_alert() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let b = engine_with(&fake, &clock, "Phone B");
  let code = a.open_join_code().unwrap();
  a.close_join_code(T0 + 10 * 60 * 1000).unwrap();
  b.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  let state = a.trust_state().unwrap();
  assert!(state.pending_joins.is_empty());
  assert!(state.alerts.iter().any(|x| x.kind == "join_code_reused"));
}

#[test]
fn closing_with_another_expiry_leaves_the_open_code_alone() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let b = engine_with(&fake, &clock, "Phone B");
  let code = a.open_join_code().unwrap();
  a.close_join_code(T0 + 1).unwrap();
  b.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  assert_eq!(a.trust_state().unwrap().pending_joins.len(), 1);
}

fn runner_ready(fake: &Fake, clock: &Clock) -> (Engine, String, String) {
  let r = engine_with(fake, clock, "Runner").with_kind("runner");
  r.register().unwrap();
  let (id, fp) = (my_id(&r), fp_of(&r));
  (r, id, fp)
}

fn is_trusted(e: &Engine, id: &str) -> bool {
  e.trust_state().unwrap().devices.iter().any(|d| d.id == id && d.state == DeviceState::Trusted)
}

#[test]
fn join_first_then_expectation_approves_once() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let (r, rid, fp) = runner_ready(&fake, &clock);
  r.join_with_code(&a.open_join_code().unwrap()).unwrap();
  a.poll_once().unwrap();
  assert_eq!(a.trust_state().unwrap().pending_joins.len(), 1);
  a.expect_join(&rid, &fp).unwrap();
  assert!(a.trust_state().unwrap().pending_joins.is_empty());
  assert!(is_trusted(&a, &rid));
  assert_eq!(fake.queue_len(&rid), 1);
  assert!(a.expect_join(&rid, &fp).is_err());
  a.poll_once().unwrap();
  assert_eq!(fake.queue_len(&rid), 1);
  r.poll_once().unwrap();
  assert!(r.lock().st.joined);
}

#[test]
fn expectation_first_then_join_approves_once() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let (r, rid, fp) = runner_ready(&fake, &clock);
  let code = a.open_join_code().unwrap();
  a.expect_join(&rid, &fp).unwrap();
  assert!(!is_trusted(&a, &rid));
  r.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  assert!(a.trust_state().unwrap().pending_joins.is_empty());
  assert!(is_trusted(&a, &rid));
  assert_eq!(fake.queue_len(&rid), 1);
  a.poll_once().unwrap();
  assert_eq!(fake.queue_len(&rid), 1);
  assert_eq!(kind_pin(&a, &rid).as_deref(), Some("runner"));
}

#[test]
fn a_mismatched_fp_falls_back_to_the_dialog_and_alerts() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let (r, rid, _) = runner_ready(&fake, &clock);
  a.register().unwrap();
  let other_fp = fp_of(&a);
  let code = a.open_join_code().unwrap();
  a.expect_join(&rid, &other_fp).unwrap();
  r.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  assert_eq!(a.trust_state().unwrap().pending_joins.len(), 1);
  assert!(!is_trusted(&a, &rid));
  assert!(alert_kinds(&a).contains(&"expected_join_mismatch".to_owned()));
}

#[test]
fn a_non_runner_with_the_expected_id_and_fp_falls_back_and_alerts() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let b = engine_with(&fake, &clock, "Desktop B");
  b.register().unwrap();
  let (bid, bfp) = (my_id(&b), fp_of(&b));
  let code = a.open_join_code().unwrap();
  a.expect_join(&bid, &bfp).unwrap();
  b.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  assert_eq!(a.trust_state().unwrap().pending_joins.len(), 1);
  assert!(!is_trusted(&a, &bid));
  assert!(alert_kinds(&a).contains(&"expected_join_mismatch".to_owned()));
}

#[test]
fn a_join_without_an_expectation_stays_pending_without_an_alert() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let (r, rid, _) = runner_ready(&fake, &clock);
  r.join_with_code(&a.open_join_code().unwrap()).unwrap();
  a.poll_once().unwrap();
  assert_eq!(a.trust_state().unwrap().pending_joins.len(), 1);
  assert!(!is_trusted(&a, &rid));
  assert!(alert_kinds(&a).is_empty());
}

#[test]
fn an_expectation_for_an_expired_or_used_code_is_refused() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let (_, rid, fp) = runner_ready(&fake, &clock);
  assert!(a.expect_join(&rid, &fp).is_err());
  a.open_join_code().unwrap();
  clock.set(T0 + 11 * 60 * 1000);
  assert!(a.expect_join(&rid, &fp).is_err());
  clock.set(T0 + 12 * 60 * 1000);
  let code = a.open_join_code().unwrap();
  let b = engine_with(&fake, &clock, "Desktop B");
  b.join_with_code(&code).unwrap();
  a.poll_once().unwrap();
  assert!(a.expect_join(&rid, &fp).is_err());
  assert!(a.expect_join("not-an-id", &fp).is_err());
}

#[test]
fn an_expectation_expires_with_its_code() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let (r, rid, fp) = runner_ready(&fake, &clock);
  let code = a.open_join_code().unwrap();
  a.expect_join(&rid, &fp).unwrap();
  r.join_with_code(&code).unwrap();
  clock.set(T0 + 11 * 60 * 1000);
  a.poll_once().unwrap();
  assert!(!is_trusted(&a, &rid));
}

#[test]
fn a_closed_gate_blocks_the_auto_approval_and_names_the_devices() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  let bid = my_id(&b);
  take_intros_by(&fake, &bid, &bid);
  let (r, rid, fp) = runner_ready(&fake, &clock);
  r.join_with_code(&a.open_join_code().unwrap()).unwrap();
  a.poll_once().unwrap();
  a.expect_join(&rid, &fp).unwrap();
  let ts = a.trust_state().unwrap();
  assert_eq!(ts.pending_joins.len(), 1);
  assert!(!is_trusted(&a, &rid));
  assert_eq!(ts.gate_blockers, ["Desktop B"]);
  let e = ts.auto_join_error.unwrap();
  assert!(e.contains("update these devices first") && e.contains("Desktop B"), "{e}");
  assert!(alert_kinds(&a).contains(&"auto_join_failed".to_owned()));
}

#[test]
fn gate_blockers_are_empty_with_a_healthy_fleet() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, _b) = pair(&fake, &clock);
  a.poll_once().unwrap();
  assert!(a.trust_state().unwrap().gate_blockers.is_empty());
  assert!(a.trust_state().unwrap().auto_join_error.is_none());
}

fn rooms_payload(machine: &str, role: &str, rooms: Value) -> Value {
  json!({ "v": 1, "machine": machine, "machine_label": "Studio", "role": role, "followers": if role == "top" { json!(["mx_fedcba9876543210"]) } else { json!([]) }, "rooms": rooms })
}

fn manager_room(machine: &str) -> Value {
  json!({ "room": { "kind": "manager", "machine": machine }, "title": "Manager", "state": "idle", "context_pct": 12, "last_ts": 1_763_800_000_000u64 })
}

fn session_room(machine: &str, session: &str) -> Value {
  json!({ "room": { "kind": "session", "machine": machine, "session": session }, "title": "fix login", "state": "running", "context_pct": null, "last_ts": null })
}

const IDX_ID: &str = "i_0123456789abcdef0123456789abcdef";

fn post_rooms(from: &Engine, to_id: &str, msg_id: &str, payload: &Value) {
  let mut g = from.lock();
  let target: Vec<Dev> = g.st.trusted.iter().filter(|d| d.id == to_id).cloned().collect();
  from.seal_and_post(&mut g, "rooms", msg_id, payload, &target, false).unwrap();
}

fn indexed(a: &Engine) -> Vec<String> {
  a.machine_indexes().into_iter().map(|m| m.machine).collect()
}

#[test]
fn a_valid_rooms_index_from_a_trusted_runner_is_stored_and_merged_into_the_room_list() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let r = join_as(&fake, &clock, &a, "Runner", "runner");
  let aid = my_id(&a);
  post_rooms(&r, &aid, IDX_ID, &rooms_payload("mx_0123456789abcdef", "top", json!([manager_room("mx_0123456789abcdef"), session_room("mx_0123456789abcdef", "s_abc")])));
  assert!(a.poll_once().unwrap().is_empty());
  let m = a.machine_indexes();
  assert_eq!((m.len(), m[0].label.as_str(), m[0].role.as_str(), m[0].followers.clone()), (1, "Studio", "top", vec!["mx_fedcba9876543210".to_owned()]));
  let rooms = a.rooms().unwrap();
  assert_eq!(rooms.len(), 2);
  assert_eq!(rooms[0].title, "Manager");
  assert_eq!(rooms[0].state.as_deref(), Some("idle"));
  assert_eq!(rooms[0].context_pct, Some(12));
  assert!(rooms.iter().all(|r| !r.local && r.last.is_none()));
  assert_eq!(rooms[1].title, "fix login");
}

#[test]
fn a_rooms_index_keeps_the_last_message_of_a_room_and_replaces_the_previous_index() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let r = join_as(&fake, &clock, &a, "Runner", "runner");
  let aid = my_id(&a);
  let mx = "mx_0123456789abcdef";
  let mut m = msg_n(2, "manager");
  m.room = RoomRef { kind: RoomKind::Manager, machine: mx.into(), session: None };
  post_chat(&r, &aid, &m);
  post_rooms(&r, &aid, IDX_ID, &rooms_payload(mx, "top", json!([manager_room(mx), session_room(mx, "s_abc")])));
  a.poll_once().unwrap();
  let rooms = a.rooms().unwrap();
  assert_eq!(rooms.len(), 2);
  assert_eq!(rooms.iter().find(|r| r.room.session.is_none()).unwrap().last.as_ref().unwrap().id, m.id);
  post_rooms(&r, &aid, "i_fedcba9876543210fedcba9876543210", &rooms_payload(mx, "sub", json!([manager_room(mx)])));
  a.poll_once().unwrap();
  assert_eq!(a.rooms().unwrap().len(), 1);
  assert_eq!(a.machine_indexes()[0].role, "sub");
}

#[test]
fn a_rooms_index_of_the_local_machine_adds_no_rooms_but_is_listed_as_a_machine() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let r = join_as(&fake, &clock, &a, "Runner", "runner");
  post_rooms(&r, &my_id(&a), IDX_ID, &rooms_payload("mx_local", "top", json!([manager_room("mx_local")])));
  a.poll_once().unwrap();
  assert!(a.rooms().unwrap().is_empty());
  assert_eq!(indexed(&a), ["mx_local"]);
}

#[test]
fn a_rooms_index_from_a_non_runner_is_ignored() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let (a, b) = pair(&fake, &clock);
  post_rooms(&b, &my_id(&a), IDX_ID, &rooms_payload("mx_0123456789abcdef", "top", json!([manager_room("mx_0123456789abcdef")])));
  a.poll_once().unwrap();
  assert!(indexed(&a).is_empty());
  assert!(a.rooms().unwrap().is_empty());
}

#[test]
fn a_rooms_index_for_a_machine_bound_to_another_runner_is_dropped_and_alerted() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let r1 = join_as(&fake, &clock, &a, "Runner 1", "runner");
  let r2 = join_as(&fake, &clock, &a, "Runner 2", "runner");
  let aid = my_id(&a);
  let mx = "mx_0123456789abcdef";
  post_rooms(&r1, &aid, IDX_ID, &rooms_payload(mx, "top", json!([manager_room(mx)])));
  a.poll_once().unwrap();
  post_rooms(&r2, &aid, "i_fedcba9876543210fedcba9876543210", &rooms_payload(mx, "top", json!([session_room(mx, "evil")])));
  a.poll_once().unwrap();
  assert_eq!(a.rooms().unwrap().len(), 1);
  assert_eq!(a.rooms().unwrap()[0].title, "Manager");
  assert!(alert_kinds(&a).contains(&"room_conflict".to_owned()));
}

#[test]
fn invalid_rooms_indexes_are_ignored() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let r = join_as(&fake, &clock, &a, "Runner", "runner");
  let aid = my_id(&a);
  let mx = "mx_0123456789abcdef";
  let mut bad: Vec<(String, Value)> = Vec::new();
  bad.push(("i_00000000000000000000000000000001".into(), rooms_payload(mx, "top", json!([manager_room("mx_other0000000000")]))));
  let mut v2 = rooms_payload(mx, "top", json!([manager_room(mx)]));
  v2["v"] = json!(2);
  bad.push(("i_00000000000000000000000000000002".into(), v2));
  bad.push(("i_00000000000000000000000000000003".into(), rooms_payload(mx, "boss", json!([]))));
  let mut empty_label = rooms_payload(mx, "top", json!([]));
  empty_label["machine_label"] = json!("");
  bad.push(("i_00000000000000000000000000000004".into(), empty_label));
  let mut ctl = rooms_payload(mx, "top", json!([]));
  ctl["machine_label"] = json!("a\nb");
  bad.push(("i_00000000000000000000000000000005".into(), ctl));
  let mut sub_followers = rooms_payload(mx, "sub", json!([]));
  sub_followers["followers"] = json!(["mx_fedcba9876543210"]);
  bad.push(("i_00000000000000000000000000000006".into(), sub_followers));
  bad.push(("i_00000000000000000000000000000007".into(), json!({ "nonsense": true })));
  bad.push(("i_00000000000000000000000000000008".into(), rooms_payload(mx, "top", json!([{ "room": { "kind": "session", "machine": mx }, "title": "x" }]))));
  let many: Vec<Value> = (0..201).map(|i| session_room(mx, &format!("s{i}"))).collect();
  bad.push(("i_00000000000000000000000000000009".into(), rooms_payload(mx, "top", Value::Array(many))));
  bad.push(("m_00000000000000000000000000000010".into(), rooms_payload(mx, "top", json!([manager_room(mx)]))));
  for (id, payload) in &bad {
    post_rooms(&r, &aid, id, payload);
  }
  a.poll_once().unwrap();
  assert!(indexed(&a).is_empty());
  assert!(a.rooms().unwrap().is_empty());
  assert!(a.trust_state().unwrap().alerts.is_empty());
  post_rooms(&r, &aid, "i_0000000000000000000000000000000a", &rooms_payload(mx, "top", json!([manager_room(mx)])));
  a.poll_once().unwrap();
  assert_eq!(indexed(&a), [mx]);
}

#[test]
fn a_revoked_runner_drops_out_of_the_machine_list() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let r = join_as(&fake, &clock, &a, "Runner", "runner");
  let (aid, rid) = (my_id(&a), my_id(&r));
  let mx = "mx_0123456789abcdef";
  post_rooms(&r, &aid, IDX_ID, &rooms_payload(mx, "top", json!([manager_room(mx)])));
  a.poll_once().unwrap();
  assert_eq!(indexed(&a), [mx]);
  a.revoke(&rid).unwrap();
  assert!(indexed(&a).is_empty());
  assert!(a.rooms().unwrap().is_empty());
}

#[test]
fn rooms_indexes_survive_a_restart() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let store = Arc::new(MemKeyStore::default());
  let a = engine_on(&fake, &clock, store.clone(), "mx_local");
  let r = join_as(&fake, &clock, &a, "Runner", "runner");
  let mx = "mx_0123456789abcdef";
  post_rooms(&r, &my_id(&a), IDX_ID, &rooms_payload(mx, "top", json!([manager_room(mx)])));
  a.poll_once().unwrap();
  clock.set(T0 + 31_000);
  a.poll_once().unwrap();
  let again = engine_on(&fake, &clock, store, "mx_local");
  assert_eq!(indexed(&again), [mx]);
}
