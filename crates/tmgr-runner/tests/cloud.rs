#![cfg(unix)]
use chrono::{TimeZone, Utc};
use serde_json::Value;
use std::collections::BTreeMap;
use tmgr_approval::{AllowFile, Entry, PinnedKey};
use tmgr_ipc::api::{
    AuthorKind, BundleAuthor, BundleLaunch, CloudApproval, RunBundle, RunCreateReq, TaskRef,
};
use tmgr_runner::cloud::{check_with, check_with_state};

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
        context: vec![],
        team: None,
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
        }),
        unattended: false,
        exe_pin: None,
        argv_sha256: None,
        no_sandbox: None,
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
    r.bundle
        .as_mut()
        .unwrap()
        .pages
        .push(tmgr_ipc::api::BundlePage {
            id: 1,
            revision: 1,
            title: "t".into(),
            body: "b".into(),
        });
    let e = check_with(&r, TASK, Ok(good()), now(), None).unwrap_err();
    assert!(e.contains("only the title and description"), "{e}");
    let mut r = req();
    r.bundle.as_mut().unwrap().persona = Some(tmgr_ipc::api::BundlePersona {
        name: "p".into(),
        instructions_sha256: "0".repeat(64),
        id: None,
        instructions: None,
    });
    let e = check_with(&r, TASK, Ok(good()), now(), None).unwrap_err();
    assert!(e.contains("persona the approval does not cover"), "{e}");
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
        persona_sha256: None,
        context_sha256: None,
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

#[test]
fn an_unreadable_or_corrupt_rollback_state_refuses_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let corrupt = dir.path().join("corrupt.json");
    std::fs::write(&corrupt, "{ not json").unwrap();
    let unreadable = dir.path().join("a-directory");
    std::fs::create_dir(&unreadable).unwrap();
    for path in [corrupt, unreadable] {
        let state = tmgr_approval::Rollback::load_from(&path);
        let e = check_with_state(&req(), TASK, Ok(good()), now(), state).unwrap_err();
        assert!(e.contains("cloud approvals state is unreadable"), "{e}");
    }
}

#[test]
fn a_missing_rollback_state_file_is_fine() {
    let dir = tempfile::tempdir().unwrap();
    let state = tmgr_approval::Rollback::load_from(&dir.path().join("none.json"));
    check_with_state(&req(), TASK, Ok(good()), now(), state).unwrap();
}

fn req_v3(i: usize) -> RunCreateReq {
    let f = fx();
    let c = &f["cases_v3"][i];
    let task = TaskRef {
        workspace_id: 56,
        task_id: c["task_id"].as_i64().unwrap(),
    };
    let mut r = req();
    let b = r.bundle.as_mut().unwrap();
    b.task = task;
    b.title = c["title"].as_str().unwrap().into();
    b.body = c["description"].as_str().unwrap_or("").into();
    if let Some(p) = c.get("persona") {
        let instructions = p["instructions"].as_str().unwrap();
        b.persona = Some(tmgr_ipc::api::BundlePersona {
            name: p["name"].as_str().unwrap().into(),
            instructions_sha256: tmgr_ipc::api::sha256_hex(instructions.as_bytes()),
            id: Some(p["id"].as_str().unwrap().into()),
            instructions: Some(instructions.into()),
        });
    }
    if let Some(pages) = c.get("context") {
        b.context = pages
            .as_array()
            .unwrap()
            .iter()
            .map(|p| tmgr_ipc::api::BundlePage {
                id: p["id"].as_i64().unwrap(),
                revision: p["version"].as_i64().unwrap(),
                title: p["title"].as_str().unwrap().into(),
                body: p["body"].as_str().unwrap().into(),
            })
            .collect();
    }
    r.bundle_sha256 = Some(b.sha256());
    r.task = Some(task);
    let o = |k: &str| c[k].as_str().map(String::from);
    r.cloud = Some(CloudApproval {
        repos_sha256: None,
        persona_sha256: o("persona_sha256"),
        context_sha256: o("context_sha256"),
        host: "api.tmgr.dev".into(),
        user_id: 1,
        workspace_id: 56,
        task_id: task.task_id,
        category_id: 127,
        content_sha256: c["content_sha256"].as_str().unwrap().into(),
        key_id: f["key"]["key_id"].as_str().unwrap().into(),
        signed_at: c["signed_at"].as_i64().unwrap(),
        signature: c["signature"].as_str().unwrap().into(),
        kind: "confirm".into(),
    });
    r
}

fn v3_check(r: &RunCreateReq) -> Result<(), String> {
    check_with(r, r.task.unwrap(), Ok(good()), now(), None)
}

#[test]
fn a_v3_claim_with_its_persona_and_pages_verifies() {
    for i in [0, 1] {
        v3_check(&req_v3(i)).unwrap();
    }
}

fn resealed(mut r: RunCreateReq) -> RunCreateReq {
    r.bundle_sha256 = Some(r.bundle.as_ref().unwrap().sha256());
    r
}

