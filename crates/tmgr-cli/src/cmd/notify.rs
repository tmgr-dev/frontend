use crate::cli_agent::{AlarmArgs, AlarmCommand, NotifyArgs};
use crate::cmd::notifyenv::NotifyEnv;
use crate::ctx::Ctx;
use crate::out::Output;
use serde_json::{json, Value};
use std::time::Duration;
use tmgr_core::active::state_dir;
use tmgr_core::config::{ensure_private_dir, write_atomic};
use tmgr_core::error::{Error, Result};
use tmgr_core::notify::{
    build_push, format_alarm, parse_duration_seconds, valid_alarm_id, Alarm, AlarmRequest,
    WaitOptions,
};
use tmgr_core::text::sanitize_line;

const PUSH_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_TITLE: &str = "TMGR";

pub fn notify(ctx: &Ctx, args: &NotifyArgs) -> Result<Output> {
    if args.text.trim().is_empty() {
        return Err(Error::usage("the notification text is empty"));
    }
    let payload = build_push(
        args.title.as_deref().unwrap_or(DEFAULT_TITLE),
        Some(&args.text),
        args.priority.map(|p| p.as_str()),
        args.link.as_deref(),
        Some("tmgr-cli"),
    );
    if payload.title.is_empty() {
        return Err(Error::usage("--title is empty"));
    }
    let mut warnings = Vec::new();
    let result =
        NotifyEnv::from_ctx(ctx).with_http(&mut warnings, |http| http.push(&payload, PUSH_TIMEOUT));
    for w in warnings {
        ctx.warn(&w.code, w.message);
    }
    let (outcome, _) = result?;
    let channels = if outcome.channels.is_empty() {
        "none".to_string()
    } else {
        outcome.channels.join(", ")
    };
    Ok(Output::new(
        json!({"status": outcome.status, "channels": outcome.channels}),
        format!(
            "{} {} (channels: {channels})",
            ctx.style.good("\u{2713}"),
            outcome.status
        ),
    ))
}

fn last_alarm_path() -> Option<std::path::PathBuf> {
    state_dir().map(|d| d.join("last_alarm"))
}

fn remember_alarm(id: &str) {
    if let Some(path) = last_alarm_path() {
        if let Some(dir) = path.parent() {
            if ensure_private_dir(dir).is_ok() {
                let _ = write_atomic(&path, id.as_bytes(), 0o600);
            }
        }
    }
}

fn last_alarm() -> Option<String> {
    let text = std::fs::read_to_string(last_alarm_path()?).ok()?;
    let id = text.trim().to_string();
    valid_alarm_id(&id).then_some(id)
}

fn alarm_data(alarm: &Alarm, timed_out: bool) -> Value {
    let mut v = serde_json::to_value(alarm).expect("alarm serializes");
    v["timed_out"] = json!(timed_out);
    v
}

fn seconds(flag: &str, raw: Option<&str>) -> Result<Option<u64>> {
    match raw {
        None => Ok(None),
        Some(r) => parse_duration_seconds(r)
            .map(Some)
            .ok_or_else(|| Error::usage(format!("{flag} needs a duration like 90, 90s, 5m or 1h"))),
    }
}

pub fn alarm(ctx: &Ctx, args: &AlarmArgs) -> Result<Output> {
    if let Some(AlarmCommand::Status { id, wait }) = &args.command {
        return status(ctx, id.as_deref(), u64::from(*wait));
    }
    let text = args
        .text
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            Error::usage("a message is required").hint("tmgr alarm \"what happened\"")
        })?;
    let title = sanitize_line(args.title.trim());
    if title.is_empty() {
        return Err(Error::usage("--title needs a value"));
    }
    let max_wait = seconds("--max-wait", Some(&args.max_wait))?
        .unwrap_or(600)
        .min(3600);
    let request = AlarmRequest {
        title: title.chars().take(120).collect(),
        message: text.chars().take(500).collect(),
        ack_timeout_seconds: seconds("--ack-timeout", args.ack_timeout.as_deref())?,
        delivery_timeout_seconds: seconds("--delivery-timeout", args.delivery_timeout.as_deref())?,
        call: args.no_call.then_some(false),
        call_attempts: args.call_attempts,
    };
    let mut warnings = Vec::new();
    let result =
        NotifyEnv::from_ctx(ctx).with_http(&mut warnings, |http| http.create_alarm(&request));
    for w in warnings {
        ctx.warn(&w.code, w.message);
    }
    let (created, session) = result?;
    remember_alarm(&created.id);
    let chatty = !ctx.json && !ctx.quiet;
    if chatty {
        eprintln!("alarm {}: {}", created.id, created.status);
    }
    if args.no_wait {
        let human = format_alarm(&created, false);
        return Ok(Output::new(alarm_data(&created, false), human));
    }
    let mut last = created.status.clone();
    let opts = WaitOptions {
        max_wait: Duration::from_secs(max_wait),
        ..WaitOptions::default()
    };
    let (alarm, timed_out) = session
        .http
        .wait_alarm(&created.id, &opts, &mut |a| {
            if chatty && a.status != last {
                eprintln!("status: {}", a.status);
            }
            last = a.status.clone();
        })
        .map_err(|f| session.error(f))
        .map_err(|e| {
            Error::new(
                e.kind,
                format!(
                    "{} (alarm {} was created; re-check with `tmgr alarm status {}`)",
                    e.message, created.id, created.id
                ),
            )
        })?;
    let exit = if alarm.status == "acknowledged" { 0 } else { 1 };
    Ok(Output::new(
        alarm_data(&alarm, timed_out),
        format_alarm(&alarm, timed_out),
    )
    .exit(exit))
}

fn status(ctx: &Ctx, id: Option<&str>, wait: u64) -> Result<Output> {
    let id = match id {
        Some(i) => i.to_string(),
        None => last_alarm().ok_or_else(|| {
            Error::usage("no alarm id given and none was created from this machine")
                .hint("tmgr alarm status <id>")
        })?,
    };
    if !valid_alarm_id(&id) {
        return Err(Error::usage("an alarm id is a number"));
    }
    let mut warnings = Vec::new();
    let result =
        NotifyEnv::from_ctx(ctx).with_http(&mut warnings, |http| http.get_alarm(&id, wait));
    for w in warnings {
        ctx.warn(&w.code, w.message);
    }
    let (alarm, _) = result?;
    Ok(Output::new(
        alarm_data(&alarm, false),
        format_alarm(&alarm, false),
    ))
}
