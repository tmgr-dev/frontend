use crate::cli::{InitArgs, InitTarget};
use crate::ctx::Ctx;
use crate::interact;
use crate::out::Output;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use tmgr_core::config::write_atomic;
use tmgr_core::error::{Error, Result};

pub const BEGIN: &str = "# >>> tmgr init >>>";
pub const END: &str = "# <<< tmgr init <<<";
const MAX_RC: u64 = 4 * 1024 * 1024;

pub fn snippet_body(target: InitTarget) -> &'static str {
    match target {
        InitTarget::Zsh => {
            "if command -v tmgr >/dev/null 2>&1; then\n  setopt prompt_subst\n  PROMPT='$(tmgr prompt --format zsh)'\"$PROMPT\"\nfi\n"
        }
        InitTarget::Bash => {
            "if command -v tmgr >/dev/null 2>&1; then\n  __TMGR_PS1_BASE=\"$PS1\"\n  __tmgr_ps1() { PS1=\"$(tmgr prompt --format bash)${__TMGR_PS1_BASE}\"; }\n  PROMPT_COMMAND=\"__tmgr_ps1${PROMPT_COMMAND:+;$PROMPT_COMMAND}\"\nfi\n"
        }
        InitTarget::Fish => {
            "if type -q tmgr; and functions -q fish_prompt; and not functions -q __tmgr_orig_prompt\n    functions -c fish_prompt __tmgr_orig_prompt\n    function fish_prompt\n        tmgr prompt --format fish\n        __tmgr_orig_prompt\n    end\nend\n"
        }
        InitTarget::Starship => {
            "[custom.tmgr]\ncommand = \"tmgr prompt --format starship\"\nwhen = \"test -f \\\"${XDG_STATE_HOME:-$HOME/.local/state}/tmgr/active.json\\\"\"\nformat = \"[$output]($style) \"\nstyle = \"bold red\"\n"
        }
        InitTarget::Tmux => {
            "set -ag status-right ' #(tmgr prompt --format tmux) '\nset -g status-interval 15\n"
        }
        InitTarget::Claude => {
            "{\n  \"statusLine\": {\n    \"type\": \"command\",\n    \"command\": \"tmgr statusline claude\"\n  }\n}\n"
        }
    }
}

