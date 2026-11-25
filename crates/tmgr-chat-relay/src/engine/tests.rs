use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use std::sync::atomic::AtomicBool;

use serde_json::{json, Value};
use tmgr_ipc::chat::{new_message_id, Author, MessageKind};

use super::*;
use crate::api::Method;
use crate::trust::{sign_spawnkey, SpawnStatement};

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
    delete_gone: bool,
    delete_unauthorized: bool,
    delete_404_listed: bool,
    posts: usize,
    pushes: Vec<bool>,
    last_seen: BTreeMap<String, u64>,
    puts: Vec<(String, String, String)>,
    fail_after_store: bool,
    gate: Option<Arc<Gate>>,
    log: Vec<String>,
    full: Option<String>,
    delete_error: Option<String>,
}

#[derive(Clone, Default)]
struct Fake(Arc<Mutex<Server>>);

impl Fake {
    fn puts_by(&self, by: &str, target: &str) -> Vec<u8> {
        self.0
            .lock()
            .unwrap()
            .puts
            .iter()
            .filter(|p| p.0 == by && p.1 == target)
            .map(|p| parse_intro(&p.2).unwrap().parsed.version)
            .collect()
    }

    fn queue_len(&self, to: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .msgs
            .iter()
            .filter(|m| m.to == to)
            .count()
    }

    fn first_body(&self, to: &str) -> (String, String, String) {
        let s = self.0.lock().unwrap();
        let m = s.msgs.iter().find(|m| m.to == to).unwrap();
        (m.msg_id.clone(), m.from.clone(), m.body.clone())
    }

    fn posts(&self) -> usize {
        self.0.lock().unwrap().posts
    }

    fn visible_text(&self) -> String {
        let s = self.0.lock().unwrap();
        let mut out = String::new();
        for m in &s.msgs {
            out.push_str(&format!("{} {} {} {}\n", m.msg_id, m.from, m.to, m.body));
        }
        for d in &s.devices {
            out.push_str(&format!("{} {} {}\n", d.id, d.kind, d.label));
        }
        for p in &s.puts {
            out.push_str(&format!("{} {} {}\n", p.0, p.1, p.2));
        }
        out
    }

    fn set_post_error(&self, e: Option<&str>) {
        self.0.lock().unwrap().post_error = e.map(str::to_owned);
    }

    fn set_full(&self, id: Option<&str>) {
        self.0.lock().unwrap().full = id.map(str::to_owned);
    }

    fn set_last_seen(&self, id: &str, ms: u64) {
        self.0.lock().unwrap().last_seen.insert(id.into(), ms);
    }

    fn set_dir_error(&self, on: bool) {
        self.0.lock().unwrap().dir_error = on;
    }

    fn add_revocation(&self, fp: &str, by: &str, statement: &str) {
        self.0
            .lock()
            .unwrap()
            .revocations
            .push((fp.into(), by.into(), statement.into()));
    }

    fn reverse_queue(&self, to: &str) {
        let mut s = self.0.lock().unwrap();
        let mut cursors: Vec<i64> = s
            .msgs
            .iter()
            .filter(|m| m.to == to)
            .map(|m| m.cursor)
            .collect();
        cursors.reverse();
        for (m, c) in s.msgs.iter_mut().filter(|m| m.to == to).zip(cursors) {
            m.cursor = c;
        }
        s.msgs.sort_by_key(|m| m.cursor);
    }

    fn drop_nth(&self, to: &str, n: usize) {
        let mut s = self.0.lock().unwrap();
        let idx = s
            .msgs
            .iter()
            .enumerate()
            .filter(|(_, m)| m.to == to)
            .map(|(i, _)| i)
            .nth(n)
            .unwrap();
        s.msgs.remove(idx);
    }

    fn inject(&self, to: &str, msg_id: &str, from: &str, body: &str) {
        let mut s = self.0.lock().unwrap();
        s.next += 1;
        let cursor = s.next;
        s.msgs.push(FMsg {
            cursor,
            msg_id: msg_id.into(),
            from: from.into(),
            to: to.into(),
            body: body.into(),
        });
    }
}

#[derive(Default)]
struct Gate {
    state: Mutex<(usize, bool)>,
    changed: std::sync::Condvar,
}

impl Gate {
    fn enter(&self) {
        let mut g = self.state.lock().unwrap();
        g.0 += 1;
        self.changed.notify_all();
        while !g.1 {
            g = self.changed.wait(g).unwrap();
        }
    }

    fn wait_entered(&self, n: usize) {
        let mut g = self.state.lock().unwrap();
        while g.0 < n {
            g = self.changed.wait(g).unwrap();
        }
    }

    fn open(&self) {
        self.state.lock().unwrap().1 = true;
        self.changed.notify_all();
    }
}

fn wrap(v: Value) -> Result<String, String> {
    Ok(json!({ "data": v }).to_string())
}

impl Transport for Fake {
    fn call(&self, req: &Request, token: Option<&str>) -> Result<String, String> {
        let gate = self.0.lock().unwrap().gate.clone();
        if let Some(gate) = gate {
            if !(req.method == Method::Post && req.path.ends_with("/messages")) {
                gate.enter();
            }
        }
        let mut s = self.0.lock().unwrap();
        let path = req.path.trim_start_matches("/api/chat-relay").to_owned();
        let caller = token.and_then(|t| {
            s.devices
                .iter()
                .find(|d| d.token == t)
                .map(|d| d.id.clone())
        });
        let body = req.body.clone().unwrap_or(Value::Null);
        let seg: Vec<&str> = path
            .split('?')
            .next()
            .unwrap()
            .trim_matches('/')
            .split('/')
            .collect();
        s.log
            .push(format!("{} {}", req.method.as_str(), seg.join("/")));
        match (req.method, seg.as_slice()) {
            (Method::Post, ["devices"]) => {
                let id = format!("00000000-0000-4000-8000-{:012x}", rand_n());
                let token = format!("tok-{id}");
                let g = |k: &str| body[k].as_str().unwrap().to_owned();
                s.devices.push(FDevice {
                    id: id.clone(),
                    token: token.clone(),
                    kind: g("kind"),
                    label: g("label"),
                    sign_pub: g("sign_pub"),
                    kex_pub: g("kex_pub"),
                    intros: vec![],
                });
                wrap(json!({ "device_id": id, "device_token": token }))
            }
            (Method::Get, ["devices"]) => {
                if caller.is_none() {
                    return Err(r#"{"http_status":401}"#.into());
                }
                if s.dir_error {
                    return Err("directory down".into());
                }
                let devices: Vec<Value> = s
          .devices
          .iter()
          .map(|d| json!({ "device_id": d.id, "kind": d.kind, "label": d.label, "sign_pub": d.sign_pub, "kex_pub": d.kex_pub, "intros": d.intros.iter().map(|i| i.1.clone()).collect::<Vec<_>>(), "last_seen_at": s.last_seen.get(&d.id) }))
          .collect();
                let revs: Vec<Value> = s
                    .revocations
                    .iter()
                    .map(|r| json!({ "target_fp": r.0, "statement": r.2 }))
                    .collect();
                wrap(json!({ "devices": devices, "revocations": revs }))
            }
            (Method::Put, ["devices", target, "intros", "self"]) => {
                let by = caller.ok_or("401")?;
                let intro = body["intro"].as_str().unwrap().to_owned();
                s.puts
                    .push((by.clone(), (*target).to_owned(), intro.clone()));
                let d = s
                    .devices
                    .iter_mut()
                    .find(|d| d.id == *target)
                    .ok_or("404")?;
                d.intros.retain(|i| i.0 != by);
                d.intros.push((by, intro));
                Ok(String::new())
            }
            (Method::Delete, ["devices", target]) => {
                if let Some(e) = s.delete_error.clone() {
                    return Err(e);
                }
                if s.delete_unauthorized {
                    s.devices.retain(|d| d.id != *target);
                    return Err(r#"{"http_status":401}"#.into());
                }
                if s.delete_404_listed {
                    return Err(r#"{"error":"not_found","http_status":404}"#.into());
                }
                if s.delete_gone {
                    s.devices.retain(|d| d.id != *target);
                    return Err(r#"{"error":"not_found","http_status":404}"#.into());
                }
                s.devices.retain(|d| d.id != *target);
                s.msgs.retain(|m| m.to != *target && m.from != *target);
                Ok(String::new())
            }
            (Method::Post, ["messages"]) => {
                let from = caller.ok_or("401")?;
                s.posts += 1;
                s.pushes.push(body["push"].as_bool().unwrap_or(false));
                if let Some(e) = s.post_error.clone() {
                    return Err(e);
                }
                let msg_id = body["msg_id"].as_str().unwrap().to_owned();
                let items = body["items"].as_array().unwrap().clone();
                if let Some(full) = s.full.clone() {
                    if items.iter().any(|i| i["to"].as_str() == Some(&full)) {
                        return Err(r#"{"error":"recipient_full"}"#.into());
                    }
                }
                if items
                    .iter()
                    .any(|i| !s.devices.iter().any(|d| d.id == i["to"].as_str().unwrap()))
                {
                    return Err(r#"{"error":"recipient_unknown"}"#.into());
                }
                let (mut stored, mut duplicates) = (0, 0);
                for i in items {
                    let to = i["to"].as_str().unwrap().to_owned();
                    if s.msgs
                        .iter()
                        .any(|m| m.to == to && m.from == from && m.msg_id == msg_id)
                    {
                        duplicates += 1;
                        continue;
                    }
                    s.next += 1;
                    let cursor = s.next;
                    s.msgs.push(FMsg {
                        cursor,
                        msg_id: msg_id.clone(),
                        from: from.clone(),
                        to,
                        body: i["body"].as_str().unwrap().into(),
                    });
                    stored += 1;
                }
                if s.fail_after_store {
                    return Err("timed out after the server stored the message".into());
                }
                wrap(json!({ "stored": stored, "duplicates": duplicates }))
            }
            (Method::Get, ["messages"]) => {
                let me = caller.ok_or("401")?;
                let after: i64 = path
                    .split("after=")
                    .nth(1)
                    .unwrap()
                    .split('&')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
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
                    s.revocations
                        .push((fp, by, body["statement"].as_str().unwrap().to_owned()));
                }
                Ok(String::new())
            }
            _ => Err(format!(
                "fake: unhandled {} {}",
                req.method.as_str(),
                req.path
            )),
        }
    }
}

fn rand_n() -> u64 {
    static N: AtomicU64 = AtomicU64::new(1 << 32);
    N.fetch_add(1, Ordering::SeqCst)
}

#[test]
fn fake_device_ids_never_collide_with_hand_written_ids() {
    let fixed = [
        "0000000000aa",
        "0000000000dd",
        "0000000000ee",
        "0000000000ff",
    ];
    for _ in 0..300 {
        let id = format!("{:012x}", rand_n());
        assert!(!fixed.contains(&id.as_str()), "fake handed out {id}");
    }
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

#[derive(Clone, Default)]
struct Rec {
    got: Arc<Mutex<Vec<(String, ChatMessage)>>>,
    kinds: Arc<Mutex<Vec<Option<String>>>>,
    fail: Arc<AtomicBool>,
    refuse_probes: Arc<AtomicBool>,
    probe_kinds: Arc<Mutex<Vec<Option<String>>>>,
    drops: Arc<Mutex<Vec<&'static str>>>,
    skipped: Arc<Mutex<Vec<(String, String)>>>,
    blobs: Arc<Mutex<Vec<(String, crate::engine::blob::BlobPart)>>>,
}

impl Rec {
    fn drops(&self) -> Vec<&'static str> {
        self.drops.lock().unwrap().clone()
    }

    fn bodies(&self) -> Vec<String> {
        self.got
            .lock()
            .unwrap()
            .iter()
            .map(|(_, m)| m.body_md.clone())
            .collect()
    }

    fn set_failing(&self, on: bool) {
        self.fail.store(on, Ordering::SeqCst);
    }
}

impl ChatSink for Rec {
    fn accepts_owner_input(&self, kind: Option<&str>) -> bool {
        self.probe_kinds
            .lock()
            .unwrap()
            .push(kind.map(str::to_owned));
        !self.refuse_probes.load(Ordering::SeqCst)
            && matches!(kind, Some("desktop" | "mobile" | "terminal"))
    }

    fn deliver(&self, from: &str, kind: Option<&str>, msg: ChatMessage) -> Result<(), String> {
        if self.fail.load(Ordering::SeqCst) {
            return Err("sink is down".into());
        }
        self.kinds.lock().unwrap().push(kind.map(str::to_owned));
        self.got.lock().unwrap().push((from.to_owned(), msg));
        Ok(())
    }

    fn deliver_blob(&self, from: &str, part: crate::engine::blob::BlobPart) -> Result<(), String> {
        if self.fail.load(Ordering::SeqCst) {
            return Err("sink is down".into());
        }
        self.blobs.lock().unwrap().push((from.to_owned(), part));
        Ok(())
    }

    fn dropped(&self, reason: &'static str) {
        self.drops.lock().unwrap().push(reason);
    }

    fn skipped_full(&self, device_id: &str, kind: &str) {
        self.skipped
            .lock()
            .unwrap()
            .push((device_id.into(), kind.into()));
    }
}

type Engine = RelayEngine<Fake, Arc<MemKeyStore>, Rec>;

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
    RelayEngine::new(
        fake.clone(),
        Arc::new(MemKeyStore::default()),
        Rec::default(),
        EngineConfig {
            user_id: USER,
            label: label.into(),
            local_machine: "mx_local".into(),
        },
    )
    .unwrap()
    .with_clock(move || c.load(Ordering::SeqCst))
}

fn room() -> RoomRef {
    RoomRef::manager("mx_remote")
}

fn owner() -> Author {
    Author {
        kind: AuthorKind::Owner,
        id: "owner".into(),
        machine: None,
        device: None,
        agent: None,
    }
}

fn text_msg(id: String, text: &str) -> ChatMessage {
    ChatMessage {
        v: 1,
        id,
        room: room(),
        author: owner(),
        ts_ms: T0,
        kind: MessageKind::Text,
        body_md: text.into(),
        attachments: vec![],
        reply_to: None,
        decision: None,
        session_meta: None,
        user_turn: false,
        mention: None,
    }
}

fn send<K: KeyStore>(e: &RelayEngine<Fake, K, Rec>, text: &str) -> Result<ChatMessage, String> {
    let m = text_msg(new_message_id(), text);
    e.send_chat(&m, true)?;
    Ok(m)
}

fn history<K: KeyStore>(e: &RelayEngine<Fake, K, Rec>) -> Vec<String> {
    e.s.bodies()
}

fn my_id<K: KeyStore>(e: &RelayEngine<Fake, K, Rec>) -> String {
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
fn is_joined_flips_when_join_ok_arrives() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let b = engine_with(&fake, &clock, "Desktop B");
    let code = a.open_join_code().unwrap();
    b.join_with_code(&code).unwrap();
    assert!(!b.is_joined());
    a.poll_once().unwrap();
    let bid = my_id(&b);
    a.decide_join(&bid, true).unwrap();
    b.poll_once().unwrap();
    assert!(b.is_joined());
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
    clock.set(T0 + 1000);
    send(&b, "reply from B").unwrap();
    assert_eq!(a.poll_once().unwrap().len(), 1);
    assert_eq!(history(&a), ["reply from B"]);
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
    clock.set(T0 + WRITE_EVERY_MS);
    a.poll_once().unwrap();
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
    assert!(b.trust_state().unwrap().devices.iter().any(|d| d.is_self));
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
fn revoke_succeeds_when_the_device_cleanup_is_refused() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let bid = my_id(&b);
    fake.0.lock().unwrap().delete_error = Some("the relay answered HTTP 401".into());
    a.revoke(&bid).unwrap();
    assert_eq!(fake.0.lock().unwrap().revocations.len(), 1);
    assert!(a
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .any(|d| d.id == bid && d.state == DeviceState::Revoked));
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
    assert!(a
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .any(|d| d.id == cid && d.state == DeviceState::Revoked));
    assert!(send(&a, "after revoke").is_ok());

    b.poll_once().unwrap();
    let ts = b.trust_state().unwrap();
    assert_eq!(ts.pending_revokes.len(), 1);
    assert_eq!(ts.pending_revokes[0].target_id, cid);
    assert_eq!(ts.pending_revokes[0].revoker_label, "Desktop A");
    assert!(ts
        .devices
        .iter()
        .any(|d| d.id == cid && d.state == DeviceState::Trusted));
    assert!(b.poll_once().unwrap().is_empty());
    b.poll_once().unwrap();
    assert_eq!(b.trust_state().unwrap().pending_revokes.len(), 1);

    b.decide_revoke(&ts.pending_revokes[0].statement_id, true)
        .unwrap();
    let ts = b.trust_state().unwrap();
    assert!(ts.pending_revokes.is_empty());
    assert!(ts
        .devices
        .iter()
        .any(|d| d.id == cid && d.state == DeviceState::Revoked));
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
    let sid = b.trust_state().unwrap().pending_revokes[0]
        .statement_id
        .clone();
    b.decide_revoke(&sid, false).unwrap();
    b.poll_once().unwrap();
    let ts = b.trust_state().unwrap();
    assert!(ts.pending_revokes.is_empty());
    assert!(ts
        .devices
        .iter()
        .any(|d| d.id == cid && d.state == DeviceState::Trusted));
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
    assert!(send(&a, "alone").is_err());

    assert!(b.poll_once().unwrap().is_empty());
    let bts = b.trust_state().unwrap();
    let new_a = my_id(&a);
    assert!(bts.devices.iter().all(|d| d.id != new_a));
    let bid = my_id(&b);
    let b_dev = b
        .lock()
        .st
        .trusted
        .iter()
        .find(|d| d.id == bid)
        .unwrap()
        .clone();
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
    assert_eq!(b.s.drops(), ["outside the time window"]);
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
    assert_eq!(
        b.trust_state().unwrap().key_mismatch,
        std::slice::from_ref(&aid)
    );
    let pinned = b
        .lock()
        .st
        .trusted
        .iter()
        .find(|d| d.id == aid)
        .unwrap()
        .fp
        .clone();
    assert_eq!(pinned, a.trust_state().unwrap().self_fp.unwrap());
}

#[test]
fn state_survives_a_restart() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let store = Arc::new(MemKeyStore::default());
    let cfg = || EngineConfig {
        user_id: USER,
        label: "Desktop A".into(),
        local_machine: "mx_local".into(),
    };
    let a = RelayEngine::new(fake.clone(), store.clone(), Rec::default(), cfg()).unwrap();
    a.register().unwrap();
    let fp = a.trust_state().unwrap().self_fp;
    let again = RelayEngine::new(fake.clone(), store, Rec::default(), cfg()).unwrap();
    let ts = again.trust_state().unwrap();
    assert!(ts.registered);
    assert_eq!(ts.self_fp, fp);
    let _ = clock;
}

#[test]
fn chat_validation_rejects_bad_ids_and_rooms() {
    let mut m = text_msg(format!("m_{}", "a".repeat(32)), "x");
    assert!(m.validate().is_ok());
    m.id = "m_short".into();
    assert!(m.validate().is_err());
    m.id = format!("m_{}", "a".repeat(32));
    m.room.session = Some("s".into());
    assert!(m.validate().is_err());
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
        assert_der_low_s(
            serde_json::from_str::<Value>(json).unwrap()["sig"]
                .as_str()
                .unwrap(),
        );
    }
    let intros: Vec<&String> = s
        .devices
        .iter()
        .flat_map(|d| d.intros.iter().map(|i| &i.1))
        .collect();
    assert!(!intros.is_empty());
    for i in intros {
        assert_der_low_s(
            serde_json::from_str::<Value>(i).unwrap()["sig"]
                .as_str()
                .unwrap(),
        );
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
    text_msg(format!("m_{}", "1".repeat(32)), "same id")
}

#[test]
fn bad_recipient_id_is_rejected_before_any_request() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, _b) = pair(&fake, &clock);
    {
        let mut g = a.lock();
        let mut bad =
            g.st.trusted
                .iter()
                .find(|d| d.id != my_id_of(&g))
                .unwrap()
                .clone();
        bad.id = "not-a-uuid".into();
        g.st.trusted.push(bad);
    }
    let posts = fake.posts();
    assert_eq!(send(&a, "x").unwrap_err(), "invalid recipient device id");
    assert_eq!(fake.posts(), posts);
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
    fake.set_post_error(None);
    send(&a, "x").unwrap();
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
        let target: Vec<Dev> =
            g.st.trusted
                .iter()
                .filter(|d| d.id == bid)
                .cloned()
                .collect();
        e.seal_and_post(&mut g, "chat", &msg.id, &payload, &target, false)
            .unwrap();
    }
    assert_eq!(fake.queue_len(&bid), 2);
    b.poll_once().unwrap();
    assert_eq!(
        b.lock().st.seen.iter().filter(|s| s.id == msg.id).count(),
        2
    );
    assert_eq!(
        b.s.got
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, m)| m.id == msg.id)
            .count(),
        2
    );
    assert_eq!(history(&b), ["same id", "same id"]);
    fake.reverse_queue(&bid);
    for e in [&a, &c] {
        let mut g = e.lock();
        let target: Vec<Dev> =
            g.st.trusted
                .iter()
                .filter(|d| d.id == bid)
                .cloned()
                .collect();
        e.seal_and_post(&mut g, "chat", &msg.id, &payload, &target, false)
            .unwrap();
    }
    b.poll_once().unwrap();
    assert_eq!(history(&b).len(), 2);
}

