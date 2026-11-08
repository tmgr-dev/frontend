use std::path::{Path, PathBuf};
use tmgr_core::config::{config_dir, Config};
use tmgr_ipc::api::RunCreateReq;
use tmgr_runner::sanitize::sanitize_text;
use tmgr_runner::sessionwt;

fn clean(s: &str) -> String {
    sanitize_text(s).into_owned()
}

pub(super) fn config_on() -> bool {
    config_dir()
        .ok()
        .and_then(|d| Config::load(&d).ok())
        .is_some_and(|c| c.attach_worktree())
}

fn decide(
    flag: bool,
    off: bool,
    trust: bool,
    no_sandbox: bool,
    cfg_on: bool,
    in_repo: bool,
) -> bool {
    flag || (!off && !trust && !no_sandbox && cfg_on && in_repo)
}

pub(super) fn wanted(flag: bool, off: bool, trust: bool, no_sandbox: bool, cwd: &Path) -> bool {
    decide(
        flag,
        off,
        trust,
        no_sandbox,
        config_on(),
        sessionwt::in_repo(cwd),
    )
}

fn trust_via_repo(caps: &[String]) -> bool {
    caps.iter()
        .any(|c| c == tmgr_ipc::api::caps::TRUST_VIA_REPO_V1)
        && !tmgr_link::privileged::RUN_ENV
            .iter()
            .any(|k| std::env::var_os(k).is_some())
}

pub(super) fn start(
    req: &mut RunCreateReq,
    taken: &[String],
    caps: &[String],
) -> std::result::Result<sessionwt::SessionClone, String> {
    let sc = sessionwt::create(
        Path::new(&req.cwd),
        req.name.as_deref(),
        &req.profile,
        taken,
    )?;
    super::run_task::mark_task_clone(&sc.path, &sc.repo.display().to_string());
    req.cwd = sc.path.display().to_string();
    req.name = Some(sc.name.clone());
    req.clone_of = Some(sc.repo.display().to_string());
    req.trust_via_repo = trust_via_repo(caps).then_some(true);
    Ok(sc)
}

pub(super) fn started_note(sc: &sessionwt::SessionClone, from: &Path) -> String {
    let below = std::fs::canonicalize(from)
        .ok()
        .and_then(|c| c.strip_prefix(&sc.repo).ok().map(Path::to_path_buf))
        .filter(|r| !r.as_os_str().is_empty());
    let root = below.map_or_else(String::new, |r| {
        format!(
            " (it starts at the root of the clone, not in {})",
            clean(&r.display().to_string())
        )
    });
    format!(
        "worktree {} on branch {}{root}; the branch stays in that folder until you bring it over: {}",
        clean(&sc.path.display().to_string()),
        clean(&sc.name),
        clean(&sessionwt::import_hint(&sc.path))
    )
}

#[derive(Debug)]
pub(super) struct Imported {
    pub repo: PathBuf,
    pub branch: String,
    pub sha: String,
}

pub(super) fn import_session(target: &str, cwd: &Path) -> tmgr_core::error::Result<Imported> {
    use tmgr_core::error::Error;
    let (repo, path, branch) = sessionwt::find(target, cwd).map_err(Error::usage)?;
    let repo_str = repo
        .to_str()
        .ok_or_else(|| Error::usage("the repository path is not valid UTF-8"))?;
    let sha = tmgr_queue::workdir::import_branch(repo_str, &path, &branch)?;
    Ok(Imported { repo, branch, sha })
}

pub(super) fn import(o: &super::Opts, target: &str) -> tmgr_core::error::Result<i32> {
    let in_agent = tmgr_link::privileged::RUN_ENV
        .iter()
        .any(|k| std::env::var_os(k).is_some());
    tmgr_link::privileged::check(in_agent, true)?;
    let cwd = std::env::current_dir().map_err(|e| tmgr_core::error::Error::usage(e.to_string()))?;
    let done = import_session(target, &cwd)?;
    let human = format!(
        "branch {} is now in {} at {}",
        clean(&done.branch),
        clean(&done.repo.display().to_string()),
        clean(done.sha.get(..12).unwrap_or(&done.sha))
    );
    Ok(super::emit(
        o,
        serde_json::json!({
            "repo": done.repo,
            "branch": done.branch,
            "commit": done.sha,
        }),
        human,
    ))
}

