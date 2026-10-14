use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

use crate::error::{Result, UsageError};
use crate::limits::LimitSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Claude,
    Codex,
}

impl Tool {
    pub fn as_str(self) -> &'static str {
        match self {
            Tool::Claude => "claude",
            Tool::Codex => "codex",
        }
    }

    pub fn parse(s: &str) -> Result<Tool> {
        match s {
            "claude" => Ok(Tool::Claude),
            "codex" => Ok(Tool::Codex),
            _ => Err(UsageError::Invalid("unknown tool".into())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub tool: Tool,
    pub label: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub reasoning: u64,
}

impl Tokens {
    pub fn prompt_total(&self) -> u64 {
        self.input + self.cache_read + self.cache_write_5m + self.cache_write_1h
    }

    pub fn add(&mut self, o: &Tokens) {
        self.input += o.input;
        self.output += o.output;
        self.cache_read += o.cache_read;
        self.cache_write_5m += o.cache_write_5m;
        self.cache_write_1h += o.cache_write_1h;
        self.reasoning += o.reasoning;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CostBasis {
    Computed,
    Unpriced,
}

impl CostBasis {
    pub fn as_str(self) -> &'static str {
        match self {
            CostBasis::Computed => "computed",
            CostBasis::Unpriced => "unpriced",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageEvent {
    pub account: String,
    pub tool_session_id: String,
    pub event_key: String,
    pub ts: DateTime<Utc>,
    pub model: String,
    pub tokens: Tokens,
    pub speed: Option<String>,
    pub service_tier: Option<String>,
    pub git_branch: Option<String>,
    pub cwd: Option<String>,
    pub is_sidechain: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SessionMeta {
    pub account: String,
    pub tool_session_id: String,
    pub started_at: Option<DateTime<Utc>>,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub parent_session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedRecord {
    Event(UsageEvent),
    Session(SessionMeta),
    Limit(LimitSnapshot),
}

pub fn project_name(cwd: &str) -> String {
    cwd.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .to_string()
}

pub fn project_hash(cwd: &str) -> String {
    hex(&Sha256::digest(cwd.as_bytes()))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
