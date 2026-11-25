use crate::error::Refusal;
use crate::tokens::{b64_decode_strict, is_hex_lower, is_token, parse_p256_key, MAX_SAFE_INT};
use crate::verify::MAX_SKEW_MS;
use crate::verify::{verify_der_b64, TrustStatus, TrustView};
use sha2::{Digest, Sha256};

const PREFIX: &str = "tmgr-spawn-key:v1:";
const PRESENCE_PREFIX: &str = "tmgr-presence-key:v1:";
const MAX_STATEMENT_BYTES: usize = 512;

pub struct DeviceKeys<'a> {
    pub device_id: &'a str,
    pub sign_pub_b64: &'a str,
    pub kex_pub_b64: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnKeyStatement {
    user_id: String,
    device_fp: String,
    device_id: String,
    spawn_pub: Vec<u8>,
    issued_at_ms: u64,
}

impl SpawnKeyStatement {
    pub fn user_id(&self) -> &str {
        &self.user_id
    }
    pub fn device_fp(&self) -> &str {
        &self.device_fp
    }
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
    pub fn spawn_pub(&self) -> &[u8] {
        &self.spawn_pub
    }
    pub fn issued_at_ms(&self) -> u64 {
        self.issued_at_ms
    }
    pub fn spawn_key_fp(&self) -> String {
        hex::encode(Sha256::digest(&self.spawn_pub))
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        user_id: &str,
        device_fp: &str,
        device_id: &str,
        spawn_pub: Vec<u8>,
        issued_at_ms: u64,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            device_fp: device_fp.into(),
            device_id: device_id.into(),
            spawn_pub,
            issued_at_ms,
        }
    }
}

pub fn device_fp(sign_pub_b64: &str, kex_pub_b64: &str) -> String {
    let input = format!("tmgr-chat-fp:v1:{sign_pub_b64}:{kex_pub_b64}");
    hex::encode(Sha256::digest(input.as_bytes()))
}

pub fn build_spawn_key_statement(
    user_id: &str,
    device_fp: &str,
    device_id: &str,
    spawn_pub_b64: &str,
    issued_at_ms: u64,
) -> String {
    format!("{PREFIX}{user_id}:{device_fp}:{device_id}:{spawn_pub_b64}:{issued_at_ms}")
}

pub fn build_presence_key_statement(
    user_id: &str,
    device_fp: &str,
    device_id: &str,
    presence_pub_b64: &str,
    issued_at_ms: u64,
) -> String {
    format!("{PRESENCE_PREFIX}{user_id}:{device_fp}:{device_id}:{presence_pub_b64}:{issued_at_ms}")
}

pub fn verify_spawn_key_statement(
    expected_user_id: &str,
    statement: &str,
    sig_b64: &str,
    spawn_sig_b64: &str,
    device: &DeviceKeys<'_>,
    trust: &dyn TrustView,
    now_ms: u64,
) -> Result<SpawnKeyStatement, Refusal> {
    verify_key_statement(
        PREFIX,
        expected_user_id,
        statement,
        sig_b64,
        spawn_sig_b64,
        device,
        trust,
        now_ms,
    )
}

/// Same checks as a spawn key statement under its own prefix; the result is kept in a separate book.
pub fn verify_presence_key_statement(
    expected_user_id: &str,
    statement: &str,
    sig_b64: &str,
    presence_sig_b64: &str,
    device: &DeviceKeys<'_>,
    trust: &dyn TrustView,
    now_ms: u64,
) -> Result<SpawnKeyStatement, Refusal> {
    verify_key_statement(
        PRESENCE_PREFIX,
        expected_user_id,
        statement,
        sig_b64,
        presence_sig_b64,
        device,
        trust,
        now_ms,
    )
}

