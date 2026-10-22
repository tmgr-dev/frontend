use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};
use tmgr_runner::attach::{parse_numstat, parse_pr, parse_status, GitInfo, PrLookup};

const GIT_TIMEOUT: Duration = Duration::from_secs(3);
const GH_TIMEOUT: Duration = Duration::from_secs(6);
const PR_EVERY: Duration = Duration::from_secs(90);
const MAX_OUT: usize = 256 * 1024;
const MAX_SCANS: usize = 64;

pub(super) struct GitJob {
    pub id: String,
    pub cwds: Vec<PathBuf>,
}

const SAFE: [&str; 6] = [
    "--no-optional-locks",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "core.hooksPath=/dev/null",
    "--no-pager",
];

fn git_args(extra: &[&str]) -> Vec<String> {
    SAFE.iter().chain(extra).map(|s| (*s).to_string()).collect()
}

fn run(program: &str, args: &[String], cwd: &Path, timeout: Duration) -> Option<(bool, String)> {
    use std::os::unix::process::CommandExt;
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_COUNT", "2")
        .env("GIT_CONFIG_KEY_0", "core.fsmonitor")
        .env("GIT_CONFIG_VALUE_0", "false")
        .env("GIT_CONFIG_KEY_1", "core.hooksPath")
        .env("GIT_CONFIG_VALUE_1", "/dev/null")
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_EXTERNAL_DIFF")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .ok()?;
    let pgid = child.id() as libc::pid_t;
    let (tx, rx) = std::sync::mpsc::channel();
    for (is_out, pipe) in [
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
    ] {
        let (tx, Some(mut pipe)) = (tx.clone(), pipe) else {
            continue;
        };
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = (&mut pipe).take(MAX_OUT as u64).read_to_end(&mut buf);
            let _ = tx.send((is_out, buf));
            let mut sink = [0u8; 4096];
            while pipe.read(&mut sink).is_ok_and(|n| n > 0) {}
        });
    }
    drop(tx);
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(15));
            }
            _ => break None,
        }
    };
    let kill = || unsafe {
        libc::killpg(pgid, libc::SIGKILL);
    };
    let Some(status) = status else {
        kill();
        let _ = child.wait();
        return None;
    };
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let end = Instant::now() + Duration::from_secs(1);
    for _ in 0..2 {
        match rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
            Ok((true, buf)) => stdout = buf,
            Ok((false, buf)) => stderr = buf,
            Err(_) => {
                kill();
                return None;
            }
        }
    }
    kill();
    let text = if status.success() { stdout } else { stderr };
    Some((
        status.success(),
        String::from_utf8_lossy(&text).into_owned(),
    ))
}

fn git(cwd: &Path, extra: &[&str]) -> Option<String> {
    let (ok, text) = run("git", &git_args(extra), cwd, GIT_TIMEOUT)?;
    ok.then_some(text)
}

pub(super) fn real_dir(path: &Path) -> bool {
    path.is_absolute() && std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
}

fn repo_root(cwd: &Path) -> Option<PathBuf> {
    let top = git(cwd, &["rev-parse", "--show-toplevel"])?;
    let root = PathBuf::from(top.trim_end_matches('\n'));
    (real_dir(&root) && cwd.starts_with(&root)).then_some(root)
}

fn base_of(root: &Path) -> Option<(String, u32)> {
    let mut candidates: Vec<String> = Vec::new();
    if let Some(head) = git(
        root,
        &["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"],
    ) {
        candidates.push(head.trim().to_string());
    }
    candidates.extend(["origin/main", "origin/master", "main", "master"].map(String::from));
    for base in candidates
        .into_iter()
        .filter(|b| !b.is_empty() && !b.starts_with('-'))
    {
        let range = format!("{base}..HEAD");
        let Some(n) = git(root, &["rev-list", "--count", &range]) else {
            continue;
        };
        if let Ok(n) = n.trim().parse() {
            return Some((base, n));
        }
    }
    None
}

fn lookup_pr(root: &Path) -> PrLookup {
    let args: Vec<String> = ["pr", "view", "--json", "number,title,url,state"]
        .map(String::from)
        .to_vec();
    match run("gh", &args, root, GH_TIMEOUT) {
        Some((true, text)) => parse_pr(&text).map_or(PrLookup::Unavailable, PrLookup::Found),
        Some((false, err)) if err.contains("no pull requests found") => PrLookup::NoPr,
        _ => PrLookup::Unavailable,
    }
}

fn config_runs_programs(list: &str) -> bool {
    list.lines().any(|line| {
        let Some((scope, key)) = line.split_once('\t') else {
            return false;
        };
        if !matches!(scope, "local" | "worktree" | "command") {
            return false;
        }
        let key = key.to_ascii_lowercase();
        let parts: Vec<&str> = key.split('.').collect();
        let last = parts.last().copied().unwrap_or("");
        key.starts_with("pager.")
            || key == "core.pager"
            || matches!(
                (parts.first().copied(), last),
                (Some("filter"), "clean" | "smudge" | "process")
                    | (Some("diff"), "textconv" | "command")
            )
    })
}

