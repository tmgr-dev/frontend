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
    }
}

fn claim() -> (CloudApproval, String, String) {
    let f = fx();
    let c = &f["cases"][2];
    (
        CloudApproval {
            repos_sha256: None,
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
