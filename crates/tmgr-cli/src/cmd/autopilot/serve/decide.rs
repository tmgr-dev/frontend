use chrono::{DateTime, Duration, Utc};
use tmgr_usage::cost::Billing;
use tmgr_usage::guard::GuardLimit;
use tmgr_usage::Tool;

pub const MAX_FAILURES: u32 = 3;
pub const HOURS_GRACE_MINUTES: i64 = 30;
pub const UNKNOWN_STOP_MINUTES: i64 = 15;

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub window: String,
    pub pct: f64,
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Blocker {
    Paused,
    Limit(Hit),
    UsageUnknown,
    CostCap,
    MaxTasks,
    Hours,
    Failures,
}

pub struct Inputs<'a> {
    pub now: DateTime<Utc>,
    pub started_at: DateTime<Utc>,
    pub paused: bool,
    pub soft: u32,
    pub hard: u32,
    pub tool: Option<Tool>,
    pub limits: &'a [GuardLimit],
    pub billing: Billing,
    pub api_usd: Option<f64>,
    pub cost_cap: Option<f64>,
    pub started: u32,
    pub max_tasks: Option<u32>,
    pub max_hours: Option<f64>,
    pub failures: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    pub blockers: Vec<Blocker>,
    pub hard_limit: Option<Hit>,
    pub hours_hard: bool,
    pub deadline: Option<DateTime<Utc>>,
}

impl Decision {
    pub fn starts_allowed(&self) -> bool {
        self.blockers.is_empty()
    }

    pub fn unknown(&self) -> bool {
        self.blockers.contains(&Blocker::UsageUnknown)
    }

    pub fn limit_hit(&self) -> Option<&Hit> {
        self.blockers.iter().find_map(|b| match b {
            Blocker::Limit(h) => Some(h),
            _ => None,
        })
    }
}

const CLAUDE_WINDOWS: [&str; 2] = ["five_hour", "seven_day"];

fn is_model_window(w: &str) -> bool {
    w.starts_with("model:")
}

pub fn deadline(started_at: DateTime<Utc>, max_hours: Option<f64>) -> Option<DateTime<Utc>> {
    max_hours
        .filter(|h| h.is_finite() && *h > 0.0)
        .map(|h| started_at + Duration::milliseconds((h * 3_600_000.0) as i64))
}

fn worst(hits: impl Iterator<Item = Hit>) -> Option<Hit> {
    hits.max_by(|a, b| a.pct.total_cmp(&b.pct))
}

pub fn decide(i: &Inputs) -> Decision {
    let mut blockers = Vec::new();
    if i.paused {
        blockers.push(Blocker::Paused);
    }
    let mut hard_limit = None;
    if i.billing != Billing::ApiKey && i.tool.is_some() {
        let rows: Vec<&GuardLimit> = i
            .limits
            .iter()
            .filter(|l| i.tool.is_none_or(|t| l.tool == t))
            .collect();
        let hit_at = |floor: u32| {
            worst(rows.iter().filter_map(|l| {
                l.used_pct.filter(|p| *p >= f64::from(floor)).map(|p| Hit {
                    window: l.window.clone(),
                    pct: p,
                    resets_at: l.resets_at,
                })
            }))
        };
        hard_limit = hit_at(i.hard);
        if let Some(hit) = hit_at(i.soft) {
            blockers.push(Blocker::Limit(hit));
        }
        let unknown_row = rows
            .iter()
            .any(|l| l.used_pct.is_none() && !is_model_window(&l.window));
        let missing = i.tool == Some(Tool::Claude)
            && CLAUDE_WINDOWS
                .iter()
                .any(|w| !rows.iter().any(|l| l.window == *w));
        if rows.is_empty() || unknown_row || missing {
            blockers.push(Blocker::UsageUnknown);
        }
    }
    if let (Some(cap), Some(spent)) = (i.cost_cap, i.api_usd) {
        if spent >= cap {
            blockers.push(Blocker::CostCap);
        }
    }
    if i.max_tasks.is_some_and(|m| i.started >= m) {
        blockers.push(Blocker::MaxTasks);
    }
    let deadline = deadline(i.started_at, i.max_hours);
    let mut hours_hard = false;
    if let Some(d) = deadline {
        if i.now >= d {
            blockers.push(Blocker::Hours);
        }
        hours_hard = i.now >= d + Duration::minutes(HOURS_GRACE_MINUTES);
    }
    if i.failures >= MAX_FAILURES {
        blockers.push(Blocker::Failures);
    }
    Decision {
        blockers,
        hard_limit,
        hours_hard,
        deadline,
    }
}

