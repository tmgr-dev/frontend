use crate::cli::WsCommand;
use crate::ctx::{local_workspaces, Ctx};
use crate::interact;
use crate::out::{table, Output};
use serde_json::json;
use std::io::{BufRead, Write};
use tmgr_core::config::Config;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::local;
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

/// Cloud and local workspaces together, plus the code of the current one.
fn all_workspaces(ctx: &Ctx) -> Result<(Vec<Workspace>, Option<String>)> {
    let mut all: Vec<Workspace> = Vec::new();
    let locals = local_workspaces()?;
    match ctx
        .cloud_client()
        .and_then(|client| ctx.workspaces(client).map(<[_]>::to_vec))
    {
        Ok(cloud) => all.extend(cloud),
        Err(e) if locals.is_empty() => return Err(e),
        Err(e) if e.kind == Kind::Auth => {}
        Err(e) => ctx.warn(
            "cloud_unavailable",
            format!("cloud workspaces are not listed: {}", e.message),
        ),
    }
    all.extend(locals);
    let wanted = ctx.wanted_workspace()?;
    let current = match &wanted {
        Some(code) => all.iter().find(|w| w.code.eq_ignore_ascii_case(code)),
        None => all
            .iter()
            .find(|w| w.is_default && !w.local)
            .or_else(|| all.iter().find(|w| !w.local)),
    }
    .map(|w| w.code.clone());
    Ok((all, current))
}

fn list(ctx: &Ctx) -> Result<Output> {
    let (all, current) = all_workspaces(ctx)?;
    let is_current = |w: &Workspace| current.as_deref() == Some(w.code.as_str());
    let data: Vec<_> = all
        .iter()
        .map(|w| {
            json!({
                "id": w.id,
                "code": sanitize_line(&w.code),
                "name": sanitize_line(&w.name),
                "current": is_current(w),
                "local": w.local,
            })
        })
        .collect();
    let rows: Vec<Vec<String>> = all
        .iter()
        .map(|w| {
            vec![
                if is_current(w) { "*" } else { " " }.to_string(),
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
    let (all, current) = all_workspaces(ctx)?;
    let rows: Vec<Vec<String>> = all
        .iter()
        .enumerate()
        .map(|(i, w)| {
            vec![
                if current.as_deref() == Some(w.code.as_str()) { "*" } else { " " }.to_string(),
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
    let code = choose_workspace(&line, &all)?;
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
    let chosen = if let Some(local) = local::local_code(code) {
        local::find(local)?;
        format!("{}{local}", local::PREFIX)
    } else {
        if !is_workspace_code(code) {
            return Err(Error::usage("invalid workspace code")
                .hint("a workspace code has letters, digits, - and _ (or local:<code>)"));
        }
        let client = ctx.cloud_client()?;
        ctx.workspaces(client)?
            .iter()
            .find(|w| w.code.eq_ignore_ascii_case(code))
            .map(|w| w.code.clone())
            .ok_or_else(|| {
                Error::not_found(format!("workspace {code} not found")).hint("tmgr ws list")
            })?
    };
    let mut cfg = Config::load(&ctx.dir)?;
    cfg.default_workspace = Some(chosen.clone());
    cfg.save(&ctx.dir)?;
    Ok(Output::new(
        json!({"default_workspace": chosen}),
        format!(
            "{} default workspace is now {}",
            ctx.style.good("\u{2713}"),
            sanitize_line(&chosen)
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(id: i64, code: &str) -> Workspace {
        Workspace {
            id,
            name: code.to_string(),
            code: code.to_string(),
            is_default: false,
            local: false,
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
