use crate::cli::{DoneArgs, StartArgs};
use crate::ctx::Ctx;
use crate::gitx::{self, Git};
use crate::interact;
use crate::out::Output;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tmgr_core::active::{self, ActiveState, ActiveTask};
use tmgr_core::api::Client;
use tmgr_core::branch;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::models::{ApiTask, Status};
use tmgr_core::refs::{parse_task_ref, TaskRef};
use tmgr_core::text::{format_duration, sanitize, sanitize_line, truncate_chars};
use tmgr_core::view::{display_title, task_ref, task_url, StatusView};

const GH_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_COMMENT_CHARS: usize = 2000;
const MAX_PR_URL: usize = 500;
const TRUNKS: [&str; 4] = ["main", "master", "develop", "trunk"];

struct Resolved {
    task: ApiTask,
    key: String,
    title: String,
    ws_code: String,
    url: String,
    statuses: Vec<Status>,
    status: StatusView,
}

fn now_s() -> i64 {
    chrono::Utc::now().timestamp()
}

fn rfc3339(epoch: i64) -> String {
    chrono::DateTime::from_timestamp(epoch, 0)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn local_clock(epoch: i64) -> String {
    chrono::DateTime::from_timestamp(epoch, 0)
        .unwrap_or_default()
        .with_timezone(&chrono::Local)
        .format("%H:%M")
        .to_string()
}

fn resolve(ctx: &Ctx, client: &Client, reference: &str) -> Result<Resolved> {
    let tref = parse_task_ref(reference)?;
    let not_found = |e: Error| {
        if e.kind == Kind::NotFound {
            Error::not_found(format!("Task {} not found", sanitize_line(reference)))
                .hint("check --ws, or tmgr task list")
        } else {
            e
        }
    };
    let task = match &tref {
        TaskRef::Key(key) => super::shared::task_by_key(ctx, client, key).map_err(not_found)?,
        TaskRef::Id(id) => client.task_by_id(*id).map_err(not_found)?,
    };
    let ws_id = match task.workspace_id {
        Some(id) => id,
        None => ctx.workspace(client)?.id,
    };
    let ws_code = ctx
        .workspaces(client)?
        .iter()
        .find(|w| w.id == ws_id)
        .map(|w| sanitize_line(&w.code))
        .ok_or_else(|| Error::not_found("the task's workspace is not available"))?;
    let statuses = client.statuses(ws_id)?;
    let status = StatusView::resolve(&task, &statuses);
    let url = task_url(&ctx.web_base()?, &ws_code, task.id);
    Ok(Resolved {
        key: task_ref(&task),
        title: display_title(&task.title),
        ws_code,
        url,
        statuses,
        status,
        task,
    })
}

fn category_code(r: &Resolved) -> Option<String> {
    r.key
        .split_once('-')
        .map(|(c, _)| c.to_string())
        .or_else(|| r.task.category.as_ref().and_then(|c| c.code.clone()))
}

fn find_repo(ctx: &Ctx, code: Option<&str>) -> Option<Git> {
    if let Some(path) = code.and_then(|c| ctx.cfg.projects.get(c)) {
        let git = Git::at(PathBuf::from(path));
        match git.toplevel() {
            Some(top) => return Some(Git::at(top)),
            None => ctx.warn(
                "project_map_stale",
                format!(
                    "the repository mapped to {} is not available; using the current directory",
                    code.unwrap_or("")
                ),
            ),
        }
    }
    gitx::cwd_repo()
}

fn repo_at(path: &str) -> Option<Git> {
    let top = Git::at(PathBuf::from(path)).toplevel()?;
    Some(Git::at(top))
}

fn named_status<'a>(statuses: &'a [Status], name: &str) -> Result<&'a Status> {
    statuses
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(name.trim()))
        .ok_or_else(|| {
            Error::not_found(format!("status {} not found", sanitize_line(name)))
                .hint("names come from the workspace's statuses")
        })
}

fn view_of(s: &Status) -> StatusView {
    StatusView::from_status(s)
}

fn task_json(r: &Resolved, status: &StatusView) -> Value {
    json!({
        "id": r.task.id,
        "ref": r.key,
        "title": r.title,
        "url": r.url,
        "status": status,
    })
}

