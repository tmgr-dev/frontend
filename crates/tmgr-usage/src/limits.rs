use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::Tool;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LimitWindow {
    Session,
    Weekly,
    Model(String),
}

impl LimitWindow {
    pub fn as_key(&self) -> String {
        match self {
            LimitWindow::Session => "session".into(),
            LimitWindow::Weekly => "weekly".into(),
            LimitWindow::Model(m) => format!("model:{m}"),
        }
    }

    pub fn from_key(key: &str) -> Option<LimitWindow> {
        match key {
            "session" => Some(LimitWindow::Session),
            "weekly" => Some(LimitWindow::Weekly),
            _ => key
                .strip_prefix("model:")
                .map(|m| LimitWindow::Model(m.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LimitSource {
    Rollout,
    Statusline,
}

impl LimitSource {
    pub fn as_str(self) -> &'static str {
        match self {
            LimitSource::Rollout => "rollout",
            LimitSource::Statusline => "statusline",
        }
    }

    pub fn from_key(key: &str) -> Option<LimitSource> {
        match key {
            "rollout" => Some(LimitSource::Rollout),
            "statusline" => Some(LimitSource::Statusline),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LimitSnapshot {
    pub tool: Tool,
    pub account: String,
    pub window: LimitWindow,
    pub used_pct: f64,
    pub resets_at: Option<DateTime<Utc>>,
    pub plan: Option<String>,
    pub observed_at: DateTime<Utc>,
    pub source: LimitSource,
    pub tool_session_id: String,
}
