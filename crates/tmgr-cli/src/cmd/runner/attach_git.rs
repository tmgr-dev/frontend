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
    pub branch_only: bool,
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

fn pin_git_env(cmd: &mut Command, cwd: &Path, git_dir: Option<&Path>) {
    match git_dir {
        Some(dir) => cmd.env("GIT_DIR", dir).env("GIT_WORK_TREE", cwd),
        None => cmd.env_remove("GIT_DIR").env_remove("GIT_WORK_TREE"),
    };
    cmd.current_dir(cwd)
        .env("PATH", tmgr_queue::workdir::absolute_path())
        .env("GIT_NO_LAZY_FETCH", "1")
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
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_EXTERNAL_DIFF")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_NAMESPACE");
}

fn run(
    program: &str,
    args: &[String],
    cwd: &Path,
    git_dir: Option<&Path>,
    timeout: Duration,
) -> Option<(bool, String)> {
    use std::os::unix::process::CommandExt;
    let mut cmd = Command::new(program);
    pin_git_env(&mut cmd, cwd, git_dir);
    let mut child = cmd
        .args(args)
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
    let exited = loop {
        match tmgr_runner::worktree::exited_unreaped(pgid) {
            Ok(true) => break true,
            Ok(false) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(15));
            }
            _ => break false,
        }
    };
    if !exited {
        unsafe { libc::killpg(pgid, libc::SIGKILL) };
        let _ = child.wait();
        return None;
    }
    tmgr_runner::worktree::kill_group_of_unreaped(pgid);
    let status = child.wait().ok()?;
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let end = Instant::now() + Duration::from_secs(1);
    for _ in 0..2 {
        match rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
            Ok((true, buf)) => stdout = buf,
            Ok((false, buf)) => stderr = buf,
            Err(_) => return None,
        }
    }
    let text = if status.success() { stdout } else { stderr };
    Some((
        status.success(),
        String::from_utf8_lossy(&text).into_owned(),
    ))
}

fn git(root: &Path, git_dir: &Path, extra: &[&str]) -> Option<String> {
    let (ok, text) = run("git", &git_args(extra), root, Some(git_dir), GIT_TIMEOUT)?;
    ok.then_some(text)
}

pub(super) fn real_dir(path: &Path) -> bool {
    path.is_absolute() && std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
}

fn work_root(cwd: &Path) -> Option<&Path> {
    cwd.ancestors()
        .find(|dir| std::fs::symlink_metadata(dir.join(".git")).is_ok())
}

fn base_of(root: &Path, git_dir: &Path) -> Option<(String, u32)> {
    let mut candidates: Vec<String> = Vec::new();
    if let Some(head) = git(
        root,
        git_dir,
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
        let Some(n) = git(root, git_dir, &["rev-list", "--count", &range]) else {
            continue;
        };
        if let Ok(n) = n.trim().parse() {
            return Some((base, n));
        }
    }
    None
}

fn lookup_pr(root: &Path, git_dir: &Path) -> PrLookup {
    let args: Vec<String> = ["pr", "view", "--json", "number,title,url,state"]
        .map(String::from)
        .to_vec();
    match run("gh", &args, root, Some(git_dir), GH_TIMEOUT) {
        Some((true, text)) => parse_pr(&text).map_or(PrLookup::Unavailable, PrLookup::Found),
        Some((false, err)) if err.contains("no pull requests found") => PrLookup::NoPr,
        _ => PrLookup::Unavailable,
    }
}

pub(super) fn collect(
    cwd: &Path,
    cached_pr: impl FnOnce(&str, &Path, &Path) -> PrLookup,
) -> Option<GitInfo> {
    if !real_dir(cwd) {
        return None;
    }
    let cwd = std::fs::canonicalize(cwd).ok()?;
    let root = work_root(&cwd)?.to_path_buf();
    let host = tempfile::tempdir().ok()?;
    tmgr_queue::workdir::host_git_dir(&root, host.path())?;
    let git_dir = host.path();
    let status = parse_status(&git(
        &root,
        git_dir,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "--ignore-submodules=all",
        ],
    )?);
    let (added, deleted) = git(
        &root,
        git_dir,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--ignore-submodules=all",
            "--numstat",
            "HEAD",
        ],
    )
    .map(|t| parse_numstat(&t))
    .unwrap_or_default();
    let (base, ahead_base) = match base_of(&root, git_dir) {
        Some((b, n)) => (Some(b), Some(n)),
        None => (None, None),
    };
    let pr = cached_pr(&status.branch, &root, git_dir);
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
        branch_only: false,
    })
}