fn probe_use(path: &Path) -> Option<String> {
    match super::gc::Probe::take() {
        Ok(p) => p.holds(path, None),
        Err(e) => Some(format!("cannot tell whether it is in use: {}", e.message)),
    }
}

fn in_use(
    path: &Path,
    others: &[String],
    probe: &dyn Fn(&Path) -> Option<String>,
) -> Option<String> {
    let shown = path.display().to_string();
    if others.iter().any(|c| c == &shown) {
        return Some("another session uses this folder".to_string());
    }
    probe(path)
}

pub(super) enum Removal {
    Note(String),
    Ask(Ask),
}

#[derive(Clone)]
pub(super) struct Ask {
    pub repo: PathBuf,
    pub path: PathBuf,
    pub name: String,
    pub question: String,
}

pub(super) fn plan_removal(cwd: &str, others: &[String]) -> Option<Removal> {
    plan_removal_with(cwd, others, &probe_use)
}

fn plan_removal_with(
    cwd: &str,
    others: &[String],
    probe: &dyn Fn(&Path) -> Option<String>,
) -> Option<Removal> {
    let path = PathBuf::from(cwd);
    let repo = sessionwt::repo_of(&path)?;
    let name = path
        .file_name()
        .map(|n| clean(&n.to_string_lossy()))
        .unwrap_or_default();
    let kept = |why: &str| {
        Some(Removal::Note(format!(
            "worktree {name} kept: {}",
            clean(why)
        )))
    };
    if let Some(why) = in_use(&path, others, probe) {
        return kept(&why);
    }
    let (branch, has_commits, ignored, unreferenced) = match sessionwt::assess(&repo, &path) {
        Ok(sessionwt::Verdict::Kept(why)) => return kept(&why),
        Ok(sessionwt::Verdict::Removable {
            branch,
            has_commits,
            ignored,
            unreferenced,
        }) => (branch, has_commits, ignored, unreferenced),
        Err(e) => return kept(&e),
    };
    let keeps = if has_commits {
        format!("branch {} is kept in the repository", clean(&branch))
    } else {
        "no new commits, so no branch to keep".to_string()
    };
    let lost = if ignored.is_empty() {
        String::new()
    } else {
        let shown: Vec<String> = ignored.iter().take(3).map(|p| clean(p)).collect();
        let more = ignored.len().saturating_sub(3);
        let tail = if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        };
        format!(
            "; ignored files are deleted too: {}{tail}",
            shown.join(", ")
        )
    };
    let dropped = if unreferenced > 0 {
        format!("; {unreferenced} commit(s) that only the reflog remembers are lost")
    } else {
        String::new()
    };
    let question = format!(
        "Delete the worktree {} ({keeps}{lost}{dropped})?",
        clean(&path.display().to_string())
    );
    Some(Removal::Ask(Ask {
        repo,
        path,
        name,
        question,
    }))
}

pub(super) fn delete(ask: &Ask) -> String {
    delete_with(ask, &probe_use)
}

fn delete_with(ask: &Ask, probe: &dyn Fn(&Path) -> Option<String>) -> String {
    if let Some(why) = probe(&ask.path) {
        return format!("worktree {} kept: {}", ask.name, clean(&why));
    }
    match sessionwt::remove(&ask.repo, &ask.path) {
        Ok(sessionwt::Verdict::Removable { .. }) => format!("worktree {} deleted", ask.name),
        Ok(sessionwt::Verdict::Kept(why)) => {
            format!("worktree {} kept: {}", ask.name, clean(&why))
        }
        Err(e) => format!("worktree {} was not deleted: {}", ask.name, clean(&e)),
    }
}