struct BranchPlan {
    name: String,
    action: &'static str,
    base: Option<String>,
}

fn plan_branch(args: &StartArgs, ctx: &Ctx, r: &Resolved, git: &Git) -> Result<Option<BranchPlan>> {
    let name = match &args.branch {
        Some(b) => b.clone(),
        None => {
            if r.key.starts_with('#') {
                ctx.warn("no_branch", "the task has no key, so no branch was created");
                return Ok(None);
            }
            branch::render_template(ctx.cfg.branch_template(), &r.key, &branch::slug(&r.title))?
        }
    };
    branch::validate_branch_name(&name)?;
    if !git.valid_branch_ref(&name) {
        return Err(Error::usage("git does not accept this branch name"));
    }
    if git.current_branch().as_deref() == Some(name.as_str()) {
        return Ok(Some(BranchPlan {
            name,
            action: "current",
            base: None,
        }));
    }
    if git.branch_exists(&name) {
        return Ok(Some(BranchPlan {
            name,
            action: "switched",
            base: None,
        }));
    }
    let base = match &args.from {
        Some(b) => {
            branch::validate_base(b)?;
            if !git.commit_exists(b) {
                return Err(Error::not_found("the --from base does not exist"));
            }
            Some(b.clone())
        }
        None => git.current_branch(),
    };
    Ok(Some(BranchPlan {
        name,
        action: "created",
        base,
    }))
}

pub fn start(ctx: &Ctx, args: &StartArgs) -> Result<Output> {
    run_start(ctx, args, false)
}

pub fn switch(ctx: &Ctx, args: &StartArgs) -> Result<Output> {
    run_start(ctx, args, true)
}

