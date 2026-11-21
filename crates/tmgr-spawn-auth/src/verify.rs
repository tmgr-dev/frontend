use crate::error::Refusal;
use crate::tokens::{b64_decode_strict, is_hex_lower, is_token};
use crate::wire::SignedSpawn;
use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};

pub const MAX_TTL_MS: u64 = 5 * 60 * 1000;
pub const MAX_SKEW_MS: u64 = 5 * 60 * 1000;

pub trait TrustView {
    fn is_trusted(&self, device_id_or_fp: &str) -> bool;
    fn is_revoked(&self, fp: &str) -> bool;
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
    pub req_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedKey {
    pub user_id: String,
    pub device_fp: String,
    pub spawn_pub: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approved {
    pub user_id: String,
    pub device_fp: String,
    pub target_id: String,
    pub card_id: String,
    pub nonce: String,
    pub req_sha256: String,
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
        record.req_sha256
    )
}

pub fn verify(
    sig: &SignedSpawn,
    record: &ChallengeRecord,
    key: &PinnedKey,
    trust: &dyn TrustView,
    now_ms: u64,
) -> Result<Approved, Refusal> {
    let matches = sig.user_id == record.user_id
        && sig.user_id == key.user_id
        && sig.device_fp == key.device_fp
        && sig.target_id == record.target_id
        && sig.target_fp == record.target_fp
        && sig.card_id == record.card_id
        && sig.nonce == record.nonce
        && sig.exp_ms == record.exp_ms
        && sig.req_sha256 == record.req_sha256;
    if !matches {
        return Err(Refusal::Mismatch);
    }
    if sig.v != 1 {
        return Err(Refusal::UnsupportedVersion);
    }
    if sig.alg != "es256" {
        return Err(Refusal::UnsupportedAlg);
    }
    if record.exp_ms > record.issued_ms.saturating_add(MAX_TTL_MS) {
        return Err(Refusal::BadTtl);
    }
    if now_ms > record.exp_ms {
        return Err(Refusal::Expired);
    }
    if sig.ts_ms.abs_diff(now_ms) > MAX_SKEW_MS {
        return Err(Refusal::ClockSkew);
    }
    let tokens_ok = [
        &record.user_id,
        &key.device_fp,
        &record.target_id,
        &record.target_fp,
        &record.card_id,
    ]
    .iter()
    .all(|t| is_token(t))
        && is_hex_lower(&record.nonce, 32)
        && is_hex_lower(&record.req_sha256, 64);
    if !tokens_ok {
        return Err(Refusal::BadToken);
    }
    let verifying =
        VerifyingKey::from_sec1_bytes(&key.spawn_pub).map_err(|_| Refusal::UnknownKey)?;
    if !trust.is_trusted(&key.device_fp) || trust.is_revoked(&key.device_fp) {
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
        req_sha256: record.req_sha256.clone(),
    })
}

