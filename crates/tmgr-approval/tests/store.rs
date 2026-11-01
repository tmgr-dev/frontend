use chrono::{TimeZone, Utc};
use serde_json::Value;
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use tmgr_approval::{check_claim, AllowFile, Entry, PinnedKey, RepoBinding, Rollback};
use tmgr_ipc::api::CloudApproval;

const FIXTURES: &str = include_str!("../../tmgr-core/tests/fixtures/agent-approval-fixtures.json");

fn fx() -> Value {
    serde_json::from_str(FIXTURES).unwrap()
}

fn entry(expires: Option<&str>) -> Entry {
    let f = fx();
    Entry {
        server: "api.tmgr.dev".into(),
        user_id: 1,
        workspace_id: 56,
        workspace_code: "tmgrdev".into(),
        allowed_at: "2025-10-01T00:00:00Z".into(),
        expires_at: expires.map(str::to_string),
        keys: vec![PinnedKey {
            key_id: f["key"]["key_id"].as_str().unwrap().into(),
            public_key: f["key"]["public_key"].as_str().unwrap().into(),
            device_name: "laptop".into(),
        }],
        repos: BTreeMap::new(),
        require_signed_rules: false,
        require_signed_persona: false,
    }
}

fn claim() -> (CloudApproval, String, String) {
    let f = fx();
    let c = &f["cases"][2];
    (
        CloudApproval {
            repos_sha256: None,
            persona_sha256: None,
            context_sha256: None,
            host: "api.tmgr.dev".into(),
            user_id: 1,
            workspace_id: 56,
            task_id: 9002,
            category_id: 127,
            content_sha256: c["content_sha256"].as_str().unwrap().into(),
            key_id: f["key"]["key_id"].as_str().unwrap().into(),
            signed_at: 1760000002,
            signature: c["signature"].as_str().unwrap().into(),
            kind: "author".into(),
        },
        c["title"].as_str().unwrap().into(),
        c["description"].as_str().unwrap().into(),
    )
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2025, 10, 10, 0, 0, 0).unwrap()
}

#[test]
fn valid_claim_passes() {
    let (c, t, b) = claim();
    check_claim(&entry(None), &c, &t, &b).unwrap();
}

#[test]
fn forged_signature_is_refused() {
    let (mut c, t, b) = claim();
    c.signature = fx()["cases"][2]["tampered_signature"]
        .as_str()
        .unwrap()
        .into();
    let r = check_claim(&entry(None), &c, &t, &b).unwrap_err();
    assert!(r.message.contains("signature is not valid"), "{r}");
}

#[test]
fn changed_text_is_refused_even_with_a_valid_signature() {
    let (c, t, b) = claim();
    let r = check_claim(&entry(None), &c, &t, &format!("{b} and rm -rf")).unwrap_err();
    assert!(r.message.contains("changed since it was approved"), "{r}");
    assert!(r.fix.contains("Confirm for agent runs"));
}

#[test]
fn key_that_is_not_pinned_is_refused_with_the_allow_command() {
    let (c, t, b) = claim();
    let mut e = entry(None);
    e.keys.clear();
    let r = check_claim(&e, &c, &t, &b).unwrap_err();
    assert!(r.message.contains("does not trust"));
    assert!(r.fix.contains("tmgr runner allow tmgrdev"));
}

#[test]
fn a_pinned_other_key_does_not_verify() {
    let (c, t, b) = claim();
    let mut e = entry(None);
    e.keys[0].public_key = fx()["other_public_key"].as_str().unwrap().into();
    let r = check_claim(&e, &c, &t, &b).unwrap_err();
    assert!(r.message.contains("signature is not valid"));
}

#[test]
fn identity_mismatch_is_refused() {
    let (c, t, b) = claim();
    for edit in [
        |c: &mut CloudApproval| c.user_id = 2,
        |c: &mut CloudApproval| c.host = "evil.example".into(),
        |c: &mut CloudApproval| c.workspace_id = 57,
        |c: &mut CloudApproval| c.kind = "other".into(),
    ] {
        let mut c = c.clone();
        edit(&mut c);
        assert!(check_claim(&entry(None), &c, &t, &b).is_err());
    }
}

#[test]
fn find_live_refuses_missing_and_expired_and_names_the_fix() {
    let mut f = AllowFile::default();
    let r = f
        .find_live("api.tmgr.dev", 1, 56, "tmgrdev", now())
        .unwrap_err();
    assert!(r.message.contains("not allowed"));
    assert_eq!(r.fix, "tmgr runner allow tmgrdev");
    f.upsert(entry(Some("2025-10-09T00:00:00Z")));
    let r = f
        .find_live("api.tmgr.dev", 1, 56, "tmgrdev", now())
        .unwrap_err();
    assert!(r.message.contains("expired on 2025-10-09"));
    f.upsert(entry(Some("2025-10-11T00:00:00Z")));
    assert!(f.find_live("api.tmgr.dev", 1, 56, "tmgrdev", now()).is_ok());
    assert_eq!(f.entries.len(), 1);
    f.upsert(entry(None));
    assert!(f.find_live("api.tmgr.dev", 1, 56, "tmgrdev", now()).is_ok());
    assert!(f
        .find_live("api.tmgr.dev", 2, 56, "tmgrdev", now())
        .is_err());
    assert!(f.find_live("other", 1, 56, "tmgrdev", now()).is_err());
}

