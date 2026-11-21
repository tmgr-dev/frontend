//! Relay engine: trust, seq, dedupe, join, intros and revocations of the zero-knowledge chat relay.
//! One mutex guards all state, so there is a single writer; transport calls happen under it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tmgr_ipc::chat::{AuthorKind, ChatMessage, RoomRef};
use tmgr_ipc::mgr_rpc::{limits, KIND_REQ, KIND_RES};
use tmgr_phone::{b64, b64_decode, h};

use crate::api::{self, Directory, Msg, Request};
use crate::fields::{check_device_id, check_device_kind, check_fp};
use crate::keys::{fingerprint, DeviceKeys};
use crate::trust::{
    decode_join_code, encode_join_code, gate_blockers, join_mac, kind_key, lowers_kind, may_vouch,
    parse_intro, parse_revoke, restrict_to_before, sign_intro, sign_revoke, trusted_set_by_kind,
    v2_self_markers, verify_join_mac, verify_revoke, verify_spawnkey, DirEntry, IntroStatement,
    JoinCode, JoinMacInput, KindCtx, RevokeStatement, SpawnKey, Trusted,
};
use crate::wire::{open_message, random_msg_id, seal_message, Opened, Peer, SealParams};

pub use ports::{
    ChatSink, KeyStore, MemKeyStore, RpcSink, Slot, SpawnKeySink, SpawnOkSink, Transport,
};
pub use trust_state::{
    DeviceState, PendingJoin, PendingRevoke, SeqGap, TrustAlert, TrustDevice, TrustState,
};

pub mod ports;
pub mod trust_state;

const JOIN_TTL_MS: u64 = 10 * 60 * 1000;
const SEEN_TTL_MS: u64 = 31 * 24 * 3600 * 1000;
const MAX_SEEN: usize = 5000;
const MAX_RPC_SEEN: usize = 2000;
const MAX_GAPS: usize = 100;
const MAX_HELD: usize = 200;
const HOLD_MS: u64 = 60_000;
const WRITE_EVERY_MS: u64 = 30_000;
const MAX_INTROS: usize = 200;
const MAX_REVOKES_PER_DEVICE: usize = 50;
const MAX_ALERTS: usize = 50;
const ALERT_TTL_MS: u64 = 7 * 24 * 3600 * 1000;
const AUTO_REVOKE_SILENCE_MS: u64 = 30 * 24 * 3600 * 1000;
const KINDS_V: u32 = 3;
pub const OLD_CODE_MSG: &str = "This join code is from an older app version. Update the desktop or mobile app that showed it and generate a new code.";
const BUSY_PREFIX: &str = "relay is busy, retry in ";

#[cfg(test)]
mod tests;

fn err<E: Display>(e: E) -> String {
    e.to_string()
}

