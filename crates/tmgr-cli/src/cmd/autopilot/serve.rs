pub mod decide;
pub mod ports;
pub mod real;

use super::model::{
    ActiveRun, Cost, Event, LimitRow, NextStop, PrLink, Reasoned, RunState, Settings, Slots,
    StatusDoc, Summary, TaskCounts, STATUS_VERSION,
};
use super::store::Store;
use chrono::{DateTime, Duration, Utc};
use decide::{
    deadline, decide, next_stop, verdict, Blocker, Decision, Facts, Hit, Inputs, StopReason,
};
use ports::{Ports, Timing};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::text::sanitize_line;
use tmgr_queue::state::{Pickup, State, Status, MAX_PUBLISH_ATTEMPTS};
use tmgr_queue::StartPolicy;
use tmgr_usage::cost::Billing;
use tmgr_usage::guard::{guard_limits, GuardLimit};
use tmgr_usage::limits::LimitSnapshot;
use tmgr_usage::link::RunWindow;
use tmgr_usage::Tool;

const MAX_STATUS_EVENTS: usize = 50;
const MAX_PASS_ERRORS: u32 = 10;
const MIN_PROBE_GAP_SECONDS: i64 = 60;
const MAX_STATUS_LOOKUPS: usize = 100;

pub struct Outcome {
    pub reason: String,
    pub summary: Summary,
}

#[derive(Clone, Debug, PartialEq)]
enum Class {
    Done,
    Paused,
    Held,
    Failed(String),
    InWork,
    Approved,
    Pending,
    Taken,
    Gone,
}

fn classify(p: &Pickup, killed: &BTreeSet<String>) -> Class {
    let exited = p.status == Status::Started && p.run_exited;
    if p.pr_url.is_some() && !p.paused_draft {
        return Class::Done;
    }
    if p.paused_draft {
        return Class::Paused;
    }
    if p.held.is_some() {
        return Class::Held;
    }
    match p.status {
        Status::Failed => {
            return Class::Failed(
                p.reason
                    .clone()
                    .unwrap_or_else(|| "the start failed".into()),
            )
        }
        Status::Expired => {
            return if p
                .reason
                .as_deref()
                .is_some_and(|r| r.starts_with("skipped_taken"))
            {
                Class::Taken
            } else {
                Class::Gone
            }
        }
        _ => {}
    }
    if exited && !p.run_clean {
        return if killed.contains(&p.id) {
            Class::InWork
        } else {
            Class::Failed("the agent exited with an error".into())
        };
    }
    if exited && p.cloud.is_none() {
        return Class::Done;
    }
    if exited && p.pr_url.is_none() && p.publish_attempts >= MAX_PUBLISH_ATTEMPTS {
        return Class::Failed(
            p.publish_error
                .clone()
                .unwrap_or_else(|| "publishing failed".into()),
        );
    }
    if p.occupies() || exited {
        return Class::InWork;
    }
    match p.status {
        Status::Approved => Class::Approved,
        Status::PendingApproval => Class::Pending,
        _ => Class::Gone,
    }
}

fn latest(st: &State, task_id: i64) -> Option<&Pickup> {
    st.pickups.iter().rev().find(|p| p.task.task_id == task_id)
}

fn skipped_ids(st: &State) -> BTreeSet<i64> {
    st.skips
        .keys()
        .filter_map(|k| k.rsplit('/').next()?.parse().ok())
        .collect()
}

fn skip_reason(st: &State, task_id: i64) -> String {
    let suffix = format!("/{task_id}");
    st.skips
        .iter()
        .find(|(k, _)| k.ends_with(&suffix))
        .map(|(_, v)| v.trim_start_matches("gate:").to_string())
        .unwrap_or_default()
}

#[derive(Default)]
struct View {
    counts: TaskCounts,
    paused: u32,
    live: Vec<Live>,
    awaiting: usize,
    eligible: usize,
    runs: Vec<ActiveRun>,
    needs_runner: bool,
}

#[derive(Clone)]
struct Live {
    pickup: String,
    run: String,
    task_id: i64,
    key: String,
}

struct Why {
    kill: String,
    comment: String,
    note: String,
}

impl Why {
    fn limit(hit: &Hit) -> Why {
        let kill = format!("Paused: usage limit reached at {:.0}%", hit.pct);
        Why {
            comment: format!("{kill} ({})", hit.window),
            note: format!("usage limit reached at {:.0}%", hit.pct),
            kill,
        }
    }

    fn stopped() -> Why {
        Why {
            kill: "Paused: autopilot stopped".into(),
            comment: "Paused: autopilot stopped".into(),
            note: "autopilot stopped".into(),
        }
    }

    fn hours() -> Why {
        Why {
            kill: "Paused: autopilot time limit reached".into(),
            comment: "Paused: autopilot time limit reached".into(),
            note: "autopilot time limit reached".into(),
        }
    }
}

