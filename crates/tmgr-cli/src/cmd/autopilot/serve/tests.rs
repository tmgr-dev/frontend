use super::ports::{
    Clock, Notifier, PlanFresh, PlanInputs, Planned, Planner, Ports, Probed, Queue, Remote,
    RunControl, Timing, Usage as UsagePort,
};
use super::*;
use crate::cmd::autopilot::model::{Answers, LoopKind, QUEUE_SETTINGS_ID};
use crate::cmd::autopilot::orchestrate::{OrchInput, Poll, TaskText};
use chrono::TimeZone;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use tmgr_core::error::Error;
use tmgr_queue::state::Held;
use tmgr_queue::{PassReport, PausedDraft, Requeued};
use tmgr_usage::cost::CostSummary;
use tmgr_usage::limits::{LimitSnapshot, LimitSource, LimitWindow};

struct World {
    state: State,
    now: DateTime<Utc>,
    passes: Vec<StartPolicy>,
    kills: Vec<(String, String, String)>,
    alive: BTreeSet<String>,
    dies_on: &'static str,
    clean_exit: BTreeSet<String>,
    comments: Vec<(i64, String)>,
    drafts: Vec<(String, String)>,
    disabled: Vec<String>,
    pushes: Vec<String>,
    order: Vec<String>,
    published: Vec<String>,
    probe: Result<Vec<(f64, f64)>>,
    probes: u32,
    unconfirmed: u32,
    task_status: BTreeMap<i64, u64>,
    start_on_pass: Vec<(String, i64)>,
    enabled: bool,
    draft_error: bool,
    finish_on_sleep: bool,
    state_fail: bool,
    requeues: Vec<(String, StartPolicy)>,
    requeue_fails: BTreeSet<String>,
    resumable_fails: bool,
    orch_on: bool,
    orch_poll: Poll,
    orch_outputs: BTreeMap<u32, Vec<u8>>,
    orch_starts: Vec<(String, OrchInput)>,
    orch_kills: Vec<String>,
    unsigned: BTreeSet<i64>,
    changed: BTreeSet<i64>,
    taken: BTreeSet<i64>,
    moved: BTreeSet<i64>,
    statuses: Vec<(u64, String, String)>,
    status_sets: Vec<(i64, u64)>,
    input_failed: BTreeSet<i64>,
    plan_keys: BTreeMap<i64, String>,
    status_calls: u32,
    statuses_fail_from: Option<u32>,
    crash_on_set_status: bool,
}

struct Fake(RefCell<World>);

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 19, 12, 0, 0).unwrap()
}

impl Fake {
    fn new() -> Fake {
        Fake(RefCell::new(World {
            state: State::default(),
            now: t0(),
            passes: Vec::new(),
            kills: Vec::new(),
            alive: BTreeSet::new(),
            dies_on: "int",
            clean_exit: BTreeSet::new(),
            comments: Vec::new(),
            drafts: Vec::new(),
            disabled: Vec::new(),
            pushes: Vec::new(),
            order: Vec::new(),
            published: Vec::new(),
            probe: Ok(vec![(10.0, 10.0)]),
            probes: 0,
            unconfirmed: 0,
            task_status: BTreeMap::new(),
            start_on_pass: Vec::new(),
            enabled: true,
            draft_error: false,
            finish_on_sleep: false,
            state_fail: false,
            requeues: Vec::new(),
            requeue_fails: BTreeSet::new(),
            resumable_fails: false,
            orch_on: false,
            orch_poll: Poll::Running,
            orch_outputs: BTreeMap::new(),
            orch_starts: Vec::new(),
            orch_kills: Vec::new(),
            unsigned: BTreeSet::new(),
            changed: BTreeSet::new(),
            taken: BTreeSet::new(),
            moved: BTreeSet::new(),
            statuses: vec![
                (1, "Needs feedback".into(), "active".into()),
                (2, "blocked".into(), "active".into()),
            ],
            status_sets: Vec::new(),
            input_failed: BTreeSet::new(),
            plan_keys: BTreeMap::new(),
            status_calls: 0,
            statuses_fail_from: None,
            crash_on_set_status: false,
        }))
    }

    fn ports(&self) -> Ports<'_> {
        Ports {
            queue: self,
            runs: self,
            remote: self,
            planner: self,
            usage: self,
            notify: self,
            clock: self,
        }
    }

    fn w(&self) -> std::cell::RefMut<'_, World> {
        self.0.borrow_mut()
    }

    fn add(&self, p: Pickup) {
        let mut w = self.w();
        if let Some(run) = &p.run_id {
            if !p.run_exited {
                w.alive.insert(run.clone());
            }
        }
        w.state.pickups.push(p);
    }

    fn advance(&self, d: Duration) {
        self.w().now += d;
    }

    fn order(&self) -> Vec<String> {
        self.0.borrow().order.clone()
    }
}

fn pickup(id: &str, task: i64, status: &str, run: Option<&str>) -> Pickup {
    let mut v = json!({
        "id": id, "rule": "r", "task": {"workspace_id": 7, "task_id": task}, "key": format!("TM-{task}"),
        "status": status,
        "bundle": {"v": 1, "task": {"workspace_id": 7, "task_id": task}, "title": format!("Task {task}"), "body": "b",
            "author": {"kind": "human"}, "comments": [], "pages": [],
            "launch": {"profile": "claude-code", "cwd": "/work/q"}},
        "bundle_sha256": "x", "context_tainted": false,
        "created_at": "2026-10-19T12:00:00Z", "updated_at": "2026-10-19T00:00:00Z",
        "cloud": {
            "claim": {"host": "api.tmgr.dev", "user_id": 1, "workspace_id": 7, "task_id": task,
                "category_id": 1, "content_sha256": "h", "key_id": "k", "signed_at": 1,
                "signature": "s", "kind": "confirm"},
            "workspace_code": "ws", "branch": format!("tmgr/tm-{task}")
        }
    });
    if let Some(run) = run {
        v["run_id"] = json!(run);
    }
    serde_json::from_value(v).unwrap()
}

fn running(id: &str, task: i64) -> Pickup {
    pickup(id, task, "started", Some(&format!("run-{id}")))
}

impl Queue for Fake {
    fn pass(&self, policy: StartPolicy) -> Result<PassReport> {
        let mut w = self.w();
        let paused = w.state.paused;
        w.order.push(format!(
            "pass paused={paused} starts={}",
            policy.starts_allowed
        ));
        w.passes.push(policy.clone());
        let World {
            state,
            alive,
            clean_exit,
            ..
        } = &mut *w;
        for p in state.pickups.iter_mut() {
            if p.status == Status::Started && !p.run_exited {
                if let Some(run) = &p.run_id {
                    if !alive.contains(run) {
                        p.run_exited = true;
                        p.run_clean = clean_exit.contains(run);
                    }
                }
            }
        }
        let mut report = PassReport {
            enabled: w.enabled,
            paused,
            ..PassReport::default()
        };
        if paused {
            return Ok(report);
        }
        let mut publish = Vec::new();
        for p in w.state.pickups.iter_mut() {
            if p.awaits_publish() {
                p.pr_url = Some(format!("https://github.com/o/r/pull/{}", p.task.task_id));
                publish.push(p.id.clone());
            }
        }
        w.published.extend(publish);
        let start = std::mem::take(&mut w.start_on_pass);
        let mut left = Vec::new();
        for (id, task) in start {
            if policy.starts_allowed {
                if let Some(p) = w.state.pickups.iter_mut().find(|p| p.id == id) {
                    p.status = Status::Started;
                    p.run_id = Some(format!("run-{id}"));
                    w.alive.insert(format!("run-{id}"));
                    let _ = task;
                }
            } else {
                left.push((id, task));
            }
        }
        w.start_on_pass = left;
        report.events.push(json!({"outcome": "noop"}));
        Ok(report)
    }

    fn state(&self) -> Result<State> {
        let w = self.0.borrow();
        if w.state_fail {
            return Err(Error::internal("state file unreadable"));
        }
        let text = serde_json::to_string(&w.state).unwrap();
        Ok(serde_json::from_str(&text).unwrap())
    }

    fn pause_to_draft(&self, pickup: &str, note: &str) -> Result<PausedDraft> {
        let mut w = self.w();
        w.order.push(format!("pause_to_draft {pickup}"));
        w.drafts.push((pickup.into(), note.into()));
        let err = w.draft_error;
        let p = w.state.pickups.iter_mut().find(|p| p.id == pickup).unwrap();
        p.paused_draft = true;
        if err {
            return Err(Error::conflict("nothing to commit"));
        }
        p.pr_url = Some(format!("https://github.com/o/r/pull/{}", p.task.task_id));
        p.wip_heads.insert("repo".into(), "abc".into());
        Ok(PausedDraft {
            pr_url: p.pr_url.clone(),
            held: false,
            commits: vec![],
        })
    }

    fn requeue_paused(&self, pickup: &str, policy: StartPolicy) -> Result<Requeued> {
        let mut w = self.w();
        w.order.push(format!("requeue {pickup}"));
        w.requeues.push((pickup.into(), policy));
        if w.requeue_fails.contains(pickup) {
            return Err(Error::conflict("the task already has an open pickup"));
        }
        let old = w.state.pickups.iter().position(|p| p.id == pickup).unwrap();
        let mut next = w.state.pickups[old].clone();
        next.id = format!("{pickup}-again");
        next.status = Status::Approved;
        next.run_id = None;
        next.run_exited = false;
        next.paused_draft = false;
        next.pr_url = None;
        next.continues = Some(pickup.into());
        next.continued_by = None;
        w.state.pickups[old].continued_by = Some(next.id.clone());
        w.state.pickups[old].released = true;
        let out = Requeued {
            pickup: next.id.clone(),
            continues: pickup.into(),
            task_id: next.task.task_id,
            key: next.key.clone(),
        };
        w.start_on_pass.push((next.id.clone(), next.task.task_id));
        w.state.pickups.push(next);
        Ok(out)
    }

    fn resumable(&self) -> Result<Vec<String>> {
        if self.w().resumable_fails {
            return Err(Error::internal("the queue state is unreadable"));
        }
        Ok(tmgr_queue::paused_resumable(&self.w().state))
    }

    fn set_paused(&self, paused: bool) -> Result<()> {
        let mut w = self.w();
        w.order.push(format!("set_paused {paused}"));
        w.state.paused = paused;
        Ok(())
    }

    fn disable(&self, reason: &str) -> Result<()> {
        let mut w = self.w();
        w.order.push("disable".into());
        w.disabled.push(reason.into());
        w.enabled = false;
        Ok(())
    }

    fn workspace_code(&self, _rule: &str) -> Option<String> {
        Some("ws".into())
    }

    fn keep_on(&self) -> Result<()> {
        self.w().order.push("keep_on".into());
        Ok(())
    }

    fn unconfirmed(&self, _rule: &str, _selected: &BTreeSet<i64>) -> Result<u32> {
        Ok(self.0.borrow().unconfirmed)
    }
}

impl RunControl for Fake {
    fn kill(&self, run: &str, signal: &str, reason: &str) -> Result<()> {
        let mut w = self.w();
        w.order.push(format!("kill {signal} {run}"));
        w.kills.push((run.into(), signal.into(), reason.into()));
        if signal == w.dies_on {
            w.alive.remove(run);
        }
        Ok(())
    }

    fn alive(&self, run: &str) -> bool {
        self.0.borrow().alive.contains(run)
    }
}

impl Remote for Fake {
    fn comment(&self, task_id: i64, text: &str) -> Result<()> {
        let mut w = self.w();
        w.order.push(format!("comment {task_id}"));
        w.comments.push((task_id, text.into()));
        Ok(())
    }

    fn workspace_id(&self, _code: &str) -> Result<i64> {
        Ok(7)
    }

    fn statuses(&self, _ws: i64) -> Result<Vec<(u64, String, String)>> {
        let mut w = self.w();
        w.status_calls += 1;
        if w.statuses_fail_from.is_some_and(|n| w.status_calls >= n) {
            return Err(Error::new(tmgr_core::error::Kind::Offline, "no network"));
        }
        Ok(w.statuses.clone())
    }

