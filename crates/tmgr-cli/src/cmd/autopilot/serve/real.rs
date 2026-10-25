use super::decide::{log_read, LogRead};
use super::ports::{
    Clock, Notifier, PlanFresh, PlanInputs, Planned, Planner, Ports, Probed, Queue, Remote,
    RunControl, Timing, Usage as UsagePort,
};
use super::serve;
use crate::cli::Cli;
use crate::cmd::autopilot::orchestrate::{self, OrchInput, Poll, TaskText};
use crate::cmd::autopilot::store::Store;
use crate::cmd::notifyenv::NotifyEnv;
use crate::cmd::queue::{
    count_unconfirmed, fresh_look, not_in_agent_session, with_deps, CliCloud, RunnerRuns,
};
use crate::cmd::usage::{self, Usage};
use crate::ctx::Ctx;
use crate::out::Output;
use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs::File;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tmgr_core::config::ensure_private_dir;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::text::sanitize_line;
use tmgr_queue::audit::Audit;
use tmgr_queue::cloud::{CloudApi, CloudWorkspace, Gated};
use tmgr_queue::rules::{is_cloud, Rule};
use tmgr_queue::runs::{RunState, RunsClient};
use tmgr_queue::state::{self, State};
use tmgr_queue::workdir::clone_dir;
use tmgr_queue::{PassReport, Paths, PausedDraft, Requeued, StartPolicy};
use tmgr_runner::client::Client;
use tmgr_usage::cost::{cost_between, Billing, CostSummary};
use tmgr_usage::limits::LimitSnapshot;
use tmgr_usage::link::RunWindow;
use tmgr_usage::probe::{claude_usage, codex_usage};
use tmgr_usage::Tool;

const LOCK_RETRY: std::time::Duration = std::time::Duration::from_secs(5);
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
const PUSH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

static TERM: AtomicBool = AtomicBool::new(false);

extern "C" fn on_term(_: libc::c_int) {
    TERM.store(true, Ordering::SeqCst);
}

fn install_handlers() {
    for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        unsafe { libc::signal(sig, on_term as *const () as libc::sighandler_t) };
    }
}

pub fn acquire_lock(store: &Store) -> Result<File> {
    ensure_private_dir(&store.dir())?;
    let f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(store.lock_path())?;
    let end = std::time::Instant::now() + LOCK_RETRY;
    loop {
        if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            let mut fl: libc::flock = unsafe { std::mem::zeroed() };
            fl.l_type = libc::F_WRLCK as _;
            fl.l_whence = libc::SEEK_SET as _;
            unsafe { libc::fcntl(f.as_raw_fd(), libc::F_SETLK, &fl) };
            return Ok(f);
        }
        if std::time::Instant::now() >= end {
            return Err(Error::new(Kind::Conflict, "autopilot is already running"));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

struct RealQueue<'a> {
    ctx: &'a Ctx,
    cli: &'a Cli,
    paths: &'a Paths,
    runs: &'a RunnerRuns,
}

impl Queue for RealQueue<'_> {
    fn pass(&self, policy: StartPolicy) -> Result<PassReport> {
        if tmgr_queue::needs_runner(self.paths)? {
            self.runs.ready()?;
        }
        with_deps(self.ctx, self.cli, self.runs, policy, |deps| {
            tmgr_queue::run_once(self.paths, deps)
        })
    }

    fn state(&self) -> Result<State> {
        state::load(self.paths)
    }

    fn pause_to_draft(&self, pickup: &str, note: &str) -> Result<PausedDraft> {
        with_deps(
            self.ctx,
            self.cli,
            self.runs,
            StartPolicy::default(),
            |deps| tmgr_queue::pause_to_draft(self.paths, deps, pickup, note),
        )
    }

    fn requeue_paused(&self, pickup: &str, policy: StartPolicy) -> Result<Requeued> {
        if tmgr_queue::needs_runner(self.paths)? {
            self.runs.ready()?;
        }
        with_deps(self.ctx, self.cli, self.runs, policy, |deps| {
            tmgr_queue::requeue_paused(self.paths, deps, pickup, Utc::now())
        })
    }

    fn resumable(&self) -> Result<Vec<String>> {
        Ok(tmgr_queue::paused_resumable(&state::load(self.paths)?))
    }

    fn set_paused(&self, paused: bool) -> Result<()> {
        let _lock = self.paths.lock()?;
        let mut st = state::load(self.paths)?;
        st.paused = paused;
        state::save(self.paths, &mut st)?;
        Audit::new(self.paths).append(
            if paused { "pause" } else { "resume" },
            json!({"by": "autopilot"}),
        )
    }

    fn disable(&self, reason: &str) -> Result<()> {
        tmgr_queue::switch::disable(self.paths, Some(reason))
    }

    fn keep_on(&self) -> Result<()> {
        crate::cmd::autopilot::queue_loop::save_settings(self.paths)
    }

    fn workspace_code(&self, rule: &str) -> Option<String> {
        tmgr_queue::rules::load(self.paths)
            .ok()?
            .file
            .rules
            .into_iter()
            .find(|r| r.name == rule)
            .map(|r| r.workspace)
    }

    fn unconfirmed(&self, rule: &str, selected: &BTreeSet<i64>) -> Result<u32> {
        count_unconfirmed(self.ctx, self.paths, rule, selected)
    }
}