#[test]
fn unreadable_expiry_counts_as_expired() {
    assert!(entry(Some("tomorrow")).expired(now()));
}

#[test]
fn store_round_trips_with_private_modes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tmgr").join("runner-allow.json");
    let mut f = AllowFile::default();
    let mut e = entry(None);
    e.repos.insert(
        "127".into(),
        tmgr_approval::RepoBinding::Path("/abs/repo".into()),
    );
    f.upsert(e.clone());
    f.save_to(&path).unwrap();
    assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(
        path.parent()
            .unwrap()
            .metadata()
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let back = AllowFile::load_from(&path).unwrap();
    assert_eq!(back, f);
    assert_eq!(back.entries[0].repo(127), Some("/abs/repo"));
    assert!(AllowFile::load_from(&dir.path().join("none.json"))
        .unwrap()
        .entries
        .is_empty());
    std::fs::write(&path, "{").unwrap();
    assert!(AllowFile::load_from(&path).is_err());
    std::fs::write(&path, r#"{"version":3,"entries":[]}"#).unwrap();
    assert!(AllowFile::load_from(&path).is_err());
}

#[test]
fn a_version_one_file_reads_unchanged_and_stays_version_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runner-allow.json");
    let mut e = serde_json::to_value(entry(None)).unwrap();
    e["repos"] = serde_json::json!({"127": "/abs/repo"});
    std::fs::write(
        &path,
        serde_json::json!({"version": 1, "entries": [e]}).to_string(),
    )
    .unwrap();
    let f = AllowFile::load_from(&path).unwrap();
    assert_eq!(f.entries[0].repo(127), Some("/abs/repo"));
    assert!(f.entries[0].repo_set(127).is_none());
    f.save_to(&path).unwrap();
    let raw: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(raw["version"], 1);
    assert_eq!(raw["entries"][0]["repos"]["127"], "/abs/repo");
}

#[test]
fn a_repository_set_is_saved_as_version_two() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runner-allow.json");
    let mut f = AllowFile::default();
    let mut e = entry(None);
    e.repos
        .insert("1".into(), RepoBinding::Path("/abs/one".into()));
    e.repos.insert(
        "127".into(),
        RepoBinding::Set(BTreeMap::from([
            ("tmgr-dev/backend".to_string(), "/abs/backend".to_string()),
            ("tmgr-dev/frontend".to_string(), "/abs/frontend".to_string()),
        ])),
    );
    f.upsert(e);
    f.save_to(&path).unwrap();
    let raw: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(raw["version"], 2);
    let back = AllowFile::load_from(&path).unwrap();
    let e = &back.entries[0];
    assert_eq!(e.repo(1), Some("/abs/one"));
    assert!(e.repo(127).is_none());
    assert_eq!(e.repo_set(127).unwrap().len(), 2);
    let mut paths = e.repo_paths();
    paths.sort();
    assert_eq!(paths, ["/abs/backend", "/abs/frontend", "/abs/one"]);
}

#[test]
fn a_set_with_bad_names_or_shared_paths_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runner-allow.json");
    for set in [
        serde_json::json!({}),
        serde_json::json!({"Tmgr-dev/backend": "/a"}),
        serde_json::json!({"tmgr-dev/backend": "/a", "tmgr-dev/frontend": "/a"}),
        serde_json::json!({"tmgr-dev/backend": "relative"}),
    ] {
        let mut e = serde_json::to_value(entry(None)).unwrap();
        e["repos"] = serde_json::json!({"127": set});
        std::fs::write(
            &path,
            serde_json::json!({"version": 2, "entries": [e]}).to_string(),
        )
        .unwrap();
        assert!(AllowFile::load_from(&path).is_err(), "{set}");
    }
}