    fn set_status(&self, task_id: i64, status_id: u64) -> Result<()> {
        let mut w = self.w();
        if w.crash_on_set_status {
            panic!("crash");
        }
        w.order.push(format!("set_status {task_id}"));
        w.status_sets.push((task_id, status_id));
        Ok(())
    }

    fn task_status_id(&self, task_id: i64) -> Result<Option<u64>> {
        Ok(self.0.borrow().task_status.get(&task_id).copied())
    }
}

impl Planner for Fake {
    fn enabled(&self, _rule: &str) -> bool {
        self.0.borrow().orch_on
    }

    fn inputs(&self, _rule: &str, tasks: &BTreeSet<i64>) -> Result<PlanInputs> {
        let w = self.0.borrow();
        let mut out = PlanInputs::default();
        for &id in tasks {
            if w.input_failed.contains(&id) {
                out.failed.push((id, "no network".into()));
                continue;
            }
            if w.unsigned.contains(&id) {
                out.not_planned
                    .push((id, format!("TM-{id}"), "needs confirm".into()));
                continue;
            }
            out.tasks.push(Planned {
                text: TaskText {
                    task_id: id,
                    key: w.plan_keys.get(&id).cloned().unwrap_or(format!("TM-{id}")),
                    title: format!("Task {id}"),
                    description: format!("Body {id}"),
                    repos: vec!["o/r".into()],
                },
                content_sha256: format!("sha{id}"),
                status_id: Some(9),
            });
        }
        Ok(out)
    }

    fn start(&self, _rule: &str, input: &OrchInput, nonce: &str) -> Result<(String, PathBuf)> {
        let mut w = self.w();
        w.order.push("orch_start".into());
        w.orch_starts.push((nonce.into(), input.clone()));
        let n = w.orch_starts.len();
        w.alive.insert(format!("orch-run-{n}"));
        Ok((
            format!("orch-run-{n}"),
            PathBuf::from(format!("/orch/orchestrator-{nonce}")),
        ))
    }

    fn poll(&self, _run_id: &str) -> Result<Poll> {
        Ok(self.0.borrow().orch_poll)
    }

    fn read(&self, dir: &Path) -> Result<Vec<u8>> {
        let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let attempt: u32 = name
            .trim_start_matches("orchestrator-")
            .split('-')
            .next()
            .and_then(|a| a.parse().ok())
            .unwrap_or(0);
        self.0
            .borrow()
            .orch_outputs
            .get(&attempt)
            .cloned()
            .ok_or_else(|| Error::conflict("plan file: the orchestrator wrote no plan"))
    }

    fn discard(&self, _dir: &Path) {}

    fn kill(&self, run_id: &str) -> Result<()> {
        let mut w = self.w();
        w.order.push(format!("orch_kill {run_id}"));
        w.orch_kills.push(run_id.into());
        w.alive.remove(run_id);
        Ok(())
    }

    fn fresh(&self, _rule: &str, task_id: i64) -> Result<PlanFresh> {
        let w = self.0.borrow();
        Ok(PlanFresh {
            content_sha256: if w.changed.contains(&task_id) {
                "other".into()
            } else {
                format!("sha{task_id}")
            },
            free: !w.taken.contains(&task_id),
            status_id: Some(if w.moved.contains(&task_id) { 8 } else { 9 }),
        })
    }
}

impl UsagePort for Fake {
    fn probe(&self, _tool: Tool) -> Result<Probed> {
        let mut w = self.w();
        w.probes += 1;
        let now = w.now;
        match &w.probe {
            Ok(rows) => Ok(Probed {
                billing: Billing::Subscription,
                windows: rows
                    .iter()
                    .flat_map(|(five, week)| {
                        [(LimitWindow::Session, *five), (LimitWindow::Weekly, *week)]
                    })
                    .map(|(window, pct)| LimitSnapshot {
                        tool: Tool::Claude,
                        account: "a".into(),
                        window,
                        used_pct: pct,
                        resets_at: Some(now + Duration::hours(2)),
                        plan: None,
                        observed_at: now,
                        source: LimitSource::Probe,
                        tool_session_id: String::new(),
                    })
                    .collect(),
            }),
            Err(e) => Err(Error::new(e.kind, e.message.clone())),
        }
    }

    fn stored(&self, _now: DateTime<Utc>) -> Vec<LimitSnapshot> {
        Vec::new()
    }

    fn scan(&self, _now: DateTime<Utc>) {}

    fn cost(
        &self,
        _from: DateTime<Utc>,
        _to: DateTime<Utc>,
        _windows: &[RunWindow],
        _billing: Billing,
    ) -> Option<CostSummary> {
        None
    }
}

impl Notifier for Fake {
    fn push(&self, title: &str, body: &str) -> bool {
        self.w().pushes.push(format!("{title}: {body}"));
        true
    }
}

impl Clock for Fake {
    fn now(&self) -> DateTime<Utc> {
        self.0.borrow().now
    }

    fn sleep(&self, d: Duration, _wake: &dyn Fn() -> bool) {
        let mut w = self.w();
        w.now += d;
        if w.finish_on_sleep {
            w.alive.clear();
        }
    }
}

fn settings(tasks: &[i64], tweak: impl Fn(&mut Answers)) -> Settings {
    let mut answers = Answers {
        rule: Some("r".into()),
        ..Answers::default()
    };
    tweak(&mut answers);
    Settings {
        id: "s1".into(),
        created_at: t0(),
        answers,
        rule: "r".into(),
        agent: "claude".into(),
        mode: "auto".into(),
        finish: crate::cmd::autopilot::model::Finish::PrOnly,
        tasks: tasks.iter().copied().collect(),
        signed: vec![],
        windows: vec!["five_hour".into(), "seven_day".into()],
        kind: LoopKind::Drain,
        resume_queue: false,
        order: vec![],
        dry_run: false,
        persona_optional: false,
    }
}

fn queue_settings() -> Settings {
    let mut s = settings(&[], |a| {
        a.usage_guard = false;
        a.rule = None;
    });
    s.id = QUEUE_SETTINGS_ID.into();
    s.rule = "all".into();
    s.windows = vec![];
    s.kind = LoopKind::Queue;
    s
}

fn timing() -> Timing {
    Timing {
        probe_every: Duration::minutes(5),
        ..Timing::default()
    }
}

struct Rig {
    dir: tempfile::TempDir,
    fake: Fake,
    term: AtomicBool,
}

impl Rig {
    fn new() -> Rig {
        Rig {
            dir: tempfile::tempdir().unwrap(),
            fake: Fake::new(),
            term: AtomicBool::new(false),
        }
    }

    fn store(&self) -> Store {
        Store::at(self.dir.path())
    }

    fn control(&self, seq: u64, command: &str) {
        let body = json!({"seq": seq, "command": command, "at": "2026-10-19T12:00:00Z"});
        let store = self.store();
        std::fs::create_dir_all(store.dir()).unwrap();
        std::fs::write(store.control_path(), body.to_string()).unwrap();
    }

    fn status(&self) -> StatusDoc {
        self.store().read_status().unwrap().unwrap()
    }

    fn log_kinds(&self) -> Vec<String> {
        std::fs::read_to_string(self.store().log_path())
            .unwrap_or_default()
            .lines()
            .map(|l| {
                serde_json::from_str::<Value>(l).unwrap()["kind"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect()
    }
}

fn tick_on(ap: &mut Autopilot) {
    if let Some(o) = ap.tick().unwrap() {
        panic!("stopped: {}", o.reason);
    }
}

fn with_ap<T>(rig: &Rig, s: &Settings, f: impl FnOnce(&mut Autopilot) -> T) -> T {
    let store = rig.store();
    let mut ap = Autopilot::new(&store, s, rig.fake.ports(), timing(), &rig.term);
    ap.begin();
    f(&mut ap)
}

#[test]
fn control_commands_are_handled_once_per_seq_and_echoed_in_the_status() {
    let rig = Rig::new();
    rig.fake.add(pickup("q1", 1, "approved", None));
    let s = settings(&[1], |_| {});
    with_ap(&rig, &s, |ap| {
        rig.control(1, "pause");
        tick_on(ap);
        assert_eq!(rig.status().state, RunState::Paused);
        assert_eq!(rig.status().control_seq, 1);
        assert!(!rig.fake.0.borrow().passes.last().unwrap().starts_allowed);

        let before = ap.events.len();
        tick_on(ap);
        assert_eq!(ap.events.iter().filter(|e| e.kind == "control").count(), 1);
        assert_eq!(before, ap.events.len());

        rig.control(2, "resume");
        tick_on(ap);
        assert_eq!(rig.status().state, RunState::Running);
        assert_eq!(rig.status().control_seq, 2);
        assert!(rig.fake.0.borrow().passes.last().unwrap().starts_allowed);

        rig.control(2, "stop");
        assert!(ap.tick().unwrap().is_none(), "an old seq is ignored");
    });
}

#[test]
fn stop_always_disables_the_queue_last_and_writes_the_summary() {
    let rig = Rig::new();
    rig.fake.add(running("q1", 1));
    rig.fake.add(pickup("q2", 2, "approved", None));
    let s = settings(&[1, 2], |_| {});
    let out = with_ap(&rig, &s, |ap| {
        rig.control(1, "stop");
        ap.tick().unwrap().expect("stopped")
    });
    assert_eq!(out.reason, "stopped by you");
    let order = rig.fake.order();
    assert_eq!(order.last().unwrap(), "disable");
    let kill = order.iter().position(|o| o == "kill int run-q1").unwrap();
    let draft = order.iter().position(|o| o == "pause_to_draft q1").unwrap();
    let disable = order.iter().position(|o| o == "disable").unwrap();
    assert!(kill < draft && draft < disable, "{order:?}");
    let w = rig.fake.0.borrow();
    assert_eq!(w.kills[0].2, "Paused: autopilot stopped");
    assert_eq!(
        w.comments,
        vec![(
            1,
            "Paused: autopilot stopped\nDraft PR: https://github.com/o/r/pull/1".to_string()
        )]
    );
    assert!(w.disabled[0].contains("stopped by you"));
    drop(w);
    let st = rig.status();
    assert_eq!(st.state, RunState::Stopped);
    assert_eq!(st.stop_reason.as_deref(), Some("stopped by you"));
    assert!(st.stopped_at.is_some());
    let summary: Summary = serde_json::from_value(st.extra["summary"].clone()).unwrap();
    assert_eq!(summary.prs.len(), 1);
    assert!(summary.prs[0].draft);
    assert_eq!(summary.paused.len(), 1);
    assert_eq!(summary.skipped[0].task, "TM-2");
    assert_eq!(summary.stopped_by, "stopped by you");
    let kinds = rig.log_kinds();
    assert_eq!(kinds.last().unwrap(), "stopped");
}

#[test]
fn sigterm_suspends_without_draining_or_turning_the_queue_off() {
    let rig = Rig::new();
    rig.fake.add(running("q1", 1));
    let s = settings(&[1], |_| {});
    let out = with_ap(&rig, &s, |ap| {
        rig.term.store(true, Ordering::SeqCst);
        ap.tick().unwrap().expect("suspended")
    });
    assert_eq!(out.reason, "suspended");
    let w = rig.fake.0.borrow();
    assert!(w.kills.is_empty() && w.disabled.is_empty() && w.drafts.is_empty());
    assert!(w.enabled);
    drop(w);
    let st = rig.status();
    assert_eq!(st.state, RunState::Suspended);
    assert_eq!(rig.log_kinds().last().unwrap(), "suspended");
}

#[test]
fn the_suspend_command_does_the_same_as_sigterm() {
    let rig = Rig::new();
    rig.fake.add(running("q1", 1));
    let s = settings(&[1], |_| {});
    let out = with_ap(&rig, &s, |ap| {
        rig.control(1, "suspend");
        ap.tick().unwrap().expect("suspended")
    });
    assert_eq!(out.reason, "suspended");
    assert!(rig.fake.0.borrow().kills.is_empty());
    assert_eq!(rig.status().state, RunState::Suspended);
    assert_eq!(rig.status().control_seq, 1);
}

#[test]
fn a_resumed_loop_keeps_the_start_time_events_pull_requests_and_counters() {
    let rig = Rig::new();
    rig.fake.add(pickup("q1", 1, "approved", None));
    let mut done = running("q2", 2);
    done.run_exited = true;
    done.run_clean = true;
    rig.fake.add(done);
    {
        let mut w = rig.fake.w();
        w.start_on_pass.push(("q1".into(), 1));
        w.alive.remove("run-q2");
    }
    let s = settings(&[1, 2], |_| {});
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_on(ap);
        assert_eq!(ap.started_ids.len(), 1);
        rig.term.store(true, Ordering::SeqCst);
        assert!(ap.tick().unwrap().is_some());
    });
    let first = rig.status();
    assert_eq!(first.state, RunState::Suspended);
    assert_eq!(first.prs.len(), 1);
    rig.term.store(false, Ordering::SeqCst);
    rig.fake.advance(Duration::minutes(7));
    let store = rig.store();
    let mut ap = Autopilot::new(&store, &s, rig.fake.ports(), timing(), &rig.term);
    ap.begin();
    assert_eq!(ap.started_at, first.started_at);
    assert_eq!(ap.started_ids.len(), 1);
    assert_eq!(ap.prs.len(), 1);
    assert!(ap.events.iter().any(|e| e.kind == "picked"));
    assert!(ap.events.iter().any(|e| e.kind == "resumed"));
    assert!(!ap
        .events
        .iter()
        .any(|e| e.kind == "started" && e.at > Some(t0())));
    let picked = ap.events.iter().filter(|e| e.kind == "picked").count();
    tick_on(&mut ap);
    assert_eq!(
        ap.events.iter().filter(|e| e.kind == "picked").count(),
        picked
    );
    assert_eq!(ap.prs.len(), 1);
    assert_eq!(rig.status().state, RunState::Running);
    assert_eq!(rig.status().started_at, first.started_at);
}

#[test]
fn a_pause_survives_a_suspend() {
    let rig = Rig::new();
    rig.fake.add(pickup("q1", 1, "approved", None));
    let s = settings(&[1], |_| {});
    with_ap(&rig, &s, |ap| {
        rig.control(1, "pause");
        tick_on(ap);
        rig.control(2, "suspend");
        assert!(ap.tick().unwrap().is_some());
    });
    assert_eq!(rig.status().state, RunState::Suspended);
    let store = rig.store();
    let _ = std::fs::remove_file(store.control_path());
    let mut ap = Autopilot::new(&store, &s, rig.fake.ports(), timing(), &rig.term);
    ap.begin();
    assert_eq!(ap.state, RunState::Paused);
    tick_on(&mut ap);
    assert!(!rig.fake.0.borrow().passes.last().unwrap().starts_allowed);
}

#[test]
fn what_happened_while_suspended_is_reported_after_the_resume() {
    let rig = Rig::new();
    rig.fake.add(running("q1", 1));
    let s = settings(&[1], |_| {});
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        rig.control(1, "suspend");
        assert!(ap.tick().unwrap().is_some());
    });
    {
        let mut w = rig.fake.w();
        w.alive.clear();
        w.clean_exit.insert("run-q1".into());
    }
    let store = rig.store();
    let _ = std::fs::remove_file(store.control_path());
    let mut ap = Autopilot::new(&store, &s, rig.fake.ports(), timing(), &rig.term);
    ap.begin();
    assert!(ap.prs.is_empty());
    let _ = ap.tick().unwrap();
    assert_eq!(ap.prs.len(), 1);
    assert!(rig.log_kinds().iter().any(|k| k == "pr"));
    assert!(rig
        .fake
        .0
        .borrow()
        .pushes
        .iter()
        .any(|p| p.starts_with("Autopilot: pull request")));
}

