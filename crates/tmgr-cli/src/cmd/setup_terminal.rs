use crate::cli::SetupTerminalArgs;
use crate::ctx::Ctx;
use crate::interact;
use crate::out::Output;
use serde::Serialize;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use tmgr_core::error::{Error, Result};

const ITERM_DOMAIN: &str = "com.googlecode.iterm2";
const APP_ROOTS_ENV: &str = "TMGR_SETUP_APP_ROOTS";
const ITERM_DOMAIN_ENV: &str = "TMGR_SETUP_ITERM_DOMAIN";

pub fn remote_session() -> bool {
    std::env::var_os("SSH_CONNECTION").is_some() || std::env::var_os("SSH_TTY").is_some()
}

#[derive(Clone)]
pub struct Env {
    pub home: PathBuf,
    pub xdg: PathBuf,
    pub app_roots: Vec<PathBuf>,
    pub path_dirs: Vec<PathBuf>,
    pub kitty_dir: Option<PathBuf>,
    pub iterm_domain: String,
}

impl Env {
    fn from_process() -> Result<Env> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|h| h.is_absolute())
            .ok_or_else(|| Error::internal("HOME is not set"))?;
        let xdg = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        let app_roots = match std::env::var_os(APP_ROOTS_ENV) {
            Some(v) => std::env::split_paths(&v)
                .filter(|p| !p.as_os_str().is_empty())
                .collect(),
            None => vec![PathBuf::from("/Applications"), home.join("Applications")],
        };
        let path_dirs = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        let iterm_domain =
            std::env::var(ITERM_DOMAIN_ENV).unwrap_or_else(|_| ITERM_DOMAIN.to_string());
        let kitty_dir = std::env::var_os("KITTY_CONFIG_DIRECTORY")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute());
        Ok(Env {
            home,
            xdg,
            app_roots,
            path_dirs,
            kitty_dir,
            iterm_domain,
        })
    }

    fn has_app(&self, bundle: &str) -> bool {
        self.app_roots.iter().any(|r| r.join(bundle).exists())
    }

    fn has_binary(&self, name: &str) -> bool {
        self.path_dirs.iter().any(|d| d.join(name).is_file())
    }

    fn short(&self, p: &Path) -> String {
        match p.strip_prefix(&self.home) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => p.display().to_string(),
        }
    }
}

enum Action {
    File {
        path: PathBuf,
        content: String,
        existed: bool,
    },
    Defaults {
        domain: String,
    },
}

struct Edit {
    shown: String,
    replaces: Option<String>,
    restart: String,
    action: Action,
}

enum Outcome {
    Ok(String),
    Edit(Edit),
    Skip(String),
}

struct Finding {
    app: &'static str,
    outcome: Outcome,
}

#[derive(Serialize)]
struct AppReport {
    app: String,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    backup: Option<String>,
    restart: bool,
    note: String,
}

#[derive(Serialize)]
struct Report {
    remote: bool,
    failed: bool,
    apps: Vec<AppReport>,
}

fn setting(line: &str, key: &str) -> Option<String> {
    let t = line.trim();
    if t.starts_with('#') {
        return None;
    }
    let (k, v) = t.split_once('=')?;
    (k.trim() == key).then(|| v.trim().trim_matches('"').trim_matches('\'').to_string())
}

fn read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn last_line<F: Fn(&str) -> Option<String>>(
    files: &[PathBuf],
    parse: F,
) -> Option<(PathBuf, usize, String, String)> {
    let mut found = None;
    for f in files {
        let Some(body) = read(f) else { continue };
        for (i, l) in body.lines().enumerate() {
            if let Some(v) = parse(l) {
                found = Some((f.clone(), i, v, body.clone()));
            }
        }
    }
    found
}

fn replace_line(body: &str, idx: usize, new: &str) -> String {
    let mut out: Vec<&str> = body.lines().collect();
    out[idx] = new;
    let mut s = out.join("\n");
    s.push('\n');
    s
}

fn append_line(body: &str, new: &str) -> String {
    let mut s = body.to_string();
    if !s.is_empty() && !s.ends_with('\n') {
        s.push('\n');
    }
    s.push_str(new);
    s.push('\n');
    s
}

