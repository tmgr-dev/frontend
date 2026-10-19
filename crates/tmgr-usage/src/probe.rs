use chrono::{DateTime, TimeZone, Utc};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::codex::classify;
use crate::error::{Result, UsageError};
use crate::limits::{LimitSnapshot, LimitSource, LimitWindow};
use crate::model::{clean_name, Tool};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

const CLAUDE_REQUEST_ID: &str = "u1";
const MAX_LINE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct ClaudeProbe {
    pub subscription_type: Option<String>,
    pub rate_limits_available: Option<bool>,
    pub windows: Vec<LimitSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodexProbe {
    pub plan: Option<String>,
    pub ordinary_usage_allowed: Option<bool>,
    pub windows: Vec<LimitSnapshot>,
}

/// Asks the installed `claude` for the account's rate-limit windows. No model turn is made.
/// The control request is experimental in Claude Code, so every field is read defensively and a
/// missing or malformed one yields no window instead of a guess.
pub fn claude_usage(claude_bin: &Path, timeout: Duration) -> Result<ClaudeProbe> {
    let request = json!({
        "type": "control_request",
        "request_id": CLAUDE_REQUEST_ID,
        "request": {"subtype": "get_usage"},
    });
    let reply = converse(
        claude_bin,
        &[
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--setting-sources",
            "",
            "--strict-mcp-config",
        ],
        vec![request.to_string()],
        timeout,
        |v| {
            (v.get("type").and_then(Value::as_str) == Some("control_response")
                && v.pointer("/response/request_id").and_then(Value::as_str)
                    == Some(CLAUDE_REQUEST_ID))
            .then(|| v.clone())
        },
    )?;
    parse_claude_usage(&reply, Utc::now())
}

pub fn parse_claude_usage(reply: &Value, observed_at: DateTime<Utc>) -> Result<ClaudeProbe> {
    let response = reply.get("response").ok_or_else(bad_reply)?;
    if response.get("subtype").and_then(Value::as_str) != Some("success") {
        return Err(UsageError::Invalid("claude usage request failed".into()));
    }
    let body = response.get("response").ok_or_else(bad_reply)?;
    let available = body.get("rate_limits_available").and_then(Value::as_bool);
    let mut probe = ClaudeProbe {
        subscription_type: body
            .get("subscription_type")
            .and_then(Value::as_str)
            .map(|s| clean_name(s, 40))
            .filter(|s| s != "other"),
        rate_limits_available: available,
        windows: Vec::new(),
    };
    if available == Some(false) {
        return Ok(probe);
    }
    let Some(limits) = body.get("rate_limits").filter(|v| v.is_object()) else {
        return Ok(probe);
    };
    let mut add = |window: LimitWindow, entry: Option<(&Value, &str)>| {
        let Some((v, pct_key)) = entry else { return };
        let Some(used_pct) = v.get(pct_key).and_then(percent) else {
            return;
        };
        if probe.windows.iter().any(|w| w.window == window) {
            return;
        }
        probe.windows.push(LimitSnapshot {
            tool: Tool::Claude,
            account: Tool::Claude.as_str().to_string(),
            window,
            used_pct,
            resets_at: v
                .get("resets_at")
                .and_then(Value::as_str)
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.with_timezone(&Utc)),
            plan: probe_plan(body),
            observed_at,
            source: LimitSource::Probe,
            tool_session_id: String::new(),
        });
    };
    add(
        LimitWindow::Session,
        limits.get("five_hour").map(|v| (v, "utilization")),
    );
    add(
        LimitWindow::Weekly,
        limits.get("seven_day").map(|v| (v, "utilization")),
    );
    for (key, name) in [("seven_day_opus", "opus"), ("seven_day_sonnet", "sonnet")] {
        add(
            LimitWindow::Model(name.into()),
            limits.get(key).map(|v| (v, "utilization")),
        );
    }
    for scoped in limits
        .get("model_scoped")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(name) = model_name(scoped.get("display_name")) {
            add(LimitWindow::Model(name), Some((scoped, "utilization")));
        }
    }
    for entry in limits
        .get("limits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let window = match entry.get("kind").and_then(Value::as_str) {
            Some("session") => Some(LimitWindow::Session),
            Some("weekly_all") => Some(LimitWindow::Weekly),
            Some("weekly_scoped") => {
                model_name(entry.pointer("/scope/model/display_name")).map(LimitWindow::Model)
            }
            _ => None,
        };
        if let Some(window) = window {
            add(window, Some((entry, "percent")));
        }
    }
    Ok(probe)
}