#[derive(Default)]
struct RemoteCache {
    at: Option<DateTime<Utc>>,
    ws: Option<i64>,
    feedback: Option<u64>,
    blocked: Option<u64>,
    task_status: BTreeMap<i64, u64>,
    unconfirmed: u32,
    last_error: Option<String>,
}

pub struct Autopilot<'a> {
    store: &'a Store,
    s: &'a Settings,
    p: Ports<'a>,
    t: Timing,
    term: &'a AtomicBool,
    tool: Option<Tool>,
    started_at: DateTime<Utc>,
    state: RunState,
    last_seq: u64,
    stop_req: Option<(Why, String)>,
    events: Vec<Event>,
    prs: Vec<PrLink>,
    baseline: BTreeSet<String>,
    started_ids: BTreeSet<String>,
    killed: BTreeSet<String>,
    counted_fail: BTreeSet<String>,
    counted_pr: BTreeSet<String>,
    counted_held: BTreeSet<String>,
    noted_skips: BTreeSet<i64>,
    failures: u32,
    run_times: BTreeMap<String, (RunWindow, bool)>,
    probes: Vec<LimitSnapshot>,
    last_probe: Option<DateTime<Utc>>,
    probe_error: Option<String>,
    billing: Billing,
    guard: Vec<GuardLimit>,
    unknown_since: Option<DateTime<Utc>>,
    cost: Cost,
    last_scan: Option<DateTime<Utc>>,
    remote: RemoteCache,
    notified_limit: Option<(String, bool)>,
    pushes: Option<bool>,
    settled: bool,
    pass_errors: u32,
    view: View,
    held_log: Vec<Reasoned>,
    failed_log: Vec<Reasoned>,
    paused_log: Vec<Reasoned>,
    blockers: Vec<String>,
    stop_reason: Option<String>,
    summary: Option<Summary>,
    limit_paused: Option<Hit>,
}

pub fn serve(
    store: &Store,
    settings: &Settings,
    ports: Ports,
    timing: Timing,
    term: &AtomicBool,
) -> Result<Outcome> {
    let mut ap = Autopilot::new(store, settings, ports, timing, term);
    ap.run()
}

fn limit_text(h: &Hit) -> String {
    format!("{} at {:.0}%", h.window, h.pct)
}

impl<'a> Autopilot<'a> {
    pub fn new(
        store: &'a Store,
        s: &'a Settings,
        p: Ports<'a>,
        t: Timing,
        term: &'a AtomicBool,
    ) -> Self {
        Autopilot {
            store,
            s,
            p,
            t,
            term,
            tool: Tool::parse(&s.agent).ok(),
            started_at: p.clock.now(),
            state: RunState::Running,
            last_seq: 0,
            stop_req: None,
            events: Vec::new(),
            prs: Vec::new(),
            baseline: BTreeSet::new(),
            started_ids: BTreeSet::new(),
            killed: BTreeSet::new(),
            counted_fail: BTreeSet::new(),
            counted_pr: BTreeSet::new(),
            counted_held: BTreeSet::new(),
            noted_skips: BTreeSet::new(),
            failures: 0,
            run_times: BTreeMap::new(),
            probes: Vec::new(),
            last_probe: None,
            probe_error: None,
            billing: Billing::Unknown,
            guard: Vec::new(),
            unknown_since: None,
            cost: Cost::default(),
            last_scan: None,
            remote: RemoteCache::default(),
            notified_limit: None,
            pushes: None,
            settled: false,
            pass_errors: 0,
            view: View::default(),
            held_log: Vec::new(),
            failed_log: Vec::new(),
            paused_log: Vec::new(),
            blockers: Vec::new(),
            stop_reason: None,
            summary: None,
            limit_paused: None,
        }
    }

    fn now(&self) -> DateTime<Utc> {
        self.p.clock.now()
    }

    fn event(&mut self, kind: &str, task: Option<&str>, text: impl Into<String>) {
        let ev = Event {
            at: Some(self.now()),
            kind: kind.into(),
            task: task.map(str::to_string),
            text: sanitize_line(&text.into()),
        };
        if let Ok(line) = serde_json::to_value(&ev) {
            let _ = self.store.append_log(&line);
        }
        self.events.push(ev);
        if self.events.len() > MAX_STATUS_EVENTS {
            self.events.remove(0);
        }
    }

    fn push(&mut self, title: &str, body: &str) {
        self.pushes = Some(self.p.notify.push(title, body));
    }

    fn wake(&self) -> bool {
        self.term.load(Ordering::SeqCst)
            || self
                .store
                .read_control()
                .ok()
                .flatten()
                .and_then(|v| v["seq"].as_u64())
                .is_some_and(|s| s > self.last_seq)
    }

