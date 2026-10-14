use crate::cli::TodayArgs;
use crate::cmd::routine;
use crate::cmd::shared::{parse_date, today_local};
use crate::ctx::Ctx;
use crate::out::{table, Output};
use chrono::{Duration, Local, NaiveDate, SecondsFormat, Utc};
use serde_json::json;
use std::collections::HashMap;
use tmgr_core::api_ext::{DailyTime, RunQuery, TimeSummary};
use tmgr_core::error::Result;
use tmgr_core::refs::parse_key;
use tmgr_core::runs::{clipped_seconds, day_bounds, task_ref_of, ApiRun};
use tmgr_core::text::{format_duration, sanitize_line};
use tmgr_core::view::display_title;

const MAX_RUN_PAGES: u32 = 10;

enum Summary {
    Daily(DailyTime),
    Legacy(TimeSummary),
}

struct Row {
    id: Option<u64>,
    reference: String,
    title: String,
    human_s: i64,
    agent_s: i64,
    status: Option<String>,
    project: Option<String>,
    url: Option<String>,
    users: Vec<(Option<u64>, String, i64)>,
}

pub(crate) fn detect_tz(ctx: &Ctx) -> String {
    let from_env = std::env::var("TZ")
        .ok()
        .map(|v| v.trim_start_matches(':').to_string());
    let valid = |z: &str| {
        !z.is_empty()
            && z.len() <= 64
            && z.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '+' | '-'))
    };
    if let Some(z) = from_env.filter(|z| valid(z)) {
        return z;
    }
    match iana_time_zone::get_timezone() {
        Ok(z) if valid(&z) => z,
        _ => {
            ctx.warn(
                "timezone_unknown",
                "could not detect the time zone; day boundaries use UTC",
            );
            "UTC".to_string()
        }
    }
}

fn dur(s: i64) -> String {
    if s > 0 {
        format_duration(s)
    } else {
        "-".to_string()
    }
}

fn pick_date(args: &TodayArgs) -> Result<NaiveDate> {
    if args.yesterday {
        return Ok(today_local() - Duration::days(1));
    }
    args.date
        .as_deref()
        .map(parse_date)
        .unwrap_or_else(|| Ok(today_local()))
}

pub(crate) const LOCAL_TIME: &str = "local_time_unavailable";
/// Local workspaces keep only each task's total time, not when it was tracked.
pub(crate) const LOCAL_TIME_WHY: &str = "tracked time per day is not recorded in local workspaces";

