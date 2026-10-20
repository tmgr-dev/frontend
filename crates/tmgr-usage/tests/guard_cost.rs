mod common;
use chrono::{DateTime, Duration, TimeZone, Utc};
use common::book;
use std::collections::BTreeMap;
use tmgr_usage::cost::{summarize, Billing, BASIS};
use tmgr_usage::guard::{guard_limits, GuardSource};
use tmgr_usage::limits::{LimitSnapshot, LimitSource, LimitWindow};
use tmgr_usage::link::{LinkEvent, RunWindow};
use tmgr_usage::{Tokens, Tool};

fn t(h: u32, m: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 19, h, m, 0).unwrap()
}

fn snap(
    tool: Tool,
    window: LimitWindow,
    pct: f64,
    resets: Option<DateTime<Utc>>,
    observed: DateTime<Utc>,
    source: LimitSource,
) -> LimitSnapshot {
    LimitSnapshot {
        tool,
        account: tool.as_str().into(),
        window,
        used_pct: pct,
        resets_at: resets,
        plan: None,
        observed_at: observed,
        source,
        tool_session_id: String::new(),
    }
}

const MAX_AGE: fn() -> Duration = || Duration::minutes(15);

#[test]
fn freshest_real_reading_wins_per_tool_and_window() {
    let now = t(12, 0);
    let out = guard_limits(
        &[
            snap(
                Tool::Claude,
                LimitWindow::Session,
                10.0,
                Some(t(15, 0)),
                t(11, 50),
                LimitSource::Statusline,
            ),
            snap(
                Tool::Claude,
                LimitWindow::Session,
                12.0,
                Some(t(15, 0)),
                t(11, 58),
                LimitSource::Probe,
            ),
            snap(
                Tool::Claude,
                LimitWindow::Weekly,
                30.0,
                Some(t(20, 0)),
                t(11, 55),
                LimitSource::Probe,
            ),
            snap(
                Tool::Codex,
                LimitWindow::Weekly,
                48.0,
                Some(t(20, 0)),
                t(11, 56),
                LimitSource::Probe,
            ),
            snap(
                Tool::Codex,
                LimitWindow::Session,
                5.0,
                Some(t(15, 0)),
                t(11, 40),
                LimitSource::Rollout,
            ),
        ],
        now,
        MAX_AGE(),
    );
    let get = |tool, w: &str| {
        out.iter()
            .find(|g| g.tool == tool && g.window == w)
            .unwrap()
    };
    let s = get(Tool::Claude, "five_hour");
    assert_eq!(
        (s.used_pct, s.source, s.origin),
        (Some(12.0), GuardSource::Real, "get_usage")
    );
    assert_eq!(get(Tool::Claude, "seven_day").used_pct, Some(30.0));
    assert_eq!(get(Tool::Codex, "seven_day").origin, "app_server");
    assert_eq!(get(Tool::Codex, "five_hour").origin, "rollout");
    assert_eq!(out.len(), 4);
}

#[test]
fn statusline_origin_and_model_window_names() {
    let out = guard_limits(
        &[
            snap(
                Tool::Claude,
                LimitWindow::Session,
                1.0,
                None,
                t(11, 59),
                LimitSource::Statusline,
            ),
            snap(
                Tool::Claude,
                LimitWindow::Model("fable".into()),
                2.0,
                None,
                t(11, 59),
                LimitSource::Probe,
            ),
        ],
        t(12, 0),
        MAX_AGE(),
    );
    assert_eq!(out[0].origin, "statusline");
    assert!(out.iter().any(|g| g.window == "model:fable"));
}

#[test]
fn reset_window_is_unknown_and_keeps_last_seen() {
    let out = guard_limits(
        &[snap(
            Tool::Claude,
            LimitWindow::Session,
            90.0,
            Some(t(11, 0)),
            t(10, 59),
            LimitSource::Probe,
        )],
        t(12, 0),
        Duration::hours(24),
    );
    let g = &out[0];
    assert_eq!((g.used_pct, g.source), (None, GuardSource::Unknown));
    assert_eq!(g.unknown_reason, Some("window_reset"));
    assert_eq!(g.last_seen.as_ref().unwrap().used_pct, 90.0);
}

#[test]
fn old_reading_is_unknown_and_keeps_last_seen() {
    let out = guard_limits(
        &[snap(
            Tool::Codex,
            LimitWindow::Weekly,
            40.0,
            Some(t(20, 0)),
            t(11, 0),
            LimitSource::Rollout,
        )],
        t(12, 0),
        MAX_AGE(),
    );
    let g = &out[0];
    assert_eq!(
        (g.used_pct, g.source, g.unknown_reason),
        (None, GuardSource::Unknown, Some("stale"))
    );
    assert_eq!(g.last_seen.as_ref().unwrap().used_pct, 40.0);
    assert_eq!(g.observed_at, Some(t(11, 0)));
}

