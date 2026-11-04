use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
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

/// Whether `path` lies in a run directory made by `clone_dir`, where a queue agent may be writing.
pub fn in_queue_run_dir(path: &Path) -> bool {
    let names: Vec<_> = path.components().map(|c| c.as_os_str()).collect();
    names
        .windows(3)
        .any(|w| w[1] == "queue" && w[0].to_str().is_some_and(|n| n.ends_with("-worktrees")))
}

pub fn branch_name(pickup_id: &str) -> String {
    format!("queue/{pickup_id}")
}

pub const LEAD_DIR: &str = "lead";

pub fn lead_dir(team_dir: &Path) -> PathBuf {
    team_dir.join(LEAD_DIR)
}

pub fn is_team_lead_clone(dir: &Path) -> bool {
    fn name(p: Option<&Path>) -> Option<&str> {
        p.and_then(|p| p.file_name()).and_then(|n| n.to_str())
    }
    let queue = dir.parent().and_then(Path::parent);
    name(Some(dir)) == Some(LEAD_DIR)
        && name(queue) == Some("queue")
        && name(queue.and_then(Path::parent)).is_some_and(|n| n.ends_with("-worktrees"))
}

pub fn worker_dir(team_dir: &Path, k: u8) -> PathBuf {
    team_dir.join(format!("w{k}"))
}

pub fn worker_branch(pickup_id: &str, k: u8) -> String {
    format!("{TEAM_REF_PREFIX}{pickup_id}/w{k}")
}

const TEAM_REF_PREFIX: &str = "queue-team/";

fn member_dir_name(name: &str) -> bool {
    name == LEAD_DIR
        || name
            .strip_prefix('w')
            .is_some_and(|k| matches!(k, "1" | "2" | "3" | "4" | "5"))
}

pub fn absolute_path_entries(value: &std::ffi::OsStr) -> std::ffi::OsString {
    let kept: Vec<PathBuf> = std::env::split_paths(value)
        .filter(|entry| entry.is_absolute())
        .collect();
    match std::env::join_paths(kept) {
        Ok(joined) if !joined.is_empty() => joined,
        _ => "/usr/bin:/bin".into(),
    }
}

pub fn absolute_path() -> std::ffi::OsString {
    absolute_path_entries(&std::env::var_os("PATH").unwrap_or_default())
}

fn fixed_git_command() -> Command {
    let mut cmd = Command::new("git");
    cmd.env_clear()
        .env("PATH", absolute_path())
        .env("HOME", "/nonexistent")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ALLOW_PROTOCOL", "file")
        .env("GIT_NO_LAZY_FETCH", "1")
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
            "-c",
            "core.alternateRefsCommand=",
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
            "-c",
            "submodule.recurse=false",
            "-c",
            "fetch.recurseSubmodules=false",
        ]);
    cmd
}

fn named_program_overrides(names: &[u8]) -> Vec<(OsString, &'static str)> {
    let key = |section: &str, sub: &[u8], var: &str| {
        let mut k = format!("{section}.").into_bytes();
        k.extend_from_slice(sub);
        k.push(b'.');
        k.extend_from_slice(var.as_bytes());
        OsString::from_vec(k)
    };
    let mut out = Vec::new();
    for name in names.split(|b| *b == 0) {
        let Some(dot) = name.iter().position(|b| *b == b'.') else {
            continue;
        };
        let (section, rest) = (&name[..dot], &name[dot + 1..]);
        let Some(last) = rest.iter().rposition(|b| *b == b'.') else {
            continue;
        };
        let sub = &rest[..last];
        match section {
            b"hook" => out.push((key("hook", sub, "enabled"), "false")),
            b"filter" => {
                for var in ["clean", "smudge", "process"] {
                    out.push((key("filter", sub, var), ""));
                }
                out.push((key("filter", sub, "required"), "false"));
            }
            _ => {}
        }
    }
    out.sort();
    out.dedup();
    out
}

pub(crate) fn git_command(dir: Option<&Path>) -> Command {
    let mut cmd = fixed_git_command();
    if let Some(d) = dir {
        let mut list = fixed_git_command();
        list.arg("-C")
            .arg(d)
            .args(["config", "--name-only", "--list", "-z"]);
        let names = output_within(&mut list, GIT_TIMEOUT)
            .map(|o| o.stdout)
            .unwrap_or_default();
        let overrides = named_program_overrides(&names);
        cmd.env("GIT_CONFIG_COUNT", overrides.len().to_string());
        for (i, (key, value)) in overrides.iter().enumerate() {
            cmd.env(format!("GIT_CONFIG_KEY_{i}"), key)
                .env(format!("GIT_CONFIG_VALUE_{i}"), value);
        }
        cmd.arg("-C").arg(d);
    }
    cmd
}

fn plain_config_key(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let Some((section, rest)) = n.split_once('.') else {
        return false;
    };
    let var = rest.rsplit('.').next().unwrap_or("");
    match section {
        "branch" | "user" => true,
        "remote" => matches!(
            var,
            "url" | "pushurl" | "fetch" | "push" | "tagopt" | "prune" | "mirror"
        ),
        "core" => matches!(
            rest,
            "repositoryformatversion"
                | "filemode"
                | "bare"
                | "logallrefupdates"
                | "ignorecase"
                | "precomposeunicode"
                | "symlinks"
                | "autocrlf"
                | "eol"
                | "safecrlf"
                | "abbrev"
                | "quotepath"
                | "untrackedcache"
                | "hookspath"
                | "fsmonitor"
        ),
        "lfs" => matches!(var, "locksverify" | "repositoryformatversion"),
        _ => matches!(
            n.as_str(),
            "commit.gpgsign"
                | "tag.gpgsign"
                | "pull.rebase"
                | "pull.ff"
                | "push.default"
                | "push.autosetupremote"
                | "fetch.prune"
                | "init.defaultbranch"
                | "rebase.autostash"
                | "gc.auto"
                | "maintenance.auto"
                | "extensions.relativeworktrees"
                | "extensions.worktreeconfig"
                | "worktree.userelativepaths"
        ),
    }
}

fn normalized(path: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::RootDir => out.push("/"),
            Component::Normal(name) => out.push(name),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            Component::Prefix(_) => {}
        }
    }
    Some(out)
}

fn named_path(dir: &Path, text: &str, prefix: &str) -> Option<PathBuf> {
    let target = text.trim_end_matches(['\n', '\r']).strip_prefix(prefix)?;
    if target.is_empty() || target.trim() != target || target.chars().any(char::is_control) {
        return None;
    }
    let joined = dir.join(target);
    let path = normalized(&joined)?;
    (std::fs::canonicalize(&joined).ok()? == path).then_some(path)
}

/// The git directory of the work folder `root` and its common directory, when `.git` is the folder's own
/// directory, a `gitdir:` file naming a directory inside the folder, or one naming a worktree that a
/// repository's `.git/worktrees/` registered for this very folder. A git directory inside the folder must
/// not name a common directory elsewhere.
pub fn expected_git_dir(root: &Path) -> Option<(PathBuf, PathBuf)> {
    let root = std::fs::canonicalize(root).ok()?;
    let dot_git = root.join(".git");
    let git_dir = if std::fs::symlink_metadata(&dot_git).ok()?.is_dir() {
        dot_git.clone()
    } else {
        named_path(&root, &read_small(&root, ".git")?, "gitdir: ")?
    };
    if git_dir.starts_with(&root) {
        let commondir = std::fs::symlink_metadata(git_dir.join("commondir"));
        return matches!(commondir, Err(e) if e.kind() == std::io::ErrorKind::NotFound)
            .then(|| (git_dir.clone(), git_dir));
    }
    let worktrees = git_dir.parent()?;
    let common = worktrees.parent()?;
    if worktrees.file_name()? != "worktrees"
        || common.file_name()? != ".git"
        || !std::fs::symlink_metadata(common).ok()?.is_dir()
    {
        return None;
    }
    let back = named_path(&git_dir, &read_small(&git_dir, "gitdir")?, "")?;
    (back == dot_git).then(|| (git_dir.clone(), common.to_path_buf()))
}

/// Whether the repository's own configuration holds only settings that run nothing once git commands pin core.hooksPath and core.fsmonitor.
pub fn repo_config_is_plain(dir: &Path) -> bool {
    tempfile::tempdir().is_ok_and(|tmp| host_git_dir(dir, tmp.path()).is_some())
}

/// Fills `into`, an empty folder only the host writes, with a git directory for the work folder `dir`
/// (as `expected_git_dir` finds it) to pass as `GIT_DIR` with `dir` as the work tree. Its config is
/// written here from the plain settings of the repository (`repo_config_is_plain`, including the
/// worktree's `config.worktree` when extensions.worktreeConfig is on); HEAD, branches, remote-tracking
/// refs, the index, info/exclude and shallow are copies read once as data, and objects come through an
/// alternate. Nothing git reads afterwards names a program from the folder.
pub fn host_git_dir(dir: &Path, into: &Path) -> Option<()> {
    let (git_dir, common) = expected_git_dir(dir)?;
    let objects = common.join("objects");
    let objects = objects.to_str().filter(|o| !o.contains('\n'))?;
    let mut settings = plain_settings(&common, "config", into)?;
    let worktree_config = settings
        .iter()
        .rev()
        .find(|(k, _)| k == "extensions.worktreeconfig")
        .is_some_and(|(_, v)| config_true(v.as_deref()));
    if worktree_config {
        settings.extend(plain_settings(&git_dir, "config.worktree", into)?);
    }
    let config = host_config(&settings)?;
    let head = read_small(&git_dir, "HEAD")?;
    let head = head.strip_suffix('\n').unwrap_or(&head);
    let head_ok = match head.strip_prefix("ref: ") {
        Some(name) => name.starts_with("refs/heads/") && plain_ref_name(name),
        None => is_sha(head),
    };
    if !head_ok {
        return None;
    }
    let (direct, symbolic) = host_refs(&common)?;
    let index = optional_file(&git_dir, "index", MAX_INDEX)?;
    let shallow = optional_file(&common, "shallow", 1 << 20)?;
    if let Some(text) = &shallow {
        let text = std::str::from_utf8(text).ok()?;
        if !text.lines().all(is_sha) {
            return None;
        }
    }
    let exclude = optional_file(&common, "info/exclude", 1 << 20).flatten();
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(into.join("objects/info"))?;
        std::fs::create_dir_all(into.join("refs"))?;
        std::fs::write(into.join("objects/info/alternates"), format!("{objects}\n"))?;
        std::fs::write(into.join("config"), &config)?;
        std::fs::write(into.join("HEAD"), format!("{head}\n"))?;
        let packed: String = direct
            .iter()
            .map(|(name, sha)| format!("{sha} {name}\n"))
            .collect();
        std::fs::write(into.join("packed-refs"), packed)?;
        for (name, target) in &symbolic {
            let path = into.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, format!("ref: {target}\n"))?;
        }
        if let Some(index) = &index {
            std::fs::write(into.join("index"), index)?;
        }
        if let Some(shallow) = &shallow {
            std::fs::write(into.join("shallow"), shallow)?;
        }
        if let Some(exclude) = &exclude {
            std::fs::create_dir_all(into.join("info"))?;
            std::fs::write(into.join("info/exclude"), exclude)?;
        }
        Ok(())
    };
    write().ok()
}