#[test]
fn a_failure_that_happened_while_suspended_counts_after_the_resume() {
    let rig = Rig::new();
    let s = settings(&[1, 2], |_| {});
    rig.fake.add(pickup("q2", 2, "approved", None));
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        rig.control(1, "suspend");
        assert!(ap.tick().unwrap().is_some());
    });
    let mut p = pickup("q1", 1, "failed", None);
    p.reason = Some("clone failed".into());
    rig.fake.add(p);
    let store = rig.store();
    let _ = std::fs::remove_file(store.control_path());
    let mut ap = Autopilot::new(&store, &s, rig.fake.ports(), timing(), &rig.term);
    ap.begin();
    tick_on(&mut ap);
    assert_eq!(ap.failures, 1);
    assert_eq!(ap.failed_log.len(), 1);
}

#[test]
fn a_queue_loop_left_running_by_a_crash_continues_from_its_status_but_a_drain_does_not() {
    let rig = Rig::new();
    let mut done = running("q1", 1);
    done.run_exited = true;
    done.run_clean = true;
    rig.fake.add(done);
    let q = queue_settings();
    let first = {
        let store = rig.store();
        let mut ap = Autopilot::new(&store, &q, rig.fake.ports(), timing(), &rig.term);
        ap.begin();
        tick_on(&mut ap);
        rig.status()
    };
    assert_eq!(first.state, RunState::Running);
    rig.fake.advance(Duration::minutes(3));
    let store = rig.store();
    let mut ap = Autopilot::new(&store, &q, rig.fake.ports(), timing(), &rig.term);
    ap.begin();
    assert_eq!(ap.started_at, first.started_at);
    assert!(ap.events.iter().any(|e| e.kind == "resumed"));
    let d = settings(&[1], |_| {});
    let mut ap = Autopilot::new(&store, &d, rig.fake.ports(), timing(), &rig.term);
    ap.begin();
    assert!(!ap.events.iter().any(|e| e.kind == "resumed"));
}

#[test]
fn a_drain_that_took_over_from_the_queue_loop_keeps_the_queue_on_and_says_so() {
    let rig = Rig::new();
    let mut s = settings(&[1], |_| {});
    s.resume_queue = true;
    rig.fake.add(pickup("q1", 1, "approved", None));
    let out = with_ap(&rig, &s, |ap| {
        rig.control(1, "stop");
        ap.tick().unwrap().expect("stopped")
    });
    assert_eq!(out.reason, "stopped by you");
    let w = rig.fake.0.borrow();
    assert!(w.disabled.is_empty());
    assert_eq!(w.order.last().unwrap(), "keep_on");
    drop(w);
    let st = rig.status();
    assert_eq!(st.state, RunState::Stopped);
    assert_eq!(st.extra["resume_queue"], true);
}

#[test]
fn a_stopped_status_or_another_settings_id_is_not_resumed() {
    let rig = Rig::new();
    let s = settings(&[1], |_| {});
    rig.fake.add(pickup("q1", 1, "approved", None));
    with_ap(&rig, &s, |ap| {
        rig.control(1, "stop");
        assert!(ap.tick().unwrap().is_some());
    });
    let stopped = rig.status();
    rig.fake.advance(Duration::minutes(3));
    let store = rig.store();
    let mut ap = Autopilot::new(&store, &s, rig.fake.ports(), timing(), &rig.term);
    ap.begin();
    assert_ne!(ap.started_at, stopped.started_at);
    assert!(!ap.events.iter().any(|e| e.kind == "resumed"));
}

fn tick_n(ap: &mut Autopilot, rig: &Rig, n: usize) {
    for _ in 0..n {
        tick_on(ap);
        rig.fake.advance(Duration::seconds(30));
    }
}

#[test]
fn the_queue_loop_keeps_ticking_when_nothing_is_left() {
    let rig = Rig::new();
    let mut done = running("q1", 1);
    done.run_exited = true;
    done.run_clean = true;
    rig.fake.add(done);
    let s = queue_settings();
    with_ap(&rig, &s, |ap| {
        tick_n(ap, &rig, 5);
        assert_eq!(rig.status().state, RunState::Running);
        assert!(rig.fake.0.borrow().disabled.is_empty());
        assert_eq!(rig.fake.0.borrow().passes.len(), 5);
    });
}

#[test]
fn the_queue_loop_hands_the_engine_a_default_policy_and_keeps_starting_after_failures() {
    let rig = Rig::new();
    for t in 1..=3 {
        let mut p = pickup(&format!("f{t}"), t, "failed", None);
        p.reason = Some("clone failed".into());
        rig.fake.add(p);
    }
    let s = queue_settings();
    with_ap(&rig, &s, |ap| {
        tick_n(ap, &rig, 3);
        let w = rig.fake.0.borrow();
        let policy = w.passes.last().unwrap();
        assert!(policy.starts_allowed);
        assert!(policy.only.is_none());
        assert!(policy.max_live.is_none());
        assert!(policy.max_new_starts.is_none());
    });
    assert_eq!(rig.status().tasks.failed, 3);
}

#[test]
fn the_queue_loop_follows_every_task_the_queue_picks_and_reports_each_step() {
    let rig = Rig::new();
    rig.fake.add(pickup("q9", 9, "approved", None));
    rig.fake.w().start_on_pass.push(("q9".into(), 9));
    let s = queue_settings();
    with_ap(&rig, &s, |ap| {
        tick_n(ap, &rig, 2);
        let st = rig.status();
        assert_eq!(st.tasks.total, 1);
        assert_eq!(st.tasks.in_work, 1);
        assert_eq!(st.extra["kind"], "queue");
        rig.fake.w().alive.clear();
        rig.fake.w().clean_exit.insert("run-q9".into());
        tick_n(ap, &rig, 2);
    });
    let kinds = rig.log_kinds();
    for kind in ["started", "picked", "running", "pr"] {
        assert!(kinds.iter().any(|k| k == kind), "{kind} in {kinds:?}");
    }
    assert_eq!(rig.status().tasks.done_pr, 1);
}

#[test]
fn a_step_is_reported_once_per_change() {
    let rig = Rig::new();
    rig.fake.add(pickup("q1", 1, "approved", None));
    rig.fake.w().start_on_pass.push(("q1".into(), 1));
    let s = queue_settings();
    with_ap(&rig, &s, |ap| tick_n(ap, &rig, 4));
    let running = rig.log_kinds().iter().filter(|k| *k == "running").count();
    assert_eq!(running, 1);
}

#[test]
fn a_clean_exit_without_a_pull_request_is_logged_as_done() {
    let rig = Rig::new();
    rig.fake.add(pickup("q1", 1, "approved", None));
    rig.fake.w().start_on_pass.push(("q1".into(), 1));
    let s = queue_settings();
    with_ap(&rig, &s, |ap| {
        tick_n(ap, &rig, 2);
        {
            let mut w = rig.fake.w();
            w.alive.clear();
            w.clean_exit.insert("run-q1".into());
            for p in w.state.pickups.iter_mut() {
                p.cloud = None;
            }
        }
        tick_n(ap, &rig, 2);
    });
    assert!(rig.log_kinds().iter().any(|k| k == "done"));
}

#[test]
fn the_queue_loop_ends_when_the_queue_is_turned_off_and_does_not_drain() {
    let rig = Rig::new();
    rig.fake.add(running("q1", 1));
    let s = queue_settings();
    let out = with_ap(&rig, &s, |ap| {
        tick_on(ap);
        rig.fake.w().enabled = false;
        ap.tick().unwrap().expect("ended")
    });
    assert_eq!(out.reason, "the queue was turned off");
    assert!(rig.fake.0.borrow().kills.is_empty());
    assert_eq!(rig.status().state, RunState::Stopped);
}

#[test]
fn the_stop_command_still_drains_and_turns_the_queue_off_in_queue_mode() {
    let rig = Rig::new();
    rig.fake.add(running("q1", 1));
    let s = queue_settings();
    let out = with_ap(&rig, &s, |ap| {
        rig.control(1, "stop");
        ap.tick().unwrap().expect("stopped")
    });
    assert_eq!(out.reason, "stopped by you");
    assert_eq!(rig.fake.order().last().unwrap(), "disable");
    assert_eq!(rig.fake.0.borrow().kills[0].1, "int");
}

