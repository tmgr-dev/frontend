use std::process::{Command, Stdio};
pub use tmgr_core::agentwork::parse_ts;
use tmgr_core::agentwork::GitCommit;
use tmgr_core::runstate::MAX_COMMITS;

const WINDOW_GRACE_S: i64 = 60;
const LOG_LIMIT: &str = "500";

pub type Git<'a> = &'a dyn Fn(&[&str]) -> Option<String>;

pub fn parse_origin(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    let url = url.strip_suffix(".git").unwrap_or(url);
    let path = match url.split_once("://") {
        Some((_, rest)) => rest.split_once('/')?.1,
        None => url.split_once(':')?.1,
    };
    let mut parts = path.rsplit('/').filter(|p| !p.is_empty());
    let name = parts.next()?;
    let owner = parts.next()?;
    Some(format!("{owner}/{name}"))
}

fn known_sha(known: &[String], sha: &str) -> bool {
    known.iter().any(|k| {
        let k = k.split_whitespace().next().unwrap_or("");
        k.len() >= 7 && sha.starts_with(k)
    })
}

pub fn collect(git: Git, started: i64, ended: i64, known: &[String]) -> Vec<GitCommit> {
    let Some(email) = git(&["config", "user.email"]) else {
        return Vec::new();
    };
    let since = format!("--since={}", started - WINDOW_GRACE_S);
    let args = [
        "log",
        "--all",
        "-n",
        LOG_LIMIT,
        since.as_str(),
        "--format=%H%x1f%ae%x1f%ct%x1f%s",
    ];
    let Some(out) = git(&args) else {
        return Vec::new();
    };
    let repo = git(&["remote", "get-url", "origin"]).and_then(|u| parse_origin(&u));
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]).filter(|b| b != "HEAD");
    let until = ended + WINDOW_GRACE_S;
    let mut found: Vec<GitCommit> = out
        .lines()
        .filter_map(|line| {
            let mut f = line.split('\u{1f}');
            let sha = f.next()?;
            let author = f.next()?;
            let at: i64 = f.next()?.parse().ok()?;
            let subject = f.next().unwrap_or("");
            let mine = author.eq_ignore_ascii_case(email.trim());
            let inside = (started..=until).contains(&at);
            (mine && inside && !known_sha(known, sha)).then(|| GitCommit {
                sha: sha.to_string(),
                message: subject.to_string(),
                repo: repo.clone(),
                branch: branch.clone(),
            })
        })
        .collect();
    found.reverse();
    if found.len() > MAX_COMMITS {
        found.drain(..found.len() - MAX_COMMITS);
    }
    found
}

pub fn auto_upload_args(task_id: u64) -> Vec<String> {
    ["usage", "sync", "--auto", "--task-id"]
        .iter()
        .map(|s| s.to_string())
        .chain([task_id.to_string(), "-q".to_string()])
        .collect()
}

