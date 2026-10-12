use crate::cli::WsCommand;
use crate::ctx::Ctx;
use crate::out::{table, Output};
use serde_json::json;
use tmgr_core::config::Config;
use tmgr_core::error::{Error, Result};
use tmgr_core::refs::is_workspace_code;
use tmgr_core::text::sanitize_line;

pub fn run(ctx: &Ctx, cmd: &WsCommand) -> Result<Output> {
    match cmd {
        WsCommand::List => list(ctx),
        WsCommand::Use { code } => use_ws(ctx, code),
    }
}

fn list(ctx: &Ctx) -> Result<Output> {
    let client = ctx.client()?;
    let current = ctx.workspace(client)?;
    let all = ctx.workspaces(client)?;
    let data: Vec<_> = all
        .iter()
        .map(|w| {
            json!({
                "id": w.id,
                "code": sanitize_line(&w.code),
                "name": sanitize_line(&w.name),
                "current": w.id == current.id,
            })
        })
        .collect();
    let rows: Vec<Vec<String>> = all
        .iter()
        .map(|w| {
            vec![
                if w.id == current.id { "*" } else { " " }.to_string(),
                sanitize_line(&w.code),
                sanitize_line(&w.name),
            ]
        })
        .collect();
    Ok(Output::new(json!({ "workspaces": data }), table(&rows)))
}

fn use_ws(ctx: &Ctx, code: &str) -> Result<Output> {
    if !is_workspace_code(code) {
        return Err(if code.starts_with("local:") {
            Error::usage("local workspaces are not supported by this version")
        } else {
            Error::usage("invalid workspace code")
                .hint("a workspace code has letters, digits, - and _")
        });
    }
    let client = ctx.client()?;
    let found = ctx
        .workspaces(client)?
        .iter()
        .find(|w| w.code.eq_ignore_ascii_case(code))
        .cloned()
        .ok_or_else(|| {
            Error::not_found(format!("workspace {code} not found")).hint("tmgr ws list")
        })?;
    let mut cfg = Config::load(&ctx.dir)?;
    cfg.default_workspace = Some(found.code.clone());
    cfg.save(&ctx.dir)?;
    Ok(Output::new(
        json!({"default_workspace": found.code}),
        format!(
            "{} default workspace is now {}",
            ctx.style.good("\u{2713}"),
            sanitize_line(&found.code)
        ),
    ))
}