    fn begin(&mut self) {
        let n = self.s.tasks.len();
        self.event(
            "started",
            None,
            format!(
                "Autopilot started: {n} task{}, up to {} at once",
                if n == 1 { "" } else { "s" },
                self.s.answers.max_concurrent
            ),
        );
        if let Ok(st) = self.p.queue.state() {
            for p in st
                .pickups
                .iter()
                .filter(|p| self.s.tasks.contains(&p.task.task_id))
            {
                if p.run_id.is_some() || p.status == Status::UnknownStart {
                    self.baseline.insert(p.id.clone());
                }
                match classify(p, &self.killed) {
                    Class::Done | Class::Paused => {
                        self.counted_pr.insert(p.id.clone());
                    }
                    Class::Held => {
                        self.counted_held.insert(p.id.clone());
                    }
                    Class::Failed(_) => {
                        self.counted_fail.insert(p.id.clone());
                    }
                    _ => {}
                }
            }
        }
        let body = format!("{n} task{} selected", if n == 1 { "" } else { "s" });
        self.push("Autopilot started", &body);
        self.write_status();
    }

    fn run(&mut self) -> Result<Outcome> {
        self.begin();
        loop {
            if let Some(out) = self.tick()? {
                return Ok(out);
            }
            let clock = self.p.clock;
            let tick = self.t.tick;
            clock.sleep(tick, &|| self.wake());
        }
    }

    fn read_control(&mut self) {
        let Ok(Some(v)) = self.store.read_control() else {
            return;
        };
        let Some(seq) = v["seq"].as_u64().filter(|s| *s > self.last_seq) else {
            return;
        };
        self.last_seq = seq;
        match v["command"].as_str() {
            Some("pause") => {
                self.state = RunState::Paused;
                self.event("control", None, "Paused: no new tasks will start");
            }
            Some("resume") => {
                self.state = RunState::Running;
                self.event("control", None, "Resumed");
            }
            Some("stop") => {
                self.stop_req = Some((Why::stopped(), "stopped by you".into()));
                self.event("control", None, "Stop requested");
            }
            other => {
                let text = format!("Unknown control command {other:?}");
                self.event("error", None, text);
            }
        }
    }

    fn tick(&mut self) -> Result<Option<Outcome>> {
        self.read_control();
        if self.term.load(Ordering::SeqCst) && self.stop_req.is_none() {
            self.stop_req = Some((Why::stopped(), "stopped by SIGTERM".into()));
            self.event("control", None, "Stop requested (SIGTERM)");
        }
        if let Some((why, text)) = self.stop_req.take() {
            return self.finish(StopReason::Requested(text), Some(why));
        }
        let now = self.now();
        self.refresh_usage(now);
        if let Ok(st) = self.p.queue.state() {
            self.refresh_remote(now, &st);
        }
        let (decision, _) = self.evaluate(now)?;
        if decision.hours_hard {
            return self.finish(StopReason::Hours, Some(Why::hours()));
        }
        if let Some(hit) = decision.hard_limit.clone() {
            if !self.view.live.is_empty() {
                self.note_limit(&hit, true);
                self.limit_paused = Some(hit.clone());
                self.drain(&Why::limit(&hit));
            }
        }
        let policy = StartPolicy {
            starts_allowed: decision.starts_allowed(),
            max_live: Some(self.s.answers.max_concurrent as usize),
            max_new_starts: self
                .s
                .answers
                .max_tasks
                .map(|m| (m as usize).saturating_sub(self.started_ids.len())),
            only: Some(self.s.tasks.clone()),
        };
        match self.p.queue.pass(policy) {
            Ok(report) => {
                self.pass_errors = 0;
                if !report.enabled {
                    return self.finish(
                        StopReason::Requested("the queue was turned off".into()),
                        None,
                    );
                }
                let mut clean = true;
                for e in &report.events {
                    match e["outcome"].as_str() {
                        Some("source_error") => {
                            clean = false;
                            let text = format!(
                                "Could not read tasks: {}",
                                e["reason"].as_str().unwrap_or("unknown error")
                            );
                            self.event("error", None, text);
                        }
                        Some("taken") => {
                            let key = e["task"].as_str().unwrap_or("").to_string();
                            let text = format!("{key} was taken by someone else");
                            self.event("taken", Some(&key), text.clone());
                            self.push("Autopilot: task taken", &text);
                        }
                        _ => {}
                    }
                }
                if clean {
                    self.settled = true;
                }
            }
            Err(e) => {
                self.pass_errors += 1;
                let text = format!("Queue pass failed: {}", e.message);
                self.event("error", None, text);
                if e.kind == Kind::Forbidden || self.pass_errors >= MAX_PASS_ERRORS {
                    return self.finish(
                        StopReason::Requested(format!("the queue cannot run: {}", e.message)),
                        None,
                    );
                }
            }
        }
        let now = self.now();
        let (decision, facts) = self.evaluate(now)?;
        self.note_decision(&decision);
        let reason = verdict(&decision, &facts, now);
        self.write_status();
        match reason {
            Some(r) => self.finish(r, None),
            None => Ok(None),
        }
    }

