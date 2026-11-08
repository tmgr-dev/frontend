use std::ffi::OsString;
use std::io::Read;
#[cfg(all(unix, feature = "runner"))]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tmgr_core::error::{Error, Result};
#[cfg(all(unix, feature = "runner"))]
use tmgr_runner::hostview::{Access, HostView};

const MAX_OUTPUT: u64 = 1024 * 1024;
const GIT_TIMEOUT: Duration = Duration::from_secs(60);

pub struct Captured {
    pub ok: bool,
    pub stdout: String,
}

// A freshly written executable can still be open for writing in a concurrently forked child
// (ETXTBSY until that child execs); a short retry covers that window.
fn spawn_retrying_busy(cmd: &mut Command) -> std::io::Result<std::process::Child> {
    const ETXTBSY: i32 = 26;
    let mut attempt = 0;
    loop {
        match cmd.spawn() {
            Err(e) if e.raw_os_error() == Some(ETXTBSY) && attempt < 5 => {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(20 * attempt));
            }
            other => return other,
        }
    }
}

pub fn run_capture_with(
    program: &str,
    dir: Option<&Path>,
    args: &[&str],
    timeout: Duration,
    env: &[(&str, &Path)],
    env_remove: &[&str],
) -> std::io::Result<Captured> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GH_PROMPT_DISABLED", "1")
        .env("LC_ALL", "C");
    for k in env_remove {
        match k.strip_suffix('*') {
            Some(prefix) => {
                for (name, _) in std::env::vars_os() {
                    if name.to_string_lossy().starts_with(prefix) {
                        cmd.env_remove(name);
                    }
                }
            }
            None => {
                cmd.env_remove(k);
            }
        }
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    #[cfg(all(unix, feature = "runner"))]
    cmd.process_group(0);
    let mut child = spawn_retrying_busy(&mut cmd)?;
    let drain = |mut r: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = (&mut r).take(MAX_OUTPUT).read_to_end(&mut buf);
            let _ = std::io::copy(&mut r, &mut std::io::sink());
            buf
        })
    };
    let out_t = drain(Box::new(child.stdout.take().expect("piped stdout")));
    let err_t = drain(Box::new(child.stderr.take().expect("piped stderr")));
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if Instant::now() >= deadline {
            #[cfg(all(unix, feature = "runner"))]
            unsafe {
                libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            let _ = out_t.join();
            let _ = err_t.join();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "timed out",
            ));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let stdout = out_t.join().unwrap_or_default();
    let _ = err_t.join();
    Ok(Captured {
        ok: status.success(),
        stdout: String::from_utf8_lossy(&stdout).to_string(),
    })
}

pub fn git_program() -> &'static str {
    let no_path = std::env::var_os("PATH").is_none_or(|p| p.is_empty());
    if no_path && Path::new("/usr/bin/git").exists() {
        "/usr/bin/git"
    } else {
        "git"
    }
}

const SCRUBBED_ENV: [&str; 19] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_KEY_*",
    "GIT_CONFIG_VALUE_*",
    "GIT_SSH_COMMAND",
    "GIT_SSH",
    "GIT_ASKPASS",
    "SSH_ASKPASS",
    "GIT_ATTR_SOURCE",
    "GIT_EXTERNAL_DIFF",
    "GIT_PAGER",
    "GIT_EDITOR",
    "GIT_SEQUENCE_EDITOR",
];

const MUTATION_PINS: [&str; 16] = [
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "submodule.recurse=false",
    "-c",
    "gc.auto=0",
    "-c",
    "maintenance.auto=false",
    "-c",
    "core.untrackedCache=false",
    "-c",
    "core.alternateRefsCommand=: #",
    "-c",
    "diff.ignoreSubmodules=all",
];

fn git_error(e: std::io::Error) -> Error {
    match e.kind() {
        std::io::ErrorKind::NotFound => {
            Error::internal("git was not found in PATH").hint("install git")
        }
        std::io::ErrorKind::TimedOut => Error::internal("git took too long and was stopped"),
        _ => Error::internal("could not run git"),
    }
}

#[cfg(all(unix, feature = "runner"))]
fn safe_path(dir: &Path) -> Option<OsString> {
    Some(tmgr_runner::hostview::path_outside(dir))
}

#[cfg(not(all(unix, feature = "runner")))]
fn safe_path(_dir: &Path) -> Option<OsString> {
    None
}

