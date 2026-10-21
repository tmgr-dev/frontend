use crate::cli_agent::{
    AgentCommand, AgentFinishArgs, AgentStartArgs, AgentUpdateArgs, ProgressArgs,
};
use crate::ctx::{Ctx, RunClient};
use crate::gitx::{self, Git};
use crate::out::Output;
use crate::runfinish;
use serde_json::{json, Value};
use std::time::Duration;
use tmgr_core::agentwork::{self, Outcome, Progress, RunView, StartBody, Tests};
use tmgr_core::branch;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::refs::{parse_task_ref, TaskRef};
use tmgr_core::runstate::{self, clean_commit, merge_commits, valid_sha, Run};
use tmgr_core::text::{sanitize_line, truncate_chars};
use tmgr_core::view::task_ref;

const STATE_WAIT: Duration = Duration::from_secs(2);

pub fn run(ctx: &Ctx, cmd: &AgentCommand) -> Result<Output> {
    match cmd {
        AgentCommand::Start(a) => start(ctx, a),
        AgentCommand::Update(a) => update(ctx, a),
        AgentCommand::Finish(a) => finish(ctx, a),
        AgentCommand::Status { command } => super::agent_status::run(ctx, command),
    }
}

fn now_s() -> i64 {
    chrono::Utc::now().timestamp()
}

