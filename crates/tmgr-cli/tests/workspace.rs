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
    assert_eq!(out.status.code(), Some(3));
    let out = env.cmd().args(["ws", "use", "local:a/b"]).output().unwrap();
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
        .and(path("/api/workspaces/1/time/daily"))
        .and(query_param("mine", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "from": "2026-10-12", "to": "2026-10-12", "tz": "UTC", "days": [
                {"date": chrono::Utc::now().format("%Y-%m-%d").to_string(), "total_s": 13260, "items": []}
            ]
        }})))
        .mount(&env.server)
        .await;
    let out = env.cmd().env("TZ", "UTC").output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let s = text(&out.stdout);
    assert!(s.contains("TMGR  workspace Personal"), "{s}");
    assert!(s.contains("TODAY   3h 41m tracked"), "{s}");
    let out = env
        .cmd()
        .env("TZ", "UTC")
        .args(["status", "--json"])
        .output()
        .unwrap();
    let v = stdout_json(&out);
    assert_valid("status", &v);
    assert_eq!(v["data"]["today"]["total_s"], 13260);
    assert_eq!(v["data"]["today"]["human_s"], 13260);
    assert!(v["data"]["today"]["agent_s"].is_null());
    assert!(
        v["data"]["active"].is_null()
            && v["data"]["git"].is_null()
            && v["data"]["agents"].is_null()
    );
    assert_eq!(v["warnings"][0]["code"], "active_unavailable");
    let req = &env.requests_to("/api/workspaces/1/time/daily").await[0];
    let q = req.url.query().unwrap();
    assert!(
        q.contains("from=") && q.contains("to=") && q.contains("tz=UTC"),
        "{q}"
    );
    assert!(env
        .requests_to("/api/workspaces/1/time-summary")
        .await
        .is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn status_omits_today_with_a_warning_when_time_is_forbidden() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    Mock::given(method("GET"))
        .and(path("/api/workspaces/1/time/daily"))
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
async fn status_falls_back_to_the_time_summary_on_an_older_server() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_json(
        "/api/workspaces/1/time-summary",
        json!({"data": {"total_s": 600, "items": []}}),
    )
    .await;
    let out = env.cmd().args(["status", "--json"]).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("status", &v);
    assert_eq!(v["data"]["today"]["total_s"], 600);
    let codes: Vec<_> = v["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"daily_time_unavailable"), "{codes:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn status_without_login_is_exit_4() {
    let env = Env::new().await;
    let out = env.cmd().output().unwrap();
    assert_eq!(out.status.code(), Some(4));
    assert!(text(&out.stderr).contains("hint: tmgr login"));
}

async fn status_fixture(env: &Env, runned: serde_json::Value) {
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_statuses().await;
    env.mock_json(
        "/api/workspaces/1/time/daily",
        json!({"data": {"days": [{"date": chrono::Utc::now().format("%Y-%m-%d").to_string(), "total_s": 60, "items": []}]}}),
    )
    .await;
    env.mock_json("/api/tasks/runned", runned).await;
}

fn running_task(id: u64, title: &str, start: i64) -> serde_json::Value {
    let mut t = task_json(id, title);
    t["start_time"] = json!(start);
    t
}

#[tokio::test(flavor = "multi_thread")]
async fn status_fills_active_from_the_running_timer() {
    let env = Env::new().await;
    let start = now_s() - 4320;
    status_fixture(
        &env,
        json!({"data": [running_task(8974, "TM-512: Checkout", start)]}),
    )
    .await;
    let out = env.cmd().args(["status", "--json"]).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("status", &v);
    let a = &v["data"]["active"];
    assert_eq!(a["task"]["ref"], "TM-512");
    assert_eq!(a["task"]["title"], "Checkout");
    assert_eq!(a["task"]["status"]["name"], "In progress");
    assert_eq!(
        a["task"]["url"],
        "http://localhost:5173/personal/tasks/8974"
    );
    assert_eq!(a["timer"]["running"], true);
    assert!((4320..4340).contains(&a["timer"]["seconds"].as_i64().unwrap()));
    assert!(v["warnings"].as_array().unwrap().is_empty());
    let out = env.cmd().arg("status").output().unwrap();
    let s = text(&out.stdout);
    assert!(s.contains("ACTIVE  TM-512  Checkout"), "{s}");
    assert!(s.contains("1h 12m"), "{s}");
}

#[tokio::test(flavor = "multi_thread")]
async fn status_with_several_timers_shows_the_latest_and_warns() {
    let env = Env::new().await;
    let now = now_s();
    status_fixture(
        &env,
        json!({"data": [
            running_task(7, "TM-7: Old", now - 9000),
            running_task(8974, "TM-512: New", now - 100)
        ]}),
    )
    .await;
    let out = env.cmd().args(["status", "--json"]).output().unwrap();
    let v = stdout_json(&out);
    assert_valid("status", &v);
    assert_eq!(v["data"]["active"]["task"]["ref"], "TM-512");
    assert_eq!(v["warnings"][0]["code"], "multiple_timers");
}

#[tokio::test(flavor = "multi_thread")]
async fn status_without_running_timers_has_no_active_and_no_warning() {
    let env = Env::new().await;
    status_fixture(&env, json!({"data": []})).await;
    let out = env.cmd().args(["status", "--json"]).output().unwrap();
    let v = stdout_json(&out);
    assert_valid("status", &v);
    assert!(v["data"]["active"].is_null());
    assert!(v["warnings"].as_array().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn ws_list_shows_cloud_and_local_workspaces_together() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    let ws = env.tmp.path().join(".tmgr.dev/workspaces/personal");
    std::fs::create_dir_all(&ws).unwrap();
    std::fs::write(
        ws.join("workspace.json"),
        r#"{"id":1,"name":"Personal","code":"personal","schema_version":12,"created_at":"x"}"#,
    )
    .unwrap();
    let out = env.cmd().args(["--json", "ws", "list"]).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    let all = v["data"]["workspaces"].as_array().unwrap();
    let codes: Vec<&str> = all.iter().map(|w| w["code"].as_str().unwrap()).collect();
    assert!(codes.contains(&"local:personal"), "{codes:?}");
    assert!(all.iter().any(|w| w["local"] == false), "{codes:?}");
    assert_eq!(all.iter().filter(|w| w["current"] == true).count(), 1);
    assert_eq!(
        all.iter().find(|w| w["current"] == true).unwrap()["local"],
        false
    );
}
