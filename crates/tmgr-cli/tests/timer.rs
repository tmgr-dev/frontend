mod common;
use common::*;
use serde_json::{json, Value};
use std::path::Path;

const STARTED: i64 = 1_791_795_600;

fn task(status_id: u64, start_time: i64) -> Value {
    let mut t = task_json(8974, "TM-512: Checkout: Apple Pay button");
    t["status_id"] = json!(status_id);
    t["start_time"] = json!(start_time);
    t
}

async fn mock_flow(env: &Env, status_id: u64, start_time: i64) {
    env.mock_workspaces().await;
    env.mock_json(
        "/api/workspaces/1/statuses",
        json!([
            {"id": 10, "name": "Backlog", "type": "default"},
            {"id": 11, "name": "In progress", "type": "active"},
            {"id": 12, "name": "Done", "type": "completed"},
            {"id": 13, "name": "Review", "type": "active"}
        ]),
    )
    .await;
    env.mock_json(
        "/api/tasks/by-key/TM-512",
        json!({"data": task(status_id, start_time)}),
    )
    .await;
    env.mock_any(
        "POST",
        "/api/tasks/8974/countdown",
        json!({"data": task(status_id, STARTED)}),
    )
    .await;
    env.mock_any(
        "DELETE",
        "/api/tasks/8974/countdown",
        json!({"data": task(status_id, 0)}),
    )
    .await;
    for s in [11, 12, 13] {
        env.mock_any("PUT", &format!("/api/tasks/8974/{s}"), json!({"data": {}}))
            .await;
    }
    env.mock_any(
        "POST",
        "/api/tasks/8974/comments",
        json!({"data": {"id": 1, "message": "x"}}),
    )
    .await;
}

fn active_json(repo: &Path, started: Option<i64>) -> Value {
    json!({
        "schema": "tmgr.active.v1",
        "task": {"id": 8974, "key": "TM-512", "title": "Checkout: Apple Pay button", "url": "http://localhost:5173/personal/tasks/8974"},
        "workspace": "personal",
        "timer_started_at": started.map(chrono_rfc),
        "branch": "tm-512-checkout-apple-pay-button",
        "repo": repo.display().to_string(),
    })
}

