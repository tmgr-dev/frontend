use super::ports::{
    Clock, Notifier, Ports, Probed, Queue, Remote, RunControl, Timing, Usage as UsagePort,
};
use super::*;
use crate::cmd::autopilot::model::Answers;
use chrono::TimeZone;
use std::cell::RefCell;
use tmgr_core::error::Error;
use tmgr_queue::state::Held;
use tmgr_queue::{PassReport, PausedDraft};
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
        }))
    }

    fn ports(&self) -> Ports<'_> {
        Ports {
            queue: self,
            runs: self,
            remote: self,
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
        "created_at": "2026-10-19T00:00:00Z", "updated_at": "2026-10-19T00:00:00Z",
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
        Ok(PausedDraft {
            pr_url: p.pr_url.clone(),
            held: false,
            commits: vec![],
        })
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

    fn statuses(&self, _ws: i64) -> Result<Vec<(u64, String)>> {
        Ok(vec![(1, "Needs feedback".into()), (2, "blocked".into())])
    }

    fn task_status_id(&self, task_id: i64) -> Result<Option<u64>> {
        Ok(self.0.borrow().task_status.get(&task_id).copied())
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
    }
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
fn sigterm_stops_like_the_stop_command() {
    let rig = Rig::new();
    rig.fake.add(running("q1", 1));
    let s = settings(&[1], |_| {});
    let out = with_ap(&rig, &s, |ap| {
        rig.term.store(true, Ordering::SeqCst);
        ap.tick().unwrap().expect("stopped")
    });
    assert_eq!(out.reason, "stopped by SIGTERM");
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
