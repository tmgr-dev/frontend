//! Relay engine: chat rooms of other machines over the zero-knowledge relay (spec TM-1017 sections 1-4a).
//! One mutex guards all state, so there is a single writer; transport calls happen under it.

use std::collections::BTreeMap;
use std::fmt::Display;
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tmgr_chat_relay::api::{self, Directory, Msg, Request};
use tmgr_chat_relay::fields::check_fp;
use tmgr_chat_relay::keys::{fingerprint, DeviceKeys};
use tmgr_chat_relay::trust::{
  decode_join_code, encode_join_code, join_mac, parse_intro, parse_revoke, sign_intro, sign_revoke, trusted_set, verify_join_mac,
  verify_revoke, DirEntry, IntroStatement, JoinCode, JoinMacInput, RevokeStatement,
};
use tmgr_chat_relay::wire::{open_message, random_msg_id, seal_message, Peer, SealParams};
use tmgr_phone::{b64, b64_decode, h, hex};

use super::port::{DeviceState, PendingJoin, PendingRevoke, RelayPort, TrustDevice, TrustState};
use super::ports::{KeyStore, Slot, Transport};
use crate::manager::backend::{
  ChatMessage, HistoryParams, RoomInfo, RoomKind, RoomRef, SearchParams, SendParams, MAX_ATTACHMENTS, MAX_BODY, MAX_HISTORY, MAX_QUERY,
};

const JOIN_TTL_MS: u64 = 10 * 60 * 1000;
const SEEN_TTL_MS: u64 = 31 * 24 * 3600 * 1000;
const MAX_LOG: usize = 2000;
const MAX_INTROS: usize = 200;

#[cfg(test)]
mod tests;

fn err<E: Display>(e: E) -> String {
  e.to_string()
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
}

