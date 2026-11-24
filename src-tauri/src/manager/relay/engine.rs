//! Relay engine: chat rooms of other machines over the zero-knowledge relay (spec TM-1017 sections 1-4a).
//! One mutex guards all state, so there is a single writer; transport calls happen under it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tmgr_chat_relay::api::{self, Directory, Msg, Request};
use tmgr_chat_relay::fields::{check_device_id, check_device_kind, check_fp};
use tmgr_chat_relay::keys::{fingerprint, DeviceKeys};
use tmgr_chat_relay::trust::{
  decode_join_code, encode_join_code, expected_join_matches, gate_blockers, join_mac, kind_key, lowers_kind, may_revoke, parse_intro, parse_revoke, restrict_to_before, sign_intro, sign_revoke,
  trusted_set_by_kind, v2_self_markers, verify_join_mac, verify_revoke, DirEntry, IntroStatement, JoinCode, JoinMacInput, KindCtx, RevokeStatement, Trusted,
};
use tmgr_chat_relay::wire::{open_message_checked, random_msg_id, seal_message, Opened, Peer, SealParams};
use tmgr_phone::{b64, b64_decode, h, hex, Error};

use super::port::{DeviceState, PendingJoin, PendingRevoke, Receipt, ReceiptState, RelayPort, SeqGap, TrustAlert, TrustDevice, TrustState};
use super::ports::{KeyStore, Slot, Transport};
use super::rooms::{valid_index_msg_id, MachineIndex, RoomsIndex, StoredRooms};
use crate::manager::backend::{
  ChatMessage, HistoryParams, RoomInfo, RoomKind, RoomRef, SearchParams, SendParams, MAX_ATTACHMENTS, MAX_BODY, MAX_HISTORY, MAX_QUERY,
};

const WAITING_MSG: &str = "this device is waiting for another device to approve it";
const JOIN_TTL_MS: u64 = 10 * 60 * 1000;
const SEEN_TTL_MS: u64 = 31 * 24 * 3600 * 1000;
const MAX_LOG: usize = 2000;
const MAX_SEEN: usize = 5000;
const MAX_GAPS: usize = 100;
const MAX_HELD: usize = 200;
const HOLD_MS: u64 = 15_000;
const MAX_RECEIPT_REASON: usize = 200;
const WRITE_EVERY_MS: u64 = 30_000;
const MAX_INTROS: usize = 200;
const MAX_REVOKES_PER_DEVICE: usize = 50;
const MAX_ALERTS: usize = 50;
const ALERT_TTL_MS: u64 = 7 * 24 * 3600 * 1000;
const AUTO_REVOKE_SILENCE_MS: u64 = 30 * 24 * 3600 * 1000;
const KINDS_V: u32 = 3;
pub const OLD_CODE_MSG: &str = "This join code is from an older app version. Update the desktop or mobile app that showed it and generate a new code.";
const BUSY_PREFIX: &str = "relay is busy, retry in ";
const SEND_BUDGET: Duration = Duration::from_secs(15);
const RETRY_AFTER_CAP_SECS: u64 = 2;
pub const SEND_TIMEOUT_MSG: &str = "The relay did not answer in time; the message may not have been sent. Retry is safe.";

#[cfg(test)]
mod tests;

fn err<E: Display>(e: E) -> String {
  e.to_string()
}

const RECIPIENT_FULL_MSG: &str = "the recipient's relay queue is full";

