mod common;
use common::*;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test(flavor = "multi_thread")]
async fn ws_list_marks_the_current_workspace() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    let out = env.cmd().args(["ws", "list"]).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let s = text(&out.stdout);
    assert!(s.contains("*  personal  Personal"), "{s}");
    assert!(s.contains("   work      Work"), "{s}");
    let out = env
        .cmd()
        .args(["--ws", "work", "ws", "list", "--json"])
        .output()
        .unwrap();
    let v = stdout_json(&out);
    assert_eq!(v["data"]["workspaces"][1]["current"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn ws_use_writes_the_default_workspace() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    let out = env.cmd().args(["ws", "use", "work"]).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let cfg = std::fs::read_to_string(env.config_dir().join("config.toml")).unwrap();
    assert!(cfg.contains("default_workspace = \"work\""), "{cfg}");
    let out = env
        .cmd()
        .args(["config", "get", "default_workspace"])
        .output()
        .unwrap();
    assert_eq!(text(&out.stdout).trim(), "work");
}

#[tokio::test(flavor = "multi_thread")]
async fn ws_use_unknown_or_malformed_codes() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    let out = env.cmd().args(["ws", "use", "nope"]).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    let out = env.cmd().args(["ws", "use", "../x"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let out = env.cmd().args(["ws", "use", "local:x"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn config_get_set_list_path() {
    let env = Env::new().await;
    let out = env.cmd().args(["config", "path"]).output().unwrap();
    assert_eq!(
        text(&out.stdout).trim(),
        env.config_dir().join("config.toml").display().to_string()
    );
    let out = env
        .cmd()
        .args(["config", "set", "server", "https://api.example.com/"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let out = env
        .cmd_for("https://ignored.example.com")
        .args(["config", "list", "--json"])
        .env_remove("TMGR_SERVER")
        .output()
        .unwrap();
    let v = stdout_json(&out);
    assert_eq!(v["data"]["values"]["server"], "https://api.example.com");
    let out = env
        .cmd()
        .args(["config", "set", "server", "http://example.com"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let out = env
        .cmd()
        .args(["config", "set", "token", "x"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let out = env
        .cmd()
        .args(["config", "get", "projects.TM"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn status_shows_workspace_and_todays_total() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    Mock::given(method("GET"))
        .and(path("/api/workspaces/1/time-summary"))
        .and(query_param("group_by", "task"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "from": "2026-10-12", "to": "2026-10-12", "total_s": 13260, "items": []
        }})))
        .mount(&env.server)
        .await;
    let out = env.cmd().output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let s = text(&out.stdout);
    assert!(s.contains("TMGR  workspace Personal"), "{s}");
    assert!(s.contains("TODAY   3h 41m tracked"), "{s}");
    let out = env.cmd().args(["status", "--json"]).output().unwrap();
    let v = stdout_json(&out);
    assert_valid("status", &v);
    assert_eq!(v["data"]["today"]["total_s"], 13260);
    assert!(
        v["data"]["active"].is_null()
            && v["data"]["git"].is_null()
            && v["data"]["agents"].is_null()
    );
    assert_eq!(v["warnings"][0]["code"], "active_unavailable");
    let req = &env.requests_to("/api/workspaces/1/time-summary").await[0];
    let q = req.url.query().unwrap();
    assert!(q.contains("from=") && q.contains("to="));
}

#[tokio::test(flavor = "multi_thread")]
async fn status_omits_today_with_a_warning_when_time_is_forbidden() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    Mock::given(method("GET"))
        .and(path("/api/workspaces/1/time-summary"))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(json!({"error": "insufficient_scope"})),
        )
        .mount(&env.server)
        .await;
    let out = env.cmd().args(["--json"]).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("status", &v);
    assert!(v["data"]["today"].is_null());
    let codes: Vec<_> = v["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"time_summary_unavailable"));
}

#[tokio::test(flavor = "multi_thread")]
async fn status_without_login_is_exit_4() {
    let env = Env::new().await;
    let out = env.cmd().output().unwrap();
    assert_eq!(out.status.code(), Some(4));
    assert!(text(&out.stderr).contains("hint: tmgr login"));
}
