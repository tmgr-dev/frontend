mod common;
use chrono::{Duration, TimeZone, Utc};
use common::*;
use tmgr_usage::aggregate::{Filters, Query, Tz};
use tmgr_usage::limits::{LimitSnapshot, LimitSource, LimitWindow};
use tmgr_usage::schema::MIGRATIONS;
use tmgr_usage::{scan_account, PriceBook, Store, Tool, UsageError};

fn totals(store: &Store) -> tmgr_usage::Row {
    store
        .aggregate(&Query {
            from: Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap(),
            to: Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap(),
            tz: Tz::Fixed(chrono::FixedOffset::east_opt(0).unwrap()),
            group_by: vec![],
            filters: Filters::default(),
        })
        .unwrap()
        .totals
}

fn scanned() -> (tempfile::TempDir, std::path::PathBuf, Store) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("claude");
    copy_dir(&fixture_dir(), &home);
    let mut store = open_store(tmp.path());
    scan_account(&mut store, &claude_account(&home), &book(), now()).unwrap();
    (tmp, home, store)
}

#[test]
fn opens_an_older_schema_and_migrates_it() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("usage.db");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(MIGRATIONS[0]).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL);
             INSERT INTO schema_version VALUES (1);
             INSERT INTO usage_source (tool, account) VALUES ('claude', 'kept');",
        )
        .unwrap();
    }
    let store = Store::open(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), Store::SCHEMA_VERSION);
    let snap = LimitSnapshot {
        tool: Tool::Claude,
        account: "kept".into(),
        window: LimitWindow::Weekly,
        used_pct: 1.0,
        resets_at: None,
        plan: None,
        observed_at: now(),
        source: LimitSource::Statusline,
        tool_session_id: "s".into(),
    };
    store.record_limit(&snap).unwrap();
    drop(store);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let kept: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM usage_source WHERE account = 'kept'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(kept, 1);
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}

#[test]
fn refuses_a_newer_schema() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("usage.db");
    drop(Store::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute("INSERT INTO schema_version VALUES (99)", [])
        .unwrap();
    drop(conn);
    assert!(matches!(
        Store::open(&path),
        Err(UsageError::SchemaTooNew { found: 99, .. })
    ));
}

#[cfg(unix)]
#[test]
fn database_file_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("fresh").join("usage.db");
    drop(Store::open(&path).unwrap());
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn prune_folds_old_events_and_keeps_totals() {
    let (_tmp, home, mut store) = scanned();
    let before = totals(&store);
    let report = store.prune(7, now()).unwrap();
    assert_eq!(report.events_folded, 8);
    assert_eq!(store.event_count().unwrap(), 0);
    let after = totals(&store);
    assert_eq!(after.tokens, before.tokens);
    assert_eq!(after.events, before.events);
    assert_eq!(after.unpriced_events, before.unpriced_events);
    assert!((after.cost_usd - before.cost_usd).abs() < 1e-9);

    let file = projects_file(&home, "s1.jsonl");
    let mut f = std::fs::OpenOptions::new().append(true).open(file).unwrap();
    use std::io::Write;
    writeln!(
        f,
        "{}",
        assistant_line("s1", "2026-10-01T09:00:00Z", "late", 1, 1)
    )
    .unwrap();
    let rescan = scan_account(&mut store, &claude_account(&home), &book(), now()).unwrap();
    assert_eq!(rescan.events_new, 0);
    assert_eq!(rescan.skipped_before_floor, 1);
    assert_eq!(totals(&store).events, before.events);
}

fn by_day(store: &Store, offset_h: i32) -> Vec<(String, u64, u64)> {
    store
        .aggregate(&Query {
            from: Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap(),
            to: Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap(),
            tz: Tz::Fixed(chrono::FixedOffset::east_opt(offset_h * 3600).unwrap()),
            group_by: vec![tmgr_usage::GroupBy::Day],
            filters: Filters::default(),
        })
        .unwrap()
        .rows
        .into_iter()
        .map(|r| (r.keys[0].clone(), r.events, r.tokens.input))
        .collect()
}

