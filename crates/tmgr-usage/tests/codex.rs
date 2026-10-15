mod common;
use common::*;
use std::io::Write;
use std::path::{Path, PathBuf};
use tmgr_usage::codex::CodexCollector;
use tmgr_usage::collector::{Collector, LogFile};
use tmgr_usage::limits::{LimitSource, LimitWindow};
use tmgr_usage::{
    scan_account, Account, ParsedRecord, ParserState, SessionMeta, Tokens, Tool, UsageEvent,
};

fn ts(n: u32) -> String {
    format!("2026-10-14T10:{:02}:{:02}Z", n / 60, n % 60)
}

fn meta(id: &str, parent: Option<&str>) -> String {
    let parent = parent
        .map(|p| format!(r#","parent_thread_id":"{p}""#))
        .unwrap_or_default();
    format!(
        r#"{{"timestamp":"{}","type":"session_meta","payload":{{"id":"{id}","session_id":"root","cwd":"/work/alpha","cli_version":"0.150.0","git":{{"branch":"feat/x","commit_hash":"abc"}}{parent},"base_instructions":{{"text":"SECRET-BASEINSTR"}}}}}}"#,
        ts(0)
    )
}

fn turn(t: u32, model: &str) -> String {
    format!(
        r#"{{"timestamp":"{}","type":"turn_context","payload":{{"model":"{model}","cwd":"/work/alpha","user_instructions":"SECRET-PROMPT-1"}}}}"#,
        ts(t)
    )
}

fn usage(i: u64, c: u64, o: u64, r: u64) -> String {
    format!(
        r#"{{"input_tokens":{i},"cached_input_tokens":{c},"output_tokens":{o},"reasoning_output_tokens":{r},"total_tokens":{}}}"#,
        i + o
    )
}

fn tc(t: u32, total: Option<(u64, u64, u64, u64)>, last: Option<(u64, u64, u64, u64)>) -> String {
    let f = |x: Option<(u64, u64, u64, u64)>| match x {
        Some((i, c, o, r)) => usage(i, c, o, r),
        None => "null".into(),
    };
    format!(
        r#"{{"timestamp":"{}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{},"last_token_usage":{}}},"rate_limits":null}}}}"#,
        ts(t),
        f(total),
        f(last)
    )
}

fn limits_line(t: u32, info: &str, rl: &str) -> String {
    format!(
        r#"{{"timestamp":"{}","type":"event_msg","payload":{{"type":"token_count","info":{info},"rate_limits":{rl}}}}}"#,
        ts(t)
    )
}

fn event_msg(t: u32, body: &str) -> String {
    format!(
        r#"{{"timestamp":"{}","type":"event_msg","payload":{body}}}"#,
        ts(t)
    )
}

fn task_started(t: u32) -> String {
    event_msg(t, r#"{"type":"task_started","turn_id":"t"}"#)
}

fn response_item(t: u32) -> String {
    format!(
        r#"{{"timestamp":"{}","type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":"SECRET-RESPONSE-1 token_count"}}]}}}}"#,
        ts(t)
    )
}

fn logfile(path: &Path) -> LogFile {
    LogFile {
        path: path.to_path_buf(),
        rel: "sessions/x.jsonl".into(),
        account: "codex".into(),
        tool: Tool::Codex,
    }
}

fn write(path: &Path, lines: &[String]) {
    write_lines(path, lines);
}

struct Parsed {
    events: Vec<UsageEvent>,
    sessions: Vec<SessionMeta>,
    limits: Vec<tmgr_usage::limits::LimitSnapshot>,
}

fn run(lines: &[String]) -> Parsed {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-a.jsonl");
    write(&path, lines);
    let mut state = ParserState::default();
    parse_with(&path, 0, &mut state).0
}

fn parse_with(path: &Path, from: u64, state: &mut ParserState) -> (Parsed, u64) {
    let mut out = Parsed {
        events: vec![],
        sessions: vec![],
        limits: vec![],
    };
    let outcome = CodexCollector
        .parse(&logfile(path), from, state, &mut |r| match r {
            ParsedRecord::Event(e) => out.events.push(e),
            ParsedRecord::Session(s) => out.sessions.push(s),
            ParsedRecord::Limit(l) => out.limits.push(l),
            _ => {}
        })
        .unwrap();
    (out, outcome.next_offset)
}

fn toks(i: u64, o: u64, cr: u64, re: u64) -> Tokens {
    Tokens {
        input: i,
        output: o,
        cache_read: cr,
        cache_write_5m: 0,
        cache_write_1h: 0,
        reasoning: re,
    }
}

#[test]
fn deltas_from_cumulative_totals_skip_repeats_and_reset() {
    let p = run(&[
        meta("s1", None),
        turn(1, "gpt-test"),
        tc(2, Some((100, 40, 10, 4)), Some((100, 40, 10, 4))),
        tc(3, Some((100, 40, 10, 4)), Some((100, 40, 10, 4))),
        tc(4, Some((250, 100, 30, 9)), Some((150, 60, 20, 5))),
        tc(5, Some((20, 0, 5, 0)), Some((20, 0, 5, 0))),
    ]);
    let t: Vec<Tokens> = p.events.iter().map(|e| e.tokens).collect();
    assert_eq!(
        t,
        vec![toks(60, 10, 40, 4), toks(90, 20, 60, 5), toks(20, 5, 0, 0)]
    );
    assert_eq!(p.events[0].event_key, "s1|1");
    assert_eq!(p.events[1].event_key, "s1|3");
    assert_eq!(p.events[2].event_key, "s1|4");
    assert!(p.events.iter().all(|e| e.model == "gpt-test"
        && e.cwd.as_deref() == Some("/work/alpha")
        && e.git_branch.as_deref() == Some("feat/x")
        && !e.is_sidechain));
    assert_eq!(p.sessions.len(), 1);
    assert_eq!(p.sessions[0].tool_session_id, "s1");
}

#[test]
fn plain_input_never_negative_and_cache_write_is_separate() {
    let line = format!(
        r#"{{"timestamp":"{}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":5,"cached_input_tokens":9,"cache_write_input_tokens":3,"output_tokens":2,"reasoning_output_tokens":1,"total_tokens":7}},"last_token_usage":null}}}}}}"#,
        ts(2)
    );
    let p = run(&[meta("s1", None), line]);
    assert_eq!(
        p.events[0].tokens,
        Tokens {
            input: 0,
            output: 2,
            cache_read: 9,
            cache_write_5m: 3,
            cache_write_1h: 0,
            reasoning: 1
        }
    );
}

#[test]
fn cache_write_tokens_are_not_billed_again_as_plain_input() {
    let line = format!(
        r#"{{"timestamp":"{}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":1000,"cached_input_tokens":200,"cache_write_input_tokens":100,"output_tokens":50,"reasoning_output_tokens":0,"total_tokens":1050}},"last_token_usage":null}}}}}}"#,
        ts(2)
    );
    let p = run(&[meta("s1", None), line]);
    assert_eq!(p.events[0].tokens.input, 700);
    assert_eq!(p.events[0].tokens.cache_read, 200);
    assert_eq!(p.events[0].tokens.cache_write_5m, 100);
}

#[test]
fn last_token_usage_is_the_fallback_without_a_previous_total() {
    let p = run(&[
        meta("s1", None),
        tc(2, Some((900, 0, 90, 0)), Some((30, 0, 3, 0))),
        tc(3, None, Some((10, 0, 1, 0))),
    ]);
    let t: Vec<Tokens> = p.events.iter().map(|e| e.tokens).collect();
    assert_eq!(t, vec![toks(30, 3, 0, 0), toks(10, 1, 0, 0)]);
}

#[test]
fn null_info_yields_limits_but_no_event() {
    let rl = r#"{"plan_type":"pro","primary":{"used_percent":12.5,"window_minutes":300,"resets_at":1791000000},"secondary":{"used_percent":40.0,"window_minutes":10080,"resets_at":1791500000}}"#;
    let p = run(&[meta("s1", None), limits_line(2, "null", rl)]);
    assert!(p.events.is_empty());
    assert_eq!(p.limits.len(), 2);
    let session = &p.limits[0];
    assert_eq!(session.window, LimitWindow::Session);
    assert_eq!(session.used_pct, 12.5);
    assert_eq!(session.plan.as_deref(), Some("pro"));
    assert_eq!(session.source, LimitSource::Rollout);
    assert_eq!(session.resets_at.unwrap().timestamp(), 1791000000);
    assert_eq!(
        session.observed_at.to_rfc3339(),
        "2026-10-14T10:00:02+00:00"
    );
    assert_eq!(p.limits[1].window, LimitWindow::Weekly);
}

#[test]
fn weekly_only_limits_and_custom_window() {
    let rl = r#"{"plan_type":"free","primary":{"used_percent":7.0,"window_minutes":10080,"resets_at":1791500000},"secondary":null}"#;
    let odd = r#"{"primary":{"used_percent":1.0,"window_minutes":90,"resets_at":1791500000},"secondary":null}"#;
    let p = run(&[
        meta("s1", None),
        limits_line(2, "null", rl),
        limits_line(3, "null", odd),
    ]);
    assert_eq!(p.limits.len(), 2);
    assert_eq!(p.limits[0].window, LimitWindow::Weekly);
    assert_eq!(p.limits[1].window, LimitWindow::Model("window:90m".into()));
}

#[test]
fn model_change_between_turns_and_unknown_model() {
    let p = run(&[
        meta("s1", None),
        tc(1, Some((10, 0, 1, 0)), None),
        turn(2, "gpt-a"),
        tc(3, Some((20, 0, 2, 0)), None),
        turn(4, "gpt-b"),
        tc(5, Some((30, 0, 3, 0)), None),
    ]);
    let models: Vec<&str> = p.events.iter().map(|e| e.model.as_str()).collect();
    assert_eq!(models, vec!["unknown", "gpt-a", "gpt-b"]);

    let (cost, basis) = book().price(&p.events[0]);
    assert_eq!(cost, None);
    assert_eq!(basis, tmgr_usage::CostBasis::Unpriced);
}

#[test]
fn service_tier_follows_thread_settings() {
    let p = run(&[
        meta("s1", None),
        tc(1, Some((10, 0, 1, 0)), None),
        event_msg(
            2,
            r#"{"type":"thread_settings_applied","thread_settings":{"service_tier":"fast"}}"#,
        ),
        tc(3, Some((20, 0, 2, 0)), None),
        event_msg(
            4,
            r#"{"type":"thread_settings_applied","thread_settings":{}}"#,
        ),
        tc(5, Some((30, 0, 3, 0)), None),
        event_msg(
            6,
            r#"{"type":"thread_settings_applied","thread_settings":{"service_tier":"mystery"}}"#,
        ),
        tc(7, Some((40, 0, 4, 0)), None),
    ]);
    let tiers: Vec<Option<&str>> = p.events.iter().map(|e| e.service_tier.as_deref()).collect();
    assert_eq!(tiers, vec![None, Some("priority"), Some("priority"), None]);
}

#[test]
fn content_lines_are_ignored_even_when_they_mention_markers() {
    let p = run(&[
        meta("s1", None),
        response_item(1),
        tc(2, Some((10, 0, 1, 0)), None),
    ]);
    assert_eq!(p.events.len(), 1);
}

#[test]
fn incremental_parse_continues_exactly() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-a.jsonl");
    let all = vec![
        meta("s1", None),
        turn(1, "gpt-a"),
        tc(2, Some((10, 0, 1, 0)), None),
        tc(3, Some((10, 0, 1, 0)), None),
        tc(4, Some((25, 5, 4, 0)), None),
        tc(5, Some((40, 5, 9, 0)), None),
    ];
    write(&path, &all[..4]);
    let mut state = ParserState::default();
    let (first, off) = parse_with(&path, 0, &mut state);
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    let rest = format!("{}\n{}", all[4], &all[5][..30]);
    f.write_all(rest.as_bytes()).unwrap();
    let (second, off2) = parse_with(&path, off, &mut state);
    f.write_all(format!("{}\n", &all[5][30..]).as_bytes())
        .unwrap();
    let (third, _) = parse_with(&path, off2, &mut state);

    let whole = run(&all);
    let mut joined = first.events;
    joined.extend(second.events);
    joined.extend(third.events);
    assert_eq!(joined, whole.events);
    assert_eq!(joined.len(), 3);
}

#[test]
fn truncated_last_line_is_not_consumed() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-a.jsonl");
    write(&path, &[meta("s1", None)]);
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    f.write_all(&tc(2, Some((10, 0, 1, 0)), None).as_bytes()[..40])
        .unwrap();
    let mut state = ParserState::default();
    let (p, off) = parse_with(&path, 0, &mut state);
    assert!(p.events.is_empty());
    assert_eq!(off, meta("s1", None).len() as u64 + 1);
}

fn child_lines(with_trigger: bool) -> Vec<String> {
    let mut v = vec![
        meta("child", Some("parent")),
        turn(1, "gpt-a"),
        tc(2, Some((100, 0, 10, 0)), Some((100, 0, 10, 0))),
        limits_line(
            3,
            r#"{"total_token_usage":{"input_tokens":300,"cached_input_tokens":0,"output_tokens":30,"reasoning_output_tokens":0,"total_tokens":330},"last_token_usage":null}"#,
            r#"{"primary":{"used_percent":99.0,"window_minutes":300,"resets_at":1791000000}}"#,
        ),
    ];
    if with_trigger {
        v.push(
            r#"{"timestamp":"2026-10-14T10:00:04Z","type":"inter_agent_communication_metadata","payload":{"trigger_turn":false}}"#.into(),
        );
        v.push(tc(4, Some((310, 0, 31, 0)), None));
        v.push(
            r#"{"timestamp":"2026-10-14T10:00:05Z","type":"inter_agent_communication_metadata","payload":{"trigger_turn":true}}"#.into(),
        );
    } else {
        v.push(task_started(5));
    }
    v.push(tc(6, Some((340, 0, 36, 0)), None));
    v.push(tc(7, Some((360, 0, 40, 0)), None));
    v
}

#[test]
fn child_rollout_skips_the_replayed_parent_prefix_task_started() {
    let p = run(&child_lines(false));
    let t: Vec<Tokens> = p.events.iter().map(|e| e.tokens).collect();
    assert_eq!(t, vec![toks(40, 6, 0, 0), toks(20, 4, 0, 0)]);
    assert!(p.events.iter().all(|e| e.is_sidechain));
    assert_eq!(p.events[0].event_key, "child|1");
    assert_eq!(p.sessions[0].parent_session_id.as_deref(), Some("parent"));
    assert!(p.limits.is_empty());
}

#[test]
fn child_rollout_trigger_turn_true_ends_the_prefix_and_false_does_not() {
    let p = run(&child_lines(true));
    let t: Vec<Tokens> = p.events.iter().map(|e| e.tokens).collect();
    assert_eq!(t, vec![toks(30, 5, 0, 0), toks(20, 4, 0, 0)]);
}

#[test]
fn parent_only_rollout_counts_everything() {
    let p = run(&[
        meta("p", None),
        task_started(1),
        tc(2, Some((10, 0, 1, 0)), None),
        task_started(3),
        tc(4, Some((30, 0, 3, 0)), None),
    ]);
    assert_eq!(p.events.len(), 2);
}

fn record(t: u32, id: &str, i: u64, o: u64) -> String {
    format!(
        r#"{{"timestamp":"{}","type":"token_usage_record","payload":{{"response_id":"{id}","usage":{}}}}}"#,
        ts(t),
        usage(i, 0, o, 0)
    )
}

fn compacted(t: u32, id: &str) -> String {
    format!(
        r#"{{"timestamp":"{}","type":"compacted","payload":{{"compaction_response_id":"{id}","message":"SECRET-OUT-1"}}}}"#,
        ts(t)
    )
}

#[test]
fn compaction_record_counts_only_when_a_compacted_marker_matches() {
    let p = run(&[
        meta("s1", None),
        tc(1, Some((10, 0, 1, 0)), None),
        record(2, "r-orphan", 500, 50),
        record(3, "r-remote", 700, 70),
        compacted(4, "r-remote"),
    ]);
    let keys: Vec<&str> = p.events.iter().map(|e| e.event_key.as_str()).collect();
    assert_eq!(keys, vec!["s1|1", "s1|compact:r-remote"]);
    assert_eq!(p.events[1].tokens, toks(700, 70, 0, 0));
    assert_eq!(p.events[1].model, "unknown");
}

#[test]
fn compaction_marker_before_record_also_counts_once() {
    let p = run(&[
        meta("s1", None),
        turn(1, "gpt-a"),
        compacted(2, "r1"),
        record(3, "r1", 80, 8),
        record(4, "r1", 80, 8),
    ]);
    assert_eq!(p.events.len(), 1);
    assert_eq!(p.events[0].model, "gpt-a");
}

#[test]
fn local_compaction_already_covered_by_an_advancing_token_count() {
    let p = run(&[
        meta("s1", None),
        tc(1, Some((10, 0, 1, 0)), None),
        record(2, "r-local", 90, 9),
        tc(3, Some((100, 0, 10, 0)), None),
        compacted(4, "r-local"),
        tc(5, Some((105, 0, 11, 0)), None),
    ]);
    let t: Vec<Tokens> = p.events.iter().map(|e| e.tokens).collect();
    assert_eq!(
        t,
        vec![toks(10, 1, 0, 0), toks(90, 9, 0, 0), toks(5, 1, 0, 0)]
    );
}

#[test]
fn local_compaction_is_counted_once_whichever_side_of_the_marker_the_record_is_on() {
    let before = run(&[
        meta("s1", None),
        tc(1, Some((10, 0, 1, 0)), None),
        record(2, "r-local", 90, 9),
        compacted(3, "r-local"),
        tc(4, Some((100, 0, 10, 0)), None),
    ]);
    let after = run(&[
        meta("s1", None),
        tc(1, Some((10, 0, 1, 0)), None),
        compacted(2, "r-local"),
        record(3, "r-local", 90, 9),
        tc(4, Some((100, 0, 10, 0)), None),
    ]);
    for p in [before, after] {
        let t: Vec<Tokens> = p.events.iter().map(|e| e.tokens).collect();
        assert_eq!(t, vec![toks(10, 1, 0, 0), toks(90, 9, 0, 0)]);
    }
}

#[test]
fn compaction_inside_a_replayed_prefix_is_ignored() {
    let p = run(&[
        meta("child", Some("parent")),
        record(1, "r1", 80, 8),
        compacted(2, "r1"),
        task_started(3),
        tc(4, Some((10, 0, 1, 0)), None),
    ]);
    assert_eq!(p.events.len(), 1);
}

#[test]
fn compaction_state_survives_incremental_parsing() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-a.jsonl");
    write(&path, &[meta("s1", None), record(2, "r1", 80, 8)]);
    let mut state = ParserState::default();
    let (a, off) = parse_with(&path, 0, &mut state);
    assert!(a.events.is_empty());
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    f.write_all(format!("{}\n", compacted(3, "r1")).as_bytes())
        .unwrap();
    let (b, _) = parse_with(&path, off, &mut state);
    assert_eq!(b.events.len(), 1);
}