fn engine_on(fake: &Fake, clock: &Clock, store: Arc<MemKeyStore>, local: &str) -> Engine {
    let c = clock.0.clone();
    RelayEngine::new(
        fake.clone(),
        store,
        Rec::default(),
        EngineConfig {
            user_id: USER,
            label: "Desktop A".into(),
            local_machine: local.into(),
        },
    )
    .unwrap()
    .with_clock(move || c.load(Ordering::SeqCst))
}

fn fp_of(e: &Engine) -> String {
    e.lock().keys.as_ref().unwrap().fingerprint()
}

fn post_chat(from: &Engine, to_id: &str, msg: &ChatMessage) {
    let payload = serde_json::to_value(msg).unwrap();
    let mut g = from.lock();
    let target: Vec<Dev> =
        g.st.trusted
            .iter()
            .filter(|d| d.id == to_id)
            .cloned()
            .collect();
    from.seal_and_post(&mut g, "chat", &msg.id, &payload, &target, false)
        .unwrap();
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
    assert!(a
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .any(|d| d.id == cid && d.state == DeviceState::Trusted));
    let intros_before = a.lock().st.intros.len();
    assert!(intros_before > 0);

    let a2 = engine_on(&fake, &clock, store, "mx_local");
    {
        let mut g = a2.lock();
        assert!(g.dir.is_none());
        apply_revoke(
            &mut g,
            USER,
            T0,
            "00000000-0000-4000-8000-0000000000dd",
            &tmgr_phone::h("device d"),
            "D",
        );
    }
    let ts = a2.trust_state().unwrap();
    assert!(ts
        .devices
        .iter()
        .any(|d| d.id == cid && d.state == DeviceState::Trusted));
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
    assert!(a
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .all(|d| d.id != cid || d.state != DeviceState::Trusted));
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
    b.lock().st.trusted.push(
        c.lock()
            .st
            .trusted
            .iter()
            .find(|d| d.id == cid)
            .unwrap()
            .clone(),
    );
    post_chat(&b, &cid, &same_id_msg());
    a.decide_join(&cid, true).unwrap();
    assert!(c.poll_once().unwrap().is_empty());
    assert!(history(&c).is_empty());
    assert!(c
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .all(|d| d.id != bid || d.state != DeviceState::Trusted));
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
    fake.0
        .lock()
        .unwrap()
        .devices
        .iter_mut()
        .find(|d| d.id == id)
        .unwrap()
        .kind = kind.into();
}

fn msg_n(n: u8, kind: AuthorKind) -> ChatMessage {
    let mut m = same_id_msg();
    m.id = format!(
        "m_{}",
        n.to_string()
            .repeat(32)
            .chars()
            .take(32)
            .collect::<String>()
    );
    m.author.kind = kind;
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
    c.st.trusted
        .iter()
        .chain(c.st.base.iter())
        .find(|d| d.id == id)
        .map(|d| d.fp.clone())
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
    e.trust_state()
        .unwrap()
        .alerts
        .into_iter()
        .map(|a| a.kind)
        .collect()
}

#[test]
fn non_runner_cannot_author_manager_session_or_system_messages() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let bid = my_id(&b);
    for (n, k) in [
        (2, AuthorKind::Manager),
        (3, AuthorKind::Session),
        (4, AuthorKind::System),
    ] {
        post_chat(&a, &bid, &msg_n(n, k));
    }
    assert!(b.poll_once().unwrap().is_empty());
    post_chat(&a, &bid, &msg_n(5, AuthorKind::Owner));
    assert_eq!(b.poll_once().unwrap().len(), 1);
}

#[test]
fn runner_cannot_author_owner_messages_but_may_author_manager_messages() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let r = join_as(&fake, &clock, &a, "Runner", "runner");
    let aid = my_id(&a);
    post_chat(&r, &aid, &msg_n(2, AuthorKind::Owner));
    assert!(a.poll_once().unwrap().is_empty());
    assert!(a.s.kinds.lock().unwrap().is_empty());
    post_chat(&r, &aid, &msg_n(3, AuthorKind::Manager));
    assert_eq!(a.poll_once().unwrap().len(), 1);
}

#[test]
fn a_desktop_senders_chat_reaches_the_sink_with_its_kind() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    send(&b, "from the desktop").unwrap();
    a.poll_once().unwrap();
    assert_eq!(*a.s.kinds.lock().unwrap(), [Some("desktop".to_owned())]);
}

#[test]
fn a_sender_with_a_pending_revoke_reaches_the_sink_without_a_kind() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let c = engine_with(&fake, &clock, "Desktop C");
    c.join_with_code(&a.open_join_code().unwrap()).unwrap();
    a.poll_once().unwrap();
    let cid = my_id(&c);
    a.decide_join(&cid, true).unwrap();
    c.poll_once().unwrap();
    b.poll_once().unwrap();
    let bid = my_id(&b);
    send(&c, "sent before the revoke arrived").unwrap();
    let (msg_id, from, body) = fake.first_body(&bid);
    a.revoke(&cid).unwrap();
    b.poll_once().unwrap();
    assert_eq!(b.trust_state().unwrap().pending_revokes.len(), 1);
    assert!(b.s.kinds.lock().unwrap().is_empty());
    fake.inject(&bid, &msg_id, &from, &body);
    b.poll_once().unwrap();
    assert_eq!(*b.s.kinds.lock().unwrap(), [None]);
}

#[test]
fn owner_devices_lists_trusted_desktops_and_phones_but_not_runners_or_pending_revokes() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let c = engine_with(&fake, &clock, "Desktop C");
    c.join_with_code(&a.open_join_code().unwrap()).unwrap();
    a.poll_once().unwrap();
    let cid = my_id(&c);
    a.decide_join(&cid, true).unwrap();
    c.poll_once().unwrap();
    b.poll_once().unwrap();
    let r = join_as(&fake, &clock, &a, "Runner 1", "runner");
    b.poll_once().unwrap();
    let owners = b.owner_devices();
    assert!(owners.contains(&cid) && owners.contains(&my_id(&a)));
    assert!(!owners.contains(&my_id(&r)));
    a.revoke(&cid).unwrap();
    b.poll_once().unwrap();
    assert_eq!(b.trust_state().unwrap().pending_revokes.len(), 1);
    assert!(!b.owner_devices().contains(&cid));
}

#[test]
fn second_runner_claiming_a_bound_machine_is_dropped_and_alerted() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let r1 = join_as(&fake, &clock, &a, "Runner 1", "runner");
    let r2 = join_as(&fake, &clock, &a, "Runner 2", "runner");
    let aid = my_id(&a);
    post_chat(&r1, &aid, &msg_n(2, AuthorKind::Manager));
    assert_eq!(a.poll_once().unwrap().len(), 1);
    post_chat(&r2, &aid, &msg_n(3, AuthorKind::Manager));
    assert!(a.poll_once().unwrap().is_empty());
    assert_eq!(alert_kinds(&a), vec!["room_conflict".to_owned()]);
    post_chat(&r1, &aid, &msg_n(4, AuthorKind::Session));
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
    post_chat(&r1, &aid, &msg_n(2, AuthorKind::Manager));
    assert_eq!(a.poll_once().unwrap().len(), 1);
    clock.set(T0 + 31_000);
    a.poll_once().unwrap();
    let again = engine_on(&fake, &clock, store, "mx_local");
    post_chat(&r2, &aid, &msg_n(3, AuthorKind::Manager));
    assert!(again.poll_once().unwrap().is_empty());
    assert_eq!(alert_kinds(&again), vec!["room_conflict".to_owned()]);
}

#[test]
fn owns_follows_the_room_scope_and_is_false_when_the_local_machine_is_unknown() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let local_room = RoomRef::manager("mx_local");
    let a = engine_with(&fake, &clock, "Desktop A");
    assert!(a.owns(&room()));
    assert!(!a.owns(&local_room));
    let runner = engine_with(&fake, &clock, "Runner").with_room_scope(RoomScope::Local);
    assert!(runner.owns(&local_room));
    assert!(!runner.owns(&room()));
    let blank = engine_on(&fake, &clock, Arc::new(MemKeyStore::default()), "");
    assert!(!blank.owns(&room()));
    assert!(!blank.owns(&RoomRef::manager("")));
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
    assert_eq!(
        a.trust_state().unwrap().pending_joins[0].kind.as_deref(),
        Some("desktop")
    );
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
    let sid = b.trust_state().unwrap().pending_revokes[0]
        .statement_id
        .clone();
    fake.set_dir_error(true);
    assert!(b.decide_revoke(&sid, true).is_err());
    assert_eq!(b.trust_state().unwrap().pending_revokes.len(), 1);
    fake.set_dir_error(false);
    b.decide_revoke(&sid, true).unwrap();
    assert!(b.trust_state().unwrap().pending_revokes.is_empty());
}

fn counted(
    fake: &Fake,
    clock: &Clock,
    label: &str,
) -> (
    RelayEngine<Fake, Arc<CountingStore>, Rec>,
    Arc<CountingStore>,
) {
    let store = Arc::new(CountingStore::default());
    let c = clock.0.clone();
    let e = RelayEngine::new(
        fake.clone(),
        store.clone(),
        Rec::default(),
        EngineConfig {
            user_id: USER,
            label: label.into(),
            local_machine: "mx_local".into(),
        },
    )
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
        self.saves
            .lock()
            .unwrap()
            .iter()
            .filter(|s| **s == slot)
            .count()
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
        RevokeStatement {
            user_id: USER,
            revoker_fp: g.keys.as_ref().unwrap().fingerprint(),
            target_id: cid.clone(),
            target_fp: target.fp,
            issued_at_ms: T0,
        }
    };
    let der = sign_revoke(a.lock().keys.as_ref().unwrap(), &st).unwrap();
    let sig = serde_json::from_str::<Value>(&der).unwrap()["sig"]
        .as_str()
        .unwrap()
        .to_owned();
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
    let sig = serde_json::from_str::<Value>(&der).unwrap()["sig"]
        .as_str()
        .unwrap()
        .to_owned();
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
    assert!(b
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .any(|d| d.id == cid && d.state == DeviceState::Trusted));
}

fn forge_chat(from: &Engine, to: &Engine, msg: &ChatMessage, raw: bool) -> (String, String) {
    let (fid, tid) = (my_id(from), my_id(to));
    let g = from.lock();
    let keys = g.keys.as_ref().unwrap();
    let to_dev = g.st.trusted.iter().find(|d| d.id == tid).unwrap().clone();
    let inner = json!({ "v": 1, "kind": "chat", "id": msg.id, "sent_at": T0, "seq": 2, "payload": serde_json::to_value(msg).unwrap() }).to_string();
    let signed = crate::wire::signed_string(
        USER,
        &keys.fingerprint(),
        &to_dev.fp,
        &fid,
        &tid,
        "chat",
        &msg.id,
        T0,
        &inner,
    )
    .unwrap();
    let mut sig = tmgr_phone::sign(keys.sign_secret(), &signed).unwrap();
    if raw {
        sig = raw_sig(&sig);
    }
    let plain = json!({ "body": inner, "sig": sig }).to_string();
    let ctx = crate::wire::chat_ctx(USER, &fid, &tid, "chat", &msg.id).unwrap();
    let ent = tmgr_phone::Entropy {
        eph_secret: [9u8; 32],
        nonce: [3u8; 12],
    };
    let s = tmgr_phone::seal_ctx_with(
        &tmgr_phone::b64_decode(&to_dev.kex_pub).unwrap(),
        &ctx,
        "chat",
        &msg.id,
        plain.as_bytes(),
        &ent,
    )
    .unwrap();
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
    clock.set(T0 + 10_000);
    assert!(b.poll_once().unwrap().is_empty());
    clock.set(T0 + 16_000);
    assert_eq!(b.poll_once().unwrap().len(), 1);
    let mut shown = history(&b);
    shown.sort();
    assert_eq!(shown, ["one", "three"]);
    let gaps = b.trust_state().unwrap().seq_gaps;
    assert_eq!(
        gaps,
        [SeqGap {
            from_device: my_id(&a),
            expected: 3,
            got: 4
        }]
    );
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
    assert!(ts
        .devices
        .iter()
        .any(|d| d.id == old_id && d.state == DeviceState::Revoked));
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
fn polls_do_not_rewrite_the_state_more_than_once_per_30_seconds() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, _) = pair(&fake, &clock);
    let (b, store) = counted(&fake, &clock, "Desktop B");
    let code = a.open_join_code().unwrap();
    b.join_with_code(&code).unwrap();
    a.poll_once().unwrap();
    a.decide_join(&my_id(&b), true).unwrap();
    b.poll_once().unwrap();
    let states = store.count(Slot::State);
    let bid = my_id(&b);
    for i in 1..=5u64 {
        clock.set(T0 + i * 1000);
        send(&a, &format!("m{i}")).unwrap();
        assert_eq!(b.poll_once().unwrap().len(), 1);
    }
    assert_eq!(store.count(Slot::State), states);
    assert_eq!(fake.queue_len(&bid), 5);
    clock.set(T0 + 31_000);
    b.poll_once().unwrap();
    assert_eq!(store.count(Slot::State), states + 1);
    assert_eq!(fake.queue_len(&bid), 0);
    b.poll_once().unwrap();
    assert_eq!(store.count(Slot::State), states + 1);
    assert_eq!(b.s.bodies().len(), 5);
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
    let restarted = RelayEngine::new(
        fake.clone(),
        store,
        Rec::default(),
        EngineConfig {
            user_id: USER,
            label: "Desktop B".into(),
            local_machine: "mx_local".into(),
        },
    )
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
            g.st.seen.push(Seen {
                from: "x".into(),
                id: format!("old{i}"),
                at: T0,
            });
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
    e.trust_state()
        .unwrap()
        .devices
        .iter()
        .any(|d| d.id == id && d.state == DeviceState::Revoked)
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
    assert!(!a
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .any(|d| d.id == cid && d.state == DeviceState::Trusted));
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
    assert!(a
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .any(|d| d.id == rid && d.kind == "runner"));
    let aid = my_id(&a);
    post_chat(&r, &aid, &msg_n(2, AuthorKind::Owner));
    assert!(a.poll_once().unwrap().is_empty());
    post_chat(&r, &aid, &msg_n(3, AuthorKind::Manager));
    assert_eq!(a.poll_once().unwrap().len(), 1);
}

#[test]
fn directory_only_kind_is_unknown_and_cannot_author_owner_messages() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let (aid, bid) = (my_id(&a), my_id(&b));
    drop_pin(&a, &bid);
    a.lock().st.intros.clear();
    fake.0
        .lock()
        .unwrap()
        .devices
        .iter_mut()
        .find(|d| d.id == bid)
        .unwrap()
        .intros
        .clear();
    post_chat(&b, &aid, &msg_n(2, AuthorKind::Owner));
    assert!(a.poll_once().unwrap().is_empty());
    set_pin(&a, &bid, "desktop");
    post_chat(&b, &aid, &msg_n(3, AuthorKind::Owner));
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
    assert!(b
        .join_with_code(&code_with_kind(&a, Some("mobile")))
        .is_err());
    assert_eq!(b.trust_state().unwrap().devices.len(), 1);
    assert!(!kind_pin(&b, &aid).is_some());
    b.join_with_code(&code_with_kind(&a, Some("desktop")))
        .unwrap();
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
    let st = RevokeStatement {
        user_id: USER,
        revoker_fp: keys.fingerprint(),
        target_id: target_id.into(),
        target_fp: target_fp.into(),
        issued_at_ms: T0,
    };
    fake.add_revocation(target_fp, &my_id_of(&g), &sign_revoke(keys, &st).unwrap());
}

fn post_bogus_author(from: &Engine, to_id: &str, n: u8) {
    let msg = msg_n(n, AuthorKind::Manager);
    let mut payload = serde_json::to_value(&msg).unwrap();
    payload["author"]["kind"] = json!("bogus");
    let mut g = from.lock();
    let target: Vec<Dev> =
        g.st.trusted
            .iter()
            .filter(|d| d.id == to_id)
            .cloned()
            .collect();
    from.seal_and_post(&mut g, "chat", &msg.id, &payload, &target, false)
        .unwrap();
}

#[test]
fn unknown_author_kind_is_dropped_and_alerted() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let r = join_as(&fake, &clock, &a, "Runner", "runner");
    post_bogus_author(&r, &my_id(&a), 2);
    assert!(a.poll_once().unwrap().is_empty());
    assert!(alert_kinds(&a).contains(&"unknown_author_kind".to_owned()));
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    post_bogus_author(&a, &my_id(&b), 2);
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
    assert!(ts
        .devices
        .iter()
        .any(|d| d.id == cid && d.state == DeviceState::Trusted));
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
    assert_eq!(
        parse_seen(&json!(1_800_000_000_123u64)),
        Some(1_800_000_000_123)
    );
    assert_eq!(
        parse_seen(&json!("2027-01-15T08:00:00Z")),
        Some(1800000000000)
    );
    assert_eq!(parse_seen(&Value::Null), None);
}

fn publish_for(a: &Engine) {
    let mut g = a.lock();
    a.publish_intros(&mut g).unwrap();
}

fn intros_of(fake: &Fake, id: &str) -> Vec<String> {
    fake.0
        .lock()
        .unwrap()
        .devices
        .iter()
        .find(|d| d.id == id)
        .unwrap()
        .intros
        .iter()
        .map(|i| i.1.clone())
        .collect()
}

#[test]
fn no_intro_is_signed_for_a_device_without_a_signed_kind() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let bid = my_id(&b);
    drop_pin(&a, &bid);
    set_kind(&fake, &bid, "desktop");
    fake.0
        .lock()
        .unwrap()
        .devices
        .iter_mut()
        .find(|d| d.id == bid)
        .unwrap()
        .intros
        .clear();
    publish_for(&a);
    assert!(intros_of(&fake, &bid).is_empty());
}

