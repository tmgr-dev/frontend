mod common;
use common::*;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::time::{Duration, Instant};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn run_json(id: u64, version: i64, status: &str) -> Value {
    json!({
        "id": id, "task_id": 8974, "workspace_id": 1, "user_id": 3, "persona_id": null,
        "agent": "claude-code", "model": "opus", "session_id": null,
        "branch": "tm-518-upload-retry", "status": status,
        "started_at": "2026-10-12T09:41:00Z", "ended_at": null, "duration_seconds": 840,
        "summary": "backoff added", "pr_url": null,
        "commits": [{"sha": "d41a0be", "message": "feat: exponential backoff"}],
        "tests": {"passed": 56, "failed": 0, "command": "cargo test"},
        "version": version,
        "task": {"id": 8974, "key": "TM-518", "title": "TM-518: Upload retry logic"}
    })
}

async fn mock_runs(env: &Env, body: Value) {
    env.mock_json("/api/workspaces/1/agent-work", body).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn log_json_matches_the_schema_and_sends_the_filters() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_json(
        "/api/tasks/by-key/TM-518",
        json!({"data": task_json(8974, "TM-518: Upload retry logic")}),
    )
    .await;
    mock_runs(
        &env,
        json!({"data": [run_json(2, 5, "running"), run_json(1, 9, "succeeded")], "next_cursor": "c2"}),
    )
    .await;
    let out = env
        .cmd()
        .args([
            "log",
            "--json",
            "--task",
            "TM-518",
            "--agent",
            "claude-code",
            "--since",
            "2h",
            "--limit",
            "5",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("run_list", &v);
    assert_eq!(v["data"]["runs"].as_array().unwrap().len(), 2);
    assert_eq!(v["data"]["runs"][0]["task"]["ref"], "TM-518");
    assert_eq!(v["data"]["runs"][0]["tests"]["passed"], 56);
    assert_eq!(v["data"]["next_cursor"], "c2");
    let req = &env.requests_to("/api/workspaces/1/agent-work").await[0];
    let q = req.url.query().unwrap();
    for part in ["task_id=8974", "agent=claude-code", "limit=5", "since="] {
        assert!(q.contains(part), "{q}");
    }
    assert_no_secrets(&out);
}

#[tokio::test(flavor = "multi_thread")]
async fn log_human_output_shows_runs_and_never_emits_server_escape_sequences() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    let mut run = run_json(1, 1, "failed");
    run["agent"] = json!("cl\u{1b}[31maude\u{1b}]0;pwned\u{7}");
    run["model"] = json!("op\u{1b}[2Jus");
    run["branch"] = json!("b\u{1b}[1mranch");
    run["summary"] = json!("did it\u{1b}[41m now");
    run["pr_url"] = json!("https://example.com/pr/1\u{1b}[0m");
    run["commits"] = json!([{"sha": "abc\u{1b}[7m", "message": "feat\u{9b}31m: x"}]);
    run["task"]["title"] = json!("TM-518: Up\u{1b}[31mload");
    mock_runs(&env, json!({"data": [run]})).await;
    let out = env.cmd().args(["log"]).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let s = text(&out.stdout);
    assert!(
        !s.contains('\u{1b}') && !s.contains('\u{9b}') && !s.contains('\u{7}'),
        "{s}"
    );
    assert!(s.contains("TM-518 Up [31mload"), "{s}");
    assert!(s.contains("commits 1"), "{s}");
    assert!(s.contains("tests 56 passed 0 failed"), "{s}");
    assert!(s.contains("failed 14m"), "{s}");
    assert!(s.contains("PR https://example.com/pr/1"), "{s}");
}

#[tokio::test(flavor = "multi_thread")]
async fn log_reports_a_disabled_agent_work_module_as_a_warning() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    Mock::given(method("GET"))
        .and(path("/api/workspaces/1/agent-work"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "message": "Agent work is disabled in this workspace",
            "error": "feature_disabled", "feature": "agent_work"
        })))
        .mount(&env.server)
        .await;
    let out = env.cmd().args(["log", "--json"]).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("run_list", &v);
    assert_eq!(v["data"]["runs"].as_array().unwrap().len(), 0);
    assert_eq!(v["warnings"][0]["code"], "agent_work_disabled");
}

