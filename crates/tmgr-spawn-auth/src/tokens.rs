use base64::engine::general_purpose::STANDARD;
use base64::Engine;

pub fn is_token(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

pub fn is_hex_lower(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn is_target_id(s: &str) -> bool {
    s.len() <= 64
        && s.strip_prefix("mx_").is_some_and(|h| {
            !h.is_empty() && h.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        })
}

pub(crate) fn parse_p256_key(bytes: &[u8]) -> Option<p256::ecdsa::VerifyingKey> {
    if bytes.len() != 65 || bytes[0] != 0x04 {
        return None;
    }
    p256::ecdsa::VerifyingKey::from_sec1_bytes(bytes).ok()
}

pub(crate) const MAX_SAFE_INT: u64 = (1 << 53) - 1;

pub fn b64_encode(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

pub fn b64_decode_strict(s: &str) -> Option<Vec<u8>> {
    let raw = STANDARD.decode(s).ok()?;
    (STANDARD.encode(&raw) == s).then_some(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens() {
        assert!(is_token("mx_ab.C-1"));
        assert!(is_token(&"a".repeat(64)));
        assert!(!is_token(&"a".repeat(65)));
        assert!(!is_token(""));
        assert!(!is_token("a:b"));
        assert!(!is_token("a b"));
        assert!(!is_token("a\n"));
        assert!(!is_token("é"));
    }

    #[test]
    fn hex() {
        assert!(is_hex_lower(&"a1".repeat(16), 32));
        assert!(!is_hex_lower(&"A1".repeat(16), 32));
        assert!(!is_hex_lower(&"a1".repeat(15), 32));
    }

    #[test]
    fn target_ids() {
        assert!(is_target_id("mx_0123456789abcdef"));
        assert!(is_target_id("mx_0"));
        assert!(!is_target_id("mx_"));
        assert!(!is_target_id("mx_ABC"));
        assert!(!is_target_id("mx_xyz"));
        assert!(!is_target_id("my_00"));
        assert!(!is_target_id(&format!("mx_{}", "a".repeat(62))));
        assert!(is_target_id(&format!("mx_{}", "a".repeat(61))));
    }

    #[test]
    fn p256_keys_must_be_uncompressed_and_on_curve() {
        let k = p256::ecdsa::SigningKey::from_slice(&[3; 32]).unwrap();
        let full = k.verifying_key().to_encoded_point(false);
        let short = k.verifying_key().to_encoded_point(true);
        assert!(parse_p256_key(full.as_bytes()).is_some());
        assert!(parse_p256_key(short.as_bytes()).is_none());
        assert!(parse_p256_key(&full.as_bytes()[..64]).is_none());
        assert!(parse_p256_key(&[4; 65]).is_none());
        let mut bad_tag = full.as_bytes().to_vec();
        bad_tag[0] = 6;
        assert!(parse_p256_key(&bad_tag).is_none());
    }

    #[test]
    fn strict_base64() {
        assert_eq!(b64_decode_strict("YQ==").unwrap(), b"a");
        assert!(b64_decode_strict("YQ").is_none());
        assert!(b64_decode_strict("YR==").is_none());
        assert!(b64_decode_strict("YQ==\n").is_none());
        assert!(b64_decode_strict("YQ-=").is_none());
    }
}