#[test]
fn directory_kind_never_ends_up_in_a_signed_intro() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let bid = my_id(&b);
    set_kind(&fake, &bid, "runner");
    a.lock()
        .st
        .trusted
        .iter_mut()
        .find(|d| d.id == bid)
        .unwrap()
        .kind = "runner".into();
    fake.0
        .lock()
        .unwrap()
        .devices
        .iter_mut()
        .find(|d| d.id == bid)
        .unwrap()
        .intros
        .clear();
    publish_for(&a);
    let intros = intros_of(&fake, &bid);
    assert_eq!(intros.len(), 1);
    assert_eq!(
        parse_intro(&intros[0]).unwrap().parsed.target_kind,
        "desktop"
    );
}

#[test]
fn trust_state_marks_only_signed_kinds_as_verified() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let bid = my_id(&b);
    let verified = |e: &Engine| {
        e.trust_state()
            .unwrap()
            .devices
            .iter()
            .find(|d| d.id == bid)
            .unwrap()
            .kind_verified
    };
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
    assert_eq!(
        a.trust_state()
            .unwrap()
            .devices
            .iter()
            .filter(|d| d.id == cid)
            .count(),
        1
    );
}

#[test]
fn ack_waits_for_the_sink_and_a_redelivery_is_deduped_once_accepted() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let bid = my_id(&b);
    send(&a, "one").unwrap();
    send(&a, "two").unwrap();
    let (msg_id, from, body) = fake.first_body(&bid);
    clock.set(T0 + 31_000);
    b.s.set_failing(true);
    assert_eq!(b.poll_once().unwrap_err(), "sink is down");
    assert_eq!(fake.queue_len(&bid), 2);
    assert!(history(&b).is_empty());
    b.s.set_failing(false);
    clock.set(T0 + 62_000);
    assert_eq!(b.poll_once().unwrap().len(), 2);
    assert_eq!(history(&b), ["one", "two"]);
    assert_eq!(fake.queue_len(&bid), 0);
    fake.inject(&bid, &msg_id, &from, &body);
    assert!(b.poll_once().unwrap().is_empty());
    assert_eq!(history(&b), ["one", "two"]);
}

#[test]
fn send_chat_seals_to_every_trusted_device_but_self_for_any_author_kind() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let r = join_as(&fake, &clock, &a, "Runner", "runner");
    let b = join_as(&fake, &clock, &a, "Desktop B", "desktop");
    r.poll_once().unwrap();
    let (aid, bid, rid) = (my_id(&a), my_id(&b), my_id(&r));
    assert_eq!(r.trust_state().unwrap().devices.len(), 3);
    let before = (
        fake.queue_len(&aid),
        fake.queue_len(&bid),
        fake.queue_len(&rid),
    );
    let mut msg = text_msg(new_message_id(), "plan is ready");
    msg.author = Author {
        kind: AuthorKind::Manager,
        id: "s_manager01".into(),
        machine: None,
        device: None,
        agent: Some("claude-code".into()),
    };
    r.send_chat(&msg, true).unwrap();
    assert_eq!(
        (
            fake.queue_len(&aid),
            fake.queue_len(&bid),
            fake.queue_len(&rid)
        ),
        (before.0 + 1, before.1 + 1, before.2)
    );
    for e in [&a, &b] {
        let got = e.poll_once().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0], msg);
    }
    assert_eq!(a.s.got.lock().unwrap().last().unwrap().0, rid);
}

#[test]
fn send_chat_refuses_an_invalid_message_before_any_request() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, _b) = pair(&fake, &clock);
    let posts = fake.posts();
    assert!(a.send_chat(&text_msg("m_short".into(), "x"), true).is_err());
    assert_eq!(fake.posts(), posts);
}

fn put_foreign_intro(fake: &Fake, signer: &Engine, target_id: &str, target_fp: &str, kind: &str) {
    let st = IntroStatement {
        version: 2,
        user_id: USER,
        introducer_fp: fp_of(signer),
        target_id: target_id.to_owned(),
        target_fp: target_fp.to_owned(),
        target_kind: kind.into(),
        issued_at_ms: T0,
    };
    let sid = my_id(signer);
    let intro = sign_intro(signer.lock().keys.as_ref().unwrap(), &sid, &st).unwrap();
    let mut s = fake.0.lock().unwrap();
    let d = s.devices.iter_mut().find(|d| d.id == target_id).unwrap();
    d.intros.push((sid, intro));
}

#[test]
fn a_runner_signed_intro_adds_no_device() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let r = join_as(&fake, &clock, &a, "Runner", "runner");
    let x = engine_with(&fake, &clock, "Desktop X");
    x.register().unwrap();
    let xid = my_id(&x);
    let x_fp = x.lock().st.trusted[0].fp.clone();
    put_foreign_intro(&fake, &r, &xid, &x_fp, "desktop");
    a.poll_once().unwrap();
    r.poll_once().unwrap();
    for e in [&a, &r] {
        assert!(e.trust_state().unwrap().devices.iter().all(|d| d.id != xid));
    }
    assert!(!kind_pin(&a, &xid).is_some());
}

#[test]
fn a_desktop_signed_intro_still_adds_a_device() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let x = engine_with(&fake, &clock, "Desktop X");
    x.register().unwrap();
    let xid = my_id(&x);
    let x_fp = x.lock().st.trusted[0].fp.clone();
    put_foreign_intro(&fake, &a, &xid, &x_fp, "desktop");
    b.poll_once().unwrap();
    assert!(b
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .any(|d| d.id == xid && d.state == DeviceState::Trusted && d.kind == "desktop"));
}

#[test]
fn a_runner_signed_kind_is_not_pinned() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let r = join_as(&fake, &clock, &a, "Runner", "runner");
    let bid = my_id(&b);
    for d in fake.0.lock().unwrap().devices.iter_mut() {
        d.intros.clear();
    }
    a.lock().st.intros.clear();
    drop_pin(&a, &bid);
    let b_fp = fp_of(&b);
    put_foreign_intro(&fake, &r, &bid, &b_fp, "desktop");
    a.poll_once().unwrap();
    assert!(!kind_pin(&a, &bid).is_some());
}

#[test]
fn a_runner_neither_publishes_intros_nor_opens_join_codes() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let r = join_as(&fake, &clock, &a, "Runner", "runner");
    let rid = my_id(&r);
    assert!(fake.0.lock().unwrap().puts.iter().all(|p| p.0 != rid));
    assert!(r.open_join_code().is_err());
}

#[test]
fn a_runner_seals_only_to_desktop_and_mobile_devices() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let r1 = join_as(&fake, &clock, &a, "Runner 1", "runner");
    let r2 = join_as(&fake, &clock, &a, "Runner 2", "runner");
    r1.poll_once().unwrap();
    let (aid, r2id) = (my_id(&a), my_id(&r2));
    assert!(r1
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .any(|d| d.id == r2id));
    let before = (fake.queue_len(&aid), fake.queue_len(&r2id));
    r1.send_chat(&text_msg(new_message_id(), "x"), false)
        .unwrap();
    assert_eq!(
        (fake.queue_len(&aid), fake.queue_len(&r2id)),
        (before.0 + 1, before.1)
    );
}

#[test]
fn send_chat_posts_with_the_given_push_flag() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, _b) = pair(&fake, &clock);
    a.send_chat(&text_msg(new_message_id(), "quiet"), false)
        .unwrap();
    a.send_chat(&text_msg(new_message_id(), "loud"), true)
        .unwrap();
    let s = fake.0.lock().unwrap();
    assert_eq!(&s.pushes[s.pushes.len() - 2..], [false, true]);
}

#[test]
fn reset_pending_join_lets_a_fresh_code_post_again() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let b = engine_with(&fake, &clock, "Desktop B");
    b.join_with_code(&a.open_join_code().unwrap()).unwrap();
    assert!(!b.is_joined());
    b.reset_pending_join().unwrap();
    assert!(b.is_joined());
    assert_eq!(b.trust_state().unwrap().devices.len(), 1);
    let posts = fake.posts();
    b.join_with_code(&a.open_join_code().unwrap()).unwrap();
    assert_eq!(fake.posts(), posts + 1);
}

fn put_raw_intro(fake: &Fake, by_id: &str, target_id: &str, intro: &str) {
    let mut s = fake.0.lock().unwrap();
    let d = s.devices.iter_mut().find(|d| d.id == target_id).unwrap();
    d.intros.push((by_id.to_owned(), intro.to_owned()));
}

fn signed_intro(signer: &Engine, target_id: &str, target_fp: &str, kind: &str) -> String {
    signed_intro_v(2, signer, target_id, target_fp, kind)
}

fn signed_intro_v(
    version: u8,
    signer: &Engine,
    target_id: &str,
    target_fp: &str,
    kind: &str,
) -> String {
    let st = IntroStatement {
        version,
        user_id: USER,
        introducer_fp: fp_of(signer),
        target_id: target_id.to_owned(),
        target_fp: target_fp.to_owned(),
        target_kind: kind.into(),
        issued_at_ms: T0,
    };
    {
        let sid = my_id(signer);
        sign_intro(signer.lock().keys.as_ref().unwrap(), &sid, &st).unwrap()
    }
}

fn clear_intros(fake: &Fake, e: &Engine) {
    for d in fake.0.lock().unwrap().devices.iter_mut() {
        d.intros.clear();
    }
    e.lock().st.intros.clear();
}

fn is_trusted_with(e: &Engine, id: &str, kind: &str) -> bool {
    e.trust_state()
        .unwrap()
        .devices
        .iter()
        .any(|d| d.id == id && d.state == DeviceState::Trusted && d.kind == kind && d.kind_verified)
}

fn registered(fake: &Fake, clock: &Clock, label: &str, kind: &str) -> (Engine, String, String) {
    let e = engine_with(fake, clock, label).with_kind(kind);
    e.register().unwrap();
    let (id, fp) = (my_id(&e), fp_of(&e));
    (e, id, fp)
}

#[test]
fn forged_directory_intros_pin_no_kind() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let r = join_as(&fake, &clock, &a, "Runner", "runner");
    let (aid, bid, b_fp) = (my_id(&a), my_id(&b), fp_of(&b));
    clear_intros(&fake, &a);
    drop_pin(&a, &bid);
    let bad_sig = signed_intro(&a, &bid, &b_fp, "mobile");
    let mut v: Value = serde_json::from_str(&signed_intro(&a, &bid, &b_fp, "desktop")).unwrap();
    v["sig"] = serde_json::from_str::<Value>(&bad_sig).unwrap()["sig"].clone();
    put_raw_intro(&fake, &aid, &bid, &v.to_string());
    let by_runner = signed_intro(&r, &bid, &b_fp, "desktop");
    let mut v: Value = serde_json::from_str(&by_runner).unwrap();
    v["by"] = json!(aid);
    put_raw_intro(&fake, &aid, &bid, &v.to_string());
    a.poll_once().unwrap();
    assert!(!kind_pin(&a, &bid).is_some());
}

#[test]
fn a_runner_intro_listed_first_does_not_block_the_desktop_intro_for_the_same_target() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let r = join_as(&fake, &clock, &a, "Runner", "runner");
    let (_x, xid, x_fp) = registered(&fake, &clock, "Desktop X", "desktop");
    put_raw_intro(
        &fake,
        &my_id(&r),
        &xid,
        &signed_intro(&r, &xid, &x_fp, "desktop"),
    );
    put_raw_intro(
        &fake,
        &my_id(&a),
        &xid,
        &signed_intro(&a, &xid, &x_fp, "mobile"),
    );
    a.poll_once().unwrap();
    assert!(is_trusted_with(&a, &xid, "mobile"));
}

#[test]
fn a_chain_of_intros_resolves_in_one_pass_whatever_the_order() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    a.register().unwrap();
    let (m, mid, m_fp) = registered(&fake, &clock, "Phone M", "mobile");
    let (_y, yid, y_fp) = registered(&fake, &clock, "Desktop Y", "desktop");
    put_raw_intro(&fake, &mid, &yid, &signed_intro(&m, &yid, &y_fp, "desktop"));
    put_raw_intro(
        &fake,
        &my_id(&a),
        &mid,
        &signed_intro(&a, &mid, &m_fp, "mobile"),
    );
    a.poll_once().unwrap();
    assert!(is_trusted_with(&a, &mid, "mobile"));
    assert!(is_trusted_with(&a, &yid, "desktop"));
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

#[test]
fn an_intro_from_a_signer_without_a_kind_counts_once_the_kind_is_learned() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let bid = my_id(&b);
    clear_intros(&fake, &a);
    drop_pin(&a, &bid);
    let (_x, xid, x_fp) = registered(&fake, &clock, "Desktop X", "desktop");
    put_raw_intro(&fake, &bid, &xid, &signed_intro(&b, &xid, &x_fp, "desktop"));
    a.poll_once().unwrap();
    assert!(a.trust_state().unwrap().devices.iter().all(|d| d.id != xid));
    set_pin(&a, &bid, "desktop");
    a.poll_once().unwrap();
    assert!(is_trusted_with(&a, &xid, "desktop"));
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
        let raw = serde_json::to_string(&g.st).unwrap();
        store.save(Slot::State, &raw).unwrap();
    }
    let a2 = engine_on(&fake, &clock, store, "mx_local");
    a2.register().unwrap();
    {
        let mut g = a2.lock();
        apply_revoke(
            &mut g,
            USER,
            T0,
            "00000000-0000-4000-8000-0000000000dd",
            &tmgr_phone::h("device d"),
            "D",
        );
    }
    assert!(a2
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .any(|d| d.id == bid && d.state == DeviceState::Trusted));
}

#[test]
fn a_revoke_with_an_unknown_root_kind_keeps_the_pins_and_alerts() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let (_c, cid, c_fp) = registered(&fake, &clock, "Desktop C", "desktop");
    let aid = my_id(&a);
    put_raw_intro(&fake, &aid, &cid, &signed_intro(&a, &cid, &c_fp, "desktop"));
    b.poll_once().unwrap();
    assert!(is_trusted_with(&b, &cid, "desktop"));
    {
        let mut g = b.lock();
        let prefix = kind_key(&aid, "");
        g.st.kinds.retain(|k, _| !k.starts_with(&prefix));
        g.dir = None;
        g.st.intros
            .retain(|i| parse_intro(i).is_ok_and(|p| p.parsed.target_id == cid));
        apply_revoke(
            &mut g,
            USER,
            T0,
            "00000000-0000-4000-8000-0000000000dd",
            &tmgr_phone::h("device d"),
            "D",
        );
    }
    assert!(alert_kinds(&b).contains(&"root_kind_unknown".to_owned()));
    assert!(is_trusted_with(&b, &cid, "desktop"));
    assert!(b
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .any(|d| d.id == aid && d.state == DeviceState::Trusted));
}

#[test]
fn a_pending_join_is_recognised_by_its_code_and_forgotten_on_reset() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let b = engine_with(&fake, &clock, "Desktop B");
    let code = a.open_join_code().unwrap();
    assert!(!b.pending_join_matches(&code));
    b.join_with_code(&code).unwrap();
    assert!(b.pending_join_matches(&code));
    let other = engine_with(&fake, &clock, "Desktop O");
    assert!(!b.pending_join_matches(&other.open_join_code().unwrap()));
    let posts = fake.posts();
    a.poll_once().unwrap();
    a.decide_join(&my_id(&b), true).unwrap();
    b.poll_once().unwrap();
    assert!(b.is_joined());
    assert!(!b.pending_join_matches(&code));
    assert!(fake.posts() > posts);
    let c = engine_with(&fake, &clock, "Desktop C");
    c.join_with_code(&code).unwrap();
    c.reset_pending_join().unwrap();
    assert!(!c.pending_join_matches(&code));
}

#[test]
fn a_runner_cannot_decide_joins_and_drops_a_leftover_join_code() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let store = Arc::new(MemKeyStore::default());
    let a = engine_with(&fake, &clock, "Desktop A");
    let r = join_as(&fake, &clock, &a, "Runner", "runner");
    {
        let mut g = r.lock();
        g.st.join_code = Some(OpenCode {
            secret: "s".into(),
            exp_ms: T0 + 1000,
        });
        g.st.pending_joins.push(JoinRec {
            device_id: "00000000-0000-4000-8000-0000000000ee".into(),
            label: "X".into(),
            fp: tmgr_phone::h("x"),
            kind: "desktop".into(),
            requested_at_ms: T0,
            v: 2,
        });
    }
    assert!(r
        .decide_join("00000000-0000-4000-8000-0000000000ee", true)
        .is_err());
    {
        let g = r.lock();
        store
            .save(Slot::State, &serde_json::to_string(&g.st).unwrap())
            .unwrap();
        store
            .save(
                Slot::Identity,
                &serde_json::to_string(g.identity.as_ref().unwrap()).unwrap(),
            )
            .unwrap();
    }
    let r2 = engine_on(&fake, &clock, store, "mx_local").with_kind("runner");
    assert!(r2.lock().st.join_code.is_none());
}

fn kind_entries_of(e: &Engine, id: &str) -> Vec<String> {
    let prefix = kind_key(id, "");
    e.lock()
        .st
        .kinds
        .keys()
        .filter(|k| k.starts_with(&prefix))
        .cloned()
        .collect()
}

fn rename_device(fake: &Fake, e: &Engine, to: &str) {
    let from = my_id(e);
    fake.0
        .lock()
        .unwrap()
        .devices
        .iter_mut()
        .find(|d| d.id == from)
        .unwrap()
        .id = to.to_owned();
    e.lock().identity.as_mut().unwrap().device_id = to.to_owned();
}

#[test]
fn a_revoked_id_reused_by_a_runner_with_new_keys_inherits_no_kind_and_is_refused() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let (aid, bid) = (my_id(&a), my_id(&b));
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
    assert!(a
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .all(|d| d.id != bid || d.state == DeviceState::Revoked));
    assert!(kind_entries_of(&a, &bid).is_empty());
    assert!(alert_kinds(&a).contains(&"kind_conflict".to_owned()));
    post_chat_untrusted(&r, &aid);
    assert!(a.poll_once().unwrap().is_empty());
}

fn post_chat_untrusted(from: &Engine, to_id: &str) {
    let msg = msg_n(2, AuthorKind::Owner);
    let payload = serde_json::to_value(&msg).unwrap();
    let mut g = from.lock();
    let target: Vec<Dev> =
        g.st.base
            .iter()
            .filter(|d| d.id == to_id)
            .cloned()
            .collect();
    from.seal_and_post(&mut g, "chat", &msg.id, &payload, &target, false)
        .unwrap();
}

#[test]
fn a_legacy_state_keeps_the_root_kind_and_the_migration_runs_once() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let store = Arc::new(MemKeyStore::default());
    let a = engine_with(&fake, &clock, "Desktop A");
    let b = engine_on(&fake, &clock, store.clone(), "mx_local");
    b.join_with_code(&a.open_join_code().unwrap()).unwrap();
    a.poll_once().unwrap();
    a.decide_join(&my_id(&b), true).unwrap();
    b.poll_once().unwrap();
    let (aid, a_fp) = (my_id(&a), fp_of(&a));
    {
        let g = b.lock();
        let mut v: Value = serde_json::to_value(&g.st).unwrap();
        let own = my_id_of(&g);
        v["kinds"] = json!({ aid.clone(): "desktop", own: "desktop", "00000000-0000-4000-8000-0000000000ff": "desktop" });
        v.as_object_mut().unwrap().remove("kinds_v");
        store.save(Slot::State, &v.to_string()).unwrap();
    }
    let b2 = engine_on(&fake, &clock, store.clone(), "mx_local");
    assert_eq!(b2.lock().st.kind_of(&aid, &a_fp), Some("desktop"));
    assert_eq!(b2.lock().st.kinds.len(), 2);
    b2.poll_once().unwrap();
    let raw = store.load(Slot::State).unwrap().unwrap();
    let again = engine_on(&fake, &clock, store, "mx_local");
    assert_eq!(again.lock().st.kinds_v, KINDS_V);
    assert!(raw.contains("\"kinds_v\":3"));
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
    assert!(!b.is_joined());
    assert!(kind_entries_of(&b, &my_id(&a))
        .iter()
        .all(|k| b.lock().st.kinds[k] != "bogus"));
}

