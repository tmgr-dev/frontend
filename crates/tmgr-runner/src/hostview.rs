use crate::worktree::{absolute_path_entries, run_bounded, Limits, FACTS, HARDENING, PLUMBING};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Read;
use std::os::unix::fs::{symlink, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;
use tempfile::TempDir;

const MAX_CONFIG: u64 = 1024 * 1024;
const MAX_SMALL: u64 = 64 * 1024;
const MAX_INDEX: u64 = 256 * 1024 * 1024;
const CONFIG_LIST: Limits = Limits {
    deadline: Duration::from_secs(30),
    max_stdout: 4 * 1024 * 1024,
    max_stderr: 64 * 1024,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
    Network,
}

pub struct HostView {
    _p: TempDir,
    dir: PathBuf,
    repo: PathBuf,
    common: PathBuf,
    access: Access,
    gitdir: PathBuf,
    work_tree: bool,
    limits: Limits,
    offline: bool,
    origin: Option<String>,
    keys: Vec<String>,
    sha256: bool,
    deferred: Option<String>,
    path: OsString,
}

struct Layout {
    root: PathBuf,
    gitdir: PathBuf,
    common: PathBuf,
}

fn open_data(path: &Path) -> std::io::Result<fs::File> {
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
}

fn read_data(path: &Path, max: u64) -> Result<Option<Vec<u8>>, String> {
    let mut file = match open_data(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let meta = file
        .metadata()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    if meta.len() > max {
        return Err(format!("{} is larger than {max} bytes", path.display()));
    }
    let mut buf = Vec::new();
    (&mut file)
        .take(max + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if buf.len() as u64 > max {
        return Err(format!("{} is larger than {max} bytes", path.display()));
    }
    Ok(Some(buf))
}

fn plain_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
}

fn linked(root: &Path, dot: &Path) -> Result<Layout, String> {
    let text = read_data(dot, MAX_SMALL)?.unwrap_or_default();
    let text = String::from_utf8_lossy(&text);
    let target = text
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("gitdir:"))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| format!("{} does not name a git folder", dot.display()))?;
    let gitdir = fs::canonicalize(root.join(target))
        .map_err(|e| format!("{}: {e}", root.join(target).display()))?;
    if !plain_dir(&gitdir) {
        return Err(format!("{} is not a git folder", gitdir.display()));
    }
    let raw = read_data(&gitdir.join("commondir"), MAX_SMALL)?
        .ok_or_else(|| format!("{} is not a linked worktree folder", gitdir.display()))?;
    let rel = String::from_utf8_lossy(&raw).trim().to_string();
    let common = fs::canonicalize(gitdir.join(&rel)).map_err(|e| format!("{rel}: {e}"))?;
    if gitdir.parent() != Some(&common.join("worktrees")) {
        return Err(format!(
            "{} is not a linked worktree folder",
            gitdir.display()
        ));
    }
    let back = read_data(&gitdir.join("gitdir"), MAX_SMALL)?
        .map(|b| String::from_utf8_lossy(&b).trim().to_string())
        .and_then(|b| fs::canonicalize(gitdir.join(b)).ok());
    if back.is_none() || back != fs::canonicalize(dot).ok() {
        return Err(format!(
            "{} does not belong to this worktree",
            gitdir.display()
        ));
    }
    Ok(Layout {
        root: root.to_path_buf(),
        gitdir,
        common,
    })
}

fn resolve(repo: &Path) -> Result<Layout, String> {
    let dot = repo.join(".git");
    match fs::symlink_metadata(&dot) {
        Ok(m) if m.is_dir() => Ok(Layout {
            root: repo.to_path_buf(),
            gitdir: dot.clone(),
            common: dot,
        }),
        Ok(m) if m.is_file() => linked(repo, &dot),
        Ok(_) => Err(format!("{} is neither a folder nor a file", dot.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if repo.join("HEAD").is_file()
                && plain_dir(&repo.join("objects"))
                && plain_dir(&repo.join("refs"))
            {
                Ok(Layout {
                    root: repo.to_path_buf(),
                    gitdir: repo.to_path_buf(),
                    common: repo.to_path_buf(),
                })
            } else {
                Err(format!("not a git repository: {}", repo.display()))
            }
        }
        Err(e) => Err(format!("{}: {e}", dot.display())),
    }
}

fn is_bool(v: Option<&str>) -> bool {
    v.is_none_or(|v| {
        matches!(
            v.to_ascii_lowercase().as_str(),
            "true" | "false" | "yes" | "no" | "on" | "off" | "1" | "0"
        )
    })
}

fn be32(b: &[u8], at: usize) -> Option<usize> {
    let s = b.get(at..at + 4)?;
    Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]) as usize)
}

fn index_is_split(idx: &[u8], hash: usize) -> Option<bool> {
    if idx.get(..4)? != b"DIRC" {
        return None;
    }
    let version = be32(idx, 4)?;
    let count = be32(idx, 8)?;
    let end = idx.len().checked_sub(hash)?;
    let mut pos = 12;
    for _ in 0..count {
        let start = pos;
        let flags =
            u16::from_be_bytes([*idx.get(start + 40 + hash)?, *idx.get(start + 41 + hash)?]);
        pos = start + 42 + hash;
        if version >= 3 && flags & 0x4000 != 0 {
            pos += 2;
        }
        if version == 4 {
            while idx.get(pos)? & 0x80 != 0 {
                pos += 1;
            }
            pos += 1;
            pos += idx.get(pos..end)?.iter().position(|b| *b == 0)? + 1;
        } else {
            let name = idx.get(pos..end)?.iter().position(|b| *b == 0)?;
            pos = start + ((pos - start + name + 8) & !7);
        }
    }
    while pos + 8 <= end {
        if &idx[pos..pos + 4] == b"link" {
            return Some(true);
        }
        pos += 8 + be32(idx, pos + 4)?;
    }
    Some(false)
}

struct Snapshot {
    keys: Vec<String>,
    config: String,
    origin: Option<String>,
    sha256: bool,
}

