use crate::workdir;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tmgr_core::branch::{slug, validate_branch_name};
use tmgr_core::error::{Error, Result};

const PREFIX: &str = "tmgr/";
const PROTECTED: &[&str] = &["main", "master", "develop"];
const MAX_TITLE: usize = 200;
const MAX_BODY: usize = 20_000;
const STEP_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    BadInput(String),
    BranchShape(String),
    BranchNotPinned,
    ProtectedBranch(String),
    NoCommits,
    WorkflowChange(String),
    RemoteMismatch { which: &'static str, url: String },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::BadInput(m) => write!(f, "publish refused: {m}"),
            Refusal::BranchShape(b) => write!(
                f,
                "publish refused: branch {b:?} is not tmgr/<lowercase letters, digits, . _ ->"
            ),
            Refusal::BranchNotPinned => write!(
                f,
                "publish refused: the clone is not on the branch pinned for this run"
            ),
            Refusal::ProtectedBranch(b) => {
                write!(f, "publish refused: {b:?} is a protected or default branch")
            }
            Refusal::NoCommits => write!(f, "publish refused: the run made no commits"),
            Refusal::WorkflowChange(p) => write!(
                f,
                "publish refused: the run changed {p:?} under .github/, which needs a human"
            ),
            Refusal::RemoteMismatch { which, url } => write!(
                f,
                "publish refused: the {which} URL of origin ({url}) is not the repository linked to the category"
            ),
        }
    }
}

impl std::error::Error for Refusal {}

#[derive(Debug, Clone)]
pub struct PublishInput {
    pub pinned_branch: String,
    pub clone_branch: String,
    pub clone_dir: PathBuf,
    pub repo: PathBuf,
    pub default_branch: String,
    pub origin_fetch_url: String,
    pub origin_push_url: String,
    pub github_full_name: String,
    pub changed_paths: Vec<String>,
    pub commit_count: usize,
    pub pr_title: String,
    pub pr_body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishSteps {
    pub branch: String,
    pub repo: PathBuf,
    pub clone_dir: PathBuf,
    pub push: Vec<String>,
    pub pr: Vec<String>,
}

pub fn branch_slug(task_key: &str, title: &str) -> String {
    let key = slug(task_key);
    let key = if key.is_empty() {
        "task".to_string()
    } else {
        key
    };
    let title = slug(title);
    if title.is_empty() {
        format!("{PREFIX}{key}")
    } else {
        format!("{PREFIX}{key}-{title}")
    }
}

fn branch_shape_ok(branch: &str) -> bool {
    let Some(rest) = branch.strip_prefix(PREFIX) else {
        return false;
    };
    let b = rest.as_bytes();
    (1..=81).contains(&b.len())
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'.' | b'_' | b'-')
        })
        && validate_branch_name(branch).is_ok()
}

fn protected(branch: &str, default_branch: &str) -> bool {
    let lower = branch.to_ascii_lowercase();
    branch == default_branch || PROTECTED.contains(&lower.as_str()) || lower.starts_with("release")
}

fn valid_full_name(full: &str) -> Option<(String, String)> {
    let (owner, repo) = full.split_once('/')?;
    let ok = |s: &str| {
        !s.is_empty()
            && s.len() <= 100
            && s != "."
            && s != ".."
            && !s.starts_with('-')
            && s.bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
    };
    (ok(owner) && ok(repo)).then(|| (owner.to_ascii_lowercase(), repo.to_ascii_lowercase()))
}

fn github_path(url: &str) -> Option<&str> {
    url.strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("ssh://git@github.com/"))
        .or_else(|| url.strip_prefix("git@github.com:"))
}

fn url_matches(url: &str, owner: &str, repo: &str) -> bool {
    let Some(path) = github_path(url) else {
        return false;
    };
    let path = path.strip_suffix('/').unwrap_or(path);
    let path = path.strip_suffix(".git").unwrap_or(path);
    match path.split_once('/') {
        Some((o, r)) => {
            !r.contains('/') && o.eq_ignore_ascii_case(owner) && r.eq_ignore_ascii_case(repo)
        }
        None => false,
    }
}

fn under_dot_github(path: &str) -> bool {
    let p = path.replace('\\', "/");
    let mut p = p.as_str();
    while let Some(rest) = p.strip_prefix("./").or_else(|| p.strip_prefix('/')) {
        p = rest;
    }
    p.split('/')
        .next()
        .is_some_and(|c| c.eq_ignore_ascii_case(".github"))
}

