use chrono::{DateTime, Duration, FixedOffset, TimeZone, Utc};
use tmgr_usage::limits::{LimitSnapshot, LimitSource, LimitWindow};
use tmgr_usage::link::LinkEvent;
use tmgr_usage::machine::{
    auto_due, build_report, clean_account, days_to_send, limit_from_hit, valid_label, Identity,
    Input,
};
use tmgr_usage::{Store, Tokens, Tool, Tz};

fn offset(secs: i32) -> Tz {
    Tz::Fixed(FixedOffset::east_opt(secs).unwrap())
}

fn ident() -> Identity {
    Identity {
        label: "work-mac".into(),
        platform: "macos".into(),
        cli_version: "1.0.140".into(),
        timezone: Some("UTC".into()),
        reenable: false,
    }
}

fn at(ts: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(ts)
        .unwrap()
        .with_timezone(&Utc)
}

fn ev(
    id: i64,
    tool: Tool,
    session: &str,
    ts: &str,
    model: &str,
    cost: Option<f64>,
) -> (LinkEvent, Tokens) {
    (
        LinkEvent {
            id,
            tool,
            account: "claude".into(),
            tool_session_id: session.into(),
            parent_session_id: None,
            ts: at(ts),
            model: model.into(),
            git_branch: Some("SECRET-BRANCH".into()),
            project_hash: Some("SECRET-HASH".into()),
            cost_usd: cost,
            tokens: 0,
            agent_id: None,
        },
        Tokens {
            input: 10,
            output: 20,
            cache_read: 30,
            cache_write_5m: 1,
            cache_write_1h: 2,
            reasoning: 4,
        },
    )
}

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 21, 10, 0, 0).unwrap()
}

fn input<'a>(
    id: &'a Identity,
    events: &'a [(LinkEvent, Tokens)],
    tasks: &'a [Option<u64>],
    limits: &'a [LimitSnapshot],
) -> Input<'a> {
    Input {
        identity: id,
        now: now(),
        tz: offset(0),
        from_day: "2026-11-20".parse().unwrap(),
        to_day: "2026-11-21".parse().unwrap(),
        complete_from: None,
        events,
        task_ids: tasks,
        limits,
    }
}

fn report(
    events: &[(LinkEvent, Tokens)],
    tasks: &[Option<u64>],
    limits: &[LimitSnapshot],
) -> serde_json::Value {
    let id = ident();
    serde_json::to_value(build_report(&input(&id, events, tasks, limits)).unwrap()).unwrap()
}

#[test]
fn days_group_by_agent_model_and_task_with_distinct_sessions() {
    let events = vec![
        ev(
            1,
            Tool::Claude,
            "s1",
            "2026-11-21T01:00:00Z",
            "opus",
            Some(1.5),
        ),
        ev(
            2,
            Tool::Claude,
            "s1",
            "2026-11-21T02:00:00Z",
            "opus",
            Some(0.5),
        ),
        ev(3, Tool::Claude, "s2", "2026-11-21T03:00:00Z", "opus", None),
        ev(
            4,
            Tool::Claude,
            "s3",
            "2026-11-21T04:00:00Z",
            "sonnet",
            Some(0.25),
        ),
        ev(
            5,
            Tool::Codex,
            "s4",
            "2026-11-20T23:30:00Z",
            "gpt",
            Some(2.0),
        ),
        ev(
            6,
            Tool::Claude,
            "s9",
            "2026-11-19T23:30:00Z",
            "opus",
            Some(9.0),
        ),
    ];
    let tasks = vec![Some(7), Some(7), Some(7), None, None, None];
    let v = report(&events, &tasks, &[]);
    assert_eq!(v["schema"], "machine_usage.v1");
    let days = v["days"].as_array().unwrap();
    assert_eq!(days.len(), 2);
    assert_eq!(days[0]["day"], "2026-11-20");
    assert_eq!(days[0]["rows"][0]["agent"], "codex");
    let rows = days[1]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let opus = rows.iter().find(|r| r["model"] == "opus").unwrap();
    assert_eq!(opus["task_id"], 7);
    assert_eq!(opus["agent"], "claude-code");
    assert_eq!(opus["events"], 3);
    assert_eq!(opus["sessions"], 2);
    assert_eq!(opus["cost_usd"], 2.0);
    assert_eq!(opus["input_tokens"], 30);
    assert_eq!(opus["output_tokens"], 60);
    assert_eq!(opus["cache_read_tokens"], 90);
    assert_eq!(opus["cache_write_tokens"], 9);
    assert_eq!(opus["reasoning_tokens"], 12);
    let sonnet = rows.iter().find(|r| r["model"] == "sonnet").unwrap();
    assert!(sonnet["task_id"].is_null());
}

