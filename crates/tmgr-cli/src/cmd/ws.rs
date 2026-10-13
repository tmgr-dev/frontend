use crate::cli::WsCommand;
use crate::ctx::Ctx;
use crate::interact;
use crate::out::{table, Output};
use serde_json::json;
use std::io::{BufRead, Write};
use tmgr_core::config::Config;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::models::Workspace;
use tmgr_core::refs::is_workspace_code;
use tmgr_core::text::sanitize_line;

pub fn run(ctx: &Ctx, cmd: &Option<WsCommand>) -> Result<Output> {
    match cmd.as_ref().unwrap_or(&WsCommand::List) {
        WsCommand::List => list(ctx),
        WsCommand::Use { code: Some(code) } => use_ws(ctx, code),
        WsCommand::Use { code: None } => pick_ws(ctx),
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

fn pick_ws(ctx: &Ctx) -> Result<Output> {
    if !interact::is_interactive() {
        return Err(Error::usage("a workspace code is required")
            .hint("tmgr ws use <code>, see `tmgr ws list`"));
    }
    let client = ctx.client()?;
    let all = ctx.workspaces(client)?;
    let current = ctx.workspace(client)?;
    let rows: Vec<Vec<String>> = all
        .iter()
        .enumerate()
        .map(|(i, w)| {
            vec![
                if w.id == current.id { "*" } else { " " }.to_string(),
                (i + 1).to_string(),
                sanitize_line(&w.code),
                sanitize_line(&w.name),
            ]
        })
        .collect();
    eprintln!("{}", table(&rows));
    eprint!("Workspace number or code: ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|_| Error::new(Kind::Interrupted, "could not read the answer"))?;
    let code = choose_workspace(&line, all)?;
    use_ws(ctx, &code)
}

pub fn choose_workspace(answer: &str, all: &[Workspace]) -> Result<String> {
    let answer = answer.trim();
    if answer.is_empty() {
        return Err(Error::new(
            Kind::Interrupted,
            "cancelled, nothing was changed",
        ));
    }
    if let Ok(n) = answer.parse::<usize>() {
        return n
            .checked_sub(1)
            .and_then(|i| all.get(i))
            .map(|w| w.code.clone())
            .ok_or_else(|| Error::usage("no workspace with that number").hint("tmgr ws list"));
    }
    all.iter()
        .find(|w| w.code.eq_ignore_ascii_case(answer))
        .map(|w| w.code.clone())
        .ok_or_else(|| {
            Error::not_found(format!("workspace {} not found", sanitize_line(answer)))
                .hint("tmgr ws list")
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(id: u64, code: &str) -> Workspace {
        Workspace {
            id,
            name: code.to_string(),
            code: code.to_string(),
            is_default: false,
        }
    }

    #[test]
    fn answer_picks_by_number_or_code() {
        let all = [ws(1, "personal"), ws(2, "team")];
        assert_eq!(choose_workspace("2\n", &all).unwrap(), "team");
        assert_eq!(choose_workspace(" PERSONAL \n", &all).unwrap(), "personal");
    }

    #[test]
    fn bad_answers_are_errors() {
        let all = [ws(1, "personal")];
        assert_eq!(choose_workspace("0", &all).unwrap_err().exit_code(), 2);
        assert_eq!(choose_workspace("5", &all).unwrap_err().exit_code(), 2);
        assert_eq!(choose_workspace("nope", &all).unwrap_err().exit_code(), 3);
        assert_eq!(choose_workspace("\n", &all).unwrap_err().exit_code(), 130);
    }
}
