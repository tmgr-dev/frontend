#![allow(dead_code)]
use chrono::{DateTime, TimeZone, Utc};
use std::path::{Path, PathBuf};
use tmgr_usage::{Account, PriceBook, Store, Tool};

pub const TEST_PRICES: &str = r#"{
  "claude-test-opus-1": {
    "mode": "chat",
    "input_cost_per_token": 1e-5,
    "output_cost_per_token": 5e-5,
    "cache_read_input_token_cost": 1e-6,
    "cache_creation_input_token_cost": 1.25e-5,
    "cache_creation_input_token_cost_above_1hr": 2e-5,
    "input_cost_per_token_above_200k_tokens": 2e-5,
    "output_cost_per_token_above_200k_tokens": 7.5e-5,
    "cache_read_input_token_cost_above_200k_tokens": 2e-6,
    "cache_creation_input_token_cost_above_200k_tokens": 2.5e-5,
    "cache_creation_input_token_cost_above_1hr_above_200k_tokens": 4e-5
  },
  "claude-test-sonnet-1": {
    "mode": "chat",
    "input_cost_per_token": 3e-6,
    "output_cost_per_token": 1.5e-5,
    "cache_read_input_token_cost": 3e-7,
    "cache_creation_input_token_cost": 3.75e-6,
    "cache_creation_input_token_cost_above_1hr": 6e-6
  }
}"#;

pub fn book() -> PriceBook {
    PriceBook::from_litellm_json(TEST_PRICES.as_bytes()).unwrap()
}

pub fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 14, 12, 0, 0).unwrap()
}

pub fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude_home")
}

pub fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), dest).unwrap();
        }
    }
}

pub fn claude_account(root: &Path) -> Account {
    Account {
        tool: Tool::Claude,
        label: "claude".into(),
        root: root.to_path_buf(),
    }
}

pub fn open_store(dir: &Path) -> Store {
    Store::open(&dir.join("usage.db")).unwrap()
}

pub fn write_lines(path: &Path, lines: &[String]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut text = lines.join("\n");
    text.push('\n');
    std::fs::write(path, text).unwrap();
}

pub fn assistant_line(session: &str, ts: &str, msg: &str, input: u64, output: u64) -> String {
    format!(
        r#"{{"type":"assistant","sessionId":"{session}","timestamp":"{ts}","cwd":"/work/alpha","gitBranch":"main","isSidechain":false,"requestId":"req-{msg}","message":{{"id":"{msg}","model":"claude-test-opus-1","content":[{{"type":"text","text":"x"}}],"usage":{{"input_tokens":{input},"output_tokens":{output},"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}}}}"#
    )
}

pub fn projects_file(root: &Path, name: &str) -> PathBuf {
    root.join("projects").join("-work-alpha").join(name)
}

pub fn ctx_fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude_ctx")
}