fn account(root: &Path) -> Account {
    Account {
        tool: Tool::Codex,
        label: "codex".into(),
        root: root.to_path_buf(),
    }
}

fn rollout_lines() -> Vec<String> {
    vec![
        meta("s1", None),
        turn(1, "gpt-a"),
        tc(2, Some((10, 0, 1, 0)), None),
        tc(3, Some((30, 0, 5, 0)), None),
    ]
}

#[test]
fn active_copy_wins_over_archived_for_the_same_relative_path() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("codex");
    let name = "rollout-2026-10-14T10-00-00-s1.jsonl";
    write(
        &root.join("sessions/2026/10/14").join(name),
        &rollout_lines(),
    );
    write(
        &root.join("archived_sessions/2026/10/14").join(name),
        &rollout_lines(),
    );
    write(
        &root.join("archived_sessions/2026/09/01/rollout-old.jsonl"),
        &[meta("old", None), tc(2, Some((7, 0, 1, 0)), None)],
    );
    write(
        &root.join("sessions/2026/10/14/notes.jsonl"),
        &rollout_lines(),
    );
    let files: Vec<PathBuf> = CodexCollector
        .discover(&account(&root))
        .into_iter()
        .map(|f| f.path)
        .collect();
    assert_eq!(files.len(), 2);
    assert!(files.iter().any(|p| p.starts_with(root.join("sessions"))));
    assert!(files.iter().any(|p| p.ends_with("rollout-old.jsonl")));
}