#[test]
fn a_join_ok_for_a_root_id_pinned_with_other_keys_is_rejected_and_alerted() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let b = engine_with(&fake, &clock, "Desktop B");
    b.join_with_code(&a.open_join_code().unwrap()).unwrap();
    a.poll_once().unwrap();
    a.decide_join(&my_id(&b), true).unwrap();
    let aid = my_id(&a);
    b.lock()
        .st
        .kinds
        .insert(kind_key(&aid, &"1".repeat(64)), "desktop".into());
    b.poll_once().unwrap();
    assert!(!b.is_joined());
    assert!(alert_kinds(&b).contains(&"kind_conflict".to_owned()));
}

#[test]
fn a_repeated_kind_conflict_does_not_touch_the_state_or_the_alert_time() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let (_x, xid, x_fp) = registered(&fake, &clock, "Desktop X", "desktop");
    put_raw_intro(
        &fake,
        &my_id(&a),
        &xid,
        &signed_intro(&a, &xid, &x_fp, "desktop"),
    );
    put_raw_intro(
        &fake,
        &my_id(&b),
        &xid,
        &signed_intro(&b, &xid, &x_fp, "mobile"),
    );
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
fn a_missing_root_kind_is_alerted_once_by_the_directory_sync() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let aid = my_id(&a);
    clear_intros(&fake, &b);
    drop_pin(&b, &aid);
    b.poll_once().unwrap();
    let at = |e: &Engine| {
        e.lock()
            .st
            .alerts
            .iter()
            .find(|x| x.kind == "root_kind_unknown")
            .map(|x| x.at_ms)
    };
    assert_eq!(at(&b), Some(T0));
    clock.set(T0 + 5000);
    b.poll_once().unwrap();
    assert_eq!(at(&b), Some(T0));
    assert_eq!(
        b.lock()
            .st
            .alerts
            .iter()
            .filter(|x| x.kind == "root_kind_unknown")
            .count(),
        1
    );
}

#[test]
fn a_failed_join_keeps_a_kind_that_was_pinned_before_and_drops_one_it_added() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let b = engine_with(&fake, &clock, "Desktop B");
    let (aid, a_fp) = (my_id_after_open(&a), fp_of(&a));
    let code = a.open_join_code().unwrap();
    fake.set_post_error(Some(r#"{"error":"recipient_full"}"#));
    assert!(b.join_with_code(&code).is_err());
    assert!(kind_entries_of(&b, &aid).is_empty());
    b.lock()
        .st
        .kinds
        .insert(kind_key(&aid, &a_fp), "desktop".into());
    assert!(b.join_with_code(&code).is_err());
    assert_eq!(
        b.lock()
            .st
            .kinds
            .get(&kind_key(&aid, &a_fp))
            .map(String::as_str),
        Some("desktop")
    );
}

fn my_id_after_open(a: &Engine) -> String {
    a.register().unwrap();
    my_id(a)
}

fn legacy_state_with_kinds(
    store: &Arc<MemKeyStore>,
    e: &Engine,
    kinds: Value,
    intros: Vec<String>,
) {
    let g = e.lock();
    let mut v: Value = serde_json::to_value(&g.st).unwrap();
    v["kinds"] = kinds;
    v.as_object_mut().unwrap().remove("kinds_v");
    v["intros"] = json!(intros);
    store.save(Slot::State, &v.to_string()).unwrap();
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
    legacy_state_with_kinds(
        &store,
        &b,
        json!({ aid.clone(): "desktop", bid.clone(): "desktop" }),
        own_intros,
    );
    let issued = |fake: &Fake| -> Vec<u64> {
        intros_of(fake, &aid)
            .iter()
            .map(|i| parse_intro(i).unwrap())
            .filter(|i| i.by == bid)
            .map(|i| i.parsed.issued_at_ms)
            .collect()
    };
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
    let (first, second) = if runner_first {
        ((my_id(&b), by_b), (my_id(&a), by_a))
    } else {
        ((my_id(&a), by_a), (my_id(&b), by_b))
    };
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
    let (_c, cid, c_fp) = registered(&fake, &clock, "Desktop C", "desktop");
    let (_x, xid, x_fp) = registered(&fake, &clock, "Desktop X", "desktop");
    let (aid, c_dev) = (my_id(&a), _c);
    put_raw_intro(&fake, &aid, &cid, &signed_intro(&a, &cid, &c_fp, "desktop"));
    put_raw_intro(
        &fake,
        &cid,
        &xid,
        &signed_intro(&c_dev, &xid, &x_fp, "desktop"),
    );
    b.poll_once().unwrap();
    assert!(is_trusted_with(&b, &cid, "desktop"));
    assert!(is_trusted_with(&b, &xid, "desktop"));
    {
        let mut g = b.lock();
        let prefix = kind_key(&aid, "");
        g.st.kinds.retain(|k, _| !k.starts_with(&prefix));
        g.dir = None;
        g.st.intros.retain(|i| {
            parse_intro(i).is_ok_and(|p| p.parsed.target_id == cid || p.parsed.target_id == xid)
        });
        apply_revoke(&mut g, USER, T0, &cid, &c_fp, "C");
    }
    assert!(alert_kinds(&b).contains(&"root_kind_unknown".to_owned()));
    assert!(b
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .all(|d| d.id != xid || d.state != DeviceState::Trusted));
    assert!(kind_entries_of(&b, &xid).is_empty());
    assert!(b
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .any(|d| d.id == aid && d.state == DeviceState::Trusted));
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
    s.devices
        .iter_mut()
        .find(|d| d.id == row)
        .unwrap()
        .intros
        .extend(intros);
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
fn m03_poisoned_legacy_state_migrates() {
    let (fake, clock, store, a, x, y) = legacy_owner_fleet();
    let (aid, xid, yid, y_fp) = (my_id(&a), my_id(&x), my_id(&y), fp_of(&y));
    let own_v1 = signed_intro_v(1, &a, &xid, &fp_of(&x), "desktop");
    legacy_state_with_kinds(
        &store,
        &a,
        json!({ aid.clone(): "desktop", xid.clone(): "mobile", yid.clone(): "runner" }),
        vec![own_v1.clone()],
    );
    clear_intros(&fake, &a);
    put_raw_intro(&fake, &aid, &xid, &own_v1);
    put_raw_intro(
        &fake,
        &aid,
        &yid,
        &signed_intro_v(1, &a, &yid, &y_fp, "runner"),
    );
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
    post_chat(&y, &aid, &msg_n(2, AuthorKind::Owner));
    assert!(a2.poll_once().unwrap().is_empty());
    let can_confirm = |e: &Engine| {
        e.trust_state()
            .unwrap()
            .devices
            .iter()
            .find(|d| d.id == yid)
            .map(|d| (d.state, d.kind_verified, d.can_confirm_kind))
    };
    assert_eq!(can_confirm(&a2), Some((DeviceState::Trusted, false, true)));
    assert!(a2.confirm_device_kind(&xid, "desktop").is_err());
    assert!(a2.confirm_device_kind(&yid, "runner").is_err());
    assert!(a2
        .confirm_device_kind("00000000-0000-4000-8000-0000000000aa", "desktop")
        .is_err());
    a2.confirm_device_kind(&yid, "desktop").unwrap();
    assert_eq!(kind_pin(&a2, &yid).as_deref(), Some("desktop"));
    assert_eq!(can_confirm(&a2), Some((DeviceState::Trusted, true, false)));
    assert!(!alert_kinds(&a2).contains(&"kind_unknown".to_owned()));
    assert!(fake.puts_by(&aid, &yid).contains(&2));
    post_chat(&y, &aid, &msg_n(3, AuthorKind::Owner));
    assert_eq!(a2.poll_once().unwrap().len(), 1);
    assert!(a2.confirm_device_kind(&yid, "mobile").is_err());
}

#[test]
fn a_kind_unknown_device_that_was_not_trusted_at_migration_cannot_be_confirmed() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let (_x, xid, x_fp) = registered(&fake, &clock, "Desktop X", "desktop");
    put_raw_intro(
        &fake,
        &my_id(&a),
        &xid,
        &signed_intro(&a, &xid, &x_fp, "desktop"),
    );
    put_raw_intro(
        &fake,
        &my_id(&b),
        &xid,
        &signed_intro(&b, &xid, &x_fp, "mobile"),
    );
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
    put_raw_intro(
        &fake,
        &aid,
        &rid,
        &signed_intro_v(1, &a, &rid, &r_fp, "desktop"),
    );
    put_raw_intro(
        &fake,
        &bid,
        &rid,
        &signed_intro_v(1, &b, &rid, &r_fp, "desktop"),
    );
    let (_x, xid, x_fp) = registered(&fake, &clock, "Desktop X", "desktop");
    put_foreign_intro(&fake, &r, &xid, &x_fp, "desktop");
    for e in [&a, &b] {
        e.poll_once().unwrap();
        assert_eq!(kind_pin(e, &rid).as_deref(), Some("runner"));
        assert!(e.trust_state().unwrap().devices.iter().all(|d| d.id != xid));
    }
    post_chat(&r, &aid, &msg_n(2, AuthorKind::Owner));
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
    assert!(a
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .all(|d| d.id != bid || d.state == DeviceState::Revoked));
    assert!(kind_entries_of(&a, &bid).is_empty());
}

#[test]
fn m07_two_counted_intros_for_one_target_give_runner_and_leave_the_state_alone() {
    for runner_first in [true, false] {
        let (fake, clock) = (Fake::default(), Clock::new(T0));
        let (a, b) = pair(&fake, &clock);
        let (_x, xid, x_fp) = registered(&fake, &clock, "Desktop X", "desktop");
        let by_a = (my_id(&a), signed_intro(&a, &xid, &x_fp, "desktop"));
        let by_b = (my_id(&b), signed_intro(&b, &xid, &x_fp, "runner"));
        let (first, second) = if runner_first {
            (by_b, by_a)
        } else {
            (by_a, by_b)
        };
        put_raw_intro(&fake, &first.0, &xid, &first.1);
        put_raw_intro(&fake, &second.0, &xid, &second.1);
        a.poll_once().unwrap();
        assert_eq!(kind_pin(&a, &xid).as_deref(), Some("runner"));
        let conflicts = |e: &Engine| {
            alert_kinds(e)
                .iter()
                .filter(|k| *k == "kind_conflict")
                .count()
        };
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
    legacy_state_with_kinds(
        &store,
        &a,
        json!({ aid.clone(): "desktop", mid.clone(): "mobile" }),
        vec![legacy.clone()],
    );
    clear_intros(&fake, &a);
    put_raw_intro(&fake, &aid, &mid, &legacy);
    clock.set(T0 + 1000);
    let a2 = engine_on(&fake, &clock, store, "mx_local");
    assert_eq!(a2.lock().st.kind_of(&mid, &fp_of(&m)), Some("mobile"));
    a2.poll_once().unwrap();
    post_chat(&m, &aid, &msg_n(2, AuthorKind::Owner));
    assert_eq!(a2.poll_once().unwrap().len(), 1);
    let v2: Vec<_> = intros_of(&fake, &mid)
        .iter()
        .map(|i| parse_intro(i).unwrap())
        .filter(|i| i.by == aid && i.parsed.version == 2 && !i.parsed.is_self(&aid))
        .collect();
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
    assert!(e.contains("Desktop B"), "{e}");
    assert_eq!(a.trust_state().unwrap().pending_joins.len(), 1);
    assert!(a.trust_state().unwrap().devices.iter().all(|d| d.id != rid));
    restore_intros(&fake, &bid, withheld);
    a.decide_join(&rid, true).unwrap();
    r.poll_once().unwrap();
    assert!(r.is_joined());
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
    let before: Vec<String> = a
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .map(|d| d.id.clone())
        .collect();
    for n in 1..4 {
        clock.set(T0 + n * 1000);
        take_intros_by(&fake, &bid, &bid);
        a.poll_once().unwrap();
        assert!(a.decide_join(&my_id(&r), true).is_err());
    }
    let after: Vec<String> = a
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .map(|d| d.id.clone())
        .collect();
    assert_eq!(before, after);
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
    assert!(all
        .puts
        .iter()
        .all(|p| parse_intro(&p.2).unwrap().parsed.version == 2));
}

#[test]
fn m17_a_second_runner_joins_after_the_first_is_trusted() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, _b) = pair(&fake, &clock);
    let r1 = join_as(&fake, &clock, &a, "Runner 1", "runner");
    let r2 = join_as(&fake, &clock, &a, "Runner 2", "runner");
    for r in [&r1, &r2] {
        assert!(a
            .trust_state()
            .unwrap()
            .devices
            .iter()
            .any(|d| d.id == my_id(r) && d.state == DeviceState::Trusted));
    }
}

#[test]
fn m18_a_cap_below_the_needed_passes_raises_trust_unstable_and_drops_the_moved_device() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, _b) = pair(&fake, &clock);
    let (_c, cid, c_fp) = registered(&fake, &clock, "Desktop C", "desktop");
    put_raw_intro(
        &fake,
        &my_id(&a),
        &cid,
        &signed_intro(&a, &cid, &c_fp, "desktop"),
    );
    a.lock().pass_cap = Some(1);
    a.poll_once().unwrap();
    assert!(alert_kinds(&a).contains(&"trust_unstable".to_owned()));
    assert!(a.trust_state().unwrap().devices.iter().all(|d| d.id != cid));
    a.lock().pass_cap = None;
    a.poll_once().unwrap();
    assert!(is_trusted_with(&a, &cid, "desktop"));
}

