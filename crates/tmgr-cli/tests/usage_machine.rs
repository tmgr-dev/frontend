mod common;

use assert_cmd::Command;
use chrono::{Duration, Utc};
use common::{assert_valid, stdout_json, text};
use serde_json::{json, Value};
use std::path::PathBuf;
use tempfile::TempDir;
use tmgr_usage::limits::{LimitSnapshot, LimitSource, LimitWindow};
use tmgr_usage::{Store, Tool};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SECRETS: [&str; 3] = ["SECRET-PROMPT-1", "SECRET-RESPONSE-1", "SECRET-TOOL-1"];

struct M {
    tmp: TempDir,
    server: MockServer,
}

fn yesterday(clock: &str) -> String {
    format!("{}T{clock}Z", Utc::now().date_naive() - Duration::days(1))
}

fn assistant(session: &str, ts: &str, msg: &str, input: u64) -> String {
    format!(
        r#"{{"type":"assistant","sessionId":"{session}","timestamp":"{ts}","cwd":"/work/SECRET-CWD","gitBranch":"main","isSidechain":false,"requestId":"req-{msg}","message":{{"id":"{msg}","model":"claude-test-opus-1","content":[{{"type":"text","text":"SECRET-RESPONSE-1"}},{{"type":"tool_use","input":{{"command":"SECRET-TOOL-1"}}}}],"usage":{{"input_tokens":{input},"output_tokens":10,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}}}}"#
    )
}

fn user(session: &str, ts: &str) -> String {
    format!(
        r#"{{"type":"user","sessionId":"{session}","timestamp":"{ts}","cwd":"/work/SECRET-CWD","message":{{"role":"user","content":"SECRET-PROMPT-1"}}}}"#
    )
}

impl M {
    async fn new(put: ResponseTemplate) -> M {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/workspaces"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"data": [{"id": 1, "name": "Personal", "code": "personal", "is_default": true}]}),
            ))
            .mount(&server)
            .await;
        let run = json!({
            "id": 10, "task_id": 5, "agent": "claude-code", "session_id": "sess-a",
            "status": "succeeded", "started_at": yesterday("11:55:00"),
            "ended_at": yesterday("12:30:00"), "duration_seconds": 600,
            "task": {"id": 5, "key": "TM-5", "title": "TM-5: a title"}
        });
        Mock::given(method("GET"))
            .and(path("/api/workspaces/1/agent-work"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"runs": [run]})))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path_regex("^/api/ai-usage/machines/[0-9a-f-]{36}$"))
            .respond_with(put)
            .mount(&server)
            .await;
        let m = M {
            tmp: tempfile::tempdir().unwrap(),
            server,
        };
        for d in [
            "home",
            "claude",
            "codex",
            "data",
            "xdg",
            "state",
            "xdgdata",
            "tmgr-home/workspaces",
        ] {
            std::fs::create_dir_all(m.p(d)).unwrap();
        }
        m
    }

    fn p(&self, rel: &str) -> PathBuf {
        self.tmp.path().join(rel)
    }

    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("tmgr").unwrap();
        c.current_dir(self.p("home"))
            .env_clear()
            .env("HOME", self.p("home"))
            .env("XDG_DATA_HOME", self.p("xdgdata"))
            .env("XDG_CONFIG_HOME", self.p("xdg"))
            .env("XDG_STATE_HOME", self.p("state"))
            .env("TMGR_USAGE_DIR", self.p("data"))
            .env("TMGR_RUNNER_DIR", self.p("run"))
            .env("CLAUDE_CONFIG_DIR", self.p("claude"))
            .env("CODEX_HOME", self.p("codex"))
            .env("TMGR_LOCAL_ROOT", self.p("tmgr-home/workspaces"))
            .env("TMGR_CREDENTIAL_STORE", "file")
            .env("TMGR_SERVER", self.server.uri())
            .env("TZ", "UTC")
            .env("TMGR_USAGE_SYNC_WAIT_MS", "30")
            .env("NO_COLOR", "1");
        c
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        self.cmd().args(args).output().unwrap()
    }

    fn ok(&self, args: &[&str]) -> std::process::Output {
        let out = self.run(args);
        assert!(out.status.success(), "{args:?}: {}", text(&out.stderr));
        out
    }

    fn json(&self, args: &[&str]) -> Value {
        let mut all = vec!["--json"];
        all.extend_from_slice(args);
        stdout_json(&self.ok(&all))
    }

    fn login(&self) {
        let account = format!("{}:default", self.server.address());
        std::fs::create_dir_all(self.p("xdg/tmgr")).unwrap();
        let creds = self.p("xdg/tmgr/credentials.json");
        std::fs::write(
            &creds,
            json!({ account: {"access_token": "acc-token-AAAA1111", "refresh_token": "ref-token-BBBB2222", "expires_at": "2099-12-01T00:00:00Z"} })
                .to_string(),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&creds, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::set_permissions(self.p("xdg/tmgr"), std::fs::Permissions::from_mode(0o700))
            .unwrap();
    }

    fn session_file(&self, rel: &str, lines: &[String]) {
        let path = self.p("claude/projects").join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("{}\n", lines.join("\n"))).unwrap();
    }

    fn fixture(&self) {
        self.session_file(
            "-s/sess-a.jsonl",
            &[
                user("sess-a", &yesterday("11:59:00")),
                assistant("sess-a", &yesterday("12:00:00"), "a1", 1000),
                assistant("sess-a", &yesterday("12:05:00"), "a2", 2000),
            ],
        );
        self.session_file(
            "-s/sess-b.jsonl",
            &[assistant("sess-b", &yesterday("18:00:00"), "b1", 500)],
        );
    }

    fn store(&self) -> Store {
        Store::open(&self.p("data/usage.db")).unwrap()
    }

    async fn puts(&self) -> Vec<Value> {
        self.server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.method.as_str() == "PUT")
            .map(|r| serde_json::from_slice(&r.body).unwrap())
            .collect()
    }

    async fn requests(&self) -> usize {
        self.server.received_requests().await.unwrap().len()
    }
}

