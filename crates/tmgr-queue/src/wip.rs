use crate::workdir::{
    clone_head_branch, clone_tip, git_bytes, git_command, is_sha, open_beneath, open_in,
    output_within_input, GIT_TIMEOUT,
};
use std::ffi::CString;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use tmgr_core::error::{Error, Result};

const MAX_FILE: u64 = 5 << 20;
const MAX_NOTE: usize = 200;
const AUTHOR_NAME: &str = "tmgr queue";
const AUTHOR_EMAIL: &str = "queue@tmgr.invalid";

enum Entry {
    File { mode: &'static str, data: Vec<u8> },
    Link(Vec<u8>),
    Skip(&'static str),
    Gone,
}

struct Scratch<'a> {
    clone: &'a Path,
    git_dir: std::path::PathBuf,
}

impl Scratch<'_> {
    fn run(&self, args: &[&str], input: Option<Vec<u8>>) -> Result<Vec<u8>> {
        let mut cmd = git_command(None);
        cmd.current_dir(self.clone)
            .env("GIT_DIR", &self.git_dir)
            .env("GIT_WORK_TREE", self.clone)
            .env("GIT_INDEX_FILE", self.git_dir.join("index"))
            .env("GIT_OBJECT_DIRECTORY", self.git_dir.join("objects"))
            .env(
                "GIT_ALTERNATE_OBJECT_DIRECTORIES",
                self.clone.join(".git/objects"),
            )
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_AUTHOR_NAME", AUTHOR_NAME)
            .env("GIT_AUTHOR_EMAIL", AUTHOR_EMAIL)
            .env("GIT_COMMITTER_NAME", AUTHOR_NAME)
            .env("GIT_COMMITTER_EMAIL", AUTHOR_EMAIL)
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.autocrlf=false",
                "-c",
                "core.attributesFile=/dev/null",
            ])
            .args(args);
        let out = output_within_input(&mut cmd, GIT_TIMEOUT, input)?;
        if !out.status.success() {
            let msg = String::from_utf8_lossy(&out.stderr);
            let line = msg.lines().next().unwrap_or("git failed");
            return Err(Error::internal(format!(
                "git {}: {line}",
                args.first().unwrap_or(&"")
            )));
        }
        Ok(out.stdout)
    }

    fn text(&self, args: &[&str]) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.run(args, None)?)
            .trim()
            .to_string())
    }
}

fn one_line(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_NOTE)
        .collect()
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\0')
        && path
            .split('/')
            .all(|c| !c.is_empty() && c != "." && c != ".." && !c.eq_ignore_ascii_case(".git"))
}

fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

