use std::collections::BTreeSet;
use std::ffi::CString;
use std::fs;
use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output, Stdio};

const MAX_REF: usize = 100;
const MAX_COPY: u64 = 1024 * 1024;
const HARDENING: [&str; 14] = [
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "protocol.file.allow=user",
    "-c",
    "credential.helper=",
    "-c",
    "submodule.recurse=false",
    "-c",
    "core.untrackedCache=false",
    "-c",
    "core.askPass=",
];

pub struct GitEnv {
    dir: Option<PathBuf>,
    filters: Vec<String>,
    extra: Vec<(String, String)>,
}

impl GitEnv {
    pub fn new() -> GitEnv {
        GitEnv {
            dir: None,
            filters: Vec::new(),
            extra: Vec::new(),
        }
    }

    pub fn bare(dir: &Path) -> GitEnv {
        GitEnv {
            dir: Some(dir.to_path_buf()),
            ..GitEnv::new()
        }
    }

    pub fn open(dir: &Path) -> Result<GitEnv, String> {
        let mut env = GitEnv::bare(dir);
        let raw = env.run_raw(&["config", "--local", "--name-only", "--list", "-z"])?;
        if !raw.status.success() {
            return Err(format!("not a git repository: {}", dir.display()));
        }
        let mut filters = BTreeSet::new();
        for name in String::from_utf8_lossy(&raw.stdout).split('\0') {
            if name.is_empty() {
                continue;
            }
            if let Some(why) = forbidden(name) {
                return Err(format!("repository config key {name} is refused: {why}"));
            }
            if let Some(f) = filter_name(name) {
                filters.insert(f.to_string());
            }
        }
        env.filters = filters.into_iter().collect();
        Ok(env)
    }

    pub fn env(mut self, key: &str, value: &str) -> GitEnv {
        self.extra.push((key.to_string(), value.to_string()));
        self
    }

    pub fn command(&self) -> Command {
        let mut cmd = Command::new("git");
        cmd.env_clear();
        for (k, v) in std::env::vars_os() {
            let key = k.to_string_lossy();
            if matches!(key.as_ref(), "PATH" | "HOME" | "LANG") || key.starts_with("LC_") {
                cmd.env(k, v);
            }
        }
        cmd.env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0");
        for (k, v) in &self.extra {
            cmd.env(k, v);
        }
        if let Some(dir) = &self.dir {
            cmd.current_dir(dir);
        }
        cmd.args(HARDENING);
        for f in &self.filters {
            for var in ["process", "smudge", "clean"] {
                cmd.arg("-c").arg(format!("filter.{f}.{var}="));
            }
            cmd.arg("-c").arg(format!("filter.{f}.required=false"));
        }
        cmd.stdin(Stdio::null());
        cmd
    }

    fn run_raw(&self, args: &[&str]) -> Result<Output, String> {
        let mut cmd = self.command();
        cmd.args(args);
        cmd.output()
            .map_err(|e| format!("cannot run git {}: {e}", args.first().unwrap_or(&"")))
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

    fn succeeds(&self, args: &[&str]) -> bool {
        self.run_raw(args).is_ok_and(|o| o.status.success())
    }
}

impl Default for GitEnv {
    fn default() -> Self {
        GitEnv::new()
    }
}

fn forbidden(name: &str) -> Option<&'static str> {
    let n = name.to_ascii_lowercase();
    let last = n.rsplit('.').next().unwrap_or("");
    if n.starts_with("include.") || n.starts_with("includeif.") {
        return Some("config includes");
    }
    if n.starts_with("credential.") || n.starts_with("uploadpack.") || n.starts_with("protocol.") {
        return Some("runs or widens external programs");
    }
    if matches!(
        n.as_str(),
        "core.fsmonitor"
            | "core.sshcommand"
            | "extensions.worktreeconfig"
            | "core.gitproxy"
            | "core.askpass"
            | "core.alternaterefscommand"
            | "diff.external"
            | "gpg.program"
    ) {
        return Some("runs an external program");
    }
    let tail = n.matches('.').count() >= 2;
    let section = n.split('.').next().unwrap_or("");
    let by_section = match section {
        "diff" => matches!(last, "textconv" | "command"),
        "merge" => last == "driver",
        "gpg" => last == "program",
        "remote" => matches!(last, "vcs" | "uploadpack" | "receivepack" | "proxy"),
        _ => false,
    };
    (tail && by_section).then_some("runs an external program")
}

fn filter_name(key: &str) -> Option<&str> {
    let rest = key.strip_prefix("filter.")?;
    let (name, _) = rest.rsplit_once('.')?;
    Some(name)
}

