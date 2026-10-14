use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::{Result, UsageError};
use crate::limits::LimitSnapshot;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RateWindow {
    pub used_pct: f64,
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatuslineSnapshot {
    pub session_id: String,
    pub config_dir_tag: Option<String>,
    pub model: Option<String>,
    pub five_hour: Option<RateWindow>,
    pub seven_day: Option<RateWindow>,
    pub cost_total_usd: Option<f64>,
    pub observed_at: DateTime<Utc>,
}

impl StatuslineSnapshot {
    pub fn to_limits(&self, _account: &str) -> Vec<LimitSnapshot> {
        Vec::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstallPlan {
    pub new_settings_text: String,
    pub installed_command: String,
    pub previous_command: Option<String>,
    pub warnings: Vec<String>,
}

pub fn write_snapshot(
    _dir: &Path,
    _config_dir_tag: Option<&str>,
    _stdin_json: &[u8],
) -> Result<StatuslineSnapshot> {
    Err(UsageError::Unsupported)
}

pub fn read_snapshots(_dir: &Path) -> Result<Vec<StatuslineSnapshot>> {
    Ok(Vec::new())
}

pub fn plan_install(_settings_text: &str, _exe: &str, _chain: Option<&str>) -> Result<InstallPlan> {
    Err(UsageError::Unsupported)
}

pub fn plan_uninstall(_settings_text: &str) -> Result<InstallPlan> {
    Err(UsageError::Unsupported)
}
