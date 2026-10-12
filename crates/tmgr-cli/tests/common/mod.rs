#![allow(dead_code)]
use assert_cmd::Command;
use serde_json::{json, Value};
use std::path::PathBuf;
use tempfile::TempDir;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub const ACCESS: &str = "acc-token-AAAA1111";
pub const REFRESH: &str = "ref-token-BBBB2222";

pub struct Env {
    pub tmp: TempDir,
    pub server: MockServer,
}

impl Env {
    pub async fn new() -> Env {
        Env {
            tmp: tempfile::tempdir().unwrap(),
            server: MockServer::start().await,
        }
    }

    pub fn xdg(&self) -> PathBuf {
        self.tmp.path().join("xdg")
    }

    pub fn config_dir(&self) -> PathBuf {
        self.xdg().join("tmgr")
    }

    pub fn creds_path(&self) -> PathBuf {
        self.config_dir().join("credentials.json")
    }

    pub fn account(&self) -> String {
        format!("{}:default", self.server.address())
    }

    pub fn cmd_for(&self, server: &str) -> Command {
        let mut c = Command::cargo_bin("tmgr").unwrap();
        c.env_clear()
            .env("HOME", self.tmp.path())
            .env("XDG_CONFIG_HOME", self.xdg())
            .env("TMGR_CREDENTIAL_STORE", "file")
            .env("TMGR_SERVER", server)
            .env("NO_COLOR", "1");
        c
    }

    pub fn std_cmd(&self) -> std::process::Command {
        let mut c = std::process::Command::new(assert_cmd::cargo::cargo_bin("tmgr"));
        c.env_clear()
            .env("HOME", self.tmp.path())
            .env("XDG_CONFIG_HOME", self.xdg())
            .env("TMGR_CREDENTIAL_STORE", "file")
            .env("TMGR_SERVER", self.server.uri())
            .env("NO_COLOR", "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        c
    }

    pub fn cmd(&self) -> Command {
        self.cmd_for(&self.server.uri())
    }

    pub fn seed_tokens(&self, access: &str, refresh: Option<&str>) {
        self.seed_account(&self.account(), access, refresh);
    }

    pub fn seed_account(&self, account: &str, access: &str, refresh: Option<&str>) {
        use std::io::Write;
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(self.config_dir())
            .unwrap();
        let mut entry = json!({"access_token": access, "expires_at": "2026-12-01T00:00:00Z"});
        if let Some(r) = refresh {
            entry["refresh_token"] = json!(r);
        }
        let body = json!({ account: entry });
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(self.creds_path())
            .unwrap();
        f.write_all(body.to_string().as_bytes()).unwrap();
    }

    pub fn login_seed(&self) {
        self.seed_tokens(ACCESS, Some(REFRESH));
    }

    pub async fn mock_json(&self, p: &str, body: Value) {
        Mock::given(method("GET"))
            .and(path(p))
            .and(header("authorization", format!("Bearer {ACCESS}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&self.server)
            .await;
    }

    pub async fn mock_workspaces(&self) {
        self.mock_json(
            "/api/workspaces",
            json!({"data": [
                {"id": 1, "name": "Personal", "code": "personal", "is_default": true},
                {"id": 2, "name": "Work", "code": "work", "is_default": false}
            ]}),
        )
        .await;
    }

    pub async fn mock_statuses(&self) {
        self.mock_json(
            "/api/workspaces/1/statuses",
            json!([
                {"id": 10, "name": "Backlog", "type": "default"},
                {"id": 11, "name": "In progress", "type": "active"},
                {"id": 12, "name": "Done", "type": "completed"}
            ]),
        )
        .await;
    }

    pub async fn mock_categories(&self) {
        Mock::given(method("GET"))
            .and(path("/api/project_categories"))
            .and(query_param("workspace_id", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": 5, "title": "Tmgr", "code": "TM"}
            ])))
            .mount(&self.server)
            .await;
    }

    pub async fn requests_to(&self, p: &str) -> Vec<wiremock::Request> {
        self.server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.url.path() == p)
            .collect()
    }

    pub fn creds_text(&self) -> String {
        std::fs::read_to_string(self.creds_path()).unwrap_or_default()
    }
}

pub fn task_json(id: u64, title: &str) -> Value {
    json!({
        "id": id,
        "title": title,
        "description": "# Plan\nDo **it** with [docs](https://example.com/d)",
        "status": "active",
        "status_id": 11,
        "project_category_id": 5,
        "category_tasks_sequence_id": 342,
        "category": {"id": 5, "title": "Tmgr", "code": "TM"},
        "workspace_id": 1,
        "common_time": 4320,
        "approximately_time": 14400,
        "priority": "high",
        "assignees": [{"id": 3, "name": "Ann", "email": "ann@example.com"}],
        "created_at": "2026-10-12T09:01:00Z",
        "updated_at": "2026-10-12T10:00:00Z"
    })
}

pub fn schema(name: &str) -> Value {
    let p = format!("{}/schemas/tmgr.v1/{name}.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

pub fn assert_valid(name: &str, instance: &Value) {
    if let Err(e) = jsonschema::validate(&schema(name), instance) {
        panic!("{name} schema violation: {e}\n{instance}");
    }
}

pub fn stdout_json(out: &std::process::Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {}",
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

pub fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

pub fn assert_no_secrets(out: &std::process::Output) {
    let all = format!("{}{}", text(&out.stdout), text(&out.stderr));
    for secret in [
        ACCESS,
        REFRESH,
        "new-access-CCCC",
        "new-refresh-DDDD",
        "dev-code-SECRET",
    ] {
        assert!(!all.contains(secret), "output leaked {secret}");
    }
}

pub fn dead_server() -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    drop(l);
    format!("http://{addr}")
}