fn valid_ref(name: &str) -> Result<(), String> {
    let bad = |why: &str| Err(format!("invalid ref name {name:?}: {why}"));
    if name.is_empty() || name.len() > MAX_REF {
        return bad("empty or too long");
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'-'))
    {
        return bad("characters outside A-Za-z0-9 . _ / -");
    }
    if !name.as_bytes()[0].is_ascii_alphanumeric() || name.contains("..") || name == "HEAD" {
        return bad("leading symbol, .. or HEAD");
    }
    for part in name.split('/') {
        if part.is_empty()
            || part.starts_with('.')
            || part.ends_with('.')
            || part.ends_with(".lock")
        {
            return bad("empty, dotted or .lock component");
        }
    }
    Ok(())
}

fn utf8(p: &Path) -> Result<&str, String> {
    p.to_str()
        .ok_or_else(|| format!("path is not valid UTF-8: {}", p.display()))
}

fn is_symlink(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink())
}

fn worktrees_root(repo: &Path) -> Result<PathBuf, String> {
    let name = repo
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("repository path has no name")?;
    let parent = repo.parent().ok_or("repository has no parent directory")?;
    Ok(parent.join(format!("{name}-worktrees")))
}

fn approved_path(repo: &Path, path: &Path) -> Result<(), String> {
    let root = worktrees_root(repo)?;
    if is_symlink(&root) {
        return Err(format!("{} is a symlink", root.display()));
    }
    let parent = path.parent().ok_or("worktree path has no parent")?;
    let canon_parent =
        fs::canonicalize(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    let canon_root = fs::canonicalize(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    if canon_parent != canon_root || path.file_name().is_none() {
        return Err(format!(
            "worktree {} is outside {}",
            path.display(),
            root.display()
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub repo: PathBuf,
    pub branch: String,
    pub base: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Worktree {
    pub repo: PathBuf,
    pub path: PathBuf,
    pub branch: String,
    pub base: String,
    pub clone: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub clean: bool,
    pub ahead: u32,
    pub pushed: bool,
    pub merged_into_base: bool,
}

pub fn plan(repo: &Path, branch: &str, base: Option<&str>) -> Result<Plan, String> {
    let repo = fs::canonicalize(repo).map_err(|e| format!("{}: {e}", repo.display()))?;
    utf8(&repo)?;
    let git = GitEnv::open(&repo)?;
    let top = git.run(&["rev-parse", "--show-toplevel"])?;
    if fs::canonicalize(&top).map_err(|e| e.to_string())? != repo {
        return Err(format!("{} is not a repository root", repo.display()));
    }
    valid_ref(branch)?;
    let base = match base {
        Some(b) => {
            valid_ref(b)?;
            b.to_string()
        }
        None => default_base(&git)?,
    };
    if git.succeeds(&[
        "show-ref",
        "--verify",
        "--quiet",
        &format!("refs/heads/{branch}"),
    ]) {
        return Err(format!("branch {branch} already exists"));
    }
    let path = worktrees_root(&repo)?.join(branch.replace('/', "-"));
    if fs::symlink_metadata(&path).is_ok() {
        return Err(format!("{} already exists", path.display()));
    }
    Ok(Plan {
        repo,
        branch: branch.to_string(),
        base,
        path,
    })
}

fn default_base(git: &GitEnv) -> Result<String, String> {
    if let Ok(head) = git.run(&["symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD"]) {
        if valid_ref(&head).is_ok() && git.succeeds(&["rev-parse", "--verify", "--quiet", &head]) {
            return Ok(head);
        }
    }
    for local in ["main", "master"] {
        if git.succeeds(&[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{local}"),
        ]) {
            return Ok(local.to_string());
        }
    }
    Err("cannot resolve a default base branch".to_string())
}

fn prepare_root(plan: &Plan) -> Result<(), String> {
    let root = worktrees_root(&plan.repo)?;
    if !root.exists() {
        fs::create_dir(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    }
    approved_path(&plan.repo, &plan.path)?;
    if fs::symlink_metadata(&plan.path).is_ok() {
        return Err(format!("{} already exists", plan.path.display()));
    }
    Ok(())
}

pub fn create(plan: &Plan, fetch: bool) -> Result<Worktree, String> {
    prepare_root(plan)?;
    let git = GitEnv::open(&plan.repo)?;
    if fetch && git.succeeds(&["remote", "get-url", "origin"]) {
        if let Some(branch) = plan.base.strip_prefix("origin/") {
            git.run(&["fetch", "--no-tags", "--", "origin", branch])?;
        }
    }
    git.run(&[
        "worktree",
        "add",
        "--no-track",
        "-b",
        &plan.branch,
        "--",
        utf8(&plan.path)?,
        &plan.base,
    ])?;
    soft_exclude(&git, &plan.repo, &plan.repo);
    Ok(Worktree {
        repo: plan.repo.clone(),
        path: plan.path.clone(),
        branch: plan.branch.clone(),
        base: plan.base.clone(),
        clone: false,
    })
}

pub fn clone_mode(plan: &Plan) -> Result<Worktree, String> {
    prepare_root(plan)?;
    let src = GitEnv::open(&plan.repo)?;
    let sha = src.run(&[
        "rev-parse",
        "--verify",
        &format!("{}^{{commit}}", plan.base),
    ])?;
    let repo = utf8(&plan.repo)?;
    src.run(&[
        "clone",
        "--no-checkout",
        "--reference",
        repo,
        "--",
        repo,
        utf8(&plan.path)?,
    ])?;
    let git = GitEnv::open(&plan.path)?;
    git.run(&["checkout", "--no-track", "-b", &plan.branch, &sha])?;
    soft_exclude(&git, &plan.path, &plan.path);
    Ok(Worktree {
        repo: plan.repo.clone(),
        path: plan.path.clone(),
        branch: plan.branch.clone(),
        base: plan.base.clone(),
        clone: true,
    })
}

fn cstr(name: &str) -> std::io::Result<CString> {
    CString::new(name).map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))
}

fn open_at(
    dir: BorrowedFd<'_>,
    name: &str,
    flags: libc::c_int,
    mode: u32,
) -> std::io::Result<OwnedFd> {
    let name = cstr(name)?;
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC,
            mode as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn open_root(path: &Path) -> std::io::Result<OwnedFd> {
    let c = cstr(utf8(path).map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?)?;
    let fd = unsafe {
        libc::open(
            c.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn open_dir_at(dir: BorrowedFd<'_>, name: &str, create: bool) -> std::io::Result<OwnedFd> {
    let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW;
    match open_at(dir, name, flags, 0) {
        Err(e) if create && e.raw_os_error() == Some(libc::ENOENT) => {
            let c = cstr(name)?;
            let rc = unsafe { libc::mkdirat(dir.as_raw_fd(), c.as_ptr(), 0o700) };
            if rc < 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST) {
                return Err(std::io::Error::last_os_error());
            }
            open_at(dir, name, flags, 0)
        }
        other => other,
    }
}

fn read_bounded(file: &mut fs::File) -> std::io::Result<Option<Vec<u8>>> {
    let mut data = Vec::new();
    file.take(MAX_COPY + 1).read_to_end(&mut data)?;
    Ok((data.len() as u64 <= MAX_COPY).then_some(data))
}

fn add_exclude(git: &GitEnv, run_dir: &Path, owner: &Path, lines: &[String]) -> Result<(), String> {
    let common = PathBuf::from(git.run(&["rev-parse", "--git-common-dir"])?);
    let common = if common.is_absolute() {
        common
    } else {
        run_dir.join(common)
    };
    let expected = fs::canonicalize(owner)
        .map_err(|e| format!("{}: {e}", owner.display()))?
        .join(".git");
    let meta =
        fs::symlink_metadata(&expected).map_err(|e| format!("{}: {e}", expected.display()))?;
    let actual = fs::canonicalize(&common).map_err(|e| format!("{}: {e}", common.display()))?;
    if !meta.is_dir() || actual != expected {
        return Err(format!(
            "git common dir {} is not {}",
            common.display(),
            expected.display()
        ));
    }
    let io = |what: &str, e: std::io::Error| format!("{}/{what}: {e}", expected.display());
    let gitfd = open_root(&expected).map_err(|e| io("", e))?;
    let infofd = open_dir_at(gitfd.as_fd(), "info", true).map_err(|e| io("info", e))?;
    let raw = open_at(
        infofd.as_fd(),
        "exclude",
        libc::O_RDWR | libc::O_APPEND | libc::O_CREAT | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        0o644,
    )
    .map_err(|e| io("info/exclude", e))?;
    let mut f = fs::File::from(raw);
    let m = f.metadata().map_err(|e| io("info/exclude", e))?;
    if !m.is_file() || m.nlink() != 1 || m.size() > MAX_COPY {
        return Err(format!(
            "{}/info/exclude is not a plain file",
            expected.display()
        ));
    }
    let existing = read_bounded(&mut f).map_err(|e| io("info/exclude", e))?;
    let existing =
        String::from_utf8_lossy(&existing.ok_or("info/exclude larger than 1 MiB")?).into_owned();
    let mut add = String::new();
    if !existing.is_empty() && !existing.ends_with('\n') {
        add.push('\n');
    }
    let before = add.len();
    for line in lines {
        if !existing.lines().any(|l| l == line) {
            add.push_str(line);
            add.push('\n');
        }
    }
    if add.len() == before {
        return Ok(());
    }
    f.write_all(add.as_bytes())
        .map_err(|e| io("info/exclude", e))
}

fn soft_exclude(git: &GitEnv, run_dir: &Path, owner: &Path) {
    if let Err(e) = add_exclude(git, run_dir, owner, &[".tmgr/".to_string()]) {
        eprintln!("tmgr-runner: skipped .tmgr/ exclude: {e}");
    }
}

fn relative_components(rel: &str) -> Result<Vec<&str>, String> {
    let bad = |why: &str| Err(format!("invalid file path {rel:?}: {why}"));
    if rel.is_empty() || rel.len() > 200 {
        return bad("empty or too long");
    }
    if !rel
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'-'))
    {
        return bad("characters outside A-Za-z0-9 . _ / -");
    }
    let mut parts = Vec::new();
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(p) => {
                let p = p.to_str().unwrap_or("");
                if p == ".git" {
                    return bad(".git component");
                }
                parts.push(p);
            }
            _ => return bad("absolute, . or .. component"),
        }
    }
    if parts.join("/") != rel {
        return bad("not normalized");
    }
    Ok(parts)
}

pub fn copy_untracked(src_repo: &Path, wt: &Worktree, files: &[String]) -> Result<(), String> {
    if !wt.clone {
        approved_path(&wt.repo, &wt.path)?;
    }
    let src_root =
        fs::canonicalize(src_repo).map_err(|e| format!("{}: {e}", src_repo.display()))?;
    let dst_root = fs::canonicalize(&wt.path).map_err(|e| format!("{}: {e}", wt.path.display()))?;
    let mut parsed = Vec::new();
    for rel in files {
        parsed.push((rel, relative_components(rel)?));
    }
    let src_fd = open_root(&src_root).map_err(|e| format!("{}: {e}", src_root.display()))?;
    let dst_fd = open_root(&dst_root).map_err(|e| format!("{}: {e}", dst_root.display()))?;
    let mut excluded = Vec::new();
    for (rel, parts) in parsed {
        let (name, dirs) = parts.split_last().ok_or("empty path")?;
        let mut sdir = src_fd.try_clone().map_err(|e| format!("{rel}: {e}"))?;
        for p in dirs {
            sdir = open_dir_at(sdir.as_fd(), p, false)
                .map_err(|e| format!("{rel}: source path: {e}"))?;
        }
        let raw = open_at(
            sdir.as_fd(),
            name,
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            0,
        )
        .map_err(|e| format!("{rel}: {e}"))?;
        let mut input = fs::File::from(raw);
        let meta = input.metadata().map_err(|e| format!("{rel}: {e}"))?;
        if !meta.is_file() || meta.size() > MAX_COPY {
            return Err(format!("{rel}: not a regular file of at most 1 MiB"));
        }
        let data = read_bounded(&mut input)
            .map_err(|e| format!("{rel}: {e}"))?
            .ok_or_else(|| format!("{rel}: larger than 1 MiB"))?;
        let mut ddir = dst_fd.try_clone().map_err(|e| format!("{rel}: {e}"))?;
        for p in dirs {
            ddir = open_dir_at(ddir.as_fd(), p, true)
                .map_err(|e| format!("{rel}: destination path: {e}"))?;
        }
        let raw = open_at(
            ddir.as_fd(),
            name,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
            0o600,
        )
        .map_err(|e| format!("{rel}: {e}"))?;
        fs::File::from(raw)
            .write_all(&data)
            .map_err(|e| format!("{rel}: {e}"))?;
        excluded.push(format!("/{rel}"));
    }
    let git = GitEnv::open(&wt.path)?;
    let owner = if wt.clone { &wt.path } else { &wt.repo };
    add_exclude(&git, &wt.path, owner, &excluded)
}

pub fn status(wt: &Worktree) -> Result<Status, String> {
    if !wt.clone {
        approved_path(&wt.repo, &wt.path)?;
    }
    let git = GitEnv::open(&wt.path)?;
    let dirty = git.run_raw(&["status", "--porcelain=v2", "-z", "--untracked-files=all"])?;
    if !dirty.status.success() {
        return Err("git status failed".to_string());
    }
    let branch_ref = format!("refs/heads/{}", wt.branch);
    let ahead = git
        .run(&["rev-list", "--count", &format!("{}..{branch_ref}", wt.base)])?
        .parse::<u32>()
        .map_err(|e| e.to_string())?;
    let remotes = git.run(&[
        "branch",
        "-r",
        "--contains",
        &branch_ref,
        "--format=%(refname)",
    ])?;
    let pushed = remotes
        .lines()
        .any(|l| !l.is_empty() && !l.ends_with("/HEAD"));
    let merged = git.run_raw(&["merge-base", "--is-ancestor", &branch_ref, &wt.base])?;
    let merged_into_base = match merged.status.code() {
        Some(0) => true,
        Some(1) => false,
        _ => return Err("cannot compare branch with its base".to_string()),
    };
    Ok(Status {
        clean: dirty.stdout.is_empty(),
        ahead,
        pushed,
        merged_into_base,
    })
}

pub fn remove(wt: &Worktree, force: bool) -> Result<(), String> {
    if wt.clone {
        return Err("clone-mode checkouts are not removed by the runner".to_string());
    }
    approved_path(&wt.repo, &wt.path)?;
    let st = status(wt)?;
    if !force {
        if !st.clean {
            return Err("worktree has uncommitted or untracked changes".to_string());
        }
        if !st.merged_into_base && !st.pushed {
            return Err("branch is neither merged into its base nor pushed".to_string());
        }
    }
    let git = GitEnv::open(&wt.repo)?;
    let path = utf8(&wt.path)?;
    if force {
        git.run(&["worktree", "remove", "--force", "--", path])?;
    } else {
        git.run(&["worktree", "remove", "--", path])?;
    }
    if st.merged_into_base {
        git.run(&["branch", "-D", "--", &wt.branch])
            .map_err(|e| format!("worktree removed, branch kept: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use tempfile::TempDir;

    fn author(g: GitEnv) -> GitEnv {
        g.env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        author(GitEnv::bare(dir)).run(args).unwrap()
    }

    fn commit_file(repo: &Path, name: &str, body: &str) {
        fs::write(repo.join(name), body).unwrap();
        git(repo, &["add", "--", name]);
        git(repo, &["commit", "-m", name]);
    }

    struct Fx {
        _tmp: TempDir,
        repo: PathBuf,
        outside: PathBuf,
    }

    fn fixture() -> Fx {
        let tmp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(tmp.path()).unwrap();
        let repo = root.join("proj");
        let outside = root.join("outside");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(&outside).unwrap();
        git(&repo, &["init", "-b", "main"]);
        commit_file(&repo, "a.txt", "a");
        Fx {
            _tmp: tmp,
            repo,
            outside,
        }
    }

    fn with_origin(fx: &Fx) -> PathBuf {
        let bare = fx.outside.join("origin.git");
        git(&fx.outside, &["init", "--bare", "-b", "main", "origin.git"]);
        git(
            &fx.repo,
            &["remote", "add", "origin", bare.to_str().unwrap()],
        );
        git(&fx.repo, &["push", "origin", "main"]);
        git(&fx.repo, &["fetch", "origin"]);
        git(&fx.repo, &["remote", "set-head", "origin", "main"]);
        bare
    }

    fn make(fx: &Fx, branch: &str) -> Worktree {
        let p = plan(&fx.repo, branch, None).unwrap();
        create(&p, false).unwrap()
    }

    #[test]
    fn plan_rejects_bad_branch_names() {
        let fx = fixture();
        for bad in [
            "",
            "-x",
            "a..b",
            "a//b",
            "a/",
            "a.lock",
            "a/b.lock",
            ".a",
            "a b",
            "a;b",
            "a$b",
            "HEAD",
            &"x".repeat(101),
        ] {
            assert!(plan(&fx.repo, bad, None).is_err(), "{bad:?}");
        }
        assert!(plan(&fx.repo, "feat/tm-1_x.y", None).is_ok());
    }

    #[test]
    fn plan_rejects_bad_base_and_existing_targets() {
        let fx = fixture();
        assert!(plan(&fx.repo, "ok", Some("--upload-pack=x")).is_err());
        git(&fx.repo, &["branch", "taken"]);
        assert!(plan(&fx.repo, "taken", None).is_err());
        let p = plan(&fx.repo, "feat/x", None).unwrap();
        fs::create_dir_all(&p.path).unwrap();
        assert!(plan(&fx.repo, "feat/x", None)
            .unwrap_err()
            .contains("exists"));
    }

    #[test]
    fn plan_resolves_paths_and_base() {
        let fx = fixture();
        let p = plan(&fx.repo, "feat/x", None).unwrap();
        assert_eq!(p.base, "main");
        assert_eq!(
            p.path,
            fx.repo.parent().unwrap().join("proj-worktrees/feat-x")
        );
        with_origin(&fx);
        assert_eq!(plan(&fx.repo, "y", None).unwrap().base, "origin/main");
        assert_eq!(plan(&fx.repo, "y", Some("main")).unwrap().base, "main");
    }

    #[test]
    fn plan_needs_a_repository_root() {
        let fx = fixture();
        let sub = fx.repo.join("sub");
        fs::create_dir(&sub).unwrap();
        assert!(plan(&sub, "x", None).is_err());
        assert!(plan(&fx.outside, "x", None).is_err());
    }

    #[test]
    fn plan_refuses_exec_and_include_config_keys() {
        for (key, val) in [
            ("include.path", "../x"),
            ("includeIf.gitdir:/x.path", "../x"),
            ("core.fsmonitor", "true"),
            ("core.sshCommand", "touch /nope"),
            ("extensions.worktreeConfig", "true"),
            ("credential.helper", "!touch /nope"),
            ("diff.x.textconv", "touch /nope"),
            ("merge.x.driver", "touch /nope"),
            ("remote.origin.uploadpack", "touch /nope"),
        ] {
            let fx = fixture();
            git(&fx.repo, &["config", key, val]);
            let err = plan(&fx.repo, "x", None).unwrap_err();
            assert!(err.contains("refused"), "{key}: {err}");
        }
    }

    fn plant_traps(fx: &Fx) -> [PathBuf; 4] {
        let marks = ["hook", "smudge", "clean", "process"].map(|n| fx.outside.join(n));
        let hooks = fx.repo.join(".git/hooks");
        fs::create_dir_all(&hooks).unwrap();
        let hook = hooks.join("post-checkout");
        fs::write(
            &hook,
            format!("#!/bin/sh\ntouch '{}'\n", marks[0].display()),
        )
        .unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(fx.repo.join("b.md"), "b").unwrap();
        fs::write(
            fx.repo.join(".gitattributes"),
            "*.txt filter=evil\n*.md filter=evil2\n",
        )
        .unwrap();
        git(&fx.repo, &["add", ".gitattributes", "b.md"]);
        git(&fx.repo, &["commit", "-m", "attrs"]);
        let cfg = |name: &str, var: &str, mark: &Path| {
            git(
                &fx.repo,
                &[
                    "config",
                    &format!("filter.{name}.{var}"),
                    &format!("sh -c 'touch {}; cat'", mark.display()),
                ],
            );
        };
        cfg("evil2", "smudge", &marks[1]);
        cfg("evil2", "clean", &marks[2]);
        cfg("evil", "process", &marks[3]);
        marks
    }

    #[test]
    fn traps_fire_with_plain_git_but_never_through_the_runner() {
        let fx = fixture();
        let marks = plant_traps(&fx);
        let control = fx.repo.parent().unwrap().join("control");
        let st = Command::new("git")
            .current_dir(&fx.repo)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args([
                "worktree",
                "add",
                "--detach",
                "--",
                control.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(st.status.success());
        assert!(
            marks[0].exists() && marks[1].exists() && marks[3].exists(),
            "control must fire: {:?}",
            marks.iter().map(|m| m.exists()).collect::<Vec<_>>()
        );
        for m in &marks {
            let _ = fs::remove_file(m);
        }
        let wt = make(&fx, "safe");
        assert!(wt.path.join("a.txt").exists());
        for m in &marks {
            assert!(!m.exists(), "{} was written", m.display());
        }
        assert!(wt.path.join("b.md").exists());
        fs::write(wt.path.join("a.txt"), "changed").unwrap();
        fs::write(wt.path.join("b.md"), "changed").unwrap();
        status(&wt).unwrap();
        for m in &marks {
            assert!(!m.exists(), "{} was written by status", m.display());
        }
    }

    #[test]
    fn create_adds_exclude_once_and_stays_in_the_sibling_root() {
        let fx = fixture();
        let wt = make(&fx, "feat/one");
        assert_eq!(
            wt.path,
            fx.repo.parent().unwrap().join("proj-worktrees/feat-one")
        );
        assert!(!wt.path.starts_with(&fx.repo));
        let exclude = fx.repo.join(".git/info/exclude");
        let git_env = GitEnv::open(&fx.repo).unwrap();
        add_exclude(&git_env, &fx.repo, &fx.repo, &[".tmgr/".to_string()]).unwrap();
        make(&fx, "feat/two");
        let text = fs::read_to_string(exclude).unwrap();
        assert_eq!(text.lines().filter(|l| *l == ".tmgr/").count(), 1);
        assert_eq!(git(&wt.path, &["branch", "--show-current"]), "feat/one");
    }

    #[test]
    fn exclude_refuses_a_symlinked_file() {
        let fx = fixture();
        let target = fx.outside.join("target");
        fs::write(&target, "").unwrap();
        let exclude = fx.repo.join(".git/info/exclude");
        let _ = fs::remove_file(&exclude);
        fs::create_dir_all(exclude.parent().unwrap()).unwrap();
        symlink(&target, &exclude).unwrap();
        let p = plan(&fx.repo, "x", None).unwrap();
        assert!(create(&p, false).is_ok());
        assert_eq!(fs::read_to_string(&target).unwrap(), "");
    }

    #[test]
    fn exclude_refuses_a_symlinked_info_directory() {
        let fx = fixture();
        let sink = fx.outside.join("info");
        fs::create_dir(&sink).unwrap();
        fs::write(sink.join("exclude"), "keep\n").unwrap();
        let info = fx.repo.join(".git/info");
        fs::remove_dir_all(&info).unwrap();
        symlink(&sink, &info).unwrap();
        let g = GitEnv::open(&fx.repo).unwrap();
        assert!(add_exclude(&g, &fx.repo, &fx.repo, &[".tmgr/".to_string()]).is_err());
        let p = plan(&fx.repo, "x", None).unwrap();
        assert!(create(&p, false).is_ok());
        assert_eq!(fs::read_to_string(sink.join("exclude")).unwrap(), "keep\n");
    }

    #[test]
    fn exclude_skips_a_common_dir_that_is_not_the_repo_git_dir() {
        let fx = fixture();
        let other = fx.outside.join("other");
        fs::create_dir(&other).unwrap();
        let g = GitEnv::open(&fx.repo).unwrap();
        assert!(add_exclude(&g, &fx.repo, &other, &[".tmgr/".to_string()]).is_err());
        assert!(!other.join(".git").exists());
    }

    #[test]
    fn copy_untracked_refuses_symlinked_parent_directories() {
        let fx = fixture();
        let wt = make(&fx, "cp");
        fs::create_dir(fx.outside.join("d")).unwrap();
        fs::write(fx.outside.join("d/f"), "s").unwrap();
        symlink(fx.outside.join("d"), fx.repo.join("sd")).unwrap();
        assert!(copy_untracked(&fx.repo, &wt, &["sd/f".into()]).is_err());
        fs::create_dir(fx.repo.join("dd")).unwrap();
        fs::write(fx.repo.join("dd/f"), "x").unwrap();
        fs::create_dir(fx.outside.join("sink")).unwrap();
        symlink(fx.outside.join("sink"), wt.path.join("dd")).unwrap();
        assert!(copy_untracked(&fx.repo, &wt, &["dd/f".into()]).is_err());
        assert!(!fx.outside.join("sink/f").exists());
    }

    #[test]
    fn create_fetches_from_origin_when_asked() {
        let fx = fixture();
        let bare = with_origin(&fx);
        let other = fx.outside.join("other");
        git(&fx.outside, &["clone", bare.to_str().unwrap(), "other"]);
        commit_file(&other, "b.txt", "b");
        git(&other, &["push", "origin", "main"]);
        let p = plan(&fx.repo, "fresh", None).unwrap();
        let wt = create(&p, true).unwrap();
        assert!(wt.path.join("b.txt").exists());
    }

    #[test]
    fn copy_untracked_copies_listed_files_with_private_mode() {
        let fx = fixture();
        let wt = make(&fx, "cp");
        fs::write(fx.repo.join(".env"), "S=1").unwrap();
        fs::create_dir(fx.repo.join("cfg")).unwrap();
        fs::write(fx.repo.join("cfg/local.toml"), "x").unwrap();
        copy_untracked(&fx.repo, &wt, &[".env".into(), "cfg/local.toml".into()]).unwrap();
        assert_eq!(fs::read_to_string(wt.path.join(".env")).unwrap(), "S=1");
        let mode = fs::metadata(wt.path.join("cfg/local.toml"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        assert!(status(&wt).unwrap().clean);
    }

    #[test]
    fn copy_untracked_enforces_grammar_symlinks_and_size() {
        let fx = fixture();
        let wt = make(&fx, "cp");
        fs::write(fx.outside.join("secret"), "s").unwrap();
        symlink(fx.outside.join("secret"), fx.repo.join("link")).unwrap();
        fs::create_dir(fx.repo.join("real")).unwrap();
        fs::write(fx.repo.join("real/f"), "f").unwrap();
        symlink(fx.repo.join("real"), fx.repo.join("dirlink")).unwrap();
        fs::write(fx.repo.join("big"), vec![0u8; (MAX_COPY + 1) as usize]).unwrap();
        fs::write(fx.repo.join("a2"), "x").unwrap();
        fs::write(wt.path.join("a2"), "taken").unwrap();
        symlink(fx.outside.join("sink"), wt.path.join("sinklink")).unwrap();
        fs::write(fx.repo.join("sinklink"), "x").unwrap();
        for bad in [
            "../x",
            "/etc/passwd",
            "a/../b",
            "./a",
            "a//b",
            "",
            ".git/config",
            "sub/.git/x",
            "link",
            "dirlink/f",
            "big",
            "missing",
            "real",
            "a2",
            "sinklink",
            "sp ace",
        ] {
            assert!(
                copy_untracked(&fx.repo, &wt, &[bad.into()]).is_err(),
                "{bad:?}"
            );
        }
        assert!(!fx.outside.join("sink").exists());
        assert_eq!(fs::read_to_string(wt.path.join("a2")).unwrap(), "taken");
    }

    #[test]
    fn status_and_remove_rules() {
        let fx = fixture();
        let bare = with_origin(&fx);
        let _ = bare;
        let wt = make(&fx, "work");
        let st = status(&wt).unwrap();
        assert_eq!(
            st,
            Status {
                clean: true,
                ahead: 0,
                pushed: true,
                merged_into_base: true
            }
        );

        fs::write(wt.path.join("new.txt"), "n").unwrap();
        assert!(!status(&wt).unwrap().clean);
        assert!(remove(&wt, false).unwrap_err().contains("changes"));
        git(&wt.path, &["add", "new.txt"]);
        git(&wt.path, &["commit", "-m", "work"]);
        let st = status(&wt).unwrap();
        assert!(st.clean && st.ahead == 1 && !st.pushed && !st.merged_into_base);
        assert!(remove(&wt, false).unwrap_err().contains("neither"));
        assert!(wt.path.exists());

        git(&wt.path, &["push", "origin", "work"]);
        let st = status(&wt).unwrap();
        assert!(st.pushed && !st.merged_into_base);
        remove(&wt, false).unwrap();
        assert!(!wt.path.exists());
        assert!(git(&fx.repo, &["branch", "--list", "work"]).contains("work"));
    }

    #[test]
    fn remove_deletes_merged_branch_and_force_overrides() {
        let fx = fixture();
        let wt = make(&fx, "done");
        git(&wt.path, &["commit", "--allow-empty", "-m", "c"]);
        git(&fx.repo, &["merge", "--ff-only", "done"]);
        assert!(status(&wt).unwrap().merged_into_base);
        remove(&wt, false).unwrap();
        assert!(!wt.path.exists());
        assert_eq!(git(&fx.repo, &["branch", "--list", "done"]), "");

        let wt = make(&fx, "wip");
        git(&wt.path, &["commit", "--allow-empty", "-m", "c"]);
        fs::write(wt.path.join("dirty"), "d").unwrap();
        assert!(remove(&wt, false).is_err());
        remove(&wt, true).unwrap();
        assert!(!wt.path.exists());
        assert!(git(&fx.repo, &["branch", "--list", "wip"]).contains("wip"));
    }

    #[test]
    fn remove_refuses_paths_outside_the_worktrees_root() {
        let fx = fixture();
        let wt = make(&fx, "ok");
        for bad in [fx.repo.clone(), fx.outside.clone(), fx.repo.join("nested")] {
            let evil = Worktree {
                path: bad,
                ..wt.clone()
            };
            assert!(remove(&evil, true).is_err());
            assert!(status(&evil).is_err());
        }
        assert!(fx.repo.exists() && wt.path.exists());
    }

    #[test]
    fn clone_mode_checks_out_a_branch_with_alternates() {
        let fx = fixture();
        let _ = plant_traps(&fx);
        let p = plan(&fx.repo, "untrusted", None).unwrap();
        let wt = clone_mode(&p).unwrap();
        assert!(wt.clone && wt.path.join("a.txt").exists());
        assert_eq!(git(&wt.path, &["branch", "--show-current"]), "untrusted");
        let alt = fs::read_to_string(wt.path.join(".git/objects/info/alternates")).unwrap();
        assert!(alt.contains(fx.repo.to_str().unwrap()));
        assert!(status(&wt).unwrap().clean);
        assert!(remove(&wt, true).is_err());
        for n in ["hook", "smudge", "clean", "process"] {
            assert!(!fx.outside.join(n).exists());
        }
    }
}