/// Transport errors are the response body of a failed call; for a 429 with Retry-After the transport adds `"retry_after": <secs>`.
fn post_error(e: String) -> String {
  let v: Value = serde_json::from_str(&e).unwrap_or(Value::Null);
  match (v["error"].as_str(), v["retry_after"].as_u64()) {
    (Some("user_full"), _) => "relay storage is full".into(),
    (Some("recipient_full"), _) => RECIPIENT_FULL_MSG.into(),
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
struct ExpectedJoin {
  device_id: String,
  fp: String,
  exp_ms: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct JoinRec {
  device_id: String,
  label: String,
  fp: String,
  kind: String,
  requested_at_ms: u64,
  #[serde(default = "join_v1")]
  v: u8,
}

fn join_v1() -> u8 {
  1
}

fn trim_seen(st: &mut State) {
  if st.seen.len() > MAX_SEEN {
    let cut = st.seen.len() - MAX_SEEN;
    for s in st.seen.drain(..cut) {
      let f = st.seen_floor.entry(s.from).or_insert(0);
      *f = (*f).max(s.at);
    }
  }
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
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  seen_floor: BTreeMap<String, u64>,
  mismatch: Vec<String>,
  #[serde(default)]
  seq_in: BTreeMap<String, u64>,
  #[serde(default)]
  seq_gaps: Vec<SeqGap>,
  #[serde(default)]
  last_seen: BTreeMap<String, u64>,
  #[serde(default)]
  pinned_at: BTreeMap<String, u64>,
  #[serde(default)]
  alerts: Vec<TrustAlert>,
  #[serde(default)]
  kinds: BTreeMap<String, String>,
  #[serde(default)]
  kinds_v: u32,
  #[serde(default)]
  legacy_unknown: Vec<String>,
  #[serde(default)]
  self_intro: bool,
  #[serde(default)]
  republish_intros: bool,
  #[serde(default)]
  bindings: BTreeMap<String, String>,
  #[serde(default)]
  expected: Option<ExpectedJoin>,
  #[serde(default, skip_serializing)]
  rooms: BTreeMap<String, StoredRooms>,
}

impl State {
  fn kind_of(&self, id: &str, fp: &str) -> Option<&str> {
    self.kinds.get(&kind_key(id, fp)).map(String::as_str)
  }

  fn kind_of_trusted(&self, id: &str) -> Option<&str> {
    let d = self.trusted.iter().find(|d| d.id == id)?;
    self.kind_of(&d.id, &d.fp)
  }

  fn drop_untrusted_bindings(&mut self) {
    let trusted: BTreeSet<String> = self.trusted.iter().map(|d| d.id.clone()).collect();
    self.bindings.retain(|_, d| trusted.contains(d));
  }

  fn pinned_for_other_fp(&self, id: &str, fp: &str) -> bool {
    let prefix = kind_key(id, "");
    let own = kind_key(id, fp);
    self.kinds.keys().any(|k| k.starts_with(&prefix) && *k != own)
  }
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
  drops: Vec<(u64, &'static str, String)>,
}

enum Inbound {
  Opened(Opened),
  Consumed { from: String, seq: u64 },
  Repeated { from: String, seq: u64 },
}

impl Inbound {
  fn key(&self) -> (&str, u64) {
    match self {
      Inbound::Opened(o) => (&o.from, o.seq),
      Inbound::Consumed { from, seq } | Inbound::Repeated { from, seq } => (from, *seq),
    }
  }
}

#[derive(Default)]
struct Core {
  identity: Option<Identity>,
  keys: Option<DeviceKeys>,
  st: State,
  log: Vec<LogEntry>,
  committed_gen: u64,
  seq_reserved: BTreeMap<String, BTreeSet<u64>>,
  seq_holes: BTreeMap<String, BTreeSet<u64>>,
  sending: BTreeMap<String, String>,
  dir: Option<Directory>,
  dir_gen: u64,
  joined_now: bool,
  written_state: String,
  written_trust: String,
  last_write: u64,
  written_cursor: u64,
  acked: u64,
  hold_since: BTreeMap<String, u64>,
  pass_cap: Option<usize>,
  auto_join_error: Option<String>,
  drop_reason: Option<&'static str>,
  drop_detail: Option<String>,
  receipts: Vec<Receipt>,
  held_receipts: Vec<HeldReceipt>,
}

struct HeldReceipt {
  machine: String,
  from: String,
  at_ms: u64,
  receipt: Receipt,
}

const HELD_RECEIPT_TTL_MS: u64 = 120_000;
const MAX_HELD_RECEIPTS: usize = 64;

struct Sealed {
  items: Vec<(String, String, u64)>,
  kinds: BTreeMap<String, String>,
}

struct Posted {
  done: Vec<(String, u64)>,
  outcome: Result<(), String>,
}

struct Snapshot {
  gen: u64,
  log: Option<(u64, String)>,
  state: Option<String>,
  cursor: u64,
  trust: String,
  at: u64,
}

fn drop_line(reason: &str, detail: &str) -> String {
  format!("[manager] relay dropped a message ({reason}) {detail}").trim_end().to_owned()
}

fn short_id(id: &str) -> String {
  id.chars().take(8).collect()
}

fn trust_key(st: &State) -> String {
  json!([st.base, st.trusted, st.revoked, st.pending_joins, st.pending_revokes, st.declined, st.seq_gaps, st.alerts, st.joined, st.kinds, st.bindings, st.legacy_unknown]).to_string()
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

fn valid_chat_id(id: &str) -> bool {
  id.len() == 34 && id.starts_with("m_") && id[2..].bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

fn validate_chat(m: &ChatMessage) -> Result<(), String> {
  let id_ok = valid_chat_id(&m.id);
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

fn recompute(c: &mut Core, dir: Option<&Directory>, uid: u64, now: u64, from_base: bool) {
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
  let assumed: BTreeSet<String> =
    if from_base { c.st.base.iter().map(|d| kind_key(&d.id, &d.fp)).filter(|k| !c.st.kinds.contains_key(k)).collect() } else { BTreeSet::new() };
  let own = own_key(c);
  let mut set = trusted_set_by_kind(
    uid,
    &pins,
    &c.st.revoked,
    &intros,
    &entries,
    Some(&KindCtx { known: &c.st.kinds, own: own.as_deref(), assumed: &assumed, max_passes: c.pass_cap }),
  );
  if from_base {
    let before: BTreeSet<String> = c.st.trusted.iter().map(|d| kind_key(&d.id, &d.fp)).collect();
    set = restrict_to_before(set, &before, &c.st.kinds);
  }
  for (id, fp, kind) in &set.kinds {
    c.st.kinds.insert(kind_key(id, fp), kind.clone());
  }
  for (id, _) in &set.conflicts {
    record_alert_once(c, "kind_conflict", id, now);
  }
  for id in &set.unstable {
    record_alert_once(c, "trust_unstable", id, now);
  }
  let mut out: Vec<Dev> = Vec::new();
  for t in &set.trusted {
    let in_dir = dir_devices.iter().find(|d| d.device_id == t.device_id && fingerprint(&d.sign_pub, &d.kex_pub) == t.fp);
    let known = c.st.trusted.iter().chain(c.st.base.iter()).find(|d| d.id == t.device_id && d.fp == t.fp).cloned();
    if let Some(mut dev) = known.or_else(|| in_dir.map(dev_from)) {
      if let Some(d) = in_dir {
        dev.label = d.label.clone();
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
  let own_id = c.identity.as_ref().map(|i| i.device_id.clone());
  let unknown: Vec<String> = c
    .st
    .trusted
    .iter()
    .filter(|d| Some(&d.id) != own_id.as_ref() && !c.st.base.iter().any(|b| b.id == d.id) && c.st.kind_of(&d.id, &d.fp).is_none())
    .map(|d| d.id.clone())
    .collect();
  for id in unknown {
    record_alert_once(c, "kind_unknown", &id, now);
  }
}

fn own_key(c: &Core) -> Option<String> {
  let (id, keys) = (c.identity.as_ref()?, c.keys.as_ref()?);
  Some(kind_key(&id.device_id, &keys.fingerprint()))
}

fn all_intros(c: &Core) -> Vec<String> {
  let mut intros = c.st.intros.clone();
  for i in c.dir.iter().flat_map(|d| d.devices.iter()).flat_map(|d| d.intros.iter()) {
    if !intros.contains(i) {
      intros.push(i.clone());
    }
  }
  intros
}

fn gate_blocked(c: &Core, uid: u64) -> Vec<Dev> {
  let trusted: Vec<Trusted> = c.st.trusted.iter().map(to_trusted).collect();
  let markers = v2_self_markers(uid, &trusted, &c.st.kinds, &all_intros(c));
  let own = own_key(c);
  let ids = gate_blockers(&trusted, &c.st.kinds, own.as_deref(), &markers);
  c.st.trusted.iter().filter(|d| ids.contains(&d.id)).cloned().collect()
}

fn runner_trusted(c: &Core) -> bool {
  c.st.trusted.iter().any(|d| c.st.kind_of(&d.id, &d.fp) == Some("runner"))
}

fn pin_kind(c: &mut Core, id: &str, fp: &str, kind: &str, now: u64) -> bool {
  if check_device_kind(kind).is_err() {
    return false;
  }
  match c.st.kind_of(id, fp) {
    None => {
      c.st.kinds.insert(kind_key(id, fp), kind.to_owned());
      true
    }
    Some(k) if k == kind => true,
    Some(k) => {
      let lowered = lowers_kind(k, kind);
      if lowered {
        c.st.kinds.insert(kind_key(id, fp), kind.to_owned());
      }
      record_alert_once(c, "kind_conflict", id, now);
      lowered
    }
  }
}

fn backfill_self(c: &mut Core, kind: &str) {
  if let (Some(id), Some(keys)) = (&c.identity, &c.keys) {
    c.st.kinds.insert(kind_key(&id.device_id, &keys.fingerprint()), kind.to_owned());
  }
}

fn migrate_kinds(c: &mut Core, own_kind: &str) {
  if c.st.kinds_v >= KINDS_V {
    return;
  }
  let old = std::mem::take(&mut c.st.kinds);
  c.st.kinds_v = KINDS_V;
  c.st.republish_intros = true;
  backfill_self(c, own_kind);
  let own = c.identity.as_ref().map(|i| i.device_id.clone());
  let devs: Vec<Dev> = c.st.trusted.iter().chain(c.st.base.iter()).cloned().collect();
  for d in devs.iter().filter(|d| Some(&d.id) != own.as_ref()) {
    let legacy = old.get(&d.id).or_else(|| old.get(&kind_key(&d.id, &d.fp))).filter(|k| matches!(k.as_str(), "desktop" | "mobile"));
    match legacy {
      Some(k) => {
        c.st.kinds.insert(kind_key(&d.id, &d.fp), k.clone());
      }
      None => {
        let key = kind_key(&d.id, &d.fp);
        if !c.st.legacy_unknown.contains(&key) {
          c.st.legacy_unknown.push(key);
        }
      }
    }
  }
}

fn unknown_roots(c: &Core, except_fp: Option<&str>) -> Vec<String> {
  let own = c.identity.as_ref().map(|i| i.device_id.as_str());
  c.st
    .base
    .iter()
    .filter(|d| {
      Some(d.id.as_str()) != own && Some(d.fp.as_str()) != except_fp && !c.st.revoked.contains(&d.fp) && c.st.kind_of(&d.id, &d.fp).is_none()
    })
    .map(|d| d.id.clone())
    .collect()
}

fn stamp_pins(c: &mut Core, now: u64) {
  for d in &c.st.trusted {
    c.st.pinned_at.entry(d.id.clone()).or_insert(now);
  }
}

fn record_alert(c: &mut Core, kind: &str, device_id: &str, now: u64) {
  c.st.alerts.retain(|a| now.saturating_sub(a.at_ms) < ALERT_TTL_MS && !(a.kind == kind && a.device_id == device_id));
  c.st.alerts.push(TrustAlert { kind: kind.to_owned(), device_id: device_id.to_owned(), at_ms: now });
  if c.st.alerts.len() > MAX_ALERTS {
    c.st.alerts.remove(0);
  }
}

fn record_alert_once(c: &mut Core, kind: &str, device_id: &str, now: u64) {
  c.st.alerts.retain(|a| now.saturating_sub(a.at_ms) < ALERT_TTL_MS);
  if c.st.alerts.iter().any(|a| a.kind == kind && a.device_id == device_id) {
    return;
  }
  c.st.alerts.push(TrustAlert { kind: kind.to_owned(), device_id: device_id.to_owned(), at_ms: now });
  if c.st.alerts.len() > MAX_ALERTS {
    c.st.alerts.remove(0);
  }
}

fn parse_seen(v: &Value) -> Option<u64> {
  match v {
    Value::Number(n) => n.as_u64().map(|t| if t < 100_000_000_000 { t * 1000 } else { t }),
    Value::String(s) => chrono::DateTime::parse_from_rfc3339(s).ok().and_then(|d| u64::try_from(d.timestamp_millis()).ok()),
    _ => None,
  }
}

fn unseen_for_30_days(c: &Core, device_id: &str, now: u64) -> bool {
  let dir_seen = c.dir.as_ref().and_then(|d| d.devices.iter().find(|x| x.device_id == device_id)).and_then(|d| parse_seen(&d.last_seen_at));
  let since = [c.st.last_seen.get(device_id).copied(), c.st.pinned_at.get(device_id).copied(), dir_seen].into_iter().flatten().max();
  since.is_some_and(|t| now.saturating_sub(t) > AUTO_REVOKE_SILENCE_MS)
}

fn apply_revoke(c: &mut Core, uid: u64, now: u64, target_id: &str, target_fp: &str, label: &str) {
  if !c.st.revoked.iter().any(|f| f == target_fp) {
    c.st.revoked.push(target_fp.to_owned());
  }
  let kind = c
    .st
    .kind_of(target_id, target_fp)
    .map(str::to_owned)
    .or_else(|| c.st.trusted.iter().find(|d| d.fp == target_fp).map(|d| d.kind.clone()))
    .unwrap_or_default();
  c.st.kinds.remove(&kind_key(target_id, target_fp));
  c.st.revoked_devs.push(Dev { id: target_id.to_owned(), kind, label: label.to_owned(), fp: target_fp.to_owned(), sign_pub: String::new(), kex_pub: String::new() });
  c.st.pending_revokes.retain(|p| p.target_fp != target_fp);
  c.st.pending_joins.retain(|j| j.fp != target_fp);
  for id in unknown_roots(c, Some(target_fp)) {
    record_alert_once(c, "root_kind_unknown", &id, now);
  }
  let before: Vec<Dev> = c.st.trusted.clone();
  let dir = c.dir.clone();
  recompute(c, dir.as_ref(), uid, now, true);
  for d in before {
    if !c.st.trusted.iter().any(|t| t.id == d.id && t.fp == d.fp) {
      c.st.kinds.remove(&kind_key(&d.id, &d.fp));
    }
  }
}

fn revoker_kind<'a>(c: &'a Core, revoker: &Dev) -> Option<&'a str> {
  match c.st.kind_of(&revoker.id, &revoker.fp) {
    None if c.st.base.iter().any(|d| d.id == revoker.id && d.fp == revoker.fp) => Some("desktop"),
    kind => kind,
  }
}

fn ingest_revoke(c: &mut Core, uid: u64, now: u64, json: &str, expect_fp: Option<&str>, budget: &mut BTreeMap<String, usize>) {
  let Ok(r) = parse_revoke(json) else { return };
  let p = &r.parsed;
  if p.user_id != uid || expect_fp.is_some_and(|f| f != p.target_fp) {
    return;
  }
  let Some(revoker) = c.st.trusted.iter().find(|d| d.fp == p.revoker_fp).cloned() else {
    return;
  };
  if verify_revoke(json, uid, &revoker.sign_pub, &revoker.kex_pub).is_err() {
    return;
  }
  if !may_revoke(revoker_kind(c, &revoker), &p.revoker_fp, &p.target_fp) {
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
  let aims_at_us = own_fp.as_deref() == Some(p.target_fp.as_str());
  if p.revoker_fp == p.target_fp || own_fp.as_deref() == Some(p.revoker_fp.as_str()) || (!aims_at_us && unseen_for_30_days(c, &p.target_id, now)) {
    apply_revoke(c, uid, now, &p.target_id, &p.target_fp, &target_label);
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
  kind: String,
  now: Box<dyn Fn() -> u64 + Send + Sync>,
  core: Mutex<Core>,
  writer: Mutex<u64>,
  log_rev: AtomicU64,
  stored_log_rev: AtomicU64,
  next_gen: AtomicU64,
  send_budget: Duration,
  #[cfg(test)]
  drops: Mutex<Vec<&'static str>>,
  #[cfg(test)]
  drop_lines: Mutex<Vec<String>>,
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
      core.written_cursor = core.st.cursor;
      core.acked = core.st.cursor;
    }
    let now = Box::new(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64));
    Ok(Self {
      t,
      k,
      cfg,
      kind: "desktop".into(),
      now,
      core: Mutex::new(core),
      writer: Mutex::new(0),
      log_rev: AtomicU64::new(0),
      stored_log_rev: AtomicU64::new(0),
      next_gen: AtomicU64::new(0),
      send_budget: SEND_BUDGET,
      #[cfg(test)]
      drops: Mutex::default(),
      #[cfg(test)]
      drop_lines: Mutex::default(),
    })
  }

  pub fn with_kind(mut self, kind: &str) -> Self {
    self.kind = kind.to_owned();
    let c = self.core.get_mut().unwrap_or_else(|p| p.into_inner());
    backfill_self(c, kind);
    self
  }

  #[cfg(test)]
  fn with_send_budget(mut self, budget: Duration) -> Self {
    self.send_budget = budget;
    self
  }

  pub fn with_clock(mut self, now: impl Fn() -> u64 + Send + Sync + 'static) -> Self {
    self.now = Box::new(now);
    self
  }

  fn lock(&self) -> MutexGuard<'_, Core> {
    let mut g = self.core.lock().unwrap_or_else(|p| p.into_inner());
    if g.identity.is_some() && g.st.kinds_v < KINDS_V {
      migrate_kinds(&mut g, &self.kind);
    }
    g
  }

  fn now(&self) -> u64 {
    (self.now)()
  }

  fn report_drop(&self, reason: &'static str, detail: &str) {
    let line = drop_line(reason, detail);
    log::warn!("{line}");
    #[cfg(test)]
    {
      self.drops.lock().unwrap_or_else(|p| p.into_inner()).push(reason);
      self.drop_lines.lock().unwrap_or_else(|p| p.into_inner()).push(line);
    }
  }

  #[cfg(test)]
  fn drop_lines(&self) -> Vec<String> {
    self.drop_lines.lock().unwrap_or_else(|p| p.into_inner()).clone()
  }

  #[cfg(test)]
  fn drops(&self) -> Vec<&'static str> {
    self.drops.lock().unwrap_or_else(|p| p.into_inner()).clone()
  }

  fn call(&self, c: &Core, req: Request) -> Result<String, String> {
    self.t.call(&req, c.identity.as_ref().map(|i| i.token.as_str()))
  }

  fn mark_log(&self) {
    self.log_rev.fetch_add(1, Ordering::SeqCst);
  }

  fn log_unstored(&self) -> bool {
    self.log_rev.load(Ordering::SeqCst) > self.stored_log_rev.load(Ordering::SeqCst)
  }

  fn writer(&self) -> MutexGuard<'_, u64> {
    self.writer.lock().unwrap_or_else(|p| p.into_inner())
  }

  fn save_now(&self, c: &mut Core) -> Result<(), String> {
    if let Some(snap) = self.plan(c, true)? {
      if self.write(&snap)? {
        Self::commit(c, snap);
      }
    }
    Ok(())
  }

  /// Under the core lock: decides what to write and serializes it. Poll-time writes are throttled unless trust changed.
  fn plan(&self, c: &mut Core, force: bool) -> Result<Option<Snapshot>, String> {
    let st = serde_json::to_string(&c.st).map_err(err)?;
    let state_changed = st != c.written_state;
    let log_rev = self.log_rev.load(Ordering::SeqCst);
    let log_dirty = log_rev > self.stored_log_rev.load(Ordering::SeqCst);
    if !state_changed && !log_dirty {
      return Ok(None);
    }
    let now = self.now();
    let trust = trust_key(&c.st);
    if !force && trust == c.written_trust && now.abs_diff(c.last_write) < WRITE_EVERY_MS {
      return Ok(None);
    }
    let log = if log_dirty {
      if c.log.len() > MAX_LOG {
        let cut = c.log.len() - MAX_LOG;
        c.log.drain(..cut);
      }
      Some((log_rev, serde_json::to_string(&c.log).map_err(err)?))
    } else {
      None
    };
    let gen = self.next_gen.fetch_add(1, Ordering::SeqCst) + 1;
    Ok(Some(Snapshot { gen, log, state: state_changed.then_some(st), cursor: c.st.cursor, trust, at: now }))
  }

  /// Without the core lock: one writer at a time, and a snapshot older than one already written is dropped (the newer one carries everything).
  fn write(&self, snap: &Snapshot) -> Result<bool, String> {
    let mut last = self.writer();
    if snap.gen < *last {
      return Ok(false);
    }
    if let Some((rev, json)) = &snap.log {
      self.k.save(Slot::Log, json)?;
      self.stored_log_rev.fetch_max(*rev, Ordering::SeqCst);
    }
    if let Some(st) = &snap.state {
      self.k.save(Slot::State, st)?;
    }
    *last = snap.gen;
    Ok(true)
  }

  fn commit(c: &mut Core, snap: Snapshot) {
    if snap.gen <= c.committed_gen {
      return;
    }
    c.committed_gen = snap.gen;
    if let Some(st) = snap.state {
      c.written_state = st;
      c.written_cursor = snap.cursor;
    }
    c.written_trust = snap.trust;
    c.last_write = snap.at;
  }

  /// A message is acked only once the log holding it and the cursor past it are stored, so the keychain write runs with the core lock released.
  fn persist_out<'a>(&'a self, g: MutexGuard<'a, Core>, force: bool) -> Result<MutexGuard<'a, Core>, String> {
    let mut g = g;
    let Some(snap) = self.plan(&mut g, force)? else {
      return Ok(g);
    };
    drop(g);
    let written = self.write(&snap)?;
    let mut g = self.lock();
    if written {
      Self::commit(&mut g, snap);
    }
    Ok(g)
  }

  fn own(c: &Core) -> Result<(&Identity, &DeviceKeys), String> {
    match (&c.identity, &c.keys) {
      (Some(i), Some(k)) => Ok((i, k)),
      _ => Err("chat sync is not set up".into()),
    }
  }

  fn ensure_registered(&self, c: &mut Core) -> Result<(), String> {
    backfill_self(c, &self.kind);
    if c.identity.is_some() {
      return Ok(());
    }
    let keys = DeviceKeys::generate().map_err(err)?;
    let req = api::register_device(&self.kind, &self.cfg.label, &keys.sign_pub_b64(), &keys.kex_pub_b64(), None).map_err(err)?;
    let reg = api::parse_registered(&self.t.call(&req, None)?).map_err(err)?;
    let me = Dev {
      id: reg.device_id.clone(),
      kind: self.kind.clone(),
      label: self.cfg.label.clone(),
      fp: keys.fingerprint(),
      sign_pub: keys.sign_pub_b64(),
      kex_pub: keys.kex_pub_b64(),
    };
    let identity = Identity { device_id: reg.device_id, token: reg.device_token, sign_secret: b64(keys.sign_secret()), kex_secret: b64(keys.kex_secret()) };
    self.k.save(Slot::Identity, &serde_json::to_string(&identity).map_err(err)?)?;
    c.identity = Some(identity);
    c.keys = Some(keys);
    c.st = State { base: vec![me.clone()], trusted: vec![me.clone()], joined: true, ..State::default() };
    c.st.kinds_v = KINDS_V;
    c.st.kinds.insert(kind_key(&me.id, &me.fp), self.kind.clone());
    c.log.clear();
    self.mark_log();
    c.acked = 0;
    self.save_now(c)
  }

  pub fn register(&self) -> Result<(), String> {
    let mut g = self.lock();
    self.ensure_registered(&mut g)
  }

  fn fetch_dir(&self, c: &mut Core) -> Result<Directory, String> {
    let raw = self.call(c, api::list_devices())?;
    Self::take_dir(c, &raw)
  }

  fn take_dir(c: &mut Core, raw: &str) -> Result<Directory, String> {
    let dir = api::parse_directory(raw).map_err(err)?;
    c.dir = Some(dir.clone());
    c.dir_gen += 1;
    Ok(dir)
  }

  fn sync_dir(&self, c: &mut Core) -> Result<(), String> {
    let dir = self.fetch_dir(c)?;
    self.sync_with(c, dir)
  }

  fn sync_with(&self, c: &mut Core, dir: Directory) -> Result<(), String> {
    if !c.st.joined {
      return Ok(());
    }
    backfill_self(c, &self.kind);
    let now = self.now();
    recompute(c, Some(&dir), self.cfg.user_id, now, false);
    for id in unknown_roots(c, None) {
      record_alert_once(c, "root_kind_unknown", &id, now);
    }
    stamp_pins(c, now);
    let mut budget = BTreeMap::new();
    for r in &dir.revocations {
      ingest_revoke(c, self.cfg.user_id, now, &r.statement, Some(&r.target_fp), &mut budget);
    }
    if (c.st.republish_intros || !c.st.self_intro) && self.publish_intros(c).is_ok() {
      c.st.republish_intros = false;
      c.st.self_intro = true;
    }
    Ok(())
  }

  fn seq_top(c: &Core, to: &str) -> u64 {
    let reserved = c.seq_reserved.get(to).and_then(|s| s.last().copied()).unwrap_or(0);
    c.st.seq_out.get(to).copied().unwrap_or(0).max(reserved)
  }

  /// A seq left behind by a failed send below a later one is reused first, so the recipient does not hold later messages or report a gap.
  fn reserve_seq(c: &mut Core, to: &str) -> u64 {
    let hole = c.seq_holes.get_mut(to).and_then(BTreeSet::pop_first);
    if c.seq_holes.get(to).is_some_and(BTreeSet::is_empty) {
      c.seq_holes.remove(to);
    }
    let seq = hole.unwrap_or_else(|| Self::seq_top(c, to) + 1);
    c.seq_reserved.entry(to.to_owned()).or_default().insert(seq);
    seq
  }

  fn release_seq(c: &mut Core, to: &str, seq: u64, delivered: bool) {
    if let Some(set) = c.seq_reserved.get_mut(to) {
      set.remove(&seq);
      if set.is_empty() {
        c.seq_reserved.remove(to);
      }
    }
    if !delivered && seq < Self::seq_top(c, to) {
      c.seq_holes.entry(to.to_owned()).or_default().insert(seq);
    }
  }

  /// Under the core lock: seals one body per target and reserves its seq so a concurrent send cannot reuse it.
  fn seal_items(&self, c: &mut Core, kind: &str, msg_id: &str, payload: &Value, targets: &[Dev]) -> Result<Sealed, String> {
    Self::own(c)?;
    let now = self.now();
    let mut items: Vec<(String, String, u64)> = Vec::new();
    for t in targets {
      if check_device_id(&t.id).is_err() {
        Self::settle(c, &items, &[]);
        return Err("invalid recipient device id".into());
      }
      let seq = Self::reserve_seq(c, &t.id);
      let (id, keys) = Self::own(c)?;
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
      .map_err(err);
      match body {
        Ok(body) => items.push((t.id.clone(), body, seq)),
        Err(e) => {
          Self::settle(c, &items, &[]);
          Self::release_seq(c, &t.id, seq, false);
          return Err(e);
        }
      }
    }
    let kinds = targets.iter().map(|t| (t.id.clone(), c.st.kind_of(&t.id, &t.fp).unwrap_or(&t.kind).to_owned())).collect();
    Ok(Sealed { items, kinds })
  }

  /// Under the core lock again: delivered seqs become `seq_out`, each reservation is released, and a failed seq below a later one becomes a hole.
  fn settle(c: &mut Core, items: &[(String, String, u64)], done: &[(String, u64)]) {
    for (to, seq) in done {
      let out = c.st.seq_out.entry(to.clone()).or_insert(0);
      *out = (*out).max(*seq);
    }
    for (to, _, seq) in items {
      let delivered = done.iter().any(|(d, s)| d == to && s == seq);
      Self::release_seq(c, to, *seq, delivered);
    }
  }

  fn post_phase(&self, token: Option<&str>, msg_id: &str, push: bool, sealed: &Sealed, deadline: Option<Instant>) -> Posted {
    let mut done: Vec<(String, u64)> = Vec::new();
    let mut delivered = false;
    let mut skipped = false;
    for chunk in sealed.items.chunks(api::MAX_ITEMS) {
      match self.post_items(token, msg_id, push, chunk, deadline) {
        Ok(()) => {
          done.extend(chunk.iter().map(|(to, _, seq)| (to.clone(), *seq)));
          delivered = true;
          continue;
        }
        Err(e) if e == RECIPIENT_FULL_MSG && chunk.len() > 1 => {}
        Err(e) => return Posted { done, outcome: Err(e) },
      }
      for item in chunk {
        match self.post_items(token, msg_id, push, std::slice::from_ref(item), deadline) {
          Ok(()) => {
            done.push((item.0.clone(), item.2));
            delivered = true;
          }
          Err(e) if e == RECIPIENT_FULL_MSG => {
            let kind = sealed.kinds.get(&item.0).cloned().unwrap_or_default();
            if !matches!(kind.as_str(), "desktop" | "mobile" | "terminal") {
              return Posted { done, outcome: Err(e) };
            }
            log::warn!("[chat-relay] skipped {kind} {}: its relay queue is full", item.0.chars().take(8).collect::<String>());
            skipped = true;
          }
          Err(e) => return Posted { done, outcome: Err(e) },
        }
      }
    }
    let outcome = if skipped && !delivered { Err(RECIPIENT_FULL_MSG.into()) } else { Ok(()) };
    Posted { done, outcome }
  }

  fn seal_and_post(&self, c: &mut Core, kind: &str, msg_id: &str, payload: &Value, targets: &[Dev], push: bool) -> Result<(), String> {
    let sealed = self.seal_items(c, kind, msg_id, payload, targets)?;
    let token = c.identity.as_ref().map(|i| i.token.clone());
    let posted = self.post_phase(token.as_deref(), msg_id, push, &sealed, None);
    Self::settle(c, &sealed.items, &posted.done);
    posted.outcome
  }

  fn post_items(&self, token: Option<&str>, msg_id: &str, push: bool, items: &[(String, String, u64)], deadline: Option<Instant>) -> Result<(), String> {
    let refs: Vec<(&str, &str)> = items.iter().map(|(to, body, _)| (to.as_str(), body.as_str())).collect();
    let req = api::post_messages(msg_id, push, &refs).map_err(err)?;
    let mut retried = false;
    loop {
      let reply = match deadline {
        None => self.t.call(&req, token),
        Some(d) => {
          let left = d.saturating_duration_since(Instant::now());
          if left.is_zero() {
            return Err(SEND_TIMEOUT_MSG.into());
          }
          self.t.call_within(&req, token, left)
        }
      };
      match reply.map_err(post_error).and_then(|raw| api::parse_posted(&raw).map(drop).map_err(err)) {
        Ok(()) => return Ok(()),
        Err(e) => {
          if let (Some(d), Some(secs), false) = (deadline, retry_after_secs(&e), retried) {
            let wait = Duration::from_secs(secs);
            if secs <= RETRY_AFTER_CAP_SECS && Instant::now() + wait < d {
              retried = true;
              std::thread::sleep(wait);
              continue;
            }
          }
          return Err(if deadline.is_some_and(|d| Instant::now() >= d) { SEND_TIMEOUT_MSG.into() } else { e });
        }
      }
    }
  }

  fn intro_json(&self, c: &Core, version: u8, target: (&str, &str, &str)) -> Result<String, String> {
    let (id, keys) = Self::own(c)?;
    let st = IntroStatement {
      version,
      user_id: self.cfg.user_id,
      introducer_fp: keys.fingerprint(),
      target_id: target.0.to_owned(),
      target_fp: target.1.to_owned(),
      target_kind: target.2.to_owned(),
      issued_at_ms: self.now(),
    };
    sign_intro(keys, &id.device_id, &st).map_err(err)
  }

  fn publish_intros(&self, c: &mut Core) -> Result<(), String> {
    if self.kind == "runner" {
      return Ok(());
    }
    let (id, keys) = Self::own(c)?;
    let (own_id, own_fp) = (id.device_id.clone(), keys.fingerprint());
    let mut signed = Vec::new();
    for t in c.st.trusted.iter().filter(|t| t.id != own_id) {
      let Some(kind) = c.st.kind_of(&t.id, &t.fp) else {
        continue;
      };
      signed.push((t.id.clone(), self.intro_json(c, 2, (&t.id, &t.fp, kind))?));
    }
    signed.push((own_id.clone(), self.intro_json(c, 2, (&own_id, &own_fp, &self.kind))?));
    for (target, json) in signed {
      self.call(c, api::put_intro(&target, &json).map_err(err)?)?;
    }
    Ok(())
  }

  fn finish_join(&self, c: &mut Core) -> Result<(), String> {
    self.sync_dir(c)?;
    self.publish_intros(c)?;
    c.st.self_intro = true;
    Ok(())
  }

  pub fn open_join_code(&self) -> Result<String, String> {
    let mut g = self.lock();
    let c = &mut *g;
    self.ensure_registered(c)?;
    if !c.st.joined {
      return Err(WAITING_MSG.into());
    }
    let (id, keys) = Self::own(c)?;
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(err)?;
    let exp_ms = self.now() + JOIN_TTL_MS;
    let code = JoinCode { version: 2, user_id: self.cfg.user_id, device_id: id.device_id.clone(), fp: keys.fingerprint(), kind: Some(self.kind.clone()), secret, exp_ms };
    let text = encode_join_code(&code).map_err(err)?;
    c.st.join_code = Some(OpenCode { secret: b64(&secret), exp_ms });
    self.save_now(c)?;
    Ok(text)
  }

  pub fn close_join_code(&self, expires_at_ms: u64) -> Result<(), String> {
    let mut g = self.lock();
    let c = &mut *g;
    if c.st.join_code.as_ref().is_some_and(|j| j.exp_ms == expires_at_ms) {
      c.st.join_code = None;
      self.save_now(c)?;
    }
    Ok(())
  }

  pub fn join_with_code(&self, code: &str) -> Result<(), String> {
    let mut g = self.lock();
    let c = &mut *g;
    let code = decode_join_code(code.trim()).map_err(|_| "the join code is not valid".to_owned())?;
    if code.version < 2 && self.kind == "runner" {
      return Err(OLD_CODE_MSG.into());
    }
    if code.user_id != self.cfg.user_id || code.is_expired(self.now()) {
      return Err("the join code is not valid".into());
    }
    self.ensure_registered(c)?;
    if c.st.joined && c.st.trusted.len() > 1 {
      return Err("this device already has trusted devices".into());
    }
    let dir = self.fetch_dir(c)?;
    let p = dir
      .devices
      .iter()
      .find(|d| d.device_id == code.device_id && fingerprint(&d.sign_pub, &d.kex_pub) == code.fp)
      .ok_or_else(|| "the device of the join code was not found".to_owned())?;
    if code.kind.as_deref().is_some_and(|k| k != p.kind) {
      return Err("the join code does not match the device".into());
    }
    let pdev = dev_from(p);
    let (id, keys) = Self::own(c)?;
    let own_id = id.device_id.clone();
    if code.device_id == own_id {
      return Err("this is the code of this device, enter the code shown on the other device".into());
    }
    let mac = join_mac(
      &code.secret,
      &JoinMacInput { version: code.version, user_id: self.cfg.user_id, p_fp: &pdev.fp, n_id: &id.device_id, n_fp: &keys.fingerprint(), n_kind: &self.kind, label: &self.cfg.label },
    )
    .map_err(err)?;
    if c.st.pinned_for_other_fp(&pdev.id, &pdev.fp) {
      return Err("the device id was seen with other keys".into());
    }
    if !c.st.joined {
      c.st.base.retain(|d| d.id == own_id);
      c.st.trusted.retain(|d| d.id == own_id);
    }
    let key = kind_key(&pdev.id, &pdev.fp);
    let inserted = code.kind.is_some() && !c.st.kinds.contains_key(&key);
    if let Some(k) = &code.kind {
      pin_kind(c, &pdev.id, &pdev.fp, k, self.now());
    }
    c.st.base.push(pdev.clone());
    c.st.trusted.push(pdev.clone());
    c.st.joined = false;
    let payload = json!({ "label": self.cfg.label, "mac": mac });
    let msg_id = random_msg_id("join").map_err(err)?;
    let res = self.seal_and_post(c, "join", &msg_id, &payload, std::slice::from_ref(&pdev), true);
    if res.is_err() {
      if inserted {
        c.st.kinds.remove(&key);
      }
      c.st.base.retain(|d| d.id != pdev.id);
      c.st.trusted.retain(|d| d.id != pdev.id);
      c.st.joined = true;
    }
    self.save_now(c)?;
    res
  }

  fn open_one(&self, c: &Core, m: &Msg, now: u64) -> Result<Inbound, Option<&'static str>> {
    let kind_hint = serde_json::from_str::<Value>(&m.body).ok().and_then(|v| v.get("kind")?.as_str().map(str::to_owned));
    let join_open = kind_hint.as_deref() == Some("join");
    let Some(dir) = c.dir.as_ref() else {
      return Err(None);
    };
    let Ok((id, keys)) = Self::own(c) else {
      return Err(None);
    };
    let trusted: Vec<Trusted> = c.st.trusted.iter().map(to_trusted).collect();
    let opened = open_message_checked(keys, self.cfg.user_id, &id.device_id, &m.from, &m.msg_id, &m.body, now, &trusted, |from| {
      join_open
        .then(|| dir.devices.iter().find(|d| d.device_id == from))
        .flatten()
        .map(|d| Peer { device_id: &d.device_id, sign_pub_b64: &d.sign_pub, kex_pub_b64: &d.kex_pub })
    });
    match opened {
      Ok(o) if c.st.seen.iter().any(|s| s.from == o.from && s.id == o.id) => match o.trusted {
        true => Ok(Inbound::Repeated { from: o.from, seq: o.seq }),
        false => Err(None),
      },
      Ok(o) => Ok(Inbound::Opened(o)),
      Err(r) => match r.verified {
        Some((from, seq)) => Ok(Inbound::Consumed { from, seq }),
        None => Err(Some(match r.error {
          Error::BadKey if !join_open && !trusted.iter().any(|t| t.device_id == m.from) => "unknown sender",
          Error::BadKey | Error::IdMismatch | Error::BadSignature => "bad signature or key",
          _ => "undecryptable or malformed",
        })),
      },
    }
  }

  fn apply(&self, c: &mut Core, opened: Opened, now: u64) -> Option<ChatMessage> {
    if opened.trusted && c.st.seen_floor.get(&opened.from).is_some_and(|f| opened.sent_at <= *f) {
      c.drop_reason = Some("replayed or too old");
      return None;
    }
    let stored = self.handle(c, &opened, now);
    if stored.is_some() && opened.kind != "rooms" {
      c.st.seen.push(Seen { from: opened.from, id: opened.id, at: opened.sent_at });
      c.st.seen.retain(|s| s.at + SEEN_TTL_MS >= now);
      trim_seen(&mut c.st);
    }
    stored.flatten()
  }

  fn claim_machine(c: &mut Core, from: &str, machine: &str, now: u64) -> bool {
    c.st.drop_untrusted_bindings();
    let bound_elsewhere = c.st.bindings.iter().any(|(m, d)| d == from && m != machine);
    match c.st.bindings.get(machine) {
      Some(d) if d == from => true,
      Some(_) => {
        record_alert(c, "room_conflict", from, now);
        false
      }
      None if bound_elsewhere => {
        record_alert(c, "room_conflict", from, now);
        false
      }
      None => {
        c.st.bindings.insert(machine.to_owned(), from.to_owned());
        Self::release_held_receipts(c, from, machine, now);
        true
      }
    }
  }

  fn release_held_receipts(c: &mut Core, from: &str, machine: &str, now: u64) {
    let held = std::mem::take(&mut c.held_receipts);
    for x in held {
      if now.saturating_sub(x.at_ms) >= HELD_RECEIPT_TTL_MS {
        continue;
      }
      if x.from == from && x.machine == machine {
        c.receipts.push(x.receipt);
      } else {
        c.held_receipts.push(x);
      }
    }
  }

  fn authorized_author(c: &mut Core, from: &str, msg: &ChatMessage, now: u64) -> bool {
    let kind = c.st.kind_of_trusted(from).map(str::to_owned);
    match msg.author.kind.as_str() {
      "owner" => matches!(kind.as_deref(), Some("desktop" | "mobile" | "terminal")),
      "manager" | "session" | "system" => {
        if kind.as_deref() != Some("runner") || msg.author.machine.as_ref().is_some_and(|m| *m != msg.room.machine) {
          return false;
        }
        Self::claim_machine(c, from, &msg.room.machine, now)
      }
      _ => {
        record_alert(c, "unknown_author_kind", from, now);
        false
      }
    }
  }

  fn handle(&self, c: &mut Core, opened: &Opened, now: u64) -> Option<Option<ChatMessage>> {
    let join_open = c.st.join_code.as_ref().is_some_and(|j| now < j.exp_ms);
    let dir = c.dir.clone()?;
    let (_, keys) = Self::own(c).ok()?;
    let own_fp = keys.fingerprint();
    let sender_trusted = c.st.trusted.iter().any(|d| d.id == opened.from);
    let mut stored = None;
    match opened.kind.as_str() {
      "chat" if sender_trusted && c.st.joined => {
        let Ok(msg) = serde_json::from_value::<ChatMessage>(opened.payload.clone()) else {
          c.drop_reason = Some("malformed chat payload");
          return None;
        };
        if validate_chat(&msg).is_err() || msg.id != opened.id {
          c.drop_reason = Some("malformed chat payload");
          return None;
        }
        if !self.owns(&msg.room) {
          c.drop_reason = Some("room not owned by this machine");
          c.drop_detail = Some(format!("room_machine={}", msg.room.machine));
          return None;
        }
        if !Self::authorized_author(c, &opened.from, &msg, now) {
          c.drop_reason = Some("unauthorized author");
          return None;
        }
        if !c.log.iter().any(|x| x.from == opened.from && x.msg.id == msg.id) {
          c.log.push(LogEntry { from: opened.from.clone(), msg: msg.clone() });
          self.mark_log();
          stored = Some(msg);
        }
      }
      "receipt" if sender_trusted && c.st.joined => {
        if c.st.kind_of_trusted(&opened.from) != Some("runner") {
          c.drop_reason = Some("receipt from a device that is not a runner");
          return None;
        }
        let receipt = serde_json::from_value::<Receipt>(opened.payload.clone())
          .ok()
          .filter(|r| valid_chat_id(&r.id) && r.reason.as_ref().is_none_or(|x| x.chars().count() <= MAX_RECEIPT_REASON));
        let Some(receipt) = receipt else {
          c.drop_reason = Some("malformed receipt");
          return None;
        };
        let own_id = c.identity.as_ref().map(|i| i.device_id.as_str()).unwrap_or_default();
        c.st.drop_untrusted_bindings();
        let machine = c
          .log
          .iter()
          .find(|x| x.from == own_id && x.msg.id == receipt.id)
          .map(|x| &x.msg.room.machine)
          .or_else(|| c.sending.get(&receipt.id));
        let Some(machine) = machine else {
          c.drop_reason = Some("receipt for a message this device did not send");
          return None;
        };
        let machine = machine.clone();
        let runner = c.st.kind_of_trusted(&opened.from) == Some("runner");
        match c.st.bindings.get(&machine).cloned() {
          Some(d) if d == opened.from => c.receipts.push(receipt),
          _ if runner && receipt.state != ReceiptState::Failed && Self::claim_machine(c, &opened.from, &machine, now) => c.receipts.push(receipt),
          _ => {
            if runner && !c.st.bindings.contains_key(&machine) {
              c.held_receipts.retain(|x| now.saturating_sub(x.at_ms) < HELD_RECEIPT_TTL_MS);
              if c.held_receipts.len() >= MAX_HELD_RECEIPTS {
                c.held_receipts.remove(0);
              }
              c.held_receipts.push(HeldReceipt { machine, from: opened.from.clone(), at_ms: now, receipt });
            }
            c.drop_reason = Some("receipt from a runner of another machine");
            return None;
          }
        }
      }
      "join" if !sender_trusted => {
        if !join_open {
          record_alert(c, "join_code_reused", &opened.from, now);
          return Some(None);
        }
        let code = c.st.join_code.clone()?;
        let secret: [u8; 32] = b64_decode(&code.secret).ok()?.try_into().ok()?;
        let label = opened.payload.get("label")?.as_str()?.to_owned();
        let mac = opened.payload.get("mac")?.as_str()?;
        let entry = dir.devices.iter().find(|d| d.device_id == opened.from)?;
        if entry.label != label || c.st.revoked.contains(&opened.from_fp) {
          return None;
        }
        let input = |version| JoinMacInput { version, user_id: self.cfg.user_id, p_fp: &own_fp, n_id: &opened.from, n_fp: &opened.from_fp, n_kind: &entry.kind, label: &label };
        let v = [2u8, 1].into_iter().find(|v| verify_join_mac(&secret, &input(*v), mac).is_ok())?;
        c.st.join_code = None;
        c.st.pending_joins.retain(|j| j.device_id != opened.from);
        c.st.pending_joins.push(JoinRec { device_id: opened.from.clone(), label, fp: opened.from_fp.clone(), kind: entry.kind.clone(), requested_at_ms: now, v });
        self.auto_approve(c, now);
      }
      "rooms" if sender_trusted && c.st.joined => {
        let index = serde_json::from_value::<RoomsIndex>(opened.payload.clone()).ok().filter(|i| valid_index_msg_id(&opened.id) && i.is_valid());
        let from_runner = c.st.kind_of_trusted(&opened.from) == Some("runner");
        if let Some(index) = index.filter(|_| from_runner) {
          if Self::claim_machine(c, &opened.from, &index.machine, now) {
            c.st.rooms.insert(index.machine.clone(), StoredRooms { from: opened.from.clone(), index });
          }
        }
      }
      "join_ok" if sender_trusted && !c.st.joined && c.st.base.iter().any(|d| d.id == opened.from) => {
        let root = c.st.base.iter().find(|d| d.id == opened.from)?.clone();
        let kind = opened.payload.get("kind").and_then(Value::as_str);
        if kind.is_some_and(|k| check_device_kind(k).is_err()) {
          return None;
        }
        if c.st.pinned_for_other_fp(&root.id, &root.fp) {
          record_alert_once(c, "kind_conflict", &root.id, now);
          return None;
        }
        for fp in opened.payload.get("revoked").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
          if check_fp(fp).is_ok() && !c.st.revoked.iter().any(|r| r == fp) {
            c.st.revoked.push(fp.to_owned());
          }
        }
        let revoked = c.st.revoked.clone();
        c.st.trusted.retain(|d| !revoked.contains(&d.fp));
        if let Some(k) = kind {
          pin_kind(c, &root.id, &root.fp, k, now);
        }
        c.st.joined = true;
        c.joined_now = true;
      }
      "revoke" if sender_trusted && c.st.joined => {
        let statement = opened.payload.get("statement")?.as_str()?;
        let sig = opened.payload.get("sig")?.as_str()?;
        let json = json!({ "v": 1, "statement": statement, "sig": sig }).to_string();
        ingest_revoke(c, self.cfg.user_id, now, &json, None, &mut BTreeMap::new());
      }
      "chat" | "receipt" => {
        c.drop_reason = Some(if sender_trusted { "this device has not joined" } else { "sender is not a trusted member" });
        return None;
      }
      _ => return None,
    }
    Some(stored)
  }

  /// Per sender, messages are applied in `seq` order; a missing predecessor holds the rest for HOLD_MS, then a gap is recorded.
  fn process_page(&self, c: &mut Core, msgs: &[Msg], now: u64, hold: &mut Hold, out: &mut Vec<ChatMessage>) -> Result<(), String> {
    stamp_pins(c, now);
    let mut opened: Vec<(u64, Inbound)> = Vec::new();
    for m in msgs {
      let cursor = m.cursor as u64;
      match self.open_one(c, m, now) {
        Ok(i) => {
          if matches!(i, Inbound::Consumed { .. }) {
            hold.drops.push((cursor, "outside the time window", format!("sender={}", short_id(&m.from))));
          }
          opened.push((cursor, i));
        }
        Err(Some(reason)) => hold.drops.push((cursor, reason, format!("sender={}", short_id(&m.from)))),
        Err(None) => {}
      }
    }
    opened.sort_by(|a, b| (a.1.key(), a.0).cmp(&(b.1.key(), b.0)));
    for (cursor, item) in opened {
      let (from, seq, trusted) = match &item {
        Inbound::Opened(o) => (o.from.clone(), o.seq, o.trusted),
        Inbound::Consumed { from, seq } | Inbound::Repeated { from, seq } => (from.clone(), *seq, true),
      };
      let mut defer = hold.blocked.contains(&from);
      if !defer && trusted {
        if let Some(expected) = c.st.seq_in.get(&from).map(|l| l + 1).filter(|e| seq > *e) {
          let since = *c.hold_since.entry(from.clone()).or_insert(now);
          if now.saturating_sub(since) < HOLD_MS && hold.held < MAX_HELD {
            defer = true;
          } else {
            c.st.seq_gaps.push(SeqGap { from_device: from.clone(), expected, got: seq });
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
      let entry = c.st.seq_in.entry(from.clone()).or_insert(0);
      *entry = (*entry).max(seq);
      let Inbound::Opened(o) = item else { continue };
      if o.trusted {
        c.st.last_seen.insert(from, now);
      }
      c.drop_reason = None;
      c.drop_detail = None;
      let who = format!("kind={} sender={}", o.kind, short_id(&o.from));
      out.extend(self.apply(c, o, now));
      if let Some(reason) = c.drop_reason.take() {
        let detail = c.drop_detail.take().map_or(who.clone(), |d| format!("{who} {d}"));
        hold.drops.push((cursor, reason, detail));
      }
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
      rooms.push(RoomInfo { room: m.room.clone(), title, agent: None, state: None, context_pct: None, context_tokens: None, model: None, last, local: false, unrestricted: false, mention: None });
    }
    let mut index_ts: Vec<(RoomRef, u64)> = Vec::new();
    for ir in self.live_indexes(&g).into_iter().flat_map(|r| r.index.rooms.iter()).filter(|ir| self.owns(&ir.room)) {
      if let Some(ts) = ir.last_ts {
        index_ts.push((ir.room.clone(), ts));
      }
      match rooms.iter_mut().find(|r| r.room == ir.room) {
        Some(r) => {
          r.title = ir.title.clone();
          r.state = ir.state.clone();
          r.context_pct = ir.context_pct;
          r.agent = ir.agent.clone();
          r.context_tokens = ir.context_tokens;
        }
        None => rooms.push(RoomInfo {
          room: ir.room.clone(),
          title: ir.title.clone(),
          agent: ir.agent.clone(),
          state: ir.state.clone(),
          context_pct: ir.context_pct,
          context_tokens: ir.context_tokens,
          model: None,
          last: None,
          local: false,
          unrestricted: false,
          mention: None,
        }),
      }
    }
    let ts_of = |r: &RoomInfo| r.last.as_ref().map(|m| m.ts_ms).or_else(|| index_ts.iter().find(|(room, _)| *room == r.room).map(|(_, ts)| *ts));
    rooms.sort_by(|a, b| ts_of(b).cmp(&ts_of(a)).then(a.title.cmp(&b.title)));
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
    let deadline = Instant::now() + self.send_budget;
    let (msg, own_id, token, sealed) = {
      let mut g = self.lock();
      let c = &mut *g;
      self.ensure_registered(c)?;
      let own_id = c.identity.as_ref().map(|i| i.device_id.clone()).unwrap_or_default();
      if let Some(stored) = params.id.as_deref().and_then(|i| c.log.iter().find(|e| e.msg.id == i && (e.from == own_id || e.from.is_empty())).map(|e| &e.msg)) {
        let same = stored.room == params.room && stored.body_md == params.body_md && stored.reply_to == params.reply_to;
        return if same { Ok(stored.clone()) } else { Err("conflict".into()) };
      }
      c.st.drop_untrusted_bindings();
      let (id, _) = Self::own(c)?;
      if !c.st.joined || !c.st.trusted.iter().any(|d| d.id == id.device_id) {
        return Err("this device is not a trusted member of the chat yet".into());
      }
      let bound = c.st.bindings.get(&params.room.machine).cloned();
      let targets: Vec<Dev> = c
        .st
        .trusted
        .iter()
        .filter(|d| d.id != id.device_id)
        .filter(|d| match (&bound, c.st.kind_of(&d.id, &d.fp).unwrap_or(&d.kind)) {
          (_, "desktop" | "mobile" | "terminal") => true,
          (Some(runner), _) => &d.id == runner,
          (None, _) => true,
        })
        .cloned()
        .collect();
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
        mention: None,
      };
      validate_chat(&msg)?;
      let payload = serde_json::to_value(&msg).map_err(err)?;
      if c.sending.contains_key(&msg.id) {
        return Err("this message is already being sent".into());
      }
      c.sending.insert(msg.id.clone(), msg.room.machine.clone());
      let sealed = match self.seal_items(c, "chat", &msg.id, &payload, &targets) {
        Ok(sealed) => sealed,
        Err(e) => {
          c.sending.remove(&msg.id);
          return Err(e);
        }
      };
      (msg, own_id, Self::token_of(c)?, sealed)
    };
    let posted = self.post_phase(Some(&token), &msg.id, true, &sealed, Some(deadline));
    let mut g = self.lock();
    let c = &mut *g;
    c.sending.remove(&msg.id);
    if !Self::is_identity(c, &token) {
      return Err("the chat identity changed while sending".into());
    }
    Self::settle(c, &sealed.items, &posted.done);
    if let Err(e) = posted.outcome {
      if !posted.done.is_empty() {
        if let Err(w) = self.persist_out(g, true) {
          log::warn!("[manager] chat store write failed: {w}");
        }
      }
      return Err(e);
    }
    c.st.seen.push(Seen { from: own_id.clone(), id: msg.id.clone(), at: self.now() });
    trim_seen(&mut c.st);
    c.log.push(LogEntry { from: own_id, msg: msg.clone() });
    self.mark_log();
    Ok(msg)
  }

  fn owns(&self, room: &RoomRef) -> bool {
    !self.cfg.local_machine.is_empty() && room.machine != self.cfg.local_machine
  }

  fn trust_state(&self) -> Result<TrustState, String> {
    let g = self.lock();
    let own_id = g.identity.as_ref().map(|i| i.device_id.as_str());
    let dev = |d: &Dev, state| TrustDevice { id: d.id.clone(), kind: g.st.kind_of(&d.id, &d.fp).unwrap_or(&d.kind).to_owned(), label: d.label.clone(), fp: d.fp.clone(), state, is_self: Some(d.id.as_str()) == own_id,
      kind_verified: g.st.kind_of(&d.id, &d.fp).is_some(),
      can_confirm_kind: g.st.kind_of(&d.id, &d.fp).is_none() && g.st.legacy_unknown.contains(&kind_key(&d.id, &d.fp)),
    };
    let joined = g.st.joined;
    let mut devices: Vec<TrustDevice> =
      g.st.trusted.iter().map(|d| dev(d, if joined || Some(d.id.as_str()) == own_id { DeviceState::Trusted } else { DeviceState::Pending })).collect();
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
      alerts: g.st.alerts.iter().filter(|a| self.now().saturating_sub(a.at_ms) < ALERT_TTL_MS).cloned().collect(),
      gate_blockers: gate_blocked(&g, self.cfg.user_id).into_iter().map(|d| d.label).collect(),
      auto_join_error: g.auto_join_error.clone(),
      joining: g.identity.is_some() && !joined,
      can_join_with_code: g.identity.is_none() || !joined || g.st.trusted.len() <= 1,
    })
  }

  fn decide_join(&self, device_id: &str, accept: bool) -> Result<(), String> {
    let mut g = self.lock();
    let c = &mut *g;
    self.decide_join_locked(c, device_id, accept)
  }

  fn confirm_device_kind(&self, device_id: &str, kind: &str) -> Result<(), String> {
    if !matches!(kind, "desktop" | "mobile") || self.kind == "runner" {
      return Err("only a desktop or phone kind can be confirmed".into());
    }
    let mut g = self.lock();
    let c = &mut *g;
    let dev = c.st.trusted.iter().find(|d| d.id == device_id).cloned().ok_or_else(|| "unknown device".to_owned())?;
    let key = kind_key(&dev.id, &dev.fp);
    if !c.st.legacy_unknown.contains(&key) || c.st.kind_of(&dev.id, &dev.fp).is_some() {
      return Err("this device kind cannot be confirmed".into());
    }
    c.st.kinds.insert(key.clone(), kind.to_owned());
    c.st.legacy_unknown.retain(|k| *k != key);
    c.st.alerts.retain(|a| !(a.kind == "kind_unknown" && a.device_id == dev.id));
    let intro = self.intro_json(c, 2, (&dev.id, &dev.fp, kind));
    self.save_now(c)?;
    let intro = intro?;
    self.call(c, api::put_intro(&dev.id, &intro).map_err(err)?)?;
    c.st.intros.push(intro);
    let dir = c.dir.clone();
    recompute(c, dir.as_ref(), self.cfg.user_id, self.now(), false);
    self.save_now(c)
  }

  fn decide_revoke(&self, statement_id: &str, accept: bool) -> Result<(), String> {
    let mut g = self.lock();
    let c = &mut *g;
    let p = c.st.pending_revokes.iter().find(|p| p.statement_id == statement_id).cloned().ok_or_else(|| "no such revoke request".to_owned())?;
    if accept {
      self.fetch_dir(c)?;
      backfill_self(c, &self.kind);
      apply_revoke(c, self.cfg.user_id, self.now(), &p.target_id, &p.target_fp, &p.target_label);
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
    let live = c.st.trusted.iter().find(|d| d.id == device_id && d.id != id.device_id).cloned();
    let already = live.is_none();
    let target = live
      .or_else(|| c.st.revoked_devs.iter().find(|d| d.id == device_id && d.id != id.device_id).cloned())
      .ok_or_else(|| "unknown device".to_owned())?;
    let st = RevokeStatement { user_id: self.cfg.user_id, revoker_fp: keys.fingerprint(), target_id: target.id.clone(), target_fp: target.fp.clone(), issued_at_ms: self.now() };
    let json = sign_revoke(keys, &st).map_err(err)?;
    let parsed = parse_revoke(&json).map_err(err)?;
    let others: Vec<Dev> = c.st.trusted.iter().filter(|d| d.id != id.device_id && d.id != target.id).cloned().collect();
    self.call(c, api::post_revocation(&target.fp, &json).map_err(err)?)?;
    if !already {
      apply_revoke(c, self.cfg.user_id, self.now(), &target.id, &target.fp, &target.label);
    }
    self.save_now(c)?;
    if !others.is_empty() {
      let msg_id = random_msg_id("revoke").map_err(err)?;
      let payload = json!({ "statement": parsed.statement, "sig": parsed.sig });
      self.seal_and_post(c, "revoke", &msg_id, &payload, &others, false)?;
      self.save_now(c)?;
    }
    if let Err(e) = self.call(c, api::delete_device(&target.id).map_err(err)?) {
      log::warn!("[manager] the device was revoked, but its relay registration was not removed: {e}");
    }
    Ok(())
  }

  fn open_join_code(&self) -> Result<String, String> {
    RelayEngine::open_join_code(self)
  }

  fn join_with_code(&self, code: &str) -> Result<(), String> {
    RelayEngine::join_with_code(self, code)
  }

  fn close_join_code(&self, expires_at_ms: u64) -> Result<(), String> {
    RelayEngine::close_join_code(self, expires_at_ms)
  }

  fn expect_join(&self, device_id: &str, fp: &str) -> Result<(), String> {
    RelayEngine::expect_join(self, device_id, fp)
  }

  fn machine_indexes(&self) -> Vec<MachineIndex> {
    RelayEngine::machine_indexes(self)
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
    {
      let _writing = self.writer();
      self.k.clear()?;
    }
    *c = Core::default();
    self.ensure_registered(c)?;
    notice.map_err(|e| format!("the old identity was wiped, but the other devices were not told: {e}"))
  }

  fn take_receipts(&self) -> Vec<Receipt> {
    std::mem::take(&mut self.lock().receipts)
  }

  fn flush(&self) {
    let g = self.lock();
    if let Err(e) = self.persist_out(g, true) {
      log::warn!("[manager] chat store write failed: {e}");
    }
  }

  fn poll_once(&self) -> Result<Vec<ChatMessage>, String> {
    self.poll_inner(None)
  }

  fn poll_with_receipts(&self, on_receipt: &dyn Fn(Receipt)) -> Result<Vec<ChatMessage>, String> {
    self.poll_inner(Some(on_receipt))
  }
}

impl<T: Transport, K: KeyStore> RelayEngine<T, K> {
  fn poll_inner(&self, on_receipt: Option<&dyn Fn(Receipt)>) -> Result<Vec<ChatMessage>, String> {
    let (token, dir_gen) = {
      let mut g = self.lock();
      self.ensure_registered(&mut g)?;
      (Self::token_of(&g)?, g.dir_gen)
    };
    let raw_dir = self.t.call(&api::list_devices(), Some(&token))?;
    let mut g = self.lock();
    if !Self::is_identity(&g, &token) {
      return Ok(vec![]);
    }
    let dir = if g.dir_gen == dir_gen { Some(Self::take_dir(&mut g, &raw_dir)?) } else { g.dir.clone() };
    if let Some(dir) = dir {
      self.sync_with(&mut g, dir)?;
    }
    let now = self.now();
    let mut out = Vec::new();
    let mut hold = Hold::default();
    let mut after = g.st.cursor;
    let mut expected_cursor = after;
    loop {
      drop(g);
      let raw = self.t.call(&api::get_messages(after, 100).map_err(err)?, Some(&token))?;
      g = self.lock();
      if !Self::is_identity(&g, &token) || g.st.cursor != expected_cursor {
        return Ok(out);
      }
      let inbox = api::parse_inbox(&raw).map_err(err)?;
      let Some(last) = inbox.messages.iter().map(|m| m.cursor as u64).max() else {
        break;
      };
      self.process_page(&mut g, &inbox.messages, now, &mut hold, &mut out)?;
      if let Some(emit) = on_receipt.filter(|_| !g.receipts.is_empty()) {
        let fresh = std::mem::take(&mut g.receipts);
        drop(g);
        fresh.into_iter().for_each(emit);
        g = self.lock();
        if !Self::is_identity(&g, &token) {
          return Ok(out);
        }
      }
      after = last;
      g.st.cursor = g.st.cursor.max(hold.stuck.map_or(last, |s| s.saturating_sub(1)));
      self.release_drops(&mut hold, g.st.cursor);
      g = self.ack_stored(g, &token)?;
      expected_cursor = g.st.cursor;
      if !inbox.more {
        break;
      }
    }
    g.hold_since.retain(|k, _| hold.blocked.contains(k));
    self.ack_stored(g, &token).map(drop)?;
    Ok(out)
  }
}

impl<T: Transport, K: KeyStore> RelayEngine<T, K> {
  fn decide_join_locked(&self, c: &mut Core, device_id: &str, accept: bool) -> Result<(), String> {
    if c.st.expected.as_ref().is_some_and(|e| e.device_id == device_id) {
      c.st.expected = None;
    }
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
    if c.st.revoked_devs.iter().any(|r| r.id == rec.device_id && r.fp != rec.fp)
      || c.st.pinned_for_other_fp(&rec.device_id, &rec.fp)
      || c.st.trusted.iter().any(|d| d.id == rec.device_id && d.fp != rec.fp)
    {
      c.st.pending_joins.retain(|j| j.device_id != device_id);
      record_alert_once(c, "kind_conflict", &rec.device_id, self.now());
      self.save_now(c)?;
      return Err("this device id belonged to another device".into());
    }
    let contradicts = c.st.kinds.iter().any(|(k, v)| k.split_once(':').is_some_and(|(id, fp)| (id == rec.device_id || fp == rec.fp) && *v != rec.kind));
    if contradicts {
      c.st.pending_joins.retain(|j| j.device_id != device_id);
      record_alert_once(c, "kind_conflict", &rec.device_id, self.now());
      self.save_now(c)?;
      return Err("this device kind contradicts a known one".into());
    }
    let dir = self.fetch_dir(c)?;
    let entry = dir
      .devices
      .iter()
      .find(|d| d.device_id == rec.device_id && fingerprint(&d.sign_pub, &d.kex_pub) == rec.fp)
      .ok_or_else(|| "the device keys changed, ask it to join again".to_owned())?;
    let mut dev = dev_from(entry);
    dev.kind = rec.kind.clone();
    let blocked = gate_blocked(c, self.cfg.user_id);
    if rec.kind == "runner" {
      if rec.v < 2 {
        return Err("the runner must join with a code from an updated app".into());
      }
      if !blocked.is_empty() {
        let labels: Vec<&str> = blocked.iter().map(|d| d.label.as_str()).collect();
        return Err(format!("update these devices first: {}", labels.join(", ")));
      }
    } else if rec.v < 2 && runner_trusted(c) {
      return Err("update the app first".into());
    }
    let intro = self.intro_json(c, 2, (&dev.id, &dev.fp, &rec.kind))?;
    self.call(c, api::put_intro(&dev.id, &intro).map_err(err)?)?;
    c.st.intros.push(intro);
    pin_kind(c, &dev.id, &dev.fp, &rec.kind, self.now());
    if !c.st.trusted.iter().any(|d| d.id == dev.id) {
      c.st.trusted.push(dev.clone());
    }
    stamp_pins(c, self.now());
    self.save_now(c)?;
    let msg_id = random_msg_id("join_ok").map_err(err)?;
    let payload = json!({ "revoked": c.st.revoked, "kind": self.kind });
    self.seal_and_post(c, "join_ok", &msg_id, &payload, &[dev], false)?;
    c.st.pending_joins.retain(|j| j.device_id != device_id);
    self.save_now(c)
  }

  fn auto_approve(&self, c: &mut Core, now: u64) {
    if c.st.expected.as_ref().is_some_and(|e| now >= e.exp_ms) {
      c.st.expected = None;
    }
    let Some(exp) = c.st.expected.clone() else {
      return;
    };
    let Some(rec) = c.st.pending_joins.iter().find(|j| j.device_id == exp.device_id).cloned() else {
      return;
    };
    c.st.expected = None;
    if !expected_join_matches(&exp.device_id, &exp.fp, &rec.device_id, &rec.fp, &rec.kind) {
      record_alert(c, "expected_join_mismatch", &rec.device_id, now);
      return;
    }
    match self.decide_join_locked(c, &rec.device_id, true) {
      Ok(()) => c.auto_join_error = None,
      Err(e) => {
        record_alert(c, "auto_join_failed", &rec.device_id, now);
        c.auto_join_error = Some(e);
      }
    }
  }

  pub fn rebind_machine(&self, machine: &str, device_id: &str) -> Result<(), String> {
    if self.kind == "runner" {
      return Err("only an owner device can move a machine".into());
    }
    let mut g = self.lock();
    let c = &mut *g;
    let now = self.now();
    if c.st.kind_of_trusted(device_id) != Some("runner") {
      return Err("the device is not a trusted runner".into());
    }
    let claimed = c.st.alerts.iter().any(|a| a.kind == "room_conflict" && a.device_id == device_id && now.saturating_sub(a.at_ms) < ALERT_TTL_MS);
    if !claimed {
      return Err("this runner has not claimed a machine".into());
    }
    c.st.bindings.retain(|_, d| d != device_id);
    c.st.bindings.insert(machine.to_owned(), device_id.to_owned());
    c.st.alerts.retain(|a| !(a.kind == "room_conflict" && a.device_id == device_id));
    Self::release_held_receipts(c, device_id, machine, now);
    self.save_now(c)
  }

  pub fn expect_join(&self, device_id: &str, fp: &str) -> Result<(), String> {
    check_device_id(device_id).map_err(|_| "invalid device id".to_owned())?;
    check_fp(fp).map_err(|_| "invalid fingerprint".to_owned())?;
    let mut g = self.lock();
    let c = &mut *g;
    let now = self.now();
    let exp_ms = match c.st.join_code.as_ref().filter(|j| now < j.exp_ms) {
      Some(j) => j.exp_ms,
      None => c
        .st
        .pending_joins
        .iter()
        .find(|j| j.device_id == device_id)
        .map(|j| j.requested_at_ms + JOIN_TTL_MS)
        .filter(|exp| now < *exp)
        .ok_or_else(|| "the join code is not open".to_owned())?,
    };
    c.st.expected = Some(ExpectedJoin { device_id: device_id.to_owned(), fp: fp.to_owned(), exp_ms });
    c.auto_join_error = None;
    self.auto_approve(c, now);
    self.save_now(c)
  }

  fn live_indexes<'a>(&self, c: &'a Core) -> Vec<&'a StoredRooms> {
    c.st.rooms.values().filter(|r| c.st.kind_of_trusted(&r.from) == Some("runner")).collect()
  }

  pub fn machine_indexes(&self) -> Vec<MachineIndex> {
    let g = self.lock();
    self
      .live_indexes(&g)
      .into_iter()
      .map(|r| MachineIndex { machine: r.index.machine.clone(), label: r.index.machine_label.clone(), role: r.index.role.clone(), followers: r.index.followers.clone() })
      .collect()
  }

  fn token_of(c: &Core) -> Result<String, String> {
    c.identity.as_ref().map(|i| i.token.clone()).ok_or_else(|| "chat sync is not set up".to_owned())
  }

  fn is_identity(c: &Core, token: &str) -> bool {
    c.identity.as_ref().is_some_and(|i| i.token == token)
  }

  fn ack_stored<'a>(&'a self, g: MutexGuard<'a, Core>, token: &str) -> Result<MutexGuard<'a, Core>, String> {
    let g = self.persist_out(g, false)?;
    let through = g.st.cursor.min(g.written_cursor);
    if self.log_unstored() || through <= g.acked {
      return Ok(g);
    }
    drop(g);
    self.t.call(&api::ack_messages(through).map_err(err)?, Some(token))?;
    let mut g = self.lock();
    if Self::is_identity(&g, token) {
      g.acked = g.acked.max(through);
    }
    Ok(g)
  }

  fn release_drops(&self, hold: &mut Hold, passed: u64) {
    hold.drops.retain(|(cursor, reason, detail)| {
      let past = *cursor <= passed;
      if past {
        self.report_drop(reason, detail);
      }
      !past
    });
  }

  fn ack_if_stored(&self, c: &mut Core) -> Result<(), String> {
    if let Some(snap) = self.plan(c, false)? {
      if self.write(&snap)? {
        Self::commit(c, snap);
      }
    }
    let through = c.st.cursor.min(c.written_cursor);
    if !self.log_unstored() && through > c.acked {
      self.call(c, api::ack_messages(through).map_err(err)?)?;
      c.acked = through;
    }
    Ok(())
  }

  fn announce_reset(&self, c: &mut Core) -> Result<(), String> {
    let (Some(id), Some(keys)) = (&c.identity, &c.keys) else {
      return Ok(());
    };
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