fn run_start(ctx: &Ctx, args: &StartArgs, replace_running: bool) -> Result<Output> {
    parse_task_ref(&args.reference)?;
    if let Some(b) = &args.branch {
        branch::validate_branch_name(b)?;
    }
    if let Some(f) = &args.from {
        branch::validate_base(f)?;
    }
    let client = ctx.client()?;
    let r = resolve(ctx, client, &args.reference)?;

    let git = if args.no_branch {
        None
    } else {
        find_repo(ctx, category_code(&r).as_deref())
    };
    let plan = match &git {
        Some(g) => plan_branch(args, ctx, &r, g)?,
        None => None,
    };
    let needs_stash = match (&git, &plan) {
        (Some(g), Some(p)) if p.action != "current" => {
            let dirty = g.is_dirty()?;
            if dirty && !args.stash {
                return Err(Error::conflict("the working tree has uncommitted changes")
                    .hint("commit them, or pass --stash"));
            }
            dirty
        }
        _ => false,
    };

    let target_status = match &args.status {
        Some(name) => Some(named_status(&r.statuses, name)?.clone()),
        None if r.status.kind == "default" => {
            r.statuses.iter().find(|s| s.kind == "active").cloned()
        }
        None => None,
    };
    if args.status.is_none() && r.status.kind == "default" && target_status.is_none() {
        ctx.warn(
            "no_active_status",
            "the workspace has no active-type status to move the task to",
        );
    }
    let target_status = target_status.filter(|s| Some(s.id) != r.status.id);

    let others: Vec<(u64, String)> = if args.no_timer {
        Vec::new()
    } else {
        match ctx.soft(
            client.running_tasks(),
            "running_timers_unavailable",
            "the list of running timers",
        )? {
            Some(list) => list
                .iter()
                .filter(|t| t.id != r.task.id)
                .map(|t| (t.id, task_ref(t)))
                .collect(),
            None => active::load()
                .filter(|a| a.timer_running() && a.task.id != r.task.id)
                .map(|a| vec![(a.task.id, a.task.key)])
                .unwrap_or_default(),
        }
    };
    if !others.is_empty() && !replace_running {
        let names: Vec<&str> = others.iter().map(|(_, k)| k.as_str()).collect();
        interact::confirm(
            &format!("A timer is running on {}. Stop it?", names.join(", ")),
            true,
            ctx.yes,
        )?;
    }

    if let (Some(g), Some(p)) = (&git, &plan) {
        if needs_stash {
            g.stash(&format!("tmgr: before {}", r.key))?;
        }
        match p.action {
            "created" => g.create_branch(&p.name, p.base.as_deref())?,
            "switched" => g.switch(&p.name)?,
            _ => {}
        }
    }

    let mut stopped_all = Vec::new();
    for (id, key) in &others {
        match client.stop_countdown(*id) {
            Ok(_) => {}
            Err(e) if e.kind == Kind::NotFound => ctx.warn(
                "previous_timer_gone",
                format!("the task {key} no longer exists"),
            ),
            Err(e) => return Err(e),
        }
        stopped_all.push(json!({"id": id, "ref": key}));
    }
    let stopped_previous = stopped_all.first().cloned();

    let mut status_change = None;
    let mut shown_status = r.status.clone();
    if let Some(to) = &target_status {
        client.set_task_status(r.task.id, to.id)?;
        let to_view = view_of(to);
        status_change = Some(json!({"from": r.status, "to": to_view}));
        shown_status = to_view;
    }

    let mut started_at = None;
    let mut already_running = false;
    if !args.no_timer {
        let server_running = r.task.start_time.filter(|t| *t > 0);
        let epoch = match server_running {
            Some(t) => {
                already_running = true;
                t
            }
            None => client
                .start_countdown(r.task.id)?
                .start_time
                .filter(|t| *t > 0)
                .unwrap_or_else(now_s),
        };
        started_at = Some(epoch);
    }

    let branch_name = match (&plan, &git) {
        (Some(p), _) => Some(p.name.clone()),
        (None, Some(g)) => g.current_branch(),
        _ => None,
    };
    let mut state = ActiveState::new(ActiveTask {
        id: r.task.id,
        key: r.key.clone(),
        title: r.title.clone(),
        url: Some(r.url.clone()),
    });
    state.workspace = Some(r.ws_code.clone());
    state.timer_started_at = started_at.map(rfc3339);
    state.branch = branch_name;
    state.repo = git.as_ref().map(|g| g.dir().display().to_string());
    if active::save(&state).is_err() {
        ctx.warn(
            "active_state_not_saved",
            "could not write the local state file; the prompt will not show this task",
        );
    }

    let s = &ctx.style;
    let mut lines = vec![format!(
        "{} {}  {} {}",
        s.good("\u{2713}"),
        s.muted("task  "),
        s.accent(&r.key),
        r.title
    )];
    if let Some(change) = &target_status {
        lines.push(format!(
            "{} {}  {} \u{2192} {}",
            s.good("\u{2713}"),
            s.muted("status"),
            r.status.name,
            s.good(&sanitize_line(&change.name))
        ));
    }
    if let Some(p) = &plan {
        let what = match p.action {
            "created" => format!(
                "created {} from {}",
                s.accent(&p.name),
                p.base
                    .as_deref()
                    .map_or_else(|| "HEAD".into(), sanitize_line)
            ),
            "switched" => format!("switched to {}", s.accent(&p.name)),
            _ => format!("already on {}", s.accent(&p.name)),
        };
        lines.push(format!(
            "{} {}  {what}",
            s.good("\u{2713}"),
            s.muted("branch")
        ));
    }
    if needs_stash {
        lines.push(format!(
            "{} {}  uncommitted changes stashed",
            s.good("\u{2713}"),
            s.muted("stash ")
        ));
    }
    for prev in &stopped_all {
        lines.push(format!(
            "{} {}  stopped {}",
            s.good("\u{2713}"),
            s.muted("timer "),
            prev["ref"].as_str().unwrap_or("")
        ));
    }
    if let Some(t) = started_at {
        let verb = if already_running {
            "already running since"
        } else {
            "started at"
        };
        lines.push(format!(
            "{} {}  {} {verb} {}",
            s.good("\u{2713}"),
            s.muted("timer "),
            s.accent("\u{25cf}"),
            local_clock(t)
        ));
    }
    let data = json!({
        "task": task_json(&r, &shown_status),
        "status_change": status_change,
        "branch": plan.as_ref().map(|p| json!({"name": p.name, "action": p.action, "base": p.base})),
        "stash": needs_stash,
        "timer": {
            "running": started_at.is_some(),
            "started_at": started_at.map(rfc3339),
            "already_running": already_running,
            "stopped_previous": stopped_previous,
            "stopped_all": stopped_all,
        },
        "repo": git.as_ref().map(gitx::repo_name),
    });
    let human = if ctx.quiet {
        r.key.clone()
    } else {
        lines.join("\n")
    };
    Ok(Output::new(data, human))
}