pub(super) fn offer_removal(cwd: &str, others: &[String], tty: bool) -> Option<String> {
    match plan_removal(cwd, others)? {
        Removal::Note(n) => Some(n),
        Removal::Ask(a) if !tty => Some(format!(
            "worktree {} is clean and can be deleted; run `tmgr rm {}` in a terminal to be asked",
            a.name,
            shell_words::quote(&a.path.display().to_string())
        )),
        Removal::Ask(a) => {
            if crate::interact::confirm(&a.question, false, false).is_err() {
                return Some(format!("worktree {} kept", a.name));
            }
            Some(delete(&a))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tmgr_runner::worktree::GitEnv;

    fn git(dir: &Path, args: &[&str]) -> String {
        GitEnv::bare(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .run(args)
            .unwrap()
    }

    fn setup() -> (tempfile::TempDir, sessionwt::SessionClone) {
        let tmp = tempfile::tempdir().unwrap();
        let repo = fs::canonicalize(tmp.path()).unwrap().join("proj");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        fs::write(repo.join("a"), "a").unwrap();
        git(&repo, &["add", "a"]);
        git(&repo, &["commit", "-m", "a"]);
        let sc = sessionwt::create(&repo, Some("s1"), "claude", &[]).unwrap();
        (tmp, sc)
    }

    #[test]
    fn sandboxed_sessions_in_a_repository_use_a_clone_unless_opted_out() {
        let on = true;
        assert!(decide(false, false, false, false, on, true));
        assert!(!decide(false, true, false, false, on, true));
        assert!(!decide(false, false, true, false, on, true));
        assert!(!decide(false, false, false, true, on, true));
        assert!(!decide(false, false, false, false, false, true));
        assert!(!decide(false, false, false, false, on, false));
        assert!(decide(true, false, false, false, false, false));
    }

    #[test]
    fn the_note_says_when_a_subfolder_start_lands_at_the_clone_root() {
        let (_t, sc) = setup();
        fs::create_dir(sc.repo.join("sub")).unwrap();
        assert!(!started_note(&sc, &sc.repo).contains("starts at the root"));
        let note = started_note(&sc, &sc.repo.join("sub"));
        assert!(note.contains("not in sub"), "{note}");
        assert!(note.contains("tmgr runner import"), "{note}");
    }

    #[test]
    fn the_default_config_turns_the_clone_on() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(Config::load(tmp.path()).unwrap().attach_worktree());
    }

    #[test]
    fn a_folder_outside_any_repository_has_nothing_to_clone() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!sessionwt::in_repo(tmp.path()));
        let (_t, sc) = setup();
        assert!(sessionwt::in_repo(&sc.repo));
        assert!(sessionwt::in_repo(&sc.path));
    }

    fn commit_in(dir: &Path, name: &str) -> String {
        fs::write(dir.join(name), name).unwrap();
        git(dir, &["add", "--", name]);
        git(dir, &["commit", "-m", name]);
        git(dir, &["rev-parse", "HEAD"])
    }

    fn arm_hostile_clone(clone: &Path, marker: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let touch = format!("touch {}", marker.display());
        let script = format!("#!/bin/sh\n{touch}\n");
        let hooks = clone.join("evil-hooks");
        fs::create_dir_all(&hooks).unwrap();
        for dir in [hooks.clone(), clone.join(".git/hooks")] {
            fs::create_dir_all(&dir).unwrap();
            for h in [
                "pre-commit",
                "post-commit",
                "post-checkout",
                "post-merge",
                "pre-push",
                "pre-auto-gc",
                "reference-transaction",
                "fsmonitor-watchman",
                "pre-rebase",
                "update",
                "post-update",
                "pre-receive",
                "post-receive",
            ] {
                let f = dir.join(h);
                fs::write(&f, &script).unwrap();
                fs::set_permissions(&f, fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        let extra = clone.join("extra.cfg");
        fs::write(&extra, format!("[core]\n\tfsmonitor = {touch}\n")).unwrap();
        fs::write(clone.join(".gitattributes"), "* filter=x\n").unwrap();
        let mut cfg = fs::read_to_string(clone.join(".git/config")).unwrap();
        cfg.push_str(&format!(
            "[core]\n\tfsmonitor = {touch}\n\thooksPath = {hp}\n\tsshCommand = {touch}\n\tpager = {touch}\n\teditor = {touch}\n\tgitProxy = {touch}\n[filter \"x\"]\n\tclean = {touch}\n\tsmudge = {touch}\n\tprocess = {touch}\n\trequired = true\n[include]\n\tpath = {extra}\n[uploadpack]\n\tpackObjectsHook = {touch}\n[alias]\n\tlog = !{touch}\n\tstatus = !{touch}\n\tfetch = !{touch}\n[gc]\n\tauto = 1\n[credential]\n\thelper = {touch}\n[diff]\n\texternal = {touch}\n[remote \"origin\"]\n\tuploadpack = {touch}\n",
            hp = hooks.display(),
            extra = extra.display()
        ));
        fs::write(clone.join(".git/config"), cfg).unwrap();
    }

    #[test]
    fn importing_a_hostile_clone_runs_nothing_and_lands_the_branch() {
        let (t, sc) = setup();
        let tip = commit_in(&sc.path, "n.txt");
        let marker = fs::canonicalize(t.path()).unwrap().join("ran");
        arm_hostile_clone(&sc.path, &marker);
        let _ = std::process::Command::new("git")
            .args(["-C", &sc.path.display().to_string(), "status"])
            .output();
        assert!(marker.exists(), "the clone is not hostile enough");
        fs::remove_file(&marker).unwrap();
        let source_config = fs::read(sc.repo.join(".git/config")).unwrap();
        let done = import_session(&sc.path.display().to_string(), t.path()).unwrap();
        assert_eq!(
            (done.branch.as_str(), done.sha.as_str()),
            ("s1", tip.as_str())
        );
        assert_eq!(done.repo, sc.repo);
        assert_eq!(git(&sc.repo, &["rev-parse", "refs/heads/s1"]), tip);
        assert_eq!(
            fs::read(sc.repo.join(".git/config")).unwrap(),
            source_config
        );
        let _ = sessionwt::assess(&sc.repo, &sc.path);
        let _ = sessionwt::remove(&sc.repo, &sc.path);
        assert!(!marker.exists(), "a program of the clone ran");
        assert_eq!(git(&sc.repo, &["rev-parse", "refs/heads/s1"]), tip);
    }

    #[test]
    fn a_session_writing_its_own_git_folder_leaves_the_repository_untouched() {
        let (_t, sc) = setup();
        let snapshot = |dir: &Path| -> Vec<(String, Vec<u8>)> {
            let mut all = vec![("config".to_string(), fs::read(dir.join("config")).unwrap())];
            let mut hooks: Vec<_> = fs::read_dir(dir.join("hooks")).unwrap().flatten().collect();
            hooks.sort_by_key(|e| e.file_name());
            for e in hooks {
                all.push((
                    e.file_name().to_string_lossy().into_owned(),
                    fs::read(e.path()).unwrap(),
                ));
            }
            all
        };
        let before = snapshot(&sc.repo.join(".git"));
        let mut cfg = fs::read_to_string(sc.path.join(".git/config")).unwrap();
        cfg.push_str("[core]\n\thooksPath = /tmp\n");
        fs::write(sc.path.join(".git/config"), cfg).unwrap();
        fs::write(sc.path.join(".git/hooks/pre-commit"), "#!/bin/sh\n").unwrap();
        fs::write(sc.path.join(".git/hooks/post-merge"), "#!/bin/sh\n").unwrap();
        assert_eq!(snapshot(&sc.repo.join(".git")), before);
    }

    #[test]
    fn import_resolves_a_session_name_from_inside_the_repository() {
        let (_t, sc) = setup();
        let tip = commit_in(&sc.path, "n.txt");
        let done = import_session("s1", &sc.repo).unwrap();
        assert_eq!(done.sha, tip);
        let again = import_session("s1", &sc.repo.join(".")).unwrap();
        assert_eq!(again.sha, tip);
        assert!(import_session("nope", &sc.repo).is_err());
        assert!(import_session("../proj", &sc.repo).is_err());
    }

    #[test]
    fn import_refuses_a_checked_out_or_diverged_branch_and_foreign_folders() {
        let (t, sc) = setup();
        commit_in(&sc.path, "n.txt");
        git(&sc.repo, &["checkout", "-q", "-b", "s1"]);
        let err = import_session("s1", &sc.repo).unwrap_err();
        assert!(err.message.contains("checked out"), "{}", err.message);
        git(&sc.repo, &["checkout", "-q", "main"]);
        git(&sc.repo, &["branch", "-q", "-D", "s1"]);
        git(&sc.repo, &["checkout", "-q", "-b", "s1"]);
        commit_in(&sc.repo, "other.txt");
        git(&sc.repo, &["checkout", "-q", "main"]);
        let before = git(&sc.repo, &["rev-parse", "refs/heads/s1"]);
        let err = import_session("s1", &sc.repo).unwrap_err();
        assert!(err.message.contains("not an ancestor"), "{}", err.message);
        assert_eq!(git(&sc.repo, &["rev-parse", "refs/heads/s1"]), before);
        let plain = t.path().join("plain");
        fs::create_dir(&plain).unwrap();
        assert!(import_session(&plain.display().to_string(), t.path()).is_err());
        assert!(import_session(&sc.repo.display().to_string(), t.path()).is_err());
    }

    fn free(_: &Path) -> Option<String> {
        None
    }

    fn cwd(sc: &sessionwt::SessionClone) -> String {
        sc.path.display().to_string()
    }

    #[test]
    fn a_clean_unused_checkout_is_offered_and_deleted_only_on_the_call() {
        let (_t, sc) = setup();
        let Some(Removal::Ask(ask)) = plan_removal_with(&cwd(&sc), &[], &free) else {
            panic!("expected an offer");
        };
        assert!(ask.question.contains("no new commits"), "{}", ask.question);
        assert!(sc.path.exists());
        let busy = |_: &Path| Some("a process is working in it".to_string());
        assert!(delete_with(&ask, &busy).contains("kept"));
        assert!(sc.path.exists());
        assert_eq!(delete_with(&ask, &free), "worktree s1 deleted");
        assert!(!sc.path.exists());
    }

    #[test]
    fn a_dirty_or_busy_checkout_is_never_offered() {
        let (_t, sc) = setup();
        let busy = |_: &Path| Some("a process is working in it".to_string());
        let Some(Removal::Note(n)) = plan_removal_with(&cwd(&sc), &[], &busy) else {
            panic!("expected a note");
        };
        assert!(n.contains("kept") && n.contains("process"), "{n}");
        let Some(Removal::Note(n)) = plan_removal_with(&cwd(&sc), &[cwd(&sc)], &free) else {
            panic!("expected a note");
        };
        assert!(n.contains("another session"), "{n}");
        fs::write(sc.path.join("x"), "x").unwrap();
        let Some(Removal::Note(n)) = plan_removal_with(&cwd(&sc), &[], &free) else {
            panic!("expected a note");
        };
        assert!(n.contains("uncommitted"), "{n}");
        assert!(sc.path.join("x").exists());
    }

    #[test]
    fn other_folders_are_not_ours() {
        let (t, _sc) = setup();
        assert!(plan_removal_with(&t.path().display().to_string(), &[], &free).is_none());
    }
}
