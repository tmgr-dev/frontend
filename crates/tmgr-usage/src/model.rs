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
    pub agent_id: Option<String>,
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
    Fact(Fact),
    FactEnd(FactEnd),
    Agent(AgentMeta),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactKind {
    ToolCall,
    Skill,
    SkillListing,
    Hook,
    Turn,
    Away,
    Compact,
    PermissionMode,
    Pr,
    FileEdit,
    Slash,
}

impl FactKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FactKind::ToolCall => "tool_call",
            FactKind::Skill => "skill",
            FactKind::SkillListing => "skill_listing",
            FactKind::Hook => "hook",
            FactKind::Turn => "turn",
            FactKind::Away => "away",
            FactKind::Compact => "compact",
            FactKind::PermissionMode => "permission_mode",
            FactKind::Pr => "pr",
            FactKind::FileEdit => "file_edit",
            FactKind::Slash => "slash",
        }
    }

    pub fn parse(s: &str) -> Option<FactKind> {
        Some(match s {
            "tool_call" => FactKind::ToolCall,
            "skill" => FactKind::Skill,
            "skill_listing" => FactKind::SkillListing,
            "hook" => FactKind::Hook,
            "turn" => FactKind::Turn,
            "away" => FactKind::Away,
            "compact" => FactKind::Compact,
            "permission_mode" => FactKind::PermissionMode,
            "pr" => FactKind::Pr,
            "file_edit" => FactKind::FileEdit,
            "slash" => FactKind::Slash,
            _ => return None,
        })
    }
}

/// One observed fact of agent work: names, numbers and times only, never text, arguments or paths.
/// `key` is unique per tool, so a re-read of the same log line updates the row instead of adding one.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Fact {
    pub account: String,
    pub key: String,
    pub tool_session_id: String,
    pub agent_id: Option<String>,
    pub ts: DateTime<Utc>,
    pub kind: Option<FactKind>,
    pub name: String,
    pub detail: Option<String>,
    pub target: Option<String>,
    pub duration_ms: Option<u64>,
    pub value: Option<u64>,
    pub is_error: bool,
    pub git_branch: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FactEnd {
    pub key: String,
    pub ended_at: DateTime<Utc>,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AgentMeta {
    pub account: String,
    pub tool_session_id: String,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub parent_agent_id: Option<String>,
    pub spawn_tool_use_id: Option<String>,
    pub spawn_depth: Option<u32>,
    pub originator: Option<String>,
    pub source: Option<String>,
    pub version: Option<String>,
    pub entrypoint: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub cwd: Option<String>,
}

/// Keeps a name to `[A-Za-z0-9._:+-]`, at most `max` characters, never starting with `.` or `~`
/// and never containing `..`, so a path, an email or free text cannot pass as a name.
pub fn clean_name(raw: &str, max: usize) -> String {
    let mut out: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '+' | '-') {
                c
            } else {
                '_'
            }
        })
        .take(max)
        .collect();
    while out.contains("..") {
        out = out.replace("..", "_");
    }
    while out.starts_with('.') {
        out.replace_range(..1, "_");
    }
    out
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
