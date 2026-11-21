//! Relay engine: chat rooms of other machines over the zero-knowledge relay (spec TM-1017 sections 1-4a).
//! One mutex guards all state, so there is a single writer; transport calls happen under it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tmgr_chat_relay::api::{self, Directory, Msg, Request};
use tmgr_chat_relay::fields::{check_device_id, check_fp};
use tmgr_chat_relay::keys::{fingerprint, DeviceKeys};
use tmgr_chat_relay::trust::{
  decode_join_code, encode_join_code, join_mac, parse_intro, parse_revoke, sign_intro, sign_revoke, trusted_set, verify_join_mac,
  verify_revoke, DirEntry, IntroStatement, JoinCode, JoinMacInput, RevokeStatement, Trusted,
};
use tmgr_chat_relay::wire::{open_message, random_msg_id, seal_message, Opened, Peer, SealParams};
use tmgr_phone::{b64, b64_decode, h, hex};

use super::port::{DeviceState, PendingJoin, PendingRevoke, RelayPort, SeqGap, TrustDevice, TrustState};
use super::ports::{KeyStore, Slot, Transport};
use crate::manager::backend::{
  ChatMessage, HistoryParams, RoomInfo, RoomKind, RoomRef, SearchParams, SendParams, MAX_ATTACHMENTS, MAX_BODY, MAX_HISTORY, MAX_QUERY,
};

const JOIN_TTL_MS: u64 = 10 * 60 * 1000;
const SEEN_TTL_MS: u64 = 31 * 24 * 3600 * 1000;
const MAX_LOG: usize = 2000;
const MAX_SEEN: usize = 5000;
const MAX_GAPS: usize = 100;
const MAX_HELD: usize = 200;
const HOLD_MS: u64 = 60_000;
const WRITE_EVERY_MS: u64 = 30_000;
const MAX_INTROS: usize = 200;
const MAX_REVOKES_PER_DEVICE: usize = 50;
const BUSY_PREFIX: &str = "relay is busy, retry in ";

#[cfg(test)]
mod tests;

fn err<E: Display>(e: E) -> String {
  e.to_string()
}

/// Transport errors are the response body of a failed call; for a 429 with Retry-After the transport adds `"retry_after": <secs>`.
fn post_error(e: String) -> String {
  let v: Value = serde_json::from_str(&e).unwrap_or(Value::Null);
  match (v["error"].as_str(), v["retry_after"].as_u64()) {
    (Some("user_full"), _) => "relay storage is full".into(),
    (Some("recipient_full"), _) => "the recipient's relay queue is full".into(),
    (_, Some(secs)) => format!("{BUSY_PREFIX}{secs}s"),
    _ => e,
  }
}

pub fn retry_after_secs(e: &str) -> Option<u64> {
  e.strip_prefix(BUSY_PREFIX)?.strip_suffix('s')?.parse().ok()
}

