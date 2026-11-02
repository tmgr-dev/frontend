use super::attach_git::config_runs_programs;
use super::cloud::{read_record, record_path};
use super::{emit, is_down, rerr, runner_config, Opts};
use crate::gitx::parse_remote;
use serde_json::{json, Value};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use tmgr_core::approval::{AllowFile, RepoBinding};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::text::{sanitize_line, truncate_chars};
use tmgr_ipc::api::StateKind;
use tmgr_queue::clones::plan_clones;
use tmgr_queue::state::{self, Pickup, Status};
use tmgr_queue::workdir;
use tmgr_runner::client::Client;

const BUILD_DIRS: &[&str] = &[
    "target",
    "node_modules",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".turbo",
    ".cache",
    ".parcel-cache",
    ".gradle",
    "__pycache__",
    ".pytest_cache",
    ".venv",
    "venv",
    "coverage",
];

const PR_LIMIT: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Pr {
    pub number: u64,
    pub state: String,
    pub head: String,
    pub cross: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Prs {
    Listed(Vec<Pr>),
    Uncertain(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EntryKind {
    Tracked,
    Untracked,
    Ignored,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Entry {
    pub kind: EntryKind,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(super) struct GitFacts {
    pub head: String,
    pub on_branch: bool,
    pub entries: Vec<Entry>,
    pub stash: bool,
    pub in_progress: Option<String>,
    pub hidden: Option<String>,
    pub unmerged: Vec<String>,
}

#[derive(Clone, Debug)]
pub(super) struct CloneFacts {
    pub name: String,
    pub git: std::result::Result<GitFacts, String>,
    pub prs: Prs,
}

#[derive(Clone, Debug)]
pub(super) struct RunFacts {
    pub branch: String,
    pub in_use: Option<String>,
    pub proof: std::result::Result<(), String>,
    pub clones: Vec<CloneFacts>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrRef {
    pub number: u64,
    pub state: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Decision {
    Remove(Vec<PrRef>),
    Skip(String),
}

pub(super) fn parse_prs(text: &str) -> Prs {
    let uncertain = |why: &str| Prs::Uncertain(why.to_string());
    let Ok(Value::Array(list)) = serde_json::from_str::<Value>(text) else {
        return uncertain("gh answered with something that is not a list");
    };
    if list.len() >= PR_LIMIT {
        return uncertain("gh answered with the limit of 100 pull requests, the list may be cut");
    }
    let mut out = Vec::new();
    for v in &list {
        let (Some(number), Some(state), Some(head), Some(cross)) = (
            v["number"].as_u64(),
            v["state"].as_str(),
            v["headRefOid"].as_str(),
            v["isCrossRepository"].as_bool(),
        ) else {
            return uncertain("gh answered with a pull request that lacks fields");
        };
        match (state, v["mergedAt"].as_str()) {
            ("OPEN", _) | ("CLOSED", _) | ("MERGED", Some(_)) => {}
            ("MERGED", None) => return uncertain("a merged pull request has no merge time"),
            _ => return uncertain("gh answered with an unknown pull request state"),
        }
        out.push(Pr {
            number,
            state: state.to_string(),
            head: head.to_string(),
            cross,
        });
    }
    Prs::Listed(out)
}

pub(super) fn build_output(path: &str) -> bool {
    path.split('/')
        .filter(|c| !c.is_empty())
        .any(|c| BUILD_DIRS.contains(&c) || c == ".DS_Store")
}

pub(super) fn parse_status(bytes: &[u8]) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut parts = bytes.split(|b| *b == 0).filter(|p| !p.is_empty());
    while let Some(part) = parts.next() {
        if part.len() < 4 {
            continue;
        }
        let (x, y) = (part[0], part[1]);
        let path = String::from_utf8_lossy(&part[3..]).into_owned();
        if x == b'R' || x == b'C' || y == b'R' || y == b'C' {
            parts.next();
        }
        let kind = match (x, y) {
            (b'?', b'?') => EntryKind::Untracked,
            (b'!', b'!') => EntryKind::Ignored,
            _ => EntryKind::Tracked,
        };
        out.push(Entry { kind, path });
    }
    out
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(10)]
}

fn clean_clone(branch: &str, c: &CloneFacts) -> std::result::Result<PrRef, String> {
    let g = c.git.as_ref().map_err(|e| e.clone())?;
    if !g.on_branch {
        return Err(format!("HEAD is not on the branch {branch}"));
    }
    if g.stash {
        return Err("a stash exists".into());
    }
    if let Some(p) = &g.in_progress {
        return Err(format!("a {p} is in progress"));
    }
    if let Some(h) = &g.hidden {
        return Err(h.clone());
    }
    if let Some(b) = g.unmerged.first() {
        return Err(format!("the branch {b} is not merged into HEAD"));
    }
    for e in &g.entries {
        match e.kind {
            EntryKind::Tracked => return Err(format!("uncommitted change in {}", e.path)),
            EntryKind::Untracked => return Err(format!("untracked file {}", e.path)),
            EntryKind::Ignored if !build_output(&e.path) => {
                return Err(format!("ignored file {} is not build output", e.path))
            }
            EntryKind::Ignored => {}
        }
    }
    let list = match &c.prs {
        Prs::Uncertain(why) => return Err(format!("pull request state is uncertain: {why}")),
        Prs::Listed(l) => l,
    };
    let ours: Vec<&Pr> = list.iter().filter(|p| !p.cross).collect();
    if ours.is_empty() {
        return Err("no pull request for this branch".into());
    }
    if let Some(open) = ours.iter().find(|p| p.state == "OPEN") {
        return Err(format!("pull request #{} is open", open.number));
    }
    let at_head = |state: &str| ours.iter().find(|p| p.state == state && p.head == g.head);
    match at_head("MERGED").or_else(|| at_head("CLOSED")) {
        Some(p) => Ok(PrRef {
            number: p.number,
            state: p.state.clone(),
        }),
        None => Err(format!(
            "no merged or closed pull request has the copy's HEAD {} as its head (unpushed commits, or the pull request moved)",
            short(&g.head)
        )),
    }
}

pub(super) fn decide(f: &RunFacts) -> Decision {
    if let Some(why) = &f.in_use {
        return Decision::Skip(why.clone());
    }
    if let Err(why) = &f.proof {
        return Decision::Skip(why.clone());
    }
    if f.clones.is_empty() {
        return Decision::Skip("no clone to check".into());
    }
    let mut refs = Vec::new();
    for c in &f.clones {
        match clean_clone(&f.branch, c) {
            Ok(r) => refs.push(r),
            Err(why) if c.name.is_empty() => return Decision::Skip(why),
            Err(why) => return Decision::Skip(format!("clone {}: {why}", c.name)),
        }
    }
    Decision::Remove(refs)
}

fn under(inner: &Path, outer: &Path) -> bool {
    inner.starts_with(outer)
}

#[derive(Default)]
pub(super) struct Probe {
    sessions: Vec<(String, PathBuf)>,
    paths: Vec<PathBuf>,
}

impl Probe {
    pub(super) fn take() -> Result<Probe> {
        let cfg = runner_config()?;
        let mut client = Client::connect(&cfg.dir).map_err(|e| {
            if is_down(&e) {
                Error::new(
                    Kind::Offline,
                    "the runner is not running, so gc cannot tell whether a copy is in use",
                )
                .hint("gc does not start the runner; start it with `tmgr runner start` and run gc again")
            } else {
                rerr(e)
            }
        })?;
        let sessions = client
            .list()
            .map_err(rerr)?
            .into_iter()
            .filter(|s| s.state.kind != StateKind::Exited)
            .map(|s| {
                let cwd = std::fs::canonicalize(&s.cwd).unwrap_or_else(|_| PathBuf::from(&s.cwd));
                (s.id, cwd)
            })
            .collect();
        let paths = process_paths().map_err(|e| {
            Error::new(
                Kind::Conflict,
                format!("cannot list the folders processes work or run in: {e}"),
            )
            .hint("nothing was removed")
        })?;
        Ok(Probe { sessions, paths })
    }

    pub(super) fn holds(&self, folder: &Path, session: Option<&str>) -> Option<String> {
        for (id, cwd) in &self.sessions {
            if session == Some(id.as_str()) || under(cwd, folder) {
                return Some(format!(
                    "a live runner session ({}) uses it",
                    sanitize_line(id)
                ));
            }
        }
        self.paths
            .iter()
            .any(|c| under(c, folder))
            .then(|| "a process is working in it or running from it".to_string())
    }
}

#[cfg(any(target_os = "macos", test))]
pub(super) fn parse_lsof(text: &str) -> Vec<(u32, PathBuf)> {
    let mut pid = 0u32;
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some(p) = line.strip_prefix('p') {
            pid = p.parse().unwrap_or(0);
        } else if let Some(path) = line.strip_prefix('n') {
            out.push((pid, PathBuf::from(path)));
        }
    }
    out
}

#[cfg(target_os = "macos")]
fn raw_paths() -> std::result::Result<Vec<(u32, PathBuf)>, String> {
    let out = std::process::Command::new("/usr/sbin/lsof")
        .args(["-w", "-d", "cwd,txt", "-Fn"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("lsof: {e}"))?;
    Ok(parse_lsof(&String::from_utf8_lossy(&out.stdout)))
}

#[cfg(not(target_os = "macos"))]
fn raw_paths() -> std::result::Result<Vec<(u32, PathBuf)>, String> {
    let dir = std::fs::read_dir("/proc").map_err(|e| format!("/proc: {e}"))?;
    let mut out = Vec::new();
    for e in dir.filter_map(|e| e.ok()) {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        for link in ["cwd", "exe"] {
            if let Ok(p) = std::fs::read_link(e.path().join(link)) {
                out.push((pid, p));
            }
        }
    }
    Ok(out)
}

fn process_paths() -> std::result::Result<Vec<PathBuf>, String> {
    let all = raw_paths()?;
    let me = std::process::id();
    if !all.iter().any(|(p, _)| *p == me) {
        return Err("the scan did not list this process".into());
    }
    Ok(all
        .into_iter()
        .map(|(_, p)| std::fs::canonicalize(&p).unwrap_or(p))
        .collect())
}

fn git_text(dir: &Path, args: &[&str]) -> std::result::Result<Vec<u8>, String> {
    workdir::git_bytes(dir, args, None).map_err(|e| sanitize_line(&e.message))
}

fn exists(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

pub(super) fn git_facts(dir: &Path, branch: &str) -> std::result::Result<GitFacts, String> {
    let config = String::from_utf8_lossy(&git_text(
        dir,
        &["config", "--list", "--show-scope", "--name-only"],
    )?)
    .lines()
    .filter(|l| !l.starts_with("command\t"))
    .collect::<Vec<_>>()
    .join("\n");
    if config_runs_programs(&config) {
        return Err("the copy's local git config runs programs".into());
    }
    let redirects = config.lines().any(|l| {
        let key = l
            .split_once('\t')
            .map_or("", |(_, k)| k)
            .to_ascii_lowercase();
        key == "core.worktree" || key.starts_with("include")
    });
    if redirects {
        return Err("the copy's local git config redirects git".into());
    }
    if exists(&dir.join(".gitmodules")) || exists(&dir.join(".git/modules")) {
        return Err("the copy has submodules, which are not checked".into());
    }
    if !git_text(
        dir,
        &[
            "rev-list",
            "--max-count=1",
            "--exclude=refs/stash",
            "--all",
            "--not",
            "--remotes",
            "HEAD",
        ],
    )?
    .iter()
    .all(|b| b.is_ascii_whitespace())
    {
        return Err("local commits outside the branch".into());
    }
    let staged = git_text(dir, &["ls-files", "-s", "-z"])?;
    if staged.split(|b| *b == 0).any(|e| e.starts_with(b"160000 ")) {
        return Err("the copy has submodules, which are not checked".into());
    }
    let head = String::from_utf8_lossy(&git_text(dir, &["rev-parse", "--verify", "HEAD"])?)
        .trim()
        .to_string();
    let on_branch = git_text(dir, &["symbolic-ref", "-q", "HEAD"])
        .map(|b| String::from_utf8_lossy(&b).trim() == format!("refs/heads/{branch}"))
        .unwrap_or(false);
    let entries = parse_status(&git_text(
        dir,
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--ignored=matching",
        ],
    )?);
    let stash = !git_text(dir, &["for-each-ref", "--format=%(refname)", "refs/stash"])?
        .iter()
        .all(|b| b.is_ascii_whitespace());
    let in_progress = [
        ("MERGE_HEAD", "merge"),
        ("CHERRY_PICK_HEAD", "cherry-pick"),
        ("REVERT_HEAD", "revert"),
        ("rebase-merge", "rebase"),
        ("rebase-apply", "rebase"),
        ("BISECT_LOG", "bisect"),
    ]
    .iter()
    .find(|(f, _)| exists(&dir.join(".git").join(f)))
    .map(|(_, what)| what.to_string());
    let listed = git_text(dir, &["ls-files", "-v", "-z"])?;
    let hidden = listed.split(|b| *b == 0).find_map(|e| {
        let tag = *e.first()?;
        let path = String::from_utf8_lossy(e.get(2..)?).into_owned();
        if tag.is_ascii_lowercase() {
            Some(format!("assume-unchanged entry {path}"))
        } else if tag == b'S' {
            Some(format!("skip-worktree entry {path}"))
        } else {
            None
        }
    });
    let unmerged = String::from_utf8_lossy(&git_text(
        dir,
        &[
            "for-each-ref",
            "--no-merged=HEAD",
            "--format=%(refname:short)",
            "refs/heads",
        ],
    )?)
    .lines()
    .map(str::to_string)
    .collect();
    Ok(GitFacts {
        head,
        on_branch,
        entries,
        stash,
        in_progress,
        hidden,
        unmerged,
    })
}

fn github_name(repo: &Path) -> std::result::Result<String, String> {
    let url = |args: &[&str]| workdir::git_query(repo, args).map_err(|e| sanitize_line(&e.message));
    let fetch = url(&["remote", "get-url", "origin"])?;
    let push = url(&["remote", "get-url", "--push", "origin"])?;
    let parse = |u: &str| {
        parse_remote(u)
            .filter(|r| r.host == "github.com" && r.path.split('/').count() == 2)
            .map(|r| r.path)
    };
    match (parse(&fetch), parse(&push)) {
        (Some(f), Some(p)) if f.eq_ignore_ascii_case(&p) => Ok(f),
        _ => Err("the origin of the repository does not fetch from and push to one github.com repository".into()),
    }
}

fn lookup_prs(full_name: &str, branch: &str, repo: &Path) -> Prs {
    let argv: Vec<String> = [
        "gh",
        "pr",
        "list",
        "--repo",
        full_name,
        "--head",
        branch,
        "--state",
        "all",
        "--limit",
        "100",
        "--json",
        "number,state,headRefOid,mergedAt,isCrossRepository,url",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    match tmgr_queue::publish::run_hardened(&argv, repo) {
        Ok(text) => parse_prs(&text),
        Err(e) => Prs::Uncertain(sanitize_line(&e.message)),
    }
}

struct CloneSpec {
    name: String,
    dir: PathBuf,
    repo: std::result::Result<(PathBuf, String), String>,
    identity: Option<(u64, u64)>,
}

enum Origin {
    Record(PathBuf),
    Pickup(String),
}

struct Candidate {
    origin: Origin,
    shown: PathBuf,
    folder: PathBuf,
    levels: usize,
    branch: String,
    session: Option<String>,
    multi: bool,
    clones: Vec<CloneSpec>,
    unusable: Option<String>,
    in_use: Option<String>,
    drop_if_gone: bool,
}

impl Candidate {
    fn broken(origin: Origin, shown: PathBuf, why: impl Into<String>) -> Candidate {
        Candidate {
            origin,
            folder: shown.clone(),
            shown,
            levels: 1,
            branch: String::new(),
            session: None,
            multi: false,
            clones: Vec::new(),
            unusable: Some(why.into()),
            in_use: None,
            drop_if_gone: false,
        }
    }
}

fn allowed_paths(full_name: &str) -> Vec<PathBuf> {
    let Ok(file) = AllowFile::load() else {
        return Vec::new();
    };
    file.entries
        .iter()
        .flat_map(|e| e.repos.values())
        .filter_map(|b| match b {
            RepoBinding::Set(s) => s.get(full_name).map(PathBuf::from),
            RepoBinding::Path(_) => None,
        })
        .collect()
}

fn verified_repo(full_name: &str, anchor: &Path) -> std::result::Result<(PathBuf, String), String> {
    let mut candidates = vec![anchor.to_path_buf()];
    candidates.extend(allowed_paths(full_name));
    for path in candidates {
        let Ok(real) = std::fs::canonicalize(&path) else {
            continue;
        };
        if real != path {
            continue;
        }
        if github_name(&real).is_ok_and(|n| n.eq_ignore_ascii_case(full_name)) {
            return Ok((real, full_name.to_string()));
        }
    }
    Err(format!(
        "no local repository is verified to be {}",
        sanitize_line(full_name)
    ))
}

fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0'])
        && name.len() <= 100
}

fn record_candidates() -> Vec<Candidate> {
    let Some(dir) = tmgr_core::active::state_dir().map(|d| d.join("task-runs")) else {
        return Vec::new();
    };
    let Ok(read) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = read
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files.into_iter().map(|f| record_candidate(&f)).collect()
}

fn record_candidate(file: &Path) -> Candidate {
    let origin = || Origin::Record(file.to_path_buf());
    let parsed = file
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.rsplit_once('-'))
        .and_then(|(code, id)| Some((code.to_string(), id.parse::<i64>().ok()?)))
        .filter(|(code, _)| !code.is_empty());
    let Some((code, id)) = parsed else {
        return Candidate::broken(
            origin(),
            file.to_path_buf(),
            "the record name is not <workspace>-<task id>",
        );
    };
    if record_path(&code, id).as_deref() != Some(file) {
        return Candidate::broken(
            origin(),
            file.to_path_buf(),
            "the record is not in the state folder",
        );
    }
    let Ok(rec) = read_record(&code, id) else {
        return Candidate::broken(
            origin(),
            file.to_path_buf(),
            "the record is damaged or is not a cloud run",
        );
    };
    let raw: Value = std::fs::read_to_string(file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    let shown = rec.clone.clone();
    let broken = |why: &str| Candidate::broken(origin(), shown.clone(), why);
    if tmgr_core::branch::validate_branch_name(&rec.branch).is_err() {
        return broken("the recorded branch name is not valid");
    }
    let Some(expected) = workdir::run_clone_dir(&rec.repo, &rec.branch) else {
        return broken("the recorded repository path has no parent folder");
    };
    if expected != rec.clone {
        return broken("the recorded clone is not where tmgr run puts the clone of this branch");
    }
    let anchor = PathBuf::from(&rec.repo);
    let multi = !rec.clones.is_empty();
    let clones = if multi {
        rec.clones
            .iter()
            .map(|c| CloneSpec {
                name: c.name.clone(),
                dir: expected.join(&c.name),
                repo: verified_repo(&c.repo, &anchor),
                identity: c.identity,
            })
            .collect()
    } else {
        vec![CloneSpec {
            name: String::new(),
            dir: expected.clone(),
            repo: github_name(&anchor).map(|n| (anchor.clone(), n)),
            identity: None,
        }]
    };
    let in_use = (!raw["held"].is_null()).then(|| "the run is held by the leak gate".to_string());
    Candidate {
        origin: origin(),
        shown,
        folder: expected,
        levels: 1,
        branch: rec.branch.clone(),
        session: rec.session.clone(),
        multi,
        clones,
        unusable: None,
        in_use,
        drop_if_gone: false,
    }
}

fn pickup_in_use(p: &Pickup) -> Option<String> {
    let why = match p.status {
        Status::PendingApproval | Status::Approved | Status::Starting | Status::UnknownStart => {
            "the queue pickup has not finished starting"
        }
        Status::Started if !p.run_exited => "the queue run is still running",
        _ if p.held.is_some() => "the queue run is held by the leak gate",
        _ if p.paused_draft => "the queue run is paused",
        _ if p.awaits_publish() => "the queue run waits to be published",
        _ => return None,
    };
    Some(why.to_string())
}

fn pickup_candidates() -> std::result::Result<Vec<Candidate>, String> {
    let paths = tmgr_queue::Paths::from_env().map_err(|e| e.message)?;
    let st = state::load(&paths).map_err(|e| e.message)?;
    Ok(st.pickups.iter().map(pickup_candidate).collect())
}

fn pickup_candidate(p: &Pickup) -> Candidate {
    let cwd = PathBuf::from(&p.bundle.launch.cwd);
    let origin = || Origin::Pickup(p.id.clone());
    let broken = |why: &str| {
        let mut c = Candidate::broken(origin(), cwd.clone(), why);
        c.drop_if_gone = p.released;
        c
    };
    let Some(claim) = &p.cloud else {
        return broken(
            "not a cloud run: it is never published, so there is no pull request to check",
        );
    };
    if tmgr_core::branch::validate_branch_name(&claim.branch).is_err() {
        return broken("the recorded branch name is not valid");
    }
    let multi = !claim.repos.is_empty();
    let (anchor, clones) = if multi {
        let Some(primary) = claim
            .repos
            .iter()
            .min_by(|a, b| a.full_name.cmp(&b.full_name))
        else {
            return broken("the pickup lists no repository");
        };
        let Ok(plans) = plan_clones(&claim.repos) else {
            return broken("the clone names of the pickup cannot be derived");
        };
        let mut specs = Vec::new();
        for plan in &plans {
            let ident = p.clones.iter().find(|c| c.name == plan.name);
            specs.push(CloneSpec {
                name: plan.name.clone(),
                dir: cwd.join(&plan.name),
                repo: verified_repo(&plan.full_name, Path::new(&plan.path)),
                identity: ident.map(|c| (c.dev, c.ino)),
            });
        }
        if p.clones.len() != specs.len() {
            return broken("the pickup does not record every clone");
        }
        (PathBuf::from(&primary.path), specs)
    } else {
        let anchor = cwd
            .parent()
            .and_then(Path::parent)
            .and_then(|wt| {
                let stem = wt.file_name()?.to_str()?.strip_suffix("-worktrees")?;
                Some(wt.parent()?.join(stem))
            })
            .unwrap_or_default();
        let spec = CloneSpec {
            name: String::new(),
            dir: cwd.clone(),
            repo: github_name(&anchor).map(|n| (anchor.clone(), n)),
            identity: None,
        };
        (anchor, vec![spec])
    };
    let Some(expected) = workdir::clone_dir(&anchor.to_string_lossy(), &p.id) else {
        return broken("the repository path has no parent folder");
    };
    if expected != cwd {
        return broken("the recorded folder is not where the queue puts the clone of this pickup");
    }
    Candidate {
        origin: origin(),
        shown: cwd.clone(),
        folder: cwd,
        levels: 2,
        branch: claim.branch.clone(),
        session: p.run_id.clone(),
        multi,
        clones,
        unusable: None,
        in_use: pickup_in_use(p),
        drop_if_gone: p.released,
    }
}

fn real_dir(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir())
}

fn prove_clone(spec: &CloneSpec) -> std::result::Result<(), String> {
    let git = spec.dir.join(".git");
    if !real_dir(&git) {
        return Err("the clone's .git is not a real folder".into());
    }
    if exists(&git.join("commondir")) {
        return Err("the clone shares another repository's git folder".into());
    }
    let linked = std::fs::read_dir(git.join("worktrees")).is_ok_and(|mut d| d.next().is_some());
    if linked {
        return Err("the clone has linked worktrees".into());
    }
    Ok(())
}

fn prove(c: &Candidate) -> std::result::Result<(), String> {
    let folder = &c.folder;
    if !real_dir(folder) {
        return Err("the copy is not a real folder (a symlink or a file)".into());
    }
    if std::fs::canonicalize(folder).ok().as_deref() != Some(folder.as_path()) {
        return Err("the copy's path goes through a symlink".into());
    }
    let wt = folder.ancestors().nth(c.levels);
    let wt_ok = wt.is_some_and(|w| {
        real_dir(w)
            && w.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("-worktrees"))
    });
    if !wt_ok {
        return Err("the copy is not inside a <repo>-worktrees folder".into());
    }
    for spec in &c.clones {
        if !plain_name(&spec.name) && c.multi {
            return Err("a clone name is not a plain folder name".into());
        }
        prove_clone(spec).map_err(|e| {
            if c.multi {
                format!("clone {}: {e}", spec.name)
            } else {
                e
            }
        })?;
        if c.multi {
            let want = spec.identity;
            if want.is_none() || workdir::clone_identity(&spec.dir) != want {
                return Err(format!(
                    "clone {} is not the folder this run started with",
                    sanitize_line(&spec.name)
                ));
            }
        }
    }
    if c.multi {
        let names: std::collections::BTreeSet<String> = std::fs::read_dir(folder)
            .map_err(|e| format!("cannot read the run folder: {e}"))?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        let want: std::collections::BTreeSet<String> =
            c.clones.iter().map(|s| s.name.clone()).collect();
        if names != want {
            return Err("the run folder holds something besides its clones".into());
        }
    }
    Ok(())
}

fn gather(c: &Candidate, probe: &Probe, prs: Option<&[Prs]>) -> RunFacts {
    let proof = prove(c);
    let in_use = c
        .in_use
        .clone()
        .or_else(|| probe.holds(&c.folder, c.session.as_deref()));
    let mut clones = Vec::new();
    if proof.is_ok() && in_use.is_none() {
        for (i, spec) in c.clones.iter().enumerate() {
            let git = git_facts(&spec.dir, &c.branch);
            let prs = match (prs.and_then(|p| p.get(i)), &spec.repo) {
                (Some(p), _) => p.clone(),
                (None, Ok((path, full))) if git.is_ok() => lookup_prs(full, &c.branch, path),
                (None, Ok(_)) => Prs::Uncertain("not looked up".into()),
                (None, Err(e)) => Prs::Uncertain(e.clone()),
            };
            clones.push(CloneFacts {
                name: spec.name.clone(),
                git,
                prs,
            });
        }
    }
    RunFacts {
        branch: c.branch.clone(),
        in_use,
        proof,
        clones,
    }
}

fn dir_size(root: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in read.filter_map(|e| e.ok()) {
            let Ok(m) = std::fs::symlink_metadata(e.path()) else {
                continue;
            };
            total += m.blocks() * 512;
            if m.is_dir() {
                stack.push(e.path());
            }
        }
    }
    total
}

fn remove_folder(folder: &Path, levels: usize) -> std::result::Result<(), String> {
    let wt = folder
        .ancestors()
        .nth(levels)
        .ok_or("the copy has no worktrees folder")?;
    let id = |p: &Path| std::fs::symlink_metadata(p).map(|m| (m.dev(), m.ino()));
    let before = id(folder).map_err(|e| e.to_string())?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let hidden = wt.join(format!(".tmgr-gc-{}-{nanos}", std::process::id()));
    if exists(&hidden) {
        return Err("the temporary name is taken".into());
    }
    std::fs::rename(folder, &hidden).map_err(|e| format!("cannot move the copy aside: {e}"))?;
    if id(&hidden).ok() != Some(before) {
        let _ = std::fs::rename(&hidden, folder);
        return Err("the copy changed while it was being moved".into());
    }
    std::fs::remove_dir_all(&hidden).map_err(|e| {
        format!(
            "{e}; what is left is in {}",
            sanitize_line(&hidden.display().to_string())
        )
    })
}

struct Item {
    path: PathBuf,
    decision: &'static str,
    pr: Vec<PrRef>,
    reason: Option<String>,
    bytes: u64,
}

impl Item {
    fn new(path: &Path, decision: &'static str, reason: Option<String>) -> Item {
        Item {
            path: path.to_path_buf(),
            decision,
            pr: Vec::new(),
            reason,
            bytes: 0,
        }
    }
}

fn after_removal(c: &Candidate) -> Option<String> {
    match &c.origin {
        Origin::Record(file) => std::fs::remove_file(file)
            .err()
            .map(|e| format!("the record could not be deleted: {e}")),
        Origin::Pickup(id) => release_pickup(id)
            .err()
            .map(|e| format!("the queue pickup could not be marked: {}", e.message)),
    }
}

fn release_pickup(id: &str) -> Result<()> {
    let paths = tmgr_queue::Paths::from_env()?;
    let _lock = paths.lock()?;
    let mut st = state::load(&paths)?;
    if let Some(p) = st.pickups.iter_mut().find(|p| p.id == id) {
        p.released = true;
        p.clones.clear();
    }
    state::save(&paths, &mut st)
}

fn pickup_changed(id: &str) -> Option<String> {
    let unreadable = || Some("cannot read queue state".to_string());
    let Ok(paths) = tmgr_queue::Paths::from_env() else {
        return unreadable();
    };
    let Ok(st) = state::load(&paths) else {
        return unreadable();
    };
    match st.pickups.iter().find(|p| p.id == id) {
        Some(p) => pickup_in_use(p),
        None => Some("the queue pickup disappeared".into()),
    }
}

fn evaluate(c: &Candidate, probe: &mut Option<Probe>, dry: bool) -> Result<Option<Item>> {
    if let Some(why) = &c.unusable {
        return Ok(Some(Item::new(&c.shown, "skipped", Some(why.clone()))));
    }
    match std::fs::symlink_metadata(&c.folder) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((!c.drop_if_gone).then(|| Item::new(&c.shown, "gone", None)));
        }
        Err(e) => {
            return Ok(Some(Item::new(
                &c.shown,
                "skipped",
                Some(format!("cannot inspect the copy: {e}")),
            )))
        }
        Ok(_) => {}
    }
    if probe.is_none() {
        *probe = Some(Probe::take()?);
    }
    let facts = gather(c, probe.as_ref().expect("probe taken"), None);
    let refs = match decide(&facts) {
        Decision::Skip(why) => return Ok(Some(Item::new(&c.shown, "skipped", Some(why)))),
        Decision::Remove(refs) => refs,
    };
    let bytes = dir_size(&c.folder);
    let mut item = Item::new(&c.shown, "would_remove", None);
    item.pr = refs;
    item.bytes = bytes;
    if dry {
        return Ok(Some(item));
    }
    let changed = |why: String| {
        let mut it = Item::new(
            &c.shown,
            "skipped",
            Some(format!("changed during the check: {why}")),
        );
        it.pr = Vec::new();
        it
    };
    if let Origin::Pickup(id) = &c.origin {
        if let Some(why) = pickup_changed(id) {
            return Ok(Some(changed(why)));
        }
    }
    let fresh = match Probe::take() {
        Ok(p) => p,
        Err(e) => return Ok(Some(changed(e.message))),
    };
    let known: Vec<Prs> = facts.clones.iter().map(|f| f.prs.clone()).collect();
    let again = gather(c, &fresh, Some(&known));
    if let Decision::Skip(why) = decide(&again) {
        return Ok(Some(changed(why)));
    }
    match remove_folder(&c.folder, c.levels) {
        Ok(()) => {
            item.reason = after_removal(c);
            item.decision = if item.reason.is_some() {
                "failed"
            } else {
                "removed"
            };
        }
        Err(e) => {
            item.decision = "failed";
            item.reason = Some(e);
            item.bytes = 0;
        }
    }
    Ok(Some(item))
}

