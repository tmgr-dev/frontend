use crate::fields::{check_device_id, check_device_kind, check_fp, parse_decimal, parse_strict};
use crate::keys::{fingerprint, DeviceKeys};
use crate::{Error, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use subtle::ConstantTimeEq;
use tmgr_phone::{b64, b64_decode, h, pair_mac, sign, verify_der};

pub const JOIN_PREFIX: &str = "tmgr-chat-join:v1:";

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
    pub user_id: u64,
    pub introducer_fp: String,
    pub target_id: String,
    pub target_fp: String,
    pub target_kind: String,
    pub issued_at_ms: u64,
}

impl IntroStatement {
    pub fn to_statement(&self) -> Result<String> {
        check_fp(&self.introducer_fp)?;
        check_device_id(&self.target_id)?;
        check_fp(&self.target_fp)?;
        check_device_kind(&self.target_kind)?;
        Ok(format!(
            "tmgr-chat-intro:v1:{}:{}:{}:{}:{}:{}",
            self.user_id,
            self.introducer_fp,
            self.target_id,
            self.target_fp,
            self.target_kind,
            self.issued_at_ms
        ))
    }

    pub fn parse(s: &str) -> Result<IntroStatement> {
        let p = parts(s, "tmgr-chat-intro", 8)?;
        let st = IntroStatement {
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
        v: 1,
        by: signer_id.to_string(),
        statement,
        sig,
    })
    .map_err(|_| Error::BadJson)
}

pub fn parse_intro(json: &str) -> Result<Intro> {
    let j: IntroJson = parse_strict(json)?;
    if j.v != 1 {
        return Err(Error::BadFormat);
    }
    check_device_id(&j.by)?;
    let parsed = IntroStatement::parse(&j.statement)?;
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnStatement {
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
            "tmgr-spawn-key:v1:{}:{}:{}:{}:{}",
            self.user_id, self.device_fp, self.device_id, self.spawn_pub_b64, self.issued_at_ms
        ))
    }

    pub fn parse(s: &str) -> Result<SpawnStatement> {
        let p = parts(s, "tmgr-spawn-key", 7)?;
        let st = SpawnStatement {
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
    pub user_id: u64,
    pub device_id: String,
    pub fp: String,
    pub secret: [u8; 32],
    pub exp_ms: u64,
}

impl std::fmt::Debug for JoinCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JoinCode")
            .field("user_id", &self.user_id)
            .field("device_id", &self.device_id)
            .field("fp", &self.fp)
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
    secret: String,
    exp: u64,
}

impl JoinCode {
    pub fn is_expired(&self, now_ms: u64) -> bool {
        now_ms >= self.exp_ms
    }
}

pub fn encode_join_code(code: &JoinCode) -> Result<String> {
    check_device_id(&code.device_id)?;
    check_fp(&code.fp)?;
    let json = serde_json::to_string(&JoinJson {
        v: 1,
        user_id: code.user_id,
        device_id: code.device_id.clone(),
        fp: code.fp.clone(),
        secret: b64(&code.secret),
        exp: code.exp_ms,
    })
    .map_err(|_| Error::BadJson)?;
    Ok(format!("{JOIN_PREFIX}{}", URL_SAFE_NO_PAD.encode(json)))
}

pub fn decode_join_code(s: &str) -> Result<JoinCode> {
    let enc = s.strip_prefix(JOIN_PREFIX).ok_or(Error::BadFormat)?;
    let raw = URL_SAFE_NO_PAD.decode(enc).map_err(|_| Error::BadFormat)?;
    let j: JoinJson = parse_strict(std::str::from_utf8(&raw).map_err(|_| Error::BadJson)?)?;
    if j.v != 1 {
        return Err(Error::BadFormat);
    }
    check_device_id(&j.device_id)?;
    check_fp(&j.fp)?;
    let secret: [u8; 32] = b64_decode(&j.secret)?
        .try_into()
        .map_err(|_| Error::BadFormat)?;
    Ok(JoinCode {
        user_id: j.user_id,
        device_id: j.device_id,
        fp: j.fp,
        secret,
        exp_ms: j.exp,
    })
}