#[test]
fn a_v2_claim_verifies_only_with_its_repository_set() {
    let f = fx();
    let c = &f["cases_v2"][0];
    let mut e = entry(None);
    e.workspace_id = 56;
    let mut claim = CloudApproval {
        host: "api.tmgr.dev".into(),
        user_id: 1,
        workspace_id: 56,
        task_id: 9100,
        category_id: 127,
        content_sha256: c["content_sha256"].as_str().unwrap().into(),
        key_id: f["key"]["key_id"].as_str().unwrap().into(),
        signed_at: 1760000100,
        signature: c["signature"].as_str().unwrap().into(),
        kind: "author".into(),
        repos_sha256: Some(c["repos_sha256"].as_str().unwrap().into()),
        persona_sha256: None,
        context_sha256: None,
    };
    let (t, d) = (
        c["title"].as_str().unwrap(),
        c["description"].as_str().unwrap(),
    );
    assert!(check_claim(&e, &claim, t, d).is_ok());
    claim.repos_sha256 = Some(tmgr_approval::repos_sha256(&["tmgr-dev/backend".into()]).unwrap());
    assert!(check_claim(&e, &claim, t, d).is_err());
    claim.repos_sha256 = Some("ZZ".into());
    assert!(check_claim(&e, &claim, t, d).is_err());
    claim.repos_sha256 = None;
    assert!(check_claim(&e, &claim, t, d).is_err());
}

#[test]
fn remove_reports_whether_anything_was_removed() {
    let mut f = AllowFile::default();
    f.upsert(entry(None));
    assert!(f.remove("api.tmgr.dev", 1, 56));
    assert!(!f.remove("api.tmgr.dev", 1, 56));
}

#[test]
fn rollback_keeps_the_highest_signed_at() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cloud-approvals.json");
    let k = Rollback::key("api.tmgr.dev", 1, 56, 9002);
    let mut r = Rollback::load_from(&path).unwrap();
    assert_eq!(r.seen(&k), None);
    r.record(k.clone(), 10, "h");
    r.record(k.clone(), 5, "h");
    r.save_to(&path).unwrap();
    assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(
        Rollback::load_from(&path).unwrap().seen(&k),
        Some((10, Some("h")))
    );
}

fn v3_claim(i: usize) -> (CloudApproval, String, String) {
    let f = fx();
    let c = &f["cases_v3"][i];
    let opt = |k: &str| c[k].as_str().map(String::from);
    (
        CloudApproval {
            host: "api.tmgr.dev".into(),
            user_id: 1,
            workspace_id: 56,
            task_id: c["task_id"].as_i64().unwrap(),
            category_id: c["category_id"].as_i64().unwrap(),
            content_sha256: c["content_sha256"].as_str().unwrap().into(),
            key_id: f["key"]["key_id"].as_str().unwrap().into(),
            signed_at: c["signed_at"].as_i64().unwrap(),
            signature: c["signature"].as_str().unwrap().into(),
            kind: "confirm".into(),
            repos_sha256: opt("repos_sha256"),
            persona_sha256: opt("persona_sha256"),
            context_sha256: opt("context_sha256"),
        },
        c["title"].as_str().unwrap().into(),
        c["description"].as_str().unwrap_or("").into(),
    )
}

#[test]
fn a_v3_claim_verifies_only_with_the_hashes_it_was_signed_with() {
    for i in 0..4 {
        let (c, t, b) = v3_claim(i);
        check_claim(&entry(None), &c, &t, &b).unwrap();
        for edit in [
            |c: &mut CloudApproval| c.persona_sha256 = None,
            |c: &mut CloudApproval| c.context_sha256 = None,
            |c: &mut CloudApproval| c.repos_sha256 = Some("0".repeat(64)),
        ] {
            let mut forged = c.clone();
            edit(&mut forged);
            if forged != c {
                assert!(check_claim(&entry(None), &forged, &t, &b).is_err(), "{i}");
            }
        }
    }
}

#[test]
fn a_v3_claim_with_a_changed_hash_or_a_non_hex_hash_is_refused() {
    let (c, t, b) = v3_claim(0);
    let mut other = c.clone();
    other.persona_sha256 = Some(c.context_sha256.clone().unwrap());
    assert!(check_claim(&entry(None), &other, &t, &b).is_err());
    for bad in ["ZZ", "", &"A".repeat(64)] {
        let mut x = c.clone();
        x.persona_sha256 = Some(bad.into());
        let r = check_claim(&entry(None), &x, &t, &b).unwrap_err();
        assert!(r.message.contains("unreadable"), "{r}");
        let mut x = c.clone();
        x.context_sha256 = Some(bad.into());
        assert!(check_claim(&entry(None), &x, &t, &b).is_err());
    }
}

#[test]
fn a_claim_without_persona_or_context_hashes_is_never_checked_as_v3() {
    let (mut c, t, b) = v3_claim(2);
    c.context_sha256 = None;
    assert!(check_claim(&entry(None), &c, &t, &b).is_err());
    let (v1, t, b) = claim();
    assert_eq!(Rollback::stamp(&v1), v1.content_sha256);
    check_claim(&entry(None), &v1, &t, &b).unwrap();
}

