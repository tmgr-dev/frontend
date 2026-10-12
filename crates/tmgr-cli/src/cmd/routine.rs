use crate::cli::RoutineCommand;
use crate::cmd::shared::{parse_date, today_local};
use crate::ctx::Ctx;
use crate::out::Output;
use chrono::{NaiveDate, Utc};
use serde_json::json;
use tmgr_core::error::{Error, Result};
use tmgr_core::routines::{views, RoutineView};

pub fn run(ctx: &Ctx, cmd: &RoutineCommand) -> Result<Output> {
    match cmd {
        RoutineCommand::List { date } => list(ctx, date.as_deref()),
        RoutineCommand::Done { id, date } => done(ctx, id, date.as_deref()),
    }
}

fn day(date: Option<&str>) -> Result<NaiveDate> {
    date.map(parse_date).unwrap_or_else(|| Ok(today_local()))
}

fn mark(ctx: &Ctx, r: &RoutineView) -> String {
    let s = &ctx.style;
    let when = r.time.as_deref().unwrap_or("");
    let glyph = if r.done {
        s.good("\u{2713}")
    } else {
        "\u{25cb}".to_string()
    };
    format!("  {glyph} {} {}", r.title, s.muted(when))
        .trim_end()
        .to_string()
}

pub fn render_list(ctx: &Ctx, routines: &[RoutineView]) -> String {
    routines
        .iter()
        .map(|r| mark(ctx, r))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn fetch(ctx: &Ctx, date: NaiveDate) -> Result<Vec<RoutineView>> {
    let client = ctx.client()?;
    let d = date.format("%Y-%m-%d").to_string();
    Ok(views(&client.routines_expand(&d, &d)?))
}

fn list(ctx: &Ctx, date: Option<&str>) -> Result<Output> {
    let date = day(date)?;
    let routines = fetch(ctx, date)?;
    let human = if routines.is_empty() {
        ctx.style.muted("No routines due.")
    } else {
        format!(
            "{}\n{}",
            ctx.style.bold("Routines"),
            render_list(ctx, &routines)
        )
    };
    Ok(Output::new(
        json!({"date": date.format("%Y-%m-%d").to_string(), "routines": routines}),
        human,
    ))
}

fn done(ctx: &Ctx, id: &str, date: Option<&str>) -> Result<Output> {
    let id = (1..=12)
        .contains(&id.len())
        .then(|| id.parse::<u64>().ok())
        .flatten()
        .filter(|v| *v > 0)
        .ok_or_else(|| {
            Error::usage("invalid routine id").hint("ids are shown by `tmgr routine list`")
        })?;
    let date = day(date)?;
    let client = ctx.client()?;
    let routines = fetch(ctx, date)?;
    let mut matching = routines
        .into_iter()
        .filter(|r| r.id == id)
        .collect::<Vec<_>>();
    matching.sort_by_key(|r| r.done);
    let Some(mut routine) = matching.into_iter().next() else {
        return Err(Error::not_found(format!(
            "routine {id} is not due on {}",
            date.format("%Y-%m-%d")
        ))
        .hint("tmgr routine list"));
    };
    let already = routine.done;
    if !already {
        match routine.instance_id {
            Some(instance) => client.complete_routine_instance(id, instance)?,
            None if date == today_local() => {
                if today_local() != Utc::now().date_naive() {
                    ctx.warn(
                        "routine_date_utc",
                        "the server completes routines for its own current date (UTC)",
                    );
                }
                client.complete_routine_today(id)?;
            }
            None => {
                return Err(Error::usage(
                    "this routine has no instance on that day yet; only today's routine can be completed",
                )
                .hint("tmgr routine done <id>"))
            }
        }
        routine.done = true;
        routine.status = "completed".to_string();
    }
    let human = if already {
        format!("{} (already done)", mark(ctx, &routine).trim_start())
    } else {
        format!("{} done", mark(ctx, &routine).trim_start())
    };
    Ok(Output::new(
        json!({"routine": routine, "already_done": already}),
        human,
    ))
}
