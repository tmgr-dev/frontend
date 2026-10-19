use super::ports::{
    Clock, Notifier, Ports, Probed, Queue, Remote, RunControl, Timing, Usage as UsagePort,
};
use super::serve;
use crate::cli::Cli;
use crate::cmd::autopilot::store::Store;
use crate::cmd::notifyenv::NotifyEnv;
use crate::cmd::queue::{count_unconfirmed, not_in_agent_session, with_deps, RunnerRuns};
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
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tmgr_core::config::ensure_private_dir;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_queue::audit::Audit;
use tmgr_queue::runs::{RunState, RunsClient};
use tmgr_queue::state::{self, State};
use tmgr_queue::{PassReport, Paths, PausedDraft, StartPolicy};
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

    fn statuses(&self, workspace_id: i64) -> Result<Vec<(u64, String)>> {
        Ok(self
            .0
            .cloud_client()?
            .statuses(workspace_id)?
            .into_iter()
            .map(|s| (s.id, s.name))
            .collect())
    }

    fn task_status_id(&self, task_id: i64) -> Result<Option<u64>> {
        Ok(self.0.cloud_client()?.task_by_id(task_id as u64)?.status_id)
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

    fn scan(&self, now: DateTime<Utc>) {
        let ctx = self.ctx;
        self.with(|u| usage::scan(ctx, u, now));
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
    let usage = RealUsage {
        ctx,
        usage: RefCell::new(None),
    };
    let notify = RealNotify(ctx);
    let ports = Ports {
        queue: &queue,
        runs: &run_control,
        remote: &remote,
        usage: &usage,
        notify: &notify,
        clock: &RealClock,
    };
    match serve(&store, &settings, ports, timing(), &TERM) {
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
