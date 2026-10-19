use serde_json::Value;
use tmgr_core::approval::{content_sha256, fingerprint, message, message_v2, repos_sha256, verify};

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
fn v2_messages_cover_the_repository_set() {
    let f = fixtures();
    let key = &f["key"];
    let public = s(key, "public_key");
    let cases = f["cases_v2"].as_array().unwrap();
    assert_eq!(cases.len(), 3);
    for c in cases {
        let names: Vec<String> = c["repositories"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap().to_string())
            .collect();
        let repos = repos_sha256(&names);
        assert_eq!(repos, s(c, "repos_sha256"), "{}", s(c, "title"));
        let hash = content_sha256(s(c, "title"), c["description"].as_str().unwrap_or(""));
        assert_eq!(hash, s(c, "content_sha256"));
        let args = (
            s(c, "host"),
            c["user_id"].as_i64().unwrap(),
            c["workspace_id"].as_i64().unwrap(),
            c["task_id"].as_i64().unwrap(),
            c["category_id"].as_i64().unwrap(),
        );
        let msg = message_v2(
            args.0,
            args.1,
            args.2,
            args.3,
            args.4,
            &hash,
            &repos,
            s(key, "key_id"),
            c["signed_at"].as_i64().unwrap(),
        );
        assert_eq!(msg, s(c, "message"));
        assert!(verify(public, &msg, s(c, "signature")));
        assert!(!verify(public, &msg, s(c, "tampered_signature")));
        let v1 = message(
            args.0,
            args.1,
            args.2,
            args.3,
            args.4,
            &hash,
            s(key, "key_id"),
            c["signed_at"].as_i64().unwrap(),
        );
        assert!(!verify(public, &v1, s(c, "signature")));
        let mut fewer = tmgr_core::approval::canonical_repos(&names);
        fewer.pop();
        let other = message_v2(
            args.0,
            args.1,
            args.2,
            args.3,
            args.4,
            &hash,
            &repos_sha256(&fewer),
            s(key, "key_id"),
            c["signed_at"].as_i64().unwrap(),
        );
        assert!(!verify(public, &other, s(c, "signature")));
    }
}

#[test]
fn repos_hash_ignores_order_case_and_duplicates() {
    let a = repos_sha256(&["tmgr-dev/frontend".into(), "tmgr-dev/backend".into()]);
    let b = repos_sha256(&[
        "TMGR-dev/Backend".into(),
        "tmgr-dev/frontend".into(),
        "tmgr-dev/backend".into(),
    ]);
    assert_eq!(a, b);
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
    assert!(a
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    assert_ne!(a, b);
}