fn size_text(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

fn clean(s: &str) -> String {
    truncate_chars(&sanitize_line(s), 300).0
}

fn pr_text(pr: &[PrRef]) -> String {
    pr.iter()
        .map(|p| format!("#{} {}", p.number, p.state.to_lowercase()))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn run(o: &Opts, dry: bool) -> Result<i32> {
    let mut cands = record_candidates();
    let mut warnings = Vec::new();
    match pickup_candidates() {
        Ok(p) => cands.extend(p),
        Err(e) => warnings.push(format!(
            "the queue state was not read, queue copies are not checked: {e}"
        )),
    }
    let mut probe = None;
    let mut items = Vec::new();
    for c in &cands {
        if let Some(item) = evaluate(c, &mut probe, dry)? {
            items.push(item);
        }
    }
    let acts = |d: &str| d == "removed" || d == "would_remove";
    let reclaimed: u64 = items
        .iter()
        .filter(|i| acts(i.decision))
        .map(|i| i.bytes)
        .sum();
    let count = items.iter().filter(|i| acts(i.decision)).count();
    let failed = items.iter().filter(|i| i.decision == "failed").count();
    let data = json!({
        "dry_run": dry,
        "copies": items.iter().map(|i| json!({
            "path": clean(&i.path.display().to_string()),
            "decision": i.decision,
            "pr": i.pr.first().map(|p| json!({"number": p.number, "state": p.state})),
            "pull_requests": i.pr.iter().map(|p| json!({"number": p.number, "state": p.state})).collect::<Vec<_>>(),
            "reason": i.reason.as_deref().map(clean),
            "bytes": i.bytes,
        })).collect::<Vec<_>>(),
        "reclaimed_bytes": reclaimed,
        "count": count,
        "failed": failed,
        "warnings": warnings.iter().map(|w| clean(w)).collect::<Vec<_>>(),
    });
    let mut human = String::new();
    for i in &items {
        let label = match i.decision {
            "would_remove" => "would remove",
            d => d,
        };
        human.push_str(&format!(
            "{label}: {}\n",
            clean(&i.path.display().to_string())
        ));
        let mut notes = Vec::new();
        if !i.pr.is_empty() {
            notes.push(format!("PR {}", pr_text(&i.pr)));
        }
        if let Some(r) = &i.reason {
            notes.push(clean(r));
        }
        if i.bytes > 0 {
            notes.push(size_text(i.bytes));
        }
        if !notes.is_empty() {
            human.push_str(&format!("    {}\n", notes.join("; ")));
        }
    }
    for w in &warnings {
        human.push_str(&format!("warning: {}\n", clean(w)));
    }
    let verb = if dry { "would reclaim" } else { "reclaimed" };
    human.push_str(&format!(
        "{verb} {} from {count} cop{}",
        size_text(reclaimed),
        if count == 1 { "y" } else { "ies" }
    ));
    emit(o, data, human);
    Ok(if failed > 0 { 1 } else { 0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn pr(number: u64, state: &str, head: &str, cross: bool) -> Pr {
        Pr {
            number,
            state: state.into(),
            head: head.into(),
            cross,
        }
    }

    fn clean_git() -> GitFacts {
        GitFacts {
            head: HEAD.into(),
            on_branch: true,
            ..GitFacts::default()
        }
    }

    fn facts(git: GitFacts, prs: Prs) -> RunFacts {
        RunFacts {
            branch: "tmgr/x".into(),
            in_use: None,
            proof: Ok(()),
            clones: vec![CloneFacts {
                name: String::new(),
                git: Ok(git),
                prs,
            }],
        }
    }

    fn merged() -> Prs {
        Prs::Listed(vec![pr(5, "MERGED", HEAD, false)])
    }

    fn skip_of(f: &RunFacts) -> String {
        match decide(f) {
            Decision::Skip(s) => s,
            d => panic!("expected a skip, got {d:?}"),
        }
    }

    fn entry(kind: EntryKind, path: &str) -> Entry {
        Entry {
            kind,
            path: path.into(),
        }
    }

    #[test]
    fn a_clean_merged_copy_is_removed() {
        let f = facts(clean_git(), merged());
        assert_eq!(
            decide(&f),
            Decision::Remove(vec![PrRef {
                number: 5,
                state: "MERGED".into()
            }])
        );
    }

    #[test]
    fn a_closed_pr_at_head_is_removed() {
        let f = facts(clean_git(), Prs::Listed(vec![pr(6, "CLOSED", HEAD, false)]));
        assert!(matches!(decide(&f), Decision::Remove(_)));
    }

    #[test]
    fn pr_table() {
        let other = "b".repeat(40);
        let cases: Vec<(Prs, &str)> = vec![
            (Prs::Listed(vec![]), "no pull request"),
            (
                Prs::Listed(vec![pr(1, "MERGED", HEAD, true)]),
                "no pull request",
            ),
            (
                Prs::Listed(vec![
                    pr(1, "MERGED", HEAD, false),
                    pr(2, "OPEN", HEAD, false),
                ]),
                "#2 is open",
            ),
            (
                Prs::Listed(vec![pr(1, "MERGED", &other, false)]),
                "unpushed",
            ),
            (Prs::Uncertain("gh failed".into()), "uncertain"),
        ];
        for (prs, want) in cases {
            let f = facts(clean_git(), prs.clone());
            let got = skip_of(&f);
            assert!(got.contains(want), "{prs:?}: {got}");
        }
    }

    #[test]
    fn copy_state_table() {
        let mut cases: Vec<(GitFacts, &str)> = Vec::new();
        let mut g = clean_git();
        g.on_branch = false;
        cases.push((g, "not on the branch"));
        let mut g = clean_git();
        g.stash = true;
        cases.push((g, "stash"));
        let mut g = clean_git();
        g.in_progress = Some("merge".into());
        cases.push((g, "merge is in progress"));
        let mut g = clean_git();
        g.hidden = Some("skip-worktree entry a".into());
        cases.push((g, "skip-worktree"));
        let mut g = clean_git();
        g.unmerged = vec!["side".into()];
        cases.push((g, "side"));
        let mut g = clean_git();
        g.entries = vec![entry(EntryKind::Tracked, "a.rs")];
        cases.push((g, "a.rs"));
        let mut g = clean_git();
        g.entries = vec![entry(EntryKind::Untracked, "n.txt")];
        cases.push((g, "n.txt"));
        let mut g = clean_git();
        g.entries = vec![entry(EntryKind::Ignored, ".env")];
        cases.push((g, ".env"));
        for (g, want) in cases {
            let got = skip_of(&facts(g, merged()));
            assert!(got.contains(want), "{want}: {got}");
        }
    }

    #[test]
    fn build_output_may_stay_ignored() {
        let mut g = clean_git();
        g.entries = vec![
            entry(EntryKind::Ignored, "target/"),
            entry(EntryKind::Ignored, "web/node_modules/"),
            entry(EntryKind::Ignored, "a/.DS_Store"),
        ];
        assert!(matches!(decide(&facts(g, merged())), Decision::Remove(_)));
    }

    #[test]
    fn build_output_names() {
        assert!(build_output("target/debug/x"));
        assert!(build_output("a/.venv"));
        assert!(!build_output(".env"));
        assert!(!build_output("targets/x"));
        assert!(!build_output("src/.env.local"));
    }

    #[test]
    fn use_and_proof_come_first() {
        let mut f = facts(clean_git(), merged());
        f.proof = Err("symlink".into());
        assert_eq!(skip_of(&f), "symlink");
        f.in_use = Some("held".into());
        assert_eq!(skip_of(&f), "held");
    }

    #[test]
    fn one_bad_clone_blocks_all() {
        let mut f = facts(clean_git(), merged());
        f.clones[0].name = "a".into();
        let mut dirty = clean_git();
        dirty.entries = vec![entry(EntryKind::Untracked, "x")];
        f.clones.push(CloneFacts {
            name: "b".into(),
            git: Ok(dirty),
            prs: merged(),
        });
        let got = skip_of(&f);
        assert!(got.starts_with("clone b:"), "{got}");
    }

    #[test]
    fn a_failed_git_gathering_is_a_skip() {
        let mut f = facts(clean_git(), merged());
        f.clones[0].git = Err("the copy's local git config runs programs".into());
        assert!(skip_of(&f).contains("runs programs"));
    }

    #[test]
    fn gh_output_is_parsed_strictly() {
        let ok = format!(
            r#"[{{"number":5,"state":"MERGED","headRefOid":"{HEAD}","mergedAt":"2026-01-01T00:00:00Z","isCrossRepository":false,"url":"u"}}]"#
        );
        assert!(matches!(parse_prs(&ok), Prs::Listed(l) if l.len() == 1));
        for bad in [
            "",
            "{}",
            "[{\"number\":1}]",
            r#"[{"number":1,"state":"MERGED","headRefOid":"a","mergedAt":null,"isCrossRepository":false}]"#,
            r#"[{"number":1,"state":"WEIRD","headRefOid":"a","mergedAt":null,"isCrossRepository":false}]"#,
        ] {
            assert!(matches!(parse_prs(bad), Prs::Uncertain(_)), "{bad}");
        }
        let many = format!("[{}]", vec![&ok[1..ok.len() - 1]; 100].join(","));
        assert!(matches!(parse_prs(&many), Prs::Uncertain(_)));
    }

    #[test]
    fn status_output_is_parsed_with_renames() {
        let raw = b" M a.rs\0?? b.txt\0!! target/\0R  new.rs\0old.rs\0A  c.rs\0";
        let got = parse_status(raw);
        let paths: Vec<&str> = got.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["a.rs", "b.txt", "target/", "new.rs", "c.rs"]);
        assert_eq!(got[1].kind, EntryKind::Untracked);
        assert_eq!(got[2].kind, EntryKind::Ignored);
        assert_eq!(got[3].kind, EntryKind::Tracked);
    }

    #[test]
    fn lsof_output_is_parsed() {
        let got = parse_lsof("p12\nfcwd\nn/a b/c\np13\nfcwd\nn/\n");
        assert_eq!(
            got,
            vec![(12, PathBuf::from("/a b/c")), (13, PathBuf::from("/"))]
        );
    }

    #[test]
    fn a_process_folder_matches_by_component() {
        let probe = Probe {
            sessions: vec![],
            paths: vec![PathBuf::from("/w/repo-worktrees/x-2/sub")],
        };
        assert!(probe
            .holds(Path::new("/w/repo-worktrees/x"), None)
            .is_none());
        assert!(probe
            .holds(Path::new("/w/repo-worktrees/x-2"), None)
            .is_some());
    }

    #[test]
    fn a_session_matches_by_id_or_folder() {
        let probe = Probe {
            sessions: vec![("s_1".into(), PathBuf::from("/other"))],
            paths: vec![],
        };
        assert!(probe.holds(Path::new("/w/x"), Some("s_1")).is_some());
        assert!(probe.holds(Path::new("/w/x"), Some("s_2")).is_none());
    }

    #[test]
    fn size_text_units() {
        assert_eq!(size_text(10), "10 B");
        assert_eq!(size_text(2048), "2.0 KiB");
    }
}