pub fn spawn_detached(args: &[String]) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let _ = cmd.spawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn sh(dir: &Path, args: &[&str], date: Option<&str>, email: &str) -> String {
        let mut c = Command::new("git");
        c.args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "T")
            .env("GIT_AUTHOR_EMAIL", email)
            .env("GIT_COMMITTER_NAME", "T")
            .env("GIT_COMMITTER_EMAIL", email);
        if let Some(d) = date {
            c.env("GIT_AUTHOR_DATE", d).env("GIT_COMMITTER_DATE", d);
        }
        let out = c.output().unwrap();
        assert!(out.status.success(), "{args:?}");
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    fn commit(dir: &Path, msg: &str, at: i64, email: &str) -> String {
        std::fs::write(dir.join(format!("{msg}.txt")), msg).unwrap();
        sh(dir, &["add", "."], None, email);
        sh(
            dir,
            &["commit", "-q", "-m", msg],
            Some(&format!("{at} +0000")),
            email,
        );
        sh(dir, &["rev-parse", "HEAD"], None, email)
    }

    fn runner(dir: &Path) -> impl Fn(&[&str]) -> Option<String> + '_ {
        move |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(dir)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .output()
                .ok()?;
            let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
            (out.status.success() && !text.is_empty()).then_some(text)
        }
    }

    const ME: &str = "me@example.com";
    const T0: i64 = 1_800_000_000;

    fn repo() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        let d = t.path();
        sh(d, &["init", "-q", "-b", "main", "."], None, ME);
        sh(d, &["config", "user.email", ME], None, ME);
        sh(
            d,
            &["remote", "add", "origin", "git@github.com:acme/tool.git"],
            None,
            ME,
        );
        commit(d, "before", T0 - 3600, ME);
        t
    }

    #[test]
    fn only_my_commits_inside_the_interval_on_any_branch_are_collected() {
        let t = repo();
        let d = t.path();
        let a = commit(d, "inside-a", T0 + 100, ME);
        commit(d, "other-author", T0 + 150, "you@example.com");
        sh(d, &["checkout", "-q", "-b", "side"], None, ME);
        let b = commit(d, "inside-b", T0 + 200, ME);
        sh(d, &["checkout", "-q", "main"], None, ME);
        let edge = commit(d, "grace", T0 + 1000 + 30, ME);
        commit(d, "after", T0 + 1000 + 600, ME);
        let got = collect(&runner(d), T0, T0 + 1000, &[]);
        let shas: Vec<&str> = got.iter().map(|c| c.sha.as_str()).collect();
        assert_eq!(shas, [a.as_str(), b.as_str(), edge.as_str()]);
        assert_eq!(got[0].message, "inside-a");
        assert_eq!(got[0].repo.as_deref(), Some("acme/tool"));
        assert_eq!(got[0].branch.as_deref(), Some("main"));
    }

    #[test]
    fn commits_the_run_already_lists_are_skipped_by_short_sha() {
        let t = repo();
        let d = t.path();
        let a = commit(d, "inside-a", T0 + 100, ME);
        let b = commit(d, "inside-b", T0 + 200, ME);
        let known = vec![format!("{} reported", &a[..7])];
        let got = collect(&runner(d), T0, T0 + 1000, &known);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].sha, b);
    }

    #[test]
    fn no_configured_email_or_no_git_means_nothing() {
        let none = |_: &[&str]| None;
        assert!(collect(&none, T0, T0 + 10, &[]).is_empty());
        let t = repo();
        let no_email = |args: &[&str]| {
            if args.first() == Some(&"config") {
                None
            } else {
                runner(t.path())(args)
            }
        };
        assert!(collect(&no_email, T0 - 7200, T0 + 10, &[]).is_empty());
    }

    #[test]
    fn origin_urls_become_owner_slash_name() {
        for (url, want) in [
            ("https://github.com/acme/tool.git", Some("acme/tool")),
            ("https://github.com/acme/tool", Some("acme/tool")),
            ("git@github.com:acme/tool.git", Some("acme/tool")),
            ("ssh://git@github.com/acme/tool.git", Some("acme/tool")),
            ("https://host.example/group/sub/tool.git/", Some("sub/tool")),
            ("/srv/git/tool.git", None),
            ("tool", None),
        ] {
            assert_eq!(parse_origin(url).as_deref(), want, "{url}");
        }
    }

    #[test]
    fn the_auto_upload_command_line_is_fixed() {
        assert_eq!(
            auto_upload_args(8974),
            ["usage", "sync", "--auto", "--task-id", "8974", "-q"]
        );
    }

    #[test]
    fn timestamps_parse_rfc3339_with_offsets() {
        assert_eq!(parse_ts("2026-10-21T10:00:00Z"), Some(1_792_576_800));
        assert_eq!(parse_ts("2026-10-21T12:00:00+02:00"), Some(1_792_576_800));
        assert_eq!(parse_ts("yesterday"), None);
    }
}
