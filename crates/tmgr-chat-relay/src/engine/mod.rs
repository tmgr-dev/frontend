//! Relay engine: trust, seq, dedupe, join, intros and revocations of the zero-knowledge chat relay.
//! One mutex guards all state, so there is a single writer; transport calls happen under it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tmgr_ipc::chat::{new_message_id, valid_message_id, AuthorKind, ChatMessage, RoomRef};
use tmgr_ipc::mgr_rpc::{limits, KIND_REQ, KIND_RES};
use tmgr_phone::{b64, b64_decode, h};

use crate::api::{self, Directory, Msg, Request};
use crate::fields::{check_device_id, check_device_kind, check_fp};
use crate::keys::{fingerprint, DeviceKeys};
use crate::rooms::{valid_index_msg_id, RoomsIndex};
use crate::trust::{
    decode_join_code, encode_join_code, gate_blockers, is_owner_device, join_mac, kind_key,
    lowers_kind, may_revoke, parse_intro, parse_revoke, restrict_to_before, sign_intro,
    sign_revoke, trusted_set_by_kind, v2_self_markers, verify_join_mac, verify_revoke,
    verify_spawnkey, DirEntry, IntroStatement, JoinCode, JoinMacInput, KindCtx, RevokeStatement,
    SpawnKey, Trusted,
};
use crate::wire::{open_message_checked, random_msg_id, seal_message, Opened, Peer, SealParams};
use tmgr_phone::Error;

pub use ports::{
    ActivitySubSink, ChatSink, KeyStore, MemKeyStore, PermitOkSink, RoomsSink, RpcSink, Slot,
    SpawnKeySink, SpawnOkSink, Transport,
};
pub use trust_state::{
    DeviceState, PendingJoin, PendingRevoke, SeqGap, TrustAlert, TrustDevice, TrustState,
};

pub mod blob;
mod permit;
pub mod ports;
pub mod trust_state;

const JOIN_TTL_MS: u64 = 10 * 60 * 1000;
const SEEN_TTL_MS: u64 = 31 * 24 * 3600 * 1000;
const MAX_SEEN: usize = 5000;
const MAX_RPC_SEEN: usize = 2000;
const CLOCK_WINDOW_MS: u64 = 30 * 60 * 1000;
const MAX_PEER_OFFSET_MS: i64 = 24 * 3600 * 1000;
const ROOMS_MAX_BYTES: usize = 256 * 1024;
const ACTIVITY_MAX_BYTES: usize = 48 * 1024;
const ACTIVITY_ON_MAX_AGE_MS: u64 = 5 * 60 * 1000;
const MAX_GAPS: usize = 100;
const MAX_HELD: usize = 200;
const HOLD_MS: u64 = 15_000;
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

pub const RECIPIENT_FULL_MSG: &str = "the recipient's relay queue is full";
pub const RECIPIENT_UNKNOWN_MSG: &str = "the recipient device is not on the relay";

/// Transport errors are the response body of a failed call; for a 429 with Retry-After the transport adds `"retry_after": <secs>`.
pub fn post_error(e: String) -> String {
    let v: Value = serde_json::from_str(&e).unwrap_or(Value::Null);
    match (v["error"].as_str(), v["retry_after"].as_u64()) {
        (Some("user_full"), _) => "relay storage is full".into(),
        (Some("recipient_full"), _) => RECIPIENT_FULL_MSG.into(),
        (Some("recipient_unknown"), _) => RECIPIENT_UNKNOWN_MSG.into(),
        (_, Some(secs)) => format!("{BUSY_PREFIX}{secs}s"),
        _ => e,
    }
}

fn per_recipient(e: &str) -> bool {
    e == RECIPIENT_FULL_MSG || e == RECIPIENT_UNKNOWN_MSG
}

pub fn http_status(e: &str) -> Option<u64> {
    serde_json::from_str::<Value>(e)
        .ok()?
        .get("http_status")?
        .as_u64()
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

#[derive(Clone, Default, Serialize, Deserialize)]
struct PeerClock {
    cur: i64,
    until: u64,
    last: u64,
    next: Option<i64>,
}

impl PeerClock {
    fn sample(sent_at: u64, now: u64) -> i64 {
        (i128::from(sent_at) - i128::from(now)).clamp(
            -i128::from(MAX_PEER_OFFSET_MS),
            i128::from(MAX_PEER_OFFSET_MS),
        ) as i64
    }

    fn first(d: i64, now: u64) -> Self {
        PeerClock {
            cur: d,
            until: now.saturating_add(CLOCK_WINDOW_MS),
            last: now,
            next: None,
        }
    }

    fn observe(&mut self, d: i64, now: u64) {
        let window_end = now.saturating_add(CLOCK_WINDOW_MS);
        if now.saturating_sub(self.last) > CLOCK_WINDOW_MS {
            self.until = window_end;
            self.next = None;
        }
        self.last = now;
        if d >= self.cur {
            self.cur = d;
            self.until = window_end;
            self.next = None;
        } else {
            self.next = Some(self.next.map_or(d, |n| n.max(d)));
            if now >= self.until {
                self.cur = self.next.take().unwrap_or(d);
                self.until = window_end;
            }
        }
    }
}

fn sender_kind<'a>(c: &'a Core, from: &str, from_fp: &str) -> Option<&'a str> {
    if c.st.pending_revokes.iter().any(|p| p.target_fp == from_fp) {
        return None;
    }
    c.st.kind_of(from, from_fp)
}