#[test]
fn a_legacy_state_without_a_runner_gate_can_still_join_runners_after_migration() {
    let (fake, clock, store, a, _x, _y) = legacy_owner_fleet();
    let aid = my_id(&a);
    let kinds: Value = {
        let g = a.lock();
        g.st.trusted
            .iter()
            .map(|d| (d.id.clone(), json!("desktop")))
            .collect::<serde_json::Map<_, _>>()
            .into()
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
    assert!(is_trusted_with(&b, &cid, "desktop"));
    {
        let mut g = b.lock();
        let prefix = kind_key(&aid, "");
        g.st.kinds.retain(|k, _| !k.starts_with(&prefix));
        g.st.intros
            .retain(|i| parse_intro(i).is_ok_and(|p| p.parsed.target_id != aid));
    }
    fake.0
        .lock()
        .unwrap()
        .devices
        .iter_mut()
        .find(|d| d.id == aid)
        .unwrap()
        .intros
        .clear();
    put_raw_intro(&fake, &aid, &did, &signed_intro(&a, &did, &d_fp, "desktop"));
    b.poll_once().unwrap();
    let trusted_ids = |e: &Engine| -> Vec<String> {
        e.trust_state()
            .unwrap()
            .devices
            .iter()
            .filter(|d| d.state == DeviceState::Trusted)
            .map(|d| d.id.clone())
            .collect()
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
    a.lock()
        .st
        .kinds
        .insert(kind_key(&nid, &n_fp), "desktop".to_owned());
    let intros_before = a.lock().st.intros.len();
    assert!(a.decide_join(&nid, true).is_err());
    assert_eq!(a.lock().st.intros.len(), intros_before);
    assert!(!is_trusted_with(&a, &nid, "runner"));
    assert!(fake
        .0
        .lock()
        .unwrap()
        .devices
        .iter()
        .find(|d| d.id == nid)
        .unwrap()
        .intros
        .is_empty());
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
    assert!(is_trusted_with(&b, &xid, "desktop"));
    put_raw_intro(&fake, &xid, &yid, &signed_intro(&x, &yid, &y_fp, "desktop"));
    put_raw_intro(&fake, &aid, &xid, &signed_intro(&a, &xid, &x_fp, "runner"));
    b.poll_once().unwrap();
    assert!(is_trusted_with(&b, &xid, "runner"));
    assert!(b
        .trust_state()
        .unwrap()
        .devices
        .iter()
        .all(|d| d.id != yid || d.state != DeviceState::Trusted));
    assert!(kind_entries_of(&b, &yid).is_empty());
}

type RpcGot = Vec<(String, String, String, String, Value)>;

#[derive(Clone, Default)]
struct RpcRec(Arc<Mutex<RpcGot>>);

impl RpcRec {
    fn n(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

impl RpcSink for RpcRec {
    fn deliver_rpc(&self, from: &str, fp: &str, kind: &str, id: &str, payload: Value) {
        self.0
            .lock()
            .unwrap()
            .push((from.into(), fp.into(), kind.into(), id.into(), payload));
    }
}

fn rpc_world() -> (Fake, Clock, Engine, Engine, Engine, RpcRec) {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let r1 = join_as(&fake, &clock, &a, "Runner 1", "runner");
    let mut r2 = join_as(&fake, &clock, &a, "Runner 2", "runner");
    for e in [&r1, &r2, &a] {
        e.poll_once().unwrap();
    }
    let sink = RpcRec::default();
    r2.rpc = Some(Arc::new(sink.clone()));
    (fake, clock, a, r1, r2, sink)
}

fn rpc_id(kind: &str) -> String {
    random_msg_id(kind).unwrap()
}

fn raw_post(from: &Engine, to_id: &str, kind: &str, id: &str, payload: &Value) {
    let mut g = from.lock();
    let target: Vec<Dev> =
        g.st.trusted
            .iter()
            .filter(|d| d.id == to_id)
            .cloned()
            .collect();
    assert_eq!(target.len(), 1);
    from.seal_and_post(&mut g, kind, id, payload, &target, false)
        .unwrap();
}

#[test]
fn rpc_from_a_trusted_runner_is_delivered_to_a_runner_engine() {
    let (_f, _c, _a, r1, r2, sink) = rpc_world();
    let id = rpc_id("mgr_req");
    r1.send_rpc(&my_id(&r2), "mgr_req", &id, &json!({ "op": "x" }))
        .unwrap();
    r2.poll_once().unwrap();
    let got = sink.0.lock().unwrap().clone();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, my_id(&r1));
    assert_eq!(got[0].1, fp_of(&r1));
    assert_eq!(got[0].2, "mgr_req");
    assert_eq!(got[0].3, id);
    assert_eq!(got[0].4, json!({ "op": "x" }));
    let rid = rpc_id("mgr_res");
    r1.send_rpc(&my_id(&r2), "mgr_res", &rid, &json!({ "ok": true }))
        .unwrap();
    r2.poll_once().unwrap();
    assert_eq!(sink.n(), 2);
}

#[test]
fn rpc_from_a_trusted_desktop_is_dropped() {
    let (_f, _c, a, _r1, r2, sink) = rpc_world();
    raw_post(&a, &my_id(&r2), "mgr_req", &rpc_id("mgr_req"), &json!({}));
    r2.poll_once().unwrap();
    assert_eq!(sink.n(), 0);
}

#[test]
fn rpc_from_an_untrusted_device_is_dropped() {
    let (fake, clock, _a, r1, r2, sink) = rpc_world();
    let stranger = engine_with(&fake, &clock, "Stranger").with_kind("runner");
    stranger.ensure_registered(&mut stranger.lock()).unwrap();
    let r2id = my_id(&r2);
    {
        let mut g = stranger.lock();
        let dev = r1
            .lock()
            .st
            .trusted
            .iter()
            .find(|d| d.id == r2id)
            .cloned()
            .unwrap();
        stranger
            .seal_and_post(
                &mut g,
                "mgr_req",
                &rpc_id("mgr_req"),
                &json!({}),
                &[dev],
                false,
            )
            .unwrap();
    }
    r2.poll_once().unwrap();
    assert_eq!(sink.n(), 0);
}

#[test]
fn rpc_is_dropped_on_a_non_runner_engine_and_without_a_sink() {
    let (_f, _c, a, r1, r2, sink) = rpc_world();
    let mut a = a;
    let a_sink = RpcRec::default();
    a.rpc = Some(Arc::new(a_sink.clone()));
    raw_post(&r1, &my_id(&a), "mgr_req", &rpc_id("mgr_req"), &json!({}));
    a.poll_once().unwrap();
    assert_eq!(a_sink.n(), 0);
    let mut r2 = r2;
    r2.rpc = None;
    r1.send_rpc(&my_id(&r2), "mgr_req", &rpc_id("mgr_req"), &json!({}))
        .unwrap();
    r2.poll_once().unwrap();
    assert_eq!(sink.n(), 0);
}

#[test]
fn oversize_rpc_is_dropped() {
    let (_f, _c, _a, r1, r2, sink) = rpc_world();
    let big = json!({ "x": "a".repeat(limits::MAX_REQ_BYTES) });
    assert!(r1
        .send_rpc(&my_id(&r2), "mgr_req", &rpc_id("mgr_req"), &big)
        .is_err());
    raw_post(&r1, &my_id(&r2), "mgr_req", &rpc_id("mgr_req"), &big);
    r2.poll_once().unwrap();
    assert_eq!(sink.n(), 0);
    let res_ok = json!({ "x": "a".repeat(limits::MAX_REQ_BYTES + 10) });
    r1.send_rpc(&my_id(&r2), "mgr_res", &rpc_id("mgr_res"), &res_ok)
        .unwrap();
    r2.poll_once().unwrap();
    assert_eq!(sink.n(), 1);
}

#[test]
fn duplicate_rpc_id_is_delivered_once() {
    let (_f, _c, _a, r1, r2, sink) = rpc_world();
    let id = rpc_id("mgr_req");
    for _ in 0..2 {
        raw_post(&r1, &my_id(&r2), "mgr_req", &id, &json!({}));
    }
    r2.poll_once().unwrap();
    assert_eq!(sink.n(), 1);
}

#[test]
fn rpc_ids_do_not_enter_the_chat_seen_ring() {
    let (_f, _c, _a, r1, r2, sink) = rpc_world();
    let before = r2.lock().st.seen.len();
    r1.send_rpc(&my_id(&r2), "mgr_req", &rpc_id("mgr_req"), &json!({}))
        .unwrap();
    r2.poll_once().unwrap();
    assert_eq!(sink.n(), 1);
    assert_eq!(r2.lock().st.seen.len(), before);
    assert_eq!(r2.lock().st.rpc_seen.len(), 1);
}

#[test]
fn send_rpc_refuses_desktops_self_and_bad_input() {
    let (_f, _c, a, r1, r2, _s) = rpc_world();
    let id = rpc_id("mgr_req");
    assert!(r1.send_rpc(&my_id(&a), "mgr_req", &id, &json!({})).is_err());
    assert!(r1
        .send_rpc(&my_id(&r1), "mgr_req", &id, &json!({}))
        .is_err());
    assert!(r1.send_rpc(&my_id(&r2), "chat", &id, &json!({})).is_err());
    assert!(r1
        .send_rpc(&my_id(&r2), "mgr_res", &id, &json!({}))
        .is_err());
    assert!(a.send_rpc(&my_id(&r1), "mgr_req", &id, &json!({})).is_err());
    assert!(r1.send_rpc("nope", "mgr_req", &id, &json!({})).is_err());
}

#[test]
fn send_rpc_posts_one_message_to_exactly_that_device() {
    let (fake, _c, a, r1, r2, _s) = rpc_world();
    let (qa, q1, q2) = (
        fake.queue_len(&my_id(&a)),
        fake.queue_len(&my_id(&r1)),
        fake.queue_len(&my_id(&r2)),
    );
    let (posts, pushes) = (fake.posts(), fake.0.lock().unwrap().pushes.len());
    r1.send_rpc(&my_id(&r2), "mgr_req", &rpc_id("mgr_req"), &json!({}))
        .unwrap();
    assert_eq!(fake.posts(), posts + 1);
    assert_eq!(fake.queue_len(&my_id(&r2)), q2 + 1);
    assert_eq!(fake.queue_len(&my_id(&a)), qa);
    assert_eq!(fake.queue_len(&my_id(&r1)), q1);
    let s = fake.0.lock().unwrap();
    assert_eq!(s.pushes.len(), pushes + 1);
    assert!(!s.pushes.last().unwrap());
}

#[test]
fn chat_after_an_rpc_from_the_same_sender_is_still_processed() {
    let (_f, _c, _a, r1, r2, sink) = rpc_world();
    r1.send_rpc(&my_id(&r2), "mgr_req", &rpc_id("mgr_req"), &json!({}))
        .unwrap();
    post_chat(&r1, &my_id(&r2), &msg_n(5, AuthorKind::Manager));
    let got = r2.poll_once().unwrap();
    assert_eq!(sink.n(), 1);
    assert_eq!(got.len(), 1);
}

#[test]
fn own_device_is_none_before_setup_and_the_public_id_and_fp_after() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let e = engine_with(&fake, &clock, "Runner").with_kind("runner");
    assert_eq!(e.own_device(), None);
    e.register().unwrap();
    assert_eq!(e.own_device(), Some((my_id(&e), fp_of(&e))));
}

#[derive(Clone, Default)]
struct KeyRec(Arc<Mutex<Vec<(String, String, SpawnKey)>>>);

impl KeyRec {
    fn n(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

impl SpawnKeySink for KeyRec {
    fn deliver_spawnkey(&self, from: &str, fp: &str, _: &str, _: &str, key: &SpawnKey) {
        self.0
            .lock()
            .unwrap()
            .push((from.into(), fp.into(), key.clone()));
    }
}

fn spawnkey_world() -> (Fake, Clock, Engine, Engine, KeyRec) {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let mut r = join_as(&fake, &clock, &a, "Runner", "runner");
    r.poll_once().unwrap();
    let sink = KeyRec::default();
    r.spawnkey = Some(Arc::new(sink.clone()));
    (fake, clock, a, r, sink)
}

fn spawnkey_payload(signer: &Engine, user: u64, device_id: &str, issued: u64) -> Value {
    let g = signer.lock();
    let dev = g.keys.as_ref().unwrap();
    let spawn_secret = [77u8; 32];
    let st = SpawnStatement {
        user_id: user,
        device_fp: dev.fingerprint(),
        device_id: device_id.into(),
        spawn_pub_b64: b64(&tmgr_phone::public_key(&spawn_secret).unwrap()),
        issued_at_ms: issued,
    };
    serde_json::to_value(sign_spawnkey(dev, &spawn_secret, &st).unwrap()).unwrap()
}

fn post_spawnkey(from: &Engine, to: &Engine, payload: &Value) {
    raw_post(from, &my_id(to), "spawnkey", &rpc_id("spawnkey"), payload);
    to.poll_once().unwrap();
}

#[test]
fn spawnkey_from_a_trusted_joined_sender_reaches_the_sink() {
    let (_f, _c, a, r, sink) = spawnkey_world();
    let payload = spawnkey_payload(&a, USER, &my_id(&a), 5);
    post_spawnkey(&a, &r, &payload);
    let got = sink.0.lock().unwrap().clone();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, my_id(&a));
    assert_eq!(got[0].1, fp_of(&a));
    assert_eq!(serde_json::to_value(&got[0].2).unwrap(), payload);
}

#[test]
fn spawnkey_with_a_bad_statement_is_dropped() {
    let (_f, _c, a, r, sink) = spawnkey_world();
    let aid = my_id(&a);
    let good = spawnkey_payload(&a, USER, &aid, 5);
    let swap = |from: &str, to: &str| {
        let mut p = good.clone();
        p[to] = good[from].clone();
        p
    };
    post_spawnkey(&a, &r, &swap("spawn_sig", "sig"));
    post_spawnkey(&a, &r, &swap("sig", "spawn_sig"));
    post_spawnkey(&a, &r, &spawnkey_payload(&a, USER + 1, &aid, 5));
    post_spawnkey(&a, &r, &spawnkey_payload(&a, USER, &my_id(&r), 5));
    for junk in [json!({}), json!("x"), json!(null), json!({"statement": 1})] {
        post_spawnkey(&a, &r, &junk);
    }
    assert_eq!(sink.n(), 0);
}

#[test]
fn spawnkey_from_an_untrusted_device_is_dropped() {
    let (fake, clock, _a, r, sink) = spawnkey_world();
    let stranger = engine_with(&fake, &clock, "Stranger");
    stranger.ensure_registered(&mut stranger.lock()).unwrap();
    let rid = my_id(&r);
    let dev = {
        let g = r.lock();
        g.st.trusted.iter().find(|d| d.id == rid).cloned().unwrap()
    };
    let payload = spawnkey_payload(&stranger, USER, &my_id(&stranger), 5);
    stranger
        .seal_and_post(
            &mut stranger.lock(),
            "spawnkey",
            &rpc_id("spawnkey"),
            &payload,
            &[dev],
            false,
        )
        .unwrap();
    r.poll_once().unwrap();
    assert_eq!(sink.n(), 0);
}

#[test]
fn spawnkey_from_a_revoked_sender_is_dropped() {
    let (_f, _c, a, r, sink) = spawnkey_world();
    let aid = my_id(&a);
    let payload = spawnkey_payload(&a, USER, &aid, 5);
    let fp = fp_of(&a);
    {
        let mut g = r.lock();
        g.st.revoked.push(fp.clone());
        g.st.trusted.retain(|d| d.fp != fp);
    }
    let target: Vec<Dev> = {
        let g = a.lock();
        let dev = g.st.trusted.iter().find(|d| d.id == my_id(&r)).cloned();
        dev.into_iter().collect()
    };
    a.seal_and_post(
        &mut a.lock(),
        "spawnkey",
        &rpc_id("spawnkey"),
        &payload,
        &target,
        false,
    )
    .unwrap();
    r.poll_once().unwrap();
    assert_eq!(sink.n(), 0);
}

#[test]
fn spawnkey_before_joining_is_dropped() {
    let (_f, _c, a, r, sink) = spawnkey_world();
    r.lock().st.joined = false;
    post_spawnkey(&a, &r, &spawnkey_payload(&a, USER, &my_id(&a), 5));
    assert_eq!(sink.n(), 0);
}

#[test]
fn spawnkey_from_the_own_device_is_dropped() {
    let (_f, _c, _a, r, sink) = spawnkey_world();
    let rid = my_id(&r);
    let payload = spawnkey_payload(&r, USER, &rid, 5);
    let me: Vec<Dev> = r
        .lock()
        .st
        .trusted
        .iter()
        .filter(|d| d.id == rid)
        .cloned()
        .collect();
    r.seal_and_post(
        &mut r.lock(),
        "spawnkey",
        &rpc_id("spawnkey"),
        &payload,
        &me,
        false,
    )
    .unwrap();
    r.poll_once().unwrap();
    assert_eq!(sink.n(), 0);
}

#[derive(Clone, Default)]
struct OkRec(Arc<Mutex<Vec<(String, String, tmgr_spawn_auth::SignedSpawn)>>>);

impl OkRec {
    fn n(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

impl SpawnOkSink for OkRec {
    fn deliver_spawnok(&self, from: &str, fp: &str, signed: &tmgr_spawn_auth::SignedSpawn) {
        self.0
            .lock()
            .unwrap()
            .push((from.into(), fp.into(), signed.clone()));
    }
}

fn spawnok_world() -> (Fake, Clock, Engine, Engine, OkRec) {
    spawnok_world_from("mobile")
}

fn spawnok_world_from(kind: &str) -> (Fake, Clock, Engine, Engine, OkRec) {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Phone").with_kind(kind);
    let mut r = join_as(&fake, &clock, &a, "Runner", "runner");
    r.poll_once().unwrap();
    let sink = OkRec::default();
    r.spawnok = Some(Arc::new(sink.clone()));
    (fake, clock, a, r, sink)
}

fn spawnok_payload(device_fp: &str) -> Value {
    json!({
        "v": 1,
        "user_id": "42",
        "device_fp": device_fp,
        "target_id": "d_runner1",
        "target_fp": "cd".repeat(32),
        "card_id": "card_00112233445566778899aabbccddeeff",
        "nonce": "ab".repeat(16),
        "ts_ms": 1,
        "exp_ms": 2,
        "req_sha256": "ef".repeat(32),
        "alg": "es256",
        "sig": "c2ln"
    })
}

fn post_spawnok(from: &Engine, to: &Engine, payload: &Value) {
    raw_post(from, &my_id(to), "spawnok", &rpc_id("spawnok"), payload);
    to.poll_once().unwrap();
}

#[test]
fn spawnok_from_a_trusted_joined_phone_reaches_the_sink() {
    let (_f, _c, a, r, sink) = spawnok_world();
    let payload = spawnok_payload(&fp_of(&a));
    post_spawnok(&a, &r, &payload);
    let got = sink.0.lock().unwrap().clone();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, my_id(&a));
    assert_eq!(got[0].1, fp_of(&a));
    assert_eq!(serde_json::to_value(&got[0].2).unwrap(), payload);
}

#[test]
fn spawnok_from_a_device_pinned_as_desktop_is_dropped() {
    let (_f, _c, a, r, sink) = spawnok_world_from("desktop");
    assert_eq!(kind_pin(&r, &my_id(&a)).as_deref(), Some("desktop"));
    post_spawnok(&a, &r, &spawnok_payload(&fp_of(&a)));
    assert_eq!(sink.n(), 0);
}

#[test]
fn spawnok_whose_device_fp_is_not_the_sender_is_dropped() {
    let (_f, _c, a, r, sink) = spawnok_world();
    post_spawnok(&a, &r, &spawnok_payload(&"11".repeat(32)));
    assert_eq!(sink.n(), 0);
}

#[test]
fn a_malformed_spawnok_is_dropped() {
    let (_f, _c, a, r, sink) = spawnok_world();
    let fp = fp_of(&a);
    let mut no_sig = spawnok_payload(&fp);
    no_sig.as_object_mut().unwrap().remove("sig");
    let mut bad_ts = spawnok_payload(&fp);
    bad_ts["ts_ms"] = json!(-1);
    let mut huge = spawnok_payload(&fp);
    huge["exp_ms"] = json!(u64::MAX);
    for junk in [no_sig, bad_ts, huge, json!("x"), json!([1]), json!({})] {
        post_spawnok(&a, &r, &junk);
    }
    assert_eq!(sink.n(), 0);
}

#[test]
fn spawnok_from_an_untrusted_device_is_dropped() {
    let (fake, clock, _a, r, sink) = spawnok_world();
    let stranger = engine_with(&fake, &clock, "Stranger");
    stranger.ensure_registered(&mut stranger.lock()).unwrap();
    let rid = my_id(&r);
    let dev = {
        let g = r.lock();
        g.st.trusted.iter().find(|d| d.id == rid).cloned().unwrap()
    };
    let payload = spawnok_payload(&fp_of(&stranger));
    stranger
        .seal_and_post(
            &mut stranger.lock(),
            "spawnok",
            &rpc_id("spawnok"),
            &payload,
            &[dev],
            false,
        )
        .unwrap();
    r.poll_once().unwrap();
    assert_eq!(sink.n(), 0);
}

#[test]
fn spawnok_from_a_revoked_sender_is_dropped() {
    let (_f, _c, a, r, sink) = spawnok_world();
    let payload = spawnok_payload(&fp_of(&a));
    let fp = fp_of(&a);
    {
        let mut g = r.lock();
        g.st.revoked.push(fp.clone());
        g.st.trusted.retain(|d| d.fp != fp);
    }
    let target: Vec<Dev> = {
        let g = a.lock();
        g.st.trusted
            .iter()
            .filter(|d| d.id == my_id(&r))
            .cloned()
            .collect()
    };
    a.seal_and_post(
        &mut a.lock(),
        "spawnok",
        &rpc_id("spawnok"),
        &payload,
        &target,
        false,
    )
    .unwrap();
    r.poll_once().unwrap();
    assert_eq!(sink.n(), 0);
}

#[test]
fn spawnok_from_the_own_device_is_dropped() {
    let (_f, _c, _a, r, sink) = spawnok_world();
    let rid = my_id(&r);
    let payload = spawnok_payload(&fp_of(&r));
    let me: Vec<Dev> = r
        .lock()
        .st
        .trusted
        .iter()
        .filter(|d| d.id == rid)
        .cloned()
        .collect();
    r.seal_and_post(
        &mut r.lock(),
        "spawnok",
        &rpc_id("spawnok"),
        &payload,
        &me,
        false,
    )
    .unwrap();
    r.poll_once().unwrap();
    assert_eq!(sink.n(), 0);
}

#[test]
fn spawnok_before_joining_is_dropped() {
    let (_f, _c, a, r, sink) = spawnok_world();
    r.lock().st.joined = false;
    post_spawnok(&a, &r, &spawnok_payload(&fp_of(&a)));
    assert_eq!(sink.n(), 0);
}

type PermitRows = Vec<(
    String,
    String,
    tmgr_spawn_auth::PermitSenderKind,
    String,
    String,
    tmgr_spawn_auth::SignedPermit,
)>;

#[derive(Clone, Default)]
struct PermitRec(Arc<Mutex<PermitRows>>, Arc<Mutex<Vec<RefreshRow>>>);

type RefreshRow = (
    String,
    String,
    tmgr_spawn_auth::PermitSenderKind,
    String,
    String,
);

impl PermitRec {
    fn refreshes(&self) -> Vec<RefreshRow> {
        self.1.lock().unwrap().clone()
    }

    fn n(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

impl PermitOkSink for PermitRec {
    fn deliver_permitok(
        &self,
        from: &str,
        fp: &str,
        kind: tmgr_spawn_auth::PermitSenderKind,
        sign_pub: &str,
        label: &str,
        signed: &tmgr_spawn_auth::SignedPermit,
    ) {
        self.0.lock().unwrap().push((
            from.into(),
            fp.into(),
            kind,
            sign_pub.into(),
            label.into(),
            signed.clone(),
        ));
    }

    fn deliver_permit_refresh(
        &self,
        from: &str,
        fp: &str,
        kind: tmgr_spawn_auth::PermitSenderKind,
        card_id: &str,
        nonce: &str,
    ) {
        self.1
            .lock()
            .unwrap()
            .push((from.into(), fp.into(), kind, card_id.into(), nonce.into()));
    }
}

fn permitok_world_from(kind: &str) -> (Fake, Clock, Engine, Engine, PermitRec) {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Sender").with_kind(kind);
    let mut r = join_as(&fake, &clock, &a, "Runner", "runner");
    r.poll_once().unwrap();
    let sink = PermitRec::default();
    r.permitok = Some(Arc::new(sink.clone()));
    (fake, clock, a, r, sink)
}

fn permitok_payload(device_fp: &str) -> Value {
    json!({
        "v": 1,
        "user_id": "42",
        "device_fp": device_fp,
        "runner_id": "d_runner1",
        "runner_fp": "cd".repeat(32),
        "card_id": "card_00112233445566778899aabbccddeeff",
        "session": "s_aaaaaaaaaa",
        "choice": "yes",
        "confirmed": 0,
        "nonce": "ab".repeat(16),
        "ts_ms": 1,
        "exp_ms": 2,
        "req_sha256": "ef".repeat(32),
        "alg": "es256",
        "sig": "c2ln"
    })
}

fn refresh_payload() -> Value {
    json!({"v": 1, "refresh": "card_00112233445566778899aabbccddeeff", "nonce": "ab".repeat(16)})
}

fn post_permitok(from: &Engine, to: &Engine, payload: &Value) {
    raw_post(from, &my_id(to), "permitok", &rpc_id("permitok"), payload);
    to.poll_once().unwrap();
}

fn sign_pub_of(e: &Engine, id: &str) -> String {
    e.lock()
        .st
        .trusted
        .iter()
        .find(|d| d.id == id)
        .unwrap()
        .sign_pub
        .clone()
}

#[test]
fn permitok_from_a_trusted_joined_desktop_reaches_the_sink_as_desktop() {
    let (_f, _c, a, r, sink) = permitok_world_from("desktop");
    let payload = permitok_payload(&fp_of(&a));
    post_permitok(&a, &r, &payload);
    let got = sink.0.lock().unwrap().clone();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, my_id(&a));
    assert_eq!(got[0].1, fp_of(&a));
    assert_eq!(got[0].2, tmgr_spawn_auth::PermitSenderKind::Desktop);
    assert_eq!(got[0].3, sign_pub_of(&r, &my_id(&a)));
    assert_eq!(got[0].4, "Sender");
    assert_eq!(serde_json::to_value(&got[0].5).unwrap(), payload);
}

#[test]
fn permitok_from_a_trusted_joined_phone_reaches_the_sink_as_mobile() {
    let (_f, _c, a, r, sink) = permitok_world_from("mobile");
    post_permitok(&a, &r, &permitok_payload(&fp_of(&a)));
    let got = sink.0.lock().unwrap().clone();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].2, tmgr_spawn_auth::PermitSenderKind::Mobile);
    assert_eq!(got[0].3, sign_pub_of(&r, &my_id(&a)));
}

fn assert_permitok_dropped_for_pin(kind: &str) {
    let (_f, _c, a, r, sink) = permitok_world_from("desktop");
    set_pin(&r, &my_id(&a), kind);
    assert_eq!(kind_pin(&r, &my_id(&a)).as_deref(), Some(kind));
    post_permitok(&a, &r, &permitok_payload(&fp_of(&a)));
    assert_eq!(sink.n(), 0, "{kind}");
}

#[test]
fn permitok_from_a_device_pinned_as_terminal_is_dropped() {
    assert_permitok_dropped_for_pin("terminal");
}

#[test]
fn permitok_from_a_device_pinned_as_runner_is_dropped() {
    assert_permitok_dropped_for_pin("runner");
}

#[test]
fn permitok_from_a_device_pinned_as_web_is_dropped() {
    assert_permitok_dropped_for_pin("web");
}

#[test]
fn permitok_from_a_device_pinned_as_an_unknown_kind_is_dropped() {
    assert_permitok_dropped_for_pin("mobile2");
    assert_permitok_dropped_for_pin("");
}

#[test]
fn permitok_from_an_unpinned_device_is_dropped() {
    let (_f, _c, a, r, sink) = permitok_world_from("desktop");
    drop_pin(&r, &my_id(&a));
    assert_eq!(kind_pin(&r, &my_id(&a)), None);
    post_permitok(&a, &r, &permitok_payload(&fp_of(&a)));
    assert_eq!(sink.n(), 0);
}

#[test]
fn permitok_whose_device_fp_is_not_the_sender_is_dropped() {
    let (_f, _c, a, r, sink) = permitok_world_from("desktop");
    post_permitok(&a, &r, &permitok_payload(&"11".repeat(32)));
    assert_eq!(sink.n(), 0);
}

#[test]
fn a_malformed_permitok_is_dropped() {
    let (_f, _c, a, r, sink) = permitok_world_from("desktop");
    let fp = fp_of(&a);
    let mut no_sig = permitok_payload(&fp);
    no_sig.as_object_mut().unwrap().remove("sig");
    let mut bad_ts = permitok_payload(&fp);
    bad_ts["ts_ms"] = json!(-1);
    let mut huge = permitok_payload(&fp);
    huge["exp_ms"] = json!(u64::MAX);
    let mut extra = permitok_payload(&fp);
    extra["owner_device"] = json!(true);
    let mut bad_choice = permitok_payload(&fp);
    bad_choice["choice"] = json!("maybe");
    for junk in [
        no_sig,
        bad_ts,
        huge,
        extra,
        bad_choice,
        json!("x"),
        json!([1]),
        json!({}),
    ] {
        post_permitok(&a, &r, &junk);
    }
    assert_eq!(sink.n(), 0);
}

#[test]
fn permitok_to_an_engine_that_is_not_a_runner_is_dropped() {
    for kind in ["desktop", "mobile", "terminal"] {
        let (_f, _c, a, mut r, sink) = permitok_world_from("desktop");
        r.kind = kind.to_owned();
        post_permitok(&a, &r, &permitok_payload(&fp_of(&a)));
        assert_eq!(sink.n(), 0, "{kind}");
    }
}

#[test]
fn permitok_without_a_sink_is_dropped() {
    let (_f, _c, a, mut r, sink) = permitok_world_from("desktop");
    r.permitok = None;
    post_permitok(&a, &r, &permitok_payload(&fp_of(&a)));
    assert_eq!(sink.n(), 0);
}

#[test]
fn permitok_from_an_untrusted_device_is_dropped() {
    let (fake, clock, _a, r, sink) = permitok_world_from("desktop");
    let stranger = engine_with(&fake, &clock, "Stranger").with_kind("desktop");
    stranger.ensure_registered(&mut stranger.lock()).unwrap();
    let rid = my_id(&r);
    let dev = {
        let g = r.lock();
        g.st.trusted.iter().find(|d| d.id == rid).cloned().unwrap()
    };
    let payload = permitok_payload(&fp_of(&stranger));
    stranger
        .seal_and_post(
            &mut stranger.lock(),
            "permitok",
            &rpc_id("permitok"),
            &payload,
            &[dev],
            false,
        )
        .unwrap();
    r.poll_once().unwrap();
    assert_eq!(sink.n(), 0);
}

#[test]
fn permitok_from_a_revoked_sender_is_dropped() {
    for keep_trusted in [false, true] {
        let (_f, _c, a, r, sink) = permitok_world_from("desktop");
        let fp = fp_of(&a);
        {
            let mut g = r.lock();
            g.st.revoked.push(fp.clone());
            if !keep_trusted {
                g.st.trusted.retain(|d| d.fp != fp);
            }
        }
        let target: Vec<Dev> = {
            let g = a.lock();
            g.st.trusted
                .iter()
                .filter(|d| d.id == my_id(&r))
                .cloned()
                .collect()
        };
        a.seal_and_post(
            &mut a.lock(),
            "permitok",
            &rpc_id("permitok"),
            &permitok_payload(&fp),
            &target,
            false,
        )
        .unwrap();
        r.poll_once().unwrap();
        assert_eq!(sink.n(), 0, "keep_trusted={keep_trusted}");
    }
}

#[test]
fn permitok_from_the_own_device_is_dropped() {
    let (_f, _c, _a, r, sink) = permitok_world_from("desktop");
    let rid = my_id(&r);
    set_pin(&r, &rid, "desktop");
    let payload = permitok_payload(&fp_of(&r));
    let me: Vec<Dev> = r
        .lock()
        .st
        .trusted
        .iter()
        .filter(|d| d.id == rid)
        .cloned()
        .collect();
    r.seal_and_post(
        &mut r.lock(),
        "permitok",
        &rpc_id("permitok"),
        &payload,
        &me,
        false,
    )
    .unwrap();
    r.poll_once().unwrap();
    assert_eq!(sink.n(), 0);
}

#[test]
fn permitok_before_joining_is_dropped() {
    let (_f, _c, a, r, sink) = permitok_world_from("desktop");
    r.lock().st.joined = false;
    post_permitok(&a, &r, &permitok_payload(&fp_of(&a)));
    assert_eq!(sink.n(), 0);
}

fn rooms_world() -> (Fake, Clock, Engine, Engine, Engine, Engine) {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let desktop = engine_with(&fake, &clock, "Desktop A");
    let runner = join_as(&fake, &clock, &desktop, "Runner 1", "runner");
    let mobile = join_as(&fake, &clock, &desktop, "Phone", "mobile");
    let other = join_as(&fake, &clock, &desktop, "Runner 2", "runner");
    runner.poll_once().unwrap();
    (fake, clock, runner, desktop, mobile, other)
}

#[test]
fn rooms_is_sealed_only_to_owner_devices_and_carries_no_label_visible_to_the_server() {
    let (fake, _c, runner, desktop, mobile, other) = rooms_world();
    let (did, mid, oid) = (my_id(&desktop), my_id(&mobile), my_id(&other));
    let before = (
        fake.queue_len(&did),
        fake.queue_len(&mid),
        fake.queue_len(&oid),
    );
    let id = rpc_id("rooms");
    let payload = json!({"v": 1, "machine_label": "Secret Studio Name"});
    runner
        .send_sealed_to_owners("rooms", &id, &payload, None)
        .unwrap();
    assert_eq!(
        (
            fake.queue_len(&did),
            fake.queue_len(&mid),
            fake.queue_len(&oid)
        ),
        (before.0 + 1, before.1 + 1, before.2)
    );
    assert!(!fake.visible_text().contains("Secret Studio Name"));
    assert_eq!(runner.owner_device_ids().unwrap().len(), 2);
}

#[test]
fn a_runner_seals_nothing_to_a_device_with_a_pending_revoke() {
    let (fake, _c, runner, desktop, mobile, _other) = rooms_world();
    let (did, mid) = (my_id(&desktop), my_id(&mobile));
    desktop.revoke(&mid).unwrap();
    runner.poll_once().unwrap();
    assert_eq!(runner.trust_state().unwrap().pending_revokes.len(), 1);
    let (bd, bm) = (fake.queue_len(&did), fake.queue_len(&mid));
    runner
        .send_chat(&text_msg(new_message_id(), "hello"), false)
        .unwrap();
    runner
        .send_sealed_to_owners("rooms", &rpc_id("rooms"), &json!({"v": 1}), None)
        .unwrap();
    assert_eq!(fake.queue_len(&did), bd + 2);
    assert_eq!(fake.queue_len(&mid), bm);
    assert!(runner.owner_device_ids().unwrap().iter().all(|i| *i != mid));
}

#[test]
fn owner_sends_skip_a_revoked_desktop_but_reach_a_normal_owner_desktop() {
    let (fake, clock, runner, desktop, _mobile, _other) = rooms_world();
    let second = join_as(&fake, &clock, &desktop, "Desktop B", "desktop");
    runner.poll_once().unwrap();
    let (did, sid) = (my_id(&desktop), my_id(&second));
    desktop.revoke(&sid).unwrap();
    runner.poll_once().unwrap();
    assert_eq!(runner.trust_state().unwrap().pending_revokes.len(), 1);
    let (bd, bs) = (fake.queue_len(&did), fake.queue_len(&sid));
    let only = [did.clone(), sid.clone()];
    let mid = new_message_id();
    runner
        .send_chat_to_owners(&text_msg(new_message_id(), "hi"), &only)
        .unwrap();
    runner
        .send_receipt(&did, &mid, ReceiptState::Delivered, None)
        .unwrap();
    assert!(runner
        .send_receipt(&sid, &mid, ReceiptState::Delivered, None)
        .is_err());
    assert!(runner
        .send_chat_to_owners(&text_msg(new_message_id(), "x"), std::slice::from_ref(&sid))
        .is_err());
    assert_eq!(fake.queue_len(&did), bd + 2);
    assert_eq!(fake.queue_len(&sid), bs);
    let owners = runner.owner_device_ids().unwrap();
    assert!(owners.contains(&did) && !owners.contains(&sid));
}

#[test]
fn rooms_can_be_restricted_to_a_subset_and_refuses_when_none_match() {
    let (fake, _c, runner, desktop, mobile, _other) = rooms_world();
    let (did, mid) = (my_id(&desktop), my_id(&mobile));
    let (bd, bm) = (fake.queue_len(&did), fake.queue_len(&mid));
    runner
        .send_sealed_to_owners(
            "rooms",
            &rpc_id("rooms"),
            &json!({"v": 1}),
            Some(std::slice::from_ref(&mid)),
        )
        .unwrap();
    assert_eq!((fake.queue_len(&did), fake.queue_len(&mid)), (bd, bm + 1));
    let none = vec!["00000000-0000-4000-8000-000000000000".to_owned()];
    assert!(runner
        .send_sealed_to_owners("rooms", &rpc_id("rooms"), &json!({}), Some(&none))
        .is_err());
}

#[test]
fn rooms_is_runner_only_and_validates_kind_and_id_prefix() {
    let (_fake, _c, runner, desktop, _mobile, _other) = rooms_world();
    assert!(desktop
        .send_sealed_to_owners("rooms", &rpc_id("rooms"), &json!({}), None)
        .is_err());
    assert!(runner
        .send_sealed_to_owners("mgr_req", &rpc_id("mgr_req"), &json!({}), None)
        .is_err());
    assert!(runner
        .send_sealed_to_owners("rooms", &rpc_id("blob"), &json!({}), None)
        .is_err());
    let big = json!({"x": "a".repeat(ROOMS_MAX_BYTES)});
    assert!(runner
        .send_sealed_to_owners("rooms", &rpc_id("rooms"), &big, None)
        .is_err());
}

#[test]
fn send_chat_to_owners_reaches_only_the_chosen_owner_device() {
    let (fake, _c, runner, desktop, mobile, other) = rooms_world();
    let (did, mid, oid) = (my_id(&desktop), my_id(&mobile), my_id(&other));
    let before = (
        fake.queue_len(&did),
        fake.queue_len(&mid),
        fake.queue_len(&oid),
    );
    let msg = text_msg(new_message_id(), "old message");
    runner
        .send_chat_to_owners(&msg, std::slice::from_ref(&mid))
        .unwrap();
    assert_eq!(
        (
            fake.queue_len(&did),
            fake.queue_len(&mid),
            fake.queue_len(&oid)
        ),
        (before.0, before.1 + 1, before.2)
    );
    assert!(runner
        .send_chat_to_owners(&msg, std::slice::from_ref(&oid))
        .is_err());
}

fn seq_out(e: &Engine, id: &str) -> u64 {
    e.lock().st.seq_out.get(id).copied().unwrap_or(0)
}

#[test]
fn a_full_owner_device_is_skipped_and_the_rest_still_get_the_message() {
    let (fake, _c, runner, desktop, mobile, _other) = rooms_world();
    let (did, mid) = (my_id(&desktop), my_id(&mobile));
    let (bd, bm) = (fake.queue_len(&did), fake.queue_len(&mid));
    let (sd, sm) = (seq_out(&runner, &did), seq_out(&runner, &mid));
    fake.set_full(Some(&mid));
    runner
        .send_sealed_to_owners("rooms", &rpc_id("rooms"), &json!({"v": 1}), None)
        .unwrap();
    assert_eq!((fake.queue_len(&did), fake.queue_len(&mid)), (bd + 1, bm));
    assert_eq!(seq_out(&runner, &did), sd + 1);
    assert_eq!(seq_out(&runner, &mid), sm);
    assert_eq!(
        runner.s.skipped.lock().unwrap().clone(),
        [(mid.clone(), "mobile".to_owned())]
    );
    let both = [did.clone(), mid.clone()];
    runner
        .send_chat_to_owners(&text_msg(new_message_id(), "x"), &both)
        .unwrap();
    assert_eq!(seq_out(&runner, &did), sd + 2);
    assert_eq!(seq_out(&runner, &mid), sm);
    fake.set_full(None);
    runner
        .send_sealed_to_owners("rooms", &rpc_id("rooms"), &json!({"v": 1}), None)
        .unwrap();
    assert_eq!(seq_out(&runner, &mid), sm + 1);
}

#[test]
fn a_removed_owner_device_does_not_block_the_chat_to_the_others() {
    let (fake, _c, runner, desktop, mobile, _other) = rooms_world();
    let (did, mid) = (my_id(&desktop), my_id(&mobile));
    fake.0.lock().unwrap().devices.retain(|d| d.id != mid);
    runner.poll_once().unwrap();
    let bd = fake.queue_len(&did);
    runner
        .send_chat(&text_msg(new_message_id(), "reply"), false)
        .unwrap();
    assert_eq!(fake.queue_len(&did), bd + 1);
}

#[test]
fn a_receipt_to_a_removed_device_fails_with_a_permanent_error() {
    let (fake, _c, runner, desktop, _mobile, _other) = rooms_world();
    let did = my_id(&desktop);
    fake.0.lock().unwrap().devices.retain(|d| d.id != did);
    let err = runner
        .send_receipt(&did, &new_message_id(), ReceiptState::Delivered, None)
        .unwrap_err();
    assert_eq!(err, RECIPIENT_UNKNOWN_MSG);
}

#[test]
fn a_chat_to_owners_skips_a_full_desktop_too() {
    let (fake, _c, runner, desktop, mobile, _other) = rooms_world();
    let (did, mid) = (my_id(&desktop), my_id(&mobile));
    let bm = fake.queue_len(&mid);
    fake.set_full(Some(&did));
    let both = [did.clone(), mid.clone()];
    runner
        .send_chat_to_owners(&text_msg(new_message_id(), "x"), &both)
        .unwrap();
    assert_eq!(
        runner.s.skipped.lock().unwrap().clone(),
        [(did, "desktop".to_owned())]
    );
    assert_eq!(fake.queue_len(&mid), bm + 1);
}

#[test]
fn when_every_owner_device_is_full_the_send_fails() {
    let (fake, _c, runner, desktop, mobile, _other) = rooms_world();
    let (did, mid) = (my_id(&desktop), my_id(&mobile));
    fake.0.lock().unwrap().post_error = Some(r#"{"error":"recipient_full"}"#.into());
    let both = [did.clone(), mid.clone()];
    assert_eq!(
        runner
            .send_chat_to_owners(&text_msg(new_message_id(), "x"), &both)
            .unwrap_err(),
        RECIPIENT_FULL_MSG
    );
    fake.0.lock().unwrap().post_error = None;
    fake.set_full(Some(&did));
    let only = [did];
    assert_eq!(
        runner
            .send_chat_to_owners(&text_msg(new_message_id(), "x"), &only)
            .unwrap_err(),
        RECIPIENT_FULL_MSG
    );
}

#[test]
fn a_full_runner_among_the_targets_still_fails_the_send() {
    let (fake, _c, _runner, desktop, mobile, other) = rooms_world();
    let oid = my_id(&other);
    fake.set_full(Some(&oid));
    let _ = mobile;
    let m = text_msg(new_message_id(), "x");
    let err = desktop.send_chat(&m, false).unwrap_err();
    assert_eq!(err, RECIPIENT_FULL_MSG);
}

#[test]
fn active_owner_devices_exclude_the_long_unseen_and_keep_unknown_ones() {
    let (fake, clock, runner, desktop, mobile, _other) = rooms_world();
    let (did, mid) = (my_id(&desktop), my_id(&mobile));
    let idle = 3 * 60 * 1000;
    fake.set_last_seen(&mid, T0 - 10 * idle);
    fake.set_last_seen(&did, T0 - 1000);
    runner.poll_once().unwrap();
    assert_eq!(
        runner.active_owner_device_ids(idle).unwrap(),
        std::slice::from_ref(&did)
    );
    fake.set_last_seen(&mid, T0);
    runner.poll_once().unwrap();
    let mut got = runner.active_owner_device_ids(idle).unwrap();
    got.sort();
    let mut want = vec![did.clone(), mid.clone()];
    want.sort();
    assert_eq!(got, want);
    fake.0.lock().unwrap().last_seen.clear();
    runner.poll_once().unwrap();
    assert_eq!(runner.active_owner_device_ids(idle).unwrap().len(), 2);
    let _ = clock;
}

#[derive(Clone, Default)]
struct ActRec(Arc<Mutex<Vec<(String, bool)>>>);

impl ActRec {
    fn got(&self) -> Vec<(String, bool)> {
        self.0.lock().unwrap().clone()
    }
}

impl ActivitySubSink for ActRec {
    fn deliver_activity_on(&self, from_device: &str, on: bool) {
        self.0.lock().unwrap().push((from_device.into(), on));
    }
}

fn activity_world_from(kind: &str) -> (Fake, Clock, Engine, Engine, ActRec) {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Phone").with_kind(kind);
    let mut r = join_as(&fake, &clock, &a, "Runner", "runner");
    r.poll_once().unwrap();
    let sink = ActRec::default();
    r.activity_sub = Some(Arc::new(sink.clone()));
    (fake, clock, a, r, sink)
}

fn post_activity_on(from: &Engine, to: &Engine, payload: &Value) {
    raw_post(
        from,
        &my_id(to),
        "activity_on",
        &rpc_id("activity_on"),
        payload,
    );
    to.poll_once().unwrap();
}

#[test]
fn activity_on_from_a_trusted_phone_reaches_the_sink() {
    let (_f, _c, a, r, sink) = activity_world_from("mobile");
    post_activity_on(&a, &r, &json!({"v": 1, "on": true}));
    post_activity_on(&a, &r, &json!({"v": 1, "on": false}));
    assert_eq!(sink.got(), [(my_id(&a), true), (my_id(&a), false)]);
}

#[test]
fn activity_on_leases_do_not_fill_the_seen_list() {
    let (_f, clock, a, r, sink) = activity_world_from("mobile");
    let before = r.lock().st.seen.len();
    for _ in 0..5 {
        post_activity_on(&a, &r, &json!({"v": 1, "on": true}));
    }
    clock.set(T0 + 6 * 60 * 1000);
    post_activity_on(&a, &r, &json!({"v": 1, "on": true}));
    assert_eq!(sink.got().len(), 6);
    assert_eq!(r.lock().st.seen.len(), before + 1);
}

#[test]
fn a_stale_activity_on_is_dropped() {
    let (_f, clock, a, r, sink) = activity_world_from("mobile");
    raw_post(
        &a,
        &my_id(&r),
        "activity_on",
        &rpc_id("activity_on"),
        &json!({"v": 1, "on": true}),
    );
    clock.set(T0 + 6 * 60 * 1000);
    r.poll_once().unwrap();
    assert!(sink.got().is_empty());
}

#[test]
fn activity_on_from_desktop_and_runner_kinds_is_dropped() {
    for kind in ["desktop", "runner"] {
        let (_f, _c, a, r, sink) = activity_world_from("mobile");
        set_pin(&r, &my_id(&a), kind);
        post_activity_on(&a, &r, &json!({"v": 1, "on": true}));
        assert!(sink.got().is_empty(), "{kind}");
    }
}

#[test]
fn activity_on_from_a_device_without_a_pinned_kind_is_dropped() {
    let (_f, _c, a, r, sink) = activity_world_from("mobile");
    drop_pin(&r, &my_id(&a));
    post_activity_on(&a, &r, &json!({"v": 1, "on": true}));
    assert!(sink.got().is_empty());
}

#[test]
fn activity_on_from_a_revoked_sender_is_dropped() {
    let (_f, _c, a, r, sink) = activity_world_from("mobile");
    let fp = fp_of(&a);
    let target: Vec<Dev> = {
        let g = a.lock();
        g.st.trusted
            .iter()
            .filter(|d| d.id == my_id(&r))
            .cloned()
            .collect()
    };
    {
        let mut g = r.lock();
        g.st.revoked.push(fp.clone());
        g.st.trusted.retain(|d| d.fp != fp);
    }
    a.seal_and_post(
        &mut a.lock(),
        "activity_on",
        &rpc_id("activity_on"),
        &json!({"v": 1, "on": true}),
        &target,
        false,
    )
    .unwrap();
    r.poll_once().unwrap();
    assert!(sink.got().is_empty());
}

#[test]
fn activity_on_from_an_untrusted_device_is_dropped() {
    let (fake, clock, _a, r, sink) = activity_world_from("mobile");
    let stranger = engine_with(&fake, &clock, "Stranger").with_kind("mobile");
    stranger.ensure_registered(&mut stranger.lock()).unwrap();
    let rid = my_id(&r);
    let dev = {
        let g = r.lock();
        g.st.trusted.iter().find(|d| d.id == rid).cloned().unwrap()
    };
    stranger
        .seal_and_post(
            &mut stranger.lock(),
            "activity_on",
            &rpc_id("activity_on"),
            &json!({"v": 1, "on": true}),
            &[dev],
            false,
        )
        .unwrap();
    r.poll_once().unwrap();
    assert!(sink.got().is_empty());
}

#[test]
fn activity_on_with_a_bad_payload_is_dropped() {
    let (_f, _c, a, r, sink) = activity_world_from("mobile");
    for p in [
        json!({"v": 2, "on": true}),
        json!({"on": true}),
        json!({"v": 1, "on": "yes"}),
        json!({"v": 1, "on": 1}),
        json!({"v": 1}),
        json!([1]),
    ] {
        post_activity_on(&a, &r, &p);
    }
    assert!(sink.got().is_empty());
}

#[test]
fn activity_on_from_the_own_device_is_dropped() {
    let (_f, _c, _a, r, sink) = activity_world_from("mobile");
    let rid = my_id(&r);
    let me: Vec<Dev> = r
        .lock()
        .st
        .trusted
        .iter()
        .filter(|d| d.id == rid)
        .cloned()
        .collect();
    r.seal_and_post(
        &mut r.lock(),
        "activity_on",
        &rpc_id("activity_on"),
        &json!({"v": 1, "on": true}),
        &me,
        false,
    )
    .unwrap();
    r.poll_once().unwrap();
    assert!(sink.got().is_empty());
}

#[test]
fn activity_on_without_a_sink_still_advances_the_sequence() {
    let (_f, _c, a, r, sink) = activity_world_from("mobile");
    r.lock().st.joined = true;
    let r2 = {
        let mut r = r;
        r.activity_sub = None;
        r
    };
    post_activity_on(&a, &r2, &json!({"v": 1, "on": true}));
    let m = text_msg(new_message_id(), "after");
    a.send_chat(&m, true).unwrap();
    let got = r2.poll_once().unwrap();
    assert_eq!(got.len(), 1);
    assert!(sink.got().is_empty());
}

fn last_body_to(fake: &Fake, to: &str) -> (String, String, String) {
    let s = fake.0.lock().unwrap();
    let m = s.msgs.iter().rev().find(|m| m.to == to).unwrap();
    (m.msg_id.clone(), m.from.clone(), m.body.clone())
}

#[test]
fn send_activity_round_trips_to_a_phone_only() {
    let (fake, _c, runner, desktop, mobile, _other) = rooms_world();
    let (did, mid) = (my_id(&desktop), my_id(&mobile));
    let dbefore = fake.queue_len(&did);
    let id = rpc_id("activity");
    let payload = json!({"v": 1, "items": [{"k": "tool", "t": "ls"}]});
    runner
        .send_activity(&id, &payload, std::slice::from_ref(&mid))
        .unwrap();
    assert_eq!(fake.queue_len(&did), dbefore);
    let (msg_id, from, body) = last_body_to(&fake, &mid);
    let opened = mobile
        .open_one(
            &mobile.lock(),
            &Msg {
                cursor: 1,
                msg_id,
                from,
                body,
                created_at: Value::Null,
            },
            T0,
        )
        .ok();
    let Some(Inbound::Opened(opened)) = opened else {
        panic!("not opened");
    };
    assert_eq!(opened.kind, "activity");
    assert!(opened.id.starts_with("t_"));
    assert_eq!(opened.payload, payload);
}

#[test]
fn send_activity_refuses_desktop_only_targets_and_bad_input() {
    let (_fake, _c, runner, desktop, mobile, _other) = rooms_world();
    let (did, mid) = (my_id(&desktop), my_id(&mobile));
    let p = json!({"v": 1});
    let err = runner
        .send_activity(&rpc_id("activity"), &p, std::slice::from_ref(&did))
        .unwrap_err();
    assert_eq!(err, "there are no other trusted devices to send to");
    let big = json!({"x": "a".repeat(ACTIVITY_MAX_BYTES)});
    assert!(runner
        .send_activity(&rpc_id("activity"), &big, std::slice::from_ref(&mid))
        .is_err());
    assert!(desktop
        .send_activity(&rpc_id("activity"), &p, std::slice::from_ref(&mid))
        .is_err());
    assert!(runner
        .send_activity(&rpc_id("rooms"), &p, std::slice::from_ref(&mid))
        .is_err());
}

mod blobs;
mod clock;
mod probe;
mod rebind;
mod receipt;
mod repro;

#[test]
fn a_chat_before_this_device_has_joined_is_dropped_as_not_joined() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    b.lock().st.joined = false;
    post_chat(&a, &my_id(&b), &msg_n(3, AuthorKind::Owner));
    b.poll_once().unwrap();
    assert_eq!(b.s.drops(), ["this device has not joined"]);
}

fn revoker_setup(kind: Option<&str>) -> (Fake, Clock, Engine, Engine, Engine) {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let d = join_as(&fake, &clock, &a, "Desktop D", "desktop");
    let x = join_as(&fake, &clock, &a, "Revoker", kind.unwrap_or("desktop"));
    if kind.is_none() {
        drop_pin(&a, &my_id(&x));
        a.lock().st.intros.clear();
        for dev in fake.0.lock().unwrap().devices.iter_mut() {
            dev.intros.clear();
        }
    }
    (fake, clock, a, d, x)
}

fn post_revoke_msg(from: &Engine, to_id: &str, target_id: &str, target_fp: &str) {
    let json = {
        let g = from.lock();
        let keys = g.keys.as_ref().unwrap();
        let st = RevokeStatement {
            user_id: USER,
            revoker_fp: keys.fingerprint(),
            target_id: target_id.into(),
            target_fp: target_fp.into(),
            issued_at_ms: T0,
        };
        sign_revoke(keys, &st).unwrap()
    };
    let parsed = parse_revoke(&json).unwrap();
    let payload = json!({ "statement": parsed.statement, "sig": parsed.sig });
    let mut g = from.lock();
    let target: Vec<Dev> =
        g.st.trusted
            .iter()
            .filter(|d| d.id == to_id)
            .cloned()
            .collect();
    from.seal_and_post(
        &mut g,
        "revoke",
        "r_1111111111111111",
        &payload,
        &target,
        false,
    )
    .unwrap();
}

fn revoke_outcome(kind: Option<&str>, sealed: bool) -> (usize, bool) {
    let (fake, _clock, a, d, x) = revoker_setup(kind);
    let (did, dfp) = (my_id(&d), fp_of(&d));
    if sealed {
        post_revoke_msg(&x, &my_id(&a), &did, &dfp);
    } else {
        directory_revoke(&fake, &x, &did, &dfp);
    }
    a.poll_once().unwrap();
    let ts = a.trust_state().unwrap();
    let trusted = ts
        .devices
        .iter()
        .any(|v| v.id == did && v.state == DeviceState::Trusted);
    (ts.pending_revokes.len(), trusted)
}

#[test]
fn a_revoke_of_a_desktop_signed_by_a_runner_terminal_or_unknown_kind_is_refused() {
    for sealed in [false, true] {
        for kind in [Some("runner"), Some("terminal"), None] {
            assert_eq!(
                revoke_outcome(kind, sealed),
                (0, true),
                "{kind:?} sealed={sealed}"
            );
        }
    }
}

#[test]
fn a_revoke_of_a_desktop_signed_by_a_desktop_or_mobile_is_pending() {
    for sealed in [false, true] {
        for kind in [Some("desktop"), Some("mobile")] {
            assert_eq!(
                revoke_outcome(kind, sealed),
                (1, true),
                "{kind:?} sealed={sealed}"
            );
        }
    }
}

#[test]
fn a_runner_or_terminal_may_revoke_itself() {
    for kind in ["runner", "terminal"] {
        for sealed in [false, true] {
            let (fake, _clock, a, _d, x) = revoker_setup(Some(kind));
            let (xid, xfp) = (my_id(&x), fp_of(&x));
            if sealed {
                post_revoke_msg(&x, &my_id(&a), &xid, &xfp);
            } else {
                directory_revoke(&fake, &x, &xid, &xfp);
            }
            a.poll_once().unwrap();
            assert!(is_revoked(&a, &xid), "{kind} sealed={sealed}");
            assert!(a.trust_state().unwrap().pending_revokes.is_empty());
        }
    }
}

#[test]
fn a_terminal_joins_pinned_as_terminal_and_keeps_the_gate_open() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let x = join_as(&fake, &clock, &a, "Terminal", "terminal");
    assert_eq!(kind_pin(&a, &my_id(&x)).as_deref(), Some("terminal"));
    assert!(gate_blocked(&a.lock(), USER).is_empty());
    join_as(&fake, &clock, &a, "Runner", "runner");
}

#[test]
fn a_terminal_cannot_vouch_issue_codes_approve_or_revoke_others() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let d = join_as(&fake, &clock, &a, "Desktop D", "desktop");
    let x = join_as(&fake, &clock, &a, "Terminal", "terminal");
    let has = |r: Result<(), String>| r.unwrap_err().contains("terminal");
    assert!(x.open_join_code().unwrap_err().contains("terminal"));
    assert!(has(x.decide_join(&my_id(&d), true)));
    assert!(has(x.confirm_device_kind(&my_id(&d), "desktop")));
    assert!(has(x.revoke(&my_id(&d))));
    x.lock().st.pending_revokes.push(PendingRevoke {
        statement_id: "s1".into(),
        revoker_label: "A".into(),
        revoker_fp: fp_of(&a),
        target_id: my_id(&d),
        target_label: "D".into(),
        target_fp: fp_of(&d),
        received_at_ms: 0,
    });
    assert!(has(x.decide_revoke("s1", true)));
    assert_eq!(x.lock().st.pending_revokes.len(), 1);
    x.decide_revoke("s1", false).unwrap();
    assert!(x.lock().st.pending_revokes.is_empty());
}

#[test]
fn a_terminal_publishes_no_intros_for_others_and_may_reset_itself() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let x = join_as(&fake, &clock, &a, "Terminal", "terminal");
    let before = fake
        .0
        .lock()
        .unwrap()
        .devices
        .iter()
        .map(|d| d.intros.len())
        .sum::<usize>();
    publish_for(&x);
    let after = fake
        .0
        .lock()
        .unwrap()
        .devices
        .iter()
        .map(|d| d.intros.len())
        .sum::<usize>();
    assert_eq!(before, after);
    x.reset_identity().unwrap();
}

