mod common;
use common::*;
use serde_json::{json, Value};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

async fn mock_create(env: &Env) {
    env.mock_workspaces().await;
    env.mock_statuses().await;
    env.mock_categories().await;
    let mut created = task_json(9001, "TM-524: fix flaky upload test");
    created["status_id"] = json!(10);
    created["status"] = json!("default");
    Mock::given(method("POST"))
        .and(path("/api/tasks"))
        .and(header("authorization", format!("Bearer {ACCESS}").as_str()))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"data": created})))
        .mount(&env.server)
        .await;
}

async fn posted(env: &Env) -> Value {
    serde_json::from_slice(
        &env.requests_to("/api/tasks")
            .await
            .iter()
            .find(|r| r.method.as_str() == "POST")
            .unwrap()
            .body,
    )
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn add_with_args_resolves_category_and_default_status() {
    let env = Env::new().await;
    env.login_seed();
    mock_create(&env).await;
    let out = env
        .cmd()
        .args(["add", "fix flaky upload test", "-p", "TM", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("task_created", &v);
    assert_eq!(v["data"]["id"], 9001);
    assert_eq!(v["data"]["ref"], "TM-524");
    assert_eq!(
        v["data"]["url"],
        "http://localhost:5173/personal/tasks/9001"
    );
    let body = posted(&env).await;
    assert_eq!(body["title"], "fix flaky upload test");
    assert_eq!(body["status_id"], 10);
    assert_eq!(body["project_category_id"], 5);
    assert_eq!(body["workspace_id"], 1);
    assert!(body.get("description").is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn add_human_output_and_quiet_mode() {
    let env = Env::new().await;
    env.login_seed();
    mock_create(&env).await;
    let out = env
        .cmd()
        .args(["add", "fix flaky upload test", "-p", "TM"])
        .output()
        .unwrap();
    let s = text(&out.stdout);
    assert!(s.contains("created TM-524 fix flaky upload test"), "{s}");
    assert!(s.contains("Backlog"));
    let out = env
        .cmd()
        .args(["add", "x", "-p", "TM", "-q"])
        .output()
        .unwrap();
    assert_eq!(text(&out.stdout).trim(), "TM-524");
}

#[tokio::test(flavor = "multi_thread")]
async fn add_status_by_name_and_description_file() {
    let env = Env::new().await;
    env.login_seed();
    mock_create(&env).await;
    let file = env.tmp.path().join("desc.md");
    std::fs::write(&file, "details here\n").unwrap();
    let out = env
        .cmd()
        .args(["add", "t", "-s", "in progress", "--desc-file"])
        .arg(&file)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let body = posted(&env).await;
    assert_eq!(body["status_id"], 11);
    assert_eq!(body["description"], "details here\n");
    assert!(body.get("project_category_id").is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn add_from_stdin_splits_title_and_description() {
    let env = Env::new().await;
    env.login_seed();
    mock_create(&env).await;
    let out = env
        .cmd()
        .args(["add", "-", "-p", "TM"])
        .write_stdin("rename Pricing to Plans\n\nin the nav\nand footer\n")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let body = posted(&env).await;
    assert_eq!(body["title"], "rename Pricing to Plans");
    assert_eq!(body["description"], "in the nav\nand footer");
}

#[tokio::test(flavor = "multi_thread")]
async fn add_rejects_oversized_stdin_and_empty_titles() {
    let env = Env::new().await;
    env.login_seed();
    let big = "a".repeat(64 * 1024 + 1);
    let out = env
        .cmd()
        .args(["add", "-"])
        .write_stdin(big)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let out = env.cmd().args(["add", "   "]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn add_with_unknown_project_is_exit_3() {
    let env = Env::new().await;
    env.login_seed();
    mock_create(&env).await;
    let out = env.cmd().args(["add", "t", "-p", "ZZ"]).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(env.requests_to("/api/tasks").await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn add_without_the_create_scope_exits_5_with_a_relogin_hint() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_statuses().await;
    Mock::given(method("POST"))
        .and(path("/api/tasks"))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(json!({"error": "insufficient_scope"})),
        )
        .mount(&env.server)
        .await;
    let out = env.cmd().args(["add", "t", "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(5));
    let v = stdout_json(&out);
    assert_valid("error", &v);
    assert!(v["error"]["message"]
        .as_str()
        .unwrap()
        .contains("tasks:create"));
    assert!(v["error"]["hint"]
        .as_str()
        .unwrap()
        .starts_with("tmgr login --scopes "));
}