fn to_local_ms(ts_ms: u64, offset: i64) -> u64 {
    (i128::from(ts_ms) - i128::from(offset)).clamp(0, i128::from(u64::MAX)) as u64
}

fn trim_seen(seen: &mut Vec<Seen>, cap: usize, floor: &mut BTreeMap<String, u64>) {
    if seen.len() > cap {
        let cut = seen.len() - cap;
        for s in seen.drain(..cut).filter(|s| !s.id.starts_with("u_")) {
            let f = floor.entry(s.from).or_insert(0);
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rpc_seen: Vec<Seen>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    seen_floor: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    rpc_seen_floor: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    clocks: BTreeMap<String, PeerClock>,
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

    fn drop_untrusted_bindings(&mut self) {
        let trusted: BTreeSet<&str> = self.trusted.iter().map(|d| d.id.as_str()).collect();
        self.bindings.retain(|_, d| trusted.contains(d.as_str()));
    }

    fn superseded(&self, bound: &str) -> bool {
        let Some(d) = self.trusted.iter().find(|d| d.id == bound) else {
            return false;
        };
        self.pending_revokes.iter().any(|p| {
            p.target_id == d.id
                && p.target_fp == d.fp
                && p.received_at_ms > 0
                && self
                    .last_seen
                    .get(bound)
                    .is_none_or(|&s| s < p.received_at_ms)
                && self.trusted.iter().any(|t| t.fp == p.revoker_fp)
        })
    }

    fn stamp_pending_revokes(&mut self, now: u64) {
        for p in self
            .pending_revokes
            .iter_mut()
            .filter(|p| p.received_at_ms == 0)
        {
            p.received_at_ms = now;
        }
    }

    fn pinned_for_other_fp(&self, id: &str, fp: &str) -> bool {
        let prefix = kind_key(id, "");
        let own = kind_key(id, fp);
        self.kinds
            .keys()
            .any(|k| k.starts_with(&prefix) && *k != own)
    }
}

const RECEIPT_REASON_MAX: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReceiptState {
    Delivered,
    Queued,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Receipt {
    pub id: String,
    pub state: ReceiptState,
    #[serde(default)]
    pub reason: Option<String>,
}

const SENT_KEEP: usize = 200;
const MAX_PROBES: usize = 64;
const MAX_RECEIPTS: usize = 256;
const MAX_PROBES_PER_SENDER: usize = 8;

fn permanent_failure(e: &str) -> bool {
    const LOCAL: [&str; 3] = [
        "invalid message id",
        "the target is not a trusted owner device",
        "this device is not a trusted member of the chat yet",
    ];
    LOCAL.contains(&e)
        || serde_json::from_str::<Value>(e)
            .ok()
            .and_then(|v| v["http_status"].as_u64())
            .is_some_and(|s| (400..500).contains(&s) && s != 429)
}

fn push_probe(c: &mut Core, from: String, id: String) {
    let mine = c.probes.iter().filter(|p| p.0 == from).count();
    if c.probes.len() < MAX_PROBES && mine < MAX_PROBES_PER_SENDER {
        c.probes.push((from, id));
    }
}

fn push_receipt(c: &mut Core, r: Receipt) {
    if let Some(have) = c.receipts.iter_mut().find(|x| x.id == r.id) {
        if have.state == ReceiptState::Queued && r.state != ReceiptState::Queued {
            *have = r;
        }
        return;
    }
    if c.receipts.len() >= MAX_RECEIPTS {
        c.receipts.remove(0);
    }
    c.receipts.push(r);
}

impl ReceiptState {
    fn wire(self) -> &'static str {
        match self {
            Self::Delivered => "delivered",
            Self::Queued => "queued",
            Self::Failed => "failed",
        }
    }

    fn letter(self) -> char {
        match self {
            Self::Delivered => 'd',
            Self::Queued => 'q',
            Self::Failed => 'f',
        }
    }
}

#[derive(Default)]
struct Hold {
    stuck: Option<u64>,
    blocked: BTreeSet<String>,
    held: usize,
    sink_err: Option<String>,
    drops: Vec<(u64, &'static str)>,
}

enum Inbound {
    Opened(Opened),
    Consumed { from: String, seq: u64 },
}

impl Inbound {
    fn key(&self) -> (&str, u64) {
        match self {
            Inbound::Opened(o) => (&o.from, o.seq),
            Inbound::Consumed { from, seq } => (from, *seq),
        }
    }
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
    written_cursor: u64,
    acked: u64,
    hold_since: BTreeMap<String, u64>,
    pass_cap: Option<usize>,
    drop_reason: Option<&'static str>,
    sent: Vec<(String, String)>,
    receipts: Vec<Receipt>,
    probes: Vec<(String, String)>,
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

fn revoker_kind<'a>(c: &'a Core, revoker: &Dev) -> Option<&'a str> {
    match c.st.kind_of(&revoker.id, &revoker.fp) {
        None if c
            .st
            .base
            .iter()
            .any(|d| d.id == revoker.id && d.fp == revoker.fp) =>
        {
            Some("desktop")
        }
        kind => kind,
    }
}

fn revoked_known(c: &Core) -> bool {
    let Some(fp) = c.keys.as_ref().map(|k| k.fingerprint()) else {
        return false;
    };
    c.st.revoked.contains(&fp) || c.st.pending_revokes.iter().any(|r| r.target_fp == fp)
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
            received_at_ms: now,
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
    permitok: Option<Arc<dyn PermitOkSink>>,
    activity_sub: Option<Arc<dyn ActivitySubSink>>,
    rooms: Option<Arc<dyn RoomsSink>>,
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
            core.written_cursor = core.st.cursor;
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
            permitok: None,
            activity_sub: None,
            rooms: None,
            core: Mutex::new(core),
        })
    }

    fn cannot_vouch(&self) -> bool {
        matches!(self.kind.as_str(), "runner" | "terminal")
    }

    pub fn with_kind(mut self, kind: &str) -> Self {
        self.kind = kind.to_owned();
        let c = self.core.get_mut().unwrap_or_else(|p| p.into_inner());
        if matches!(kind, "runner" | "terminal") {
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

    pub fn with_permitok_sink(mut self, sink: Arc<dyn PermitOkSink>) -> Self {
        self.permitok = Some(sink);
        self
    }

    pub fn with_activity_sub_sink(mut self, sink: Arc<dyn ActivitySubSink>) -> Self {
        self.activity_sub = Some(sink);
        self
    }

    pub fn with_rooms_sink(mut self, sink: Arc<dyn RoomsSink>) -> Self {
        self.rooms = Some(sink);
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

    /// Poll-time writes are throttled unless trust changed; a message is acked only once the state that dedupes it and the cursor past it are stored.
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
            c.written_cursor = c.st.cursor;
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
        let mut delivered = false;
        let mut skipped: Option<String> = None;
        for chunk in items.chunks(api::MAX_ITEMS) {
            match self.post_items(c, msg_id, push, chunk) {
                Ok(()) => {
                    for (to, _, seq) in chunk {
                        c.st.seq_out.insert(to.clone(), *seq);
                    }
                    delivered = true;
                    continue;
                }
                Err(e) if per_recipient(&e) && chunk.len() > 1 => {}
                Err(e) => return Err(e),
            }
            for item in chunk {
                match self.post_items(c, msg_id, push, std::slice::from_ref(item)) {
                    Ok(()) => {
                        c.st.seq_out.insert(item.0.clone(), item.2);
                        delivered = true;
                    }
                    Err(e) if per_recipient(&e) => {
                        let kind = targets
                            .iter()
                            .find(|t| t.id == item.0)
                            .map(|t| c.st.kind_of(&t.id, &t.fp).unwrap_or(&t.kind).to_owned())
                            .unwrap_or_default();
                        if !is_owner_device(Some(kind.as_str())) {
                            return Err(e);
                        }
                        if e == RECIPIENT_FULL_MSG {
                            self.s.skipped_full(&item.0, &kind);
                        }
                        skipped = Some(e);
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        if let Some(e) = skipped.filter(|_| !delivered) {
            return Err(e);
        }
        Ok(())
    }

    fn post_items(
        &self,
        c: &Core,
        msg_id: &str,
        push: bool,
        items: &[(String, String, u64)],
    ) -> Result<(), String> {
        let refs: Vec<(&str, &str)> = items
            .iter()
            .map(|(to, body, _)| (to.as_str(), body.as_str()))
            .collect();
        let req = api::post_messages(msg_id, push, &refs).map_err(err)?;
        api::parse_posted(&self.call(c, req).map_err(post_error)?).map_err(err)?;
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
        if self.cannot_vouch() {
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
        if self.cannot_vouch() {
            return Err(format!("a {} cannot issue join codes", self.kind));
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

    fn open_one(&self, c: &Core, m: &Msg, now: u64) -> Result<Inbound, Option<&'static str>> {
        if c.st
            .seen
            .iter()
            .any(|s| s.from == m.from && s.id == m.msg_id)
        {
            return Err(None);
        }
        let kind_hint = serde_json::from_str::<Value>(&m.body)
            .ok()
            .and_then(|v| v.get("kind")?.as_str().map(str::to_owned));
        let join_open = kind_hint.as_deref() == Some("join");
        let Some(dir) = c.dir.as_ref() else {
            return Err(None);
        };
        let Ok((id, keys)) = Self::own(c) else {
            return Err(None);
        };
        let trusted: Vec<Trusted> = c.st.trusted.iter().map(to_trusted).collect();
        let opened = open_message_checked(
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
        );
        match opened {
            Ok(o) => Ok(Inbound::Opened(o)),
            Err(r) => match r.verified {
                Some((from, seq)) => Ok(Inbound::Consumed { from, seq }),
                None => Err(Some(match r.error {
                    Error::BadKey
                        if !join_open && !trusted.iter().any(|t| t.device_id == m.from) =>
                    {
                        "unknown sender"
                    }
                    Error::BadKey | Error::IdMismatch | Error::BadSignature => {
                        "bad signature or key"
                    }
                    _ => "undecryptable or malformed",
                })),
            },
        }
    }

    fn apply_rpc(&self, c: &mut Core, o: &Opened, now: u64, offset: i64) {
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
            || c.st
                .rpc_seen_floor
                .get(&o.from)
                .is_some_and(|f| o.sent_at <= *f)
            || serde_json::to_vec(&o.payload).map_or(true, |b| b.len() > cap)
        {
            return;
        }
        c.st.rpc_seen.push(Seen {
            from: o.from.clone(),
            id: o.id.clone(),
            at: o.sent_at,
        });
        c.st.rpc_seen
            .retain(|s| s.at.saturating_add(SEEN_TTL_MS) >= now);
        trim_seen(&mut c.st.rpc_seen, MAX_RPC_SEEN, &mut c.st.rpc_seen_floor);
        let mut payload = o.payload.clone();
        if o.kind == KIND_REQ {
            if let Some(ts) = payload["ts_ms"].as_u64() {
                payload["ts_ms"] = to_local_ms(ts, offset).into();
            }
        }
        sink.deliver_rpc(&o.from, &o.from_fp, &o.kind, &o.id, payload);
    }

    fn apply(
        &self,
        c: &mut Core,
        opened: Opened,
        now: u64,
        offset: i64,
    ) -> Result<Option<ChatMessage>, String> {
        if opened.kind == KIND_REQ || opened.kind == KIND_RES {
            self.apply_rpc(c, &opened, now, offset);
            return Ok(None);
        }
        if opened.kind == "blob" {
            return self.apply_blob(c, &opened).map(|()| None);
        }
        if opened.trusted
            && !opened.id.starts_with("u_")
            && c.st
                .seen_floor
                .get(&opened.from)
                .is_some_and(|f| opened.sent_at <= *f)
        {
            c.drop_reason = Some("replayed or too old");
            return Ok(None);
        }
        let Some(mut chat) = self.handle(c, &opened, now) else {
            return Ok(None);
        };
        if let Some(msg) = &mut chat {
            msg.ts_ms = to_local_ms(msg.ts_ms, offset);
        }
        if let Some(msg) = &chat {
            let from_kind = sender_kind(c, &opened.from, &opened.from_fp);
            self.s.deliver(&opened.from, from_kind, msg.clone())?;
            c.unsaved = true;
        }
        c.st.seen.push(Seen {
            from: opened.from,
            id: opened.id,
            at: opened.sent_at,
        });
        c.st.seen.retain(|s| {
            let ttl = if s.id.starts_with("u_") {
                ACTIVITY_ON_MAX_AGE_MS
            } else {
                SEEN_TTL_MS
            };
            s.at.saturating_add(ttl) >= now
        });
        trim_seen(&mut c.st.seen, MAX_SEEN, &mut c.st.seen_floor);
        Ok(chat)
    }

    fn authorized_author(c: &mut Core, from: &str, msg: &ChatMessage, now: u64) -> bool {
        let kind = c.st.kind_of_trusted(from).map(str::to_owned);
        match msg.author.kind {
            AuthorKind::Owner => matches!(kind.as_deref(), Some("desktop" | "mobile" | "terminal")),
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
                Self::claim_machine(c, from, &msg.room.machine, now)
            }
        }
    }

    fn claim_machine(c: &mut Core, from: &str, machine: &str, now: u64) -> bool {
        c.st.drop_untrusted_bindings();
        let bound_elsewhere = c.st.bindings.iter().any(|(m, d)| d == from && m != machine);
        match c.st.bindings.get(machine) {
            Some(d) if d == from => true,
            Some(d) if !bound_elsewhere && c.st.superseded(d) && !c.st.superseded(from) => {
                c.st.bindings.insert(machine.to_owned(), from.to_owned());
                true
            }
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
                true
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
                    c.drop_reason = Some("malformed chat payload");
                    return None;
                };
                if msg.validate().is_err() || msg.id != opened.id {
                    c.drop_reason = Some("malformed chat payload");
                    return None;
                }
                if !self.owns(&msg.room) {
                    c.drop_reason = Some("room not owned by this machine");
                    return None;
                }
                if !Self::authorized_author(c, &opened.from, &msg, now) {
                    c.drop_reason = Some("unauthorized author");
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
            "receipt" if sender_trusted && c.st.joined => {
                if c.st.kind_of_trusted(&opened.from) != Some("runner") {
                    return None;
                }
                let receipt = serde_json::from_value::<Receipt>(opened.payload.clone())
                    .ok()
                    .filter(|r| {
                        valid_message_id(&r.id)
                            && r.reason
                                .as_ref()
                                .is_none_or(|x| x.chars().count() <= RECEIPT_REASON_MAX)
                    })?;
                if !c
                    .sent
                    .iter()
                    .any(|(id, runner)| *id == receipt.id && *runner == opened.from)
                {
                    return None;
                }
                push_receipt(c, receipt);
            }
            "probe" if sender_trusted && c.st.joined && self.kind == "runner" => {
                c.st.trusted.iter().find(|d| d.id == opened.from)?;
                let from_kind = sender_kind(c, &opened.from, &opened.from_fp);
                if !self.s.accepts_owner_input(from_kind) || !valid_message_id(&opened.id) {
                    return None;
                }
                push_probe(c, opened.from.clone(), opened.id.clone());
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
                let listed = dir
                    .devices
                    .iter()
                    .find(|d| d.device_id == root.id)
                    .map(|d| d.kind.as_str());
                if [kind, listed]
                    .into_iter()
                    .flatten()
                    .any(|k| matches!(k, "runner" | "terminal"))
                {
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
                if c.st
                    .kind_of(&dev.id, &dev.fp)
                    .is_some_and(|k| k != "mobile")
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
            "permitok" if sender_trusted && c.st.joined => {
                self.accept_permitok(c, opened)?;
            }
            "rooms" if sender_trusted && c.st.joined => {
                let sink = self.rooms.as_ref()?;
                if self.kind == "runner"
                    || !valid_index_msg_id(&opened.id)
                    || c.st.kind_of_trusted(&opened.from) != Some("runner")
                {
                    return None;
                }
                let index = RoomsIndex::parse(opened.payload.clone()).ok()?;
                if !Self::claim_machine(c, &opened.from, &index.machine, now) {
                    return None;
                }
                sink.deliver_rooms(&opened.from, index);
            }
            "activity_on" if sender_trusted && c.st.joined => {
                let sink = self.activity_sub.as_ref()?;
                if self.kind != "runner" {
                    return None;
                }
                let own = c.identity.as_ref().map(|i| i.device_id.as_str());
                let dev = c.st.trusted.iter().find(|d| d.id == opened.from)?;
                if Some(dev.id.as_str()) == own
                    || dev.fp != opened.from_fp
                    || c.st.revoked.contains(&dev.fp)
                    || c.st.kind_of(&dev.id, &dev.fp) != Some("mobile")
                    || now.saturating_sub(opened.sent_at) > ACTIVITY_ON_MAX_AGE_MS
                {
                    return None;
                }
                let obj = opened.payload.as_object()?;
                if obj.get("v").and_then(Value::as_u64) != Some(1) {
                    return None;
                }
                let on = obj.get("on")?.as_bool()?;
                sink.deliver_activity_on(&dev.id, on);
            }
            "chat" => {
                c.drop_reason = Some(if sender_trusted {
                    "this device has not joined"
                } else {
                    "sender is not a trusted member"
                });
                return None;
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
        let mut opened: Vec<(u64, Inbound)> = Vec::new();
        for m in msgs {
            let cursor = m.cursor as u64;
            match self.open_one(c, m, now) {
                Ok(i) => {
                    if matches!(i, Inbound::Consumed { .. }) {
                        hold.drops.push((cursor, "outside the time window"));
                    }
                    opened.push((cursor, i));
                }
                Err(Some(reason)) => hold.drops.push((cursor, reason)),
                Err(None) => {}
            }
        }
        opened.sort_by(|a, b| (a.1.key(), a.0).cmp(&(b.1.key(), b.0)));
        for (_, i) in &opened {
            if let Inbound::Opened(o) = i {
                if o.trusted {
                    c.st.last_seen.insert(o.from.clone(), now);
                }
            }
        }
        for (cursor, item) in opened {
            let (from, seq, trusted) = match &item {
                Inbound::Opened(o) => (o.from.clone(), o.seq, o.trusted),
                Inbound::Consumed { from, seq } => (from.clone(), *seq, true),
            };
            let mut defer = hold.blocked.contains(&from);
            if !defer && trusted {
                if let Some(expected) = c.st.seq_in.get(&from).map(|l| l + 1).filter(|e| seq > *e) {
                    let since = *c.hold_since.entry(from.clone()).or_insert(now);
                    if now.saturating_sub(since) < HOLD_MS && hold.held < MAX_HELD {
                        defer = true;
                    } else {
                        c.st.seq_gaps.push(SeqGap {
                            from_device: from.clone(),
                            expected,
                            got: seq,
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
            let prev_seq = c.st.seq_in.get(&from).copied();
            let entry = c.st.seq_in.entry(from.clone()).or_insert(0);
            *entry = (*entry).max(seq);
            let Inbound::Opened(o) = item else {
                continue;
            };
            c.drop_reason = None;
            let sample = o.trusted.then(|| PeerClock::sample(o.sent_at, now));
            let offset = c.st.clocks.get(&from).map_or(0, |p| p.cur);
            let applied = self.apply(c, o, now, offset);
            let dropped = c.drop_reason.take();
            if let Some(reason) = dropped {
                hold.drops.push((cursor, reason));
            }
            match applied {
                Ok(delivered) => {
                    out.extend(delivered);
                    if let Some(d) = sample.filter(|_| dropped.is_none()) {
                        c.st.clocks
                            .entry(from.clone())
                            .and_modify(|p| p.observe(d, now))
                            .or_insert_with(|| PeerClock::first(d, now));
                        let trusted = &c.st.trusted;
                        c.st.clocks
                            .retain(|k, _| trusted.iter().any(|t| t.id == *k));
                    }
                }
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

    pub fn owner_devices(&self) -> Vec<String> {
        let g = self.lock();
        g.st.trusted
            .iter()
            .filter(|d| is_owner_device(g.st.kind_of(&d.id, &d.fp)))
            .filter(|d| !g.st.pending_revokes.iter().any(|p| p.target_fp == d.fp))
            .map(|d| d.id.clone())
            .collect()
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
        let runner = self.cannot_vouch();
        let targets: Vec<Dev> =
            c.st.trusted
                .iter()
                .filter(|d| d.id != id.device_id)
                .filter(|d| !runner || is_owner_device(c.st.kind_of(&d.id, &d.fp)))
                .filter(|d| !runner || !c.st.pending_revokes.iter().any(|p| p.target_fp == d.fp))
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

    fn owner_targets(c: &Core, own_id: &str, only: Option<&[String]>) -> Vec<Dev> {
        c.st.trusted
            .iter()
            .filter(|d| d.id != own_id)
            .filter(|d| is_owner_device(c.st.kind_of(&d.id, &d.fp)))
            .filter(|d| !c.st.pending_revokes.iter().any(|p| p.target_fp == d.fp))
            .filter(|d| only.is_none_or(|o| o.contains(&d.id)))
            .cloned()
            .collect()
    }

    pub fn owner_device_ids(&self) -> Result<Vec<String>, String> {
        let g = self.lock();
        let Some(own_id) = g.identity.as_ref().map(|i| i.device_id.clone()) else {
            return Ok(Vec::new());
        };
        if !g.st.joined {
            return Ok(Vec::new());
        }
        Ok(Self::owner_targets(&g, &own_id, None)
            .into_iter()
            .map(|d| d.id)
            .collect())
    }

    pub fn active_owner_device_ids(&self, max_idle_ms: u64) -> Result<Vec<String>, String> {
        let owners = self.owner_device_ids()?;
        let g = self.lock();
        let now = self.now();
        let mut ids = Vec::new();
        for id in owners {
            let seen = g
                .dir
                .as_ref()
                .and_then(|d| d.devices.iter().find(|x| x.device_id == id))
                .and_then(|d| parse_seen(&d.last_seen_at));
            if seen.is_none_or(|t| now.saturating_sub(t) <= max_idle_ms) {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    pub fn send_sealed_to_owners(
        &self,
        kind: &str,
        msg_id: &str,
        payload: &Value,
        only: Option<&[String]>,
    ) -> Result<(), String> {
        if kind != "rooms" || !msg_id.starts_with("i_") {
            return Err("unsupported sealed kind".into());
        }
        if self.kind != "runner" {
            return Err("only a runner can send the rooms index".into());
        }
        if serde_json::to_vec(payload).map_err(err)?.len() > ROOMS_MAX_BYTES {
            return Err("the rooms payload is too large".into());
        }
        let mut g = self.lock();
        let c = &mut *g;
        self.ensure_registered(c)?;
        let (id, _) = Self::own(c)?;
        if !c.st.joined || !c.st.trusted.iter().any(|d| d.id == id.device_id) {
            return Err("this device is not a trusted member of the chat yet".into());
        }
        let targets = Self::owner_targets(c, &id.device_id, only);
        if targets.is_empty() {
            return Err("there are no other trusted devices to send to".into());
        }
        self.seal_and_post(c, kind, msg_id, payload, &targets, false)?;
        self.save_now(c)
    }

    pub fn send_activity(
        &self,
        msg_id: &str,
        payload: &Value,
        only: &[String],
    ) -> Result<(), String> {
        if !msg_id.starts_with("t_") {
            return Err("unsupported sealed kind".into());
        }
        if self.kind != "runner" {
            return Err("only a runner can send activity".into());
        }
        if serde_json::to_vec(payload).map_err(err)?.len() > ACTIVITY_MAX_BYTES {
            return Err("the activity payload is too large".into());
        }
        let mut g = self.lock();
        let c = &mut *g;
        self.ensure_registered(c)?;
        let (id, _) = Self::own(c)?;
        if !c.st.joined || !c.st.trusted.iter().any(|d| d.id == id.device_id) {
            return Err("this device is not a trusted member of the chat yet".into());
        }
        let targets: Vec<Dev> = Self::owner_targets(c, &id.device_id, Some(only))
            .into_iter()
            .filter(|d| c.st.kind_of(&d.id, &d.fp) == Some("mobile"))
            .collect();
        if targets.is_empty() {
            return Err("there are no other trusted devices to send to".into());
        }
        self.seal_and_post(c, "activity", msg_id, payload, &targets, false)?;
        self.save_now(c)
    }

    pub fn send_receipt(
        &self,
        to_device: &str,
        id: &str,
        state: ReceiptState,
        reason: Option<&str>,
    ) -> Result<(), String> {
        let mut g = self.lock();
        self.receipt_locked(&mut g, to_device, id, state, reason)
    }

    fn receipt_locked(
        &self,
        c: &mut Core,
        to_device: &str,
        id: &str,
        state: ReceiptState,
        reason: Option<&str>,
    ) -> Result<(), String> {
        if !valid_message_id(id) {
            return Err("invalid message id".into());
        }
        let mut payload = json!({ "id": id, "state": state.wire() });
        if let (ReceiptState::Failed | ReceiptState::Queued, Some(r)) =
            (state, reason.filter(|r| !r.is_empty()))
        {
            payload["reason"] = json!(r.chars().take(RECEIPT_REASON_MAX).collect::<String>());
        }
        let msg_id = format!("rc_{}_{}", &h(id)[..24], state.letter());
        self.ensure_registered(c)?;
        let (own, _) = Self::own(c)?;
        if !c.st.joined || !c.st.trusted.iter().any(|d| d.id == own.device_id) {
            return Err("this device is not a trusted member of the chat yet".into());
        }
        let target = Self::owner_targets(c, &own.device_id, Some(&[to_device.to_owned()]))
            .into_iter()
            .find(|d| is_owner_device(c.st.kind_of(&d.id, &d.fp)))
            .ok_or_else(|| "the target is not a trusted owner device".to_owned())?;
        self.seal_and_post(c, "receipt", &msg_id, &payload, &[target], false)?;
        self.save_now(c)
    }

    pub fn send_probes(&self) -> Result<Vec<(String, String)>, String> {
        if !is_owner_device(Some(self.kind.as_str())) {
            return Err(format!("a {} cannot probe runners", self.kind));
        }
        let mut g = self.lock();
        let c = &mut *g;
        self.ensure_registered(c)?;
        let (id, _) = Self::own(c)?;
        if !c.st.joined || !c.st.trusted.iter().any(|d| d.id == id.device_id) {
            return Err("this device is not a trusted member of the chat yet".into());
        }
        let runners: Vec<Dev> =
            c.st.trusted
                .iter()
                .filter(|d| c.st.kind_of_trusted(&d.id) == Some("runner"))
                .filter(|d| !c.st.pending_revokes.iter().any(|p| p.target_fp == d.fp))
                .cloned()
                .collect();
        if runners.is_empty() {
            return Err("there are no trusted runners to probe".into());
        }
        let mut sent = Vec::new();
        for runner in runners {
            let probe = new_message_id();
            self.seal_and_post(
                c,
                "probe",
                &probe,
                &json!({ "v": 1 }),
                std::slice::from_ref(&runner),
                false,
            )?;
            c.sent.push((probe.clone(), runner.id.clone()));
            if c.sent.len() > SENT_KEEP {
                c.sent.remove(0);
            }
            sent.push((runner.id, probe));
        }
        self.save_now(c)?;
        Ok(sent)
    }

    fn answer_probes(&self, c: &mut Core) {
        let pending = std::mem::take(&mut c.probes);
        for (from, id) in pending {
            if !c.st.trusted.iter().any(|d| d.id == from) {
                continue;
            }
            if let Err(e) = self.receipt_locked(c, &from, &id, ReceiptState::Delivered, None) {
                if !permanent_failure(&e) {
                    push_probe(c, from, id);
                }
            }
        }
    }

    pub fn send_chat_to_owners(&self, msg: &ChatMessage, only: &[String]) -> Result<(), String> {
        msg.validate()?;
        let mut g = self.lock();
        let c = &mut *g;
        self.ensure_registered(c)?;
        let (id, _) = Self::own(c)?;
        if !c.st.joined || !c.st.trusted.iter().any(|d| d.id == id.device_id) {
            return Err("this device is not a trusted member of the chat yet".into());
        }
        let targets = Self::owner_targets(c, &id.device_id, Some(only));
        if targets.is_empty() {
            return Err("there are no other trusted devices to send to".into());
        }
        let payload = serde_json::to_value(msg).map_err(err)?;
        self.seal_and_post(c, "chat", &msg.id, &payload, &targets, false)?;
        self.save_now(c)
    }

    pub fn send_chat_for_machine(&self, msg: &ChatMessage, push: bool) -> Result<(), String> {
        msg.validate()?;
        let mut g = self.lock();
        let c = &mut *g;
        self.ensure_registered(c)?;
        let (id, _) = Self::own(c)?;
        if !c.st.joined || !c.st.trusted.iter().any(|d| d.id == id.device_id) {
            return Err("this device is not a trusted member of the chat yet".into());
        }
        let runner =
            c.st.bindings
                .get(&msg.room.machine)
                .filter(|d| c.st.kind_of_trusted(d) == Some("runner"))
                .cloned()
                .ok_or_else(|| "no runner of that machine has been seen yet".to_owned())?;
        let revoking =
            c.st.trusted
                .iter()
                .filter(|d| d.id == runner)
                .any(|d| c.st.pending_revokes.iter().any(|p| p.target_fp == d.fp));
        if revoking {
            return Err("the runner of that machine is being revoked".into());
        }
        let mut targets = Self::owner_targets(c, &id.device_id, None);
        targets.extend(c.st.trusted.iter().filter(|d| d.id == runner).cloned());
        let payload = serde_json::to_value(msg).map_err(err)?;
        self.seal_and_post(c, "chat", &msg.id, &payload, &targets, push)?;
        c.sent.push((msg.id.clone(), runner));
        if c.sent.len() > SENT_KEEP {
            c.sent.remove(0);
        }
        self.save_now(c)
    }

    pub fn take_receipts(&self) -> Vec<Receipt> {
        std::mem::take(&mut self.lock().receipts)
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
        if self.cannot_vouch() {
            return Err(format!("a {} cannot approve joins", self.kind));
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
        if self.cannot_vouch() {
            return Err(format!("a {} cannot confirm device kinds", self.kind));
        }
        if !matches!(kind, "desktop" | "mobile") {
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
            if self.cannot_vouch() {
                return Err(format!("a {} cannot accept revokes", self.kind));
            }
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
        if self.cannot_vouch() {
            return Err(format!("a {} cannot revoke other devices", self.kind));
        }
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
        if let Err(e) = self.call(c, api::delete_device(&target.id).map_err(err)?) {
            log::warn!(
                "[chat-relay] the device was revoked, but its relay registration was not removed: {e}"
            );
        }
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

    fn confirmed_absent(&self, c: &mut Core, id: &str) -> bool {
        match self.fetch_dir(c) {
            Ok(dir) => !dir.devices.iter().any(|d| d.device_id == id),
            Err(e) => http_status(&e) == Some(401),
        }
    }

    pub fn own_removed(&self) -> Result<bool, String> {
        let mut g = self.lock();
        let c = &mut *g;
        let Some(id) = c.identity.as_ref().map(|i| i.device_id.clone()) else {
            return Ok(false);
        };
        if revoked_known(c) {
            return Ok(true);
        }
        match self.sync_dir(c) {
            Ok(()) => {
                let listed = c
                    .dir
                    .as_ref()
                    .is_some_and(|d| d.devices.iter().any(|x| x.device_id == id));
                Ok(!listed || revoked_known(c))
            }
            Err(e) if http_status(&e) == Some(401) => Ok(true),
            Err(e) => Err(e),
        }
    }

    pub fn own_removed_known(&self) -> bool {
        let g = self.lock();
        let c = &*g;
        let Some(id) = c.identity.as_ref().map(|i| i.device_id.as_str()) else {
            return false;
        };
        revoked_known(c)
            || c.dir
                .as_ref()
                .is_some_and(|d| !d.devices.iter().any(|x| x.device_id == id))
    }

    pub fn sent_log(&self) -> Vec<(String, String)> {
        self.lock().sent.clone()
    }

    pub fn restore_sent(&self, log: Vec<(String, String)>) {
        let mut g = self.lock();
        let skip = log.len().saturating_sub(SENT_KEEP);
        g.sent = log.into_iter().skip(skip).collect();
    }

    pub fn leave_confirmed(&self) -> Result<Option<String>, String> {
        let mut g = self.lock();
        let c = &mut *g;
        let notice = self.announce_reset(c);
        let told = notice.is_ok();
        if let Some(id) = c.identity.as_ref().map(|i| i.device_id.clone()) {
            api::delete_device(&id)
                .map_err(err)
                .and_then(|req| self.call(c, req))
                .or_else(|e| match http_status(&e) {
                    Some(401) => Ok(String::new()),
                    Some(404) if self.confirmed_absent(c, &id) => Ok(String::new()),
                    _ => Err(e),
                })
                .map_err(|e| {
                    if told {
                        format!("the relay did not confirm the removal ({e}); this device is already revoked for your other devices but its relay row is still there")
                    } else {
                        format!("the relay did not confirm the removal ({e}) and the other devices were not told")
                    }
                })?;
        }
        self.k.clear()?;
        *c = Core::default();
        Ok(notice
            .err()
            .map(|e| format!("the device was removed, but the other devices were not told: {e}")))
    }

    pub fn poll_once(&self) -> Result<Vec<ChatMessage>, String> {
        let mut g = self.lock();
        let c = &mut *g;
        self.ensure_registered(c)?;
        self.sync_dir(c)?;
        let now = self.now();
        c.st.stamp_pending_revokes(now);
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
            let passed = c.st.cursor;
            hold.drops.retain(|&(cursor, reason)| {
                let past = cursor <= passed;
                if past {
                    self.s.dropped(reason);
                }
                !past
            });
            self.ack_if_stored(c)?;
            if !inbox.more {
                break;
            }
        }
        c.hold_since.retain(|k, _| hold.blocked.contains(k));
        self.ack_if_stored(c)?;
        self.answer_probes(c);
        match hold.sink_err {
            Some(e) => Err(e),
            None => Ok(out),
        }
    }

    fn ack_if_stored(&self, c: &mut Core) -> Result<(), String> {
        self.persist(c, false)?;
        let through = c.st.cursor.min(c.written_cursor);
        if !c.unsaved && through > c.acked {
            self.call(c, api::ack_messages(through).map_err(err)?)?;
            c.acked = through;
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