#[test]
fn the_hard_limit_pauses_the_queue_kills_with_a_reason_drafts_comments_and_then_stops() {
    let rig = Rig::new();
    rig.fake.add(running("q1", 1));
    rig.fake.add(pickup("q2", 2, "approved", None));
    rig.fake.w().probe = Ok(vec![(85.0, 10.0)]);
    rig.fake.w().clean_exit.insert("run-q1".into());
    let s = settings(&[1, 2], |_| {});
    let out = with_ap(&rig, &s, |ap| ap.tick().unwrap().expect("stopped"));
    assert!(
        out.reason
            .starts_with("hard usage limit reached: five_hour at 85%"),
        "{}",
        out.reason
    );
    let order = rig.fake.order();
    let pos = |needle: &str| {
        order
            .iter()
            .position(|o| o == needle)
            .unwrap_or_else(|| panic!("{needle} in {order:?}"))
    };
    assert!(pos("set_paused true") < pos("kill int run-q1"));
    assert!(pos("kill int run-q1") < pos("pass paused=true starts=false"));
    assert!(pos("pass paused=true starts=false") < pos("pause_to_draft q1"));
    assert!(pos("pause_to_draft q1") < pos("comment 1"));
    assert!(pos("comment 1") < pos("set_paused false"));
    assert_eq!(order.last().unwrap(), "disable");
    let w = rig.fake.0.borrow();
    assert!(
        w.published.is_empty(),
        "a run that exited cleanly after the kill is never published as a normal PR"
    );
    assert_eq!(w.kills[0].2, "Paused: usage limit reached at 85%");
    assert_eq!(
        w.drafts,
        vec![("q1".to_string(), "usage limit reached at 85%".to_string())]
    );
    assert!(w.comments[0]
        .1
        .starts_with("Paused: usage limit reached at 85% (five_hour)"));
    assert!(w.pushes.iter().any(|p| p.contains("Hard usage limit")));
    assert!(w.pushes.last().unwrap().starts_with("Autopilot stopped"));
}

#[test]
fn a_draft_failure_still_comments_and_records_the_reason() {
    let rig = Rig::new();
    rig.fake.add(running("q1", 1));
    rig.fake.w().draft_error = true;
    let s = settings(&[1], |_| {});
    let out = with_ap(&rig, &s, |ap| {
        rig.control(1, "stop");
        ap.tick().unwrap().expect("stopped")
    });
    assert_eq!(out.summary.paused[0].task, "TM-1");
    assert!(out.summary.paused[0]
        .reason
        .contains("no draft PR: nothing to commit"));
    assert!(rig.fake.0.borrow().comments[0]
        .1
        .contains("No draft PR: nothing to commit"));
}

#[test]
fn a_run_that_ignores_the_interrupt_gets_a_term_after_ten_seconds() {
    let rig = Rig::new();
    rig.fake.add(running("q1", 1));
    rig.fake.w().dies_on = "term";
    let s = settings(&[1], |_| {});
    with_ap(&rig, &s, |ap| {
        rig.control(1, "stop");
        let before = rig.fake.0.borrow().now;
        ap.tick().unwrap().expect("stopped");
        let waited = rig.fake.0.borrow().now - before;
        assert!(
            waited >= Duration::seconds(10) && waited < Duration::seconds(16),
            "{waited}"
        );
    });
    let kills: Vec<String> = rig
        .fake
        .0
        .borrow()
        .kills
        .iter()
        .map(|k| k.1.clone())
        .collect();
    assert_eq!(kills, ["int", "term"]);
    assert!(rig.fake.order().contains(&"pause_to_draft q1".to_string()));
}

#[test]
fn a_run_that_never_exits_is_not_drafted() {
    let rig = Rig::new();
    rig.fake.add(running("q1", 1));
    rig.fake.w().dies_on = "never";
    let s = settings(&[1], |_| {});
    let out = with_ap(&rig, &s, |ap| {
        rig.control(1, "stop");
        ap.tick().unwrap().expect("stopped")
    });
    assert!(rig.fake.0.borrow().drafts.is_empty());
    assert!(rig.fake.0.borrow().comments.is_empty());
    assert_eq!(rig.fake.order().last().unwrap(), "disable");
    assert!(rig
        .status()
        .events
        .iter()
        .any(|e| e.kind == "error" && e.text.contains("did not exit")));
    assert_eq!(out.summary.paused.len(), 0);
}

#[test]
fn the_soft_limit_blocks_starts_lets_runs_finish_and_stops_when_idle() {
    let rig = Rig::new();
    rig.fake.add(running("q1", 1));
    rig.fake.add(pickup("q2", 2, "approved", None));
    rig.fake.w().probe = Ok(vec![(55.0, 10.0)]);
    let s = settings(&[1, 2], |_| {});
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        assert!(!rig.fake.0.borrow().passes.last().unwrap().starts_allowed);
        assert!(rig.fake.0.borrow().kills.is_empty());
        assert!(rig.status().extra["blockers"][0]
            .as_str()
            .unwrap()
            .starts_with("usage limit: five_hour at 55%"));
        rig.fake.w().alive.clear();
        rig.fake.w().clean_exit.insert("run-q1".into());
        let out = ap.tick().unwrap().expect("stopped once idle");
        assert!(out
            .reason
            .starts_with("usage limit reached: five_hour at 55%"));
    });
    assert!(
        rig.fake.0.borrow().published.contains(&"q1".to_string()),
        "the finished run still gets its PR"
    );
    assert_eq!(rig.fake.order().last().unwrap(), "disable");
    assert!(rig
        .fake
        .0
        .borrow()
        .pushes
        .iter()
        .any(|p| p.starts_with("Autopilot: usage limit")));
}

#[test]
fn resume_after_reset_keeps_waiting_instead_of_stopping() {
    let rig = Rig::new();
    rig.fake.add(pickup("q2", 2, "approved", None));
    rig.fake.w().probe = Ok(vec![(55.0, 10.0)]);
    let s = settings(&[2], |a| a.resume_after_reset = true);
    with_ap(&rig, &s, |ap| {
        for _ in 0..3 {
            tick_on(ap);
            rig.fake.advance(Duration::seconds(30));
        }
        rig.fake.advance(Duration::hours(3));
        rig.fake.w().probe = Ok(vec![(1.0, 10.0)]);
        tick_on(ap);
        assert!(
            rig.fake.0.borrow().passes.last().unwrap().starts_allowed,
            "starts again after the reset"
        );
    });
}

#[test]
fn resume_after_reset_requeues_each_paused_task_once_after_the_reset_with_the_pass_policy() {
    let rig = Rig::new();
    let s = settings(&[1, 2], |a| a.resume_after_reset = true);
    rig.fake.add(running("q1", 1));
    rig.fake.add(running("q2", 2));
    rig.fake.w().probe = Ok(vec![(85.0, 10.0)]);
    let store = rig.store();
    let mut ap = Autopilot::new(&store, &s, rig.fake.ports(), timing(), &rig.term);
    ap.begin();
    tick_on(&mut ap);
    assert!(rig.fake.0.borrow().requeues.is_empty());
    rig.fake.advance(Duration::minutes(10));
    tick_on(&mut ap);
    assert!(
        rig.fake.0.borrow().requeues.is_empty(),
        "nothing is requeued before the window resets"
    );
    rig.fake.advance(Duration::hours(3));
    rig.fake.w().probe = Ok(vec![(1.0, 10.0)]);
    tick_on(&mut ap);
    let w = rig.fake.0.borrow();
    let ids: Vec<_> = w.requeues.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, vec!["q1", "q2"]);
    let pass = w.passes.last().unwrap();
    assert!(pass.starts_allowed);
    for (_, policy) in &w.requeues {
        assert!(policy.starts_allowed);
        assert_eq!(policy.only, pass.only);
        assert_eq!(policy.max_live, pass.max_live);
    }
    let order = &w.order;
    let at = |needle: &str| order.iter().rposition(|o| o == needle).unwrap();
    assert!(at("requeue q2") < at("pass paused=false starts=true"));
    assert!(w.pushes.iter().any(|p| p.contains("Resumed TM-1")));
    drop(w);
    assert_eq!(rig.status().tasks.in_work, 2);
    rig.fake.advance(Duration::seconds(30));
    tick_on(&mut ap);
    assert_eq!(rig.fake.0.borrow().requeues.len(), 2, "each task once");
}

#[test]
fn only_pickups_this_run_paused_under_its_rule_are_requeued() {
    let rig = Rig::new();
    let s = settings(&[1, 2], |a| a.resume_after_reset = true);
    rig.fake.add(running("q1", 1));
    let mut other_rule = running("q2", 2);
    other_rule.rule = "elsewhere".into();
    rig.fake.add(other_rule);
    let mut stale = running("q3", 3);
    stale.paused_draft = true;
    stale.pr_url = Some("https://github.com/o/r/pull/3".into());
    stale.wip_heads.insert("repo".into(), "abc".into());
    stale.status = Status::Started;
    stale.run_exited = true;
    rig.fake.add(stale);
    rig.fake.w().probe = Ok(vec![(85.0, 10.0)]);
    let store = rig.store();
    let mut ap = Autopilot::new(&store, &s, rig.fake.ports(), timing(), &rig.term);
    ap.begin();
    tick_on(&mut ap);
    rig.fake.advance(Duration::hours(3));
    rig.fake.w().probe = Ok(vec![(1.0, 10.0)]);
    tick_on(&mut ap);
    let w = rig.fake.0.borrow();
    let ids: Vec<_> = w.requeues.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, vec!["q1"]);
}

#[test]
fn the_pickups_paused_by_a_run_survive_a_restart_of_the_loop() {
    let rig = Rig::new();
    let s = settings(&[1], |a| a.resume_after_reset = true);
    rig.fake.add(running("q1", 1));
    rig.fake.w().probe = Ok(vec![(85.0, 10.0)]);
    let store = rig.store();
    let mut ap = Autopilot::new(&store, &s, rig.fake.ports(), timing(), &rig.term);
    ap.begin();
    tick_on(&mut ap);
    assert_eq!(rig.status().extra["resume"]["paused_pickups"][0], "q1");
}

#[test]
fn a_lookup_error_keeps_the_pause_marker_so_the_next_tick_resumes() {
    let rig = Rig::new();
    let s = settings(&[1], |a| a.resume_after_reset = true);
    rig.fake.add(running("q1", 1));
    rig.fake.w().probe = Ok(vec![(85.0, 10.0)]);
    let store = rig.store();
    let mut ap = Autopilot::new(&store, &s, rig.fake.ports(), timing(), &rig.term);
    ap.begin();
    tick_on(&mut ap);
    rig.fake.advance(Duration::hours(3));
    rig.fake.w().probe = Ok(vec![(1.0, 10.0)]);
    rig.fake.w().resumable_fails = true;
    tick_on(&mut ap);
    assert!(rig.fake.0.borrow().requeues.is_empty());
    rig.fake.advance(Duration::seconds(30));
    rig.fake.w().resumable_fails = false;
    tick_on(&mut ap);
    assert_eq!(rig.fake.0.borrow().requeues.len(), 1);
}

#[test]
fn resume_after_reset_off_never_requeues() {
    let rig = Rig::new();
    let s = settings(&[1, 2], |_| {});
    rig.fake.add(running("q1", 1));
    rig.fake.add(running("q2", 2));
    rig.fake.w().probe = Ok(vec![(85.0, 10.0)]);
    with_ap(&rig, &s, |ap| {
        let out = ap.tick().unwrap().expect("stopped");
        assert!(out.reason.starts_with("hard usage limit"));
    });
    rig.fake.advance(Duration::hours(3));
    rig.fake.w().probe = Ok(vec![(1.0, 10.0)]);
    with_ap(&rig, &s, |ap| {
        ap.tick().unwrap();
    });
    assert!(rig.fake.0.borrow().requeues.is_empty());
}

#[test]
fn a_requeue_error_is_an_event_and_the_loop_goes_on() {
    let rig = Rig::new();
    let s = settings(&[1, 2], |a| a.resume_after_reset = true);
    rig.fake.add(running("q1", 1));
    rig.fake.add(running("q2", 2));
    rig.fake.w().probe = Ok(vec![(85.0, 10.0)]);
    rig.fake.w().requeue_fails.insert("q1".into());
    let store = rig.store();
    let mut ap = Autopilot::new(&store, &s, rig.fake.ports(), timing(), &rig.term);
    ap.begin();
    tick_on(&mut ap);
    rig.fake.advance(Duration::hours(3));
    rig.fake.w().probe = Ok(vec![(1.0, 10.0)]);
    tick_on(&mut ap);
    let w = rig.fake.0.borrow();
    assert_eq!(w.requeues.len(), 2);
    assert!(w.state.pickups.iter().any(|p| p.id == "q2-again"));
    drop(w);
    let doc = rig.status();
    assert!(
        doc.events
            .iter()
            .any(|e| e.text.contains("Could not resume TM-1")),
        "{:?}",
        doc.events
    );
    assert_eq!(doc.tasks.in_work, 1);
}