    fn evaluate(&mut self, now: DateTime<Utc>) -> Result<(Decision, Facts)> {
        let st = self.p.queue.state()?;
        self.observe(&st, now);
        let decision = decide(&Inputs {
            now,
            started_at: self.started_at,
            paused: self.state == RunState::Paused,
            soft: self.s.answers.usage_soft,
            hard: self.s.answers.usage_hard,
            guard: self.s.answers.usage_guard,
            tool: self.tool,
            limits: &self.guard,
            billing: self.billing,
            api_usd: self.cost.api_usd,
            cost_cap: self.s.answers.max_cost_usd,
            started: self.started_ids.len() as u32,
            max_tasks: self.s.answers.max_tasks,
            max_hours: self.s.answers.max_hours,
            failures: self.failures,
        });
        if decision.unknown() {
            self.unknown_since.get_or_insert(now);
        } else {
            self.unknown_since = None;
        }
        let facts = Facts {
            live: self.view.live.len(),
            awaiting_publish: self.view.awaiting,
            eligible: self.view.eligible,
            settled: self.settled,
            resume_after_reset: self.s.answers.resume_after_reset,
            unknown_for: self.unknown_since.map(|t| now - t),
            hard: self.s.answers.usage_hard,
            limit_paused: self.limit_paused.clone(),
        };
        self.blockers = decision
            .blockers
            .iter()
            .map(|b| match b {
                Blocker::Paused => "paused".to_string(),
                Blocker::Limit(h) => format!("usage limit: {}", limit_text(h)),
                Blocker::UsageUnknown => "no fresh usage numbers".into(),
                Blocker::CostCap => "cost cap reached".into(),
                Blocker::MaxTasks => "task limit reached".into(),
                Blocker::Hours => "time limit reached".into(),
                Blocker::Failures => "3 failures in a row".into(),
            })
            .collect();
        Ok((decision, facts))
    }

    fn note_decision(&mut self, d: &Decision) {
        match d.limit_hit().cloned() {
            Some(hit) => {
                let hard = f64::from(self.s.answers.usage_hard) <= hit.pct;
                self.note_limit(&hit, hard);
            }
            None => self.notified_limit = None,
        }
    }

    fn note_limit(&mut self, hit: &Hit, hard: bool) {
        let key = (hit.window.clone(), hard);
        if self.notified_limit.as_ref() == Some(&key) {
            return;
        }
        self.notified_limit = Some(key);
        let text = if hard {
            format!(
                "Hard usage limit: {}. Running tasks are paused.",
                limit_text(hit)
            )
        } else {
            format!("Usage limit: {}. No new tasks will start.", limit_text(hit))
        };
        self.event("limit", None, text.clone());
        self.push("Autopilot: usage limit", &text);
    }

    fn observe(&mut self, st: &State, now: DateTime<Utc>) {
        let mut view = View::default();
        let skips = skipped_ids(st);
        let sel: Vec<i64> = self.s.tasks.iter().copied().collect();
        view.counts.total = sel.len() as u32;
        for id in sel {
            let p = latest(st, id);
            if let Some(p) = p {
                self.note_start(p);
                self.track_run(p, now);
            }
            let class = p.map(|p| classify(p, &self.killed));
            match (p, class) {
                (Some(p), Some(Class::Done)) => {
                    view.counts.done_pr += 1;
                    self.note_pr(p);
                }
                (Some(p), Some(Class::Paused)) => {
                    view.paused += 1;
                    self.note_pr(p);
                }
                (Some(p), Some(Class::Held)) => {
                    view.counts.held += 1;
                    self.note_held(p);
                }
                (Some(p), Some(Class::Failed(reason))) => {
                    view.counts.failed += 1;
                    self.note_failure(p, &reason);
                }
                (Some(p), Some(Class::Taken)) => {
                    view.counts.skipped += 1;
                    self.note_taken(p);
                }
                (Some(p), Some(Class::InWork)) => {
                    view.counts.in_work += 1;
                    self.add_active(&mut view, p);
                }
                (_, class) => {
                    let sid = self.remote.task_status.get(&id).copied();
                    if sid.is_some() && sid == self.remote.feedback {
                        view.counts.needs_feedback += 1;
                    } else if sid.is_some() && sid == self.remote.blocked {
                        view.counts.blocked += 1;
                    } else if class == Some(Class::Approved) {
                        view.counts.queued += 1;
                        view.eligible += 1;
                    } else if class == Some(Class::Pending) {
                        view.counts.queued += 1;
                    } else if skips.contains(&id) || self.settled {
                        view.counts.skipped += 1;
                        self.note_skip(id, st);
                    } else {
                        view.counts.queued += 1;
                    }
                }
            }
        }
        view.awaiting = st
            .pickups
            .iter()
            .filter(|p| self.s.tasks.contains(&p.task.task_id) && p.awaits_publish())
            .count();
        view.live = st
            .pickups
            .iter()
            .filter(|p| {
                self.s.tasks.contains(&p.task.task_id) && p.occupies() && p.run_id.is_some()
            })
            .map(|p| Live {
                pickup: p.id.clone(),
                run: p.run_id.clone().unwrap_or_default(),
                task_id: p.task.task_id,
                key: p.key.clone(),
            })
            .collect();
        view.needs_runner = st.pickups.iter().any(|p| p.occupies());
        self.view = view;
    }

