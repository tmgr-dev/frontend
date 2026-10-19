use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const STATUS_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Answers {
    pub rule: Option<String>,
    pub all_tasks: bool,
    pub max_concurrent: u32,
    pub usage_soft: u32,
    pub usage_hard: u32,
    pub resume_after_reset: bool,
    pub max_cost_usd: Option<f64>,
    pub max_hours: Option<f64>,
    pub max_tasks: Option<u32>,
    pub usage_guard: bool,
    pub feedback_status: String,
    pub blocked_status: String,
}

impl Default for Answers {
    fn default() -> Self {
        Answers {
            rule: None,
            all_tasks: true,
            max_concurrent: 2,
            usage_soft: 50,
            usage_hard: 80,
            resume_after_reset: false,
            max_cost_usd: None,
            max_hours: None,
            max_tasks: None,
            usage_guard: true,
            feedback_status: "Needs feedback".into(),
            blocked_status: "Blocked".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Finish {
    PrOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub answers: Answers,
    pub rule: String,
    pub agent: String,
    pub mode: String,
    pub finish: Finish,
    pub tasks: BTreeSet<i64>,
    pub signed: Vec<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Running,
    Paused,
    Stopping,
    Stopped,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskCounts {
    pub total: u32,
    pub done_pr: u32,
    pub in_work: u32,
    pub needs_feedback: u32,
    pub blocked: u32,
    pub held: u32,
    pub failed: u32,
    pub skipped: u32,
    pub queued: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Slots {
    pub running: u32,
    pub max: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ActiveRun {
    pub task: String,
    pub task_id: i64,
    pub title: String,
    pub mode: String,
    pub started_at: Option<DateTime<Utc>>,
    pub repos: Vec<String>,
    pub step: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LimitRow {
    pub tool: String,
    pub window: String,
    pub used_pct: Option<f64>,
    pub resets_at: Option<DateTime<Utc>>,
    pub source: String,
    pub origin: Option<String>,
    pub observed_at: Option<DateTime<Utc>>,
    pub stop_new_at: Option<u32>,
    pub pause_at: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cost {
    pub subscription_usd: f64,
    pub subscription_tokens: u64,
    pub api_usd: Option<f64>,
    pub api_cap_usd: Option<f64>,
    pub billing: String,
    pub basis: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NextStop {
    pub window: String,
    pub pct_left: f64,
    pub kind: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrLink {
    pub task: String,
    pub url: String,
    pub draft: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Event {
    pub at: Option<DateTime<Utc>>,
    pub kind: String,
    pub task: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusDoc {
    pub version: u32,
    pub state: RunState,
    pub pid: u32,
    pub control_seq: u64,
    pub settings_id: String,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub rule: String,
    pub mode: String,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub stopped_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub settings: Answers,
    #[serde(default)]
    pub tasks: TaskCounts,
    #[serde(default)]
    pub teams: Slots,
    #[serde(default)]
    pub agents: Slots,
    #[serde(default)]
    pub runs: Vec<ActiveRun>,
    #[serde(default)]
    pub limits: Vec<LimitRow>,
    #[serde(default)]
    pub cost: Cost,
    #[serde(default)]
    pub next_stop: Option<NextStop>,
    #[serde(default)]
    pub unconfirmed_new: u32,
    #[serde(default)]
    pub failures_in_row: u32,
    #[serde(default)]
    pub prs: Vec<PrLink>,
    #[serde(default)]
    pub events: Vec<Event>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reasoned {
    pub task: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Summary {
    pub text: String,
    pub stopped_by: String,
    pub started: u32,
    pub prs: Vec<PrLink>,
    pub held: Vec<Reasoned>,
    pub skipped: Vec<Reasoned>,
    pub failed: Vec<Reasoned>,
    pub paused: Vec<Reasoned>,
    pub unconfirmed_new: u32,
}
