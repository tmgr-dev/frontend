mod common;
use common::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

fn summary(id: u64, title: &str, parent: Option<u64>, updated: &str) -> Value {
    json!({"id": id, "title": title, "slug": "ignored/../slug", "type": "plain",
           "parent_id": parent, "position": 0, "pinned": false, "updated_at": updated})
}

fn page(id: u64, title: &str, body: &str, version: i64, updated: &str) -> Value {
    json!({"id": id, "title": title, "slug": "x", "type": "plain", "parent_id": null,
           "updated_at": updated, "workspace_id": 1, "body": body, "version": version})
}

struct Remote {
    pages: Vec<(Value, Value)>,
}

async fn mount(env: &Env, remote: &Remote) {
    env.server.reset().await;
    env.mock_workspaces().await;
    Mock::given(method("GET"))
        .and(path("/api/pages/tree"))
        .and(header("x-workspace-id", "1"))
        .and(header("authorization", format!("Bearer {ACCESS}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": remote.pages.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>()
        })))
        .mount(&env.server)
        .await;
    for (s, full) in &remote.pages {
        let id = s["id"].as_u64().unwrap();
        Mock::given(method("GET"))
            .and(path(format!("/api/pages/{id}")))
            .and(header("x-workspace-id", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": full})))
            .mount(&env.server)
            .await;
    }
}

fn remote_v1() -> Remote {
    Remote {
        pages: vec![
            (
                summary(1, "Deploy checklist", None, "2026-10-12T10:00:00Z"),
                page(
                    1,
                    "Deploy checklist",
                    "# Deploy\n\n- [x] migrations\n",
                    3,
                    "2026-10-12T10:00:00Z",
                ),
            ),
            (
                summary(2, "Notes", Some(1), "2026-10-12T10:01:00Z"),
                page(2, "Notes", "child notes\n", 1, "2026-10-12T10:01:00Z"),
            ),
            (
                summary(3, "Notes", None, "2026-10-12T10:02:00Z"),
                page(3, "Notes", "other notes\n", 1, "2026-10-12T10:02:00Z"),
            ),
            (
                summary(
                    4,
                    "../../etc/passwd\u{1b}[31m",
                    None,
                    "2026-10-12T10:03:00Z",
                ),
                page(
                    4,
                    "../../etc/passwd\u{1b}[31m",
                    "evil title\n",
                    1,
                    "2026-10-12T10:03:00Z",
                ),
            ),
        ],
    }
}

fn pages_dir(env: &Env) -> PathBuf {
    env.tmp.path().join("work").join("pages")
}

fn entries(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    v.sort();
    v
}

async fn pulled() -> (Env, PathBuf) {
    let env = Env::new().await;
    env.login_seed();
    mount(&env, &remote_v1()).await;
    let dir = pages_dir(&env);
    let out = env
        .cmd()
        .args(["page", "pull", dir.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    (env, dir)
}

async fn patch_requests(env: &Env, id: u64) -> Vec<wiremock::Request> {
    env.requests_to(&format!("/api/pages/{id}"))
        .await
        .into_iter()
        .filter(|r| r.method.as_str() == "PATCH")
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn pull_writes_strict_slugs_front_matter_and_an_index_inside_the_directory() {
    let env = Env::new().await;
    env.login_seed();
    mount(&env, &remote_v1()).await;
    let dir = pages_dir(&env);
    let out = env
        .cmd()
        .args(["page", "pull", dir.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("page_sync", &v);
    assert_eq!(v["data"]["counts"]["new"], 4);
    assert_eq!(
        entries(&dir),
        vec![
            ".tmgr-pages.json",
            "deploy-checklist.md",
            "etc-passwd-31m.md",
            "notes-3.md",
            "notes.md"
        ]
    );
    assert_eq!(entries(dir.parent().unwrap()), vec!["pages"]);
    let file = std::fs::read_to_string(dir.join("deploy-checklist.md")).unwrap();
    assert_eq!(
        file,
        "---\ntmgr:\n  page_id: 1\n  version: 3\n  updated_at: 2026-10-12T10:00:00Z\n  workspace: personal\n---\n# Deploy\n\n- [x] migrations\n"
    );
    let titles: Vec<&str> = v["data"]["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["title"].as_str().unwrap())
        .collect();
    assert!(titles.iter().all(|t| !t.contains('\u{1b}')), "{titles:?}");
    assert!(!text(&out.stdout).contains('\u{1b}'));
    assert_no_secrets(&out);
}

#[tokio::test(flavor = "multi_thread")]
async fn pull_human_output_lists_files_and_a_summary_without_escape_sequences() {
    let env = Env::new().await;
    env.login_seed();
    mount(&env, &remote_v1()).await;
    let dir = pages_dir(&env);
    let out = env
        .cmd()
        .args(["page", "pull", dir.to_str().unwrap()])
        .output()
        .unwrap();
    let s = text(&out.stdout);
    assert!(s.contains("\u{2193} deploy-checklist.md  new"), "{s}");
    assert!(s.contains("4 new"), "{s}");
    assert!(!s.contains('\u{1b}'), "{s}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_pull_downloads_nothing_when_the_server_did_not_change() {
    let (env, dir) = pulled().await;
    mount(&env, &remote_v1()).await;
    let out = env
        .cmd()
        .args(["page", "pull", dir.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_eq!(v["data"]["counts"]["unchanged"], 4);
    for id in 1..=4 {
        assert!(env
            .requests_to(&format!("/api/pages/{id}"))
            .await
            .is_empty());
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn pull_tree_limits_the_selection_to_a_page_and_its_children() {
    let env = Env::new().await;
    env.login_seed();
    mount(&env, &remote_v1()).await;
    let dir = pages_dir(&env);
    let out = env
        .cmd()
        .args([
            "page",
            "pull",
            dir.to_str().unwrap(),
            "--tree",
            "1",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        entries(&dir),
        vec![".tmgr-pages.json", "deploy-checklist.md", "notes.md"]
    );
    let out = env
        .cmd()
        .args(["page", "pull", dir.to_str().unwrap(), "--tree", "99"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
}

#[tokio::test(flavor = "multi_thread")]
async fn push_sends_only_changed_files_with_the_version_and_refreshes_the_front_matter() {
    let (env, dir) = pulled().await;
    let f = dir.join("deploy-checklist.md");
    let edited = std::fs::read_to_string(&f).unwrap().replace("[x]", "[ ]");
    std::fs::write(&f, edited).unwrap();
    mount(&env, &remote_v1()).await;
    Mock::given(method("PATCH"))
        .and(path("/api/pages/1"))
        .and(header("x-workspace-id", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": page(1, "Deploy checklist", "# Deploy\n\n- [ ] migrations\n", 4, "2026-10-12T11:00:00Z")
        })))
        .mount(&env.server)
        .await;
    let out = env
        .cmd()
        .args(["page", "push", "--json"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("page_sync", &v);
    assert_eq!(v["data"]["counts"]["pushed"], 1);
    assert_eq!(v["data"]["counts"]["unchanged"], 3);
    assert_eq!(patch_requests(&env, 1).await.len(), 1);
    for id in 2..=4 {
        assert!(patch_requests(&env, id).await.is_empty());
    }
    let sent: Value = serde_json::from_slice(&patch_requests(&env, 1).await[0].body).unwrap();
    assert_eq!(sent["version"], 3);
    assert_eq!(sent["body"], "# Deploy\n\n- [ ] migrations\n");
    assert!(sent.get("title").is_none());
    let after = std::fs::read_to_string(&f).unwrap();
    assert!(
        after.contains("  version: 4\n") && after.contains("2026-10-12T11:00:00Z"),
        "{after}"
    );
    assert!(after.ends_with("- [ ] migrations\n"));

    let st = env
        .cmd()
        .args(["page", "status", "--json"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let sv = stdout_json(&st);
    assert_valid("page_status", &sv);
    let states: Vec<&str> = sv["data"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["state"].as_str().unwrap())
        .collect();
    assert!(states.contains(&"remote_changed"), "{states:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn push_conflict_exits_6_keeps_the_local_file_and_writes_the_remote_copy() {
    let (env, dir) = pulled().await;
    let f = dir.join("notes.md");
    let edited = std::fs::read_to_string(&f).unwrap() + "my edit\n";
    std::fs::write(&f, &edited).unwrap();
    mount(&env, &remote_v1()).await;
    Mock::given(method("PATCH"))
        .and(path("/api/pages/2"))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({
            "error": "page_conflict",
            "data": page(2, "Notes", "someone else wrote this\n", 2, "2026-10-12T12:00:00Z")
        })))
        .mount(&env.server)
        .await;
    let out = env
        .cmd()
        .args(["page", "push", f.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(6), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_eq!(v["error"]["code"], "conflict");
    assert!(v["error"]["message"].as_str().unwrap().contains("notes.md"));
    assert_eq!(std::fs::read_to_string(&f).unwrap(), edited);
    let remote = std::fs::read_to_string(dir.join("notes.remote.md")).unwrap();
    assert!(
        remote.contains("  version: 2\n") && remote.ends_with("someone else wrote this\n"),
        "{remote}"
    );
    let human = env
        .cmd()
        .args(["page", "push", f.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(human.status.code(), Some(6));
    assert!(text(&human.stdout).contains("! notes.md  changed on both sides"));
    assert!(text(&human.stderr).contains("notes.remote.md"));
}

#[tokio::test(flavor = "multi_thread")]
async fn pull_refuses_to_overwrite_local_edits_unless_forced() {
    let (env, dir) = pulled().await;
    let f = dir.join("notes.md");
    let edited = std::fs::read_to_string(&f).unwrap() + "local line\n";
    std::fs::write(&f, &edited).unwrap();
    let mut remote = remote_v1();
    remote.pages[1] = (
        summary(2, "Notes", Some(1), "2026-10-12T13:00:00Z"),
        page(2, "Notes", "new remote text\n", 2, "2026-10-12T13:00:00Z"),
    );
    mount(&env, &remote).await;
    let out = env
        .cmd()
        .args(["page", "pull", dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(6), "{}", text(&out.stderr));
    assert_eq!(std::fs::read_to_string(&f).unwrap(), edited);
    assert!(text(&out.stdout).contains("! notes.md  changed on both sides"));

    let out = env
        .cmd()
        .args(["page", "pull", dir.to_str().unwrap(), "--force", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(std::fs::read_to_string(&f)
        .unwrap()
        .ends_with("new remote text\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn local_edits_survive_a_pull_when_the_server_did_not_change() {
    let (env, dir) = pulled().await;
    let f = dir.join("notes.md");
    let edited = std::fs::read_to_string(&f).unwrap() + "local line\n";
    std::fs::write(&f, &edited).unwrap();
    mount(&env, &remote_v1()).await;
    let out = env
        .cmd()
        .args(["page", "pull", dir.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(stdout_json(&out)["data"]["counts"]["local_changes"], 1);
    assert_eq!(std::fs::read_to_string(&f).unwrap(), edited);
}

#[tokio::test(flavor = "multi_thread")]
async fn symlinks_are_never_followed_on_pull_or_push() {
    let env = Env::new().await;
    env.login_seed();
    mount(&env, &remote_v1()).await;
    let dir = pages_dir(&env);
    std::fs::create_dir_all(&dir).unwrap();
    let outside = env.tmp.path().join("outside.txt");
    std::fs::write(&outside, "precious").unwrap();
    std::os::unix::fs::symlink(&outside, dir.join("deploy-checklist.md")).unwrap();
    let out = env
        .cmd()
        .args(["page", "pull", dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "precious");
    assert!(std::fs::symlink_metadata(dir.join("deploy-checklist.md"))
        .unwrap()
        .is_symlink());
    assert!(dir.join("deploy-checklist-1.md").is_file());

    let f = dir.join("notes.md");
    std::fs::remove_file(&f).unwrap();
    std::os::unix::fs::symlink(&outside, &f).unwrap();
    let out = env
        .cmd()
        .args(["page", "push", f.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("symbolic link"));
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "precious");
    let out = env
        .cmd()
        .args(["page", "pull", dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "precious");
}

#[tokio::test(flavor = "multi_thread")]
async fn push_refuses_files_that_were_not_pulled_and_tampered_front_matter() {
    let (env, dir) = pulled().await;
    let stray = env.tmp.path().join("stray.md");
    std::fs::write(
        &stray,
        "---\ntmgr:\n  page_id: 1\n  version: 1\n  workspace: personal\n---\nx\n",
    )
    .unwrap();
    let out = env
        .cmd()
        .args(["page", "push", stray.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));

    let rem = dir.join("notes.remote.md");
    std::fs::write(&rem, "x").unwrap();
    let out = env
        .cmd()
        .args(["page", "push", rem.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));

    let f = dir.join("notes.md");
    let tampered = std::fs::read_to_string(&f)
        .unwrap()
        .replace("page_id: 2", "page_id: 3")
        + "x\n";
    std::fs::write(&f, tampered).unwrap();
    let out = env
        .cmd()
        .args(["page", "push", f.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    for id in 1..=4 {
        assert!(patch_requests(&env, id).await.is_empty());
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_damaged_or_hostile_index_is_rejected() {
    let env = Env::new().await;
    env.login_seed();
    mount(&env, &remote_v1()).await;
    let dir = pages_dir(&env);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(".tmgr-pages.json"),
        json!({"schema": "tmgr.pages.v1", "workspace": "personal", "pages": {
            "../evil.md": {"page_id": 1, "version": 1, "body_sha256": "x"}
        }})
        .to_string(),
    )
    .unwrap();
    for args in [
        vec!["page", "pull", dir.to_str().unwrap()],
        vec!["page", "status", dir.to_str().unwrap()],
        vec!["page", "push", dir.to_str().unwrap()],
    ] {
        let out = env.cmd().args(&args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?} {}", text(&out.stderr));
    }
    assert!(!env.tmp.path().join("work").join("evil.md").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn pulling_into_a_directory_of_another_workspace_is_refused() {
    let (env, dir) = pulled().await;
    let out = env
        .cmd()
        .args(["page", "pull", dir.to_str().unwrap(), "--ws", "work"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
}

#[tokio::test(flavor = "multi_thread")]
async fn status_works_offline_and_reports_local_changes() {
    let (env, dir) = pulled().await;
    let f = dir.join("notes.md");
    std::fs::write(&f, std::fs::read_to_string(&f).unwrap() + "more\n").unwrap();
    std::fs::remove_file(dir.join("notes-3.md")).unwrap();
    let offline = Env::new().await;
    let dead = dead_server();
    offline.seed_account(
        &format!("{}:default", dead.trim_start_matches("http://")),
        ACCESS,
        None,
    );
    let mut c = offline.cmd_for(&dead);
    let out = c
        .args(["page", "status", "--json"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("page_status", &v);
    let state = |file: &str| {
        v["data"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["file"] == file)
            .unwrap()["state"]
            .clone()
    };
    assert_eq!(state("notes.md"), "modified");
    assert_eq!(state("notes-3.md"), "missing");
    assert_eq!(state("deploy-checklist.md"), "unchanged");
    assert_eq!(v["warnings"][0]["code"], "remote_unavailable");
    assert!(v["warnings"][0]["message"]
        .as_str()
        .unwrap()
        .contains("cannot reach"));
}