#[test]
fn unknown_usage_blocks_starts_and_stops_after_fifteen_minutes() {
    let rig = Rig::new();
    rig.fake.add(pickup("q2", 2, "approved", None));
    rig.fake.w().probe = Err(Error::internal("claude not installed"));
    let s = settings(&[2], |_| {});
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        assert!(!rig.fake.0.borrow().passes.last().unwrap().starts_allowed);
        rig.fake.advance(Duration::minutes(14));
        tick_on(ap);
        rig.fake.advance(Duration::minutes(2));
        let out = ap.tick().unwrap().expect("stopped");
        assert_eq!(out.reason, "no fresh usage numbers for 15 minutes");
    });
}

#[test]
fn usage_is_probed_at_most_every_five_minutes_unless_a_number_is_missing() {
    let rig = Rig::new();
    rig.fake.add(pickup("q2", 2, "approved", None));
    let s = settings(&[2], |_| {});
    with_ap(&rig, &s, |ap| {
        for _ in 0..8 {
            ap.tick().unwrap();
            rig.fake.advance(Duration::seconds(30));
        }
        assert_eq!(rig.fake.0.borrow().probes, 1);
        rig.fake.advance(Duration::minutes(5));
        ap.tick().unwrap();
        assert_eq!(rig.fake.0.borrow().probes, 2);
    });
}

#[test]
fn max_tasks_counts_starts_blocks_more_and_stops_after_the_runs_finish() {
    let rig = Rig::new();
    rig.fake.add(pickup("q1", 1, "approved", None));
    rig.fake.add(pickup("q2", 2, "approved", None));
    {
        let mut w = rig.fake.w();
        w.start_on_pass.push(("q1".into(), 1));
        w.clean_exit.insert("run-q1".into());
    }
    let s = settings(&[1, 2], |a| a.max_tasks = Some(1));
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        let first = rig.fake.0.borrow().passes[0].clone();
        assert!(first.starts_allowed);
        assert_eq!(first.max_new_starts, Some(1));
        assert_eq!(first.max_live, Some(2));
        assert_eq!(first.only, Some([1, 2].into()));
        tick_on(ap);
        let second = rig.fake.0.borrow().passes[1].clone();
        assert!(!second.starts_allowed);
        assert_eq!(second.max_new_starts, Some(0));
        rig.fake.w().alive.clear();
        let out = ap.tick().unwrap().expect("stopped");
        assert_eq!(out.reason, "task limit reached");
    });
}

#[test]
fn failures_are_counted_once_and_a_pr_resets_the_streak() {
    let rig = Rig::new();
    let s = settings(&[1, 2, 3, 4], |_| {});
    rig.fake.add(pickup("q4", 4, "approved", None));
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        for (i, id) in ["q1", "q2"].iter().enumerate() {
            let mut p = running(id, i as i64 + 1);
            p.run_exited = true;
            rig.fake.add(p);
        }
        tick_on(ap);
        tick_on(ap);
        assert_eq!(rig.status().failures_in_row, 2);
        assert_eq!(rig.status().tasks.failed, 2);
        let mut ok = running("q3", 3);
        ok.run_exited = true;
        ok.run_clean = true;
        ok.pr_url = Some("https://github.com/o/r/pull/3".into());
        rig.fake.add(ok);
        tick_on(ap);
        assert_eq!(rig.status().failures_in_row, 0);
        assert_eq!(rig.status().prs.len(), 1);
        assert!(rig
            .fake
            .0
            .borrow()
            .pushes
            .iter()
            .any(|p| p
                == "Autopilot: pull request: PR opened for TM-3: https://github.com/o/r/pull/3"));
    });
}

#[test]
fn held_runs_killed_runs_and_prs_do_not_count_as_failures() {
    let rig = Rig::new();
    let s = settings(&[1, 2, 3], |_| {});
    let mut held = running("q1", 1);
    held.run_exited = true;
    held.run_clean = true;
    held.held = Some(Held::default());
    rig.fake.add(held);
    let mut paused = running("q2", 2);
    paused.run_exited = true;
    paused.paused_draft = true;
    rig.fake.add(paused);
    with_ap(&rig, &s, |ap| {
        ap.tick().unwrap();
    });
    let st = rig.status();
    assert_eq!(st.failures_in_row, 0);
    assert_eq!(st.tasks.held, 1);
    assert_eq!(st.extra["paused_tasks"], 1);
}

#[test]
fn three_failed_tasks_stop_with_the_failures_in_the_summary() {
    let rig = Rig::new();
    let s = settings(&[1, 2, 3, 4], |_| {});
    rig.fake.add(pickup("q4", 4, "approved", None));
    let out = with_ap(&rig, &s, |ap| {
        tick_on(ap);
        for t in 1..=3 {
            let mut p = pickup(&format!("q{t}"), t, "failed", None);
            p.reason = Some("clone failed".into());
            rig.fake.add(p);
        }
        ap.tick().unwrap().expect("stopped")
    });
    assert_eq!(out.reason, "3 failures in a row");
    assert_eq!(out.summary.failed.len(), 3);
    assert_eq!(out.summary.failed[0].reason, "clone failed");
    assert!(out.summary.text.contains("3 failed"));
}

#[test]
fn nothing_left_stops_after_the_first_clean_pass() {
    let rig = Rig::new();
    let mut done = running("q1", 1);
    done.run_exited = true;
    done.run_clean = true;
    done.pr_url = Some("https://github.com/o/r/pull/1".into());
    rig.fake.add(done);
    let s = settings(&[1], |_| {});
    let out = with_ap(&rig, &s, |ap| ap.tick().unwrap().expect("stopped"));
    assert_eq!(out.reason, "nothing left to do");
    assert_eq!(
        out.summary.prs,
        Vec::<PrLink>::new(),
        "a PR that existed before the autopilot is not its own"
    );
    assert_eq!(rig.status().tasks.done_pr, 1);
}

#[test]
fn the_hours_deadline_is_soft_then_hard_after_the_grace() {
    let rig = Rig::new();
    rig.fake.add(running("q1", 1));
    let s = settings(&[1], |a| a.max_hours = Some(1.0));
    with_ap(&rig, &s, |ap| {
        rig.fake.advance(Duration::minutes(61));
        assert!(
            ap.tick().unwrap().is_none(),
            "live runs keep going during the grace"
        );
        assert!(!rig.fake.0.borrow().passes.last().unwrap().starts_allowed);
        rig.fake.advance(Duration::minutes(30));
        let out = ap.tick().unwrap().expect("hard stop");
        assert_eq!(out.reason, "time limit reached");
    });
    assert_eq!(
        rig.fake.0.borrow().kills[0].2,
        "Paused: autopilot time limit reached"
    );
}

#[test]
fn the_status_shows_counters_runs_limits_and_server_status_names() {
    let rig = Rig::new();
    rig.fake.add(running("q1", 1));
    rig.fake.add(pickup("q2", 2, "approved", None));
    {
        let mut w = rig.fake.w();
        w.task_status.insert(3, 1);
        w.task_status.insert(4, 2);
        w.unconfirmed = 2;
    }
    let s = settings(&[1, 2, 3, 4], |_| {});
    with_ap(&rig, &s, |ap| {
        ap.tick().unwrap();
    });
    let st = rig.status();
    assert_eq!(st.tasks.total, 4);
    assert_eq!(st.tasks.in_work, 1);
    assert_eq!(st.tasks.queued, 1);
    assert_eq!(st.tasks.needs_feedback, 1);
    assert_eq!(st.tasks.blocked, 1);
    assert_eq!(st.runs.len(), 1);
    assert_eq!(st.runs[0].task, "TM-1");
    assert_eq!(st.runs[0].step, "running");
    assert_eq!(st.runs[0].repos, Vec::<String>::new());
    assert_eq!(st.unconfirmed_new, 2);
    let five = st.limits.iter().find(|l| l.window == "five_hour").unwrap();
    assert_eq!(
        (five.tool.as_str(), five.source.as_str(), five.used_pct),
        ("claude", "real", Some(10.0))
    );
    assert_eq!((five.stop_new_at, five.pause_at), (Some(50), Some(80)));
    let next = st.next_stop.unwrap();
    assert_eq!(
        (next.window.as_str(), next.pct_left, next.kind.as_str()),
        ("five_hour", 40.0, "no new tasks")
    );
    assert_eq!(st.teams.max, 2);
    assert_eq!(st.pid, std::process::id());
    assert!(rig
        .fake
        .0
        .borrow()
        .pushes
        .iter()
        .any(|p| p == "Autopilot: new tasks: 2 new tasks need confirm"));
}

#[test]
fn the_queue_being_turned_off_by_someone_else_ends_the_autopilot() {
    let rig = Rig::new();
    rig.fake.add(pickup("q2", 2, "approved", None));
    rig.fake.w().enabled = false;
    let s = settings(&[2], |_| {});
    let out = with_ap(&rig, &s, |ap| ap.tick().unwrap().expect("stopped"));
    assert_eq!(out.reason, "the queue was turned off");
}

#[test]
fn run_starts_waits_for_the_run_publishes_and_stops_when_nothing_is_left() {
    let rig = Rig::new();
    rig.fake.add(pickup("q1", 1, "approved", None));
    {
        let mut w = rig.fake.w();
        w.start_on_pass.push(("q1".into(), 1));
        w.clean_exit.insert("run-q1".into());
        w.finish_on_sleep = true;
    }
    let s = settings(&[1], |_| {});
    let store = rig.store();
    let out = serve(&store, &s, rig.fake.ports(), timing(), &rig.term).unwrap();
    assert_eq!(out.reason, "nothing left to do");
    assert_eq!(out.summary.prs.len(), 1);
    assert!(!out.summary.prs[0].draft);
    assert_eq!(out.summary.started, 1);
    let w = rig.fake.0.borrow();
    assert!(w
        .pushes
        .iter()
        .any(|p| p.starts_with("Autopilot: task picked")));
    assert!(w
        .pushes
        .iter()
        .any(|p| p.starts_with("Autopilot: pull request")));
    assert_eq!(w.order.last().unwrap(), "disable");
    assert_eq!(w.disabled.len(), 1);
}

#[test]
fn with_the_usage_guard_off_a_hot_limit_neither_blocks_starts_nor_drains_runs() {
    let rig = Rig::new();
    rig.fake.add(running("q1", 1));
    rig.fake.add(pickup("q2", 2, "approved", None));
    rig.fake.w().probe = Ok(vec![(99.0, 99.0)]);
    let s = settings(&[1, 2], |a| a.usage_guard = false);
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        assert!(rig.fake.0.borrow().passes.last().unwrap().starts_allowed);
        assert!(rig.fake.0.borrow().kills.is_empty());
        assert!(ap.blockers.is_empty(), "{:?}", ap.blockers);
        assert!(!ap.limit_rows().is_empty(), "limits are still shown");
    });
}

#[test]
fn with_the_usage_guard_off_unknown_numbers_never_stop_the_run() {
    let rig = Rig::new();
    rig.fake.add(pickup("q2", 2, "approved", None));
    rig.fake.w().probe = Err(Error::conflict("no usage"));
    let s = settings(&[2], |a| a.usage_guard = false);
    with_ap(&rig, &s, |ap| {
        for _ in 0..3 {
            tick_on(ap);
            rig.fake.advance(Duration::minutes(10));
        }
        assert!(ap.blockers.is_empty());
    });
}

#[test]
fn a_pickup_expired_as_taken_is_skipped_not_failed() {
    let rig = Rig::new();
    let mut p = pickup("q1", 1, "expired", None);
    p.reason =
        Some("skipped_taken: the task was taken before the run started (Anna (assignee))".into());
    rig.fake.add(p);
    rig.fake.add(pickup("q2", 2, "approved", None));
    let s = settings(&[1, 2], |_| {});
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_on(ap);
        let st = rig.status();
        assert_eq!(st.tasks.skipped, 1);
        assert_eq!(st.tasks.failed, 0);
        assert_eq!(ap.failures, 0);
        let taken: Vec<_> = ap
            .events
            .iter()
            .filter(|e| e.text == "skipped TM-1: taken by someone else")
            .collect();
        assert_eq!(taken.len(), 1);
        let pushes = rig.fake.0.borrow().pushes.clone();
        assert_eq!(
            pushes
                .iter()
                .filter(|p| p.contains("skipped TM-1: taken by someone else"))
                .count(),
            1,
            "{pushes:?}"
        );
        let summary = ap.build_summary(&ap.p.queue.state().unwrap(), &StopReason::NothingLeft);
        assert_eq!(summary.skipped[0].reason, "taken by someone else");
    });
}