#[test]
fn days_before_the_history_floor_are_not_sent() {
    let events = vec![
        ev(
            1,
            Tool::Claude,
            "s1",
            "2026-11-20T10:00:00Z",
            "opus",
            Some(1.0),
        ),
        ev(
            2,
            Tool::Claude,
            "s1",
            "2026-11-21T10:00:00Z",
            "opus",
            Some(1.0),
        ),
    ];
    let id = ident();
    let mut inp = input(&id, &events, &[None, None], &[]);
    inp.complete_from = Some(Utc.with_ymd_and_hms(2026, 11, 20, 12, 0, 0).unwrap());
    let r = build_report(&inp).unwrap();
    assert_eq!(r.days.len(), 1);
    assert_eq!(r.days[0].day, "2026-11-21");
}

#[test]
fn days_follow_the_local_zone() {
    let events = vec![ev(
        1,
        Tool::Claude,
        "s",
        "2026-11-20T23:30:00Z",
        "opus",
        Some(1.0),
    )];
    let id = ident();
    let mut inp = input(&id, &events, &[None], &[]);
    inp.tz = offset(3600);
    assert_eq!(build_report(&inp).unwrap().days[0].day, "2026-11-21");
}

fn snap(
    account: &str,
    window: LimitWindow,
    pct: f64,
    observed_min: i64,
    source: LimitSource,
) -> LimitSnapshot {
    LimitSnapshot {
        tool: Tool::Claude,
        account: account.into(),
        window,
        used_pct: pct,
        resets_at: Some(now() + Duration::hours(2)),
        plan: Some("max".into()),
        observed_at: now() - Duration::minutes(observed_min),
        source,
        tool_session_id: String::new(),
    }
}

#[test]
fn limits_are_cleaned_and_newest_wins_per_key() {
    let limits = vec![
        snap(
            "/Users/x/.claude",
            LimitWindow::Session,
            10.0,
            30,
            LimitSource::Statusline,
        ),
        snap("a@b.com", LimitWindow::Session, 62.5, 5, LimitSource::Probe),
        snap(
            "claude",
            LimitWindow::Weekly,
            40.0,
            1,
            LimitSource::LimitHit,
        ),
    ];
    let v = report(&[], &[], &limits);
    let rows = v["limits"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let session = rows.iter().find(|r| r["window"] == "session").unwrap();
    assert_eq!(session["account"], "default");
    assert_eq!(session["used_pct"], 62.5);
    assert_eq!(session["agent"], "claude-code");
    assert_eq!(session["source"], "probe");
    assert_eq!(session["resets_at"], "2026-11-21T12:00:00Z");
    assert_eq!(session["plan"], "max");
    let weekly = rows.iter().find(|r| r["window"] == "weekly").unwrap();
    assert_eq!(weekly["account"], "claude");
    assert_eq!(weekly["source"], "limit_hit");
}

#[test]
fn account_labels_that_look_like_paths_or_emails_become_default() {
    for bad in [
        "/home/u/.claude",
        "C:\\Users\\u",
        "~/.codex",
        "me@example.com",
        "",
        "  ",
    ] {
        assert_eq!(clean_account(bad), "default", "{bad}");
    }
    assert_eq!(clean_account("claude-env"), "claude-env");
}

#[test]
fn no_content_field_reaches_the_body() {
    let mut events = vec![ev(
        1,
        Tool::Claude,
        "s1",
        "2026-11-21T01:00:00Z",
        "opus",
        Some(1.0),
    )];
    events[0].0.account = "/Users/SECRET-ACCOUNT/x".into();
    let limits = vec![snap(
        "SECRET-OWNER@example.com",
        LimitWindow::Session,
        5.0,
        1,
        LimitSource::Statusline,
    )];
    let id = ident();
    let r = build_report(&input(&id, &events, &[None], &limits)).unwrap();
    let text = serde_json::to_string(&r.bodies().unwrap()).unwrap();
    for s in [
        "SECRET-BRANCH",
        "SECRET-HASH",
        "SECRET-ACCOUNT",
        "SECRET-OWNER",
        "/Users",
    ] {
        assert!(!text.contains(s), "{s} leaked: {text}");
    }
}

#[test]
fn a_day_over_500_rows_merges_tasks_and_never_truncates() {
    let mut events = Vec::new();
    let mut tasks = Vec::new();
    for i in 0..600u64 {
        events.push(ev(
            i as i64,
            Tool::Claude,
            "s",
            "2026-11-21T01:00:00Z",
            "opus",
            Some(1.0),
        ));
        tasks.push(Some(i + 1));
    }
    let v = report(&events, &tasks, &[]);
    let rows = v["days"][0]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["events"], 600);
    assert!(rows[0]["task_id"].is_null());
}

