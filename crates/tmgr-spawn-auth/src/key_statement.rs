use crate::error::Refusal;
use crate::tokens::{b64_decode_strict, is_hex_lower, is_token};
use crate::verify::{verify_der_b64, TrustView};
use p256::ecdsa::VerifyingKey;
use sha2::{Digest, Sha256};

const PREFIX: &str = "tmgr-spawn-key:v1:";

pub struct DeviceKeys<'a> {
    pub device_id: &'a str,
    pub sign_pub_b64: &'a str,
    pub kex_pub_b64: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnKeyStatement {
    pub user_id: String,
    pub device_fp: String,
    pub device_id: String,
    pub spawn_pub: Vec<u8>,
    pub issued_at_ms: u64,
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

pub fn verify_spawn_key_statement(
    expected_user_id: &str,
    statement: &str,
    sig_b64: &str,
    spawn_sig_b64: &str,
    device: &DeviceKeys<'_>,
    trust: &dyn TrustView,
) -> Result<SpawnKeyStatement, Refusal> {
    let body = statement.strip_prefix(PREFIX).ok_or(Refusal::BadToken)?;
    let parts: Vec<&str> = body.split(':').collect();
    let [user_id, fp, device_id, spawn_pub_b64, issued] = parts[..] else {
        return Err(Refusal::BadToken);
    };
    let issued_at_ms = parse_canonical_u64(issued).ok_or(Refusal::BadToken)?;
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
    let spawn_key = VerifyingKey::from_sec1_bytes(&spawn_pub).map_err(|_| Refusal::UnknownKey)?;
    let sign_pub = b64_decode_strict(device.sign_pub_b64).ok_or(Refusal::UnknownKey)?;
    let device_key = VerifyingKey::from_sec1_bytes(&sign_pub).map_err(|_| Refusal::UnknownKey)?;
    if !trust.is_trusted(fp) || trust.is_revoked(fp) {
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

    struct Trust(bool, bool);
    impl TrustView for Trust {
        fn is_trusted(&self, _: &str) -> bool {
            self.0
        }
        fn is_revoked(&self, _: &str) -> bool {
            self.1
        }
    }

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

    fn fx() -> Fx {
        let dev = SigningKey::from_slice(&[5; 32]).unwrap();
        let spawn = SigningKey::from_slice(&[6; 32]).unwrap();
        let kex = SigningKey::from_slice(&[7; 32]).unwrap();
        let (sign_pub, kex_pub) = (pubb64(&dev), pubb64(&kex));
        let fp = device_fp(&sign_pub, &kex_pub);
        let stmt = build_spawn_key_statement("u1", &fp, "dev-1", &pubb64(&spawn), 42);
        Fx {
            dev,
            spawn,
            sign_pub,
            kex_pub,
            stmt,
        }
    }

    fn run(
        f: &Fx,
        stmt: &str,
        sig: &str,
        ssig: &str,
        t: Trust,
    ) -> Result<SpawnKeyStatement, Refusal> {
        let d = DeviceKeys {
            device_id: "dev-1",
            sign_pub_b64: &f.sign_pub,
            kex_pub_b64: &f.kex_pub,
        };
        verify_spawn_key_statement("u1", stmt, sig, ssig, &d, &t)
    }

    #[test]
    fn accepts_doubly_signed_statement() {
        let f = fx();
        let r = run(
            &f,
            &f.stmt,
            &sign(&f.dev, &f.stmt),
            &sign(&f.spawn, &f.stmt),
            Trust(true, false),
        )
        .unwrap();
        assert_eq!(r.issued_at_ms, 42);
        assert_eq!(r.spawn_pub.len(), 65);
    }

    #[test]
    fn each_signature_is_required() {
        let f = fx();
        let good_d = sign(&f.dev, &f.stmt);
        let good_s = sign(&f.spawn, &f.stmt);
        assert_eq!(
            run(&f, &f.stmt, &good_d, &good_d, Trust(true, false)),
            Err(Refusal::BadSig)
        );
        assert_eq!(
            run(&f, &f.stmt, &good_s, &good_s, Trust(true, false)),
            Err(Refusal::BadSig)
        );
    }

    #[test]
    fn trust_user_and_format_checks() {
        let f = fx();
        let (a, b) = (sign(&f.dev, &f.stmt), sign(&f.spawn, &f.stmt));
        assert_eq!(
            run(&f, &f.stmt, &a, &b, Trust(false, false)),
            Err(Refusal::NotTrusted)
        );
        assert_eq!(
            run(&f, &f.stmt, &a, &b, Trust(true, true)),
            Err(Refusal::NotTrusted)
        );
        let other_user = f.stmt.replacen(":u1:", ":u2:", 1);
        assert_eq!(
            run(
                &f,
                &other_user,
                &sign(&f.dev, &other_user),
                &sign(&f.spawn, &other_user),
                Trust(true, false)
            ),
            Err(Refusal::Mismatch)
        );
        let padded = f.stmt.replace(":42", ":042");
        assert_eq!(
            run(
                &f,
                &padded,
                &sign(&f.dev, &padded),
                &sign(&f.spawn, &padded),
                Trust(true, false)
            ),
            Err(Refusal::BadToken)
        );
        assert_eq!(
            run(&f, "nope", &a, &b, Trust(true, false)),
            Err(Refusal::BadToken)
        );
    }

    #[test]
    fn fp_matches_plan_formula() {
        let want = hex::encode(Sha256::digest(b"tmgr-chat-fp:v1:A:B"));
        assert_eq!(device_fp("A", "B"), want);
    }
}