fn snapshot(entries: Vec<(String, Option<String>)>) -> Result<Snapshot, String> {
    let mut version = 0i64;
    let mut object_format = None;
    let mut ref_storage = None;
    let mut relative = None;
    let mut origin = None;
    let mut core: Vec<(String, String)> = Vec::new();
    let mut keys = Vec::new();
    for (key, value) in &entries {
        keys.push(key.clone());
        let v = value.as_deref();
        match key.as_str() {
            "remote.origin.url" => {
                if origin.is_none() {
                    origin = value.clone();
                }
            }
            "core.repositoryformatversion" => {
                version = v
                    .and_then(|v| v.trim().parse().ok())
                    .filter(|n| (0..=1).contains(n))
                    .ok_or("repository format version is not supported")?;
            }
            "core.filemode" | "core.ignorecase" | "core.precomposeunicode" | "core.symlinks" => {
                if is_bool(v) {
                    core.retain(|(k, _)| k != key);
                    core.push((key.clone(), v.unwrap_or("true").to_string()));
                }
            }
            "core.autocrlf" | "core.eol" | "core.safecrlf" => {
                let ok: &[&str] = match key.as_str() {
                    "core.autocrlf" => &["true", "false", "input"],
                    "core.eol" => &["lf", "crlf", "native"],
                    _ => &["true", "false", "warn"],
                };
                if let Some(v) = v
                    .map(str::to_ascii_lowercase)
                    .filter(|v| ok.contains(&v.as_str()))
                {
                    core.retain(|(k, _)| k != key);
                    core.push((key.clone(), v));
                }
            }
            "extensions.objectformat" => match v {
                Some("sha1") | Some("sha256") => object_format = v.map(String::from),
                _ => return Err("repository object format is not supported".into()),
            },
            "extensions.refstorage" => match v {
                Some("files") => ref_storage = Some("files"),
                _ => return Err("repository ref storage is not supported".into()),
            },
            "extensions.relativeworktrees" => {
                if !is_bool(v) {
                    return Err("repository extension relativeworktrees is not supported".into());
                }
                relative = Some(v.unwrap_or("true").to_string());
            }
            "extensions.worktreeconfig" => {}
            k if k.starts_with("extensions.") => {
                return Err(format!(
                    "repository extension {} is not supported",
                    &k["extensions.".len()..]
                ));
            }
            _ => {}
        }
    }
    let ext = version == 1;
    let sha256 = ext && object_format.as_deref() == Some("sha256");
    let mut config = format!("[core]\n\trepositoryformatversion = {version}\n\tbare = false\n");
    for (k, v) in &core {
        config.push_str(&format!("\t{} = {v}\n", &k["core.".len()..]));
    }
    if ext && (sha256 || ref_storage.is_some() || relative.is_some()) {
        config.push_str("[extensions]\n");
        if sha256 {
            config.push_str("\tobjectformat = sha256\n");
        }
        if let Some(r) = ref_storage {
            config.push_str(&format!("\trefstorage = {r}\n"));
        }
        if let Some(r) = relative {
            config.push_str(&format!("\trelativeworktrees = {r}\n"));
        }
    }
    Ok(Snapshot {
        keys,
        config,
        origin,
        sha256,
    })
}

fn parse_list(raw: &[u8]) -> Vec<(String, Option<String>)> {
    raw.split(|b| *b == 0)
        .filter(|e| !e.is_empty())
        .map(|e| match e.iter().position(|b| *b == b'\n') {
            Some(i) => (
                String::from_utf8_lossy(&e[..i]).into_owned(),
                Some(String::from_utf8_lossy(&e[i + 1..]).into_owned()),
            ),
            None => (String::from_utf8_lossy(e).into_owned(), None),
        })
        .collect()
}

fn base_command(network: bool, path: &OsStr) -> Command {
    let mut cmd = Command::new("git");
    cmd.env_clear();
    for (k, v) in std::env::vars_os() {
        let key = k.to_string_lossy();
        if matches!(key.as_ref(), "HOME" | "LANG")
            || key.starts_with("LC_")
            || (network && key == "SSH_AUTH_SOCK")
        {
            cmd.env(k, v);
        }
    }
    cmd.env("PATH", path)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1");
    if !network {
        cmd.env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null");
    }
    for pin in HARDENING.chunks(2) {
        let value = pin[1];
        let skip = value == "protocol.file.allow=user"
            || (network
                && (value.starts_with("credential.helper") || value.starts_with("core.askPass")));
        if !skip {
            cmd.args(pin);
        }
    }
    cmd.stdin(Stdio::null());
    cmd
}

fn clean_path(guards: &[&Path]) -> OsString {
    clean_path_in(&std::env::var_os("PATH").unwrap_or_default(), guards)
}

fn clean_path_in(value: &OsStr, guards: &[&Path]) -> OsString {
    let abs = absolute_path_entries(value);
    let kept: Vec<PathBuf> = std::env::split_paths(&abs)
        .filter(|e| {
            let real = fs::canonicalize(e).unwrap_or_else(|_| e.clone());
            !guards.iter().any(|g| real.starts_with(g))
        })
        .collect();
    match std::env::join_paths(kept) {
        Ok(joined) if !joined.is_empty() => joined,
        _ => "/usr/bin:/bin".into(),
    }
}

fn config_command(scratch: &Path, copy: &Path, path: &OsStr) -> Command {
    let mut cmd = base_command(false, path);
    cmd.current_dir(scratch)
        .env(
            "GIT_CEILING_DIRECTORIES",
            scratch.parent().unwrap_or(scratch),
        )
        .args(["config", "--file"])
        .arg(copy)
        .args(["--no-includes", "--list", "-z"]);
    cmd
}