pub fn run(ctx: &Ctx, args: &TodayArgs) -> Result<Output> {
    let date = pick_date(args)?;
    let ds = date.format("%Y-%m-%d").to_string();
    let client = ctx.client()?;
    let ws = ctx.workspace(client)?;
    let (day_start, day_end) = day_bounds(&Local, date);
    let now = Utc::now();
    let tz = detect_tz(ctx);
    let daily = if client.local().is_some() {
        ctx.warn(LOCAL_TIME, LOCAL_TIME_WHY);
        None
    } else {
        ctx.soft(
            client.time_daily(ws.id, &ds, &tz, true),
            "time_summary_unavailable",
            "tracked time",
        )?
    };
    let summary = match daily {
        Some(Some(d)) => Some(Summary::Daily(d)),
        Some(None) => {
            ctx.warn(
                "daily_time_unavailable",
                "this server has no per-day time endpoint; using the time summary",
            );
            ctx.soft(
                client.time_summary(ws.id, &ds, &ds),
                "time_summary_unavailable",
                "tracked time",
            )?
            .map(Summary::Legacy)
        }
        None => None,
    };
    let mut runs: Vec<ApiRun> = Vec::new();
    let mut cursor = None;
    for _ in 0..MAX_RUN_PAGES {
        let q = RunQuery {
            since: Some(
                (day_start - Duration::hours(24)).to_rfc3339_opts(SecondsFormat::Secs, true),
            ),
            until: Some(day_end.to_rfc3339_opts(SecondsFormat::Secs, true)),
            limit: Some(200),
            cursor: cursor.take(),
            ..RunQuery::default()
        };
        match client.workspace_agent_runs(ws.id, &q)? {
            Some(page) => {
                runs.extend(page.runs);
                match page.next_cursor {
                    Some(c) => cursor = Some(c),
                    None => break,
                }
            }
            None => {
                ctx.warn(
                    "agent_work_disabled",
                    "Agent work is turned off in this workspace",
                );
                break;
            }
        }
    }
    if args.mine {
        let me = client.current_user_id()?;
        runs.retain(|r| r.user_id.is_some() && r.user_id == me);
    }
    let routines = ctx
        .soft(
            routine::fetch(ctx, date),
            "routines_unavailable",
            "routines",
        )?
        .unwrap_or_default();

    let mut rows: HashMap<u64, Row> = HashMap::new();
    let mut human_total = 0;
    match &summary {
        Some(Summary::Daily(d)) => {
            human_total = d.total_s;
            for item in &d.items {
                let reference = if parse_key(&item.key).is_some() {
                    item.key.clone()
                } else {
                    format!("#{}", item.task_id)
                };
                rows.insert(
                    item.task_id,
                    Row {
                        id: Some(item.task_id),
                        reference,
                        title: display_title(&item.title),
                        human_s: item.seconds.max(0),
                        agent_s: 0,
                        status: item.status.as_deref().map(sanitize_line),
                        project: item.category.as_deref().map(sanitize_line),
                        url: item.url.as_deref().map(sanitize_line),
                        users: item
                            .users
                            .iter()
                            .map(|u| (u.id, sanitize_line(&u.name), u.seconds.max(0)))
                            .collect(),
                    },
                );
            }
        }
        Some(Summary::Legacy(s)) => {
            human_total = s.total_s;
            for item in &s.items {
                let reference = if parse_key(&item.key).is_some() {
                    item.key.clone()
                } else {
                    format!("#{}", item.task_id)
                };
                rows.insert(
                    item.task_id,
                    Row {
                        id: Some(item.task_id),
                        reference,
                        title: display_title(&item.title),
                        human_s: item.seconds.max(0),
                        agent_s: 0,
                        status: None,
                        project: None,
                        url: None,
                        users: Vec::new(),
                    },
                );
            }
        }
        None => {}
    }
    for r in &runs {
        let secs = clipped_seconds(r, day_start, day_end, now);
        let Some(task) = r.task.as_ref() else {
            continue;
        };
        let Some(id) = task.id.or(r.task_id) else {
            continue;
        };
        if secs == 0 {
            continue;
        }
        let row = rows.entry(id).or_insert_with(|| Row {
            id: Some(id),
            reference: task_ref_of(task),
            title: display_title(task.title.as_deref().unwrap_or("")),
            human_s: 0,
            agent_s: 0,
            status: None,
            project: None,
            url: None,
            users: Vec::new(),
        });
        row.agent_s += secs;
    }
    let mut rows: Vec<Row> = rows.into_values().collect();
    rows.sort_by(|a, b| {
        (b.human_s + b.agent_s, &a.reference).cmp(&(a.human_s + a.agent_s, &b.reference))
    });
    let agent_total: i64 = rows.iter().map(|r| r.agent_s).sum();
    let human_total = human_total.max(0);

    let s = &ctx.style;
    let title = if date == today_local() {
        format!("Today {}", date.format("%a %d %b"))
    } else {
        date.format("%a %d %b %Y").to_string()
    };
    let mut lines = vec![format!(
        "{}   {} {}   {} {}   {} {}",
        s.bold(&title),
        s.muted("tracked"),
        s.bold(&dur(human_total + agent_total)),
        s.muted("you"),
        dur(human_total),
        s.muted("agents"),
        dur(agent_total)
    )];
    lines.push(String::new());
    if rows.is_empty() {
        lines.push(s.muted("Nothing tracked yet. Start a task: tmgr start TM-512"));
    } else {
        let mut t = vec![["TASK", "PROJECT", "YOU", "AGENTS", "STATUS"]
            .map(String::from)
            .to_vec()];
        for r in &rows {
            t.push(vec![
                format!("{} {}", r.reference, r.title),
                r.project.clone().unwrap_or_else(|| "-".into()),
                dur(r.human_s),
                dur(r.agent_s),
                r.status.clone().unwrap_or_else(|| "-".into()),
            ]);
        }
        let text = table(&t);
        match text.split_once('\n') {
            Some((head, rest)) => {
                lines.push(s.muted(head));
                lines.push(rest.to_string());
            }
            None => lines.push(text),
        }
        let sum = human_total + agent_total;
        if sum > 0 && agent_total > 0 {
            let width = 24usize;
            let you_cells = ((human_total as f64 / sum as f64) * width as f64).round() as usize;
            let pct = (human_total as f64 / sum as f64 * 100.0).round() as i64;
            lines.push(String::new());
            lines.push(format!(
                "{} you {}{} agents  {pct}% / {}%",
                s.muted("time split"),
                s.accent(&"\u{2588}".repeat(you_cells)),
                "\u{2591}".repeat(width - you_cells.min(width)),
                100 - pct
            ));
        }
    }
    if !routines.is_empty() {
        lines.push(String::new());
        lines.push(s.bold("Routines"));
        lines.push(routine::render_list(ctx, &routines));
    }
    let tasks: Vec<_> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.id, "ref": r.reference, "title": r.title,
                "human_s": r.human_s, "agent_s": r.agent_s,
                "status": r.status, "project": r.project, "url": r.url,
                "users": r.users.iter().map(|(id, name, secs)| json!({"id": id, "name": name, "human_s": secs})).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(Output::new(
        json!({
            "date": ds,
            "workspace": sanitize_line(&ws.code),
            "tz": tz,
            "tasks": tasks,
            "routines": routines,
            "totals": {"human_s": human_total, "agent_s": agent_total},
        }),
        lines.join("\n"),
    ))
}