fn read_small_file(path: &Path) -> Option<String> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)
        .ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut text = String::new();
    (&mut file).take(4096).read_to_string(&mut text).ok()?;
    Some(text)
}

fn branch_of_head(head: &str) -> String {
    head.trim_end()
        .strip_prefix("ref: refs/heads/")
        .filter(|name| {
            !name.is_empty()
                && name.len() <= 100
                && !name.chars().any(|c| c.is_control() || c.is_whitespace())
        })
        .unwrap_or("(detached)")
        .to_string()
}

pub(super) fn collect_branch_only(cwd: &Path) -> Option<GitInfo> {
    if !real_dir(cwd) {
        return None;
    }
    let cwd = std::fs::canonicalize(cwd).ok()?;
    let root = work_root(&cwd)?;
    let (git_dir, _) = tmgr_queue::workdir::expected_git_dir(root)?;
    let head = read_small_file(&git_dir.join("HEAD"))?;
    Some(GitInfo {
        root: root.to_string_lossy().into_owned(),
        branch: branch_of_head(&head),
        branch_only: true,
        pr: PrLookup::Unavailable,
        ..GitInfo::default()
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
                if job.branch_only {
                    return collect_branch_only(cwd);
                }
                collect(cwd, |branch, root, git_dir| match prs.get(&job.id) {
                    Some((b, r, at, pr))
                        if b == branch && root == Path::new(r) && at.elapsed() < PR_EVERY =>
                    {
                        pr.clone()
                    }
                    _ => {
                        let pr = if branch.is_empty() || branch.starts_with('(') {
                            PrLookup::NoPr
                        } else {
                            lookup_pr(root, git_dir)
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
    argv.extend(
        [
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--ignore-submodules=all",
            "HEAD",
        ]
        .map(String::from),
    );
    argv
}

pub(super) fn diff_pane_argv(exe: &Path, root: &str, pager: Option<&str>) -> Vec<String> {
    let mut argv = vec![
        exe.display().to_string(),
        "runner".to_string(),
        "git-diff".to_string(),
        "--dir".to_string(),
        root.to_string(),
    ];
    if let Some(pager) = pager {
        argv.extend(["--pager".to_string(), pager.to_string()]);
    }
    argv
}

pub(super) fn diff_in_pane(dir: &str, pager: Option<&str>) -> tmgr_core::error::Result<i32> {
    use tmgr_core::error::Error;
    let root = Path::new(dir);
    if !real_dir(root) {
        return Err(Error::usage("the folder to diff does not exist"));
    }
    let root = std::fs::canonicalize(root)?;
    let host = tempfile::tempdir()?;
    if tmgr_queue::workdir::host_git_dir(&root, host.path()).is_none() {
        return Err(Error::usage(
            "this folder's git settings are not plain, so the diff is not shown",
        ));
    }
    let argv = diff_argv(pager);
    let mut cmd = Command::new(&argv[0]);
    pin_git_env(&mut cmd, &root, Some(host.path()));
    match std::env::var_os("NO_COLOR") {
        Some(v) => cmd.env("NO_COLOR", v),
        None => cmd.env_remove("NO_COLOR"),
    };
    let mut child = cmd.args(&argv[1..]).spawn()?;
    for sig in [libc::SIGINT, libc::SIGHUP, libc::SIGTERM, libc::SIGQUIT] {
        unsafe { libc::signal(sig, libc::SIG_IGN) };
    }
    let status = child.wait()?;
    drop(host);
    Ok(status.code().unwrap_or(1))
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
        let info = collect(&root, |_, _, _| PrLookup::NoPr).unwrap();
        assert_eq!(info.branch, "feature");
        assert_eq!(info.root, root.to_string_lossy());
        assert_eq!((info.changed, info.untracked), (1, 1));
        assert_eq!((info.added, info.deleted), (1, 0));
        assert_eq!(info.base.as_deref(), Some("main"));
        assert_eq!(info.ahead_base, Some(1));
        assert_eq!(info.pr, PrLookup::NoPr);
    }

    #[test]
    fn a_session_checkout_shows_its_branch_and_commits_ahead_of_the_base() {
        let dir = tempfile::tempdir().unwrap();
        let repo = canon(dir.path()).join("proj");
        std::fs::create_dir(&repo).unwrap();
        sh(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("a"), "a").unwrap();
        sh(&repo, &["add", "a"]);
        sh(&repo, &["commit", "-q", "-m", "a"]);
        let sc = tmgr_runner::sessionwt::create(&repo, Some("agent-1"), "claude", &[]).unwrap();
        std::fs::write(sc.path.join("b"), "b").unwrap();
        sh(&sc.path, &["add", "b"]);
        sh(&sc.path, &["commit", "-q", "-m", "b"]);
        let info = collect(&sc.path, |_, _, _| PrLookup::NoPr).unwrap();
        assert_eq!(info.branch, "agent-1");
        assert_eq!(
            info.base.as_deref().map(|b| b.ends_with("main")),
            Some(true)
        );
        assert_eq!(info.ahead_base, Some(1));
    }

    #[test]
    fn a_subdirectory_resolves_to_the_repo_root_and_a_plain_dir_to_none() {
        let dir = repo();
        let sub = canon(dir.path()).join("sub");
        std::fs::create_dir(&sub).unwrap();
        let info = collect(&sub, |_, _, _| PrLookup::NoPr).unwrap();
        assert_eq!(info.root, canon(dir.path()).to_string_lossy());
        let plain = tempfile::tempdir().unwrap();
        let no_repo = canon(plain.path());
        assert!(collect(&no_repo, |_, _, _| PrLookup::NoPr).is_none());
        assert!(collect(Path::new("relative"), |_, _, _| PrLookup::NoPr).is_none());
        assert!(collect(&no_repo.join("missing"), |_, _, _| PrLookup::NoPr).is_none());
    }

    #[test]
    fn a_git_file_naming_another_repository_is_not_inspected() {
        let dir = repo();
        let p = canon(dir.path());
        let other = tempfile::tempdir().unwrap();
        let other = canon(other.path());
        std::fs::create_dir(other.join("sub")).unwrap();
        std::fs::write(
            other.join(".git"),
            format!("gitdir: {}\n", p.join(".git").display()),
        )
        .unwrap();
        assert!(collect(&other.join("sub"), |_, _, _| PrLookup::NoPr).is_none());
    }

    #[test]
    fn a_bogus_git_folder_never_sends_git_to_the_parent_repository() {
        let dir = repo();
        let p = canon(dir.path());
        let inner = p.join("inner");
        std::fs::create_dir_all(inner.join(".git")).unwrap();
        assert!(collect(&inner, |_, _, _| PrLookup::NoPr).is_none());
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
        collect(&root, |_, _, _| PrLookup::NoPr).unwrap();
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
        assert!(collect(&root, |_, _, _| PrLookup::NoPr).is_none());
        assert!(!marker.exists());
    }

    #[test]
    fn a_grandchild_holding_the_pipe_cannot_hang_the_call() {
        let start = Instant::now();
        let args = ["-c".to_string(), "sleep 5 & echo hi".to_string()];
        let out = run("sh", &args, Path::new("/"), None, Duration::from_secs(3));
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
            None,
            Duration::from_millis(200),
        );
        assert!(out.is_none());
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(run(
            "definitely-not-a-program",
            &[],
            Path::new("/"),
            None,
            GIT_TIMEOUT
        )
        .is_none());
    }

    #[test]
    fn a_sandboxed_session_reads_its_branch_without_running_git() {
        let dir = repo();
        let p = canon(dir.path());
        let touched = p.join("touched");
        std::fs::write(p.join(".gitattributes"), "*.txt filter=x\n").unwrap();
        let config = format!(
            "[filter \"x\"]\n\tclean = touch {0}\n[core]\n\tfsmonitor = touch {0}\n",
            touched.display()
        );
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(p.join(".git/config"))
            .unwrap();
        std::io::Write::write_all(&mut file, config.as_bytes()).unwrap();
        let (tx, rx) = mpsc::sync_channel(1);
        let (out_tx, out_rx) = mpsc::channel();
        let handle = std::thread::spawn(move || git_worker(rx, move |m| out_tx.send(m).is_ok()));
        tx.send(vec![GitJob {
            id: "a".into(),
            cwds: vec![p.clone()],
            branch_only: true,
        }])
        .unwrap();
        let found = out_rx.recv().unwrap();
        drop(tx);
        handle.join().unwrap();
        assert!(!touched.exists());
        let info = &found["a"];
        assert_eq!(info.branch, "feature");
        assert_eq!(info.root, p.to_string_lossy());
        assert!(info.branch_only);
        assert_eq!(info.pr, PrLookup::Unavailable);
    }

    #[test]
    fn the_branch_is_read_from_a_worktree_git_file_and_from_a_subfolder() {
        let dir = repo();
        let p = canon(dir.path());
        let base = tempfile::tempdir().unwrap();
        let wt = canon(base.path()).join("wt");
        sh(
            &p,
            &["worktree", "add", "-q", "-b", "side", wt.to_str().unwrap()],
        );
        std::fs::create_dir(wt.join("sub")).unwrap();
        let info = collect_branch_only(&wt.join("sub")).unwrap();
        assert_eq!(info.branch, "side");
        assert_eq!(info.root, wt.to_string_lossy());

        let other = canon(base.path()).join("other");
        std::fs::create_dir(&other).unwrap();
        std::fs::write(
            other.join(".git"),
            format!("gitdir: {}\n", p.join(".git").display()),
        )
        .unwrap();
        assert!(collect_branch_only(&other).is_none());
    }

    #[test]
    fn an_odd_head_never_reaches_the_screen_or_blocks_the_reader() {
        assert_eq!(branch_of_head("ref: refs/heads/a\x1b[2Jb\n"), "(detached)");
        assert_eq!(branch_of_head("ref: refs/tags/v1\n"), "(detached)");
        assert_eq!(branch_of_head("0123abc\n"), "(detached)");
        let dir = repo();
        let p = canon(dir.path());
        let head = p.join(".git/HEAD");
        std::fs::remove_file(&head).unwrap();
        let status = Command::new("mkfifo").arg(&head).status().unwrap();
        assert!(status.success());
        assert_eq!(collect_branch_only(&p), None);
        std::fs::remove_file(&head).unwrap();
        std::os::unix::fs::symlink("/etc/hosts", &head).unwrap();
        assert_eq!(collect_branch_only(&p), None);
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
                branch_only: false,
            },
            GitJob {
                id: "b".into(),
                cwds: vec![canon(plain.path())],
                branch_only: false,
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
            &argv[argv.len() - 5..],
            [
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--ignore-submodules=all",
                "HEAD"
            ]
        );
        assert!(argv.contains(&"pager.diff=less -R".to_string()));
        assert!(!argv.contains(&"--no-pager".to_string()));
        assert!(diff_argv(Some("bad\u{1b}pager"))
            .iter()
            .any(|a| a.starts_with("core.pager=") && !a.contains('\u{1b}')));
    }

    fn hostile(marker: &Path) -> String {
        format!(
            "[filter \"x\"]\n\tclean = touch {0}\n[core]\n\tfsmonitor = touch {0}\n\tpager = touch {0}\n",
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

    fn enable_worktree_config(repo: &Path) {
        sh(repo, &["config", "core.repositoryformatversion", "1"]);
        sh(repo, &["config", "extensions.worktreeConfig", "true"]);
    }

    fn clone_with_upstream() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = canon(tmp.path());
        let origin = root.join("origin.git");
        sh(&root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
        let seed = root.join("seed");
        std::fs::create_dir(&seed).unwrap();
        sh(&seed, &["init", "-q", "-b", "main"]);
        sh(
            &seed,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        std::fs::write(seed.join("a.txt"), "one\n").unwrap();
        sh(&seed, &["add", "."]);
        sh(&seed, &["commit", "-q", "-m", "first"]);
        sh(&seed, &["push", "-q", "origin", "main"]);
        sh(&root, &["clone", "-q", origin.to_str().unwrap(), "clone"]);
        let clone = root.join("clone");
        sh(&clone, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(clone.join("a.txt"), "one\ntwo\n").unwrap();
        sh(&clone, &["commit", "-q", "-am", "f1"]);
        sh(&clone, &["push", "-q", "-u", "origin", "feature"]);
        sh(&seed, &["fetch", "-q", "origin"]);
        sh(
            &seed,
            &["checkout", "-q", "-b", "feature", "origin/feature"],
        );
        std::fs::write(seed.join("s.txt"), "s\n").unwrap();
        sh(&seed, &["add", "."]);
        sh(&seed, &["commit", "-q", "-m", "s1"]);
        sh(&seed, &["push", "-q", "origin", "feature"]);
        sh(&clone, &["fetch", "-q", "origin"]);
        std::fs::write(clone.join("c.txt"), "c\n").unwrap();
        sh(&clone, &["add", "."]);
        sh(&clone, &["commit", "-q", "-m", "c1"]);
        std::fs::write(clone.join("a.txt"), "one\ntwo\nthree\n").unwrap();
        std::fs::write(clone.join("new.txt"), "x\n").unwrap();
        (tmp, clone)
    }

    fn assert_clone_info(info: &GitInfo, root: &Path, branch: &str) {
        assert_eq!(info.root, root.to_string_lossy());
        assert_eq!(info.branch, branch);
        assert_eq!((info.changed, info.untracked), (1, 1));
        assert_eq!((info.added, info.deleted), (1, 0));
        assert_eq!(info.base.as_deref(), Some("origin/main"));
        assert_eq!(info.ahead_base, Some(2));
    }

    #[test]
    fn a_clone_with_an_upstream_reports_ahead_behind_and_the_origin_head_base() {
        let (_tmp, clone) = clone_with_upstream();
        let info = collect(&clone, |_, _, _| PrLookup::NoPr).unwrap();
        assert_clone_info(&info, &clone, "feature");
        assert_eq!(info.upstream.as_deref(), Some("origin/feature"));
        assert_eq!((info.ahead, info.behind), (1, 1));
    }

    #[test]
    fn a_clone_with_worktree_config_and_its_linked_worktree_are_inspected() {
        let (tmp, clone) = clone_with_upstream();
        enable_worktree_config(&clone);
        let info = collect(&clone, |_, _, _| PrLookup::NoPr).unwrap();
        assert_clone_info(&info, &clone, "feature");
        assert_eq!(info.upstream.as_deref(), Some("origin/feature"));
        assert_eq!((info.ahead, info.behind), (1, 1));

        let wt = canon(tmp.path()).join("wt");
        sh(
            &clone,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "side",
                wt.to_str().unwrap(),
                "origin/main",
            ],
        );
        std::fs::write(wt.join("w.txt"), "w\n").unwrap();
        sh(&wt, &["add", "."]);
        sh(&wt, &["commit", "-q", "-m", "w1"]);
        std::fs::write(wt.join("a.txt"), "one\nmore\n").unwrap();
        std::fs::write(wt.join("u1.txt"), "x\n").unwrap();
        std::fs::write(wt.join("u2.txt"), "x\n").unwrap();
        let info = collect(&wt, |_, _, _| PrLookup::NoPr).unwrap();
        assert_eq!(info.root, wt.to_string_lossy());
        assert_eq!(info.branch, "side");
        assert_eq!((info.changed, info.untracked), (1, 2));
        assert_eq!((info.added, info.deleted), (1, 0));
        assert_eq!(info.base.as_deref(), Some("origin/main"));
        assert_eq!(info.ahead_base, Some(1));
    }

    #[test]
    fn a_hostile_worktree_config_is_not_inspected_and_runs_nothing() {
        let (tmp, clone) = clone_with_upstream();
        enable_worktree_config(&clone);
        let marker = canon(tmp.path()).join("ran");
        let wt = canon(tmp.path()).join("wt");
        sh(
            &clone,
            &["worktree", "add", "-q", "--detach", wt.to_str().unwrap()],
        );
        std::fs::write(clone.join(".gitattributes"), "*.txt filter=x\n").unwrap();
        std::fs::write(clone.join(".git/config.worktree"), hostile(&marker)).unwrap();
        assert!(collect(&clone, |_, _, _| PrLookup::NoPr).is_none());
        std::fs::write(
            clone.join(".git/worktrees/wt/config.worktree"),
            hostile(&marker),
        )
        .unwrap();
        assert!(collect(&wt, |_, _, _| PrLookup::NoPr).is_none());
        assert!(!marker.exists());
    }

    #[test]
    fn a_config_rewritten_after_the_private_dir_is_built_never_reaches_attach_git() {
        let dir = repo();
        let root = canon(dir.path());
        let marker = root.join("ran");
        let host = tempfile::tempdir().unwrap();
        tmgr_queue::workdir::host_git_dir(&root, host.path()).unwrap();
        append(&root.join(".git/config"), &hostile(&marker));
        std::fs::write(root.join(".git/commondir"), "/nonexistent\n").unwrap();
        std::fs::write(root.join(".gitattributes"), "*.txt filter=x\n").unwrap();
        std::fs::write(root.join("a.txt"), "one\ntwo\nfour\n").unwrap();
        let status = git(
            &root,
            host.path(),
            &[
                "status",
                "--porcelain=v2",
                "--branch",
                "--ignore-submodules=all",
            ],
        )
        .unwrap();
        assert!(status.contains("a.txt"), "{status}");
        let diff = git(
            &root,
            host.path(),
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--numstat",
                "HEAD",
            ],
        )
        .unwrap();
        assert!(diff.contains("a.txt"), "{diff}");
        let count = git(&root, host.path(), &["rev-list", "--count", "main..HEAD"]).unwrap();
        assert_eq!(count.trim(), "1");
        assert!(!marker.exists());
    }

    #[test]
    fn the_diff_pane_command_names_the_runner_subcommand_and_the_folder() {
        let exe = Path::new("/opt/tmgr/tmgr");
        assert_eq!(
            diff_pane_argv(exe, "/work/r", None),
            ["/opt/tmgr/tmgr", "runner", "git-diff", "--dir", "/work/r"]
        );
        assert_eq!(
            diff_pane_argv(exe, "/work/r", Some("less -R")),
            [
                "/opt/tmgr/tmgr",
                "runner",
                "git-diff",
                "--dir",
                "/work/r",
                "--pager",
                "less -R"
            ]
        );
    }

    #[test]
    fn the_diff_pane_refuses_a_folder_with_settings_that_run_programs() {
        let dir = repo();
        let root = canon(dir.path());
        let marker = root.join("ran");
        append(&root.join(".git/config"), &hostile(&marker));
        std::fs::write(root.join(".gitattributes"), "*.txt filter=x\n").unwrap();
        assert!(diff_in_pane(root.to_str().unwrap(), Some("cat")).is_err());
        assert!(diff_in_pane(root.join("missing").to_str().unwrap(), Some("cat")).is_err());
        assert!(!marker.exists());
    }

    #[test]
    fn the_diff_pane_shows_the_diff_of_a_plain_repo_and_exits_zero() {
        let dir = repo();
        let root = canon(dir.path());
        assert_eq!(
            diff_in_pane(root.to_str().unwrap(), Some("cat")).unwrap(),
            0
        );
    }

    #[test]
    fn the_diff_pane_never_runs_a_submodule_filter() {
        let dir = repo();
        let root = canon(dir.path());
        let sub = root.join("sub");
        std::fs::create_dir(&sub).unwrap();
        sh(&sub, &["init", "-q", "-b", "main"]);
        std::fs::write(sub.join("f"), "one\n").unwrap();
        sh(&sub, &["add", "."]);
        sh(&sub, &["commit", "-q", "-m", "sub"]);
        sh(&root, &["add", "sub"]);
        sh(&root, &["commit", "-q", "-m", "with sub"]);
        let marker = root.join("ran");
        append(&sub.join(".git/config"), &hostile(&marker));
        std::fs::write(sub.join(".gitattributes"), "* filter=x\n").unwrap();
        std::fs::write(sub.join("f"), "two\n").unwrap();
        assert_eq!(
            diff_in_pane(root.to_str().unwrap(), Some("cat")).unwrap(),
            0
        );
        assert!(!marker.exists());
    }
}