pub struct JoinMacInput<'a> {
    pub user_id: u64,
    pub p_fp: &'a str,
    pub n_id: &'a str,
    pub n_fp: &'a str,
    pub n_kind: &'a str,
    pub label: &'a str,
}

fn join_transcript(i: &JoinMacInput) -> Result<String> {
    check_fp(i.p_fp)?;
    check_device_id(i.n_id)?;
    check_fp(i.n_fp)?;
    check_device_kind(i.n_kind)?;
    Ok(format!(
        "tmgr-chat-join:v1:{}:{}:{}:{}:{}:{}",
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

pub fn lowers_kind(have: &str, new: &str) -> bool {
    new == "runner" && have != "runner"
}

pub fn may_vouch(kind: Option<&str>) -> bool {
    matches!(kind, Some("desktop" | "mobile"))
}

/// With `kinds` (`device_id:fp` -> signed kind) only desktop and mobile devices may vouch; a runner key is readable by agents.
pub fn trusted_set_by_kind(
    user_id: u64,
    pins: &[Trusted],
    revoked: &[String],
    intros: &[String],
    directory: &[DirEntry],
    kinds: Option<&std::collections::BTreeMap<String, String>>,
) -> TrustSet {
    let mut known = kinds.map(|k| {
        k.iter()
            .filter(|(_, v)| crate::fields::check_device_kind(v).is_ok())
            .map(|(a, b)| (a.clone(), b.clone()))
            .collect::<std::collections::BTreeMap<_, _>>()
    });
    let is_revoked = |fp: &str| revoked.iter().any(|r| r == fp);
    let mut trusted: Vec<Trusted> = Vec::new();
    for p in pins {
        if is_revoked(&p.fp)
            || p.fp != fingerprint(&p.sign_pub, &p.kex_pub)
            || trusted.iter().any(|t| t.device_id == p.device_id)
        {
            continue;
        }
        trusted.push(p.clone());
    }
    let mut seen = HashSet::new();
    let parsed: Vec<Intro> = intros
        .iter()
        .filter(|s| seen.insert(s.as_str()))
        .filter_map(|s| parse_intro(s).ok())
        .filter(|i| i.parsed.user_id == user_id)
        .collect();
    let mut done = vec![false; parsed.len()];
    let mut learned: Vec<(String, String, String)> = Vec::new();
    loop {
        let mut changed = false;
        for (n, intro) in parsed.iter().enumerate() {
            let st = &intro.parsed;
            if done[n] || is_revoked(&st.target_fp) {
                continue;
            }
            let target_pinned = match trusted.iter().find(|t| t.device_id == st.target_id) {
                Some(t) if t.fp == st.target_fp => true,
                Some(_) => continue,
                None => false,
            };
            let Some(introducer) = trusted
                .iter()
                .find(|t| t.device_id == intro.by && t.fp == st.introducer_fp)
            else {
                continue;
            };
            if known.as_ref().is_some_and(|k| {
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
                let Some(entry) = directory.iter().find(|d| {
                    d.device_id == st.target_id
                        && fingerprint(&d.sign_pub_b64, &d.kex_pub_b64) == st.target_fp
                }) else {
                    continue;
                };
                Some(entry)
            };
            let verified = b64_decode(&introducer.sign_pub)
                .and_then(|key| verify_der(&key, &intro.statement, &intro.sig))
                .is_ok();
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
            if let Some(k) = known.as_mut() {
                let key = kind_key(&st.target_id, &st.target_fp);
                if k.get(&key)
                    .is_none_or(|have| lowers_kind(have, &st.target_kind))
                {
                    k.insert(key, st.target_kind.clone());
                }
                learned.push((
                    st.target_id.clone(),
                    st.target_fp.clone(),
                    st.target_kind.clone(),
                ));
            }
            changed = true;
        }
        if !changed {
            break;
        }
    }
    trusted.sort();
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
    TrustSet {
        trusted,
        mismatches,
        kinds: learned,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            format!("tmgr-chat-intro:v2:42:{fp}:{}:{fp}:mobile:7", IDS[0]),
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
}