fn rfc3339(epoch: i64) -> String {
    chrono::DateTime::from_timestamp(epoch, 0)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn run_data(view: Option<&RunView>, task_id: Option<u64>, task_ref: Option<&str>) -> Value {
    json!({
        "run_id": view.map(|v| v.id),
        "task_id": view.and_then(|v| v.task_id).or(task_id),
        "task_ref": task_ref,
        "status": view.map(|v| v.status.clone()),
        "agent": view.and_then(|v| v.agent.clone()),
        "model": view.and_then(|v| v.model.clone()),
        "branch": view.and_then(|v| v.branch.clone()),
        "pr_url": view.and_then(|v| v.pr_url.clone()),
        "commits": view.map(|v| v.commits.clone()).unwrap_or_default(),
        "duration_s": view.and_then(|v| v.duration_s),
        "disabled": view.is_none(),
    })
}

fn disabled(ctx: &Ctx, task_id: Option<u64>, task_ref: Option<&str>) -> Output {
    ctx.warn(
        "agent_work_disabled",
        "Agent work is turned off in this workspace; nothing was recorded",
    );
    Output::new(
        run_data(None, task_id, task_ref),
        ctx.style.muted("agent work is disabled in this workspace"),
    )
}

fn save_state(ctx: &Ctx, f: impl FnOnce(&mut runstate::Runs)) {
    let Some(path) = runstate::runs_path() else {
        return;
    };
    if runstate::modify(&path, STATE_WAIT, f).is_err() {
        ctx.warn(
            "run_state_not_saved",
            "could not save the local run state; pass --run next time",
        );
    }
}

fn parse_commits(raw: &[String]) -> Result<Vec<String>> {
    raw.iter()
        .map(|c| {
            clean_commit(c).ok_or_else(|| {
                Error::usage("a commit must look like \"<sha> <subject>\"")
                    .hint("example: --commit \"a1b2c3d fix: retry\"")
            })
        })
        .collect()
}

fn tests_of(p: &ProgressArgs) -> Option<Tests> {
    (p.tests_passed.is_some() || p.tests_cmd.is_some()).then(|| Tests {
        passed: p.tests_passed,
        failed: p.tests_failed,
        command: p.tests_cmd.clone(),
    })
}

fn resolve_task(
    ctx: &Ctx,
    client: &tmgr_core::api::Client,
    reference: &str,
) -> Result<tmgr_core::models::ApiTask> {
    let tref = parse_task_ref(reference)?;
    let not_found = |e: Error| {
        if e.kind == Kind::NotFound {
            Error::not_found(format!("Task {} not found", sanitize_line(reference)))
                .hint("check --ws, or tmgr task list")
        } else {
            e
        }
    };
    Ok(match &tref {
        TaskRef::Key(key) => super::shared::task_by_key(ctx, client, key).map_err(not_found)?,
        TaskRef::Id(id) => client.task_by_id(*id).map_err(not_found)?,
    })
}

fn sent_commits(
    ctx: &Ctx,
    client: &tmgr_core::api::Client,
    run_id: u64,
    known: Option<&Run>,
    p: &ProgressArgs,
) -> Result<Vec<String>> {
    if let Some(r) = known {
        return Ok(r.commits.clone());
    }
    if p.commits.is_empty() {
        return Ok(Vec::new());
    }
    let reference = p.task.as_deref().ok_or_else(|| {
        Error::usage("this run is not in the local state, so its commits cannot be merged")
            .hint("pass --task REF")
    })?;
    let task = resolve_task(ctx, client, reference)?;
    agentwork::run_commits(client, task.id, run_id)
}

fn start(ctx: &Ctx, args: &AgentStartArgs) -> Result<Output> {
    let client = ctx.client()?;
    let task = resolve_task(ctx, client, &args.reference)?;
    let key = task_ref(&task);
    let git = gitx::cwd_repo();
    let branch = match &args.branch {
        Some(b) => {
            branch::validate_branch_name(b)?;
            Some(b.clone())
        }
        None => git.as_ref().and_then(Git::current_branch),
    };
    let start_sha = git
        .as_ref()
        .and_then(|g| g.query(&["rev-parse", "HEAD"]))
        .filter(|s| valid_sha(s));
    let body = StartBody {
        agent: args.agent.trim(),
        model: args.model.as_deref(),
        session_id: None,
        branch: branch.as_deref(),
    };
    if body.agent.is_empty() {
        return Err(Error::usage("--agent is empty"));
    }
    match agentwork::start(client, task.id, &body)? {
        Outcome::Disabled => Ok(disabled(ctx, Some(task.id), Some(&key))),
        Outcome::Run(view) => {
            let entry = Run {
                run_id: view.id,
                task_id: task.id,
                task_key: key.clone(),
                branch: view.branch.clone().or(branch),
                repo: git.as_ref().map(|g| g.dir().display().to_string()),
                start_sha,
                agent: view.agent.clone().unwrap_or_else(|| args.agent.clone()),
                started_at: rfc3339(now_s()),
                commits: Vec::new(),
                last_sent: now_s(),
                workspace: ctx.local_tag()?,
            };
            save_state(ctx, |r| r.upsert(entry));
            let human = format!(
                "{} run {} started on {} ({})",
                ctx.style.good("\u{2713}"),
                view.id,
                key,
                view.agent.as_deref().unwrap_or("agent")
            );
            Ok(Output::new(
                run_data(Some(&view), Some(task.id), Some(&key)),
                human,
            ))
        }
    }
}

fn find_run(ctx: &Ctx, run: Option<u64>) -> Result<(u64, Option<Run>)> {
    let state = runstate::load();
    if let Some(id) = run {
        return Ok((id, state.find_run(id, ctx.local_tag()?.as_deref()).cloned()));
    }
    let git = gitx::cwd_repo();
    let branch = git.as_ref().and_then(Git::current_branch);
    let repo = git.as_ref().map(|g| g.dir().display().to_string());
    let key = branch.as_deref().and_then(branch::key_from_branch);
    let found = branch
        .as_deref()
        .and_then(|b| state.find_branch(repo.as_deref(), b, key.as_deref()));
    match found {
        Some(r) => Ok((r.run_id, Some(r.clone()))),
        None => Err(Error::usage("no active run for this branch")
            .hint("tmgr agent start <ref>, or pass --run ID")),
    }
}

fn progress_of(
    p: &ProgressArgs,
    branch: Option<String>,
    sent: &[String],
) -> Result<(Progress, Option<Vec<String>>)> {
    let new = parse_commits(&p.commits)?;
    let merged = (!new.is_empty()).then(|| merge_commits(sent, &new));
    if let Some(b) = &branch {
        branch::validate_branch_name(b)?;
    }
    let progress = Progress {
        branch,
        summary: p.summary.clone(),
        pr_url: p.pr.clone(),
        commits: merged.clone(),
        tests: tests_of(p),
        ..Default::default()
    };
    Ok((progress, merged))
}

fn run_client<'a>(ctx: &'a Ctx, known: Option<&Run>) -> Result<RunClient<'a>> {
    match known {
        Some(run) => ctx.client_for_run(run.workspace.as_deref()),
        None => ctx.client().map(RunClient::Shared),
    }
}

