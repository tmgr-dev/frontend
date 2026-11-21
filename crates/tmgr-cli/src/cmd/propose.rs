use crate::cli_agent::{ProposeCmd, ProposeWait};
use crate::ctx::Ctx;
use crate::out::Output;
use tmgr_core::error::{Error, Result};

#[cfg(not(unix))]
pub fn run(_ctx: &Ctx, _command: &ProposeCmd) -> Result<Output> {
    Err(Error::usage(
        "tmgr propose is not supported on this platform (the Decisions inbox is macOS and Linux only)",
    ))
}

#[cfg(unix)]
pub use unix::{mcp_call, run};

#[cfg(unix)]
mod unix {
    use super::*;
    use crate::cmd::decisions::{
        event_text, is_refusal, parent_pid, send_action_card, source, wait_loop, Waited,
        ACTIONS_DOWN, ACTIONS_OLD, MAX_TITLE,
    };
    use crate::cmd::notifyenv::NotifyEnv;
    use serde_json::{json, Value};
    use std::io::Read;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};
    use tmgr_core::error::Kind;
    use tmgr_ipc::actions::{
        ActionIn, CommentPayload, Commit, PrPayload, TagPayload, TaskPayload, COMMITS_MAX,
    };
    use tmgr_ipc::decisions::{CardIn, DiffSpec, Event};

    pub const EXIT_NO_ANSWER: i32 = 8;
    pub const EXIT_REJECTED: i32 = 11;
    pub const EXIT_FAILED: i32 = 12;
    pub const EXIT_DISMISSED: i32 = 13;

    const GIT_TIMEOUT: Duration = Duration::from_secs(60);
    const LABEL_CHARS: usize = 70;
    const SCRUBBED: [&str; 4] = [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
    ];
    const RETRY: Duration = Duration::from_secs(2);

    pub enum Proposal {
        Comment {
            task: String,
            text: String,
        },
        Task {
            workspace: String,
            title: String,
            description: String,
            category: Option<u64>,
            status: Option<u64>,
        },
        Pr {
            repo_dir: Option<String>,
            branch: Option<String>,
            base: Option<String>,
            title: String,
            body: String,
        },
        Tag {
            repo_dir: Option<String>,
            name: String,
            target: Option<String>,
        },
    }

    pub fn github_repo(url: &str) -> std::result::Result<String, String> {
        let url = url.trim();
        let scheme_ok = url.starts_with("https://")
            || url.starts_with("ssh://")
            || (!url.contains("://") && url.contains('@') && url.contains(':'));
        if !scheme_ok {
            return Err(format!(
                "the origin remote {url:?} is not a github.com https or ssh url"
            ));
        }
        let authority_has_secret = url
            .strip_prefix("https://")
            .and_then(|r| r.split('/').next())
            .is_some_and(|a| a.contains('@'));
        match crate::gitx::parse_remote(url) {
            Some(r)
                if r.host == "github.com"
                    && !authority_has_secret
                    && r.path.split('/').count() == 2 =>
            {
                Ok(r.path)
            }
            _ => Err(format!(
                "the origin remote {url:?} is not a github.com owner/name repository"
            )),
        }
    }

    fn git(dir: &Path, args: &[&str]) -> std::result::Result<String, String> {
        let mut full = vec!["-c", "core.fsmonitor=false"];
        full.extend_from_slice(args);
        let out = crate::gitx::run_capture_with(
            crate::gitx::git_program(),
            Some(dir),
            &full,
            GIT_TIMEOUT,
            &[],
            &SCRUBBED,
        )
        .map_err(|e| format!("could not run git: {e}"))?;
        if out.ok {
            Ok(out.stdout.trim().to_string())
        } else {
            Err(format!("git {} failed", args.first().unwrap_or(&"")))
        }
    }

    fn remote_ref(dir: &Path, name: &str) -> std::result::Result<Option<String>, String> {
        let out = git(dir, &["ls-remote", "origin", name])?;
        Ok(out.lines().find_map(|l| {
            let (sha, r) = l.split_once('\t')?;
            (r == name).then(|| sha.to_string())
        }))
    }

    fn remote_default_branch(dir: &Path) -> std::result::Result<String, String> {
        let out = git(dir, &["ls-remote", "--symref", "origin", "HEAD"])?;
        out.lines()
            .find_map(|l| {
                l.strip_prefix("ref: refs/heads/")
                    .and_then(|r| r.split('\t').next())
                    .map(str::to_string)
            })
            .ok_or_else(|| "could not find the default branch of origin; pass --base".to_string())
    }

    fn toplevel(dir: &Path) -> std::result::Result<PathBuf, String> {
        let top = git(dir, &["rev-parse", "--show-toplevel"])
            .map_err(|_| format!("{} is not inside a git repository", dir.display()))?;
        std::fs::canonicalize(top).map_err(|e| e.to_string())
    }

    fn origin_repo(top: &Path) -> std::result::Result<String, String> {
        let url = git(top, &["config", "--get", "remote.origin.url"])
            .map_err(|_| "the repository has no origin remote".to_string())?;
        github_repo(&url)
    }

    fn safe_rev(rev: &str) -> std::result::Result<(), String> {
        if rev.is_empty() || rev.starts_with('-') || rev.chars().any(char::is_control) {
            return Err(format!("{rev:?} is not a valid revision"));
        }
        Ok(())
    }

    pub fn pr_payload(
        dir: &Path,
        branch: Option<&str>,
        base: Option<&str>,
        title: &str,
        body: &str,
    ) -> std::result::Result<(PrPayload, PathBuf), String> {
        let top = toplevel(dir)?;
        let repo = origin_repo(&top)?;
        let branch = match branch {
            Some(b) => b.to_string(),
            None => git(&top, &["symbolic-ref", "--short", "-q", "HEAD"])
                .map_err(|_| "HEAD is detached; pass --branch".to_string())?,
        };
        safe_rev(&branch)?;
        let base = match base {
            Some(b) => b.to_string(),
            None => remote_default_branch(&top)?,
        };
        safe_rev(&base)?;
        let head_ref = format!("refs/heads/{branch}");
        let head_sha = git(
            &top,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{head_ref}^{{commit}}"),
            ],
        )
        .map_err(|_| format!("branch {branch} does not exist locally"))?;
        let pushed = remote_ref(&top, &head_ref)?;
        if pushed.as_deref() != Some(head_sha.as_str()) {
            return Err(format!(
                "push the branch first: origin has {} for {branch}, local is {head_sha}",
                pushed.as_deref().unwrap_or("nothing")
            ));
        }
        let base_sha = remote_ref(&top, &format!("refs/heads/{base}"))?
            .ok_or_else(|| format!("origin has no branch {base}"))?;
        git(&top, &["cat-file", "-e", &format!("{base_sha}^{{commit}}")])
            .map_err(|_| format!("origin/{base} is not fetched locally; run git fetch origin"))?;
        let range = format!("{base_sha}..{head_sha}");
        let limit = format!("-n{}", COMMITS_MAX + 1);
        let log = git(&top, &["log", &limit, "--format=%H%x09%s", &range])?;
        let commits: Vec<Commit> = log
            .lines()
            .filter_map(|l| {
                let (sha, subject) = l.split_once('\t')?;
                Some(Commit {
                    sha: sha.to_string(),
                    subject: subject.to_string(),
                })
            })
            .collect();
        if commits.len() > COMMITS_MAX {
            return Err(format!(
                "the branch has more than {COMMITS_MAX} commits over {base}; refusing to truncate"
            ));
        }
        Ok((
            PrPayload {
                repo,
                branch,
                base,
                base_sha,
                head_sha,
                title: title.to_string(),
                body: body.to_string(),
                commits,
            },
            top,
        ))
    }

    pub fn tag_payload(
        dir: &Path,
        name: &str,
        target: Option<&str>,
    ) -> std::result::Result<(TagPayload, PathBuf), String> {
        let top = toplevel(dir)?;
        let repo = origin_repo(&top)?;
        let target = target.unwrap_or("HEAD");
        safe_rev(target)?;
        let sha = git(
            &top,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{target}^{{commit}}"),
            ],
        )
        .map_err(|_| format!("{target} is not a commit"))?;
        let remote = git(&top, &["branch", "-r", "--contains", &sha])?;
        if remote.trim().is_empty() {
            return Err(format!(
                "push first: {sha} is not on any remote branch of the local clone"
            ));
        }
        let tag_ref = format!("refs/tags/{name}");
        if !name.is_empty() && !name.starts_with('-') && remote_ref(&top, &tag_ref)?.is_some() {
            return Err(format!("tag {name} already exists on origin"));
        }
        Ok((
            TagPayload {
                repo,
                name: name.to_string(),
                target_sha: sha,
            },
            top,
        ))
    }

    pub fn origin_of(server: &str) -> String {
        server.trim_end_matches('/').to_string()
    }

    fn cloud_client(ctx: &Ctx) -> Result<&tmgr_core::api::Client> {
        let client = ctx.cloud_client()?;
        if ctx.local_tag()?.is_some() {
            return Err(Error::usage(
                "actions work on cloud workspaces only, not on a local workspace",
            ));
        }
        Ok(client)
    }

    fn user_id(client: &tmgr_core::api::Client) -> Result<u64> {
        client
            .current_user_id()?
            .and_then(|id| u64::try_from(id).ok())
            .filter(|id| *id > 0)
            .ok_or_else(|| Error::usage("cannot tell which account you are signed in as"))
    }

    fn workspace_id(ctx: &Ctx, client: &tmgr_core::api::Client, wanted: &str) -> Result<u64> {
        if let Ok(id) = wanted.parse::<u64>() {
            return Ok(id);
        }
        ctx.workspaces(client)?
            .iter()
            .find(|w| w.code.eq_ignore_ascii_case(wanted))
            .and_then(|w| u64::try_from(w.id).ok())
            .ok_or_else(|| Error::not_found(format!("workspace {wanted} not found")))
    }

    fn repo_dir(arg: Option<&str>) -> Result<PathBuf> {
        match arg {
            Some(d) => Ok(PathBuf::from(d)),
            None => std::env::current_dir().map_err(|e| Error::usage(e.to_string())),
        }
    }

    fn action_title(action: &ActionIn) -> String {
        match action {
            ActionIn::Comment(p) => format!("Comment on {}", p.task_label),
            ActionIn::Task(p) => format!("New task: {}", p.title),
            ActionIn::Pr(p) => format!("PR: {}", p.title),
            ActionIn::Tag(p) => format!("Tag {} on {}", p.name, p.repo),
        }
    }

    fn card_kind(action: &ActionIn) -> &'static str {
        match action {
            ActionIn::Pr(_) => "review",
            ActionIn::Tag(_) => "release",
            _ => "decision",
        }
    }

    pub fn action_card(
        action: ActionIn,
        diffs: Vec<DiffSpec>,
        src: tmgr_ipc::decisions::Source,
        reply_via: &str,
    ) -> std::result::Result<CardIn, String> {
        action.validate()?;
        let mut diffs = diffs.into_iter();
        let diff = diffs.next();
        Ok(CardIn {
            id: Some(tmgr_ipc::decisions::new_card_id()),
            source: src,
            kind: card_kind(&action).into(),
            blocking: true,
            title: crate::cmd::decisions::truncate(&action_title(&action), MAX_TITLE),
            diff,
            diffs: diffs.collect(),
            reply_via: reply_via.into(),
            action: Some(action),
            ..CardIn::default()
        })
    }

    pub fn prepare(
        ctx: &Ctx,
        proposal: &Proposal,
        src: tmgr_ipc::decisions::Source,
        reply_via: &str,
    ) -> Result<CardIn> {
        let (action, diffs) = match proposal {
            Proposal::Comment { task, text } => {
                let client = cloud_client(ctx)?;
                let found = crate::cmd::shared::resolve_task(ctx, client, task)?;
                let label = crate::cmd::decisions::truncate(
                    &tmgr_core::text::sanitize_line(&found.title),
                    LABEL_CHARS,
                );
                (
                    ActionIn::Comment(CommentPayload {
                        origin: origin_of(&ctx.server()?),
                        user_id: user_id(client)?,
                        task_id: found.id,
                        task_label: label,
                        text: text.clone(),
                    }),
                    Vec::new(),
                )
            }
            Proposal::Task {
                workspace,
                title,
                description,
                category,
                status,
            } => {
                let client = cloud_client(ctx)?;
                (
                    ActionIn::Task(TaskPayload {
                        origin: origin_of(&ctx.server()?),
                        user_id: user_id(client)?,
                        workspace_id: workspace_id(ctx, client, workspace)?,
                        category_id: *category,
                        status_id: *status,
                        title: title.clone(),
                        description: description.clone(),
                    }),
                    Vec::new(),
                )
            }
            Proposal::Pr {
                repo_dir: dir,
                branch,
                base,
                title,
                body,
            } => {
                let (p, top) = pr_payload(
                    &repo_dir(dir.as_deref())?,
                    branch.as_deref(),
                    base.as_deref(),
                    title,
                    body,
                )
                .map_err(Error::usage)?;
                let diff = DiffSpec {
                    repo: top.display().to_string(),
                    base: Some(p.base_sha.clone()),
                    head: Some(p.head_sha.clone()),
                };
                (ActionIn::Pr(p), vec![diff])
            }
            Proposal::Tag {
                repo_dir: dir,
                name,
                target,
            } => {
                let (p, _) = tag_payload(&repo_dir(dir.as_deref())?, name, target.as_deref())
                    .map_err(Error::usage)?;
                (ActionIn::Tag(p), Vec::new())
            }
        };
        action_card(action, diffs, src, reply_via).map_err(Error::usage)
    }

    fn proposal_of(command: &ProposeCmd) -> Result<(Proposal, &ProposeWait)> {
        Ok(match command {
            ProposeCmd::Comment(a) => {
                let text = match (&a.text, &a.text_file) {
                    (Some(t), _) => t.clone(),
                    (None, Some(f)) => read_text(f)?,
                    (None, None) => return Err(Error::usage("--text or --text-file is required")),
                };
                (
                    Proposal::Comment {
                        task: a.task.clone(),
                        text,
                    },
                    &a.wait,
                )
            }
            ProposeCmd::Task(a) => (
                Proposal::Task {
                    workspace: a.workspace.clone(),
                    title: a.title.clone(),
                    description: a.description.clone(),
                    category: a.category,
                    status: a.status,
                },
                &a.wait,
            ),
            ProposeCmd::Pr(a) => (
                Proposal::Pr {
                    repo_dir: a.repo_dir.clone(),
                    branch: a.branch.clone(),
                    base: a.base.clone(),
                    title: a.title.clone(),
                    body: a.body.clone(),
                },
                &a.wait,
            ),
            ProposeCmd::Tag(a) => (
                Proposal::Tag {
                    repo_dir: a.repo_dir.clone(),
                    name: a.name.clone(),
                    target: a.target.clone(),
                },
                &a.wait,
            ),
        })
    }

    fn read_text(file: &str) -> Result<String> {
        let mut text = String::new();
        let res = if file == "-" {
            std::io::stdin().read_to_string(&mut text)
        } else {
            std::fs::File::open(file).and_then(|mut f| f.read_to_string(&mut text))
        };
        res.map_err(|e| Error::usage(format!("cannot read {file}: {e}")))?;
        Ok(text)
    }

    fn agent() -> &'static str {
        if std::env::var_os("CLAUDECODE").is_some() {
            "claude-code"
        } else if std::env::vars_os().any(|(k, _)| k.to_string_lossy().starts_with("CODEX_SANDBOX"))
        {
            "codex"
        } else {
            "other"
        }
    }

    fn send_error(e: std::io::Error) -> Error {
        if e.kind() == std::io::ErrorKind::NotConnected {
            Error::new(Kind::Offline, ACTIONS_DOWN)
        } else if is_refusal(&e) {
            let m = e.to_string();
            let err = Error::usage(format!("the Decisions inbox refused the card: {m}"));
            if m == ACTIONS_OLD {
                err.hint(ACTIONS_OLD)
            } else {
                err
            }
        } else {
            Error::internal(format!("the card could not be sent: {e}"))
        }
    }

    pub fn outcome_of(e: &Event) -> (String, i32) {
        if e.dismissed {
            return ("the owner dismissed the card".into(), EXIT_DISMISSED);
        }
        let reason = || {
            e.note
                .clone()
                .or_else(|| e.free_text.clone())
                .filter(|r| !r.trim().is_empty())
                .unwrap_or_else(|| "no reason given".into())
        };
        match e.choice.as_deref() {
            Some("reject") => (format!("rejected: {}", reason()), EXIT_REJECTED),
            Some("approve") => {
                let edited = e.approved.as_ref().is_some_and(|a| a.edited);
                let tail = if edited { " (edited by the owner)" } else { "" };
                match &e.result {
                    Some(r) if r.ok => {
                        let what = r
                            .url
                            .clone()
                            .or_else(|| r.sha.clone())
                            .unwrap_or_else(|| "done".into());
                        (format!("approved{tail}: {what}"), 0)
                    }
                    Some(r) => {
                        let mut m = format!(
                            "approved{tail} but failed: {}",
                            r.error.as_deref().unwrap_or("unknown error")
                        );
                        if let Some(u) = &r.url {
                            m.push_str(&format!(" ({u})"));
                        }
                        (m, EXIT_FAILED)
                    }
                    None => (
                        format!("approved{tail} but the desktop reported no result"),
                        EXIT_FAILED,
                    ),
                }
            }
            _ => (event_text(e), EXIT_DISMISSED),
        }
    }

    pub fn run(ctx: &Ctx, command: &ProposeCmd) -> Result<Output> {
        let (proposal, wait) = proposal_of(command)?;
        let pid = (!wait.no_wait).then(parent_pid);
        let card = prepare(ctx, &proposal, source(agent(), pid), "cli")?;
        let response = send_action_card(card, None).map_err(send_error)?;
        let id = response.card_id.unwrap_or_default();
        if wait.no_wait {
            return Ok(Output::new(
                json!({"card_id": id, "status": "created"}),
                id.clone(),
            ));
        }
        let deadline = Instant::now() + wait.timeout.map_or(FOREVER, Duration::from_secs);
        let mut since = 0;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let unreachable = match wait_loop(&id, since, left, false, None, &mut || {}) {
                Waited::Event(_, e) => {
                    since = e.seq;
                    if e.is_final() {
                        let (text, code) = outcome_of(&e);
                        return Ok(Output::new(&*e, text).exit(code));
                    }
                    if !ctx.quiet {
                        eprintln!("{}", event_text(&e));
                    }
                    continue;
                }
                Waited::Pending { unreachable } => unreachable,
                Waited::Refused(m) => {
                    return Err(Error::internal(format!("the Decisions inbox: {m}")))
                }
                Waited::Cancelled => unreachable!("no cancel handle is passed"),
            };
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(Output::new(
                    crate::cmd::decisions::pending_json(&id, unreachable),
                    id.clone(),
                )
                .exit(EXIT_NO_ANSWER));
            }
            std::thread::sleep(if unreachable { RETRY } else { RETRY / 10 }.min(left));
        }
    }

    const FOREVER: Duration = Duration::from_secs(10 * 365 * 86400);

    fn text_arg(a: &Value, key: &str) -> std::result::Result<Option<String>, String> {
        match a.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(format!("{key} must be a string")),
        }
    }

    fn need(a: &Value, key: &str) -> std::result::Result<String, String> {
        text_arg(a, key)?.ok_or_else(|| format!("{key} is required"))
    }

    fn number_arg(a: &Value, key: &str) -> std::result::Result<Option<u64>, String> {
        match a.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_u64()
                .map(Some)
                .ok_or_else(|| format!("{key} must be a positive integer")),
        }
    }

    fn scalar_text(a: &Value, key: &str) -> std::result::Result<String, String> {
        match a.get(key) {
            Some(Value::Number(n)) => Ok(n.to_string()),
            _ => need(a, key),
        }
    }

    pub fn proposal_from_args(a: &Value) -> std::result::Result<Proposal, String> {
        match need(a, "kind")?.as_str() {
            "comment" => Ok(Proposal::Comment {
                task: scalar_text(a, "task")?,
                text: need(a, "text")?,
            }),
            "task" => Ok(Proposal::Task {
                workspace: scalar_text(a, "workspace")?,
                title: need(a, "title")?,
                description: text_arg(a, "description")?.unwrap_or_default(),
                category: number_arg(a, "category_id")?,
                status: number_arg(a, "status_id")?,
            }),
            "pr" => Ok(Proposal::Pr {
                repo_dir: text_arg(a, "repo_dir")?,
                branch: text_arg(a, "branch")?,
                base: text_arg(a, "base")?,
                title: need(a, "title")?,
                body: text_arg(a, "body")?.unwrap_or_default(),
            }),
            "tag" => Ok(Proposal::Tag {
                repo_dir: text_arg(a, "repo_dir")?,
                name: need(a, "name")?,
                target: text_arg(a, "target")?,
            }),
            other => Err(format!(
                "kind must be comment, task, pr or tag, not {:?}",
                other.chars().take(20).collect::<String>()
            )),
        }
    }

    pub fn mcp_call(env: Option<&NotifyEnv>, args: &Value, meta_session: Option<&str>) -> Value {
        use crate::cmd::mcp::{arg_err, text_result};
        let proposal = match proposal_from_args(args) {
            Ok(p) => p,
            Err(m) => return arg_err(&m),
        };
        let ctx = match mcp_ctx(env) {
            Ok(c) => c,
            Err(e) => return text_result(format!("error: {}", e.message), true),
        };
        let src = source(
            crate::cmd::decisions_mcp::client_agent(),
            Some(parent_pid()),
        );
        let card = match prepare(&ctx, &proposal, src, "mcp") {
            Ok(c) => c,
            Err(e) => return text_result(format!("error: {}", e.message), true),
        };
        match send_action_card(card, meta_session) {
            Ok(r) => {
                let id = r.card_id.unwrap_or_default();
                text_result(
                    json!({"status": "proposed", "card_id": id, "note": "nothing runs unless the owner approves; call get_answer with this card_id"})
                        .to_string(),
                    false,
                )
            }
            Err(e) => text_result(format!("error: {}", send_error(e).message), true),
        }
    }

    fn mcp_ctx(env: Option<&NotifyEnv>) -> Result<Ctx> {
        use clap::Parser;
        let mut argv = vec!["tmgr".to_string()];
        if let Some(s) = env.and_then(|e| e.server_override.clone()) {
            argv.push("--server".into());
            argv.push(s);
        }
        let cli =
            crate::cli::Cli::try_parse_from(argv).map_err(|e| Error::internal(e.to_string()))?;
        Ctx::new(&cli)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::unix::github_repo;

    #[test]
    fn github_urls_are_reduced_to_owner_name() {
        for url in [
            "https://github.com/acme/widgets",
            "https://github.com/acme/widgets.git",
            "https://github.com/acme/widgets/",
            "git@github.com:acme/widgets.git",
            "ssh://git@github.com/acme/widgets.git",
        ] {
            assert_eq!(github_repo(url).unwrap(), "acme/widgets", "{url}");
        }
    }

    #[test]
    fn other_hosts_schemes_and_shapes_are_refused() {
        for url in [
            "https://gitlab.com/acme/widgets.git",
            "https://github.example.com/acme/widgets.git",
            "http://github.com/acme/widgets.git",
            "git://github.com/acme/widgets.git",
            "https://user:pw@github.com/acme/widgets.git",
            "https://github.com/acme",
            "https://github.com/acme/widgets/extra",
            "/srv/git/widgets.git",
            "file:///srv/git/widgets.git",
            "",
        ] {
            assert!(github_repo(url).is_err(), "{url}");
        }
    }
}
