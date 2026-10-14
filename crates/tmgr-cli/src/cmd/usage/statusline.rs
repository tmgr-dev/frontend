use super::fmt::{scrub, tilde};
use super::{fail, Usage};
use crate::cli_usage::{StatuslineAccountArgs, StatuslineCommand, StatuslineHookArgs};
use crate::cmd::hookinstall::shell_quote;
use crate::cmd::init::{self, Change};
use crate::ctx::Ctx;
use crate::interact;
use crate::out::Output;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tmgr_core::error::{Error, Result};
use tmgr_usage::paths::{self, Env};
use tmgr_usage::statusline::{self, InstallPlan};
use tmgr_usage::Tool;

const MAX_STDIN: u64 = 1024 * 1024;
const MAX_SETTINGS: u64 = 4 * 1024 * 1024;
const MAX_CHAIN_OUTPUT: u64 = 256 * 1024;
const CHAIN_TIMEOUT: Duration = Duration::from_secs(2);

pub fn statusline_hook(args: &StatuslineHookArgs) -> i32 {
    let mut input = Vec::new();
    let _ = std::io::stdin().take(MAX_STDIN).read_to_end(&mut input);
    if !input.is_empty() {
        let tag = std::env::var("CLAUDE_CONFIG_DIR")
            .ok()
            .filter(|v| !v.is_empty());
        let _ = save_snapshot(tag.as_deref(), &input);
    }
    if let Some(cmd) = args.chain.as_deref() {
        let out = run_chain(cmd, &input);
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(&out);
        let _ = stdout.flush();
    }
    0
}

fn save_snapshot(tag: Option<&str>, input: &[u8]) -> tmgr_usage::Result<()> {
    let env = Env::from_process();
    let dir = paths::ensure_data_dir(&env)?;
    let snaps = paths::snapshots_dir(&dir).join("claude");
    paths::create_private_dir(&snaps)?;
    statusline::write_snapshot(&snaps, tag, input).map(|_| ())
}

fn run_chain(cmd: &str, input: &[u8]) -> Vec<u8> {
    let Ok(mut child) = Command::new("/bin/sh")
        .arg("-c")
        .arg(cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Vec::new();
    };
    if let Some(mut stdin) = child.stdin.take() {
        let data = input.to_vec();
        std::thread::spawn(move || {
            let _ = stdin.write_all(&data);
        });
    }
    let (tx, rx) = std::sync::mpsc::channel();
    if let Some(stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stdout.take(MAX_CHAIN_OUTPUT).read_to_end(&mut buf);
            let _ = tx.send(buf);
        });
    }
    let deadline = Instant::now() + CHAIN_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Vec::new();
            }
        }
    }
    rx.recv_timeout(Duration::from_millis(500))
        .unwrap_or_default()
}

pub fn run(ctx: &Ctx, cmd: &StatuslineCommand) -> Result<Output> {
    match cmd {
        StatuslineCommand::Install(a) => {
            change(ctx, &a.account, Mode::Install { apply: a.install })
        }
        StatuslineCommand::Uninstall(a) => change(ctx, a, Mode::Uninstall),
    }
}

enum Mode {
    Install { apply: bool },
    Uninstall,
}

fn settings_path(account: &StatuslineAccountArgs) -> Result<(String, PathBuf)> {
    let u = Usage::open()?;
    let label = account.account.as_deref().unwrap_or("claude");
    let acc = u
        .accounts()?
        .into_iter()
        .find(|a| a.tool == Tool::Claude && a.label == label)
        .ok_or_else(|| {
            Error::not_found("no Claude account with this label").hint("tmgr usage accounts list")
        })?;
    Ok((acc.label, acc.root.join("settings.json")))
}