fn list_config(
    scratch: &Path,
    bytes: &[u8],
    l: Limits,
    path: &OsStr,
) -> Result<Vec<(String, Option<String>)>, String> {
    let copy = scratch.join("config.copy");
    fs::write(&copy, bytes).map_err(|e| e.to_string())?;
    let cmd = config_command(scratch, &copy, path);
    let out = run_bounded(cmd, l.deadline, l.max_stdout, l.max_stderr)
        .map_err(|e| format!("git config failed: {e}"))?;
    let _ = fs::remove_file(&copy);
    if !out.status.success() {
        return Err(format!(
            "repository config cannot be read: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(parse_list(&out.stdout))
}

fn writes(sub: &str, rest: &[&str]) -> bool {
    match sub {
        "fetch" | "ls-remote" | "update-ref" | "branch" | "tag" | "pack-refs" | "gc" | "repack"
        | "prune" | "maintenance" | "commit" | "commit-tree" | "write-tree" | "mktree"
        | "mktag" | "checkout" | "switch" | "restore" | "reset" | "merge" | "rebase"
        | "cherry-pick" | "revert" | "am" | "pull" | "stash" | "notes" | "replace" | "worktree"
        | "clone" | "remote" | "fast-import" | "unpack-objects" | "index-pack" | "fetch-pack"
        | "receive-pack" | "submodule" | "rm" | "mv" | "add" => true,
        "reflog" => rest
            .first()
            .is_some_and(|a| matches!(*a, "expire" | "delete")),
        "hash-object" => rest.contains(&"-w"),
        "symbolic-ref" => {
            rest.iter().any(|a| matches!(*a, "-d" | "--delete"))
                || rest.iter().filter(|a| !a.starts_with('-')).count() > 1
        }
        _ => false,
    }
}

fn remote_ok(url: &str) -> bool {
    if url.is_empty() || url.starts_with('-') || url.chars().any(char::is_control) {
        return false;
    }
    let host_ok = |authority: &str| {
        let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
        !host.is_empty() && !host.starts_with('-')
    };
    if let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("ssh://"))
    {
        return host_ok(rest.split('/').next().unwrap_or(""));
    }
    if url.contains("://") {
        return false;
    }
    match url.split_once(':') {
        Some((left, path)) if !left.contains('/') && left.contains('@') && !path.is_empty() => {
            host_ok(left)
        }
        _ => false,
    }
}

fn find_root(start: &Path) -> Result<PathBuf, String> {
    let start = fs::canonicalize(start).map_err(|e| format!("{}: {e}", start.display()))?;
    start
        .ancestors()
        .find(|d| fs::symlink_metadata(d.join(".git")).is_ok())
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("not a git repository: {}", start.display()))
}

impl HostView {
    pub fn open(repo: &Path, access: Access) -> Result<HostView, String> {
        HostView::open_in(repo, access, &std::env::temp_dir(), false)
    }

    pub fn open_facts(repo: &Path, access: Access) -> Result<HostView, String> {
        HostView::open_in(repo, access, &std::env::temp_dir(), true)
    }

    fn open_in(repo: &Path, access: Access, base: &Path, facts: bool) -> Result<HostView, String> {
        let repo = fs::canonicalize(repo).map_err(|e| format!("{}: {e}", repo.display()))?;
        let layout = resolve(&repo)?;
        if access != Access::Read && layout.gitdir != layout.common {
            return Err("only read views are supported on a linked worktree".into());
        }
        let p = tempfile::Builder::new()
            .prefix("tmgr-hostview-")
            .tempdir_in(base)
            .map_err(|e| format!("cannot make a private git folder: {e}"))?;
        let dir = fs::canonicalize(p.path()).map_err(|e| e.to_string())?;
        for guarded in [&layout.root, &layout.common, &layout.gitdir] {
            if dir.starts_with(guarded) {
                return Err(format!(
                    "the temp folder {} is inside the repository",
                    dir.display()
                ));
            }
        }
        let path = clean_path(&[&layout.root, &layout.common, &layout.gitdir]);
        let scratch = dir.join("scratch");
        fs::create_dir(&scratch).map_err(|e| e.to_string())?;
        let raw = read_data(&layout.common.join("config"), MAX_CONFIG)?
            .ok_or_else(|| format!("{} has no config", layout.common.display()))?;
        let list_limits = if facts { FACTS } else { CONFIG_LIST };
        let snap = snapshot(list_config(&scratch, &raw, list_limits, &path)?)?;
        fs::write(dir.join("config"), &snap.config).map_err(|e| e.to_string())?;

        let head = read_data(&layout.gitdir.join("HEAD"), MAX_SMALL)?
            .ok_or_else(|| format!("{} has no HEAD", layout.gitdir.display()))?;
        fs::write(dir.join("HEAD"), head).map_err(|e| e.to_string())?;
        if let Some(shallow) = read_data(&layout.common.join("shallow"), MAX_CONFIG)? {
            fs::write(dir.join("shallow"), shallow).map_err(|e| e.to_string())?;
        }
        fs::create_dir(dir.join("info")).map_err(|e| e.to_string())?;
        if let Ok(Some(exclude)) = read_data(&layout.common.join("info/exclude"), MAX_CONFIG) {
            fs::write(dir.join("info/exclude"), exclude).map_err(|e| e.to_string())?;
        }
        fs::write(dir.join("info/attributes"), "* !filter !diff !merge\n")
            .map_err(|e| e.to_string())?;
        let mut linked_names = vec!["refs", "packed-refs", "worktrees"];
        if access == Access::Read {
            linked_names.push("logs");
        } else {
            fs::create_dir(dir.join("logs")).map_err(|e| e.to_string())?;
        }
        for name in linked_names {
            symlink(layout.common.join(name), dir.join(name)).map_err(|e| e.to_string())?;
        }
        Ok(HostView {
            _p: p,
            dir,
            repo: layout.root,
            common: layout.common,
            access,
            gitdir: layout.gitdir,
            work_tree: false,
            limits: if facts { FACTS } else { PLUMBING },
            offline: facts,
            origin: snap.origin,
            keys: snap.keys,
            sha256: snap.sha256,
            deferred: None,
            path,
        })
    }

    pub fn discover(dir: &Path, access: Access) -> Result<HostView, String> {
        HostView::open(&find_root(dir)?, access)
    }

    pub fn discover_facts(dir: &Path, access: Access) -> Result<HostView, String> {
        HostView::open_facts(&find_root(dir)?, access)
    }

    pub fn with_work_tree(mut self) -> HostView {
        self.work_tree = true;
        if let Err(e) = self.copy_index() {
            self.deferred = Some(e);
        }
        self
    }

    fn copy_index(&self) -> Result<(), String> {
        let Some(index) = read_data(&self.gitdir.join("index"), MAX_INDEX)? else {
            return Ok(());
        };
        let hash = if self.sha256 { 32 } else { 20 };
        match index_is_split(&index, hash) {
            Some(false) => {}
            Some(true) => {
                return Err("the repository uses a split index, which is not supported".into())
            }
            None => return Err("the repository index cannot be read".into()),
        }
        fs::write(self.dir.join("index"), index).map_err(|e| e.to_string())
    }

    fn command(&self) -> Command {
        let network = self.access == Access::Network;
        let mut cmd = base_command(network, &self.path);
        cmd.env("GIT_DIR", &self.dir)
            .env("GIT_OBJECT_DIRECTORY", self.common.join("objects"));
        if self.work_tree {
            cmd.env("GIT_WORK_TREE", &self.repo).current_dir(&self.repo);
        } else {
            cmd.current_dir(&self.dir);
        }
        for pin in [
            "gc.auto=0",
            "maintenance.auto=false",
            "diff.ignoreSubmodules=all",
            "fetch.recurseSubmodules=false",
            "push.recurseSubmodules=no",
        ] {
            cmd.args(["-c", pin]);
        }
        if self.access != Access::Read {
            cmd.args(["-c", "core.logAllRefUpdates=false"]);
        } else {
            cmd.args(["-c", "core.commitGraph=false"]);
        }
        if self.offline {
            cmd.args([
                "-c",
                "protocol.allow=never",
                "-c",
                "protocol.file.allow=never",
            ]);
        } else if network {
            cmd.args([
                "-c",
                "protocol.allow=never",
                "-c",
                "protocol.https.allow=always",
                "-c",
                "protocol.ssh.allow=always",
                "-c",
                "protocol.file.allow=never",
            ]);
        } else {
            cmd.args([
                "-c",
                "protocol.allow=never",
                "-c",
                "protocol.file.allow=always",
            ]);
        }
        cmd
    }

    pub fn prepared(&self, pre: &[&str], args: &[&str]) -> Result<Command, String> {
        if let Some(e) = &self.deferred {
            return Err(e.clone());
        }
        self.check(args)?;
        let mut cmd = self.command();
        cmd.args(pre).arg(args[0]);
        if self.work_tree && matches!(args[0], "status" | "diff" | "diff-index" | "diff-files") {
            cmd.arg("--ignore-submodules=all");
        }
        cmd.args(&args[1..]);
        Ok(cmd)
    }

    fn check(&self, args: &[&str]) -> Result<(), String> {
        let Some(sub) = args.first().filter(|a| !a.starts_with('-')) else {
            return Err("git needs a command name first".into());
        };
        let refused = |why: &str| Err(format!("git {sub} is not allowed here: {why}"));
        if matches!(*sub, "push" | "send-pack") && self.access != Access::Network {
            return refused("pushing needs a network view");
        }
        if self.access == Access::Read && writes(sub, &args[1..]) {
            return refused("a read view cannot change the repository");
        }
        if self.access == Access::Network && matches!(*sub, "push" | "fetch" | "ls-remote") {
            let mut rest = args[1..].iter().copied();
            let mut remote = None;
            while let Some(a) = rest.next() {
                if a == "--" {
                    remote = rest.next();
                    break;
                }
                if ["--upload-pack", "--receive-pack", "--exec", "--repo"]
                    .iter()
                    .any(|o| a.starts_with(o))
                {
                    return refused("it would run a program");
                }
                if !a.starts_with('-') {
                    remote = Some(a);
                    break;
                }
            }
            if !remote.is_some_and(remote_ok) {
                return refused("the remote must be an https or ssh address");
            }
        }
        Ok(())
    }

    pub fn run_raw(&self, args: &[&str]) -> Result<Output, String> {
        let cmd = self.prepared(&[], args)?;
        let l = self.limits;
        run_bounded(cmd, l.deadline, l.max_stdout, l.max_stderr)
            .map_err(|e| format!("git {} failed: {e}", args.first().unwrap_or(&"")))
    }

    pub fn run(&self, args: &[&str]) -> Result<String, String> {
        let out = self.run_raw(args)?;
        if !out.status.success() {
            return Err(format!(
                "git {} failed: {}",
                args.first().unwrap_or(&""),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    pub fn succeeds(&self, args: &[&str]) -> bool {
        self.run_raw(args).is_ok_and(|o| o.status.success())
    }

    pub fn origin_url(&self) -> Option<&str> {
        self.origin.as_deref()
    }

    pub fn has_config_key(&self, key: &str) -> bool {
        self.keys.iter().any(|k| k.eq_ignore_ascii_case(key))
    }

    pub fn repo(&self) -> &Path {
        &self.repo
    }

    pub fn common_dir(&self) -> &Path {
        &self.common
    }

    #[cfg(test)]
    fn private_dir(&self) -> &Path {
        &self.dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worktree::GitEnv;

    fn g(dir: &Path) -> GitEnv {
        GitEnv::bare(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        g(dir).run(args).unwrap()
    }

    fn commit(dir: &Path, name: &str, body: &str) {
        fs::write(dir.join(name), body).unwrap();
        git(dir, &["add", "--", name]);
        git(dir, &["commit", "-q", "-m", name]);
    }

    struct Fx {
        tmp: TempDir,
        repo: PathBuf,
    }

    fn fixture_with(init: &[&str]) -> Fx {
        let tmp = tempfile::tempdir().unwrap();
        let repo = fs::canonicalize(tmp.path()).unwrap().join("r");
        fs::create_dir(&repo).unwrap();
        let mut args = vec!["init", "-q", "-b", "main"];
        args.extend_from_slice(init);
        git(&repo, &args);
        commit(&repo, "a.txt", "1");
        Fx { tmp, repo }
    }

    fn fixture() -> Fx {
        fixture_with(&[])
    }

    fn hostile_hook(fx: &Fx, repo_dir: &Path, name: &str) -> PathBuf {
        let marker = fx.tmp.path().join(name);
        git(
            repo_dir,
            &["config", "hook.x.event", "reference-transaction"],
        );
        git(
            repo_dir,
            &[
                "config",
                "hook.x.command",
                &format!("touch {}", marker.display()),
            ],
        );
        marker
    }

    fn bare_source(fx: &Fx) -> PathBuf {
        let src = fx.tmp.path().join("src.git");
        git(
            &fx.repo,
            &[
                "clone",
                "-q",
                "--bare",
                fx.repo.to_str().unwrap(),
                src.to_str().unwrap(),
            ],
        );
        let work = fx.tmp.path().join("srcwork");
        git(
            fx.tmp.path(),
            &["clone", "-q", src.to_str().unwrap(), work.to_str().unwrap()],
        );
        commit(&work, "b.txt", "2");
        git(&work, &["push", "-q", "origin", "main"]);
        src
    }

    #[test]
    fn a_hook_added_after_open_does_not_run_and_the_ref_lands_in_the_real_repo() {
        let fx = fixture();
        let view = HostView::open(&fx.repo, Access::Write).unwrap();
        let marker = hostile_hook(&fx, &fx.repo, "ran");
        view.run(&["update-ref", "refs/heads/t", "HEAD"]).unwrap();
        assert!(!marker.exists(), "a repository hook ran");
        assert_eq!(
            git(&fx.repo, &["rev-parse", "refs/heads/t"]),
            git(&fx.repo, &["rev-parse", "HEAD"])
        );
    }

    #[test]
    fn a_filter_added_after_open_does_not_run_on_status() {
        let fx = fixture();
        let view = HostView::open(&fx.repo, Access::Read)
            .unwrap()
            .with_work_tree();
        let marker = fx.tmp.path().join("filtered");
        fs::write(fx.repo.join(".gitattributes"), "a.txt filter=x diff=x\n").unwrap();
        git(
            &fx.repo,
            &[
                "config",
                "filter.x.clean",
                &format!("sh -c 'touch {}; cat'", marker.display()),
            ],
        );
        fs::write(fx.repo.join("a.txt"), "2").unwrap();
        let status = view.run(&["status", "--porcelain=v1"]).unwrap();
        assert!(status.contains("a.txt"), "{status}");
        assert!(!marker.exists(), "a repository filter ran");
    }

    #[test]
    fn hostile_config_present_from_the_start_is_ignored() {
        let fx = fixture();
        let marker = hostile_hook(&fx, &fx.repo, "ran");
        let touch = format!("touch {}", marker.display());
        git(&fx.repo, &["config", "core.fsmonitor", &touch]);
        git(&fx.repo, &["config", "core.hooksPath", "/nonexistent"]);
        git(&fx.repo, &["config", "core.sshCommand", &touch]);
        git(&fx.repo, &["config", "include.path", "/nonexistent"]);
        git(&fx.repo, &["config", "credential.helper", &touch]);
        let view = HostView::open(&fx.repo, Access::Write)
            .unwrap()
            .with_work_tree();
        view.run(&["update-ref", "refs/heads/t", "HEAD"]).unwrap();
        view.run(&["status", "--porcelain"]).unwrap();
        assert!(!marker.exists());
        assert!(view.has_config_key("core.fsmonitor"));
    }

    #[test]
    fn the_private_config_carries_only_the_allowed_keys() {
        let fx = fixture();
        git(&fx.repo, &["config", "core.autocrlf", "input"]);
        git(&fx.repo, &["config", "core.eol", "bogus"]);
        git(&fx.repo, &["config", "core.bare", "true"]);
        git(&fx.repo, &["config", "core.worktree", "/tmp"]);
        git(
            &fx.repo,
            &["config", "remote.origin.url", "https://example.com/x.git"],
        );
        git(&fx.repo, &["config", "user.name", "someone"]);
        let view = HostView::open(&fx.repo, Access::Read).unwrap();
        assert_eq!(view.origin_url(), Some("https://example.com/x.git"));
        let cfg = fs::read_to_string(view.private_dir().join("config")).unwrap();
        assert!(cfg.contains("bare = false"), "{cfg}");
        assert!(cfg.contains("autocrlf = input"), "{cfg}");
        for absent in ["eol", "worktree", "remote", "user", "bogus"] {
            assert!(!cfg.contains(absent), "{absent} in {cfg}");
        }
        assert!(!view.private_dir().starts_with(&fx.repo));
        assert!(view
            .private_dir()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("tmgr-hostview-"));
    }

    #[test]
    fn create_only_update_ref_fails_the_second_time() {
        let fx = fixture();
        let view = HostView::open(&fx.repo, Access::Write).unwrap();
        let sha = git(&fx.repo, &["rev-parse", "HEAD"]);
        view.run(&["update-ref", "refs/heads/x", &sha, ""]).unwrap();
        let err = view
            .run(&["update-ref", "refs/heads/x", &sha, ""])
            .unwrap_err();
        assert!(err.contains("exists") || err.contains("reference"), "{err}");
        assert_eq!(git(&fx.repo, &["rev-parse", "refs/heads/x"]), sha);
    }

    #[test]
    fn packed_and_loose_refs_can_be_deleted_through_the_view() {
        let fx = fixture();
        git(&fx.repo, &["branch", "packed"]);
        git(&fx.repo, &["branch", "loose"]);
        git(&fx.repo, &["pack-refs", "--all"]);
        git(&fx.repo, &["branch", "loose2"]);
        let sha = git(&fx.repo, &["rev-parse", "HEAD"]);
        let view = HostView::open(&fx.repo, Access::Write).unwrap();
        view.run(&["update-ref", "-d", "refs/heads/packed", &sha])
            .unwrap();
        view.run(&["update-ref", "-d", "refs/heads/loose2", &sha])
            .unwrap();
        let refs = git(&fx.repo, &["for-each-ref", "--format=%(refname)"]);
        assert!(!refs.contains("packed"), "{refs}");
        assert!(!refs.contains("loose2"), "{refs}");
        assert!(refs.contains("refs/heads/loose"), "{refs}");
        let packed = fs::read_to_string(fx.repo.join(".git/packed-refs")).unwrap();
        assert!(!packed.contains("refs/heads/packed"), "{packed}");
    }

    #[test]
    fn fetch_lands_refs_and_objects_in_the_real_repo() {
        let fx = fixture();
        let src = bare_source(&fx);
        let tip = git(&src, &["rev-parse", "main"]);
        let view = HostView::open(&fx.repo, Access::Write).unwrap();
        view.run(&[
            "fetch",
            "-q",
            src.to_str().unwrap(),
            "refs/heads/main:refs/heads/imported",
        ])
        .unwrap();
        assert_eq!(git(&fx.repo, &["rev-parse", "refs/heads/imported"]), tip);
        git(&fx.repo, &["cat-file", "-e", &tip]);
        assert!(!fx.repo.join(".git/FETCH_HEAD").exists());
    }

    #[test]
    fn fetch_into_a_checked_out_branch_is_refused() {
        let fx = fixture();
        let src = bare_source(&fx);
        let before = git(&fx.repo, &["rev-parse", "main"]);
        let view = HostView::open(&fx.repo, Access::Write).unwrap();
        let err = view
            .run(&[
                "fetch",
                "-q",
                src.to_str().unwrap(),
                "refs/heads/main:refs/heads/main",
            ])
            .unwrap_err();
        assert!(
            err.contains("checked out") || err.contains("refusing"),
            "{err}"
        );
        assert_eq!(git(&fx.repo, &["rev-parse", "main"]), before);
    }

    #[test]
    fn fetch_into_a_branch_checked_out_in_a_linked_worktree_is_refused() {
        let fx = fixture();
        let src = bare_source(&fx);
        let lw = fx.tmp.path().join("lw");
        git(
            &fx.repo,
            &["worktree", "add", "-q", "-b", "lw", lw.to_str().unwrap()],
        );
        git(&fx.repo, &["branch", "main2", "main"]);
        let src_work = fx.tmp.path().join("srcwork");
        git(&src_work, &["push", "-q", "origin", "main:lw"]);
        let before = git(&fx.repo, &["rev-parse", "lw"]);
        let view = HostView::open(&fx.repo, Access::Write).unwrap();
        let err = view
            .run(&[
                "fetch",
                "-q",
                src.to_str().unwrap(),
                "refs/heads/lw:refs/heads/lw",
            ])
            .unwrap_err();
        assert!(
            err.contains("checked out") || err.contains("refusing"),
            "{err}"
        );
        assert_eq!(git(&fx.repo, &["rev-parse", "lw"]), before);
    }

    #[test]
    fn a_symlinked_reflog_target_is_not_written_by_a_write_view() {
        let fx = fixture();
        let outside = fx.tmp.path().join("outside.txt");
        fs::write(&outside, "keep\n").unwrap();
        let logs = fx.repo.join(".git/logs/refs/heads");
        fs::create_dir_all(&logs).unwrap();
        symlink(&outside, logs.join("x")).unwrap();
        let sha = git(&fx.repo, &["rev-parse", "HEAD"]);
        let view = HostView::open(&fx.repo, Access::Write).unwrap();
        view.run(&["update-ref", "refs/heads/x", &sha, ""]).unwrap();
        view.run(&["update-ref", "-d", "refs/heads/x", &sha])
            .unwrap();
        assert_eq!(fs::read_to_string(&outside).unwrap(), "keep\n");
        assert!(fs::symlink_metadata(logs.join("x"))
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn a_read_view_sees_reflogs() {
        let fx = fixture();
        let view = HostView::open(&fx.repo, Access::Read).unwrap();
        let log = view.run(&["log", "-g", "--format=%H", "--all"]).unwrap();
        assert!(
            log.contains(&git(&fx.repo, &["rev-parse", "HEAD"])),
            "{log}"
        );
    }

    #[test]
    fn a_bare_flag_in_the_real_config_does_not_unprotect_the_checked_out_branch() {
        let fx = fixture();
        let src = bare_source(&fx);
        git(&fx.repo, &["config", "core.bare", "true"]);
        let before = fs::read_to_string(fx.repo.join(".git/refs/heads/main")).unwrap();
        let view = HostView::open(&fx.repo, Access::Write).unwrap();
        let err = view
            .run(&[
                "fetch",
                "-q",
                src.to_str().unwrap(),
                "refs/heads/main:refs/heads/main",
            ])
            .unwrap_err();
        assert!(
            err.contains("checked out") || err.contains("refusing"),
            "{err}"
        );
        assert_eq!(
            fs::read_to_string(fx.repo.join(".git/refs/heads/main")).unwrap(),
            before
        );
    }

    #[test]
    fn submodule_config_does_not_run_on_status_through_a_work_tree_view() {
        let fx = fixture();
        let sub = fx.repo.join("sub");
        fs::create_dir(&sub).unwrap();
        git(&sub, &["init", "-q"]);
        fs::write(sub.join(".gitattributes"), "f filter=x\n").unwrap();
        commit(&sub, "f", "1");
        fs::write(
            fx.repo.join(".gitmodules"),
            "[submodule \"sub\"]\n\tpath = sub\n\turl = ./sub\n\tignore = none\n",
        )
        .unwrap();
        git(&fx.repo, &["add", ".gitmodules", "sub"]);
        git(&fx.repo, &["commit", "-q", "-m", "link"]);
        let marker = fx.tmp.path().join("sub-ran");
        let clean = format!("sh -c 'touch {}; cat'", marker.display());
        git(&sub, &["config", "filter.x.clean", &clean]);
        fs::write(sub.join("f"), "2").unwrap();
        let view = HostView::open(&fx.repo, Access::Read)
            .unwrap()
            .with_work_tree();
        view.run(&["status", "--porcelain"]).unwrap();
        assert!(!marker.exists(), "a submodule config ran");
    }

    #[test]
    fn network_views_allow_only_https_and_ssh_addresses() {
        let fx = fixture();
        let src = bare_source(&fx);
        let view = HostView::open(&fx.repo, Access::Network).unwrap();
        let file_url = format!("file://{}", src.display());
        for url in [
            "git://127.0.0.1:1/x",
            "http://127.0.0.1:1/x",
            "ext::true",
            "-oProxyCommand=true",
            "../src.git",
            src.to_str().unwrap(),
            file_url.as_str(),
            "host:path",
            "u@-host:path",
            "ssh://-oProxyCommand=true/x",
        ] {
            let err = view.run_raw(&["ls-remote", "--", url]).unwrap_err();
            assert!(err.contains("not allowed"), "{url}: {err}");
        }
        let err = view
            .run_raw(&["fetch", "--", src.to_str().unwrap()])
            .unwrap_err();
        assert!(err.contains("not allowed"), "{err}");
        let err = view
            .run_raw(&["push", src.to_str().unwrap(), "HEAD"])
            .unwrap_err();
        assert!(err.contains("not allowed"), "{err}");
        let err = view.run_raw(&["ls-remote"]).unwrap_err();
        assert!(err.contains("not allowed"), "{err}");
        let err = view
            .run_raw(&["ls-remote", "--upload-pack=true", "https://127.0.0.1:1/x"])
            .unwrap_err();
        assert!(err.contains("not allowed"), "{err}");
        let out = view
            .run_raw(&["ls-remote", "https://127.0.0.1:1/x"])
            .unwrap();
        assert!(!String::from_utf8_lossy(&out.stderr).contains("not allowed"));
    }

    #[test]
    fn https_and_ssh_addresses_are_accepted() {
        for url in [
            "https://github.com/o/r.git",
            "ssh://git@github.com/o/r.git",
            "ssh://git@github.com:22/o/r.git",
            "git@github.com:o/r.git",
        ] {
            assert!(remote_ok(url), "{url}");
        }
        for url in [
            "",
            "/x/y.git",
            "file:///x",
            "github.com:o/r",
            "https://",
            "a/b@c:d",
        ] {
            assert!(!remote_ok(url), "{url}");
        }
    }

    #[test]
    fn the_file_transport_is_never_allowed_on_a_network_view() {
        let fx = fixture();
        let src = bare_source(&fx);
        let view = HostView::open(&fx.repo, Access::Network).unwrap();
        let mut cmd = view.command();
        cmd.args(["ls-remote", "--", src.to_str().unwrap()]);
        let out = cmd.output().unwrap();
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("not allowed"),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn push_and_send_pack_are_refused_without_a_network_view() {
        let fx = fixture();
        let src = bare_source(&fx);
        let marker = hostile_hook(&fx, &src, "pushed");
        for access in [Access::Read, Access::Write] {
            let view = HostView::open(&fx.repo, access).unwrap();
            for args in [
                vec!["push", src.to_str().unwrap(), "HEAD:refs/heads/z"],
                vec!["send-pack", src.to_str().unwrap(), "HEAD:refs/heads/z"],
            ] {
                let err = view.run_raw(&args).unwrap_err();
                assert!(err.contains("not allowed"), "{args:?}: {err}");
            }
        }
        assert!(!marker.exists());
        assert!(!git(&src, &["for-each-ref"]).contains("refs/heads/z"));
    }

    #[test]
    fn read_views_cannot_change_refs_or_reach_remotes() {
        let fx = fixture();
        let src = bare_source(&fx);
        let sha = git(&fx.repo, &["rev-parse", "HEAD"]);
        let view = HostView::open(&fx.repo, Access::Read).unwrap();
        for args in [
            vec!["update-ref", "refs/heads/z", sha.as_str()],
            vec!["fetch", src.to_str().unwrap()],
            vec!["ls-remote", src.to_str().unwrap()],
            vec!["branch", "z"],
            vec!["tag", "z"],
            vec!["symbolic-ref", "HEAD", "refs/heads/z"],
            vec!["symbolic-ref", "-d", "HEAD"],
            vec!["reflog", "expire", "--all"],
            vec!["pack-refs", "--all"],
            vec!["hash-object", "-w", "a.txt"],
        ] {
            let err = view.run_raw(&args).unwrap_err();
            assert!(err.contains("not allowed"), "{args:?}: {err}");
        }
        assert!(view
            .run_raw(&["symbolic-ref", "--short", "-q", "HEAD"])
            .is_ok());
        assert!(view.run_raw(&["for-each-ref"]).is_ok());
        assert_eq!(
            git(&fx.repo, &["for-each-ref", "--format=%(refname)"]),
            "refs/heads/main"
        );
    }

    #[test]
    fn local_views_refuse_network_protocols() {
        let fx = fixture();
        let view = HostView::open(&fx.repo, Access::Write).unwrap();
        let out = view
            .run_raw(&["ls-remote", "https://127.0.0.1:1/x"])
            .unwrap();
        assert!(String::from_utf8_lossy(&out.stderr).contains("not allowed"));
    }

    #[test]
    fn a_temp_folder_inside_the_repository_is_refused() {
        let fx = fixture();
        fs::create_dir(fx.repo.join("deep")).unwrap();
        let err = HostView::open_in(&fx.repo, Access::Read, &fx.repo.join("deep"), false)
            .err()
            .unwrap();
        assert!(err.contains("inside the repository"), "{err}");
    }

    #[test]
    fn write_views_on_a_linked_worktree_are_refused() {
        let fx = fixture();
        let lw = fx.tmp.path().join("lw");
        git(
            &fx.repo,
            &["worktree", "add", "-q", "-b", "lw", lw.to_str().unwrap()],
        );
        for access in [Access::Write, Access::Network] {
            assert!(HostView::open(&lw, access).is_err());
        }
    }

    #[test]
    fn network_views_pass_the_publish_environment_and_others_do_not() {
        std::env::set_var("GH_TMGR_VIEW_PROBE", "probe");
        let fx = fixture();
        let sent = |access| {
            let view = HostView::open(&fx.repo, access).unwrap();
            let cmd = view.prepared(&[], &["rev-parse", "HEAD"]).unwrap();
            cmd.get_envs()
                .any(|(k, v)| k == "GH_TMGR_VIEW_PROBE" && v.is_some())
        };
        assert!(sent(Access::Network));
        assert!(!sent(Access::Write));
        assert!(!sent(Access::Read));
    }

    #[test]
    fn sha256_repositories_work() {
        let fx = fixture_with(&["--object-format=sha256"]);
        let view = HostView::open(&fx.repo, Access::Write).unwrap();
        let sha = view.run(&["rev-parse", "HEAD"]).unwrap();
        assert_eq!(sha.len(), 64);
        view.run(&["update-ref", "refs/heads/s", &sha, ""]).unwrap();
        assert_eq!(git(&fx.repo, &["rev-parse", "refs/heads/s"]), sha);
        let wt = HostView::open(&fx.repo, Access::Read)
            .unwrap()
            .with_work_tree();
        assert_eq!(wt.run(&["status", "--porcelain"]).unwrap(), "");
    }

    #[test]
    fn unknown_or_unsafe_extensions_are_refused() {
        for (key, value) in [
            ("extensions.foo", "bar"),
            ("extensions.partialclone", "origin"),
            ("extensions.compatobjectformat", "sha256"),
            ("extensions.refstorage", "reftable"),
        ] {
            let fx = fixture();
            git(&fx.repo, &["config", "core.repositoryformatversion", "1"]);
            git(&fx.repo, &["config", key, value]);
            let err = HostView::open(&fx.repo, Access::Read).err().unwrap();
            assert!(err.contains("not supported"), "{key}: {err}");
        }
        let fx = fixture();
        git(&fx.repo, &["config", "core.repositoryformatversion", "2"]);
        assert!(HostView::open(&fx.repo, Access::Read).is_err());
    }

    #[test]
    fn worktree_config_extension_is_dropped_not_followed() {
        let fx = fixture();
        git(&fx.repo, &["config", "core.repositoryformatversion", "1"]);
        git(&fx.repo, &["config", "extensions.worktreeconfig", "true"]);
        let marker = fx.tmp.path().join("wt-ran");
        fs::write(
            fx.repo.join(".git/config.worktree"),
            format!("[core]\n\tfsmonitor = touch {}\n", marker.display()),
        )
        .unwrap();
        let view = HostView::open(&fx.repo, Access::Read)
            .unwrap()
            .with_work_tree();
        view.run(&["status", "--porcelain"]).unwrap();
        assert!(!marker.exists());
        let cfg = fs::read_to_string(view.private_dir().join("config")).unwrap();
        assert!(!cfg.contains("worktreeconfig"), "{cfg}");
    }

    #[test]
    fn tree_commands_always_ignore_submodules() {
        let fx = fixture();
        let view = HostView::open(&fx.repo, Access::Read)
            .unwrap()
            .with_work_tree();
        for sub in ["status", "diff", "diff-index", "diff-files"] {
            let cmd = view.prepared(&[], &[sub, "--quiet"]).unwrap();
            let args: Vec<_> = cmd
                .get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            let at = args.iter().position(|a| a == sub).unwrap();
            assert_eq!(args[at + 1], "--ignore-submodules=all", "{sub}");
        }
    }

    #[test]
    fn alternates_are_not_asked_for_refs_during_a_fetch() {
        let fx = fixture();
        let src = bare_source(&fx);
        let alt = fx.tmp.path().join("alt");
        fs::create_dir(&alt).unwrap();
        git(&alt, &["init", "-q", "-b", "main"]);
        commit(&alt, "alt.txt", "x");
        let marker = fx.tmp.path().join("alt-ran");
        git(
            &alt,
            &[
                "config",
                "core.pager",
                &format!("touch {}", marker.display()),
            ],
        );
        fs::write(
            fx.repo.join(".git/objects/info/alternates"),
            format!("{}\n", alt.join(".git/objects").display()),
        )
        .unwrap();
        let trace = fx.tmp.path().join("trace");
        let view = HostView::open(&fx.repo, Access::Write).unwrap();
        let mut cmd = view
            .prepared(
                &[],
                &[
                    "fetch",
                    "-q",
                    src.to_str().unwrap(),
                    "refs/heads/main:refs/heads/imported",
                ],
            )
            .unwrap();
        let out = cmd.env("GIT_TRACE", &trace).output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let log = fs::read_to_string(&trace).unwrap_or_default();
        assert!(!log.contains("for-each-ref"), "{log}");
        assert!(!marker.exists());
        git(&fx.repo, &["rev-parse", "refs/heads/imported"]);
    }

    #[test]
    fn a_git_file_pointing_at_another_repository_is_refused() {
        let fx = fixture();
        let other = fx.tmp.path().join("other");
        fs::create_dir(&other).unwrap();
        git(&other, &["init", "-q", "-b", "main"]);
        commit(&other, "o.txt", "o");
        let fake = fx.tmp.path().join("fake");
        fs::create_dir(&fake).unwrap();
        fs::write(
            fake.join(".git"),
            format!("gitdir: {}\n", other.join(".git").display()),
        )
        .unwrap();
        assert!(HostView::open(&fake, Access::Read).is_err());
        assert!(HostView::discover(&fake, Access::Read).is_err());

        let lw = fx.tmp.path().join("lw");
        git(
            &other,
            &["worktree", "add", "-q", "-b", "lw", lw.to_str().unwrap()],
        );
        fs::copy(lw.join(".git"), fake.join(".git")).unwrap();
        assert!(HostView::open(&fake, Access::Read).is_err());
        assert!(HostView::open(&lw, Access::Read).is_ok());
    }

    #[test]
    fn programs_in_the_repository_are_not_on_the_path() {
        let fx = fixture();
        let bin = fx.repo.join("bin");
        fs::create_dir(&bin).unwrap();
        let inside_common = fx.repo.join(".git/hooks");
        let system = fx.tmp.path().join("system");
        fs::create_dir(&system).unwrap();
        let guards = [fx.repo.as_path(), inside_common.as_path()];
        let joined = std::env::join_paths([
            bin.clone(),
            inside_common.clone(),
            system.clone(),
            PathBuf::from("relative/bin"),
        ])
        .unwrap();
        let kept: Vec<PathBuf> = std::env::split_paths(&clean_path_in(&joined, &guards)).collect();
        assert_eq!(kept, vec![system]);
    }

    #[test]
    fn the_index_is_copied_only_for_work_tree_views() {
        let fx = fixture();
        let view = HostView::open(&fx.repo, Access::Read).unwrap();
        assert!(!view.private_dir().join("index").exists());
        let view = view.with_work_tree();
        assert!(view.private_dir().join("index").exists());
    }

    #[test]
    fn read_views_do_not_trust_the_commit_graph() {
        let fx = fixture();
        let view = HostView::open(&fx.repo, Access::Read).unwrap();
        assert_eq!(
            view.run(&["config", "--get", "core.commitGraph"]).unwrap(),
            "false"
        );
    }

    #[test]
    fn the_config_listing_is_bounded_by_the_facts_limits() {
        let fx = fixture();
        let mut big = String::from("[x]\n");
        for i in 0..20000 {
            big.push_str(&format!("\tk{i} = vvvvvvvvvv\n"));
        }
        let cfg = fx.repo.join(".git/config");
        let mut text = fs::read_to_string(&cfg).unwrap();
        text.push_str(&big);
        fs::write(&cfg, text).unwrap();
        assert!(HostView::open(&fx.repo, Access::Read).is_ok());
        assert!(HostView::open_facts(&fx.repo, Access::Read).is_err());
    }

    #[test]
    fn the_config_listing_stops_looking_for_a_repository_at_its_scratch_folder() {
        let fx = fixture();
        let scratch = fx.tmp.path().join("p").join("scratch");
        let cmd = config_command(&scratch, &scratch.join("c"), OsStr::new("/usr/bin"));
        let ceiling = cmd
            .get_envs()
            .find(|(k, _)| *k == "GIT_CEILING_DIRECTORIES")
            .and_then(|(_, v)| v)
            .map(PathBuf::from);
        assert_eq!(ceiling.as_deref(), scratch.parent());
    }

    #[test]
    fn the_first_origin_url_is_the_one_git_uses() {
        let fx = fixture();
        git(
            &fx.repo,
            &[
                "config",
                "remote.origin.url",
                "https://example.com/first.git",
            ],
        );
        git(
            &fx.repo,
            &[
                "config",
                "--add",
                "remote.origin.url",
                "https://example.com/second.git",
            ],
        );
        let view = HostView::open(&fx.repo, Access::Read).unwrap();
        assert_eq!(view.origin_url(), Some("https://example.com/first.git"));
    }

    #[test]
    fn a_split_index_is_refused() {
        let fx = fixture();
        commit(&fx.repo, "b.txt", "2");
        git(&fx.repo, &["update-index", "--split-index"]);
        fs::write(fx.repo.join("b.txt"), "3").unwrap();
        git(&fx.repo, &["add", "b.txt"]);
        let view = HostView::open(&fx.repo, Access::Read)
            .unwrap()
            .with_work_tree();
        let err = view.run(&["status", "--porcelain"]).unwrap_err();
        assert!(err.contains("split index"), "{err}");
    }

    #[test]
    fn a_normal_index_of_every_version_is_accepted() {
        for version in ["2", "3", "4"] {
            let fx = fixture();
            fs::create_dir(fx.repo.join("dir")).unwrap();
            commit(&fx.repo, "dir/c.txt", "c");
            git(&fx.repo, &["update-index", "--index-version", version]);
            fs::write(fx.repo.join("n.txt"), "n").unwrap();
            git(&fx.repo, &["add", "-N", "n.txt"]);
            let view = HostView::open(&fx.repo, Access::Read)
                .unwrap()
                .with_work_tree();
            assert!(view.run(&["status", "--porcelain"]).is_ok(), "v{version}");
        }
    }

    #[test]
    fn the_work_tree_view_reads_a_linked_worktree_head() {
        let fx = fixture();
        let lw = fx.tmp.path().join("lw");
        git(
            &fx.repo,
            &["worktree", "add", "-q", "-b", "lw", lw.to_str().unwrap()],
        );
        commit(&lw, "l.txt", "l");
        let lw = fs::canonicalize(lw).unwrap();
        let view = HostView::open(&lw, Access::Read).unwrap().with_work_tree();
        assert_eq!(view.common_dir(), fx.repo.join(".git"));
        assert_eq!(view.repo(), lw);
        assert_eq!(
            view.run(&["rev-parse", "HEAD"]).unwrap(),
            git(&lw, &["rev-parse", "HEAD"])
        );
        assert_eq!(view.run(&["status", "--porcelain"]).unwrap(), "");
    }

    #[test]
    fn a_bare_repository_and_a_non_repository() {
        let fx = fixture();
        let src = bare_source(&fx);
        let view = HostView::open(&src, Access::Read).unwrap();
        assert_eq!(
            view.run(&["rev-parse", "main"]).unwrap(),
            git(&src, &["rev-parse", "main"])
        );
        let plain = fx.tmp.path().join("plain");
        fs::create_dir(&plain).unwrap();
        assert!(HostView::open(&plain, Access::Read).is_err());
    }

    #[test]
    fn a_config_that_is_a_symlink_or_too_large_is_refused() {
        let fx = fixture();
        let cfg = fx.repo.join(".git/config");
        let real = fx.tmp.path().join("cfg");
        fs::rename(&cfg, &real).unwrap();
        symlink(&real, &cfg).unwrap();
        assert!(HostView::open(&fx.repo, Access::Read).is_err());
        fs::remove_file(&cfg).unwrap();
        fs::write(&cfg, vec![b'#'; 2 * 1024 * 1024]).unwrap();
        let err = HostView::open(&fx.repo, Access::Read).err().unwrap();
        assert!(err.contains("larger"), "{err}");
    }

    #[test]
    fn an_exclude_file_that_is_not_plain_is_skipped() {
        let fx = fixture();
        let exclude = fx.repo.join(".git/info/exclude");
        fs::create_dir_all(exclude.parent().unwrap()).unwrap();
        let _ = fs::remove_file(&exclude);
        symlink("/dev/zero", &exclude).unwrap();
        let view = HostView::open(&fx.repo, Access::Read).unwrap();
        assert!(!view.private_dir().join("info/exclude").exists());
    }

    #[test]
    fn the_private_folder_goes_away_with_the_view() {
        let fx = fixture();
        let view = HostView::open(&fx.repo, Access::Read).unwrap();
        let dir = view.private_dir().to_path_buf();
        assert!(dir.exists());
        drop(view);
        assert!(!dir.exists());
        assert!(fx.repo.join(".git/refs").exists());
    }

    #[test]
    fn discover_finds_the_root_from_a_subfolder() {
        let fx = fixture();
        fs::create_dir(fx.repo.join("deep")).unwrap();
        let view = HostView::discover(&fx.repo.join("deep"), Access::Read).unwrap();
        assert_eq!(view.repo(), fx.repo);
    }
}
