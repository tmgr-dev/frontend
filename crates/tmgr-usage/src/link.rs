use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::Tool;

#[derive(Debug, Clone, PartialEq)]
pub struct RunWindow {
    pub run_ref: String,
    pub task_ref: String,
    pub workspace: String,
    pub session_id: Option<String>,
    pub branch: Option<String>,
    pub project_hash: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkEvent {
    pub id: i64,
    pub tool: Tool,
    pub account: String,
    pub tool_session_id: String,
    pub ts: DateTime<Utc>,
    pub model: String,
    pub git_branch: Option<String>,
    pub project_hash: Option<String>,
    pub cost_usd: Option<f64>,
    pub tokens: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkMethod {
    SessionId,
    BranchWindow,
    ProjectWindow,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub event_index: usize,
    pub run_ref: String,
    pub task_ref: String,
    pub method: LinkMethod,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ambiguous {
    pub event_index: usize,
    pub candidates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LinkReport {
    pub links: Vec<Link>,
    pub ambiguous: Vec<Ambiguous>,
    pub unattributed: Vec<usize>,
}

pub fn attribute(_events: &[LinkEvent], _runs: &[RunWindow]) -> LinkReport {
    LinkReport::default()
}
