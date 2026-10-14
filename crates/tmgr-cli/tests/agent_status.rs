mod common;
use common::*;
use serde_json::{json, Value};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

const SESSION: &str = "sess-abcd1234";

fn session(extra: Value) -> Value {
    let mut v = json!({
        "session_id": SESSION, "workspace_id": 1, "task": {"id": 8974, "key": "TM-512"},
        "state": "working", "state_since": "2026-10-14T09:30:00Z",
        "started_at": "2026-10-14T09:00:00Z", "updated_at": "2026-10-14T09:30:00Z",
        "ended_at": null, "agent_profile": "claude-code", "host_label": "laptop",
        "user": {"id": 3, "name": "Ann"}, "stale": false
    });
    for (k, val) in extra.as_object().cloned().unwrap_or_default() {
        v[k] = val;
    }
    v
}

async fn mock_status(env: &Env, verb: &str, p: &str, template: ResponseTemplate) {
    Mock::given(method(verb))
        .and(path(p))
        .and(header("authorization", format!("Bearer {ACCESS}").as_str()))
        .respond_with(template)
        .mount(&env.server)
        .await;
}

async fn enabled() -> Env {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    let out = env
        .cmd()
        .args(["config", "set", "upload_status", "true"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    env
}

fn set_args(extra: &[&'static str]) -> Vec<&'static str> {
    let mut a = vec![
        "agent", "status", "set", SESSION, "--state", "working", "--task", "TM-512",
    ];
    a.extend_from_slice(extra);
    a.push("--json");
    a
}

#[tokio::test(flavor = "multi_thread")]
async fn enabling_upload_prints_what_is_sent_and_it_defaults_to_off() {
    let env = Env::new().await;
    let out = env
        .cmd()
        .args(["config", "get", "upload_status"])
        .output()
        .unwrap();
    assert_eq!(text(&out.stdout).trim(), "false");
    let out = env
        .cmd()
        .args(["config", "set", "upload_status", "true"])
        .output()
        .unwrap();
    let human = text(&out.stdout);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(human.contains("upload_status = true"));
    for word in ["task", "state", "agent profile", "timestamps", "host label"] {
        assert!(human.contains(word), "note lacks {word}: {human}");
    }
    assert!(human.contains("Terminal content is never sent"));
    let out = env
        .cmd()
        .args(["config", "set", "upload_status", "false"])
        .output()
        .unwrap();
    assert!(!text(&out.stdout).contains("host label"));
    let out = env
        .cmd()
        .args(["config", "set", "upload_status", "perhaps"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn set_and_clear_are_refused_while_upload_is_off() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    for args in [
        vec![
            "agent", "status", "set", SESSION, "--state", "idle", "--json",
        ],
        vec!["agent", "status", "clear", SESSION, "--json"],
    ] {
        let out = env.cmd().args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(5));
        let v = stdout_json(&out);
        assert_eq!(v["ok"], false);
        assert_eq!(v["error"]["code"], "forbidden");
        assert_eq!(v["error"]["hint"], "tmgr config set upload_status true");
    }
    assert!(env
        .requests_to(&format!("/api/agent-status/{SESSION}"))
        .await
        .is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn set_puts_the_state_with_a_key_and_the_workspace() {
    let env = enabled().await;
    mock_status(
        &env,
        "PUT",
        &format!("/api/agent-status/{SESSION}"),
        ResponseTemplate::new(200).set_body_json(json!({"data": session(json!({}))})),
    )
    .await;
    let out = env
        .cmd()
        .args(set_args(&["--since", "2026-10-14T11:30:00+02:00"]))
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_no_secrets(&out);
    let v = stdout_json(&out);
    assert_valid("agent_status_session", &v);
    assert_eq!(v["data"]["session"]["task"]["key"], "TM-512");
    assert_eq!(v["data"]["disabled"], false);
    let sent: Value = serde_json::from_slice(
        &env.requests_to(&format!("/api/agent-status/{SESSION}"))
            .await[0]
            .body,
    )
    .unwrap();
    assert_eq!(
        sent,
        json!({"task_key": "TM-512", "workspace_id": 1, "agent_profile": "claude-code",
               "state": "working", "state_since": "2026-10-14T09:30:00Z"})
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_id_profile_and_workspace_are_sent_as_given() {
    let env = enabled().await;
    mock_status(
        &env,
        "PUT",
        &format!("/api/agent-status/{SESSION}"),
        ResponseTemplate::new(200)
            .set_body_json(json!({"data": session(json!({"state": "idle"}))})),
    )
    .await;
    let out = env
        .cmd()
        .args([
            "--ws",
            "work",
            "agent",
            "status",
            "set",
            SESSION,
            "--state",
            "idle",
            "--task",
            "#8974",
            "--profile",
            "codex",
            "--started",
            "2026-10-14T09:00:00Z",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("idle"));
    let sent: Value = serde_json::from_slice(
        &env.requests_to(&format!("/api/agent-status/{SESSION}"))
            .await[0]
            .body,
    )
    .unwrap();
    assert_eq!(sent["task_id"], 8974);
    assert!(sent.get("task_key").is_none());
    assert_eq!(sent["workspace_id"], 2);
    assert_eq!(sent["agent_profile"], "codex");
    assert_eq!(sent["started_at"], "2026-10-14T09:00:00Z");
}

#[tokio::test(flavor = "multi_thread")]
async fn clear_deletes_the_session() {
    let env = enabled().await;
    mock_status(
        &env,
        "DELETE",
        &format!("/api/agent-status/{SESSION}"),
        ResponseTemplate::new(200)
            .set_body_json(json!({"data": session(json!({"ended_at": "2026-10-14T10:00:00Z"}))})),
    )
    .await;
    let out = env
        .cmd()
        .args(["agent", "status", "clear", SESSION, "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("agent_status_session", &v);
    assert_eq!(v["data"]["session"]["ended_at"], "2026-10-14T10:00:00Z");
    let human = env
        .cmd()
        .args(["agent", "status", "clear", SESSION])
        .output()
        .unwrap();
    assert!(text(&human.stdout).contains("ended"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_scope_exits_5_with_the_login_hint() {
    let env = enabled().await;
    mock_status(
        &env,
        "PUT",
        &format!("/api/agent-status/{SESSION}"),
        ResponseTemplate::new(403).set_body_json(json!({"error": "insufficient_scope"})),
    )
    .await;
    let out = env.cmd().args(set_args(&[])).output().unwrap();
    assert_eq!(out.status.code(), Some(5));
    let v = stdout_json(&out);
    assert!(v["error"]["hint"]
        .as_str()
        .unwrap()
        .starts_with("tmgr login --scopes "));
    assert!(v["error"]["hint"]
        .as_str()
        .unwrap()
        .ends_with(",agent-status:write"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rate_limit_exits_6_and_names_the_wait() {
    let env = enabled().await;
    mock_status(
        &env,
        "PUT",
        &format!("/api/agent-status/{SESSION}"),
        ResponseTemplate::new(429)
            .insert_header("Retry-After", "17")
            .set_body_json(json!({"error": "rate_limited", "message": "slow down"})),
    )
    .await;
    let out = env.cmd().args(set_args(&[])).output().unwrap();
    assert_eq!(out.status.code(), Some(6));
    assert!(stdout_json(&out)["error"]["message"]
        .as_str()
        .unwrap()
        .contains("retry in 17s"));
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_work_off_is_a_warning_not_a_failure() {
    let env = enabled().await;
    mock_status(
        &env,
        "PUT",
        &format!("/api/agent-status/{SESSION}"),
        ResponseTemplate::new(403).set_body_json(json!({"error": "feature_disabled"})),
    )
    .await;
    mock_status(
        &env,
        "GET",
        "/api/workspaces/1/agent-status",
        ResponseTemplate::new(403).set_body_json(json!({"error": "feature_disabled"})),
    )
    .await;
    let out = env.cmd().args(set_args(&[])).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("agent_status_session", &v);
    assert_eq!(v["data"]["disabled"], true);
    assert_eq!(v["warnings"][0]["code"], "agent_work_disabled");
    let out = env
        .cmd()
        .args(["agent", "status", "list", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v = stdout_json(&out);
    assert_valid("agent_status_list", &v);
    assert_eq!(v["data"]["sessions"], json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_arguments_exit_2_before_any_request() {
    let env = enabled().await;
    for args in [
        vec!["agent", "status", "set", "short", "--state", "idle"],
        vec!["agent", "status", "set", SESSION, "--state", "running"],
        vec![
            "agent",
            "status",
            "set",
            SESSION,
            "--state",
            "idle",
            "--profile",
            "Bad",
        ],
        vec![
            "agent", "status", "set", SESSION, "--state", "idle", "--since", "soon",
        ],
        vec![
            "agent", "status", "set", SESSION, "--state", "idle", "--task", "nope",
        ],
        vec!["agent", "status", "clear", "bad id"],
    ] {
        let out = env.cmd().args(&args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
    assert!(env
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.url.path() == "/api/workspaces"));
}

#[tokio::test(flavor = "multi_thread")]
async fn list_needs_no_upload_opt_in_and_sanitises_server_text() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    mock_status(
        &env,
        "GET",
        "/api/workspaces/1/agent-status",
        ResponseTemplate::new(200).set_body_json(json!({"data": [
            session(json!({"host_label": "evil\u{1b}[31m-box", "user": {"id": 3, "name": "An\u{7}n"}})),
            session(json!({"session_id": "sess-other999", "task": null, "state": "waiting",
                           "stale": true, "host_label": null}))
        ]})),
    )
    .await;
    let out = env
        .cmd()
        .args(["agent", "status", "list"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let human = text(&out.stdout);
    assert!(human.starts_with("TASK"), "{human}");
    assert!(human.contains("TM-512") && human.contains("waiting (stale)"));
    assert!(!human.contains('\u{1b}') && !human.contains('\u{7}'));
    let out = env
        .cmd()
        .args(["agent", "status", "list", "--json"])
        .output()
        .unwrap();
    let v = stdout_json(&out);
    assert_valid("agent_status_list", &v);
    assert_eq!(v["data"]["workspace"], "personal");
    assert_eq!(v["data"]["sessions"].as_array().unwrap().len(), 2);
    assert_eq!(v["data"]["sessions"][1]["task"], Value::Null);
}

#[tokio::test(flavor = "multi_thread")]
async fn list_honours_ws() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    mock_status(
        &env,
        "GET",
        "/api/workspaces/2/agent-status",
        ResponseTemplate::new(200).set_body_json(json!({"data": []})),
    )
    .await;
    let out = env
        .cmd()
        .args(["--ws", "work", "agent", "status", "list"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("No agent sessions"));
}
