#![cfg(unix)]
use chrono::{TimeZone, Utc};
use serde_json::Value;
use std::collections::BTreeMap;
use tmgr_approval::{AllowFile, Entry, PinnedKey};
use tmgr_ipc::api::{
    AuthorKind, BundleAuthor, BundleLaunch, CloudApproval, RunBundle, RunCreateReq, TaskRef,
};
use tmgr_runner::cloud::check_with;

const FIXTURES: &str = include_str!("../../tmgr-core/tests/fixtures/agent-approval-fixtures.json");
const TASK: TaskRef = TaskRef {
    workspace_id: 56,
    task_id: 9002,
};

fn fx() -> Value {
    serde_json::from_str(FIXTURES).unwrap()
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2025, 10, 10, 0, 0, 0).unwrap()
}

fn allow(expires: Option<&str>, repo: Option<&str>) -> AllowFile {
    let f = fx();
    let mut repos = BTreeMap::new();
    if let Some(r) = repo {
        repos.insert("127".to_string(), r.to_string());
    }
    let mut a = AllowFile::default();
    a.upsert(Entry {
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
        repos: repos
            .into_iter()
            .map(|(k, v)| (k, tmgr_approval::RepoBinding::Path(v)))
            .collect(),
    });
    a
}

fn req() -> RunCreateReq {
    let f = fx();
    let c = &f["cases"][2];
    let bundle = RunBundle {
        v: 1,
        task: TASK,
        title: c["title"].as_str().unwrap().into(),
        body: c["description"].as_str().unwrap().into(),
        author: BundleAuthor {
            kind: AuthorKind::Human,
            id: None,
        },
        comments: vec![],
        pages: vec![],
        persona: None,
        launch: BundleLaunch {
            profile: "claude-code".into(),
            model: None,
            cwd: "/tmp/clone".into(),
            permission_mode: Some("accept-edits".into()),
        },
    };
    RunCreateReq {
        clones: Vec::new(),
        profile: "claude-code".into(),
        task: Some(TASK),
        cwd: "/tmp/clone".into(),
        name: None,
        model: None,
        cols: 80,
        rows: 24,
        bundle_sha256: Some(bundle.sha256()),
        bundle: Some(bundle),
        cwd_pin: None,
        origin: None,
        permission_mode: Some("accept-edits".into()),
        start_key: None,
        clone_of: Some("/repo".into()),
        trust_cwd: None,
        trust_via_repo: None,
        agent_work: true,
        cloud: Some(CloudApproval {
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
        }),
        unattended: false,
        exe_pin: None,
        argv_sha256: None,
    }
}

fn good() -> AllowFile {
    allow(Some("2025-11-01T00:00:00Z"), Some("/repo"))
}

#[test]
fn valid_approval_is_accepted() {
    check_with(&req(), TASK, Ok(good()), now(), None).unwrap();
}

#[test]
fn an_older_or_conflicting_stamp_than_the_cli_saw_is_refused() {
    let key = tmgr_approval::Rollback::key("api.tmgr.dev", 1, 56, 9002);
    let seen = |at: i64, hash: &str| {
        let mut r = tmgr_approval::Rollback::default();
        r.record(key.clone(), at, hash);
        Some(r)
    };
    let c = req().cloud.unwrap();
    let ok = check_with(
        &req(),
        TASK,
        Ok(good()),
        now(),
        seen(c.signed_at, &c.content_sha256),
    );
    assert!(ok.is_ok(), "{ok:?}");
    for rb in [
        seen(c.signed_at + 1, &c.content_sha256),
        seen(c.signed_at, "other"),
    ] {
        let e = check_with(&req(), TASK, Ok(good()), now(), rb).unwrap_err();
        assert!(e.contains("older approved version"), "{e}");
    }
}

#[test]
fn missing_approval_is_refused() {
    let mut r = req();
    r.cloud = None;
    let e = check_with(&r, TASK, Ok(good()), now(), None).unwrap_err();
    assert!(e.contains("needs a signed approval"), "{e}");
}

#[test]
fn forged_signature_is_refused() {
    let mut r = req();
    r.cloud.as_mut().unwrap().signature = fx()["cases"][2]["tampered_signature"]
        .as_str()
        .unwrap()
        .into();
    let e = check_with(&r, TASK, Ok(good()), now(), None).unwrap_err();
    assert!(e.contains("signature is not valid"), "{e}");
}

#[test]
fn changed_text_is_refused() {
    let mut r = req();
    let b = r.bundle.as_mut().unwrap();
    b.body.push_str("\nAlso delete everything");
    let e = check_with(&r, TASK, Ok(good()), now(), None).unwrap_err();
    assert!(e.contains("changed since it was approved"), "{e}");
}

#[test]
fn expired_permission_is_refused() {
    let a = allow(Some("2025-10-09T00:00:00Z"), Some("/repo"));
    let e = check_with(&req(), TASK, Ok(a), now(), None).unwrap_err();
    assert!(e.contains("expired"), "{e}");
}