fn ok_put() -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_json(json!({"machine_id": "x", "days": 1, "limits": 1, "alerts": 0}))
}

fn without_clock(mut v: Value) -> Value {
    v.as_object_mut().unwrap().remove("generated_at");
    v
}

fn rows(body: &Value) -> Vec<Value> {
    body["days"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|d| d["rows"].as_array().unwrap().clone())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_manual_sync_sends_the_contract_body_and_is_idempotent() {
    let m = M::new(ok_put()).await;
    m.login();
    m.fixture();
    m.ok(&["usage", "machine", "label", "work-mac"]);
    m.store()
        .record_limit(&LimitSnapshot {
            tool: Tool::Claude,
            account: "/Users/someone/.claude".into(),
            window: LimitWindow::Weekly,
            used_pct: 42.5,
            resets_at: Some(Utc::now() + Duration::days(2)),
            plan: Some("max".into()),
            observed_at: Utc::now(),
            source: LimitSource::Statusline,
            tool_session_id: String::new(),
        })
        .unwrap();
    let out = m.ok(&["usage", "machine", "sync"]);
    assert!(
        text(&out.stdout).contains("numbers only"),
        "{}",
        text(&out.stdout)
    );
    let puts = m.puts().await;
    assert_eq!(puts.len(), 1);
    let body = &puts[0];
    assert_eq!(body["schema"], "machine_usage.v1");
    assert_eq!(body["label"], "work-mac");
    assert!(["macos", "linux", "windows", "other"].contains(&body["platform"].as_str().unwrap()));
    assert!(!body["cli_version"].as_str().unwrap().is_empty());
    let limit = &body["limits"][0];
    assert_eq!(limit["agent"], "claude-code");
    assert_eq!(limit["account"], "default");
    assert_eq!(limit["window"], "weekly");
    assert_eq!(limit["used_pct"], 42.5);
    assert_eq!(limit["source"], "statusline");
    assert!(body.get("reenable").is_none());
    let all = rows(body);
    let linked = all.iter().find(|r| r["task_id"] == 5).expect("linked row");
    assert_eq!(linked["agent"], "claude-code");
    assert_eq!(linked["model"], "claude-test-opus-1");
    assert_eq!(linked["input_tokens"], 3000);
    assert_eq!(linked["events"], 2);
    assert_eq!(linked["sessions"], 1);
    let unlinked = all
        .iter()
        .find(|r| r["task_id"].is_null())
        .expect("unlinked row");
    assert_eq!(unlinked["input_tokens"], 500);
    let text_body = body.to_string();
    for s in SECRETS
        .iter()
        .chain(&["/work", "SECRET-CWD", "/Users/someone"])
    {
        assert!(!text_body.contains(s), "{s} leaked");
    }

    m.ok(&["usage", "machine", "sync"]);
    let puts = m.puts().await;
    assert_eq!(puts.len(), 2);
    assert_eq!(
        without_clock(puts[0].clone()),
        without_clock(puts[1].clone())
    );
    let again = m.ok(&["usage", "machine", "sync"]);
    assert!(!text(&again.stdout).contains("numbers only"));
}

#[tokio::test(flavor = "multi_thread")]
async fn auto_sync_is_silent_throttled_and_needs_a_login() {
    let m = M::new(ok_put()).await;
    m.fixture();
    let v = m.json(&["usage", "machine", "sync", "--auto", "-q"]);
    assert_valid("usage_machine", &v);
    assert_eq!(v["data"]["reason"], "not_logged_in");
    assert_eq!(m.requests().await, 0);

    m.login();
    m.ok(&["usage", "machine", "sync"]);
    let before = m.puts().await.len();
    let v = m.json(&["usage", "machine", "sync", "--auto", "-q"]);
    assert_eq!(v["data"]["reason"], "throttled");
    assert_eq!(m.puts().await.len(), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn auto_sync_sends_two_days_and_a_limit_hit_bypasses_the_throttle() {
    let m = M::new(ok_put()).await;
    m.login();
    m.fixture();
    m.ok(&["usage", "consent", "allow"]);
    let hit = m.p("data/limit-hit-1.json");
    std::fs::write(
        &hit,
        json!({"profile": "claude-code", "line": "5-hour limit reached ∙ resets 3pm", "at_ms": Utc::now().timestamp_millis()})
            .to_string(),
    )
    .unwrap();
    m.ok(&["usage", "machine", "sync", "--auto", "-q"]);
    let first = m.puts().await;
    assert_eq!(first.len(), 1);
    assert!(!first[0]["days"].as_array().unwrap().is_empty());

    m.ok(&[
        "usage",
        "machine",
        "sync",
        "--auto",
        "-q",
        "--limit-hit-file",
        hit.to_str().unwrap(),
    ]);
    let puts = m.puts().await;
    assert_eq!(puts.len(), 2);
    let limit = puts[1]["limits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["source"] == "limit_hit")
        .expect("limit hit row");
    assert_eq!(limit["window"], "session");
    assert_eq!(limit["used_pct"], 100.0);
    assert_eq!(limit["agent"], "claude-code");
    assert!(!hit.exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn off_stops_sending_and_on_reenables_once() {
    let m = M::new(ok_put()).await;
    m.login();
    m.fixture();
    m.ok(&["usage", "machine", "off"]);
    let v = m.json(&["usage", "machine", "sync", "--auto", "-q"]);
    assert_eq!(v["data"]["reason"], "off");
    let refused = m.run(&["usage", "machine", "sync"]);
    assert!(!refused.status.success());
    assert!(text(&refused.stderr).contains("usage machine on"));
    assert_eq!(m.requests().await, 0);

    let v = m.json(&["usage", "machine"]);
    assert_valid("usage_machine", &v);
    assert_eq!(v["data"]["sync"], false);

    m.ok(&["usage", "machine", "on"]);
    m.ok(&["usage", "machine", "sync"]);
    m.ok(&["usage", "machine", "sync"]);
    let puts = m.puts().await;
    assert_eq!(puts[0]["reenable"], true);
    assert!(puts[1].get("reenable").is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_removed_machine_turns_sending_off_with_a_way_back() {
    let m =
        M::new(ResponseTemplate::new(410).set_body_json(json!({"error": "machine_removed"}))).await;
    m.login();
    m.fixture();
    let v = m.json(&["usage", "machine", "sync"]);
    assert_valid("usage_machine", &v);
    assert_eq!(v["data"]["removed"], true);
    let human = text(&m.ok(&["usage", "machine", "status"]).stdout);
    assert!(human.contains("off"), "{human}");
    let status = m.json(&["usage", "machine", "status"]);
    assert_eq!(status["data"]["sync"], false);
    let after = m.puts().await.len();
    m.ok(&["usage", "machine", "sync", "--auto", "-q"]);
    assert_eq!(m.puts().await.len(), after);
    let msg = m.run(&["usage", "machine", "sync"]);
    assert!(text(&msg.stderr).contains("usage machine on"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_label_must_be_one_to_sixty_four_characters() {
    let m = M::new(ok_put()).await;
    assert!(!m.run(&["usage", "machine", "label", ""]).status.success());
    assert!(!m
        .run(&["usage", "machine", "label", &"a".repeat(65)])
        .status
        .success());
    let v = m.json(&["usage", "machine", "label", "desk"]);
    assert_valid("usage_machine", &v);
    let status = m.json(&["usage", "machine", "status"]);
    assert_eq!(status["data"]["label"], "desk");
    assert_eq!(status["data"]["machine_id"].as_str().unwrap().len(), 36);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_forbidden_login_gets_the_scope_hint() {
    let m =
        M::new(ResponseTemplate::new(403).set_body_json(json!({"error": "insufficient_scope"})))
            .await;
    m.login();
    m.fixture();
    let out = m.run(&["usage", "machine", "sync"]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("agent-usage:write"),
        "{}",
        text(&out.stderr)
    );
}