fn config_true(value: Option<&str>) -> bool {
    value.is_none_or(|v| {
        !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "false" | "no" | "off" | "0" | ""
        )
    })
}

/// The settings of the config file `rel` in `dir`, read once and parsed from our own copy, when every
/// key is plain. A missing file has none.
fn plain_settings(dir: &Path, rel: &str, scratch: &Path) -> Option<Vec<(String, Option<String>)>> {
    let Some(bytes) = optional_file(dir, rel, 1 << 20)? else {
        return Some(Vec::new());
    };
    let copy = scratch.join("snapshot.config");
    std::fs::write(&copy, bytes).ok()?;
    let mut list = fixed_git_command();
    list.env("GIT_CEILING_DIRECTORIES", scratch)
        .args(["config", "--no-includes", "--list", "-z", "--file"])
        .arg(&copy)
        .current_dir(scratch);
    let out = output_bounded(&mut list, std::time::Duration::from_secs(10), 4 << 20);
    std::fs::remove_file(&copy).ok()?;
    let out = out.ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?;
    let settings: Vec<(String, Option<String>)> = text
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .map(|entry| match entry.split_once('\n') {
            Some((key, value)) => (key.to_string(), Some(value.to_string())),
            None => (entry.to_string(), None),
        })
        .collect();
    settings
        .iter()
        .all(|(key, _)| plain_config_key(key))
        .then_some(settings)
}

