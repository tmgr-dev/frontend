use crate::ctx::Ctx;
use crate::out::Style;
use chrono::{Local, NaiveDate};
use tmgr_core::api::Client;
use tmgr_core::error::{Error, Result};
use tmgr_core::models::ApiTask;
use tmgr_core::refs::{parse_task_ref, TaskRef};
use tmgr_core::runs::RunView;
use tmgr_core::text::format_duration;

pub fn resolve_task(ctx: &Ctx, client: &Client, reference: &str) -> Result<ApiTask> {
    let not_found = |e: Error| {
        if e.exit_code() == 3 {
            Error::not_found(format!("task {} not found", printable(reference)))
                .hint("check --ws, or tmgr task list")
        } else {
            e
        }
    };
    match parse_task_ref(reference)? {
        TaskRef::Key(key) => {
            let ws = ctx.workspace(client)?;
            client.task_by_key(&key, ws.id).map_err(not_found)
        }
        TaskRef::Id(id) => client.task_by_id(id).map_err(not_found),
    }
}

fn printable(s: &str) -> String {
    s.chars()
        .take(40)
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

pub fn parse_date(input: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(input, "%Y-%m-%d")
        .ok()
        .filter(|_| input.len() == 10)
        .ok_or_else(|| Error::usage("invalid date").hint("use YYYY-MM-DD"))
}

pub fn today_local() -> NaiveDate {
    Local::now().date_naive()
}

fn local_time(raw: &str) -> String {
    match tmgr_core::runs::parse_ts(raw) {
        Some(t) => {
            let local = t.with_timezone(&Local);
            if local.date_naive() == today_local() {
                local.format("%H:%M").to_string()
            } else {
                local.format("%m-%d %H:%M").to_string()
            }
        }
        None => String::new(),
    }
}

pub fn render_run(s: &Style, r: &RunView) -> String {
    let when = r.started_at.as_deref().map(local_time).unwrap_or_default();
    let glyph = tmgr_core::runs::status_glyph(&r.status);
    let glyph = match r.status.as_str() {
        "succeeded" => s.good(glyph),
        "failed" => s.accent(glyph),
        _ => glyph.to_string(),
    };
    let mut head = format!("{when} {glyph} {}", s.bold(&r.agent));
    if let Some(m) = &r.model {
        head.push(' ');
        head.push_str(&s.muted(m));
    }
    if let Some(t) = &r.task {
        head.push_str(&format!("  {} {}", s.accent(&t.reference), t.title));
    }
    let mut detail = Vec::new();
    if let Some(b) = &r.branch {
        detail.push(format!("branch {b}"));
    }
    detail.push(format!("commits {}", r.commits.len()));
    if let Some(t) = &r.tests {
        match (t.passed, t.failed) {
            (Some(p), Some(f)) => detail.push(format!("tests {p} passed {f} failed")),
            (Some(p), None) => detail.push(format!("tests {p} passed")),
            (None, Some(f)) => detail.push(format!("tests {f} failed")),
            _ => {}
        }
    }
    let state = match r.status.as_str() {
        "succeeded" => "done",
        other => other,
    };
    detail.push(format!("{state} {}", format_duration(r.duration_s)));
    let mut lines = vec![head, format!("      {}", s.muted(&detail.join("   ")))];
    if let Some(c) = r.commits.last() {
        lines.push(format!(
            "      {}",
            s.muted(&format!("{} {}", c.sha, c.message).trim().to_string())
        ));
    }
    if let Some(pr) = &r.pr_url {
        lines.push(format!("      PR {pr}"));
    }
    if let Some(sum) = &r.summary {
        lines.push(format!("      {}", s.muted(sum)));
    }
    lines.join("\n")
}