#[allow(clippy::too_many_arguments)]
fn verify_key_statement(
    prefix: &str,
    expected_user_id: &str,
    statement: &str,
    sig_b64: &str,
    spawn_sig_b64: &str,
    device: &DeviceKeys<'_>,
    trust: &dyn TrustView,
    now_ms: u64,
) -> Result<SpawnKeyStatement, Refusal> {
    if statement.len() > MAX_STATEMENT_BYTES {
        return Err(Refusal::BadToken);
    }
    let body = statement.strip_prefix(prefix).ok_or(Refusal::BadToken)?;
    let parts: Vec<&str> = body.splitn(6, ':').collect();
    let [user_id, fp, device_id, spawn_pub_b64, issued] = parts[..] else {
        return Err(Refusal::BadToken);
    };
    let issued_at_ms = parse_canonical_u64(issued)
        .filter(|n| *n <= MAX_SAFE_INT)
        .ok_or(Refusal::BadToken)?;
    if !is_token(user_id) || !is_token(device_id) || !is_hex_lower(fp, 64) {
        return Err(Refusal::BadToken);
    }
    let spawn_pub = b64_decode_strict(spawn_pub_b64).ok_or(Refusal::BadToken)?;
    if user_id != expected_user_id
        || device_id != device.device_id
        || fp != device_fp(device.sign_pub_b64, device.kex_pub_b64)
    {
        return Err(Refusal::Mismatch);
    }
    if issued_at_ms > now_ms.saturating_add(MAX_SKEW_MS) {
        return Err(Refusal::ClockSkew);
    }
    let spawn_key = parse_p256_key(&spawn_pub).ok_or(Refusal::UnknownKey)?;
    let sign_pub = b64_decode_strict(device.sign_pub_b64).ok_or(Refusal::UnknownKey)?;
    let device_key = parse_p256_key(&sign_pub).ok_or(Refusal::UnknownKey)?;
    let kex_pub = b64_decode_strict(device.kex_pub_b64).ok_or(Refusal::UnknownKey)?;
    parse_p256_key(&kex_pub).ok_or(Refusal::UnknownKey)?;
    if spawn_pub == sign_pub || spawn_pub == kex_pub {
        return Err(Refusal::UnknownKey);
    }
    if trust.status(device_id, fp) != TrustStatus::Trusted {
        return Err(Refusal::NotTrusted);
    }
    verify_der_b64(&device_key, statement, sig_b64)?;
    verify_der_b64(&spawn_key, statement, spawn_sig_b64)?;
    Ok(SpawnKeyStatement {
        user_id: user_id.into(),
        device_fp: fp.into(),
        device_id: device_id.into(),
        spawn_pub,
        issued_at_ms,
    })
}

/// Same user and device_fp with a strictly greater issued_at_ms; an equal issued_at_ms with a different key is a conflict to refuse, a byte-identical statement is a no-op.
pub fn supersedes(new: &SpawnKeyStatement, old: &SpawnKeyStatement) -> bool {
    new.user_id == old.user_id
        && new.device_fp == old.device_fp
        && new.issued_at_ms > old.issued_at_ms
}