struct RealRuns<'a>(&'a RunnerRuns);

impl RunControl for RealRuns<'_> {
    fn kill(&self, run: &str, signal: &str, reason: &str) -> Result<()> {
        self.0
            .kill(run, signal, reason)
            .map_err(|e| Error::new(Kind::Offline, e.to_string()))
    }

    fn alive(&self, run: &str) -> bool {
        matches!(self.0.status(run), RunState::Running | RunState::Unknown)
    }
}

struct RealRemote<'a>(&'a Ctx);

impl Remote for RealRemote<'_> {
    fn comment(&self, task_id: i64, text: &str) -> Result<()> {
        self.0.cloud_client()?.add_comment(task_id as u64, text)
    }

    fn workspace_id(&self, code: &str) -> Result<i64> {
        self.0
            .cloud_client()?
            .workspaces()?
            .into_iter()
            .find(|w| w.code.eq_ignore_ascii_case(code))
            .map(|w| w.id)
            .ok_or_else(|| Error::not_found(format!("no workspace {code}")))
    }

    fn statuses(&self, workspace_id: i64) -> Result<Vec<(u64, String, String)>> {
        Ok(self
            .0
            .cloud_client()?
            .statuses(workspace_id)?
            .into_iter()
            .map(|s| (s.id, s.name, s.kind))
            .collect())
    }

    fn task_status_id(&self, task_id: i64) -> Result<Option<u64>> {
        Ok(self.0.cloud_client()?.task_by_id(task_id as u64)?.status_id)
    }

    fn set_status(&self, task_id: i64, status_id: u64) -> Result<()> {
        self.0.cloud_client()?.set_task_status(
            u64::try_from(task_id).map_err(|_| Error::usage("bad task id"))?,
            status_id,
        )
    }
}

struct RealPlanner<'a> {
    ctx: &'a Ctx,
    paths: &'a Paths,
    runs: &'a RunnerRuns,
    persona_optional_rule: Option<&'a str>,
    tasks: &'a BTreeSet<i64>,
}

fn persona_optional_for(
    optional_rule: Option<&str>,
    rule: &str,
    tasks: &BTreeSet<i64>,
    id: i64,
) -> bool {
    optional_rule == Some(rule) && tasks.contains(&id)
}

const ORCHESTRATOR_PROFILE: &str = "claude-code";

impl RealPlanner<'_> {
    fn rule(&self, name: &str) -> Result<Rule> {
        tmgr_queue::rules::load(self.paths)?
            .file
            .rules
            .into_iter()
            .find(|r| r.name == name)
            .ok_or_else(|| Error::not_found(format!("no rule {name}")))
    }

    fn workspace(&self, cloud: &CliCloud, rule: &Rule) -> Result<CloudWorkspace> {
        cloud
            .allow(&rule.workspace, Utc::now())
            .map_err(|m| Error::new(Kind::Forbidden, m))
    }

    fn status_id(&self, task_id: i64) -> Result<Option<u64>> {
        Ok(self
            .ctx
            .cloud_client()?
            .task_by_id(u64::try_from(task_id).map_err(|_| Error::usage("bad task id"))?)?
            .status_id)
    }

    fn via<T>(&self, f: impl FnOnce(&mut Client) -> Result<T>) -> Result<T> {
        self.runs
            .with(|c| f(c).map_err(|e| std::io::Error::other(e.message)))
            .map_err(|e| Error::new(Kind::Offline, e.to_string()))
    }
}

