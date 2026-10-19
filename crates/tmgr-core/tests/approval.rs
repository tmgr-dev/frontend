use serde_json::Value;
use tmgr_core::approval::{content_sha256, fingerprint, message, verify};

fn fixtures() -> Value {
    serde_json::from_str(include_str!("fixtures/agent-approval-fixtures.json")).unwrap()
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap()
}

#[test]
fn hashes_messages_and_signatures_match_the_shared_fixtures() {
    let f = fixtures();
    let key = &f["key"];
    let public = s(key, "public_key");
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 7);
    for c in cases {
        let hash = content_sha256(s(c, "title"), c["description"].as_str().unwrap_or(""));
        assert_eq!(hash, s(c, "content_sha256"), "{}", s(c, "title"));
        let msg = message(
            s(c, "host"),
            c["user_id"].as_i64().unwrap(),
            c["workspace_id"].as_i64().unwrap(),
            c["task_id"].as_i64().unwrap(),
            c["category_id"].as_i64().unwrap(),
            &hash,
            s(key, "key_id"),
            c["signed_at"].as_i64().unwrap(),
        );
        assert_eq!(msg, s(c, "message"));
        assert!(verify(public, &msg, s(c, "signature")), "{}", s(c, "title"));
        assert!(!verify(public, &msg, s(c, "tampered_signature")));
        assert!(!verify(s(&f, "other_public_key"), &msg, s(c, "signature")));
        assert!(!verify(public, &format!("{msg}x"), s(c, "signature")));
    }
}

#[test]
fn garbage_inputs_never_verify() {
    let f = fixtures();
    let public = s(&f["key"], "public_key");
    let sig = s(&f["cases"][0], "signature");
    assert!(!verify("", "m", sig));
    assert!(!verify(public, "m", ""));
    assert!(!verify("!!!", "m", "!!!"));
    assert!(!verify(public, "m", "AAAA"));
}

#[test]
fn fingerprint_is_sixteen_hex_chars_of_the_key_hash() {
    let f = fixtures();
    let a = fingerprint(s(&f["key"], "public_key"));
    let b = fingerprint(s(&f, "other_public_key"));
    assert_eq!(a.len(), 16);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    assert_ne!(a, b);
}