fn parse_canonical_u64(s: &str) -> Option<u64> {
    let n: u64 = s.parse().ok()?;
    (n.to_string() == s).then_some(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::b64_encode;
    use p256::ecdsa::signature::Signer;
    use p256::ecdsa::{Signature, SigningKey};

    struct Trust(TrustStatus);
    impl TrustView for Trust {
        fn status(&self, device_id: &str, _: &str) -> TrustStatus {
            if device_id == "dev-1" {
                self.0
            } else {
                TrustStatus::NotTrusted
            }
        }
    }

    const NOW: u64 = 1_000;

    fn pubb64(k: &SigningKey) -> String {
        b64_encode(k.verifying_key().to_encoded_point(false).as_bytes())
    }

    fn sign(k: &SigningKey, m: &str) -> String {
        let s: Signature = k.sign(m.as_bytes());
        b64_encode(s.to_der().as_bytes())
    }

    struct Fx {
        dev: SigningKey,
        spawn: SigningKey,
        sign_pub: String,
        kex_pub: String,
        stmt: String,
    }

    fn fx_with(spawn_pub_b64: Option<String>, issued: u64) -> Fx {
        let dev = SigningKey::from_slice(&[5; 32]).unwrap();
        let spawn = SigningKey::from_slice(&[6; 32]).unwrap();
        let kex = SigningKey::from_slice(&[7; 32]).unwrap();
        let (sign_pub, kex_pub) = (pubb64(&dev), pubb64(&kex));
        let fp = device_fp(&sign_pub, &kex_pub);
        let sp = spawn_pub_b64.unwrap_or_else(|| pubb64(&spawn));
        let stmt = build_spawn_key_statement("42", &fp, "dev-1", &sp, issued);
        Fx {
            dev,
            spawn,
            sign_pub,
            kex_pub,
            stmt,
        }
    }

    fn fx() -> Fx {
        fx_with(None, 42)
    }

    fn run_with(
        f: &Fx,
        stmt: &str,
        sig: &str,
        ssig: &str,
        t: Trust,
        kex: &str,
    ) -> Result<SpawnKeyStatement, Refusal> {
        let d = DeviceKeys {
            device_id: "dev-1",
            sign_pub_b64: &f.sign_pub,
            kex_pub_b64: kex,
        };
        verify_spawn_key_statement("42", stmt, sig, ssig, &d, &t, NOW)
    }

    fn run(
        f: &Fx,
        stmt: &str,
        sig: &str,
        ssig: &str,
        t: Trust,
    ) -> Result<SpawnKeyStatement, Refusal> {
        run_with(f, stmt, sig, ssig, t, &f.kex_pub)
    }

    fn both(f: &Fx, stmt: &str) -> (String, String) {
        (sign(&f.dev, stmt), sign(&f.spawn, stmt))
    }

    #[test]
    fn accepts_doubly_signed_statement() {
        let f = fx();
        let (a, b) = both(&f, &f.stmt);
        let r = run(&f, &f.stmt, &a, &b, Trust(TrustStatus::Trusted)).unwrap();
        assert_eq!(r.issued_at_ms(), 42);
        assert_eq!(r.spawn_pub().len(), 65);
    }

    #[test]
    fn each_signature_is_required() {
        let f = fx();
        let good_d = sign(&f.dev, &f.stmt);
        let good_s = sign(&f.spawn, &f.stmt);
        assert_eq!(
            run(&f, &f.stmt, &good_d, &good_d, Trust(TrustStatus::Trusted)),
            Err(Refusal::BadSig)
        );
        assert_eq!(
            run(&f, &f.stmt, &good_s, &good_s, Trust(TrustStatus::Trusted)),
            Err(Refusal::BadSig)
        );
    }

    #[test]
    fn trust_user_and_format_checks() {
        let f = fx();
        let (a, b) = both(&f, &f.stmt);
        assert_eq!(
            run(&f, &f.stmt, &a, &b, Trust(TrustStatus::NotTrusted)),
            Err(Refusal::NotTrusted)
        );
        assert_eq!(
            run(&f, &f.stmt, &a, &b, Trust(TrustStatus::Revoked)),
            Err(Refusal::NotTrusted)
        );
        let bad: [(&str, String, Refusal); 4] = [
            (
                "other_user",
                f.stmt.replacen(":42:", ":43:", 1),
                Refusal::Mismatch,
            ),
            (
                "leading_zero",
                f.stmt.replace(":42", ":042"),
                Refusal::BadToken,
            ),
            (
                "plus_sign",
                f.stmt.replace(":42", ":+42"),
                Refusal::BadToken,
            ),
            ("extra_segment", format!("{}:x", f.stmt), Refusal::BadToken),
        ];
        for (name, s, want) in bad {
            let (sa, sb) = both(&f, &s);
            assert_eq!(
                run(&f, &s, &sa, &sb, Trust(TrustStatus::Trusted)),
                Err(want),
                "{name}"
            );
        }
        assert_eq!(
            run(&f, "nope", &a, &b, Trust(TrustStatus::Trusted)),
            Err(Refusal::BadToken)
        );
    }

    #[test]
    fn future_dated_statement_is_clock_skew() {
        let at_limit = fx_with(None, NOW + MAX_SKEW_MS);
        let (a, b) = both(&at_limit, &at_limit.stmt);
        assert!(run(
            &at_limit,
            &at_limit.stmt,
            &a,
            &b,
            Trust(TrustStatus::Trusted)
        )
        .is_ok());
        let future = fx_with(None, NOW + MAX_SKEW_MS + 1);
        let (a, b) = both(&future, &future.stmt);
        assert_eq!(
            run(&future, &future.stmt, &a, &b, Trust(TrustStatus::Trusted)),
            Err(Refusal::ClockSkew)
        );
    }

    #[test]
    fn issued_above_js_safe_integer_is_bad_token() {
        let f = fx_with(None, 1 << 53);
        let (a, b) = both(&f, &f.stmt);
        assert_eq!(
            run(&f, &f.stmt, &a, &b, Trust(TrustStatus::Trusted)),
            Err(Refusal::BadToken)
        );
    }

    #[test]
    fn oversized_inputs_are_refused() {
        let f = fx();
        let (a, b) = both(&f, &f.stmt);
        let long = "A".repeat(97);
        assert_eq!(
            run(&f, &f.stmt, &long, &b, Trust(TrustStatus::Trusted)),
            Err(Refusal::BadSig)
        );
        assert_eq!(
            run(&f, &f.stmt, &a, &long, Trust(TrustStatus::Trusted)),
            Err(Refusal::BadSig)
        );
        let huge = format!("{}{}", f.stmt, "0".repeat(600));
        assert_eq!(
            run(&f, &huge, &a, &b, Trust(TrustStatus::Trusted)),
            Err(Refusal::BadToken)
        );
    }

    #[test]
    fn spawn_key_must_be_uncompressed_and_distinct_from_sign_key() {
        let spawn = SigningKey::from_slice(&[6; 32]).unwrap();
        let compressed = b64_encode(spawn.verifying_key().to_encoded_point(true).as_bytes());
        let f = fx_with(Some(compressed), 42);
        let (a, b) = both(&f, &f.stmt);
        assert_eq!(
            run(&f, &f.stmt, &a, &b, Trust(TrustStatus::Trusted)),
            Err(Refusal::UnknownKey)
        );
        let f = fx_with(None, 42);
        let same = fx_with(Some(f.sign_pub.clone()), 42);
        let (a, b) = (sign(&f.dev, &same.stmt), sign(&f.dev, &same.stmt));
        assert_eq!(
            run(&same, &same.stmt, &a, &b, Trust(TrustStatus::Trusted)),
            Err(Refusal::UnknownKey)
        );
    }

    #[test]
    fn kex_key_must_decode_strictly() {
        let f = fx();
        let sloppy = format!("{}\n", f.kex_pub);
        let fp = device_fp(&f.sign_pub, &sloppy);
        let stmt = build_spawn_key_statement("42", &fp, "dev-1", &pubb64(&f.spawn), 42);
        let (a, b) = both(&f, &stmt);
        assert_eq!(
            run_with(&f, &stmt, &a, &b, Trust(TrustStatus::Trusted), &sloppy),
            Err(Refusal::UnknownKey)
        );
    }

    #[test]
    fn kex_key_must_be_uncompressed_on_curve_and_distinct_from_spawn_key() {
        let f = fx();
        let kex = SigningKey::from_slice(&[7; 32]).unwrap();
        let bad_kex = [
            b64_encode(kex.verifying_key().to_encoded_point(true).as_bytes()),
            b64_encode(&[4; 65]),
            b64_encode(&[4; 64]),
            pubb64(&f.spawn),
        ];
        for k in bad_kex {
            let fp = device_fp(&f.sign_pub, &k);
            let stmt = build_spawn_key_statement("42", &fp, "dev-1", &pubb64(&f.spawn), 42);
            let (a, b) = both(&f, &stmt);
            assert_eq!(
                run_with(&f, &stmt, &a, &b, Trust(TrustStatus::Trusted), &k),
                Err(Refusal::UnknownKey),
                "{k}"
            );
        }
    }

    #[test]
    fn only_strictly_newer_statements_supersede() {
        let f = fx();
        let (a, b) = both(&f, &f.stmt);
        let old = run(&f, &f.stmt, &a, &b, Trust(TrustStatus::Trusted)).unwrap();
        let mut newer = old.clone();
        newer.issued_at_ms += 1;
        assert!(supersedes(&newer, &old));
        assert!(!supersedes(&old, &newer));
        let mut same_time_other_key = old.clone();
        same_time_other_key.spawn_pub[1] ^= 1;
        assert!(!supersedes(&same_time_other_key, &old));
        assert!(!supersedes(&old, &old));
        let mut other_device = newer.clone();
        other_device.device_fp = "e".repeat(64);
        assert!(!supersedes(&other_device, &old));
    }

    #[test]
    fn fp_matches_plan_formula() {
        let want = hex::encode(Sha256::digest(b"tmgr-chat-fp:v1:A:B"));
        assert_eq!(device_fp("A", "B"), want);
    }

    #[test]
    fn spawn_and_presence_statements_never_cross() {
        let f = fx();
        let fp = device_fp(&f.sign_pub, &f.kex_pub);
        let pres = build_presence_key_statement("42", &fp, "dev-1", &pubb64(&f.spawn), 42);
        assert!(pres.starts_with("tmgr-presence-key:v1:"));
        assert_eq!(
            pres.strip_prefix("tmgr-presence-key:v1:"),
            f.stmt.strip_prefix("tmgr-spawn-key:v1:")
        );
        let d = DeviceKeys {
            device_id: "dev-1",
            sign_pub_b64: &f.sign_pub,
            kex_pub_b64: &f.kex_pub,
        };
        let t = Trust(TrustStatus::Trusted);
        let (pa, pb) = both(&f, &pres);
        let got = verify_presence_key_statement("42", &pres, &pa, &pb, &d, &t, NOW).unwrap();
        assert_eq!(got.spawn_pub().len(), 65);
        assert_eq!(
            verify_spawn_key_statement("42", &pres, &pa, &pb, &d, &t, NOW),
            Err(Refusal::BadToken)
        );
        let (sa, sb) = both(&f, &f.stmt);
        assert_eq!(
            verify_presence_key_statement("42", &f.stmt, &sa, &sb, &d, &t, NOW),
            Err(Refusal::BadToken)
        );
        assert_eq!(
            verify_presence_key_statement("42", &pres, &sa, &sb, &d, &t, NOW),
            Err(Refusal::BadSig)
        );
    }

    #[test]
    fn a_presence_statement_needs_both_signatures_and_trust() {
        let f = fx();
        let fp = device_fp(&f.sign_pub, &f.kex_pub);
        let pres = build_presence_key_statement("42", &fp, "dev-1", &pubb64(&f.spawn), 42);
        let d = DeviceKeys {
            device_id: "dev-1",
            sign_pub_b64: &f.sign_pub,
            kex_pub_b64: &f.kex_pub,
        };
        let t = Trust(TrustStatus::Trusted);
        let (a, b) = both(&f, &pres);
        assert_eq!(
            verify_presence_key_statement("42", &pres, &a, &a, &d, &t, NOW),
            Err(Refusal::BadSig)
        );
        assert_eq!(
            verify_presence_key_statement("42", &pres, &b, &b, &d, &t, NOW),
            Err(Refusal::BadSig)
        );
        for st in [
            TrustStatus::NotTrusted,
            TrustStatus::Revoked,
            TrustStatus::Unknown,
        ] {
            assert_eq!(
                verify_presence_key_statement("42", &pres, &a, &b, &d, &Trust(st), NOW),
                Err(Refusal::NotTrusted)
            );
        }
    }
}
