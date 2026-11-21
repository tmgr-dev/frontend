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
    fn strict_base64() {
        assert_eq!(b64_decode_strict("YQ==").unwrap(), b"a");
        assert!(b64_decode_strict("YQ").is_none());
        assert!(b64_decode_strict("YR==").is_none());
        assert!(b64_decode_strict("YQ==\n").is_none());
        assert!(b64_decode_strict("YQ-=").is_none());
    }
}