#[test]
fn big_reports_split_into_bodies_with_limits_only_in_the_first() {
    let mut events = Vec::new();
    for d in 1..=20 {
        for m in 0..400 {
            events.push(ev(
                d * 1000 + m,
                Tool::Claude,
                "s",
                &format!("2026-10-{d:02}T01:00:00Z"),
                &format!("model-with-a-fairly-long-name-{m}"),
                Some(1.0),
            ));
        }
    }
    let tasks = vec![None; events.len()];
    let limits = [snap(
        "claude",
        LimitWindow::Weekly,
        1.0,
        1,
        LimitSource::Statusline,
    )];
    let id = ident();
    let mut inp = input(&id, &events, &tasks, &limits);
    inp.from_day = "2026-10-01".parse().unwrap();
    inp.to_day = "2026-10-31".parse().unwrap();
    let bodies = build_report(&inp).unwrap().bodies().unwrap();
    assert!(bodies.len() > 1);
    assert_eq!(bodies[0]["limits"].as_array().unwrap().len(), 1);
    assert!(bodies[1]["limits"].as_array().unwrap().is_empty());
    let days: usize = bodies
        .iter()
        .map(|b| b["days"].as_array().unwrap().len())
        .sum();
    assert_eq!(days, 20);
    for b in &bodies {
        assert!(serde_json::to_vec(b).unwrap().len() <= 256 * 1024);
    }
}

#[test]
fn reenable_is_sent_only_when_set() {
    let id = Identity {
        reenable: true,
        ..ident()
    };
    let r = build_report(&input(&id, &[], &[], &[])).unwrap();
    assert_eq!(serde_json::to_value(&r).unwrap()["reenable"], true);
    assert!(report(&[], &[], &[]).get("reenable").is_none());
}

#[test]
fn auto_throttle_skips_within_five_minutes_unless_a_limit_hit_came() {
    let n = now();
    assert!(auto_due(None, n, false));
    assert!(!auto_due(Some(n - Duration::minutes(4)), n, false));
    assert!(auto_due(Some(n - Duration::minutes(5)), n, false));
    assert!(auto_due(Some(n - Duration::minutes(1)), n, true));
    assert!(auto_due(Some(n + Duration::minutes(10)), n, false));
}

#[test]
fn day_counts_follow_the_mode() {
    assert_eq!(days_to_send(true, false, None), 2);
    assert_eq!(days_to_send(true, true, None), 30);
    assert_eq!(days_to_send(false, false, None), 30);
    assert_eq!(days_to_send(false, false, Some(7)), 7);
    assert_eq!(days_to_send(false, false, Some(900)), 120);
}

#[test]
fn labels_are_one_to_sixty_four_characters() {
    assert!(valid_label("work-mac"));
    assert!(!valid_label(""));
    assert!(!valid_label("  "));
    assert!(valid_label(&"a".repeat(64)));
    assert!(!valid_label(&"a".repeat(65)));
}