#[test]
fn missing_permission_and_unreadable_store_are_refused() {
    assert!(check_with(&req(), TASK, Ok(AllowFile::default()), now(), None).is_err());
    assert!(check_with(&req(), TASK, Err("broken".into()), now(), None).is_err());
}

#[test]
fn clone_of_another_repository_is_refused() {
    let mut r = req();
    r.clone_of = Some("/elsewhere".into());
    let e = check_with(&r, TASK, Ok(good()), now(), None).unwrap_err();
    assert!(e.contains("bound to this category"), "{e}");
    let unbound = allow(None, None);
    assert!(check_with(&req(), TASK, Ok(unbound), now(), None).is_err());
    let mut none = req();
    none.clone_of = None;
    assert!(check_with(&none, TASK, Ok(good()), now(), None).is_err());
}

#[test]
fn approval_for_another_task_is_refused() {
    let mut r = req();
    r.cloud.as_mut().unwrap().task_id = 1;
    assert!(check_with(&r, TASK, Ok(good()), now(), None).is_err());
}

#[test]
fn comments_pages_and_persona_never_ride_along() {
    let mut r = req();
    r.bundle.as_mut().unwrap().persona = Some(tmgr_ipc::api::BundlePersona {
        name: "p".into(),
        instructions_sha256: "0".repeat(64),
    });
    let e = check_with(&r, TASK, Ok(good()), now(), None).unwrap_err();
    assert!(e.contains("only the title and description"), "{e}");
}

