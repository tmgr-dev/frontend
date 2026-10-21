use crate::ctx::{local_workspaces, Ctx};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use std::collections::{HashMap, HashSet};
use tmgr_core::api_ext::RunQuery;
use tmgr_core::error::{Kind, Result};
use tmgr_core::local;
use tmgr_core::models::Workspace;
use tmgr_core::refs::parse_key;
use tmgr_core::runs::{parse_ts, task_ref_of, ApiRun};
use tmgr_core::runstate::{self, Runs};
use tmgr_usage::bind::RunIds;
use tmgr_usage::facts::StoredFact;
use tmgr_usage::link::RunWindow;
use tmgr_usage::model::project_hash;

const LOOKBACK_DAYS: i64 = 3;
const MAX_PAGES: usize = 10;

#[derive(Clone)]
pub struct RunMeta {
    pub task_ref: String,
    pub task_id: Option<u64>,
    pub title: Option<String>,
}

#[derive(Default)]
pub struct RunSet {
    pub windows: Vec<RunWindow>,
    pub ids: Vec<RunIds>,
    pub roles: Vec<Option<String>>,
    pub meta: HashMap<String, RunMeta>,
    pub workspaces_ok: Vec<String>,
    pub workspaces_failed: Vec<String>,
}

impl RunSet {
    pub fn task_matches(&self, run_ref: &str, want: &WantedTask) -> bool {
        let Some(m) = self.meta.get(run_ref) else {
            return false;
        };
        match want {
            WantedTask::Id(id) => m.task_id == Some(*id),
            WantedTask::Key(key) => {
                let plain = m.task_ref.rsplit('/').next().unwrap_or(&m.task_ref);
                plain.eq_ignore_ascii_case(key)
            }
        }
    }
}