fn chrono_rfc(epoch: i64) -> String {
    chrono::DateTime::from_timestamp(epoch, 0)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[test]
fn rfc_helper_matches_a_known_instant() {
    assert_eq!(chrono_rfc(STARTED), "2026-10-12T09:00:00Z");
}

#[tokio::test(flavor = "multi_thread")]
async fn start_creates_the_branch_moves_the_status_and_starts_the_timer() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 10, 0).await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let out = env
        .tmgr_in(&repo, None)
        .args(["start", "TM-512", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("start", &v);
    assert_eq!(
        v["data"],
        json!({
            "task": {"id": 8974, "ref": "TM-512", "title": "Checkout: Apple Pay button",
                     "url": "http://localhost:5173/personal/tasks/8974",
                     "status": {"id": 11, "name": "In progress", "type": "active"}},
            "status_change": {"from": {"id": 10, "name": "Backlog", "type": "default"},
                              "to": {"id": 11, "name": "In progress", "type": "active"}},
            "branch": {"name": "tm-512-checkout-apple-pay-button", "action": "created", "base": "main"},
            "stash": false,
            "timer": {"running": true, "started_at": "2026-10-12T09:00:00Z", "already_running": false, "stopped_previous": null, "stopped_all": []},
            "repo": "repo"
        })
    );
    assert_eq!(
        git(&repo, &["branch", "--show-current"]),
        "tm-512-checkout-apple-pay-button"
    );
    assert_eq!(env.requests_to("/api/tasks/8974/countdown").await.len(), 1);
    assert_eq!(env.requests_to("/api/tasks/8974/11").await.len(), 1);
    let active = env.read_active().unwrap();
    assert_eq!(active["task"]["key"], "TM-512");
    assert_eq!(active["timer_started_at"], "2026-10-12T09:00:00Z");
    assert_eq!(active["branch"], "tm-512-checkout-apple-pay-button");
    assert_eq!(active["repo"], repo.display().to_string());
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(env.active_path())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    assert_no_secrets(&out);
}

#[tokio::test(flavor = "multi_thread")]
async fn start_human_output_lists_what_happened() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 10, 0).await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let out = env
        .tmgr_in(&repo, None)
        .args(["start", "TM-512"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let s = text(&out.stdout);
    assert!(
        s.contains("task") && s.contains("TM-512 Checkout: Apple Pay button"),
        "{s}"
    );
    assert!(s.contains("Backlog \u{2192} In progress"), "{s}");
    assert!(
        s.contains("created tm-512-checkout-apple-pay-button from main"),
        "{s}"
    );
    assert!(s.contains("started at"), "{s}");
    assert!(!s.contains('\u{1b}'));
}

#[tokio::test(flavor = "multi_thread")]
async fn start_on_a_dirty_tree_is_a_conflict_and_changes_nothing() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 10, 0).await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    std::fs::write(repo.join("README.md"), "changed\n").unwrap();
    let out = env
        .tmgr_in(&repo, None)
        .args(["start", "TM-512", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(6));
    assert_valid("error", &stdout_json(&out));
    assert_eq!(git(&repo, &["branch", "--show-current"]), "main");
    assert!(env
        .requests_to("/api/tasks/8974/countdown")
        .await
        .is_empty());
    assert!(env.requests_to("/api/tasks/8974/11").await.is_empty());
    assert!(env.read_active().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn start_with_stash_stashes_untracked_files_too() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 11, 0).await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    std::fs::write(repo.join("README.md"), "changed\n").unwrap();
    std::fs::write(repo.join("new.txt"), "x\n").unwrap();
    let out = env
        .tmgr_in(&repo, None)
        .args(["start", "TM-512", "--stash", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(stdout_json(&out)["data"]["stash"], true);
    assert_eq!(git(&repo, &["status", "--porcelain"]), "");
    assert!(git(&repo, &["stash", "list"]).contains("tmgr: before TM-512"));
    assert_eq!(
        git(&repo, &["branch", "--show-current"]),
        "tm-512-checkout-apple-pay-button"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn start_on_the_target_branch_ignores_a_dirty_tree() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 11, 0).await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    git(&repo, &["switch", "-c", "tm-512-checkout-apple-pay-button"]);
    std::fs::write(repo.join("README.md"), "changed\n").unwrap();
    let out = env
        .tmgr_in(&repo, None)
        .args(["start", "TM-512", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(stdout_json(&out)["data"]["branch"]["action"], "current");
}

#[tokio::test(flavor = "multi_thread")]
async fn start_switches_to_an_existing_branch_and_honours_branch_flags() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 11, 0).await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    git(&repo, &["branch", "tm-512-checkout-apple-pay-button"]);
    let out = env
        .tmgr_in(&repo, None)
        .args(["start", "TM-512", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(stdout_json(&out)["data"]["branch"]["action"], "switched");
    git(&repo, &["switch", "main"]);
    let out = env
        .tmgr_in(&repo, None)
        .args([
            "start",
            "TM-512",
            "--branch",
            "feat/custom",
            "--from",
            "main",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_eq!(
        v["data"]["branch"],
        json!({"name": "feat/custom", "action": "created", "base": "main"})
    );
    git(&repo, &["switch", "main"]);
    let out = env
        .tmgr_in(&repo, None)
        .args(["start", "TM-512", "--no-branch", "--no-timer", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_eq!(v["data"]["branch"], Value::Null);
    assert_eq!(v["data"]["timer"]["running"], false);
    assert_eq!(git(&repo, &["branch", "--show-current"]), "main");
}

#[tokio::test(flavor = "multi_thread")]
async fn hostile_branch_and_base_arguments_are_rejected_before_any_request() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 11, 0).await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    for args in [
        vec!["start", "TM-512", "--branch=--upload-pack=x"],
        vec!["start", "TM-512", "--branch", "../x"],
        vec!["start", "TM-512", "--branch", "a b"],
        vec!["start", "TM-512", "--from=--output=x"],
        vec!["start", "--help-me"],
        vec!["start", "TM-512;id"],
    ] {
        let out = env.tmgr_in(&repo, None).args(&args).output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "{args:?}: {}",
            text(&out.stderr)
        );
    }
    assert!(env.server.received_requests().await.unwrap().is_empty());
    assert_eq!(git(&repo, &["branch", "--list"]).lines().count(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn injection_like_titles_become_safe_branch_names() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_statuses().await;
    let mut t = task(11, 0);
    t["title"] = json!("TM-512: --upload-pack=touch pwned ../../x $(id)");
    env.mock_json("/api/tasks/by-key/TM-512", json!({"data": t}))
        .await;
    env.mock_any(
        "POST",
        "/api/tasks/8974/countdown",
        json!({"data": task(11, STARTED)}),
    )
    .await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let out = env
        .tmgr_in(&repo, None)
        .args(["start", "TM-512", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        git(&repo, &["branch", "--show-current"]),
        "tm-512-upload-pack-touch-pwned-x-id"
    );
    assert!(!repo.join("pwned").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn start_outside_a_repository_still_starts_the_timer() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 11, 0).await;
    let plain = env.tmp.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let out = env
        .tmgr_in(&plain, None)
        .args(["start", "TM-512", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_eq!(v["data"]["branch"], Value::Null);
    assert_eq!(v["data"]["repo"], Value::Null);
    assert_eq!(v["data"]["timer"]["running"], true);
    assert_eq!(v["data"]["status_change"], Value::Null);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_mapped_repository_is_used_from_any_directory() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 11, 0).await;
    let repo = init_repo(&env.tmp.path().join("mapped"));
    let plain = env.tmp.path().join("elsewhere");
    std::fs::create_dir_all(&plain).unwrap();
    let out = env
        .tmgr_in(&plain, None)
        .args(["project", "map", "tm", repo.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let out = env
        .tmgr_in(&plain, None)
        .args(["start", "TM-512", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(stdout_json(&out)["data"]["repo"], "mapped");
    assert_eq!(
        git(&repo, &["branch", "--show-current"]),
        "tm-512-checkout-apple-pay-button"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_timer_on_another_task_needs_confirmation() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 11, 0).await;
    env.mock_any(
        "DELETE",
        "/api/tasks/7/countdown",
        json!({"data": {"id": 7, "title": "TM-7: x"}}),
    )
    .await;
    let plain = env.tmp.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let mut other = active_json(&plain, Some(STARTED));
    other["task"] = json!({"id": 7, "key": "TM-7", "title": "Other"});
    env.seed_active(other);

    let out = env
        .tmgr_in(&plain, None)
        .args(["start", "TM-512", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10), "{}", text(&out.stdout));
    assert_eq!(stdout_json(&out)["error"]["code"], "needs_confirmation");
    assert!(env.requests_to("/api/tasks/7/countdown").await.is_empty());
    assert!(env
        .requests_to("/api/tasks/8974/countdown")
        .await
        .is_empty());
    assert_eq!(env.read_active().unwrap()["task"]["key"], "TM-7");

    let out = env
        .tmgr_in(&plain, None)
        .args(["start", "TM-512", "--yes", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("start", &v);
    assert_eq!(
        v["data"]["timer"]["stopped_previous"],
        json!({"id": 7, "ref": "TM-7"})
    );
    assert_eq!(env.requests_to("/api/tasks/7/countdown").await.len(), 1);
    assert_eq!(env.read_active().unwrap()["task"]["key"], "TM-512");
}

#[tokio::test(flavor = "multi_thread")]
async fn start_never_restarts_a_timer_that_already_runs() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 11, STARTED).await;
    let plain = env.tmp.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let out = env
        .tmgr_in(&plain, None)
        .args(["start", "TM-512", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_eq!(v["data"]["timer"]["already_running"], true);
    assert_eq!(v["data"]["timer"]["started_at"], "2026-10-12T09:00:00Z");
    assert!(env
        .requests_to("/api/tasks/8974/countdown")
        .await
        .is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_stops_the_timer_and_keeps_the_active_task() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 11, 0).await;
    let plain = env.tmp.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    env.seed_active(active_json(&plain, Some(now_s() - 3700)));
    let out = env
        .tmgr_in(&plain, None)
        .args(["stop", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("stop", &v);
    assert_eq!(v["data"]["stopped"], true);
    assert_eq!(v["data"]["task"]["ref"], "TM-512");
    assert_eq!(v["data"]["tracked_s"], 4320);
    let secs = v["data"]["elapsed_s"].as_i64().unwrap();
    assert!((3700..3720).contains(&secs), "{secs}");
    let active = env.read_active().unwrap();
    assert_eq!(active["timer_started_at"], Value::Null);
    assert_eq!(active["task"]["key"], "TM-512");

    let out = env
        .tmgr_in(&plain, None)
        .args(["stop", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v = stdout_json(&out);
    assert_valid("stop", &v);
    assert_eq!(v["data"]["stopped"], false);
    assert_eq!(env.requests_to("/api/tasks/8974/countdown").await.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_without_an_active_task_is_an_empty_state_not_an_error() {
    let env = Env::new().await;
    env.login_seed();
    let out = env.cmd().args(["stop", "--json"]).output().unwrap();
    assert!(out.status.success());
    let v = stdout_json(&out);
    assert_valid("stop", &v);
    assert_eq!(v["data"]["task"], Value::Null);
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn switch_stops_the_previous_timer_without_asking() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 11, 0).await;
    env.mock_any(
        "DELETE",
        "/api/tasks/7/countdown",
        json!({"data": {"id": 7, "title": "TM-7: x"}}),
    )
    .await;
    let plain = env.tmp.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let mut other = active_json(&plain, Some(STARTED));
    other["task"] = json!({"id": 7, "key": "TM-7", "title": "Other"});
    env.seed_active(other);
    let out = env
        .tmgr_in(&plain, None)
        .args(["switch", "TM-512", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("start", &v);
    assert_eq!(v["data"]["timer"]["stopped_previous"]["ref"], "TM-7");
    assert_eq!(env.read_active().unwrap()["task"]["key"], "TM-512");
}

fn stub_gh(dir: &Path, url: &str) {
    let log = dir.join("gh.args");
    write_script(
        dir,
        "gh",
        &format!(
            "printf '%s\\n' \"$@\" > '{}'\necho '{{\"url\":\"{url}\"}}'",
            log.display()
        ),
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn done_stops_the_timer_moves_to_review_and_comments_the_pr() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 11, now_s() - 5220).await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    git(&repo, &["switch", "-c", "tm-512-checkout-apple-pay-button"]);
    let stubs = env.tmp.path().join("stubs");
    stub_gh(&stubs, "https://github.com/acme/site/pull/214");
    env.seed_active(active_json(&repo, Some(now_s() - 5220)));
    let out = env
        .tmgr_in(&repo, Some(&stubs))
        .args(["done", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("done", &v);
    let d = &v["data"];
    assert_eq!(d["task"]["status"]["name"], "Review");
    assert_eq!(d["status_change"]["from"]["name"], "In progress");
    assert_eq!(d["timer"]["stopped"], true);
    assert!((5220..5240).contains(&d["timer"]["elapsed_s"].as_i64().unwrap()));
    assert_eq!(d["pr_url"], "https://github.com/acme/site/pull/214");
    assert_eq!(d["pr_source"], "gh");
    assert_eq!(d["comment_posted"], true);
    assert_eq!(d["branch"], "tm-512-checkout-apple-pay-button");
    assert_eq!(env.requests_to("/api/tasks/8974/13").await.len(), 1);
    assert_eq!(env.requests_to("/api/tasks/8974/countdown").await.len(), 1);
    let comment = &env.requests_to("/api/tasks/8974/comments").await[0];
    let body: Value = serde_json::from_slice(&comment.body).unwrap();
    assert_eq!(body["message"], "PR: https://github.com/acme/site/pull/214");
    assert!(env.read_active().is_none());
    let args = std::fs::read_to_string(stubs.join("gh.args")).unwrap();
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        ["pr", "view", "--json", "url"]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn done_on_a_dirty_tree_is_a_conflict_unless_forced() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 11, 0).await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    std::fs::write(repo.join("README.md"), "changed\n").unwrap();
    env.seed_active(active_json(&repo, None));
    let out = env
        .tmgr_in(&repo, None)
        .args(["done", "--no-pr", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(6), "{}", text(&out.stdout));
    assert!(env.requests_to("/api/tasks/8974/13").await.is_empty());
    assert!(env.read_active().is_some());
    let out = env
        .tmgr_in(&repo, None)
        .args(["done", "--no-pr", "--force", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(env.requests_to("/api/tasks/8974/13").await.len(), 1);
    assert!(env.requests_to("/api/tasks/8974/comments").await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn done_without_gh_links_the_compare_url_of_the_origin_remote() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 11, 0).await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    git(&repo, &["switch", "-c", "tm-512-x"]);
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "https://user:s3cret@github.com/acme/site.git",
        ],
    );
    let stubs = env.tmp.path().join("stubs");
    write_script(&stubs, "gh", "exit 1");
    let out = env
        .tmgr_in(&repo, Some(&stubs))
        .args([
            "done",
            "TM-512",
            "--comment",
            "ready\u{1b}[31m for review",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_eq!(v["data"]["pr_source"], "compare");
    let comment = &env.requests_to("/api/tasks/8974/comments").await[0];
    let body: Value = serde_json::from_slice(&comment.body).unwrap();
    assert_eq!(
        body["message"],
        "ready[31m for review\n\nPR: https://github.com/acme/site/compare/tm-512-x?expand=1"
    );
    assert!(!text(&comment.body).contains("s3cret"));
    assert!(!text(&out.stdout).contains("s3cret"));
}

#[tokio::test(flavor = "multi_thread")]
async fn done_falls_back_to_the_completed_status_and_honours_flags() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_statuses().await;
    env.mock_json("/api/tasks/by-key/TM-512", json!({"data": task(11, 0)}))
        .await;
    env.mock_any("PUT", "/api/tasks/8974/12", json!({"data": {}}))
        .await;
    env.mock_any(
        "POST",
        "/api/tasks/8974/comments",
        json!({"data": {"id": 1}}),
    )
    .await;
    let plain = env.tmp.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let out = env
        .tmgr_in(&plain, None)
        .args(["done", "TM-512", "--comment", "shipped", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_eq!(v["data"]["task"]["status"]["name"], "Done");
    assert_eq!(v["data"]["timer"]["stopped"], false);
    assert_eq!(env.requests_to("/api/tasks/8974/12").await.len(), 1);
    let comment = &env.requests_to("/api/tasks/8974/comments").await[0];
    assert_eq!(
        serde_json::from_slice::<Value>(&comment.body).unwrap()["message"],
        "shipped"
    );
    let out = env
        .tmgr_in(&plain, None)
        .args(["done", "TM-512", "--status", "Nope", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
}

#[tokio::test(flavor = "multi_thread")]
async fn done_needs_a_task() {
    let env = Env::new().await;
    env.login_seed();
    let out = env.cmd().args(["done", "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn review_status_comes_from_the_config() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 11, 0).await;
    let plain = env.tmp.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let out = env
        .tmgr_in(&plain, None)
        .args(["config", "set", "review_status", "done"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let out = env
        .tmgr_in(&plain, None)
        .args(["done", "TM-512", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(stdout_json(&out)["data"]["task"]["status"]["name"], "Done");
}

#[tokio::test(flavor = "multi_thread")]
async fn branch_template_comes_from_the_config() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 11, 0).await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let out = env
        .tmgr_in(&repo, None)
        .args([
            "config",
            "set",
            "branch_template",
            "feat/{code_lower}-{seq}",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let out = env
        .tmgr_in(&repo, None)
        .args(["start", "TM-512", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(git(&repo, &["branch", "--show-current"]), "feat/tm-512");
    let out = env
        .tmgr_in(&repo, None)
        .args(["config", "set", "branch_template", "--x"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn project_map_list_unmap() {
    let env = Env::new().await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let out = env
        .tmgr_in(&repo, None)
        .args(["project", "map", "TM", ".", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        stdout_json(&out)["data"]["path"],
        repo.display().to_string()
    );
    let out = env
        .tmgr_in(&repo, None)
        .args(["project", "list", "--json"])
        .output()
        .unwrap();
    let v = stdout_json(&out);
    assert_eq!(v["data"]["projects"][0]["code"], "TM");
    assert_eq!(v["data"]["projects"][0]["exists"], true);
    let cfg = std::fs::read_to_string(env.config_dir().join("config.toml")).unwrap();
    assert!(
        cfg.contains(&format!("TM = \"{}\"", repo.display())),
        "{cfg}"
    );
    for bad in [
        vec!["project", "map", "tm-1", "."],
        vec!["project", "map", "TM", "/nonexistent-dir-xyz"],
    ] {
        let out = env.tmgr_in(&repo, None).args(&bad).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{bad:?}");
    }
    let out = env
        .tmgr_in(&repo, None)
        .args(["project", "unmap", "TM", "--json"])
        .output()
        .unwrap();
    assert_eq!(stdout_json(&out)["data"]["removed"], true);
    let out = env
        .tmgr_in(&repo, None)
        .args(["project", "list"])
        .output()
        .unwrap();
    assert!(text(&out.stdout).contains("No projects mapped"));
}

fn running(id: u64, title: &str, start: i64) -> Value {
    let mut t = task_json(id, title);
    t["start_time"] = json!(start);
    t
}

#[tokio::test(flavor = "multi_thread")]
async fn start_finds_other_running_timers_on_the_server() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 11, 0).await;
    env.mock_json(
        "/api/tasks/runned",
        json!({"data": [running(7, "TM-7: Other", STARTED), running(9, "TM-9: Third", STARTED + 5)], "meta": {"current_page": 1, "last_page": 1}}),
    )
    .await;
    env.mock_any(
        "DELETE",
        "/api/tasks/7/countdown",
        json!({"data": {"id": 7}}),
    )
    .await;
    env.mock_any(
        "DELETE",
        "/api/tasks/9/countdown",
        json!({"data": {"id": 9}}),
    )
    .await;
    let plain = env.tmp.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();

    let out = env
        .tmgr_in(&plain, None)
        .args(["start", "TM-512", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10), "{}", text(&out.stdout));
    for p in [
        "/api/tasks/7/countdown",
        "/api/tasks/9/countdown",
        "/api/tasks/8974/countdown",
    ] {
        assert!(env.requests_to(p).await.is_empty(), "{p}");
    }

    let out = env
        .tmgr_in(&plain, None)
        .args(["start", "TM-512", "--yes", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("start", &v);
    assert_eq!(
        v["data"]["timer"]["stopped_all"].as_array().unwrap().len(),
        2
    );
    assert_eq!(v["data"]["timer"]["stopped_previous"]["ref"], "TM-7");
    assert_eq!(env.requests_to("/api/tasks/7/countdown").await.len(), 1);
    assert_eq!(env.requests_to("/api/tasks/9/countdown").await.len(), 1);
    assert_eq!(env.requests_to("/api/tasks/8974/countdown").await.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_server_list_wins_over_a_stale_active_file() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 11, 0).await;
    env.mock_json("/api/tasks/runned", json!({"data": []}))
        .await;
    let plain = env.tmp.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let mut stale = active_json(&plain, Some(STARTED));
    stale["task"] = json!({"id": 7, "key": "TM-7", "title": "Other"});
    env.seed_active(stale);
    let out = env
        .tmgr_in(&plain, None)
        .args(["start", "TM-512", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(env.requests_to("/api/tasks/7/countdown").await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_that_already_runs_is_not_restarted_or_stopped() {
    let env = Env::new().await;
    env.login_seed();
    mock_flow(&env, 11, STARTED).await;
    env.mock_json(
        "/api/tasks/runned",
        json!({"data": [running(8974, "TM-512: Checkout: Apple Pay button", STARTED)]}),
    )
    .await;
    let plain = env.tmp.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let out = env
        .tmgr_in(&plain, None)
        .args(["start", "TM-512", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_eq!(v["data"]["timer"]["already_running"], true);
    assert_eq!(v["data"]["timer"]["stopped_all"], json!([]));
    assert!(env
        .requests_to("/api/tasks/8974/countdown")
        .await
        .is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn done_and_start_output_is_sanitised() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_json(
        "/api/workspaces/1/statuses",
        json!([
            {"id": 10, "name": "Back\u{1b}[31mlog\u{202e}", "type": "default"},
            {"id": 11, "name": "In\u{7} progress", "type": "active"}
        ]),
    )
    .await;
    let mut t = task(10, 0);
    t["title"] = json!("TM-512: evil \u{1b}]0;pwn\u{7} \u{202e}title");
    env.mock_json("/api/tasks/by-key/TM-512", json!({"data": t}))
        .await;
    env.mock_any("PUT", "/api/tasks/8974/11", json!({"data": {}}))
        .await;
    env.mock_any(
        "POST",
        "/api/tasks/8974/countdown",
        json!({"data": task(11, STARTED)}),
    )
    .await;
    let plain = env.tmp.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let out = env
        .tmgr_in(&plain, None)
        .args(["start", "TM-512"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let all = format!("{}{}", text(&out.stdout), text(&out.stderr));
    for bad in ['\u{1b}', '\u{7}', '\u{202e}'] {
        assert!(!all.contains(bad), "{all:?}");
    }
}