#[test]
fn archived_copy_with_another_layout_dedupes_by_event_key() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("codex");
    write(
        &root.join("sessions/2026/10/14/rollout-a.jsonl"),
        &rollout_lines(),
    );
    write(
        &root.join("archived_sessions/rollout-a.jsonl"),
        &rollout_lines(),
    );
    let mut store = open_store(tmp.path());
    let report = scan_account(&mut store, &account(&root), &book(), now()).unwrap();
    assert_eq!(report.events_new, 2);
    assert_eq!(store.event_count().unwrap(), 2);
}

#[test]
fn scan_is_incremental_and_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("codex");
    let path = root.join("sessions/2026/10/14/rollout-a.jsonl");
    write(&path, &rollout_lines());
    let mut store = open_store(tmp.path());
    let acc = account(&root);
    assert_eq!(
        scan_account(&mut store, &acc, &book(), now())
            .unwrap()
            .events_new,
        2
    );
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    f.write_all(format!("{}\n", tc(4, Some((50, 0, 8, 0)), None)).as_bytes())
        .unwrap();
    let r = scan_account(&mut store, &acc, &book(), now()).unwrap();
    assert_eq!(r.events_new, 1);
    assert_eq!(store.event_count().unwrap(), 3);
}

#[test]
fn session_upsert_rule_is_safe_for_codex_keys() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("codex");
    let lines = rollout_lines();
    write(&root.join("sessions/2026/10/14/rollout-a.jsonl"), &lines);
    let mut store = open_store(tmp.path());
    scan_account(&mut store, &account(&root), &book(), now()).unwrap();
    std::fs::remove_file(root.join("sessions/2026/10/14/rollout-a.jsonl")).unwrap();
    write(&root.join("archived_sessions/rollout-a.jsonl"), &lines);
    let r = scan_account(&mut store, &account(&root), &book(), now()).unwrap();
    assert_eq!((r.events_new, r.events_updated), (0, 0));
}