fn probe_plan(body: &Value) -> Option<String> {
    body.get("subscription_type")
        .and_then(Value::as_str)
        .map(|s| clean_name(s, 40))
        .filter(|s| s != "other")
}

fn model_name(v: Option<&Value>) -> Option<String> {
    let raw = v?.as_str()?.trim().to_lowercase().replace(' ', "-");
    let name = clean_name(&raw, 40);
    (name != "other").then_some(name)
}

fn percent(v: &Value) -> Option<f64> {
    let p = v.as_f64().filter(|p| p.is_finite() && *p >= 0.0)?;
    Some(p.min(100.0))
}

fn bad_reply() -> UsageError {
    UsageError::Invalid("unexpected reply from the tool".into())
}

/// Reads the account's rate limits from `codex app-server` (JSON-RPC `account/rateLimits/read`).
/// Windows are classified by their length in minutes; none is assumed to exist.
pub fn codex_usage(codex_bin: &Path, timeout: Duration) -> Result<CodexProbe> {
    let lines = vec![
        json!({"jsonrpc":"2.0","id":1,"method":"initialize",
               "params":{"clientInfo":{"name":"tmgr","version":env!("CARGO_PKG_VERSION")}}})
        .to_string(),
        json!({"jsonrpc":"2.0","method":"initialized"}).to_string(),
        json!({"jsonrpc":"2.0","id":2,"method":"account/rateLimits/read"}).to_string(),
    ];
    let reply = converse(codex_bin, &["app-server"], lines, timeout, |v| {
        (v.get("id").and_then(Value::as_i64) == Some(2)).then(|| v.clone())
    })?;
    if reply.get("error").is_some() {
        return Err(UsageError::Invalid("codex usage request failed".into()));
    }
    let result = reply.get("result").ok_or_else(bad_reply)?;
    parse_codex_rate_limits(result, Utc::now())
}

pub fn parse_codex_rate_limits(result: &Value, observed_at: DateTime<Utc>) -> Result<CodexProbe> {
    let limits = result.get("rateLimits").ok_or_else(bad_reply)?;
    let plan = limits
        .get("planType")
        .and_then(Value::as_str)
        .map(|s| clean_name(s, 40))
        .filter(|s| s != "other");
    let mut windows: Vec<LimitSnapshot> = Vec::new();
    for key in ["primary", "secondary"] {
        let Some(w) = limits.get(key).filter(|v| v.is_object()) else {
            continue;
        };
        let (Some(used_pct), Some(minutes)) = (
            w.get("usedPercent").and_then(percent),
            w.get("windowDurationMins")
                .and_then(Value::as_u64)
                .filter(|m| *m > 0),
        ) else {
            continue;
        };
        let window = classify(minutes);
        if windows.iter().any(|s| s.window == window) {
            continue;
        }
        windows.push(LimitSnapshot {
            tool: Tool::Codex,
            account: Tool::Codex.as_str().to_string(),
            window,
            used_pct,
            resets_at: w
                .get("resetsAt")
                .and_then(Value::as_i64)
                .and_then(|s| Utc.timestamp_opt(s, 0).single()),
            plan: plan.clone(),
            observed_at,
            source: LimitSource::Probe,
            tool_session_id: String::new(),
        });
    }
    Ok(CodexProbe {
        plan,
        ordinary_usage_allowed: result.get("ordinaryUsageAllowed").and_then(Value::as_bool),
        windows,
    })
}

fn converse(
    bin: &Path,
    args: &[&str],
    input: Vec<String>,
    timeout: Duration,
    want: impl Fn(&Value) -> Option<Value> + Send + 'static,
) -> Result<Value> {
    let mut child = Command::new(bin)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let stdout = child.stdout.take().ok_or_else(bad_reply)?;
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut lines = BufReader::new(stdout.take(MAX_LINE_BYTES * 4)).lines();
        while let Some(Ok(line)) = lines.next() {
            if let Some(v) = serde_json::from_str::<Value>(&line)
                .ok()
                .and_then(|v| want(&v))
            {
                let _ = tx.send(v);
                return;
            }
        }
    });
    let mut stdin = child.stdin.take();
    if let Some(w) = stdin.as_mut() {
        for line in &input {
            if writeln!(w, "{line}").and_then(|_| w.flush()).is_err() {
                break;
            }
        }
    }
    let started = Instant::now();
    let outcome = rx.recv_timeout(timeout);
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    let _ = reader.join();
    match outcome {
        Ok(v) => Ok(v),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(UsageError::Invalid(format!(
            "usage probe timed out after {}s",
            started.elapsed().as_secs().max(timeout.as_secs())
        ))),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(UsageError::Invalid(
            "usage probe ended without an answer".into(),
        )),
    }
}