#[derive(Clone, Debug)]
pub struct EngineConfig {
  pub user_id: u64,
  pub label: String,
  pub local_machine: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Dev {
  id: String,
  kind: String,
  label: String,
  fp: String,
  sign_pub: String,
  kex_pub: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct Identity {
  device_id: String,
  token: String,
  sign_secret: String,
  kex_secret: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct OpenCode {
  secret: String,
  exp_ms: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct JoinRec {
  device_id: String,
  label: String,
  fp: String,
  kind: String,
  requested_at_ms: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct Seen {
  from: String,
  id: String,
  at: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct State {
  base: Vec<Dev>,
  trusted: Vec<Dev>,
  revoked: Vec<String>,
  revoked_devs: Vec<Dev>,
  intros: Vec<String>,
  cursor: u64,
  seq_out: BTreeMap<String, u64>,
  joined: bool,
  join_code: Option<OpenCode>,
  pending_joins: Vec<JoinRec>,
  pending_revokes: Vec<PendingRevoke>,
  declined: Vec<String>,
  seen: Vec<Seen>,
  mismatch: Vec<String>,
  #[serde(default)]
  seq_in: BTreeMap<String, u64>,
  #[serde(default)]
  seq_gaps: Vec<SeqGap>,
}

#[derive(Clone, Serialize, Deserialize)]
struct LogEntry {
  #[serde(default)]
  from: String,
  #[serde(flatten)]
  msg: ChatMessage,
}

#[derive(Default)]
struct Hold {
  stuck: Option<u64>,
  blocked: BTreeSet<String>,
  held: usize,
}

#[derive(Default)]
struct Core {
  identity: Option<Identity>,
  keys: Option<DeviceKeys>,
  st: State,
  log: Vec<LogEntry>,
  log_dirty: bool,
  dir: Option<Directory>,
  joined_now: bool,
  written_state: String,
  written_trust: String,
  last_write: u64,
  acked: u64,
  hold_since: BTreeMap<String, u64>,
}

fn trust_key(st: &State) -> String {
  json!([st.base, st.trusted, st.revoked, st.pending_joins, st.pending_revokes, st.declined, st.seq_gaps]).to_string()
}

fn dev_from(d: &api::Device) -> Dev {
  Dev {
    id: d.device_id.clone(),
    kind: d.kind.clone(),
    label: d.label.clone(),
    fp: fingerprint(&d.sign_pub, &d.kex_pub),
    sign_pub: d.sign_pub.clone(),
    kex_pub: d.kex_pub.clone(),
  }
}

fn to_trusted(d: &Dev) -> Trusted {
  Trusted { device_id: d.id.clone(), fp: d.fp.clone(), sign_pub: d.sign_pub.clone(), kex_pub: d.kex_pub.clone() }
}

fn intro_matches(i: &str, uid: u64) -> bool {
  parse_intro(i).is_ok_and(|p| p.parsed.user_id == uid)
}

fn validate_chat(m: &ChatMessage) -> Result<(), String> {
  let id_ok = m.id.len() == 34 && m.id.starts_with("m_") && m.id[2..].bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'));
  let room_ok = match m.room.kind {
    RoomKind::Manager => m.room.session.is_none(),
    RoomKind::Session => m.room.session.is_some(),
  };
  if m.v != 1 || !id_ok || !room_ok || m.kind.is_empty() || m.author.kind.is_empty() {
    return Err("invalid message".into());
  }
  if m.body_md.len() > MAX_BODY || m.attachments.len() > MAX_ATTACHMENTS {
    return Err("message is too large".into());
  }
  Ok(())
}

fn recompute(c: &mut Core, dir: Option<&Directory>, uid: u64, from_base: bool) {
  let no_devices: &[api::Device] = &[];
  let dir_devices = dir.map_or(no_devices, |d| d.devices.as_slice());
  let mut intros: Vec<String> = c.st.intros.clone();
  for i in dir_devices.iter().flat_map(|d| d.intros.iter()) {
    if !intros.contains(i) {
      intros.push(i.clone());
    }
  }
  intros.retain(|i| intro_matches(i, uid));
  let mut pins: Vec<Trusted> = c.st.base.iter().map(to_trusted).collect();
  if !from_base {
    pins.extend(c.st.trusted.iter().map(to_trusted));
  }
  let mut entries: Vec<DirEntry> = dir_devices
    .iter()
    .map(|d| DirEntry { device_id: d.device_id.clone(), sign_pub_b64: d.sign_pub.clone(), kex_pub_b64: d.kex_pub.clone() })
    .collect();
  entries.extend(
    c.st.trusted.iter().chain(c.st.base.iter()).map(|d| DirEntry { device_id: d.id.clone(), sign_pub_b64: d.sign_pub.clone(), kex_pub_b64: d.kex_pub.clone() }),
  );
  let set = trusted_set(uid, &pins, &c.st.revoked, &intros, &entries);
  let mut out: Vec<Dev> = Vec::new();
  for t in &set.trusted {
    let in_dir = dir_devices.iter().find(|d| d.device_id == t.device_id && fingerprint(&d.sign_pub, &d.kex_pub) == t.fp);
    let known = c.st.trusted.iter().chain(c.st.base.iter()).find(|d| d.id == t.device_id && d.fp == t.fp).cloned();
    if let Some(mut dev) = known.or_else(|| in_dir.map(dev_from)) {
      if let Some(d) = in_dir {
        dev.label = d.label.clone();
        dev.kind = d.kind.clone();
      }
      out.push(dev);
    }
  }
  if dir.is_some() {
    c.st.mismatch = Vec::new();
    for m in &set.mismatches {
      if !c.st.mismatch.contains(&m.device_id) {
        c.st.mismatch.push(m.device_id.clone());
      }
    }
    for id in &c.st.mismatch {
      log::warn!("[chat-relay] directory keys of device {id} differ from the pinned ones");
    }
  }
  c.st.intros = intros
    .into_iter()
    .filter(|i| parse_intro(i).is_ok_and(|p| out.iter().any(|t| t.id == p.parsed.target_id && t.fp == p.parsed.target_fp)))
    .take(MAX_INTROS)
    .collect();
  c.st.trusted = out;
}

fn apply_revoke(c: &mut Core, uid: u64, target_id: &str, target_fp: &str, label: &str) {
  if !c.st.revoked.iter().any(|f| f == target_fp) {
    c.st.revoked.push(target_fp.to_owned());
  }
  let kind = c.st.trusted.iter().find(|d| d.fp == target_fp).map(|d| d.kind.clone()).unwrap_or_default();
  c.st.revoked_devs.push(Dev { id: target_id.to_owned(), kind, label: label.to_owned(), fp: target_fp.to_owned(), sign_pub: String::new(), kex_pub: String::new() });
  c.st.pending_revokes.retain(|p| p.target_fp != target_fp);
  c.st.pending_joins.retain(|j| j.fp != target_fp);
  let dir = c.dir.clone();
  recompute(c, dir.as_ref(), uid, true);
}

fn ingest_revoke(c: &mut Core, uid: u64, json: &str, expect_fp: Option<&str>, budget: &mut BTreeMap<String, usize>) {
  let Ok(r) = parse_revoke(json) else { return };
  let p = &r.parsed;
  if p.user_id != uid || expect_fp.is_some_and(|f| f != p.target_fp) {
    return;
  }
  let Some(revoker) = c.st.trusted.iter().find(|d| d.fp == p.revoker_fp).cloned() else { return };
  if verify_revoke(json, uid, &revoker.sign_pub, &revoker.kex_pub).is_err() {
    return;
  }
  let used = budget.entry(p.revoker_fp.clone()).or_default();
  *used += 1;
  if *used > MAX_REVOKES_PER_DEVICE || c.st.revoked.contains(&p.target_fp) {
    return;
  }
  let sid = h(&r.statement);
  if c.st.declined.contains(&sid) || c.st.pending_revokes.iter().any(|x| x.statement_id == sid) {
    return;
  }
  let own_fp = c.keys.as_ref().map(|k| k.fingerprint());
  let target_label = c.st.trusted.iter().find(|d| d.fp == p.target_fp).map(|d| d.label.clone()).unwrap_or_default();
  if p.revoker_fp == p.target_fp || own_fp.as_deref() == Some(p.revoker_fp.as_str()) {
    apply_revoke(c, uid, &p.target_id, &p.target_fp, &target_label);
  } else {
    c.st.pending_revokes.push(PendingRevoke {
      statement_id: sid,
      revoker_label: revoker.label,
      revoker_fp: p.revoker_fp.clone(),
      target_id: p.target_id.clone(),
      target_label,
      target_fp: p.target_fp.clone(),
    });
  }
}

pub struct RelayEngine<T: Transport, K: KeyStore> {
  t: T,
  k: K,
  cfg: EngineConfig,
  now: Box<dyn Fn() -> u64 + Send + Sync>,
  core: Mutex<Core>,
}

impl<T: Transport, K: KeyStore> RelayEngine<T, K> {
  pub fn new(t: T, k: K, cfg: EngineConfig) -> Result<Self, String> {
    let mut core = Core::default();
    if let Some(raw) = k.load(Slot::Identity)? {
      let id: Identity = serde_json::from_str(&raw).map_err(|_| "the chat identity is unreadable".to_owned())?;
      let keys = DeviceKeys::from_secrets(&b64_decode(&id.sign_secret).map_err(err)?, &b64_decode(&id.kex_secret).map_err(err)?).map_err(err)?;
      core.keys = Some(keys);
      core.identity = Some(id);
      if let Some(raw) = k.load(Slot::State)? {
        core.st = serde_json::from_str(&raw).map_err(|_| "the chat state is unreadable".to_owned())?;
      }
      if let Some(raw) = k.load(Slot::Log)? {
        core.log = serde_json::from_str(&raw).map_err(|_| "the chat log is unreadable".to_owned())?;
      }
      core.written_state = serde_json::to_string(&core.st).map_err(err)?;
      core.written_trust = trust_key(&core.st);
      core.acked = core.st.cursor;
    }
    let now = Box::new(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64));
    Ok(Self { t, k, cfg, now, core: Mutex::new(core) })
  }

  pub fn with_clock(mut self, now: impl Fn() -> u64 + Send + Sync + 'static) -> Self {
    self.now = Box::new(now);
    self
  }

  fn lock(&self) -> MutexGuard<'_, Core> {
    self.core.lock().unwrap_or_else(|p| p.into_inner())
  }

  fn now(&self) -> u64 {
    (self.now)()
  }

  fn call(&self, c: &Core, req: Request) -> Result<String, String> {
    self.t.call(&req, c.identity.as_ref().map(|i| i.token.as_str()))
  }

  fn save_now(&self, c: &mut Core) -> Result<(), String> {
    self.persist(c, true).map(|_| ())
  }

  /// Poll-time writes are throttled unless trust changed; a message is acked only once the log holding it is stored.
  fn persist(&self, c: &mut Core, force: bool) -> Result<bool, String> {
    let st = serde_json::to_string(&c.st).map_err(err)?;
    let state_changed = st != c.written_state;
    if !state_changed && !c.log_dirty {
      return Ok(true);
    }
    let now = self.now();
    let trust = trust_key(&c.st);
    if !force && trust == c.written_trust && now.abs_diff(c.last_write) < WRITE_EVERY_MS {
      return Ok(false);
    }
    if c.log_dirty {
      if c.log.len() > MAX_LOG {
        let cut = c.log.len() - MAX_LOG;
        c.log.drain(..cut);
      }
      self.k.save(Slot::Log, &serde_json::to_string(&c.log).map_err(err)?)?;
      c.log_dirty = false;
    }
    if state_changed {
      self.k.save(Slot::State, &st)?;
      c.written_state = st;
    }
    c.written_trust = trust;
    c.last_write = now;
    Ok(true)
  }

  fn own(c: &Core) -> Result<(&Identity, &DeviceKeys), String> {
    match (&c.identity, &c.keys) {
      (Some(i), Some(k)) => Ok((i, k)),
      _ => Err("chat sync is not set up".into()),
    }
  }

  fn ensure_registered(&self, c: &mut Core) -> Result<(), String> {
    if c.identity.is_some() {
      return Ok(());
    }
    let keys = DeviceKeys::generate().map_err(err)?;
    let req = api::register_device("desktop", &self.cfg.label, &keys.sign_pub_b64(), &keys.kex_pub_b64(), None).map_err(err)?;
    let reg = api::parse_registered(&self.t.call(&req, None)?).map_err(err)?;
    let me = Dev {
      id: reg.device_id.clone(),
      kind: "desktop".into(),
      label: self.cfg.label.clone(),
      fp: keys.fingerprint(),
      sign_pub: keys.sign_pub_b64(),
      kex_pub: keys.kex_pub_b64(),
    };
    let identity = Identity { device_id: reg.device_id, token: reg.device_token, sign_secret: b64(keys.sign_secret()), kex_secret: b64(keys.kex_secret()) };
    self.k.save(Slot::Identity, &serde_json::to_string(&identity).map_err(err)?)?;
    c.identity = Some(identity);
    c.keys = Some(keys);
    c.st = State { base: vec![me.clone()], trusted: vec![me], joined: true, ..State::default() };
    c.log.clear();
    c.log_dirty = true;
    c.acked = 0;
    self.save_now(c)
  }

  pub fn register(&self) -> Result<(), String> {
    let mut g = self.lock();
    self.ensure_registered(&mut g)
  }

  fn fetch_dir(&self, c: &mut Core) -> Result<Directory, String> {
    let dir = api::parse_directory(&self.call(c, api::list_devices())?).map_err(err)?;
    c.dir = Some(dir.clone());
    Ok(dir)
  }

  fn sync_dir(&self, c: &mut Core) -> Result<(), String> {
    let dir = self.fetch_dir(c)?;
    if !c.st.joined {
      return Ok(());
    }
    recompute(c, Some(&dir), self.cfg.user_id, false);
    let mut budget = BTreeMap::new();
    for r in &dir.revocations {
      ingest_revoke(c, self.cfg.user_id, &r.statement, Some(&r.target_fp), &mut budget);
    }
    Ok(())
  }

  fn seal_and_post(&self, c: &mut Core, kind: &str, msg_id: &str, payload: &Value, targets: &[Dev], push: bool) -> Result<(), String> {
    let (id, keys) = Self::own(c)?;
    let now = self.now();
    let mut items: Vec<(String, String, u64)> = Vec::new();
    for t in targets {
      check_device_id(&t.id).map_err(|_| "invalid recipient device id".to_owned())?;
      let seq = c.st.seq_out.get(&t.id).copied().unwrap_or(0) + 1;
      let body = seal_message(
        keys,
        &SealParams {
          user_id: self.cfg.user_id,
          from_device_id: &id.device_id,
          to: Peer { device_id: &t.id, sign_pub_b64: &t.sign_pub, kex_pub_b64: &t.kex_pub },
          kind,
          msg_id,
          sent_at_ms: now,
          seq,
          payload: payload.clone(),
        },
      )
      .map_err(err)?;
      items.push((t.id.clone(), body, seq));
    }
    for chunk in items.chunks(api::MAX_ITEMS) {
      let refs: Vec<(&str, &str)> = chunk.iter().map(|(to, body, _)| (to.as_str(), body.as_str())).collect();
      let req = api::post_messages(msg_id, push, &refs).map_err(err)?;
      api::parse_posted(&self.call(c, req).map_err(post_error)?).map_err(err)?;
      for (to, _, seq) in chunk {
        c.st.seq_out.insert(to.clone(), *seq);
      }
    }
    Ok(())
  }

  fn publish_intros(&self, c: &mut Core) -> Result<(), String> {
    let (id, keys) = Self::own(c)?;
    let now = self.now();
    let mut reqs = Vec::new();
    for t in c.st.trusted.iter().filter(|t| t.id != id.device_id) {
      let st = IntroStatement {
        user_id: self.cfg.user_id,
        introducer_fp: keys.fingerprint(),
        target_id: t.id.clone(),
        target_fp: t.fp.clone(),
        target_kind: t.kind.clone(),
        issued_at_ms: now,
      };
      let json = sign_intro(keys, &id.device_id, &st).map_err(err)?;
      reqs.push(api::put_intro(&t.id, &json).map_err(err)?);
    }
    for r in reqs {
      self.call(c, r)?;
    }
    Ok(())
  }

  fn finish_join(&self, c: &mut Core) -> Result<(), String> {
    self.sync_dir(c)?;
    self.publish_intros(c)
  }

  pub fn open_join_code(&self) -> Result<String, String> {
    let mut g = self.lock();
    let c = &mut *g;
    self.ensure_registered(c)?;
    let (id, keys) = Self::own(c)?;
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(err)?;
    let exp_ms = self.now() + JOIN_TTL_MS;
    let code = JoinCode { user_id: self.cfg.user_id, device_id: id.device_id.clone(), fp: keys.fingerprint(), secret, exp_ms };
    let text = encode_join_code(&code).map_err(err)?;
    c.st.join_code = Some(OpenCode { secret: b64(&secret), exp_ms });
    self.save_now(c)?;
    Ok(text)
  }

  pub fn join_with_code(&self, code: &str) -> Result<(), String> {
    let mut g = self.lock();
    let c = &mut *g;
    let code = decode_join_code(code.trim()).map_err(err)?;
    if code.user_id != self.cfg.user_id || code.is_expired(self.now()) {
      return Err("the join code is not valid".into());
    }
    self.ensure_registered(c)?;
    if c.st.trusted.len() > 1 {
      return Err("this device already has trusted devices".into());
    }
    let dir = self.fetch_dir(c)?;
    let p = dir
      .devices
      .iter()
      .find(|d| d.device_id == code.device_id && fingerprint(&d.sign_pub, &d.kex_pub) == code.fp)
      .ok_or_else(|| "the device of the join code was not found".to_owned())?;
    let pdev = dev_from(p);
    let (id, keys) = Self::own(c)?;
    let mac = join_mac(
      &code.secret,
      &JoinMacInput { user_id: self.cfg.user_id, p_fp: &pdev.fp, n_id: &id.device_id, n_fp: &keys.fingerprint(), n_kind: "desktop", label: &self.cfg.label },
    )
    .map_err(err)?;
    c.st.base.push(pdev.clone());
    c.st.trusted.push(pdev.clone());
    c.st.joined = false;
    let payload = json!({ "label": self.cfg.label, "mac": mac });
    let msg_id = random_msg_id("join").map_err(err)?;
    let res = self.seal_and_post(c, "join", &msg_id, &payload, std::slice::from_ref(&pdev), true);
    if res.is_err() {
      c.st.base.retain(|d| d.id != pdev.id);
      c.st.trusted.retain(|d| d.id != pdev.id);
      c.st.joined = true;
    }
    self.save_now(c)?;
    res
  }

  fn open_one(&self, c: &Core, m: &Msg, now: u64) -> Option<Opened> {
    if c.st.seen.iter().any(|s| s.from == m.from && s.id == m.msg_id) {
      return None;
    }
    let kind_hint = serde_json::from_str::<Value>(&m.body).ok().and_then(|v| v.get("kind")?.as_str().map(str::to_owned));
    let join_open = kind_hint.as_deref() == Some("join") && c.st.join_code.as_ref().is_some_and(|j| now < j.exp_ms);
    let dir = c.dir.as_ref()?;
    let (id, keys) = Self::own(c).ok()?;
    let trusted: Vec<Trusted> = c.st.trusted.iter().map(to_trusted).collect();
    open_message(keys, self.cfg.user_id, &id.device_id, &m.from, &m.msg_id, &m.body, now, &trusted, |from| {
      join_open
        .then(|| dir.devices.iter().find(|d| d.device_id == from))
        .flatten()
        .map(|d| Peer { device_id: &d.device_id, sign_pub_b64: &d.sign_pub, kex_pub_b64: &d.kex_pub })
    })
    .ok()
  }

  fn apply(&self, c: &mut Core, opened: Opened, now: u64) -> Option<ChatMessage> {
    let stored = self.handle(c, &opened, now);
    if stored.is_some() {
      c.st.seen.push(Seen { from: opened.from, id: opened.id, at: opened.sent_at });
      c.st.seen.retain(|s| s.at + SEEN_TTL_MS >= now);
      if c.st.seen.len() > MAX_SEEN {
        let cut = c.st.seen.len() - MAX_SEEN;
        c.st.seen.drain(..cut);
      }
    }
    stored.flatten()
  }

  fn handle(&self, c: &mut Core, opened: &Opened, now: u64) -> Option<Option<ChatMessage>> {
    let join_open = opened.kind == "join" && c.st.join_code.as_ref().is_some_and(|j| now < j.exp_ms);
    let dir = c.dir.clone()?;
    let (_, keys) = Self::own(c).ok()?;
    let own_fp = keys.fingerprint();
    let sender_trusted = c.st.trusted.iter().any(|d| d.id == opened.from);
    let mut stored = None;
    match opened.kind.as_str() {
      "chat" if sender_trusted && c.st.joined => {
        let msg: ChatMessage = serde_json::from_value(opened.payload.clone()).ok()?;
        if validate_chat(&msg).is_err() || msg.id != opened.id || !self.owns(&msg.room) {
          return None;
        }
        let sender_kind = c.st.trusted.iter().find(|d| d.id == opened.from).map(|d| d.kind.as_str());
        if matches!(msg.author.kind.as_str(), "manager" | "session" | "system") && sender_kind != Some("runner") {
          return None;
        }
        if !c.log.iter().any(|x| x.from == opened.from && x.msg.id == msg.id) {
          c.log.push(LogEntry { from: opened.from.clone(), msg: msg.clone() });
          c.log_dirty = true;
          stored = Some(msg);
        }
      }
      "join" if !sender_trusted && join_open => {
        let code = c.st.join_code.clone()?;
        let secret: [u8; 32] = b64_decode(&code.secret).ok()?.try_into().ok()?;
        let label = opened.payload.get("label")?.as_str()?.to_owned();
        let mac = opened.payload.get("mac")?.as_str()?;
        let entry = dir.devices.iter().find(|d| d.device_id == opened.from)?;
        if entry.label != label || c.st.revoked.contains(&opened.from_fp) {
          return None;
        }
        let input = JoinMacInput { user_id: self.cfg.user_id, p_fp: &own_fp, n_id: &opened.from, n_fp: &opened.from_fp, n_kind: &entry.kind, label: &label };
        verify_join_mac(&secret, &input, mac).ok()?;
        c.st.join_code = None;
        c.st.pending_joins.retain(|j| j.device_id != opened.from);
        c.st.pending_joins.push(JoinRec { device_id: opened.from.clone(), label, fp: opened.from_fp.clone(), kind: entry.kind.clone(), requested_at_ms: now });
      }
      "join_ok" if sender_trusted && !c.st.joined && c.st.base.iter().any(|d| d.id == opened.from) => {
        for fp in opened.payload.get("revoked").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
          if check_fp(fp).is_ok() && !c.st.revoked.iter().any(|r| r == fp) {
            c.st.revoked.push(fp.to_owned());
          }
        }
        let revoked = c.st.revoked.clone();
        c.st.trusted.retain(|d| !revoked.contains(&d.fp));
        c.st.joined = true;
        c.joined_now = true;
      }
      "revoke" if sender_trusted && c.st.joined => {
        let statement = opened.payload.get("statement")?.as_str()?;
        let sig = opened.payload.get("sig")?.as_str()?;
        let json = json!({ "v": 1, "statement": statement, "sig": sig }).to_string();
        ingest_revoke(c, self.cfg.user_id, &json, None, &mut BTreeMap::new());
      }
      _ => return None,
    }
    Some(stored)
  }

  /// Per sender, messages are applied in `seq` order; a missing predecessor holds the rest for HOLD_MS, then a gap is recorded.
  fn process_page(&self, c: &mut Core, msgs: &[Msg], now: u64, hold: &mut Hold, out: &mut Vec<ChatMessage>) -> Result<(), String> {
    let mut opened: Vec<(u64, Opened)> = msgs.iter().filter_map(|m| Some((m.cursor as u64, self.open_one(c, m, now)?))).collect();
    opened.sort_by(|a, b| (&a.1.from, a.1.seq, a.0).cmp(&(&b.1.from, b.1.seq, b.0)));
    for (cursor, o) in opened {
      let from = o.from.clone();
      let mut defer = hold.blocked.contains(&from);
      if !defer && o.trusted {
        if let Some(expected) = c.st.seq_in.get(&from).map(|l| l + 1).filter(|e| o.seq > *e) {
          let since = *c.hold_since.entry(from.clone()).or_insert(now);
          if now.saturating_sub(since) < HOLD_MS && hold.held < MAX_HELD {
            defer = true;
          } else {
            c.st.seq_gaps.push(SeqGap { from_device: from.clone(), expected, got: o.seq });
            if c.st.seq_gaps.len() > MAX_GAPS {
              c.st.seq_gaps.remove(0);
            }
          }
        }
      }
      if defer {
        hold.blocked.insert(from);
        hold.held += 1;
        hold.stuck = Some(hold.stuck.map_or(cursor, |s| s.min(cursor)));
        continue;
      }
      let entry = c.st.seq_in.entry(from).or_insert(0);
      *entry = (*entry).max(o.seq);
      out.extend(self.apply(c, o, now));
      if std::mem::take(&mut c.joined_now) {
        self.finish_join(c)?;
      }
    }
    Ok(())
  }
}

impl<T: Transport, K: KeyStore> RelayPort for RelayEngine<T, K> {
  fn rooms(&self) -> Result<Vec<RoomInfo>, String> {
    let g = self.lock();
    let mut rooms: Vec<RoomInfo> = Vec::new();
    for m in g.log.iter().map(|e| &e.msg) {
      if rooms.iter().any(|r| r.room == m.room) {
        continue;
      }
      let last = g.log.iter().map(|e| &e.msg).filter(|x| x.room == m.room).max_by_key(|x| (x.ts_ms, x.id.clone())).cloned();
      let title = match (m.room.kind, m.room.session.as_deref()) {
        (RoomKind::Session, Some(s)) => s.to_owned(),
        _ => "Manager".to_owned(),
      };
      rooms.push(RoomInfo { room: m.room.clone(), title, agent: None, state: None, context_pct: None, context_tokens: None, last });
    }
    rooms.sort_by(|a, b| b.last.as_ref().map(|m| m.ts_ms).cmp(&a.last.as_ref().map(|m| m.ts_ms)).then(a.title.cmp(&b.title)));
    Ok(rooms)
  }

  fn history(&self, params: &HistoryParams) -> Result<Vec<ChatMessage>, String> {
    if !self.owns(&params.room) {
      return Err("unknown room".into());
    }
    let limit = params.limit.unwrap_or(50).min(MAX_HISTORY) as usize;
    let g = self.lock();
    let mut all: Vec<ChatMessage> = g.log.iter().map(|e| &e.msg).filter(|m| m.room == params.room).cloned().collect();
    all.sort_by(|a, b| (a.ts_ms, &a.id).cmp(&(b.ts_ms, &b.id)));
    if let Some(before) = params.before.as_deref() {
      let cut = all.iter().position(|m| m.id == before).unwrap_or(all.len());
      all.truncate(cut);
    }
    let skip = all.len().saturating_sub(limit);
    Ok(all.split_off(skip))
  }

  fn search(&self, params: &SearchParams) -> Result<Vec<ChatMessage>, String> {
    let needle = params.query.trim().to_lowercase();
    if needle.is_empty() || params.query.len() > MAX_QUERY {
      return Ok(vec![]);
    }
    let limit = params.limit.unwrap_or(50).min(MAX_HISTORY) as usize;
    let g = self.lock();
    let mut hits: Vec<ChatMessage> = g
      .log
      .iter()
      .map(|e| &e.msg)
      .filter(|m| params.room.as_ref().is_none_or(|r| r == &m.room))
      .filter(|m| m.body_md.to_lowercase().contains(&needle))
      .cloned()
      .collect();
    hits.sort_by_key(|m| std::cmp::Reverse(m.ts_ms));
    hits.truncate(limit);
    Ok(hits)
  }

  fn send(&self, params: SendParams) -> Result<ChatMessage, String> {
    if params.body_md.trim().is_empty() {
      return Err("empty message".into());
    }
    if params.body_md.len() > MAX_BODY {
      return Err("body_md is too large".into());
    }
    if !params.attachments.is_empty() {
      return Err("attachments are not supported over chat sync yet".into());
    }
    if !self.owns(&params.room) {
      return Err("unknown room".into());
    }
    let mut g = self.lock();
    let c = &mut *g;
    self.ensure_registered(c)?;
    let own_id = c.identity.as_ref().map(|i| i.device_id.clone()).unwrap_or_default();
    if let Some(stored) = params.id.as_deref().and_then(|i| c.log.iter().find(|e| e.msg.id == i && (e.from == own_id || e.from.is_empty())).map(|e| &e.msg)) {
      let same = stored.room == params.room && stored.body_md == params.body_md && stored.reply_to == params.reply_to;
      return if same { Ok(stored.clone()) } else { Err("conflict".into()) };
    }
    let (id, _) = Self::own(c)?;
    if !c.st.joined || !c.st.trusted.iter().any(|d| d.id == id.device_id) {
      return Err("this device is not a trusted member of the chat yet".into());
    }
    let targets: Vec<Dev> = c.st.trusted.iter().filter(|d| d.id != id.device_id).cloned().collect();
    if targets.is_empty() {
      return Err("there are no other trusted devices to send to".into());
    }
    let msg_id = match params.id {
      Some(i) => i,
      None => {
        let mut rnd = [0u8; 16];
        getrandom::fill(&mut rnd).map_err(err)?;
        format!("m_{}", hex(&rnd))
      }
    };
    let now = self.now();
    let msg = ChatMessage {
      v: 1,
      id: msg_id,
      room: params.room,
      author: crate::manager::backend::Author { kind: "owner".into(), id: "owner".into(), machine: None, agent: None },
      ts_ms: params.ts_ms.unwrap_or(now),
      kind: "text".into(),
      body_md: params.body_md,
      attachments: vec![],
      reply_to: params.reply_to,
      decision: None,
    };
    validate_chat(&msg)?;
    let payload = serde_json::to_value(&msg).map_err(err)?;
    self.seal_and_post(c, "chat", &msg.id, &payload, &targets, true)?;
    c.st.seen.push(Seen { from: own_id.clone(), id: msg.id.clone(), at: now });
    if c.st.seen.len() > MAX_SEEN {
      c.st.seen.remove(0);
    }
    c.log.push(LogEntry { from: own_id, msg: msg.clone() });
    c.log_dirty = true;
    self.save_now(c)?;
    Ok(msg)
  }

  fn owns(&self, room: &RoomRef) -> bool {
    !self.cfg.local_machine.is_empty() && room.machine != self.cfg.local_machine
  }

  fn trust_state(&self) -> Result<TrustState, String> {
    let g = self.lock();
    let own_id = g.identity.as_ref().map(|i| i.device_id.as_str());
    let dev = |d: &Dev, state| TrustDevice { id: d.id.clone(), kind: d.kind.clone(), label: d.label.clone(), fp: d.fp.clone(), state, is_self: Some(d.id.as_str()) == own_id };
    let mut devices: Vec<TrustDevice> = g.st.trusted.iter().map(|d| dev(d, DeviceState::Trusted)).collect();
    devices.extend(g.st.revoked_devs.iter().map(|d| dev(d, DeviceState::Revoked)));
    Ok(TrustState {
      registered: g.identity.is_some(),
      self_fp: g.keys.as_ref().map(|k| k.fingerprint()),
      devices,
      pending_joins: g
        .st
        .pending_joins
        .iter()
        .map(|j| PendingJoin { device_id: j.device_id.clone(), label: j.label.clone(), fp: j.fp.clone(), kind: Some(j.kind.clone()), requested_at_ms: j.requested_at_ms })
        .collect(),
      pending_revokes: g.st.pending_revokes.clone(),
      key_mismatch: g.st.mismatch.clone(),
      seq_gaps: g.st.seq_gaps.clone(),
    })
  }

  fn decide_join(&self, device_id: &str, accept: bool) -> Result<(), String> {
    let mut g = self.lock();
    let c = &mut *g;
    let rec = c.st.pending_joins.iter().find(|j| j.device_id == device_id).cloned().ok_or_else(|| "no such join request".to_owned())?;
    if !accept {
      c.st.pending_joins.retain(|j| j.device_id != device_id);
      return self.save_now(c);
    }
    if c.st.revoked.contains(&rec.fp) {
      c.st.pending_joins.retain(|j| j.device_id != device_id);
      self.save_now(c)?;
      return Err("this device was revoked".into());
    }
    let dir = self.fetch_dir(c)?;
    let entry = dir
      .devices
      .iter()
      .find(|d| d.device_id == rec.device_id && fingerprint(&d.sign_pub, &d.kex_pub) == rec.fp)
      .ok_or_else(|| "the device keys changed, ask it to join again".to_owned())?;
    let dev = dev_from(entry);
    let (id, keys) = Self::own(c)?;
    let st = IntroStatement {
      user_id: self.cfg.user_id,
      introducer_fp: keys.fingerprint(),
      target_id: dev.id.clone(),
      target_fp: dev.fp.clone(),
      target_kind: dev.kind.clone(),
      issued_at_ms: self.now(),
    };
    let intro = sign_intro(keys, &id.device_id, &st).map_err(err)?;
    self.call(c, api::put_intro(&dev.id, &intro).map_err(err)?)?;
    c.st.intros.push(intro);
    if !c.st.trusted.iter().any(|d| d.id == dev.id) {
      c.st.trusted.push(dev.clone());
    }
    self.save_now(c)?;
    let msg_id = random_msg_id("join_ok").map_err(err)?;
    let payload = json!({ "revoked": c.st.revoked });
    self.seal_and_post(c, "join_ok", &msg_id, &payload, &[dev], false)?;
    c.st.pending_joins.retain(|j| j.device_id != device_id);
    self.save_now(c)
  }

  fn decide_revoke(&self, statement_id: &str, accept: bool) -> Result<(), String> {
    let mut g = self.lock();
    let c = &mut *g;
    let p = c.st.pending_revokes.iter().find(|p| p.statement_id == statement_id).cloned().ok_or_else(|| "no such revoke request".to_owned())?;
    if accept {
      self.fetch_dir(c)?;
      apply_revoke(c, self.cfg.user_id, &p.target_id, &p.target_fp, &p.target_label);
    } else {
      c.st.pending_revokes.retain(|x| x.statement_id != statement_id);
      c.st.declined.push(p.statement_id);
    }
    self.save_now(c)
  }

  fn revoke(&self, device_id: &str) -> Result<(), String> {
    let mut g = self.lock();
    let c = &mut *g;
    self.ensure_registered(c)?;
    let (id, keys) = Self::own(c)?;
    let target = c.st.trusted.iter().find(|d| d.id == device_id && d.id != id.device_id).cloned().ok_or_else(|| "unknown device".to_owned())?;
    let st = RevokeStatement { user_id: self.cfg.user_id, revoker_fp: keys.fingerprint(), target_id: target.id.clone(), target_fp: target.fp.clone(), issued_at_ms: self.now() };
    let json = sign_revoke(keys, &st).map_err(err)?;
    let parsed = parse_revoke(&json).map_err(err)?;
    let others: Vec<Dev> = c.st.trusted.iter().filter(|d| d.id != id.device_id && d.id != target.id).cloned().collect();
    self.call(c, api::post_revocation(&target.fp, &json).map_err(err)?)?;
    apply_revoke(c, self.cfg.user_id, &target.id, &target.fp, &target.label);
    self.save_now(c)?;
    if !others.is_empty() {
      let msg_id = random_msg_id("revoke").map_err(err)?;
      let payload = json!({ "statement": parsed.statement, "sig": parsed.sig });
      self.seal_and_post(c, "revoke", &msg_id, &payload, &others, false)?;
      self.save_now(c)?;
    }
    self.call(c, api::delete_device(&target.id).map_err(err)?)?;
    Ok(())
  }

  fn reset_identity(&self) -> Result<(), String> {
    let mut g = self.lock();
    let c = &mut *g;
    let notice = self.announce_reset(c);
    if let Some(id) = c.identity.as_ref().map(|i| i.device_id.clone()) {
      if let Ok(req) = api::delete_device(&id) {
        let _ = self.call(c, req);
      }
    }
    self.k.clear()?;
    *c = Core::default();
    self.ensure_registered(c)?;
    notice.map_err(|e| format!("the old identity was wiped, but the other devices were not told: {e}"))
  }

  fn poll_once(&self) -> Result<Vec<ChatMessage>, String> {
    let mut g = self.lock();
    let c = &mut *g;
    self.ensure_registered(c)?;
    self.sync_dir(c)?;
    let now = self.now();
    let mut out = Vec::new();
    let mut hold = Hold::default();
    let mut after = c.st.cursor;
    loop {
      let inbox = api::parse_inbox(&self.call(c, api::get_messages(after, 100).map_err(err)?)?).map_err(err)?;
      let Some(last) = inbox.messages.iter().map(|m| m.cursor as u64).max() else { break };
      self.process_page(c, &inbox.messages, now, &mut hold, &mut out)?;
      after = last;
      c.st.cursor = c.st.cursor.max(hold.stuck.map_or(last, |s| s.saturating_sub(1)));
      self.ack_if_stored(c)?;
      if !inbox.more {
        break;
      }
    }
    c.hold_since.retain(|k, _| hold.blocked.contains(k));
    self.ack_if_stored(c)?;
    Ok(out)
  }
}

impl<T: Transport, K: KeyStore> RelayEngine<T, K> {
  fn ack_if_stored(&self, c: &mut Core) -> Result<(), String> {
    self.persist(c, false)?;
    if !c.log_dirty && c.st.cursor > c.acked {
      self.call(c, api::ack_messages(c.st.cursor).map_err(err)?)?;
      c.acked = c.st.cursor;
    }
    Ok(())
  }

  fn announce_reset(&self, c: &mut Core) -> Result<(), String> {
    let (Some(id), Some(keys)) = (&c.identity, &c.keys) else { return Ok(()) };
    let others: Vec<Dev> = c.st.trusted.iter().filter(|d| d.id != id.device_id).cloned().collect();
    if !c.st.joined || others.is_empty() {
      return Ok(());
    }
    let fp = keys.fingerprint();
    let st = RevokeStatement { user_id: self.cfg.user_id, revoker_fp: fp.clone(), target_id: id.device_id.clone(), target_fp: fp.clone(), issued_at_ms: self.now() };
    let json = sign_revoke(keys, &st).map_err(err)?;
    let parsed = parse_revoke(&json).map_err(err)?;
    if let Ok(req) = api::post_revocation(&fp, &json) {
      let _ = self.call(c, req);
    }
    let msg_id = random_msg_id("revoke").map_err(err)?;
    let payload = json!({ "statement": parsed.statement, "sig": parsed.sig });
    self.seal_and_post(c, "revoke", &msg_id, &payload, &others, false)
  }
}
