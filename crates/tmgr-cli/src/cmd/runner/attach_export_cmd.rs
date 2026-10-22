use super::attach_export::{
    decide_all, dir_is_safe, home_dir, load, new_job, page_title, state_for, write_private,
};
use super::attach_title::agent_src;
use super::{emit, Opts};
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use tmgr_core::error::{Error, Result};
use tmgr_ipc::api::StateKind;
use tmgr_link::privileged;
use tmgr_runner::attach::{fmt_size, Decision, ExportState};

pub(super) fn run(o: &Opts, a: &super::ExportArgs) -> Result<i32> {
    let in_agent = super::attach_new::in_agent();
    if a.page {
        privileged::check(
            in_agent,
            std::io::stdin().is_terminal() && std::io::stderr().is_terminal(),
        )?;
    } else if in_agent && a.md.is_some() {
        return Err(Error::usage("export is refused inside an agent session"));
    }
    let cfg = super::runner_config()?;
    let sessions = super::connect(&cfg)?.list().map_err(super::rerr)?;
    let info = super::ops::resolve(&sessions, &a.session)?;
    let src = agent_src(info)
        .filter(|s| s.format == tmgr_runner::attach::Format::Claude)
        .ok_or_else(|| Error::usage("export is available for Claude sessions"))?;
    let home = home_dir().ok_or_else(|| Error::usage("cannot find your home folder"))?;
    let home_s = home.to_string_lossy().to_string();
    let job = new_job(
        &info.id,
        src,
        info.child_pid,
        &cfg,
        info.state.kind == StateKind::Exited,
    );
    let loaded = load(&job, &info.name, &home_s)
        .ok_or_else(|| Error::not_found("the session transcript is not available"))?;
    let mut state = state_for(loaded, &info.name, None, None, &PathBuf::new(), &home_s);
    eprintln!(
        "{} lines, {}, {} findings in the secret scan",
        state.lines.len(),
        fmt_size(state.text().len()),
        state.findings.len()
    );
    if !a.page && a.md.is_none() {
        eprintln!("nothing was written: pass --md <file> or --page");
        return Ok(emit(
            o,
            serde_json::json!({"lines": state.lines.len(), "findings": state.findings.len()}),
            "",
        ));
    }
    let tty = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    review(&mut state, a.remove_findings, tty)?;
    let text = state.ready_text().ok_or_else(|| {
        Error::internal("the final scan found something undecided; nothing was written")
    })?;
    if let Some(path) = &a.md {
        let (path, overwrite) = target(path, &info.cwd, tty)?;
        write_private(&path, text.as_bytes(), overwrite)?;
        let human = format!("exported {} to {}", fmt_size(text.len()), path.display());
        return Ok(emit(
            o,
            serde_json::json!({"file": path.display().to_string(), "bytes": text.len()}),
            human,
        ));
    }
    let ctx = crate::ctx::Ctx::new(o.cli)?;
    let client = ctx.client()?;
    let ws = ctx.workspace(client)?;
    eprintln!(
        "This sends {} to the TMGR server, workspace {}.",
        fmt_size(text.len()),
        ws.code
    );
    privileged::confirm_typed(
        &ws.code,
        &format!("Type the workspace code ({}) to confirm:", ws.code),
        &mut std::io::stdin().lock(),
        &mut std::io::stderr(),
    )?;
    let title = page_title(
        &info.name,
        &chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
    );
    let page = client.page_create(ws.id, &serde_json::json!({"title": title, "body": text}))?;
    let human = format!("exported to a new page in {} (id {})", ws.code, page.id);
    Ok(emit(
        o,
        serde_json::json!({"page_id": page.id, "workspace": ws.code}),
        human,
    ))
}