pub fn earlier_facts(
    store: &tmgr_usage::Store,
    set: &RunSet,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Option<Vec<StoredFact>>> {
    tmgr_usage::bind::facts_from(&set.windows, from)
        .map(|e| store.facts(e, to).map_err(super::fail))
        .transpose()
}

pub enum WantedTask {
    Id(u64),
    Key(String),
}

fn targets(ctx: &Ctx, set: &mut RunSet) -> Vec<Workspace> {
    let locals = local_workspaces().unwrap_or_default();
    let mut all: Vec<Workspace> = Vec::new();
    match ctx.cloud_client().and_then(|c| ctx.workspaces(c)) {
        Ok(cloud) => all.extend(cloud.into_iter().filter(|w| !w.local)),
        Err(e) if e.kind == Kind::Auth => ctx.warn(
            "usage_runs_cloud_skipped",
            "not signed in: runs of cloud workspaces are not linked (tmgr login)",
        ),
        Err(_) => ctx.warn(
            "usage_runs_cloud_skipped",
            "cloud workspaces could not be reached: their runs are not linked",
        ),
    }
    all.extend(locals);
    match ctx.ws.as_deref() {
        Some(code) => {
            let picked: Vec<Workspace> = all
                .into_iter()
                .filter(|w| w.code.eq_ignore_ascii_case(code))
                .collect();
            if picked.is_empty() {
                set.workspaces_failed.push(code.to_string());
                ctx.warn(
                    "usage_runs_unavailable",
                    format!("workspace {code} was not found: no runs are linked"),
                );
            }
            picked
        }
        None => all,
    }
}

fn fetch(
    ctx: &Ctx,
    ws: &Workspace,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> tmgr_core::error::Result<Option<(Vec<ApiRun>, bool)>> {
    let is_local = ws.local || local::local_code(&ws.code).is_some();
    let client = if is_local {
        ctx.client_for_run(Some(&ws.code))?
    } else {
        crate::ctx::RunClient::Shared(ctx.cloud_client()?)
    };
    let stamp = |t: DateTime<Utc>| t.to_rfc3339_opts(SecondsFormat::Secs, true);
    let mut runs = Vec::new();
    let mut cursor = None;
    let mut truncated = false;
    for page_no in 0..MAX_PAGES {
        let q = RunQuery {
            since: Some(stamp(from - Duration::days(LOOKBACK_DAYS))),
            until: Some(stamp(to)),
            limit: Some(200),
            cursor: cursor.take(),
            ..RunQuery::default()
        };
        let Some(page) = client.workspace_agent_runs(ws.id, &q)? else {
            return Ok(None);
        };
        runs.extend(page.runs);
        match page.next_cursor {
            Some(_) if page_no + 1 == MAX_PAGES => truncated = true,
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    Ok(Some((runs, truncated)))
}

fn repo_of(
    ctx: &Ctx,
    state: &Runs,
    tag: Option<&str>,
    run: &ApiRun,
    task_ref: &str,
) -> Option<String> {
    let recorded = state.find_run(run.id, tag).and_then(|r| r.repo.clone());
    recorded.or_else(|| {
        let key = parse_key(task_ref)?;
        let code = key.split('-').next()?;
        ctx.cfg.projects.get(code).cloned()
    })
}

fn window_of(run: &ApiRun, now: DateTime<Utc>) -> Option<(DateTime<Utc>, Option<DateTime<Utc>>)> {
    let start = run.started_at.as_deref().and_then(parse_ts)?;
    let end = match run.ended_at.as_deref().and_then(parse_ts) {
        Some(e) => Some(e),
        None if run.status.as_deref() == Some("running") => None,
        None => Some(start + Duration::seconds(run.duration_seconds.unwrap_or(0).max(0))),
    };
    (start <= now).then_some((start, end))
}

pub fn collect(ctx: &Ctx, from: DateTime<Utc>, to: DateTime<Utc>, now: DateTime<Utc>) -> RunSet {
    let mut set = RunSet::default();
    let workspaces = targets(ctx, &mut set);
    let state = runstate::runs_path()
        .map(|p| runstate::load_from(&p))
        .unwrap_or_default();
    let mut ref_workspaces: HashMap<String, HashSet<String>> = HashMap::new();
    let mut staged: Vec<(RunWindow, RunMeta, RunIds, Option<String>)> = Vec::new();
    for ws in &workspaces {
        let runs = match fetch(ctx, ws, from, to) {
            Ok(Some((runs, truncated))) => {
                if truncated {
                    ctx.warn(
                        "usage_runs_truncated",
                        format!(
                            "workspace {} has more runs than were read: some usage may stay unlinked",
                            ws.code
                        ),
                    );
                }
                runs
            }
            Ok(None) => continue,
            Err(_) => {
                set.workspaces_failed.push(ws.code.clone());
                ctx.warn(
                    "usage_runs_unavailable",
                    format!("runs of workspace {} could not be read", ws.code),
                );
                continue;
            }
        };
        set.workspaces_ok.push(ws.code.clone());
        let tag = (ws.local || local::local_code(&ws.code).is_some()).then_some(ws.code.as_str());
        for run in &runs {
            let Some(task) = run.task.as_ref() else {
                continue;
            };
            let task_ref = task_ref_of(task);
            let Some((start, end)) = window_of(run, now) else {
                continue;
            };
            if end.is_some_and(|e| e <= from) || start >= to {
                continue;
            }
            let hash = repo_of(ctx, &state, tag, run, &task_ref)
                .map(|r| project_hash(r.trim_end_matches('/')));
            ref_workspaces
                .entry(task_ref.clone())
                .or_default()
                .insert(ws.code.clone());
            staged.push((
                RunWindow {
                    run_ref: format!("{}#{}", ws.code, run.id),
                    task_ref: task_ref.clone(),
                    workspace: ws.code.clone(),
                    session_id: run.session_id.clone().filter(|s| !s.is_empty()),
                    branch: run.branch.clone().filter(|s| !s.is_empty()),
                    project_hash: hash,
                    started_at: start,
                    ended_at: end,
                },
                RunMeta {
                    task_ref,
                    task_id: task.id.or(run.task_id),
                    title: task.title.clone(),
                },
                RunIds {
                    run_id: run.id,
                    task_id: task.id.or(run.task_id),
                },
                run.role.clone().filter(|r| !r.is_empty()),
            ));
        }
    }
    for (mut window, mut meta, ids, role) in staged {
        let shared = ref_workspaces
            .get(&window.task_ref)
            .is_some_and(|w| w.len() > 1);
        if shared {
            window.task_ref = format!("{}/{}", window.workspace, window.task_ref);
            meta.task_ref = window.task_ref.clone();
        }
        set.meta.insert(window.run_ref.clone(), meta);
        set.windows.push(window);
        set.ids.push(ids);
        set.roles.push(role);
    }
    set
}