#[test]
fn a_few_unreadable_states_are_tolerated_and_many_end_the_run_with_a_final_status() {
    let rig = Rig::new();
    rig.fake.add(pickup("q1", 1, "approved", None));
    let s = settings(&[1], |_| {});
    with_ap(&rig, &s, |ap| {
        rig.fake.w().state_fail = true;
        for _ in 0..(MAX_PASS_ERRORS - 1) {
            assert!(ap.step().is_none());
        }
        rig.fake.w().state_fail = false;
        assert!(ap.step().is_none());
        assert_eq!(ap.state_errors, 0);
        rig.fake.w().state_fail = true;
        let mut out = None;
        for _ in 0..MAX_PASS_ERRORS {
            out = out.or(ap.step());
        }
        let out = out.expect("stopped");
        assert!(out.reason.starts_with("error: "), "{}", out.reason);
        rig.fake.w().state_fail = false;
    });
    let st = rig.status();
    assert_eq!(st.state, RunState::Stopped);
    assert!(st.stop_reason.unwrap().starts_with("error: "));
    assert_eq!(rig.fake.order().last().unwrap(), "disable");
    assert!(rig
        .fake
        .0
        .borrow()
        .pushes
        .last()
        .unwrap()
        .starts_with("Autopilot stopped"));
}

fn tick_any(ap: &mut Autopilot) {
    let _ = ap.tick().unwrap();
}

fn plan_json(plan: &[(u32, &str, &str)], triage: &[(u32, &str)]) -> Vec<u8> {
    json!({
        "version": 1,
        "plan": plan.iter().map(|(n, g, m)| json!({"ref": n, "group": g, "mode": m, "reason": "fine"})).collect::<Vec<_>>(),
        "triage": triage.iter().map(|(n, v)| json!({"ref": n, "verdict": v, "reason": "unclear", "question": "Which one?"})).collect::<Vec<_>>(),
    })
    .to_string()
    .into_bytes()
}

fn orch_rig(plan: &[(u32, &str, &str)], triage: &[(u32, &str)]) -> Rig {
    let rig = Rig::new();
    {
        let mut w = rig.fake.w();
        w.orch_on = true;
        w.orch_poll = Poll::Exited(Some(0));
        w.orch_outputs.insert(1, plan_json(plan, triage));
        w.orch_outputs.insert(2, plan_json(plan, triage));
    }
    rig
}

fn orch_settings(tasks: &[i64]) -> Settings {
    settings(tasks, |a| {
        a.usage_guard = false;
        a.max_teams = 2;
        a.team_size = 4;
    })
}

fn last_policy(rig: &Rig) -> StartPolicy {
    rig.fake
        .0
        .borrow()
        .passes
        .iter()
        .rev()
        .find(|p| p.plan.is_some())
        .unwrap()
        .clone()
}

#[test]
fn triage_comments_with_the_template_then_moves_the_task_and_dispatch_skips_it() {
    let rig = orch_rig(
        &[(3, "a", "heavy"), (1, "", "eco")],
        &[(2, "needs_feedback")],
    );
    let s = orch_settings(&[1, 2, 3]);
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        assert!(
            rig.fake.0.borrow().passes.is_empty(),
            "no pass before the plan"
        );
        tick_any(ap);
    });
    let w = rig.fake.0.borrow();
    assert_eq!(w.comments.len(), 1);
    assert_eq!(w.comments[0].0, 2);
    assert!(w.comments[0].1.contains("(needs feedback)"));
    assert!(w.comments[0].1.contains("Question for you: Which one?"));
    assert_eq!(w.status_sets, vec![(2, 1)]);
    let comment_at = w.order.iter().position(|o| o == "comment 2").unwrap();
    let set_at = w.order.iter().position(|o| o == "set_status 2").unwrap();
    let pass_at = w.order.iter().position(|o| o.starts_with("pass")).unwrap();
    assert!(comment_at < set_at && set_at < pass_at, "{:?}", w.order);
    assert!(w
        .pushes
        .iter()
        .any(|p| p.contains("1 task need your feedback")));
    drop(w);
    let only = last_policy(&rig).only.unwrap();
    assert_eq!(only, BTreeSet::from([1, 3]));
    let st = rig.status();
    assert!(st.triage[0].applied && st.triage[0].error.is_none());
    assert_eq!(st.tasks.needs_feedback, 1);
    assert_eq!(st.needs_feedback_tasks.unwrap()[0].key, "TM-2");
    assert_eq!(
        st.plan.unwrap().state,
        crate::cmd::autopilot::model::PlanState::Done
    );
}

#[test]
fn a_task_counted_by_triage_and_by_the_server_status_is_counted_once() {
    let rig = orch_rig(&[(1, "", "eco")], &[(2, "needs_feedback")]);
    rig.fake.w().task_status.insert(2, 1);
    let s = orch_settings(&[1, 2]);
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_any(ap);
    });
    let st = rig.status();
    assert_eq!(st.tasks.needs_feedback, 1);
    assert_eq!(st.needs_feedback_tasks.unwrap().len(), 1);
}

#[test]
fn an_ambiguous_or_forbidden_status_rejects_the_plan_before_any_write() {
    let rig = orch_rig(&[(1, "", "eco")], &[(2, "needs_feedback"), (3, "blocked")]);
    rig.fake.w().statuses = vec![
        (1, "Needs feedback".into(), "active".into()),
        (4, "needs FEEDBACK".into(), "active".into()),
        (2, "Blocked".into(), "review".into()),
    ];
    let s = orch_settings(&[1, 2, 3]);
    let out = with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_any(ap);
        ap.tick().unwrap().expect("stopped")
    });
    assert!(
        out.reason.starts_with("orchestrator output rejected: "),
        "{}",
        out.reason
    );
    let w = rig.fake.0.borrow();
    assert!(w.comments.is_empty() && w.status_sets.is_empty());
    assert!(w.passes.is_empty(), "{:?}", w.order);
    assert_eq!(w.orch_starts.len(), 2);
}

#[test]
fn only_the_statuses_a_verdict_uses_are_checked() {
    let rig = orch_rig(&[(1, "", "eco")], &[(2, "needs_feedback")]);
    rig.fake.w().statuses = vec![(1, "Needs feedback".into(), "active".into())];
    let s = orch_settings(&[1, 2]);
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_any(ap);
    });
    assert_eq!(rig.fake.0.borrow().status_sets, vec![(2, 1)]);
}

#[test]
fn a_statuses_network_error_before_accepting_retries_next_tick_without_failing() {
    let rig = orch_rig(&[(1, "", "eco")], &[(2, "needs_feedback")]);
    rig.fake.w().statuses_fail_from = Some(1);
    let s = orch_settings(&[1, 2]);
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_on(ap);
        tick_on(ap);
        {
            let w = rig.fake.0.borrow();
            assert_eq!(w.orch_starts.len(), 1);
            assert!(w.passes.is_empty() && w.comments.is_empty());
        }
        assert_eq!(
            rig.status().plan.unwrap().state,
            crate::cmd::autopilot::model::PlanState::Running
        );
        rig.fake.w().statuses_fail_from = None;
        tick_any(ap);
    });
    let w = rig.fake.0.borrow();
    assert_eq!(w.orch_starts.len(), 1);
    assert_eq!(w.status_sets, vec![(2, 1)]);
}

#[test]
fn a_statuses_error_while_applying_keeps_the_rows_pending_and_retries() {
    let rig = orch_rig(&[(1, "", "eco")], &[(2, "needs_feedback")]);
    rig.fake.w().statuses_fail_from = Some(3);
    let s = orch_settings(&[1, 2]);
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_any(ap);
        let st = rig.status();
        assert!(!st.triage[0].applied && st.triage[0].error.is_none());
        assert!(rig.fake.0.borrow().comments.is_empty());
        rig.fake.w().statuses_fail_from = None;
        tick_any(ap);
    });
    let st = rig.status();
    assert!(st.triage[0].applied && st.triage[0].error.is_none());
    assert_eq!(rig.fake.0.borrow().comments.len(), 1);
}

#[test]
fn an_input_read_error_retries_next_tick_and_is_not_a_refusal() {
    let rig = orch_rig(&[(1, "", "eco")], &[]);
    rig.fake.w().input_failed.insert(2);
    let s = orch_settings(&[1, 2]);
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_on(ap);
        assert!(rig.fake.0.borrow().orch_starts.is_empty());
        assert!(rig.status().not_planned.is_empty());
        rig.fake.w().input_failed.clear();
        tick_on(ap);
    });
    let w = rig.fake.0.borrow();
    assert_eq!(w.orch_starts.len(), 1);
    assert_eq!(w.orch_starts[0].1.tasks.len(), 2);
}

#[test]
fn tasks_sharing_a_key_get_distinct_labels_and_the_plan_maps_by_id() {
    let rig = orch_rig(&[(1, "", "eco"), (2, "", "eco")], &[]);
    {
        let mut w = rig.fake.w();
        w.plan_keys.insert(1, "TM-210".into());
        w.plan_keys.insert(2, "TM-210".into());
    }
    let mut s = orch_settings(&[1, 2]);
    s.order = vec![2, 1];
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_any(ap);
    });
    let w = rig.fake.0.borrow();
    assert_eq!(w.orch_starts.len(), 1);
    let input = &w.orch_starts[0].1;
    let keys: Vec<_> = input.tasks.iter().map(|t| t.key.as_str()).collect();
    assert_eq!(keys, ["task-1", "task-2"]);
    assert_eq!(input.settings.forced_order, [2, 1]);
    drop(w);
    assert_eq!(last_policy(&rig).only.unwrap(), BTreeSet::from([1, 2]));
}

#[test]
fn a_persistent_input_read_error_ends_the_run() {
    let rig = orch_rig(&[(1, "", "eco")], &[]);
    rig.fake.w().input_failed.insert(2);
    let s = orch_settings(&[1, 2]);
    let out = with_ap(&rig, &s, |ap| {
        let mut out = None;
        for _ in 0..MAX_PASS_ERRORS {
            out = out.or(ap.tick().unwrap());
        }
        out.expect("stopped")
    });
    assert!(
        out.reason.starts_with("orchestrator input unavailable"),
        "{}",
        out.reason
    );
}

#[test]
fn a_changed_taken_or_moved_task_is_skipped_with_an_error() {
    let rig = orch_rig(
        &[(1, "", "eco")],
        &[(2, "needs_feedback"), (3, "blocked"), (4, "blocked")],
    );
    {
        let mut w = rig.fake.w();
        w.changed.insert(2);
        w.taken.insert(3);
        w.moved.insert(4);
    }
    let s = orch_settings(&[1, 2, 3, 4]);
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_any(ap);
    });
    let w = rig.fake.0.borrow();
    assert!(w.comments.is_empty() && w.status_sets.is_empty());
    drop(w);
    let st = rig.status();
    for row in &st.triage {
        assert_eq!(row.error.as_deref(), Some("changed since planning"));
        assert!(!row.applied);
    }
}

#[test]
fn rejected_output_writes_nothing_starts_nothing_and_stops_with_a_safe_reason() {
    let rig = orch_rig(&[], &[]);
    {
        let mut w = rig.fake.w();
        w.orch_outputs.insert(1, b"SECRET-RAW not json".to_vec());
        w.orch_outputs.insert(2, plan_json(&[(9, "", "eco")], &[]));
    }
    let s = orch_settings(&[1, 2]);
    let out = with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_any(ap);
        ap.tick().unwrap().expect("stopped")
    });
    assert!(
        out.reason.starts_with("orchestrator output rejected: "),
        "{}",
        out.reason
    );
    assert!(!out.reason.contains("SECRET-RAW"));
    let w = rig.fake.0.borrow();
    assert!(w.passes.is_empty(), "no pass: {:?}", w.order);
    assert!(w.comments.is_empty() && w.status_sets.is_empty());
    assert_eq!(w.orch_starts.len(), 2);
    assert!(w.pushes.iter().any(|p| p.contains("plan failed")));
    drop(w);
    let st = rig.status();
    assert_eq!(st.state, RunState::Stopped);
    assert_eq!(
        st.plan.unwrap().state,
        crate::cmd::autopilot::model::PlanState::Rejected
    );
}