pub fn stop(ctx: &Ctx) -> Result<Output> {
    let Some(mut state) = active::load() else {
        return Ok(Output::new(
            json!({"task": null, "stopped": false, "elapsed_s": null, "tracked_s": null}),
            ctx.style
                .muted("No active task. Start one: tmgr start TM-512"),
        ));
    };
    let task_json_view = json!({
        "id": state.task.id,
        "ref": state.task.key,
        "title": state.task.title,
        "url": state.task.url,
    });
    let Some(elapsed) = state.elapsed_s(now_s()) else {
        return Ok(Output::new(
            json!({"task": task_json_view, "stopped": false, "elapsed_s": null, "tracked_s": null}),
            ctx.style.muted(&format!(
                "The timer on {} is already stopped",
                state.task.key
            )),
        ));
    };
    let client = ctx.client()?;
    let tracked = match client.stop_countdown(state.task.id) {
        Ok(t) => t.common_time,
        Err(e) if e.kind == Kind::NotFound => {
            ctx.warn("task_gone", "the task no longer exists on the server");
            None
        }
        Err(e) => return Err(e),
    };
    state.timer_started_at = None;
    if active::save(&state).is_err() {
        ctx.warn(
            "active_state_not_saved",
            "could not update the local state file",
        );
    }
    let s = &ctx.style;
    let human = format!(
        "{} {}  stopped {} on {}",
        s.good("\u{2713}"),
        s.muted("timer"),
        s.bold(&format_duration(elapsed)),
        s.accent(&state.task.key)
    );
    Ok(Output::new(
        json!({"task": task_json_view, "stopped": true, "elapsed_s": elapsed, "tracked_s": tracked}),
        human,
    ))
}

fn clean_pr_url(raw: &str) -> Option<String> {
    let url = raw.trim();
    let ok = url.starts_with("https://")
        && url.len() <= MAX_PR_URL
        && !url.chars().any(|c| c.is_control() || c.is_whitespace());
    ok.then(|| url.to_string())
}

fn find_pr(git: &Git) -> (Option<String>, Option<&'static str>) {
    if let Ok(out) = gitx::run_capture(
        "gh",
        Some(git.dir()),
        &["pr", "view", "--json", "url"],
        GH_TIMEOUT,
    ) {
        if out.ok {
            if let Some(url) = serde_json::from_str::<Value>(&out.stdout)
                .ok()
                .and_then(|v| v.get("url").and_then(Value::as_str).and_then(clean_pr_url))
            {
                return (Some(url), Some("gh"));
            }
        }
    }
    let compare = git
        .origin_url()
        .and_then(|u| gitx::parse_remote(&u))
        .zip(git.current_branch())
        .filter(|(_, b)| !TRUNKS.contains(&b.as_str()))
        .and_then(|(remote, b)| gitx::compare_url(&remote, &b));
    match compare {
        Some(url) => (Some(url), Some("compare")),
        None => (None, None),
    }
}

