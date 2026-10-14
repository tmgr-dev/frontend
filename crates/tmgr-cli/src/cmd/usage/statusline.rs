use super::fmt::{display_path, scrub, tilde};
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
use tmgr_core::error::{Error, Result};
use tmgr_core::redact::redact_secrets;
use tmgr_usage::paths::{self, Env};
use tmgr_usage::statusline::{self, InstallPlan};
use tmgr_usage::Tool;

const MAX_STDIN: u64 = 1024 * 1024;
const MAX_SETTINGS: u64 = 4 * 1024 * 1024;

pub fn statusline_hook(args: &StatuslineHookArgs) -> i32 {
    let mut input = Vec::new();
    let _ = std::io::stdin().take(MAX_STDIN).read_to_end(&mut input);
    let snaps = if input.is_empty() {
        None
    } else {
        snapshot_dir()
    };
    let tag = snaps.as_ref().and_then(|_| {
        paths::claude_config_dir(&Env::from_process()).map(|d| statusline::account_tag(&d))
    });
    let out = match snaps {
        Some(dir) => statusline::run_hook(
            &input,
            tag.as_deref(),
            &dir,
            args.chain.as_deref(),
            &mut statusline::run_shell_chain,
        ),
        None => match args
            .chain
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
        {
            Some(cmd) => statusline::run_shell_chain(cmd, &input),
            None => Vec::new(),
        },
    };
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(&out);
    let _ = stdout.flush();
    0
}

fn snapshot_dir() -> Option<PathBuf> {
    let dir = paths::ensure_data_dir(&Env::from_process()).ok()?;
    let snaps = paths::snapshots_dir(&dir).join("claude");
    paths::create_private_dir(&snaps).ok()?;
    Some(snaps)
}

pub fn run(ctx: &Ctx, cmd: &StatuslineCommand) -> Result<Output> {
    if cfg!(windows) {
        return Err(Error::usage(
            "installing the Claude Code status line is not supported on Windows yet",
        ));
    }
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

fn refuse_unsafe_link(path: &PathBuf) -> Result<()> {
    let Ok(link) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    if !link.file_type().is_symlink() {
        return Ok(());
    }
    match std::fs::metadata(path) {
        Ok(m) if m.is_dir() => Err(Error::usage(
            "settings.json is a symlink to a directory; nothing written",
        )),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Error::usage(
            "settings.json is a symlink whose target does not exist; nothing written",
        )),
        Err(_) => Err(Error::internal("cannot read settings.json")),
    }
}

fn read_settings(path: &PathBuf) -> Result<(PathBuf, String, bool)> {
    refuse_unsafe_link(path)?;
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

const REDACTED: &str = "<redacted>";
const SECRET_WORDS: [&str; 17] = [
    "token",
    "secret",
    "password",
    "passwd",
    "pwd",
    "--pass",
    "pass=",
    "apikey",
    "api-key",
    "api_key",
    "auth",
    "bearer",
    "authorization",
    "credential",
    "private_key",
    "private-key",
    "x-api",
];

fn credential_argument(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    words.iter().any(|w| {
        matches!(*w, "-u" | "--user")
            || w.starts_with("--user=")
            || w.split_once('@')
                .is_some_and(|(before, _)| before.contains(':'))
    })
}

fn long_token(text: &str) -> bool {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || "+/_=-".contains(c)))
        .any(|piece| {
            piece.len() >= 24
                && piece.chars().any(|c| c.is_ascii_digit())
                && piece.chars().any(|c| c.is_ascii_alphabetic())
        })
}

fn looks_secret(text: &str) -> bool {
    if redact_secrets(text) != text {
        return true;
    }
    let lower = text.to_ascii_lowercase();
    SECRET_WORDS.iter().any(|w| lower.contains(w)) || credential_argument(text) || long_token(text)
}

fn shown(text: &str) -> String {
    if looks_secret(text) {
        REDACTED.to_string()
    } else {
        scrub(text)
    }
}

fn region_lines(settings: &str) -> Vec<String> {
    let region = serde_json::from_str::<Value>(settings)
        .ok()
        .and_then(|v| v.get("statusLine").cloned());
    let Some(Value::Object(map)) = region else {
        return Vec::new();
    };
    let mut lines = vec!["\"statusLine\": {".to_string()];
    for (k, v) in &map {
        let value = match v {
            Value::String(t) => json!(shown(t)),
            other if looks_secret(&other.to_string()) => json!(REDACTED),
            other => other.clone(),
        };
        lines.push(format!("  {}: {}", json!(k), value));
    }
    lines.push("}".to_string());
    lines
}

fn region_diff(old: &str, new: &str) -> String {
    let (o, n) = (region_lines(old), region_lines(new));
    let mut p = 0;
    while p < o.len() && p < n.len() && o[p] == n[p] {
        p += 1;
    }
    let mut s = 0;
    while s < o.len() - p && s < n.len() - p && o[o.len() - 1 - s] == n[n.len() - 1 - s] {
        s += 1;
    }
    if p == o.len() && p == n.len() {
        return String::new();
    }
    let mut out = String::new();
    if p > 0 {
        out.push_str(&format!("  {}\n", o[p - 1]));
    }
    for l in &o[p..o.len() - s] {
        out.push_str(&format!("- {l}\n"));
    }
    for l in &n[p..n.len() - s] {
        out.push_str(&format!("+ {l}\n"));
    }
    if s > 0 {
        out.push_str(&format!("  {}\n", o[o.len() - s]));
    }
    out
}

