use crate::ctx::Ctx;
use crate::out::Output;
use serde_json::json;
use tmgr_core::api::Client;
use tmgr_core::error::Result;
use tmgr_core::models::ApiTask;
use tmgr_core::text::{format_duration, sanitize_line};
use tmgr_core::view::{display_title, task_ref, CategoryView, StatusView};

pub fn run(ctx: &Ctx) -> Result<Output> {
    let client = ctx.client()?;
    let ws = ctx.workspace(client)?;
    let today = chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    let tz = crate::cmd::today::detect_tz(ctx);
    let total = match ctx.soft(
        client.time_daily(ws.id, &today, &tz, true),
        "time_summary_unavailable",
        "today's time",
    )? {
        Some(Some(d)) => Some(d.total_s),
        Some(None) => {
            if client.local().is_none() {
                ctx.warn(
                    "daily_time_unavailable",
                    "this server has no per-day time endpoint; using the time summary",
                );
            }
            ctx.soft(
                client.time_summary_total(ws.id, &today, &today),
                "time_summary_unavailable",
                "today's time",
            )?
        }
        None => None,
    };
    let running = ctx.soft(
        client.running_tasks(),
        "active_unavailable",
        "the running task",
    )?;
    let mut running = running.unwrap_or_default();
    running.sort_by_key(|t| std::cmp::Reverse(t.start_time.unwrap_or(0)));
    if running.len() > 1 {
        ctx.warn(
            "multiple_timers",
            format!(
                "{} timers are running; showing the most recently started",
                running.len()
            ),
        );
    }
    let active = match running.first() {
        Some(task) => Some(active_view(ctx, client, task)?),
        None => None,
    };
    let s = &ctx.style;
    let mut lines = vec![format!(
        "{}  {} {}",
        s.bold("TMGR"),
        s.muted("workspace"),
        sanitize_line(&ws.name)
    )];
    lines.push(String::new());
    match &active {
        Some((_, human)) => lines.push(format!("{}  {human}", s.muted("ACTIVE"))),
        None => lines.push(format!(
            "{}  {}",
            s.muted("ACTIVE"),
            s.muted("no active task")
        )),
    }
    if let Some(t) = total {
        lines.push(format!(
            "{}   {} tracked",
            s.muted("TODAY"),
            s.bold(&format_duration(t))
        ));
    }
    lines.push(String::new());
    lines.push(s.muted("hint: tmgr task list \u{b7} tmgr add \"title\" \u{b7} tmgr ws list"));
    let data = json!({
        "workspace": {"id": ws.id, "code": sanitize_line(&ws.code), "name": sanitize_line(&ws.name)},
        "active": active.as_ref().map(|(v, _)| v),
        "git": null,
        "today": total.map(|t| json!({"total_s": t, "human_s": t, "agent_s": null})),
        "agents": null,
    });
    Ok(Output::new(data, lines.join("\n")))
}

fn active_view(ctx: &Ctx, client: &Client, task: &ApiTask) -> Result<(serde_json::Value, String)> {
    let ws_code = task.workspace_id.and_then(|id| {
        ctx.workspaces(client)
            .ok()?
            .iter()
            .find(|w| w.id == id)
            .map(|w| sanitize_line(&w.code))
    });
    let url = match &ws_code {
        Some(code) => ctx.task_link(code, task.id).ok().flatten(),
        None => None,
    };
    let statuses = match task.workspace_id {
        Some(w) => ctx
            .soft(client.statuses(w), "status_unresolved", "status names")?
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let status = StatusView::resolve(task, &statuses);
    let category = task.category.as_ref().map(|c| CategoryView {
        id: Some(c.id),
        code: c.code.as_ref().map(|s| sanitize_line(s)),
        title: sanitize_line(&c.title),
    });
    let started = task.start_time.unwrap_or(0);
    let seconds = (chrono::Utc::now().timestamp() - started).max(0);
    let started_at = chrono::DateTime::from_timestamp(started, 0)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    let key = task_ref(task);
    let title = display_title(&task.title);
    let s = &ctx.style;
    let human = format!(
        "{}  {}  {} {}",
        s.accent(&key),
        s.bold(&title),
        s.accent("\u{23f1}"),
        format_duration(seconds)
    );
    let view = serde_json::json!({
        "task": {"id": task.id, "ref": key, "title": title, "url": url, "status": status, "category": category},
        "timer": {"running": true, "started_at": started_at, "seconds": seconds},
    });
    Ok((view, human))
}
