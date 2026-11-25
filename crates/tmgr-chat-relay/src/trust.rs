use crate::fields::{check_device_id, check_device_kind, check_fp, parse_decimal, parse_strict};
use crate::keys::{fingerprint, DeviceKeys};
use crate::{Error, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use subtle::ConstantTimeEq;
use tmgr_phone::{b64, b64_decode, h, pair_mac, sign, verify_der};

pub const JOIN_PREFIX: &str = "tmgr-chat-join:v1:";
pub const JOIN_PREFIX_V2: &str = "tmgr-chat-join:v2:";

fn parts<'a>(s: &'a str, tag: &str, count: usize) -> Result<Vec<&'a str>> {
    let p: Vec<&str> = s.split(':').collect();
    if p.len() != count || p[0] != tag || p[1] != "v1" {
        return Err(Error::BadFormat);
    }
    Ok(p)
}

fn check_pub(pub_b64: &str) -> Result<()> {
    let raw = b64_decode(pub_b64)?;
    if raw.len() == 65 && raw[0] == 4 {
        Ok(())
    } else {
        Err(Error::BadKey)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntroStatement {
    pub version: u8,
    pub user_id: u64,
    pub introducer_fp: String,
    pub target_id: String,
    pub target_fp: String,
    pub target_kind: String,
    pub issued_at_ms: u64,
}

impl IntroStatement {
    pub fn to_statement(&self) -> Result<String> {
        if !matches!(self.version, 1 | 2) {
            return Err(Error::BadFormat);
        }
        check_fp(&self.introducer_fp)?;
        check_device_id(&self.target_id)?;
        check_fp(&self.target_fp)?;
        check_device_kind(&self.target_kind)?;
        Ok(format!(
            "tmgr-chat-intro:v{}:{}:{}:{}:{}:{}:{}",
            self.version,
            self.user_id,
            self.introducer_fp,
            self.target_id,
            self.target_fp,
            self.target_kind,
            self.issued_at_ms
        ))
    }

    pub fn parse(s: &str) -> Result<IntroStatement> {
        let p: Vec<&str> = s.split(':').collect();
        let version = match (p.len(), p.first().copied(), p.get(1).copied()) {
            (8, Some("tmgr-chat-intro"), Some("v1")) => 1,
            (8, Some("tmgr-chat-intro"), Some("v2")) => 2,
            _ => return Err(Error::BadFormat),
        };
        let st = IntroStatement {
            version,
            user_id: parse_decimal(p[2])?,
            introducer_fp: p[3].to_string(),
            target_id: p[4].to_string(),
            target_fp: p[5].to_string(),
            target_kind: p[6].to_string(),
            issued_at_ms: parse_decimal(p[7])?,
        };
        st.to_statement()?;
        Ok(st)
    }

    pub fn is_self(&self, by: &str) -> bool {
        self.target_id == by && self.target_fp == self.introducer_fp
    }
}

#[derive(Serialize, Deserialize)]
struct IntroJson {
    v: u32,
    by: String,
    statement: String,
    sig: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Intro {
    pub by: String,
    pub statement: String,
    pub sig: String,
    pub parsed: IntroStatement,
}

pub fn sign_intro(signer: &DeviceKeys, signer_id: &str, st: &IntroStatement) -> Result<String> {
    check_device_id(signer_id)?;
    if st.introducer_fp != signer.fingerprint() {
        return Err(Error::BadKey);
    }
    let statement = st.to_statement()?;
    let sig = sign(signer.sign_secret(), &statement)?;
    serde_json::to_string(&IntroJson {
        v: u32::from(st.version),
        by: signer_id.to_string(),
        statement,
        sig,
    })
    .map_err(|_| Error::BadJson)
}

pub fn parse_intro(json: &str) -> Result<Intro> {
    let j: IntroJson = parse_strict(json)?;
    check_device_id(&j.by)?;
    let parsed = IntroStatement::parse(&j.statement)?;
    if j.v != u32::from(parsed.version) {
        return Err(Error::BadFormat);
    }
    Ok(Intro {
        by: j.by,
        statement: j.statement,
        sig: j.sig,
        parsed,
    })
}

pub fn verify_intro(
    json: &str,
    user_id: u64,
    signer_id: &str,
    signer_sign_pub_b64: &str,
    signer_kex_pub_b64: &str,
) -> Result<Intro> {
    let intro = parse_intro(json)?;
    if intro.parsed.user_id != user_id || intro.by != signer_id {
        return Err(Error::IdMismatch);
    }
    if fingerprint(signer_sign_pub_b64, signer_kex_pub_b64) != intro.parsed.introducer_fp {
        return Err(Error::BadKey);
    }
    verify_der(
        &b64_decode(signer_sign_pub_b64)?,
        &intro.statement,
        &intro.sig,
    )?;
    Ok(intro)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevokeStatement {
    pub user_id: u64,
    pub revoker_fp: String,
    pub target_id: String,
    pub target_fp: String,
    pub issued_at_ms: u64,
}

impl RevokeStatement {
    pub fn to_statement(&self) -> Result<String> {
        check_fp(&self.revoker_fp)?;
        check_device_id(&self.target_id)?;
        check_fp(&self.target_fp)?;
        Ok(format!(
            "tmgr-chat-revoke:v1:{}:{}:{}:{}:{}",
            self.user_id, self.revoker_fp, self.target_id, self.target_fp, self.issued_at_ms
        ))
    }

    pub fn parse(s: &str) -> Result<RevokeStatement> {
        let p = parts(s, "tmgr-chat-revoke", 7)?;
        let st = RevokeStatement {
            user_id: parse_decimal(p[2])?,
            revoker_fp: p[3].to_string(),
            target_id: p[4].to_string(),
            target_fp: p[5].to_string(),
            issued_at_ms: parse_decimal(p[6])?,
        };
        st.to_statement()?;
        Ok(st)
    }
}

#[derive(Serialize, Deserialize)]
struct RevokeJson {
    v: u32,
    statement: String,
    sig: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Revoke {
    pub statement: String,
    pub sig: String,
    pub parsed: RevokeStatement,
}

pub fn sign_revoke(signer: &DeviceKeys, st: &RevokeStatement) -> Result<String> {
    if st.revoker_fp != signer.fingerprint() {
        return Err(Error::BadKey);
    }
    let statement = st.to_statement()?;
    let sig = sign(signer.sign_secret(), &statement)?;
    serde_json::to_string(&RevokeJson {
        v: 1,
        statement,
        sig,
    })
    .map_err(|_| Error::BadJson)
}

pub fn parse_revoke(json: &str) -> Result<Revoke> {
    let j: RevokeJson = parse_strict(json)?;
    if j.v != 1 {
        return Err(Error::BadFormat);
    }
    let parsed = RevokeStatement::parse(&j.statement)?;
    Ok(Revoke {
        statement: j.statement,
        sig: j.sig,
        parsed,
    })
}

pub fn verify_revoke(
    json: &str,
    user_id: u64,
    revoker_sign_pub_b64: &str,
    revoker_kex_pub_b64: &str,
) -> Result<Revoke> {
    let r = parse_revoke(json)?;
    if r.parsed.user_id != user_id {
        return Err(Error::IdMismatch);
    }
    if fingerprint(revoker_sign_pub_b64, revoker_kex_pub_b64) != r.parsed.revoker_fp {
        return Err(Error::BadKey);
    }
    verify_der(&b64_decode(revoker_sign_pub_b64)?, &r.statement, &r.sig)?;
    Ok(r)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyPurpose {
    #[default]
    Spawn,
    Presence,
}

impl KeyPurpose {
    fn tag(self) -> &'static str {
        match self {
            KeyPurpose::Spawn => "tmgr-spawn-key",
            KeyPurpose::Presence => "tmgr-presence-key",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnStatement {
    pub purpose: KeyPurpose,
    pub user_id: u64,
    pub device_fp: String,
    pub device_id: String,
    pub spawn_pub_b64: String,
    pub issued_at_ms: u64,
}

impl SpawnStatement {
    pub fn to_statement(&self) -> Result<String> {
        check_fp(&self.device_fp)?;
        check_device_id(&self.device_id)?;
        check_pub(&self.spawn_pub_b64)?;
        Ok(format!(
            "{}:v1:{}:{}:{}:{}:{}",
            self.purpose.tag(),
            self.user_id,
            self.device_fp,
            self.device_id,
            self.spawn_pub_b64,
            self.issued_at_ms
        ))
    }

    pub fn parse(s: &str) -> Result<SpawnStatement> {
        let purpose = [KeyPurpose::Spawn, KeyPurpose::Presence]
            .into_iter()
            .find(|k| s.split(':').next() == Some(k.tag()))
            .ok_or(Error::BadFormat)?;
        let p = parts(s, purpose.tag(), 7)?;
        let st = SpawnStatement {
            purpose,
            user_id: parse_decimal(p[2])?,
            device_fp: p[3].to_string(),
            device_id: p[4].to_string(),
            spawn_pub_b64: p[5].to_string(),
            issued_at_ms: parse_decimal(p[6])?,
        };
        st.to_statement()?;
        Ok(st)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SpawnKey {
    pub statement: String,
    pub sig: String,
    pub spawn_sig: String,
}

pub fn sign_spawnkey(
    device: &DeviceKeys,
    spawn_secret: &[u8],
    st: &SpawnStatement,
) -> Result<SpawnKey> {
    if st.device_fp != device.fingerprint() {
        return Err(Error::BadKey);
    }
    if b64_decode(&st.spawn_pub_b64)? != tmgr_phone::public_key(spawn_secret)? {
        return Err(Error::BadKey);
    }
    let statement = st.to_statement()?;
    Ok(SpawnKey {
        sig: sign(device.sign_secret(), &statement)?,
        spawn_sig: sign(spawn_secret, &statement)?,
        statement,
    })
}

pub fn verify_spawnkey(
    json: &str,
    user_id: u64,
    device_id: &str,
    device_sign_pub_b64: &str,
    device_kex_pub_b64: &str,
) -> Result<SpawnStatement> {
    let k: SpawnKey = parse_strict(json)?;
    let st = SpawnStatement::parse(&k.statement)?;
    if st.user_id != user_id {
        return Err(Error::IdMismatch);
    }
    if st.device_id != device_id
        || st.device_fp != fingerprint(device_sign_pub_b64, device_kex_pub_b64)
    {
        return Err(Error::BadKey);
    }
    verify_der(&b64_decode(device_sign_pub_b64)?, &k.statement, &k.sig)?;
    verify_der(&b64_decode(&st.spawn_pub_b64)?, &k.statement, &k.spawn_sig)?;
    Ok(st)
}

#[derive(Clone, PartialEq, Eq)]
pub struct JoinCode {
    pub version: u8,
    pub user_id: u64,
    pub device_id: String,
    pub fp: String,
    pub kind: Option<String>,
    pub secret: [u8; 32],
    pub exp_ms: u64,
}

impl std::fmt::Debug for JoinCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JoinCode")
            .field("version", &self.version)
            .field("user_id", &self.user_id)
            .field("device_id", &self.device_id)
            .field("fp", &self.fp)
            .field("kind", &self.kind)
            .field("secret", &"<redacted>")
            .field("exp_ms", &self.exp_ms)
            .finish()
    }
}

#[derive(Serialize, Deserialize)]
struct JoinJson {
    v: u32,
    user_id: u64,
    device_id: String,
    fp: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    secret: String,
    exp: u64,
}

impl JoinCode {
    pub fn is_expired(&self, now_ms: u64) -> bool {
        now_ms >= self.exp_ms
    }
}

fn check_code_kind(kind: &str) -> Result<()> {
    if matches!(kind, "desktop" | "mobile") {
        Ok(())
    } else {
        Err(Error::BadFormat)
    }
}

pub fn encode_join_code(code: &JoinCode) -> Result<String> {
    check_device_id(&code.device_id)?;
    check_fp(&code.fp)?;
    let (prefix, kind) = match (code.version, &code.kind) {
        (1, None) => (JOIN_PREFIX, None),
        (2, Some(k)) => {
            check_code_kind(k)?;
            (JOIN_PREFIX_V2, Some(k.clone()))
        }
        _ => return Err(Error::BadFormat),
    };
    let json = serde_json::to_string(&JoinJson {
        v: u32::from(code.version),
        user_id: code.user_id,
        device_id: code.device_id.clone(),
        fp: code.fp.clone(),
        kind,
        secret: b64(&code.secret),
        exp: code.exp_ms,
    })
    .map_err(|_| Error::BadJson)?;
    Ok(format!("{prefix}{}", URL_SAFE_NO_PAD.encode(json)))
}

pub fn decode_join_code(s: &str) -> Result<JoinCode> {
    let (version, enc) = match (s.strip_prefix(JOIN_PREFIX), s.strip_prefix(JOIN_PREFIX_V2)) {
        (Some(e), _) => (1u8, e),
        (_, Some(e)) => (2u8, e),
        _ => return Err(Error::BadFormat),
    };
    let raw = URL_SAFE_NO_PAD.decode(enc).map_err(|_| Error::BadFormat)?;
    let j: JoinJson = parse_strict(std::str::from_utf8(&raw).map_err(|_| Error::BadJson)?)?;
    if j.v != u32::from(version) {
        return Err(Error::BadFormat);
    }
    check_device_id(&j.device_id)?;
    check_fp(&j.fp)?;
    let kind = if version == 2 {
        let k = j.kind.ok_or(Error::BadFormat)?;
        check_code_kind(&k)?;
        Some(k)
    } else {
        None
    };
    let secret: [u8; 32] = b64_decode(&j.secret)?
        .try_into()
        .map_err(|_| Error::BadFormat)?;
    Ok(JoinCode {
        version,
        user_id: j.user_id,
        device_id: j.device_id,
        fp: j.fp,
        kind,
        secret,
        exp_ms: j.exp,
    })
}

pub struct JoinMacInput<'a> {
    pub version: u8,
    pub user_id: u64,
    pub p_fp: &'a str,
    pub n_id: &'a str,
    pub n_fp: &'a str,
    pub n_kind: &'a str,
    pub label: &'a str,
}

fn join_transcript(i: &JoinMacInput) -> Result<String> {
    if !matches!(i.version, 1 | 2) {
        return Err(Error::BadFormat);
    }
    check_fp(i.p_fp)?;
    check_device_id(i.n_id)?;
    check_fp(i.n_fp)?;
    check_device_kind(i.n_kind)?;
    Ok(format!(
        "tmgr-chat-join:v{}:{}:{}:{}:{}:{}:{}",
        i.version,
        i.user_id,
        i.p_fp,
        i.n_id,
        i.n_fp,
        i.n_kind,
        h(i.label)
    ))
}

pub fn join_mac(secret: &[u8; 32], i: &JoinMacInput) -> Result<String> {
    pair_mac(secret, &join_transcript(i)?)
}

pub fn verify_join_mac(secret: &[u8; 32], i: &JoinMacInput, mac_hex: &str) -> Result<()> {
    let expected = join_mac(secret, i)?;
    if bool::from(expected.as_bytes().ct_eq(mac_hex.as_bytes())) {
        Ok(())
    } else {
        Err(Error::BadMac)
    }
}

#[derive(Clone, Debug)]
pub struct DirEntry {
    pub device_id: String,
    pub sign_pub_b64: String,
    pub kex_pub_b64: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Trusted {
    pub device_id: String,
    pub fp: String,
    pub sign_pub: String,
    pub kex_pub: String,
}

impl Trusted {
    pub fn new(device_id: &str, sign_pub_b64: &str, kex_pub_b64: &str) -> Result<Trusted> {
        check_device_id(device_id)?;
        check_pub(sign_pub_b64)?;
        check_pub(kex_pub_b64)?;
        Ok(Trusted {
            device_id: device_id.to_string(),
            fp: fingerprint(sign_pub_b64, kex_pub_b64),
            sign_pub: sign_pub_b64.to_string(),
            kex_pub: kex_pub_b64.to_string(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct KeyMismatch {
    pub device_id: String,
    pub pinned_fp: String,
    pub directory_fp: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustSet {
    pub trusted: Vec<Trusted>,
    pub mismatches: Vec<KeyMismatch>,
    pub kinds: Vec<(String, String, String)>,
    pub conflicts: Vec<(String, String)>,
    pub unstable: Vec<String>,
    pub passes: usize,
}

pub struct KindCtx<'a> {
    pub known: &'a BTreeMap<String, String>,
    pub own: Option<&'a str>,
    pub assumed: &'a BTreeSet<String>,
    pub max_passes: Option<usize>,
}

pub fn kind_key(device_id: &str, fp: &str) -> String {
    format!("{device_id}:{fp}")
}

pub fn trusted_set(
    user_id: u64,
    pins: &[Trusted],
    revoked: &[String],
    intros: &[String],
    directory: &[DirEntry],
) -> TrustSet {
    trusted_set_by_kind(user_id, pins, revoked, intros, directory, None)
}

/// Revoke recompute from base: the result may only drop trust and lower kinds (to `runner`) relative to what was trusted and known before.
pub fn restrict_to_before(
    mut set: TrustSet,
    trusted_before: &BTreeSet<String>,
    kinds_before: &BTreeMap<String, String>,
) -> TrustSet {
    set.trusted
        .retain(|t| trusted_before.contains(&kind_key(&t.device_id, &t.fp)));
    set.kinds.retain(|(id, fp, kind)| {
        kinds_before
            .get(&kind_key(id, fp))
            .is_some_and(|have| lowers_kind(have, kind))
    });
    set.mismatches
        .retain(|m| set.trusted.iter().any(|t| t.device_id == m.device_id));
    set
}

pub fn lowers_kind(have: &str, new: &str) -> bool {
    (new == "runner" && have != "runner")
        || (new == "terminal" && matches!(have, "desktop" | "mobile"))
}

pub fn expected_join_matches(
    expected_id: &str,
    expected_fp: &str,
    pending_id: &str,
    pending_fp: &str,
    pending_kind: &str,
) -> bool {
    pending_kind == "runner"
        && !expected_id.is_empty()
        && !expected_fp.is_empty()
        && expected_id == pending_id
        && expected_fp == pending_fp
}

pub fn may_vouch(kind: Option<&str>) -> bool {
    matches!(kind, Some("desktop" | "mobile"))
}

pub fn is_owner_device(kind: Option<&str>) -> bool {
    matches!(kind, Some("desktop" | "mobile" | "terminal"))
}

pub fn may_revoke(revoker_kind: Option<&str>, revoker_fp: &str, target_fp: &str) -> bool {
    may_vouch(revoker_kind) || revoker_fp == target_fp
}

fn parse_intros(user_id: u64, intros: &[String]) -> Vec<Intro> {
    let mut seen = HashSet::new();
    intros
        .iter()
        .filter(|s| seen.insert(s.as_str()))
        .filter_map(|s| parse_intro(s).ok())
        .filter(|i| i.parsed.user_id == user_id)
        .collect()
}

fn find_trusted<'a>(trusted: &'a [Trusted], id: &str, fp: &str) -> Option<&'a Trusted> {
    trusted.iter().find(|t| t.device_id == id && t.fp == fp)
}

fn signed_by(signer: &Trusted, intro: &Intro) -> bool {
    b64_decode(&signer.sign_pub)
        .and_then(|key| verify_der(&key, &intro.statement, &intro.sig))
        .is_ok()
}

fn self_markers_of(
    parsed: &[Intro],
    trusted: &[Trusted],
    kinds: &BTreeMap<String, String>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for i in parsed {
        let st = &i.parsed;
        if st.version != 2 || !st.is_self(&i.by) {
            continue;
        }
        let Some(signer) = find_trusted(trusted, &i.by, &st.introducer_fp) else {
            continue;
        };
        let key = kind_key(&signer.device_id, &signer.fp);
        if may_vouch(kinds.get(&key).map(String::as_str)) && signed_by(signer, i) {
            out.insert(key);
        }
    }
    out
}

/// Keys (`device_id:fp`) of devices whose signed v2 self-intro is counted: the signer is trusted and may vouch.
pub fn v2_self_markers(
    user_id: u64,
    trusted: &[Trusted],
    kinds: &BTreeMap<String, String>,
    intros: &[String],
) -> BTreeSet<String> {
    self_markers_of(&parse_intros(user_id, intros), trusted, kinds)
}

/// Devices that keep the runner gate closed: desktop/mobile devices without a counted v2 self-intro and devices of unknown kind. Runners, terminals and `own` never block.
pub fn gate_blockers(
    trusted: &[Trusted],
    kinds: &BTreeMap<String, String>,
    own: Option<&str>,
    markers: &BTreeSet<String>,
) -> Vec<String> {
    trusted
        .iter()
        .filter(|t| {
            let key = kind_key(&t.device_id, &t.fp);
            if Some(key.as_str()) == own {
                return false;
            }
            match kinds.get(&key).map(String::as_str) {
                Some("runner" | "terminal") => false,
                Some("desktop" | "mobile") => !markers.contains(&key),
                _ => true,
            }
        })
        .map(|t| t.device_id.clone())
        .collect()
}

fn start_set(pins: &[Trusted], revoked: &[String]) -> Vec<Trusted> {
    let mut trusted: Vec<Trusted> = Vec::new();
    for p in pins {
        if revoked.contains(&p.fp)
            || p.fp != fingerprint(&p.sign_pub, &p.kex_pub)
            || trusted.iter().any(|t| t.device_id == p.device_id)
        {
            continue;
        }
        trusted.push(p.clone());
    }
    trusted
}

struct Pass {
    trusted: Vec<Trusted>,
    candidates: Vec<(String, String, String)>,
}

struct PassInput<'a> {
    revoked: &'a [String],
    parsed: &'a [Intro],
    directory: &'a [DirEntry],
}

fn run_pass(
    inp: &PassInput,
    start: &[Trusted],
    known: Option<&BTreeMap<String, String>>,
    allow_v1: bool,
) -> Pass {
    let is_revoked = |fp: &str| inp.revoked.iter().any(|r| r == fp);
    let mut trusted = start.to_vec();
    let mut candidates: Vec<(String, String, String)> = Vec::new();
    let mut done = vec![false; inp.parsed.len()];
    loop {
        let mut changed = false;
        for (n, intro) in inp.parsed.iter().enumerate() {
            let st = &intro.parsed;
            if done[n] || is_revoked(&st.target_fp) {
                continue;
            }
            if let Some(k) = known {
                if st.is_self(&intro.by) {
                    done[n] = true;
                    continue;
                }
                if st.version == 1
                    && (!allow_v1
                        || !may_vouch(
                            k.get(&kind_key(&st.target_id, &st.target_fp))
                                .map(String::as_str),
                        ))
                {
                    continue;
                }
            }
            let target_pinned = match trusted.iter().find(|t| t.device_id == st.target_id) {
                Some(t) if t.fp == st.target_fp => true,
                Some(_) => continue,
                None => false,
            };
            let Some(introducer) = find_trusted(&trusted, &intro.by, &st.introducer_fp) else {
                continue;
            };
            if known.is_some_and(|k| {
                !may_vouch(
                    k.get(&kind_key(&introducer.device_id, &introducer.fp))
                        .map(String::as_str),
                )
            }) {
                continue;
            }
            let entry = if target_pinned {
                None
            } else {
                let Some(entry) = inp.directory.iter().find(|d| {
                    d.device_id == st.target_id
                        && fingerprint(&d.sign_pub_b64, &d.kex_pub_b64) == st.target_fp
                }) else {
                    continue;
                };
                Some(entry)
            };
            let verified = signed_by(introducer, intro);
            done[n] = true;
            if !verified {
                continue;
            }
            if let Some(entry) = entry {
                trusted.push(Trusted {
                    device_id: st.target_id.clone(),
                    fp: st.target_fp.clone(),
                    sign_pub: entry.sign_pub_b64.clone(),
                    kex_pub: entry.kex_pub_b64.clone(),
                });
            }
            if known.is_some() && st.version == 2 {
                let c = (
                    st.target_id.clone(),
                    st.target_fp.clone(),
                    st.target_kind.clone(),
                );
                if !candidates.contains(&c) {
                    candidates.push(c);
                }
            }
            changed = true;
        }
        if !changed {
            break;
        }
    }
    trusted.sort();
    Pass {
        trusted,
        candidates,
    }
}

struct Merge {
    kinds: BTreeMap<String, String>,
    conflicts: Vec<(String, String)>,
}

fn merge_kinds(
    known: &BTreeMap<String, String>,
    candidates: &[(String, String, String)],
    own: Option<&str>,
    assumed: &BTreeSet<String>,
) -> Merge {
    let mut kinds = known.clone();
    let mut conflicts = Vec::new();
    let keys: BTreeSet<(String, String)> = candidates
        .iter()
        .map(|(id, fp, _)| (id.clone(), fp.clone()))
        .collect();
    for (id, fp) in keys {
        let key = kind_key(&id, &fp);
        if Some(key.as_str()) == own || assumed.contains(&key) {
            continue;
        }
        let existing = known.get(&key).cloned();
        let mut all: BTreeSet<&str> = candidates
            .iter()
            .filter(|(i, f, _)| *i == id && *f == fp)
            .map(|(_, _, k)| k.as_str())
            .collect();
        if let Some(e) = &existing {
            all.insert(e.as_str());
        }
        let conflict = all.len() > 1;
        let result: Option<String> = if all.contains("runner") {
            Some("runner".to_owned())
        } else if all.len() == 1 {
            all.iter().next().map(|k| (*k).to_owned())
        } else {
            existing.clone()
        };
        if conflict {
            conflicts.push((id.clone(), fp.clone()));
        }
        if let Some(r) = result {
            if existing.as_deref() != Some(r.as_str()) {
                kinds.insert(key.clone(), r);
            }
        }
    }
    Merge { kinds, conflicts }
}

fn mismatches_of(trusted: &[Trusted], directory: &[DirEntry]) -> Vec<KeyMismatch> {
    let mut mismatches: Vec<KeyMismatch> = trusted
        .iter()
        .flat_map(|t| {
            directory
                .iter()
                .filter(|d| d.device_id == t.device_id)
                .filter_map(|d| {
                    let directory_fp = fingerprint(&d.sign_pub_b64, &d.kex_pub_b64);
                    (directory_fp != t.fp).then(|| KeyMismatch {
                        device_id: t.device_id.clone(),
                        pinned_fp: t.fp.clone(),
                        directory_fp,
                    })
                })
        })
        .collect();
    mismatches.sort();
    mismatches
}

/// With `kinds`, only desktop and mobile devices may vouch, and kinds come from counted v2 intros merged after each pass (runner wins; desktop vs mobile keeps the known value). The pass reruns from the start set while a merge changes who may vouch.
pub fn trusted_set_by_kind(
    user_id: u64,
    pins: &[Trusted],
    revoked: &[String],
    intros: &[String],
    directory: &[DirEntry],
    kinds: Option<&KindCtx>,
) -> TrustSet {
    let parsed = parse_intros(user_id, intros);
    let inp = PassInput {
        revoked,
        parsed: &parsed,
        directory,
    };
    let start = start_set(pins, revoked);
    let Some(ctx) = kinds else {
        let pass = run_pass(&inp, &start, None, true);
        return TrustSet {
            mismatches: mismatches_of(&pass.trusted, directory),
            trusted: pass.trusted,
            kinds: Vec::new(),
            conflicts: Vec::new(),
            unstable: Vec::new(),
            passes: 1,
        };
    };
    let original: BTreeMap<String, String> = ctx
        .known
        .iter()
        .filter(|(_, v)| check_device_kind(v).is_ok())
        .map(|(a, b)| (a.clone(), b.clone()))
        .collect();
    let mut base = original.clone();
    for k in ctx.assumed {
        base.entry(k.clone())
            .or_insert_with(|| "desktop".to_owned());
    }
    let population: BTreeSet<String> = base
        .keys()
        .cloned()
        .chain(start.iter().map(|t| kind_key(&t.device_id, &t.fp)))
        .chain(
            parsed
                .iter()
                .map(|i| kind_key(&i.parsed.target_id, &i.parsed.target_fp)),
        )
        .collect();
    let cap = ctx.max_passes.unwrap_or(2 * population.len() + 1);
    let mut lowered: BTreeSet<String> = BTreeSet::new();
    let mut raised: BTreeMap<String, String> = BTreeMap::new();
    let mut passes = 0;
    loop {
        passes += 1;
        let mut known = base.clone();
        for (k, v) in &raised {
            known.entry(k.clone()).or_insert_with(|| v.clone());
        }
        for k in &lowered {
            known.insert(k.clone(), "runner".to_owned());
        }
        let mut pass = run_pass(&inp, &start, Some(&known), true);
        let markers = self_markers_of(&parsed, &pass.trusted, &known);
        if gate_blockers(&pass.trusted, &known, ctx.own, &markers).is_empty() {
            pass = run_pass(&inp, &start, Some(&known), false);
        }
        let merge = merge_kinds(&known, &pass.candidates, ctx.own, ctx.assumed);
        let demoted: Vec<String> = merge
            .kinds
            .iter()
            .filter(|(_, v)| v.as_str() == "runner")
            .filter(|(k, _)| known.get(*k).map(String::as_str) != Some("runner"))
            .map(|(k, _)| k.clone())
            .collect();
        let next_raised: BTreeMap<String, String> = merge
            .kinds
            .iter()
            .filter(|(k, v)| {
                v.as_str() != "runner" && !base.contains_key(*k) && !lowered.contains(*k)
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let moving = !demoted.is_empty() || next_raised != raised;
        if !moving {
            let learned = merge
                .kinds
                .iter()
                .filter(|(k, v)| original.get(*k) != Some(*v) && !ctx.assumed.contains(*k))
                .filter(|(k, _)| {
                    pass.trusted
                        .iter()
                        .any(|t| kind_key(&t.device_id, &t.fp) == **k)
                })
                .filter_map(|(k, v)| {
                    let (id, fp) = k.split_once(':')?;
                    Some((id.to_owned(), fp.to_owned(), v.clone()))
                })
                .collect();
            return TrustSet {
                mismatches: mismatches_of(&pass.trusted, directory),
                trusted: pass.trusted,
                kinds: learned,
                conflicts: merge.conflicts,
                unstable: Vec::new(),
                passes,
            };
        }
        if passes >= cap {
            let mut trusted = pass.trusted;
            trusted.retain(|t| Some(kind_key(&t.device_id, &t.fp).as_str()) == ctx.own);
            let unstable = demoted
                .iter()
                .chain(next_raised.keys().filter(|k| !raised.contains_key(*k)))
                .filter_map(|k| k.split(':').next().map(str::to_owned))
                .collect();
            return TrustSet {
                mismatches: mismatches_of(&trusted, directory),
                trusted,
                kinds: Vec::new(),
                conflicts: merge.conflicts,
                unstable,
                passes,
            };
        }
        if demoted.is_empty() {
            raised = next_raised;
        } else {
            lowered.extend(demoted);
            raised.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_join_needs_exact_id_fp_and_runner_kind() {
        assert!(expected_join_matches("d1", "fp1", "d1", "fp1", "runner"));
        assert!(!expected_join_matches("d1", "fp1", "d2", "fp1", "runner"));
        assert!(!expected_join_matches("d1", "fp1", "d1", "fp2", "runner"));
        for kind in ["desktop", "mobile", "", "Runner"] {
            assert!(
                !expected_join_matches("d1", "fp1", "d1", "fp1", kind),
                "{kind}"
            );
        }
        assert!(!expected_join_matches("", "", "", "", "runner"));
    }

    #[test]
    fn terminal_is_a_device_kind_that_cannot_vouch() {
        crate::fields::check_device_kind("terminal").unwrap();
        for k in [Some("terminal"), Some("runner"), Some("weird"), None] {
            assert!(!may_vouch(k), "{k:?}");
        }
    }

    #[test]
    fn lowers_kind_table() {
        let kinds = ["desktop", "mobile", "runner", "terminal"];
        let lowers = [
            ("desktop", "runner"),
            ("mobile", "runner"),
            ("terminal", "runner"),
            ("desktop", "terminal"),
            ("mobile", "terminal"),
        ];
        for have in kinds {
            for new in kinds {
                assert_eq!(
                    lowers_kind(have, new),
                    lowers.contains(&(have, new)),
                    "{have} -> {new}"
                );
            }
        }
    }

    #[test]
    fn only_desktop_mobile_or_the_device_itself_may_revoke() {
        for k in [Some("desktop"), Some("mobile")] {
            assert!(may_revoke(k, "a", "b"));
        }
        for k in [Some("runner"), Some("terminal"), Some("weird"), None] {
            assert!(!may_revoke(k, "a", "b"), "{k:?}");
            assert!(may_revoke(k, "a", "a"), "{k:?}");
        }
    }

    #[test]
    fn a_terminal_does_not_block_the_runner_gate() {
        let t = |id: &str, fp: &str| Trusted {
            device_id: id.into(),
            fp: fp.into(),
            sign_pub: String::new(),
            kex_pub: String::new(),
        };
        let trusted = [t("d1", "f1"), t("d2", "f2"), t("d3", "f3")];
        let kinds: BTreeMap<String, String> = [("d1", "f1", "terminal"), ("d2", "f2", "runner")]
            .into_iter()
            .map(|(i, f, k)| (kind_key(i, f), k.to_owned()))
            .collect();
        let blockers = gate_blockers(&trusted, &kinds, None, &BTreeSet::new());
        assert_eq!(blockers, ["d3"]);
    }

    const U: u64 = 42;
    const IDS: [&str; 5] = [
        "00000000-0000-4000-8000-00000000000a",
        "00000000-0000-4000-8000-00000000000b",
        "00000000-0000-4000-8000-00000000000c",
        "00000000-0000-4000-8000-00000000000d",
        "00000000-0000-4000-8000-00000000000e",
    ];

    fn key(n: u8) -> DeviceKeys {
        DeviceKeys::from_secrets(&[n; 32], &[n + 100; 32]).unwrap()
    }

    fn dir(ids_keys: &[(&str, &DeviceKeys)]) -> Vec<DirEntry> {
        ids_keys
            .iter()
            .map(|(id, k)| DirEntry {
                device_id: id.to_string(),
                sign_pub_b64: k.sign_pub_b64(),
                kex_pub_b64: k.kex_pub_b64(),
            })
            .collect()
    }

    fn statement(
        by: &DeviceKeys,
        target: &DeviceKeys,
        target_id: &str,
        user: u64,
    ) -> IntroStatement {
        IntroStatement {
            version: 1,
            user_id: user,
            introducer_fp: by.fingerprint(),
            target_id: target_id.into(),
            target_fp: target.fingerprint(),
            target_kind: "mobile".into(),
            issued_at_ms: 1_800_000_000_000,
        }
    }

    fn intro(by: (&str, &DeviceKeys), target: (&str, &DeviceKeys)) -> String {
        sign_intro(by.1, by.0, &statement(by.1, target.1, target.0, U)).unwrap()
    }

    fn pin(id: &str, k: &DeviceKeys) -> Trusted {
        Trusted::new(id, &k.sign_pub_b64(), &k.kex_pub_b64()).unwrap()
    }

    fn ids(set: &TrustSet) -> Vec<String> {
        set.trusted.iter().map(|t| t.device_id.clone()).collect()
    }

    fn run(pins: &[Trusted], revoked: &[String], intros: &[String], d: &[DirEntry]) -> TrustSet {
        trusted_set(U, pins, revoked, intros, d)
    }

    #[test]
    fn intro_statement_format_and_round_trip() {
        let k = key(1);
        let st = IntroStatement {
            version: 1,
            user_id: U,
            introducer_fp: k.fingerprint(),
            target_id: IDS[1].into(),
            target_fp: key(2).fingerprint(),
            target_kind: "runner".into(),
            issued_at_ms: 7,
        };
        let s = st.to_statement().unwrap();
        assert_eq!(
            s,
            format!(
                "tmgr-chat-intro:v1:42:{}:{}:{}:runner:7",
                st.introducer_fp, st.target_id, st.target_fp
            )
        );
        assert_eq!(IntroStatement::parse(&s).unwrap(), st);
        let json = sign_intro(&k, IDS[0], &st).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["v", "by", "statement", "sig"]);
        let (sp, kp) = (k.sign_pub_b64(), k.kex_pub_b64());
        let got = verify_intro(&json, U, IDS[0], &sp, &kp).unwrap();
        assert_eq!(got.by, IDS[0]);
        assert_eq!(got.parsed, st);
    }

    #[test]
    fn verify_intro_binds_user_by_and_keys() {
        let k = key(1);
        let json = intro((IDS[0], &k), (IDS[1], &key(2)));
        let (sp, kp) = (k.sign_pub_b64(), k.kex_pub_b64());
        assert_eq!(
            verify_intro(&json, 43, IDS[0], &sp, &kp).err(),
            Some(Error::IdMismatch)
        );
        assert_eq!(
            verify_intro(&json, U, IDS[3], &sp, &kp).err(),
            Some(Error::IdMismatch)
        );
        let other = key(9);
        assert_eq!(
            verify_intro(
                &json,
                U,
                IDS[0],
                &other.sign_pub_b64(),
                &other.kex_pub_b64()
            )
            .err(),
            Some(Error::BadKey)
        );
        let swapped_kex = key(1).kex_pub_b64().replace('A', "B");
        assert!(verify_intro(&json, U, IDS[0], &sp, &swapped_kex).is_err());
        let forged = json.replace(
            &intro_sig(&json),
            &intro_sig(&intro((IDS[0], &key(5)), (IDS[1], &key(2)))),
        );
        assert_eq!(
            verify_intro(&forged, U, IDS[0], &sp, &kp).err(),
            Some(Error::BadSignature)
        );
    }

    fn intro_sig(json: &str) -> String {
        parse_intro(json).unwrap().sig
    }

    #[test]
    fn statements_reject_bad_shapes() {
        let fp = "a".repeat(64);
        let good = format!("tmgr-chat-intro:v1:42:{fp}:{}:{fp}:mobile:7", IDS[0]);
        IntroStatement::parse(&good).unwrap();
        for bad in [
            format!("{good}:extra"),
            format!("tmgr-chat-intro:v3:42:{fp}:{}:{fp}:mobile:7", IDS[0]),
            format!("tmgr-chat-intro:v1:042:{fp}:{}:{fp}:mobile:7", IDS[0]),
            format!("tmgr-chat-intro:v1:42:{fp}:{}:{fp}:phone:7", IDS[0]),
            format!("tmgr-chat-intro:v1:42:{fp}:x:{fp}:mobile:7"),
            format!(
                "tmgr-chat-intro:v1:42:{}:{}:{fp}:mobile:7",
                "A".repeat(64),
                IDS[0]
            ),
            format!("tmgr-chat-intro:v1:42:{fp}:{}:{fp}:mobile:-7", IDS[0]),
            format!("tmgr-chat-revoke:v1:42:{fp}:{}:{fp}:7", IDS[0]),
        ] {
            assert!(IntroStatement::parse(&bad).is_err(), "{bad}");
        }
        let rev = format!("tmgr-chat-revoke:v1:42:{fp}:{}:{fp}:7", IDS[0]);
        RevokeStatement::parse(&rev).unwrap();
        assert!(RevokeStatement::parse(&format!("{rev}:1")).is_err());
        assert!(RevokeStatement::parse(&good).is_err());
    }

    #[test]
    fn intro_json_with_duplicate_keys_is_rejected() {
        let k = key(1);
        let json = intro((IDS[0], &k), (IDS[1], &key(2)));
        let dup = json.replacen("{\"v\":1,", "{\"v\":1,\"v\":1,", 1);
        assert_eq!(parse_intro(&dup).err(), Some(Error::BadJson));
    }

    #[test]
    fn unknown_json_fields_are_ignored() {
        let k = key(1);
        let json = intro((IDS[0], &k), (IDS[1], &key(2)));
        let extra = json.replacen("{\"v\":1,", "{\"v\":1,\"future\":{\"a\":[1]},", 1);
        let (sp, kp) = (k.sign_pub_b64(), k.kex_pub_b64());
        verify_intro(&extra, U, IDS[0], &sp, &kp).unwrap();
    }

    #[test]
    fn sign_refuses_wrong_signer_fp() {
        let st = statement(&key(2), &key(3), IDS[1], U);
        assert!(sign_intro(&key(1), IDS[0], &st).is_err());
    }

    #[test]
    fn revoke_round_trip_and_bindings() {
        let k = key(1);
        let st = RevokeStatement {
            user_id: U,
            revoker_fp: k.fingerprint(),
            target_id: IDS[1].into(),
            target_fp: key(2).fingerprint(),
            issued_at_ms: 99,
        };
        let json = sign_revoke(&k, &st).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["v", "statement", "sig"]);
        let (sp, kp) = (k.sign_pub_b64(), k.kex_pub_b64());
        assert_eq!(verify_revoke(&json, U, &sp, &kp).unwrap().parsed, st);
        assert_eq!(
            verify_revoke(&json, 43, &sp, &kp).err(),
            Some(Error::IdMismatch)
        );
        let o = key(5);
        assert_eq!(
            verify_revoke(&json, U, &o.sign_pub_b64(), &o.kex_pub_b64()).err(),
            Some(Error::BadKey)
        );
        assert_eq!(
            st.to_statement().unwrap(),
            format!(
                "tmgr-chat-revoke:v1:42:{}:{}:{}:99",
                st.revoker_fp, st.target_id, st.target_fp
            )
        );
    }

    #[test]
    fn join_code_round_trip_and_strictness() {
        let code = JoinCode {
            version: 1,
            kind: None,
            user_id: U,
            device_id: IDS[0].into(),
            fp: key(1).fingerprint(),
            secret: [9u8; 32],
            exp_ms: 1_800_000_600_000,
        };
        let s = encode_join_code(&code).unwrap();
        assert!(s.starts_with("tmgr-chat-join:v1:"));
        let enc = &s[JOIN_PREFIX.len()..];
        assert!(!enc.contains('=') && !enc.contains('+') && !enc.contains('/'));
        assert_eq!(decode_join_code(&s).unwrap(), code);
        let json = String::from_utf8(URL_SAFE_NO_PAD.decode(enc).unwrap()).unwrap();
        assert!(json.starts_with("{\"v\":1,\"user_id\":42,\"device_id\":"));
        assert!(code.is_expired(code.exp_ms));
        assert!(!code.is_expired(code.exp_ms - 1));
        assert!(decode_join_code(&format!("{s}=")).is_err());
        assert!(decode_join_code(&s.replace("v1", "v2")).is_err());
        assert!(decode_join_code(enc).is_err());
        let dup = json.replacen("{\"v\":1,", "{\"v\":1,\"v\":1,", 1);
        let s2 = format!("{JOIN_PREFIX}{}", URL_SAFE_NO_PAD.encode(dup));
        assert_eq!(decode_join_code(&s2).err(), Some(Error::BadJson));
        let short = json.replace(&b64(&[9u8; 32]), &b64(&[9u8; 31]));
        let s3 = format!("{JOIN_PREFIX}{}", URL_SAFE_NO_PAD.encode(short));
        assert!(decode_join_code(&s3).is_err());
    }

    #[test]
    fn join_code_debug_redacts_secret() {
        let code = JoinCode {
            version: 1,
            kind: None,
            user_id: U,
            device_id: IDS[0].into(),
            fp: key(1).fingerprint(),
            secret: [9u8; 32],
            exp_ms: 1,
        };
        let d = format!("{code:?}");
        assert!(d.contains("<redacted>"));
        assert!(!d.contains(&b64(&[9u8; 32])));
        assert!(!d.contains("[9, 9"));
    }

    #[test]
    fn join_mac_binds_every_field() {
        let secret = [9u8; 32];
        let (pf, nf) = (key(1).fingerprint(), key(2).fingerprint());
        let base = JoinMacInput {
            version: 1,
            user_id: U,
            p_fp: &pf,
            n_id: IDS[1],
            n_fp: &nf,
            n_kind: "mobile",
            label: "Phone",
        };
        let mac = join_mac(&secret, &base).unwrap();
        assert_eq!(mac.len(), 64);
        verify_join_mac(&secret, &base, &mac).unwrap();
        assert_eq!(
            verify_join_mac(&[8u8; 32], &base, &mac).err(),
            Some(Error::BadMac)
        );
        assert!(verify_join_mac(&secret, &base, &mac[..63]).is_err());
        assert!(verify_join_mac(&secret, &base, &mac.to_uppercase()).is_err());
        let other = key(3).fingerprint();
        let variants = [
            JoinMacInput {
                user_id: 43,
                ..base
            },
            JoinMacInput {
                p_fp: &other,
                ..base
            },
            JoinMacInput {
                n_id: IDS[2],
                ..base
            },
            JoinMacInput {
                n_fp: &other,
                ..base
            },
            JoinMacInput {
                n_kind: "runner",
                ..base
            },
            JoinMacInput {
                label: "Phone2",
                ..base
            },
        ];
        for v in &variants {
            assert!(verify_join_mac(&secret, v, &mac).is_err());
        }
    }

    #[test]
    fn spawnkey_needs_both_signatures_and_user() {
        let dev = key(1);
        let spawn_secret = [77u8; 32];
        let spawn_pub = b64(&tmgr_phone::public_key(&spawn_secret).unwrap());
        let st = SpawnStatement {
            purpose: KeyPurpose::Spawn,
            user_id: U,
            device_fp: dev.fingerprint(),
            device_id: IDS[0].into(),
            spawn_pub_b64: spawn_pub.clone(),
            issued_at_ms: 5,
        };
        assert_eq!(
            st.to_statement().unwrap(),
            format!(
                "tmgr-spawn-key:v1:42:{}:{}:{spawn_pub}:5",
                st.device_fp, IDS[0]
            )
        );
        let k = sign_spawnkey(&dev, &spawn_secret, &st).unwrap();
        let json = serde_json::to_string(&k).unwrap();
        let keys: Vec<_> = serde_json::from_str::<serde_json::Value>(&json)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(keys, ["statement", "sig", "spawn_sig"]);
        let (sp, kp) = (dev.sign_pub_b64(), dev.kex_pub_b64());
        assert_eq!(verify_spawnkey(&json, U, IDS[0], &sp, &kp).unwrap(), st);
        assert!(verify_spawnkey(&json, U, IDS[1], &sp, &kp).is_err());
        assert_eq!(
            verify_spawnkey(&json, 43, IDS[0], &sp, &kp).err(),
            Some(Error::IdMismatch)
        );
        let as_json = |k: &SpawnKey| serde_json::to_string(k).unwrap();
        let swap_sig = SpawnKey {
            sig: k.spawn_sig.clone(),
            ..k.clone()
        };
        assert!(verify_spawnkey(&as_json(&swap_sig), U, IDS[0], &sp, &kp).is_err());
        let swap_spawn = SpawnKey {
            spawn_sig: k.sig.clone(),
            ..k.clone()
        };
        assert!(verify_spawnkey(&as_json(&swap_spawn), U, IDS[0], &sp, &kp).is_err());
        assert!(sign_spawnkey(&dev, &[78u8; 32], &st).is_err());
    }

    #[test]
    fn a_presence_statement_keeps_its_purpose_and_never_reads_as_spawn() {
        let dev = key(1);
        let secret = [77u8; 32];
        let st = SpawnStatement {
            purpose: KeyPurpose::Presence,
            user_id: U,
            device_fp: dev.fingerprint(),
            device_id: IDS[0].into(),
            spawn_pub_b64: b64(&tmgr_phone::public_key(&secret).unwrap()),
            issued_at_ms: 5,
        };
        let text = st.to_statement().unwrap();
        assert!(text.starts_with("tmgr-presence-key:v1:42:"));
        let json = serde_json::to_string(&sign_spawnkey(&dev, &secret, &st).unwrap()).unwrap();
        let (sp, kp) = (dev.sign_pub_b64(), dev.kex_pub_b64());
        let got = verify_spawnkey(&json, U, IDS[0], &sp, &kp).unwrap();
        assert_eq!(got.purpose, KeyPurpose::Presence);
        let relabeled = json.replace("tmgr-presence-key", "tmgr-spawn-key");
        assert!(verify_spawnkey(&relabeled, U, IDS[0], &sp, &kp).is_err());
        let unknown = json.replace("tmgr-presence-key", "tmgr-other-key");
        assert_eq!(
            verify_spawnkey(&unknown, U, IDS[0], &sp, &kp).err(),
            Some(Error::BadFormat)
        );
    }

    #[test]
    fn chain_is_trusted_transitively_with_keys() {
        let (a, b, c) = (key(1), key(2), key(3));
        let d = dir(&[(IDS[0], &a), (IDS[1], &b), (IDS[2], &c)]);
        let intros = vec![
            intro((IDS[1], &b), (IDS[2], &c)),
            intro((IDS[0], &a), (IDS[1], &b)),
        ];
        let set = run(&[pin(IDS[0], &a)], &[], &intros, &d);
        assert_eq!(ids(&set), [IDS[0], IDS[1], IDS[2]]);
        assert_eq!(set.trusted[2], pin(IDS[2], &c));
        assert!(set.mismatches.is_empty());
    }

    #[test]
    fn revoked_hub_cascades() {
        let (a, b, c) = (key(1), key(2), key(3));
        let d = dir(&[(IDS[0], &a), (IDS[1], &b), (IDS[2], &c)]);
        let intros = vec![
            intro((IDS[0], &a), (IDS[1], &b)),
            intro((IDS[1], &b), (IDS[2], &c)),
        ];
        let set = run(&[pin(IDS[0], &a)], &[b.fingerprint()], &intros, &d);
        assert_eq!(ids(&set), [IDS[0]]);
        let dense = [intros, vec![intro((IDS[0], &a), (IDS[2], &c))]].concat();
        let set = run(&[pin(IDS[0], &a)], &[b.fingerprint()], &dense, &d);
        assert_eq!(ids(&set), [IDS[0], IDS[2]]);
    }

    #[test]
    fn revoked_pin_is_dropped_and_cannot_introduce() {
        let (a, b, c) = (key(1), key(2), key(3));
        let d = dir(&[(IDS[0], &a), (IDS[1], &b), (IDS[2], &c)]);
        let intros = vec![intro((IDS[0], &a), (IDS[2], &c))];
        let set = run(
            &[pin(IDS[0], &a), pin(IDS[1], &b)],
            &[a.fingerprint()],
            &intros,
            &d,
        );
        assert_eq!(ids(&set), [IDS[1]]);
    }

    #[test]
    fn revoked_fp_cannot_reenter_under_a_new_id() {
        let (a, b) = (key(1), key(2));
        let d = dir(&[(IDS[0], &a), (IDS[3], &b)]);
        let intros = vec![intro((IDS[0], &a), (IDS[3], &b))];
        let set = run(&[pin(IDS[0], &a)], &[b.fingerprint()], &intros, &d);
        assert_eq!(ids(&set), [IDS[0]]);
    }

    #[test]
    fn pinned_keys_verify_intros_and_directory_mismatch_is_reported() {
        let (a, b, evil) = (key(1), key(2), key(66));
        let intros = vec![intro((IDS[0], &a), (IDS[1], &b))];
        let d = dir(&[(IDS[0], &evil), (IDS[1], &b)]);
        let set = run(&[pin(IDS[0], &a)], &[], &intros, &d);
        assert_eq!(ids(&set), [IDS[0], IDS[1]]);
        assert_eq!(set.trusted[0], pin(IDS[0], &a));
        assert_eq!(
            set.mismatches,
            [KeyMismatch {
                device_id: IDS[0].into(),
                pinned_fp: a.fingerprint(),
                directory_fp: evil.fingerprint(),
            }]
        );
        let forged = vec![intro((IDS[0], &evil), (IDS[3], &key(4)))];
        let d = dir(&[(IDS[0], &evil), (IDS[3], &key(4))]);
        let set = run(&[pin(IDS[0], &a)], &[], &forged, &d);
        assert_eq!(ids(&set), [IDS[0]]);
    }

    #[test]
    fn target_must_be_in_directory_with_matching_keys() {
        let (a, b, other) = (key(1), key(2), key(7));
        let intros = vec![intro((IDS[0], &a), (IDS[1], &b))];
        let missing = dir(&[(IDS[0], &a)]);
        assert_eq!(
            ids(&run(&[pin(IDS[0], &a)], &[], &intros, &missing)),
            [IDS[0]]
        );
        let wrong = dir(&[(IDS[0], &a), (IDS[1], &other)]);
        assert_eq!(
            ids(&run(&[pin(IDS[0], &a)], &[], &intros, &wrong)),
            [IDS[0]]
        );
    }

    #[test]
    fn by_mismatch_and_wrong_user_and_untrusted_introducer_are_ignored() {
        let (a, b, c) = (key(1), key(2), key(3));
        let d = dir(&[(IDS[0], &a), (IDS[1], &b), (IDS[2], &c)]);
        let by_mismatch = sign_intro(&a, IDS[3], &statement(&a, &b, IDS[1], U)).unwrap();
        assert_eq!(
            ids(&run(&[pin(IDS[0], &a)], &[], &[by_mismatch], &d)),
            [IDS[0]]
        );
        let by_other_trusted = sign_intro(&a, IDS[1], &statement(&a, &c, IDS[2], U)).unwrap();
        let set = run(
            &[pin(IDS[0], &a), pin(IDS[1], &b)],
            &[],
            &[by_other_trusted],
            &d,
        );
        assert_eq!(ids(&set), [IDS[0], IDS[1]]);
        let wrong_user = sign_intro(&a, IDS[0], &statement(&a, &b, IDS[1], 43)).unwrap();
        assert_eq!(
            ids(&run(&[pin(IDS[0], &a)], &[], &[wrong_user], &d)),
            [IDS[0]]
        );
        let stranger = intro((IDS[2], &c), (IDS[3], &key(4)));
        let d2 = dir(&[(IDS[0], &a), (IDS[2], &c), (IDS[3], &key(4))]);
        assert_eq!(
            ids(&run(&[pin(IDS[0], &a)], &[], &[stranger], &d2)),
            [IDS[0]]
        );
        let good = intro((IDS[0], &a), (IDS[1], &b));
        let tampered = good.replace(IDS[1], IDS[2]);
        assert_eq!(
            ids(&run(
                &[pin(IDS[0], &a)],
                &[],
                &[tampered, "garbage".into()],
                &d
            )),
            [IDS[0]]
        );
    }

    #[test]
    fn cached_intros_keep_trust_when_server_withholds() {
        let (a, b) = (key(1), key(2));
        let d = dir(&[(IDS[0], &a), (IDS[1], &b)]);
        let cached = vec![intro((IDS[0], &a), (IDS[1], &b))];
        assert_eq!(ids(&run(&[pin(IDS[0], &a)], &[], &[], &d)), [IDS[0]]);
        assert_eq!(
            ids(&run(&[pin(IDS[0], &a)], &[], &cached, &d)),
            [IDS[0], IDS[1]]
        );
    }

    #[test]
    fn pinned_id_is_not_overridden_by_intro_with_other_fp() {
        let (a, b, other) = (key(1), key(2), key(7));
        let d = dir(&[(IDS[0], &a), (IDS[1], &b)]);
        let intros = vec![intro((IDS[0], &a), (IDS[1], &other))];
        let pins = [pin(IDS[0], &a), pin(IDS[1], &b)];
        let set = run(&pins, &[], &intros, &d);
        assert_eq!(set.trusted, pins);
    }

    #[test]
    fn pins_with_inconsistent_fp_are_dropped() {
        let a = key(1);
        let mut bad = pin(IDS[0], &a);
        bad.fp = key(2).fingerprint();
        assert!(run(&[bad], &[], &[], &dir(&[(IDS[0], &a)]))
            .trusted
            .is_empty());
    }

    #[test]
    fn forged_intros_do_not_stall_and_duplicates_are_collapsed() {
        let (a, b, c) = (key(1), key(2), key(3));
        let d = dir(&[(IDS[0], &a), (IDS[1], &b), (IDS[2], &c)]);
        let mut forged = intro((IDS[0], &a), (IDS[1], &b));
        let real_sig = parse_intro(&forged).unwrap().sig;
        let other_sig = parse_intro(&intro((IDS[0], &key(9)), (IDS[1], &b)))
            .unwrap()
            .sig;
        forged = forged.replace(&real_sig, &other_sig);
        let mut intros = vec![forged; 50];
        intros.push(intro((IDS[0], &a), (IDS[2], &c)));
        let set = run(&[pin(IDS[0], &a)], &[], &intros, &d);
        assert_eq!(ids(&set), [IDS[0], IDS[2]]);
    }

    fn intro_k(
        version: u8,
        by: (&str, &DeviceKeys),
        target: (&str, &DeviceKeys),
        kind: &str,
    ) -> String {
        let st = IntroStatement {
            version,
            user_id: U,
            introducer_fp: by.1.fingerprint(),
            target_id: target.0.into(),
            target_fp: target.1.fingerprint(),
            target_kind: kind.into(),
            issued_at_ms: 1_800_000_000_000,
        };
        sign_intro(by.1, by.0, &st).unwrap()
    }

    fn kmap(entries: &[(&str, &DeviceKeys, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(id, k, kind)| (kind_key(id, &k.fingerprint()), (*kind).to_owned()))
            .collect()
    }

    fn by_kind(
        pins: &[Trusted],
        intros: &[String],
        d: &[DirEntry],
        known: &BTreeMap<String, String>,
        cap: Option<usize>,
    ) -> TrustSet {
        trusted_set_by_kind(
            U,
            pins,
            &[],
            intros,
            d,
            Some(&KindCtx {
                known,
                own: None,
                assumed: &BTreeSet::new(),
                max_passes: cap,
            }),
        )
    }

    fn learned(set: &TrustSet, id: &str) -> Option<String> {
        set.kinds
            .iter()
            .find(|(i, _, _)| i == id)
            .map(|(_, _, k)| k.clone())
    }

    #[test]
    fn m01_v2_intro_round_trips_and_the_version_is_signed() {
        let (a, x) = (key(1), key(2));
        let json = intro_k(2, (IDS[0], &a), (IDS[1], &x), "mobile");
        let v = verify_intro(&json, U, IDS[0], &a.sign_pub_b64(), &a.kex_pub_b64()).unwrap();
        assert_eq!(v.parsed.version, 2);
        assert_eq!(
            v.statement,
            format!(
                "tmgr-chat-intro:v2:42:{}:{}:{}:mobile:1800000000000",
                a.fingerprint(),
                IDS[1],
                x.fingerprint()
            )
        );
        assert!(json.starts_with("{\"v\":2,"));
        assert!(parts(&v.statement, "tmgr-chat-intro", 8).is_err());
        let retagged = json.replace("tmgr-chat-intro:v2:", "tmgr-chat-intro:v1:");
        assert!(verify_intro(&retagged, U, IDS[0], &a.sign_pub_b64(), &a.kex_pub_b64()).is_err());
        let wrong_env = json.replacen("{\"v\":2,", "{\"v\":1,", 1);
        assert!(parse_intro(&wrong_env).is_err());
    }

    #[test]
    fn m02_v1_intro_counts_only_for_a_target_with_a_known_owner_kind() {
        let (a, x) = (key(1), key(2));
        let d = dir(&[(IDS[1], &x)]);
        let v1 = intro_k(1, (IDS[0], &a), (IDS[1], &x), "desktop");
        let pins = [pin(IDS[0], &a)];
        let none = by_kind(
            &pins,
            std::slice::from_ref(&v1),
            &d,
            &kmap(&[(IDS[0], &a, "desktop")]),
            None,
        );
        assert_eq!(ids(&none), [IDS[0]]);
        assert!(none.kinds.is_empty());
        let known = kmap(&[(IDS[0], &a, "desktop"), (IDS[1], &x, "mobile")]);
        let some = by_kind(&pins, std::slice::from_ref(&v1), &d, &known, None);
        assert_eq!(ids(&some), [IDS[0], IDS[1]]);
        assert!(some.kinds.is_empty());
        let runner = kmap(&[(IDS[0], &a, "desktop"), (IDS[1], &x, "runner")]);
        assert_eq!(ids(&by_kind(&pins, &[v1], &d, &runner, None)), [IDS[0]]);
    }

    #[test]
    fn v1_intros_are_ignored_once_every_owner_device_has_a_v2_self_intro() {
        let (a, x, y) = (key(1), key(2), key(3));
        let d = dir(&[(IDS[1], &x), (IDS[2], &y)]);
        let pins = [pin(IDS[0], &a), pin(IDS[1], &x)];
        let known = kmap(&[
            (IDS[0], &a, "desktop"),
            (IDS[1], &x, "desktop"),
            (IDS[2], &y, "desktop"),
        ]);
        let v1 = intro_k(1, (IDS[0], &a), (IDS[2], &y), "desktop");
        let marker = intro_k(2, (IDS[1], &x), (IDS[1], &x), "desktop");
        let marker_y = intro_k(2, (IDS[2], &y), (IDS[2], &y), "desktop");
        let own = kind_key(IDS[0], &a.fingerprint());
        let run = |intros: &[String]| {
            trusted_set_by_kind(
                U,
                &pins,
                &[],
                intros,
                &d,
                Some(&KindCtx {
                    known: &known,
                    own: Some(&own),
                    assumed: &BTreeSet::new(),
                    max_passes: None,
                }),
            )
        };
        let closed = run(&[v1.clone(), marker.clone()]);
        assert!(ids(&closed).contains(&IDS[2].to_owned()));
        let open = run(&[v1, marker, marker_y]);
        assert!(!ids(&open).contains(&IDS[2].to_owned()));
    }

    #[test]
    fn a_self_intro_yields_no_kind_and_no_trust() {
        let (a, x) = (key(1), key(2));
        let d = dir(&[(IDS[1], &x)]);
        let me = intro_k(2, (IDS[0], &a), (IDS[0], &a), "runner");
        let set = by_kind(
            &[pin(IDS[0], &a)],
            std::slice::from_ref(&me),
            &d,
            &kmap(&[(IDS[0], &a, "desktop")]),
            None,
        );
        assert!(set.kinds.is_empty() && set.conflicts.is_empty());
        let trusted = [pin(IDS[0], &a)];
        let markers = v2_self_markers(U, &trusted, &kmap(&[(IDS[0], &a, "desktop")]), &[me]);
        assert!(markers.contains(&kind_key(IDS[0], &a.fingerprint())));
    }

    #[test]
    fn m05_replayed_intros_for_a_revoked_fp_or_new_keys_do_not_count() {
        let (a, x, x2) = (key(1), key(2), key(7));
        let old = intro_k(2, (IDS[0], &a), (IDS[1], &x), "desktop");
        let pins = [pin(IDS[0], &a)];
        let known = kmap(&[(IDS[0], &a, "desktop")]);
        let revoked = trusted_set_by_kind(
            U,
            &pins,
            &[x.fingerprint()],
            std::slice::from_ref(&old),
            &dir(&[(IDS[1], &x)]),
            Some(&KindCtx {
                known: &known,
                own: None,
                assumed: &BTreeSet::new(),
                max_passes: None,
            }),
        );
        assert_eq!(ids(&revoked), [IDS[0]]);
        let rekeyed = by_kind(&pins, &[old], &dir(&[(IDS[1], &x2)]), &known, None);
        assert_eq!(ids(&rekeyed), [IDS[0]]);
        assert!(rekeyed.kinds.is_empty());
    }

    fn orders(items: &[String]) -> Vec<Vec<String>> {
        let n = items.len();
        let mut out = Vec::new();
        let mut idx: Vec<usize> = (0..n).collect();
        fn go(k: usize, idx: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
            if k == idx.len() {
                out.push(idx.clone());
                return;
            }
            for i in k..idx.len() {
                idx.swap(k, i);
                go(k + 1, idx, out);
                idx.swap(k, i);
            }
        }
        let mut perms = Vec::new();
        go(0, &mut idx, &mut perms);
        for p in perms {
            out.push(p.into_iter().map(|i| items[i].clone()).collect());
        }
        out
    }

    #[test]
    fn m06_a_late_demotion_drops_what_the_demoted_device_vouched_for() {
        let (a, b, x, y) = (key(1), key(2), key(3), key(4));
        let d = dir(&[(IDS[2], &x), (IDS[3], &y)]);
        let pins = [pin(IDS[0], &a), pin(IDS[1], &b)];
        let items = [
            intro_k(2, (IDS[0], &a), (IDS[2], &x), "desktop"),
            intro_k(2, (IDS[2], &x), (IDS[3], &y), "desktop"),
            intro_k(2, (IDS[1], &b), (IDS[2], &x), "runner"),
        ];
        for known_x in [false, true] {
            let mut known = kmap(&[(IDS[0], &a, "desktop"), (IDS[1], &b, "desktop")]);
            if known_x {
                known.extend(kmap(&[(IDS[2], &x, "desktop")]));
            }
            for order in orders(&items) {
                let set = by_kind(&pins, &order, &d, &known, None);
                assert!(ids(&set).contains(&IDS[2].to_owned()));
                assert!(!ids(&set).contains(&IDS[3].to_owned()), "{known_x}");
                assert_eq!(learned(&set, IDS[2]).as_deref(), Some("runner"));
            }
        }
    }

    #[test]
    fn m07_runner_and_desktop_intros_for_one_target_give_runner_in_both_orders() {
        let (a, b, x) = (key(1), key(2), key(3));
        let d = dir(&[(IDS[2], &x)]);
        let pins = [pin(IDS[0], &a), pin(IDS[1], &b)];
        let known = kmap(&[(IDS[0], &a, "desktop"), (IDS[1], &b, "desktop")]);
        let by_a = intro_k(2, (IDS[0], &a), (IDS[2], &x), "desktop");
        let by_b = intro_k(2, (IDS[1], &b), (IDS[2], &x), "runner");
        for order in [[by_a.clone(), by_b.clone()], [by_b, by_a]] {
            let set = by_kind(&pins, &order, &d, &known, None);
            assert_eq!(learned(&set, IDS[2]).as_deref(), Some("runner"));
            assert_eq!(set.conflicts.len(), 1);
        }
    }

    #[test]
    fn disagreeing_owner_kinds_without_a_known_value_leave_the_kind_unknown() {
        let (a, b, x) = (key(1), key(2), key(3));
        let d = dir(&[(IDS[2], &x)]);
        let pins = [pin(IDS[0], &a), pin(IDS[1], &b)];
        let mut known = kmap(&[(IDS[0], &a, "desktop"), (IDS[1], &b, "desktop")]);
        let items = [
            intro_k(2, (IDS[0], &a), (IDS[2], &x), "desktop"),
            intro_k(2, (IDS[1], &b), (IDS[2], &x), "mobile"),
        ];
        for order in orders(&items) {
            let set = by_kind(&pins, &order, &d, &known, None);
            assert!(ids(&set).contains(&IDS[2].to_owned()));
            assert_eq!(learned(&set, IDS[2]), None);
            assert_eq!(set.conflicts.len(), 1);
        }
        known.extend(kmap(&[(IDS[2], &x, "mobile")]));
        let set = by_kind(&pins, &items, &d, &known, None);
        assert_eq!(learned(&set, IDS[2]), None);
        assert_eq!(set.conflicts.len(), 1);
    }

    #[test]
    fn m18_a_chain_settles_within_the_bound_and_a_lower_cap_flags_trust_unstable() {
        let keys: Vec<DeviceKeys> = (1..=5).map(key).collect();
        let d: Vec<DirEntry> = (1..5)
            .map(|i| dir(&[(IDS[i], &keys[i])]).remove(0))
            .collect();
        let pins = [pin(IDS[0], &keys[0])];
        let items: Vec<String> = (0..4)
            .map(|i| intro_k(2, (IDS[i], &keys[i]), (IDS[i + 1], &keys[i + 1]), "desktop"))
            .collect();
        let known = kmap(&[(IDS[0], &keys[0], "desktop")]);
        let set = by_kind(&pins, &items, &d, &known, None);
        assert_eq!(set.trusted.len(), 5);
        assert!(set.unstable.is_empty());
        assert!(set.passes <= 2 * 6 + 1);
        let capped = by_kind(&pins, &items, &d, &known, Some(1));
        assert_eq!(capped.passes, 1);
        assert!(!capped.unstable.is_empty());
        assert!(capped
            .trusted
            .iter()
            .all(|t| !capped.unstable.contains(&t.device_id)));
    }

    #[test]
    fn join_code_v2_carries_the_kind_and_v1_is_still_decoded() {
        let mut code = JoinCode {
            version: 2,
            kind: Some("mobile".into()),
            user_id: U,
            device_id: IDS[0].into(),
            fp: "ab".repeat(32),
            secret: [3u8; 32],
            exp_ms: 9,
        };
        let enc = encode_join_code(&code).unwrap();
        assert!(enc.starts_with(JOIN_PREFIX_V2));
        assert_eq!(decode_join_code(&enc).unwrap(), code);
        code.kind = Some("runner".into());
        assert!(encode_join_code(&code).is_err());
        code.kind = None;
        assert!(encode_join_code(&code).is_err());
        code.version = 1;
        let v1 = encode_join_code(&code).unwrap();
        assert!(v1.starts_with(JOIN_PREFIX));
        assert_eq!(decode_join_code(&v1).unwrap().version, 1);
        let body = URL_SAFE_NO_PAD.encode(
            r#"{"v":2,"user_id":42,"device_id":"00000000-0000-4000-8000-00000000000a","fp":"abababababababababababababababababababababababababababababababab","kind":"runner","secret":"AwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwM=","exp":9}"#,
        );
        assert!(decode_join_code(&format!("{JOIN_PREFIX_V2}{body}")).is_err());
        assert!(decode_join_code(&format!("{JOIN_PREFIX}{body}")).is_err());
    }

    #[test]
    fn join_mac_transcript_is_versioned() {
        let secret = [5u8; 32];
        let (pf, nf) = ("a".repeat(64), "b".repeat(64));
        let mk = |version| JoinMacInput {
            version,
            user_id: U,
            p_fp: &pf,
            n_id: IDS[1],
            n_fp: &nf,
            n_kind: "runner",
            label: "Runner",
        };
        let m2 = join_mac(&secret, &mk(2)).unwrap();
        assert!(verify_join_mac(&secret, &mk(2), &m2).is_ok());
        assert!(verify_join_mac(&secret, &mk(1), &m2).is_err());
        assert!(join_mac(&secret, &mk(3)).is_err());
    }
}
