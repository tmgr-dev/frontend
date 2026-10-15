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
    let out = output_within(cmd.args(args), GIT_TIMEOUT)?;
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

const GIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

fn output_within(cmd: &mut Command, limit: std::time::Duration) -> Result<std::process::Output> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::internal(format!("cannot run git: {e}")))?;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let err = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = std::time::Instant::now() + limit;
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::internal("git did not finish in time"));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    Ok(std::process::Output {
        status,
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
    })
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
    validate_branch(branch)?;
    let base = match base {
        Some(b) => {
            tmgr_core::branch::validate_base(b)?;
            let commit = format!("{b}^{{commit}}");
            git(
                Some(Path::new(repo)),
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    "--end-of-options",
                    &commit,
                ],
            )
            .map_err(|_| {
                Error::usage(format!(
                    "{b} is not a branch, tag or commit of the repository"
                ))
            })?
        }
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

fn read_small(path: &Path) -> Option<String> {
    use std::io::Read;
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.file_type().is_file() || meta.len() > 1 << 20 {
        return None;
    }
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(1 << 20)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

fn is_sha(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The commit `branch` points at in a clone, read from its ref files without running git there.
fn clone_tip(clone: &Path, branch: &str) -> Option<String> {
    let git = clone.join(".git");
    if !std::fs::symlink_metadata(&git).ok()?.is_dir() {
        return None;
    }
    let reference = format!("refs/heads/{branch}");
    if let Some(text) = read_small(&git.join(&reference)) {
        let sha = text.trim();
        return is_sha(sha).then(|| sha.to_string());
    }
    read_small(&git.join("packed-refs"))?
        .lines()
        .filter_map(|l| l.split_once(' '))
        .find(|(_, r)| *r == reference)
        .map(|(sha, _)| sha.to_string())
        .filter(|sha| is_sha(sha))
}

/// Brings `branch` of an agent's clone into `repo` without running git inside the clone: its
/// configuration and hooks were writable by the agent. A fresh bare repository of ours borrows the
/// clone's objects, and `repo` fetches from that. Returns the imported commit.
pub fn import_branch(repo: &str, clone: &Path, branch: &str) -> Result<String> {
    crate::workdir::validate_branch(branch)?;
    let sha = clone_tip(clone, branch)
        .ok_or_else(|| Error::usage(format!("the clone has no branch {branch}")))?;
    let tmp = tempfile::Builder::new()
        .prefix("tmgr-import")
        .tempdir()
        .map_err(|e| Error::internal(format!("temporary directory: {e}")))?;
    let bare = tmp.path().join("b.git");
    let b = bare.to_string_lossy().into_owned();
    git(None, &["init", "-q", "--bare", "--template=", &b])?;
    let objects = clone.join(".git/objects");
    std::fs::write(
        bare.join("objects/info/alternates"),
        format!(
            "{}\n{}\n",
            objects.display(),
            Path::new(repo).join(".git/objects").display()
        ),
    )?;
    git(
        Some(&bare),
        &["update-ref", &format!("refs/heads/{branch}"), &sha],
    )?;
    git(
        Some(Path::new(repo)),
        &[
            "fetch",
            "--no-tags",
            "--",
            &b,
            &format!("refs/heads/{branch}:refs/heads/{branch}"),
        ],
    )?;
    Ok(sha)
}

fn validate_branch(branch: &str) -> Result<()> {
    tmgr_core::branch::validate_branch_name(branch)
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
    fn a_run_clone_sits_beside_the_repo_on_its_branch_and_bases_are_checked() {
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
        prepare_clone(&r, &dest, "feat/tm-1-fix", Some("main")).unwrap();
        assert_eq!(
            git(Some(&dest), &["symbolic-ref", "--short", "HEAD"]).unwrap(),
            "feat/tm-1-fix"
        );
        assert!(prepare_clone(&r, &dest, "feat/tm-1-fix", None).is_err());
        for base in ["no-such-base", "--no-overlay", "a..b"] {
            let bad = dest.with_file_name("bad");
            assert!(prepare_clone(&r, &bad, "x", Some(base)).is_err(), "{base}");
            assert!(!bad.exists());
        }
        assert!(prepare_clone(&r, &dest.with_file_name("dash"), "-x", None).is_err());
    }

    #[test]
    fn importing_a_branch_never_runs_the_clones_own_configuration() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = root.join("proj");
        std::fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        let r = repo.to_string_lossy().into_owned();
        let dest = run_clone_dir(&r, "tm-1-fix").unwrap();
        prepare_clone(&r, &dest, "tm-1-fix", None).unwrap();
        std::fs::write(dest.join("work.txt"), "agent\n").unwrap();
        let commit = |args: &[&str]| {
            let out = Command::new("git")
                .current_dir(&dest)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_AUTHOR_NAME", "a")
                .env("GIT_AUTHOR_EMAIL", "a@example.com")
                .env("GIT_COMMITTER_NAME", "a")
                .env("GIT_COMMITTER_EMAIL", "a@example.com")
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success());
        };
        commit(&["add", "work.txt"]);
        commit(&["commit", "-q", "-m", "agent change"]);
        let marker = root.join("pwned");
        let mut cfg = std::fs::OpenOptions::new()
            .append(true)
            .open(dest.join(".git/config"))
            .unwrap();
        use std::io::Write;
        writeln!(
            cfg,
            "[core]\n\talternateRefsCommand = touch {m}\n\tfsmonitor = touch {m}\n[uploadpack]\n\tpackObjectsHook = touch {m}\n",
            m = marker.display()
        )
        .unwrap();
        let sha = import_branch(&r, &dest, "tm-1-fix").unwrap();
        assert_eq!(git(Some(&repo), &["rev-parse", "tm-1-fix"]).unwrap(), sha);
        assert_eq!(
            git(Some(&repo), &["log", "-1", "--format=%s", "tm-1-fix"]).unwrap(),
            "agent change"
        );
        assert!(!marker.exists());
        assert!(import_branch(&r, &dest, "no-such-branch").is_err());
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