fn one_line(s: &str, max: usize) -> String {
    let flat: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    flat.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect()
}

fn abs_clean(what: &str, p: &Path) -> std::result::Result<String, Refusal> {
    let s = p
        .to_str()
        .filter(|s| !s.contains('\0') && !s.contains('\n'));
    match s {
        Some(s) if p.is_absolute() => Ok(s.to_string()),
        _ => Err(Refusal::BadInput(format!(
            "{what} must be an absolute path"
        ))),
    }
}

pub fn publish_plan(input: &PublishInput) -> std::result::Result<PublishSteps, Refusal> {
    let branch = input.pinned_branch.as_str();
    if !branch_shape_ok(branch) {
        return Err(Refusal::BranchShape(branch.to_string()));
    }
    if input.clone_branch != branch {
        return Err(Refusal::BranchNotPinned);
    }
    if validate_branch_name(&input.default_branch).is_err() {
        return Err(Refusal::BadInput(
            "the default branch name is not usable".into(),
        ));
    }
    if protected(branch, &input.default_branch) {
        return Err(Refusal::ProtectedBranch(branch.to_string()));
    }
    if input.commit_count == 0 {
        return Err(Refusal::NoCommits);
    }
    if let Some(p) = input.changed_paths.iter().find(|p| under_dot_github(p)) {
        return Err(Refusal::WorkflowChange(p.clone()));
    }
    let (owner, repo_name) = valid_full_name(&input.github_full_name)
        .ok_or_else(|| Refusal::BadInput("the repository name is not owner/name".into()))?;
    for (which, url) in [
        ("fetch", &input.origin_fetch_url),
        ("push", &input.origin_push_url),
    ] {
        if !url_matches(url, &owner, &repo_name) {
            return Err(Refusal::RemoteMismatch {
                which,
                url: one_line(url, 120),
            });
        }
    }
    let repo = abs_clean("the bound repository", &input.repo)?;
    abs_clean("the clone", &input.clone_dir)?;
    let title = one_line(&input.pr_title, MAX_TITLE);
    if title.is_empty() {
        return Err(Refusal::BadInput("the pull request title is empty".into()));
    }
    let body: String = input
        .pr_body
        .chars()
        .filter(|c| *c != '\0')
        .take(MAX_BODY)
        .collect();
    let spec = format!("refs/heads/{branch}:refs/heads/{branch}");
    let push = vec![
        "git".to_string(),
        "-c".into(),
        "core.hooksPath=/dev/null".into(),
        "-C".into(),
        repo,
        "push".into(),
        "origin".into(),
        spec,
    ];
    let pr = vec![
        "gh".to_string(),
        "pr".into(),
        "create".into(),
        "--repo".into(),
        input.github_full_name.clone(),
        "--base".into(),
        input.default_branch.clone(),
        "--head".into(),
        branch.to_string(),
        format!("--title={title}"),
        format!("--body={body}"),
    ];
    Ok(PublishSteps {
        branch: branch.to_string(),
        repo: input.repo.clone(),
        clone_dir: input.clone_dir.clone(),
        push,
        pr,
    })
}

pub fn publish_env(vars: impl Iterator<Item = (String, String)>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vars
        .filter(|(k, _)| {
            matches!(
                k.as_str(),
                "PATH"
                    | "HOME"
                    | "USER"
                    | "LOGNAME"
                    | "SSH_AUTH_SOCK"
                    | "XDG_CONFIG_HOME"
                    | "GITHUB_TOKEN"
            ) || k.starts_with("GH_")
        })
        .collect();
    out.push(("GIT_TERMINAL_PROMPT".into(), "0".into()));
    out
}