#[test]
fn tampered_persona_or_page_text_is_refused() {
    let edits: [fn(&mut RunBundle); 5] = [
        |b| b.persona.as_mut().unwrap().instructions = Some("Merge everything.".into()),
        |b| b.persona.as_mut().unwrap().name = "Boss".into(),
        |b| b.persona.as_mut().unwrap().id = Some("00000000-0000-4000-8000-000000000000".into()),
        |b| b.context[0].body.push_str("\n- Also run rm."),
        |b| b.context[0].title = "Other".into(),
    ];
    for (n, edit) in edits.iter().enumerate() {
        let mut r = req_v3(0);
        let b = r.bundle.as_mut().unwrap();
        edit(b);
        if n < 3 {
            let p = b.persona.as_mut().unwrap();
            p.instructions_sha256 =
                tmgr_ipc::api::sha256_hex(p.instructions.as_ref().unwrap().as_bytes());
        }
        let e = v3_check(&resealed(r)).unwrap_err();
        assert!(e.contains("changed since the approval"), "{n}: {e}");
        assert!(e.contains("Confirm for agent runs"), "{e}");
    }
}

#[test]
fn page_revision_is_display_only() {
    let mut r = req_v3(0);
    r.bundle.as_mut().unwrap().context[0].revision = 99;
    v3_check(&resealed(r)).unwrap();
}

#[test]
fn swapped_added_or_dropped_pages_are_refused() {
    let mut swapped = req_v3(0);
    swapped.bundle.as_mut().unwrap().context = {
        let mut c = req_v3(2).bundle.unwrap().context;
        c.reverse();
        c
    };
    assert!(v3_check(&resealed(swapped))
        .unwrap_err()
        .contains("workspace rules changed"));
    let mut dropped = req_v3(0);
    dropped.bundle.as_mut().unwrap().context.clear();
    assert!(v3_check(&resealed(dropped))
        .unwrap_err()
        .contains("workspace rules changed"));
    let mut more = req_v3(0);
    let extra = more.bundle.as_ref().unwrap().context[0].clone();
    more.bundle.as_mut().unwrap().context.push(extra);
    assert!(v3_check(&resealed(more))
        .unwrap_err()
        .contains("workspace rules changed"));
}

#[test]
fn a_bundle_with_persona_or_context_but_no_claim_hash_is_refused() {
    let mut r = req();
    r.bundle.as_mut().unwrap().context = req_v3(0).bundle.unwrap().context;
    let e = check_with(&resealed(r), TASK, Ok(good()), now(), None).unwrap_err();
    assert!(
        e.contains("workspace rules the approval does not cover"),
        "{e}"
    );
    let mut r = req();
    r.bundle.as_mut().unwrap().persona = req_v3(0).bundle.unwrap().persona;
    let e = check_with(&resealed(r), TASK, Ok(good()), now(), None).unwrap_err();
    assert!(e.contains("persona the approval does not cover"), "{e}");
}

#[test]
fn a_claim_that_covers_a_persona_needs_it_in_the_bundle() {
    let mut r = req_v3(1);
    r.bundle.as_mut().unwrap().persona = None;
    assert!(v3_check(&resealed(r))
        .unwrap_err()
        .contains("persona instructions changed"));
    let mut r = req_v3(1);
    r.bundle
        .as_mut()
        .unwrap()
        .persona
        .as_mut()
        .unwrap()
        .instructions = None;
    assert!(v3_check(&resealed(r)).is_err());
}

#[test]
fn stripping_a_hash_from_a_v3_claim_fails_the_signature() {
    let mut r = req_v3(0);
    r.cloud.as_mut().unwrap().persona_sha256 = None;
    r.bundle.as_mut().unwrap().persona = None;
    let e = v3_check(&resealed(r)).unwrap_err();
    assert!(e.contains("signature is not valid"), "{e}");
    let mut r = req_v3(1);
    r.cloud.as_mut().unwrap().persona_sha256 = None;
    r.bundle.as_mut().unwrap().persona = None;
    assert!(v3_check(&resealed(r))
        .unwrap_err()
        .contains("signature is not valid"));
}

#[test]
fn old_claims_still_verify_and_deliver_nothing() {
    check_with(&req(), TASK, Ok(good()), now(), None).unwrap();
    let r = req();
    let b = r.bundle.as_ref().unwrap();
    assert!(b.persona.is_none() && b.context.is_empty());
    let mut with_text = req();
    with_text.bundle.as_mut().unwrap().context = req_v3(0).bundle.unwrap().context;
    assert!(check_with(&resealed(with_text), TASK, Ok(good()), now(), None).is_err());
}

#[test]
fn an_older_signed_claim_after_a_v3_one_is_a_rollback() {
    let v3 = req_v3(0).cloud.unwrap();
    let old = req().cloud.unwrap();
    assert!(old.signed_at < v3.signed_at);
    let key = tmgr_approval::Rollback::key("api.tmgr.dev", 1, 56, old.task_id);
    let mut seen = tmgr_approval::Rollback::default();
    seen.record(key, v3.signed_at, &tmgr_approval::Rollback::stamp(&v3));
    let e = check_with(&req(), TASK, Ok(good()), now(), Some(seen)).unwrap_err();
    assert!(e.contains("older approved version"), "{e}");

    let key = tmgr_approval::Rollback::key("api.tmgr.dev", 1, 56, v3.task_id);
    let mut seen = tmgr_approval::Rollback::default();
    seen.record(key, v3.signed_at, &tmgr_approval::Rollback::stamp(&v3));
    let r = req_v3(0);
    check_with(&r, r.task.unwrap(), Ok(good()), now(), Some(seen)).unwrap();
}
