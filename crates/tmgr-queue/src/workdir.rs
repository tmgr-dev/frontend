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

pub(crate) fn git_command(dir: Option<&Path>) -> Command {
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
    cmd
}

pub(crate) fn git_bytes(dir: &Path, args: &[&str], input: Option<Vec<u8>>) -> Result<Vec<u8>> {
    let mut cmd = git_command(Some(dir));
    cmd.args(["-c", "core.quotepath=off"]).args(args);
    let out = output_within_input(&mut cmd, GIT_TIMEOUT, input)?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr);
        let line = msg.lines().next().unwrap_or("git failed");
        return Err(Error::new(
            Kind::Internal,
            format!("git {}: {line}", args.first().unwrap_or(&"")),
        ));
    }
    Ok(out.stdout)
}

fn git(dir: Option<&Path>, args: &[&str]) -> Result<String> {
    let mut cmd = git_command(dir);
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

pub(crate) const GIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

pub(crate) fn output_within(
    cmd: &mut Command,
    limit: std::time::Duration,
) -> Result<std::process::Output> {
    output_within_input(cmd, limit, None)
}

pub(crate) fn output_within_input(
    cmd: &mut Command,
    limit: std::time::Duration,
    input: Option<Vec<u8>>,
) -> Result<std::process::Output> {
    use std::io::{Read, Write};
    use std::process::Stdio;
    let mut child = cmd
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::internal(format!("cannot run git: {e}")))?;
    if let (Some(data), Some(mut stdin)) = (input, child.stdin.take()) {
        std::thread::spawn(move || {
            let _ = stdin.write_all(&data);
        });
    }
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

    fn prepare_branch(
        &self,
        _repo: &str,
        _pickup_id: &str,
        _cwd: &str,
        _branch: &str,
    ) -> Result<()> {
        Err(Error::internal(
            "this work directory provider cannot pin a branch",
        ))
    }

    fn identity(&self, dir: &Path) -> Option<(u64, u64)> {
        clone_identity(dir)
    }

    fn discard(&self, _pickup_id: &str, _cwd: &str) {}

    fn prepare_multi(
        &self,
        _primary: &str,
        _pickup_id: &str,
        _cwd: &str,
        _clones: &[(String, String)],
        _branch: &str,
    ) -> Result<()> {
        Err(Error::internal(
            "this work directory provider cannot make several clones",
        ))
    }
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

/// A run folder holding one hardened clone per `(name, repository)`. The folder must not exist yet and is
/// removed again when any clone fails.
pub fn prepare_run_folder(run_dir: &Path, clones: &[(String, String)], branch: &str) -> Result<()> {
    if run_dir.exists() {
        return Err(Error::conflict("the run directory already exists"));
    }
    std::fs::create_dir_all(run_dir)?;
    let result = clones
        .iter()
        .try_for_each(|(name, repo)| prepare_clone(repo, &run_dir.join(name), branch, None));
    if result.is_err() {
        let _ = std::fs::remove_dir_all(run_dir);
    }
    result
}

/// `(device, inode)` of a clone folder, opened without following a symlink; `None` when it is not a real directory.
pub fn clone_identity(dir: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let name = dir.file_name()?.to_str()?;
    let meta = open_beneath(dir.parent()?, name, libc::O_DIRECTORY)?
        .metadata()
        .ok()?;
    Some((meta.dev(), meta.ino()))
}

pub(crate) fn open_beneath(
    root: &Path,
    rel: &str,
    last_flags: libc::c_int,
) -> Option<std::fs::File> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    let at = |dir: libc::c_int, name: &[u8], flags: libc::c_int| -> Option<std::fs::File> {
        let name = CString::new(name).ok()?;
        let fd = unsafe {
            libc::openat(
                dir,
                name.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_RDONLY,
            )
        };
        (fd >= 0).then(|| unsafe { std::fs::File::from_raw_fd(fd) })
    };
    let mut dir = at(
        libc::AT_FDCWD,
        root.as_os_str().as_bytes(),
        libc::O_DIRECTORY,
    )?;
    let parts: Vec<&str> = rel.split('/').collect();
    let (last, inner) = parts.split_last()?;
    for part in inner {
        if part.is_empty() || *part == "." || *part == ".." {
            return None;
        }
        dir = at(dir.as_raw_fd(), part.as_bytes(), libc::O_DIRECTORY)?;
    }
    if last.is_empty() || *last == "." || *last == ".." {
        return None;
    }
    at(dir.as_raw_fd(), last.as_bytes(), last_flags)
}