    fn add_active(&self, view: &mut View, p: &Pickup) {
        let step = if p.awaits_publish() || (p.run_exited && p.status == Status::Started) {
            "publishing"
        } else if p.status == Status::Started {
            "running"
        } else {
            "starting"
        };
        view.runs.push(ActiveRun {
            task: p.key.clone(),
            task_id: p.task.task_id,
            title: p.bundle.title.clone(),
            mode: self.s.mode.clone(),
            started_at: self.run_times.get(&p.id).map(|(w, _)| w.started_at),
            repos: p
                .cloud
                .as_ref()
                .map(|c| c.repos.iter().map(|r| r.full_name.clone()).collect())
                .unwrap_or_default(),
            step: step.into(),
        });
    }

    fn track_run(&mut self, p: &Pickup, now: DateTime<Utc>) {
        let Some(run) = p.run_id.clone() else { return };
        let entry = self.run_times.entry(p.id.clone()).or_insert_with(|| {
            (
                RunWindow {
                    run_ref: run.clone(),
                    task_ref: p.key.clone(),
                    workspace: p
                        .cloud
                        .as_ref()
                        .map(|c| c.workspace_code.clone())
                        .unwrap_or_default(),
                    session_id: None,
                    branch: p.cloud.as_ref().map(|c| c.branch.clone()),
                    project_hash: None,
                    started_at: now,
                    ended_at: None,
                },
                false,
            )
        });
        if p.run_exited && !entry.1 {
            entry.0.ended_at = Some(now);
            entry.1 = true;
        }
    }

    fn note_start(&mut self, p: &Pickup) {
        let began = p.run_id.is_some() || p.status == Status::UnknownStart;
        if began && !self.baseline.contains(&p.id) && self.started_ids.insert(p.id.clone()) {
            let text = format!("Picked {}: {}", p.key, p.bundle.title);
            self.event("picked", Some(&p.key.clone()), text.clone());
            self.push("Autopilot: task picked", &text);
        }
    }

    fn note_pr(&mut self, p: &Pickup) {
        let Some(url) = p.pr_url.clone() else {
            return;
        };
        if !self.counted_pr.insert(p.id.clone()) {
            return;
        }
        let draft = p.paused_draft;
        self.prs.push(PrLink {
            task: p.key.clone(),
            url: url.clone(),
            draft,
        });
        if !draft {
            self.failures = 0;
        }
        let text = format!(
            "{} {}: {url}",
            if draft {
                "Draft PR for"
            } else {
                "PR opened for"
            },
            p.key
        );
        self.event("pr", Some(&p.key.clone()), text.clone());
        self.push("Autopilot: pull request", &text);
    }

    fn note_held(&mut self, p: &Pickup) {
        if !self.counted_held.insert(p.id.clone()) {
            return;
        }
        let reason = p
            .held
            .as_ref()
            .map(|h| format!("the leak gate held the push ({})", h.categories.join(", ")))
            .unwrap_or_default();
        self.held_log.push(Reasoned {
            task: p.key.clone(),
            reason: reason.clone(),
        });
        let text = format!("{} held: {reason}", p.key);
        self.event("held", Some(&p.key.clone()), text.clone());
        self.push("Autopilot: held", &text);
    }

    fn note_failure(&mut self, p: &Pickup, reason: &str) {
        if !self.counted_fail.insert(p.id.clone()) {
            return;
        }
        self.failures += 1;
        self.failed_log.push(Reasoned {
            task: p.key.clone(),
            reason: reason.to_string(),
        });
        let text = format!("{} failed: {reason}", p.key);
        self.event("failed", Some(&p.key.clone()), text);
    }

    fn note_taken(&mut self, p: &Pickup) {
        if !self.noted_skips.insert(p.task.task_id) {
            return;
        }
        let text = format!("skipped {}: taken by someone else", p.key);
        self.event("skipped", Some(&p.key.clone()), text.clone());
        self.push("Autopilot: task taken", &text);
    }

    fn note_skip(&mut self, id: i64, st: &State) {
        if !self.noted_skips.insert(id) {
            return;
        }
        let key = latest(st, id).map_or_else(|| format!("#{id}"), |p| p.key.clone());
        let reason = skip_reason(st, id);
        let text = if reason.is_empty() {
            format!("{key} skipped")
        } else {
            format!("{key} skipped: {reason}")
        };
        self.event("skipped", Some(&key), text);
    }