impl Planner for RealPlanner<'_> {
    fn enabled(&self, rule: &str) -> bool {
        self.rule(rule).is_ok_and(|r| is_cloud(&r))
    }

    fn inputs(&self, rule: &str, tasks: &BTreeSet<i64>) -> Result<PlanInputs> {
        let rule = self.rule(rule)?;
        let cloud = CliCloud::new(self.ctx);
        let ws = self.workspace(&cloud, &rule)?;
        let mut out = PlanInputs::default();
        for &id in tasks {
            match cloud.gate_readonly(&rule, &ws, id) {
                Ok(Gated::Ready(t)) => {
                    let status_id = self.status_id(id).unwrap_or(None);
                    out.tasks.push(Planned {
                        content_sha256: t.claim.content_sha256.clone(),
                        status_id,
                        text: TaskText {
                            task_id: id,
                            key: sanitize_line(&t.key),
                            title: t.title,
                            description: t.body,
                            repos: t.repos.iter().map(|r| r.full_name.clone()).collect(),
                        },
                    });
                }
                Ok(Gated::Refused { key, reason }) => out.not_planned.push((id, key, reason)),
                Err(e) => out.failed.push((id, sanitize_line(&e.message))),
            }
        }
        Ok(out)
    }

    fn start(&self, rule: &str, input: &OrchInput, nonce: &str) -> Result<(String, PathBuf)> {
        let rule = self.rule(rule)?;
        let first = rule
            .repos
            .first()
            .ok_or_else(|| Error::usage("the rule has no repository"))?;
        let root = clone_dir(first, "orchestrator")
            .and_then(|d| d.parent().map(Path::to_path_buf))
            .ok_or_else(|| Error::usage("the rule's repository has no parent folder"))?;
        let dir = orchestrate::prepare_dir(&root, nonce)?;
        orchestrate::write_files(&dir, input)?;
        let key = format!("orch-{nonce}");
        let run = self.via(|c| {
            orchestrate::start(
                c,
                ORCHESTRATOR_PROFILE,
                &dir,
                &key,
                orchestrate::launch_pin(ORCHESTRATOR_PROFILE),
            )
        })?;
        Ok((run, dir))
    }

    fn poll(&self, run_id: &str) -> Result<Poll> {
        self.via(|c| orchestrate::poll(c, run_id))
    }

    fn read(&self, dir: &Path) -> Result<Vec<u8>> {
        orchestrate::read_plan(dir)
    }

    fn discard(&self, dir: &Path) {
        let ours = dir
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("orchestrator-"));
        if ours {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    fn kill(&self, run_id: &str) -> Result<()> {
        self.runs
            .kill(run_id, "term", "autopilot: orchestrator stopped")
            .map_err(|e| Error::new(Kind::Offline, e.to_string()))
    }

    fn fresh(&self, rule: &str, task_id: i64) -> Result<PlanFresh> {
        let rule = self.rule(rule)?;
        let cloud = CliCloud::new(self.ctx);
        let ws = self.workspace(&cloud, &rule)?;
        let content_sha256 = match cloud.gate_readonly(&rule, &ws, task_id)? {
            Gated::Ready(t) => t.claim.content_sha256.clone(),
            Gated::Refused { .. } => String::new(),
        };
        let free = fresh_look(self.ctx, &rule, ws.id, task_id)?.free_with(persona_optional_for(
            self.persona_optional_rule,
            &rule.name,
            self.tasks,
            task_id,
        ));
        Ok(PlanFresh {
            content_sha256,
            free,
            status_id: self.status_id(task_id)?,
        })
    }
}

struct RealUsage<'a> {
    ctx: &'a Ctx,
    usage: RefCell<Option<Usage>>,
}

fn find_bin(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(name))
        .find(|p| {
            std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
}

impl RealUsage<'_> {
    fn with<T>(&self, f: impl FnOnce(&mut Usage) -> T) -> Option<T> {
        let mut slot = self.usage.borrow_mut();
        if slot.is_none() {
            *slot = Usage::open().ok();
        }
        slot.as_mut().map(f)
    }
}

impl UsagePort for RealUsage<'_> {
    fn probe(&self, tool: Tool) -> Result<Probed> {
        if let Some((windows, billing)) = crate::cmd::autopilot::usage::injected(tool) {
            return Ok(Probed { windows, billing });
        }
        let missing = || Error::not_found(format!("{} is not installed", tool.as_str()));
        match tool {
            Tool::Claude => {
                let bin = find_bin("claude").ok_or_else(missing)?;
                let p = claude_usage(&bin, PROBE_TIMEOUT).map_err(usage::fail)?;
                Ok(Probed {
                    billing: match p.rate_limits_available {
                        Some(true) => Billing::Subscription,
                        Some(false) => Billing::ApiKey,
                        None => Billing::Unknown,
                    },
                    windows: p.windows,
                })
            }
            Tool::Codex => {
                let bin = find_bin("codex").ok_or_else(missing)?;
                let p = codex_usage(&bin, PROBE_TIMEOUT).map_err(usage::fail)?;
                Ok(Probed {
                    billing: if p.windows.is_empty() {
                        Billing::Unknown
                    } else {
                        Billing::Subscription
                    },
                    windows: p.windows,
                })
            }
        }
    }

    fn stored(&self, now: DateTime<Utc>) -> Vec<LimitSnapshot> {
        self.with(|u| u.store.current_limits(now).unwrap_or_default())
            .unwrap_or_default()
    }

    fn scan(&self, now: DateTime<Utc>, tool: Option<Tool>) -> LogRead {
        let ctx = self.ctx;
        let Some(Ok(t)) = self.with(|u| usage::scan(ctx, u, now, false)) else {
            return LogRead::Unavailable;
        };
        log_read(&t.allowed, &t.failed, tool)
    }

    fn cost(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        windows: &[RunWindow],
        billing: Billing,
    ) -> Option<CostSummary> {
        self.with(|u| cost_between(&u.store, &u.book, from, to, windows, |_| billing).ok())
            .flatten()
    }
}

