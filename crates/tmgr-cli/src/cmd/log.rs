use crate::cli::{Cli, Command, LogArgs};
use crate::cmd::shared::{render_run, resolve_task};
use crate::ctx::Ctx;
use crate::out::{print_error, Output};
use chrono::{SecondsFormat, Utc};
use serde_json::json;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tmgr_core::api::Client;
use tmgr_core::api_ext::RunQuery;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::models::Workspace;
use tmgr_core::refs::parse_task_ref;
use tmgr_core::runs::{parse_since, RunView};
use tmgr_core::text::sanitize_line;

const MAX_LIMIT: u32 = 200;
const DEFAULT_INTERVAL_MS: u64 = 5_000;
const DEFAULT_OFFLINE_MS: u64 = 60_000;
const MAX_BACKOFF_MS: u64 = 30_000;

fn env_ms(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

struct Filters {
    since: Option<String>,
    agent: Option<String>,
}

fn validate(args: &LogArgs) -> Result<Filters> {
    if !(1..=MAX_LIMIT).contains(&args.limit) {
        return Err(Error::usage(format!("--limit must be 1 to {MAX_LIMIT}")));
    }
    if let Some(r) = args.task.as_deref() {
        parse_task_ref(r)?;
    }
    let since = match args.since.as_deref() {
        Some(raw) => {
            let d = parse_since(raw).ok_or_else(|| {
                Error::usage("invalid --since")
                    .hint("examples: --since 30m, --since 2h, --since 1d")
            })?;
            Some((Utc::now() - d).to_rfc3339_opts(SecondsFormat::Secs, true))
        }
        None => None,
    };
    let agent = match args.agent.as_deref() {
        Some(a) if (1..=64).contains(&a.len()) && a.chars().all(|c| c.is_ascii_graphic()) => {
            Some(a.to_string())
        }
        Some(_) => return Err(Error::usage("invalid --agent")),
        None => None,
    };
    Ok(Filters { since, agent })
}

fn build_query(ctx: &Ctx, client: &Client, args: &LogArgs, f: Filters) -> Result<RunQuery> {
    let task_id = match args.task.as_deref() {
        Some(r) => Some(resolve_task(ctx, client, r)?.id),
        None => None,
    };
    Ok(RunQuery {
        since: f.since,
        until: None,
        task_id,
        agent: f.agent,
        limit: Some(args.limit),
        cursor: args.cursor.clone(),
    })
}

pub fn run(ctx: &Ctx, args: &LogArgs) -> Result<Output> {
    if args.follow {
        return Err(Error::internal("handled before the session starts"));
    }
    let filters = validate(args)?;
    let client = ctx.client()?;
    let ws = ctx.workspace(client)?;
    let query = build_query(ctx, client, args, filters)?;
    let page = client.workspace_agent_runs(ws.id, &query)?;
    let s = &ctx.style;
    let (views, next, disabled) = match page {
        Some(p) => (
            p.runs.iter().map(RunView::from_api).collect::<Vec<_>>(),
            p.next_cursor,
            false,
        ),
        None => {
            ctx.warn(
                "agent_work_disabled",
                "Agent work is turned off in this workspace",
            );
            (Vec::new(), None, true)
        }
    };
    let human = if disabled {
        s.muted("Agent work is turned off in this workspace.")
    } else if views.is_empty() {
        s.muted("No agent work found.")
    } else {
        let mut text = views
            .iter()
            .map(|v| render_run(s, v))
            .collect::<Vec<_>>()
            .join("\n\n");
        if let Some(c) = &next {
            text.push_str(&format!(
                "\n\n{}",
                s.muted(&format!("more: tmgr log --cursor {c}"))
            ));
        }
        text
    };
    Ok(Output::new(
        json!({"workspace": sanitize_line(&ws.code), "runs": views, "next_cursor": next}),
        human,
    ))
}

pub fn follow_entry(ctx: &Ctx, cli: &Cli) -> Option<i32> {
    let Some(Command::Log(args)) = &cli.command else {
        return None;
    };
    if !args.follow {
        return None;
    }
    Some(match follow(ctx, args) {
        Ok(code) => code,
        Err(e) => print_error(ctx.json, ctx.verbose, ctx.err_style, &e),
    })
}

fn emit_run(ctx: &Ctx, view: &RunView) {
    if ctx.json {
        println!(
            "{}",
            json!({"schema": tmgr_core::SCHEMA, "event": "run.updated", "data": view})
        );
    } else {
        println!("{}\n", render_run(&ctx.style, view));
    }
}

fn follow(ctx: &Ctx, args: &LogArgs) -> Result<i32> {
    let filters = validate(args)?;
    let client = ctx.client()?;
    let ws = ctx.workspace(client)?;
    let query = build_query(ctx, client, args, filters)?;
    let _ = ctrlc::set_handler(|| std::process::exit(130));
    let interval = Duration::from_millis(env_ms("TMGR_FOLLOW_INTERVAL_MS", DEFAULT_INTERVAL_MS));
    let offline_limit = Duration::from_millis(env_ms("TMGR_FOLLOW_OFFLINE_MS", DEFAULT_OFFLINE_MS));
    if !ctx.json {
        println!(
            "{}\n",
            ctx.style
                .muted("agent work \u{b7} live \u{b7} Ctrl-C to exit")
        );
    }
    let mut seen: HashMap<u64, String> = HashMap::new();
    let mut offline_since: Option<Instant> = None;
    let mut failures = 0u32;
    loop {
        match poll(client, &ws, &query, &mut seen, ctx) {
            Ok(true) => {
                offline_since = None;
                failures = 0;
            }
            Ok(false) => {
                eprintln!(
                    "{}",
                    ctx.err_style
                        .muted("Agent work is turned off in this workspace.")
                );
                return Ok(0);
            }
            Err(e) if e.kind == Kind::Offline => {
                let since = *offline_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= offline_limit {
                    return Err(e);
                }
                if failures == 0 {
                    eprintln!("{}", ctx.err_style.muted("offline, retrying..."));
                }
                failures += 1;
            }
            Err(e) => return Err(e),
        }
        let wait = if failures == 0 {
            interval
        } else {
            let factor = 1u64 << failures.min(5);
            (interval * factor as u32).min(Duration::from_millis(MAX_BACKOFF_MS))
        };
        std::thread::sleep(wait);
    }
}

fn poll(
    client: &Client,
    ws: &Workspace,
    query: &RunQuery,
    seen: &mut HashMap<u64, String>,
    ctx: &Ctx,
) -> Result<bool> {
    let Some(page) = client.workspace_agent_runs(ws.id, query)? else {
        return Ok(false);
    };
    for run in page.runs.iter().rev() {
        let view = RunView::from_api(run);
        let fp = view.fingerprint();
        if seen.get(&view.id) != Some(&fp) {
            emit_run(ctx, &view);
            seen.insert(view.id, fp);
        }
    }
    Ok(true)
}
