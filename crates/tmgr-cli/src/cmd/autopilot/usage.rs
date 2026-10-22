use chrono::{DateTime, Duration, Utc};
use tmgr_usage::cost::Billing;
use tmgr_usage::guard::{guard_limits, GuardLimit};
use tmgr_usage::limits::LimitSnapshot;
use tmgr_usage::{probe, Tool};

use super::model::LimitRow;
use crate::cmd::usage::Usage;

const MAX_AGE_MINUTES: i64 = 15;

pub trait Source {
    fn probe(&self, tool: Tool) -> Vec<LimitSnapshot>;
    fn stored(&self, now: DateTime<Utc>) -> Vec<LimitSnapshot>;
}

pub struct Real;

#[cfg(debug_assertions)]
pub fn injected(tool: Tool) -> Option<(Vec<LimitSnapshot>, Billing)> {
    use tmgr_usage::limits::{LimitSource, LimitWindow};
    let path = std::env::var_os("TMGR_TEST_AUTOPILOT_USAGE")?;
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let now = Utc::now();
    let snap = |window: LimitWindow, key: &str| {
        v[key].as_f64().map(|used_pct| LimitSnapshot {
            tool,
            account: "test".into(),
            window,
            used_pct,
            resets_at: Some(now + Duration::hours(2)),
            plan: None,
            observed_at: now,
            source: LimitSource::Probe,
            tool_session_id: String::new(),
        })
    };
    let windows = [
        snap(LimitWindow::Session, "session"),
        snap(LimitWindow::Weekly, "weekly"),
    ]
    .into_iter()
    .flatten()
    .collect();
    let billing = match v["billing"].as_str() {
        Some("api_key") => Billing::ApiKey,
        Some("subscription") => Billing::Subscription,
        _ => Billing::Unknown,
    };
    Some((windows, billing))
}

#[cfg(not(debug_assertions))]
pub fn injected(_: Tool) -> Option<(Vec<LimitSnapshot>, Billing)> {
    None
}

impl Source for Real {
    fn probe(&self, tool: Tool) -> Vec<LimitSnapshot> {
        if let Some((windows, _)) = injected(tool) {
            return windows;
        }
        let Ok(bin) = tmgr_queue::rules::resolve_exe(tool.as_str()) else {
            return Vec::new();
        };
        let windows = match tool {
            Tool::Claude => probe::claude_usage(&bin, probe::DEFAULT_TIMEOUT).map(|p| p.windows),
            Tool::Codex => probe::codex_usage(&bin, probe::DEFAULT_TIMEOUT).map(|p| p.windows),
        };
        windows.unwrap_or_default()
    }

    fn stored(&self, now: DateTime<Utc>) -> Vec<LimitSnapshot> {
        Usage::open()
            .and_then(|u| u.store.current_limits(now).map_err(crate::cmd::usage::fail))
            .unwrap_or_default()
    }
}

pub fn read(source: &dyn Source, tool: Tool, now: DateTime<Utc>) -> Vec<GuardLimit> {
    let mut all = source.probe(tool);
    all.extend(source.stored(now));
    guard_limits(&all, now, Duration::minutes(MAX_AGE_MINUTES))
        .into_iter()
        .filter(|l| l.tool == tool && matches!(l.window.as_str(), "five_hour" | "seven_day"))
        .collect()
}

pub fn has_real_number(rows: &[GuardLimit]) -> bool {
    rows.iter().any(|l| l.used_pct.is_some())
}

pub fn rows(limits: &[GuardLimit], soft: Option<u32>, hard: Option<u32>) -> Vec<LimitRow> {
    limits
        .iter()
        .map(|l| LimitRow {
            tool: l.tool.as_str().into(),
            window: l.window.clone(),
            used_pct: l.used_pct,
            resets_at: l.resets_at,
            source: l.source.as_str().into(),
            origin: Some(l.origin.into()),
            observed_at: l.observed_at,
            stop_new_at: soft,
            pause_at: hard,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use tmgr_usage::limits::{LimitSource, LimitWindow};

    struct Fake {
        probe: Vec<LimitSnapshot>,
        stored: Vec<LimitSnapshot>,
    }

    impl Source for Fake {
        fn probe(&self, _: Tool) -> Vec<LimitSnapshot> {
            self.probe.clone()
        }
        fn stored(&self, _: DateTime<Utc>) -> Vec<LimitSnapshot> {
            self.stored.clone()
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 19, 20, 0, 0).unwrap()
    }

    fn snap(tool: Tool, w: LimitWindow, pct: f64, age_min: i64, src: LimitSource) -> LimitSnapshot {
        LimitSnapshot {
            tool,
            account: "a".into(),
            window: w,
            used_pct: pct,
            resets_at: Some(now() + Duration::hours(1)),
            plan: None,
            observed_at: now() - Duration::minutes(age_min),
            source: src,
            tool_session_id: String::new(),
        }
    }

    #[test]
    fn a_fresh_probe_gives_a_real_number() {
        let f = Fake {
            probe: vec![snap(
                Tool::Claude,
                LimitWindow::Session,
                41.0,
                0,
                LimitSource::Probe,
            )],
            stored: vec![],
        };
        let rows = read(&f, Tool::Claude, now());
        assert!(has_real_number(&rows));
        assert_eq!(rows[0].used_pct, Some(41.0));
        assert_eq!(rows[0].origin, "get_usage");
    }

    #[test]
    fn stored_snapshots_count_only_when_fresh() {
        let fresh = Fake {
            probe: vec![],
            stored: vec![snap(
                Tool::Claude,
                LimitWindow::Session,
                30.0,
                10,
                LimitSource::Statusline,
            )],
        };
        assert!(has_real_number(&read(&fresh, Tool::Claude, now())));
        let stale = Fake {
            probe: vec![],
            stored: vec![snap(
                Tool::Claude,
                LimitWindow::Session,
                30.0,
                16,
                LimitSource::Statusline,
            )],
        };
        let rows = read(&stale, Tool::Claude, now());
        assert!(!has_real_number(&rows));
        assert_eq!(rows[0].used_pct, None);
    }

    #[test]
    fn only_the_tool_of_the_rule_is_looked_at() {
        let f = Fake {
            probe: vec![],
            stored: vec![snap(
                Tool::Codex,
                LimitWindow::Session,
                10.0,
                1,
                LimitSource::Rollout,
            )],
        };
        assert!(read(&f, Tool::Claude, now()).is_empty());
        assert!(has_real_number(&read(&f, Tool::Codex, now())));
    }
}
