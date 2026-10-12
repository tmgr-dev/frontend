use crate::secret;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

pub const SERVER: &str = "tmgr-runner-server";
pub const CLIENT: &str = "tmgr-runner-client";
pub const SHIM: &str = "tmgr-shim";
pub const RUNNER_ADOPT: &str = "tmgr-runner-adopt";

pub fn nonce() -> String {
    secret::generate()
}

pub fn proof(secret: &str, label: &str, first: &str, second: &str) -> String {
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes())
        .expect("hmac accepts any key length");
    mac.update(label.as_bytes());
    mac.update(&[0]);
    mac.update(first.as_bytes());
    mac.update(second.as_bytes());
    secret::hex(&mac.finalize().into_bytes())
}

pub fn verify(secret: &str, label: &str, first: &str, second: &str, given: &str) -> bool {
    secret::eq(&proof(secret, label, first, second), given)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hmac_sha256(key: &[u8], msg: &[u8]) -> String {
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key).unwrap();
        mac.update(msg);
        secret::hex(&mac.finalize().into_bytes())
    }

    #[test]
    fn rfc4231_case_1_and_2() {
        assert_eq!(
            hmac_sha256(&[0x0b; 20], b"Hi There"),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        assert_eq!(
            hmac_sha256(b"Jefe", b"what do ya want for nothing?"),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn proofs_depend_on_every_input() {
        let p = proof("k", SERVER, "a", "b");
        assert!(verify("k", SERVER, "a", "b", &p));
        assert!(!verify("k2", SERVER, "a", "b", &p));
        assert!(!verify("k", CLIENT, "a", "b", &p));
        assert!(!verify("k", SERVER, "b", "a", &p));
        assert!(!verify("k", SERVER, "a", "c", &p));
        assert!(!verify("k", SERVER, "a", "b", ""));
    }
}
