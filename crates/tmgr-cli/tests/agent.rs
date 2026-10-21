mod common;
use common::*;
use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

fn run_view(extra: Value) -> Value {
    let mut v = json!({
        "id": 41, "task_id": 8974, "workspace_id": 1, "user_id": 1, "agent": "claude-code",
        "model": "opus", "branch": "tm-512-x", "status": "running",
        "started_at": "2026-10-12T09:00:00Z", "duration_seconds": 12, "commits": [], "version": 1
    });
    for (k, val) in extra.as_object().cloned().unwrap_or_default() {
        v[k] = val;
    }
    v
}

async fn mock_task(env: &Env) {
    env.mock_workspaces().await;
    env.mock_json(
        "/api/tasks/by-key/TM-512",
        json!({"data": task_json(8974, "TM-512: Checkout: Apple Pay button")}),
    )
    .await;
}

async fn mock_run(env: &Env, verb: &str, p: &str, status: u16, body: Value) {
    Mock::given(method(verb))
        .and(path(p))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&env.server)
        .await;
}

fn runs_json(env: &Env) -> Value {
    serde_json::from_str(&std::fs::read_to_string(env.state_dir().join("runs.json")).unwrap())
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn start_update_finish_keep_one_run_and_merge_commits() {
    let env = Env::new().await;
    env.login_seed();
    mock_task(&env).await;
    mock_run(
        &env,
        "POST",
        "/api/tasks/8974/agent-work",
        201,
        run_view(json!({})),
    )
    .await;
    mock_run(
        &env,
        "PATCH",
        "/api/agent-work/41",
        200,
        run_view(json!({"commits": [{"sha": "abc1234", "message": "one"}]})),
    )
    .await;
    mock_run(
        &env,
        "POST",
        "/api/agent-work/41/finish",
        200,
        run_view(json!({"status": "succeeded", "pr_url": "https://example.com/pr/1"})),
    )
    .await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    git(&repo, &["checkout", "-q", "-b", "tm-512-x"]);

    let out = env
        .tmgr_in(&repo, None)
        .args(["agent", "start", "TM-512", "--model", "opus", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("agent_run", &v);
    assert_eq!(v["data"]["run_id"], 41);
    assert_eq!(v["data"]["task_ref"], "TM-512");
    assert_eq!(v["data"]["disabled"], false);
    let body = &env.requests_to("/api/tasks/8974/agent-work").await[0];
    let sent: Value = serde_json::from_slice(&body.body).unwrap();
    assert_eq!(sent["agent"], "claude-code");
    assert_eq!(sent["model"], "opus");
    assert_eq!(sent["branch"], "tm-512-x");
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(env.state_dir().join("runs.json"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    let state = runs_json(&env);
    assert_eq!(state["runs"][0]["run_id"], 41);
    assert_eq!(state["runs"][0]["branch"], "tm-512-x");
    assert_eq!(state["runs"][0]["start_sha"].as_str().unwrap().len(), 40);

    for (commit, expect) in [("abc1234 one", 1), ("def5678 two", 2)] {
        let out = env
            .tmgr_in(&repo, None)
            .args([
                "agent",
                "update",
                "--commit",
                commit,
                "--tests-passed",
                "3",
                "--tests-failed",
                "0",
                "--tests-cmd",
                "cargo test",
                "--json",
            ])
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", text(&out.stderr));
        assert_valid("agent_run", &stdout_json(&out));
        let patches = env.requests_to("/api/agent-work/41").await;
        let last: Value = serde_json::from_slice(&patches.last().unwrap().body).unwrap();
        assert_eq!(last["commits"].as_array().unwrap().len(), expect);
        assert_eq!(
            last["tests"],
            json!({"passed": 3, "failed": 0, "command": "cargo test"})
        );
    }
    let patches = env.requests_to("/api/agent-work/41").await;
    let last: Value = serde_json::from_slice(&patches.last().unwrap().body).unwrap();
    assert_eq!(
        last["commits"],
        json!([{"sha": "abc1234", "message": "one"}, {"sha": "def5678", "message": "two"}])
    );

    let out = env
        .tmgr_in(&repo, None)
        .args([
            "agent",
            "finish",
            "--status",
            "succeeded",
            "--summary",
            "done",
            "--pr",
            "https://example.com/pr/1",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("agent_run", &v);
    assert_eq!(v["data"]["status"], "succeeded");
    let fin: Value =
        serde_json::from_slice(&env.requests_to("/api/agent-work/41/finish").await[0].body)
            .unwrap();
    assert_eq!(fin["status"], "succeeded");
    assert_eq!(fin["summary"], "done");
    assert_eq!(fin["pr_url"], "https://example.com/pr/1");
    assert!(runs_json(&env)["runs"].as_array().unwrap().is_empty());
    assert_no_secrets(&out);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disabled_workspace_is_a_warning_not_a_failure() {
    let env = Env::new().await;
    env.login_seed();
    mock_task(&env).await;
    mock_run(
        &env,
        "POST",
        "/api/tasks/8974/agent-work",
        403,
        json!({"message": "Agent work is disabled in this workspace", "error": "feature_disabled", "feature": "agent_work"}),
    )
    .await;
    let out = env
        .cmd()
        .args(["agent", "start", "TM-512", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("agent_run", &v);
    assert_eq!(v["data"]["run_id"], Value::Null);
    assert_eq!(v["data"]["disabled"], true);
    assert_eq!(v["warnings"][0]["code"], "agent_work_disabled");
    assert!(!env.state_dir().join("runs.json").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_scope_is_exit_5_with_a_relogin_hint() {
    let env = Env::new().await;
    env.login_seed();
    mock_task(&env).await;
    mock_run(
        &env,
        "POST",
        "/api/tasks/8974/agent-work",
        403,
        json!({"error": "insufficient_scope"}),
    )
    .await;
    let out = env
        .cmd()
        .args(["agent", "start", "TM-512", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    let v = stdout_json(&out);
    assert!(v["error"]["hint"]
        .as_str()
        .unwrap()
        .ends_with(",agent-work:write"));
}

#[tokio::test(flavor = "multi_thread")]
async fn update_without_a_run_for_the_branch_is_a_usage_error() {
    let env = Env::new().await;
    env.login_seed();
    let repo = init_repo(&env.tmp.path().join("repo"));
    let out = env
        .tmgr_in(&repo, None)
        .args(["agent", "update", "--commit", "abc1234 x", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let out = env
        .tmgr_in(&repo, None)
        .args([
            "agent",
            "update",
            "--run",
            "41",
            "--commit",
            "not a commit",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_run_merges_with_the_commits_the_server_already_has() {
    let env = Env::new().await;
    env.login_seed();
    mock_task(&env).await;
    env.mock_json(
        "/api/tasks/8974/agent-work",
        json!({"runs": [{"id": 41, "task_id": 8974, "status": "running", "commits": [{"sha": "aaa1111", "message": "old one"}]}], "totals": {}}),
    )
    .await;
    mock_run(
        &env,
        "PATCH",
        "/api/agent-work/41",
        200,
        run_view(json!({})),
    )
    .await;
    let repo = init_repo(&env.tmp.path().join("repo"));

    let out = env
        .tmgr_in(&repo, None)
        .args([
            "agent",
            "update",
            "--run",
            "41",
            "--commit",
            "bbb2222 new one",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stdout));
    assert!(env.requests_to("/api/agent-work/41").await.is_empty());

    let out = env
        .tmgr_in(&repo, None)
        .args([
            "agent",
            "update",
            "--run",
            "41",
            "--task",
            "TM-512",
            "--commit",
            "bbb2222 new one",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let sent: Value =
        serde_json::from_slice(&env.requests_to("/api/agent-work/41").await[0].body).unwrap();
    assert_eq!(
        sent["commits"],
        json!([{"sha": "aaa1111", "message": "old one"}, {"sha": "bbb2222", "message": "new one"}])
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn finish_posts_my_git_commits_from_the_run_interval() {
    let env = Env::new().await;
    env.login_seed();
    let t0 = 1_800_000_000;
    let started = chrono::DateTime::from_timestamp(t0, 0)
        .unwrap()
        .to_rfc3339();
    let ended = chrono::DateTime::from_timestamp(t0 + 600, 0)
        .unwrap()
        .to_rfc3339();
    mock_run(
        &env,
        "POST",
        "/api/agent-work/41/finish",
        200,
        run_view(json!({"status": "succeeded", "started_at": started, "ended_at": ended})),
    )
    .await;
    mock_run(
        &env,
        "POST",
        "/api/agent-work/41/commits",
        200,
        run_view(json!({})),
    )
    .await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    git(&repo, &["config", "user.email", "me@example.com"]);
    commit_at(&repo, "early", t0 - 7200, "me@example.com");
    let mine = commit_at(&repo, "mine", t0 + 60, "me@example.com");
    commit_at(&repo, "theirs", t0 + 90, "you@example.com");
    let out = env
        .tmgr_in(&repo, None)
        .args([
            "agent",
            "finish",
            "--run",
            "41",
            "--status",
            "succeeded",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let posts = env.requests_to("/api/agent-work/41/commits").await;
    assert_eq!(posts.len(), 1);
    let body: Value = serde_json::from_slice(&posts[0].body).unwrap();
    assert_eq!(body["commits"].as_array().unwrap().len(), 1);
    assert_eq!(body["commits"][0]["sha"], mine.as_str());
    assert_eq!(body["commits"][0]["message"], "mine");
    assert_valid("agent_run", &stdout_json(&out));
}