pub fn block(target: InitTarget) -> String {
    format!("{BEGIN}\n{}{END}\n", snippet_body(target))
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

fn home() -> Result<PathBuf> {
    env_path("HOME").ok_or_else(|| Error::internal("HOME is not set"))
}

pub fn rc_path(target: InitTarget) -> Result<PathBuf> {
    let home = home()?;
    let config = env_path("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
    Ok(match target {
        InitTarget::Zsh => env_path("ZDOTDIR").unwrap_or(home).join(".zshrc"),
        InitTarget::Bash => home.join(".bashrc"),
        InitTarget::Fish => config.join("fish").join("config.fish"),
        InitTarget::Starship => {
            env_path("STARSHIP_CONFIG").unwrap_or_else(|| config.join("starship.toml"))
        }
        InitTarget::Tmux => {
            let xdg = config.join("tmux").join("tmux.conf");
            let classic = home.join(".tmux.conf");
            if !classic.exists() && xdg.exists() {
                xdg
            } else {
                classic
            }
        }
        InitTarget::Claude => {
            return Err(Error::usage("tmgr does not edit Claude Code settings")
                .hint("add the printed snippet to settings.json yourself"))
        }
    })
}

pub fn with_block(existing: &str, new_block: &str) -> Result<String> {
    match find_block(existing)? {
        Some((start, end)) => Ok(format!(
            "{}{}{}",
            &existing[..start],
            new_block,
            &existing[end..]
        )),
        None => {
            let mut out = existing.to_string();
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(new_block);
            Ok(out)
        }
    }
}

pub fn without_block(existing: &str) -> Result<String> {
    match find_block(existing)? {
        Some((start, end)) => {
            let before = existing[..start].trim_end_matches('\n');
            let after = existing[end..].trim_start_matches('\n');
            Ok(match (before.is_empty(), after.is_empty()) {
                (true, true) => String::new(),
                (true, false) => after.to_string(),
                (false, true) => format!("{before}\n"),
                (false, false) => format!("{before}\n\n{after}"),
            })
        }
        None => Ok(existing.to_string()),
    }
}

fn find_block(text: &str) -> Result<Option<(usize, usize)>> {
    let Some(start) = text.find(BEGIN) else {
        return Ok(None);
    };
    let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let end_rel = text[start..].find(END).ok_or_else(|| {
        Error::conflict("the tmgr block in the file has no closing marker")
            .hint("fix or remove it by hand")
    })?;
    let after_end = start + end_rel + END.len();
    let end = text[after_end..]
        .find('\n')
        .map_or(text.len(), |i| after_end + i + 1);
    Ok(Some((line_start, end)))
}

pub fn diff(old: &str, new: &str) -> String {
    let o: Vec<&str> = old.lines().collect();
    let n: Vec<&str> = new.lines().collect();
    let mut p = 0;
    while p < o.len() && p < n.len() && o[p] == n[p] {
        p += 1;
    }
    let mut s = 0;
    while s < o.len() - p && s < n.len() - p && o[o.len() - 1 - s] == n[n.len() - 1 - s] {
        s += 1;
    }
    let mut out = String::new();
    for l in &o[p.saturating_sub(2)..p] {
        out.push_str(&format!("  {l}\n"));
    }
    for l in &o[p..o.len() - s] {
        out.push_str(&format!("- {l}\n"));
    }
    for l in &n[p..n.len() - s] {
        out.push_str(&format!("+ {l}\n"));
    }
    for l in &o[o.len() - s..(o.len() - s + 2).min(o.len())] {
        out.push_str(&format!("  {l}\n"));
    }
    out
}

pub struct Change {
    pub path: PathBuf,
    pub old: String,
    pub new: String,
}

pub fn plan(path: &Path, target: InitTarget, remove: bool) -> Result<Change> {
    let real = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let old = match fs::metadata(&real) {
        Ok(m) if m.len() > MAX_RC => return Err(Error::usage("the file is too large to edit")),
        Ok(_) => fs::read_to_string(&real)
            .map_err(|_| Error::usage("the file is not readable UTF-8 text"))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return Err(Error::internal("cannot read the file")),
    };
    let new = if remove {
        without_block(&old)?
    } else {
        with_block(&old, &block(target))?
    };
    Ok(Change {
        path: real,
        old,
        new,
    })
}

pub fn apply(change: &Change, stamp: &str) -> Result<Option<PathBuf>> {
    let mut backup = None;
    let mut mode = 0o644;
    if change.path.exists() {
        let b = PathBuf::from(format!("{}.tmgr-backup-{stamp}", change.path.display()));
        fs::copy(&change.path, &b)?;
        backup = Some(b);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            mode = fs::metadata(&change.path)?.permissions().mode() & 0o777;
        }
    } else if let Some(parent) = change.path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_atomic(&change.path, change.new.as_bytes(), mode)?;
    Ok(backup)
}

pub fn run(ctx: &Ctx, args: &InitArgs) -> Result<Output> {
    if !args.install && !args.uninstall {
        let text = block(args.target);
        return Ok(Output::new(
            json!({"snippet": text}),
            text.trim_end().to_string(),
        ));
    }
    let path = rc_path(args.target)?;
    let change = plan(&path, args.target, args.uninstall)?;
    if change.old == change.new {
        let what = if args.uninstall {
            "nothing to remove"
        } else {
            "already installed"
        };
        return Ok(Output::new(
            json!({"file": change.path.display().to_string(), "changed": false, "backup": null}),
            ctx.style
                .muted(&format!("{}: {what}", change.path.display())),
        ));
    }
    if !ctx.json {
        eprintln!("{}", change.path.display());
        eprint!("{}", diff(&change.old, &change.new));
    }
    interact::confirm(
        &format!("Write these changes to {}?", change.path.display()),
        false,
        false,
    )?;
    let stamp = chrono::Local::now().format("%Y%m%d%H%M%S").to_string();
    let backup = apply(&change, &stamp)?;
    let mut human = format!(
        "{} {} {}",
        ctx.style.good("\u{2713}"),
        if args.uninstall {
            "removed from"
        } else {
            "added to"
        },
        change.path.display()
    );
    if let Some(b) = &backup {
        human.push_str(&format!("\n  backup: {}", b.display()));
    }
    Ok(Output::new(
        json!({"file": change.path.display().to_string(), "changed": true, "backup": backup.map(|b| b.display().to_string())}),
        human,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [InitTarget; 6] = [
        InitTarget::Zsh,
        InitTarget::Bash,
        InitTarget::Fish,
        InitTarget::Starship,
        InitTarget::Tmux,
        InitTarget::Claude,
    ];

    #[test]
    fn install_is_idempotent_and_replaces_in_place() {
        let b = block(InitTarget::Zsh);
        let once = with_block("export A=1\n", &b).unwrap();
        assert!(once.starts_with("export A=1\n\n# >>> tmgr init >>>\n"));
        assert_eq!(with_block(&once, &b).unwrap(), once);
        let edited = once.replace("prompt_subst", "prompt_subst # edited");
        assert_eq!(with_block(&edited, &b).unwrap(), once);
        assert_eq!(once.matches(BEGIN).count(), 1);
    }

    #[test]
    fn install_then_uninstall_restores_the_original_text() {
        for original in ["", "export A=1\n", "export A=1", "a\n\nb\n"] {
            let b = block(InitTarget::Bash);
            let installed = with_block(original, &b).unwrap();
            let back = without_block(&installed).unwrap();
            assert_eq!(back.trim_end(), original.trim_end(), "{original:?}");
        }
    }

    #[test]
    fn content_around_the_block_is_kept() {
        let text = format!("before\n\n{BEGIN}\nold\n{END}\n\nafter\n");
        assert_eq!(without_block(&text).unwrap(), "before\n\nafter\n");
        let replaced = with_block(&text, &block(InitTarget::Tmux)).unwrap();
        assert!(replaced.starts_with("before\n\n# >>>"));
        assert!(replaced.ends_with("# <<< tmgr init <<<\n\nafter\n"));
    }

    #[test]
    fn a_block_without_an_end_marker_is_refused() {
        let text = format!("x\n{BEGIN}\nno end\n");
        assert_eq!(with_block(&text, "b").unwrap_err().exit_code(), 6);
        assert_eq!(without_block(&text).unwrap_err().exit_code(), 6);
    }

    #[test]
    fn diff_shows_only_the_changed_lines() {
        let d = diff("a\nb\nc\n", "a\nb\nX\nc\n");
        assert_eq!(d, "  a\n  b\n+ X\n  c\n");
        let d = diff("a\n", "a\n\n# >>>\nx\n");
        assert!(d.contains("+ # >>>"));
        assert!(!d.contains("- "));
    }

    #[test]
    fn apply_writes_a_backup_and_keeps_the_mode() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let rc = tmp.path().join(".zshrc");
        fs::write(&rc, "export A=1\n").unwrap();
        fs::set_permissions(&rc, fs::Permissions::from_mode(0o600)).unwrap();
        let change = plan(&rc, InitTarget::Zsh, false).unwrap();
        let backup = apply(&change, "20260101000000").unwrap().unwrap();
        assert_eq!(fs::read_to_string(&backup).unwrap(), "export A=1\n");
        assert!(backup
            .to_string_lossy()
            .ends_with(".zshrc.tmgr-backup-20260101000000"));
        let now = fs::read_to_string(&rc).unwrap();
        assert!(now.contains(BEGIN) && now.starts_with("export A=1\n"));
        assert_eq!(
            fs::metadata(&rc).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let again = plan(&rc, InitTarget::Zsh, false).unwrap();
        assert_eq!(again.old, again.new);
        let removal = plan(&rc, InitTarget::Zsh, true).unwrap();
        apply(&removal, "20260101000001").unwrap();
        assert_eq!(fs::read_to_string(&rc).unwrap(), "export A=1\n");
    }

    #[test]
    fn symlinked_rc_files_are_edited_in_place_not_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("dotfiles-zshrc");
        let link = tmp.path().join(".zshrc");
        fs::write(&real, "x\n").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let change = plan(&link, InitTarget::Zsh, false).unwrap();
        apply(&change, "1").unwrap();
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(fs::read_to_string(&real).unwrap().contains(BEGIN));
    }

    #[test]
    fn missing_files_are_created_with_their_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let rc = tmp.path().join("fish").join("config.fish");
        let change = plan(&rc, InitTarget::Fish, false).unwrap();
        assert_eq!(apply(&change, "1").unwrap(), None);
        assert!(fs::read_to_string(&rc).unwrap().contains("fish_prompt"));
    }

    #[test]
    fn every_snippet_names_the_right_command() {
        for t in ALL {
            assert!(snippet_body(t).contains("tmgr "), "{}", snippet_body(t));
        }
        assert!(snippet_body(InitTarget::Tmux).contains("--format tmux"));
        assert!(snippet_body(InitTarget::Claude).contains("tmgr statusline claude"));
        assert!(rc_path(InitTarget::Claude).is_err());
    }
}