#[tokio::test(flavor = "multi_thread")]
async fn log_rejects_bad_flags_before_any_request() {
    let env = Env::new().await;
    env.login_seed();
    for args in [
        vec!["log", "--since", "yesterday"],
        vec!["log", "--limit", "0"],
        vec!["log", "--limit", "999"],
        vec!["log", "--task", "../x"],
        vec!["log", "--agent", "a b"],
    ] {
        let out = env.cmd().args(&args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?} {}", text(&out.stderr));
    }
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn log_without_a_token_exits_4() {
    let env = Env::new().await;
    let out = env.cmd().args(["log"]).output().unwrap();
    assert_eq!(out.status.code(), Some(4));
}

fn spawn_follow(env: &Env, server: &str, extra: &[(&str, &str)]) -> std::process::Child {
    let mut c = env.std_cmd();
    c.env("TMGR_SERVER", server)
        .env("TMGR_FOLLOW_INTERVAL_MS", "100")
        .args(["log", "--follow", "--json"]);
    for (k, v) in extra {
        c.env(k, v);
    }
    c.spawn().unwrap()
}

fn read_events(child: &mut std::process::Child, want: usize) -> Vec<Value> {
    let out = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut events = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    while events.len() < want && Instant::now() < deadline {
        if let Ok(line) = rx.recv_timeout(Duration::from_millis(200)) {
            events.push(serde_json::from_str(&line).unwrap_or_else(|_| panic!("not JSON: {line}")));
        }
    }
    std::thread::sleep(Duration::from_millis(700));
    while let Ok(line) = rx.try_recv() {
        events.push(serde_json::from_str(&line).unwrap());
    }
    events
}

#[tokio::test(flavor = "multi_thread")]
async fn follow_emits_each_change_once_and_exits_130_on_sigint() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    Mock::given(method("GET"))
        .and(path("/api/workspaces/1/agent-work"))
        .and(header("authorization", format!("Bearer {ACCESS}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
            run_json(2, 1, "running"), run_json(1, 3, "succeeded")
        ]})))
        .up_to_n_times(1)
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/workspaces/1/agent-work"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
            run_json(2, 2, "succeeded"), run_json(1, 3, "succeeded")
        ]})))
        .mount(&env.server)
        .await;
    let mut child = spawn_follow(&env, &env.server.uri(), &[]);
    let events = read_events(&mut child, 3);
    let status = std::process::Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());
    let exit = child.wait().unwrap();
    assert_eq!(exit.code(), Some(130));
    assert_eq!(events.len(), 3, "{events:?}");
    for e in &events {
        assert_valid_event("run", e);
    }
    let seq: Vec<(u64, i64)> = events
        .iter()
        .map(|e| {
            (
                e["data"]["id"].as_u64().unwrap(),
                e["data"]["version"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(seq, vec![(1, 3), (2, 1), (2, 2)]);
    assert_eq!(events[0]["event"], "run.updated");
}

fn assert_valid_event(schema_name: &str, instance: &Value) {
    if let Err(e) = jsonschema::validate(&schema(schema_name), instance) {
        panic!("{schema_name} schema violation: {e}\n{instance}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn follow_gives_up_with_exit_7_after_the_server_stays_away() {
    let env = Env::new().await;
    let server = MockServer::builder().start().await;
    let account = format!("{}:default", server.address());
    env.seed_account(&account, ACCESS, Some(REFRESH));
    Mock::given(method("GET"))
        .and(path("/api/workspaces"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
            {"id": 1, "name": "Personal", "code": "personal", "is_default": true}
        ]})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/workspaces/1/agent-work"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"data": [run_json(1, 1, "running")]})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let uri = server.uri();
    let mut child = spawn_follow(&env, &uri, &[("TMGR_FOLLOW_OFFLINE_MS", "600")]);
    let out = child.stdout.take().unwrap();
    let mut reader = BufReader::new(out);
    let mut first = String::new();
    reader.read_line(&mut first).unwrap();
    assert!(first.contains("run.updated"), "{first}");
    drop(server);
    let mut rest = String::new();
    std::io::Read::read_to_string(&mut reader, &mut rest).unwrap();
    let exit = child.wait().unwrap();
    assert_eq!(exit.code(), Some(7), "{rest}");
    let v: Value = serde_json::from_str(rest.trim().lines().last().unwrap()).unwrap();
    assert_eq!(v["error"]["code"], "offline");
}

#[tokio::test(flavor = "multi_thread")]
async fn follow_stops_at_once_on_an_auth_error() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    Mock::given(method("GET"))
        .and(path("/api/workspaces/1/agent-work"))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(json!({"error": "insufficient_scope"})),
        )
        .mount(&env.server)
        .await;
    let out = env
        .cmd()
        .env("TMGR_FOLLOW_INTERVAL_MS", "50")
        .args(["log", "--follow", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    assert_eq!(stdout_json(&out)["error"]["code"], "forbidden");
}
