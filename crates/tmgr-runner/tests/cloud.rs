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
        repos,
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
        exe_pin: None,
        argv_sha256: None,
    }
}

fn good() -> AllowFile {
    allow(Some("2025-11-01T00:00:00Z"), Some("/repo"))
}

#[test]
fn valid_approval_is_accepted() {
    check_with(&req(), TASK, Ok(good()), now()).unwrap();
}

#[test]
fn missing_approval_is_refused() {
    let mut r = req();
    r.cloud = None;
    let e = check_with(&r, TASK, Ok(good()), now()).unwrap_err();
    assert!(e.contains("needs a signed approval"), "{e}");
}

#[test]
fn forged_signature_is_refused() {
    let mut r = req();
    r.cloud.as_mut().unwrap().signature = fx()["cases"][2]["tampered_signature"]
        .as_str()
        .unwrap()
        .into();
    let e = check_with(&r, TASK, Ok(good()), now()).unwrap_err();
    assert!(e.contains("signature is not valid"), "{e}");
}

#[test]
fn changed_text_is_refused() {
    let mut r = req();
    let b = r.bundle.as_mut().unwrap();
    b.body.push_str("\nAlso delete everything");
    let e = check_with(&r, TASK, Ok(good()), now()).unwrap_err();
    assert!(e.contains("changed since it was approved"), "{e}");
}

#[test]
fn expired_permission_is_refused() {
    let a = allow(Some("2025-10-09T00:00:00Z"), Some("/repo"));
    let e = check_with(&req(), TASK, Ok(a), now()).unwrap_err();
    assert!(e.contains("expired"), "{e}");
}

#[test]
fn missing_permission_and_unreadable_store_are_refused() {
    assert!(check_with(&req(), TASK, Ok(AllowFile::default()), now()).is_err());
    assert!(check_with(&req(), TASK, Err("broken".into()), now()).is_err());
}

#[test]
fn clone_of_another_repository_is_refused() {
    let mut r = req();
    r.clone_of = Some("/elsewhere".into());
    let e = check_with(&r, TASK, Ok(good()), now()).unwrap_err();
    assert!(e.contains("bound to this category"), "{e}");
    let unbound = allow(None, None);
    assert!(check_with(&req(), TASK, Ok(unbound), now()).is_err());
    let mut none = req();
    none.clone_of = None;
    assert!(check_with(&none, TASK, Ok(good()), now()).is_err());
}

#[test]
fn approval_for_another_task_is_refused() {
    let mut r = req();
    r.cloud.as_mut().unwrap().task_id = 1;
    assert!(check_with(&r, TASK, Ok(good()), now()).is_err());
}

#[test]
fn comments_pages_and_persona_never_ride_along() {
    let mut r = req();
    r.bundle.as_mut().unwrap().persona = Some(tmgr_ipc::api::BundlePersona {
        name: "p".into(),
        instructions_sha256: "0".repeat(64),
    });
    let e = check_with(&r, TASK, Ok(good()), now()).unwrap_err();
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