#[test]
fn stale_and_expired_windows_are_dropped_from_current_limits() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("codex");
    let old = r#"{"plan_type":"pro","primary":{"used_percent":90.0,"window_minutes":300,"resets_at":1791000000},"secondary":{"used_percent":10.0,"window_minutes":10080,"resets_at":1799000000}}"#;
    let new = r#"{"plan_type":"pro","primary":{"used_percent":5.0,"window_minutes":300,"resets_at":1791100000},"secondary":{"used_percent":11.0,"window_minutes":10080,"resets_at":1799000000}}"#;
    write(
        &root.join("sessions/2026/10/14/rollout-a.jsonl"),
        &[meta("s1", None), limits_line(1, "null", old)],
    );
    write(
        &root.join("sessions/2026/10/14/rollout-b.jsonl"),
        &[meta("s2", None), limits_line(9, "null", new)],
    );
    let mut store = open_store(tmp.path());
    scan_account(&mut store, &account(&root), &book(), now()).unwrap();
    let at = chrono::DateTime::from_timestamp(1790990000, 0).unwrap();
    let cur = store.current_limits(at).unwrap();
    assert_eq!(cur.len(), 2);
    let session = cur
        .iter()
        .find(|l| l.window == LimitWindow::Session)
        .unwrap();
    assert_eq!(session.used_pct, 5.0);
    assert_eq!(session.tool_session_id, "s2");
    let weekly = cur
        .iter()
        .find(|l| l.window == LimitWindow::Weekly)
        .unwrap();
    assert_eq!(weekly.used_pct, 11.0);

    let later = chrono::DateTime::from_timestamp(1791200000, 0).unwrap();
    let cur = store.current_limits(later).unwrap();
    assert_eq!(cur.len(), 1);
    assert_eq!(cur[0].window, LimitWindow::Weekly);
}

#[test]
fn a_child_thread_keeps_its_parent_link_across_prune_and_later_events() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("codex");
    let path = root.join("sessions/2026/10/14/rollout-child.jsonl");
    write(&path, &[meta("child", Some("parent")), turn(1, "gpt-a")]);
    let mut store = open_store(tmp.path());
    let acc = account(&root);
    scan_account(&mut store, &acc, &book(), now()).unwrap();
    store.prune(90, now()).unwrap();
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    f.write_all(
        format!(
            "{}\n{}\n",
            task_started(2),
            tc(3, Some((50, 0, 8, 0)), None)
        )
        .as_bytes(),
    )
    .unwrap();
    scan_account(&mut store, &acc, &book(), now()).unwrap();
    let events = store
        .link_events(now() - chrono::Duration::days(1), now())
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].tool_session_id, "child");
    assert_eq!(events[0].parent_session_id.as_deref(), Some("parent"));
}