fn update(ctx: &Ctx, args: &AgentUpdateArgs) -> Result<Output> {
    let (run_id, known) = find_run(ctx, args.run)?;
    let client = &*run_client(ctx, known.as_ref())?;
    let workspace = known.as_ref().and_then(|r| r.workspace.clone());
    let sent = sent_commits(ctx, client, run_id, known.as_ref(), &args.progress)?;
    let (progress, merged) = progress_of(&args.progress, args.branch.clone(), &sent)?;
    let key = known.as_ref().map(|r| r.task_key.clone());
    match agentwork::update(client, run_id, &progress, true)? {
        Outcome::Disabled => Ok(disabled(
            ctx,
            known.as_ref().map(|r| r.task_id),
            key.as_deref(),
        )),
        Outcome::Run(view) => {
            if known.is_some() {
                save_state(ctx, |r| {
                    if let Some(e) = r
                        .runs
                        .iter_mut()
                        .find(|e| e.run_id == run_id && e.workspace == workspace)
                    {
                        if let Some(c) = merged {
                            e.commits = c;
                        }
                        e.last_sent = now_s();
                    }
                });
            }
            let human = format!(
                "{} run {} updated ({} commits)",
                ctx.style.good("\u{2713}"),
                view.id,
                view.commits.len()
            );
            Ok(Output::new(
                run_data(
                    Some(&view),
                    known.as_ref().map(|r| r.task_id),
                    key.as_deref(),
                ),
                human,
            ))
        }
    }
}

fn after_finish(client: &tmgr_core::api::Client, view: &RunView, known: Option<&Run>) {
    if let Some(task_id) = view.task_id.or(known.map(|r| r.task_id)) {
        runfinish::spawn_detached(&runfinish::auto_upload_args(task_id));
    }
    let Some(git) = gitx::cwd_repo() else { return };
    let Some(started) = view
        .started_at
        .or_else(|| known.and_then(|r| runfinish::parse_ts(&r.started_at)))
    else {
        return;
    };
    let ended = view.ended_at.unwrap_or_else(now_s);
    let mut listed = view.commits.clone();
    listed.extend(known.map(|r| r.commits.clone()).unwrap_or_default());
    let query = |args: &[&str]| git.query(args);
    let commits = runfinish::collect(&query, started, ended, &listed);
    let _ = agentwork::add_git_commits(client, view.id, &commits);
}

fn finish(ctx: &Ctx, args: &AgentFinishArgs) -> Result<Output> {
    let (run_id, known) = find_run(ctx, args.run)?;
    let client = &*run_client(ctx, known.as_ref())?;
    let workspace = known.as_ref().and_then(|r| r.workspace.clone());
    let sent = sent_commits(ctx, client, run_id, known.as_ref(), &args.progress)?;
    let (progress, _) = progress_of(&args.progress, None, &sent)?;
    let key = known.as_ref().map(|r| r.task_key.clone());
    let task_id = known.as_ref().map(|r| r.task_id);
    let result = agentwork::finish(client, run_id, args.status.as_str(), &progress);
    let stale = matches!(&result, Err(e) if matches!(e.kind, Kind::NotFound | Kind::Conflict));
    if known.is_some() && (stale || result.is_ok()) {
        save_state(ctx, |r| r.remove(run_id, workspace.as_deref()));
    }
    match result? {
        Outcome::Disabled => Ok(disabled(ctx, task_id, key.as_deref())),
        Outcome::Run(view) => {
            after_finish(client, &view, known.as_ref());
            let summary = truncate_chars(&sanitize_line(&view.status), 20).0;
            let human = format!("{} run {} {}", ctx.style.good("\u{2713}"), view.id, summary);
            Ok(Output::new(
                run_data(Some(&view), task_id, key.as_deref()),
                human,
            ))
        }
    }
}
