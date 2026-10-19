use chrono::{DateTime, Duration, Utc};
use std::collections::BTreeSet;
use tmgr_core::error::Result;
use tmgr_queue::state::State;
use tmgr_queue::{PassReport, PausedDraft, StartPolicy};
use tmgr_usage::cost::{Billing, CostSummary};
use tmgr_usage::limits::LimitSnapshot;
use tmgr_usage::link::RunWindow;
use tmgr_usage::Tool;

pub trait Queue {
    fn pass(&self, policy: StartPolicy) -> Result<PassReport>;
    fn state(&self) -> Result<State>;
    fn pause_to_draft(&self, pickup: &str, note: &str) -> Result<PausedDraft>;
    fn set_paused(&self, paused: bool) -> Result<()>;
    fn disable(&self, reason: &str) -> Result<()>;
    fn workspace_code(&self, rule: &str) -> Option<String>;
    fn unconfirmed(&self, rule: &str, selected: &BTreeSet<i64>) -> Result<u32>;
}

pub trait RunControl {
    fn kill(&self, run: &str, signal: &str, reason: &str) -> Result<()>;
    fn alive(&self, run: &str) -> bool;
}

pub trait Remote {
    fn comment(&self, task_id: i64, text: &str) -> Result<()>;
    fn workspace_id(&self, code: &str) -> Result<i64>;
    fn statuses(&self, workspace_id: i64) -> Result<Vec<(u64, String)>>;
    fn task_status_id(&self, task_id: i64) -> Result<Option<u64>>;
}

pub struct Probed {
    pub windows: Vec<LimitSnapshot>,
    pub billing: Billing,
}

pub trait Usage {
    fn probe(&self, tool: Tool) -> Result<Probed>;
    fn stored(&self, now: DateTime<Utc>) -> Vec<LimitSnapshot>;
    fn scan(&self, now: DateTime<Utc>);
    fn cost(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        windows: &[RunWindow],
        billing: Billing,
    ) -> Option<CostSummary>;
}

pub trait Notifier {
    fn push(&self, title: &str, body: &str) -> bool;
}

pub trait Clock {
    fn now(&self) -> DateTime<Utc>;
    fn sleep(&self, d: Duration, wake: &dyn Fn() -> bool);
}

#[derive(Clone, Copy)]
pub struct Ports<'a> {
    pub queue: &'a dyn Queue,
    pub runs: &'a dyn RunControl,
    pub remote: &'a dyn Remote,
    pub usage: &'a dyn Usage,
    pub notify: &'a dyn Notifier,
    pub clock: &'a dyn Clock,
}

#[derive(Clone, Copy, Debug)]
pub struct Timing {
    pub tick: Duration,
    pub probe_every: Duration,
    pub remote_every: Duration,
    pub scan_every: Duration,
    pub usage_max_age: Duration,
    pub int_wait: Duration,
    pub term_wait: Duration,
    pub poll: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            tick: Duration::seconds(30),
            probe_every: Duration::minutes(5),
            remote_every: Duration::minutes(5),
            scan_every: Duration::minutes(5),
            usage_max_age: Duration::minutes(15),
            int_wait: Duration::seconds(10),
            term_wait: Duration::seconds(5),
            poll: Duration::milliseconds(250),
        }
    }
}