pub(crate) fn open_in(
    dir: &std::fs::File,
    name: &std::ffi::OsStr,
    flags: libc::c_int,
) -> Option<std::fs::File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::CString::new(name.as_bytes()).ok()?;
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_RDONLY,
        )
    };
    (fd >= 0).then(|| unsafe { std::fs::File::from_raw_fd(fd) })
}

pub(crate) fn entries(dir: &std::fs::File) -> Vec<std::ffi::OsString> {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    let mut names = Vec::new();
    let fd = unsafe { libc::dup(dir.as_raw_fd()) };
    if fd < 0 {
        return names;
    }
    let stream = unsafe { libc::fdopendir(fd) };
    if stream.is_null() {
        unsafe { libc::close(fd) };
        return names;
    }
    unsafe { libc::rewinddir(stream) };
    loop {
        let entry = unsafe { libc::readdir(stream) };
        if entry.is_null() {
            break;
        }
        let name = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) };
        let name = std::ffi::OsStr::from_bytes(name.to_bytes());
        if name != "." && name != ".." {
            names.push(name.to_os_string());
        }
    }
    unsafe { libc::closedir(stream) };
    names
}

const MAX_IMPORT: u64 = 2 << 30;

fn copy_regular(
    dir: &std::fs::File,
    name: &std::ffi::OsStr,
    to: &Path,
    budget: &mut u64,
) -> Result<()> {
    use std::io::Read;
    let Some(file) = open_in(dir, name, libc::O_NONBLOCK) else {
        return Ok(());
    };
    let meta = file.metadata()?;
    if !meta.file_type().is_file() {
        return Ok(());
    }
    *budget = budget.checked_sub(meta.len()).ok_or_else(|| {
        Error::usage("the clone's objects are larger than 2 GiB, too large to import")
    })?;
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to.join(name))?;
    std::io::copy(&mut file.take(meta.len()), &mut out)?;
    Ok(())
}