pub fn done(ctx: &Ctx, args: &DoneArgs) -> Result<Output> {
    let active_state = active::load();
    let reference = match (&args.reference, &active_state) {
        (Some(r), _) => r.clone(),
        (None, Some(a)) => a.task.key.clone(),
        (None, None) => {
            return Err(
                Error::usage("no active task and no task reference given").hint("tmgr done TM-512")
            )
        }
    };
    let client = ctx.client()?;
    let r = resolve(ctx, client, &reference)?;
    let same_as_active = active_state.as_ref().filter(|a| a.task.id == r.task.id);

    let git = match same_as_active
        .and_then(|a| a.repo.as_deref())
        .and_then(repo_at)
    {
        Some(g) => Some(g),
        None => find_repo(ctx, category_code(&r).as_deref()),
    };
    if let Some(g) = &git {
        if !args.force && g.is_dirty()? {
            return Err(Error::conflict("the working tree has uncommitted changes")
                .hint("commit them, or pass --force"));
        }
    }

    let target = match &args.status {
        Some(name) => Some(named_status(&r.statuses, name)?.clone()),
        None => r
            .statuses
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(ctx.cfg.review_status()))
            .or_else(|| r.statuses.iter().find(|s| s.kind == "completed"))
            .cloned(),
    };
    if target.is_none() {
        ctx.warn(
            "no_review_status",
            "no review or completed status found; the status was not changed",
        );
    }

    let server_started = r.task.start_time.filter(|t| *t > 0);
    let running_since = server_started.or_else(|| same_as_active.and_then(|a| a.started_epoch()));
    let mut elapsed = None;
    let mut timer_stopped = false;
    if let Some(since) = running_since {
        match client.stop_countdown(r.task.id) {
            Ok(_) => {
                timer_stopped = true;
                elapsed = Some((now_s() - since).max(0));
            }
            Err(e) if e.kind == Kind::NotFound => {
                ctx.warn("task_gone", "the task no longer exists")
            }
            Err(e) => return Err(e),
        }
    }

    let mut status_change = None;
    let mut shown_status = r.status.clone();
    if let Some(to) = target.filter(|s| Some(s.id) != r.status.id) {
        client.set_task_status(r.task.id, to.id)?;
        let to_view = view_of(&to);
        status_change = Some(json!({"from": r.status, "to": to_view}));
        shown_status = to_view;
    }

    if same_as_active.is_some() && active::clear().is_err() {
        ctx.warn(
            "active_state_not_saved",
            "could not remove the local state file",
        );
    }

    let branch_name = git.as_ref().and_then(Git::current_branch);
    let (pr_url, pr_source) = match (&git, args.no_pr) {
        (Some(g), false) => find_pr(g),
        _ => (None, None),
    };
    let mut comment = String::new();
    if let Some(text) = &args.comment {
        comment.push_str(
            truncate_chars(sanitize(text).trim(), MAX_COMMENT_CHARS)
                .0
                .as_str(),
        );
    }
    if let Some(url) = &pr_url {
        if !comment.is_empty() {
            comment.push_str("\n\n");
        }
        comment.push_str(&format!("PR: {url}"));
    }
    let comment_posted = !comment.is_empty();
    if comment_posted {
        client.add_comment(r.task.id, &comment)?;
    }

    let s = &ctx.style;
    let mut lines = Vec::new();
    if let Some(e) = elapsed {
        lines.push(format!(
            "{} {}  stopped {} logged to {}",
            s.good("\u{2713}"),
            s.muted("timer "),
            s.bold(&format_duration(e)),
            s.accent(&r.key)
        ));
    }
    if let Some(to) = &status_change {
        lines.push(format!(
            "{} {}  {} \u{2192} {}",
            s.good("\u{2713}"),
            s.muted("status"),
            r.status.name,
            s.good(to["to"]["name"].as_str().unwrap_or(""))
        ));
    }
    match (&pr_url, args.no_pr) {
        (Some(url), _) => lines.push(format!(
            "{} {}  {}",
            s.good("\u{2713}"),
            s.muted("PR    "),
            s.bold(url)
        )),
        (None, false) if git.is_some() => lines.push(s.muted("  no pull request found")),
        _ => {}
    }
    if lines.is_empty() {
        lines.push(s.muted("nothing to change"));
    }
    lines.push(String::new());
    lines.push(s.muted("hint: tmgr start <ref> to pick the next task"));
    let data = json!({
        "task": task_json(&r, &shown_status),
        "timer": {"stopped": timer_stopped, "elapsed_s": elapsed},
        "status_change": status_change,
        "branch": branch_name,
        "pr_url": pr_url,
        "pr_source": pr_source,
        "comment_posted": comment_posted,
    });
    let human = if ctx.quiet {
        r.key.clone()
    } else {
        lines.join("\n")
    };
    Ok(Output::new(data, human))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pr_urls_must_be_plain_https() {
        assert_eq!(
            clean_pr_url(" https://github.com/a/b/pull/1\n").as_deref(),
            Some("https://github.com/a/b/pull/1")
        );
        for bad in [
            "http://x/y",
            "javascript:alert(1)",
            "https://x/y z",
            "https://x/\u{1b}[31m",
            "",
        ] {
            assert_eq!(clean_pr_url(bad), None, "{bad:?}");
        }
        assert_eq!(
            clean_pr_url(&format!("https://x/{}", "a".repeat(600))),
            None
        );
    }
}