#[test]
fn the_stamp_covers_every_hash_and_stays_compatible_with_v2() {
    let (c, _, _) = v3_claim(3);
    let h = &c.content_sha256;
    assert_eq!(
        Rollback::stamp(&c),
        format!(
            "{h}:{}:{}:{}",
            c.repos_sha256.as_ref().unwrap(),
            c.persona_sha256.as_ref().unwrap(),
            c.context_sha256.as_ref().unwrap()
        )
    );
    let (c, _, _) = v3_claim(1);
    assert_eq!(
        Rollback::stamp(&c),
        format!(
            "{h_}:-:{}",
            c.persona_sha256.as_ref().unwrap(),
            h_ = c.content_sha256
        )
    );
    let mut v2 = c.clone();
    v2.persona_sha256 = None;
    v2.repos_sha256 = Some("r".into());
    assert_eq!(Rollback::stamp(&v2), format!("{}:r", v2.content_sha256));
}

fn with_policy(rules: bool, persona: bool) -> Entry {
    let mut e = entry(None);
    e.require_signed_rules = rules;
    e.require_signed_persona = persona;
    e
}

#[test]
fn the_policy_is_judged_on_the_signed_hashes_only() {
    let (v2, t2, b2) = claim();
    let accepted = |rules: bool, persona: bool, c: &CloudApproval, t: &str, b: &str| {
        check_claim(&with_policy(rules, persona), c, t, b).is_ok()
    };
    assert!(accepted(false, false, &v2, &t2, &b2));
    assert!(!accepted(true, false, &v2, &t2, &b2));
    assert!(!accepted(false, true, &v2, &t2, &b2));
    assert!(!accepted(true, true, &v2, &t2, &b2));
    // (persona, rules) covered by cases_v3: 0 both, 1 persona, 2 rules, 3 both
    for (i, persona, rules) in [
        (0, true, true),
        (1, true, false),
        (2, false, true),
        (3, true, true),
    ] {
        let (c, t, b) = v3_claim(i);
        assert!(accepted(false, false, &c, &t, &b), "{i}");
        assert_eq!(accepted(true, false, &c, &t, &b), rules, "{i} rules");
        assert_eq!(accepted(false, true, &c, &t, &b), persona, "{i} persona");
        assert_eq!(
            accepted(true, true, &c, &t, &b),
            rules && persona,
            "{i} both"
        );
    }
}

#[test]
fn a_policy_refusal_names_what_is_missing_and_how_to_fix_it() {
    let (c, t, b) = claim();
    let r = check_claim(&with_policy(true, false), &c, &t, &b).unwrap_err();
    assert!(r.message.contains("workspace rules"), "{}", r.message);
    assert!(r.message.contains("no rules"), "{}", r.message);
    assert!(r.fix.contains("Confirm for agent runs"), "{}", r.fix);
    let r = check_claim(&with_policy(false, true), &c, &t, &b).unwrap_err();
    assert!(r.message.contains("persona"), "{}", r.message);
}

#[test]
fn a_forged_coverage_cannot_satisfy_the_policy() {
    let (mut c, t, b) = claim();
    c.persona_sha256 = Some("a".repeat(64));
    c.context_sha256 = Some("b".repeat(64));
    let r = check_claim(&with_policy(true, true), &c, &t, &b).unwrap_err();
    assert!(
        r.message.contains("signature is not valid"),
        "{}",
        r.message
    );
}

#[test]
fn the_policy_round_trips_and_an_unset_policy_leaves_the_file_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runner-allow.json");
    let mut f = AllowFile::default();
    f.upsert(with_policy(true, false));
    f.save_to(&path).unwrap();
    let raw: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(raw["entries"][0]["require_signed_rules"], true);
    assert!(raw["entries"][0].get("require_signed_persona").is_none());
    let back = AllowFile::load_from(&path).unwrap();
    assert!(back.entries[0].require_signed_rules && !back.entries[0].require_signed_persona);
    let mut f = AllowFile::default();
    f.upsert(with_policy(false, true));
    f.save_to(&path).unwrap();
    assert!(AllowFile::load_from(&path).unwrap().entries[0].require_signed_persona);

    let mut f = AllowFile::default();
    f.upsert(entry(None));
    f.save_to(&path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("require_signed"), "{text}");
}

#[test]
fn an_old_file_without_the_fields_loads_with_the_policy_off() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runner-allow.json");
    let e = serde_json::json!({
        "server": "api.tmgr.dev", "user_id": 1, "workspace_id": 56, "workspace_code": "tmgrdev",
        "allowed_at": "2025-10-01T00:00:00Z", "expires_at": null, "keys": [], "repos": {}
    });
    std::fs::write(
        &path,
        serde_json::json!({"version": 1, "entries": [e]}).to_string(),
    )
    .unwrap();
    let f = AllowFile::load_from(&path).unwrap();
    assert!(!f.entries[0].require_signed_rules && !f.entries[0].require_signed_persona);
}