fn ensure_unchanged(path: &PathBuf, expected: &str, existed: bool) -> Result<()> {
    let (_, now, exists) = read_settings(path)?;
    if exists != existed || now != expected {
        return Err(
            Error::conflict("settings.json changed while waiting; nothing written")
                .hint("run the command again"),
        );
    }
    Ok(())
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
    let diff = region_diff(&old, &plan.new_settings_text);
    let diff = if diff.is_empty() && changed {
        "  (only the formatting of the file changes)\n".to_string()
    } else {
        diff
    };
    let account_flag = account
        .account
        .as_deref()
        .map_or(String::new(), |a| format!(" --account {a}"));
    let apply = matches!(mode, Mode::Install { apply: true } | Mode::Uninstall);
    let data = |installed: bool, backup: Option<&PathBuf>| {
        json!({
            "account": label,
            "settings_path": display_path(&real),
            "installed_command": shown(&plan.installed_command),
            "previous_command": plan.previous_command.as_deref().map(shown),
            "changed": changed,
            "installed": installed,
            "backup": backup.map(|b| tilde(b)),
        })
    };
    if !apply {
        let human = format!(
            "Claude Code settings: {}\nstatusLine command that reports rate limits to tmgr:\n  {}\n{}\nNothing was changed. Apply it (needs a terminal confirmation):\n  tmgr usage statusline install{account_flag} --install",
            real.display(),
            shown(&plan.installed_command),
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
    let stamp = chrono::Local::now().format("%Y%m%d%H%M%S%6f").to_string();
    let backup = init::apply_checked(
        &Change {
            path: real.clone(),
            old: old.clone(),
            new: plan.new_settings_text.clone(),
        },
        &stamp,
        true,
        &|| ensure_unchanged(&real, &old, existed),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settings_file_that_changed_since_the_diff_is_refused_with_exit_six() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{}\n").unwrap();
        assert!(ensure_unchanged(&path, "{}\n", true).is_ok());
        std::fs::write(&path, "{\"theme\":\"dark\"}\n").unwrap();
        let err = ensure_unchanged(&path, "{}\n", true).unwrap_err();
        assert_eq!(err.exit_code(), 6);
        assert!(err
            .to_string()
            .contains("changed while waiting; nothing written"));
        std::fs::remove_file(&path).unwrap();
        assert!(ensure_unchanged(&path, "{}\n", true).is_err());
        assert!(ensure_unchanged(&path, "{}\n", false).is_ok());
    }

    #[test]
    fn secret_looking_commands_are_hidden() {
        assert_eq!(shown("my-line --token=abc"), REDACTED);
        assert_eq!(shown("curl -H 'Authorization: Bearer abc'"), REDACTED);
        assert_eq!(shown("echo hi"), "echo hi");
    }

    #[test]
    fn credentials_in_arguments_urls_and_long_tokens_are_hidden() {
        for c in [
            &["curl", "-u", "alice:pass123", "https://x"].join(" "),
            "ssh host --pass=hunter2",
            "run --api-key abcdef0123456789abcdef0123456789",
            &["curl", "--user", "alice:pw", "https://x"].join(" "),
            "git clone https://bob:pw@host/repo",
            &["curl -H 'Authorization:", "Basic YWxpY2U6cHc=' https://x"].join(" "),
            "sh -c 'PWD=1 x'",
            "x deadbeefdeadbeefdeadbeef0123456789",
            "login --Password hunter2",
        ] {
            assert_eq!(shown(c), REDACTED, "{c}");
        }
        for c in ["echo hi", "my-line --compact", "git clone git@host:a/b.git"] {
            assert_eq!(shown(c), c, "{c}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_or_directory_symlink_is_refused_before_anything_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("settings.json");
        std::os::unix::fs::symlink(dir.path().join("missing.json"), &link).unwrap();
        assert!(read_settings(&link).is_err());
        std::fs::remove_file(&link).unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("d"), &link).unwrap();
        assert!(read_settings(&link).is_err());
        std::fs::remove_file(&link).unwrap();
        std::fs::write(dir.path().join("real.json"), "{}\n").unwrap();
        std::os::unix::fs::symlink(dir.path().join("real.json"), &link).unwrap();
        assert!(read_settings(&link).is_ok());
    }

    #[test]
    fn the_region_diff_has_at_most_one_context_line() {
        let old = r#"{"statusLine":{"type":"command","command":"echo hi","padding":0}}"#;
        let new = r#"{"statusLine":{"type":"command","command":"tmgr x","padding":0}}"#;
        let d = region_diff(old, new);
        assert_eq!(d.lines().filter(|l| l.starts_with("  ")).count(), 2);
        assert!(d.contains("- ") && d.contains("+ "));
        assert!(region_diff(old, old).is_empty());
    }
}