pub(super) fn collect(
    cwd: &Path,
    cached_pr: impl FnOnce(&str, &Path) -> PrLookup,
) -> Option<GitInfo> {
    if !real_dir(cwd) {
        return None;
    }
    let cwd = std::fs::canonicalize(cwd).ok()?;
    let root = repo_root(&cwd)?;
    if config_runs_programs(&git(
        &root,
        &["config", "--list", "--show-scope", "--name-only"],
    )?) {
        return None;
    }
    let status = parse_status(&git(&root, &["status", "--porcelain=v2", "--branch"])?);
    let (added, deleted) = git(
        &root,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--numstat",
            "HEAD",
        ],
    )
    .map(|t| parse_numstat(&t))
    .unwrap_or_default();
    let (base, ahead_base) = match base_of(&root) {
        Some((b, n)) => (Some(b), Some(n)),
        None => (None, None),
    };
    let pr = cached_pr(&status.branch, &root);
    Some(GitInfo {
        root: root.to_string_lossy().into_owned(),
        branch: status.branch,
        upstream: status.upstream,
        ahead: status.ahead,
        behind: status.behind,
        changed: status.changed,
        untracked: status.untracked,
        added,
        deleted,
        base,
        ahead_base,
        pr,
    })
}

pub(super) fn git_worker(
    jobs: Receiver<Vec<GitJob>>,
    mut send: impl FnMut(HashMap<String, GitInfo>) -> bool,
) {
    let mut prs: HashMap<String, (String, String, Instant, PrLookup)> = HashMap::new();
    for batch in jobs {
        if prs.len() > MAX_SCANS {
            prs.clear();
        }
        let mut found = HashMap::new();
        for job in &batch {
            let info = job.cwds.iter().find_map(|cwd| {
                collect(cwd, |branch, root| match prs.get(&job.id) {
                    Some((b, r, at, pr))
                        if b == branch && root == Path::new(r) && at.elapsed() < PR_EVERY =>
                    {
                        pr.clone()
                    }
                    _ => {
                        let pr = if branch.is_empty() || branch.starts_with('(') {
                            PrLookup::NoPr
                        } else {
                            lookup_pr(root)
                        };
                        prs.insert(
                            job.id.clone(),
                            (
                                branch.to_string(),
                                root.to_string_lossy().into_owned(),
                                Instant::now(),
                                pr.clone(),
                            ),
                        );
                        pr
                    }
                })
            });
            if let Some(info) = info {
                found.insert(job.id.clone(), info);
            }
        }
        if !send(found) {
            return;
        }
    }
}