#[test]
fn runner_dependency_closure_has_no_http_client() {
    let lock: toml::Table = include_str!("../../../Cargo.lock").parse().unwrap();
    let packages = lock["package"].as_array().unwrap();
    let deps_of = |name: &str| -> Vec<String> {
        packages
            .iter()
            .filter(|p| p["name"].as_str() == Some(name))
            .flat_map(|p| {
                p.get("dependencies")
                    .and_then(|d| d.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|d| d.as_str())
                    .map(|d| d.split(' ').next().unwrap().to_string())
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut todo = vec!["tmgr-runner".to_string()];
    while let Some(n) = todo.pop() {
        if seen.insert(n.clone()) {
            todo.extend(deps_of(&n));
        }
    }
    assert!(seen.contains("tmgr-approval") && seen.contains("p256"));
    for banned in [
        "reqwest",
        "hyper",
        "ureq",
        "isahc",
        "attohttpc",
        "tmgr-core",
    ] {
        assert!(
            !seen.contains(banned),
            "{banned} is reachable from tmgr-runner"
        );
    }
}

const TASK_V2: TaskRef = TaskRef {
    workspace_id: 56,
    task_id: 9100,
};

fn set_entry(set: &[(&str, &str)]) -> AllowFile {
    let mut a = allow(Some("2025-11-01T00:00:00Z"), None);
    let mut entry = a.find("api.tmgr.dev", 1, 56).unwrap().clone();
    entry.repos.insert(
        "127".into(),
        tmgr_approval::RepoBinding::Set(
            set.iter()
                .map(|(n, p)| (n.to_string(), p.to_string()))
                .collect(),
        ),
    );
    a.upsert(entry);
    a
}

fn two() -> AllowFile {
    set_entry(&[
        ("tmgr-dev/backend", "/code/backend"),
        ("tmgr-dev/frontend", "/code/frontend"),
    ])
}

fn clone_spec(name: &str, of: &str) -> tmgr_ipc::api::CloneSpec {
    tmgr_ipc::api::CloneSpec {
        name: name.into(),
        clone_of: of.into(),
    }
}

fn req_v2() -> RunCreateReq {
    let f = fx();
    let c = &f["cases_v2"][0];
    let mut r = req();
    let b = r.bundle.as_mut().unwrap();
    b.task = TASK_V2;
    b.title = c["title"].as_str().unwrap().into();
    b.body = c["description"].as_str().unwrap().into();
    r.bundle_sha256 = Some(b.sha256());
    r.task = Some(TASK_V2);
    r.clone_of = None;
    r.clones = vec![
        clone_spec("backend", "/code/backend"),
        clone_spec("frontend", "/code/frontend"),
    ];
    r.cloud = Some(CloudApproval {
        repos_sha256: Some(c["repos_sha256"].as_str().unwrap().into()),
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
    });
    r
}

fn v2_err(r: &RunCreateReq, a: AllowFile) -> String {
    check_with(r, TASK_V2, Ok(a), now(), None).unwrap_err()
}

#[test]
fn a_signed_set_with_its_clones_is_accepted() {
    check_with(&req_v2(), TASK_V2, Ok(two()), now(), None).unwrap();
    let primary = tmgr_runner::cloud::clone_names(&[
        "tmgr-dev/backend".to_string(),
        "tmgr-dev/frontend".to_string(),
    ])
    .unwrap();
    assert_eq!(primary, ["backend", "frontend"]);
}

#[test]
fn a_set_run_with_another_set_of_clones_is_refused() {
    let mut added = req_v2();
    added.clones.push(clone_spec("extra", "/code/extra"));
    let mut dropped = req_v2();
    dropped.clones.pop();
    let mut swapped = req_v2();
    swapped.clones[0].clone_of = "/code/frontend".into();
    swapped.clones[1].clone_of = "/code/backend".into();
    let mut renamed = req_v2();
    renamed.clones[1].name = "web".into();
    let mut twice = req_v2();
    twice.clones[1] = clone_spec("backend", "/code/frontend");
    let mut doubled = req_v2();
    doubled.clones[1] = clone_spec("frontend", "/code/backend");
    let mut elsewhere = req_v2();
    elsewhere.clones[1].clone_of = "/code/other".into();
    for (what, r) in [
        ("added", added),
        ("dropped", dropped),
        ("swapped", swapped),
        ("renamed", renamed),
        ("duplicate names", twice),
        ("duplicate paths", doubled),
        ("another path", elsewhere),
    ] {
        let e = v2_err(&r, two());
        assert!(e.contains("bound to this category"), "{what}: {e}");
    }
}

#[test]
fn a_set_claim_needs_clones_and_a_set_binding() {
    let mut none = req_v2();
    none.clones.clear();
    assert!(v2_err(&none, two()).contains("needs its clones"));

    let mut with_of = req_v2();
    with_of.clone_of = Some("/code/backend".into());
    assert!(v2_err(&with_of, two()).contains("clone_of"));

    let path_binding = allow(Some("2025-11-01T00:00:00Z"), Some("/code/backend"));
    assert!(v2_err(&req_v2(), path_binding).contains("no repository set"));
    assert!(
        v2_err(&req_v2(), allow(Some("2025-11-01T00:00:00Z"), None)).contains("no repository set")
    );

    let one_more = set_entry(&[
        ("tmgr-dev/backend", "/code/backend"),
        ("tmgr-dev/frontend", "/code/frontend"),
        ("tmgr-dev/docs", "/code/docs"),
    ]);
    assert!(v2_err(&req_v2(), one_more).contains("another set"));
    let fewer = set_entry(&[("tmgr-dev/backend", "/code/backend")]);
    assert!(v2_err(&req_v2(), fewer).contains("another set"));
}

#[test]
fn a_plain_claim_with_clones_is_refused() {
    let mut r = req();
    r.clone_of = None;
    r.clones = vec![clone_spec("repo", "/repo")];
    let e = check_with(&r, TASK, Ok(good()), now(), None).unwrap_err();
    assert!(
        e.contains("clones need an approval of a repository set"),
        "{e}"
    );
    let mut both = req();
    both.clones = vec![clone_spec("repo", "/repo")];
    assert!(check_with(&both, TASK, Ok(good()), now(), None).is_err());
}

#[test]
fn a_plain_claim_on_a_set_binding_is_refused() {
    let a = set_entry(&[("tmgr-dev/repo", "/repo")]);
    assert!(check_with(&req(), TASK, Ok(a), now(), None).is_err());
}

#[test]
fn a_forged_or_edited_set_claim_is_refused() {
    let mut forged = req_v2();
    forged.cloud.as_mut().unwrap().signature = fx()["cases_v2"][0]["tampered_signature"]
        .as_str()
        .unwrap()
        .into();
    assert!(v2_err(&forged, two()).contains("signature is not valid"));
    let mut other_hash = req_v2();
    other_hash.cloud.as_mut().unwrap().repos_sha256 = Some("0".repeat(64));
    assert!(
        v2_err(&other_hash, two()).contains("another set")
            || v2_err(&other_hash, two()).contains("signature")
    );
}

#[test]
fn clone_names_use_the_repo_part_and_the_owner_when_it_collides() {
    let names = |v: &[&str]| {
        tmgr_runner::cloud::clone_names(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    };
    assert_eq!(names(&["a/web", "b/api"]).unwrap(), ["web", "api"]);
    assert_eq!(
        names(&["a/web", "b/web", "c/api"]).unwrap(),
        ["a-web", "b-web", "api"]
    );
    assert!(names(&["a/web", "b/web", "x/a-web"]).is_err());
    assert!(names(&["a/_hidden"]).is_err());
    assert!(names(&["a/.git"]).is_err());
}

#[test]
fn the_rollback_guard_compares_the_set_stamp() {
    let key = tmgr_approval::Rollback::key("api.tmgr.dev", 1, 56, 9100);
    let c = req_v2().cloud.unwrap();
    let seen = |stamp: &str| {
        let mut r = tmgr_approval::Rollback::default();
        r.record(key.clone(), c.signed_at, stamp);
        Some(r)
    };
    let ok = check_with(
        &req_v2(),
        TASK_V2,
        Ok(two()),
        now(),
        seen(&tmgr_approval::Rollback::stamp(&c)),
    );
    assert!(ok.is_ok(), "{ok:?}");
    for stale in [c.content_sha256.as_str(), "other"] {
        let e = check_with(&req_v2(), TASK_V2, Ok(two()), now(), seen(stale)).unwrap_err();
        assert!(e.contains("older approved version"), "{e}");
    }
}
