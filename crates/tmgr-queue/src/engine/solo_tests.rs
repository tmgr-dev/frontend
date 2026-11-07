use tmgr_ipc::api::TeamMode;

fn solo_fx() -> Fx {
    let fx = cloud_fx();
    fx.cloud.tasks.replace(vec![signed(1, "do it")]);
    fx.pass().unwrap();
    fx
}

fn solo_cwd(fx: &Fx) -> String {
    fx.pickups().remove(0).bundle.launch.cwd
}

fn solo_marker(fx: &Fx) {
    let dir = std::path::Path::new(&solo_cwd(fx)).join(".tmgr");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("done"), "").unwrap();
}

fn solo_idle(fx: &Fx) {
    fx.runs.idle_runs.borrow_mut().push("mock-1".into());
}

fn solo_exits(fx: &Fx) -> usize {
    fx.runs
        .sent
        .borrow()
        .iter()
        .filter(|(r, t)| r == "mock-1" && t == "/exit\r")
        .count()
}

fn later(t0: DateTime<Utc>, secs: i64) -> DateTime<Utc> {
    t0 + chrono::Duration::seconds(secs)
}

#[test]
fn an_idle_solo_run_with_the_marker_is_asked_to_exit_once() {
    let fx = solo_fx();
    solo_marker(&fx);
    solo_idle(&fx);
    let t0 = Utc::now();
    fx.pass_now(t0).unwrap();
    fx.pass_now(later(t0, 5)).unwrap();
    assert_eq!(solo_exits(&fx), 1);
    assert_eq!(fx.audit_events("run_exit_sent"), 1);
}

#[test]
fn a_solo_run_without_the_marker_is_left_alone() {
    let fx = solo_fx();
    solo_idle(&fx);
    fx.pass().unwrap();
    assert_eq!(solo_exits(&fx), 0);
}

#[test]
fn a_busy_solo_run_with_the_marker_is_left_alone() {
    let fx = solo_fx();
    solo_marker(&fx);
    fx.pass().unwrap();
    assert_eq!(solo_exits(&fx), 0);
    assert_eq!(fx.audit_events("run_exit_sent"), 0);
}

#[test]
fn solo_exit_requests_are_capped_and_the_silence_is_noted_once() {
    let fx = solo_fx();
    solo_marker(&fx);
    solo_idle(&fx);
    let t0 = Utc::now();
    for secs in [0, 30, 60, 120] {
        fx.pass_now(later(t0, secs)).unwrap();
    }
    assert_eq!(solo_exits(&fx), 3);
    assert_eq!(fx.audit_events("run_exit_unanswered"), 0);
    fx.pass_now(later(t0, 180)).unwrap();
    fx.pass_now(later(t0, 600)).unwrap();
    assert_eq!(solo_exits(&fx), 3);
    assert_eq!(fx.audit_events("run_exit_unanswered"), 1);
}

#[test]
fn only_a_solo_claude_code_run_asks_for_the_marker() {
    let fx = solo_fx();
    assert!(fx.runs.requests.borrow()[0].finish_marker);

    let fx = Fx::new(|r| {
        r.workspace = "tmgrdev".into();
        r.project = Some("TM".into());
        r.auto = true;
        r.mode = Some("bypass".into());
        r.profile = "codex".into();
    });
    fx.cloud.tasks.replace(vec![signed(1, "do it")]);
    fx.pass().unwrap();
    assert!(!fx.runs.requests.borrow()[0].finish_marker);
    solo_marker(&fx);
    solo_idle(&fx);
    fx.pass().unwrap();
    assert_eq!(solo_exits(&fx), 0);

    let fx = cloud_fx();
    fx.cloud.tasks.replace(vec![signed(1, "do it")]);
    request_team(
        &fx.paths,
        TaskRef {
            workspace_id: 56,
            task_id: 1,
        },
        2,
        TeamMode::Eco,
        Utc::now(),
    )
    .unwrap();
    fx.pass().unwrap();
    assert!(!fx.runs.requests.borrow()[0].finish_marker);
}

#[test]
fn a_stale_marker_is_removed_before_the_run_starts() {
    let fx = cloud_fx();
    fx.cloud.tasks.replace(vec![signed(1, "do it")]);
    fx.pass_dirs(&StaleMarker, StartPolicy::default()).unwrap();
    assert_eq!(fx.runs.requests.borrow().len(), 1);
    let cwd = solo_cwd(&fx);
    assert!(!std::path::Path::new(&cwd).join(".tmgr/done").exists());
}

struct StaleMarker;

impl WorkDirs for StaleMarker {
    fn prepare(&self, repo: &str, pickup_id: &str, cwd: &str) -> Result<()> {
        GitClone.prepare(repo, pickup_id, cwd)?;
        self.plant(cwd);
        Ok(())
    }

    fn prepare_branch(
        &self,
        repo: &str,
        pickup_id: &str,
        cwd: &str,
        branch: &str,
    ) -> Result<()> {
        GitClone.prepare_branch(repo, pickup_id, cwd, branch)?;
        self.plant(cwd);
        Ok(())
    }
}

impl StaleMarker {
    fn plant(&self, cwd: &str) {
        let dir = std::path::Path::new(cwd).join(".tmgr");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("done"), "").unwrap();
    }
}

#[test]
fn a_solo_run_that_exits_after_the_request_reaches_publish() {
    let fx = solo_fx();
    solo_marker(&fx);
    solo_idle(&fx);
    fx.pass().unwrap();
    assert_eq!(solo_exits(&fx), 1);
    fx.runs.exited.borrow_mut().push("mock-1".into());
    for _ in 0..3 {
        fx.pass().unwrap();
    }
    assert_eq!(fx.publisher.calls.borrow().len(), 1);
}

#[test]
fn a_state_saved_before_the_solo_exit_fields_still_loads() {
    let fx = solo_fx();
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fx.paths.state()).unwrap()).unwrap();
    let p = &mut v["pickups"][0];
    for k in ["finish_marker", "exit_attempts", "exit_sent_at", "exit_noted"] {
        p.as_object_mut().unwrap().remove(k);
    }
    let p: Pickup = serde_json::from_value(p.clone()).unwrap();
    assert!(!p.finish_marker && p.exit_attempts == 0 && !p.exit_noted);
}