fn inspect(clone: &Path, rel: &str) -> Entry {
    let (parent, name) = match rel.rsplit_once('/') {
        Some((p, n)) => (Some(p), n),
        None => (None, rel),
    };
    let dir = match parent {
        None => {
            open_beneath(clone, ".", libc::O_DIRECTORY).or_else(|| std::fs::File::open(clone).ok())
        }
        Some(p) => open_beneath(clone, p, libc::O_DIRECTORY),
    };
    let Some(dir) = dir else {
        return Entry::Gone;
    };
    let Ok(cname) = CString::new(name) else {
        return Entry::Skip("an unusable name");
    };
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        libc::fstatat(
            dir.as_raw_fd(),
            cname.as_ptr(),
            &mut st,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if rc != 0 {
        return if errno() == libc::ENOENT {
            Entry::Gone
        } else {
            Entry::Skip("unreadable")
        };
    }
    match st.st_mode & libc::S_IFMT {
        libc::S_IFLNK => {
            let mut buf = vec![0u8; 4096];
            let n = unsafe {
                libc::readlinkat(
                    dir.as_raw_fd(),
                    cname.as_ptr(),
                    buf.as_mut_ptr().cast(),
                    buf.len(),
                )
            };
            if n <= 0 || n as usize >= buf.len() {
                return Entry::Skip("an unreadable link");
            }
            buf.truncate(n as usize);
            Entry::Link(buf)
        }
        libc::S_IFREG => {
            if st.st_nlink > 1 {
                return Entry::Skip("hard-linked");
            }
            if st.st_size as u64 > MAX_FILE {
                return Entry::Skip("larger than 5 MB");
            }
            let Some(file) = open_in(
                &dir,
                std::ffi::OsStr::from_bytes(name.as_bytes()),
                libc::O_NONBLOCK,
            ) else {
                return Entry::Skip("unreadable");
            };
            let mut data = Vec::new();
            if file.take(MAX_FILE + 1).read_to_end(&mut data).is_err() {
                return Entry::Skip("unreadable");
            }
            if data.len() as u64 > MAX_FILE {
                return Entry::Skip("larger than 5 MB");
            }
            let mode = if st.st_mode & 0o100 != 0 {
                "100755"
            } else {
                "100644"
            };
            Entry::File { mode, data }
        }
        _ => Entry::Skip("not a regular file"),
    }
}

fn dir_in(dir: &std::fs::File, name: &str, create: bool) -> Result<std::fs::File> {
    let os = std::ffi::OsStr::new(name);
    if let Some(d) = open_in(dir, os, libc::O_DIRECTORY) {
        return Ok(d);
    }
    if create && errno() == libc::ENOENT {
        let c = CString::new(name).map_err(|_| Error::internal("bad name"))?;
        unsafe { libc::mkdirat(dir.as_raw_fd(), c.as_ptr(), 0o755) };
        if let Some(d) = open_in(dir, os, libc::O_DIRECTORY) {
            return Ok(d);
        }
    }
    Err(Error::internal(format!(
        "the clone's .git has an unexpected entry at {name}"
    )))
}

fn write_in_git(
    git: &std::fs::File,
    rel: &str,
    data: &[u8],
    replace: bool,
    mode: u32,
) -> Result<()> {
    let parts: Vec<&str> = rel.split('/').collect();
    let (last, inner) = parts
        .split_last()
        .ok_or_else(|| Error::internal("empty path"))?;
    let mut owned: Option<std::fs::File> = None;
    for part in inner {
        if part.is_empty() || *part == "." || *part == ".." {
            return Err(Error::internal("bad path"));
        }
        let next = dir_in(owned.as_ref().unwrap_or(git), part, true)?;
        owned = Some(next);
    }
    let dir = owned.as_ref().unwrap_or(git);
    if last.is_empty() || *last == "." || *last == ".." {
        return Err(Error::internal("bad path"));
    }
    let final_name = CString::new(*last).map_err(|_| Error::internal("bad name"))?;
    if !replace {
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        let rc = unsafe {
            libc::fstatat(
                dir.as_raw_fd(),
                final_name.as_ptr(),
                &mut st,
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if rc == 0 {
            return Ok(());
        }
    }
    let tmp = CString::new(format!(".{last}.tmgr-{}", std::process::id()))
        .map_err(|_| Error::internal("bad name"))?;
    unsafe { libc::unlinkat(dir.as_raw_fd(), tmp.as_ptr(), 0) };
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            tmp.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            mode as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(Error::internal(format!(
            "cannot write {rel} in the clone's .git"
        )));
    }
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let written = file.write_all(data).and_then(|_| file.flush());
    drop(file);
    let renamed = written.is_ok()
        && unsafe {
            libc::renameat(
                dir.as_raw_fd(),
                tmp.as_ptr(),
                dir.as_raw_fd(),
                final_name.as_ptr(),
            )
        } == 0;
    if !renamed {
        unsafe { libc::unlinkat(dir.as_raw_fd(), tmp.as_ptr(), 0) };
        return Err(Error::internal(format!(
            "cannot write {rel} in the clone's .git"
        )));
    }
    Ok(())
}

fn copy_new_objects(from: &Path, git: &std::fs::File) -> Result<()> {
    for fan in std::fs::read_dir(from.join("objects"))? {
        let fan = fan?;
        let name = fan.file_name().to_string_lossy().into_owned();
        if name.len() != 2 || !name.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        for obj in std::fs::read_dir(fan.path())? {
            let obj = obj?;
            let file = obj.file_name().to_string_lossy().into_owned();
            if !obj.file_type()?.is_file() || !file.bytes().all(|b| b.is_ascii_hexdigit()) {
                continue;
            }
            let data = std::fs::read(obj.path())?;
            write_in_git(git, &format!("objects/{name}/{file}"), &data, false, 0o444)?;
        }
    }
    Ok(())
}

fn changed_paths(raw: &[u8]) -> Vec<(char, String)> {
    let mut parts = raw.split(|b| *b == 0).filter(|p| !p.is_empty());
    let mut out = Vec::new();
    while let (Some(status), Some(path)) = (parts.next(), parts.next()) {
        if let (Some(s), Ok(p)) = (status.first(), std::str::from_utf8(path)) {
            out.push((*s as char, p.to_string()));
        }
    }
    out
}

/// Commits the tracked changes of an agent's clone onto `branch` and returns the new commit, or `None` when
/// nothing tracked changed. Git runs against a scratch repository of ours, so the clone's own configuration,
/// hooks, filters and signing settings never take effect; files are read without following symlinks, and
/// hard-linked files and files over 5 MB are left out.
pub fn wip_commit(clone: &Path, branch: &str, note: &str) -> Result<Option<String>> {
    tmgr_core::branch::validate_branch_name(branch)?;
    if clone.to_string_lossy().contains(':') {
        return Err(Error::usage("the clone path contains a colon"));
    }
    if clone_head_branch(clone).as_deref() != Some(branch) {
        return Err(Error::usage(format!(
            "the clone is not on the branch {branch}"
        )));
    }
    let git = open_beneath(clone, ".git", libc::O_DIRECTORY)
        .ok_or_else(|| Error::usage("the clone has no .git folder of its own"))?;
    let tip = clone_tip(clone, branch)
        .filter(|t| is_sha(t))
        .ok_or_else(|| Error::usage(format!("the clone has no branch {branch}")))?;
    if let Ok(lock) = CString::new("index.lock") {
        unsafe { libc::unlinkat(git.as_raw_fd(), lock.as_ptr(), 0) };
    }
    let tmp = tempfile::Builder::new()
        .prefix("tmgr-wip")
        .tempdir()
        .map_err(|e| Error::internal(format!("temporary directory: {e}")))?;
    git_bytes(tmp.path(), &["init", "-q", "--template=", "."], None)?;
    let scratch = Scratch {
        clone,
        git_dir: tmp.path().join(".git"),
    };
    scratch.run(&["read-tree", &tip], None)?;
    let diff = scratch.run(
        &[
            "diff-files",
            "-z",
            "--name-status",
            "--no-renames",
            "--ignore-submodules=all",
        ],
        None,
    )?;
    let zero = "0".repeat(tip.len());
    let mut index_info: Vec<u8> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for (status, path) in changed_paths(&diff) {
        if !safe_path(&path) {
            continue;
        }
        let removal = |buf: &mut Vec<u8>| {
            buf.extend_from_slice(format!("0 {zero}\t{path}\0").as_bytes());
        };
        match status {
            'D' => removal(&mut index_info),
            'M' | 'T' => match inspect(clone, &path) {
                Entry::Gone => removal(&mut index_info),
                Entry::Skip(why) => skipped.push(format!("{path} ({why})")),
                Entry::File { mode, data } => {
                    let sha = blob(&scratch, data)?;
                    index_info.extend_from_slice(format!("{mode} {sha}\t{path}\0").as_bytes());
                }
                Entry::Link(target) => {
                    let sha = blob(&scratch, target)?;
                    index_info.extend_from_slice(format!("120000 {sha}\t{path}\0").as_bytes());
                }
            },
            _ => {}
        }
    }
    if !index_info.is_empty() {
        scratch.run(&["update-index", "-z", "--index-info"], Some(index_info))?;
    }
    let tree = scratch.text(&["write-tree"])?;
    if tree == scratch.text(&["rev-parse", &format!("{tip}^{{tree}}")])? {
        return Ok(None);
    }
    let mut message = format!("WIP: paused ({})", one_line(note));
    if !skipped.is_empty() {
        message.push_str("\n\nLeft out:\n");
        for s in skipped.iter().take(50) {
            message.push_str(&format!("- {}\n", one_line(s)));
        }
    }
    let commit = scratch.text(&[
        "commit-tree",
        &tree,
        "-p",
        &tip,
        "--no-gpg-sign",
        "-m",
        &message,
    ])?;
    if !is_sha(&commit) {
        return Err(Error::internal("git did not return a commit id"));
    }
    copy_new_objects(&scratch.git_dir, &git)?;
    if clone_tip(clone, branch).as_deref() != Some(tip.as_str()) {
        return Err(Error::conflict(
            "the clone's branch moved while it was saved",
        ));
    }
    write_in_git(
        &git,
        &format!("refs/heads/{branch}"),
        format!("{commit}\n").as_bytes(),
        true,
        0o644,
    )?;
    if let Ok(index) = std::fs::read(scratch.git_dir.join("index")) {
        write_in_git(&git, "index", &index, true, 0o644)?;
    }
    Ok(Some(commit))
}

fn blob(scratch: &Scratch, data: Vec<u8>) -> Result<String> {
    scratch.text_in(
        &["hash-object", "-w", "--no-filters", "-t", "blob", "--stdin"],
        data,
    )
}

impl Scratch<'_> {
    fn text_in(&self, args: &[&str], input: Vec<u8>) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.run(args, Some(input))?)
            .trim()
            .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workdir::{init_repo, prepare_clone, run_clone_dir};
    use std::process::Command;

    struct Fx {
        _tmp: tempfile::TempDir,
        root: std::path::PathBuf,
        clone: std::path::PathBuf,
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
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
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn fx() -> Fx {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = root.join("proj");
        std::fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        std::fs::write(repo.join("keep.txt"), "keep\n").unwrap();
        std::fs::write(repo.join("gone.txt"), "gone\n").unwrap();
        std::fs::create_dir(repo.join("sub")).unwrap();
        std::fs::write(repo.join("sub/inner.txt"), "inner\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "more"]);
        let r = repo.to_string_lossy().into_owned();
        let clone = run_clone_dir(&r, "tm-1-fix").unwrap();
        prepare_clone(&r, &clone, "tm-1-fix", None).unwrap();
        Fx {
            _tmp: tmp,
            root,
            clone,
        }
    }

    fn files(dir: &Path, rev: &str) -> Vec<String> {
        git(dir, &["ls-tree", "-r", "--name-only", rev])
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn tracked_edits_deletes_and_mode_changes_are_committed_and_untracked_files_are_not() {
        let f = fx();
        let old = git(&f.clone, &["rev-parse", "HEAD"]);
        std::fs::write(f.clone.join("README.md"), "changed\n").unwrap();
        std::fs::remove_file(f.clone.join("gone.txt")).unwrap();
        std::fs::write(f.clone.join("sub/inner.txt"), "inner 2\n").unwrap();
        std::fs::write(f.clone.join("untracked.txt"), "new\n").unwrap();
        let sha = wip_commit(&f.clone, "tm-1-fix", "usage limit reached at 80%")
            .unwrap()
            .unwrap();
        assert_eq!(git(&f.clone, &["rev-parse", "HEAD"]), sha);
        assert_eq!(git(&f.clone, &["rev-parse", "HEAD^"]), old);
        assert_eq!(
            git(&f.clone, &["log", "-1", "--format=%s%n%an <%ae>"]),
            "WIP: paused (usage limit reached at 80%)\ntmgr queue <queue@tmgr.invalid>"
        );
        let names = files(&f.clone, "HEAD");
        assert!(!names.contains(&"gone.txt".to_string()));
        assert!(!names.contains(&"untracked.txt".to_string()));
        assert_eq!(git(&f.clone, &["show", "HEAD:README.md"]), "changed");
        assert_eq!(git(&f.clone, &["show", "HEAD:sub/inner.txt"]), "inner 2");
        assert_eq!(
            git(&f.clone, &["status", "--porcelain"]),
            "?? untracked.txt"
        );
        git(&f.clone, &["fsck", "--no-dangling"]);
    }

    #[test]
    fn nothing_changed_gives_none_and_a_wrong_branch_is_refused() {
        let f = fx();
        assert_eq!(wip_commit(&f.clone, "tm-1-fix", "x").unwrap(), None);
        assert!(wip_commit(&f.clone, "tm-2-other", "x").is_err());
    }

    #[test]
    fn an_executable_bit_and_a_symlink_are_kept() {
        use std::os::unix::fs::PermissionsExt;
        let f = fx();
        std::fs::set_permissions(
            f.clone.join("keep.txt"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        std::fs::remove_file(f.clone.join("gone.txt")).unwrap();
        std::os::unix::fs::symlink("keep.txt", f.clone.join("gone.txt")).unwrap();
        wip_commit(&f.clone, "tm-1-fix", "x").unwrap().unwrap();
        let tree = git(&f.clone, &["ls-tree", "HEAD", "keep.txt", "gone.txt"]);
        assert!(
            tree.contains("100755 blob") && tree.contains("120000 blob"),
            "{tree}"
        );
    }

    #[test]
    fn hard_linked_and_big_files_are_left_out_and_named_in_the_message() {
        let f = fx();
        let outside = f.root.join("secret.txt");
        std::fs::write(&outside, "secret\n").unwrap();
        std::fs::remove_file(f.clone.join("keep.txt")).unwrap();
        std::fs::hard_link(&outside, f.clone.join("keep.txt")).unwrap();
        std::fs::write(f.clone.join("gone.txt"), vec![b'x'; 6 << 20]).unwrap();
        std::fs::write(f.clone.join("README.md"), "changed\n").unwrap();
        wip_commit(&f.clone, "tm-1-fix", "x").unwrap().unwrap();
        assert_eq!(git(&f.clone, &["show", "HEAD:keep.txt"]), "keep");
        assert_eq!(git(&f.clone, &["show", "HEAD:gone.txt"]), "gone");
        assert_eq!(git(&f.clone, &["show", "HEAD:README.md"]), "changed");
        let msg = git(&f.clone, &["log", "-1", "--format=%B"]);
        assert!(
            msg.contains("keep.txt (hard-linked)") && msg.contains("gone.txt (larger than 5 MB)"),
            "{msg}"
        );
        assert!(!git(&f.clone, &["log", "-p", "-1", "--format="]).contains("secret"));
    }

    #[test]
    fn a_directory_swapped_for_a_symlink_never_reads_what_it_points_at() {
        let f = fx();
        let outside = f.root.join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("inner.txt"), "stolen\n").unwrap();
        std::fs::remove_dir_all(f.clone.join("sub")).unwrap();
        std::os::unix::fs::symlink(&outside, f.clone.join("sub")).unwrap();
        wip_commit(&f.clone, "tm-1-fix", "x").unwrap();
        let all = git(&f.clone, &["log", "-p", "-1", "--format="]);
        assert!(!all.contains("stolen"), "{all}");
    }

    #[test]
    fn a_stale_index_lock_is_removed() {
        let f = fx();
        std::fs::write(f.clone.join(".git/index.lock"), "").unwrap();
        std::fs::write(f.clone.join("README.md"), "changed\n").unwrap();
        wip_commit(&f.clone, "tm-1-fix", "x").unwrap().unwrap();
        assert!(!f.clone.join(".git/index.lock").exists());
    }

    #[test]
    fn the_clones_own_configuration_hooks_filters_and_signing_never_run() {
        let f = fx();
        let marker = f.root.join("pwned");
        let m = marker.display();
        let mut cfg = std::fs::OpenOptions::new()
            .append(true)
            .open(f.clone.join(".git/config"))
            .unwrap();
        writeln!(
            cfg,
            "[core]\n\tfsmonitor = touch {m}\n\thooksPath = {h}\n\tattributesFile = {a}\n\tsshCommand = touch {m}\n\
             [filter \"x\"]\n\tclean = touch {m}\n\tsmudge = touch {m}\n\tprocess = touch {m}\n\
             [commit]\n\tgpgsign = true\n[gpg]\n\tprogram = touch {m}\n\tformat = ssh\n[gpg \"ssh\"]\n\tprogram = touch {m}\n\
             [diff]\n\texternal = touch {m}\n[diff \"y\"]\n\ttextconv = touch {m}\n\
             [user]\n\tname = evil\n\temail = evil@example.com\n\
             [include]\n\tpath = /nonexistent\n[alias]\n\tcommit-tree = !touch {m}\n",
            h = f.clone.join(".git/hooks").display(),
            a = f.root.join("attrs").display(),
        )
        .unwrap();
        std::fs::write(f.root.join("attrs"), "* filter=x diff=y\n").unwrap();
        std::fs::write(f.clone.join(".gitattributes"), "* filter=x diff=y ident\n").unwrap();
        std::fs::create_dir_all(f.clone.join(".git/info")).unwrap();
        std::fs::write(f.clone.join(".git/info/attributes"), "* filter=x\n").unwrap();
        std::fs::create_dir_all(f.clone.join(".git/hooks")).unwrap();
        for hook in [
            "pre-commit",
            "commit-msg",
            "post-commit",
            "reference-transaction",
            "post-index-change",
            "pre-auto-gc",
        ] {
            let path = f.clone.join(".git/hooks").join(hook);
            std::fs::write(&path, format!("#!/bin/sh\ntouch {m}\n")).unwrap();
            std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
                .unwrap();
        }
        std::fs::write(f.clone.join("README.md"), "changed\n").unwrap();
        let sha = wip_commit(&f.clone, "tm-1-fix", "x").unwrap().unwrap();
        assert!(!marker.exists());
        let out = Command::new("git")
            .args(["--git-dir"])
            .arg(f.clone.join(".git"))
            .args(["cat-file", "commit", &sha])
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        let commit = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(
            commit.contains("tmgr queue <queue@tmgr.invalid>"),
            "{commit}"
        );
        assert!(
            !commit.contains("gpgsig") && !commit.contains("evil"),
            "{commit}"
        );
        assert!(!marker.exists());
    }
}