#[test]
fn terminal_owner_messages_are_accepted_and_manager_session_ones_are_not() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let x = join_as(&fake, &clock, &a, "Terminal", "terminal");
    let aid = my_id(&a);
    for (n, k) in [(2, AuthorKind::Manager), (3, AuthorKind::Session)] {
        post_chat(&x, &aid, &msg_n(n, k));
    }
    assert!(a.poll_once().unwrap().is_empty());
    post_chat(&x, &aid, &msg_n(4, AuthorKind::Owner));
    assert_eq!(a.poll_once().unwrap().len(), 1);
    assert_eq!(*a.s.kinds.lock().unwrap(), [Some("terminal".to_owned())]);
}

fn terminal_world() -> (Fake, Clock, Engine, Engine, Engine) {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let desktop = engine_with(&fake, &clock, "Desktop A");
    let runner = join_as(&fake, &clock, &desktop, "Runner 1", "runner");
    let terminal = join_as(&fake, &clock, &desktop, "Terminal", "terminal");
    runner.poll_once().unwrap();
    (fake, clock, runner, desktop, terminal)
}

#[test]
fn a_runner_seals_output_rooms_and_receipts_to_a_trusted_terminal_like_a_desktop() {
    let (fake, _c, runner, desktop, terminal) = terminal_world();
    let (did, tid) = (my_id(&desktop), my_id(&terminal));
    assert_eq!(runner.owner_devices().len(), 2);
    assert!(runner.owner_devices().contains(&tid));
    assert_eq!(runner.owner_device_ids().unwrap().len(), 2);
    let before = (fake.queue_len(&did), fake.queue_len(&tid));
    runner
        .send_chat(&msg_n(5, AuthorKind::Session), false)
        .unwrap();
    assert_eq!(
        (fake.queue_len(&did), fake.queue_len(&tid)),
        (before.0 + 1, before.1 + 1)
    );
    runner
        .send_chat_to_owners(&msg_n(6, AuthorKind::Session), std::slice::from_ref(&tid))
        .unwrap();
    assert_eq!(fake.queue_len(&tid), before.1 + 2);
    assert_eq!(fake.queue_len(&did), before.0 + 1);
    runner
        .send_sealed_to_owners("rooms", &rpc_id("rooms"), &json!({"v": 1}), None)
        .unwrap();
    assert_eq!(
        (fake.queue_len(&did), fake.queue_len(&tid)),
        (before.0 + 2, before.1 + 3)
    );
    runner
        .send_receipt(&tid, &mid_of(7), ReceiptState::Delivered, None)
        .unwrap();
    assert_eq!(fake.queue_len(&tid), before.1 + 4);
}