pub struct Facts {
    pub live: usize,
    pub awaiting_publish: usize,
    pub eligible: usize,
    pub settled: bool,
    pub resume_after_reset: bool,
    pub unknown_for: Option<Duration>,
    pub hard: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StopReason {
    NothingLeft,
    SoftLimit(Hit),
    HardLimit(Hit),
    UsageUnknown,
    CostCap,
    MaxTasks,
    Hours,
    Failures,
    Requested(String),
}

impl StopReason {
    pub fn text(&self) -> String {
        match self {
            StopReason::NothingLeft => "nothing left to do".into(),
            StopReason::SoftLimit(h) => {
                format!("usage limit reached: {} at {:.0}%", h.window, h.pct)
            }
            StopReason::HardLimit(h) => {
                format!("hard usage limit reached: {} at {:.0}%", h.window, h.pct)
            }
            StopReason::UsageUnknown => "no fresh usage numbers for 15 minutes".into(),
            StopReason::CostCap => "cost cap reached".into(),
            StopReason::MaxTasks => "task limit reached".into(),
            StopReason::Hours => "time limit reached".into(),
            StopReason::Failures => "3 failures in a row".into(),
            StopReason::Requested(why) => why.clone(),
        }
    }
}

pub fn verdict(d: &Decision, f: &Facts, now: DateTime<Utc>) -> Option<StopReason> {
    if d.hours_hard {
        return Some(StopReason::Hours);
    }
    if f.live > 0 || f.awaiting_publish > 0 {
        return None;
    }
    if d.blockers.contains(&Blocker::Failures) {
        return Some(StopReason::Failures);
    }
    if f.settled && f.eligible == 0 && !d.blockers.contains(&Blocker::Paused) {
        return Some(StopReason::NothingLeft);
    }
    for b in &d.blockers {
        match b {
            Blocker::Paused => {}
            Blocker::Hours => return Some(StopReason::Hours),
            Blocker::Failures => return Some(StopReason::Failures),
            Blocker::CostCap => return Some(StopReason::CostCap),
            Blocker::MaxTasks => return Some(StopReason::MaxTasks),
            Blocker::Limit(hit) => {
                let waits = f.resume_after_reset && hit.resets_at.is_some_and(|r| r > now);
                if !waits {
                    return Some(if hit.pct >= f64::from(f.hard) {
                        StopReason::HardLimit(hit.clone())
                    } else {
                        StopReason::SoftLimit(hit.clone())
                    });
                }
            }
            Blocker::UsageUnknown => {
                if f.unknown_for
                    .is_some_and(|u| u >= Duration::minutes(UNKNOWN_STOP_MINUTES))
                {
                    return Some(StopReason::UsageUnknown);
                }
            }
        }
    }
    None
}

#[derive(Clone, Debug, PartialEq)]
pub struct NextStop {
    pub window: String,
    pub pct_left: f64,
    pub kind: &'static str,
}

pub fn next_stop(limits: &[GuardLimit], soft: u32, hard: u32) -> Option<NextStop> {
    let left = |floor: u32| {
        limits
            .iter()
            .filter_map(|l| l.used_pct.map(|p| (l.window.clone(), f64::from(floor) - p)))
            .filter(|(_, left)| *left > 0.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
    };
    left(soft)
        .map(|(window, pct_left)| NextStop {
            window,
            pct_left,
            kind: "no new tasks",
        })
        .or_else(|| {
            left(hard).map(|(window, pct_left)| NextStop {
                window,
                pct_left,
                kind: "pause runs",
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_usage::guard::GuardSource;

    fn now() -> DateTime<Utc> {
        "2026-10-19T12:00:00Z".parse().unwrap()
    }

    fn row(window: &str, pct: Option<f64>) -> GuardLimit {
        GuardLimit {
            tool: Tool::Claude,
            window: window.into(),
            used_pct: pct,
            resets_at: pct.map(|_| now() + Duration::hours(2)),
            source: if pct.is_some() {
                GuardSource::Real
            } else {
                GuardSource::Unknown
            },
            origin: "get_usage",
            observed_at: Some(now()),
            unknown_reason: pct.is_none().then_some("stale"),
            last_seen: None,
        }
    }

    fn rows(five: Option<f64>, week: Option<f64>) -> Vec<GuardLimit> {
        vec![row("five_hour", five), row("seven_day", week)]
    }

    struct Case<'a> {
        limits: &'a [GuardLimit],
        paused: bool,
        billing: Billing,
        api_usd: Option<f64>,
        cost_cap: Option<f64>,
        started: u32,
        max_tasks: Option<u32>,
        max_hours: Option<f64>,
        elapsed_min: i64,
        failures: u32,
    }

    impl<'a> Case<'a> {
        fn new(limits: &'a [GuardLimit]) -> Self {
            Case {
                limits,
                paused: false,
                billing: Billing::Subscription,
                api_usd: None,
                cost_cap: None,
                started: 0,
                max_tasks: None,
                max_hours: None,
                elapsed_min: 10,
                failures: 0,
            }
        }

        fn decide(&self) -> Decision {
            decide(&Inputs {
                now: now(),
                started_at: now() - Duration::minutes(self.elapsed_min),
                paused: self.paused,
                soft: 50,
                hard: 80,
                tool: Some(Tool::Claude),
                limits: self.limits,
                billing: self.billing,
                api_usd: self.api_usd,
                cost_cap: self.cost_cap,
                started: self.started,
                max_tasks: self.max_tasks,
                max_hours: self.max_hours,
                failures: self.failures,
            })
        }
    }

    fn kinds(d: &Decision) -> Vec<&'static str> {
        d.blockers
            .iter()
            .map(|b| match b {
                Blocker::Paused => "paused",
                Blocker::Limit(_) => "limit",
                Blocker::UsageUnknown => "unknown",
                Blocker::CostCap => "cost",
                Blocker::MaxTasks => "tasks",
                Blocker::Hours => "hours",
                Blocker::Failures => "failures",
            })
            .collect()
    }

    #[test]
    fn real_numbers_below_the_soft_limit_allow_starts() {
        let l = rows(Some(40.0), Some(10.0));
        let d = Case::new(&l).decide();
        assert!(d.starts_allowed());
        assert!(d.hard_limit.is_none());
    }

    #[test]
    fn the_soft_limit_blocks_starts_on_either_window() {
        for (five, week) in [(Some(50.0), Some(1.0)), (Some(1.0), Some(63.0))] {
            let l = rows(five, week);
            let d = Case::new(&l).decide();
            assert_eq!(kinds(&d), ["limit"], "{five:?} {week:?}");
            assert!(d.hard_limit.is_none());
        }
    }

    #[test]
    fn the_hard_limit_is_reported_with_its_window() {
        let l = rows(Some(30.0), Some(81.0));
        let d = Case::new(&l).decide();
        let hit = d.hard_limit.clone().unwrap();
        assert_eq!((hit.window.as_str(), hit.pct), ("seven_day", 81.0));
        assert_eq!(kinds(&d), ["limit"]);
    }

    #[test]
    fn unknown_or_stale_numbers_block_starts() {
        for l in [
            rows(None, Some(10.0)),
            rows(Some(10.0), None),
            vec![row("five_hour", Some(10.0))],
            vec![],
        ] {
            assert_eq!(kinds(&Case::new(&l).decide()), ["unknown"]);
        }
    }

    #[test]
    fn a_stale_model_window_does_not_block_but_a_hot_one_does() {
        let mut l = rows(Some(10.0), Some(10.0));
        l.push(row("model:opus", None));
        assert!(Case::new(&l).decide().starts_allowed());
        l.push(row("model:sonnet", Some(70.0)));
        assert_eq!(kinds(&Case::new(&l).decide()), ["limit"]);
    }

    #[test]
    fn an_api_key_has_no_limits_and_only_the_cost_cap_governs() {
        let l = vec![];
        let mut c = Case::new(&l);
        c.billing = Billing::ApiKey;
        c.cost_cap = Some(5.0);
        c.api_usd = Some(4.99);
        assert!(c.decide().starts_allowed());
        c.api_usd = Some(5.0);
        assert_eq!(kinds(&c.decide()), ["cost"]);
    }

    #[test]
    fn caps_pause_and_failures_block_starts() {
        let l = rows(Some(1.0), Some(1.0));
        let mut c = Case::new(&l);
        c.paused = true;
        assert_eq!(kinds(&c.decide()), ["paused"]);
        c.paused = false;
        c.max_tasks = Some(3);
        c.started = 2;
        assert!(c.decide().starts_allowed());
        c.started = 3;
        assert_eq!(kinds(&c.decide()), ["tasks"]);
        c.max_tasks = None;
        c.failures = 2;
        assert!(c.decide().starts_allowed());
        c.failures = 3;
        assert_eq!(kinds(&c.decide()), ["failures"]);
    }

    #[test]
    fn hours_block_at_the_deadline_and_turn_hard_after_the_grace() {
        let l = rows(Some(1.0), Some(1.0));
        let mut c = Case::new(&l);
        c.max_hours = Some(1.0);
        c.elapsed_min = 59;
        assert!(c.decide().starts_allowed());
        c.elapsed_min = 60;
        let d = c.decide();
        assert_eq!(kinds(&d), ["hours"]);
        assert!(!d.hours_hard);
        c.elapsed_min = 90;
        assert!(c.decide().hours_hard);
    }

    fn facts() -> Facts {
        Facts {
            live: 0,
            awaiting_publish: 0,
            eligible: 1,
            settled: true,
            resume_after_reset: false,
            unknown_for: None,
            hard: 80,
        }
    }

    fn verdict_of(l: &[GuardLimit], tweak: impl Fn(&mut Case, &mut Facts)) -> Option<StopReason> {
        let mut c = Case::new(l);
        let mut f = facts();
        tweak(&mut c, &mut f);
        verdict(&c.decide(), &f, now())
    }

    #[test]
    fn nothing_eligible_stops_only_once_settled_and_idle() {
        let l = rows(Some(1.0), Some(1.0));
        let none_left = |_: &mut Case, f: &mut Facts| f.eligible = 0;
        assert_eq!(verdict_of(&l, none_left), Some(StopReason::NothingLeft));
        assert_eq!(
            verdict_of(&l, |_, f| {
                f.eligible = 0;
                f.settled = false;
            }),
            None
        );
        assert_eq!(
            verdict_of(&l, |_, f| {
                f.eligible = 0;
                f.live = 1;
            }),
            None
        );
        assert_eq!(
            verdict_of(&l, |_, f| {
                f.eligible = 0;
                f.awaiting_publish = 1;
            }),
            None
        );
        assert_eq!(
            verdict_of(&l, |c, f| {
                f.eligible = 0;
                c.paused = true;
            }),
            None
        );
    }

    #[test]
    fn soft_conditions_wait_for_live_runs_then_stop() {
        let hot = rows(Some(60.0), Some(1.0));
        assert_eq!(verdict_of(&hot, |_, f| f.live = 1), None);
        assert!(matches!(
            verdict_of(&hot, |_, _| {}),
            Some(StopReason::SoftLimit(_))
        ));
        let cold = rows(Some(1.0), Some(1.0));
        assert_eq!(
            verdict_of(&cold, |c, _| {
                c.max_tasks = Some(1);
                c.started = 1;
            }),
            Some(StopReason::MaxTasks)
        );
        assert_eq!(
            verdict_of(&cold, |c, _| {
                c.billing = Billing::ApiKey;
                c.cost_cap = Some(1.0);
                c.api_usd = Some(2.0);
            }),
            Some(StopReason::CostCap)
        );
        assert_eq!(
            verdict_of(&cold, |c, _| c.failures = 3),
            Some(StopReason::Failures)
        );
        assert_eq!(
            verdict_of(&cold, |c, _| {
                c.max_hours = Some(1.0);
                c.elapsed_min = 61;
            }),
            Some(StopReason::Hours)
        );
    }

    #[test]
    fn a_limit_over_the_hard_percent_is_a_hard_stop_reason() {
        let l = rows(Some(85.0), Some(1.0));
        assert!(matches!(
            verdict_of(&l, |_, _| {}),
            Some(StopReason::HardLimit(_))
        ));
    }

    #[test]
    fn resume_after_reset_waits_while_the_window_has_a_future_reset() {
        let l = rows(Some(85.0), Some(1.0));
        assert_eq!(verdict_of(&l, |_, f| f.resume_after_reset = true), None);
        let mut no_reset = rows(Some(85.0), Some(1.0));
        no_reset[0].resets_at = None;
        assert!(verdict_of(&no_reset, |_, f| f.resume_after_reset = true).is_some());
    }

    #[test]
    fn hours_ignore_the_resume_wait() {
        let l = rows(Some(85.0), Some(1.0));
        assert_eq!(
            verdict_of(&l, |c, f| {
                f.resume_after_reset = true;
                c.max_hours = Some(1.0);
                c.elapsed_min = 61;
            }),
            Some(StopReason::Hours)
        );
    }

    #[test]
    fn hard_hours_stop_even_with_live_runs() {
        let l = rows(Some(1.0), Some(1.0));
        assert_eq!(
            verdict_of(&l, |c, f| {
                f.live = 2;
                c.max_hours = Some(1.0);
                c.elapsed_min = 91;
            }),
            Some(StopReason::Hours)
        );
    }

    #[test]
    fn unknown_numbers_stop_only_after_fifteen_minutes() {
        let l = rows(None, None);
        assert_eq!(
            verdict_of(&l, |_, f| f.unknown_for = Some(Duration::minutes(14))),
            None
        );
        assert_eq!(
            verdict_of(&l, |_, f| f.unknown_for = Some(Duration::minutes(15))),
            Some(StopReason::UsageUnknown)
        );
    }

    #[test]
    fn next_stop_is_the_smallest_distance_to_the_soft_limit() {
        let l = rows(Some(41.0), Some(20.0));
        let n = next_stop(&l, 50, 80).unwrap();
        assert_eq!(
            (n.window.as_str(), n.pct_left, n.kind),
            ("five_hour", 9.0, "no new tasks")
        );
        let hot = rows(Some(60.0), Some(70.0));
        let n = next_stop(&hot, 50, 80).unwrap();
        assert_eq!(
            (n.window.as_str(), n.pct_left, n.kind),
            ("seven_day", 10.0, "pause runs")
        );
        assert!(next_stop(&rows(None, None), 50, 80).is_none());
    }
}
