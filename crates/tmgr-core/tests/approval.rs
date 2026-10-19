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
        let repos = repos_sha256(&names).unwrap();
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
        let mut fewer = tmgr_core::approval::canonical_repos(&names).unwrap();
        fewer.pop();
        let other = message_v2(
            args.0,
            args.1,
            args.2,
            args.3,
            args.4,
            &hash,
            &repos_sha256(&fewer).unwrap_or_default(),
            s(key, "key_id"),
            c["signed_at"].as_i64().unwrap(),
        );
        assert!(!verify(public, &other, s(c, "signature")));
    }
}

#[test]
fn repos_hash_ignores_order_and_refuses_non_canonical_lists() {
    let a = repos_sha256(&["tmgr-dev/frontend".into(), "tmgr-dev/backend".into()]).unwrap();
    let b = repos_sha256(&["tmgr-dev/backend".into(), "tmgr-dev/frontend".into()]).unwrap();
    assert_eq!(a, b);
    let f = fixtures();
    for list in f["invalid_repositories"].as_array().unwrap() {
        let names: Vec<String> = list
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap().to_string())
            .collect();
        assert!(repos_sha256(&names).is_err(), "{names:?}");
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
    assert!(a
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    assert_ne!(a, b);
}

fn payload(c: &Value, key_id: &str) -> tmgr_core::approval::Payload {
    let opt = |k: &str| c[k].as_str().map(String::from);
    tmgr_core::approval::Payload {
        host: s(c, "host").into(),
        user_id: c["user_id"].as_i64().unwrap(),
        workspace_id: c["workspace_id"].as_i64().unwrap(),
        task_id: c["task_id"].as_i64().unwrap(),
        category_id: c["category_id"].as_i64().unwrap(),
        content_sha256: s(c, "content_sha256").into(),
        repos_sha256: opt("repos_sha256"),
        persona_sha256: opt("persona_sha256"),
        context_sha256: opt("context_sha256"),
        key_id: key_id.into(),
        signed_at: c["signed_at"].as_i64().unwrap(),
    }
}

#[test]
fn v3_hashes_messages_and_signatures_match_the_shared_fixtures() {
    use tmgr_core::approval::{approval_message, context_sha256, page_sha256, persona_sha256};
    let f = fixtures();
    let key = &f["key"];
    let public = s(key, "public_key");
    let cases = f["cases_v3"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    for c in cases {
        let t = s(c, "title");
        assert_eq!(
            content_sha256(t, c["description"].as_str().unwrap_or("")),
            s(c, "content_sha256"),
            "{t}"
        );
        if let Some(p) = c.get("persona") {
            let h = persona_sha256(s(p, "id"), s(p, "name"), s(p, "instructions")).unwrap();
            assert_eq!(h, s(c, "persona_sha256"), "{t}");
        }
        if let Some(pages) = c.get("context") {
            let hashes: Vec<String> = pages
                .as_array()
                .unwrap()
                .iter()
                .map(|p| {
                    let h = page_sha256(p["id"].as_i64().unwrap(), s(p, "title"), s(p, "body"));
                    assert_eq!(h, s(p, "sha256"), "{t}");
                    h
                })
                .collect();
            assert_eq!(context_sha256(&hashes).unwrap(), s(c, "context_sha256"));
        }
        let msg = approval_message(&payload(c, s(key, "key_id")));
        assert_eq!(msg, s(c, "message"), "{t}");
        assert!(msg.starts_with("tmgr-agent-approval:v3:"));
        assert!(verify(public, &msg, s(c, "signature")), "{t}");
        assert!(!verify(public, &msg, s(c, "tampered_signature")));
    }
}

#[test]
fn the_version_follows_what_is_covered() {
    use tmgr_core::approval::approval_message;
    let f = fixtures();
    let kid = s(&f["key"], "key_id");
    let mut p = payload(&f["cases_v3"][0], kid);
    p.persona_sha256 = None;
    p.context_sha256 = None;
    assert!(approval_message(&p).starts_with("tmgr-agent-approval:v1:"));
    p.repos_sha256 = Some(s(&f["cases_v2"][0], "repos_sha256").into());
    assert!(approval_message(&p).starts_with("tmgr-agent-approval:v2:"));
    p.context_sha256 = Some(s(&f["cases_v3"][0], "context_sha256").into());
    assert!(approval_message(&p).starts_with("tmgr-agent-approval:v3:"));
}

#[test]
fn persona_ids_must_be_lowercase_uuids_and_context_has_one_to_five_pages() {
    use tmgr_core::approval::{context_sha256, persona_sha256, MAX_CONTEXT_PAGES};
    let f = fixtures();
    for id in f["invalid_persona_ids"].as_array().unwrap() {
        assert!(
            persona_sha256(id.as_str().unwrap(), "n", "i").is_err(),
            "{id}"
        );
    }
    let h = s(&f["cases_v3"][0]["context"][0], "sha256").to_string();
    assert!(context_sha256(&[]).is_err());
    assert!(context_sha256(&vec![h.clone(); MAX_CONTEXT_PAGES + 1]).is_err());
    assert!(context_sha256(&vec![h.clone(); MAX_CONTEXT_PAGES]).is_ok());
    assert!(context_sha256(&["ABC".into()]).is_err());
    let swapped = context_sha256(&[
        s(&f["cases_v3"][2]["context"][1], "sha256").into(),
        s(&f["cases_v3"][2]["context"][0], "sha256").into(),
    ])
    .unwrap();
    assert_ne!(swapped, s(&f["cases_v3"][2], "context_sha256"));
}

#[test]
fn the_caps_match_the_shared_limits() {
    use tmgr_core::approval::{MAX_CONTEXT_BYTES, MAX_CONTEXT_PAGES, MAX_PERSONA_BYTES};
    let l = &fixtures()["limits"];
    assert_eq!(l["max_persona_bytes"], MAX_PERSONA_BYTES);
    assert_eq!(l["max_context_pages"], MAX_CONTEXT_PAGES);
    assert_eq!(l["max_context_bytes"], MAX_CONTEXT_BYTES);
}