fn run(argv: &[String], cwd: &Path) -> Result<String> {
    let (program, args) = argv
        .split_first()
        .ok_or_else(|| Error::internal("empty publish command"))?;
    let mut cmd = Command::new(program);
    cmd.env_clear()
        .envs(publish_env(std::env::vars()))
        .current_dir(cwd)
        .args(args);
    let out = workdir::output_within(&mut cmd, STEP_TIMEOUT)?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr);
        let line = msg.lines().next().unwrap_or("failed");
        return Err(Error::internal(format!("{program}: {line}")));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn publish(steps: &PublishSteps) -> Result<String> {
    let repo = steps
        .repo
        .to_str()
        .ok_or_else(|| Error::internal("the repository path is not valid UTF-8"))?;
    workdir::import_branch(repo, &steps.clone_dir, &steps.branch)?;
    run(&steps.push, &steps.repo)?;
    let out = run(&steps.pr, &steps.repo)?;
    Ok(out.lines().last().unwrap_or("").trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PR_FIXED_ARGS: usize = 9;

    fn input() -> PublishInput {
        PublishInput {
            pinned_branch: "tmgr/tm-410-fix-login".into(),
            clone_branch: "tmgr/tm-410-fix-login".into(),
            clone_dir: "/work/repo-worktrees/queue/p1".into(),
            repo: "/work/repo".into(),
            default_branch: "master".into(),
            origin_fetch_url: "https://github.com/tmgr-dev/backend.git".into(),
            origin_push_url: "git@github.com:tmgr-dev/backend.git".into(),
            github_full_name: "tmgr-dev/backend".into(),
            changed_paths: vec!["src/a.rs".into(), "README.md".into()],
            commit_count: 2,
            pr_title: "TM-410: Fix login".into(),
            pr_body: "Done by an agent.\nhttps://tmgr.dev/x/tasks/1".into(),
        }
    }

    fn refused(edit: impl FnOnce(&mut PublishInput)) -> Refusal {
        let mut i = input();
        edit(&mut i);
        publish_plan(&i).unwrap_err()
    }

    #[test]
    fn the_happy_path_builds_exact_argv() {
        let s = publish_plan(&input()).unwrap();
        assert_eq!(
            s.push,
            [
                "git",
                "-c",
                "core.hooksPath=/dev/null",
                "-C",
                "/work/repo",
                "push",
                "origin",
                "refs/heads/tmgr/tm-410-fix-login:refs/heads/tmgr/tm-410-fix-login"
            ]
        );
        assert_eq!(
            s.pr,
            [
                "gh",
                "pr",
                "create",
                "--repo",
                "tmgr-dev/backend",
                "--base",
                "master",
                "--head",
                "tmgr/tm-410-fix-login",
                "--title=TM-410: Fix login",
                "--body=Done by an agent.\nhttps://tmgr.dev/x/tasks/1"
            ]
        );
        assert_eq!(s.branch, "tmgr/tm-410-fix-login");
        assert_eq!(s.repo, PathBuf::from("/work/repo"));
        assert_eq!(s.clone_dir, PathBuf::from("/work/repo-worktrees/queue/p1"));
    }

    #[test]
    fn no_step_can_merge_force_or_widen_the_push() {
        let mut variants = vec![input()];
        for title in ["--force", "merge", "-f", "x --mirror --all --tags"] {
            let mut i = input();
            i.pr_title = title.into();
            i.pr_body = format!("{title}\n--force");
            variants.push(i);
        }
        for i in variants {
            let s = publish_plan(&i).unwrap();
            for a in s.push.iter().chain(s.pr.iter().take(PR_FIXED_ARGS)) {
                let a = a.to_ascii_lowercase();
                assert!(
                    !a.contains("merge")
                        && !a.contains("--force")
                        && a != "-f"
                        && !a.contains("--mirror")
                        && !a.contains("--all")
                        && !a.contains("--tags")
                        && !a.contains("--delete")
                        && !a.starts_with('+'),
                    "{a}"
                );
            }
            assert_eq!(s.push.iter().filter(|a| a.contains(':')).count(), 1);
            let spec = s.push.last().unwrap();
            assert_eq!(
                spec,
                &format!("refs/heads/{0}:refs/heads/{0}", i.pinned_branch)
            );
            assert!(!spec.contains("master"));
            assert_eq!(&s.pr[1..3], ["pr", "create"]);
            assert!(s
                .pr
                .iter()
                .skip(PR_FIXED_ARGS)
                .all(|a| a.starts_with("--title=") || a.starts_with("--body=")));
        }
    }

    #[test]
    fn titles_with_a_leading_dash_stay_values_of_their_option() {
        let mut i = input();
        i.pr_title = "--web".into();
        let s = publish_plan(&i).unwrap();
        assert_eq!(s.pr[PR_FIXED_ARGS], "--title=--web");
    }

    #[test]
    fn branch_shape_is_enforced() {
        for b in [
            "main",
            "master",
            "develop",
            "release/1.0",
            "feature/x",
            "tmgr/",
            "tmgr/-x",
            "tmgr/.x",
            "tmgr/X",
            "tmgr/a b",
            "tmgr/a..b",
            "tmgr/a/b",
            "tmgr/a.lock",
            "tmgr/a;rm",
            "tmgr/ä",
            "TMGR/x",
            "refs/heads/tmgr/x",
            "+tmgr/x",
            "-tmgr/x",
        ] {
            let r = refused(|i| {
                i.pinned_branch = b.into();
                i.clone_branch = b.into();
            });
            assert!(matches!(r, Refusal::BranchShape(_)), "{b}: {r:?}");
        }
        let long = format!("tmgr/a{}", "b".repeat(81));
        assert!(matches!(
            refused(|i| {
                i.pinned_branch = long.clone();
                i.clone_branch = long.clone();
            }),
            Refusal::BranchShape(_)
        ));
        let max = format!("tmgr/a{}", "b".repeat(80));
        let mut i = input();
        i.pinned_branch = max.clone();
        i.clone_branch = max;
        assert!(publish_plan(&i).is_ok());
    }

    #[test]
    fn the_clone_must_be_on_the_pinned_branch() {
        assert_eq!(
            refused(|i| i.clone_branch = "tmgr/tm-999-other".into()),
            Refusal::BranchNotPinned
        );
        assert_eq!(
            refused(|i| i.clone_branch = "master".into()),
            Refusal::BranchNotPinned
        );
    }

    #[test]
    fn default_and_protected_branches_are_refused() {
        let r = refused(|i| i.default_branch = "tmgr/tm-410-fix-login".into());
        assert!(matches!(r, Refusal::ProtectedBranch(_)), "{r:?}");
        for bad in ["", "-x", "a..b", "a b", "x;y"] {
            let r = refused(|i| i.default_branch = bad.into());
            assert!(matches!(r, Refusal::BadInput(_)), "{bad}: {r:?}");
        }
        assert!(protected("main", "x") && protected("Master", "x") && protected("develop", "x"));
        assert!(
            protected("release/1", "x")
                && protected("release-2", "x")
                && protected("releases", "x")
        );
        assert!(!protected("tmgr/a", "main"));
    }

    #[test]
    fn zero_commits_are_refused() {
        assert_eq!(refused(|i| i.commit_count = 0), Refusal::NoCommits);
    }

    #[test]
    fn workflow_changes_are_refused() {
        for p in [
            ".github/workflows/ci.yml",
            ".github/CODEOWNERS",
            "./.github/x",
            "/.github/x",
            ".GitHub/workflows/a.yml",
            ".github",
            ".github\\workflows\\a.yml",
        ] {
            let r = refused(|i| i.changed_paths.push(p.into()));
            assert!(matches!(r, Refusal::WorkflowChange(_)), "{p}: {r:?}");
        }
        for ok in ["src/.github/x", "docs/github.md", ".githubx/a", "github/a"] {
            let mut i = input();
            i.changed_paths.push(ok.into());
            assert!(publish_plan(&i).is_ok(), "{ok}");
        }
    }

    #[test]
    fn origin_urls_must_match_the_linked_repository() {
        for url in [
            "https://github.com/tmgr-dev/backend",
            "https://github.com/tmgr-dev/backend.git",
            "https://github.com/tmgr-dev/backend/",
            "https://github.com/TMGR-DEV/Backend.git",
            "git@github.com:tmgr-dev/backend.git",
            "git@github.com:tmgr-dev/backend",
            "ssh://git@github.com/tmgr-dev/backend.git",
        ] {
            let mut i = input();
            i.origin_fetch_url = url.into();
            i.origin_push_url = url.into();
            assert!(publish_plan(&i).is_ok(), "{url}");
        }
        for url in [
            "",
            "https://github.com/tmgr-dev/frontend.git",
            "https://github.com/evil/backend.git",
            "https://github.com/tmgr-dev/backend-fork.git",
            "https://github.com/tmgr-dev/backend/extra",
            "https://github.com.evil.io/tmgr-dev/backend.git",
            "https://evil.io/github.com/tmgr-dev/backend.git",
            "https://user:pw@github.com/tmgr-dev/backend.git",
            "http://github.com/tmgr-dev/backend.git",
            "git@evil.io:tmgr-dev/backend.git",
            "ssh://git@github.com:22/tmgr-dev/backend.git",
            "/work/other.git",
            "file:///work/repo",
            "https://gitlab.com/tmgr-dev/backend.git",
            "https://github.com/tmgr-dev",
            "https://github.com/",
        ] {
            let r = refused(|i| i.origin_fetch_url = url.into());
            assert!(
                matches!(r, Refusal::RemoteMismatch { which: "fetch", .. }),
                "{url}: {r:?}"
            );
            let r = refused(|i| i.origin_push_url = url.into());
            assert!(
                matches!(r, Refusal::RemoteMismatch { which: "push", .. }),
                "{url}: {r:?}"
            );
        }
    }

    #[test]
    fn a_push_url_that_differs_from_the_fetch_url_is_refused() {
        let r = refused(|i| i.origin_push_url = "git@github.com:someone/else.git".into());
        assert!(matches!(r, Refusal::RemoteMismatch { which: "push", .. }));
    }

    #[test]
    fn bad_repository_names_and_paths_are_refused() {
        for full in [
            "", "a", "a/b/c", "/b", "a/", "../x", "a/..", "-a/b", "a/-b", "a b/c", "a/b;c",
        ] {
            let r = refused(|i| i.github_full_name = full.into());
            assert!(matches!(r, Refusal::BadInput(_)), "{full}: {r:?}");
        }
        for repo in ["relative/repo", "", "/a\nb"] {
            let r = refused(|i| i.repo = repo.into());
            assert!(matches!(r, Refusal::BadInput(_)), "{repo:?}: {r:?}");
        }
        let r = refused(|i| i.clone_dir = "clone".into());
        assert!(matches!(r, Refusal::BadInput(_)));
        let r = refused(|i| i.pr_title = " \n\t ".into());
        assert!(matches!(r, Refusal::BadInput(_)));
    }

    #[test]
    fn only_the_bound_repository_appears_in_the_push_command() {
        let s = publish_plan(&input()).unwrap();
        let paths: Vec<&String> = s.push.iter().filter(|a| a.starts_with('/')).collect();
        assert_eq!(paths, ["/work/repo"]);
        assert!(!s.push.iter().chain(&s.pr).any(|a| a.contains("worktrees")));
    }

    #[test]
    fn titles_are_one_line_and_bounded() {
        let mut i = input();
        i.pr_title = format!("TM-1: a\nb\r\n{}", "x".repeat(500));
        let s = publish_plan(&i).unwrap();
        let t = s.pr[PR_FIXED_ARGS].strip_prefix("--title=").unwrap();
        assert!(!t.contains(['\n', '\r']) && t.chars().count() <= MAX_TITLE);
        assert!(t.starts_with("TM-1: a b "));
    }

    #[test]
    fn env_keeps_only_what_the_owners_auth_needs() {
        let vars = [
            ("PATH", "/usr/bin"),
            ("HOME", "/h"),
            ("SSH_AUTH_SOCK", "/s"),
            ("GH_TOKEN", "t"),
            ("GH_HOST", "h"),
            ("GITHUB_TOKEN", "t"),
            ("AWS_SECRET_ACCESS_KEY", "x"),
            ("GIT_DIR", "/evil"),
            ("GIT_SSH_COMMAND", "evil"),
            ("GIT_CONFIG_GLOBAL", "/evil"),
            ("LD_PRELOAD", "/evil"),
            ("TMGR_TOKEN", "x"),
            ("HTTPS_PROXY", "x"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()));
        let out = publish_env(vars.into_iter());
        let keys: Vec<&str> = out.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            [
                "PATH",
                "HOME",
                "SSH_AUTH_SOCK",
                "GH_TOKEN",
                "GH_HOST",
                "GITHUB_TOKEN",
                "GIT_TERMINAL_PROMPT"
            ]
        );
    }

    #[test]
    fn branch_slug_follows_the_plan() {
        assert_eq!(
            branch_slug("TM-410", "Fix the login page"),
            "tmgr/tm-410-fix-the-login-page"
        );
        assert_eq!(branch_slug("TM-410", "  --Fix!!  "), "tmgr/tm-410-fix");
        assert_eq!(branch_slug("TM-410", "Привет"), "tmgr/tm-410");
        assert_eq!(branch_slug("TM-410", ""), "tmgr/tm-410");
        assert_eq!(branch_slug("", "x"), "tmgr/task-x");
        assert_eq!(branch_slug("tmgrdev:9979", "Café"), "tmgr/tmgrdev-9979-caf");
        let long = branch_slug("TM-410", &"word ".repeat(40));
        assert!(long.len() <= "tmgr/tm-410-".len() + 40);
        assert!(!long.ends_with('-'));
        assert!(branch_shape_ok(&long));
        for title in ["../../etc", "$(rm -rf)", "a/b", "A.B_C", "-x", "x-"] {
            assert!(branch_shape_ok(&branch_slug("TM-1", title)), "{title}");
        }
    }
}