struct RealNotify<'a>(&'a Ctx);

impl Notifier for RealNotify<'_> {
    fn push(&self, title: &str, body: &str) -> bool {
        let payload =
            tmgr_core::notify::build_push(title, Some(body), None, None, Some("autopilot"));
        let mut warnings = Vec::new();
        NotifyEnv::from_ctx(self.0)
            .with_http(&mut warnings, |http| http.push(&payload, PUSH_TIMEOUT))
            .is_ok()
    }
}

struct RealClock;

impl Clock for RealClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn sleep(&self, d: Duration, wake: &dyn Fn() -> bool) {
        let end = std::time::Instant::now() + d.to_std().unwrap_or_default();
        while std::time::Instant::now() < end {
            if wake() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
}

#[cfg(debug_assertions)]
fn timing() -> Timing {
    let mut t = Timing::default();
    if let Some(ms) = std::env::var("TMGR_TEST_AUTOPILOT_TICK_MS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
    {
        t.tick = Duration::milliseconds(ms);
    }
    if let Some(ms) = std::env::var("TMGR_TEST_AUTOPILOT_PROBE_MS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
    {
        t.probe_every = Duration::milliseconds(ms);
    }
    t
}

#[cfg(not(debug_assertions))]
fn timing() -> Timing {
    Timing::default()
}

pub fn run(ctx: &Ctx, cli: &Cli, settings_id: &str) -> Result<Output> {
    not_in_agent_session()?;
    if !tmgr_runner::autopilot::valid_settings_id(settings_id) {
        return Err(Error::usage("invalid settings id"));
    }
    let store = Store::from_env()?;
    let settings = store.load_settings(settings_id)?;
    let _lock = acquire_lock(&store)?;
    install_handlers();
    let paths = Paths::from_env()?;
    let runs = RunnerRuns::attach_only()?;
    let queue = RealQueue {
        ctx,
        cli,
        paths: &paths,
        runs: &runs,
    };
    let run_control = RealRuns(&runs);
    let remote = RealRemote(ctx);
    let planner = RealPlanner {
        ctx,
        paths: &paths,
        runs: &runs,
        persona_optional_rule: settings.persona_optional.then_some(settings.rule.as_str()),
        tasks: &settings.tasks,
    };
    let usage = RealUsage {
        ctx,
        usage: RefCell::new(None),
    };
    let notify = RealNotify(ctx);
    let ports = Ports {
        queue: &queue,
        runs: &run_control,
        remote: &remote,
        planner: &planner,
        usage: &usage,
        notify: &notify,
        clock: &RealClock,
    };
    match serve(&store, &settings, ports, timing(), &TERM) {
        Ok(out) if out.suspended => Ok(Output::new(
            json!({"suspended": true}),
            "Autopilot suspended: the queue stays on.",
        )),
        Ok(out) => Ok(Output::new(
            json!({"stopped": true, "reason": out.reason, "summary": out.summary}),
            format!("Autopilot stopped: {}", out.summary.text),
        )),
        Err(e) => {
            let _ = queue.disable(&format!("autopilot failed: {}", e.message));
            Err(e)
        }
    }
}

#[cfg(test)]
mod persona_tests {
    use super::*;

    #[test]
    fn the_persona_is_optional_only_for_the_runs_own_tasks() {
        let tasks = BTreeSet::from([1, 2]);
        assert!(persona_optional_for(Some("r"), "r", &tasks, 1));
        assert!(!persona_optional_for(Some("r"), "r", &tasks, 3));
        assert!(!persona_optional_for(Some("r"), "other", &tasks, 1));
        assert!(!persona_optional_for(None, "r", &tasks, 1));
    }
}