/// The bytes of the regular file `rel` in `dir` up to `cap`, `Some(None)` when it does not exist, and
/// `None` when it is anything else or larger.
fn optional_file(dir: &Path, rel: &str, cap: u64) -> Option<Option<Vec<u8>>> {
    use std::io::Read;
    match std::fs::symlink_metadata(dir.join(rel)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(None),
        _ => {}
    }
    let file = open_beneath(dir, rel, libc::O_NONBLOCK)?;
    let meta = file.metadata().ok()?;
    if !meta.file_type().is_file() || meta.len() > cap {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(cap + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= cap).then_some(Some(bytes))
}

fn host_config(settings: &[(String, Option<String>)]) -> Option<String> {
    let quoted = |s: &str| -> Option<String> {
        (!s.chars().any(char::is_control))
            .then(|| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")))
    };
    let mut out = String::from("[core]\n\trepositoryformatversion = 0\n\tbare = false\n");
    for (key, value) in settings {
        let (section, rest) = key.split_once('.')?;
        let (sub, var) = match rest.rsplit_once('.') {
            Some((sub, var)) => (Some(sub), var),
            None => (None, rest),
        };
        let carried = match (section, sub) {
            ("core", None) => matches!(
                var,
                "filemode"
                    | "ignorecase"
                    | "precomposeunicode"
                    | "symlinks"
                    | "autocrlf"
                    | "eol"
                    | "safecrlf"
            ),
            ("branch", Some(_)) => matches!(var, "remote" | "pushremote" | "merge" | "rebase"),
            ("remote", Some(_)) => matches!(var, "url" | "pushurl" | "fetch" | "push"),
            _ => false,
        };
        if !carried {
            continue;
        }
        let header = match sub {
            Some(sub) => format!("[{section} {}]\n", quoted(sub)?),
            None => format!("[{section}]\n"),
        };
        let value = quoted(value.as_deref().unwrap_or("true"))?;
        out.push_str(&format!("{header}\t{var} = {value}\n"));
    }
    Some(out)
}

const MAX_HOST_REFS: usize = 100_000;

/// The branches and remote-tracking refs of the common directory `common`, read as data: direct refs
/// with their objects and symbolic refs with their targets. Loose refs win over packed ones.
fn host_refs(common: &Path) -> Option<(BTreeMap<String, String>, BTreeMap<String, String>)> {
    use std::io::Read;
    let wanted = |name: &str| {
        (name.starts_with("refs/heads/") || name.starts_with("refs/remotes/"))
            && plain_ref_name(name)
    };
    let mut direct = BTreeMap::new();
    if let Some(bytes) = optional_file(common, "packed-refs", 64 << 20)? {
        for line in std::str::from_utf8(&bytes).ok()?.lines() {
            let Some((sha, name)) = line.split_once(' ') else {
                continue;
            };
            if is_sha(sha) && wanted(name) {
                direct.insert(name.to_string(), sha.to_string());
            }
        }
    }
    let mut symbolic = BTreeMap::new();
    let mut pending = vec!["refs/heads".to_string(), "refs/remotes".to_string()];
    let mut seen = 0usize;
    while let Some(rel) = pending.pop() {
        let Some(dir) = open_beneath(common, &rel, libc::O_DIRECTORY) else {
            continue;
        };
        for name in entries(&dir) {
            seen += 1;
            if seen > MAX_HOST_REFS {
                return None;
            }
            let Some(name_text) = name.to_str() else {
                continue;
            };
            let full = format!("{rel}/{name_text}");
            if open_in(&dir, &name, libc::O_DIRECTORY).is_some() {
                pending.push(full);
                continue;
            }
            let Some(file) = open_in(&dir, &name, libc::O_NONBLOCK) else {
                continue;
            };
            if !file.metadata().is_ok_and(|m| m.file_type().is_file()) || !wanted(&full) {
                continue;
            }
            let mut text = String::new();
            if file.take(4096).read_to_string(&mut text).is_err() {
                continue;
            }
            let text = text.trim_end_matches('\n');
            if let Some(target) = text.strip_prefix("ref: ") {
                if wanted(target) {
                    direct.remove(&full);
                    symbolic.insert(full, target.to_string());
                }
            } else if is_sha(text) {
                direct.insert(full, text.to_string());
            }
        }
    }
    Some((direct, symbolic))
}

pub fn git_bytes(dir: &Path, args: &[&str], input: Option<Vec<u8>>) -> Result<Vec<u8>> {
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

pub const TRUSTED_OUTPUT_CAP: usize = 64 << 20;

/// Like `output_within`, but the deadline also covers collecting the output (a grandchild keeping a pipe
/// open cannot hold the caller) and more than `cap` bytes on a pipe is an error, never a truncated result.
pub fn output_bounded(
    cmd: &mut Command,
    limit: std::time::Duration,
    cap: usize,
) -> Result<std::process::Output> {
    use std::io::Read;
    use std::process::Stdio;
    use std::sync::mpsc::{channel, RecvTimeoutError};
    use std::time::{Duration, Instant};
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::internal(format!("cannot run git: {e}")))?;
    let (tx, rx) = channel::<(bool, Option<Vec<u8>>)>();
    let pipes: [(bool, Option<Box<dyn Read + Send>>); 2] = [
        (
            true,
            child
                .stdout
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        ),
        (
            false,
            child
                .stderr
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        ),
    ];
    for (is_out, pipe) in pipes {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let mut chunk = [0u8; 8192];
                loop {
                    match p.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            buf.extend_from_slice(&chunk[..n]);
                            if buf.len() > cap {
                                let _ = tx.send((is_out, None));
                                return;
                            }
                        }
                    }
                }
            }
            let _ = tx.send((is_out, Some(buf)));
        });
    }
    drop(tx);
    let fail = |child: &mut std::process::Child, msg: String| {
        let _ = child.kill();
        let _ = child.wait();
        Err(Error::internal(msg))
    };
    let deadline = Instant::now() + limit;
    let mut out: Option<Vec<u8>> = None;
    let mut err: Option<Vec<u8>> = None;
    let mut status: Option<std::process::ExitStatus> = None;
    loop {
        if status.is_none() {
            match child.try_wait() {
                Ok(st) => status = st,
                Err(e) => return fail(&mut child, format!("cannot wait for git: {e}")),
            }
        }
        if let (Some(st), Some(o), Some(e)) = (status, out.as_ref(), err.as_ref()) {
            return Ok(std::process::Output {
                status: st,
                stdout: o.clone(),
                stderr: e.clone(),
            });
        }
        let now = Instant::now();
        if now >= deadline {
            return fail(&mut child, "git did not finish in time".into());
        }
        let wait = (deadline - now).min(Duration::from_millis(10));
        match rx.recv_timeout(wait) {
            Ok((is_out, Some(buf))) => *(if is_out { &mut out } else { &mut err }) = Some(buf),
            Ok((_, None)) => return fail(&mut child, format!("git printed more than {cap} bytes")),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                if out.is_none() || err.is_none() {
                    return fail(&mut child, "git output could not be read".into());
                }
                std::thread::sleep(wait);
            }
        }
    }
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

    /// One clone of a team run (`<run folder>/lead` or `<run folder>/wK`) on a new `branch`.
    fn prepare_member(
        &self,
        _repo: &str,
        _pickup_id: &str,
        _cwd: &str,
        _branch: &str,
    ) -> Result<()> {
        Err(Error::internal(
            "this work directory provider cannot make team clones",
        ))
    }

    /// A team member folder (`<run folder>/lead` or `wK`) holding one clone per `(name, repository)`.
    fn prepare_member_multi(
        &self,
        _primary: &str,
        _pickup_id: &str,
        _cwd: &str,
        _clones: &[(String, String)],
        _branch: &str,
    ) -> Result<()> {
        Err(Error::internal(
            "this work directory provider cannot make several team clones",
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

    /// Continues a paused run: a clone (`clones` empty: one clone of `primary`, saved under `""`) whose task branch
    /// starts at the commit saved in `heads`. Refuses unless the remote branch is still there.
    fn prepare_continue(
        &self,
        _primary: &str,
        _pickup_id: &str,
        _cwd: &str,
        _clones: &[(String, String)],
        _branch: &str,
        _heads: &BTreeMap<String, String>,
    ) -> Result<()> {
        Err(Error::internal(
            "this work directory provider cannot continue a paused run",
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

/// A hardened `git clone --no-local --reference` of `repo` at `dest` on a new `branch` (from `base`, else the repo's
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
    make_clone(repo, dest, &|dest| {
        git(
            Some(dest),
            &["checkout", "-b", branch, &format!("origin/{base}")],
        )
        .or_else(|_| git(Some(dest), &["checkout", "-b", branch, &base]))
        .map(|_| ())
    })
}

fn make_clone(repo: &str, dest: &Path, checkout: &dyn Fn(&Path) -> Result<()>) -> Result<()> {
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
                "--no-local",
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
        checkout(dest)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(dest);
    }
    result
}

/// The commit `refs/heads/<branch>` points at on the remote `origin` of `repo`, or `None` when the remote has no such branch.
pub fn remote_branch_head(repo: &str, branch: &str) -> Result<Option<String>> {
    validate_branch(branch)?;
    let reference = format!("refs/heads/{branch}");
    let argv: Vec<String> = ["git", "-C", repo, "ls-remote", "origin", &reference]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let out = crate::publish::run_hardened(&argv, Path::new(repo))?;
    Ok(out
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .find(|(_, r)| *r == reference)
        .map(|(sha, _)| sha.to_string()))
}

fn moved_remotely(branch: &str) -> Error {
    Error::conflict(format!(
        "the task branch {branch} on the remote is not at the commit saved when the run paused; nothing was cloned or pushed"
    ))
}

/// A hardened clone of `repo` at `dest` whose `branch` starts at `sha`, the commit the paused run pushed. Refuses
/// unless the remote branch is still at `sha`.
pub fn prepare_continue_clone(repo: &str, dest: &Path, branch: &str, sha: &str) -> Result<()> {
    validate_branch(branch)?;
    if dest.exists() {
        return Err(Error::conflict("the run directory already exists"));
    }
    if !is_sha(sha) {
        return Err(Error::internal("the saved commit is not a full object id"));
    }
    match remote_branch_head(repo, branch)? {
        Some(head) if head.eq_ignore_ascii_case(sha) => {}
        _ => return Err(moved_remotely(branch)),
    }
    let commit = format!("{sha}^{{commit}}");
    let have = |r: &str| git(Some(Path::new(r)), &["cat-file", "-e", &commit]).is_ok();
    if !have(repo) {
        let argv: Vec<String> = [
            "git",
            "-C",
            repo,
            "fetch",
            "--no-tags",
            "origin",
            &format!("refs/heads/{branch}"),
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        crate::publish::run_hardened(&argv, Path::new(repo))?;
        if !have(repo) {
            return Err(moved_remotely(branch));
        }
    }
    make_clone(repo, dest, &|dest| {
        git(Some(dest), &["checkout", "-b", branch, sha]).map(|_| ())
    })
}

fn prepare_continue_folder(
    run_dir: &Path,
    clones: &[(String, String)],
    branch: &str,
    heads: &BTreeMap<String, String>,
) -> Result<()> {
    if run_dir.exists() {
        return Err(Error::conflict("the run directory already exists"));
    }
    std::fs::create_dir_all(run_dir)?;
    let result = clones.iter().try_for_each(|(name, repo)| {
        let dest = run_dir.join(name);
        match heads.get(name) {
            Some(sha) => match remote_branch_head(repo, branch)? {
                None => Err(moved_remotely(branch)),
                Some(_) => prepare_continue_clone(repo, &dest, branch, sha),
            },
            None => match remote_branch_head(repo, branch)? {
                None => prepare_clone(repo, &dest, branch, None),
                Some(_) => Err(moved_remotely(branch)),
            },
        }
    });
    if result.is_err() {
        let _ = std::fs::remove_dir_all(run_dir);
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

pub(crate) fn unlink_beneath(root: &Path, rel: &str) -> bool {
    use std::os::fd::AsRawFd;
    let Some((parent, name)) = rel.rsplit_once('/') else {
        return false;
    };
    let Some(dir) = open_beneath(root, parent, libc::O_DIRECTORY) else {
        return false;
    };
    let Ok(name) = std::ffi::CString::new(name) else {
        return false;
    };
    unsafe { libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), 0) == 0 }
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

/// What a team worker's copy says about its refs, read as data without running git there.
pub struct CopyRefs {
    pub head: String,
    /// Every commit the copy's reflogs name.
    pub logged: Vec<String>,
    pub stash: bool,
    /// Every other ref of the copy (branches, tags, remote-tracking refs, notes...) with its object.
    pub others: Vec<(String, String)>,
}

fn plain_ref_name(name: &str) -> bool {
    name.starts_with("refs/")
        && name.split('/').all(|c| {
            !c.is_empty() && c != "." && c != ".." && !c.bytes().any(|b| b.is_ascii_control())
        })
}

fn loose_refs(dir: &std::fs::File, prefix: &str, out: &mut Vec<String>) -> bool {
    for name in entries(dir) {
        let full = format!("{prefix}{}", name.to_string_lossy());
        if let Some(sub) = open_in(dir, &name, libc::O_DIRECTORY) {
            if !loose_refs(&sub, &format!("{full}/"), out) {
                return false;
            }
        } else if open_in(dir, &name, libc::O_NONBLOCK)
            .and_then(|f| f.metadata().ok())
            .is_some_and(|m| m.file_type().is_file())
        {
            out.push(full);
        } else {
            return false;
        }
    }
    true
}

/// The branch tip, the stash flag and every other ref of a worker's copy. HEAD must name `branch`.
/// Symbolic refs are skipped: they point at refs that are listed themselves.
pub fn copy_refs(copy: &Path, branch: &str) -> std::result::Result<CopyRefs, String> {
    if clone_head_branch(copy).as_deref() != Some(branch) {
        return Err(format!("HEAD is not on the branch {branch}"));
    }
    let reference = format!("refs/heads/{branch}");
    let root = open_beneath(copy, ".git/refs", libc::O_DIRECTORY)
        .ok_or("the copy's refs cannot be read")?;
    let mut loose = Vec::new();
    if !loose_refs(&root, "refs/", &mut loose) {
        return Err("the copy's refs have entries that are not plain files".into());
    }
    let mut refs: BTreeMap<String, String> = BTreeMap::new();
    let mut packed: BTreeMap<String, String> = BTreeMap::new();
    if std::fs::symlink_metadata(copy.join(".git/packed-refs")).is_ok() {
        let text =
            read_small(copy, ".git/packed-refs").ok_or("the copy's packed refs cannot be read")?;
        for line in text.lines() {
            if line.starts_with('#') || line.starts_with('^') || line.is_empty() {
                continue;
            }
            let (sha, name) = line
                .split_once(' ')
                .ok_or("the copy's packed refs are malformed")?;
            if !plain_ref_name(name) || !is_sha(sha) {
                return Err("the copy has a ref that is not readable".into());
            }
            packed.insert(name.to_string(), sha.to_string());
        }
    }
    for name in &loose {
        if !plain_ref_name(name) {
            return Err("the copy has a ref that is not readable".into());
        }
        let text =
            read_small(copy, &format!(".git/{name}")).ok_or("the copy's refs cannot be read")?;
        let text = text.trim();
        if text.starts_with("ref: ") {
            continue;
        }
        if !is_sha(text) {
            return Err("the copy has a ref that is not readable".into());
        }
        refs.insert(name.clone(), text.to_string());
    }
    for (name, sha) in packed {
        refs.entry(name).or_insert(sha);
    }
    let stash = refs.remove("refs/stash").is_some();
    let head = refs
        .remove(&reference)
        .ok_or("the copy's branch has no readable commit")?;
    Ok(CopyRefs {
        head,
        logged: logged_commits(copy)?,
        stash,
        others: refs.into_iter().collect(),
    })
}

fn logged_commits(copy: &Path) -> std::result::Result<Vec<String>, String> {
    let unreadable = || "the copy's reflog cannot be read".to_string();
    match std::fs::symlink_metadata(copy.join(".git/logs")) {
        Err(_) => return Ok(Vec::new()),
        Ok(m) if !m.is_dir() => return Err(unreadable()),
        Ok(_) => {}
    }
    let mut files = Vec::new();
    if std::fs::symlink_metadata(copy.join(".git/logs/HEAD")).is_ok() {
        files.push("HEAD".to_string());
    }
    if std::fs::symlink_metadata(copy.join(".git/logs/refs")).is_ok() {
        let root =
            open_beneath(copy, ".git/logs/refs", libc::O_DIRECTORY).ok_or_else(unreadable)?;
        let mut names = Vec::new();
        if !loose_refs(&root, "refs/", &mut names) {
            return Err(unreadable());
        }
        files.extend(names);
    }
    let mut shas = std::collections::BTreeSet::new();
    for file in files {
        let text = read_small(copy, &format!(".git/logs/{file}")).ok_or_else(unreadable)?;
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let mut tokens = line.split(' ');
            for sha in [tokens.next(), tokens.next()] {
                let sha = sha.filter(|s| is_sha(s)).ok_or_else(unreadable)?;
                if !sha.bytes().all(|b| b == b'0') {
                    shas.insert(sha.to_string());
                }
            }
        }
    }
    Ok(shas.into_iter().collect())
}

const MAX_INDEX: u64 = 64 << 20;

fn only_regular_files(dir: &std::fs::File) -> bool {
    entries(dir).iter().all(|name| {
        open_in(dir, name, libc::O_NONBLOCK)
            .and_then(|f| f.metadata().ok())
            .is_some_and(|m| m.file_type().is_file())
    })
}

fn object_store_plain(copy: &Path) -> bool {
    let Some(objects) = open_beneath(copy, ".git/objects", libc::O_DIRECTORY) else {
        return false;
    };
    entries(&objects).iter().all(|name| {
        let n = name.to_string_lossy();
        let fan_out = n.len() == 2 && n.bytes().all(|b| b.is_ascii_hexdigit());
        if !fan_out && n != "pack" {
            return true;
        }
        open_in(&objects, name, libc::O_DIRECTORY).is_some_and(|d| only_regular_files(&d))
    })
}

/// Whether the copy's own object alternates are none, or only `allowed` (the repository it was cloned with
/// `--reference`). Anything else would make git read objects from elsewhere.
pub fn copy_alternates_ok(copy: &Path, allowed: &Path) -> bool {
    let real_dir = |rel: &str| std::fs::symlink_metadata(copy.join(rel)).map(|m| m.is_dir());
    if [".git", ".git/objects"]
        .iter()
        .any(|r| !matches!(real_dir(r), Ok(true)))
        || [".git/objects/info", ".git/objects/pack"]
            .iter()
            .any(|r| matches!(real_dir(r), Ok(false)))
    {
        return false;
    }
    if !object_store_plain(copy) {
        return false;
    }
    let rel = ".git/objects/info/alternates";
    if std::fs::symlink_metadata(copy.join(rel)).is_err() {
        return true;
    }
    let Some(text) = read_small(copy, rel) else {
        return false;
    };
    let allowed = std::fs::canonicalize(allowed).ok();
    text.lines().filter(|l| !l.trim().is_empty()).all(|l| {
        let p = Path::new(l.trim());
        p.is_absolute() && allowed.is_some() && std::fs::canonicalize(p).ok() == allowed
    })
}

/// Makes `git_dir` (an empty folder of ours) a bare repository that reads a worker's copy as data:
/// its objects through alternates, its index, its branch as `HEAD`, every other ref of the copy as
/// `refs/copy/<n>` and `trusted` commits as `refs/host/<n>`. None of the copy's git settings or ref
/// names reach git.
pub fn trusted_git_dir(
    git_dir: &Path,
    copy: &Path,
    branch: &str,
    refs: &CopyRefs,
    trusted: &[String],
    limit: std::time::Duration,
) -> std::result::Result<(), String> {
    use std::io::Read;
    validate_branch(branch).map_err(|e| e.message)?;
    let objects = copy.join(".git/objects");
    let objects = objects
        .to_str()
        .filter(|o| !o.contains('\n'))
        .ok_or("the copy's path is not plain")?;
    let mut init = fixed_git_command();
    init.args(["init", "--bare", "-q"])
        .arg(git_dir)
        .current_dir(git_dir);
    let out = output_bounded(&mut init, limit, TRUSTED_OUTPUT_CAP).map_err(|e| e.message)?;
    if !out.status.success() {
        return Err("git init failed".into());
    }
    let file = open_beneath(copy, ".git/index", libc::O_NONBLOCK).ok_or("the copy has no index")?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.file_type().is_file() || meta.len() > MAX_INDEX {
        return Err("the copy's index is not a plain file of a sane size".into());
    }
    let mut index = Vec::new();
    file.take(MAX_INDEX)
        .read_to_end(&mut index)
        .map_err(|e| e.to_string())?;
    let mut packed = format!("{} refs/heads/{branch}\n", refs.head);
    for (n, (_, sha)) in refs.others.iter().enumerate() {
        packed.push_str(&format!("{sha} refs/copy/{n}\n"));
    }
    for (n, sha) in trusted.iter().filter(|s| is_sha(s)).enumerate() {
        packed.push_str(&format!("{sha} refs/host/{n}\n"));
    }
    for (n, sha) in refs.logged.iter().enumerate() {
        packed.push_str(&format!("{sha} refs/log/{n}\n"));
    }
    let exclude = match std::fs::symlink_metadata(copy.join(".git/info/exclude")) {
        Ok(_) => Some(
            read_small(copy, ".git/info/exclude")
                .ok_or("the copy's exclude file cannot be read")?,
        ),
        Err(_) => None,
    };
    let write = || -> std::io::Result<()> {
        std::fs::write(
            git_dir.join("objects/info/alternates"),
            format!("{objects}\n"),
        )?;
        std::fs::write(git_dir.join("index"), &index)?;
        std::fs::write(git_dir.join("HEAD"), format!("ref: refs/heads/{branch}\n"))?;
        if let Some(text) = &exclude {
            std::fs::create_dir_all(git_dir.join("info"))?;
            std::fs::write(git_dir.join("info/exclude"), text)?;
        }
        std::fs::write(git_dir.join("packed-refs"), packed)
    };
    write().map_err(|e| e.to_string())
}

/// Runs git over a folder from `trusted_git_dir` with `copy` as the work tree.
pub fn trusted_git_bytes(
    git_dir: &Path,
    copy: &Path,
    args: &[&str],
    limit: std::time::Duration,
) -> std::result::Result<Vec<u8>, String> {
    let mut cmd = fixed_git_command();
    cmd.arg("--git-dir")
        .arg(git_dir)
        .arg("--work-tree")
        .arg(copy)
        .args([
            "-c",
            "core.bare=false",
            "-c",
            "core.quotepath=off",
            "-c",
            "core.commitGraph=false",
        ])
        .args(args)
        .current_dir(git_dir);
    let out = output_bounded(&mut cmd, limit, TRUSTED_OUTPUT_CAP).map_err(|e| e.message)?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr);
        let line = msg.lines().next().unwrap_or("failed");
        return Err(format!("git {}: {line}", args.first().unwrap_or(&"")));
    }
    Ok(out.stdout)
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
    let mut dir = Path::new(cwd);
    let named = |p: Option<&Path>, want: &dyn Fn(&str) -> bool| {
        p.and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .is_some_and(want)
    };
    let team = named(Some(dir), &|n| n == LEAD_DIR) && named(dir.parent(), &|n| n == pickup_id);
    if team {
        dir = dir.parent().unwrap_or(dir);
    }
    let ours = named(Some(dir), &|n| n == pickup_id)
        && named(dir.parent(), &|n| n == "queue")
        && named(dir.parent().and_then(Path::parent), &|n| {
            n.ends_with("-worktrees")
        });
    let real = std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir());
    if ours && real {
        let _ = std::fs::remove_dir_all(dir);
        if team {
            if let Some(repo) = repo_of_run_dir(dir) {
                remove_team_refs(&repo, pickup_id);
            }
        }
    }
}

/// `<parent>/<repo>` for a run folder `<parent>/<repo>-worktrees/queue/<id>`.
pub fn repo_of_run_dir(run_dir: &Path) -> Option<PathBuf> {
    let worktrees = run_dir.parent()?.parent()?;
    let stem = worktrees
        .file_name()?
        .to_str()?
        .strip_suffix("-worktrees")?;
    Some(worktrees.parent()?.join(stem))
}

/// Deletes the temporary `queue-team/<pickup>/*` branches of a finished team run from the repository.
pub fn remove_team_refs(repo: &Path, pickup_id: &str) {
    let plain = !pickup_id.is_empty()
        && pickup_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !plain {
        return;
    }
    let prefix = format!("refs/heads/{TEAM_REF_PREFIX}{pickup_id}/");
    let Ok(listed) = git(
        Some(repo),
        &["for-each-ref", "--format=%(refname)", &prefix],
    ) else {
        return;
    };
    for name in listed.lines().filter(|l| l.starts_with(&prefix)) {
        let _ = git(Some(repo), &["update-ref", "-d", name]);
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

    fn prepare_member(&self, repo: &str, pickup_id: &str, cwd: &str, branch: &str) -> Result<()> {
        let team_dir = clone_dir(repo, pickup_id)
            .ok_or_else(|| Error::internal("the repository path has no parent directory"))?;
        let dest = Path::new(cwd);
        let named = dest
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(member_dir_name);
        if dest.parent() != Some(team_dir.as_path()) || !named {
            return Err(Error::internal(
                "the run directory does not belong to this pickup",
            ));
        }
        prepare_clone(repo, dest, branch, None)
    }

    fn prepare_member_multi(
        &self,
        primary: &str,
        pickup_id: &str,
        cwd: &str,
        clones: &[(String, String)],
        branch: &str,
    ) -> Result<()> {
        let team_dir = clone_dir(primary, pickup_id)
            .ok_or_else(|| Error::internal("the repository path has no parent directory"))?;
        let dest = Path::new(cwd);
        let named = dest
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(member_dir_name);
        if dest.parent() != Some(team_dir.as_path()) || !named {
            return Err(Error::internal(
                "the run directory does not belong to this pickup",
            ));
        }
        prepare_run_folder(dest, clones, branch)
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

    fn prepare_continue(
        &self,
        primary: &str,
        pickup_id: &str,
        cwd: &str,
        clones: &[(String, String)],
        branch: &str,
        heads: &BTreeMap<String, String>,
    ) -> Result<()> {
        let dest = clone_dir(primary, pickup_id)
            .ok_or_else(|| Error::internal("the repository path has no parent directory"))?;
        if dest != Path::new(cwd) {
            return Err(Error::internal(
                "the run directory does not belong to this pickup",
            ));
        }
        if clones.is_empty() {
            let sha = heads
                .get("")
                .ok_or_else(|| Error::internal("no commit was saved for this run"))?;
            return prepare_continue_clone(primary, &dest, branch, sha);
        }
        prepare_continue_folder(&dest, clones, branch, heads)
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
    fn host_git_commands_keep_only_absolute_path_entries() {
        assert_eq!(
            absolute_path_entries(":.:/usr/bin:bin:/bin".as_ref()),
            std::ffi::OsString::from("/usr/bin:/bin")
        );
        assert_eq!(
            absolute_path_entries("relative".as_ref()),
            std::ffi::OsString::from("/usr/bin:/bin")
        );
    }

    #[test]
    fn only_settings_known_to_run_nothing_count_as_plain() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = std::fs::canonicalize(tmp.path()).unwrap();
        init_repo(&repo);
        assert!(repo_config_is_plain(&repo));
        git(Some(&repo), &["config", "remote.origin.url", "file:///x"]).unwrap();
        git(Some(&repo), &["config", "branch.main.remote", "origin"]).unwrap();
        assert!(repo_config_is_plain(&repo));
        git(Some(&repo), &["config", "unknown.setting", "1"]).unwrap();
        assert!(!repo_config_is_plain(&repo));
        assert!(!repo_config_is_plain(&repo.join("missing")));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_repository_path_that_is_not_utf8_is_never_plain() {
        use std::os::unix::ffi::OsStrExt;
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = root.join(std::ffi::OsStr::from_bytes(b"repo\xff"));
        std::fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        assert!(!repo_config_is_plain(&repo));
    }

    #[test]
    fn only_queue_run_directories_count_as_queue_run_directories() {
        assert!(in_queue_run_dir(Path::new("/w/p-worktrees/queue/id")));
        assert!(in_queue_run_dir(Path::new("/w/p-worktrees/queue/id/src")));
        assert!(!in_queue_run_dir(Path::new("/w/p-worktrees/queue")));
        assert!(!in_queue_run_dir(Path::new("/w/p-worktrees/feature")));
        assert!(!in_queue_run_dir(Path::new("/w/queue/id/src")));
    }

    #[test]
    fn team_member_copies_count_as_queue_run_directories() {
        let team = Path::new("/w/p-worktrees/queue/q_abc");
        for dir in [lead_dir(team), worker_dir(team, 1), worker_dir(team, 5)] {
            assert!(in_queue_run_dir(&dir), "{}", dir.display());
            assert!(in_queue_run_dir(&dir.join("src")));
        }
    }

    fn worktree_fixture() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        let wt = root.join("wt");
        git(
            Some(&repo),
            &["worktree", "add", "-q", "--detach", wt.to_str().unwrap()],
        )
        .unwrap();
        (tmp, root, repo, wt)
    }

    #[test]
    fn a_git_file_of_a_registered_worktree_is_followed_to_its_repository_configuration() {
        let (_tmp, _root, repo, wt) = worktree_fixture();
        assert!(repo_config_is_plain(&wt));
        git(Some(&repo), &["config", "include.path", "elsewhere"]).unwrap();
        assert!(!repo_config_is_plain(&wt));
    }

    #[test]
    fn relative_git_file_and_back_pointer_of_a_worktree_are_accepted() {
        let (_tmp, _root, repo, wt) = worktree_fixture();
        let admin = repo.join(".git/worktrees/wt");
        std::fs::write(wt.join(".git"), "gitdir: ../repo/.git/worktrees/wt\n").unwrap();
        std::fs::write(admin.join("gitdir"), "../../../../wt/.git\n").unwrap();
        git(
            Some(&repo),
            &["config", "core.repositoryFormatVersion", "1"],
        )
        .unwrap();
        git(
            Some(&repo),
            &["config", "extensions.relativeWorktrees", "true"],
        )
        .unwrap();
        assert!(repo_config_is_plain(&wt));
    }

    #[test]
    fn a_git_dir_inside_the_work_folder_is_accepted_without_a_commondir() {
        let (_tmp, root, repo, _wt) = worktree_fixture();
        let own = root.join("own");
        std::fs::create_dir_all(&own).unwrap();
        init_repo(&own);
        assert!(repo_config_is_plain(&own));
        std::fs::write(
            own.join(".git/commondir"),
            format!("{}\n", repo.join(".git").display()),
        )
        .unwrap();
        assert!(!repo_config_is_plain(&own));

        let moved = root.join("moved");
        std::fs::create_dir_all(&moved).unwrap();
        init_repo(&moved);
        std::fs::rename(moved.join(".git"), moved.join("inner")).unwrap();
        std::fs::write(moved.join(".git"), "gitdir: inner\n").unwrap();
        assert!(repo_config_is_plain(&moved));
        for text in ["gitdir: inner \n", "gitdir: inner\t\n", "gitdir:  inner\n"] {
            std::fs::write(moved.join(".git"), text).unwrap();
            assert!(!repo_config_is_plain(&moved), "{text:?}");
        }
        std::os::unix::fs::symlink(&root, moved.join("up")).unwrap();
        std::fs::write(moved.join(".git"), "gitdir: up/../inner\n").unwrap();
        assert!(!repo_config_is_plain(&moved));
        std::fs::write(moved.join(".git"), "gitdir: inner\r\n").unwrap();
        assert!(repo_config_is_plain(&moved));
        std::fs::write(moved.join("inner/commondir"), "../../repo/.git\n").unwrap();
        assert!(!repo_config_is_plain(&moved));
    }

    #[test]
    fn a_git_file_naming_anything_else_is_refused() {
        let (_tmp, root, repo, wt) = worktree_fixture();
        let admin = repo.join(".git/worktrees/wt");
        let refused = |name: &str, text: &str| {
            let dir = root.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(".git"), text).unwrap();
            assert!(!repo_config_is_plain(&dir), "{name}");
        };
        refused(
            "other",
            &format!("gitdir: {}\n", repo.join(".git").display()),
        );
        refused("up", "gitdir: ../repo/.git\n");
        refused("stranger", &format!("gitdir: {}\n", admin.display()));
        refused(
            "dots",
            &format!(
                "gitdir: {}/x/../../repo/.git\n",
                root.join("dots").display()
            ),
        );
        refused("empty", "gitdir: \n");
        refused("nul", &format!("gitdir: {}\0\n", admin.display()));
        refused(
            "two-lines",
            &format!("gitdir: {}\n{}\n", admin.display(), admin.display()),
        );

        std::os::unix::fs::symlink(&repo, root.join("link")).unwrap();
        std::fs::write(
            wt.join(".git"),
            format!(
                "gitdir: {}\n",
                root.join("link/.git/worktrees/wt").display()
            ),
        )
        .unwrap();
        assert!(!repo_config_is_plain(&wt));

        let linked = root.join("linked");
        std::fs::create_dir_all(&linked).unwrap();
        std::os::unix::fs::symlink(wt.join(".git"), linked.join(".git")).unwrap();
        assert!(!repo_config_is_plain(&linked));
    }

    #[test]
    fn a_repository_with_a_large_file_filter_still_answers() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = std::fs::canonicalize(tmp.path()).unwrap();
        init_repo(&repo);
        git(
            Some(&repo),
            &["config", "filter.lfs.clean", "git-lfs clean -- %f"],
        )
        .unwrap();
        git(Some(&repo), &["config", "filter.lfs.required", "true"]).unwrap();
        let head = git_query(&repo, &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(head.len(), 40);
        assert_eq!(branch_tip(&repo, "main").unwrap().as_deref(), Some(&*head));
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
    fn a_lead_squashes_imported_worker_commits_through_alternates_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = root.join("proj");
        std::fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        let r = repo.to_string_lossy().into_owned();
        let base = git(Some(&repo), &["rev-parse", "HEAD"]).unwrap();
        let team = clone_dir(&r, "q_team").unwrap();
        let lead = lead_dir(&team);
        let mut tips = Vec::new();
        for k in 1..=2u8 {
            let w = worker_dir(&team, k);
            let branch = worker_branch("q_team", k);
            GitClone
                .prepare_member(&r, "q_team", &w.to_string_lossy(), &branch)
                .unwrap();
            commit_file(&w, &format!("w{k}.txt"));
            commit_file(&w, &format!("w{k}b.txt"));
            tips.push(import_branch(&r, &w, &branch).unwrap());
        }
        GitClone
            .prepare_member(&r, "q_team", &lead.to_string_lossy(), "queue/q_team")
            .unwrap();
        for (k, tip) in tips.iter().enumerate() {
            run_git(&lead, &["merge", "--squash", tip]);
            run_git(&lead, &["commit", "-q", "-m", &format!("worker {}", k + 1)]);
        }
        for f in ["w1.txt", "w1b.txt", "w2.txt", "w2b.txt"] {
            assert!(lead.join(f).is_file(), "{f}");
        }
        let head = git(Some(&lead), &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(history_facts(&lead, &base, &head).unwrap(), (true, 0));
        let count = git(
            Some(&lead),
            &["rev-list", "--count", &format!("{base}..{head}")],
        )
        .unwrap();
        assert_eq!(count, "2");
    }

    #[test]
    fn only_the_lead_folder_of_a_queue_run_is_a_team_lead_clone() {
        assert!(is_team_lead_clone(Path::new(
            "/w/proj-worktrees/queue/q_1/lead"
        )));
        for other in [
            "/w/proj-worktrees/queue/q_1/w1",
            "/w/proj-worktrees/queue/q_1",
            "/w/proj-worktrees/lead",
            "/w/proj/queue/q_1/lead",
            "/w/proj-worktrees/other/q_1/lead",
        ] {
            assert!(!is_team_lead_clone(Path::new(other)), "{other}");
        }
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

    fn repo_with_origin(root: &Path, name: &str) -> String {
        let repo = root.join(name);
        std::fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        let bare = root.join(format!("{name}-origin.git"));
        run_git(root, &["init", "-q", "--bare", &bare.to_string_lossy()]);
        run_git(&repo, &["remote", "add", "origin", &bare.to_string_lossy()]);
        run_git(&repo, &["push", "-q", "origin", "main"]);
        repo.to_string_lossy().into_owned()
    }

    fn push_branch_commit(repo: &str, branch: &str, file: &str) -> String {
        let repo = Path::new(repo);
        run_git(repo, &["checkout", "-q", "-B", branch]);
        commit_file(repo, file);
        run_git(repo, &["push", "-q", "-f", "origin", branch]);
        let sha = git(Some(repo), &["rev-parse", "HEAD"]).unwrap();
        run_git(repo, &["checkout", "-q", "main"]);
        sha
    }

    #[test]
    fn a_continuation_clone_starts_at_the_saved_commit_on_the_task_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = repo_with_origin(&root, "proj");
        let wip = push_branch_commit(&repo, "tmgr/tm-1-fix", "wip.txt");
        let dest = clone_dir(&repo, "q_c1").unwrap();
        let heads = BTreeMap::from([(String::new(), wip.clone())]);
        GitClone
            .prepare_continue(
                &repo,
                "q_c1",
                &dest.to_string_lossy(),
                &[],
                "tmgr/tm-1-fix",
                &heads,
            )
            .unwrap();
        assert_eq!(git(Some(&dest), &["rev-parse", "HEAD"]).unwrap(), wip);
        assert_eq!(
            git(Some(&dest), &["symbolic-ref", "--short", "HEAD"]).unwrap(),
            "tmgr/tm-1-fix"
        );
        assert!(dest.join("wip.txt").is_file());
    }

    #[test]
    fn a_branch_that_moved_or_vanished_on_the_remote_is_refused_and_leaves_no_clone() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = repo_with_origin(&root, "proj");
        let wip = push_branch_commit(&repo, "tmgr/tm-1-fix", "wip.txt");
        let other = push_branch_commit(&repo, "tmgr/tm-1-fix", "other.txt");
        assert_ne!(wip, other);
        let dest = clone_dir(&repo, "q_c2").unwrap();
        let heads = BTreeMap::from([(String::new(), wip)]);
        let err = GitClone
            .prepare_continue(
                &repo,
                "q_c2",
                &dest.to_string_lossy(),
                &[],
                "tmgr/tm-1-fix",
                &heads,
            )
            .unwrap_err();
        assert!(err.message.contains("not at the commit"), "{}", err.message);
        assert!(!dest.exists());
        let gone = BTreeMap::from([(String::new(), other)]);
        let dest = clone_dir(&repo, "q_c3").unwrap();
        assert!(GitClone
            .prepare_continue(
                &repo,
                "q_c3",
                &dest.to_string_lossy(),
                &[],
                "tmgr/tm-2-none",
                &gone,
            )
            .is_err());
        assert!(!dest.exists());
    }

    #[test]
    fn a_multi_repository_continuation_resumes_pushed_repos_and_starts_the_rest_fresh() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let a = repo_with_origin(&root, "alpha");
        let b = repo_with_origin(&root, "beta");
        let wip = push_branch_commit(&a, "tmgr/tm-3-x", "wip.txt");
        let dest = clone_dir(&a, "q_c4").unwrap();
        let clones = vec![("alpha".to_string(), a.clone()), ("beta".to_string(), b)];
        let heads = BTreeMap::from([("alpha".to_string(), wip.clone())]);
        GitClone
            .prepare_continue(
                &a,
                "q_c4",
                &dest.to_string_lossy(),
                &clones,
                "tmgr/tm-3-x",
                &heads,
            )
            .unwrap();
        assert_eq!(
            git(Some(&dest.join("alpha")), &["rev-parse", "HEAD"]).unwrap(),
            wip
        );
        let beta = dest.join("beta");
        assert_eq!(
            git(Some(&beta), &["symbolic-ref", "--short", "HEAD"]).unwrap(),
            "tmgr/tm-3-x"
        );
        assert!(!beta.join("wip.txt").exists());
        let moved = push_branch_commit(&a, "tmgr/tm-3-x", "more.txt");
        assert_ne!(moved, wip);
        let dest = clone_dir(&a, "q_c5").unwrap();
        assert!(GitClone
            .prepare_continue(
                &a,
                "q_c5",
                &dest.to_string_lossy(),
                &clones,
                "tmgr/tm-3-x",
                &heads,
            )
            .is_err());
        assert!(!dest.exists());
    }

    #[test]
    fn a_saved_commit_whose_remote_branch_vanished_is_refused_never_cloned_fresh() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let a = repo_with_origin(&root, "alpha");
        let b = repo_with_origin(&root, "beta");
        let wip_a = push_branch_commit(&a, "tmgr/tm-6-x", "wip.txt");
        let wip_b = push_branch_commit(&b, "tmgr/tm-6-x", "wip.txt");
        run_git(
            Path::new(&b),
            &["push", "-q", "origin", "--delete", "tmgr/tm-6-x"],
        );
        let clones = vec![("alpha".to_string(), a.clone()), ("beta".to_string(), b)];
        let heads = BTreeMap::from([("alpha".to_string(), wip_a), ("beta".to_string(), wip_b)]);
        let dest = clone_dir(&a, "q_c6").unwrap();
        let err = GitClone
            .prepare_continue(
                &a,
                "q_c6",
                &dest.to_string_lossy(),
                &clones,
                "tmgr/tm-6-x",
                &heads,
            )
            .unwrap_err();
        assert!(err.message.contains("not at the commit"), "{}", err.message);
        assert!(!dest.exists());
    }

    fn shared_objects(dir: &Path) -> Vec<PathBuf> {
        use std::os::unix::fs::MetadataExt;
        let mut out = Vec::new();
        let mut todo = vec![dir.join(".git/objects")];
        while let Some(d) = todo.pop() {
            for e in std::fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                let m = std::fs::symlink_metadata(&p).unwrap();
                if m.is_dir() {
                    todo.push(p);
                } else if m.is_file() && m.nlink() > 1 {
                    out.push(p);
                }
            }
        }
        out
    }

    fn objects_by_kind(dir: &Path) -> (usize, usize) {
        let mut loose = 0;
        let mut packed = 0;
        for e in std::fs::read_dir(dir.join(".git/objects")).unwrap() {
            let p = e.unwrap().path();
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            if name.len() == 2 {
                loose += std::fs::read_dir(&p).unwrap().count();
            } else if name == "pack" {
                packed += std::fs::read_dir(&p)
                    .unwrap()
                    .filter(|e| {
                        e.as_ref()
                            .unwrap()
                            .path()
                            .extension()
                            .is_some_and(|x| x == "pack")
                    })
                    .count();
            }
        }
        (loose, packed)
    }

    #[test]
    fn a_clone_shares_no_object_files_with_its_source() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = std::fs::canonicalize(tmp.path()).unwrap().join("proj");
        std::fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        run_git(&repo, &["gc", "-q"]);
        std::fs::write(repo.join("later.txt"), "later\n").unwrap();
        run_git(&repo, &["add", "."]);
        run_git(&repo, &["commit", "-q", "-m", "later"]);
        let (loose, packed) = objects_by_kind(&repo);
        assert!(loose > 0 && packed > 0);
        let r = repo.to_string_lossy().into_owned();
        let dest = clone_dir(&r, "q_nl").unwrap();
        GitClone
            .prepare(&r, "q_nl", &dest.to_string_lossy())
            .unwrap();
        assert!(dest.join(".git/objects/info/alternates").is_file());
        assert_eq!(shared_objects(&dest), Vec::<PathBuf>::new());
        assert_eq!(shared_objects(&repo), Vec::<PathBuf>::new());
        let log = git(Some(&dest), &["log", "--format=%s", "-n", "5"]).unwrap();
        assert_eq!(log, "later\ninit");
        assert!(dest.join("later.txt").is_file());
    }

    #[test]
    fn a_clone_works_from_a_source_that_has_alternates() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let base = root.join("base");
        std::fs::create_dir(&base).unwrap();
        init_repo(&base);
        let source = root.join("proj");
        run_git(
            &root,
            &[
                "clone",
                "-q",
                "--reference",
                base.to_str().unwrap(),
                base.to_str().unwrap(),
                "proj",
            ],
        );
        assert!(source.join(".git/objects/info/alternates").is_file());
        let r = source.to_string_lossy().into_owned();
        let dest = clone_dir(&r, "q_alt").unwrap();
        GitClone
            .prepare(&r, "q_alt", &dest.to_string_lossy())
            .unwrap();
        assert_eq!(shared_objects(&dest), Vec::<PathBuf>::new());
        assert!(dest.join("README.md").is_file());
        git(Some(&dest), &["fsck", "--connectivity-only"]).unwrap();
    }

    #[test]
    fn a_clone_works_from_a_repository_without_packs() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = std::fs::canonicalize(tmp.path()).unwrap().join("proj");
        std::fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        assert_eq!(objects_by_kind(&repo).1, 0);
        let r = repo.to_string_lossy().into_owned();
        let dest = clone_dir(&r, "q_np").unwrap();
        GitClone
            .prepare(&r, "q_np", &dest.to_string_lossy())
            .unwrap();
        assert_eq!(shared_objects(&dest), Vec::<PathBuf>::new());
        assert_eq!(
            git(Some(&dest), &["rev-list", "--count", "HEAD"]).unwrap(),
            "1"
        );
        std::fs::write(dest.join("new.txt"), "n\n").unwrap();
        run_git(&dest, &["add", "."]);
        run_git(&dest, &["commit", "-q", "-m", "next"]);
        assert_eq!(
            git(Some(&dest), &["rev-list", "--count", "HEAD"]).unwrap(),
            "2"
        );
    }

    fn within<T: Send + 'static>(secs: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(std::time::Duration::from_secs(secs))
            .expect("did not return in time")
    }

    fn sh_cmd(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.args(["-c", script]);
        c
    }

    #[test]
    fn bounded_runner_kills_a_command_that_runs_too_long() {
        let started = std::time::Instant::now();
        let r = within(5, || {
            output_bounded(
                &mut sh_cmd("sleep 30"),
                std::time::Duration::from_millis(300),
                1 << 20,
            )
        });
        assert!(r.is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(4));
    }

    #[test]
    fn bounded_runner_stops_a_command_that_prints_too_much() {
        let r = within(5, || {
            output_bounded(
                &mut Command::new("yes"),
                std::time::Duration::from_secs(30),
                64 << 10,
            )
        });
        assert!(r.unwrap_err().message.contains("more than"));
    }

    #[test]
    fn bounded_runner_does_not_wait_for_a_grandchild_holding_the_pipe() {
        let started = std::time::Instant::now();
        let _ = within(5, || {
            output_bounded(
                &mut sh_cmd("(sleep 30 &) ; echo hi"),
                std::time::Duration::from_millis(500),
                1 << 20,
            )
        });
        assert!(started.elapsed() < std::time::Duration::from_secs(4));
    }

    #[test]
    fn bounded_runner_returns_the_output_of_a_small_command() {
        let out = within(5, || {
            output_bounded(
                &mut sh_cmd("printf out; printf err >&2"),
                std::time::Duration::from_secs(5),
                1 << 20,
            )
        })
        .unwrap();
        assert!(out.status.success());
        assert_eq!((out.stdout, out.stderr), (b"out".to_vec(), b"err".to_vec()));
    }

    fn copy_with_reflog(lines: &str) -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = std::fs::canonicalize(tmp.path()).unwrap();
        run_git(&dir, &["init", "-q", "-b", "tmgr/w1"]);
        run_git(&dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
        std::fs::remove_dir_all(dir.join(".git/logs")).unwrap();
        std::fs::create_dir_all(dir.join(".git/logs")).unwrap();
        std::fs::write(dir.join(".git/logs/HEAD"), lines).unwrap();
        (tmp, dir)
    }

    #[test]
    fn copy_refs_lists_commits_named_in_the_reflog() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        let zero = "0".repeat(40);
        let (_t, dir) = copy_with_reflog(&format!(
            "{zero} {a} t <t@example.com> 1 +0000\tcommit: x\n\n{a} {b} t <t@example.com> 2 +0000\treset: y\n"
        ));
        let refs = copy_refs(&dir, "tmgr/w1").unwrap();
        assert_eq!(refs.logged, [a, b]);
    }

    #[test]
    fn copy_refs_refuse_a_malformed_or_unreadable_reflog() {
        let a = "a".repeat(40);
        let (_t, dir) = copy_with_reflog(&format!("{a} zzz t\tx\n"));
        assert!(copy_refs(&dir, "tmgr/w1").is_err());
        let (_t, dir) = copy_with_reflog("");
        std::fs::remove_file(dir.join(".git/logs/HEAD")).unwrap();
        assert!(copy_refs(&dir, "tmgr/w1").unwrap().logged.is_empty());
        std::fs::create_dir_all(dir.join(".git/logs/refs/heads")).unwrap();
        std::os::unix::fs::symlink("/etc/hosts", dir.join(".git/logs/refs/heads/l")).unwrap();
        assert!(copy_refs(&dir, "tmgr/w1").is_err());
        std::fs::remove_file(dir.join(".git/logs/refs/heads/l")).unwrap();
        std::fs::write(
            dir.join(".git/logs/refs/heads/big"),
            vec![b'x'; (1 << 20) + 1],
        )
        .unwrap();
        assert!(copy_refs(&dir, "tmgr/w1").is_err());
    }

    #[test]
    fn copy_refs_without_a_log_folder_has_no_logged_commits() {
        let (_t, dir) = copy_with_reflog("");
        std::fs::remove_dir_all(dir.join(".git/logs")).unwrap();
        assert!(copy_refs(&dir, "tmgr/w1").unwrap().logged.is_empty());
    }

    #[test]
    fn alternates_check_refuses_links_and_odd_files_in_the_object_store() {
        let (_t, dir) = copy_with_reflog("");
        let allowed = dir.join("elsewhere");
        assert!(copy_alternates_ok(&dir, &allowed));
        let objects = dir.join(".git/objects");
        let fan = std::fs::read_dir(&objects)
            .unwrap()
            .flatten()
            .find(|e| e.file_name().len() == 2)
            .unwrap()
            .path();
        let moved = dir.join("moved");
        std::fs::rename(&fan, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &fan).unwrap();
        assert!(!copy_alternates_ok(&dir, &allowed));
        std::fs::remove_file(&fan).unwrap();
        std::fs::rename(&moved, &fan).unwrap();
        assert!(copy_alternates_ok(&dir, &allowed));
        let object = std::fs::read_dir(&fan)
            .unwrap()
            .flatten()
            .next()
            .unwrap()
            .path();
        let kept = dir.join("kept");
        std::fs::rename(&object, &kept).unwrap();
        std::os::unix::fs::symlink(&kept, &object).unwrap();
        assert!(!copy_alternates_ok(&dir, &allowed));
        std::fs::remove_file(&object).unwrap();
        std::fs::rename(&kept, &object).unwrap();
        let fifo = std::ffi::CString::new(fan.join("ff").to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert!(!copy_alternates_ok(&dir, &allowed));
    }

    fn host_run(repo: &Path, host: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(repo)
            .env_clear()
            .env("PATH", absolute_path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_DIR", host)
            .env("GIT_WORK_TREE", repo)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn hostile_config(marker: &Path) -> String {
        format!(
            "[filter \"x\"]\n\tclean = touch {0}\n\tsmudge = touch {0}\n[core]\n\tfsmonitor = touch {0}\n\tpager = touch {0}\n[diff \"x\"]\n\ttextconv = touch {0}\n",
            marker.display()
        )
    }

    fn append(path: &Path, text: &str) {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        std::io::Write::write_all(&mut file, text.as_bytes()).unwrap();
    }

    fn repo_at(root: &Path, name: &str) -> PathBuf {
        let repo = root.join(name);
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        repo
    }

    fn canon_tmp() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        (tmp, root)
    }

    fn prepared(repo: &Path) -> (tempfile::TempDir, PathBuf) {
        let host = tempfile::tempdir().unwrap();
        let path = host.path().to_path_buf();
        assert!(host_git_dir(repo, &path).is_some());
        (host, path)
    }

    #[test]
    fn a_config_rewritten_after_the_private_dir_is_built_never_reaches_git() {
        let (_tmp, root) = canon_tmp();
        let repo = repo_at(&root, "repo");
        let marker = root.join("ran");
        let (_host, host) = prepared(&repo);
        append(&repo.join(".git/config"), &hostile_config(&marker));
        std::fs::write(repo.join(".gitattributes"), "*.md filter=x diff=x\n").unwrap();
        std::fs::write(repo.join("README.md"), "changed after the check\n").unwrap();
        let status = host_run(&repo, &host, &["status", "--porcelain"]);
        assert!(status.contains("README.md"), "{status}");
        let numstat = host_run(&repo, &host, &["diff", "HEAD", "--numstat"]);
        assert!(numstat.contains("README.md"), "{numstat}");
        assert!(!marker.exists());
    }

    #[test]
    fn a_commondir_added_after_the_private_dir_is_built_never_reaches_git() {
        let (_tmp, root) = canon_tmp();
        let repo = repo_at(&root, "repo");
        let other = repo_at(&root, "other");
        let marker = root.join("ran");
        append(&other.join(".git/config"), &hostile_config(&marker));
        let (_host, host) = prepared(&repo);
        std::fs::write(
            repo.join(".git/commondir"),
            format!("{}\n", other.join(".git").display()),
        )
        .unwrap();
        std::fs::write(repo.join(".gitattributes"), "*.md filter=x\n").unwrap();
        std::fs::write(repo.join("README.md"), "changed\n").unwrap();
        let status = host_run(&repo, &host, &["status", "--porcelain"]);
        assert!(status.contains("README.md"), "{status}");
        host_run(&repo, &host, &["diff", "HEAD", "--numstat"]);
        assert!(!marker.exists());
    }

    #[test]
    fn a_swapped_dot_git_after_the_private_dir_is_built_never_reaches_git() {
        let (_tmp, root) = canon_tmp();
        let repo = repo_at(&root, "repo");
        let marker = root.join("ran");
        let (_host, host) = prepared(&repo);
        std::fs::rename(repo.join(".git"), root.join("real-git")).unwrap();
        std::fs::create_dir(repo.join(".git")).unwrap();
        std::os::unix::fs::symlink(root.join("real-git/objects"), repo.join(".git/objects"))
            .unwrap();
        std::fs::write(repo.join(".git/config"), hostile_config(&marker)).unwrap();
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(repo.join(".gitattributes"), "*.md filter=x\n").unwrap();
        std::fs::write(repo.join("README.md"), "changed\n").unwrap();
        let status = host_run(&repo, &host, &["status", "--porcelain"]);
        assert!(status.contains("README.md"), "{status}");
        host_run(&repo, &host, &["diff", "HEAD", "--numstat"]);
        assert!(!marker.exists());
    }

    #[test]
    fn a_config_that_runs_a_program_is_refused_before_the_private_dir_is_built() {
        for (key, value) in [
            ("filter.x.clean", "touch m"),
            ("filter.x.process", "touch m"),
            ("include.path", "elsewhere"),
            ("includeIf.gitdir:/.path", "elsewhere"),
            ("core.pager", "touch m"),
            ("core.sshCommand", "touch m"),
            ("core.editor", "touch m"),
            ("diff.x.textconv", "touch m"),
            ("diff.external", "touch m"),
            ("alias.st", "!touch m"),
            ("credential.helper", "!touch m"),
            ("url.x.insteadOf", "y"),
        ] {
            let (_tmp, root) = canon_tmp();
            let repo = repo_at(&root, "repo");
            git(Some(&repo), &["config", key, value]).unwrap();
            let host = tempfile::tempdir().unwrap();
            assert!(host_git_dir(&repo, host.path()).is_none(), "{key}");
            assert!(!repo_config_is_plain(&repo), "{key}");
        }
    }

    #[test]
    fn a_config_that_is_a_fifo_is_refused_without_blocking() {
        let (_tmp, root) = canon_tmp();
        let repo = repo_at(&root, "repo");
        let config = repo.join(".git/config");
        std::fs::remove_file(&config).unwrap();
        assert!(Command::new("mkfifo")
            .arg(&config)
            .status()
            .unwrap()
            .success());
        let refused = within(10, move || {
            let host = tempfile::tempdir().unwrap();
            host_git_dir(&repo, host.path()).is_none()
        });
        assert!(refused);
    }

    #[test]
    fn a_config_that_is_a_symlink_is_refused() {
        let (_tmp, root) = canon_tmp();
        let repo = repo_at(&root, "repo");
        let real = root.join("elsewhere.config");
        std::fs::copy(repo.join(".git/config"), &real).unwrap();
        std::fs::remove_file(repo.join(".git/config")).unwrap();
        std::os::unix::fs::symlink(&real, repo.join(".git/config")).unwrap();
        let host = tempfile::tempdir().unwrap();
        assert!(host_git_dir(&repo, host.path()).is_none());
    }

    #[test]
    fn a_config_value_with_quotes_and_backslashes_round_trips() {
        let (_tmp, root) = canon_tmp();
        let repo = repo_at(&root, "repo");
        let url = r#"https://example.com/a"b\c d"#;
        git(Some(&repo), &["config", "remote.origin.url", url]).unwrap();
        let (_host, host) = prepared(&repo);
        let read = host_run(
            &repo,
            &host,
            &[
                "config",
                "--file",
                host.join("config").to_str().unwrap(),
                "--get",
                "remote.origin.url",
            ],
        );
        assert_eq!(read.trim_end_matches('\n'), url);
    }

    #[test]
    fn a_config_value_with_a_control_character_is_refused() {
        let (_tmp, root) = canon_tmp();
        let repo = repo_at(&root, "repo");
        git(Some(&repo), &["config", "remote.origin.url", "a\tb"]).unwrap();
        let host = tempfile::tempdir().unwrap();
        assert!(host_git_dir(&repo, host.path()).is_none());
    }

    #[test]
    fn only_a_few_inert_settings_are_carried_into_the_private_config() {
        let (_tmp, root) = canon_tmp();
        let repo = repo_at(&root, "repo");
        for (key, value) in [
            ("remote.origin.url", "file:///x"),
            ("branch.main.remote", "origin"),
            ("branch.main.merge", "refs/heads/main"),
            ("core.hooksPath", "/tmp/hooks"),
            ("core.fsmonitor", "true"),
            ("core.abbrev", "12"),
            ("extensions.worktreeConfig", "true"),
            ("user.name", "someone"),
            ("user.email", "someone@example.com"),
            ("commit.gpgsign", "true"),
            ("pull.rebase", "true"),
        ] {
            git(Some(&repo), &["config", key, value]).unwrap();
        }
        let (_host, host) = prepared(&repo);
        let text = std::fs::read_to_string(host.join("config")).unwrap();
        let keys = host_run(
            &repo,
            &host,
            &[
                "config",
                "--file",
                host.join("config").to_str().unwrap(),
                "--list",
                "--name-only",
            ],
        );
        for key in keys.lines() {
            assert!(
                matches!(
                    key,
                    "core.repositoryformatversion"
                        | "core.bare"
                        | "core.filemode"
                        | "core.ignorecase"
                        | "core.precomposeunicode"
                        | "remote.origin.url"
                        | "branch.main.remote"
                        | "branch.main.merge"
                ),
                "{key}\n{text}"
            );
        }
        for banned in [
            "hookspath",
            "fsmonitor",
            "worktreeconfig",
            "user",
            "gpgsign",
        ] {
            assert!(!text.to_ascii_lowercase().contains(banned), "{banned}");
        }
        assert!(keys.lines().any(|k| k == "remote.origin.url"));
        assert!(keys.lines().any(|k| k == "branch.main.merge"));
    }

    #[test]
    fn a_detached_head_works_through_the_private_dir() {
        let (_tmp, root) = canon_tmp();
        let repo = repo_at(&root, "repo");
        git(Some(&repo), &["checkout", "-q", "--detach"]).unwrap();
        let sha = git(Some(&repo), &["rev-parse", "HEAD"]).unwrap();
        let (_host, host) = prepared(&repo);
        assert_eq!(host_run(&repo, &host, &["rev-parse", "HEAD"]).trim(), sha);
        host_run(&repo, &host, &["status", "--porcelain"]);
    }

    #[test]
    fn remote_tracking_refs_symbolic_and_packed_survive_in_the_private_dir() {
        let (_tmp, root) = canon_tmp();
        let repo = repo_at(&root, "repo");
        let sha = git(Some(&repo), &["rev-parse", "HEAD"]).unwrap();
        git(
            Some(&repo),
            &["update-ref", "refs/remotes/origin/main", "HEAD"],
        )
        .unwrap();
        git(
            Some(&repo),
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        )
        .unwrap();
        let (_a, loose) = prepared(&repo);
        assert_eq!(
            host_run(&repo, &loose, &["symbolic-ref", "refs/remotes/origin/HEAD"]).trim(),
            "refs/remotes/origin/main"
        );
        git(Some(&repo), &["pack-refs", "--all"]).unwrap();
        let (_b, packed) = prepared(&repo);
        for name in ["main", "refs/remotes/origin/main", "origin/HEAD"] {
            assert_eq!(
                host_run(&repo, &packed, &["rev-parse", name]).trim(),
                sha,
                "{name}"
            );
        }
        assert_eq!(
            host_run(
                &repo,
                &packed,
                &["symbolic-ref", "refs/remotes/origin/HEAD"]
            )
            .trim(),
            "refs/remotes/origin/main"
        );
    }

    #[test]
    fn a_shallow_file_with_anything_but_commit_ids_is_refused() {
        let (_tmp, root) = canon_tmp();
        let repo = repo_at(&root, "repo");
        let sha = git(Some(&repo), &["rev-parse", "HEAD"]).unwrap();
        std::fs::write(repo.join(".git/shallow"), format!("{sha}\n")).unwrap();
        let host = tempfile::tempdir().unwrap();
        assert!(host_git_dir(&repo, host.path()).is_some());
        std::fs::write(
            repo.join(".git/shallow"),
            format!("{sha}\n../../etc/passwd\n"),
        )
        .unwrap();
        let host = tempfile::tempdir().unwrap();
        assert!(host_git_dir(&repo, host.path()).is_none());
    }

    fn enable_worktree_config(repo: &Path) {
        git(Some(repo), &["config", "core.repositoryformatversion", "1"]).unwrap();
        git(Some(repo), &["config", "extensions.worktreeConfig", "true"]).unwrap();
    }

    fn linked(repo: &Path, root: &Path) -> PathBuf {
        let wt = root.join("wt");
        git(
            Some(repo),
            &["worktree", "add", "-q", "--detach", wt.to_str().unwrap()],
        )
        .unwrap();
        wt
    }

    #[test]
    fn a_repository_with_worktree_config_and_its_linked_worktree_are_plain() {
        let (_tmp, root) = canon_tmp();
        let repo = repo_at(&root, "repo");
        enable_worktree_config(&repo);
        assert!(repo_config_is_plain(&repo));
        let wt = linked(&repo, &root);
        assert!(repo_config_is_plain(&wt));
    }

    #[test]
    fn a_hostile_worktree_config_makes_the_main_and_the_linked_worktree_not_plain() {
        for hostile in [
            "[filter \"x\"]\n\tclean = touch m\n",
            "[core]\n\tpager = touch m\n",
        ] {
            let (_tmp, root) = canon_tmp();
            let repo = repo_at(&root, "repo");
            enable_worktree_config(&repo);
            let wt = linked(&repo, &root);
            assert!(repo_config_is_plain(&repo));
            assert!(repo_config_is_plain(&wt));
            std::fs::write(repo.join(".git/config.worktree"), hostile).unwrap();
            assert!(!repo_config_is_plain(&repo), "{hostile}");
            assert!(repo_config_is_plain(&wt), "{hostile}");
            std::fs::write(repo.join(".git/worktrees/wt/config.worktree"), hostile).unwrap();
            assert!(!repo_config_is_plain(&wt), "{hostile}");
        }
    }

    #[test]
    fn an_allowed_setting_in_the_worktree_config_stays_plain() {
        let (_tmp, root) = canon_tmp();
        let repo = repo_at(&root, "repo");
        enable_worktree_config(&repo);
        let wt = linked(&repo, &root);
        let text = "[user]\n\tname = someone\n";
        std::fs::write(repo.join(".git/config.worktree"), text).unwrap();
        std::fs::write(repo.join(".git/worktrees/wt/config.worktree"), text).unwrap();
        assert!(repo_config_is_plain(&repo));
        assert!(repo_config_is_plain(&wt));
    }

    #[test]
    fn a_worktree_config_fsmonitor_never_runs_through_the_private_dir() {
        let (_tmp, root) = canon_tmp();
        let repo = repo_at(&root, "repo");
        enable_worktree_config(&repo);
        let marker = root.join("ran");
        std::fs::write(
            repo.join(".git/config.worktree"),
            format!("[core]\n\tfsmonitor = touch {}\n", marker.display()),
        )
        .unwrap();
        let host = tempfile::tempdir().unwrap();
        if host_git_dir(&repo, host.path()).is_some() {
            host_run(&repo, host.path(), &["status", "--porcelain"]);
            host_run(&repo, host.path(), &["diff", "HEAD", "--numstat"]);
        }
        assert!(!marker.exists());
    }

    #[test]
    fn a_worktree_config_is_ignored_while_the_extension_is_off() {
        let (_tmp, root) = canon_tmp();
        let repo = repo_at(&root, "repo");
        let marker = root.join("ran");
        std::fs::write(repo.join(".git/config.worktree"), hostile_config(&marker)).unwrap();
        std::fs::write(repo.join(".gitattributes"), "*.md filter=x\n").unwrap();
        std::fs::write(repo.join("README.md"), "changed\n").unwrap();
        assert!(repo_config_is_plain(&repo));
        let (_host, host) = prepared(&repo);
        let text = std::fs::read_to_string(host.join("config")).unwrap();
        assert!(
            !text.contains("worktree") && !text.contains("filter"),
            "{text}"
        );
        host_run(&repo, &host, &["status", "--porcelain"]);
        host_run(&repo, &host, &["diff", "HEAD", "--numstat"]);
        assert!(!marker.exists());
    }

    #[test]
    fn relative_worktree_paths_setting_is_plain() {
        let (_tmp, root) = canon_tmp();
        let repo = repo_at(&root, "repo");
        git(
            Some(&repo),
            &["config", "worktree.useRelativePaths", "true"],
        )
        .unwrap();
        assert!(repo_config_is_plain(&repo));
    }
}
