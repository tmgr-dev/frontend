use std::path::{Path, PathBuf};
use std::process::Command;
use tmgr_core::error::{Error, Kind, Result};

pub fn clone_dir(repo: &str, pickup_id: &str) -> Option<PathBuf> {
    let repo = Path::new(repo);
    let name = repo.file_name()?.to_str()?;
    Some(
        repo.parent()?
            .join(format!("{name}-worktrees"))
            .join("queue")
            .join(pickup_id),
    )
}

pub fn branch_name(pickup_id: &str) -> String {
    format!("queue/{pickup_id}")
}

fn git(dir: Option<&Path>, args: &[&str]) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.env_clear()
        .env(
            "PATH",
            std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()),
        )
        .env("HOME", "/nonexistent")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ALLOW_PROTOCOL", "file")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "protocol.file.allow=user",
            "-c",
            "credential.helper=",
            "-c",
            "core.askPass=",
            "-c",
            "diff.external=",
            "-c",
            "core.pager=cat",
        ]);
    if let Some(d) = dir {
        cmd.arg("-C").arg(d);
    }
    let out = cmd
        .args(args)
        .output()
        .map_err(|e| Error::internal(format!("cannot run git: {e}")))?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr);
        let line = msg.lines().next().unwrap_or("git failed");
        return Err(Error::new(
            Kind::Internal,
            format!("git {}: {line}", args.first().unwrap_or(&"")),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub trait WorkDirs {
    fn prepare(&self, repo: &str, pickup_id: &str, cwd: &str) -> Result<()>;
}

/// Where `tmgr run` keeps the clone for a branch: `<repo>-worktrees/<branch with / as ->`.
pub fn run_clone_dir(repo: &str, branch: &str) -> Option<PathBuf> {
    let repo = Path::new(repo);
    let name = repo.file_name()?.to_str()?;
    Some(
        repo.parent()?
            .join(format!("{name}-worktrees"))
            .join(branch.replace('/', "-")),
    )
}

/// A hardened `git clone --reference` of `repo` at `dest` on a new `branch` (from `base`, else the repo's
/// current branch); hooks, fsmonitor and credential helpers are off. Nothing is left behind on failure.
pub fn prepare_clone(repo: &str, dest: &Path, branch: &str, base: Option<&str>) -> Result<()> {
    if dest.exists() {
        return Err(Error::conflict("the run directory already exists"));
    }
    let base = match base {
        Some(b) => b.to_string(),
        None => git(
            Some(Path::new(repo)),
            &["symbolic-ref", "--quiet", "--short", "HEAD"],
        )
        .or_else(|_| git(Some(Path::new(repo)), &["rev-parse", "HEAD"]))?,
    };
    let parent = dest
        .parent()
        .ok_or_else(|| Error::internal("the run directory has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let result = (|| -> Result<()> {
        let d = dest.to_string_lossy();
        git(
            None,
            &[
                "clone",
                "--no-checkout",
                "--template=",
                "--reference",
                repo,
                "--",
                repo,
                &d,
            ],
        )?;
        git(Some(dest), &["config", "core.hooksPath", "/dev/null"])?;
        git(Some(dest), &["config", "core.fsmonitor", "false"])?;
        git(
            Some(dest),
            &["checkout", "-b", branch, &format!("origin/{base}")],
        )
        .or_else(|_| git(Some(dest), &["checkout", "-b", branch, &base]))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(dest);
    }
    result
}

/// Whether `dest` is a clone made by [`prepare_clone`] from `repo` that is on `branch`.
pub fn is_clone_of(repo: &str, dest: &Path, branch: &str) -> bool {
    if !dest.join(".git").is_dir() {
        return false;
    }
    let alternates =
        std::fs::read_to_string(dest.join(".git/objects/info/alternates")).unwrap_or_default();
    let objects = Path::new(repo).join(".git/objects");
    let same_objects = alternates.lines().count() == 1
        && alternates
            .lines()
            .next()
            .is_some_and(|l| Path::new(l.trim()) == objects);
    same_objects
        && git(Some(dest), &["symbolic-ref", "--quiet", "--short", "HEAD"])
            .is_ok_and(|head| head == branch)
}

pub struct GitClone;

impl WorkDirs for GitClone {
    fn prepare(&self, repo: &str, pickup_id: &str, cwd: &str) -> Result<()> {
        let dest = clone_dir(repo, pickup_id)
            .ok_or_else(|| Error::internal("the repository path has no parent directory"))?;
        if dest != Path::new(cwd) {
            return Err(Error::internal(
                "the run directory does not belong to this pickup",
            ));
        }
        prepare_clone(repo, &dest, &branch_name(pickup_id), None)
    }
}

#[cfg(test)]
pub fn init_repo(dir: &Path) {
    let run = |args: &[&str]| {
        let out = Command::new("git")
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    run(&["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("README.md"), "hello\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_beside_the_repo_on_a_new_branch_without_hooks() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = std::fs::canonicalize(tmp.path()).unwrap().join("proj");
        std::fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        let hooks = repo.join(".git/hooks");
        std::fs::write(
            hooks.join("post-checkout"),
            "#!/bin/sh\ntouch \"$PWD/hook-ran\"\n",
        )
        .unwrap();
        std::fs::set_permissions(
            hooks.join("post-checkout"),
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .unwrap();
        let r = repo.to_string_lossy().into_owned();
        let dest = clone_dir(&r, "q_abc").unwrap();
        assert_eq!(
            dest,
            tmp.path()
                .canonicalize()
                .unwrap()
                .join("proj-worktrees/queue/q_abc")
        );
        GitClone
            .prepare(&r, "q_abc", &dest.to_string_lossy())
            .unwrap();
        assert!(dest.join("README.md").is_file());
        assert!(!dest.join("hook-ran").exists());
        assert!(!dest.join(".git/hooks/post-checkout").exists());
        let branch = git(Some(&dest), &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap();
        assert_eq!(branch, "queue/q_abc");
        assert!(dest.join(".git/objects/info/alternates").is_file());
        assert_eq!(
            git(Some(&dest), &["config", "core.hooksPath"]).unwrap(),
            "/dev/null"
        );
        assert!(GitClone
            .prepare(&r, "q_abc", &dest.to_string_lossy())
            .is_err());
        assert!(GitClone
            .prepare(&r, "q_abd", &dest.to_string_lossy())
            .is_err());
    }

    #[test]
    fn a_run_clone_sits_beside_the_repo_on_its_branch_and_is_recognised_again() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = std::fs::canonicalize(tmp.path()).unwrap().join("proj");
        std::fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        let r = repo.to_string_lossy().into_owned();
        let dest = run_clone_dir(&r, "feat/tm-1-fix").unwrap();
        assert_eq!(
            dest,
            repo.parent().unwrap().join("proj-worktrees/feat-tm-1-fix")
        );
        assert!(!is_clone_of(&r, &dest, "feat/tm-1-fix"));
        prepare_clone(&r, &dest, "feat/tm-1-fix", None).unwrap();
        assert!(is_clone_of(&r, &dest, "feat/tm-1-fix"));
        assert!(!is_clone_of(&r, &dest, "other"));
        assert!(prepare_clone(&r, &dest, "feat/tm-1-fix", None).is_err());
        let other = tmp.path().join("other");
        std::fs::create_dir(&other).unwrap();
        init_repo(&other);
        assert!(!is_clone_of(
            &other.to_string_lossy(),
            &dest,
            "feat/tm-1-fix"
        ));
        assert!(prepare_clone(&r, &dest.with_file_name("bad"), "x", Some("no-such-base")).is_err());
        assert!(!dest.with_file_name("bad").exists());
    }

    #[test]
    fn a_failed_clone_leaves_no_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = std::fs::canonicalize(tmp.path()).unwrap().join("notgit");
        std::fs::create_dir(&repo).unwrap();
        let r = repo.to_string_lossy().into_owned();
        let dest = clone_dir(&r, "q_x").unwrap();
        assert!(GitClone
            .prepare(&r, "q_x", &dest.to_string_lossy())
            .is_err());
        assert!(!dest.exists());
    }
}