    fn refresh_usage(&mut self, now: DateTime<Utc>) {
        let unknown_row = self.guard.iter().any(|l| l.used_pct.is_none());
        let due = match self.last_probe {
            None => true,
            Some(t) => {
                now - t >= self.t.probe_every
                    || (unknown_row && now - t >= Duration::seconds(MIN_PROBE_GAP_SECONDS))
            }
        };
        if due {
            self.last_probe = Some(now);
            if let Some(tool) = self.tool {
                match self.p.usage.probe(tool) {
                    Ok(p) => {
                        self.probes = p.windows;
                        self.billing = p.billing;
                        self.probe_error = None;
                    }
                    Err(e) => {
                        if self.probe_error.as_deref() != Some(e.message.as_str()) {
                            let text = format!("Usage numbers unavailable: {}", e.message);
                            self.event("usage", None, text);
                        }
                        self.probe_error = Some(e.message);
                    }
                }
            }
        }
        let mut snaps = self.probes.clone();
        snaps.extend(self.p.usage.stored(now));
        self.guard = guard_limits(&snaps, now, self.t.usage_max_age);
        if self.last_scan.is_none_or(|t| now - t >= self.t.scan_every) {
            self.last_scan = Some(now);
            self.p.usage.scan(now);
            self.refresh_cost(now);
        }
    }

    fn refresh_cost(&mut self, now: DateTime<Utc>) {
        let windows: Vec<RunWindow> = self.run_times.values().map(|(w, _)| w.clone()).collect();
        let summary = self
            .p
            .usage
            .cost(self.started_at, now, &windows, self.billing);
        self.cost = Cost {
            subscription_usd: 0.0,
            subscription_tokens: summary.as_ref().map_or(0, |s| s.subscription_tokens),
            api_usd: summary.as_ref().and_then(|s| s.api_usd),
            api_cap_usd: self.s.answers.max_cost_usd,
            billing: match self.billing {
                Billing::Subscription => "subscription",
                Billing::ApiKey => "api_key",
                Billing::Unknown => "unknown",
            }
            .into(),
            basis: summary.map_or_else(String::new, |s| s.basis.to_string()),
        };
    }

    fn refresh_remote(&mut self, now: DateTime<Utc>, st: &State) {
        if self
            .remote
            .at
            .is_some_and(|t| now - t < self.t.remote_every)
        {
            return;
        }
        self.remote.at = Some(now);
        let result = self.load_remote(st);
        match result {
            Ok(()) => self.remote.last_error = None,
            Err(e) => {
                if self.remote.last_error.as_deref() != Some(e.message.as_str()) {
                    let text = format!("Could not refresh task statuses: {}", e.message);
                    self.event("error", None, text);
                }
                self.remote.last_error = Some(e.message);
            }
        }
    }

    fn load_remote(&mut self, st: &State) -> Result<()> {
        if self.remote.ws.is_none() {
            let code = self
                .p
                .queue
                .workspace_code(&self.s.rule)
                .ok_or_else(|| Error::not_found("the rule has no workspace"))?;
            self.remote.ws = Some(self.p.remote.workspace_id(&code)?);
        }
        let ws = self.remote.ws.unwrap_or_default();
        let statuses = self.p.remote.statuses(ws)?;
        let find = |name: &str| {
            statuses
                .iter()
                .find(|(_, n)| n.eq_ignore_ascii_case(name.trim()))
                .map(|(id, _)| *id)
        };
        self.remote.feedback = find(&self.s.answers.feedback_status);
        self.remote.blocked = find(&self.s.answers.blocked_status);
        let open: Vec<i64> = self
            .s
            .tasks
            .iter()
            .copied()
            .filter(|id| {
                latest(st, *id).is_none_or(|p| {
                    matches!(
                        classify(p, &self.killed),
                        Class::Approved | Class::Pending | Class::Gone
                    )
                })
            })
            .take(MAX_STATUS_LOOKUPS)
            .collect();
        self.remote.task_status.clear();
        for id in open {
            if let Some(sid) = self.p.remote.task_status_id(id)? {
                self.remote.task_status.insert(id, sid);
            }
        }
        let n = self.p.queue.unconfirmed(&self.s.rule, &self.s.tasks)?;
        if n > self.remote.unconfirmed {
            let text = format!("{n} new task{} need confirm", if n == 1 { "" } else { "s" });
            self.event("confirm", None, text.clone());
            self.push("Autopilot: new tasks", &text);
        }
        self.remote.unconfirmed = n;
        Ok(())
    }

    fn wait_dead(&self, runs: &[Live], within: Duration) -> Vec<Live> {
        let end = self.now() + within;
        let mut left: Vec<Live> = runs.to_vec();
        loop {
            left.retain(|l| self.p.runs.alive(&l.run));
            if left.is_empty() || self.now() >= end {
                return left;
            }
            self.p.clock.sleep(self.t.poll, &|| false);
        }
    }