fn read_settings(path: &PathBuf) -> Result<(PathBuf, String, bool)> {
    let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
    match std::fs::metadata(&real) {
        Ok(m) if m.len() > MAX_SETTINGS => Err(Error::usage("settings.json is too large to edit")),
        Ok(_) => {
            let text = std::fs::read_to_string(&real)
                .map_err(|_| Error::usage("settings.json is not readable UTF-8 text"))?;
            Ok((real, text, true))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((real, "{}\n".to_string(), false)),
        Err(_) => Err(Error::internal("cannot read settings.json")),
    }
}

fn project_overrides(ctx: &Ctx) {
    let Ok(cwd) = std::env::current_dir() else {
        return;
    };
    for name in ["settings.json", "settings.local.json"] {
        let path = cwd.join(".claude").join(name);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let defines = serde_json::from_str::<Value>(&text)
            .ok()
            .is_some_and(|v| v.get("statusLine").is_some());
        if defines {
            ctx.warn(
                "statusline_project_override",
                format!(
                    ".claude/{name} in this directory defines its own statusLine, which overrides the account setting here"
                ),
            );
        }
    }
}

fn change(ctx: &Ctx, account: &StatuslineAccountArgs, mode: Mode) -> Result<Output> {
    let (label, settings) = settings_path(account)?;
    let (real, old, existed) = read_settings(&settings)?;
    let plan: InstallPlan = match mode {
        Mode::Install { .. } => {
            let exe = std::env::current_exe()
                .ok()
                .and_then(|p| p.to_str().map(str::to_string))
                .ok_or_else(|| Error::internal("cannot locate the tmgr executable"))?;
            statusline::plan_install(&old, &exe, None).map_err(fail)?
        }
        Mode::Uninstall => statusline::plan_uninstall(&old).map_err(fail)?,
    };
    if matches!(mode, Mode::Install { .. }) {
        project_overrides(ctx);
    }
    for w in &plan.warnings {
        ctx.warn("statusline", w.clone());
    }
    let changed = plan.new_settings_text != old;
    let diff = init::diff(&old, &plan.new_settings_text);
    let account_flag = account
        .account
        .as_deref()
        .map_or(String::new(), |a| format!(" --account {a}"));
    let apply = matches!(mode, Mode::Install { apply: true } | Mode::Uninstall);
    let data = |installed: bool, backup: Option<&PathBuf>| {
        json!({
            "account": label,
            "settings_path": tilde(&real),
            "installed_command": scrub(&plan.installed_command),
            "previous_command": plan.previous_command.as_deref().map(scrub),
            "changed": changed,
            "installed": installed,
            "backup": backup.map(|b| tilde(b)),
            "diff": scrub(&diff),
        })
    };
    if !apply {
        let human = format!(
            "Claude Code settings: {}\nstatusLine command that reports rate limits to tmgr:\n  {}\n{}\nNothing was changed. Apply it (needs a terminal confirmation):\n  tmgr usage statusline install{account_flag} --install",
            real.display(),
            plan.installed_command,
            if changed {
                format!("\nChange preview:\n{diff}")
            } else {
                "\nIt is already in place.".to_string()
            },
        );
        return Ok(Output::new(data(false, None), human));
    }
    if !changed {
        let what = if matches!(mode, Mode::Uninstall) {
            "nothing to remove"
        } else {
            "already installed"
        };
        return Ok(Output::new(
            data(false, None),
            ctx.style.muted(&format!("{}: {what}", real.display())),
        ));
    }
    if !ctx.json {
        eprintln!("{}", real.display());
        eprint!("{diff}");
    }
    interact::confirm(
        &format!("Write these changes to {}?", real.display()),
        false,
        false,
    )?;
    let stamp = chrono::Local::now().format("%Y%m%d%H%M%S").to_string();
    let backup = init::apply(
        &Change {
            path: real.clone(),
            old,
            new: plan.new_settings_text.clone(),
        },
        &stamp,
    )?;
    let mut human = format!(
        "{} {} {}",
        ctx.style.good("\u{2713}"),
        if matches!(mode, Mode::Uninstall) {
            "restored"
        } else {
            "updated"
        },
        real.display()
    );
    if let Some(b) = &backup {
        human.push_str(&format!(
            "\n  backup: {}\nRoll back with either:\n  tmgr usage statusline uninstall{account_flag}\n  cp {} {}",
            b.display(),
            shell_quote(&b.display().to_string()),
            shell_quote(&real.display().to_string())
        ));
    } else if existed {
        human.push_str("\n  (no backup was written)");
    }
    Ok(Output::new(data(true, backup.as_ref()), human))
}