#[test]
fn the_retry_gets_a_new_nonce_and_a_good_second_output_is_accepted() {
    let rig = orch_rig(&[(1, "", "eco")], &[]);
    rig.fake.w().orch_outputs.insert(1, b"{".to_vec());
    let s = orch_settings(&[1]);
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_any(ap);
        assert_eq!(rig.fake.0.borrow().orch_starts.len(), 2);
        tick_any(ap);
    });
    let w = rig.fake.0.borrow();
    assert_ne!(w.orch_starts[0].0, w.orch_starts[1].0);
    assert!(w.orch_starts[0].0.starts_with("1-") && w.orch_starts[1].0.starts_with("2-"));
    assert!(!w.passes.is_empty());
}

#[test]
fn a_session_that_runs_too_long_is_killed_and_retried() {
    let rig = orch_rig(&[(1, "", "eco")], &[]);
    rig.fake.w().orch_poll = Poll::Running;
    let s = orch_settings(&[1]);
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        rig.fake.advance(Duration::minutes(21));
        tick_on(ap);
    });
    let w = rig.fake.0.borrow();
    assert_eq!(w.orch_kills, vec!["orch-run-1".to_string()]);
    assert_eq!(w.orch_starts.len(), 2);
    assert!(w.passes.is_empty());
}

#[test]
fn stop_during_the_phase_kills_the_session_and_makes_no_pass() {
    let rig = orch_rig(&[(1, "", "eco")], &[]);
    rig.fake.w().orch_poll = Poll::Running;
    let s = orch_settings(&[1]);
    let out = with_ap(&rig, &s, |ap| {
        tick_on(ap);
        rig.control(1, "stop");
        ap.tick().unwrap().expect("stopped")
    });
    assert_eq!(out.reason, "stopped by you");
    let w = rig.fake.0.borrow();
    assert_eq!(w.orch_kills.len(), 1);
    assert!(w.passes.is_empty());
    assert_eq!(w.disabled.len(), 1);
}

#[test]
fn suspend_during_the_phase_kills_the_session() {
    let rig = orch_rig(&[(1, "", "eco")], &[]);
    rig.fake.w().orch_poll = Poll::Running;
    let s = orch_settings(&[1]);
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        rig.term.store(true, Ordering::SeqCst);
        assert!(ap.tick().unwrap().unwrap().suspended);
    });
    assert_eq!(rig.fake.0.borrow().orch_kills.len(), 1);
}

#[test]
fn a_hard_usage_limit_during_the_phase_kills_the_session() {
    let rig = orch_rig(&[(1, "", "eco")], &[]);
    rig.fake.w().orch_poll = Poll::Running;
    let s = settings(&[1], |_| {});
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        rig.fake.w().probe = Ok(vec![(95.0, 95.0)]);
        rig.fake.advance(Duration::minutes(6));
        let _ = ap.tick();
    });
    assert_eq!(rig.fake.0.borrow().orch_kills.len(), 1);
    assert!(rig.fake.0.borrow().passes.is_empty());
}

#[test]
fn a_dry_run_plans_and_triages_without_any_write_or_pass() {
    let rig = orch_rig(&[(1, "", "eco")], &[(2, "blocked")]);
    let mut s = orch_settings(&[1, 2]);
    s.dry_run = true;
    let out = with_ap(&rig, &s, |ap| {
        tick_on(ap);
        ap.tick().unwrap().expect("finished")
    });
    assert_eq!(out.reason, "dry run finished");
    assert!(out
        .summary
        .text
        .starts_with("dry run: 1 planned, 1 need a human"));
    let w = rig.fake.0.borrow();
    assert!(w.passes.is_empty());
    assert!(w.comments.is_empty() && w.status_sets.is_empty());
    assert!(w.disabled.is_empty());
    assert!(!w.order.iter().any(|o| o == "keep_on"));
    drop(w);
    let st = rig.status();
    let plan = st.plan.unwrap();
    assert!(plan.dry_run);
    assert_eq!(plan.items.len(), 1);
    assert_eq!(st.triage.len(), 1);
    assert!(!st.triage[0].applied && st.triage[0].error.is_none());
    assert_eq!(st.stop_reason.as_deref(), Some("dry run finished"));
}

#[test]
fn a_dry_run_ignores_old_failed_pickups_and_shows_the_tasks_as_planned() {
    let rig = orch_rig(&[(1, "", "eco")], &[]);
    rig.fake.add(pickup("old", 1, "failed", None));
    let mut s = orch_settings(&[1]);
    s.dry_run = true;
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        ap.tick().unwrap();
    });
    let st = rig.status();
    assert_eq!(st.tasks.failed, 0);
    assert_eq!(st.plan.unwrap().items[0].state, "planned (dry run)");
}

const STALE_GATE: &str = "gate:the task has no approval for agent runs (sign it first)";

fn skip_events(ap: &Autopilot) -> Vec<String> {
    ap.events
        .iter()
        .filter(|e| e.kind == "skipped")
        .map(|e| e.text.clone())
        .collect()
}

#[test]
fn queue_skips_from_an_earlier_session_are_not_shown_before_this_run_has_a_pass() {
    let rig = orch_rig(&[(1, "", "eco"), (2, "", "eco")], &[]);
    rig.fake.w().orch_poll = Poll::Running;
    for id in [1, 2] {
        rig.fake
            .w()
            .state
            .skips
            .insert(format!("r/{id}"), STALE_GATE.into());
    }
    let s = orch_settings(&[1, 2]);
    with_ap(&rig, &s, |ap| {
        tick_any(ap);
        tick_any(ap);
        assert_eq!(ap.view.counts.skipped, 0);
        assert!(skip_events(ap).is_empty(), "{:?}", skip_events(ap));
        rig.fake.w().state.skips.clear();
        tick_any(ap);
        assert_eq!(ap.view.counts.skipped, 0);
        assert!(skip_events(ap).is_empty());
    });
}

#[test]
fn a_skip_recorded_by_a_pass_of_this_run_is_still_reported() {
    let rig = Rig::new();
    rig.fake
        .w()
        .state
        .skips
        .insert("r/1".into(), STALE_GATE.into());
    let s = settings(&[1], |_| {});
    with_ap(&rig, &s, |ap| {
        tick_any(ap);
        assert_eq!(ap.view.counts.skipped, 1);
        let events = skip_events(ap);
        assert_eq!(events.len(), 1, "{events:?}");
        assert!(
            events[0].contains("no approval for agent runs"),
            "{events:?}"
        );
    });
}

#[test]
fn a_finished_pickup_from_before_this_run_is_not_counted() {
    let rig = Rig::new();
    let mut old = pickup("old", 1, "failed", None);
    old.created_at = "2026-10-12T09:00:00Z".into();
    rig.fake.add(old);
    let s = settings(&[1], |_| {});
    with_ap(&rig, &s, |ap| {
        tick_any(ap);
        assert_eq!(ap.view.counts.failed, 0);
        assert!(!ap.events.iter().any(|e| e.kind == "failed"));
    });
}

#[test]
fn a_live_pickup_from_before_this_run_is_still_shown() {
    let rig = Rig::new();
    let mut old = running("old", 1);
    old.created_at = "2026-10-12T09:00:00Z".into();
    rig.fake.add(old);
    let s = settings(&[1], |_| {});
    with_ap(&rig, &s, |ap| {
        tick_any(ap);
        assert_eq!(ap.view.counts.in_work, 1);
    });
}

#[test]
fn a_run_limited_to_chosen_tasks_does_not_count_the_rest_as_new() {
    let rig = Rig::new();
    rig.fake.w().unconfirmed = 35;
    let mut s = settings(&[1], |_| {});
    s.answers.all_tasks = false;
    with_ap(&rig, &s, |ap| {
        ap.tick().unwrap();
    });
    assert_eq!(rig.status().unconfirmed_new, 0);
}

#[test]
fn the_start_policy_carries_order_limits_groups_and_team_sizes() {
    let rig = orch_rig(&[(2, "db", "heavy"), (1, "db", "eco"), (3, "", "eco")], &[]);
    let mut s = orch_settings(&[1, 2, 3]);
    s.order = vec![3];
    s.answers.profile = Some("claude-code".into());
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_any(ap);
    });
    let policy = last_policy(&rig);
    assert_eq!(policy.max_live, Some(2));
    let plan = policy.plan.unwrap();
    assert_eq!(plan.order, vec![3, 2, 1]);
    assert_eq!(plan.groups.get(&2).map(String::as_str), Some("db"));
    assert_eq!(plan.groups.get(&1).map(String::as_str), Some("db"));
    assert!(!plan.groups.contains_key(&3));
    assert_eq!(plan.teams[&2].size, 4);
    assert_eq!(plan.teams[&2].mode.as_str(), "heavy");
    assert_eq!(plan.teams[&1].mode.as_str(), "eco");
    assert_eq!(plan.profile.as_deref(), Some("claude-code"));
    assert_eq!(
        rig.fake.0.borrow().orch_starts[0].1.settings.forced_order,
        vec![3]
    );
}

#[test]
fn the_start_policy_names_the_rule_only_when_the_persona_is_optional() {
    for optional in [false, true] {
        let rig = orch_rig(&[(1, "", "eco")], &[]);
        let mut s = orch_settings(&[1]);
        s.persona_optional = optional;
        with_ap(&rig, &s, |ap| {
            tick_on(ap);
            tick_any(ap);
        });
        let policy = last_policy(&rig);
        assert_eq!(policy.persona_optional_rule.is_some(), optional);
        assert_eq!(policy.persona_optional("r", 1), optional);
        assert!(!policy.persona_optional("r", 2));
    }
}

#[test]
fn the_drain_policy_names_the_rule_only_when_the_persona_is_optional() {
    for optional in [false, true] {
        let rig = Rig::new();
        rig.fake.add(pickup("q1", 1, "approved", None));
        let mut s = settings(&[1], |_| {});
        s.persona_optional = optional;
        with_ap(&rig, &s, tick_on);
        let passes = rig.fake.0.borrow();
        let policy = passes.passes.last().unwrap();
        assert_eq!(policy.persona_optional("r", 1), optional);
    }
}

#[test]
fn the_queue_loop_policy_never_makes_the_persona_optional() {
    let rig = Rig::new();
    let mut s = queue_settings();
    s.persona_optional = true;
    with_ap(&rig, &s, tick_on);
    let passes = rig.fake.0.borrow();
    assert!(passes
        .passes
        .last()
        .unwrap()
        .persona_optional_rule
        .is_none());
}

#[test]
fn a_forced_team_mode_overrides_the_orchestrator() {
    let rig = orch_rig(&[(1, "", "eco")], &[]);
    let mut s = orch_settings(&[1]);
    s.answers.team_mode = crate::cmd::autopilot::orchestrate::ModeSetting::Heavy;
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_any(ap);
    });
    assert_eq!(
        last_policy(&rig).plan.unwrap().teams[&1].mode.as_str(),
        "heavy"
    );
}

#[test]
fn codex_teams_have_one_agent() {
    let rig = orch_rig(&[(1, "", "eco")], &[]);
    let mut s = orch_settings(&[1]);
    s.agent = "codex".into();
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_any(ap);
    });
    assert_eq!(last_policy(&rig).plan.unwrap().teams[&1].size, 1);
    assert_eq!(rig.fake.0.borrow().orch_starts[0].1.settings.team_size, 1);
    assert_eq!(rig.status().agents_planned.unwrap().max, 2);
}