pub(super) fn diff_argv(pager: Option<&str>) -> Vec<String> {
    let pager = pager
        .map(str::trim)
        .filter(|p| !p.is_empty() && !p.chars().any(char::is_control))
        .map_or_else(
            || {
                if on_path("delta") {
                    "delta".to_string()
                } else {
                    "less -R".to_string()
                }
            },
            str::to_string,
        );
    let mut argv = vec!["git".to_string()];
    argv.extend(SAFE.iter().take(5).map(|s| (*s).to_string()));
    argv.extend(
        ["-c", "color.ui=always", "-c"]
            .map(String::from)
            .into_iter()
            .chain([
                format!("core.pager={pager}"),
                "-c".to_string(),
                format!("pager.diff={pager}"),
            ]),
    );
    argv.extend(["diff", "--no-ext-diff", "--no-textconv", "HEAD"].map(String::from));
    argv
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| {
            std::fs::metadata(dir.join(program)).is_ok_and(|m| {
                use std::os::unix::fs::PermissionsExt;
                m.is_file() && m.permissions().mode() & 0o111 != 0
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn sh(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        sh(p, &["init", "-q", "-b", "main"]);
        std::fs::write(p.join("a.txt"), "one\n").unwrap();
        sh(p, &["add", "."]);
        sh(p, &["commit", "-q", "-m", "first"]);
        sh(p, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(p.join("a.txt"), "one\ntwo\n").unwrap();
        sh(p, &["commit", "-q", "-am", "second"]);
        std::fs::write(p.join("a.txt"), "one\ntwo\nthree\n").unwrap();
        std::fs::write(p.join("new.txt"), "x\n").unwrap();
        dir
    }

    fn canon(p: &Path) -> PathBuf {
        std::fs::canonicalize(p).unwrap()
    }

    #[test]
    fn a_repo_gives_branch_changes_and_commits_ahead_of_base() {
        let dir = repo();
        let root = canon(dir.path());
        let info = collect(&root, |_, _| PrLookup::NoPr).unwrap();
        assert_eq!(info.branch, "feature");
        assert_eq!(info.root, root.to_string_lossy());
        assert_eq!((info.changed, info.untracked), (1, 1));
        assert_eq!((info.added, info.deleted), (1, 0));
        assert_eq!(info.base.as_deref(), Some("main"));
        assert_eq!(info.ahead_base, Some(1));
        assert_eq!(info.pr, PrLookup::NoPr);
    }

    #[test]
    fn a_subdirectory_resolves_to_the_repo_root_and_a_plain_dir_to_none() {
        let dir = repo();
        let sub = canon(dir.path()).join("sub");
        std::fs::create_dir(&sub).unwrap();
        let info = collect(&sub, |_, _| PrLookup::NoPr).unwrap();
        assert_eq!(info.root, canon(dir.path()).to_string_lossy());
        let plain = tempfile::tempdir().unwrap();
        let no_repo = canon(plain.path());
        assert!(collect(&no_repo, |_, _| PrLookup::NoPr).is_none());
        assert!(collect(Path::new("relative"), |_, _| PrLookup::NoPr).is_none());
        assert!(collect(&no_repo.join("missing"), |_, _| PrLookup::NoPr).is_none());
    }

    #[test]
    fn git_does_not_write_the_index_or_run_a_repo_fsmonitor() {
        let dir = repo();
        let root = canon(dir.path());
        let marker = root.join("ran");
        sh(
            &root,
            &[
                "config",
                "core.fsmonitor",
                &format!("touch {}", marker.display()),
            ],
        );
        let index = std::fs::metadata(root.join(".git/index"))
            .unwrap()
            .modified()
            .unwrap();
        collect(&root, |_, _| PrLookup::NoPr).unwrap();
        assert!(!marker.exists());
        assert_eq!(
            std::fs::metadata(root.join(".git/index"))
                .unwrap()
                .modified()
                .unwrap(),
            index
        );
    }

    #[test]
    fn config_that_runs_programs_hides_the_panel() {
        assert!(config_runs_programs("local\tfilter.lfs.clean\n"));
        assert!(config_runs_programs("local\tdiff.x.textconv\n"));
        assert!(config_runs_programs("worktree\tpager.diff\n"));
        assert!(config_runs_programs("local\tcore.pager\n"));
        assert!(!config_runs_programs(
            "global\tfilter.lfs.clean\nlocal\tcore.bare\nlocal\tremote.origin.url\n"
        ));
    }

    #[test]
    fn a_repo_with_a_textconv_driver_is_not_inspected() {
        let dir = repo();
        let root = canon(dir.path());
        let marker = root.join("ran");
        sh(
            &root,
            &[
                "config",
                "filter.evil.clean",
                &format!("touch {}", marker.display()),
            ],
        );
        std::fs::write(root.join(".gitattributes"), "*.txt filter=evil\n").unwrap();
        assert!(collect(&root, |_, _| PrLookup::NoPr).is_none());
        assert!(!marker.exists());
    }

    #[test]
    fn a_grandchild_holding_the_pipe_cannot_hang_the_call() {
        let start = Instant::now();
        let args = ["-c".to_string(), "sleep 5 & echo hi".to_string()];
        let out = run("sh", &args, Path::new("/"), Duration::from_secs(3));
        assert!(start.elapsed() < Duration::from_secs(4));
        assert!(out.is_none() || out.is_some_and(|(ok, text)| ok && text.contains("hi")));
    }

    #[test]
    fn a_slow_command_is_killed_at_the_timeout() {
        let start = Instant::now();
        let out = run(
            "sleep",
            &["5".to_string()],
            Path::new("/"),
            Duration::from_millis(200),
        );
        assert!(out.is_none());
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(run("definitely-not-a-program", &[], Path::new("/"), GIT_TIMEOUT).is_none());
    }

    #[test]
    fn the_worker_reports_repos_only_and_caches_the_pr_lookup() {
        let dir = repo();
        let plain = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::sync_channel(1);
        let (out_tx, out_rx) = mpsc::channel();
        let handle = std::thread::spawn(move || git_worker(rx, move |m| out_tx.send(m).is_ok()));
        tx.send(vec![
            GitJob {
                id: "a".into(),
                cwds: vec![canon(plain.path()), canon(dir.path())],
            },
            GitJob {
                id: "b".into(),
                cwds: vec![canon(plain.path())],
            },
        ])
        .unwrap();
        let found = out_rx.recv().unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found["a"].branch, "feature");
        drop(tx);
        handle.join().unwrap();
    }

    #[test]
    fn the_diff_argv_is_safe_and_uses_the_pager_as_a_single_config_value() {
        let argv = diff_argv(Some("less -R"));
        assert_eq!(argv[0], "git");
        assert!(argv.contains(&"core.hooksPath=/dev/null".to_string()));
        assert!(argv.contains(&"core.fsmonitor=false".to_string()));
        assert!(argv.contains(&"core.pager=less -R".to_string()));
        assert_eq!(
            &argv[argv.len() - 4..],
            ["diff", "--no-ext-diff", "--no-textconv", "HEAD"]
        );
        assert!(argv.contains(&"pager.diff=less -R".to_string()));
        assert!(!argv.contains(&"--no-pager".to_string()));
        assert!(diff_argv(Some("bad\u{1b}pager"))
            .iter()
            .any(|a| a.starts_with("core.pager=") && !a.contains('\u{1b}')));
    }
}
