use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::orchestrate::ModeSetting;

pub const STATUS_VERSION: u32 = 1;
pub const QUEUE_SETTINGS_ID: &str = "queue";

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
    pub max_teams: u32,
    pub team_size: u32,
    pub team_mode: ModeSetting,
    pub profile: Option<String>,
    pub fallback_hours: f64,
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
            max_teams: 2,
            team_size: 3,
            team_mode: ModeSetting::Orchestrator,
            profile: None,
            fallback_hours: 8.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Finish {
    PrOnly,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopKind {
    #[default]
    Drain,
    Queue,
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
    #[serde(default)]
    pub windows: Vec<String>,
    #[serde(default)]
    pub kind: LoopKind,
    #[serde(default)]
    pub resume_queue: bool,
    #[serde(default)]
    pub order: Vec<i64>,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub persona_optional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Running,
    Paused,
    Stopping,
    Stopped,
    Suspended,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<PlanDoc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triage: Vec<TriageRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_planned: Vec<NotPlanned>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents_planned: Option<Slots>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub needs_feedback_tasks: Option<Vec<TaskBrief>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_tasks: Option<Vec<TaskBrief>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queued_tasks: Option<Vec<TaskBrief>>,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanState {
    #[default]
    Pending,
    Running,
    Done,
    Rejected,
    Skipped,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanRow {
    pub task: String,
    pub task_id: i64,
    pub position: u32,
    pub group: Option<String>,
    pub mode: String,
    pub reason: String,
    pub state: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanDoc {
    pub state: PlanState,
    pub dry_run: bool,
    pub profile: String,
    pub made_at: Option<DateTime<Utc>>,
    pub attempt: u32,
    pub error: Option<String>,
    pub items: Vec<PlanRow>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TriageRow {
    pub task: String,
    pub task_id: i64,
    pub verdict: String,
    pub reason: String,
    pub question: String,
    pub applied: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotPlanned {
    pub task: String,
    pub task_id: i64,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskBrief {
    pub key: String,
    pub id: i64,
    pub title: String,
    pub workspace_code: String,
}
