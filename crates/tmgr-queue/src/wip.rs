use crate::workdir::{
    clone_head_branch, clone_tip, git_bytes, git_command, is_sha, open_beneath, open_in,
    output_within_input, read_small, GIT_TIMEOUT,
};
use std::ffi::CString;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::Path;
use tmgr_core::error::{Error, Result};

const MAX_FILE: u64 = 5 << 20;
const MAX_UNCHANGED_CHECK: u64 = 256 << 20;
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

fn fstat(file: &std::fs::File) -> Option<libc::stat> {
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    (unsafe { libc::fstat(file.as_raw_fd(), &mut st) } == 0).then_some(st)
}

/// Opens a regular file and proves the opened descriptor is the file that was examined: same type, device,
/// inode, link count and size.
fn open_checked(dir: &std::fs::File, name: &str, seen: &libc::stat) -> Option<std::fs::File> {
    let file = open_in(dir, std::ffi::OsStr::new(name), libc::O_NONBLOCK)?;
    let now = fstat(&file)?;
    let same = now.st_mode & libc::S_IFMT == libc::S_IFREG
        && now.st_dev == seen.st_dev
        && now.st_ino == seen.st_ino
        && now.st_nlink == seen.st_nlink
        && now.st_size == seen.st_size;
    same.then_some(file)
}

struct TipEntry {
    mode: String,
    sha: String,
    size: u64,
    path: String,
}

fn tip_entries(raw: &[u8]) -> Vec<TipEntry> {
    raw.split(|b| *b == 0)
        .filter(|r| !r.is_empty())
        .filter_map(|r| {
            let text = std::str::from_utf8(r).ok()?;
            let (meta, path) = text.split_once('\t')?;
            let mut f = meta.split_whitespace();
            let (mode, kind, sha, size) = (f.next()?, f.next()?, f.next()?, f.next()?);
            (kind == "blob" && matches!(mode, "100644" | "100755" | "120000")).then(|| TipEntry {
                mode: mode.to_string(),
                sha: sha.to_string(),
                size: size.parse().unwrap_or(u64::MAX),
                path: path.to_string(),
            })
        })
        .collect()
}

fn blob_id(data: &[u8], width: usize) -> String {
    let header = format!("blob {}\0", data.len());
    hash_blob(width, header.as_bytes(), &mut std::io::Cursor::new(data)).unwrap_or_default()
}

fn hash_blob(width: usize, header: &[u8], body: &mut dyn Read) -> std::io::Result<String> {
    use sha2::Digest;
    fn run<D: Digest>(header: &[u8], body: &mut dyn Read) -> std::io::Result<String> {
        let mut d = D::new();
        d.update(header);
        let mut buf = vec![0u8; 64 << 10];
        loop {
            let n = body.read(&mut buf)?;
            if n == 0 {
                break;
            }
            d.update(&buf[..n]);
        }
        Ok(d.finalize().iter().map(|b| format!("{b:02x}")).collect())
    }
    if width == 64 {
        run::<sha2::Sha256>(header, body)
    } else {
        run::<sha1::Sha1>(header, body)
    }
}