    fn drain(&mut self, why: &Why) {
        let st = match self.p.queue.state() {
            Ok(st) => st,
            Err(e) => {
                let text = format!("Could not read the queue: {}", e.message);
                self.event("error", None, text);
                return;
            }
        };
        let live: Vec<Live> = self.view.live.clone();
        if live.is_empty() {
            return;
        }
        let text = format!("Stopping {} running task(s): {}", live.len(), why.kill);
        self.event("stopping", None, text);
        let was_paused = st.paused;
        if !was_paused {
            if let Err(e) = self.p.queue.set_paused(true) {
                let text = format!("Could not pause the queue: {}", e.message);
                self.event("error", None, text);
            }
        }
        for l in &live {
            self.killed.insert(l.pickup.clone());
            if let Err(e) = self.p.runs.kill(&l.run, "int", &why.kill) {
                let text = format!("Could not interrupt {}: {}", l.key, e.message);
                self.event("error", Some(&l.key), text);
            }
        }
        let mut left = self.wait_dead(&live, self.t.int_wait);
        for l in &left {
            if let Err(e) = self.p.runs.kill(&l.run, "term", &why.kill) {
                let text = format!("Could not stop {}: {}", l.key, e.message);
                self.event("error", Some(&l.key), text);
            }
        }
        if !left.is_empty() {
            left = self.wait_dead(&left, self.t.term_wait);
        }
        for l in &left {
            let text = format!("{} did not exit", l.key);
            self.event("error", Some(&l.key), text);
        }
        if let Err(e) = self.p.queue.pass(StartPolicy {
            starts_allowed: false,
            ..StartPolicy::default()
        }) {
            let text = format!("Queue pass failed: {}", e.message);
            self.event("error", None, text);
        }
        let st = self.p.queue.state().ok();
        for l in live
            .iter()
            .filter(|l| !left.iter().any(|x| x.pickup == l.pickup))
        {
            let ready = st
                .as_ref()
                .and_then(|s| s.pickups.iter().find(|p| p.id == l.pickup))
                .is_some_and(|p| {
                    p.status == Status::Started
                        && p.run_exited
                        && !p.paused_draft
                        && p.pr_url.is_none()
                        && p.held.is_none()
                });
            if !ready {
                continue;
            }
            self.pause_one(l, why);
        }
        if !was_paused {
            if let Err(e) = self.p.queue.set_paused(false) {
                let text = format!("Could not resume the queue: {}", e.message);
                self.event("error", None, text);
            }
        }
    }

    fn pause_one(&mut self, l: &Live, why: &Why) {
        let mut comment = why.comment.clone();
        let mut detail = String::new();
        match self.p.queue.pause_to_draft(&l.pickup, &why.note) {
            Ok(d) => match d.pr_url {
                Some(url) => {
                    comment = format!("{comment}\nDraft PR: {url}");
                    detail = format!("draft PR {url}");
                    if self.counted_pr.insert(l.pickup.clone()) {
                        self.prs.push(PrLink {
                            task: l.key.clone(),
                            url: url.clone(),
                            draft: true,
                        });
                    }
                }
                None if d.held => {
                    comment = format!("{comment}\nThe draft PR was held by the leak gate.");
                    detail = "held by the leak gate".into();
                }
                None => {}
            },
            Err(e) => {
                let msg = sanitize_line(&e.message);
                comment = format!("{comment}\nNo draft PR: {msg}");
                detail = format!("no draft PR: {msg}");
            }
        }
        if let Err(e) = self.p.remote.comment(l.task_id, &comment) {
            let text = format!("Could not comment on {}: {}", l.key, e.message);
            self.event("error", Some(&l.key), text);
        }
        let reason = if detail.is_empty() {
            why.note.clone()
        } else {
            format!("{} ({detail})", why.note)
        };
        self.paused_log.push(Reasoned {
            task: l.key.clone(),
            reason: reason.clone(),
        });
        let text = format!("{} paused: {reason}", l.key);
        self.event("paused", Some(&l.key.clone()), text.clone());
        self.push("Autopilot: task paused", &text);
    }

    fn finish(&mut self, reason: StopReason, drain: Option<Why>) -> Result<Option<Outcome>> {
        self.state = RunState::Stopping;
        self.stop_reason = Some(reason.text());
        let text = format!("Stopping: {}", reason.text());
        self.event("stopping", None, text);
        self.write_status();
        let now = self.now();
        if let Ok(st) = self.p.queue.state() {
            self.observe(&st, now);
        }
        if let Some(why) = drain {
            self.drain(&why);
        }
        if let Err(e) = self.p.queue.pass(StartPolicy {
            starts_allowed: false,
            ..StartPolicy::default()
        }) {
            let text = format!("Final queue pass failed: {}", e.message);
            self.event("error", None, text);
        }
        let now = self.now();
        if let Ok(st) = self.p.queue.state() {
            self.observe(&st, now);
            self.summary = Some(self.build_summary(&st, &reason));
        }
        if let Err(e) = self
            .p
            .queue
            .disable(&format!("autopilot: {}", reason.text()))
        {
            let text = format!("Could not turn the queue off: {}", e.message);
            self.event("error", None, text);
        }
        let summary = self.summary.clone().unwrap_or_else(|| Summary {
            text: reason.text(),
            stopped_by: reason.text(),
            ..Summary::default()
        });
        self.state = RunState::Stopped;
        let text = format!("Stopped: {}", summary.text);
        self.event("stopped", None, text.clone());
        self.write_status();
        self.push("Autopilot stopped", &text);
        Ok(Some(Outcome {
            reason: reason.text(),
            summary,
        }))
    }