/// Copies the clone's loose objects and packs, regular files only, never following a symlink and
/// ignoring the clone's own alternates.
fn copy_objects(clone: &Path, into: &Path) -> Result<()> {
    let objects = open_beneath(clone, ".git/objects", libc::O_DIRECTORY)
        .ok_or_else(|| Error::usage("the clone has no object store of its own"))?;
    let mut budget = MAX_IMPORT;
    for name in entries(&objects) {
        let text = name.to_string_lossy();
        let fanout = text.len() == 2 && text.bytes().all(|b| b.is_ascii_hexdigit());
        if !fanout && text != "pack" {
            continue;
        }
        let Some(sub) = open_in(&objects, &name, libc::O_DIRECTORY) else {
            continue;
        };
        let to = into.join(&name);
        std::fs::create_dir_all(&to)?;
        for file in entries(&sub) {
            let f = file.to_string_lossy();
            let wanted = if fanout {
                f.bytes().all(|b| b.is_ascii_hexdigit())
            } else {
                f.starts_with("pack-")
                    && (f.ends_with(".pack") || f.ends_with(".idx") || f.ends_with(".rev"))
            };
            if wanted {
                copy_regular(&sub, &file, &to, &mut budget)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn read_small(root: &Path, rel: &str) -> Option<String> {
    use std::io::Read;
    let file = open_beneath(root, rel, libc::O_NONBLOCK)?;
    let meta = file.metadata().ok()?;
    if !meta.file_type().is_file() || meta.len() > 1 << 20 {
        return None;
    }
    let mut text = String::new();
    file.take(1 << 20).read_to_string(&mut text).ok()?;
    Some(text)
}

pub(crate) fn is_sha(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The commit `branch` points at in a clone, read from its ref files without running git there.
pub(crate) fn clone_tip(clone: &Path, branch: &str) -> Option<String> {
    let reference = format!("refs/heads/{branch}");
    if let Some(text) = read_small(clone, &format!(".git/{reference}")) {
        let sha = text.trim();
        return is_sha(sha).then(|| sha.to_string());
    }
    read_small(clone, ".git/packed-refs")?
        .lines()
        .filter_map(|l| l.split_once(' '))
        .find(|(_, r)| *r == reference)
        .map(|(sha, _)| sha.to_string())
        .filter(|sha| is_sha(sha))
}

/// Brings `branch` of an agent's clone into `repo` without running git inside the clone: its
/// configuration and hooks were writable by the agent. A fresh bare repository of ours gets a copy
/// of the clone's objects and borrows the rest from `repo`, which fetches from it. Returns the
/// imported commit.
pub fn git_query(repo: &Path, args: &[&str]) -> Result<String> {
    git(Some(repo), args)
}

/// The branch a clone's HEAD names, read from `.git/HEAD` as data without running git there.
pub fn clone_head_branch(clone: &Path) -> Option<String> {
    let head = read_small(clone, ".git/HEAD")?;
    head.trim()
        .strip_prefix("ref: refs/heads/")
        .map(str::to_string)
}

/// Whether the new commits `base..head` sit on `base`'s history (no root commit among them, a shared
/// merge base; `base` may have moved on since), and how many of them are merge commits.
pub fn history_facts(repo: &Path, base: &str, head: &str) -> Result<(bool, usize)> {
    let range = format!("{base}..{head}");
    let count = |extra: &str| -> Result<usize> {
        let out = git(Some(repo), &["rev-list", extra, "--count", &range])?;
        Ok(out.trim().parse().unwrap_or(usize::MAX))
    };
    let shared = git(Some(repo), &["merge-base", base, head]).is_ok();
    let roots = count("--max-parents=0")?;
    Ok((shared && roots == 0, count("--merges")?))
}

/// The commit `refs/heads/<branch>` points at in `repo`, or `None` when the branch does not exist.
pub fn branch_tip(repo: &Path, branch: &str) -> Result<Option<String>> {
    validate_branch(branch)?;
    let reference = format!("refs/heads/{branch}");
    Ok(git(
        Some(repo),
        &[
            "rev-parse",
            "--verify",
            "-q",
            &format!("{reference}^{{commit}}"),
        ],
    )
    .ok())
}

/// Puts `refs/heads/<branch>` back to what `branch_tip` saw before an import. It only acts while the
/// branch still points at the `imported` commit.
pub fn restore_branch(
    repo: &Path,
    branch: &str,
    before: Option<&str>,
    imported: &str,
) -> Result<()> {
    validate_branch(branch)?;
    let reference = format!("refs/heads/{branch}");
    match before {
        None => git(Some(repo), &["update-ref", "-d", &reference, imported])?,
        Some(old) => git(Some(repo), &["update-ref", &reference, old, imported])?,
    };
    Ok(())
}

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
    copy_objects(clone, &bare.join("objects"))?;
    std::fs::write(
        bare.join("objects/info/alternates"),
        format!("{}\n", Path::new(repo).join(".git/objects").display()),
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

pub fn discard_run_dir(pickup_id: &str, cwd: &str) {
    let dir = Path::new(cwd);
    let named = |p: Option<&Path>, want: &dyn Fn(&str) -> bool| {
        p.and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .is_some_and(want)
    };
    let ours = named(Some(dir), &|n| n == pickup_id)
        && named(dir.parent(), &|n| n == "queue")
        && named(dir.parent().and_then(Path::parent), &|n| {
            n.ends_with("-worktrees")
        });
    let real = std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir());
    if ours && real {
        let _ = std::fs::remove_dir_all(dir);
    }
}

impl WorkDirs for GitClone {
    fn discard(&self, pickup_id: &str, cwd: &str) {
        discard_run_dir(pickup_id, cwd);
    }

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

    fn prepare_branch(&self, repo: &str, pickup_id: &str, cwd: &str, branch: &str) -> Result<()> {
        let dest = clone_dir(repo, pickup_id)
            .ok_or_else(|| Error::internal("the repository path has no parent directory"))?;
        if dest != Path::new(cwd) {
            return Err(Error::internal(
                "the run directory does not belong to this pickup",
            ));
        }
        prepare_clone(repo, &dest, branch, None)
    }

    fn prepare_multi(
        &self,
        primary: &str,
        pickup_id: &str,
        cwd: &str,
        clones: &[(String, String)],
        branch: &str,
    ) -> Result<()> {
        let dest = clone_dir(primary, pickup_id)
            .ok_or_else(|| Error::internal("the repository path has no parent directory"))?;
        if dest != Path::new(cwd) {
            return Err(Error::internal(
                "the run directory does not belong to this pickup",
            ));
        }
        prepare_run_folder(&dest, clones, branch)
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
    fn importing_refuses_refs_or_objects_reached_through_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = root.join("proj");
        std::fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        let r = repo.to_string_lossy().into_owned();
        let dest = run_clone_dir(&r, "tm-2-fix").unwrap();
        prepare_clone(&r, &dest, "tm-2-fix", None).unwrap();
        let tip = git(Some(&repo), &["rev-parse", "HEAD"]).unwrap();
        let outside = root.join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("tm-2-fix"), format!("{tip}\n")).unwrap();
        let heads = dest.join(".git/refs/heads");
        std::fs::remove_dir_all(&heads).unwrap();
        std::os::unix::fs::symlink(&outside, &heads).unwrap();
        let _ = std::fs::remove_file(dest.join(".git/packed-refs"));
        assert!(import_branch(&r, &dest, "tm-2-fix").is_err());

        std::fs::remove_file(&heads).unwrap();
        std::fs::create_dir(&heads).unwrap();
        std::fs::write(heads.join("tm-2-fix"), format!("{tip}\n")).unwrap();
        let objects = dest.join(".git/objects");
        let alternates = objects.join("info/alternates");
        let original = std::fs::read_to_string(&alternates).unwrap_or_default();
        let foreign = root.join("foreign");
        std::fs::create_dir(&foreign).unwrap();
        init_repo(&foreign);
        let out = Command::new("git")
            .current_dir(&foreign)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "f")
            .env("GIT_AUTHOR_EMAIL", "f@example.com")
            .env("GIT_COMMITTER_NAME", "f")
            .env("GIT_COMMITTER_EMAIL", "f@example.com")
            .args(["commit", "-q", "--allow-empty", "-m", "secret"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let foreign_tip = git(Some(&foreign), &["rev-parse", "HEAD"]).unwrap();
        std::fs::write(
            &alternates,
            format!("{original}{}\n", foreign.join(".git/objects").display()),
        )
        .unwrap();
        std::fs::write(heads.join("tm-2-fix"), format!("{foreign_tip}\n")).unwrap();
        assert!(import_branch(&r, &dest, "tm-2-fix").is_err());
        std::fs::write(&alternates, original).unwrap();
        std::fs::write(heads.join("tm-2-fix"), format!("{tip}\n")).unwrap();
        assert!(import_branch(&r, &dest, "tm-2-fix").is_ok());
        let huge = std::fs::File::create(objects.join("pack/pack-big.pack")).unwrap();
        huge.set_len(MAX_IMPORT + 1).unwrap();
        let err = import_branch(&r, &dest, "tm-2-fix").unwrap_err();
        assert!(err.to_string().contains("too large"), "{err}");
        std::fs::remove_file(objects.join("pack/pack-big.pack")).unwrap();
        std::fs::rename(&objects, root.join("moved-objects")).unwrap();
        std::os::unix::fs::symlink(root.join("moved-objects"), &objects).unwrap();
        assert!(import_branch(&r, &dest, "tm-2-fix").is_err());
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

    #[test]
    fn a_run_folder_holds_every_clone_and_a_failure_removes_all_of_it() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let (a, b) = (root.join("a"), root.join("b"));
        for r in [&a, &b] {
            std::fs::create_dir(r).unwrap();
            init_repo(r);
        }
        let run = root.join("a-worktrees/queue/p1");
        let clones = vec![
            ("a".to_string(), a.to_string_lossy().into_owned()),
            ("b".to_string(), b.to_string_lossy().into_owned()),
        ];
        prepare_run_folder(&run, &clones, "tmgr/x").unwrap();
        assert!(run.join("a/.git").is_dir() && run.join("b/.git").is_dir());
        assert!(!run.join(".git").exists());
        assert!(prepare_run_folder(&run, &clones, "tmgr/x").is_err());
        assert!(run.join("a/.git").is_dir());
        let run2 = root.join("a-worktrees/queue/p2");
        let broken = vec![
            clones[0].clone(),
            (
                "c".to_string(),
                root.join("missing").to_string_lossy().into_owned(),
            ),
        ];
        assert!(prepare_run_folder(&run2, &broken, "tmgr/x").is_err());
        assert!(!run2.exists());
    }

    #[test]
    fn a_clone_identity_follows_the_directory_and_never_a_symlink() {
        use std::os::unix::fs::MetadataExt;
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let dir = root.join("run/backend");
        std::fs::create_dir_all(&dir).unwrap();
        let m = std::fs::metadata(&dir).unwrap();
        assert_eq!(clone_identity(&dir), Some((m.dev(), m.ino())));
        std::fs::rename(&dir, root.join("run/moved")).unwrap();
        let other = root.join("other");
        std::fs::create_dir(&other).unwrap();
        std::os::unix::fs::symlink(&other, &dir).unwrap();
        assert_eq!(clone_identity(&dir), None);
        std::fs::remove_file(&dir).unwrap();
        std::fs::create_dir(&dir).unwrap();
        assert_ne!(
            clone_identity(&dir),
            Some((m.dev(), m.ino())),
            "a replacement directory is another inode"
        );
        std::fs::write(root.join("run/file"), "x").unwrap();
        assert_eq!(clone_identity(&root.join("run/file")), None);
    }

    fn run_git(dir: &Path, args: &[&str]) {
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
    }

    fn commit_file(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), name).unwrap();
        run_git(dir, &["add", name]);
        run_git(dir, &["commit", "-q", "-m", name]);
    }

    #[test]
    fn history_facts_tell_a_straight_line_from_merges_and_unrelated_history() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = std::fs::canonicalize(tmp.path()).unwrap().join("proj");
        std::fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        let base = git(Some(&repo), &["rev-parse", "HEAD"]).unwrap();
        run_git(&repo, &["checkout", "-q", "-b", "line"]);
        commit_file(&repo, "a.txt");
        let line = git(Some(&repo), &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(history_facts(&repo, &base, &line).unwrap(), (true, 0));

        run_git(&repo, &["checkout", "-q", "-b", "side", &base]);
        commit_file(&repo, "b.txt");
        run_git(&repo, &["checkout", "-q", "line"]);
        run_git(&repo, &["merge", "-q", "--no-ff", "-m", "merge", "side"]);
        let merged = git(Some(&repo), &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(history_facts(&repo, &base, &merged).unwrap(), (true, 1));

        run_git(&repo, &["checkout", "-q", "--orphan", "other"]);
        commit_file(&repo, "c.txt");
        let other = git(Some(&repo), &["rev-parse", "HEAD"]).unwrap();
        assert!(!history_facts(&repo, &base, &other).unwrap().0);

        run_git(&repo, &["checkout", "-q", "-b", "moved", &base]);
        commit_file(&repo, "d.txt");
        let moved = git(Some(&repo), &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(history_facts(&repo, &moved, &line).unwrap(), (true, 0));
    }

    #[test]
    fn a_restored_branch_is_deleted_or_reset_and_only_while_it_is_the_imported_one() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = std::fs::canonicalize(tmp.path()).unwrap().join("proj");
        std::fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        let first = git(Some(&repo), &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(branch_tip(&repo, "tm-1-fix").unwrap(), None);
        run_git(&repo, &["branch", "tm-1-fix"]);
        assert_eq!(branch_tip(&repo, "tm-1-fix").unwrap(), Some(first.clone()));
        commit_file(&repo, "a.txt");
        let second = git(Some(&repo), &["rev-parse", "HEAD"]).unwrap();
        run_git(&repo, &["branch", "-f", "tm-1-fix", &second]);
        assert!(restore_branch(&repo, "tm-1-fix", Some(&first), &first).is_err());
        restore_branch(&repo, "tm-1-fix", Some(&first), &second).unwrap();
        assert_eq!(branch_tip(&repo, "tm-1-fix").unwrap(), Some(first));
        run_git(&repo, &["branch", "-f", "tm-1-fix", &second]);
        restore_branch(&repo, "tm-1-fix", None, &second).unwrap();
        assert_eq!(branch_tip(&repo, "tm-1-fix").unwrap(), None);
    }
}
