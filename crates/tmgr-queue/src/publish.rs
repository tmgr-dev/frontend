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
const PR_REQUERY_DELAY: Duration = Duration::from_secs(2);
const STEP_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    BadInput(String),
    BranchShape(String),
    BranchNotPinned,
    ProtectedBranch(String),
    NoCommits,
    WorkflowChange(String),
    ForeignHistory(String),
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
            Refusal::ForeignHistory(base) => write!(
                f,
                "publish refused: the agent's branch must be a straight line of new commits on {base}; merge commits and unrelated history are refused"
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
    pub sha: String,
    pub clone_branch: String,
    pub clone_dir: PathBuf,
    pub repo: PathBuf,
    pub default_branch: String,
    pub origin_fetch_url: String,
    pub origin_push_url: String,
    pub github_full_name: String,
    pub changed_paths: Vec<String>,
    pub commit_count: usize,
    pub descends_from_base: bool,
    pub merge_commits: usize,
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

impl PublishSteps {
    pub fn draft(mut self) -> Self {
        self.pr.push("--draft".into());
        self
    }
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

fn clean_body(body: &str) -> String {
    body.chars().filter(|c| *c != '\0').take(MAX_BODY).collect()
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
    if !input.descends_from_base || input.merge_commits > 0 {
        return Err(Refusal::ForeignHistory(input.default_branch.clone()));
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
    let body = clean_body(&input.pr_body);
    if !matches!(input.sha.len(), 40 | 64) || !input.sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Refusal::BadInput(
            "the commit to push is not a full object id".into(),
        ));
    }
    let spec = format!("{}:refs/heads/{branch}", input.sha);
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
    run(&steps.push, &steps.repo)?;
    let out = run(&steps.pr, &steps.repo)?;
    Ok(out.lines().last().unwrap_or("").trim().to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllRefusal {
    pub repo: Option<String>,
    pub refusal: Refusal,
}

impl std::fmt::Display for AllRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.repo {
            Some(r) => write!(f, "{r}: {}", self.refusal),
            None => write!(f, "{}", self.refusal),
        }
    }
}

impl std::error::Error for AllRefusal {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoPublish {
    pub full_name: String,
    pub sha: String,
    pub body: String,
    pub steps: PublishSteps,
    pub list: Vec<String>,
    pub wip: Option<String>,
    pub verify: Option<Vec<String>>,
    pub ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishAllPlan {
    pub repos: Vec<RepoPublish>,
    pub skipped: Vec<String>,
}

impl PublishAllPlan {
    pub fn draft(mut self) -> Self {
        for r in &mut self.repos {
            r.steps = r.steps.clone().draft();
            r.ready = false;
        }
        self
    }

    /// Continues a pull request opened as a draft by a paused run: `wip` lists `(repository, commit pushed
    /// when the run paused)`. The existing pull request is accepted while it still shows that commit, and is
    /// marked ready once the new commits are pushed.
    pub fn resuming(mut self, wip: &[(String, String)]) -> Self {
        for r in &mut self.repos {
            if let Some((_, sha)) = wip.iter().find(|(f, _)| *f == r.full_name) {
                r.wip = Some(sha.clone());
                r.ready = true;
                let lease = format!("--force-with-lease=refs/heads/{}:{sha}", r.steps.branch);
                if let Some(at) = r.steps.push.iter().position(|a| a == "push") {
                    r.steps.push.insert(at + 1, lease);
                }
                r.verify = Some(vec![
                    "git".to_string(),
                    "-C".into(),
                    r.steps.repo.to_string_lossy().into_owned(),
                    "merge-base".into(),
                    "--is-ancestor".into(),
                    sha.clone(),
                    r.sha.clone(),
                ]);
            }
        }
        self
    }

    pub fn links(&self) -> bool {
        self.repos.len() >= 2
    }
}

/// Plans the publish of every repository of a multi-repository run. A repository without new commits is
/// skipped; any other refusal refuses the whole publish, so nothing is pushed.
pub fn publish_all_plan(
    inputs: &[PublishInput],
) -> std::result::Result<PublishAllPlan, AllRefusal> {
    let mut repos = Vec::new();
    let mut skipped = Vec::new();
    for input in inputs {
        if input.commit_count == 0 {
            skipped.push(input.github_full_name.clone());
            continue;
        }
        let steps = publish_plan(input).map_err(|refusal| AllRefusal {
            repo: Some(input.github_full_name.clone()),
            refusal,
        })?;
        repos.push(RepoPublish {
            full_name: input.github_full_name.clone(),
            sha: input.sha.clone(),
            body: clean_body(&input.pr_body),
            list: vec![
                "gh".to_string(),
                "pr".into(),
                "list".into(),
                "--repo".into(),
                input.github_full_name.clone(),
                "--head".into(),
                steps.branch.clone(),
                "--state".into(),
                "open".into(),
                "--json".into(),
                "url,headRefOid,isCrossRepository,headRepositoryOwner".into(),
            ],
            wip: None,
            verify: None,
            ready: false,
            steps,
        });
    }
    if repos.is_empty() {
        return Err(AllRefusal {
            repo: None,
            refusal: Refusal::NoCommits,
        });
    }
    Ok(PublishAllPlan { repos, skipped })
}

fn pr_url_of(full_name: &str, url: &str) -> bool {
    let prefix = format!("https://github.com/{full_name}/pull/");
    url.len() > prefix.len()
        && url[..prefix.len()].eq_ignore_ascii_case(&prefix)
        && url[prefix.len()..].bytes().all(|b| b.is_ascii_digit())
}

/// The open pull request to reuse for a pushed head, if there is one. Any open pull request for that head
/// name that comes from a fork, another owner or another commit refuses the publish.
pub fn reuse_pr(
    list_json: &str,
    full_name: &str,
    sha: &str,
) -> std::result::Result<Option<String>, String> {
    reuse_pr_continuing(list_json, full_name, sha, None)
}

/// Like `reuse_pr`, and a pull request still showing `wip` (the commit a paused run pushed before `sha`
/// was pushed on top of it) counts as the one just pushed.
pub fn reuse_pr_continuing(
    list_json: &str,
    full_name: &str,
    sha: &str,
    wip: Option<&str>,
) -> std::result::Result<Option<String>, String> {
    let text = list_json.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let found: serde_json::Value = serde_json::from_str(text)
        .map_err(|_| "gh answered with something unreadable".to_string())?;
    let list = found
        .as_array()
        .ok_or_else(|| "gh answered with something unreadable".to_string())?;
    let owner = full_name.split('/').next().unwrap_or_default();
    let mut reuse = None;
    for pr in list {
        let url = pr["url"].as_str().unwrap_or_default();
        let same_owner = pr["headRepositoryOwner"]["login"]
            .as_str()
            .is_some_and(|l| l.eq_ignore_ascii_case(owner));
        let good = pr["isCrossRepository"] == serde_json::Value::Bool(false)
            && same_owner
            && pr["headRefOid"].as_str().is_some_and(|o| {
                o.eq_ignore_ascii_case(sha) || wip.is_some_and(|w| o.eq_ignore_ascii_case(w))
            })
            && pr_url_of(full_name, url);
        if !good {
            return Err(format!(
                "an open pull request for this branch in {full_name} is not the one just pushed; close or rename it first"
            ));
        }
        reuse.get_or_insert_with(|| url.to_string());
    }
    Ok(reuse)
}

pub fn link_body(body: &str, others: &[&str]) -> String {
    let list: Vec<String> = others.iter().map(|u| format!("- {u}")).collect();
    let section = format!("\n\nRelated pull requests:\n{}", list.join("\n"));
    let room = MAX_BODY.saturating_sub(section.chars().count());
    let all = format!("{}{section}", body.chars().take(room).collect::<String>());
    all.chars().take(MAX_BODY).collect()
}

/// Pushes and opens (or reuses) the pull request of every planned repository, then links them to each
/// other when there are two or more. Returns `(repository, url)` in plan order.
pub fn publish_all_with(
    plan: &PublishAllPlan,
    exec: &mut dyn FnMut(&[String], &Path) -> Result<String>,
) -> Result<Vec<(String, String)>> {
    publish_all_after(plan, exec, Duration::ZERO)
}

fn publish_all_after(
    plan: &PublishAllPlan,
    exec: &mut dyn FnMut(&[String], &Path) -> Result<String>,
    requery_after: Duration,
) -> Result<Vec<(String, String)>> {
    let mut urls: Vec<(String, String)> = Vec::new();
    for r in &plan.repos {
        if let Some(verify) = &r.verify {
            exec(verify, &r.steps.repo).map_err(|_| {
                Error::new(
                    tmgr_core::error::Kind::Forbidden,
                    format!(
                        "the new head of {} does not descend from the commit saved when the run paused; nothing was pushed",
                        r.full_name
                    ),
                )
            })?;
        }
        exec(&r.steps.push, &r.steps.repo)?;
        let listed = exec(&r.list, &r.steps.repo)?;
        let wip = r.wip.as_deref();
        let reuse = match reuse_pr_continuing(&listed, &r.full_name, &r.sha, wip) {
            Ok(reuse) => Ok(reuse),
            Err(_) => {
                std::thread::sleep(requery_after);
                let listed = exec(&r.list, &r.steps.repo)?;
                reuse_pr_continuing(&listed, &r.full_name, &r.sha, wip)
            }
        };
        let url = match reuse.map_err(|m| Error::new(tmgr_core::error::Kind::Forbidden, m))? {
            Some(url) => {
                if r.ready {
                    exec(
                        &["gh".to_string(), "pr".into(), "ready".into(), url.clone()],
                        &r.steps.repo,
                    )?;
                }
                url
            }
            None => {
                let out = exec(&r.steps.pr, &r.steps.repo)?;
                let url = out.lines().last().unwrap_or("").trim().to_string();
                if !pr_url_of(&r.full_name, &url) {
                    return Err(Error::internal(format!(
                        "gh did not return a pull request link for {}",
                        r.full_name
                    )));
                }
                url
            }
        };
        urls.push((r.full_name.clone(), url));
    }
    if plan.links() {
        for (r, (_, url)) in plan.repos.iter().zip(&urls) {
            let others: Vec<&str> = urls
                .iter()
                .filter(|(_, u)| u != url)
                .map(|(_, u)| u.as_str())
                .collect();
            let edit = vec![
                "gh".to_string(),
                "pr".into(),
                "edit".into(),
                url.clone(),
                format!("--body={}", link_body(&r.body, &others)),
            ];
            exec(&edit, &r.steps.repo)?;
        }
    }
    Ok(urls)
}

pub fn publish_all(plan: &PublishAllPlan) -> Result<Vec<(String, String)>> {
    publish_all_after(plan, &mut |argv, cwd| run(argv, cwd), PR_REQUERY_DELAY)
}

/// Runs a command the way the publish steps do: cleaned environment, bounded time, in `cwd`.
pub fn run_hardened(argv: &[String], cwd: &Path) -> Result<String> {
    run(argv, cwd)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PR_FIXED_ARGS: usize = 9;

    fn input() -> PublishInput {
        PublishInput {
            sha: "0123456789abcdef0123456789abcdef01234567".into(),
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
            descends_from_base: true,
            merge_commits: 0,
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
                "0123456789abcdef0123456789abcdef01234567:refs/heads/tmgr/tm-410-fix-login"
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
    fn a_draft_plan_adds_the_flag_to_every_pr_create_and_nothing_else() {
        let plain = publish_plan(&input()).unwrap();
        let draft = publish_plan(&input()).unwrap().draft();
        assert_eq!(draft.push, plain.push);
        assert_eq!(draft.pr[..draft.pr.len() - 1], plain.pr[..]);
        assert_eq!(draft.pr.last().map(String::as_str), Some("--draft"));
        let all = publish_all_plan(&[repo_input("o/a", 1), repo_input("o/b", 2)])
            .unwrap()
            .draft();
        assert!(all
            .repos
            .iter()
            .all(|r| r.steps.pr.last().map(String::as_str) == Some("--draft")));
        assert!(all
            .repos
            .iter()
            .all(|r| !r.list.contains(&"--draft".to_string())));
    }

    #[test]
    fn the_push_names_the_planned_commit_and_a_bad_id_is_refused() {
        let s = publish_plan(&input()).unwrap();
        assert!(s.push.last().unwrap().starts_with(&input().sha));
        for bad in ["", "main", "0123", "refs/heads/x"] {
            assert!(matches!(
                refused(|i| i.sha = bad.into()),
                Refusal::BadInput(_)
            ));
        }
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
            assert_eq!(spec, &format!("{}:refs/heads/{}", i.sha, i.pinned_branch));
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

    fn repo_input(full: &str, commits: usize) -> PublishInput {
        let name = full.split('/').nth(1).unwrap();
        let mut i = input();
        i.github_full_name = full.into();
        i.repo = format!("/work/{name}").into();
        i.clone_dir = format!("/work/{name}-worktrees/queue/p1/{name}").into();
        i.origin_fetch_url = format!("https://github.com/{full}.git");
        i.origin_push_url = format!("git@github.com:{full}.git");
        i.commit_count = commits;
        i.sha = if name == "backend" {
            "a".repeat(40)
        } else {
            "b".repeat(40)
        };
        i
    }

    fn both() -> Vec<PublishInput> {
        vec![
            repo_input("tmgr-dev/backend", 2),
            repo_input("tmgr-dev/frontend", 1),
        ]
    }

    #[test]
    fn one_refused_repository_refuses_everything_and_names_it() {
        let mut inputs = both();
        inputs[1].changed_paths = vec![".github/workflows/ci.yml".into()];
        let e = publish_all_plan(&inputs).unwrap_err();
        assert_eq!(e.repo.as_deref(), Some("tmgr-dev/frontend"));
        assert!(matches!(e.refusal, Refusal::WorkflowChange(_)));
        assert!(e
            .to_string()
            .starts_with("tmgr-dev/frontend: publish refused"));
        let mut inputs = both();
        inputs[0].clone_branch = "main".into();
        assert!(matches!(
            publish_all_plan(&inputs).unwrap_err().refusal,
            Refusal::BranchNotPinned
        ));
        let mut inputs = both();
        inputs[1].origin_push_url = "https://github.com/evil/frontend.git".into();
        assert!(matches!(
            publish_all_plan(&inputs).unwrap_err().refusal,
            Refusal::RemoteMismatch { .. }
        ));
    }

    #[test]
    fn a_repository_without_commits_is_skipped_and_all_empty_is_todays_error() {
        let mut inputs = both();
        inputs[0].commit_count = 0;
        let p = publish_all_plan(&inputs).unwrap();
        assert_eq!(p.skipped, ["tmgr-dev/backend"]);
        assert_eq!(p.repos.len(), 1);
        assert!(!p.links());
        let mut none = both();
        none[0].commit_count = 0;
        none[1].commit_count = 0;
        let e = publish_all_plan(&none).unwrap_err();
        assert_eq!(e.refusal, Refusal::NoCommits);
        assert_eq!(e.to_string(), Refusal::NoCommits.to_string());
        assert_eq!(
            publish_all_plan(&[]).unwrap_err().refusal,
            Refusal::NoCommits
        );
    }

    #[test]
    fn refspecs_are_exact_per_repository_and_never_merge() {
        let p = publish_all_plan(&both()).unwrap();
        assert!(p.links());
        for (r, sha) in p.repos.iter().zip(["a", "b"]) {
            assert_eq!(
                r.steps.push.last().unwrap(),
                &format!("{}:refs/heads/tmgr/tm-410-fix-login", sha.repeat(40))
            );
            assert_eq!(
                r.steps.push[4],
                format!("/work/{}", r.full_name.split('/').nth(1).unwrap())
            );
            assert_eq!(
                r.list,
                [
                    "gh",
                    "pr",
                    "list",
                    "--repo",
                    &r.full_name,
                    "--head",
                    "tmgr/tm-410-fix-login",
                    "--state",
                    "open",
                    "--json",
                    "url,headRefOid,isCrossRepository,headRepositoryOwner"
                ]
            );
            assert_eq!(&r.steps.pr[1..3], ["pr", "create"]);
        }
    }

    fn fake<'a>(
        lists: &[(&str, &str)],
        log: &'a std::cell::RefCell<Vec<Vec<String>>>,
    ) -> impl FnMut(&[String], &Path) -> Result<String> + 'a {
        let lists: Vec<(String, String)> = lists
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        move |argv, _cwd| {
            log.borrow_mut().push(argv.to_vec());
            match (argv[0].as_str(), argv[1].as_str()) {
                ("gh", "pr") if argv[2] == "list" => Ok(lists
                    .iter()
                    .find(|(r, _)| *r == argv[4])
                    .map(|(_, j)| j.clone())
                    .unwrap_or_else(|| "[]".into())),
                ("gh", "pr") if argv[2] == "create" => {
                    let n = if argv[4] == "tmgr-dev/backend" {
                        11
                    } else {
                        12
                    };
                    Ok(format!(
                        "warning line\nhttps://github.com/{}/pull/{n}",
                        argv[4]
                    ))
                }
                _ => Ok(String::new()),
            }
        }
    }

    #[test]
    fn two_pull_requests_are_pushed_opened_and_linked_without_a_merge() {
        let plan = publish_all_plan(&both()).unwrap();
        let log = std::cell::RefCell::new(Vec::new());
        let urls = publish_all_with(&plan, &mut fake(&[], &log)).unwrap();
        assert_eq!(
            urls,
            [
                (
                    "tmgr-dev/backend".to_string(),
                    "https://github.com/tmgr-dev/backend/pull/11".to_string()
                ),
                (
                    "tmgr-dev/frontend".to_string(),
                    "https://github.com/tmgr-dev/frontend/pull/12".to_string()
                )
            ]
        );
        let log = log.borrow();
        let kinds: Vec<String> = log
            .iter()
            .map(|a| {
                if a[0] == "git" {
                    "push".to_string()
                } else {
                    a[2].clone()
                }
            })
            .collect();
        assert_eq!(
            kinds,
            ["push", "list", "create", "push", "list", "create", "edit", "edit"]
        );
        assert!(log.iter().flatten().all(|a| !a.contains("merge")));
        let edit = &log[6];
        assert_eq!(edit[3], "https://github.com/tmgr-dev/backend/pull/11");
        assert!(edit[4].starts_with("--body=Done by an agent."));
        assert!(edit[4]
            .ends_with("Related pull requests:\n- https://github.com/tmgr-dev/frontend/pull/12"));
        assert!(log[7][4].ends_with("- https://github.com/tmgr-dev/backend/pull/11"));
    }

    #[test]
    fn one_pull_request_gets_no_link_step() {
        let mut inputs = both();
        inputs[1].commit_count = 0;
        let plan = publish_all_plan(&inputs).unwrap();
        let log = std::cell::RefCell::new(Vec::new());
        let urls = publish_all_with(&plan, &mut fake(&[], &log)).unwrap();
        assert_eq!(urls.len(), 1);
        assert!(log
            .borrow()
            .iter()
            .all(|a| a[1] != "edit" && a.get(2).map(String::as_str) != Some("edit")));
    }

    fn open_pr(url: &str, cross: bool, owner: &str, oid: &str) -> String {
        serde_json::json!([{"url": url, "headRefOid": oid, "isCrossRepository": cross,
            "headRepositoryOwner": {"login": owner}}])
        .to_string()
    }

    #[test]
    fn an_open_pull_request_of_the_same_commit_is_reused() {
        let plan = publish_all_plan(&both()).unwrap();
        let be = open_pr(
            "https://github.com/tmgr-dev/backend/pull/5",
            false,
            "TMGR-dev",
            &"A".repeat(40),
        );
        let log = std::cell::RefCell::new(Vec::new());
        let urls = publish_all_with(&plan, &mut fake(&[("tmgr-dev/backend", &be)], &log)).unwrap();
        assert_eq!(urls[0].1, "https://github.com/tmgr-dev/backend/pull/5");
        let creates = log
            .borrow()
            .iter()
            .filter(|a| a.get(2).map(String::as_str) == Some("create"))
            .count();
        assert_eq!(creates, 1);
    }

    #[test]
    fn a_draft_publish_opens_drafts_and_leaves_an_existing_pull_request_alone() {
        let plan = publish_all_plan(&both()).unwrap().draft();
        let be = open_pr(
            "https://github.com/tmgr-dev/backend/pull/5",
            false,
            "TMGR-dev",
            &"A".repeat(40),
        );
        let log = std::cell::RefCell::new(Vec::new());
        let urls = publish_all_with(&plan, &mut fake(&[("tmgr-dev/backend", &be)], &log)).unwrap();
        assert_eq!(urls[0].1, "https://github.com/tmgr-dev/backend/pull/5");
        let log = log.borrow();
        let creates: Vec<&Vec<String>> = log
            .iter()
            .filter(|a| a.get(2).map(String::as_str) == Some("create"))
            .collect();
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0].last().map(String::as_str), Some("--draft"));
        assert!(log
            .iter()
            .filter(|a| a.get(2).map(String::as_str) == Some("edit"))
            .all(|a| !a.contains(&"--draft".to_string())));
    }

    fn wip_pairs() -> Vec<(String, String)> {
        vec![("tmgr-dev/backend".to_string(), "c".repeat(40))]
    }

    fn calls(log: &std::cell::RefCell<Vec<Vec<String>>>, what: &str) -> usize {
        log.borrow()
            .iter()
            .filter(|a| a.get(2).map(String::as_str) == Some(what))
            .count()
    }

    #[test]
    fn a_resumed_run_reuses_the_pull_request_of_its_saved_commit_and_marks_it_ready() {
        let url = "https://github.com/tmgr-dev/backend/pull/5";
        for oid in ["c".repeat(40), "a".repeat(40)] {
            let plan = publish_all_plan(&both()).unwrap().resuming(&wip_pairs());
            let be = open_pr(url, false, "tmgr-dev", &oid);
            let log = std::cell::RefCell::new(Vec::new());
            let urls =
                publish_all_with(&plan, &mut fake(&[("tmgr-dev/backend", &be)], &log)).unwrap();
            assert_eq!(urls[0].1, url);
            assert_eq!(calls(&log, "ready"), 1, "{oid}");
            assert!(log
                .borrow()
                .iter()
                .any(|a| a[..3] == ["gh", "pr", "ready"] && a[3] == url));
            assert_eq!(calls(&log, "create"), 1);
        }
    }

    #[test]
    fn a_resumed_push_leases_the_saved_commit_and_checks_descent_first() {
        let plan = publish_all_plan(&both()).unwrap().resuming(&wip_pairs());
        let log = std::cell::RefCell::new(Vec::new());
        publish_all_with(&plan, &mut fake(&[], &log)).unwrap();
        let log = log.borrow();
        let wip = "c".repeat(40);
        let verify = log
            .iter()
            .position(|a| a.contains(&"merge-base".to_string()))
            .unwrap();
        assert_eq!(log[verify][4], "--is-ancestor");
        assert_eq!(log[verify][5], wip);
        assert_eq!(log[verify][6], "a".repeat(40));
        let lease = format!("--force-with-lease=refs/heads/tmgr/tm-410-fix-login:{wip}");
        let pushes: Vec<&Vec<String>> = log
            .iter()
            .filter(|a| a.contains(&"push".to_string()))
            .collect();
        assert_eq!(pushes.len(), 2);
        assert!(pushes[0].contains(&lease) && !pushes[1].iter().any(|a| a.contains("lease")));
        assert!(verify < log.iter().position(|a| a.contains(&lease)).unwrap());
    }

    #[test]
    fn a_resumed_head_that_does_not_descend_from_the_saved_commit_pushes_nothing() {
        let plan = publish_all_plan(&both()).unwrap().resuming(&wip_pairs());
        let log = std::cell::RefCell::new(Vec::new());
        let mut exec = |argv: &[String], _: &Path| -> Result<String> {
            log.borrow_mut().push(argv.to_vec());
            if argv.contains(&"merge-base".to_string()) {
                return Err(Error::internal("git: not an ancestor"));
            }
            Ok(String::new())
        };
        let e = publish_all_with(&plan, &mut exec).unwrap_err();
        assert!(e.message.contains("does not descend"), "{}", e.message);
        assert!(log
            .borrow()
            .iter()
            .all(|a| !a.contains(&"push".to_string())));
    }

    #[test]
    fn a_plain_publish_has_no_lease_and_no_descent_check() {
        let plan = publish_all_plan(&both()).unwrap();
        let log = std::cell::RefCell::new(Vec::new());
        publish_all_with(&plan, &mut fake(&[], &log)).unwrap();
        assert!(log
            .borrow()
            .iter()
            .flatten()
            .all(|a| !a.contains("lease") && a != "merge-base"));
    }

    #[test]
    fn a_resumed_run_still_refuses_a_pull_request_of_any_other_commit() {
        let plan = publish_all_plan(&both()).unwrap().resuming(&wip_pairs());
        let be = open_pr(
            "https://github.com/tmgr-dev/backend/pull/5",
            false,
            "tmgr-dev",
            &"d".repeat(40),
        );
        let log = std::cell::RefCell::new(Vec::new());
        assert!(publish_all_with(&plan, &mut fake(&[("tmgr-dev/backend", &be)], &log)).is_err());
        assert_eq!(calls(&log, "ready"), 0);
        assert_eq!(calls(&log, "create"), 0);
    }

    #[test]
    fn a_plan_without_a_resume_or_a_paused_again_resume_marks_nothing_ready() {
        let url = "https://github.com/tmgr-dev/backend/pull/5";
        let be = open_pr(url, false, "tmgr-dev", &"a".repeat(40));
        let plain = publish_all_plan(&both()).unwrap();
        let again = publish_all_plan(&both())
            .unwrap()
            .resuming(&wip_pairs())
            .draft();
        for plan in [plain, again] {
            let log = std::cell::RefCell::new(Vec::new());
            publish_all_with(&plan, &mut fake(&[("tmgr-dev/backend", &be)], &log)).unwrap();
            assert_eq!(calls(&log, "ready"), 0);
        }
    }

    #[test]
    fn a_fork_another_owner_or_another_commit_is_never_reused() {
        let plan = publish_all_plan(&both()).unwrap();
        let url = "https://github.com/tmgr-dev/backend/pull/5";
        let sha = "a".repeat(40);
        for bad in [
            open_pr(url, true, "tmgr-dev", &sha),
            open_pr(url, false, "someone", &sha),
            open_pr(url, false, "tmgr-dev", &"c".repeat(40)),
            open_pr(
                "https://github.com/other/backend/pull/5",
                false,
                "tmgr-dev",
                &sha,
            ),
            "not json".to_string(),
        ] {
            let log = std::cell::RefCell::new(Vec::new());
            let e = publish_all_with(&plan, &mut fake(&[("tmgr-dev/backend", &bad)], &log))
                .unwrap_err();
            assert_eq!(e.kind, tmgr_core::error::Kind::Forbidden.clone(), "{bad}");
            assert!(log
                .borrow()
                .iter()
                .all(|a| a.get(2).map(String::as_str) != Some("create")
                    && a.get(2).map(String::as_str) != Some("edit")));
        }
    }

    #[test]
    fn a_link_that_is_not_a_pull_request_of_that_repository_is_refused() {
        let plan = publish_all_plan(&both()).unwrap();
        let mut bad = |argv: &[String], _: &Path| -> Result<String> {
            Ok(if argv.get(2).map(String::as_str) == Some("create") {
                "https://evil.example/x".into()
            } else {
                String::new()
            })
        };
        assert!(publish_all_with(&plan, &mut bad).is_err());
    }

    #[test]
    fn history_that_is_not_a_straight_line_on_the_default_branch_is_refused() {
        let msg = |r: Refusal| r.to_string();
        let e = refused(|i| i.merge_commits = 1);
        assert_eq!(
            msg(e),
            "publish refused: the agent's branch must be a straight line of new commits on master; merge commits and unrelated history are refused"
        );
        assert!(matches!(
            refused(|i| i.descends_from_base = false),
            Refusal::ForeignHistory(_)
        ));
        let mut inputs = both();
        inputs[1].descends_from_base = false;
        let e = publish_all_plan(&inputs).unwrap_err();
        assert_eq!(e.repo.as_deref(), Some("tmgr-dev/frontend"));
        assert!(matches!(e.refusal, Refusal::ForeignHistory(_)));
    }

    #[test]
    fn a_stale_list_is_queried_once_more_before_refusing() {
        let plan = publish_all_plan(&both()).unwrap();
        let url = "https://github.com/tmgr-dev/backend/pull/5";
        let stale = open_pr(url, false, "tmgr-dev", &"c".repeat(40));
        let fresh = open_pr(url, false, "tmgr-dev", &"a".repeat(40));
        let lists = std::cell::RefCell::new(vec![stale.clone(), fresh]);
        let mut exec = |argv: &[String], _: &Path| -> Result<String> {
            if argv.get(2).map(String::as_str) == Some("list") && argv[4] == "tmgr-dev/backend" {
                let mut l = lists.borrow_mut();
                return Ok(if l.len() > 1 {
                    l.remove(0)
                } else {
                    l[0].clone()
                });
            }
            Ok(if argv.get(2).map(String::as_str) == Some("create") {
                "https://github.com/tmgr-dev/frontend/pull/12".into()
            } else {
                "[]".into()
            })
        };
        let urls = publish_all_with(&plan, &mut exec).unwrap();
        assert_eq!(urls[0].1, url);

        let log = std::cell::RefCell::new(Vec::new());
        let e =
            publish_all_with(&plan, &mut fake(&[("tmgr-dev/backend", &stale)], &log)).unwrap_err();
        assert_eq!(e.kind, tmgr_core::error::Kind::Forbidden);
        let lists = log
            .borrow()
            .iter()
            .filter(|a| a.get(2).map(String::as_str) == Some("list"))
            .count();
        assert_eq!(lists, 2);
    }

    #[test]
    fn the_linked_body_never_passes_the_body_limit() {
        let body = "x".repeat(MAX_BODY);
        let linked = link_body(&body, &["https://github.com/tmgr-dev/frontend/pull/12"]);
        assert_eq!(linked.chars().count(), MAX_BODY);
        assert!(linked.ends_with("- https://github.com/tmgr-dev/frontend/pull/12"));
    }
}