#[derive(Default)]
struct Core {
  identity: Option<Identity>,
  keys: Option<DeviceKeys>,
  st: State,
  log: Vec<ChatMessage>,
  log_dirty: bool,
  dir: Option<Directory>,
  joined_now: bool,
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

fn recompute(c: &mut Core, dir: &Directory, uid: u64, from_base: bool) {
  let mut intros: Vec<String> = c.st.intros.clone();
  for i in dir.devices.iter().flat_map(|d| d.intros.iter()) {
    if !intros.contains(i) {
      intros.push(i.clone());
    }
  }
  intros.retain(|i| intro_matches(i, uid));
  let mut pins: Vec<(String, String)> = c.st.base.iter().map(|d| (d.id.clone(), d.fp.clone())).collect();
  if !from_base {
    pins.extend(c.st.trusted.iter().map(|d| (d.id.clone(), d.fp.clone())));
  }
  let entries: Vec<DirEntry> = dir
    .devices
    .iter()
    .map(|d| DirEntry { device_id: d.device_id.clone(), sign_pub_b64: d.sign_pub.clone(), kex_pub_b64: d.kex_pub.clone() })
    .collect();
  let set = trusted_set(&pins, &c.st.revoked, &intros, &entries);
  let mut out: Vec<Dev> = Vec::new();
  for (id, fp) in set {
    let in_dir = dir.devices.iter().find(|d| d.device_id == id && fingerprint(&d.sign_pub, &d.kex_pub) == fp);
    let known = c.st.trusted.iter().chain(c.st.base.iter()).find(|d| d.id == id && d.fp == fp).cloned();
    if let Some(mut dev) = known.or_else(|| in_dir.map(dev_from)) {
      if let Some(d) = in_dir {
        dev.label = d.label.clone();
        dev.kind = d.kind.clone();
      }
      out.push(dev);
    }
  }
  c.st.mismatch = out
    .iter()
    .filter(|t| dir.devices.iter().any(|d| d.device_id == t.id && fingerprint(&d.sign_pub, &d.kex_pub) != t.fp))
    .map(|t| t.id.clone())
    .collect();
  for id in &c.st.mismatch {
    log::warn!("[chat-relay] directory keys of device {id} differ from the pinned ones");
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
  let dir = c.dir.clone().unwrap_or(Directory { devices: vec![], revocations: vec![] });
  recompute(c, &dir, uid, true);
}

fn ingest_revoke(c: &mut Core, uid: u64, json: &str, expect_fp: Option<&str>) {
  let Ok(r) = parse_revoke(json) else { return };
  let p = &r.parsed;
  if p.user_id != uid || expect_fp.is_some_and(|f| f != p.target_fp) {
    return;
  }
  let Some(revoker) = c.st.trusted.iter().find(|d| d.fp == p.revoker_fp).cloned() else { return };
  if verify_revoke(json, &revoker.sign_pub).is_err() || c.st.revoked.contains(&p.target_fp) {
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

  fn persist(&self, c: &mut Core) -> Result<(), String> {
    self.k.save(Slot::State, &serde_json::to_string(&c.st).map_err(err)?)?;
    if c.log_dirty {
      if c.log.len() > MAX_LOG {
        let cut = c.log.len() - MAX_LOG;
        c.log.drain(..cut);
      }
      self.k.save(Slot::Log, &serde_json::to_string(&c.log).map_err(err)?)?;
      c.log_dirty = false;
    }
    Ok(())
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
    self.persist(c)
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
    recompute(c, &dir, self.cfg.user_id, false);
    for r in &dir.revocations {
      ingest_revoke(c, self.cfg.user_id, &r.statement, Some(&r.target_fp));
    }
    Ok(())
  }

  fn seal_and_post(&self, c: &mut Core, kind: &str, msg_id: &str, payload: &Value, targets: &[Dev], push: bool) -> Result<(), String> {
    let (id, keys) = Self::own(c)?;
    let now = self.now();
    let mut items: Vec<(String, String, u64)> = Vec::new();
    for t in targets {
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
      api::parse_posted(&self.call(c, req)?).map_err(err)?;
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
    self.persist(c)?;
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
    let res = self.seal_and_post(c, "join", &msg_id, &payload, &[pdev], true);
    self.persist(c)?;
    res
  }

  fn process(&self, c: &mut Core, m: &Msg, now: u64) -> Option<ChatMessage> {
    if c.st.seen.iter().any(|s| s.from == m.from && s.id == m.msg_id) {
      return None;
    }
    let kind_hint = serde_json::from_str::<Value>(&m.body).ok().and_then(|v| v.get("kind")?.as_str().map(str::to_owned));
    let join_open = kind_hint.as_deref() == Some("join") && c.st.join_code.as_ref().is_some_and(|j| now < j.exp_ms);
    let dir = c.dir.clone()?;
    let (id, keys) = Self::own(c).ok()?;
    let opened = {
      let trusted = &c.st.trusted;
      let dir = &dir;
      open_message(keys, self.cfg.user_id, &id.device_id, &m.from, &m.msg_id, &m.body, now, |from| {
        trusted
          .iter()
          .find(|d| d.id == from)
          .map(|d| Peer { device_id: &d.id, sign_pub_b64: &d.sign_pub, kex_pub_b64: &d.kex_pub })
          .or_else(|| {
            join_open
              .then(|| dir.devices.iter().find(|d| d.device_id == from))
              .flatten()
              .map(|d| Peer { device_id: &d.device_id, sign_pub_b64: &d.sign_pub, kex_pub_b64: &d.kex_pub })
          })
      })
    }
    .ok()?;
    let sender_trusted = c.st.trusted.iter().any(|d| d.id == opened.from);
    let mut stored = None;
    match opened.kind.as_str() {
      "chat" if sender_trusted => {
        let msg: ChatMessage = serde_json::from_value(opened.payload.clone()).ok()?;
        if validate_chat(&msg).is_err() || msg.id != opened.id || !self.owns(&msg.room) {
          return None;
        }
        if !c.log.iter().any(|x| x.id == msg.id) {
          c.log.push(msg.clone());
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
        if entry.label != label {
          return None;
        }
        let input = JoinMacInput { user_id: self.cfg.user_id, p_fp: &keys.fingerprint(), n_id: &opened.from, n_fp: &opened.from_fp, n_kind: &entry.kind, label: &label };
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
        c.st.joined = true;
        c.joined_now = true;
      }
      "revoke" if sender_trusted => {
        let statement = opened.payload.get("statement")?.as_str()?;
        let sig = opened.payload.get("sig")?.as_str()?;
        let json = json!({ "v": 1, "statement": statement, "sig": sig }).to_string();
        ingest_revoke(c, self.cfg.user_id, &json, None);
      }
      _ => return None,
    }
    c.st.seen.push(Seen { from: opened.from, id: opened.id, at: opened.sent_at });
    c.st.seen.retain(|s| s.at + SEEN_TTL_MS >= now);
    stored
  }
}

impl<T: Transport, K: KeyStore> RelayPort for RelayEngine<T, K> {
  fn rooms(&self) -> Result<Vec<RoomInfo>, String> {
    let g = self.lock();
    let mut rooms: Vec<RoomInfo> = Vec::new();
    for m in &g.log {
      if rooms.iter().any(|r| r.room == m.room) {
        continue;
      }
      let last = g.log.iter().filter(|x| x.room == m.room).max_by_key(|x| (x.ts_ms, x.id.clone())).cloned();
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
    let mut all: Vec<ChatMessage> = g.log.iter().filter(|m| m.room == params.room).cloned().collect();
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
    let (id, _) = Self::own(c)?;
    if !c.st.joined || !c.st.trusted.iter().any(|d| d.id == id.device_id) {
      return Err("this device is not a trusted member of the chat yet".into());
    }
    let targets: Vec<Dev> = c.st.trusted.iter().filter(|d| d.id != id.device_id).cloned().collect();
    if targets.is_empty() {
      return Err("there are no other trusted devices to send to".into());
    }
    let mut rnd = [0u8; 16];
    getrandom::fill(&mut rnd).map_err(err)?;
    let now = self.now();
    let msg = ChatMessage {
      v: 1,
      id: format!("m_{}", hex(&rnd)),
      room: params.room,
      author: crate::manager::backend::Author { kind: "owner".into(), id: "owner".into(), machine: None, agent: None },
      ts_ms: now,
      kind: "text".into(),
      body_md: params.body_md,
      attachments: vec![],
      reply_to: params.reply_to,
      decision: None,
    };
    validate_chat(&msg)?;
    let payload = serde_json::to_value(&msg).map_err(err)?;
    self.seal_and_post(c, "chat", &msg.id, &payload, &targets, true)?;
    c.st.seen.push(Seen { from: c.identity.as_ref().map(|i| i.device_id.clone()).unwrap_or_default(), id: msg.id.clone(), at: now });
    c.log.push(msg.clone());
    c.log_dirty = true;
    self.persist(c)?;
    Ok(msg)
  }

  fn owns(&self, room: &RoomRef) -> bool {
    room.machine != self.cfg.local_machine
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
        .map(|j| PendingJoin { device_id: j.device_id.clone(), label: j.label.clone(), fp: j.fp.clone(), requested_at_ms: j.requested_at_ms })
        .collect(),
      pending_revokes: g.st.pending_revokes.clone(),
      key_mismatch: g.st.mismatch.clone(),
    })
  }

  fn decide_join(&self, device_id: &str, accept: bool) -> Result<(), String> {
    let mut g = self.lock();
    let c = &mut *g;
    let pos = c.st.pending_joins.iter().position(|j| j.device_id == device_id).ok_or_else(|| "no such join request".to_owned())?;
    let rec = c.st.pending_joins.remove(pos);
    if !accept {
      return self.persist(c);
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
    c.st.trusted.push(dev.clone());
    self.persist(c)?;
    let msg_id = random_msg_id("join_ok").map_err(err)?;
    let payload = json!({ "revoked": c.st.revoked });
    self.seal_and_post(c, "join_ok", &msg_id, &payload, &[dev], false)?;
    self.persist(c)
  }

  fn decide_revoke(&self, statement_id: &str, accept: bool) -> Result<(), String> {
    let mut g = self.lock();
    let c = &mut *g;
    let pos = c.st.pending_revokes.iter().position(|p| p.statement_id == statement_id).ok_or_else(|| "no such revoke request".to_owned())?;
    let p = c.st.pending_revokes.remove(pos);
    if accept {
      self.fetch_dir(c)?;
      apply_revoke(c, self.cfg.user_id, &p.target_id, &p.target_fp, &p.target_label);
    } else {
      c.st.declined.push(p.statement_id);
    }
    self.persist(c)
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
    self.persist(c)?;
    if !others.is_empty() {
      let msg_id = random_msg_id("revoke").map_err(err)?;
      let payload = json!({ "statement": parsed.statement, "sig": parsed.sig });
      self.seal_and_post(c, "revoke", &msg_id, &payload, &others, false)?;
      self.persist(c)?;
    }
    self.call(c, api::delete_device(&target.id).map_err(err)?)?;
    Ok(())
  }

  fn reset_identity(&self) -> Result<(), String> {
    let mut g = self.lock();
    let c = &mut *g;
    if let Some(id) = c.identity.as_ref().map(|i| i.device_id.clone()) {
      if let Ok(req) = api::delete_device(&id) {
        let _ = self.call(c, req);
      }
    }
    self.k.clear()?;
    *c = Core::default();
    self.ensure_registered(c)
  }

  fn poll_once(&self) -> Result<Vec<ChatMessage>, String> {
    let mut g = self.lock();
    let c = &mut *g;
    self.ensure_registered(c)?;
    self.sync_dir(c)?;
    let now = self.now();
    let mut out = Vec::new();
    loop {
      let inbox = api::parse_inbox(&self.call(c, api::get_messages(c.st.cursor, 100).map_err(err)?)?).map_err(err)?;
      let Some(last) = inbox.messages.iter().map(|m| m.cursor as u64).max() else { break };
      for m in &inbox.messages {
        out.extend(self.process(c, m, now));
        if std::mem::take(&mut c.joined_now) {
          self.finish_join(c)?;
        }
      }
      c.st.cursor = c.st.cursor.max(last);
      self.persist(c)?;
      self.call(c, api::ack_messages(last).map_err(err)?)?;
      if !inbox.more {
        break;
      }
    }
    self.persist(c)?;
    Ok(out)
  }
}