pub fn config_read(dir: &Path, args: &[&str], timeout: Duration) -> Option<String> {
    let mut full = vec!["-c", "core.fsmonitor=false", "config"];
    if !args.contains(&"--show-scope") {
        full.push("--no-includes");
    }
    full.extend_from_slice(args);
    let path = safe_path(dir);
    let env: Vec<(&str, &Path)> = path.iter().map(|p| ("PATH", Path::new(p))).collect();
    let out = run_capture_with(
        git_program(),
        Some(dir),
        &full,
        timeout,
        &env,
        &SCRUBBED_ENV,
    )
    .ok()?;
    let text = out.stdout.trim().to_string();
    (out.ok && !text.is_empty()).then_some(text)
}

pub struct Reader {
    #[cfg(all(unix, feature = "runner"))]
    view: std::result::Result<HostView, String>,
    #[cfg(all(unix, feature = "runner"))]
    timeout: Duration,
    #[cfg(not(all(unix, feature = "runner")))]
    dir: PathBuf,
    #[cfg(not(all(unix, feature = "runner")))]
    timeout: Duration,
}

#[cfg(all(unix, feature = "runner"))]
impl Reader {
    pub fn open(dir: &Path, timeout: Duration, facts: bool) -> Reader {
        let view = if facts {
            HostView::discover_facts_within(dir, Access::Read, timeout)
        } else {
            HostView::discover_within(dir, Access::Read, timeout)
        };
        Reader { view, timeout }
    }

    pub fn repo(&self) -> Option<PathBuf> {
        self.view.as_ref().ok().map(|v| v.repo().to_path_buf())
    }

    pub fn hooks_dir(&self) -> Option<PathBuf> {
        self.view
            .as_ref()
            .ok()
            .map(|v| v.common_dir().join("hooks"))
    }

    pub fn run(&self, args: &[&str]) -> std::result::Result<String, String> {
        let view = self.view.as_ref().map_err(Clone::clone)?;
        match args {
            ["rev-parse", "--show-toplevel"] => Ok(view.repo().display().to_string()),
            ["rev-parse", "--is-inside-work-tree"] => Ok("true".into()),
            ["remote", "get-url", "origin"] => view
                .origin_url()
                .map(str::to_string)
                .ok_or_else(|| "origin is not set".to_string()),
            ["config", key] => config_read(view.repo(), &["--get", key], self.timeout)
                .ok_or_else(|| format!("{key} is not set")),
            _ => view.run(args),
        }
    }
}

#[cfg(not(all(unix, feature = "runner")))]
impl Reader {
    pub fn open(dir: &Path, timeout: Duration, _facts: bool) -> Reader {
        Reader {
            dir: dir.to_path_buf(),
            timeout,
        }
    }

    pub fn repo(&self) -> Option<PathBuf> {
        self.run(&["rev-parse", "--show-toplevel"])
            .ok()
            .filter(|t| !t.is_empty())
            .map(PathBuf::from)
    }

    pub fn hooks_dir(&self) -> Option<PathBuf> {
        let path = self
            .run(&["rev-parse", "--git-path", "hooks"])
            .ok()
            .filter(|t| !t.is_empty())?;
        let path = PathBuf::from(path);
        Some(if path.is_absolute() {
            path
        } else {
            self.dir.join(path)
        })
    }

    pub fn run(&self, args: &[&str]) -> std::result::Result<String, String> {
        let mut full = vec!["-c", "core.fsmonitor=false"];
        full.extend_from_slice(args);
        let out = run_capture_with(
            git_program(),
            Some(&self.dir),
            &full,
            self.timeout,
            &[],
            &[],
        )
        .map_err(|e| e.to_string())?;
        if out.ok {
            Ok(out.stdout.trim().to_string())
        } else {
            Err(format!("git {} failed", args.first().unwrap_or(&"")))
        }
    }
}

impl Reader {
    pub fn query(&self, args: &[&str]) -> Option<String> {
        self.run(args).ok().filter(|t| !t.is_empty())
    }
}