#[test]
fn folded_days_follow_the_reporting_timezone_and_merge_with_live_events() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("claude");
    write_lines(
        &projects_file(&home, "s.jsonl"),
        &[
            assistant_line("s", "2026-10-01T23:30:00Z", "m1", 10, 1),
            assistant_line("s", "2026-10-02T00:30:00Z", "m2", 20, 1),
            assistant_line("s", "2026-10-01T10:00:00Z", "m3", 40, 1),
        ],
    );
    let mut store = open_store(tmp.path());
    scan_account(&mut store, &claude_account(&home), &book(), now()).unwrap();
    let before = by_day(&store, 2);
    assert_eq!(
        before,
        vec![
            ("2026-10-01".to_string(), 1, 40),
            ("2026-10-02".to_string(), 2, 30)
        ]
    );
    let tz = Tz::Fixed(chrono::FixedOffset::east_opt(2 * 3600).unwrap());
    let later = Utc.with_ymd_and_hms(2026, 10, 9, 15, 0, 0).unwrap();
    let report = store.prune_with_tz(7, later, tz).unwrap();
    assert_eq!(report.events_folded, 2);
    assert_eq!(by_day(&store, 2), before);
    assert_eq!(
        store.history_floor().unwrap().unwrap().to_rfc3339(),
        "2026-10-02T00:00:00+00:00"
    );
}

#[test]
fn prune_keeps_recent_events_in_the_event_table() {
    let (_tmp, _home, mut store) = scanned();
    let before = totals(&store);
    let cutoff_now = Utc.with_ymd_and_hms(2026, 10, 9, 15, 0, 0).unwrap();
    let report = store.prune(7, cutoff_now).unwrap();
    assert_eq!(report.events_folded, 7);
    assert_eq!(store.event_count().unwrap(), 1);
    let after = totals(&store);
    assert_eq!(after.events, before.events);
    assert_eq!(after.tokens, before.tokens);
    assert!((after.cost_usd - before.cost_usd).abs() < 1e-9);
    let again = store.prune(7, cutoff_now).unwrap();
    assert_eq!(again.events_folded, 0);
    assert_eq!(totals(&store).events, before.events);
}

#[test]
fn delete_all_wipes_everything_and_rescan_restores() {
    let (_tmp, home, mut store) = scanned();
    store.delete_all().unwrap();
    assert_eq!(store.event_count().unwrap(), 0);
    assert_eq!(totals(&store).events, 0);
    let report = scan_account(&mut store, &claude_account(&home), &book(), now()).unwrap();
    assert_eq!(report.events_new, 8);
}

fn limit(
    window: LimitWindow,
    pct: f64,
    observed_min: i64,
    resets_h: Option<i64>,
    session: &str,
) -> LimitSnapshot {
    LimitSnapshot {
        tool: Tool::Codex,
        account: "codex".into(),
        window,
        used_pct: pct,
        resets_at: resets_h.map(|h| now() + Duration::hours(h)),
        plan: Some("pro".into()),
        observed_at: now() - Duration::minutes(observed_min),
        source: LimitSource::Rollout,
        tool_session_id: session.into(),
    }
}

