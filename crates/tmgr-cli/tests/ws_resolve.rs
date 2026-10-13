mod common;
use common::*;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

async fn by_key(env: &Env, key: &str, ws: u64, found: bool) {
    let mut t = task_json(8974, &format!("{key}: Checkout"));
    t["workspace_id"] = json!(ws);
    let resp = if found {
        ResponseTemplate::new(200).set_body_json(json!({ "data": t }))
    } else {
        ResponseTemplate::new(404).set_body_json(json!({"message": "not found"}))
    };
    Mock::given(method("GET"))
        .and(path(format!("/api/tasks/by-key/{key}")))
        .and(query_param("workspace_id", ws.to_string()))
        .respond_with(resp)
        .mount(&env.server)
        .await;
}

async fn setup(env: &Env) {
    env.login_seed();
    env.mock_json(
        "/api/workspaces",
        json!({"data": [
            {"id": 1, "name": "Personal", "code": "personal", "is_default": true},
            {"id": 2, "name": "Team", "code": "team", "is_default": false},
            {"id": 3, "name": "Other", "code": "other", "is_default": false}
        ]}),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn key_found_only_in_another_workspace_switches_with_a_note() {
    let env = Env::new().await;
    setup(&env).await;
    by_key(&env, "TM-537", 1, false).await;
    by_key(&env, "TM-537", 2, true).await;
    by_key(&env, "TM-537", 3, false).await;
    let out = env.cmd().args(["task", "show", "TM-537"]).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("using workspace team (TM-537 is not in personal)"),
        "{}",
        text(&out.stderr)
    );
    assert!(
        text(&out.stdout).contains("TM-537"),
        "{}",
        text(&out.stdout)
    );
    assert!(!env.config_dir().join("config.toml").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn json_mode_gets_a_workspace_switched_warning() {
    let env = Env::new().await;
    setup(&env).await;
    by_key(&env, "TM-537", 1, false).await;
    by_key(&env, "TM-537", 2, true).await;
    by_key(&env, "TM-537", 3, false).await;
    let out = env
        .cmd()
        .args(["task", "show", "TM-537", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    let codes: Vec<_> = v["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap().to_string())
        .collect();
    assert!(codes.contains(&"workspace_switched".to_string()), "{v}");
    assert!(!text(&out.stderr).contains("using workspace"));
}

#[tokio::test(flavor = "multi_thread")]
async fn key_in_several_workspaces_is_a_conflict() {
    let env = Env::new().await;
    setup(&env).await;
    by_key(&env, "TM-537", 1, false).await;
    by_key(&env, "TM-537", 2, true).await;
    by_key(&env, "TM-537", 3, true).await;
    let out = env.cmd().args(["task", "show", "TM-537"]).output().unwrap();
    assert_eq!(out.status.code(), Some(6));
    let err = text(&out.stderr);
    assert!(err.contains("team, other"), "{err}");
    assert!(err.contains("add --ws CODE"), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn key_nowhere_is_exit_3() {
    let env = Env::new().await;
    setup(&env).await;
    for ws in [1, 2, 3] {
        by_key(&env, "TM-537", ws, false).await;
    }
    let out = env.cmd().args(["task", "show", "TM-537"]).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(text(&out.stderr).contains("hint:"), "{}", text(&out.stderr));
}

#[tokio::test(flavor = "multi_thread")]
async fn explicit_ws_never_searches_other_workspaces() {
    let env = Env::new().await;
    setup(&env).await;
    by_key(&env, "TM-537", 1, false).await;
    by_key(&env, "TM-537", 2, true).await;
    let out = env
        .cmd()
        .args(["--ws", "personal", "task", "show", "TM-537"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let calls = env.requests_to("/api/tasks/by-key/TM-537").await;
    assert_eq!(calls.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn ws_without_a_value_is_a_friendly_usage_error() {
    let env = Env::new().await;
    let out = env.cmd().arg("--ws").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = text(&out.stderr);
    assert!(
        err.contains("error: `--ws` needs a workspace code"),
        "{err}"
    );
    assert!(
        err.contains("hint:") && err.contains("tmgr ws list"),
        "{err}"
    );
    let out = env.cmd().args(["--ws", "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(stdout_json(&out)["error"]["code"], "usage");
}

#[tokio::test(flavor = "multi_thread")]
async fn list_flag_suggests_the_list_commands() {
    let env = Env::new().await;
    let out = env.cmd().arg("--list").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = text(&out.stderr);
    assert!(
        err.contains("did you mean `tmgr ws list` or `tmgr task list`?"),
        "{err}"
    );
    let out = env.cmd().args(["--list", "--json"]).output().unwrap();
    let v = stdout_json(&out);
    assert_eq!(v["error"]["exit"], 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn ws_flag_with_list_as_code_hints_ws_list() {
    let env = Env::new().await;
    setup(&env).await;
    let out = env.cmd().args(["--ws", "list"]).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(
        text(&out.stderr).contains("tmgr ws list"),
        "{}",
        text(&out.stderr)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn ws_alone_lists_workspaces() {
    let env = Env::new().await;
    setup(&env).await;
    let out = env.cmd().arg("ws").output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let s = text(&out.stdout);
    assert!(s.contains("personal") && s.contains("team"), "{s}");
}

#[tokio::test(flavor = "multi_thread")]
async fn ws_use_without_code_needs_a_terminal() {
    let env = Env::new().await;
    setup(&env).await;
    let out = env.cmd().args(["ws", "use"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("tmgr ws use <code>"),
        "{}",
        text(&out.stderr)
    );
    let out = env.cmd().args(["ws", "use", "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(stdout_json(&out)["error"]["code"], "usage");
}