#[test]
fn reading_from_the_future_is_unknown() {
    let out = guard_limits(
        &[snap(
            Tool::Codex,
            LimitWindow::Weekly,
            40.0,
            None,
            t(13, 0),
            LimitSource::Rollout,
        )],
        t(12, 0),
        MAX_AGE(),
    );
    assert_eq!(out[0].unknown_reason, Some("clock"));
}

#[test]
fn nothing_read_gives_no_rows() {
    assert!(guard_limits(&[], t(12, 0), MAX_AGE()).is_empty());
}

fn run(name: &str, session: &str, from: DateTime<Utc>, to: Option<DateTime<Utc>>) -> RunWindow {
    RunWindow {
        run_ref: name.into(),
        task_ref: format!("task-{name}"),
        workspace: "ws".into(),
        session_id: Some(session.into()),
        branch: None,
        project_hash: None,
        started_at: from,
        ended_at: to,
    }
}

fn ev(
    id: i64,
    session: &str,
    at: DateTime<Utc>,
    tokens: u64,
    cost: Option<f64>,
    model: &str,
) -> (LinkEvent, Tokens) {
    (
        LinkEvent {
            id,
            tool: Tool::Claude,
            account: "claude".into(),
            tool_session_id: session.into(),
            parent_session_id: None,
            ts: at,
            model: model.into(),
            git_branch: None,
            project_hash: None,
            cost_usd: cost,
            tokens,
            agent_id: None,
        },
        Tokens {
            input: tokens,
            ..Tokens::default()
        },
    )
}

#[test]
fn only_events_of_the_given_runs_count() {
    let runs = [run("r1", "s1", t(9, 0), Some(t(10, 0)))];
    let events = [
        ev(1, "s1", t(9, 10), 1000, Some(0.5), "claude-test-opus-1"),
        ev(2, "s1", t(9, 20), 500, Some(0.25), "claude-test-opus-1"),
        ev(3, "other", t(9, 30), 9999, Some(9.0), "claude-test-opus-1"),
        ev(4, "s1", t(11, 0), 7777, Some(7.0), "claude-test-opus-1"),
    ];
    let s = summarize(&events, &book(), &runs, t(12, 0), |_| Billing::Subscription);
    assert_eq!(s.subscription_tokens, 1500);
    assert_eq!(s.api_usd, None);
    assert_eq!(s.billing, Billing::Subscription);
    assert_eq!(s.basis, BASIS);
    assert_eq!(s.events, 2);
}

#[test]
fn api_key_runs_are_priced_and_missing_prices_are_counted() {
    let runs = [run("r1", "s1", t(9, 0), None)];
    let events = [
        ev(1, "s1", t(9, 10), 100, Some(0.5), "claude-test-opus-1"),
        ev(2, "s1", t(9, 20), 1_000_000, None, "claude-test-sonnet-1"),
        ev(3, "s1", t(9, 30), 10, None, "no-such-model"),
    ];
    let s = summarize(&events, &book(), &runs, t(12, 0), |_| Billing::ApiKey);
    assert_eq!(s.subscription_tokens, 0);
    assert!((s.api_usd.unwrap() - (0.5 + 3.0)).abs() < 1e-9);
    assert_eq!(s.unpriced_events, 1);
    assert_eq!(s.billing, Billing::ApiKey);
}

#[test]
fn unknown_billing_reports_both_numbers_and_says_so() {
    let runs = [run("r1", "s1", t(9, 0), None)];
    let events = [ev(1, "s1", t(9, 10), 100, Some(0.5), "claude-test-opus-1")];
    let s = summarize(&events, &book(), &runs, t(12, 0), |_| Billing::Unknown);
    assert_eq!(s.subscription_tokens, 100);
    assert_eq!(s.api_usd, Some(0.5));
    assert_eq!(s.billing, Billing::Unknown);
}

#[test]
fn mixed_billing_is_unknown() {
    let runs = [run("r1", "s1", t(9, 0), None)];
    let mut e2 = ev(2, "s1", t(9, 20), 50, Some(0.1), "claude-test-opus-1");
    e2.0.tool = Tool::Codex;
    let events = [
        ev(1, "s1", t(9, 10), 100, Some(0.5), "claude-test-opus-1"),
        e2,
    ];
    let by_tool: BTreeMap<Tool, Billing> = [
        (Tool::Claude, Billing::Subscription),
        (Tool::Codex, Billing::ApiKey),
    ]
    .into();
    let s = summarize(&events, &book(), &runs, t(12, 0), |tool| by_tool[&tool]);
    assert_eq!(s.subscription_tokens, 100);
    assert_eq!(s.api_usd, Some(0.1));
    assert_eq!(s.billing, Billing::Unknown);
}

#[test]
fn empty_runs_cost_nothing() {
    let events = [ev(1, "s1", t(9, 10), 100, Some(0.5), "claude-test-opus-1")];
    let s = summarize(&events, &book(), &[], t(12, 0), |_| Billing::Unknown);
    assert_eq!((s.subscription_tokens, s.api_usd, s.events), (0, None, 0));
}