/// Transport errors are the response body of a failed call; for a 429 with Retry-After the transport adds `"retry_after": <secs>`.
pub fn post_error(e: String) -> String {
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RoomScope {
    #[default]
    Remote,
    Local,
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
    #[serde(default = "join_v1")]
    v: u8,
}

fn join_v1() -> u8 {
    1
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rpc_seen: Vec<Seen>,
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
    pending_code: Option<String>,
}

impl State {
    fn kind_of(&self, id: &str, fp: &str) -> Option<&str> {
        self.kinds.get(&kind_key(id, fp)).map(String::as_str)
    }

    fn kind_of_trusted(&self, id: &str) -> Option<&str> {
        let d = self.trusted.iter().find(|d| d.id == id)?;
        self.kind_of(&d.id, &d.fp)
    }

    fn pinned_for_other_fp(&self, id: &str, fp: &str) -> bool {
        let prefix = kind_key(id, "");
        let own = kind_key(id, fp);
        self.kinds
            .keys()
            .any(|k| k.starts_with(&prefix) && *k != own)
    }
}

#[derive(Default)]
struct Hold {
    stuck: Option<u64>,
    blocked: BTreeSet<String>,
    held: usize,
    sink_err: Option<String>,
}

#[derive(Default)]
struct Core {
    identity: Option<Identity>,
    keys: Option<DeviceKeys>,
    st: State,
    unsaved: bool,
    dir: Option<Directory>,
    joined_now: bool,
    written_state: String,
    written_trust: String,
    last_write: u64,
    acked: u64,
    hold_since: BTreeMap<String, u64>,
    pass_cap: Option<usize>,
}

fn trust_key(st: &State) -> String {
    json!([
        st.base,
        st.trusted,
        st.revoked,
        st.pending_joins,
        st.pending_revokes,
        st.declined,
        st.seq_gaps,
        st.alerts,
        st.joined,
        st.kinds,
        st.bindings,
        st.legacy_unknown
    ])
    .to_string()
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
    Trusted {
        device_id: d.id.clone(),
        fp: d.fp.clone(),
        sign_pub: d.sign_pub.clone(),
        kex_pub: d.kex_pub.clone(),
    }
}

fn intro_matches(i: &str, uid: u64) -> bool {
    parse_intro(i).is_ok_and(|p| p.parsed.user_id == uid)
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
        .map(|d| DirEntry {
            device_id: d.device_id.clone(),
            sign_pub_b64: d.sign_pub.clone(),
            kex_pub_b64: d.kex_pub.clone(),
        })
        .collect();
    entries.extend(
        c.st.trusted
            .iter()
            .chain(c.st.base.iter())
            .map(|d| DirEntry {
                device_id: d.id.clone(),
                sign_pub_b64: d.sign_pub.clone(),
                kex_pub_b64: d.kex_pub.clone(),
            }),
    );
    let assumed: BTreeSet<String> = if from_base {
        c.st.base
            .iter()
            .map(|d| kind_key(&d.id, &d.fp))
            .filter(|k| !c.st.kinds.contains_key(k))
            .collect()
    } else {
        BTreeSet::new()
    };
    let own = own_key(c);
    let mut set = trusted_set_by_kind(
        uid,
        &pins,
        &c.st.revoked,
        &intros,
        &entries,
        Some(&KindCtx {
            known: &c.st.kinds,
            own: own.as_deref(),
            assumed: &assumed,
            max_passes: c.pass_cap,
        }),
    );
    if from_base {
        let before: BTreeSet<String> =
            c.st.trusted
                .iter()
                .map(|d| kind_key(&d.id, &d.fp))
                .collect();
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
        let in_dir = dir_devices
            .iter()
            .find(|d| d.device_id == t.device_id && fingerprint(&d.sign_pub, &d.kex_pub) == t.fp);
        let known =
            c.st.trusted
                .iter()
                .chain(c.st.base.iter())
                .find(|d| d.id == t.device_id && d.fp == t.fp)
                .cloned();
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
        .filter(|i| {
            parse_intro(i).is_ok_and(|p| {
                out.iter()
                    .any(|t| t.id == p.parsed.target_id && t.fp == p.parsed.target_fp)
            })
        })
        .take(MAX_INTROS)
        .collect();
    c.st.trusted = out;
    let own_id = c.identity.as_ref().map(|i| i.device_id.clone());
    let unknown: Vec<String> =
        c.st.trusted
            .iter()
            .filter(|d| {
                Some(&d.id) != own_id.as_ref()
                    && !c.st.base.iter().any(|b| b.id == d.id)
                    && c.st.kind_of(&d.id, &d.fp).is_none()
            })
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
    for i in c
        .dir
        .iter()
        .flat_map(|d| d.devices.iter())
        .flat_map(|d| d.intros.iter())
    {
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
    c.st.trusted
        .iter()
        .filter(|d| ids.contains(&d.id))
        .cloned()
        .collect()
}

fn runner_trusted(c: &Core) -> bool {
    c.st.trusted
        .iter()
        .any(|d| c.st.kind_of(&d.id, &d.fp) == Some("runner"))
}

fn backfill_self(c: &mut Core, kind: &str) {
    if let (Some(id), Some(keys)) = (&c.identity, &c.keys) {
        c.st.kinds.insert(
            kind_key(&id.device_id, &keys.fingerprint()),
            kind.to_owned(),
        );
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
    let devs: Vec<Dev> =
        c.st.trusted
            .iter()
            .chain(c.st.base.iter())
            .cloned()
            .collect();
    for d in devs.iter().filter(|d| Some(&d.id) != own.as_ref()) {
        let legacy = old
            .get(&d.id)
            .or_else(|| old.get(&kind_key(&d.id, &d.fp)))
            .filter(|k| matches!(k.as_str(), "desktop" | "mobile"));
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
    c.st.base
        .iter()
        .filter(|d| {
            Some(d.id.as_str()) != own
                && Some(d.fp.as_str()) != except_fp
                && !c.st.revoked.contains(&d.fp)
                && c.st.kind_of(&d.id, &d.fp).is_none()
        })
        .map(|d| d.id.clone())
        .collect()
}

fn code_id(code: &JoinCode) -> String {
    h(&format!(
        "{}:{}:{}",
        code.device_id,
        code.fp,
        b64(&code.secret)
    ))
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

fn stamp_pins(c: &mut Core, now: u64) {
    for d in &c.st.trusted {
        c.st.pinned_at.entry(d.id.clone()).or_insert(now);
    }
}

fn record_alert(c: &mut Core, kind: &str, device_id: &str, now: u64) {
    c.st.alerts.retain(|a| {
        now.saturating_sub(a.at_ms) < ALERT_TTL_MS && !(a.kind == kind && a.device_id == device_id)
    });
    c.st.alerts.push(TrustAlert {
        kind: kind.to_owned(),
        device_id: device_id.to_owned(),
        at_ms: now,
    });
    if c.st.alerts.len() > MAX_ALERTS {
        c.st.alerts.remove(0);
    }
}

fn record_alert_once(c: &mut Core, kind: &str, device_id: &str, now: u64) {
    c.st.alerts
        .retain(|a| now.saturating_sub(a.at_ms) < ALERT_TTL_MS);
    if c.st
        .alerts
        .iter()
        .any(|a| a.kind == kind && a.device_id == device_id)
    {
        return;
    }
    c.st.alerts.push(TrustAlert {
        kind: kind.to_owned(),
        device_id: device_id.to_owned(),
        at_ms: now,
    });
    if c.st.alerts.len() > MAX_ALERTS {
        c.st.alerts.remove(0);
    }
}

fn parse_seen(v: &Value) -> Option<u64> {
    match v {
        Value::Number(n) => n
            .as_u64()
            .map(|t| if t < 100_000_000_000 { t * 1000 } else { t }),
        Value::String(s) => chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .and_then(|d| u64::try_from(d.timestamp_millis()).ok()),
        _ => None,
    }
}

fn unseen_for_30_days(c: &Core, device_id: &str, now: u64) -> bool {
    let dir_seen = c
        .dir
        .as_ref()
        .and_then(|d| d.devices.iter().find(|x| x.device_id == device_id))
        .and_then(|d| parse_seen(&d.last_seen_at));
    let since = [
        c.st.last_seen.get(device_id).copied(),
        c.st.pinned_at.get(device_id).copied(),
        dir_seen,
    ]
    .into_iter()
    .flatten()
    .max();
    since.is_some_and(|t| now.saturating_sub(t) > AUTO_REVOKE_SILENCE_MS)
}

fn apply_revoke(c: &mut Core, uid: u64, now: u64, target_id: &str, target_fp: &str, label: &str) {
    if !c.st.revoked.iter().any(|f| f == target_fp) {
        c.st.revoked.push(target_fp.to_owned());
    }
    let kind =
        c.st.kind_of(target_id, target_fp)
            .map(str::to_owned)
            .or_else(|| {
                c.st.trusted
                    .iter()
                    .find(|d| d.fp == target_fp)
                    .map(|d| d.kind.clone())
            })
            .unwrap_or_default();
    c.st.kinds.remove(&kind_key(target_id, target_fp));
    c.st.revoked_devs.push(Dev {
        id: target_id.to_owned(),
        kind,
        label: label.to_owned(),
        fp: target_fp.to_owned(),
        sign_pub: String::new(),
        kex_pub: String::new(),
    });
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

fn ingest_revoke(
    c: &mut Core,
    uid: u64,
    now: u64,
    json: &str,
    expect_fp: Option<&str>,
    budget: &mut BTreeMap<String, usize>,
) {
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
    let target_label =
        c.st.trusted
            .iter()
            .find(|d| d.fp == p.target_fp)
            .map(|d| d.label.clone())
            .unwrap_or_default();
    let aims_at_us = own_fp.as_deref() == Some(p.target_fp.as_str());
    if p.revoker_fp == p.target_fp
        || own_fp.as_deref() == Some(p.revoker_fp.as_str())
        || (!aims_at_us && unseen_for_30_days(c, &p.target_id, now))
    {
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

pub struct RelayEngine<T: Transport, K: KeyStore, S: ChatSink> {
    t: T,
    k: K,
    s: S,
    cfg: EngineConfig,
    kind: String,
    now: Box<dyn Fn() -> u64 + Send + Sync>,
    scope: RoomScope,
    rpc: Option<Arc<dyn RpcSink>>,
    spawnkey: Option<Arc<dyn SpawnKeySink>>,
    spawnok: Option<Arc<dyn SpawnOkSink>>,
    core: Mutex<Core>,
}

impl<T: Transport, K: KeyStore, S: ChatSink> RelayEngine<T, K, S> {
    pub fn new(t: T, k: K, s: S, cfg: EngineConfig) -> Result<Self, String> {
        let mut core = Core::default();
        if let Some(raw) = k.load(Slot::Identity)? {
            let id: Identity = serde_json::from_str(&raw)
                .map_err(|_| "the chat identity is unreadable".to_owned())?;
            let keys = DeviceKeys::from_secrets(
                &b64_decode(&id.sign_secret).map_err(err)?,
                &b64_decode(&id.kex_secret).map_err(err)?,
            )
            .map_err(err)?;
            core.keys = Some(keys);
            core.identity = Some(id);
            if let Some(raw) = k.load(Slot::State)? {
                core.st = serde_json::from_str(&raw)
                    .map_err(|_| "the chat state is unreadable".to_owned())?;
            }
            core.written_state = serde_json::to_string(&core.st).map_err(err)?;
            core.written_trust = trust_key(&core.st);
            core.acked = core.st.cursor;
        }
        let now = Box::new(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as u64)
        });
        Ok(Self {
            t,
            k,
            s,
            cfg,
            kind: "desktop".into(),
            now,
            scope: RoomScope::default(),
            rpc: None,
            spawnkey: None,
            spawnok: None,
            core: Mutex::new(core),
        })
    }

    pub fn with_kind(mut self, kind: &str) -> Self {
        self.kind = kind.to_owned();
        let c = self.core.get_mut().unwrap_or_else(|p| p.into_inner());
        if kind == "runner" {
            c.st.join_code = None;
        }
        backfill_self(c, kind);
        self
    }

    pub fn with_rpc_sink(mut self, sink: Arc<dyn RpcSink>) -> Self {
        self.rpc = Some(sink);
        self
    }

    pub fn with_spawnkey_sink(mut self, sink: Arc<dyn SpawnKeySink>) -> Self {
        self.spawnkey = Some(sink);
        self
    }

    pub fn with_spawnok_sink(mut self, sink: Arc<dyn SpawnOkSink>) -> Self {
        self.spawnok = Some(sink);
        self
    }

    pub fn with_room_scope(mut self, scope: RoomScope) -> Self {
        self.scope = scope;
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

    fn call(&self, c: &Core, req: Request) -> Result<String, String> {
        self.t
            .call(&req, c.identity.as_ref().map(|i| i.token.as_str()))
    }

    fn save_now(&self, c: &mut Core) -> Result<(), String> {
        self.persist(c, true).map(|_| ())
    }

    /// Poll-time writes are throttled unless trust changed; a message is acked only once the state that dedupes it is stored.
    fn persist(&self, c: &mut Core, force: bool) -> Result<bool, String> {
        let st = serde_json::to_string(&c.st).map_err(err)?;
        let state_changed = st != c.written_state;
        if !state_changed && !c.unsaved {
            return Ok(true);
        }
        let now = self.now();
        let trust = trust_key(&c.st);
        if !force && trust == c.written_trust && now.abs_diff(c.last_write) < WRITE_EVERY_MS {
            return Ok(false);
        }
        if state_changed {
            self.k.save(Slot::State, &st)?;
            c.written_state = st;
        }
        c.unsaved = false;
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
            backfill_self(c, &self.kind);
            return Ok(());
        }
        let keys = DeviceKeys::generate().map_err(err)?;
        let req = api::register_device(
            &self.kind,
            &self.cfg.label,
            &keys.sign_pub_b64(),
            &keys.kex_pub_b64(),
            None,
        )
        .map_err(err)?;
        let reg = api::parse_registered(&self.t.call(&req, None)?).map_err(err)?;
        let me = Dev {
            id: reg.device_id.clone(),
            kind: self.kind.clone(),
            label: self.cfg.label.clone(),
            fp: keys.fingerprint(),
            sign_pub: keys.sign_pub_b64(),
            kex_pub: keys.kex_pub_b64(),
        };
        let identity = Identity {
            device_id: reg.device_id,
            token: reg.device_token,
            sign_secret: b64(keys.sign_secret()),
            kex_secret: b64(keys.kex_secret()),
        };
        self.k.save(
            Slot::Identity,
            &serde_json::to_string(&identity).map_err(err)?,
        )?;
        c.identity = Some(identity);
        c.keys = Some(keys);
        c.st = State {
            base: vec![me.clone()],
            trusted: vec![me.clone()],
            joined: true,
            ..State::default()
        };
        c.st.kinds_v = KINDS_V;
        c.st.kinds
            .insert(kind_key(&me.id, &me.fp), self.kind.clone());
        c.unsaved = false;
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
        backfill_self(c, &self.kind);
        let now = self.now();
        recompute(c, Some(&dir), self.cfg.user_id, now, false);
        for id in unknown_roots(c, None) {
            record_alert_once(c, "root_kind_unknown", &id, now);
        }
        stamp_pins(c, now);
        let mut budget = BTreeMap::new();
        for r in &dir.revocations {
            ingest_revoke(
                c,
                self.cfg.user_id,
                now,
                &r.statement,
                Some(&r.target_fp),
                &mut budget,
            );
        }
        if (c.st.republish_intros || !c.st.self_intro) && self.publish_intros(c).is_ok() {
            c.st.republish_intros = false;
            c.st.self_intro = true;
        }
        Ok(())
    }

    fn seal_and_post(
        &self,
        c: &mut Core,
        kind: &str,
        msg_id: &str,
        payload: &Value,
        targets: &[Dev],
        push: bool,
    ) -> Result<(), String> {
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
                    to: Peer {
                        device_id: &t.id,
                        sign_pub_b64: &t.sign_pub,
                        kex_pub_b64: &t.kex_pub,
                    },
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
            let refs: Vec<(&str, &str)> = chunk
                .iter()
                .map(|(to, body, _)| (to.as_str(), body.as_str()))
                .collect();
            let req = api::post_messages(msg_id, push, &refs).map_err(err)?;
            api::parse_posted(&self.call(c, req).map_err(post_error)?).map_err(err)?;
            for (to, _, seq) in chunk {
                c.st.seq_out.insert(to.clone(), *seq);
            }
        }
        Ok(())
    }

    fn intro_json(
        &self,
        c: &Core,
        version: u8,
        target: (&str, &str, &str),
    ) -> Result<String, String> {
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
        signed.push((
            own_id.clone(),
            self.intro_json(c, 2, (&own_id, &own_fp, &self.kind))?,
        ));
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
        if self.kind == "runner" {
            return Err("a runner cannot issue join codes".into());
        }
        let mut g = self.lock();
        let c = &mut *g;
        self.ensure_registered(c)?;
        let (id, keys) = Self::own(c)?;
        let mut secret = [0u8; 32];
        getrandom::fill(&mut secret).map_err(err)?;
        let exp_ms = self.now() + JOIN_TTL_MS;
        let code = JoinCode {
            version: 2,
            user_id: self.cfg.user_id,
            device_id: id.device_id.clone(),
            fp: keys.fingerprint(),
            kind: Some(self.kind.clone()),
            secret,
            exp_ms,
        };
        let text = encode_join_code(&code).map_err(err)?;
        c.st.join_code = Some(OpenCode {
            secret: b64(&secret),
            exp_ms,
        });
        self.save_now(c)?;
        Ok(text)
    }

    pub fn join_with_code(&self, code: &str) -> Result<(), String> {
        let mut g = self.lock();
        let c = &mut *g;
        let code =
            decode_join_code(code.trim()).map_err(|_| "the join code is not valid".to_owned())?;
        if code.version < 2 && self.kind == "runner" {
            return Err(OLD_CODE_MSG.into());
        }
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
            .find(|d| {
                d.device_id == code.device_id && fingerprint(&d.sign_pub, &d.kex_pub) == code.fp
            })
            .ok_or_else(|| "the device of the join code was not found".to_owned())?;
        if code.kind.as_deref().is_some_and(|k| k != p.kind) {
            return Err("the join code does not match the device".into());
        }
        let pdev = dev_from(p);
        let (id, keys) = Self::own(c)?;
        let mac = join_mac(
            &code.secret,
            &JoinMacInput {
                version: code.version,
                user_id: self.cfg.user_id,
                p_fp: &pdev.fp,
                n_id: &id.device_id,
                n_fp: &keys.fingerprint(),
                n_kind: &self.kind,
                label: &self.cfg.label,
            },
        )
        .map_err(err)?;
        if c.st.pinned_for_other_fp(&pdev.id, &pdev.fp) {
            return Err("the device id was seen with other keys".into());
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
        let res = self.seal_and_post(
            c,
            "join",
            &msg_id,
            &payload,
            std::slice::from_ref(&pdev),
            true,
        );
        if res.is_err() {
            if inserted {
                c.st.kinds.remove(&key);
            }
            c.st.base.retain(|d| d.id != pdev.id);
            c.st.trusted.retain(|d| d.id != pdev.id);
            c.st.joined = true;
        } else {
            c.st.pending_code = Some(code_id(&code));
        }
        self.save_now(c)?;
        res
    }

    fn open_one(&self, c: &Core, m: &Msg, now: u64) -> Option<Opened> {
        if c.st
            .seen
            .iter()
            .any(|s| s.from == m.from && s.id == m.msg_id)
        {
            return None;
        }
        let kind_hint = serde_json::from_str::<Value>(&m.body)
            .ok()
            .and_then(|v| v.get("kind")?.as_str().map(str::to_owned));
        let join_open = kind_hint.as_deref() == Some("join");
        let dir = c.dir.as_ref()?;
        let (id, keys) = Self::own(c).ok()?;
        let trusted: Vec<Trusted> = c.st.trusted.iter().map(to_trusted).collect();
        open_message(
            keys,
            self.cfg.user_id,
            &id.device_id,
            &m.from,
            &m.msg_id,
            &m.body,
            now,
            &trusted,
            |from| {
                join_open
                    .then(|| dir.devices.iter().find(|d| d.device_id == from))
                    .flatten()
                    .map(|d| Peer {
                        device_id: &d.device_id,
                        sign_pub_b64: &d.sign_pub,
                        kex_pub_b64: &d.kex_pub,
                    })
            },
        )
        .ok()
    }

    fn apply_rpc(&self, c: &mut Core, o: &Opened, now: u64) {
        let Some(sink) = &self.rpc else { return };
        let cap = if o.kind == KIND_REQ {
            limits::MAX_REQ_BYTES
        } else {
            limits::MAX_RES_BYTES
        };
        let own = c.identity.as_ref().map(|i| i.device_id.as_str());
        if self.kind != "runner"
            || !c.st.joined
            || !o.trusted
            || Some(o.from.as_str()) == own
            || c.st.kind_of_trusted(&o.from) != Some("runner")
            || c.st
                .rpc_seen
                .iter()
                .any(|s| s.from == o.from && s.id == o.id)
            || serde_json::to_vec(&o.payload).map_or(true, |b| b.len() > cap)
        {
            return;
        }
        c.st.rpc_seen.push(Seen {
            from: o.from.clone(),
            id: o.id.clone(),
            at: o.sent_at,
        });
        c.st.rpc_seen.retain(|s| s.at + SEEN_TTL_MS >= now);
        if c.st.rpc_seen.len() > MAX_RPC_SEEN {
            let cut = c.st.rpc_seen.len() - MAX_RPC_SEEN;
            c.st.rpc_seen.drain(..cut);
        }
        sink.deliver_rpc(&o.from, &o.from_fp, &o.kind, &o.id, o.payload.clone());
    }

    fn apply(&self, c: &mut Core, opened: Opened, now: u64) -> Result<Option<ChatMessage>, String> {
        if opened.kind == KIND_REQ || opened.kind == KIND_RES {
            self.apply_rpc(c, &opened, now);
            return Ok(None);
        }
        let Some(chat) = self.handle(c, &opened, now) else {
            return Ok(None);
        };
        if let Some(msg) = &chat {
            self.s.deliver(&opened.from, msg.clone())?;
            c.unsaved = true;
        }
        c.st.seen.push(Seen {
            from: opened.from,
            id: opened.id,
            at: opened.sent_at,
        });
        c.st.seen.retain(|s| s.at + SEEN_TTL_MS >= now);
        if c.st.seen.len() > MAX_SEEN {
            let cut = c.st.seen.len() - MAX_SEEN;
            c.st.seen.drain(..cut);
        }
        Ok(chat)
    }

    fn authorized_author(c: &mut Core, from: &str, msg: &ChatMessage, now: u64) -> bool {
        let kind = c.st.kind_of_trusted(from).map(str::to_owned);
        match msg.author.kind {
            AuthorKind::Owner => matches!(kind.as_deref(), Some("desktop" | "mobile")),
            AuthorKind::Manager | AuthorKind::Session | AuthorKind::System => {
                if kind.as_deref() != Some("runner")
                    || msg
                        .author
                        .machine
                        .as_ref()
                        .is_some_and(|m| *m != msg.room.machine)
                {
                    return false;
                }
                let machine = &msg.room.machine;
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
                        c.st.bindings.insert(machine.clone(), from.to_owned());
                        true
                    }
                }
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
                    let unknown_author = opened.payload["author"]["kind"]
                        .as_str()
                        .is_some_and(|k| !matches!(k, "owner" | "manager" | "session" | "system"));
                    if unknown_author && opened.payload["id"].as_str() == Some(opened.id.as_str()) {
                        record_alert(c, "unknown_author_kind", &opened.from, now);
                    }
                    return None;
                };
                if msg.validate().is_err() || msg.id != opened.id || !self.owns(&msg.room) {
                    return None;
                }
                if !Self::authorized_author(c, &opened.from, &msg, now) {
                    return None;
                }
                if !c
                    .st
                    .seen
                    .iter()
                    .any(|s| s.from == opened.from && s.id == msg.id)
                {
                    stored = Some(msg);
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
                let input = |version| JoinMacInput {
                    version,
                    user_id: self.cfg.user_id,
                    p_fp: &own_fp,
                    n_id: &opened.from,
                    n_fp: &opened.from_fp,
                    n_kind: &entry.kind,
                    label: &label,
                };
                let v = [2u8, 1]
                    .into_iter()
                    .find(|v| verify_join_mac(&secret, &input(*v), mac).is_ok())?;
                c.st.join_code = None;
                c.st.pending_joins.retain(|j| j.device_id != opened.from);
                c.st.pending_joins.push(JoinRec {
                    device_id: opened.from.clone(),
                    label,
                    fp: opened.from_fp.clone(),
                    kind: entry.kind.clone(),
                    requested_at_ms: now,
                    v,
                });
            }
            "join_ok"
                if sender_trusted
                    && !c.st.joined
                    && c.st.base.iter().any(|d| d.id == opened.from) =>
            {
                let root = c.st.base.iter().find(|d| d.id == opened.from)?.clone();
                let kind = opened.payload.get("kind").and_then(Value::as_str);
                if kind.is_some_and(|k| check_device_kind(k).is_err()) {
                    return None;
                }
                if c.st.pinned_for_other_fp(&root.id, &root.fp) {
                    record_alert_once(c, "kind_conflict", &root.id, now);
                    return None;
                }
                for fp in opened
                    .payload
                    .get("revoked")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
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
                c.st.pending_code = None;
                c.joined_now = true;
            }
            "revoke" if sender_trusted && c.st.joined => {
                let statement = opened.payload.get("statement")?.as_str()?;
                let sig = opened.payload.get("sig")?.as_str()?;
                let json = json!({ "v": 1, "statement": statement, "sig": sig }).to_string();
                ingest_revoke(c, self.cfg.user_id, now, &json, None, &mut BTreeMap::new());
            }
            "spawnkey" if sender_trusted && c.st.joined => {
                let sink = self.spawnkey.as_ref()?;
                let own = c.identity.as_ref().map(|i| i.device_id.as_str());
                let dev = c.st.trusted.iter().find(|d| d.id == opened.from)?;
                if Some(dev.id.as_str()) == own
                    || dev.fp != opened.from_fp
                    || c.st.revoked.contains(&dev.fp)
                {
                    return None;
                }
                let json = serde_json::to_string(&opened.payload).ok()?;
                verify_spawnkey(
                    &json,
                    self.cfg.user_id,
                    &dev.id,
                    &dev.sign_pub,
                    &dev.kex_pub,
                )
                .ok()?;
                let key: SpawnKey = serde_json::from_str(&json).ok()?;
                sink.deliver_spawnkey(&dev.id, &dev.fp, &dev.sign_pub, &dev.kex_pub, &key);
            }
            "spawnok" if sender_trusted && c.st.joined => {
                let sink = self.spawnok.as_ref()?;
                let own = c.identity.as_ref().map(|i| i.device_id.as_str());
                let dev = c.st.trusted.iter().find(|d| d.id == opened.from)?;
                if Some(dev.id.as_str()) == own
                    || dev.fp != opened.from_fp
                    || c.st.revoked.contains(&dev.fp)
                {
                    return None;
                }
                let signed: tmgr_spawn_auth::SignedSpawn =
                    serde_json::from_value(opened.payload.clone()).ok()?;
                if signed.device_fp != dev.fp {
                    return None;
                }
                sink.deliver_spawnok(&dev.id, &dev.fp, &signed);
            }
            _ => return None,
        }
        Some(stored)
    }

    /// Per sender, messages are applied in `seq` order; a missing predecessor holds the rest for HOLD_MS, then a gap is recorded.
    fn process_page(
        &self,
        c: &mut Core,
        msgs: &[Msg],
        now: u64,
        hold: &mut Hold,
        out: &mut Vec<ChatMessage>,
    ) -> Result<(), String> {
        stamp_pins(c, now);
        let mut opened: Vec<(u64, Opened)> = msgs
            .iter()
            .filter_map(|m| Some((m.cursor as u64, self.open_one(c, m, now)?)))
            .collect();
        opened.sort_by(|a, b| (&a.1.from, a.1.seq, a.0).cmp(&(&b.1.from, b.1.seq, b.0)));
        for (cursor, o) in opened {
            let from = o.from.clone();
            let mut defer = hold.blocked.contains(&from);
            if !defer && o.trusted {
                if let Some(expected) = c.st.seq_in.get(&from).map(|l| l + 1).filter(|e| o.seq > *e)
                {
                    let since = *c.hold_since.entry(from.clone()).or_insert(now);
                    if now.saturating_sub(since) < HOLD_MS && hold.held < MAX_HELD {
                        defer = true;
                    } else {
                        c.st.seq_gaps.push(SeqGap {
                            from_device: from.clone(),
                            expected,
                            got: o.seq,
                        });
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
            if o.trusted {
                c.st.last_seen.insert(from.clone(), now);
            }
            let prev_seq = c.st.seq_in.get(&from).copied();
            let entry = c.st.seq_in.entry(from.clone()).or_insert(0);
            *entry = (*entry).max(o.seq);
            match self.apply(c, o, now) {
                Ok(delivered) => out.extend(delivered),
                Err(e) => {
                    match prev_seq {
                        Some(s) => c.st.seq_in.insert(from.clone(), s),
                        None => c.st.seq_in.remove(&from),
                    };
                    hold.blocked.insert(from);
                    hold.stuck = Some(hold.stuck.map_or(cursor, |s| s.min(cursor)));
                    hold.sink_err.get_or_insert(e);
                    continue;
                }
            }
            if std::mem::take(&mut c.joined_now) {
                self.finish_join(c)?;
            }
        }
        Ok(())
    }

    pub fn own_device(&self) -> Option<(String, String)> {
        let g = self.lock();
        Some((
            g.identity.as_ref()?.device_id.clone(),
            g.keys.as_ref()?.fingerprint(),
        ))
    }

    pub fn owns(&self, room: &RoomRef) -> bool {
        if self.cfg.local_machine.is_empty() {
            return false;
        }
        match self.scope {
            RoomScope::Remote => room.machine != self.cfg.local_machine,
            RoomScope::Local => room.machine == self.cfg.local_machine,
        }
    }

    pub fn send_chat(&self, msg: &ChatMessage, push: bool) -> Result<(), String> {
        msg.validate()?;
        let mut g = self.lock();
        let c = &mut *g;
        self.ensure_registered(c)?;
        let (id, _) = Self::own(c)?;
        if !c.st.joined || !c.st.trusted.iter().any(|d| d.id == id.device_id) {
            return Err("this device is not a trusted member of the chat yet".into());
        }
        let runner = self.kind == "runner";
        let targets: Vec<Dev> =
            c.st.trusted
                .iter()
                .filter(|d| d.id != id.device_id)
                .filter(|d| !runner || may_vouch(c.st.kind_of(&d.id, &d.fp)))
                .cloned()
                .collect();
        if targets.is_empty() {
            return Err("there are no other trusted devices to send to".into());
        }
        let payload = serde_json::to_value(msg).map_err(err)?;
        self.seal_and_post(c, "chat", &msg.id, &payload, &targets, push)?;
        self.save_now(c)
    }

    pub fn send_rpc(
        &self,
        to_device: &str,
        kind: &str,
        msg_id: &str,
        payload: &Value,
    ) -> Result<(), String> {
        let (cap, letter) = match kind {
            KIND_REQ => (limits::MAX_REQ_BYTES, "g_"),
            KIND_RES => (limits::MAX_RES_BYTES, "h_"),
            _ => return Err("unsupported rpc kind".into()),
        };
        if !msg_id.starts_with(letter) {
            return Err("the rpc message id has the wrong prefix".into());
        }
        if self.kind != "runner" {
            return Err("only a runner can send rpc messages".into());
        }
        if serde_json::to_vec(payload).map_err(err)?.len() > cap {
            return Err("the rpc payload is too large".into());
        }
        let mut g = self.lock();
        let c = &mut *g;
        self.ensure_registered(c)?;
        let (id, _) = Self::own(c)?;
        if !c.st.joined || !c.st.trusted.iter().any(|d| d.id == id.device_id) {
            return Err("this device is not a trusted member of the chat yet".into());
        }
        if to_device == id.device_id {
            return Err("cannot send an rpc message to this device".into());
        }
        let target =
            c.st.trusted
                .iter()
                .find(|d| d.id == to_device && c.st.kind_of(&d.id, &d.fp) == Some("runner"))
                .cloned()
                .ok_or_else(|| "the target is not a trusted runner device".to_owned())?;
        self.seal_and_post(c, kind, msg_id, payload, &[target], false)?;
        self.save_now(c)
    }

    pub fn pending_join_matches(&self, code: &str) -> bool {
        let Ok(code) = decode_join_code(code.trim()) else {
            return false;
        };
        let g = self.lock();
        !g.st.joined && g.st.pending_code.as_deref() == Some(code_id(&code).as_str())
    }

    pub fn reset_pending_join(&self) -> Result<(), String> {
        let mut g = self.lock();
        let c = &mut *g;
        if c.st.joined {
            return Ok(());
        }
        let own = c.identity.as_ref().map(|i| i.device_id.clone());
        let stale: Vec<(String, String)> =
            c.st.base
                .iter()
                .filter(|d| Some(&d.id) != own.as_ref())
                .map(|d| (d.id.clone(), d.fp.clone()))
                .collect();
        for (id, fp) in stale {
            c.st.kinds.remove(&kind_key(&id, &fp));
            c.st.base.retain(|d| d.id != id);
            c.st.trusted.retain(|d| d.id != id);
        }
        c.st.joined = true;
        c.st.pending_code = None;
        self.save_now(c)
    }

    pub fn is_joined(&self) -> bool {
        self.lock().st.joined
    }

    pub fn trust_state(&self) -> Result<TrustState, String> {
        let g = self.lock();
        let own_id = g.identity.as_ref().map(|i| i.device_id.as_str());
        let dev = |d: &Dev, state| TrustDevice {
            id: d.id.clone(),
            kind: g.st.kind_of(&d.id, &d.fp).unwrap_or(&d.kind).to_owned(),
            label: d.label.clone(),
            fp: d.fp.clone(),
            state,
            is_self: Some(d.id.as_str()) == own_id,
            kind_verified: g.st.kind_of(&d.id, &d.fp).is_some(),
            can_confirm_kind: g.st.kind_of(&d.id, &d.fp).is_none()
                && g.st.legacy_unknown.contains(&kind_key(&d.id, &d.fp)),
        };
        let mut devices: Vec<TrustDevice> =
            g.st.trusted
                .iter()
                .map(|d| dev(d, DeviceState::Trusted))
                .collect();
        devices.extend(
            g.st.revoked_devs
                .iter()
                .map(|d| dev(d, DeviceState::Revoked)),
        );
        Ok(TrustState {
            registered: g.identity.is_some(),
            self_fp: g.keys.as_ref().map(|k| k.fingerprint()),
            devices,
            pending_joins: g
                .st
                .pending_joins
                .iter()
                .map(|j| PendingJoin {
                    device_id: j.device_id.clone(),
                    label: j.label.clone(),
                    fp: j.fp.clone(),
                    kind: Some(j.kind.clone()),
                    requested_at_ms: j.requested_at_ms,
                })
                .collect(),
            pending_revokes: g.st.pending_revokes.clone(),
            key_mismatch: g.st.mismatch.clone(),
            seq_gaps: g.st.seq_gaps.clone(),
            alerts: g
                .st
                .alerts
                .iter()
                .filter(|a| self.now().saturating_sub(a.at_ms) < ALERT_TTL_MS)
                .cloned()
                .collect(),
        })
    }

    pub fn decide_join(&self, device_id: &str, accept: bool) -> Result<(), String> {
        if self.kind == "runner" {
            return Err("a runner cannot approve joins".into());
        }
        let mut g = self.lock();
        let c = &mut *g;
        let rec =
            c.st.pending_joins
                .iter()
                .find(|j| j.device_id == device_id)
                .cloned()
                .ok_or_else(|| "no such join request".to_owned())?;
        if !accept {
            c.st.pending_joins.retain(|j| j.device_id != device_id);
            return self.save_now(c);
        }
        if c.st.revoked.contains(&rec.fp) {
            c.st.pending_joins.retain(|j| j.device_id != device_id);
            self.save_now(c)?;
            return Err("this device was revoked".into());
        }
        if c.st
            .revoked_devs
            .iter()
            .any(|r| r.id == rec.device_id && r.fp != rec.fp)
            || c.st.pinned_for_other_fp(&rec.device_id, &rec.fp)
            || c.st
                .trusted
                .iter()
                .any(|d| d.id == rec.device_id && d.fp != rec.fp)
        {
            c.st.pending_joins.retain(|j| j.device_id != device_id);
            record_alert_once(c, "kind_conflict", &rec.device_id, self.now());
            self.save_now(c)?;
            return Err("this device id belonged to another device".into());
        }
        let contradicts = c.st.kinds.iter().any(|(k, v)| {
            k.split_once(':')
                .is_some_and(|(id, fp)| (id == rec.device_id || fp == rec.fp) && *v != rec.kind)
        });
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
            .find(|d| {
                d.device_id == rec.device_id && fingerprint(&d.sign_pub, &d.kex_pub) == rec.fp
            })
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

    pub fn confirm_device_kind(&self, device_id: &str, kind: &str) -> Result<(), String> {
        if !matches!(kind, "desktop" | "mobile") || self.kind == "runner" {
            return Err("only a desktop or phone kind can be confirmed".into());
        }
        let mut g = self.lock();
        let c = &mut *g;
        let dev =
            c.st.trusted
                .iter()
                .find(|d| d.id == device_id)
                .cloned()
                .ok_or_else(|| "unknown device".to_owned())?;
        let key = kind_key(&dev.id, &dev.fp);
        if !c.st.legacy_unknown.contains(&key) || c.st.kind_of(&dev.id, &dev.fp).is_some() {
            return Err("this device kind cannot be confirmed".into());
        }
        c.st.kinds.insert(key.clone(), kind.to_owned());
        c.st.legacy_unknown.retain(|k| *k != key);
        c.st.alerts
            .retain(|a| !(a.kind == "kind_unknown" && a.device_id == dev.id));
        let intro = self.intro_json(c, 2, (&dev.id, &dev.fp, kind));
        self.save_now(c)?;
        let intro = intro?;
        self.call(c, api::put_intro(&dev.id, &intro).map_err(err)?)?;
        c.st.intros.push(intro);
        let dir = c.dir.clone();
        recompute(c, dir.as_ref(), self.cfg.user_id, self.now(), false);
        self.save_now(c)
    }

    pub fn decide_revoke(&self, statement_id: &str, accept: bool) -> Result<(), String> {
        let mut g = self.lock();
        let c = &mut *g;
        let p =
            c.st.pending_revokes
                .iter()
                .find(|p| p.statement_id == statement_id)
                .cloned()
                .ok_or_else(|| "no such revoke request".to_owned())?;
        if accept {
            self.fetch_dir(c)?;
            backfill_self(c, &self.kind);
            apply_revoke(
                c,
                self.cfg.user_id,
                self.now(),
                &p.target_id,
                &p.target_fp,
                &p.target_label,
            );
        } else {
            c.st.pending_revokes
                .retain(|x| x.statement_id != statement_id);
            c.st.declined.push(p.statement_id);
        }
        self.save_now(c)
    }

    pub fn revoke(&self, device_id: &str) -> Result<(), String> {
        let mut g = self.lock();
        let c = &mut *g;
        self.ensure_registered(c)?;
        let (id, keys) = Self::own(c)?;
        let live =
            c.st.trusted
                .iter()
                .find(|d| d.id == device_id && d.id != id.device_id)
                .cloned();
        let already = live.is_none();
        let target = live
            .or_else(|| {
                c.st.revoked_devs
                    .iter()
                    .find(|d| d.id == device_id && d.id != id.device_id)
                    .cloned()
            })
            .ok_or_else(|| "unknown device".to_owned())?;
        let st = RevokeStatement {
            user_id: self.cfg.user_id,
            revoker_fp: keys.fingerprint(),
            target_id: target.id.clone(),
            target_fp: target.fp.clone(),
            issued_at_ms: self.now(),
        };
        let json = sign_revoke(keys, &st).map_err(err)?;
        let parsed = parse_revoke(&json).map_err(err)?;
        let others: Vec<Dev> =
            c.st.trusted
                .iter()
                .filter(|d| d.id != id.device_id && d.id != target.id)
                .cloned()
                .collect();
        self.call(c, api::post_revocation(&target.fp, &json).map_err(err)?)?;
        if !already {
            let now = self.now();
            apply_revoke(
                c,
                self.cfg.user_id,
                now,
                &target.id,
                &target.fp,
                &target.label,
            );
        }
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

    pub fn reset_identity(&self) -> Result<(), String> {
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
        notice.map_err(|e| {
            format!("the old identity was wiped, but the other devices were not told: {e}")
        })
    }

    pub fn poll_once(&self) -> Result<Vec<ChatMessage>, String> {
        let mut g = self.lock();
        let c = &mut *g;
        self.ensure_registered(c)?;
        self.sync_dir(c)?;
        let now = self.now();
        let mut out = Vec::new();
        let mut hold = Hold::default();
        let mut after = c.st.cursor;
        loop {
            let inbox =
                api::parse_inbox(&self.call(c, api::get_messages(after, 100).map_err(err)?)?)
                    .map_err(err)?;
            let Some(last) = inbox.messages.iter().map(|m| m.cursor as u64).max() else {
                break;
            };
            self.process_page(c, &inbox.messages, now, &mut hold, &mut out)?;
            after = last;
            c.st.cursor =
                c.st.cursor
                    .max(hold.stuck.map_or(last, |s| s.saturating_sub(1)));
            self.ack_if_stored(c)?;
            if !inbox.more {
                break;
            }
        }
        c.hold_since.retain(|k, _| hold.blocked.contains(k));
        self.ack_if_stored(c)?;
        match hold.sink_err {
            Some(e) => Err(e),
            None => Ok(out),
        }
    }

    fn ack_if_stored(&self, c: &mut Core) -> Result<(), String> {
        self.persist(c, false)?;
        if !c.unsaved && c.st.cursor > c.acked {
            self.call(c, api::ack_messages(c.st.cursor).map_err(err)?)?;
            c.acked = c.st.cursor;
        }
        Ok(())
    }

    fn announce_reset(&self, c: &mut Core) -> Result<(), String> {
        let (Some(id), Some(keys)) = (&c.identity, &c.keys) else {
            return Ok(());
        };
        let others: Vec<Dev> =
            c.st.trusted
                .iter()
                .filter(|d| d.id != id.device_id)
                .cloned()
                .collect();
        if !c.st.joined || others.is_empty() {
            return Ok(());
        }
        let fp = keys.fingerprint();
        let st = RevokeStatement {
            user_id: self.cfg.user_id,
            revoker_fp: fp.clone(),
            target_id: id.device_id.clone(),
            target_fp: fp.clone(),
            issued_at_ms: self.now(),
        };
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
