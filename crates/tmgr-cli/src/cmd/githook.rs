use crate::cli::GitCommand;
use crate::ctx::Ctx;
use crate::gitx::{self, Git};
use crate::interact;
use crate::out::Output;
use serde_json::json;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use tmgr_core::branch::key_from_branch;
use tmgr_core::config::write_atomic;
use tmgr_core::error::{Error, Result};
use tmgr_core::trailer::add_refs;

const MAX_MESSAGE: u64 = 1024 * 1024;
pub const HOOK_MARKER: &str = "# tmgr-managed commit-msg hook";
const PREV: &str = "commit-msg.tmgr-prev";

pub fn commit_msg(file: &Path) -> i32 {
    if let Err(why) = try_commit_msg(file) {
        eprintln!("tmgr: commit-msg hook: {why}");
    }
    0
}

fn try_commit_msg(file: &Path) -> std::result::Result<(), String> {
    let meta = fs::metadata(file).map_err(|_| "cannot read the message file")?;
    if !meta.is_file() || meta.len() > MAX_MESSAGE {
        return Err("the message file is not a regular file or is too large".into());
    }
    let mut raw = Vec::new();
    fs::File::open(file)
        .and_then(|f| f.take(MAX_MESSAGE).read_to_end(&mut raw))
        .map_err(|_| "cannot read the message file")?;
    let message = String::from_utf8(raw).map_err(|_| "the message is not valid UTF-8")?;
    if message.starts_with("Merge ") {
        return Ok(());
    }
    let cwd = std::env::current_dir().map_err(|_| "no working directory")?;
    let Some(branch) = Git::at(cwd).current_branch() else {
        return Ok(());
    };
    let Some(key) = key_from_branch(&branch) else {
        return Ok(());
    };
    if let Some(updated) = add_refs(&message, &key) {
        fs::write(file, updated).map_err(|_| "cannot write the message file")?;
    }
    Ok(())
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

pub fn hook_script(exe: &str, chain_repo_hook: bool) -> String {
    let mut s = format!("#!/bin/sh\n{HOOK_MARKER}\n");
    s.push_str(&format!(
        "prev=\"$(dirname \"$0\")/{PREV}\"\nif [ -x \"$prev\" ]; then\n  \"$prev\" \"$@\" || exit $?\nfi\n"
    ));
    if chain_repo_hook {
        s.push_str(
            "repo_hook=\"$(git rev-parse --git-common-dir 2>/dev/null)/hooks/commit-msg\"\nif [ -x \"$repo_hook\" ] && [ \"$repo_hook\" != \"$0\" ]; then\n  \"$repo_hook\" \"$@\" || exit $?\nfi\n",
        );
    }
    s.push_str(&format!(
        "{} git hook commit-msg \"$1\" || true\nexit 0\n",
        sh_quote(exe)
    ));
    s
}

pub fn run(ctx: &Ctx, cmd: &GitCommand) -> Result<Output> {
    match cmd {
        GitCommand::InstallHooks { global } => install(ctx, *global),
        GitCommand::Hook { .. } => Err(Error::internal("the hook runs without a session")),
    }
}

struct Target {
    dir: PathBuf,
    shared: bool,
    set_global_config: bool,
}

fn absolute(dir: &str, base: &Path) -> PathBuf {
    let p = PathBuf::from(dir);
    if p.is_absolute() {
        p
    } else {
        base.join(p)
    }
}

fn target(ctx: &Ctx, global: bool) -> Result<Target> {
    if global {
        let cwd = std::env::current_dir()?;
        let git = Git::at(cwd);
        return match git.query(&["config", "--global", "--path", "--get", "core.hooksPath"]) {
            Some(p) if PathBuf::from(&p).is_absolute() => Ok(Target {
                dir: PathBuf::from(p),
                shared: true,
                set_global_config: false,
            }),
            Some(_) => Err(Error::usage("the global core.hooksPath is relative")
                .hint("make it absolute first")),
            None => Ok(Target {
                dir: ctx.dir.join("hooks"),
                shared: true,
                set_global_config: true,
            }),
        };
    }
    let repo = gitx::cwd_repo().ok_or_else(|| {
        Error::usage("not inside a git repository").hint("run it in a repository, or use --global")
    })?;
    let top = repo.dir().to_path_buf();
    if let Some(line) = repo.query(&[
        "config",
        "--show-scope",
        "--path",
        "--get",
        "core.hooksPath",
    ]) {
        let (scope, value) = line.split_once('\t').unwrap_or(("local", line.as_str()));
        let dir = absolute(value, &top);
        let outside = !dir.starts_with(&top)
            || dir
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir));
        return Ok(Target {
            dir,
            shared: outside || matches!(scope, "global" | "system"),
            set_global_config: false,
        });
    }
    let hooks = repo
        .query(&["rev-parse", "--git-path", "hooks"])
        .ok_or_else(|| Error::internal("cannot locate the hooks directory"))?;
    Ok(Target {
        dir: absolute(&hooks, &top),
        shared: false,
        set_global_config: false,
    })
}