#[test]
fn newest_unexpired_limit_wins_per_window() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    store
        .record_limit(&limit(LimitWindow::Session, 10.0, 30, Some(2), "a"))
        .unwrap();
    store
        .record_limit(&limit(LimitWindow::Session, 40.0, 5, Some(2), "b"))
        .unwrap();
    store
        .record_limit(&limit(LimitWindow::Session, 90.0, 1, Some(-1), "c"))
        .unwrap();
    store
        .record_limit(&limit(LimitWindow::Weekly, 55.0, 3, None, "a"))
        .unwrap();
    store
        .record_limit(&limit(LimitWindow::Weekly, 99.0, 60, None, "a"))
        .unwrap();
    let current = store.current_limits(now()).unwrap();
    assert_eq!(current.len(), 2);
    let session = current
        .iter()
        .find(|l| l.window == LimitWindow::Session)
        .unwrap();
    assert_eq!(
        (session.used_pct, session.tool_session_id.as_str()),
        (40.0, "b")
    );
    let weekly = current
        .iter()
        .find(|l| l.window == LimitWindow::Weekly)
        .unwrap();
    assert_eq!(weekly.used_pct, 55.0);
}

#[test]
fn prices_round_trip_and_reprice_changes_costs() {
    let (_tmp, _home, mut store) = scanned();
    store.save_prices(&book(), "test", now()).unwrap();
    let (loaded, fetched) = store.load_prices().unwrap().unwrap();
    assert_eq!(fetched, now());
    assert_eq!(loaded.len(), book().len());

    assert_eq!(store.reprice(&book()).unwrap(), 0);

    let doubled = TEST_PRICES
        .replace("\"input_cost_per_token\": 1e-5", "\"input_cost_per_token\": 2e-5")
        .replace(
            "\"claude-test-sonnet-1\"",
            "\"claude-unknown-9\": {\"mode\": \"chat\", \"input_cost_per_token\": 1e-6, \"output_cost_per_token\": 1e-6}, \"claude-test-sonnet-1\"",
        );
    let changed = PriceBook::from_litellm_json(doubled.as_bytes()).unwrap();
    let n = store.reprice(&changed).unwrap();
    assert_eq!(n, 4);
    let t = totals(&store);
    assert_eq!(t.unpriced_events, 0);
    assert!(t.cost_usd > 3.151619);
}

#[test]
fn empty_store_has_no_prices() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(open_store(tmp.path()).load_prices().unwrap().is_none());
}

#[test]
fn link_events_expose_session_and_project_without_cwd() {
    let (_tmp, _home, store) = scanned();
    let events = store
        .link_events(
            Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 10, 1, 10, 3, 0).unwrap(),
        )
        .unwrap();
    assert_eq!(events.len(), 4);
    assert!(events
        .iter()
        .all(|e| e.project_hash.is_some() && e.tool == Tool::Claude));
}

#[test]
fn rollups_folded_in_different_time_zones_share_a_day_label_and_count_once() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("claude");
    write_lines(
        &projects_file(&home, "s.jsonl"),
        &[
            assistant_line("s", "2026-10-01T10:00:00Z", "m1", 10, 1),
            assistant_line("s", "2026-10-02T01:00:00Z", "m2", 20, 1),
        ],
    );
    let mut store = open_store(tmp.path());
    scan_account(&mut store, &claude_account(&home), &book(), now()).unwrap();
    let utc = Tz::Fixed(chrono::FixedOffset::east_opt(0).unwrap());
    let first = store
        .prune_with_tz(7, Utc.with_ymd_and_hms(2026, 10, 9, 0, 0, 0).unwrap(), utc)
        .unwrap();
    assert_eq!(first.events_folded, 1);
    let west = Tz::Fixed(chrono::FixedOffset::west_opt(5 * 3600).unwrap());
    let second = store
        .prune_with_tz(
            7,
            Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap(),
            west,
        )
        .unwrap();
    assert_eq!(second.events_folded, 1);
    assert_eq!(store.event_count().unwrap(), 0);
    let conn = rusqlite::Connection::open(tmp.path().join("usage.db")).unwrap();
    let rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM usage_day_rollup WHERE day = '2026-10-01'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 1);
    let t = totals(&store);
    assert_eq!((t.events, t.tokens.input), (2, 30));
}