    fn build_summary(&self, st: &State, reason: &StopReason) -> Summary {
        let mut skipped = Vec::new();
        for id in &self.s.tasks {
            let class = latest(st, *id).map(|p| classify(p, &self.killed));
            if matches!(
                class,
                None | Some(Class::Gone)
                    | Some(Class::Taken)
                    | Some(Class::Pending)
                    | Some(Class::Approved)
            ) {
                let key = latest(st, *id).map_or_else(|| format!("#{id}"), |p| p.key.clone());
                let why = match class {
                    Some(Class::Pending) => "waiting for approval".to_string(),
                    Some(Class::Approved) => "not started".to_string(),
                    Some(Class::Taken) => "taken by someone else".to_string(),
                    _ => {
                        let r = skip_reason(st, *id);
                        if r.is_empty() {
                            "no longer matches the rule".into()
                        } else {
                            r
                        }
                    }
                };
                skipped.push(Reasoned {
                    task: key,
                    reason: why,
                });
            }
        }
        let drafts = self.prs.iter().filter(|p| p.draft).count();
        let real = self.prs.len() - drafts;
        let mut parts = vec![format!("{real} PR{}", if real == 1 { "" } else { "s" })];
        for (n, label) in [
            (drafts, "draft"),
            (self.held_log.len(), "held"),
            (self.failed_log.len(), "failed"),
            (skipped.len(), "skipped"),
        ] {
            if n > 0 {
                parts.push(format!("{n} {label}"));
            }
        }
        Summary {
            text: format!("{} - stopped: {}", parts.join(", "), reason.text()),
            stopped_by: reason.text(),
            started: self.started_ids.len() as u32,
            prs: self.prs.clone(),
            held: self.held_log.clone(),
            skipped,
            failed: self.failed_log.clone(),
            paused: self.paused_log.clone(),
            unconfirmed_new: self.remote.unconfirmed,
        }
    }

    fn limit_rows(&self) -> Vec<LimitRow> {
        self.guard
            .iter()
            .map(|l| {
                let guarded = Some(l.tool) == self.tool;
                LimitRow {
                    tool: l.tool.as_str().into(),
                    window: l.window.clone(),
                    used_pct: l.used_pct,
                    resets_at: l.resets_at,
                    source: l.source.as_str().into(),
                    origin: Some(l.origin.into()),
                    observed_at: l.observed_at,
                    stop_new_at: guarded.then_some(self.s.answers.usage_soft),
                    pause_at: guarded.then_some(self.s.answers.usage_hard),
                }
            })
            .collect()
    }

    fn write_status(&mut self) {
        let now = self.now();
        let mut extra = serde_json::Map::new();
        extra.insert("paused_tasks".into(), json!(self.view.paused));
        extra.insert("blockers".into(), json!(self.blockers));
        extra.insert(
            "pushes".into(),
            match self.pushes {
                Some(on) => json!(on),
                None => Value::Null,
            },
        );
        extra.insert("agent".into(), json!(self.s.agent));
        if let Some(d) = deadline(self.started_at, self.s.answers.max_hours) {
            extra.insert("deadline".into(), json!(d));
        }
        if let Some(s) = &self.summary {
            if let Ok(v) = serde_json::to_value(s) {
                extra.insert("summary".into(), v);
            }
        }
        let ns = next_stop(
            &self.guard_for_tool(),
            self.s.answers.usage_soft,
            self.s.answers.usage_hard,
        );
        let doc = StatusDoc {
            version: STATUS_VERSION,
            state: self.state,
            pid: std::process::id(),
            control_seq: self.last_seq,
            settings_id: self.s.id.clone(),
            started_at: self.started_at,
            updated_at: now,
            rule: self.s.rule.clone(),
            mode: self.s.mode.clone(),
            stop_reason: self.stop_reason.clone(),
            stopped_at: (self.state == RunState::Stopped).then_some(now),
            settings: self.s.answers.clone(),
            tasks: self.view.counts.clone(),
            teams: Slots {
                running: self.view.live.len() as u32,
                max: self.s.answers.max_concurrent,
            },
            agents: Slots {
                running: self.view.live.len() as u32,
                max: self.s.answers.max_concurrent,
            },
            runs: self.view.runs.clone(),
            limits: self.limit_rows(),
            cost: self.cost.clone(),
            next_stop: ns.map(|n| NextStop {
                window: n.window,
                pct_left: n.pct_left,
                kind: n.kind.into(),
            }),
            unconfirmed_new: self.remote.unconfirmed,
            failures_in_row: self.failures,
            prs: self.prs.clone(),
            events: self.events.clone(),
            extra,
        };
        if let Err(e) = self.store.write_status(&doc) {
            eprintln!("autopilot: status not written: {}", e.message);
        }
    }

    fn guard_for_tool(&self) -> Vec<GuardLimit> {
        self.guard
            .iter()
            .filter(|l| Some(l.tool) == self.tool)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests;