#[test]
fn settings_round_trip_and_the_id_is_created_once() {
    let s = Store::open_in_memory().unwrap();
    let id = s.machine_id().unwrap();
    assert_eq!(id.len(), 36);
    assert_eq!(id.as_bytes()[14], b'4');
    assert_eq!(id, id.to_lowercase());
    assert_eq!(s.machine_id().unwrap(), id);
    assert!(!s.machine_label().unwrap().is_empty());
    s.set_machine_label("desk").unwrap();
    assert_eq!(s.machine_label().unwrap(), "desk");
    assert!(s.set_machine_label("").is_err());
    assert!(s.machine_sync_on().unwrap());
    s.set_machine_sync(false).unwrap();
    assert!(!s.machine_sync_on().unwrap());
    assert!(!s.machine_reenable().unwrap());
    s.set_machine_sync(true).unwrap();
    assert!(s.machine_sync_on().unwrap());
    assert!(s.machine_reenable().unwrap());
    s.clear_machine_reenable().unwrap();
    assert!(!s.machine_reenable().unwrap());
    assert!(s.machine_last_sync().unwrap().is_none());
    s.set_machine_last_sync(now()).unwrap();
    assert_eq!(s.machine_last_sync().unwrap(), Some(now()));
}

fn hit(line: &str, when: DateTime<Utc>) -> Option<LimitSnapshot> {
    limit_from_hit(
        "claude-code",
        line,
        when,
        offset(3600),
        Some("Europe/Warsaw"),
    )
}

#[test]
fn limit_hit_reads_window_and_next_reset() {
    let t = at("2026-11-21T10:00:00Z");
    let s = hit("5-hour limit reached ∙ resets 3pm", t).unwrap();
    assert_eq!(s.window, LimitWindow::Session);
    assert_eq!(s.used_pct, 100.0);
    assert_eq!(s.source, LimitSource::LimitHit);
    assert_eq!(s.account, "default");
    assert_eq!(s.observed_at, t);
    assert_eq!(s.resets_at, Some(at("2026-11-21T14:00:00Z")));

    let s = hit("5-hour limit reached ∙ resets 9am", t).unwrap();
    assert_eq!(s.resets_at, Some(at("2026-11-22T08:00:00Z")));

    let s = hit("Session limit reached · resets 3:30pm (Europe/Warsaw)", t).unwrap();
    assert_eq!(s.window, LimitWindow::Session);
    assert_eq!(s.resets_at, Some(at("2026-11-21T14:30:00Z")));

    let s = hit("Weekly limit reached ∙ resets Nov 24, 9am", t).unwrap();
    assert_eq!(s.window, LimitWindow::Weekly);
    assert_eq!(s.resets_at, Some(at("2026-11-24T08:00:00Z")));

    let s = hit("Weekly limit reached ∙ resets Jan 2, 9am", t).unwrap();
    assert_eq!(s.resets_at, Some(at("2027-01-02T08:00:00Z")));

    let s = hit("Weekly limit reached ∙ resets Mon 9am", t).unwrap();
    assert_eq!(s.resets_at, Some(at("2026-11-23T08:00:00Z")));
}

#[test]
fn limit_hit_without_a_window_or_a_time_keeps_what_is_known() {
    let t = at("2026-11-21T10:00:00Z");
    assert!(hit("You've hit your limit · resets 5pm", t).is_none());
    assert!(limit_from_hit("shell", "5-hour limit reached", t, offset(0), None).is_none());
    assert!(hit("5-hour limit reached", t).unwrap().resets_at.is_none());
    let other_zone = hit("5-hour limit reached ∙ resets 3pm (America/Chicago)", t).unwrap();
    assert!(other_zone.resets_at.is_none());
    let junk = hit("5-hour limit reached ∙ resets whenever", t).unwrap();
    assert!(junk.resets_at.is_none());
    let c = limit_from_hit(
        "codex",
        "weekly limit reached resets 3pm",
        t,
        offset(0),
        None,
    )
    .unwrap();
    assert_eq!(c.tool, Tool::Codex);
}