fn mid_of(n: u8) -> String {
    msg_n(n, AuthorKind::Owner).id
}

fn revoke_json(from: &Engine, target_id: &str, target_fp: &str) -> String {
    let g = from.lock();
    let keys = g.keys.as_ref().unwrap();
    let st = RevokeStatement {
        user_id: USER,
        revoker_fp: keys.fingerprint(),
        target_id: target_id.into(),
        target_fp: target_fp.into(),
        issued_at_ms: T0,
    };
    sign_revoke(keys, &st).unwrap()
}

fn pending_after_unlearned_revoke(root: bool) -> usize {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let (_c, cid, c_fp) = registered(&fake, &clock, "Desktop C", "desktop");
    let aid = my_id(&a);
    put_raw_intro(&fake, &aid, &cid, &signed_intro(&a, &cid, &c_fp, "desktop"));
    b.poll_once().unwrap();
    assert!(is_trusted_with(&b, &cid, "desktop"));
    let json = revoke_json(&a, &cid, &c_fp);
    let mut g = b.lock();
    g.st.kinds
        .retain(|k, _| !k.starts_with(&kind_key(&aid, "")));
    if !root {
        g.st.base.retain(|d| d.id != aid);
    }
    ingest_revoke(&mut g, USER, T0, &json, None, &mut BTreeMap::new());
    g.st.pending_revokes.len()
}

#[test]
fn a_root_of_unknown_kind_may_revoke_but_a_non_root_of_unknown_kind_may_not() {
    assert_eq!(pending_after_unlearned_revoke(true), 1);
    assert_eq!(pending_after_unlearned_revoke(false), 0);
}

#[test]
fn a_restart_without_a_flush_does_not_hold_the_next_rpc() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let store = Arc::new(MemKeyStore::default());
    let runner = |store: Arc<MemKeyStore>| {
        let mut r = engine_on(&fake, &clock, store, "").with_kind("runner");
        let sink = RpcRec::default();
        r.rpc = Some(Arc::new(sink.clone()));
        (r, sink)
    };
    let (r, _) = runner(store.clone());
    let code = a.open_join_code().unwrap();
    r.join_with_code(&code).unwrap();
    a.poll_once().unwrap();
    a.decide_join(&my_id(&r), true).unwrap();
    r.poll_once().unwrap();
    let r1 = join_as(&fake, &clock, &a, "Runner 1", "runner");
    for e in [&r1, &r, &a] {
        e.poll_once().unwrap();
    }
    let rid = my_id(&r);
    r1.send_rpc(&rid, "mgr_req", &rpc_id("mgr_req"), &json!({ "op": "w" }))
        .unwrap();
    r.poll_once().unwrap();
    r.save_now(&mut r.lock()).unwrap();
    clock.set(T0 + 1_000);
    r1.send_rpc(&rid, "mgr_req", &rpc_id("mgr_req"), &json!({ "op": "x" }))
        .unwrap();
    r.poll_once().unwrap();
    drop(r);
    let (r, sink) = runner(store);
    clock.set(T0 + 2_000);
    let id = rpc_id("mgr_req");
    r1.send_rpc(&rid, "mgr_req", &id, &json!({ "op": "y" }))
        .unwrap();
    r.poll_once().unwrap();
    assert!(
        sink.0.lock().unwrap().iter().any(|g| g.3 == id),
        "the rpc was held"
    );
    assert!(r.lock().st.seq_gaps.is_empty());
}