fn toml_set(
    path: &Path,
    table: &str,
    key: &str,
    value: &str,
) -> std::result::Result<(String, bool), String> {
    let existing = read(path);
    let existed = existing.is_some();
    let mut doc: toml_edit::DocumentMut = existing
        .unwrap_or_default()
        .parse()
        .map_err(|_| format!("{} is not valid TOML, left untouched", path.display()))?;
    if !doc.contains_key(table) {
        doc[table] = toml_edit::table();
    }
    let Some(t) = doc[table].as_table_like_mut() else {
        return Err(format!("[{table}] in {} is not a table", path.display()));
    };
    t.insert(key, toml_edit::value(value));
    Ok((doc.to_string(), existed))
}

fn toml_get(path: &Path, table: &str, key: &str) -> std::result::Result<Option<String>, String> {
    let Some(body) = read(path) else {
        return Ok(None);
    };
    let doc: toml_edit::DocumentMut = body
        .parse()
        .map_err(|_| format!("{} is not valid TOML, left untouched", path.display()))?;
    Ok(doc
        .get(table)
        .and_then(|t| t.get(key))
        .and_then(|v| v.as_str())
        .map(str::to_string))
}

fn warp(env: &Env, app: &'static str, dir: &str, bundle: &str) -> Option<Finding> {
    let root = env.home.join(dir);
    if !root.is_dir() && !env.has_app(bundle) {
        return None;
    }
    let path = root.join("settings.toml");
    let key = "osc52_clipboard_access";
    let outcome = match toml_get(&path, "terminal", key) {
        Err(e) => Outcome::Skip(e),
        Ok(Some(v)) if v == "write_only" || v == "read_write" => {
            Outcome::Ok(format!("{key} is already \"{v}\""))
        }
        Ok(_) => match toml_set(&path, "terminal", key, "write_only") {
            Err(e) => Outcome::Skip(e),
            Ok((content, existed)) => Outcome::Edit(Edit {
                replaces: None,
                shown: format!("[terminal]\n{key} = \"write_only\""),
                restart: format!("Restart {app}"),
                action: Action::File {
                    path,
                    content,
                    existed,
                },
            }),
        },
    };
    Some(Finding { app, outcome })
}

fn ghostty(env: &Env) -> Option<Finding> {
    let mut files = vec![
        env.xdg.join("ghostty/config"),
        env.xdg.join("ghostty/config.ghostty"),
    ];
    let support = env
        .home
        .join("Library/Application Support/com.mitchellh.ghostty");
    files.push(support.join("config"));
    files.push(support.join("config.ghostty"));
    if !files.iter().any(|f| f.exists()) && !env.has_app("Ghostty.app") {
        return None;
    }
    let outcome = match last_line(&files, |l| setting(l, "clipboard-write")) {
        Some((path, idx, v, body)) if v == "deny" || v == "ask" => {
            let line = "clipboard-write = allow";
            Outcome::Edit(Edit {
                replaces: body.lines().nth(idx).map(|l| l.trim().to_string()),
                shown: line.into(),
                restart: "Restart Ghostty (or reload its config)".into(),
                action: Action::File {
                    path,
                    content: replace_line(&body, idx, line),
                    existed: true,
                },
            })
        }
        Some((_, _, v, _)) if !v.is_empty() => {
            Outcome::Ok(format!("clipboard-write is already \"{v}\""))
        }
        Some(_) => Outcome::Ok("clipboard-write is reset to its default (allow)".into()),
        None => Outcome::Ok("clipboard-write defaults to allow".into()),
    };
    Some(Finding {
        app: "Ghostty",
        outcome,
    })
}

fn kitty(env: &Env) -> Option<Finding> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(d) = &env.kitty_dir {
        candidates.push(d.join("kitty.conf"));
    }
    candidates.push(env.xdg.join("kitty/kitty.conf"));
    candidates.push(env.home.join(".config/kitty/kitty.conf"));
    candidates.push(env.home.join("Library/Preferences/kitty/kitty.conf"));
    let files: Vec<PathBuf> = candidates
        .into_iter()
        .filter(|f| f.exists())
        .take(1)
        .collect();
    if files.is_empty() && !env.has_app("kitty.app") {
        return None;
    }
    let outcome = match last_line(&files, |l| {
        let t = l.trim();
        let rest = t.strip_prefix("clipboard_control")?;
        (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim().to_string())
    }) {
        Some((path, idx, rest, body)) => {
            if rest.split_whitespace().any(|t| t == "write-clipboard") {
                Outcome::Ok("clipboard_control already has write-clipboard".into())
            } else {
                let line = format!("clipboard_control {rest} write-clipboard")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                Outcome::Edit(Edit {
                    replaces: body.lines().nth(idx).map(|l| l.trim().to_string()),
                    shown: line.clone(),
                    restart: "Restart kitty (or reload its config with ctrl+shift+f5)".into(),
                    action: Action::File {
                        path,
                        content: replace_line(&body, idx, &line),
                        existed: true,
                    },
                })
            }
        }
        None => Outcome::Ok("clipboard_control defaults to write-clipboard".into()),
    };
    Some(Finding {
        app: "kitty",
        outcome,
    })
}

