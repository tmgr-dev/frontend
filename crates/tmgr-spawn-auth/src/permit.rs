use crate::proposal::sha256_hex;
use crate::tokens::{
    is_hex_lower, is_session_id, is_target_id, is_token, parse_p256_key, MAX_SAFE_INT,
};
use crate::verify::{verify_der_b64, PinnedKey, TrustStatus, TrustView};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

pub const MAX_PERMIT_TTL_MS: u64 = 120_000;
const MAX_SHORT: usize = 256;
const MAX_TEXT: usize = 8192;
const MAX_LONG: usize = 1024;
const MAX_DANGER: usize = 16;
const CARD_MAGIC: &[u8] = b"tmgr-permit-card:v1\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermitChoice {
    Yes,
    No,
    Always,
}

impl PermitChoice {
    fn as_str(self) -> &'static str {
        match self {
            PermitChoice::Yes => "yes",
            PermitChoice::No => "no",
            PermitChoice::Always => "always",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermitScreen {
    pub agent: String,
    pub tool: String,
    pub text: String,
    pub path: Option<String>,
    pub reason: Option<String>,
    pub from: Option<String>,
    pub warning: Option<String>,
    pub danger: Vec<String>,
    pub always_label: Option<String>,
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_be_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn put_opt(out: &mut Vec<u8>, s: &Option<String>) {
    match s {
        None => out.push(0),
        Some(s) => {
            out.push(1);
            put_str(out, s);
        }
    }
}

impl PermitScreen {
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = CARD_MAGIC.to_vec();
        put_str(&mut out, &self.agent);
        put_str(&mut out, &self.tool);
        put_str(&mut out, &self.text);
        put_opt(&mut out, &self.path);
        put_opt(&mut out, &self.reason);
        put_opt(&mut out, &self.from);
        put_opt(&mut out, &self.warning);
        out.extend_from_slice(&(self.danger.len() as u32).to_be_bytes());
        for d in &self.danger {
            put_str(&mut out, d);
        }
        put_opt(&mut out, &self.always_label);
        out
    }

    pub fn req_sha256(&self) -> String {
        sha256_hex(&self.canonical_bytes())
    }

    pub fn is_danger(&self) -> bool {
        !self.danger.is_empty() || self.warning.is_some()
    }

    pub fn always_offered(&self) -> bool {
        self.always_label.is_some()
    }

    pub fn validate(&self) -> Result<(), PermitRefusal> {
        let opt_ok = |o: &Option<String>| o.as_ref().is_none_or(|s| s.len() <= MAX_LONG);
        let ok = self.agent.len() <= MAX_SHORT
            && self.tool.len() <= MAX_SHORT
            && self.text.len() <= MAX_TEXT
            && opt_ok(&self.path)
            && opt_ok(&self.reason)
            && opt_ok(&self.from)
            && opt_ok(&self.warning)
            && opt_ok(&self.always_label)
            && self.danger.len() <= MAX_DANGER
            && self.danger.iter().all(|d| d.len() <= MAX_LONG);
        if ok {
            Ok(())
        } else {
            Err(PermitRefusal::BadToken)
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermitKey {
    #[default]
    Chat,
    Presence,
}

impl PermitKey {
    pub fn is_chat(&self) -> bool {
        *self == PermitKey::Chat
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedPermit {
    pub v: u32,
    pub user_id: String,
    pub device_fp: String,
    pub runner_id: String,
    pub runner_fp: String,
    pub card_id: String,
    pub session: String,
    pub choice: PermitChoice,
    pub confirmed: u8,
    pub nonce: String,
    #[serde(deserialize_with = "safe_int")]
    pub ts_ms: u64,
    #[serde(deserialize_with = "safe_int")]
    pub exp_ms: u64,
    pub req_sha256: String,
    pub alg: String,
    pub sig: String,
    #[serde(default, skip_serializing_if = "PermitKey::is_chat")]
    pub k: PermitKey,
}

fn safe_int<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let n = u64::deserialize(d)?;
    if n > MAX_SAFE_INT {
        return Err(serde::de::Error::custom("integer above 2^53-1"));
    }
    Ok(n)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermitRecord {
    pub user_id: String,
    pub runner_id: String,
    pub runner_fp: String,
    pub card_id: String,
    pub session: String,
    pub nonce: String,
    pub issued_ms: u64,
    pub exp_ms: u64,
    pub screen: PermitScreen,
}

pub fn permit_signed_string(record: &PermitRecord, key: &PinnedKey, sig: &SignedPermit) -> String {
    let domain = match sig.k {
        PermitKey::Chat => "tmgr-permit:v1",
        PermitKey::Presence => "tmgr-permit-presence:v1",
    };
    format!(
        "{domain}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
        record.user_id,
        key.device_fp,
        record.runner_id,
        record.runner_fp,
        record.card_id,
        record.session,
        sig.choice.as_str(),
        sig.confirmed,
        record.nonce,
        sig.ts_ms,
        record.exp_ms,
        record.screen.req_sha256()
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermitOutcome {
    Drop,
    Reissue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermitRefusal {
    NotOwnerDevice,
    SenderMismatch,
    DesktopNotAllowed,
    NotTrusted,
    UnknownKey,
    Stale,
    Expired,
    NeedsConfirm,
    BadChoice,
    ScreenMismatch,
    BadToken,
    BadTtl,
    UnsupportedVersion,
    UnsupportedAlg,
    BadSig,
}

impl PermitRefusal {
    pub fn code(self) -> &'static str {
        match self {
            PermitRefusal::NotOwnerDevice => "not_owner_device",
            PermitRefusal::SenderMismatch => "sender_mismatch",
            PermitRefusal::DesktopNotAllowed => "desktop_not_allowed",
            PermitRefusal::NotTrusted => "not_trusted",
            PermitRefusal::UnknownKey => "unknown_key",
            PermitRefusal::Stale => "stale",
            PermitRefusal::Expired => "expired",
            PermitRefusal::NeedsConfirm => "needs_confirm",
            PermitRefusal::BadChoice => "bad_choice",
            PermitRefusal::ScreenMismatch => "screen_mismatch",
            PermitRefusal::BadToken => "bad_token",
            PermitRefusal::BadTtl => "bad_ttl",
            PermitRefusal::UnsupportedVersion => "unsupported_version",
            PermitRefusal::UnsupportedAlg => "unsupported_alg",
            PermitRefusal::BadSig => "bad_sig",
        }
    }

    pub fn outcome(self) -> PermitOutcome {
        match self {
            PermitRefusal::NotOwnerDevice
            | PermitRefusal::SenderMismatch
            | PermitRefusal::DesktopNotAllowed
            | PermitRefusal::NotTrusted
            | PermitRefusal::UnknownKey
            | PermitRefusal::Stale
            | PermitRefusal::BadSig => PermitOutcome::Drop,
            PermitRefusal::Expired
            | PermitRefusal::NeedsConfirm
            | PermitRefusal::BadChoice
            | PermitRefusal::ScreenMismatch
            | PermitRefusal::BadToken
            | PermitRefusal::BadTtl
            | PermitRefusal::UnsupportedVersion
            | PermitRefusal::UnsupportedAlg => PermitOutcome::Reissue,
        }
    }
}

impl fmt::Display for PermitRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            PermitRefusal::NotOwnerDevice => "permission answer did not come from an owner device",
            PermitRefusal::SenderMismatch => {
                "permission answer sender does not match the signing device"
            }
            PermitRefusal::DesktopNotAllowed => {
                "permission answers from a desktop are not allowed for this card"
            }
            PermitRefusal::NotTrusted => "permission answer device is not trusted",
            PermitRefusal::UnknownKey => "permission answer has no usable pinned key",
            PermitRefusal::Stale => "permission answer is for another or an older card",
            PermitRefusal::Expired => "permission answer expired",
            PermitRefusal::NeedsConfirm => "permission answer needs an explicit confirmation",
            PermitRefusal::BadChoice => "permission answer choice is not offered on this card",
            PermitRefusal::ScreenMismatch => {
                "permission answer does not match the card that was shown"
            }
            PermitRefusal::BadToken => "permission answer field has an invalid format",
            PermitRefusal::BadTtl => "permission answer card lifetime is invalid",
            PermitRefusal::UnsupportedVersion => "permission answer version is unsupported",
            PermitRefusal::UnsupportedAlg => "permission answer algorithm is unsupported",
            PermitRefusal::BadSig => "permission answer signature is invalid",
        };
        f.write_str(text)
    }
}

impl std::error::Error for PermitRefusal {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermitSenderKind {
    Mobile,
    Desktop,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DesktopPolicy {
    None,
    Safe,
    All,
    #[default]
    Presence,
}

impl DesktopPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            DesktopPolicy::None => "none",
            DesktopPolicy::Safe => "safe",
            DesktopPolicy::All => "all",
            DesktopPolicy::Presence => "presence",
        }
    }

    /// Whether an answer signed with the device's chat key (mobile: its spawn key) is enough.
    pub fn allows(self, kind: PermitSenderKind, screen: &PermitScreen) -> bool {
        self.allows_key(kind, PermitKey::Chat, screen)
    }

    /// Whether a desktop may answer this card with a presence-key signature.
    pub fn wants_presence(self, screen: &PermitScreen) -> bool {
        self == DesktopPolicy::Presence && screen.is_danger()
    }

    pub fn allows_key(self, kind: PermitSenderKind, k: PermitKey, screen: &PermitScreen) -> bool {
        match (kind, k) {
            (PermitSenderKind::Mobile, PermitKey::Chat) => true,
            (PermitSenderKind::Mobile, PermitKey::Presence) => false,
            (PermitSenderKind::Desktop, k) => match self {
                DesktopPolicy::None => false,
                DesktopPolicy::Safe => k.is_chat() && !screen.is_danger(),
                DesktopPolicy::All => true,
                DesktopPolicy::Presence => !k.is_chat() || !screen.is_danger(),
            },
        }
    }
}

impl std::str::FromStr for DesktopPolicy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(DesktopPolicy::None),
            "safe" => Ok(DesktopPolicy::Safe),
            "all" => Ok(DesktopPolicy::All),
            "presence" => Ok(DesktopPolicy::Presence),
            _ => Err(format!(
                "desktop policy must be none, safe, all or presence, got {s:?}"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PermitSender<'a> {
    pub device_fp: &'a str,
    pub owner_device: bool,
    pub kind: PermitSenderKind,
    pub desktop: DesktopPolicy,
    /// The sender's approved presence key, if any (desktop only).
    pub presence: Option<&'a PinnedKey>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermittedAnswer {
    device_fp: String,
    card_id: String,
    nonce: String,
    session: String,
    choice: PermitChoice,
    req_sha256: String,
}

impl PermittedAnswer {
    pub fn device_fp(&self) -> &str {
        &self.device_fp
    }
    pub fn card_id(&self) -> &str {
        &self.card_id
    }
    pub fn nonce(&self) -> &str {
        &self.nonce
    }
    pub fn session(&self) -> &str {
        &self.session
    }
    pub fn choice(&self) -> PermitChoice {
        self.choice
    }
    pub fn req_sha256(&self) -> &str {
        &self.req_sha256
    }
}

/// Caller burns the nonce only when the result is Ok or `refusal.outcome() != Drop`.
/// The signature is checked before any outcome that burns, reissues or closes.
pub fn verify_permit(
    sig: &SignedPermit,
    record: &PermitRecord,
    key: &PinnedKey,
    sender: PermitSender<'_>,
    trust: &dyn TrustView,
    now_ms: u64,
) -> Result<PermittedAnswer, PermitRefusal> {
    if !sender.owner_device {
        return Err(PermitRefusal::NotOwnerDevice);
    }
    if !sender
        .desktop
        .allows_key(sender.kind, sig.k, &record.screen)
    {
        return Err(PermitRefusal::DesktopNotAllowed);
    }
    let key = match sig.k {
        PermitKey::Chat => key,
        PermitKey::Presence => sender.presence.ok_or(PermitRefusal::UnknownKey)?,
    };
    if sender.device_fp != sig.device_fp || sig.device_fp != key.device_fp {
        return Err(PermitRefusal::SenderMismatch);
    }
    if trust.status(&key.device_id, &key.device_fp) != TrustStatus::Trusted {
        return Err(PermitRefusal::NotTrusted);
    }
    let verifying = parse_p256_key(&key.spawn_pub).ok_or(PermitRefusal::UnknownKey)?;
    let same = sig.user_id == record.user_id
        && key.user_id == record.user_id
        && sig.runner_id == record.runner_id
        && sig.runner_fp == record.runner_fp
        && sig.card_id == record.card_id
        && sig.session == record.session
        && sig.nonce == record.nonce;
    if !same {
        return Err(PermitRefusal::Stale);
    }
    if record.exp_ms > MAX_SAFE_INT {
        return Err(PermitRefusal::BadToken);
    }
    if record.exp_ms <= record.issued_ms
        || record.exp_ms > record.issued_ms.saturating_add(MAX_PERMIT_TTL_MS)
    {
        return Err(PermitRefusal::BadTtl);
    }
    let tokens_ok = is_token(&record.user_id)
        && is_token(&key.device_id)
        && is_token(&record.card_id)
        && is_target_id(&record.runner_id)
        && is_session_id(&record.session)
        && is_hex_lower(&key.device_fp, 64)
        && is_hex_lower(&record.runner_fp, 64)
        && is_hex_lower(&record.nonce, 32);
    if !tokens_ok {
        return Err(PermitRefusal::BadToken);
    }
    record.screen.validate()?;
    let message = permit_signed_string(record, key, sig);
    verify_der_b64(&verifying, &message, &sig.sig).map_err(|_| PermitRefusal::BadSig)?;
    if sig.v != 1 {
        return Err(PermitRefusal::UnsupportedVersion);
    }
    if sig.alg != "es256" {
        return Err(PermitRefusal::UnsupportedAlg);
    }
    if sig.ts_ms > MAX_SAFE_INT
        || sig.exp_ms > MAX_SAFE_INT
        || !is_hex_lower(&sig.req_sha256, 64)
        || sig.confirmed > 1
    {
        return Err(PermitRefusal::BadToken);
    }
    let req_sha256 = record.screen.req_sha256();
    if sig.exp_ms != record.exp_ms || sig.req_sha256 != req_sha256 {
        return Err(PermitRefusal::ScreenMismatch);
    }
    if now_ms > record.exp_ms {
        return Err(PermitRefusal::Expired);
    }
    let screen = &record.screen;
    if sig.choice == PermitChoice::Always && !screen.always_offered() {
        return Err(PermitRefusal::BadChoice);
    }
    let needs_confirm = sig.choice == PermitChoice::Always
        || (screen.is_danger() && sig.choice == PermitChoice::Yes);
    if needs_confirm && sig.confirmed != 1 {
        return Err(PermitRefusal::NeedsConfirm);
    }
    Ok(PermittedAnswer {
        device_fp: key.device_fp.clone(),
        card_id: record.card_id.clone(),
        nonce: record.nonce.clone(),
        session: record.session.clone(),
        choice: sig.choice,
        req_sha256,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::b64_encode;
    use p256::ecdsa::signature::Signer;
    use p256::ecdsa::{Signature, SigningKey};

    struct Trust(TrustStatus);
    impl TrustView for Trust {
        fn status(&self, _: &str, _: &str) -> TrustStatus {
            self.0
        }
    }
    type EditS = Box<dyn Fn(&mut SignedPermit)>;
    type EditC = Box<dyn Fn(&mut PermitScreen)>;
    type EditK = Box<dyn Fn(&mut PermitRecord, &mut PinnedKey)>;
    const T: Trust = Trust(TrustStatus::Trusted);
    const NOW: u64 = 1_001_000;

    fn sk(b: u8) -> SigningKey {
        SigningKey::from_slice(&[b; 32]).unwrap()
    }

    fn screen() -> PermitScreen {
        PermitScreen {
            agent: "claude-code".into(),
            tool: "Bash".into(),
            text: "ls -la".into(),
            path: Some("/work/app".into()),
            reason: Some("list files".into()),
            from: Some("agent".into()),
            warning: None,
            danger: vec![],
            always_label: Some("Always allow ls".into()),
        }
    }

    fn record() -> PermitRecord {
        PermitRecord {
            user_id: "42".into(),
            runner_id: "mx_00aa".into(),
            runner_fp: "f".repeat(64),
            card_id: "card-1".into(),
            session: "s_abcdefghij".into(),
            nonce: "ab".repeat(16),
            issued_ms: 1_000_000,
            exp_ms: 1_000_000 + MAX_PERMIT_TTL_MS,
            screen: screen(),
        }
    }

    fn key() -> PinnedKey {
        PinnedKey {
            user_id: "42".into(),
            device_id: "dev-1".into(),
            device_fp: "d".repeat(64),
            spawn_pub: sk(3)
                .verifying_key()
                .to_encoded_point(false)
                .as_bytes()
                .to_vec(),
        }
    }

    fn signed(
        r: &PermitRecord,
        k: &PinnedKey,
        choice: PermitChoice,
        confirmed: u8,
    ) -> SignedPermit {
        let mut s = SignedPermit {
            v: 1,
            user_id: r.user_id.clone(),
            device_fp: k.device_fp.clone(),
            runner_id: r.runner_id.clone(),
            runner_fp: r.runner_fp.clone(),
            card_id: r.card_id.clone(),
            session: r.session.clone(),
            choice,
            confirmed,
            nonce: r.nonce.clone(),
            ts_ms: 1_000_500,
            exp_ms: r.exp_ms,
            req_sha256: r.screen.req_sha256(),
            alg: "es256".into(),
            sig: String::new(),
            k: PermitKey::Chat,
        };
        resign(&mut s, r, k);
        s
    }

    fn resign(s: &mut SignedPermit, r: &PermitRecord, k: &PinnedKey) {
        let sig: Signature = sk(3).sign(permit_signed_string(r, k, s).as_bytes());
        s.sig = b64_encode(sig.to_der().as_bytes());
    }

    fn run(
        s: &SignedPermit,
        r: &PermitRecord,
        k: &PinnedKey,
        trust: TrustStatus,
        now: u64,
    ) -> Result<PermittedAnswer, PermitRefusal> {
        let sender = PermitSender {
            device_fp: &s.device_fp,
            owner_device: true,
            kind: PermitSenderKind::Mobile,
            desktop: DesktopPolicy::Safe,
            presence: None,
        };
        verify_permit(s, r, k, sender, &Trust(trust), now)
    }

    fn ok(
        s: &SignedPermit,
        r: &PermitRecord,
        k: &PinnedKey,
    ) -> Result<PermittedAnswer, PermitRefusal> {
        run(s, r, k, TrustStatus::Trusted, NOW)
    }

    #[test]
    fn owner_devices_get_their_choice_back() {
        let (r, k) = (record(), key());
        for c in [PermitChoice::Yes, PermitChoice::No] {
            let a = ok(&signed(&r, &k, c, 0), &r, &k).unwrap();
            assert_eq!(a.choice(), c);
            assert_eq!(a.card_id(), "card-1");
            assert_eq!(a.session(), "s_abcdefghij");
            assert_eq!(a.device_fp(), "d".repeat(64));
            assert_eq!(a.nonce(), "ab".repeat(16));
            assert_eq!(a.req_sha256(), r.screen.req_sha256());
        }
        let a = ok(&signed(&r, &k, PermitChoice::Always, 1), &r, &k).unwrap();
        assert_eq!(a.choice(), PermitChoice::Always);
    }

    #[test]
    fn non_owner_sender_is_dropped_even_with_valid_sig() {
        let (r, k) = (record(), key());
        let s = signed(&r, &k, PermitChoice::Yes, 0);
        let sender = PermitSender {
            device_fp: &s.device_fp,
            owner_device: false,
            kind: PermitSenderKind::Mobile,
            desktop: DesktopPolicy::Safe,
            presence: None,
        };
        let e = verify_permit(&s, &r, &k, sender, &T, NOW).unwrap_err();
        assert_eq!(e, PermitRefusal::NotOwnerDevice);
        assert_eq!(e.outcome(), PermitOutcome::Drop);
    }

    #[test]
    fn untrusted_statuses_are_dropped() {
        let (r, k) = (record(), key());
        let s = signed(&r, &k, PermitChoice::Yes, 0);
        for st in [
            TrustStatus::NotTrusted,
            TrustStatus::Revoked,
            TrustStatus::Unknown,
        ] {
            let e = run(&s, &r, &k, st, NOW).unwrap_err();
            assert_eq!(e, PermitRefusal::NotTrusted, "{st:?}");
            assert_eq!(e.outcome(), PermitOutcome::Drop);
        }
    }

    #[test]
    fn sender_fp_must_match_sig_and_key() {
        let (r, k) = (record(), key());
        let s = signed(&r, &k, PermitChoice::Yes, 0);
        let other = "e".repeat(64);
        let sender = PermitSender {
            device_fp: &other,
            owner_device: true,
            kind: PermitSenderKind::Mobile,
            desktop: DesktopPolicy::Safe,
            presence: None,
        };
        let e = verify_permit(&s, &r, &k, sender, &T, NOW).unwrap_err();
        assert_eq!(e, PermitRefusal::SenderMismatch);
        assert_eq!(e.outcome(), PermitOutcome::Drop);

        let mut s2 = s.clone();
        s2.device_fp = other.clone();
        let sender = PermitSender {
            device_fp: &other,
            owner_device: true,
            kind: PermitSenderKind::Mobile,
            desktop: DesktopPolicy::Safe,
            presence: None,
        };
        assert_eq!(
            verify_permit(&s2, &r, &k, sender, &T, NOW),
            Err(PermitRefusal::SenderMismatch)
        );
    }

    #[test]
    fn bad_key_bytes_are_unknown_key_drop() {
        let (r, mut k) = (record(), key());
        let s = signed(&r, &k, PermitChoice::Yes, 0);
        k.spawn_pub = vec![4; 65];
        let e = ok(&s, &r, &k).unwrap_err();
        assert_eq!(e, PermitRefusal::UnknownKey);
        assert_eq!(e.outcome(), PermitOutcome::Drop);
    }

    #[test]
    fn answers_for_other_cards_are_stale_drop() {
        let (r, k) = (record(), key());
        let base = signed(&r, &k, PermitChoice::Yes, 0);
        let edits: Vec<EditS> = vec![
            Box::new(|s| s.nonce = "cd".repeat(16)),
            Box::new(|s| s.card_id = "card-2".into()),
            Box::new(|s| s.session = "s_zzzzzzzzzz".into()),
            Box::new(|s| s.user_id = "43".into()),
            Box::new(|s| s.runner_id = "mx_00bb".into()),
            Box::new(|s| s.runner_fp = "e".repeat(64)),
        ];
        for edit in edits {
            let mut s = base.clone();
            edit(&mut s);
            let e = ok(&s, &r, &k).unwrap_err();
            assert_eq!(e, PermitRefusal::Stale);
            assert_eq!(e.outcome(), PermitOutcome::Drop);
        }
        let mut k2 = key();
        k2.user_id = "43".into();
        assert_eq!(ok(&base, &r, &k2), Err(PermitRefusal::Stale));
    }

    #[test]
    fn expiry_is_inclusive_and_reissues() {
        let (r, k) = (record(), key());
        let s = signed(&r, &k, PermitChoice::Yes, 0);
        assert!(run(&s, &r, &k, TrustStatus::Trusted, r.exp_ms).is_ok());
        let e = run(&s, &r, &k, TrustStatus::Trusted, r.exp_ms + 1).unwrap_err();
        assert_eq!(e, PermitRefusal::Expired);
        assert_eq!(e.outcome(), PermitOutcome::Reissue);
    }

    #[test]
    fn ttl_bounds() {
        let k = key();
        let mut r = record();
        r.exp_ms = r.issued_ms + MAX_PERMIT_TTL_MS + 1;
        assert_eq!(
            ok(&signed(&r, &k, PermitChoice::Yes, 0), &r, &k),
            Err(PermitRefusal::BadTtl)
        );
        r.exp_ms = r.issued_ms;
        assert_eq!(
            ok(&signed(&r, &k, PermitChoice::Yes, 0), &r, &k),
            Err(PermitRefusal::BadTtl)
        );
        r.exp_ms = r.issued_ms - 1;
        assert_eq!(
            ok(&signed(&r, &k, PermitChoice::Yes, 0), &r, &k),
            Err(PermitRefusal::BadTtl)
        );
        r.exp_ms = r.issued_ms + MAX_PERMIT_TTL_MS;
        assert!(ok(&signed(&r, &k, PermitChoice::Yes, 0), &r, &k).is_ok());
    }

    #[test]
    fn ts_is_not_clock_checked() {
        let (r, k) = (record(), key());
        let mut s = signed(&r, &k, PermitChoice::Yes, 0);
        s.ts_ms = 5;
        resign(&mut s, &r, &k);
        assert!(ok(&s, &r, &k).is_ok());
    }

    #[test]
    fn confirmation_rules() {
        let (mut r, k) = (record(), key());
        r.screen.danger = vec!["deletes files".into()];
        let e = ok(&signed(&r, &k, PermitChoice::Yes, 0), &r, &k).unwrap_err();
        assert_eq!(e, PermitRefusal::NeedsConfirm);
        assert_eq!(e.outcome(), PermitOutcome::Reissue);
        assert!(ok(&signed(&r, &k, PermitChoice::Yes, 1), &r, &k).is_ok());
        assert!(ok(&signed(&r, &k, PermitChoice::No, 0), &r, &k).is_ok());
        assert_eq!(
            ok(&signed(&r, &k, PermitChoice::Always, 0), &r, &k),
            Err(PermitRefusal::NeedsConfirm)
        );
        let r = record();
        assert_eq!(
            ok(&signed(&r, &k, PermitChoice::Always, 0), &r, &k),
            Err(PermitRefusal::NeedsConfirm)
        );
        assert!(ok(&signed(&r, &k, PermitChoice::Yes, 0), &r, &k).is_ok());
    }

    #[test]
    fn always_requires_it_to_be_offered() {
        let (mut r, k) = (record(), key());
        r.screen.always_label = None;
        let e = ok(&signed(&r, &k, PermitChoice::Always, 1), &r, &k).unwrap_err();
        assert_eq!(e, PermitRefusal::BadChoice);
        assert_eq!(e.outcome(), PermitOutcome::Reissue);
    }

    #[test]
    fn confirmed_above_one_is_bad_token() {
        let (r, k) = (record(), key());
        assert_eq!(
            ok(&signed(&r, &k, PermitChoice::Yes, 2), &r, &k),
            Err(PermitRefusal::BadToken)
        );
    }

    #[test]
    fn any_screen_change_is_a_mismatch() {
        let (r, k) = (record(), key());
        let s = signed(&r, &k, PermitChoice::Yes, 0);
        let edits: Vec<EditC> = vec![
            Box::new(|c| c.agent = "codex".into()),
            Box::new(|c| c.tool = "Edit".into()),
            Box::new(|c| c.text = "ls -la /etc".into()),
            Box::new(|c| c.path = Some("/etc".into())),
            Box::new(|c| c.reason = None),
            Box::new(|c| c.from = Some("someone".into())),
            Box::new(|c| c.warning = Some("writes outside cwd".into())),
            Box::new(|c| c.danger = vec!["x".into()]),
            Box::new(|c| c.always_label = Some("Always allow all".into())),
        ];
        for edit in edits {
            let mut r2 = r.clone();
            edit(&mut r2.screen);
            let mut s = s.clone();
            resign(&mut s, &r2, &k);
            let e = ok(&s, &r2, &k).unwrap_err();
            assert_eq!(e, PermitRefusal::ScreenMismatch);
            assert_eq!(e.outcome(), PermitOutcome::Reissue);
        }
        let mut s2 = s.clone();
        s2.exp_ms += 1;
        assert_eq!(ok(&s2, &r, &k), Err(PermitRefusal::ScreenMismatch));
    }

    #[test]
    fn bad_signatures_are_dropped() {
        let (r, k) = (record(), key());
        let mut s = signed(&r, &k, PermitChoice::Yes, 0);
        let other: Signature = sk(4).sign(permit_signed_string(&r, &k, &s).as_bytes());
        s.sig = b64_encode(other.to_der().as_bytes());
        let e = ok(&s, &r, &k).unwrap_err();
        assert_eq!(e, PermitRefusal::BadSig);
        assert_eq!(e.outcome(), PermitOutcome::Drop);
        s.sig = "AAAA".into();
        assert_eq!(ok(&s, &r, &k), Err(PermitRefusal::BadSig));
        let mut tampered = signed(&r, &k, PermitChoice::No, 0);
        tampered.choice = PermitChoice::Yes;
        assert_eq!(ok(&tampered, &r, &k), Err(PermitRefusal::BadSig));
    }

    #[test]
    fn unsigned_junk_never_reaches_a_burning_outcome() {
        let (r, k) = (record(), key());
        let mut danger = record();
        danger.screen.danger = vec!["deletes files".into()];
        let late = r.exp_ms + 1;
        let cases: Vec<(&str, SignedPermit, PermitRecord, u64)> = vec![
            (
                "expired",
                signed(&r, &k, PermitChoice::Yes, 0),
                r.clone(),
                late,
            ),
            (
                "needs_confirm",
                signed(&danger, &k, PermitChoice::Yes, 0),
                danger.clone(),
                NOW,
            ),
            (
                "bad_choice",
                {
                    let mut r2 = record();
                    r2.screen.always_label = None;
                    signed(&r2, &k, PermitChoice::Always, 1)
                },
                {
                    let mut r2 = record();
                    r2.screen.always_label = None;
                    r2
                },
                NOW,
            ),
            (
                "version",
                SignedPermit {
                    v: 2,
                    ..signed(&r, &k, PermitChoice::Yes, 0)
                },
                r.clone(),
                NOW,
            ),
            (
                "alg",
                SignedPermit {
                    alg: "es384".into(),
                    ..signed(&r, &k, PermitChoice::Yes, 0)
                },
                r.clone(),
                NOW,
            ),
            (
                "confirmed",
                signed(&r, &k, PermitChoice::Yes, 2),
                r.clone(),
                NOW,
            ),
            (
                "exp",
                SignedPermit {
                    exp_ms: r.exp_ms + 1,
                    ..signed(&r, &k, PermitChoice::Yes, 0)
                },
                r.clone(),
                NOW,
            ),
        ];
        for (name, mut s, rec, now) in cases {
            s.sig = "AAAA".into();
            let e = run(&s, &rec, &k, TrustStatus::Trusted, now).unwrap_err();
            assert_eq!(e, PermitRefusal::BadSig, "{name}");
            assert_eq!(e.outcome(), PermitOutcome::Drop, "{name}");
        }
    }

    #[test]
    fn a_valid_signature_still_gets_the_specific_refusal() {
        let (mut r, k) = (record(), key());
        r.screen.danger = vec!["deletes files".into()];
        let e = ok(&signed(&r, &k, PermitChoice::Yes, 0), &r, &k).unwrap_err();
        assert_eq!(
            (e, e.outcome()),
            (PermitRefusal::NeedsConfirm, PermitOutcome::Reissue)
        );
        let r = record();
        let s = signed(&r, &k, PermitChoice::Yes, 0);
        let e = run(&s, &r, &k, TrustStatus::Trusted, r.exp_ms + 1).unwrap_err();
        assert_eq!(
            (e, e.outcome()),
            (PermitRefusal::Expired, PermitOutcome::Reissue)
        );
    }

    #[test]
    fn high_s_is_accepted() {
        let (r, k) = (record(), key());
        let mut s = signed(&r, &k, PermitChoice::Yes, 0);
        let low = Signature::from_der(&crate::tokens::b64_decode_strict(&s.sig).unwrap()).unwrap();
        let high =
            Signature::from_scalars(low.r().to_bytes(), (-*low.s().as_ref()).to_bytes()).unwrap();
        s.sig = b64_encode(high.to_der().as_bytes());
        assert!(ok(&s, &r, &k).is_ok());
    }

    #[test]
    fn version_and_alg() {
        let (r, k) = (record(), key());
        let mut s = signed(&r, &k, PermitChoice::Yes, 0);
        s.v = 2;
        assert_eq!(ok(&s, &r, &k), Err(PermitRefusal::UnsupportedVersion));
        s.v = 1;
        s.alg = "es384".into();
        assert_eq!(ok(&s, &r, &k), Err(PermitRefusal::UnsupportedAlg));
    }

    #[test]
    fn colons_in_signed_fields_are_bad_token() {
        let k = key();
        let cases: Vec<(&str, EditK)> = vec![
            (
                "user",
                Box::new(|r, k| {
                    r.user_id = "4:2".into();
                    k.user_id = "4:2".into();
                }),
            ),
            (
                "runner_id",
                Box::new(|r, _| r.runner_id = "mx_00:aa".into()),
            ),
            (
                "runner_fp",
                Box::new(|r, _| r.runner_fp = format!("{}:", "f".repeat(63))),
            ),
            ("card", Box::new(|r, _| r.card_id = "c:1".into())),
            (
                "session",
                Box::new(|r, _| r.session = "s_abcde:ghij".into()),
            ),
            (
                "nonce",
                Box::new(|r, _| r.nonce = format!("{}:", "a".repeat(31))),
            ),
            (
                "device_fp",
                Box::new(|_, k| k.device_fp = format!("{}:", "d".repeat(63))),
            ),
        ];
        for (name, edit) in cases {
            let (mut r, mut k2) = (record(), k.clone());
            edit(&mut r, &mut k2);
            let s = signed(&r, &k2, PermitChoice::Yes, 0);
            assert_eq!(ok(&s, &r, &k2), Err(PermitRefusal::BadToken), "{name}");
        }
        let (r, k) = (record(), key());
        let mut s = signed(&r, &k, PermitChoice::Yes, 0);
        s.req_sha256 = format!("{}:", "a".repeat(63));
        assert_eq!(ok(&s, &r, &k), Err(PermitRefusal::BadToken));
    }

    #[test]
    fn shifted_fields_never_collide() {
        let k = key();
        let a = record();
        let mut b = record();
        b.card_id = "card-1x".into();
        b.session = "s_abcdefghij".into();
        let sa = permit_signed_string(&a, &k, &signed(&a, &k, PermitChoice::Yes, 0));
        let sb = permit_signed_string(&b, &k, &signed(&b, &k, PermitChoice::Yes, 0));
        assert_ne!(sa, sb);
        let mut c = record();
        c.card_id = "card".into();
        c.session = "1:s_abcdefghij".into();
        let sc = permit_signed_string(&c, &k, &signed(&c, &k, PermitChoice::Yes, 0));
        assert_ne!(sa, sc);
        assert_eq!(sa.matches(':').count(), 13);
    }

    #[test]
    fn canonical_bytes_are_unambiguous() {
        let mut a = screen();
        let mut b = screen();
        a.danger = vec!["a".into(), "b".into()];
        b.danger = vec!["ab".into()];
        assert_ne!(a.canonical_bytes(), b.canonical_bytes());
        let mut c = screen();
        let mut d = screen();
        c.reason = None;
        d.reason = Some(String::new());
        assert_ne!(c.canonical_bytes(), d.canonical_bytes());
        assert!(screen()
            .canonical_bytes()
            .starts_with(b"tmgr-permit-card:v1\n"));
    }

    #[test]
    fn screen_validate_caps() {
        let mut s = screen();
        assert!(s.validate().is_ok());
        s.agent = "x".repeat(257);
        assert_eq!(s.validate(), Err(PermitRefusal::BadToken));
        let mut s = screen();
        s.tool = "x".repeat(257);
        assert_eq!(s.validate(), Err(PermitRefusal::BadToken));
        let mut s = screen();
        s.text = "t".repeat(8192);
        assert!(s.validate().is_ok());
        s.text = "t".repeat(8193);
        assert_eq!(s.validate(), Err(PermitRefusal::BadToken));
        for edit in [
            (|s: &mut PermitScreen| s.path = Some("p".repeat(1025))) as fn(&mut PermitScreen),
            |s| s.reason = Some("p".repeat(1025)),
            |s| s.from = Some("p".repeat(1025)),
            |s| s.warning = Some("p".repeat(1025)),
            |s| s.always_label = Some("p".repeat(1025)),
            |s| s.danger = vec!["d".repeat(1025)],
            |s| s.danger = vec!["d".into(); 17],
        ] {
            let mut s = screen();
            edit(&mut s);
            assert_eq!(s.validate(), Err(PermitRefusal::BadToken));
        }
        let mut s = screen();
        s.danger = vec!["d".repeat(1024); 16];
        assert!(s.validate().is_ok());
        let (mut r, k) = (record(), key());
        r.screen.agent = "t".repeat(257);
        assert_eq!(
            ok(&signed(&r, &k, PermitChoice::Yes, 0), &r, &k),
            Err(PermitRefusal::BadToken)
        );
    }

    #[test]
    fn outcome_table_is_exhaustive() {
        use PermitOutcome::*;
        use PermitRefusal::*;
        let all = [
            NotOwnerDevice,
            SenderMismatch,
            DesktopNotAllowed,
            NotTrusted,
            UnknownKey,
            Stale,
            Expired,
            NeedsConfirm,
            BadChoice,
            ScreenMismatch,
            BadToken,
            BadTtl,
            UnsupportedVersion,
            UnsupportedAlg,
            BadSig,
        ];
        for r in all {
            let want = match r {
                NotOwnerDevice | SenderMismatch | DesktopNotAllowed | NotTrusted | UnknownKey
                | Stale | BadSig => Drop,
                Expired | NeedsConfirm | BadChoice | ScreenMismatch | BadToken | BadTtl
                | UnsupportedVersion | UnsupportedAlg => Reissue,
            };
            assert_eq!(r.outcome(), want, "{r:?}");
            assert!(r.to_string().starts_with("permission"), "{r:?}");
            assert!(!r.code().is_empty());
        }
    }

    const J: &str = r#"{"v":1,"user_id":"u","device_fp":"d","runner_id":"t","runner_fp":"f","card_id":"c","session":"s","choice":"yes","confirmed":0,"nonce":"n","ts_ms":1,"exp_ms":2,"req_sha256":"r","alg":"es256","sig":"s"}"#;

    #[test]
    fn wire_roundtrip_and_strictness() {
        let w: SignedPermit = serde_json::from_str(J).unwrap();
        assert_eq!(serde_json::to_string(&w).unwrap(), J);
        let bad = J.replace("\"sig\":\"s\"", "\"sig\":\"s\",\"x\":1");
        assert!(serde_json::from_str::<SignedPermit>(&bad).is_err());
        let dup = J.replace("\"v\":1,", "\"v\":1,\"v\":1,");
        assert!(serde_json::from_str::<SignedPermit>(&dup).is_err());
        let maybe = J.replace("\"yes\"", "\"maybe\"");
        assert!(serde_json::from_str::<SignedPermit>(&maybe).is_err());
        let upper = J.replace("\"yes\"", "\"Yes\"");
        assert!(serde_json::from_str::<SignedPermit>(&upper).is_err());
        for c in ["yes", "no", "always"] {
            let j = J.replace("\"yes\"", &format!("\"{c}\""));
            assert!(serde_json::from_str::<SignedPermit>(&j).is_ok());
        }
    }

    #[test]
    fn wire_ints_must_be_js_safe() {
        let ok_j = J.replace("\"ts_ms\":1", "\"ts_ms\":9007199254740991");
        assert!(serde_json::from_str::<SignedPermit>(&ok_j).is_ok());
        for bad in ["9007199254740992", "1.0", "1e3", "-1", "\"1\""] {
            for (field, from) in [("ts_ms", "\"ts_ms\":1"), ("exp_ms", "\"exp_ms\":2")] {
                let j = J.replace(from, &format!("\"{field}\":{bad}"));
                assert!(
                    serde_json::from_str::<SignedPermit>(&j).is_err(),
                    "{field}={bad}"
                );
            }
        }
    }

    fn danger_record() -> PermitRecord {
        let mut r = record();
        r.screen.danger = vec!["deletes files".into()];
        r.screen.warning = Some("writes outside cwd".into());
        r.screen.always_label = None;
        r
    }

    fn as_sender(
        s: &SignedPermit,
        kind: PermitSenderKind,
        desktop: DesktopPolicy,
    ) -> PermitSender<'_> {
        PermitSender {
            device_fp: &s.device_fp,
            owner_device: true,
            kind,
            desktop,
            presence: None,
        }
    }

    #[test]
    fn desktop_policy_matrix() {
        let k = key();
        let cases = [
            (DesktopPolicy::None, false, false),
            (DesktopPolicy::None, true, false),
            (DesktopPolicy::Safe, false, true),
            (DesktopPolicy::Safe, true, false),
            (DesktopPolicy::All, false, true),
            (DesktopPolicy::All, true, true),
        ];
        for (policy, danger, allowed) in cases {
            let r = if danger { danger_record() } else { record() };
            let s = signed(&r, &k, PermitChoice::Yes, u8::from(danger));
            let got = verify_permit(
                &s,
                &r,
                &k,
                as_sender(&s, PermitSenderKind::Desktop, policy),
                &T,
                NOW,
            );
            if allowed {
                assert!(got.is_ok(), "{policy:?} danger={danger}");
            } else {
                let e = got.unwrap_err();
                assert_eq!(e, PermitRefusal::DesktopNotAllowed, "{policy:?} {danger}");
                assert_eq!(e.outcome(), PermitOutcome::Drop);
                assert_eq!(e.code(), "desktop_not_allowed");
            }
        }
    }

    #[test]
    fn a_warning_alone_makes_a_danger_card() {
        let k = key();
        let mut r = record();
        r.screen.danger = vec![];
        r.screen.warning = Some("Dangerous rm operation".into());
        r.screen.always_label = None;
        let s = signed(&r, &k, PermitChoice::Yes, 0);
        let desk = as_sender(&s, PermitSenderKind::Desktop, DesktopPolicy::Safe);
        assert_eq!(
            verify_permit(&s, &r, &k, desk, &T, NOW).unwrap_err(),
            PermitRefusal::DesktopNotAllowed
        );
        let phone = as_sender(&s, PermitSenderKind::Mobile, DesktopPolicy::Safe);
        assert_eq!(
            verify_permit(&s, &r, &k, phone, &T, NOW).unwrap_err(),
            PermitRefusal::NeedsConfirm
        );
    }

    #[test]
    fn mobile_is_never_limited_by_the_desktop_policy() {
        let k = key();
        for danger in [false, true] {
            let r = if danger { danger_record() } else { record() };
            let s = signed(&r, &k, PermitChoice::Yes, u8::from(danger));
            for policy in [DesktopPolicy::None, DesktopPolicy::Safe, DesktopPolicy::All] {
                let sender = as_sender(&s, PermitSenderKind::Mobile, policy);
                assert!(verify_permit(&s, &r, &k, sender, &T, NOW).is_ok());
            }
        }
    }

    #[test]
    fn desktop_policy_parses_strictly() {
        assert_eq!(DesktopPolicy::default(), DesktopPolicy::Presence);
        for (text, p) in [
            ("none", DesktopPolicy::None),
            ("safe", DesktopPolicy::Safe),
            ("all", DesktopPolicy::All),
            ("presence", DesktopPolicy::Presence),
        ] {
            assert_eq!(text.parse::<DesktopPolicy>(), Ok(p));
            assert_eq!(p.as_str(), text);
        }
        for bad in ["", "Safe", "ALL", " safe", "yes", "safe\n"] {
            assert!(bad.parse::<DesktopPolicy>().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn desktop_check_runs_before_the_trust_check() {
        let (r, k) = (danger_record(), key());
        let s = signed(&r, &k, PermitChoice::Yes, 1);
        let sender = as_sender(&s, PermitSenderKind::Desktop, DesktopPolicy::Safe);
        let untrusted = Trust(TrustStatus::NotTrusted);
        assert_eq!(
            verify_permit(&s, &r, &k, sender, &untrusted, NOW),
            Err(PermitRefusal::DesktopNotAllowed)
        );
    }

    fn presence_key() -> PinnedKey {
        PinnedKey {
            spawn_pub: sk(5)
                .verifying_key()
                .to_encoded_point(false)
                .as_bytes()
                .to_vec(),
            ..key()
        }
    }

    fn signed_by(
        r: &PermitRecord,
        choice: PermitChoice,
        k: PermitKey,
        signer: &SigningKey,
    ) -> SignedPermit {
        let mut s = signed(r, &key(), choice, u8::from(r.screen.is_danger()));
        s.k = k;
        let sig: Signature = signer.sign(permit_signed_string(r, &key(), &s).as_bytes());
        s.sig = b64_encode(sig.to_der().as_bytes());
        s
    }

    fn desk<'a>(
        s: &'a SignedPermit,
        policy: DesktopPolicy,
        presence: Option<&'a PinnedKey>,
    ) -> PermitSender<'a> {
        PermitSender {
            presence,
            ..as_sender(s, PermitSenderKind::Desktop, policy)
        }
    }

    #[test]
    fn desktop_key_policy_table() {
        use DesktopPolicy as P;
        use PermitKey::{Chat, Presence};
        let pk = presence_key();
        let cases = [
            (P::None, false, Chat, true, false),
            (P::None, true, Presence, true, false),
            (P::Safe, false, Chat, true, true),
            (P::Safe, false, Presence, true, false),
            (P::Safe, true, Chat, true, false),
            (P::Safe, true, Presence, true, false),
            (P::All, true, Chat, true, true),
            (P::All, true, Chat, false, true),
            (P::All, true, Presence, true, true),
            (P::Presence, false, Chat, true, true),
            (P::Presence, false, Chat, false, true),
            (P::Presence, false, Presence, true, true),
            (P::Presence, true, Presence, true, true),
            (P::Presence, true, Chat, true, false),
            (P::Presence, true, Chat, false, false),
        ];
        for (policy, danger, k, pinned, allowed) in cases {
            let r = if danger { danger_record() } else { record() };
            let signer = if k == Presence { sk(5) } else { sk(3) };
            let s = signed_by(&r, PermitChoice::Yes, k, &signer);
            let sender = desk(&s, policy, pinned.then_some(&pk));
            let got = verify_permit(&s, &r, &key(), sender, &T, NOW);
            let tag = format!("{policy:?} danger={danger} {k:?} pinned={pinned}");
            if allowed {
                assert!(got.is_ok(), "{tag}: {got:?}");
            } else {
                let e = got.unwrap_err();
                assert_eq!(e, PermitRefusal::DesktopNotAllowed, "{tag}");
                assert_eq!(e.outcome(), PermitOutcome::Drop, "{tag}");
            }
        }
    }

    #[test]
    fn a_presence_answer_without_a_pinned_presence_key_is_dropped() {
        let r = danger_record();
        let s = signed_by(&r, PermitChoice::Yes, PermitKey::Presence, &sk(5));
        let e = verify_permit(
            &s,
            &r,
            &key(),
            desk(&s, DesktopPolicy::Presence, None),
            &T,
            NOW,
        )
        .unwrap_err();
        assert_eq!(e, PermitRefusal::UnknownKey);
        assert_eq!(e.outcome(), PermitOutcome::Drop);
    }

    #[test]
    fn a_mobile_presence_answer_is_dropped() {
        let r = danger_record();
        let pk = presence_key();
        let s = signed_by(&r, PermitChoice::Yes, PermitKey::Presence, &sk(5));
        for policy in [DesktopPolicy::All, DesktopPolicy::Presence] {
            let sender = PermitSender {
                presence: Some(&pk),
                ..as_sender(&s, PermitSenderKind::Mobile, policy)
            };
            let e = verify_permit(&s, &r, &key(), sender, &T, NOW).unwrap_err();
            assert_eq!(e.outcome(), PermitOutcome::Drop, "{policy:?} {e:?}");
        }
    }

    #[test]
    fn the_key_kind_is_bound_into_the_signature() {
        let r = danger_record();
        let pk = presence_key();
        let mut relabelled = signed_by(&r, PermitChoice::Yes, PermitKey::Presence, &sk(3));
        relabelled.k = PermitKey::Chat;
        let s = relabelled.clone();
        assert_eq!(
            verify_permit(
                &s,
                &r,
                &key(),
                desk(&s, DesktopPolicy::All, Some(&pk)),
                &T,
                NOW
            ),
            Err(PermitRefusal::BadSig)
        );
        let mut relabelled = signed_by(&r, PermitChoice::Yes, PermitKey::Chat, &sk(5));
        relabelled.k = PermitKey::Presence;
        let s = relabelled.clone();
        assert_eq!(
            verify_permit(
                &s,
                &r,
                &key(),
                desk(&s, DesktopPolicy::Presence, Some(&pk)),
                &T,
                NOW
            ),
            Err(PermitRefusal::BadSig)
        );
        let chat_signed_as_presence = signed_by(&r, PermitChoice::Yes, PermitKey::Presence, &sk(3));
        let s = chat_signed_as_presence;
        assert_eq!(
            verify_permit(
                &s,
                &r,
                &key(),
                desk(&s, DesktopPolicy::Presence, Some(&pk)),
                &T,
                NOW
            ),
            Err(PermitRefusal::BadSig)
        );
    }

    #[test]
    fn a_presence_key_of_another_device_is_a_mismatch() {
        let r = danger_record();
        let other = PinnedKey {
            device_fp: "e".repeat(64),
            ..presence_key()
        };
        let s = signed_by(&r, PermitChoice::Yes, PermitKey::Presence, &sk(5));
        let e = verify_permit(
            &s,
            &r,
            &key(),
            desk(&s, DesktopPolicy::Presence, Some(&other)),
            &T,
            NOW,
        )
        .unwrap_err();
        assert_eq!(e, PermitRefusal::SenderMismatch);
    }

    #[test]
    fn a_presence_answer_from_an_untrusted_device_is_refused() {
        let r = danger_record();
        let pk = presence_key();
        let s = signed_by(&r, PermitChoice::Yes, PermitKey::Presence, &sk(5));
        for t in [
            TrustStatus::NotTrusted,
            TrustStatus::Revoked,
            TrustStatus::Unknown,
        ] {
            let got = verify_permit(
                &s,
                &r,
                &key(),
                desk(&s, DesktopPolicy::Presence, Some(&pk)),
                &Trust(t),
                NOW,
            );
            assert_eq!(got, Err(PermitRefusal::NotTrusted), "{t:?}");
        }
    }

    #[test]
    fn the_key_kind_is_on_the_wire_only_for_presence() {
        let w: SignedPermit = serde_json::from_str(J).unwrap();
        assert_eq!(w.k, PermitKey::Chat);
        assert!(!serde_json::to_string(&w).unwrap().contains("\"k\""));
        let chat = J.replace("\"sig\":\"s\"", "\"sig\":\"s\",\"k\":\"chat\"");
        assert_eq!(
            serde_json::from_str::<SignedPermit>(&chat).unwrap().k,
            PermitKey::Chat
        );
        let pres = J.replace("\"sig\":\"s\"", "\"sig\":\"s\",\"k\":\"presence\"");
        let p: SignedPermit = serde_json::from_str(&pres).unwrap();
        assert_eq!(p.k, PermitKey::Presence);
        assert_eq!(serde_json::to_string(&p).unwrap(), pres);
        for bad in ["\"spawn\"", "\"Presence\"", "1", "null"] {
            let j = J.replace("\"sig\":\"s\"", &format!("\"sig\":\"s\",\"k\":{bad}"));
            assert!(serde_json::from_str::<SignedPermit>(&j).is_err(), "{bad}");
        }
    }

    #[test]
    fn presence_uses_its_own_signing_domain() {
        let r = record();
        let mut s = signed(&r, &key(), PermitChoice::Yes, 0);
        let chat = permit_signed_string(&r, &key(), &s);
        s.k = PermitKey::Presence;
        let pres = permit_signed_string(&r, &key(), &s);
        assert!(chat.starts_with("tmgr-permit:v1:"));
        assert_eq!(
            pres,
            chat.replacen("tmgr-permit:v1:", "tmgr-permit-presence:v1:", 1)
        );
    }

    #[test]
    fn desktop_cards_want_presence_only_when_danger_under_presence() {
        let (safe, danger) = (record().screen, danger_record().screen);
        for p in [DesktopPolicy::None, DesktopPolicy::Safe, DesktopPolicy::All] {
            assert!(
                !p.wants_presence(&safe) && !p.wants_presence(&danger),
                "{p:?}"
            );
        }
        assert!(!DesktopPolicy::Presence.wants_presence(&safe));
        assert!(DesktopPolicy::Presence.wants_presence(&danger));
        assert!(DesktopPolicy::Presence.allows(PermitSenderKind::Desktop, &safe));
        assert!(!DesktopPolicy::Presence.allows(PermitSenderKind::Desktop, &danger));
    }
}