#[test]
fn a_permit_refresh_from_a_trusted_desktop_or_phone_reaches_the_sink() {
    for (pin, want) in [
        ("desktop", tmgr_spawn_auth::PermitSenderKind::Desktop),
        ("mobile", tmgr_spawn_auth::PermitSenderKind::Mobile),
    ] {
        let (_f, _c, a, r, sink) = permitok_world_from(pin);
        post_permitok(&a, &r, &refresh_payload());
        assert_eq!(sink.n(), 0);
        assert_eq!(
            sink.refreshes(),
            vec![(
                my_id(&a),
                fp_of(&a),
                want,
                "card_00112233445566778899aabbccddeeff".to_owned(),
                "ab".repeat(16)
            )]
        );
    }
}

#[test]
fn a_permit_refresh_from_a_runner_or_untrusted_or_revoked_sender_is_dropped() {
    let (_f, _c, a, r, sink) = permitok_world_from("desktop");
    set_pin(&r, &my_id(&a), "runner");
    post_permitok(&a, &r, &refresh_payload());
    assert!(sink.refreshes().is_empty());

    let (fake, clock, _a, r, sink) = permitok_world_from("desktop");
    let stranger = engine_with(&fake, &clock, "Stranger").with_kind("desktop");
    stranger.ensure_registered(&mut stranger.lock()).unwrap();
    let rid = my_id(&r);
    let dev = {
        let g = r.lock();
        g.st.trusted.iter().find(|d| d.id == rid).cloned().unwrap()
    };
    stranger
        .seal_and_post(
            &mut stranger.lock(),
            "permitok",
            &rpc_id("permitok"),
            &refresh_payload(),
            &[dev],
            false,
        )
        .unwrap();
    r.poll_once().unwrap();
    assert!(sink.refreshes().is_empty());

    let (_f, _c, a, r, sink) = permitok_world_from("desktop");
    r.lock().st.revoked.push(fp_of(&a));
    post_permitok(&a, &r, &refresh_payload());
    assert!(sink.refreshes().is_empty());
}

#[test]
fn a_garbage_permit_refresh_is_dropped() {
    let (_f, _c, a, r, sink) = permitok_world_from("desktop");
    let mut extra = refresh_payload();
    extra["x"] = json!(1);
    let mut bad_v = refresh_payload();
    bad_v["v"] = json!(2);
    let mut no_nonce = refresh_payload();
    no_nonce.as_object_mut().unwrap().remove("nonce");
    let mut huge = refresh_payload();
    huge["refresh"] = json!("c".repeat(4096));
    let mut num = refresh_payload();
    num["refresh"] = json!(7);
    for junk in [
        extra,
        bad_v,
        no_nonce,
        huge,
        num,
        json!({"v": 1, "refresh": "c"}),
    ] {
        post_permitok(&a, &r, &junk);
    }
    assert!(sink.refreshes().is_empty());
    assert_eq!(sink.n(), 0);
}

#[test]
fn a_permit_refresh_from_a_non_desktop_non_mobile_pin_is_dropped() {
    for kind in ["runner", "terminal", "web", "mobile2", ""] {
        let (_f, _c, a, r, sink) = permitok_world_from("desktop");
        set_pin(&r, &my_id(&a), kind);
        post_permitok(&a, &r, &refresh_payload());
        assert!(sink.refreshes().is_empty(), "{kind}");
    }
}

#[test]
fn a_permit_refresh_from_a_device_with_a_pending_revoke_is_dropped() {
    let (_f, _c, a, r, sink) = permitok_world_from("desktop");
    r.lock().st.pending_revokes.push(PendingRevoke {
        statement_id: "s".into(),
        revoker_label: "x".into(),
        revoker_fp: "11".repeat(32),
        target_id: my_id(&a),
        target_label: "Sender".into(),
        target_fp: fp_of(&a),
    });
    post_permitok(&a, &r, &refresh_payload());
    post_permitok(&a, &r, &permitok_payload(&fp_of(&a)));
    assert!(sink.refreshes().is_empty());
    assert_eq!(sink.n(), 0);
}

#[test]
fn a_permit_refresh_before_the_runner_has_joined_is_dropped() {
    let (_f, _c, a, r, sink) = permitok_world_from("desktop");
    r.lock().st.joined = false;
    post_permitok(&a, &r, &refresh_payload());
    assert!(sink.refreshes().is_empty());
}

#[test]
fn a_runner_cannot_revoke_other_devices() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let d = join_as(&fake, &clock, &a, "Desktop D", "desktop");
    let x = join_as(&fake, &clock, &a, "Runner", "runner");
    let e = x.revoke(&my_id(&d)).unwrap_err();
    assert!(e.contains("runner"), "{e}");
}

#[test]
fn a_terminal_does_not_send_chat_to_a_device_with_a_pending_revoke() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let d = join_as(&fake, &clock, &a, "Desktop D", "desktop");
    let x = join_as(&fake, &clock, &a, "Terminal", "terminal");
    x.poll_once().unwrap();
    let (did, dfp) = (my_id(&d), fp_of(&d));
    x.lock().st.pending_revokes.push(PendingRevoke {
        statement_id: "s1".into(),
        revoker_label: "A".into(),
        revoker_fp: fp_of(&a),
        target_id: did.clone(),
        target_label: "D".into(),
        target_fp: dfp,
    });
    let (before_d, before_a) = (fake.queue_len(&did), fake.queue_len(&my_id(&a)));
    x.send_chat(&text_msg(new_message_id(), "hi"), false)
        .unwrap();
    assert_eq!(fake.queue_len(&did), before_d);
    assert!(fake.queue_len(&my_id(&a)) > before_a);
}

#[test]
fn a_join_ok_from_a_root_the_relay_lists_as_a_terminal_or_runner_is_refused() {
    for kind in ["terminal", "runner"] {
        let (fake, clock) = (Fake::default(), Clock::new(T0));
        let a = engine_with(&fake, &clock, "Desktop A");
        let b = engine_with(&fake, &clock, "Desktop B");
        let code = a.open_join_code().unwrap();
        let aid = my_id(&a);
        b.join_with_code(&code).unwrap();
        for dev in fake.0.lock().unwrap().devices.iter_mut() {
            if dev.id == aid {
                dev.kind = kind.to_owned();
            }
        }
        b.lock().dir = None;
        a.poll_once().unwrap();
        a.decide_join(&my_id(&b), true).unwrap();
        b.poll_once().unwrap();
        assert!(!b.is_joined(), "{kind}");
    }
}

#[derive(Clone, Default)]
struct RoomsRec(Arc<Mutex<Vec<(String, RoomsIndex)>>>);

impl RoomsSink for RoomsRec {
    fn deliver_rooms(&self, from: &str, index: RoomsIndex) {
        self.0.lock().unwrap().push((from.to_owned(), index));
    }
}

fn index_json(machine: &str) -> Value {
    json!({
        "v": 1,
        "machine": machine,
        "machine_label": "Studio",
        "role": "top",
        "rooms": [{"room": {"kind": "manager", "machine": machine}, "title": "Manager"}]
    })
}

fn with_rooms_sink(fake: &Fake, clock: &Clock, label: &str) -> (Engine, RoomsRec) {
    let rec = RoomsRec::default();
    let c = clock.0.clone();
    let e = RelayEngine::new(
        fake.clone(),
        Arc::new(MemKeyStore::default()),
        Rec::default(),
        EngineConfig {
            user_id: USER,
            label: label.into(),
            local_machine: "mx_0000000000000001".into(),
        },
    )
    .unwrap()
    .with_clock(move || c.load(Ordering::SeqCst))
    .with_rooms_sink(Arc::new(rec.clone()));
    (e, rec)
}

#[test]
fn a_trusted_runners_rooms_index_reaches_the_rooms_sink_and_is_deduped() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, rec) = with_rooms_sink(&fake, &clock, "Desktop A");
    let r = join_as(&fake, &clock, &a, "Runner 1", "runner");
    let aid = my_id(&a);
    let id = rpc_id("rooms");
    assert!(valid_index_msg_id(&id));
    raw_post(&r, &aid, "rooms", &id, &index_json("mx_00000000000000bb"));
    a.poll_once().unwrap();
    let got = rec.0.lock().unwrap().clone();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, my_id(&r));
    assert_eq!(got[0].1.machine, "mx_00000000000000bb");
    raw_post(&r, &aid, "rooms", &id, &index_json("mx_00000000000000bb"));
    a.poll_once().unwrap();
    assert_eq!(rec.0.lock().unwrap().len(), 1);
}

#[test]
fn a_rooms_index_from_a_non_runner_a_bad_id_or_a_bad_body_is_ignored() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, rec) = with_rooms_sink(&fake, &clock, "Desktop A");
    let phone = join_as(&fake, &clock, &a, "Phone", "mobile");
    let r = join_as(&fake, &clock, &a, "Runner 1", "runner");
    let aid = my_id(&a);
    raw_post(
        &phone,
        &aid,
        "rooms",
        &rpc_id("rooms"),
        &index_json("mx_00000000000000bb"),
    );
    raw_post(
        &r,
        &aid,
        "rooms",
        "i_short",
        &index_json("mx_00000000000000bb"),
    );
    raw_post(&r, &aid, "rooms", &rpc_id("rooms"), &json!({"v": 1}));
    a.poll_once().unwrap();
    assert!(rec.0.lock().unwrap().is_empty());
}

#[test]
fn a_second_runner_cannot_claim_a_machine_bound_to_another_runner_in_the_rooms_index() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, rec) = with_rooms_sink(&fake, &clock, "Desktop A");
    let r1 = join_as(&fake, &clock, &a, "Runner 1", "runner");
    let r2 = join_as(&fake, &clock, &a, "Runner 2", "runner");
    let aid = my_id(&a);
    raw_post(
        &r1,
        &aid,
        "rooms",
        &rpc_id("rooms"),
        &index_json("mx_00000000000000bb"),
    );
    raw_post(
        &r2,
        &aid,
        "rooms",
        &rpc_id("rooms"),
        &index_json("mx_00000000000000bb"),
    );
    a.poll_once().unwrap();
    assert_eq!(rec.0.lock().unwrap().len(), 1);
    assert!(alert_kinds(&a).contains(&"room_conflict".to_owned()));
}

#[test]
fn a_rooms_index_is_ignored_without_a_sink() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let r = join_as(&fake, &clock, &a, "Runner 1", "runner");
    raw_post(
        &r,
        &my_id(&a),
        "rooms",
        &rpc_id("rooms"),
        &index_json("mx_00000000000000bb"),
    );
    assert!(a.poll_once().unwrap().is_empty());
    assert!(alert_kinds(&a).is_empty());
}

#[test]
fn send_for_machine_reaches_owner_devices_and_the_bound_runner_only() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let phone = join_as(&fake, &clock, &a, "Phone", "mobile");
    let r1 = join_as(&fake, &clock, &a, "Runner 1", "runner");
    let r2 = join_as(&fake, &clock, &a, "Runner 2", "runner");
    let (aid, pid, r1id, r2id) = (my_id(&a), my_id(&phone), my_id(&r1), my_id(&r2));
    let m = msg_n(2, AuthorKind::Manager);
    assert!(a
        .send_chat_for_machine(&text_msg(new_message_id(), "early"), false)
        .unwrap_err()
        .contains("has been seen"));
    post_chat(&r1, &aid, &m);
    assert_eq!(a.poll_once().unwrap().len(), 1);
    let before = (
        fake.queue_len(&pid),
        fake.queue_len(&r1id),
        fake.queue_len(&r2id),
    );
    let mut out = text_msg(new_message_id(), "hello");
    out.room = m.room.clone();
    a.send_chat_for_machine(&out, true).unwrap();
    assert_eq!(
        (
            fake.queue_len(&pid),
            fake.queue_len(&r1id),
            fake.queue_len(&r2id)
        ),
        (before.0 + 1, before.1 + 1, before.2)
    );
}

#[test]
fn send_for_machine_skips_a_runner_with_a_pending_revoke() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let r = join_as(&fake, &clock, &a, "Runner 1", "runner");
    b.poll_once().unwrap();
    let (bid, rid) = (my_id(&b), my_id(&r));
    let m = msg_n(2, AuthorKind::Manager);
    post_chat(&r, &bid, &m);
    assert_eq!(b.poll_once().unwrap().len(), 1);
    let mut out = text_msg(new_message_id(), "hello");
    out.room = m.room.clone();
    a.revoke(&rid).unwrap();
    b.poll_once().unwrap();
    assert_eq!(b.trust_state().unwrap().pending_revokes.len(), 1);
    let before = fake.queue_len(&rid);
    let e = b.send_chat_for_machine(&out, true).unwrap_err();
    assert!(e.contains("revok"), "{e}");
    assert_eq!(fake.queue_len(&rid), before);
}

#[test]
fn receipts_are_capped_by_evicting_the_oldest_and_keep_the_latest_state_per_id() {
    let mut c = Core::default();
    let receipt = |id: String, state| Receipt {
        id,
        state,
        reason: None,
    };
    push_receipt(&mut c, receipt("same".into(), ReceiptState::Queued));
    push_receipt(&mut c, receipt("same".into(), ReceiptState::Failed));
    push_receipt(&mut c, receipt("same".into(), ReceiptState::Queued));
    assert_eq!(c.receipts.len(), 1);
    assert_eq!(c.receipts[0].state, ReceiptState::Failed);
    for n in 0..(MAX_RECEIPTS + 10) {
        push_receipt(&mut c, receipt(format!("r{n}"), ReceiptState::Delivered));
    }
    assert_eq!(c.receipts.len(), MAX_RECEIPTS);
    assert_eq!(
        c.receipts.last().unwrap().id,
        format!("r{}", MAX_RECEIPTS + 9)
    );
    assert!(!c.receipts.iter().any(|r| r.id == "same"));
}

#[test]
fn leave_confirmed_tells_the_others_deletes_the_device_and_clears_the_store() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let (old_id, old_fp) = (my_id(&b), fp_of(&b));
    assert_eq!(b.leave_confirmed().unwrap(), None);
    assert!(fake
        .0
        .lock()
        .unwrap()
        .devices
        .iter()
        .all(|d| d.id != old_id));
    assert!(b.own_device().is_none());
    assert!(b.k.load(Slot::Identity).unwrap().is_none());
    assert!(b.k.load(Slot::State).unwrap().is_none());
    a.poll_once().unwrap();
    assert!(a.lock().st.revoked.contains(&old_fp));
    assert!(a.trust_state().unwrap().pending_revokes.is_empty());
}

#[test]
fn leave_confirmed_keeps_the_identity_and_state_when_the_relay_does_not_confirm() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (_a, b) = pair(&fake, &clock);
    fake.0.lock().unwrap().delete_error = Some("boom".into());
    let e = b.leave_confirmed().unwrap_err();
    assert!(e.contains("did not confirm"), "{e}");
    assert!(b.own_device().is_some());
    assert!(b.k.load(Slot::Identity).unwrap().is_some());
    assert!(b.k.load(Slot::State).unwrap().is_some());
    fake.0.lock().unwrap().delete_error = None;
    assert_eq!(b.leave_confirmed().unwrap(), None);
    assert!(b.own_device().is_none());
    assert!(b.k.load(Slot::Identity).unwrap().is_none());
    assert!(b.k.load(Slot::State).unwrap().is_none());
}

#[test]
fn leave_confirmed_without_an_identity_changes_nothing() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let e = engine_with(&fake, &clock, "Desktop A");
    assert_eq!(e.leave_confirmed().unwrap(), None);
    assert_eq!(fake.0.lock().unwrap().devices.len(), 0);
}

#[test]
fn leave_confirmed_treats_a_device_the_relay_no_longer_knows_as_removed() {
    for (gone, unauthorized) in [(true, false), (false, true)] {
        let (fake, clock) = (Fake::default(), Clock::new(T0));
        let (_a, b) = pair(&fake, &clock);
        {
            let mut s = fake.0.lock().unwrap();
            s.delete_gone = gone;
            s.delete_unauthorized = unauthorized;
        }
        assert!(b.leave_confirmed().is_ok());
        assert!(b.own_device().is_none());
        assert!(b.k.load(Slot::Identity).unwrap().is_none());
    }
}

#[test]
fn a_404_while_the_device_is_still_listed_is_not_a_removal() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (_a, b) = pair(&fake, &clock);
    fake.0.lock().unwrap().delete_404_listed = true;
    let e = b.leave_confirmed().unwrap_err();
    assert!(e.contains("did not confirm"), "{e}");
    assert!(b.own_device().is_some());
}

#[test]
fn a_status_claimed_inside_a_relay_error_message_is_not_a_removal() {
    assert_eq!(http_status(r#"{"http_status":404}"#), Some(404));
    assert_eq!(http_status("not json http_status 404"), None);
    assert_eq!(http_status(r#"{"error":"x"}"#), None);
}

#[test]
fn own_removed_is_false_while_listed_and_true_once_another_device_revoked_it() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    assert_eq!(b.own_removed(), Ok(false));
    fake.0.lock().unwrap().dir_error = true;
    assert!(b.own_removed().is_err());
    fake.0.lock().unwrap().dir_error = false;
    a.revoke(&my_id(&b)).unwrap();
    assert_eq!(b.own_removed(), Ok(true));
}

#[test]
fn own_removed_is_false_for_a_device_that_never_registered() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let e = engine_with(&fake, &clock, "Desktop A");
    e.leave_confirmed().unwrap();
    assert_eq!(e.own_removed(), Ok(false));
}

#[test]
fn a_failed_delete_after_the_revoke_notice_says_the_device_is_revoked_but_its_row_remains() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let bfp = fp_of(&b);
    fake.0.lock().unwrap().delete_error = Some("boom".into());
    let e = b.leave_confirmed().unwrap_err();
    assert!(
        e.contains("already revoked") && e.contains("relay row is still there"),
        "{e}"
    );
    assert!(!e.contains("nothing was deleted"), "{e}");
    a.poll_once().unwrap();
    assert!(a.lock().st.revoked.contains(&bfp));
}

#[test]
fn the_sent_log_survives_a_restart_and_is_bounded() {
    let (a, _p, r1, _r2, id) = receipt::sent_world();
    let log = a.sent_log();
    assert!(log.iter().any(|(m, _)| *m == id));
    a.restore_sent(Vec::new());
    r1.send_receipt(&my_id(&a), &id, ReceiptState::Failed, Some("busy"))
        .unwrap();
    a.poll_once().unwrap();
    assert!(a.take_receipts().is_empty());
    a.restore_sent(log);
    r1.send_receipt(&my_id(&a), &id, ReceiptState::Delivered, None)
        .unwrap();
    a.poll_once().unwrap();
    assert_eq!(a.take_receipts().len(), 1);
    let many: Vec<(String, String)> = (0..SENT_KEEP + 50)
        .map(|n| (format!("m{n}"), "r".into()))
        .collect();
    a.restore_sent(many);
    let kept = a.sent_log();
    assert_eq!(kept.len(), SENT_KEEP);
    assert_eq!(kept.last().unwrap().0, format!("m{}", SENT_KEEP + 49));
}
