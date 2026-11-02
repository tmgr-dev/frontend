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

pub(super) fn wanted(flag: bool, off: bool, trust: bool) -> bool {
    flag || (!off && !trust && config_on())
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
    req.cwd = sc.path.display().to_string();
    req.name = Some(sc.name.clone());
    req.clone_of = Some(sc.repo.display().to_string());
    req.trust_via_repo = trust_via_repo(caps).then_some(true);
    Ok(sc)
}

pub(super) fn started_note(sc: &sessionwt::SessionClone) -> String {
    format!(
        "worktree {} on branch {}; the branch stays in that folder until you bring it over: {}",
        clean(&sc.path.display().to_string()),
        clean(&sc.name),
        clean(&sessionwt::import_hint(&sc.repo, &sc.path, &sc.name))
    )
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
    let (branch, has_commits, ignored) = match sessionwt::assess(&repo, &path) {
        Ok(sessionwt::Verdict::Kept(why)) => return kept(&why),
        Ok(sessionwt::Verdict::Removable {
            branch,
            has_commits,
            ignored,
        }) => (branch, has_commits, ignored),
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
    let question = format!(
        "Delete the worktree {} ({keeps}{lost})?",
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