fn alacritty(env: &Env) -> Option<Finding> {
    let path = [
        env.xdg.join("alacritty/alacritty.toml"),
        env.home.join(".config/alacritty/alacritty.toml"),
        env.home.join(".alacritty.toml"),
    ]
    .into_iter()
    .find(|p| p.exists());
    if path.is_none() && !env.has_app("Alacritty.app") {
        return None;
    }
    let outcome = match path {
        None => Outcome::Ok("osc52 defaults to OnlyCopy".into()),
        Some(path) => match toml_get(&path, "terminal", "osc52") {
            Err(e) => Outcome::Skip(e),
            Ok(Some(v)) if v == "OnlyPaste" => Outcome::Skip(
                "osc52 is \"OnlyPaste\": set it to \"OnlyCopy\" or \"CopyPaste\" yourself".into(),
            ),
            Ok(Some(v)) if v == "Disabled" => {
                let new = "OnlyCopy";
                match toml_set(&path, "terminal", "osc52", new) {
                    Err(e) => Outcome::Skip(e),
                    Ok((content, existed)) => Outcome::Edit(Edit {
                        replaces: Some("osc52 = \"Disabled\"".into()),
                        shown: format!("[terminal]\nosc52 = \"{new}\""),
                        restart: "Alacritty reloads its config by itself, no restart needed".into(),
                        action: Action::File {
                            path,
                            content,
                            existed,
                        },
                    }),
                }
            }
            Ok(Some(v)) => Outcome::Ok(format!("osc52 is already \"{v}\"")),
            Ok(None) => Outcome::Ok("osc52 defaults to OnlyCopy".into()),
        },
    };
    Some(Finding {
        app: "Alacritty",
        outcome,
    })
}

fn wezterm(env: &Env) -> Option<Finding> {
    let found = env.has_app("WezTerm.app")
        || env.xdg.join("wezterm").exists()
        || env.home.join(".wezterm.lua").exists();
    found.then(|| Finding {
        app: "WezTerm",
        outcome: Outcome::Ok("OSC 52 writes are allowed by default, there is no switch".into()),
    })
}

fn tmux(env: &Env) -> Option<Finding> {
    let files: Vec<PathBuf> = [env.home.join(".tmux.conf"), env.xdg.join("tmux/tmux.conf")]
        .into_iter()
        .filter(|f| f.exists())
        .take(1)
        .collect();
    if files.is_empty() && !env.has_binary("tmux") {
        return None;
    }
    let parse = |l: &str| {
        let toks: Vec<&str> = l.split_whitespace().collect();
        if toks.first().is_some_and(|t| t.starts_with('#')) {
            return None;
        }
        matches!(toks.first(), Some(&"set" | &"set-option"))
            .then(|| toks.iter().position(|t| *t == "set-clipboard"))
            .flatten()
            .map(|i| toks.get(i + 1).copied().unwrap_or("").to_string())
    };
    let line = "set -s set-clipboard on";
    let outcome = match last_line(&files, parse) {
        Some((_, _, v, _)) if v == "on" => Outcome::Ok("set-clipboard is already on".into()),
        _ => {
            let (path, body, existed) = match files.first() {
                Some(p) => (p.clone(), read(p).unwrap_or_default(), true),
                None => (env.home.join(".tmux.conf"), String::new(), false),
            };
            Outcome::Edit(Edit {
                replaces: None,
                shown: line.into(),
                restart: format!("Reload tmux with `tmux source-file {}`", env.short(&path)),
                action: Action::File {
                    content: append_line(&body, line),
                    path,
                    existed,
                },
            })
        }
    };
    Some(Finding {
        app: "tmux",
        outcome,
    })
}

fn iterm_plist(env: &Env) -> PathBuf {
    let d = &env.iterm_domain;
    if d.starts_with('/') {
        let p = PathBuf::from(d);
        if p.extension().is_some_and(|e| e == "plist") {
            p
        } else {
            PathBuf::from(format!("{d}.plist"))
        }
    } else {
        env.home.join(format!("Library/Preferences/{d}.plist"))
    }
}

