mod common;
use chrono::{FixedOffset, TimeZone, Utc};
use common::*;
use tmgr_usage::aggregate::{Filters, GroupBy, Query, Tz};
use tmgr_usage::model::project_hash;
use tmgr_usage::scan_account;

fn scanned() -> (tempfile::TempDir, tmgr_usage::Store) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("claude");
    copy_dir(&fixture_dir(), &home);
    let mut store = open_store(tmp.path());
    let report = scan_account(&mut store, &claude_account(&home), &book(), now()).unwrap();
    assert_eq!(report.files, 3);
    assert_eq!(report.events_new, 8);
    assert_eq!(report.events_updated, 2);
    assert_eq!(report.errors, 0);
    assert_eq!(report.skipped_oversized, 0);
    (tmp, store)
}

fn query(group_by: Vec<GroupBy>, tz: Tz) -> Query {
    Query {
        from: Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap(),
        to: Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap(),
        tz,
        group_by,
        filters: Filters::default(),
    }
}

fn utc() -> Tz {
    Tz::Fixed(FixedOffset::east_opt(0).unwrap())
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

#[test]
fn totals_match_hand_calculation() {
    let (_tmp, store) = scanned();
    let report = store.aggregate(&query(vec![], utc())).unwrap();
    let t = &report.totals;
    assert_eq!(t.tokens.input, 150126);
    assert_eq!(t.tokens.output, 229);
    assert_eq!(t.tokens.cache_read, 63000);
    assert_eq!(t.tokens.cache_write_5m, 500);
    assert_eq!(t.tokens.cache_write_1h, 400);
    assert_eq!(t.events, 8);
    assert_eq!(t.unpriced_events, 1);
    close(t.cost_usd, 3.151619);
}

#[test]
fn days_in_utc() {
    let (_tmp, store) = scanned();
    let report = store.aggregate(&query(vec![GroupBy::Day], utc())).unwrap();
    let days: Vec<_> = report.rows.iter().map(|r| r.keys[0].as_str()).collect();
    assert_eq!(days, ["2026-10-01", "2026-10-02"]);
    assert_eq!(report.rows[0].events, 7);
    close(report.rows[0].cost_usd, 0.024119);
    assert_eq!(report.rows[1].events, 1);
    close(report.rows[1].cost_usd, 3.1275);
}

#[test]
fn days_follow_the_timezone() {
    let (_tmp, store) = scanned();
    let west = Tz::Fixed(FixedOffset::west_opt(3600).unwrap());
    let report = store.aggregate(&query(vec![GroupBy::Day], west)).unwrap();
    assert_eq!(report.rows.len(), 1);
    assert_eq!(report.rows[0].keys[0], "2026-10-01");
    assert_eq!(report.rows[0].events, 8);
}

#[test]
fn by_model_prices_each_model() {
    let (_tmp, store) = scanned();
    let report = store
        .aggregate(&query(vec![GroupBy::Model], utc()))
        .unwrap();
    let find = |m: &str| report.rows.iter().find(|r| r.keys[0] == m).unwrap();
    assert_eq!(find("claude-test-opus-1").events, 5);
    close(find("claude-test-opus-1").cost_usd, 3.15146);
    assert_eq!(find("claude-test-sonnet-1").events, 2);
    close(find("claude-test-sonnet-1").cost_usd, 0.000159);
    assert_eq!(find("claude-unknown-9").unpriced_events, 1);
    close(find("claude-unknown-9").cost_usd, 0.0);
}

#[test]
fn earliest_session_owns_resumed_copy_and_subagents_join_parent() {
    let (_tmp, store) = scanned();
    let report = store
        .aggregate(&query(vec![GroupBy::Session], utc()))
        .unwrap();
    let find = |s: &str| report.rows.iter().find(|r| r.keys[0] == s).unwrap();
    assert_eq!(find("s1").events, 7);
    assert_eq!(find("s0").events, 1);
}

#[test]
fn project_branch_tool_and_account_groups() {
    let (_tmp, store) = scanned();
    let by = |g| store.aggregate(&query(vec![g], utc())).unwrap();
    let projects = by(GroupBy::Project);
    assert_eq!(projects.rows.len(), 1);
    assert_eq!(projects.rows[0].keys[0], project_hash("/work/alpha"));
    assert_eq!(projects.rows[0].labels[0], "alpha");
    let branches = by(GroupBy::Branch);
    let feature = branches
        .rows
        .iter()
        .find(|r| r.keys[0] == "feature-x")
        .unwrap();
    assert_eq!(feature.events, 1);
    assert_eq!(by(GroupBy::Tool).rows[0].keys[0], "claude");
    assert_eq!(by(GroupBy::Account).rows[0].keys[0], "claude");
}

#[test]
fn filters_narrow_the_result() {
    let (_tmp, store) = scanned();
    let mut q = query(vec![GroupBy::Model], utc());
    q.filters.sessions = vec!["s1".into()];
    q.filters.models = vec!["claude-test-sonnet-1".into()];
    let report = store.aggregate(&q).unwrap();
    assert_eq!(report.totals.events, 1);
    assert_eq!(report.totals.tokens.input, 5);
    let mut q = query(vec![], utc());
    q.from = Utc.with_ymd_and_hms(2026, 10, 2, 0, 0, 0).unwrap();
    assert_eq!(store.aggregate(&q).unwrap().totals.events, 1);
}

#[test]
fn scan_order_does_not_change_attribution() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("claude");
    copy_dir(&fixture_dir(), &home);
    let resume = projects_file(&home, "s0-resume.jsonl");
    let held = tmp.path().join("held.jsonl");
    std::fs::rename(&resume, &held).unwrap();
    let mut store = open_store(tmp.path());
    let account = claude_account(&home);
    scan_account(&mut store, &account, &book(), now()).unwrap();
    std::fs::rename(&held, &resume).unwrap();
    let report = scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!(report.events_new, 1);
    let sessions = store
        .aggregate(&query(vec![GroupBy::Session], utc()))
        .unwrap();
    let find = |s: &str| sessions.rows.iter().find(|r| r.keys[0] == s).unwrap();
    assert_eq!(find("s1").events, 7);
    assert_eq!(find("s0").events, 1);
    assert_eq!(
        store
            .aggregate(&query(vec![], utc()))
            .unwrap()
            .totals
            .events,
        8
    );
}