#[cfg(all(unix, feature = "runner"))]
fn config_key_allowed(key: &str) -> bool {
    let Some((section, rest)) = key.split_once('.') else {
        return false;
    };
    let (sub, var) = match rest.rsplit_once('.') {
        Some((sub, var)) => (Some(sub), var),
        None => (None, rest),
    };
    let section = section.to_ascii_lowercase();
    let var = var.to_ascii_lowercase();
    let var = var.as_str();
    match (section.as_str(), sub) {
        ("user" | "author" | "committer" | "gui" | "color" | "column", _) => true,
        ("core", None) => matches!(
            var,
            "repositoryformatversion"
                | "filemode"
                | "bare"
                | "logallrefupdates"
                | "ignorecase"
                | "precomposeunicode"
                | "symlinks"
                | "hookspath"
                | "autocrlf"
                | "eol"
                | "safecrlf"
                | "longpaths"
                | "sparsecheckout"
                | "sharedrepository"
                | "abbrev"
                | "compression"
                | "untrackedcache"
                | "commentchar"
                | "quotepath"
        ),
        ("advice", None) => true,
        ("status", None) => var != "submodulesummary",
        ("pull", None) => !matches!(var, "twohead" | "octopus"),
        ("push", None) => matches!(var, "default" | "autosetupremote" | "followtags"),
        ("fetch", None) => matches!(var, "prune" | "prunetags"),
        ("init", None) => var == "defaultbranch",
        ("merge", None) => matches!(var, "ff" | "conflictstyle"),
        ("rebase", None) => matches!(var, "autostash" | "autosquash" | "updaterefs"),
        ("rerere", None) => var == "enabled",
        ("commit", None) => matches!(var, "verbose" | "cleanup" | "gpgsign"),
        ("tag", None) => matches!(var, "gpgsign" | "sort"),
        ("maintenance", None) => matches!(var, "auto" | "strategy"),
        ("extensions", None) => matches!(
            var,
            "worktreeconfig" | "objectformat" | "refstorage" | "relativeworktrees"
        ),
        ("lfs", None) => matches!(var, "url" | "repositoryformatversion" | "locksverify"),
        ("lfs", Some(_)) => matches!(var, "access" | "locksverify"),
        ("branch", Some(_)) => matches!(
            var,
            "remote" | "merge" | "rebase" | "pushremote" | "description" | "vscode-merge-base"
        ),
        ("remote", Some(_)) => matches!(
            var,
            "url"
                | "pushurl"
                | "fetch"
                | "tagopt"
                | "prune"
                | "mirror"
                | "skipfetchall"
                | "gh-resolved"
        ),
        ("submodule", Some(_)) => matches!(var, "url" | "active"),
        _ => false,
    }
}

#[cfg(all(unix, feature = "runner"))]
fn checked_view(dir: &Path) -> Result<HostView> {
    let refuse =
        |why: String| Error::conflict(why).hint("remove the setting from the repository config");
    let view = HostView::discover_within(dir, Access::Read, GIT_TIMEOUT).map_err(|e| {
        refuse(format!(
            "the repository cannot be checked before changing it: {}",
            tmgr_core::text::sanitize_line(&e)
        ))
    })?;
    let worktree = view.worktree_config_entries().map_err(|e| {
        refuse(format!(
            "the repository cannot be checked before changing it: {}",
            tmgr_core::text::sanitize_line(&e)
        ))
    })?;
    for (key, _) in view.config_entries().iter().chain(worktree.iter()) {
        if !config_key_allowed(key) {
            return Err(refuse(format!(
                "the repository config sets {}, which can run programs",
                tmgr_core::text::sanitize_line(key)
            )));
        }
    }
    Ok(view)
}

#[derive(Clone)]
pub struct Git {
    dir: PathBuf,
}