pub(crate) fn verify_der_b64(
    key: &VerifyingKey,
    message: &str,
    sig_b64: &str,
) -> Result<(), Refusal> {
    let der = b64_decode_strict(sig_b64).ok_or(Refusal::BadSig)?;
    let sig = Signature::from_der(&der).map_err(|_| Refusal::BadSig)?;
    let sig = sig.normalize_s().unwrap_or(sig);
    key.verify(message.as_bytes(), &sig)
        .map_err(|_| Refusal::BadSig)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::b64_encode;
    use p256::ecdsa::signature::Signer;
    use p256::ecdsa::SigningKey;

    struct Trust {
        trusted: bool,
        revoked: bool,
    }
    impl TrustView for Trust {
        fn is_trusted(&self, _: &str) -> bool {
            self.trusted
        }
        fn is_revoked(&self, _: &str) -> bool {
            self.revoked
        }
    }
    const OK: Trust = Trust {
        trusted: true,
        revoked: false,
    };

    fn sk(b: u8) -> SigningKey {
        SigningKey::from_slice(&[b; 32]).unwrap()
    }

    fn fixture() -> (SignedSpawn, ChallengeRecord, PinnedKey) {
        let record = ChallengeRecord {
            user_id: "u1".into(),
            target_id: "mx_00aa".into(),
            target_fp: "f".repeat(64),
            card_id: "card-1".into(),
            nonce: "ab".repeat(16),
            issued_ms: 1_000_000,
            exp_ms: 1_000_000 + MAX_TTL_MS,
            req_sha256: "c".repeat(64),
        };
        let key = PinnedKey {
            user_id: "u1".into(),
            device_fp: "d".repeat(64),
            spawn_pub: sk(3)
                .verifying_key()
                .to_encoded_point(false)
                .as_bytes()
                .to_vec(),
        };
        let ts = 1_000_500;
        let s: Signature = sk(3).sign(signed_string(&record, &key, ts).as_bytes());
        let wire = SignedSpawn {
            v: 1,
            user_id: record.user_id.clone(),
            device_fp: key.device_fp.clone(),
            target_id: record.target_id.clone(),
            target_fp: record.target_fp.clone(),
            card_id: record.card_id.clone(),
            nonce: record.nonce.clone(),
            ts_ms: ts,
            exp_ms: record.exp_ms,
            req_sha256: record.req_sha256.clone(),
            alg: "es256".into(),
            sig: b64_encode(s.to_der().as_bytes()),
        };
        (wire, record, key)
    }

    const NOW: u64 = 1_001_000;

    #[test]
    fn happy_path() {
        let (w, r, k) = fixture();
        let a = verify(&w, &r, &k, &OK, NOW).unwrap();
        assert_eq!(a.card_id, "card-1");
    }

    #[test]
    fn high_s_is_accepted() {
        let (mut w, r, k) = fixture();
        let der = b64_decode_strict(&w.sig).unwrap();
        let s = Signature::from_der(&der).unwrap();
        let high =
            Signature::from_scalars(s.r().to_bytes(), (-*s.s().as_ref()).to_bytes()).unwrap();
        assert!(high.normalize_s().is_some());
        w.sig = b64_encode(high.to_der().as_bytes());
        assert!(verify(&w, &r, &k, &OK, NOW).is_ok());
    }

    #[test]
    fn trust_is_checked_every_call() {
        let (w, r, k) = fixture();
        let untrusted = Trust {
            trusted: false,
            revoked: false,
        };
        let revoked = Trust {
            trusted: true,
            revoked: true,
        };
        assert_eq!(
            verify(&w, &r, &k, &untrusted, NOW),
            Err(Refusal::NotTrusted)
        );
        assert_eq!(verify(&w, &r, &k, &revoked, NOW), Err(Refusal::NotTrusted));
        assert!(verify(&w, &r, &k, &OK, NOW).is_ok());
    }

    #[test]
    fn each_field_mismatch_refuses() {
        let (w, r, k) = fixture();
        let edits: [fn(&mut SignedSpawn); 9] = [
            |w| w.user_id = "u2".into(),
            |w| w.device_fp = "e".repeat(64),
            |w| w.target_id = "mx_ffff".into(),
            |w| w.target_fp = "e".repeat(64),
            |w| w.card_id = "card-2".into(),
            |w| w.nonce = "cd".repeat(16),
            |w| w.exp_ms += 1,
            |w| w.req_sha256 = "e".repeat(64),
            |w| w.sig.clear(),
        ];
        for (i, e) in edits.iter().enumerate() {
            let mut x = w.clone();
            e(&mut x);
            let want = if i == 8 {
                Refusal::BadSig
            } else {
                Refusal::Mismatch
            };
            assert_eq!(verify(&x, &r, &k, &OK, NOW), Err(want), "edit {i}");
        }
    }

    #[test]
    fn version_alg_time_checks() {
        let (w, r, k) = fixture();
        let mut x = w.clone();
        x.v = 2;
        assert_eq!(
            verify(&x, &r, &k, &OK, NOW),
            Err(Refusal::UnsupportedVersion)
        );
        let mut x = w.clone();
        x.alg = "es384".into();
        assert_eq!(verify(&x, &r, &k, &OK, NOW), Err(Refusal::UnsupportedAlg));
        assert_eq!(verify(&w, &r, &k, &OK, r.exp_ms + 1), Err(Refusal::Expired));
        assert!(verify(&w, &r, &k, &OK, r.exp_ms).is_ok());
        let mut r2 = r.clone();
        r2.exp_ms += 1;
        let mut w2 = w.clone();
        w2.exp_ms += 1;
        assert_eq!(verify(&w2, &r2, &k, &OK, NOW), Err(Refusal::BadTtl));
    }

    #[test]
    fn skew_boundaries() {
        let (w, r, k) = fixture();
        let ts = w.ts_ms;
        assert_eq!(
            verify(&w, &r, &k, &OK, ts.saturating_sub(MAX_SKEW_MS + 1)),
            Err(Refusal::ClockSkew)
        );
        assert!(verify(&w, &r, &k, &OK, ts.saturating_sub(MAX_SKEW_MS)).is_ok());
    }

    #[test]
    fn bad_key_bytes_are_unknown_key() {
        let (w, r, mut k) = fixture();
        k.spawn_pub.truncate(64);
        assert_eq!(verify(&w, &r, &k, &OK, NOW), Err(Refusal::UnknownKey));
        k.spawn_pub = vec![4; 65];
        assert_eq!(verify(&w, &r, &k, &OK, NOW), Err(Refusal::UnknownKey));
    }

    #[test]
    fn display_never_echoes_wire_text() {
        let (mut w, r, k) = fixture();
        w.card_id = "SECRET-WIRE-TEXT".into();
        let e = verify(&w, &r, &k, &OK, NOW).unwrap_err();
        assert!(!e.to_string().contains("SECRET"));
        assert!(!format!("{e:?}").contains("SECRET"));
    }
}