fn install(ctx: &Ctx, global: bool) -> Result<Output> {
    let t = target(ctx, global)?;
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| fs::canonicalize(p).ok())
        .and_then(|p| p.to_str().map(str::to_string))
        .ok_or_else(|| Error::internal("cannot locate the tmgr binary"))?;
    let hook = t.dir.join("commit-msg");
    let prev = t.dir.join(PREV);
    let existing = match fs::read_to_string(&hook) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => {
            return Err(
                Error::conflict("the existing commit-msg hook cannot be read as text")
                    .hint("move it away, then run the command again"),
            )
        }
    };
    let ours = existing.as_deref().is_some_and(|t| t.contains(HOOK_MARKER));
    let moves_existing = existing.is_some() && !ours;
    if moves_existing && prev.exists() {
        return Err(Error::conflict(format!(
            "{} already exists next to a foreign commit-msg hook",
            PREV
        ))
        .hint("resolve the two hooks by hand, then run the command again"));
    }
    let script = hook_script(&exe, t.shared);
    if existing.as_deref() == Some(script.as_str()) && !t.set_global_config {
        return Ok(Output::new(
            json!({"hook": hook.display().to_string(), "chained": false, "backup": null, "unchanged": true, "global": t.shared}),
            ctx.style.muted("The commit-msg hook is already installed"),
        ));
    }

    let mut plan = vec![format!("write {}", hook.display())];
    if moves_existing {
        plan.push(format!(
            "move the existing hook to {} (it keeps running first)",
            prev.display()
        ));
    }
    if t.set_global_config {
        plan.push(format!(
            "run: git config --global core.hooksPath {}",
            t.dir.display()
        ));
    }
    if t.shared {
        plan.push(
            "this directory is used by every repository: hooks in a repository's own hooks directory stop running, except commit-msg, which is chained"
                .to_string(),
        );
    }
    if !ctx.json && !ctx.quiet {
        eprintln!("tmgr will:");
        for step in &plan {
            eprintln!("  - {step}");
        }
    }
    if t.shared {
        interact::confirm("Change the hooks for every repository?", false, false)?;
    }

    fs::create_dir_all(&t.dir)?;
    let mut backup = None;
    if moves_existing {
        fs::rename(&hook, &prev)?;
        backup = Some(prev.display().to_string());
    }
    write_atomic(&hook, script.as_bytes(), 0o755)?;
    if t.set_global_config {
        let dir = t
            .dir
            .to_str()
            .ok_or_else(|| Error::usage("the hooks path is not valid UTF-8"))?;
        let cwd = std::env::current_dir()?;
        Git::at(cwd).run_ok(&["config", "--global", "core.hooksPath", dir], "config")?;
    }
    let mut human = format!(
        "{} commit-msg hook installed in {}",
        ctx.style.good("\u{2713}"),
        hook.display()
    );
    if let Some(b) = &backup {
        human.push_str(&format!(
            "\n  your previous hook was moved to {b} and still runs first"
        ));
    }
    Ok(Output::new(
        json!({"hook": hook.display().to_string(), "chained": backup.is_some(), "backup": backup, "unchanged": false, "global": t.shared}),
        human,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_single_quoted_safely() {
        assert_eq!(sh_quote("/a/b"), "'/a/b'");
        assert_eq!(sh_quote("/a'b; rm -rf ~"), "'/a'\\''b; rm -rf ~'");
    }

    #[test]
    fn the_script_calls_the_binary_by_absolute_path_and_always_exits_zero() {
        let s = hook_script("/opt/tmgr/bin/tmgr", false);
        assert!(s.starts_with("#!/bin/sh\n# tmgr-managed commit-msg hook\n"));
        assert!(s.contains("'/opt/tmgr/bin/tmgr' git hook commit-msg \"$1\" || true\nexit 0\n"));
        assert!(s.contains("commit-msg.tmgr-prev"));
        assert!(!s.contains("git-common-dir"));
        assert!(hook_script("/x", true).contains("git-common-dir"));
    }

    #[test]
    fn the_script_is_valid_shell() {
        let tmp = tempfile::tempdir().unwrap();
        for global in [false, true] {
            let p = tmp.path().join("hook.sh");
            fs::write(&p, hook_script("/it's/tmgr", global)).unwrap();
            let status = std::process::Command::new("sh")
                .arg("-n")
                .arg(&p)
                .status()
                .unwrap();
            assert!(status.success());
        }
    }
}
