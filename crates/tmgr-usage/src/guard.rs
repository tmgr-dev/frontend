use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use std::collections::BTreeMap;

use crate::limits::{LimitSnapshot, LimitSource, LimitWindow};
use crate::model::Tool;

const CLOCK_SKEW: i64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GuardSource {
    Real,
    Estimate,
    Unknown,
}

impl GuardSource {
    pub fn as_str(self) -> &'static str {
        match self {
            GuardSource::Real => "real",
            GuardSource::Estimate => "estimate",
            GuardSource::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LastSeen {
    pub used_pct: f64,
    pub resets_at: Option<DateTime<Utc>>,
}

/// One row of the usage guard. `used_pct` is set only for a real reading that is fresh and whose
/// window has not reset; otherwise it is `None`, `source` is `unknown`, and the old number stays in
/// `last_seen` for display only.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GuardLimit {
    pub tool: Tool,
    pub window: String,
    pub used_pct: Option<f64>,
    pub resets_at: Option<DateTime<Utc>>,
    pub source: GuardSource,
    pub origin: &'static str,
    pub observed_at: Option<DateTime<Utc>>,
    pub unknown_reason: Option<&'static str>,
    pub last_seen: Option<LastSeen>,
}

pub fn window_name(window: &LimitWindow) -> String {
    match window {
        LimitWindow::Session => "five_hour".into(),
        LimitWindow::Weekly => "seven_day".into(),
        LimitWindow::Model(m) => format!("model:{m}"),
    }
}

fn origin(snap: &LimitSnapshot) -> &'static str {
    match (snap.source, snap.tool) {
        (LimitSource::Probe, Tool::Claude) => "get_usage",
        (LimitSource::Probe, Tool::Codex) => "app_server",
        (LimitSource::Statusline, _) => "statusline",
        (LimitSource::Rollout, _) => "rollout",
    }
}

fn fresher(candidate: &LimitSnapshot, best: &LimitSnapshot) -> bool {
    candidate.observed_at > best.observed_at
        || (candidate.observed_at == best.observed_at && candidate.used_pct > best.used_pct)
}

pub fn guard_limits(
    snapshots: &[LimitSnapshot],
    now: DateTime<Utc>,
    max_age: Duration,
) -> Vec<GuardLimit> {
    let mut best: BTreeMap<(Tool, String), &LimitSnapshot> = BTreeMap::new();
    for snap in snapshots {
        let key = (snap.tool, window_name(&snap.window));
        match best.get(&key) {
            Some(current) if !fresher(snap, current) => {}
            _ => {
                best.insert(key, snap);
            }
        }
    }
    best.into_iter()
        .map(|((tool, window), snap)| {
            let reason = if snap.resets_at.is_some_and(|r| r <= now) {
                Some("window_reset")
            } else if snap.observed_at > now + Duration::seconds(CLOCK_SKEW) {
                Some("clock")
            } else if now - snap.observed_at > max_age {
                Some("stale")
            } else if !snap.used_pct.is_finite() {
                Some("invalid")
            } else {
                None
            };
            let usable = reason.is_none();
            GuardLimit {
                tool,
                window,
                used_pct: usable.then_some(snap.used_pct),
                resets_at: if usable { snap.resets_at } else { None },
                source: if usable {
                    GuardSource::Real
                } else {
                    GuardSource::Unknown
                },
                origin: origin(snap),
                observed_at: Some(snap.observed_at),
                unknown_reason: reason,
                last_seen: (!usable && snap.used_pct.is_finite()).then_some(LastSeen {
                    used_pct: snap.used_pct,
                    resets_at: snap.resets_at,
                }),
            }
        })
        .collect()
}