impl Git {
    pub fn at(dir: impl Into<PathBuf>) -> Git {
        Git { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn reader(&self) -> Reader {
        Reader::open(&self.dir, GIT_TIMEOUT, false)
    }

    #[cfg(all(unix, feature = "runner"))]
    fn pinned_env(&self) -> Result<Vec<(&'static str, PathBuf)>> {
        let view = checked_view(&self.dir)?;
        Ok(vec![
            ("GIT_DIR", view.git_dir().to_path_buf()),
            ("GIT_COMMON_DIR", view.common_dir().to_path_buf()),
            ("GIT_WORK_TREE", view.repo().to_path_buf()),
            ("PATH", PathBuf::from(view.tool_path())),
        ])
    }

    #[cfg(not(all(unix, feature = "runner")))]
    fn pinned_env(&self) -> Result<Vec<(&'static str, PathBuf)>> {
        Ok(Vec::new())
    }

    fn mutate(&self, args: &[&str], what: &str) -> Result<String> {
        let mut pinned = self.pinned_env()?;
        pinned.push(("GIT_NO_LAZY_FETCH", PathBuf::from("1")));
        let env: Vec<(&str, &Path)> = pinned.iter().map(|(k, v)| (*k, v.as_path())).collect();
        let mut full: Vec<&str> = MUTATION_PINS.to_vec();
        full.extend_from_slice(args);
        let out = run_capture_with(
            git_program(),
            Some(&self.dir),
            &full,
            GIT_TIMEOUT,
            &env,
            &SCRUBBED_ENV,
        )
        .map_err(git_error)?;
        if out.ok {
            Ok(out.stdout)
        } else {
            Err(Error::internal(format!("git failed: {what}")))
        }
    }

    pub fn query(&self, args: &[&str]) -> Option<String> {
        self.reader().query(args)
    }

    pub fn toplevel(&self) -> Option<PathBuf> {
        self.reader().repo()
    }

    pub fn hooks_dir(&self) -> Option<PathBuf> {
        self.reader().hooks_dir()
    }

    pub fn config(&self, args: &[&str]) -> Option<String> {
        config_read(&self.dir, args, GIT_TIMEOUT)
    }

    pub fn current_branch(&self) -> Option<String> {
        self.query(&["symbolic-ref", "--short", "-q", "HEAD"])
    }

    pub fn is_dirty(&self) -> Result<bool> {
        let out = self.mutate(
            &["status", "--porcelain", "-z", "--ignore-submodules=all"],
            "status",
        )?;
        Ok(!out.is_empty())
    }

    pub fn branch_exists(&self, name: &str) -> bool {
        self.reader()
            .run(&[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{name}"),
            ])
            .is_ok()
    }

    pub fn commit_exists(&self, base: &str) -> bool {
        self.reader()
            .run(&[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{base}^{{commit}}"),
            ])
            .is_ok()
    }

    pub fn valid_branch_ref(&self, name: &str) -> bool {
        self.reader()
            .run(&["check-ref-format", "--branch", name])
            .is_ok()
    }

    fn tracks_nested_repository(&self) -> Result<bool> {
        for args in [&["ls-files", "--stage"][..], &["ls-tree", "-r", "HEAD"][..]] {
            let listing = self.mutate(args, "list tracked files")?;
            if listing.lines().any(|line| line.starts_with("160000 ")) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn stash(&self, message: &str) -> Result<()> {
        if self.tracks_nested_repository()? {
            return Err(Error::conflict("cannot stash a repository with submodules")
                .hint("commit or stash the changes yourself, then run this again"));
        }
        self.mutate(&["stash", "push", "-u", "-m", message], "stash")
            .map(|_| ())
    }

    pub fn create_branch(&self, name: &str, base: Option<&str>) -> Result<()> {
        let mut args = vec!["branch", "--no-track", "--", name];
        if let Some(b) = base {
            args.push(b);
        }
        self.mutate(&args, "branch")?;
        self.switch(name)
    }

    pub fn switch(&self, name: &str) -> Result<()> {
        self.mutate(&["switch", "--quiet", "--", name], "switch")
            .map(|_| ())
    }

    pub fn origin_url(&self) -> Option<String> {
        self.query(&["remote", "get-url", "origin"])
    }
}

pub struct Neutral {
    dir: tempfile::TempDir,
    path: Option<OsString>,
}

impl Neutral {
    pub fn new() -> Result<Neutral> {
        tempfile::tempdir()
            .map(|dir| Neutral { dir, path: None })
            .map_err(|_| Error::internal("could not make a temporary folder"))
    }

    pub fn outside(repo: &Path) -> Result<Neutral> {
        let mut neutral = Neutral::new()?;
        neutral.path = safe_path(repo);
        Ok(neutral)
    }

    pub fn git(&self, args: &[&str]) -> Result<Captured> {
        self.program(git_program(), args, GIT_TIMEOUT)
    }

    pub fn program(&self, program: &str, args: &[&str], timeout: Duration) -> Result<Captured> {
        let ceiling = self.dir.path().parent().unwrap_or(self.dir.path());
        let mut env = vec![("GIT_CEILING_DIRECTORIES", ceiling)];
        if let Some(path) = &self.path {
            env.push(("PATH", Path::new(path)));
        }
        run_capture_with(
            program,
            Some(self.dir.path()),
            args,
            timeout,
            &env,
            &SCRUBBED_ENV,
        )
        .map_err(git_error)
    }

    pub fn query(&self, args: &[&str]) -> Option<String> {
        let out = self.git(args).ok()?;
        let text = out.stdout.trim().to_string();
        (out.ok && !text.is_empty()).then_some(text)
    }

    pub fn run_ok(&self, args: &[&str], what: &str) -> Result<String> {
        let out = self.git(args)?;
        if out.ok {
            Ok(out.stdout)
        } else {
            Err(Error::internal(format!("git failed: {what}")))
        }
    }
}

pub fn cwd_repo() -> Option<Git> {
    let cwd = std::env::current_dir().ok()?;
    let top = Git::at(cwd).toplevel()?;
    Some(Git::at(top))
}

pub fn repo_name(git: &Git) -> String {
    git.dir()
        .file_name()
        .map(|n| tmgr_core::text::sanitize_line(&n.to_string_lossy()))
        .unwrap_or_default()
}

#[derive(Debug, PartialEq, Eq)]
pub struct Remote {
    pub host: String,
    pub path: String,
}

pub fn parse_remote(url: &str) -> Option<Remote> {
    let url = url.trim();
    let (host_part, path) = if let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .or_else(|| url.strip_prefix("ssh://"))
        .or_else(|| url.strip_prefix("git://"))
    {
        let (authority, path) = rest.split_once('/')?;
        (authority.to_string(), path.to_string())
    } else if !url.contains("://") {
        let (authority, path) = url.split_once(':')?;
        (authority.to_string(), path.to_string())
    } else {
        return None;
    };
    let host_port = host_part.rsplit('@').next()?;
    let host = host_port.split(':').next()?.to_ascii_lowercase();
    let host_ok = !host.is_empty()
        && host.len() <= 100
        && host
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-');
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path).to_string();
    let segments: Vec<&str> = path.split('/').collect();
    let path_ok = segments.len() >= 2
        && segments.iter().all(|s| {
            !s.is_empty()
                && *s != "."
                && *s != ".."
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        });
    (host_ok && path_ok).then_some(Remote { host, path })
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn compare_url(remote: &Remote, branch: &str) -> Option<String> {
    let host = &remote.host;
    if host == "github.com" || host.starts_with("github.") {
        Some(format!(
            "https://{host}/{}/compare/{}?expand=1",
            remote.path,
            encode(branch)
        ))
    } else if host == "gitlab.com" || host.starts_with("gitlab.") {
        Some(format!(
            "https://{host}/{}/-/merge_requests/new?merge_request%5Bsource_branch%5D={}",
            remote.path,
            encode(branch).replace('/', "%2F")
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(host: &str, path: &str) -> Option<Remote> {
        Some(Remote {
            host: host.into(),
            path: path.into(),
        })
    }

    #[test]
    fn remotes_parse_in_every_common_form() {
        assert_eq!(
            parse_remote("https://github.com/acme/site.git"),
            r("github.com", "acme/site")
        );
        assert_eq!(
            parse_remote("git@github.com:acme/site.git"),
            r("github.com", "acme/site")
        );
        assert_eq!(
            parse_remote("ssh://git@gitlab.com:2222/grp/sub/site.git"),
            r("gitlab.com", "grp/sub/site")
        );
        assert_eq!(
            parse_remote("https://user:tok3n@github.com/acme/site"),
            r("github.com", "acme/site")
        );
        assert_eq!(
            parse_remote("https://GitHub.com/acme/site/"),
            r("github.com", "acme/site")
        );
    }

    #[test]
    fn odd_remotes_are_rejected() {
        for bad in [
            "",
            "/srv/git/site.git",
            "file:///x/y",
            "https://host/onlyone",
            "https://h/a/../b",
            "https://h o/a/b",
            "ftp://h/a/b",
            "https://h/a/b c",
        ] {
            assert_eq!(parse_remote(bad), None, "{bad}");
        }
    }

    #[test]
    fn credentials_never_reach_the_compare_url() {
        let remote = parse_remote("https://user:tok3n@github.com/acme/site.git").unwrap();
        let url = compare_url(&remote, "tm-1-x").unwrap();
        assert_eq!(url, "https://github.com/acme/site/compare/tm-1-x?expand=1");
        assert!(!url.contains("tok3n"));
    }

    #[test]
    fn compare_urls_for_github_and_gitlab() {
        let gh = parse_remote("git@github.com:acme/site.git").unwrap();
        assert_eq!(
            compare_url(&gh, "feat/tm-1").unwrap(),
            "https://github.com/acme/site/compare/feat/tm-1?expand=1"
        );
        let gl = parse_remote("git@gitlab.com:grp/site.git").unwrap();
        assert_eq!(
            compare_url(&gl, "feat/tm-1").unwrap(),
            "https://gitlab.com/grp/site/-/merge_requests/new?merge_request%5Bsource_branch%5D=feat%2Ftm-1"
        );
        let other = parse_remote("git@bitbucket.org:a/b.git").unwrap();
        assert_eq!(compare_url(&other, "x"), None);
    }

    #[test]
    fn branches_are_percent_encoded() {
        let gh = parse_remote("git@github.com:a/b.git").unwrap();
        assert_eq!(
            compare_url(&gh, "x y#z").unwrap(),
            "https://github.com/a/b/compare/x%20y%23z?expand=1"
        );
    }

    #[test]
    fn scrubbed_variables_are_removed_by_name_and_by_prefix() {
        std::env::set_var("TMGR_SCRUB_TEST_A", "1");
        let out = run_capture_with(
            "sh",
            None,
            &["-c", "env"],
            Duration::from_secs(5),
            &[],
            &["TMGR_SCRUB_TEST_*"],
        )
        .unwrap();
        assert!(!out.stdout.contains("TMGR_SCRUB_TEST_A"));
        for name in [
            "GIT_SSH_COMMAND",
            "GIT_SSH",
            "GIT_ASKPASS",
            "SSH_ASKPASS",
            "GIT_ATTR_SOURCE",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_KEY_*",
            "GIT_CONFIG_VALUE_*",
            "GIT_EXTERNAL_DIFF",
            "GIT_PAGER",
            "GIT_EDITOR",
        ] {
            assert!(SCRUBBED_ENV.contains(&name), "{name}");
        }
    }

    #[cfg(all(unix, feature = "runner"))]
    #[test]
    fn a_timeout_stops_a_grandchild_that_holds_the_pipe() {
        let began = Instant::now();
        let slow = run_capture_with(
            "sh",
            None,
            &["-c", "sleep 30 & sleep 30"],
            Duration::from_millis(200),
            &[],
            &[],
        );
        assert_eq!(slow.err().unwrap().kind(), std::io::ErrorKind::TimedOut);
        assert!(began.elapsed() < Duration::from_secs(10));
    }

    #[cfg(all(unix, feature = "runner"))]
    #[test]
    fn only_inert_config_keys_are_allowed() {
        for ok in [
            "core.hooksPath",
            "CORE.FileMode",
            "user.name",
            "branch.feat/x.remote",
            "remote.origin.URL",
            "remote.origin.gh-resolved",
            "lfs.repositoryformatversion",
            "lfs.https://example.com/a/b/info/lfs.access",
            "submodule.lib.active",
            "extensions.worktreeConfig",
            "advice.detachedHead",
            "ADVICE.statusHints",
            "status.showUntrackedFiles",
            "status.relativePaths",
        ] {
            assert!(config_key_allowed(ok), "{ok}");
        }
        for bad in [
            "lfs.extension.x.clean",
            "lfs.customtransfer.x.path",
            "remote.x.promisor",
            "remote.origin.proxy",
            "extensions.partialclone",
            "protocol.ext.allow",
            "url.x.insteadof",
            "filter.x.clean",
            "core.sshCommand",
            "core.worktree",
            "include.path",
            "pull.twohead",
            "branch.autosetupmerge",
            "nodots",
            "advice.x.y",
            "status.x.short",
            "status.submoduleSummary",
            "filter.x.required",
            "alias.st",
            "http.proxy",
            "gc.auto",
            "core.excludesFile",
        ] {
            assert!(!config_key_allowed(bad), "{bad}");
        }
    }

    #[test]
    fn capture_times_out_and_reports_exit_status() {
        let ok = run_capture_with(
            "sh",
            None,
            &["-c", "echo hi"],
            Duration::from_secs(5),
            &[],
            &[],
        )
        .unwrap();
        assert!(ok.ok);
        assert_eq!(ok.stdout.trim(), "hi");
        let bad = run_capture_with(
            "sh",
            None,
            &["-c", "exit 3"],
            Duration::from_secs(5),
            &[],
            &[],
        )
        .unwrap();
        assert!(!bad.ok);
        let slow = run_capture_with(
            "sh",
            None,
            &["-c", "sleep 5"],
            Duration::from_millis(100),
            &[],
            &[],
        );
        assert_eq!(slow.err().unwrap().kind(), std::io::ErrorKind::TimedOut);
    }
}
