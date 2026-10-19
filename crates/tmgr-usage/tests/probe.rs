#![cfg(unix)]

use chrono::{TimeZone, Utc};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tmgr_usage::limits::{LimitSource, LimitWindow};
use tmgr_usage::probe::{claude_usage, codex_usage, parse_claude_usage, parse_codex_rate_limits};
use tmgr_usage::Tool;

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 19, 12, 0, 0).unwrap()
}

fn script(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("fake-tool");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn usage_response(rate_limits: serde_json::Value, available: serde_json::Value) -> String {
    json!({"type":"control_response","response":{"subtype":"success","request_id":"u1","response":{
        "subscription_type":"max","rate_limits_available":available,"rate_limits":rate_limits}}})
    .to_string()
}

fn serve(dir: &Path, line: &str) -> PathBuf {
    std::fs::write(dir.join("reply.json"), format!("{line}\n")).unwrap();
    script(
        dir,
        &format!(
            "read x\ncat '{}'\nexec cat >/dev/null",
            dir.join("reply.json").display()
        ),
    )
}

#[test]
fn parses_five_hour_seven_day_and_model_windows() {
    let v = serde_json::from_str(&usage_response(
        json!({
            "five_hour":{"utilization":11,"resets_at":"2026-10-19T18:00:00.353003+00:00"},
            "seven_day":{"utilization":20.5,"resets_at":"2026-10-23T21:59:59+00:00"},
            "seven_day_opus":null,
            "seven_day_sonnet":{"utilization":3,"resets_at":"2026-10-23T21:59:59+00:00"},
            "model_scoped":[{"display_name":"Fable","utilization":1,"resets_at":"2026-10-23T21:59:59+00:00"}],
            "limits":[{"kind":"session","percent":99,"resets_at":"2026-10-19T18:00:00+00:00"}]
        }),
        json!(true),
    ))
    .unwrap();
    let probe = parse_claude_usage(&v, now()).unwrap();
    assert_eq!(probe.subscription_type.as_deref(), Some("max"));
    assert_eq!(probe.rate_limits_available, Some(true));
    let by = |w: &LimitWindow| probe.windows.iter().find(|s| &s.window == w).unwrap();
    assert_eq!(by(&LimitWindow::Session).used_pct, 11.0);
    assert_eq!(by(&LimitWindow::Weekly).used_pct, 20.5);
    assert_eq!(by(&LimitWindow::Model("sonnet".into())).used_pct, 3.0);
    assert_eq!(by(&LimitWindow::Model("fable".into())).used_pct, 1.0);
    assert_eq!(probe.windows.len(), 4);
    let s = by(&LimitWindow::Session);
    assert_eq!(s.source, LimitSource::Probe);
    assert_eq!(s.tool, Tool::Claude);
    assert_eq!(s.observed_at, now());
    assert!(s.resets_at.is_some());
}

#[test]
fn falls_back_to_limits_array_when_named_windows_are_missing() {
    let v = serde_json::from_str(&usage_response(
        json!({"limits":[
            {"kind":"session","percent":13,"resets_at":"2026-10-19T18:00:00+00:00"},
            {"kind":"weekly_all","percent":21,"resets_at":"2026-10-23T21:59:59+00:00"},
            {"kind":"weekly_scoped","percent":1,"resets_at":"2026-10-23T21:59:59+00:00",
             "scope":{"model":{"id":null,"display_name":"Fable"}}}
        ]}),
        json!(true),
    ))
    .unwrap();
    let probe = parse_claude_usage(&v, now()).unwrap();
    let keys: Vec<String> = probe.windows.iter().map(|w| w.window.as_key()).collect();
    assert_eq!(keys, ["session", "weekly", "model:fable"]);
}

#[test]
fn missing_or_bad_fields_never_become_numbers() {
    let v = serde_json::from_str(&usage_response(
        json!({
            "five_hour":{"resets_at":"2026-10-19T18:00:00+00:00"},
            "seven_day":{"utilization":"20"},
            "seven_day_sonnet":{"utilization":-4},
            "model_scoped":[{"utilization":5},{"display_name":"../x","utilization":5}]
        }),
        json!(true),
    ))
    .unwrap();
    assert!(parse_claude_usage(&v, now()).unwrap().windows.is_empty());
}

#[test]
fn unparsable_reset_time_keeps_the_number_without_a_reset() {
    let v = serde_json::from_str(&usage_response(
        json!({"five_hour":{"utilization":7,"resets_at":"soon"}}),
        json!(true),
    ))
    .unwrap();
    let w = &parse_claude_usage(&v, now()).unwrap().windows[0];
    assert_eq!(w.used_pct, 7.0);
    assert_eq!(w.resets_at, None);
}

#[test]
fn utilization_above_one_hundred_is_capped() {
    let v = serde_json::from_str(&usage_response(
        json!({"five_hour":{"utilization":137,"resets_at":"2026-10-19T18:00:00+00:00"}}),
        json!(true),
    ))
    .unwrap();
    assert_eq!(
        parse_claude_usage(&v, now()).unwrap().windows[0].used_pct,
        100.0
    );
}

#[test]
fn api_key_account_has_no_windows() {
    let v = serde_json::from_str(&usage_response(
        json!({"five_hour":{"utilization":50,"resets_at":"2026-10-19T18:00:00+00:00"}}),
        json!(false),
    ))
    .unwrap();
    let probe = parse_claude_usage(&v, now()).unwrap();
    assert!(probe.windows.is_empty());
    assert_eq!(probe.rate_limits_available, Some(false));
}

#[test]
fn error_response_is_an_error() {
    let v = json!({"type":"control_response","response":{"subtype":"error","request_id":"u1","error":"nope"}});
    assert!(parse_claude_usage(&v, now()).is_err());
}

#[test]
fn claude_probe_reads_the_reply_of_a_fake_binary() {
    let dir = tempfile::tempdir().unwrap();
    let line = usage_response(
        json!({"five_hour":{"utilization":11,"resets_at":"2026-10-19T18:00:00+00:00"}}),
        json!(true),
    );
    let bin = serve(dir.path(), &line);
    let probe = claude_usage(&bin, Duration::from_secs(10)).unwrap();
    assert_eq!(probe.windows.len(), 1);
    assert_eq!(probe.windows[0].used_pct, 11.0);
}

#[test]
fn claude_probe_skips_other_lines_and_other_request_ids() {
    let dir = tempfile::tempdir().unwrap();
    let other = json!({"type":"control_response","response":{"subtype":"success","request_id":"zz","response":{}}});
    let line = usage_response(
        json!({"seven_day":{"utilization":9,"resets_at":"2026-10-23T21:59:59+00:00"}}),
        json!(true),
    );
    let bin = serve(
        dir.path(),
        &format!("not json\n{other}\n{{\"type\":\"system\"}}\n{line}"),
    );
    let probe = claude_usage(&bin, Duration::from_secs(10)).unwrap();
    assert_eq!(probe.windows[0].window, LimitWindow::Weekly);
}

#[test]
fn claude_probe_rejects_wrong_json_and_silent_exit() {
    let dir = tempfile::tempdir().unwrap();
    let bin = serve(dir.path(), "{\"type\":\"result\"}");
    assert!(claude_usage(&bin, Duration::from_millis(800)).is_err());
    let bin = script(dir.path(), "exit 3");
    assert!(claude_usage(&bin, Duration::from_secs(10)).is_err());
    assert!(claude_usage(&dir.path().join("missing"), Duration::from_secs(10)).is_err());
}

#[test]
fn claude_probe_times_out_and_kills_the_child() {
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("pid");
    let bin = script(
        dir.path(),
        &format!("echo $$ > '{}'\nexec sleep 30", pidfile.display()),
    );
    let started = Instant::now();
    let err = claude_usage(&bin, Duration::from_secs(3)).unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(err.to_string().contains("timed out"));
    let pid = std::fs::read_to_string(&pidfile).unwrap();
    let alive = std::process::Command::new("kill")
        .args(["-0", pid.trim()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success();
    assert!(!alive);
}

#[test]
fn claude_probe_passes_the_safe_flags() {
    let dir = tempfile::tempdir().unwrap();
    let argfile = dir.path().join("args");
    let line = usage_response(json!({}), json!(true));
    std::fs::write(dir.path().join("reply.json"), format!("{line}\n")).unwrap();
    let bin = script(
        dir.path(),
        &format!(
            "echo \"$@\" > '{}'\nread x\ncat '{}'\nexec cat >/dev/null",
            argfile.display(),
            dir.path().join("reply.json").display()
        ),
    );
    claude_usage(&bin, Duration::from_secs(10)).unwrap();
    let args = std::fs::read_to_string(&argfile).unwrap();
    for want in [
        "--input-format stream-json",
        "--output-format stream-json",
        "--setting-sources",
        "--strict-mcp-config",
    ] {
        assert!(args.contains(want), "{args}");
    }
}

fn codex_result(rate_limits: serde_json::Value) -> serde_json::Value {
    json!({"ordinaryUsageAllowed":true,"rateLimits":rate_limits})
}

#[test]
fn codex_windows_are_classified_by_minutes() {
    let r = codex_result(json!({
        "planType":"prolite",
        "primary":{"usedPercent":48,"windowDurationMins":10080,"resetsAt":1792874648},
        "secondary":null}));
    let probe = parse_codex_rate_limits(&r, now()).unwrap();
    assert_eq!(probe.plan.as_deref(), Some("prolite"));
    assert_eq!(probe.windows.len(), 1);
    assert_eq!(probe.windows[0].window, LimitWindow::Weekly);
    assert_eq!(probe.windows[0].used_pct, 48.0);
    assert_eq!(probe.windows[0].source, LimitSource::Probe);
    assert_eq!(probe.windows[0].tool, Tool::Codex);

    let r = codex_result(json!({
        "primary":{"usedPercent":10,"windowDurationMins":300,"resetsAt":1792874648},
        "secondary":{"usedPercent":20,"windowDurationMins":10080,"resetsAt":1792874648}}));
    let probe = parse_codex_rate_limits(&r, now()).unwrap();
    let keys: Vec<String> = probe.windows.iter().map(|w| w.window.as_key()).collect();
    assert_eq!(keys, ["session", "weekly"]);
}

#[test]
fn codex_windows_without_minutes_or_percent_are_dropped() {
    let r = codex_result(json!({
        "primary":{"usedPercent":10,"windowDurationMins":null},
        "secondary":{"windowDurationMins":300}}));
    assert!(parse_codex_rate_limits(&r, now())
        .unwrap()
        .windows
        .is_empty());
    assert!(parse_codex_rate_limits(&json!({}), now()).is_err());
}

#[test]
fn codex_probe_talks_json_rpc_to_a_fake_binary() {
    let dir = tempfile::tempdir().unwrap();
    let init = json!({"id":1,"result":{"userAgent":"x"}});
    let note = json!({"method":"account/updated","params":{}});
    let reply = json!({"id":2,"result":codex_result(json!({
        "primary":{"usedPercent":30,"windowDurationMins":300,"resetsAt":1792874648}}))});
    std::fs::write(dir.path().join("out"), format!("{init}\n{note}\n{reply}\n")).unwrap();
    let bin = script(
        dir.path(),
        &format!(
            "cat '{}'\nexec cat >/dev/null",
            dir.path().join("out").display()
        ),
    );
    let probe = codex_usage(&bin, Duration::from_secs(10)).unwrap();
    assert_eq!(probe.windows[0].window, LimitWindow::Session);
}

#[test]
fn codex_probe_reports_rpc_errors_and_timeouts() {
    let dir = tempfile::tempdir().unwrap();
    let reply = json!({"id":2,"error":{"code":-32601,"message":"no"}});
    std::fs::write(dir.path().join("out"), format!("{reply}\n")).unwrap();
    let bin = script(
        dir.path(),
        &format!(
            "cat '{}'\nexec cat >/dev/null",
            dir.path().join("out").display()
        ),
    );
    assert!(codex_usage(&bin, Duration::from_secs(10)).is_err());
    let bin = script(dir.path(), "exec sleep 30");
    let err = codex_usage(&bin, Duration::from_millis(500)).unwrap_err();
    assert!(err.to_string().contains("timed out"));
}

#[test]
fn a_timed_out_probe_kills_grandchildren_too_and_returns_promptly() {
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("grandchild");
    let bin = script(
        dir.path(),
        &format!("sleep 30 &\necho $! > '{}'\nwait", pidfile.display()),
    );
    let started = Instant::now();
    let err = claude_usage(&bin, Duration::from_secs(2)).unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(6));
    assert!(err.to_string().contains("timed out"));
    let pid = std::fs::read_to_string(&pidfile).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let alive = std::process::Command::new("kill")
        .args(["-0", pid.trim()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success();
    assert!(!alive, "the grandchild survived the timeout");
}