fn defaults(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("/usr/bin/defaults")
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn iterm(env: &Env) -> Option<Finding> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    if !iterm_plist(env).exists() && !env.has_app("iTerm.app") {
        return None;
    }
    let outcome = if defaults(&["read", &env.iterm_domain, "AllowClipboardAccess"]).as_deref()
        == Some("1")
    {
        Outcome::Ok("Applications in terminal may access clipboard is already on".into())
    } else {
        Outcome::Edit(Edit {
            replaces: None,
            shown: format!(
                "defaults write {} AllowClipboardAccess -bool true",
                env.iterm_domain
            ),
            restart: "Quit and reopen iTerm2 (an open iTerm2 can overwrite the setting on quit)"
                .into(),
            action: Action::Defaults {
                domain: env.iterm_domain.clone(),
            },
        })
    };
    Some(Finding {
        app: "iTerm2",
        outcome,
    })
}

fn detect(env: &Env) -> Vec<Finding> {
    [
        warp(env, "Warp", ".warp", "Warp.app"),
        warp(env, "WarpOss", ".warp-oss", "WarpOss.app"),
        iterm(env),
        ghostty(env),
        kitty(env),
        alacritty(env),
        wezterm(env),
        tmux(env),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn stamp() -> String {
    chrono::Utc::now().format("%Y%m%d%H%M%S").to_string()
}

fn backup_name(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let base = format!("{name}.tmgr-backup-{}", stamp());
    let mut candidate = path.with_file_name(&base);
    let mut n = 1;
    while candidate.exists() {
        candidate = path.with_file_name(format!("{base}-{n}"));
        n += 1;
    }
    candidate
}

fn write_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = target.with_file_name(format!(
        ".{}.tmgr-tmp",
        target
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    ));
    std::fs::write(&tmp, content)?;
    if let Ok(meta) = std::fs::metadata(&target) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, &target).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

fn apply(env: &Env, edit: &Edit, snapshot: &Option<String>) -> Result<Option<String>> {
    match &edit.action {
        Action::File {
            path,
            content,
            existed,
        } => {
            if read(path) != *snapshot {
                return Err(Error::internal(format!(
                    "{} changed while waiting, run the command again",
                    env.short(path)
                )));
            }
            let backup = if *existed {
                let b = backup_name(path);
                std::fs::copy(path, &b)
                    .map_err(|e| Error::internal(format!("cannot back up the file: {e}")))?;
                Some(b)
            } else {
                None
            };
            write_atomic(path, content)
                .map_err(|e| Error::internal(format!("cannot write {}: {e}", path.display())))?;
            Ok(backup.map(|b| env.short(&b)))
        }
        Action::Defaults { domain } => apply_defaults(env, domain),
    }
}

fn apply_defaults(env: &Env, domain: &str) -> Result<Option<String>> {
    let plist = iterm_plist(env);
    let backup = backup_name(&plist);
    let saved = plist.exists()
        && std::process::Command::new("/usr/bin/defaults")
            .args(["export", domain])
            .arg(&backup)
            .status()
            .is_ok_and(|s| s.success());
    if plist.exists() && !saved {
        return Err(Error::internal("cannot back up the iTerm2 settings"));
    }
    defaults(&["write", domain, "AllowClipboardAccess", "-bool", "true"])
        .ok_or_else(|| Error::internal("defaults could not write the iTerm2 setting"))?;
    Ok(saved.then(|| env.short(&backup)))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Check,
    Apply,
    Ask,
    Report,
}

fn file_of(env: &Env, edit: &Edit) -> Option<String> {
    match &edit.action {
        Action::File { path, .. } => Some(env.short(path)),
        Action::Defaults { .. } => None,
    }
}

fn run_with(env: &Env, mode: Mode, ask: &mut dyn FnMut(&str, &Edit) -> bool) -> Result<Report> {
    let mut apps = Vec::new();
    let findings = detect(env);
    let snapshots: Vec<Option<String>> = findings
        .iter()
        .map(|f| match &f.outcome {
            Outcome::Edit(Edit {
                action: Action::File { path, .. },
                ..
            }) => read(path),
            _ => None,
        })
        .collect();
    let mut failed = false;
    for (f, snap) in findings.into_iter().zip(snapshots) {
        let rep = match f.outcome {
            Outcome::Ok(note) => AppReport {
                app: f.app.into(),
                status: "already_ok",
                file: None,
                line: None,
                backup: None,
                restart: false,
                note,
            },
            Outcome::Skip(note) => AppReport {
                app: f.app.into(),
                status: "skipped",
                file: None,
                line: None,
                backup: None,
                restart: false,
                note,
            },
            Outcome::Edit(edit) => {
                let file = file_of(env, &edit);
                let go = match mode {
                    Mode::Apply => true,
                    Mode::Ask => ask(f.app, &edit),
                    Mode::Check | Mode::Report => false,
                };
                let mut note = edit.restart.clone();
                let (status, backup) = if go {
                    match apply(env, &edit, &snap) {
                        Ok(b) => ("changed", b),
                        Err(e) => {
                            failed = true;
                            note = e.to_string();
                            ("failed", None)
                        }
                    }
                } else if mode == Mode::Ask {
                    ("declined", None)
                } else {
                    ("would_change", None)
                };
                AppReport {
                    app: f.app.into(),
                    status,
                    file,
                    line: Some(match &edit.replaces {
                        Some(old) => format!("{old}  ->  {}", edit.shown),
                        None => edit.shown,
                    }),
                    backup,
                    restart: status == "changed",
                    note,
                }
            }
        };
        apps.push(rep);
    }
    Ok(Report {
        remote: remote_session(),
        failed,
        apps,
    })
}

fn prompt(env: &Env, app: &str, edit: &Edit) -> bool {
    eprintln!(
        "{app}: allow programs in the terminal to write to your clipboard (OSC 52, write only)."
    );
    if let Some(f) = file_of(env, edit) {
        eprintln!("  file: {f}");
    }
    if let Some(old) = &edit.replaces {
        eprintln!("  change: {old}");
        eprintln!("  to:     {}", edit.shown);
    } else {
        for l in edit.shown.lines() {
            eprintln!("  add: {l}");
        }
    }
    eprint!("Apply this change? [y/N] ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    if std::io::stdin().lock().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

fn human(report: &Report) -> String {
    let mut out = String::new();
    if report.remote {
        out.push_str(
            "This looks like a remote session. The OSC 52 setting belongs to the terminal on your own computer: run `tmgr setup terminal` there. Only the tmux entry below applies to this host, if you run tmux here.\n\n",
        );
    }
    if report.apps.is_empty() {
        out.push_str("No supported terminal app was found on this machine. Run `tmgr setup terminal` on the computer where your terminal runs.");
        return out;
    }
    let mut restarts: Vec<&str> = Vec::new();
    for a in &report.apps {
        let head = match a.status {
            "already_ok" => format!("{}: already OK ({})", a.app, a.note),
            "skipped" => format!("{}: skipped ({})", a.app, a.note),
            "changed" => format!("{}: changed", a.app),
            "declined" => format!("{}: not changed (declined)", a.app),
            "failed" => format!("{}: FAILED ({})", a.app, a.note),
            _ => format!("{}: needs a change (not applied)", a.app),
        };
        out.push_str(&head);
        out.push('\n');
        if matches!(a.status, "changed" | "declined" | "would_change" | "failed") {
            if let Some(f) = &a.file {
                out.push_str(&format!("  file: {f}\n"));
            }
            for l in a.line.as_deref().unwrap_or_default().lines() {
                out.push_str(&format!("  {l}\n"));
            }
            if let Some(b) = &a.backup {
                out.push_str(&format!("  backup: {b}\n"));
            }
        }
        if a.status == "changed" {
            restarts.push(&a.note);
        }
    }
    if report.apps.iter().any(|a| a.status == "would_change") {
        out.push_str("\nNothing was changed. Run `tmgr setup terminal` in a terminal to confirm each change, or pass --yes.\n");
    }
    if !restarts.is_empty() {
        out.push('\n');
        for r in restarts {
            out.push_str(&format!("{r} for the change to take effect.\n"));
        }
    }
    out.trim_end().to_string()
}

pub fn run(ctx: &Ctx, args: &SetupTerminalArgs) -> Result<Output> {
    let env = Env::from_process()?;
    let mode = if args.check {
        Mode::Check
    } else if ctx.yes {
        Mode::Apply
    } else if interact::is_interactive() {
        Mode::Ask
    } else {
        Mode::Report
    };
    let report = run_with(&env, mode, &mut |app, edit| prompt(&env, app, edit))?;
    let text = human(&report);
    let failed = report.failed;
    let out = Output::new(report, text);
    Ok(if failed { out.exit(1) } else { out })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(tmp: &Path) -> Env {
        Env {
            home: tmp.to_path_buf(),
            xdg: tmp.join("xdg"),
            app_roots: vec![tmp.join("apps")],
            path_dirs: vec![],
            kitty_dir: None,
            iterm_domain: tmp.join("iterm/domain").display().to_string(),
        }
    }

    #[test]
    fn declining_leaves_the_file_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join(".warp/settings.toml");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, "[terminal]\n").unwrap();
        let mut asked = Vec::new();
        let r = run_with(&env(tmp.path()), Mode::Ask, &mut |app, edit| {
            asked.push((app.to_string(), edit.shown.clone()));
            false
        })
        .unwrap();
        assert_eq!(r.apps[0].status, "declined");
        assert_eq!(asked.len(), 1);
        assert!(asked[0]
            .1
            .contains("osc52_clipboard_access = \"write_only\""));
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "[terminal]\n");
    }

    #[test]
    fn accepting_applies_after_asking() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join(".tmux.conf");
        std::fs::write(&f, "").unwrap();
        let r = run_with(&env(tmp.path()), Mode::Ask, &mut |_, _| true).unwrap();
        assert_eq!(r.apps[0].status, "changed");
        assert_eq!(
            std::fs::read_to_string(&f).unwrap(),
            "set -s set-clipboard on\n"
        );
    }

    #[test]
    fn invalid_toml_is_skipped_not_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join(".warp/settings.toml");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, "[terminal\n").unwrap();
        let r = run_with(&env(tmp.path()), Mode::Apply, &mut |_, _| true).unwrap();
        assert_eq!(r.apps[0].status, "skipped");
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "[terminal\n");
    }

    #[test]
    fn a_file_changed_while_waiting_is_not_overwritten_and_other_apps_still_run() {
        let tmp = tempfile::tempdir().unwrap();
        let warp = tmp.path().join(".warp/settings.toml");
        std::fs::create_dir_all(warp.parent().unwrap()).unwrap();
        std::fs::write(&warp, "[terminal]\n").unwrap();
        let tmux = tmp.path().join(".tmux.conf");
        std::fs::write(&tmux, "").unwrap();
        let w = warp.clone();
        let r = run_with(&env(tmp.path()), Mode::Ask, &mut |app, _| {
            if app == "Warp" {
                std::fs::write(&w, "[terminal]\ncopy_on_select = false\n").unwrap();
            }
            true
        })
        .unwrap();
        assert!(r.failed);
        assert_eq!(r.apps[0].status, "failed");
        assert_eq!(r.apps[1].status, "changed");
        assert_eq!(
            std::fs::read_to_string(&warp).unwrap(),
            "[terminal]\ncopy_on_select = false\n"
        );
    }

    #[test]
    fn kitty_follows_its_own_search_order_and_never_edits_a_file_it_ignores() {
        let tmp = tempfile::tempdir().unwrap();
        let xdg = tmp.path().join("xdg/kitty/kitty.conf");
        let mac = tmp.path().join("Library/Preferences/kitty/kitty.conf");
        for f in [&xdg, &mac] {
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, "clipboard_control no-append\n").unwrap();
        }
        run_with(&env(tmp.path()), Mode::Apply, &mut |_, _| true).unwrap();
        assert!(std::fs::read_to_string(&xdg)
            .unwrap()
            .contains("write-clipboard"));
        assert_eq!(
            std::fs::read_to_string(&mac).unwrap(),
            "clipboard_control no-append\n"
        );
    }

    #[test]
    fn alacritty_only_paste_is_reported_not_rewritten() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("xdg/alacritty/alacritty.toml");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, "[terminal]\nosc52 = \"OnlyPaste\"\n").unwrap();
        let r = run_with(&env(tmp.path()), Mode::Apply, &mut |_, _| true).unwrap();
        assert_eq!(r.apps[0].status, "skipped");
        assert!(std::fs::read_to_string(&f).unwrap().contains("OnlyPaste"));
    }

    #[test]
    fn two_backups_in_the_same_second_never_overwrite_each_other() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("conf");
        std::fs::write(&f, "x").unwrap();
        let a = backup_name(&f);
        std::fs::write(&a, "first").unwrap();
        let b = backup_name(&f);
        assert_ne!(a, b);
    }
}
