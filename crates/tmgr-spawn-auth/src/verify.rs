use crate::error::Refusal;
use crate::proposal::{sha256_hex, SpawnProposal};
use crate::tokens::{
    b64_decode_strict, is_hex_lower, is_target_id, is_token, parse_p256_key, MAX_SAFE_INT,
};
use crate::wire::SignedSpawn;
use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};

pub const MAX_TTL_MS: u64 = 5 * 60 * 1000;
pub const MAX_SKEW_MS: u64 = 5 * 60 * 1000;
pub(crate) const MAX_SIG_B64_LEN: usize = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustStatus {
    Trusted,
    NotTrusted,
    Revoked,
    Unknown,
}

/// A failed lookup must return `Unknown`; only `Trusted` is accepted.
pub trait TrustView {
    fn status(&self, device_id: &str, fp: &str) -> TrustStatus;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeRecord {
    pub user_id: String,
    pub target_id: String,
    pub target_fp: String,
    pub card_id: String,
    pub nonce: String,
    pub issued_ms: u64,
    pub exp_ms: u64,
    pub proposal: SpawnProposal,
}

/// Only approved keys may be passed to `verify`; a pending key must never reach it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedKey {
    pub user_id: String,
    pub device_id: String,
    pub device_fp: String,
    pub spawn_pub: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approved {
    user_id: String,
    device_fp: String,
    target_id: String,
    card_id: String,
    nonce: String,
    req_sha256: String,
    proposal: SpawnProposal,
}

impl Approved {
    pub fn user_id(&self) -> &str {
        &self.user_id
    }
    pub fn device_fp(&self) -> &str {
        &self.device_fp
    }
    pub fn target_id(&self) -> &str {
        &self.target_id
    }
    pub fn card_id(&self) -> &str {
        &self.card_id
    }
    pub fn nonce(&self) -> &str {
        &self.nonce
    }
    pub fn req_sha256(&self) -> &str {
        &self.req_sha256
    }
    pub fn proposal(&self) -> &SpawnProposal {
        &self.proposal
    }
    pub fn check_prompt(&self, prompt: Option<&[u8]>) -> Result<(), Refusal> {
        match (self.proposal.prompt_sha256.as_deref(), prompt) {
            (None, None) => Ok(()),
            (Some(h), Some(p)) if sha256_hex(p) == h => Ok(()),
            _ => Err(Refusal::Mismatch),
        }
    }
    pub fn into_proposal(self) -> SpawnProposal {
        self.proposal
    }
}

pub fn signed_string(record: &ChallengeRecord, key: &PinnedKey, ts_ms: u64) -> String {
    format!(
        "tmgr-spawn:v1:{}:{}:{}:{}:{}:{}:{}:{}:{}",
        record.user_id,
        key.device_fp,
        record.target_id,
        record.target_fp,
        record.card_id,
        record.nonce,
        ts_ms,
        record.exp_ms,
        record.proposal.req_sha256()
    )
}

pub fn verify(
    sig: &SignedSpawn,
    record: &ChallengeRecord,
    key: &PinnedKey,
    trust: &dyn TrustView,
    now_ms: u64,
) -> Result<Approved, Refusal> {
    let req_sha256 = record.proposal.req_sha256();
    let matches = sig.user_id == record.user_id
        && sig.user_id == key.user_id
        && sig.device_fp == key.device_fp
        && sig.target_id == record.target_id
        && record.proposal.target_id == record.target_id
        && sig.target_fp == record.target_fp
        && sig.card_id == record.card_id
        && sig.nonce == record.nonce
        && sig.exp_ms == record.exp_ms
        && sig.req_sha256 == req_sha256;
    if !matches {
        return Err(Refusal::Mismatch);
    }
    if sig.v != 1 {
        return Err(Refusal::UnsupportedVersion);
    }
    if sig.alg != "es256" {
        return Err(Refusal::UnsupportedAlg);
    }
    if sig.ts_ms > MAX_SAFE_INT || sig.exp_ms > MAX_SAFE_INT || record.exp_ms > MAX_SAFE_INT {
        return Err(Refusal::BadToken);
    }
    if record.exp_ms <= record.issued_ms
        || record.exp_ms > record.issued_ms.saturating_add(MAX_TTL_MS)
    {
        return Err(Refusal::BadTtl);
    }
    if now_ms > record.exp_ms {
        return Err(Refusal::Expired);
    }
    if record.issued_ms > now_ms.saturating_add(MAX_SKEW_MS)
        || sig.ts_ms.abs_diff(now_ms) > MAX_SKEW_MS
    {
        return Err(Refusal::ClockSkew);
    }
    let tokens_ok = is_token(&record.user_id)
        && is_token(&key.device_id)
        && is_token(&record.card_id)
        && is_target_id(&record.target_id)
        && is_hex_lower(&key.device_fp, 64)
        && is_hex_lower(&record.target_fp, 64)
        && is_hex_lower(&record.nonce, 32);
    if !tokens_ok {
        return Err(Refusal::BadToken);
    }
    record.proposal.validate()?;
    let verifying = parse_p256_key(&key.spawn_pub).ok_or(Refusal::UnknownKey)?;
    if trust.status(&key.device_id, &key.device_fp) != TrustStatus::Trusted {
        return Err(Refusal::NotTrusted);
    }
    let message = signed_string(record, key, sig.ts_ms);
    verify_der_b64(&verifying, &message, &sig.sig)?;
    Ok(Approved {
        user_id: record.user_id.clone(),
        device_fp: key.device_fp.clone(),
        target_id: record.target_id.clone(),
        card_id: record.card_id.clone(),
        nonce: record.nonce.clone(),
        req_sha256,
        proposal: record.proposal.clone(),
    })
}

pub(crate) fn verify_der_b64(
    key: &VerifyingKey,
    message: &str,
    sig_b64: &str,
) -> Result<(), Refusal> {
    if sig_b64.len() > MAX_SIG_B64_LEN {
        return Err(Refusal::BadSig);
    }
    let der = b64_decode_strict(sig_b64).ok_or(Refusal::BadSig)?;
    let sig = Signature::from_der(&der).map_err(|_| Refusal::BadSig)?;
    let sig = sig.normalize_s().unwrap_or(sig);
    key.verify(message.as_bytes(), &sig)
        .map_err(|_| Refusal::BadSig)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proposal::SpawnMode;
    use crate::tokens::b64_encode;
    use p256::ecdsa::signature::Signer;
    use p256::ecdsa::SigningKey;
    use std::cell::RefCell;

    struct Trust {
        status: TrustStatus,
        seen: RefCell<Vec<(String, String)>>,
    }
    impl TrustView for Trust {
        fn status(&self, device_id: &str, fp: &str) -> TrustStatus {
            self.seen.borrow_mut().push((device_id.into(), fp.into()));
            self.status
        }
    }
    fn trust(status: TrustStatus) -> Trust {
        Trust {
            status,
            seen: RefCell::default(),
        }
    }

    fn sk(b: u8) -> SigningKey {
        SigningKey::from_slice(&[b; 32]).unwrap()
    }

    fn proposal() -> SpawnProposal {
        SpawnProposal {
            v: 1,
            profile: "claude".into(),
            cwd: "/work/app".into(),
            mode: SpawnMode::Bypass,
            prompt_sha256: None,
            name: None,
            task: None,
            target_id: "mx_00aa".into(),
        }
    }

    fn sign(r: &ChallengeRecord, k: &PinnedKey, ts: u64) -> String {
        let s: Signature = sk(3).sign(signed_string(r, k, ts).as_bytes());
        b64_encode(s.to_der().as_bytes())
    }

    fn wire_for(r: &ChallengeRecord, k: &PinnedKey, ts: u64) -> SignedSpawn {
        SignedSpawn {
            v: 1,
            user_id: r.user_id.clone(),
            device_fp: k.device_fp.clone(),
            target_id: r.target_id.clone(),
            target_fp: r.target_fp.clone(),
            card_id: r.card_id.clone(),
            nonce: r.nonce.clone(),
            ts_ms: ts,
            exp_ms: r.exp_ms,
            req_sha256: r.proposal.req_sha256(),
            alg: "es256".into(),
            sig: sign(r, k, ts),
        }
    }

    fn fixture() -> (SignedSpawn, ChallengeRecord, PinnedKey) {
        let record = ChallengeRecord {
            user_id: "42".into(),
            target_id: "mx_00aa".into(),
            target_fp: "f".repeat(64),
            card_id: "card-1".into(),
            nonce: "ab".repeat(16),
            issued_ms: 1_000_000,
            exp_ms: 1_000_000 + MAX_TTL_MS,
            proposal: proposal(),
        };
        let key = PinnedKey {
            user_id: "42".into(),
            device_id: "dev-1".into(),
            device_fp: "d".repeat(64),
            spawn_pub: sk(3)
                .verifying_key()
                .to_encoded_point(false)
                .as_bytes()
                .to_vec(),
        };
        let wire = wire_for(&record, &key, 1_000_500);
        (wire, record, key)
    }

    const NOW: u64 = 1_001_000;

    #[test]
    fn happy_path_returns_the_recorded_proposal() {
        let (w, r, k) = fixture();
        let a = verify(&w, &r, &k, &trust(TrustStatus::Trusted), NOW).unwrap();
        assert_eq!(a.card_id(), "card-1");
        assert_eq!(a.user_id(), "42");
        assert_eq!(a.proposal(), &r.proposal);
        assert_eq!(a.req_sha256(), r.proposal.req_sha256());
        assert_eq!(a.into_proposal().mode, SpawnMode::Bypass);
    }

    #[test]
    fn trust_is_asked_with_device_id_and_fp_every_call() {
        let (w, r, k) = fixture();
        let t = trust(TrustStatus::Trusted);
        verify(&w, &r, &k, &t, NOW).unwrap();
        assert_eq!(t.seen.borrow()[0], ("dev-1".to_string(), "d".repeat(64)));
        assert_eq!(
            verify(&w, &r, &k, &trust(TrustStatus::NotTrusted), NOW),
            Err(Refusal::NotTrusted)
        );
        assert_eq!(
            verify(&w, &r, &k, &trust(TrustStatus::Revoked), NOW),
            Err(Refusal::NotTrusted)
        );
    }

    #[test]
    fn only_trusted_status_is_accepted() {
        let (w, r, k) = fixture();
        for st in [
            TrustStatus::NotTrusted,
            TrustStatus::Revoked,
            TrustStatus::Unknown,
        ] {
            assert_eq!(
                verify(&w, &r, &k, &trust(st), NOW),
                Err(Refusal::NotTrusted),
                "{st:?}"
            );
        }
    }

    fn approved_with(prompt_sha256: Option<String>) -> Approved {
        let (_, mut r, k) = fixture();
        r.proposal.prompt_sha256 = prompt_sha256;
        let w = wire_for(&r, &k, 1_000_500);
        verify(&w, &r, &k, &trust(TrustStatus::Trusted), NOW).unwrap()
    }

    #[test]
    fn check_prompt_binds_the_prompt_hash() {
        let none = approved_with(None);
        assert_eq!(none.check_prompt(None), Ok(()));
        assert_eq!(none.check_prompt(Some(b"x")), Err(Refusal::Mismatch));
        assert_eq!(none.check_prompt(Some(b"")), Err(Refusal::Mismatch));
        let some = approved_with(Some(crate::proposal::sha256_hex(b"fix it")));
        assert_eq!(some.check_prompt(Some(b"fix it")), Ok(()));
        assert_eq!(some.check_prompt(Some(b"fix it ")), Err(Refusal::Mismatch));
        assert_eq!(some.check_prompt(Some(b"")), Err(Refusal::Mismatch));
        assert_eq!(some.check_prompt(None), Err(Refusal::Mismatch));
    }

    #[test]
    fn ts_and_exp_above_js_safe_integer_are_bad_token() {
        let (w, r, k) = fixture();
        let t = trust(TrustStatus::Trusted);
        let mut x = w.clone();
        x.ts_ms = 1 << 53;
        assert_eq!(verify(&x, &r, &k, &t, NOW), Err(Refusal::BadToken));
        let mut r2 = r.clone();
        r2.exp_ms = 1 << 53;
        let mut x = w.clone();
        x.exp_ms = 1 << 53;
        assert_eq!(verify(&x, &r2, &k, &t, NOW), Err(Refusal::BadToken));
    }

    #[test]
    fn hash_is_recomputed_from_the_recorded_proposal() {
        let (w, mut r, k) = fixture();
        r.proposal.mode = SpawnMode::Ask;
        assert_eq!(
            verify(&w, &r, &k, &trust(TrustStatus::Trusted), NOW),
            Err(Refusal::Mismatch)
        );
    }

    #[test]
    fn proposal_must_name_the_recorded_target() {
        let (_, mut r, k) = fixture();
        r.proposal.target_id = "mx_ffff".into();
        let w = wire_for(&r, &k, 1_000_500);
        assert_eq!(
            verify(&w, &r, &k, &trust(TrustStatus::Trusted), NOW),
            Err(Refusal::Mismatch)
        );
    }

    #[test]
    fn invalid_proposal_is_bad_token() {
        let (_, mut r, k) = fixture();
        r.proposal.cwd = "/a\u{1}b".into();
        let w = wire_for(&r, &k, 1_000_500);
        assert_eq!(
            verify(&w, &r, &k, &trust(TrustStatus::Trusted), NOW),
            Err(Refusal::BadToken)
        );
    }

    #[test]
    fn pinned_key_user_and_device_must_match() {
        let (w, r, mut k) = fixture();
        k.user_id = "43".into();
        assert_eq!(
            verify(&w, &r, &k, &trust(TrustStatus::Trusted), NOW),
            Err(Refusal::Mismatch)
        );
    }

    #[test]
    fn issued_in_the_future_is_clock_skew() {
        let (_, mut r, k) = fixture();
        r.issued_ms = NOW + MAX_SKEW_MS + 1;
        r.exp_ms = r.issued_ms + MAX_TTL_MS;
        let w = wire_for(&r, &k, NOW);
        assert_eq!(
            verify(&w, &r, &k, &trust(TrustStatus::Trusted), NOW),
            Err(Refusal::ClockSkew)
        );
        r.issued_ms = NOW + MAX_SKEW_MS;
        r.exp_ms = r.issued_ms + MAX_TTL_MS;
        let w = wire_for(&r, &k, NOW);
        assert!(verify(&w, &r, &k, &trust(TrustStatus::Trusted), NOW).is_ok());
    }

    #[test]
    fn exp_not_after_issued_is_bad_ttl() {
        let (_, mut r, k) = fixture();
        r.exp_ms = r.issued_ms;
        let w = wire_for(&r, &k, 1_000_500);
        assert_eq!(
            verify(&w, &r, &k, &trust(TrustStatus::Trusted), NOW),
            Err(Refusal::BadTtl)
        );
        r.exp_ms = r.issued_ms - 1;
        let w = wire_for(&r, &k, 1_000_500);
        assert_eq!(
            verify(&w, &r, &k, &trust(TrustStatus::Trusted), NOW),
            Err(Refusal::BadTtl)
        );
    }

    #[test]
    fn fingerprints_and_target_id_formats() {
        let (_, r, k) = fixture();
        type Edit = (
            &'static str,
            Box<dyn Fn(&mut ChallengeRecord, &mut PinnedKey)>,
        );
        let cases: Vec<Edit> = vec![
            (
                "target_fp_upper",
                Box::new(|r, _| r.target_fp = "F".repeat(64)),
            ),
            (
                "target_fp_short",
                Box::new(|r, _| r.target_fp = "f".repeat(63)),
            ),
            (
                "device_fp_upper",
                Box::new(|_, k| k.device_fp = "D".repeat(64)),
            ),
            (
                "device_fp_short",
                Box::new(|_, k| k.device_fp = "d".repeat(63)),
            ),
            (
                "device_id_colon",
                Box::new(|_, k| k.device_id = "a:b".into()),
            ),
            (
                "target_id_not_mx",
                Box::new(|r, _| {
                    r.target_id = "tgt".into();
                    r.proposal.target_id = "tgt".into();
                }),
            ),
        ];
        for (name, e) in cases {
            let (mut r, mut k) = (r.clone(), k.clone());
            e(&mut r, &mut k);
            let w = wire_for(&r, &k, 1_000_500);
            assert_eq!(
                verify(&w, &r, &k, &trust(TrustStatus::Trusted), NOW),
                Err(Refusal::BadToken),
                "{name}"
            );
        }
    }

    #[test]
    fn spawn_key_must_be_uncompressed_on_curve() {
        let (w, r, mut k) = fixture();
        let t = trust(TrustStatus::Trusted);
        k.spawn_pub = sk(3)
            .verifying_key()
            .to_encoded_point(true)
            .as_bytes()
            .to_vec();
        assert_eq!(verify(&w, &r, &k, &t, NOW), Err(Refusal::UnknownKey));
        k.spawn_pub.clear();
        assert_eq!(verify(&w, &r, &k, &t, NOW), Err(Refusal::UnknownKey));
        k.spawn_pub = vec![4; 65];
        assert_eq!(verify(&w, &r, &k, &t, NOW), Err(Refusal::UnknownKey));
    }

    #[test]
    fn high_s_is_accepted() {
        let (mut w, r, k) = fixture();
        let der = b64_decode_strict(&w.sig).unwrap();
        let s = Signature::from_der(&der).unwrap();
        let s = s.normalize_s().unwrap_or(s);
        let high =
            Signature::from_scalars(s.r().to_bytes(), (-*s.s().as_ref()).to_bytes()).unwrap();
        assert!(high.normalize_s().is_some());
        w.sig = b64_encode(high.to_der().as_bytes());
        assert!(verify(&w, &r, &k, &trust(TrustStatus::Trusted), NOW).is_ok());
    }

    #[test]
    fn version_alg_time_checks() {
        let (w, r, k) = fixture();
        let t = trust(TrustStatus::Trusted);
        let mut x = w.clone();
        x.v = 2;
        assert_eq!(
            verify(&x, &r, &k, &t, NOW),
            Err(Refusal::UnsupportedVersion)
        );
        let mut x = w.clone();
        x.alg = "es384".into();
        assert_eq!(verify(&x, &r, &k, &t, NOW), Err(Refusal::UnsupportedAlg));
        assert_eq!(verify(&w, &r, &k, &t, r.exp_ms + 1), Err(Refusal::Expired));
        assert!(verify(&w, &r, &k, &t, r.exp_ms).is_ok());
        let mut r2 = r.clone();
        r2.exp_ms += 1;
        let mut w2 = w.clone();
        w2.exp_ms += 1;
        assert_eq!(verify(&w2, &r2, &k, &t, NOW), Err(Refusal::BadTtl));
    }

    #[test]
    fn ts_skew_boundaries() {
        let (w, r, k) = fixture();
        let ts = w.ts_ms;
        let t = trust(TrustStatus::Trusted);
        assert_eq!(
            verify(&w, &r, &k, &t, ts.saturating_sub(MAX_SKEW_MS + 1)),
            Err(Refusal::ClockSkew)
        );
        assert!(verify(&w, &r, &k, &t, ts.saturating_sub(MAX_SKEW_MS)).is_ok());
    }

    #[test]
    fn display_never_echoes_wire_text() {
        let (mut w, r, k) = fixture();
        w.card_id = "SECRET-WIRE-TEXT".into();
        let e = verify(&w, &r, &k, &trust(TrustStatus::Trusted), NOW).unwrap_err();
        assert!(!e.to_string().contains("SECRET"));
        assert!(!format!("{e:?}").contains("SECRET"));
    }
}