/// Whether a file that is too big or hard-linked to be saved is nevertheless byte for byte what the branch has.
fn unchanged_in_tip(clone: &Path, tip: &TipEntry) -> bool {
    if tip.size > MAX_UNCHANGED_CHECK || tip.mode == "120000" {
        return false;
    }
    let (parent, name) = match tip.path.rsplit_once('/') {
        Some((p, n)) => (Some(p), n),
        None => (None, tip.path.as_str()),
    };
    let dir = match parent {
        None => {
            open_beneath(clone, ".", libc::O_DIRECTORY).or_else(|| std::fs::File::open(clone).ok())
        }
        Some(p) => open_beneath(clone, p, libc::O_DIRECTORY),
    };
    let Some(dir) = dir else { return false };
    let Ok(cname) = CString::new(name) else {
        return false;
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
    if rc != 0 || st.st_mode & libc::S_IFMT != libc::S_IFREG || st.st_size as u64 != tip.size {
        return false;
    }
    let exec = if st.st_mode & 0o100 != 0 {
        "100755"
    } else {
        "100644"
    };
    let Some(file) = open_checked(&dir, name, &st) else {
        return false;
    };
    let header = format!("blob {}\0", tip.size);
    let mut body = file.take(tip.size);
    exec == tip.mode
        && hash_blob(tip.sha.len(), header.as_bytes(), &mut body).is_ok_and(|h| h == tip.sha)
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
            let Some(file) = open_checked(&dir, name, &st) else {
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

fn alternates_trusted(clone: &Path, source_repos: &[String]) -> bool {
    let rel = ".git/objects/info/alternates";
    if std::fs::symlink_metadata(clone.join(rel)).is_err() {
        return true;
    }
    let Some(text) = read_small(clone, rel) else {
        return false;
    };
    let allowed: Vec<std::path::PathBuf> = source_repos
        .iter()
        .filter_map(|r| std::fs::canonicalize(Path::new(r).join(".git/objects")).ok())
        .collect();
    text.lines().filter(|l| !l.trim().is_empty()).all(|l| {
        Path::new(l.trim()).is_absolute()
            && std::fs::canonicalize(l.trim()).is_ok_and(|c| allowed.contains(&c))
    })
}

/// Commits the tracked changes of an agent's clone onto `branch` and returns the new commit, or `None` when
/// nothing tracked changed. Git runs against a scratch repository of ours, so the clone's own configuration,
/// hooks, filters and signing settings never take effect; files are read without following symlinks, and
/// hard-linked files and files over 5 MB are left out.
pub fn wip_commit(
    clone: &Path,
    branch: &str,
    note: &str,
    source_repos: &[String],
) -> Result<Option<String>> {
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
    if open_beneath(clone, ".git/objects", libc::O_DIRECTORY).is_none() {
        return Err(Error::usage(
            "the clone's object store is not a plain folder",
        ));
    }
    if !alternates_trusted(clone, source_repos) {
        return Err(Error::usage("the clone borrows objects from elsewhere"));
    }
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
    let listing = scratch.run(&["ls-tree", "-r", "-l", "-z", &tip], None)?;
    let zero = "0".repeat(tip.len());
    let mut index_info: Vec<u8> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for entry in tip_entries(&listing) {
        let path = &entry.path;
        if !safe_path(path) {
            continue;
        }
        let removal = |buf: &mut Vec<u8>| {
            buf.extend_from_slice(format!("0 {zero}\t{path}\0").as_bytes());
        };
        let put = |mode: &str, data: Vec<u8>, buf: &mut Vec<u8>| -> Result<()> {
            if blob_id(&data, tip.len()) == entry.sha && mode == entry.mode {
                return Ok(());
            }
            let sha = blob(&scratch, data)?;
            buf.extend_from_slice(format!("{mode} {sha}\t{path}\0").as_bytes());
            Ok(())
        };
        match inspect(clone, path) {
            Entry::Gone => removal(&mut index_info),
            Entry::Skip(why) => {
                let heavy = matches!(why, "hard-linked" | "larger than 5 MB");
                if !(heavy && unchanged_in_tip(clone, &entry)) {
                    skipped.push(format!("{path} ({why})"));
                }
            }
            Entry::File { mode, data } => put(mode, data, &mut index_info)?,
            Entry::Link(target) => put("120000", target, &mut index_info)?,
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

    impl Fx {
        fn sources(&self) -> Vec<String> {
            vec![self.root.join("proj").to_string_lossy().into_owned()]
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
        let sha = wip_commit(
            &f.clone,
            "tm-1-fix",
            "usage limit reached at 80%",
            &f.sources(),
        )
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
        assert_eq!(
            wip_commit(&f.clone, "tm-1-fix", "x", &f.sources()).unwrap(),
            None
        );
        assert!(wip_commit(&f.clone, "tm-2-other", "x", &f.sources()).is_err());
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
        wip_commit(&f.clone, "tm-1-fix", "x", &f.sources())
            .unwrap()
            .unwrap();
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
        wip_commit(&f.clone, "tm-1-fix", "x", &f.sources())
            .unwrap()
            .unwrap();
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
        wip_commit(&f.clone, "tm-1-fix", "x", &f.sources()).unwrap();
        let all = git(&f.clone, &["log", "-p", "-1", "--format="]);
        assert!(!all.contains("stolen"), "{all}");
    }

    #[test]
    fn unchanged_tracked_files_are_never_listed_as_left_out() {
        let f = fx();
        let outside = f.root.join("shared.txt");
        std::fs::write(&outside, "linked\n").unwrap();
        std::fs::write(f.clone.join("linked.txt"), "linked\n").unwrap();
        std::fs::write(f.clone.join("big.bin"), vec![b'x'; 6 << 20]).unwrap();
        git(&f.clone, &["add", "."]);
        git(&f.clone, &["commit", "-q", "-m", "more files"]);
        std::fs::remove_file(f.clone.join("linked.txt")).unwrap();
        std::fs::hard_link(&outside, f.clone.join("linked.txt")).unwrap();
        std::fs::write(f.clone.join("README.md"), "changed\n").unwrap();
        wip_commit(&f.clone, "tm-1-fix", "x", &f.sources())
            .unwrap()
            .unwrap();
        let msg = git(&f.clone, &["log", "-1", "--format=%B"]);
        assert!(!msg.contains("Left out"), "{msg}");
        assert_eq!(git(&f.clone, &["show", "HEAD:README.md"]), "changed");
        assert_eq!(
            git(&f.clone, &["diff", "--stat", "HEAD^", "HEAD"])
                .lines()
                .count(),
            2
        );
    }

    #[test]
    fn a_changed_big_file_is_still_listed_and_an_unchanged_one_beside_it_is_not() {
        let f = fx();
        std::fs::write(f.clone.join("big.bin"), vec![b'x'; 6 << 20]).unwrap();
        std::fs::write(f.clone.join("big2.bin"), vec![b'y'; 6 << 20]).unwrap();
        git(&f.clone, &["add", "."]);
        git(&f.clone, &["commit", "-q", "-m", "big"]);
        std::fs::write(f.clone.join("big2.bin"), vec![b'z'; 6 << 20]).unwrap();
        std::fs::write(f.clone.join("README.md"), "changed\n").unwrap();
        wip_commit(&f.clone, "tm-1-fix", "x", &f.sources())
            .unwrap()
            .unwrap();
        let msg = git(&f.clone, &["log", "-1", "--format=%B"]);
        assert!(msg.contains("big2.bin (larger than 5 MB)"), "{msg}");
        assert!(!msg.contains("- big.bin"), "{msg}");
    }

    #[test]
    fn a_clone_with_borrowed_objects_or_a_linked_object_store_is_refused() {
        let f = fx();
        std::fs::write(f.clone.join("README.md"), "changed\n").unwrap();
        let other = f.root.join("other/.git/objects");
        std::fs::create_dir_all(&other).unwrap();
        let alternates = f.clone.join(".git/objects/info/alternates");
        let original = std::fs::read_to_string(&alternates).unwrap();
        std::fs::write(&alternates, format!("{original}{}\n", other.display())).unwrap();
        assert!(wip_commit(&f.clone, "tm-1-fix", "x", &f.sources()).is_err());
        std::fs::write(&alternates, original).unwrap();
        let moved = f.root.join("objects-elsewhere");
        std::fs::rename(f.clone.join(".git/objects"), &moved).unwrap();
        std::os::unix::fs::symlink(&moved, f.clone.join(".git/objects")).unwrap();
        assert!(wip_commit(&f.clone, "tm-1-fix", "x", &f.sources()).is_err());
    }

    #[test]
    fn a_stale_index_lock_is_removed() {
        let f = fx();
        std::fs::write(f.clone.join(".git/index.lock"), "").unwrap();
        std::fs::write(f.clone.join("README.md"), "changed\n").unwrap();
        wip_commit(&f.clone, "tm-1-fix", "x", &f.sources())
            .unwrap()
            .unwrap();
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
        let sha = wip_commit(&f.clone, "tm-1-fix", "x", &f.sources())
            .unwrap()
            .unwrap();
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
