use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tmgr_core::error::{Error, Result};

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

pub fn run_capture(
    program: &str,
    dir: Option<&Path>,
    args: &[&str],
    timeout: Duration,
) -> std::io::Result<Captured> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GH_PROMPT_DISABLED", "1")
        .env("LC_ALL", "C");
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
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

    pub fn run(&self, args: &[&str]) -> Result<Captured> {
        let mut full = vec!["-c", "core.fsmonitor=false"];
        full.extend_from_slice(args);
        run_capture("git", Some(&self.dir), &full, GIT_TIMEOUT).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                Error::internal("git was not found in PATH").hint("install git")
            }
            std::io::ErrorKind::TimedOut => Error::internal("git took too long and was stopped"),
            _ => Error::internal("could not run git"),
        })
    }

    pub fn run_ok(&self, args: &[&str], what: &str) -> Result<String> {
        let out = self.run(args)?;
        if out.ok {
            Ok(out.stdout)
        } else {
            Err(Error::internal(format!("git failed: {what}")))
        }
    }

    pub fn query(&self, args: &[&str]) -> Option<String> {
        let out = self.run(args).ok()?;
        let text = out.stdout.trim().to_string();
        (out.ok && !text.is_empty()).then_some(text)
    }

    pub fn toplevel(&self) -> Option<PathBuf> {
        self.query(&["rev-parse", "--show-toplevel"])
            .map(PathBuf::from)
    }

    pub fn current_branch(&self) -> Option<String> {
        self.query(&["symbolic-ref", "--short", "-q", "HEAD"])
    }

    pub fn is_dirty(&self) -> Result<bool> {
        let out = self.run_ok(&["status", "--porcelain", "-z"], "status")?;
        Ok(!out.is_empty())
    }

    pub fn branch_exists(&self, name: &str) -> bool {
        self.run(&[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{name}"),
        ])
        .map(|o| o.ok)
        .unwrap_or(false)
    }

    pub fn commit_exists(&self, base: &str) -> bool {
        self.run(&[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{base}^{{commit}}"),
        ])
        .map(|o| o.ok)
        .unwrap_or(false)
    }

    pub fn valid_branch_ref(&self, name: &str) -> bool {
        self.run(&["check-ref-format", "--branch", name])
            .map(|o| o.ok)
            .unwrap_or(false)
    }

    pub fn stash(&self, message: &str) -> Result<()> {
        self.run_ok(&["stash", "push", "-u", "-m", message], "stash")
            .map(|_| ())
    }

    pub fn create_branch(&self, name: &str, base: Option<&str>) -> Result<()> {
        let mut args = vec!["branch", "--no-track", "--", name];
        if let Some(b) = base {
            args.push(b);
        }
        self.run_ok(&args, "branch")?;
        self.switch(name)
    }

    pub fn switch(&self, name: &str) -> Result<()> {
        self.run_ok(&["switch", "--", name], "switch").map(|_| ())
    }

    pub fn origin_url(&self) -> Option<String> {
        self.query(&["remote", "get-url", "origin"])
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
    fn capture_times_out_and_reports_exit_status() {
        let ok = run_capture("sh", None, &["-c", "echo hi"], Duration::from_secs(5)).unwrap();
        assert!(ok.ok);
        assert_eq!(ok.stdout.trim(), "hi");
        let bad = run_capture("sh", None, &["-c", "exit 3"], Duration::from_secs(5)).unwrap();
        assert!(!bad.ok);
        let slow = run_capture("sh", None, &["-c", "sleep 5"], Duration::from_millis(100));
        assert_eq!(slow.err().unwrap().kind(), std::io::ErrorKind::TimedOut);
    }
}