#[test]
fn the_fullest_snapshot_of_the_current_window_wins_over_newer_idle_ones() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let resets = now() + Duration::days(5);
    let snap = |session: &str, window: LimitWindow, pct: f64, observed_min: i64, skew_s: i64| {
        LimitSnapshot {
            tool: Tool::Claude,
            account: "claude".into(),
            window,
            used_pct: pct,
            resets_at: Some(resets + Duration::seconds(skew_s)),
            plan: None,
            observed_at: now() - Duration::minutes(observed_min),
            source: LimitSource::Statusline,
            tool_session_id: session.into(),
        }
    };
    store
        .record_limit(&snap("active", LimitWindow::Weekly, 68.0, 4, 0))
        .unwrap();
    store
        .record_limit(&snap("active", LimitWindow::Session, 25.0, 4, 0))
        .unwrap();
    for (i, pct) in [1.0, 5.0, 27.0, 32.0, 45.0, 58.0].into_iter().enumerate() {
        store
            .record_limit(&snap(
                &format!("idle{i}"),
                LimitWindow::Weekly,
                pct,
                i as i64,
                61,
            ))
            .unwrap();
    }
    let mut old = snap("old", LimitWindow::Weekly, 97.0, 0, 0);
    old.resets_at = Some(now() - Duration::hours(1));
    store.record_limit(&old).unwrap();
    for _ in 0..3 {
        let current = store.current_limits(now()).unwrap();
        let weekly = current
            .iter()
            .find(|l| l.window == LimitWindow::Weekly)
            .unwrap();
        assert_eq!(weekly.used_pct, 68.0);
        assert_eq!(weekly.tool_session_id, "active");
        assert_eq!(weekly.observed_at, now() - Duration::minutes(4));
        let session = current
            .iter()
            .find(|l| l.window == LimitWindow::Session)
            .unwrap();
        assert_eq!(session.used_pct, 25.0);
    }
}

#[test]
fn a_new_window_replaces_the_fuller_old_one() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let mut fresh = limit(LimitWindow::Weekly, 3.0, 1, Some(24 * 7), "new");
    fresh.tool = Tool::Claude;
    fresh.source = LimitSource::Statusline;
    let mut stale = limit(LimitWindow::Weekly, 90.0, 0, Some(2), "old");
    stale.tool = Tool::Claude;
    stale.source = LimitSource::Statusline;
    store.record_limit(&fresh).unwrap();
    store.record_limit(&stale).unwrap();
    let weekly = store.current_limits(now()).unwrap();
    assert_eq!(weekly.len(), 1);
    assert_eq!(weekly[0].used_pct, 3.0);
}

#[test]
fn rollout_readings_keep_the_newest_one() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    store
        .record_limit(&limit(LimitWindow::Weekly, 70.0, 30, Some(24), "a"))
        .unwrap();
    store
        .record_limit(&limit(LimitWindow::Weekly, 20.0, 1, Some(24), "b"))
        .unwrap();
    store
        .record_limit(&limit(LimitWindow::Session, 50.0, 30, Some(2), "a"))
        .unwrap();
    store
        .record_limit(&limit(LimitWindow::Session, 10.0, 1, None, "b"))
        .unwrap();
    let current = store.current_limits(now()).unwrap();
    let pct = |w: LimitWindow| current.iter().find(|l| l.window == w).unwrap().used_pct;
    assert_eq!(pct(LimitWindow::Weekly), 20.0);
    assert_eq!(pct(LimitWindow::Session), 10.0);
}

#[test]
fn a_notice_is_taken_at_most_the_given_times_across_connections() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("usage.db");
    let a = Store::open(&path).unwrap();
    let b = Store::open(&path).unwrap();
    let taken: Vec<bool> = (0..5)
        .map(|i| {
            if i % 2 == 0 { &a } else { &b }
                .take_notice("n", 3)
                .unwrap()
        })
        .collect();
    assert_eq!(taken, [true, true, true, false, false]);
    assert!(a.take_notice("other", 3).unwrap());
}
