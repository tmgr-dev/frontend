use super::fmt::display_path;
use super::{fail, Usage};
use crate::cli_usage::{AccountsCommand, ToolArg};
use crate::ctx::Ctx;
use crate::out::{table, Output};
use serde_json::json;
use tmgr_core::error::{Error, Result};
use tmgr_core::text::sanitize_line;
use tmgr_usage::paths;
use tmgr_usage::{Account, Tool};

pub fn tool_of(t: ToolArg) -> Tool {
    match t {
        ToolArg::Claude => Tool::Claude,
        ToolArg::Codex => Tool::Codex,
    }
}

pub fn run(ctx: &Ctx, cmd: Option<&AccountsCommand>) -> Result<Output> {
    let u = Usage::open()?;
    match cmd.unwrap_or(&AccountsCommand::List) {
        AccountsCommand::List => list(ctx, &u),
        AccountsCommand::Add { tool, root, label } => {
            let root = std::fs::canonicalize(root)
                .map_err(|_| Error::usage("the root directory does not exist"))?;
            if !root.is_dir() {
                return Err(Error::usage("the root is not a directory"));
            }
            let account = Account {
                tool: tool_of(*tool),
                label: label.clone(),
                root,
            };
            paths::add_account(&u.dir, &u.env, account.clone()).map_err(fail)?;
            Ok(Output::new(
                json!({"added": account_json(&account, false)}),
                format!(
                    "Added {} account {} ({}). The next `tmgr usage` scans it.",
                    account.tool.as_str(),
                    account.label,
                    display_path(&account.root)
                ),
            ))
        }
        AccountsCommand::Remove { label } => {
            let removed = paths::remove_account(&u.dir, label).map_err(fail)?;
            if !removed {
                return Err(
                    Error::not_found("no account with this label was added by you")
                        .hint("tmgr usage accounts list"),
                );
            }
            Ok(Output::new(
                json!({"removed": label}),
                format!(
                    "Removed account {}. Already stored usage stays.",
                    sanitize_line(label)
                ),
            ))
        }
    }
}

fn account_json(a: &Account, default: bool) -> serde_json::Value {
    json!({
        "label": a.label,
        "tool": a.tool.as_str(),
        "root": display_path(&a.root),
        "source": if default { "default" } else { "user" },
        "exists": a.root.is_dir(),
    })
}

fn list(ctx: &Ctx, u: &Usage) -> Result<Output> {
    let user = paths::list_user_accounts(&u.dir).map_err(fail)?;
    let all = u.accounts()?;
    let rows: Vec<_> = all
        .iter()
        .map(|a| {
            let is_user = user.iter().any(|x| x.label == a.label);
            account_json(a, !is_user)
        })
        .collect();
    let human = if rows.is_empty() {
        ctx.style.muted(
            "No accounts found. Add one: tmgr usage accounts add --tool claude --root DIR --label work",
        )
    } else {
        let mut lines = vec![vec![
            "Label".to_string(),
            "Tool".to_string(),
            "Source".to_string(),
            "Root".to_string(),
        ]];
        for r in &rows {
            let text = |k: &str| sanitize_line(r[k].as_str().unwrap_or(""));
            lines.push(vec![
                text("label"),
                text("tool"),
                text("source"),
                text("root"),
            ]);
        }
        table(&lines)
    };
    Ok(Output::new(json!({ "accounts": rows }), human))
}