#[test]
fn the_frozen_plan_survives_a_suspend_without_a_second_orchestrator_or_comment() {
    let rig = orch_rig(&[(1, "", "eco")], &[(2, "blocked")]);
    let s = orch_settings(&[1, 2]);
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_any(ap);
        rig.term.store(true, Ordering::SeqCst);
        assert!(ap.tick().unwrap().unwrap().suspended);
    });
    rig.term.store(false, Ordering::SeqCst);
    let first = last_policy(&rig).plan.unwrap();
    let store = rig.store();
    let mut ap = Autopilot::new(&store, &s, rig.fake.ports(), timing(), &rig.term);
    ap.begin();
    let _ = ap.tick().unwrap();
    let w = rig.fake.0.borrow();
    assert_eq!(w.orch_starts.len(), 1);
    assert_eq!(w.comments.len(), 1);
    assert_eq!(w.status_sets.len(), 1);
    drop(w);
    let again = last_policy(&rig).plan.unwrap();
    assert_eq!(first.order, again.order);
    assert_eq!(first.teams, again.teams);
    assert_eq!(rig.status().triage.len(), 1);
}

#[test]
fn tasks_without_a_valid_approval_are_not_planned_or_dispatched() {
    let rig = orch_rig(&[(1, "", "eco")], &[]);
    rig.fake.w().unsigned.insert(2);
    let s = orch_settings(&[1, 2]);
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_any(ap);
    });
    assert_eq!(rig.fake.0.borrow().orch_starts[0].1.tasks.len(), 1);
    assert_eq!(last_policy(&rig).only.unwrap(), BTreeSet::from([1]));
    let st = rig.status();
    assert_eq!(st.not_planned[0].task, "TM-2");
}

#[test]
fn nothing_plannable_stops_without_a_session() {
    let rig = orch_rig(&[], &[]);
    rig.fake.w().unsigned.insert(1);
    let s = orch_settings(&[1]);
    let out = with_ap(&rig, &s, |ap| ap.tick().unwrap().expect("stopped"));
    assert_eq!(out.reason, "nothing to plan");
    let w = rig.fake.0.borrow();
    assert!(w.orch_starts.is_empty() && w.passes.is_empty());
}

#[test]
fn live_runs_show_their_team_in_the_status() {
    let rig = orch_rig(&[(1, "", "heavy")], &[]);
    rig.fake.add(running("q1", 1));
    let s = orch_settings(&[1]);
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_any(ap);
    });
    let st = rig.status();
    let run = &st.runs[0];
    assert_eq!(
        (run.team_size, run.team_mode.as_deref(), run.agents),
        (Some(4), Some("heavy"), Some(4))
    );
    assert_eq!(run.workspace_code.as_deref(), Some("ws"));
    assert_eq!(run.workspace_id, Some(7));
    assert_eq!(run.session_id.as_deref(), Some("run-q1"));
    let planned = st.agents_planned.unwrap();
    assert_eq!((planned.running, planned.max), (4, 8));
    assert_eq!((st.teams.running, st.teams.max), (1, 2));
    assert_eq!(st.plan.unwrap().items[0].state, "running");
}

#[test]
fn a_queue_mode_status_has_none_of_the_orchestrator_keys() {
    let rig = Rig::new();
    rig.fake.w().orch_on = true;
    rig.fake.add(running("q1", 1));
    let s = queue_settings();
    with_ap(&rig, &s, tick_on);
    let raw: Value =
        serde_json::from_str(&std::fs::read_to_string(rig.store().status_path()).unwrap()).unwrap();
    for key in [
        "plan",
        "triage",
        "not_planned",
        "agents_planned",
        "needs_feedback_tasks",
        "blocked_tasks",
        "queued_tasks",
    ] {
        assert!(raw.get(key).is_none(), "{key}");
    }
    assert!(raw["resume"].get("plan").is_none());
    for key in [
        "team_size",
        "team_mode",
        "agents",
        "workspace_code",
        "workspace_id",
        "session_id",
    ] {
        assert!(raw["runs"][0].get(key).is_none(), "{key}");
    }
    assert!(rig.fake.0.borrow().orch_starts.is_empty());
}

fn patch_status(rig: &Rig, f: impl FnOnce(&mut Value)) {
    let path = rig.store().status_path();
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    f(&mut v);
    std::fs::write(&path, v.to_string()).unwrap();
}

fn resumed<'a>(rig: &'a Rig, s: &'a Settings, store: &'a Store) -> Autopilot<'a> {
    rig.term.store(false, Ordering::SeqCst);
    let mut ap = Autopilot::new(store, s, rig.fake.ports(), timing(), &rig.term);
    ap.begin();
    ap
}

#[test]
fn a_hard_limit_drain_and_the_final_pass_keep_the_planned_policy() {
    let rig = orch_rig(&[(2, "", "heavy"), (1, "", "eco")], &[]);
    let s = settings(&[1, 2], |_| {});
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_any(ap);
        rig.fake.add(running("q1", 1));
        rig.fake.w().probe = Ok(vec![(95.0, 95.0)]);
        rig.fake.advance(Duration::minutes(6));
        let before = rig.fake.0.borrow().passes.len();
        let _ = ap.tick();
        let w = rig.fake.0.borrow();
        let off: Vec<_> = w.passes[before..]
            .iter()
            .filter(|p| !p.starts_allowed)
            .collect();
        assert!(!off.is_empty());
        for p in off {
            let plan = p.plan.as_ref().expect("planned policy");
            assert_eq!(plan.order, vec![2, 1]);
            assert_eq!(plan.teams.len(), 2);
        }
    });
}

#[test]
fn the_final_pass_of_a_stop_keeps_the_planned_policy() {
    let rig = orch_rig(&[(1, "", "eco")], &[]);
    let s = orch_settings(&[1]);
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_any(ap);
        rig.control(1, "stop");
        assert!(ap.tick().unwrap().is_some());
    });
    let w = rig.fake.0.borrow();
    let last = w.passes.last().unwrap();
    assert!(!last.starts_allowed);
    assert_eq!(last.plan.as_ref().unwrap().teams.len(), 1);
}

#[test]
fn the_profile_and_order_apply_without_the_orchestrator() {
    let rig = Rig::new();
    rig.fake.add(pickup("q1", 1, "approved", None));
    let mut s = settings(&[1, 2], |a| a.profile = Some("claude-code".into()));
    s.order = vec![2, 1];
    with_ap(&rig, &s, tick_on);
    let policy = rig.fake.0.borrow().passes[0].clone();
    let plan = policy.plan.expect("a plan carries the profile");
    assert_eq!(plan.profile.as_deref(), Some("claude-code"));
    assert_eq!(plan.order, vec![2, 1]);
    assert!(plan.teams.is_empty() && plan.groups.is_empty());

    let rig = Rig::new();
    rig.fake.add(pickup("q1", 1, "approved", None));
    with_ap(&rig, &settings(&[1], |_| {}), tick_on);
    assert!(rig.fake.0.borrow().passes[0].plan.is_none());
}

#[test]
fn a_queue_mode_status_keeps_exactly_the_old_settings_keys() {
    let rig = Rig::new();
    rig.fake.add(running("q1", 1));
    let s = queue_settings();
    with_ap(&rig, &s, tick_on);
    let raw: Value =
        serde_json::from_str(&std::fs::read_to_string(rig.store().status_path()).unwrap()).unwrap();
    let mut keys: Vec<&str> = raw["settings"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    let mut want = vec![
        "all_tasks",
        "blocked_status",
        "feedback_status",
        "max_concurrent",
        "max_cost_usd",
        "max_hours",
        "max_tasks",
        "resume_after_reset",
        "rule",
        "usage_guard",
        "usage_hard",
        "usage_soft",
    ];
    want.sort_unstable();
    assert_eq!(keys, want);
    assert!(rig.store().read_status().unwrap().is_some());
}

fn paused_before_apply(rig: &Rig, s: &Settings) {
    with_ap(rig, s, |ap| {
        tick_on(ap);
        rig.control(1, "pause");
        tick_any(ap);
        let w = rig.fake.0.borrow();
        assert!(w.comments.is_empty() && w.status_sets.is_empty());
        drop(w);
        rig.term.store(true, Ordering::SeqCst);
        assert!(ap.tick().unwrap().unwrap().suspended);
    });
}

#[test]
fn triage_waits_while_paused_and_a_restored_plan_is_checked_again() {
    let rig = orch_rig(&[(1, "", "eco")], &[(2, "blocked"), (3, "needs_feedback")]);
    let s = orch_settings(&[1, 2, 3, 4, 5, 6]);
    paused_before_apply(&rig, &s);
    patch_status(&rig, |v| {
        let plan = &mut v["resume"]["plan"];
        plan["triage"][0]["reason"] = json!("see https://evil.example/x");
        plan["items"].as_array_mut().unwrap().push(json!({
            "num": 9, "task_id": 99, "key": "TM-99", "group": null, "mode": "eco", "reason": "x"
        }));
        plan["items"][0]["group"] = json!("Bad Group");
        plan["triage"].as_array_mut().unwrap().push(json!({
            "num": 8, "task_id": 98, "key": "TM-98", "verdict": "blocked", "reason": "r", "question": "q"
        }));
    });
    rig.control(2, "resume");
    let store = rig.store();
    let mut ap = resumed(&rig, &s, &store);
    tick_any(&mut ap);
    let w = rig.fake.0.borrow();
    assert_eq!(w.orch_starts.len(), 1);
    assert_eq!(w.comments.len(), 1);
    assert_eq!(w.comments[0].0, 3);
    drop(w);
    let plan = last_policy(&rig).plan.unwrap();
    assert!(!plan.teams.contains_key(&99));
    assert!(!plan.groups.contains_key(&1));
    let st = rig.status();
    assert_eq!(st.triage.len(), 1);
    assert_eq!(st.triage[0].task_id, 3);
}

#[test]
fn a_saved_plan_with_a_broken_input_hash_is_dropped_and_planned_again() {
    let rig = orch_rig(&[(1, "", "eco")], &[]);
    let s = orch_settings(&[1]);
    with_ap(&rig, &s, |ap| {
        tick_on(ap);
        tick_any(ap);
        rig.term.store(true, Ordering::SeqCst);
        assert!(ap.tick().unwrap().unwrap().suspended);
    });
    patch_status(&rig, |v| {
        v["resume"]["plan"]["input_sha256"] = json!("nope")
    });
    let store = rig.store();
    let mut ap = resumed(&rig, &s, &store);
    tick_any(&mut ap);
    assert_eq!(rig.fake.0.borrow().orch_starts.len(), 2);
}

#[test]
fn a_crash_between_the_comment_and_the_status_does_not_comment_twice() {
    let rig = orch_rig(&[(1, "", "eco")], &[(2, "blocked")]);
    let s = orch_settings(&[1, 2]);
    {
        let store = rig.store();
        let mut ap = Autopilot::new(&store, &s, rig.fake.ports(), timing(), &rig.term);
        ap.begin();
        tick_on(&mut ap);
        rig.fake.w().crash_on_set_status = true;
        let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = ap.tick();
        }));
        assert!(crashed.is_err());
    }
    assert_eq!(rig.fake.0.borrow().comments.len(), 1);
    rig.fake.w().crash_on_set_status = false;
    patch_status(&rig, |v| v["state"] = json!("suspended"));
    let store = rig.store();
    let mut ap = resumed(&rig, &s, &store);
    tick_any(&mut ap);
    let w = rig.fake.0.borrow();
    assert_eq!(w.comments.len(), 1);
    assert_eq!(w.status_sets, vec![(2, 2)]);
}

#[test]
fn a_leftover_orchestrator_run_is_killed_on_restore_before_a_new_attempt() {
    let rig = orch_rig(&[(1, "", "eco")], &[]);
    rig.fake.w().orch_poll = Poll::Running;
    let s = orch_settings(&[1]);
    with_ap(&rig, &s, tick_on);
    assert_eq!(rig.status().extra["resume"]["orch_run"], "orch-run-1");
    patch_status(&rig, |v| v["state"] = json!("suspended"));
    let store = rig.store();
    let mut ap = resumed(&rig, &s, &store);
    tick_on(&mut ap);
    let order = rig.fake.order();
    let kill = order
        .iter()
        .position(|o| o == "orch_kill orch-run-1")
        .unwrap();
    let start = order.iter().rposition(|o| o == "orch_start").unwrap();
    assert!(kill < start, "{order:?}");
    assert_eq!(rig.fake.0.borrow().orch_starts.len(), 2);
}
