use super::{Agent, Entry, Live};
use chrono::Utc;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tmgr_usage::consent::{ConsentChange, ReadLogs};
use tmgr_usage::paths::{db_path, Env};
use tmgr_usage::Store;

pub fn uid(n: u64) -> String {
    format!("{n:08x}-0000-4000-8000-{n:012x}")
}

pub fn entry(agent: Agent) -> Entry {
    Entry {
        agent,
        id: uid(1),
        path: PathBuf::from("/nowhere"),
        cwd: None,
        mtime_ms: 0,
        started_at: None,
        title: None,
        first_message: None,
        git_branch: None,
        project: None,
        readable: true,
        live: Live::No,
    }
}

pub struct Fx {
    pub tmp: tempfile::TempDir,
    pub env: Env,
}

impl Fx {
    pub fn new() -> Fx {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let env = Env {
            home: Some(home),
            vars: HashMap::from([(
                "TMGR_USAGE_DIR".to_string(),
                tmp.path().join("usage").display().to_string(),
            )]),
        };
        Fx { tmp, env }
    }

    pub fn home(&self) -> &Path {
        self.env.home.as_deref().unwrap()
    }

    pub fn consent(&self, state: Option<ReadLogs>, claude: Option<bool>, codex: Option<bool>) {
        let dir = self.tmp.path().join("usage");
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&db_path(&dir)).unwrap();
        store
            .change_log_consent(
                &ConsentChange {
                    state,
                    claude,
                    codex,
                },
                Utc::now(),
            )
            .unwrap();
    }

    pub fn grant(&self) {
        self.consent(Some(ReadLogs::Granted), None, None);
    }

    pub fn claude_root(&self) -> PathBuf {
        self.home().join(".claude")
    }

    pub fn codex_root(&self) -> PathBuf {
        self.home().join(".codex")
    }

    pub fn write(&self, path: &Path, body: &str) -> PathBuf {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
        path.to_path_buf()
    }

    pub fn claude(&self, project: &str, id: &str, lines: &[String]) -> PathBuf {
        let p = self
            .claude_root()
            .join("projects")
            .join(project)
            .join(format!("{id}.jsonl"));
        self.write(&p, &join(lines))
    }

    pub fn codex(&self, id: &str, lines: &[String]) -> PathBuf {
        let p = self
            .codex_root()
            .join("sessions/2026/11/05")
            .join(format!("rollout-2026-11-05T10-00-00-{id}.jsonl"));
        self.write(&p, &join(lines))
    }
}

fn join(lines: &[String]) -> String {
    lines.iter().map(|l| format!("{l}\n")).collect()
}

pub fn claude_user(cwd: &str, text: &str) -> String {
    serde_json::json!({"type":"user","sessionId":"x","timestamp":"2026-11-05T10:00:00Z","cwd":cwd,
        "gitBranch":"main","message":{"role":"user","content":text}})
    .to_string()
}

pub fn claude_assistant(text: &str) -> String {
    serde_json::json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":text}]}})
        .to_string()
}

pub fn codex_meta(id: &str, cwd: &str, source: serde_json::Value) -> String {
    serde_json::json!({"type":"session_meta","payload":{"id":id,"session_id":id,
        "timestamp":"2026-11-05T10:00:00Z","cwd":cwd,"source":source,"originator":"codex_cli",
        "cli_version":"0","git":{"branch":"dev"}}})
    .to_string()
}

pub fn codex_user(text: &str) -> String {
    serde_json::json!({"type":"event_msg","payload":{"type":"user_message","message":text}})
        .to_string()
}

pub fn codex_agent(text: &str) -> String {
    serde_json::json!({"type":"event_msg","payload":{"type":"agent_message","message":text}})
        .to_string()
}
