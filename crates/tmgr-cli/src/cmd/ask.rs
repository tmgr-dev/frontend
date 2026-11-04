use crate::cli_agent::AskArgs;
use crate::ctx::Ctx;
use crate::out::Output;
use tmgr_core::error::{Error, Result};

#[cfg(not(unix))]
pub fn run(_ctx: &Ctx, _args: &AskArgs) -> Result<Output> {
    Err(Error::usage(
        "tmgr ask is not supported on this platform (the Decisions inbox is macOS and Linux only)",
    ))
}

#[cfg(unix)]
const NO_ANSWER_EXIT: i32 = 8;
#[cfg(unix)]
const FOREVER: std::time::Duration = std::time::Duration::from_secs(10 * 365 * 86400);

#[cfg(unix)]
fn spec_of(args: &AskArgs) -> std::result::Result<crate::cmd::decisions::Spec, String> {
    use crate::cmd::decisions::{parse_check, parse_fact, Spec};
    Ok(Spec {
        title: args.title.clone(),
        question: args.question.clone(),
        context: args.context.clone(),
        kind: args.kind.clone(),
        blocking: args.blocking || args.wait,
        options: args.options.clone(),
        recommended: args.recommended,
        facts: args
            .facts
            .iter()
            .map(|f| parse_fact(f))
            .collect::<std::result::Result<_, _>>()?,
        checks: args
            .checks
            .iter()
            .map(|c| parse_check(c))
            .collect::<std::result::Result<_, _>>()?,
        repo: args.repo.clone(),
        base: args.base.clone(),
        head: args.head.clone(),
        task: args.task.clone(),
        pr: args.pr.clone(),
    })
}

#[cfg(unix)]
pub fn run(ctx: &Ctx, args: &AskArgs) -> Result<Output> {
    use crate::cmd::decisions::{
        event_text, parent_pid, pending_json, send_card, source, wait_loop, Sent, Waited,
        PENDING_NOTE,
    };
    use serde_json::json;
    use std::time::{Duration, Instant};

    let agent = if std::env::var_os("CLAUDECODE").is_some() {
        "claude-code"
    } else if std::env::vars_os().any(|(k, _)| k.to_string_lossy().starts_with("CODEX_SANDBOX")) {
        "codex"
    } else {
        "other"
    };
    let pid = args.wait.then(parent_pid);
    let card = spec_of(args)
        .and_then(|s| s.into_card("cli", source(agent, pid)))
        .map_err(Error::usage)?;
    let id = match send_card(card) {
        Ok(Sent::Live(id)) => id,
        Ok(Sent::Spooled(id)) => {
            ctx.warn("decisions_spooled", PENDING_NOTE);
            let status = if args.wait { "pending" } else { "spooled" };
            let out = Output::new(json!({"card_id": id, "status": status}), id.clone());
            return Ok(if args.wait {
                out.exit(NO_ANSWER_EXIT)
            } else {
                out
            });
        }
        Err(e) if crate::cmd::decisions::is_refusal(&e) => {
            return Err(Error::usage(format!(
                "the Decisions inbox refused the card: {e}"
            )))
        }
        Err(e) => {
            return Err(Error::internal(format!(
                "the card could not be queued: {e}"
            )))
        }
    };
    if !args.wait {
        return Ok(Output::new(
            json!({"card_id": id, "status": "created"}),
            id.clone(),
        ));
    }
    let deadline = Instant::now() + args.timeout.map_or(FOREVER, Duration::from_secs);
    let mut since = 0;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match wait_loop(&id, since, left, false, &mut || {}) {
            Waited::Event(e) => {
                since = e.seq;
                if e.is_final() {
                    return Ok(Output::new(
                        json!({"card_id": id, "status": "answered", "answer": *e}),
                        event_text(&e),
                    ));
                }
                if !ctx.quiet {
                    eprintln!("{}", event_text(&e));
                }
            }
            Waited::Pending { unreachable } => {
                if unreachable {
                    ctx.warn("decisions_unreachable", PENDING_NOTE);
                }
                return Ok(
                    Output::new(pending_json(&id, unreachable), id.clone()).exit(NO_ANSWER_EXIT)
                );
            }
            Waited::Refused(m) => return Err(Error::internal(format!("the Decisions inbox: {m}"))),
        }
    }
}