fn target(path: &Path, session_cwd: &str, tty: bool) -> Result<(PathBuf, bool)> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| Error::usage("cannot read the current folder"))?
            .join(path)
    };
    let parent = abs
        .parent()
        .ok_or_else(|| Error::usage("invalid file path"))?
        .canonicalize()
        .map_err(|_| Error::usage("the folder does not exist"))?;
    if let Ok(cwd) = Path::new(session_cwd).canonicalize() {
        if parent.starts_with(&cwd) {
            return Err(
                Error::usage("refusing to write into the session's own folder")
                    .hint("pick a folder the agent cannot write to, such as your home folder"),
            );
        }
    }
    dir_is_safe(&parent)?;
    let file = abs
        .file_name()
        .ok_or_else(|| Error::usage("invalid file path"))?;
    let full = parent.join(file);
    if std::fs::symlink_metadata(&full).is_ok() {
        if !tty {
            return Err(
                Error::needs_confirmation(format!("{} already exists", full.display()))
                    .hint("nothing was overwritten; pick another file name"),
            );
        }
        privileged::confirm_typed(
            "overwrite",
            &format!("{} exists. Type overwrite to replace it:", full.display()),
            &mut std::io::stdin().lock(),
            &mut std::io::stderr(),
        )?;
        return Ok((full, true));
    }
    Ok((full, false))
}

fn review(state: &mut ExportState, remove_all: bool, tty: bool) -> Result<()> {
    if state.findings.is_empty() {
        return Ok(());
    }
    if remove_all {
        state.decisions = decide_all(&state.findings, true);
        return Ok(());
    }
    if !tty {
        return Err(
            Error::needs_confirmation("the secret scan found items to review")
                .hint("run it in a terminal, or pass --remove-findings"),
        );
    }
    let total = state.findings.len();
    let mut rest_removed = false;
    for i in 0..total {
        let f = state.findings[i].clone();
        if rest_removed || !f.keepable {
            state.decisions[i] = Decision::Remove;
            continue;
        }
        eprintln!(
            "\nfinding {}/{} on line {} [{}]",
            i + 1,
            total,
            f.line + 1,
            f.kind
        );
        eprintln!("  text:  {}", f.original);
        eprintln!("  as:    {}", f.cleaned);
        loop {
            eprint!("[r]emove, [k]eep, remove [a]ll the rest? ");
            let _ = std::io::stderr().flush();
            let mut line = String::new();
            if std::io::stdin().lock().read_line(&mut line).unwrap_or(0) == 0 {
                return Err(Error::new(
                    tmgr_core::error::Kind::Interrupted,
                    "cancelled, nothing was written",
                ));
            }
            match line.trim() {
                "r" => state.decisions[i] = Decision::Remove,
                "k" => state.decisions[i] = Decision::Keep,
                "a" => {
                    state.decisions[i] = Decision::Remove;
                    rest_removed = true;
                }
                _ => continue,
            }
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tmgr_runner::attach::scan_export;

    fn private_dir() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        d
    }

    fn state(lines: &[&str]) -> ExportState {
        let lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        let found = scan_export(&lines, "/home/me");
        ExportState::new(
            "t".into(),
            lines,
            found,
            None,
            None,
            String::new(),
            "/home/me".into(),
        )
    }

    #[test]
    fn findings_without_a_terminal_need_the_explicit_flag() {
        let mut s = state(&["see /home/me/a"]);
        assert!(review(&mut s, false, false).is_err());
        assert!(s.ready_text().is_none());
        review(&mut s, true, false).unwrap();
        assert_eq!(s.ready_text().unwrap(), "see ~/a\n");
    }

    #[test]
    fn a_clean_session_needs_no_review() {
        let mut s = state(&["nothing here"]);
        review(&mut s, false, false).unwrap();
        assert_eq!(s.ready_text().unwrap(), "nothing here\n");
    }

    #[test]
    fn the_target_refuses_the_session_folder_existing_files_without_a_terminal_and_open_folders() {
        let work = private_dir();
        let out = private_dir();
        let cwd = work.path().to_string_lossy().to_string();
        assert!(target(&work.path().join("x.md"), &cwd, true).is_err());
        let ok = out.path().join("x.md");
        assert!(!target(&ok, &cwd, false).unwrap().1);
        std::fs::write(&ok, "old").unwrap();
        assert!(target(&ok, &cwd, false).is_err());
        let open = tempfile::tempdir().unwrap();
        std::fs::set_permissions(open.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(target(&open.path().join("x.md"), &cwd, false).is_err());
    }
}
