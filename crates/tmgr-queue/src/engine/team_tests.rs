use crate::state::TeamPhase;
use std::path::Path;
use std::process::Command;
use tmgr_ipc::api::{TeamMode, TeamRole};

const WS: i64 = 56;

fn task() -> TaskRef {
    TaskRef {
        workspace_id: WS,
        task_id: 1,
    }
}

fn request(fx: &Fx, size: u8) {
    request_team(&fx.paths, task(), size, TeamMode::Eco, Utc::now()).unwrap();
}

fn team_fx(size: u8) -> Fx {
    let fx = cloud_fx();
    fx.cloud.tasks.replace(vec![signed(1, "do it")]);
    request(&fx, size);
    fx.pass().unwrap();
    fx
}

fn pickup(fx: &Fx) -> Pickup {
    fx.pickups().remove(0)
}

fn team(fx: &Fx) -> state::TeamState {
    pickup(fx).team.expect("team")
}

fn idle(fx: &Fx, run: &str) {
    fx.runs.idle_runs.borrow_mut().push(run.to_string());
}

fn plan_at(cwd: &str, text: &str) {
    let dir = Path::new(cwd).join(".tmgr");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("team-plan.json"), text).unwrap();
}

fn plan(fx: &Fx, text: &str) {
    plan_at(&pickup(fx).bundle.launch.cwd, text);
}

fn marker(cwd: &str) {
    let dir = Path::new(cwd).join(".tmgr");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("done"), "").unwrap();
}

fn git_in(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn commit_in(dir: &Path, file: &str) -> String {
    std::fs::write(dir.join(file), "work\n").unwrap();
    git_in(dir, &["add", file]);
    git_in(dir, &["commit", "-q", "-m", "work"]);
    git_in(dir, &["rev-parse", "HEAD"])
}

fn two_briefs() -> &'static str {
    r#"{"v":1,"workers":[{"brief":"do the api"},{"brief":"do the ui"}]}"#
}

fn exit_run(fx: &Fx, run: &str) {
    fx.runs.exited.borrow_mut().push(run.to_string());
}

fn working_team() -> Fx {
    let fx = team_fx(2);
    plan(&fx, two_briefs());
    idle(&fx, "mock-1");
    fx.pass().unwrap();
    fx
}

#[test]
fn the_phases_run_in_order_and_the_pull_request_is_opened_once() {
    let fx = team_fx(2);
    let p = pickup(&fx);
    assert!(p.bundle.launch.cwd.ends_with("/lead"));
    assert_eq!(team(&fx).phase, TeamPhase::Planning);
    assert!(state::load(&fx.paths).unwrap().team_requests.is_empty());
    let lead = fx.runs.requests.borrow()[0].clone();
    assert_eq!(lead.team.as_ref().unwrap().role, TeamRole::Lead);
    assert_eq!(lead.team.as_ref().unwrap().of, p.id);
    assert_eq!(lead.model.as_deref(), Some("sonnet"));
    assert_eq!(lead.bundle.launch.model.as_deref(), Some("sonnet"));
    assert_eq!(lead.bundle.team.unwrap().size, 2);

    fx.pass().unwrap();
    assert_eq!(fx.runs.created.borrow().len(), 1);

    plan(&fx, two_briefs());
    fx.pass().unwrap();
    assert_eq!(fx.runs.created.borrow().len(), 1, "a busy lead is not read");

    idle(&fx, "mock-1");
    fx.pass().unwrap();
    assert_eq!(team(&fx).phase, TeamPhase::Working);
    let reqs = fx.runs.requests.borrow().clone();
    assert_eq!(reqs.len(), 3);
    for (n, req) in reqs[1..].iter().enumerate() {
        let k = n as u8 + 1;
        assert_eq!(req.team.as_ref().unwrap().role, TeamRole::Worker(k));
        assert_eq!(req.model.as_deref(), Some("haiku"));
        assert!(req.start_key.ends_with(&format!("-1-w{k}")));
        assert!(req.cwd.ends_with(&format!("/w{k}")));
        assert!(req.bundle.team.is_none());
        assert_eq!(req.bundle.launch.cwd, req.cwd);
        assert!(req.bundle.assignment.is_some());
        assert!(Path::new(&req.cwd).join(".git").is_dir());
    }
    assert_eq!(reqs[1].bundle.assignment.as_deref(), Some("do the api"));
    assert_eq!(reqs[2].bundle.assignment.as_deref(), Some("do the ui"));
    assert_ne!(reqs[1].cwd, reqs[2].cwd);

    let w1 = team(&fx).members[0].clone();
    let tip = commit_in(Path::new(&w1.cwd), "api.txt");
    for member in &team(&fx).members {
        marker(&member.cwd);
    }
    idle(&fx, "mock-2");
    idle(&fx, "mock-3");
    fx.pass().unwrap();
    let exits: Vec<_> = fx
        .runs
        .sent
        .borrow()
        .iter()
        .filter(|(_, t)| t == "/exit\r")
        .map(|(r, _)| r.clone())
        .collect();
    assert_eq!(exits, ["mock-2", "mock-3"]);
    exit_run(&fx, "mock-2");
    exit_run(&fx, "mock-3");
    fx.pass().unwrap();
    assert_eq!(team(&fx).phase, TeamPhase::Finishing);
    let told = fx.runs.sent.borrow().last().cloned().unwrap();
    assert_eq!(told.0, "mock-1");
    assert!(told.1.contains(&format!("w1 {tip}")), "{}", told.1);
    assert!(!told.1.contains("w2 "), "{}", told.1);
    assert!(told.1.contains("git merge --squash"));
    assert!(told.1.ends_with('\r') && !told.1[..told.1.len() - 1].contains('\n'));
    assert_eq!(
        git_in(
            Path::new(&fx.repo),
            &["rev-parse", &format!("queue-team/{}/w1", pickup(&fx).id)]
        ),
        tip
    );

    fx.pass().unwrap();
    assert!(!team(&fx).lead_exit_sent, "no marker yet");
    marker(&pickup(&fx).bundle.launch.cwd);
    fx.pass().unwrap();
    assert!(team(&fx).lead_exit_sent);
    assert!(fx.publisher.calls.borrow().is_empty());
    exit_run(&fx, "mock-1");
    for _ in 0..4 {
        fx.pass().unwrap();
    }
    assert_eq!(fx.publisher.calls.borrow().len(), 1);
}

#[test]
fn an_invalid_plan_starts_nothing_and_is_noted_once() {
    let long = "x".repeat(tmgr_ipc::api::MAX_ASSIGNMENT_BYTES + 1);
    let cases = [
        r#"{"v":1,"workers":[{"brief":"a"},{"brief":"b"},{"brief":"c"}]}"#.to_string(),
        r#"{"v":1,"workers":[{"brief":"a","model":"opus"}]}"#.to_string(),
        r#"{"v":1,"workers":[{"brief":"a"}],"cwd":"/"}"#.to_string(),
        r#"{"v":2,"workers":[{"brief":"a"}]}"#.to_string(),
        r#"{"v":1,"workers":[{"brief":"  "}]}"#.to_string(),
        format!(r#"{{"v":1,"workers":[{{"brief":"{long}"}}]}}"#),
        format!("{{\"v\":1,\"workers\":[],\"pad\":\"{}\"}}", "y".repeat(200_000)),
        "not json".to_string(),
    ];
    for text in cases {
        let fx = team_fx(2);
        plan(&fx, &text);
        idle(&fx, "mock-1");
        fx.pass().unwrap();
        fx.pass().unwrap();
        assert_eq!(fx.runs.created.borrow().len(), 1, "{}", &text[..text.len().min(60)]);
        assert_eq!(team(&fx).phase, TeamPhase::Planning);
        let notes = fx.runs.sent.borrow().len();
        assert_eq!(notes, 1);
        let sent = fx.runs.sent.borrow()[0].1.clone();
        assert!(!sent.contains("opus") && !sent.contains("not json"));
    }
}

#[test]
fn a_plan_that_is_a_symlink_or_a_folder_starts_nothing() {
    let fx = team_fx(2);
    let cwd = pickup(&fx).bundle.launch.cwd;
    let outside = Path::new(&fx.repo).join("outside.json");
    std::fs::write(&outside, two_briefs()).unwrap();
    std::fs::create_dir_all(Path::new(&cwd).join(".tmgr")).unwrap();
    std::os::unix::fs::symlink(&outside, Path::new(&cwd).join(".tmgr/team-plan.json")).unwrap();
    idle(&fx, "mock-1");
    fx.pass().unwrap();
    assert_eq!(fx.runs.created.borrow().len(), 1);
    assert_eq!(team(&fx).phase, TeamPhase::Planning);
    assert_eq!(fx.runs.sent.borrow().len(), 1, "the lead is told once");

    let fx = team_fx(2);
    let cwd = pickup(&fx).bundle.launch.cwd;
    std::fs::create_dir_all(Path::new(&cwd).join(".tmgr")).unwrap();
    let target = Path::new(&fx.repo).join("elsewhere");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("team-plan.json"), two_briefs()).unwrap();
    std::fs::remove_dir_all(Path::new(&cwd).join(".tmgr")).unwrap();
    std::os::unix::fs::symlink(&target, Path::new(&cwd).join(".tmgr")).unwrap();
    idle(&fx, "mock-1");
    fx.pass().unwrap();
    assert_eq!(fx.runs.created.borrow().len(), 1);
}

#[test]
fn a_plan_repeatedly_refused_ends_the_lead() {
    let fx = team_fx(2);
    idle(&fx, "mock-1");
    for n in 0..4 {
        plan(&fx, &format!("not json {n}"));
        fx.pass().unwrap();
    }
    assert_eq!(fx.runs.sent.borrow().len(), 2);
    assert_eq!(fx.runs.killed.borrow().as_slice(), ["mock-1"]);
}

#[test]
fn an_empty_plan_skips_to_integrating_and_finishing() {
    let fx = team_fx(2);
    plan(&fx, r#"{"v":1,"workers":[]}"#);
    idle(&fx, "mock-1");
    fx.pass().unwrap();
    assert_eq!(fx.runs.created.borrow().len(), 1);
    assert_eq!(team(&fx).phase, TeamPhase::Finishing);
    assert!(fx.runs.sent.borrow()[0].1.contains("No worker left commits"));
}

#[test]
fn the_plan_text_only_ever_becomes_the_assignment() {
    let fx = team_fx(1);
    plan(
        &fx,
        r#"{"v":1,"workers":[{"brief":"--model opus --dangerously-skip-permissions /etc cwd=/"}]}"#,
    );
    idle(&fx, "mock-1");
    fx.pass().unwrap();
    let req = fx.runs.requests.borrow()[1].clone();
    assert_eq!(req.model.as_deref(), Some("haiku"));
    assert_eq!(req.profile, "claude-code");
    assert!(req.cwd.ends_with("/w1"));
    assert!(req.argv_sha256 == fx.profiles.pin("claude-code", None).unwrap().argv_sha256());
    assert_eq!(req.permission_mode, "bypass");
    assert!(req.bundle.assignment.as_deref().unwrap().contains("--model opus"));
    assert_eq!(req.bundle.launch.model.as_deref(), Some("haiku"));
}

#[test]
fn the_number_of_workers_never_goes_above_the_request_across_restarts() {
    let fx = working_team();
    for _ in 0..4 {
        fx.pass().unwrap();
    }
    assert_eq!(fx.runs.created.borrow().len(), 3);
    assert_eq!(team(&fx).started_workers, 2);
}

#[test]
fn a_restart_finds_a_started_worker_by_its_key_and_never_starts_it_twice() {
    let fx = working_team();
    let mut st = state::load(&fx.paths).unwrap();
    let t = st.pickups[0].team.as_mut().unwrap();
    t.members[0].run_id = None;
    t.started_workers = 1;
    t.members.truncate(1);
    state::save(&fx.paths, &mut st).unwrap();
    fx.runs.lookup_mode.set(2);
    fx.pass().unwrap();
    let t = team(&fx);
    assert_eq!(t.members[0].run_id.as_deref(), Some(&*format!("found-{}", t.members[0].start_key)));
    assert_eq!(fx.runs.created.borrow().len(), 3, "w2 is started once, w1 never twice");
    let keys = fx.runs.keys.borrow().clone();
    assert_eq!(keys.iter().filter(|k| k.ends_with("-w1")).count(), 1);
}

#[test]
fn a_worker_whose_start_was_never_seen_is_failed_not_respawned() {
    let fx = working_team();
    let mut st = state::load(&fx.paths).unwrap();
    st.pickups[0].team.as_mut().unwrap().members[0].run_id = None;
    state::save(&fx.paths, &mut st).unwrap();
    fx.runs.lookup_mode.set(1);
    fx.pass().unwrap();
    let t = team(&fx);
    assert!(t.members[0].exited && !t.members[0].clean);
    assert_eq!(fx.runs.created.borrow().len(), 3);
}

#[test]
fn the_death_of_the_lead_kills_the_workers_and_nothing_is_published() {
    let fx = working_team();
    fx.runs.failed.borrow_mut().push("mock-1".into());
    fx.pass().unwrap();
    let killed = fx.runs.killed.borrow().clone();
    assert!(killed.contains(&"mock-2".to_string()) && killed.contains(&"mock-3".to_string()));
    exit_run(&fx, "mock-2");
    exit_run(&fx, "mock-3");
    for _ in 0..4 {
        fx.pass().unwrap();
    }
    assert!(fx.publisher.calls.borrow().is_empty());
    assert!(!pickup(&fx).awaits_publish());
}

#[test]
fn a_lead_that_exits_cleanly_before_it_was_asked_to_is_not_published() {
    let fx = working_team();
    exit_run(&fx, "mock-1");
    fx.pass().unwrap();
    let p = pickup(&fx);
    assert!(p.run_exited && !p.run_clean);
    assert_eq!(fx.runs.killed.borrow().len(), 2);
    for _ in 0..3 {
        fx.pass().unwrap();
    }
    assert!(fx.publisher.calls.borrow().is_empty());
}

#[test]
fn no_publish_until_every_member_has_exited() {
    let fx = working_team();
    for member in &team(&fx).members {
        marker(&member.cwd);
    }
    idle(&fx, "mock-2");
    idle(&fx, "mock-3");
    exit_run(&fx, "mock-2");
    fx.pass().unwrap();
    fx.pass().unwrap();
    assert_eq!(team(&fx).phase, TeamPhase::Working);
    let mut st = state::load(&fx.paths).unwrap();
    st.pickups[0].run_exited = true;
    st.pickups[0].run_clean = true;
    state::save(&fx.paths, &mut st).unwrap();
    assert!(!pickup(&fx).awaits_publish());
}

#[test]
fn without_a_request_nothing_about_teams_appears() {
    let fx = cloud_fx();
    fx.cloud.tasks.replace(vec![signed(1, "do it")]);
    fx.pass().unwrap();
    idle(&fx, "mock-1");
    marker(&pickup(&fx).bundle.launch.cwd);
    plan(&fx, two_briefs());
    fx.pass().unwrap();
    exit_run(&fx, "mock-1");
    for _ in 0..4 {
        fx.pass().unwrap();
    }
    assert!(fx.runs.sent.borrow().is_empty());
    assert!(fx.runs.killed.borrow().is_empty());
    assert_eq!(fx.runs.created.borrow().len(), 1);
    assert_eq!(fx.publisher.calls.borrow().len(), 1);
    assert!(pickup(&fx).team.is_none());
    let text = std::fs::read_to_string(fx.paths.state()).unwrap();
    assert!(!text.contains("team"), "{text}");
    let req = fx.runs.requests.borrow()[0].clone();
    assert!(req.team.is_none() && req.bundle.team.is_none());
    assert!(!pickup(&fx).bundle.launch.cwd.ends_with("/lead"));
}

#[test]
fn a_marker_on_a_solo_pickup_does_nothing_and_on_a_team_only_causes_an_exit() {
    let fx = cloud_fx();
    fx.cloud.tasks.replace(vec![signed(1, "do it")]);
    fx.pass().unwrap();
    marker(&pickup(&fx).bundle.launch.cwd);
    idle(&fx, "mock-1");
    for _ in 0..3 {
        fx.pass().unwrap();
    }
    assert!(fx.runs.sent.borrow().is_empty());

    let fx = working_team();
    marker(&pickup(&fx).bundle.launch.cwd);
    idle(&fx, "mock-1");
    fx.pass().unwrap();
    assert!(
        fx.runs.sent.borrow().iter().all(|(r, _)| r != "mock-1"),
        "the lead marker counts only after integration"
    );
    let w = team(&fx).members[0].clone();
    marker(&w.cwd);
    fx.pass().unwrap();
    assert!(
        fx.runs.sent.borrow().iter().all(|(r, _)| r != "mock-2"),
        "a busy worker is not told to exit"
    );
    idle(&fx, "mock-2");
    fx.pass().unwrap();
    assert_eq!(
        fx.runs.sent.borrow().as_slice(),
        [("mock-2".to_string(), "/exit\r".to_string())]
    );
}

#[test]
fn a_lead_marker_written_before_the_integrate_text_never_ends_the_lead_early() {
    let fx = team_fx(1);
    let lead_cwd = pickup(&fx).bundle.launch.cwd;
    plan(&fx, r#"{"v":1,"workers":[]}"#);
    marker(&lead_cwd);
    idle(&fx, "mock-1");
    fx.pass().unwrap();
    fx.pass().unwrap();
    assert_eq!(team(&fx).phase, TeamPhase::Finishing);
    assert!(
        fx.runs.sent.borrow().iter().all(|(_, t)| t != "/exit\r"),
        "the stale marker ended the lead before it merged"
    );
    assert!(!team(&fx).lead_exit_sent);
    assert!(!Path::new(&lead_cwd).join(".tmgr/done").exists());
    marker(&lead_cwd);
    fx.pass().unwrap();
    assert!(team(&fx).lead_exit_sent);
}

#[test]
fn the_integrate_text_waits_for_the_lead_to_be_idle() {
    let fx = working_team();
    for member in &team(&fx).members {
        marker(&member.cwd);
    }
    exit_run(&fx, "mock-2");
    exit_run(&fx, "mock-3");
    fx.runs.sent.borrow_mut().clear();
    fx.runs.idle_runs.borrow_mut().clear();
    fx.pass().unwrap();
    assert!(fx.runs.sent.borrow().is_empty());
    assert_eq!(team(&fx).phase, TeamPhase::Integrating);
    idle(&fx, "mock-1");
    fx.pass().unwrap();
    assert_eq!(fx.runs.sent.borrow().len(), 1);
    assert_eq!(team(&fx).phase, TeamPhase::Finishing);
}

#[test]
fn clearing_the_lead_marker_never_follows_a_link() {
    let fx = team_fx(1);
    let lead_cwd = pickup(&fx).bundle.launch.cwd;
    plan(&fx, r#"{"v":1,"workers":[]}"#);
    let outside = Path::new(&fx.repo).parent().unwrap().join("outside-done");
    std::fs::write(&outside, "keep").unwrap();
    std::os::unix::fs::symlink(&outside, Path::new(&lead_cwd).join(".tmgr/done")).unwrap();
    idle(&fx, "mock-1");
    fx.pass().unwrap();
    assert_eq!(team(&fx).phase, TeamPhase::Finishing);
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "keep");

    let fx = team_fx(1);
    let lead_cwd = pickup(&fx).bundle.launch.cwd;
    plan(&fx, r#"{"v":1,"workers":[]}"#);
    let dir = Path::new(&fx.repo).parent().unwrap().join("outside-dir");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("done"), "keep").unwrap();
    std::fs::remove_dir_all(Path::new(&lead_cwd).join(".tmgr")).unwrap();
    std::os::unix::fs::symlink(&dir, Path::new(&lead_cwd).join(".tmgr")).unwrap();
    idle(&fx, "mock-1");
    fx.pass().unwrap();
    assert!(dir.join("done").is_file());
}

#[test]
fn a_marker_never_skips_a_failing_publish_gate() {
    let fx = team_fx(1);
    plan(&fx, r#"{"v":1,"workers":[]}"#);
    idle(&fx, "mock-1");
    fx.pass().unwrap();
    marker(&pickup(&fx).bundle.launch.cwd);
    fx.pass().unwrap();
    exit_run(&fx, "mock-1");
    fx.publisher.hold.set(true);
    for _ in 0..4 {
        fx.pass().unwrap();
    }
    let p = pickup(&fx);
    assert!(p.held.is_some() && p.pr_url.is_none());
    assert_eq!(fx.publisher.calls.borrow().len(), 1);
}

#[test]
fn a_team_over_an_inconsistent_local_rule_or_a_non_claude_team_is_refused() {
    let (fx, _) = multi_fx();
    fx.cloud.claim_repos.replace(vec![]);
    fx.cloud.tasks.replace(vec![signed(1, "do it")]);
    request(&fx, 2);
    fx.pass().unwrap();
    let p = pickup(&fx);
    assert_eq!(p.status, Status::Failed);
    assert!(p.reason.unwrap().contains("one repository"));
    assert!(fx.runs.created.borrow().is_empty());
    assert!(state::load(&fx.paths).unwrap().team_requests.is_empty());

    let other = Fx::new(|r| {
        r.profile = "codex".into();
    });
    other.source.0.replace(vec![cand(1, "b")]);
    request_team(
        &other.paths,
        TaskRef {
            workspace_id: -7,
            task_id: 1,
        },
        2,
        TeamMode::Eco,
        Utc::now(),
    )
    .unwrap();
    other.pass().unwrap();
    other.approve_all();
    other.pass().unwrap();
    let p = other.pickups().remove(0);
    assert_eq!(p.status, Status::Failed);
    assert!(p.reason.unwrap().contains("claude-code"));
    assert!(other.runs.created.borrow().is_empty());
}

#[test]
fn an_autopilot_policy_never_starts_a_task_with_a_team_request() {
    let fx = cloud_fx();
    fx.cloud.tasks.replace(vec![signed(1, "do it")]);
    request(&fx, 2);
    let only: BTreeSet<i64> = [1].into();
    let policy = StartPolicy {
        only: Some(only),
        ..StartPolicy::default()
    };
    fx.pass_with(policy).unwrap();
    assert!(fx.runs.created.borrow().is_empty());
    assert_eq!(state::load(&fx.paths).unwrap().team_requests.len(), 1);
    fx.pass().unwrap();
    assert_eq!(fx.runs.created.borrow().len(), 1);
}

#[test]
fn a_team_run_is_neither_released_paused_nor_continued_while_workers_run() {
    let fx = working_team();
    let id = pickup(&fx).id;
    assert!(fx.draft(&id).is_err());
    assert!(fx.requeue(&id, StartPolicy::default()).is_err());
    let mut st = state::load(&fx.paths).unwrap();
    st.pickups[0].status = Status::UnknownStart;
    state::save(&fx.paths, &mut st).unwrap();
    assert!(release(&fx.paths, &id, Utc::now()).is_err());
}

#[test]
fn a_failed_team_start_removes_the_whole_run_folder() {
    let fx = cloud_fx();
    fx.cloud.tasks.replace(vec![signed(1, "do it")]);
    request(&fx, 2);
    fx.runs.fail.set(true);
    fx.pass().unwrap();
    let p = pickup(&fx);
    assert_eq!(p.status, Status::Failed);
    let lead = Path::new(&p.bundle.launch.cwd);
    assert!(!lead.exists() && !lead.parent().unwrap().exists());
}

#[test]
fn discarding_a_team_folder_removes_every_clone_and_ref_and_never_follows_a_link() {
    let fx = working_team();
    let p = pickup(&fx);
    let team_dir = Path::new(&p.bundle.launch.cwd).parent().unwrap().to_path_buf();
    let w1 = team(&fx).members[0].clone();
    commit_in(Path::new(&w1.cwd), "a.txt");
    crate::workdir::import_branch(&fx.repo, Path::new(&w1.cwd), &w1.branch).unwrap();
    let victim = Path::new(&fx.repo).join("victim");
    std::fs::create_dir(&victim).unwrap();
    std::fs::write(victim.join("keep.txt"), "keep").unwrap();
    std::os::unix::fs::symlink(&victim, Path::new(&w1.cwd).join("link")).unwrap();
    assert!(!git_in(Path::new(&fx.repo), &["branch", "--list", "queue-team/*"]).is_empty());
    crate::workdir::discard_run_dir(&p.id, &p.bundle.launch.cwd);
    assert!(!team_dir.exists());
    assert!(victim.join("keep.txt").exists());
    assert!(git_in(Path::new(&fx.repo), &["branch", "--list", "queue-team/*"]).is_empty());
}

#[test]
fn a_hostile_copy_never_runs_anything_while_the_engine_reads_and_imports_it() {
    let fx = working_team();
    let t = team(&fx);
    let lead_cwd = pickup(&fx).bundle.launch.cwd;
    let marker_file = Path::new(&fx.repo).parent().unwrap().join("pwned");
    let m = marker_file.display();
    let w1 = t.members[0].clone();
    let tip = commit_in_plain(Path::new(&w1.cwd), "api.txt");
    for dir in [lead_cwd.as_str(), w1.cwd.as_str()] {
        let dir = Path::new(dir);
        let mut cfg = std::fs::OpenOptions::new()
            .append(true)
            .open(dir.join(".git/config"))
            .unwrap();
        use std::io::Write;
        writeln!(
            cfg,
            "[core]\n\tfsmonitor = touch {m}\n\tsshCommand = touch {m}\n\thooksPath = {hooks}\n[filter \"x\"]\n\tclean = touch {m}\n\tsmudge = touch {m}\n\tprocess = touch {m}\n[uploadpack]\n\tpackObjectsHook = touch {m}\n[credential]\n\thelper = !touch {m}\n",
            hooks = dir.join(".git/hooks").display()
        )
        .unwrap();
        std::fs::create_dir_all(dir.join(".git/hooks")).unwrap();
        for hook in ["pre-commit", "post-checkout", "post-merge", "reference-transaction", "pre-push", "fsmonitor-watchman"] {
            let path = dir.join(".git/hooks").join(hook);
            std::fs::write(&path, format!("#!/bin/sh\ntouch {m}\n")).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::write(dir.join(".gitattributes"), "* filter=x\n").unwrap();
    }
    plan(&fx, two_briefs());
    marker(&w1.cwd);
    idle(&fx, "mock-2");
    idle(&fx, "mock-3");
    exit_run(&fx, "mock-2");
    exit_run(&fx, "mock-3");
    for _ in 0..3 {
        fx.pass().unwrap();
    }
    assert_eq!(team(&fx).phase, TeamPhase::Finishing);
    assert!(fx.runs.sent.borrow().iter().any(|(_, t)| t.contains(&tip)));
    assert!(!marker_file.exists(), "something ran inside an agent-writable copy");
}

fn commit_in_plain(dir: &Path, file: &str) -> String {
    std::fs::write(dir.join(file), "work\n").unwrap();
    let tree = std::process::Command::new("git")
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(["-c", "core.fsmonitor=false", "-c", "core.hooksPath=/dev/null", "-c", "core.attributesFile=/dev/null", "add", "--", file])
        .output()
        .unwrap();
    assert!(tree.status.success());
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .args(["-c", "core.fsmonitor=false", "-c", "core.hooksPath=/dev/null", "commit", "-q", "-m", "work", "--no-verify"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    git_plain(dir, &["rev-parse", "HEAD"])
}

fn git_plain(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(["-c", "core.fsmonitor=false", "-c", "core.hooksPath=/dev/null"])
        .args(args)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn the_runner_is_still_needed_while_a_worker_runs_after_the_lead_has_gone() {
    let fx = working_team();
    let mut st = state::load(&fx.paths).unwrap();
    st.paused = true;
    state::save(&fx.paths, &mut st).unwrap();
    assert!(needs_runner(&fx.paths).unwrap());
    let mut st = state::load(&fx.paths).unwrap();
    st.pickups[0].run_exited = true;
    st.pickups[0].held = Some(state::Held::default());
    state::save(&fx.paths, &mut st).unwrap();
    assert!(needs_runner(&fx.paths).unwrap());
    let mut st = state::load(&fx.paths).unwrap();
    for m in &mut st.pickups[0].team.as_mut().unwrap().members {
        m.exited = true;
    }
    state::save(&fx.paths, &mut st).unwrap();
    assert!(!needs_runner(&fx.paths).unwrap());
}

#[test]
fn a_released_pickup_stays_listed_only_while_its_team_is_live() {
    let fx = working_team();
    let mut p = pickup(&fx);
    assert!(p.listed());
    p.released = true;
    assert!(p.listed());
    p.status = Status::Failed;
    assert!(!p.listed());
    p.status = Status::Started;
    p.team = None;
    assert!(!p.listed());
}

fn live_workers(fx: &Fx) -> usize {
    team(fx)
        .members
        .iter()
        .filter(|m| !m.exited && m.role != TeamRole::Lead)
        .count()
}

#[test]
fn a_failed_kill_is_retried_on_the_next_pass() {
    let fx = working_team();
    fx.runs.kill_failures.set(2);
    exit_run(&fx, "mock-1");
    fx.pass().unwrap();
    assert!(fx.runs.killed.borrow().is_empty());
    assert_eq!(live_workers(&fx), 2);
    fx.pass().unwrap();
    let killed = fx.runs.killed.borrow().clone();
    assert!(killed.contains(&"mock-2".to_string()) && killed.contains(&"mock-3".to_string()));
    assert_eq!(live_workers(&fx), 2);
}

fn kills_of(fx: &Fx, run: &str) -> usize {
    fx.runs.killed.borrow().iter().filter(|r| *r == run).count()
}

#[test]
fn an_acknowledged_kill_does_not_end_a_worker_that_still_runs() {
    let fx = working_team();
    exit_run(&fx, "mock-1");
    fx.pass().unwrap();
    assert_eq!(kills_of(&fx, "mock-2"), 1);
    assert_eq!(live_workers(&fx), 2);
    assert!(needs_runner(&fx.paths).unwrap());
    fx.pass().unwrap();
    assert_eq!(kills_of(&fx, "mock-2"), 2, "the kill is sent again");
    assert!(fx.runs.forced.borrow().is_empty());
    assert_eq!(fx.audit_events("team_member_killed"), 2);
    assert!(fx.publisher.calls.borrow().is_empty());
}

#[test]
fn a_worker_that_outlives_the_grace_period_is_killed_by_force() {
    let fx = working_team();
    exit_run(&fx, "mock-1");
    fx.pass().unwrap();
    let mut st = state::load(&fx.paths).unwrap();
    let old = (Utc::now() - chrono::Duration::seconds(crate::engine::team::KILL_GRACE_SECS + 5))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    for m in &mut st.pickups[0].team.as_mut().unwrap().members {
        if m.kill_sent_at.is_some() {
            m.kill_sent_at = Some(old.clone());
        }
    }
    state::save(&fx.paths, &mut st).unwrap();
    fx.pass().unwrap();
    let forced = fx.runs.forced.borrow().clone();
    assert!(forced.contains(&"mock-2".to_string()) && forced.contains(&"mock-3".to_string()));
    exit_run(&fx, "mock-2");
    exit_run(&fx, "mock-3");
    fx.pass().unwrap();
    assert_eq!(live_workers(&fx), 0);
}

#[test]
fn a_worker_the_runner_no_longer_knows_counts_as_ended() {
    let fx = working_team();
    exit_run(&fx, "mock-1");
    fx.pass().unwrap();
    fx.runs.failed.borrow_mut().push("mock-2".into());
    fx.runs.failed.borrow_mut().push("mock-3".into());
    fx.pass().unwrap();
    assert_eq!(live_workers(&fx), 0);
    assert!(team(&fx).members.iter().all(|m| !m.clean || m.role == TeamRole::Lead));
}

#[test]
fn a_clean_exit_of_the_lead_still_ends_live_workers() {
    let fx = working_team();
    let mut st = state::load(&fx.paths).unwrap();
    let t = st.pickups[0].team.as_mut().unwrap();
    t.phase = TeamPhase::Finishing;
    t.lead_exit_sent = true;
    state::save(&fx.paths, &mut st).unwrap();
    exit_run(&fx, "mock-1");
    fx.pass().unwrap();
    assert_eq!(fx.runs.killed.borrow().len(), 2);
}

#[test]
fn workers_are_killed_when_the_pickup_is_in_any_other_status() {
    for status in [Status::Failed, Status::Expired, Status::UnknownStart] {
        let fx = working_team();
        let mut st = state::load(&fx.paths).unwrap();
        st.pickups[0].status = status;
        state::save(&fx.paths, &mut st).unwrap();
        fx.pass().unwrap();
        let killed = fx.runs.killed.borrow().clone();
        assert!(
            killed.contains(&"mock-2".to_string()) && killed.contains(&"mock-3".to_string()),
            "{status:?}"
        );
        assert_eq!(live_workers(&fx), 2, "{status:?}");
    }
}

#[test]
fn the_runner_is_needed_for_live_workers_in_any_status() {
    let fx = working_team();
    let mut st = state::load(&fx.paths).unwrap();
    st.paused = true;
    st.pickups[0].status = Status::Failed;
    state::save(&fx.paths, &mut st).unwrap();
    assert!(needs_runner(&fx.paths).unwrap());
}

#[test]
fn a_released_pickup_with_live_workers_is_never_pruned_from_the_state() {
    let fx = working_team();
    let id = pickup(&fx).id;
    let mut st = state::load(&fx.paths).unwrap();
    let mut spare = st.pickups[0].clone();
    spare.team = None;
    spare.status = Status::Failed;
    spare.released = true;
    st.pickups[0].released = true;
    st.pickups[0].run_exited = true;
    for n in 0..(state::KEEP_RELEASED + 2) {
        let mut extra = spare.clone();
        extra.id = format!("x{n}");
        st.pickups.push(extra);
    }
    state::save(&fx.paths, &mut st).unwrap();
    assert!(fx.pickups().iter().any(|p| p.id == id));
}

fn lead_gone_with_live_workers() -> Fx {
    let fx = working_team();
    exit_run(&fx, "mock-1");
    fx
}

fn turn_queue_off(fx: &Fx) {
    let mut file = rules::load(&fx.paths).unwrap().file;
    file.enabled = false;
    fx.write(file);
}

fn assert_workers_killed_and_nothing_else(fx: &Fx, created: usize) {
    let killed = fx.runs.killed.borrow().clone();
    assert!(killed.contains(&"mock-2".to_string()) && killed.contains(&"mock-3".to_string()));
    assert_eq!(live_workers(fx), 2, "a kill is a request until the runner reports the exit");
    exit_run(fx, "mock-2");
    exit_run(fx, "mock-3");
    let _ = fx.pass();
    assert_eq!(live_workers(fx), 0);
    assert_eq!(fx.runs.created.borrow().len(), created);
    assert!(fx.publisher.calls.borrow().is_empty());
}

#[test]
fn a_disabled_queue_still_kills_the_workers_of_a_dead_lead() {
    let fx = lead_gone_with_live_workers();
    let created = fx.runs.created.borrow().len();
    turn_queue_off(&fx);
    let report = fx.pass().unwrap();
    assert!(!report.enabled);
    assert_workers_killed_and_nothing_else(&fx, created);
}

#[test]
fn a_disabled_queue_leaves_workers_alone_while_the_lead_runs() {
    let fx = working_team();
    turn_queue_off(&fx);
    fx.pass().unwrap();
    assert!(fx.runs.killed.borrow().is_empty());
    assert_eq!(live_workers(&fx), 2);
}

#[test]
fn a_tampered_config_still_kills_the_workers_and_returns_forbidden() {
    let fx = lead_gone_with_live_workers();
    let created = fx.runs.created.borrow().len();
    let text = std::fs::read_to_string(fx.paths.rules()).unwrap();
    std::fs::write(fx.paths.rules(), format!("{text}\n# edited\n")).unwrap();
    std::fs::set_permissions(
        fx.paths.rules(),
        std::os::unix::fs::PermissionsExt::from_mode(0o600),
    )
    .unwrap();
    let e = fx.pass().err().unwrap();
    assert_eq!(e.exit_code(), 5);
    assert_workers_killed_and_nothing_else(&fx, created);
}

#[test]
fn a_disabled_queue_needs_the_runner_only_while_a_team_is_live() {
    let fx = lead_gone_with_live_workers();
    turn_queue_off(&fx);
    assert!(needs_runner(&fx.paths).unwrap());
    fx.pass().unwrap();
    assert!(needs_runner(&fx.paths).unwrap(), "workers still run");
    exit_run(&fx, "mock-2");
    exit_run(&fx, "mock-3");
    fx.pass().unwrap();
    assert!(!needs_runner(&fx.paths).unwrap());
}

fn after(t0: DateTime<Utc>, secs: i64) -> DateTime<Utc> {
    t0 + chrono::Duration::seconds(secs)
}

fn busy(fx: &Fx, run: &str) {
    fx.runs.idle_runs.borrow_mut().retain(|r| r != run);
}

fn sent_matching(fx: &Fx, run: &str, needle: &str) -> usize {
    fx.runs
        .sent
        .borrow()
        .iter()
        .filter(|(r, t)| r == run && t.contains(needle))
        .count()
}

fn exits_to(fx: &Fx, run: &str) -> usize {
    sent_matching(fx, run, "/exit\r")
}

fn idle_notes(fx: &Fx) -> usize {
    sent_matching(fx, "mock-1", "waiting at")
}

fn lead_cwd(fx: &Fx) -> String {
    pickup(fx).bundle.launch.cwd
}

fn end_file(fx: &Fx, name: &str) -> std::path::PathBuf {
    let dir = Path::new(&lead_cwd(fx)).join(".tmgr");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, "").unwrap();
    path
}

fn worker_cwd(fx: &Fx, k: usize) -> String {
    team(fx).members[k - 1].cwd.clone()
}

#[test]
fn an_idle_worker_is_reported_once_per_stretch_after_ten_minutes() {
    let fx = working_team();
    let t0 = Utc::now();
    idle(&fx, "mock-2");
    fx.pass_now(t0).unwrap();
    fx.pass_now(after(t0, 599)).unwrap();
    assert_eq!(idle_notes(&fx), 0);
    fx.pass_now(after(t0, 600)).unwrap();
    assert_eq!(idle_notes(&fx), 1);
    let note = fx.runs.sent.borrow().last().cloned().unwrap();
    assert!(note.1.contains("w1") && note.1.contains(".tmgr/end-w1"), "{}", note.1);
    assert!(note.1.ends_with('\r') && !note.1[..note.1.len() - 1].contains('\n'));
    fx.pass_now(after(t0, 700)).unwrap();
    fx.pass_now(after(t0, 1400)).unwrap();
    assert_eq!(idle_notes(&fx), 1);
    assert_eq!(fx.audit_events("team_worker_idle"), 1);

    busy(&fx, "mock-2");
    fx.pass_now(after(t0, 1500)).unwrap();
    idle(&fx, "mock-2");
    fx.pass_now(after(t0, 1600)).unwrap();
    fx.pass_now(after(t0, 2199)).unwrap();
    assert_eq!(idle_notes(&fx), 1);
    fx.pass_now(after(t0, 2200)).unwrap();
    assert_eq!(idle_notes(&fx), 2);
}

#[test]
fn several_idle_workers_share_one_note() {
    let fx = working_team();
    let t0 = Utc::now();
    idle(&fx, "mock-2");
    idle(&fx, "mock-3");
    fx.pass_now(t0).unwrap();
    fx.pass_now(after(t0, 600)).unwrap();
    assert_eq!(idle_notes(&fx), 1);
    let note = fx.runs.sent.borrow().last().cloned().unwrap().1;
    assert!(note.contains("w1") && note.contains("w2"), "{note}");
    assert_eq!(fx.audit_events("team_worker_idle"), 2);
}

#[test]
fn the_idle_note_waits_for_a_lead_that_is_busy() {
    let fx = working_team();
    let t0 = Utc::now();
    idle(&fx, "mock-2");
    busy(&fx, "mock-1");
    fx.pass_now(t0).unwrap();
    fx.pass_now(after(t0, 600)).unwrap();
    fx.pass_now(after(t0, 630)).unwrap();
    assert_eq!(idle_notes(&fx), 0);
    assert!(!team(&fx).members[0].idle_noted);
    idle(&fx, "mock-1");
    fx.pass_now(after(t0, 660)).unwrap();
    assert_eq!(idle_notes(&fx), 1);
    fx.pass_now(after(t0, 690)).unwrap();
    assert_eq!(idle_notes(&fx), 1);
}

#[test]
fn a_merely_idle_worker_is_never_stopped() {
    let fx = working_team();
    let t0 = Utc::now();
    idle(&fx, "mock-2");
    for n in 0..10 {
        fx.pass_now(after(t0, n * 600)).unwrap();
    }
    assert_eq!(exits_to(&fx, "mock-2"), 0);
    assert!(fx.runs.killed.borrow().is_empty());
}

#[test]
fn an_end_file_from_the_lead_asks_that_worker_to_exit_and_is_removed() {
    let fx = working_team();
    let t0 = Utc::now();
    let file = end_file(&fx, "end-w1");
    fx.pass_now(t0).unwrap();
    assert!(!file.exists());
    assert_eq!(exits_to(&fx, "mock-2"), 0, "a busy worker is not typed at");
    assert_eq!(fx.audit_events("team_worker_end_requested"), 1);
    idle(&fx, "mock-2");
    fx.pass_now(after(t0, 30)).unwrap();
    assert_eq!(exits_to(&fx, "mock-2"), 1);
    assert_eq!(exits_to(&fx, "mock-3"), 0);
}

#[test]
fn an_end_file_for_an_unknown_or_finished_worker_is_ignored_and_removed() {
    let fx = working_team();
    exit_run(&fx, "mock-3");
    idle(&fx, "mock-2");
    idle(&fx, "mock-3");
    let t0 = Utc::now();
    fx.pass_now(t0).unwrap();
    let nine = end_file(&fx, "end-w9");
    let done = end_file(&fx, "end-w2");
    fx.pass_now(after(t0, 30)).unwrap();
    assert!(!nine.exists() && !done.exists());
    assert_eq!(exits_to(&fx, "mock-2") + exits_to(&fx, "mock-3"), 0);
    assert_eq!(fx.audit_events("team_worker_end_requested"), 0);
}

#[test]
fn an_end_file_that_is_a_symlink_is_not_followed() {
    let fx = working_team();
    idle(&fx, "mock-2");
    let outside = Path::new(&fx.repo).join("outside");
    std::fs::write(&outside, "").unwrap();
    let dir = Path::new(&lead_cwd(&fx)).join(".tmgr");
    std::fs::create_dir_all(&dir).unwrap();
    std::os::unix::fs::symlink(&outside, dir.join("end-w1")).unwrap();
    fx.pass_now(Utc::now()).unwrap();
    assert_eq!(exits_to(&fx, "mock-2"), 0);
    assert!(outside.exists());
}

#[test]
fn exit_is_resent_every_minute_up_to_three_times_and_only_while_the_marker_stays() {
    let fx = working_team();
    let t0 = Utc::now();
    marker(&worker_cwd(&fx, 1));
    idle(&fx, "mock-2");
    fx.pass_now(t0).unwrap();
    fx.pass_now(after(t0, 59)).unwrap();
    assert_eq!(exits_to(&fx, "mock-2"), 1);
    fx.pass_now(after(t0, 60)).unwrap();
    assert_eq!(exits_to(&fx, "mock-2"), 2);
    fx.pass_now(after(t0, 120)).unwrap();
    assert_eq!(exits_to(&fx, "mock-2"), 3);
    fx.pass_now(after(t0, 178)).unwrap();
    assert_eq!(exits_to(&fx, "mock-2"), 3);

    let fx = working_team();
    marker(&worker_cwd(&fx, 1));
    idle(&fx, "mock-2");
    fx.pass_now(t0).unwrap();
    std::fs::remove_file(Path::new(&worker_cwd(&fx, 1)).join(".tmgr/done")).unwrap();
    fx.pass_now(after(t0, 60)).unwrap();
    assert_eq!(exits_to(&fx, "mock-2"), 1);
}

#[test]
fn a_busy_worker_is_not_sent_exit_again() {
    let fx = working_team();
    let t0 = Utc::now();
    marker(&worker_cwd(&fx, 1));
    idle(&fx, "mock-2");
    fx.pass_now(t0).unwrap();
    busy(&fx, "mock-2");
    fx.pass_now(after(t0, 60)).unwrap();
    fx.pass_now(after(t0, 90)).unwrap();
    assert_eq!(exits_to(&fx, "mock-2"), 1);
    idle(&fx, "mock-2");
    fx.pass_now(after(t0, 120)).unwrap();
    assert_eq!(exits_to(&fx, "mock-2"), 2);
}

#[test]
fn a_done_worker_that_ignores_exit_is_stopped_and_its_commits_still_integrate() {
    let fx = working_team();
    let t0 = Utc::now();
    let tip = commit_in(Path::new(&worker_cwd(&fx, 1)), "api.txt");
    marker(&worker_cwd(&fx, 1));
    idle(&fx, "mock-2");
    for secs in [0, 60, 120] {
        fx.pass_now(after(t0, secs)).unwrap();
    }
    fx.pass_now(after(t0, 179)).unwrap();
    assert!(fx.runs.killed.borrow().is_empty());
    fx.pass_now(after(t0, 180)).unwrap();
    assert_eq!(fx.runs.killed.borrow().as_slice(), ["mock-2"]);
    assert_eq!(fx.audit_events("team_worker_exit_unanswered"), 1);
    fx.pass_now(after(t0, 190)).unwrap();
    assert_eq!(fx.audit_events("team_worker_exit_unanswered"), 1);
    assert!(fx.runs.forced.borrow().is_empty());
    fx.pass_now(after(t0, 215)).unwrap();
    assert_eq!(fx.runs.forced.borrow().as_slice(), ["mock-2"]);
    assert_eq!(idle_notes(&fx), 0);

    fx.runs.failed.borrow_mut().push("mock-2".into());
    exit_run(&fx, "mock-3");
    fx.pass_now(after(t0, 240)).unwrap();
    assert_eq!(team(&fx).phase, TeamPhase::Finishing);
    let told = fx.runs.sent.borrow().last().cloned().unwrap();
    assert_eq!(told.0, "mock-1");
    assert!(told.1.contains(&format!("w1 {tip}")), "{}", told.1);
}

#[test]
fn a_worker_ended_by_the_lead_that_ignores_exit_is_stopped_too() {
    let fx = working_team();
    let t0 = Utc::now();
    end_file(&fx, "end-w2");
    idle(&fx, "mock-3");
    for secs in [0, 60, 120, 180] {
        fx.pass_now(after(t0, secs)).unwrap();
    }
    assert_eq!(exits_to(&fx, "mock-3"), 3);
    assert_eq!(fx.runs.killed.borrow().as_slice(), ["mock-3"]);
}

#[test]
fn the_lead_exit_is_retried_and_never_killed() {
    let fx = working_team();
    let mut st = state::load(&fx.paths).unwrap();
    st.pickups[0].team.as_mut().unwrap().phase = TeamPhase::Finishing;
    state::save(&fx.paths, &mut st).unwrap();
    marker(&lead_cwd(&fx));
    let t0 = Utc::now();
    fx.pass_now(t0).unwrap();
    assert!(team(&fx).lead_exit_sent);
    fx.pass_now(after(t0, 59)).unwrap();
    assert_eq!(exits_to(&fx, "mock-1"), 1);
    fx.pass_now(after(t0, 60)).unwrap();
    fx.pass_now(after(t0, 120)).unwrap();
    assert_eq!(exits_to(&fx, "mock-1"), 3);
    assert_eq!(fx.audit_events("team_lead_exit_unanswered"), 0);
    fx.pass_now(after(t0, 180)).unwrap();
    fx.pass_now(after(t0, 600)).unwrap();
    assert_eq!(exits_to(&fx, "mock-1"), 3);
    assert_eq!(fx.audit_events("team_lead_exit_unanswered"), 1);
    assert!(!fx.runs.killed.borrow().contains(&"mock-1".to_string()));
}

#[test]
fn a_state_saved_with_the_old_exit_flag_is_not_resent_at_once() {
    let fx = working_team();
    let t0 = Utc::now();
    marker(&worker_cwd(&fx, 1));
    idle(&fx, "mock-2");
    let mut st = state::load(&fx.paths).unwrap();
    st.pickups[0].team.as_mut().unwrap().members[0].exit_sent = true;
    state::save(&fx.paths, &mut st).unwrap();
    fx.pass_now(t0).unwrap();
    assert_eq!(exits_to(&fx, "mock-2"), 0);
    fx.pass_now(after(t0, 60)).unwrap();
    assert_eq!(exits_to(&fx, "mock-2"), 1);

    let old = r#"{"role":"lead","cwd":"/x","branch":"b","start_key":"k","exit_sent":true}"#;
    assert!(serde_json::from_str::<state::Member>(old).is_ok());
}

#[test]
fn a_worker_that_removes_its_marker_after_exit_is_reported_idle_again() {
    let fx = working_team();
    let t0 = Utc::now();
    marker(&worker_cwd(&fx, 1));
    idle(&fx, "mock-2");
    fx.pass_now(t0).unwrap();
    assert_eq!(exits_to(&fx, "mock-2"), 1);
    std::fs::remove_file(Path::new(&worker_cwd(&fx, 1)).join(".tmgr/done")).unwrap();
    fx.pass_now(after(t0, 10)).unwrap();
    fx.pass_now(after(t0, 609)).unwrap();
    assert_eq!(idle_notes(&fx), 0);
    fx.pass_now(after(t0, 610)).unwrap();
    assert_eq!(idle_notes(&fx), 1);
}

#[test]
fn a_worker_with_a_legacy_exit_flag_and_no_marker_is_reported_idle() {
    let fx = working_team();
    let t0 = Utc::now();
    idle(&fx, "mock-2");
    let mut st = state::load(&fx.paths).unwrap();
    st.pickups[0].team.as_mut().unwrap().members[0].exit_sent = true;
    state::save(&fx.paths, &mut st).unwrap();
    fx.pass_now(t0).unwrap();
    fx.pass_now(after(t0, 600)).unwrap();
    assert_eq!(idle_notes(&fx), 1);
}

#[test]
fn an_idle_stretch_starts_after_the_marker_is_removed() {
    let fx = working_team();
    let t0 = Utc::now();
    idle(&fx, "mock-2");
    fx.pass_now(t0).unwrap();
    busy(&fx, "mock-2");
    marker(&worker_cwd(&fx, 1));
    fx.pass_now(after(t0, 500)).unwrap();
    std::fs::remove_file(Path::new(&worker_cwd(&fx, 1)).join(".tmgr/done")).unwrap();
    idle(&fx, "mock-2");
    fx.pass_now(after(t0, 700)).unwrap();
    assert_eq!(idle_notes(&fx), 0);
    fx.pass_now(after(t0, 1299)).unwrap();
    assert_eq!(idle_notes(&fx), 0);
    fx.pass_now(after(t0, 1300)).unwrap();
    assert_eq!(idle_notes(&fx), 1);
}

#[test]
fn a_timestamp_in_the_future_restarts_the_wait() {
    let fx = working_team();
    let t0 = Utc::now();
    idle(&fx, "mock-2");
    fx.pass_now(t0).unwrap();
    fx.pass_now(after(t0, -1000)).unwrap();
    fx.pass_now(after(t0, -401)).unwrap();
    assert_eq!(idle_notes(&fx), 0);
    fx.pass_now(after(t0, -400)).unwrap();
    assert_eq!(idle_notes(&fx), 1);

    let fx = working_team();
    marker(&worker_cwd(&fx, 1));
    idle(&fx, "mock-2");
    fx.pass_now(t0).unwrap();
    fx.pass_now(after(t0, -1000)).unwrap();
    fx.pass_now(after(t0, -941)).unwrap();
    assert_eq!(exits_to(&fx, "mock-2"), 1);
    fx.pass_now(after(t0, -940)).unwrap();
    assert_eq!(exits_to(&fx, "mock-2"), 2);
}

#[test]
fn a_lead_exit_timestamp_in_the_future_restarts_the_wait() {
    let fx = working_team();
    let mut st = state::load(&fx.paths).unwrap();
    st.pickups[0].team.as_mut().unwrap().phase = TeamPhase::Finishing;
    state::save(&fx.paths, &mut st).unwrap();
    marker(&lead_cwd(&fx));
    let t0 = Utc::now();
    fx.pass_now(t0).unwrap();
    fx.pass_now(after(t0, -1000)).unwrap();
    fx.pass_now(after(t0, -941)).unwrap();
    assert_eq!(exits_to(&fx, "mock-1"), 1);
    fx.pass_now(after(t0, -940)).unwrap();
    assert_eq!(exits_to(&fx, "mock-1"), 2);
}

fn team_multi_fx(size: u8) -> (Fx, String) {
    let (fx, second) = multi_fx();
    fx.cloud.tasks.replace(vec![signed(1, "do it")]);
    request(&fx, size);
    fx.pass().unwrap();
    (fx, second)
}

fn has_branch(repo: &str, branch: &str) -> bool {
    Command::new("git")
        .args(["-C", repo, "rev-parse", "--verify", "-q"])
        .arg(format!("refs/heads/{branch}"))
        .output()
        .unwrap()
        .status
        .success()
}

fn branch_of(dir: &Path) -> String {
    git_in(dir, &["rev-parse", "--abbrev-ref", "HEAD"])
}

fn working_multi_team() -> (Fx, String) {
    let (fx, second) = team_multi_fx(2);
    plan(&fx, two_briefs());
    idle(&fx, "mock-1");
    fx.pass().unwrap();
    (fx, second)
}

#[test]
fn a_multi_repo_team_start_makes_a_lead_folder_with_one_clone_per_repository() {
    let (fx, second) = team_multi_fx(2);
    let p = pickup(&fx);
    assert_eq!(p.status, Status::Started);
    let lead = fx.runs.requests.borrow()[0].clone();
    assert_eq!(lead.team.as_ref().unwrap().role, TeamRole::Lead);
    assert_eq!(lead.cwd, p.bundle.launch.cwd);
    assert!(lead.cwd.ends_with(&format!("/queue/{}/lead", p.id)));
    assert_eq!(lead.clone_of, fx.repo);
    let names: Vec<_> = lead.clones.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["backend", "frontend"]);
    assert_eq!(lead.clones[1].clone_of, second);
    let branch = p.cloud.as_ref().unwrap().branch.clone();
    assert!(!Path::new(&lead.cwd).join(".git").exists());
    for name in names {
        assert_eq!(branch_of(&Path::new(&lead.cwd).join(name)), branch);
    }
    assert_eq!(p.clones.len(), 2);
}

#[test]
fn multi_repo_workers_get_a_clone_per_repository_on_their_own_branch() {
    let (fx, _) = working_multi_team();
    let id = pickup(&fx).id;
    let reqs = fx.runs.requests.borrow().clone();
    assert_eq!(reqs.len(), 3);
    for (n, req) in reqs[1..].iter().enumerate() {
        let k = n + 1;
        assert!(req.cwd.ends_with(&format!("/queue/{id}/w{k}")));
        assert_eq!(req.clone_of, fx.repo);
        let names: Vec<_> = req.clones.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["backend", "frontend"]);
        assert!(!Path::new(&req.cwd).join(".git").exists());
        for name in names {
            assert_eq!(
                branch_of(&Path::new(&req.cwd).join(name)),
                format!("queue-team/{id}/w{k}")
            );
        }
    }
    let members = team(&fx).members;
    assert_eq!(members[0].clones.len(), 2);
    assert!(members[0].clones.iter().all(|c| c.base.is_some()));
}

fn finish_workers(fx: &Fx) {
    for member in &team(fx).members {
        marker(&member.cwd);
    }
    idle(fx, "mock-2");
    idle(fx, "mock-3");
    fx.pass().unwrap();
    exit_run(fx, "mock-2");
    exit_run(fx, "mock-3");
    fx.pass().unwrap();
}

#[test]
fn a_worker_that_changed_one_repository_is_imported_into_that_repository_only() {
    let (fx, second) = working_multi_team();
    let id = pickup(&fx).id;
    let w1 = team(&fx).members[0].clone();
    let tip = commit_in(&Path::new(&w1.cwd).join("frontend"), "ui.txt");
    finish_workers(&fx);
    assert_eq!(team(&fx).phase, TeamPhase::Finishing);
    let branch = format!("queue-team/{id}/w1");
    assert!(has_branch(&second, &branch));
    assert!(!has_branch(&fx.repo, &branch));
    assert_eq!(git_in(Path::new(&second), &["rev-parse", &branch]), tip);
    let told = fx.runs.sent.borrow().last().cloned().unwrap();
    assert!(told.1.contains(&format!("w1 in frontend: {tip}")), "{}", told.1);
    assert!(told.1.contains("inside the folder"), "{}", told.1);
    assert!(!told.1.contains("backend"), "{}", told.1);
}

#[test]
fn a_worker_whose_clone_folder_was_replaced_is_counted_failed() {
    let (fx, second) = working_multi_team();
    let id = pickup(&fx).id;
    let w1 = team(&fx).members[0].clone();
    let dir = Path::new(&w1.cwd);
    std::fs::rename(dir.join("frontend"), dir.join("frontend.old")).unwrap();
    git_in(dir, &["clone", "-q", &second, "frontend"]);
    let fresh = dir.join("frontend");
    git_in(&fresh, &["checkout", "-q", "-b", &w1.branch]);
    commit_in(&fresh, "ui.txt");
    finish_workers(&fx);
    assert_eq!(team(&fx).phase, TeamPhase::Finishing);
    assert!(!has_branch(&second, &format!("queue-team/{id}/w1")));
    let told = fx.runs.sent.borrow().last().cloned().unwrap();
    assert!(told.1.contains("w1 did not finish cleanly"), "{}", told.1);
    assert!(told.1.starts_with("No worker left commits"), "{}", told.1);
}

#[test]
fn a_member_state_without_clones_still_loads_and_serializes_without_them() {
    let fx = working_team();
    let m = team(&fx).members[0].clone();
    assert!(m.clones.is_empty());
    let text = serde_json::to_string(&m).unwrap();
    assert!(!text.contains("clones"));
    let back: state::Member = serde_json::from_str(&text).unwrap();
    assert_eq!(back, m);
}

#[test]
fn an_old_member_json_without_clones_deserializes() {
    let fx = working_team();
    let m = team(&fx).members[0].clone();
    let mut v = serde_json::to_value(&m).unwrap();
    v.as_object_mut().unwrap().remove("clones");
    let back: state::Member = serde_json::from_str(&v.to_string()).unwrap();
    assert_eq!(back, m);
}

#[test]
fn a_single_repo_integration_note_is_exact() {
    let fx = working_team();
    let tip = commit_in(Path::new(&worker_cwd(&fx, 1)), "api.txt");
    finish_workers(&fx);
    let told = fx.runs.sent.borrow().last().cloned().unwrap();
    assert_eq!(
        told.1,
        format!("All workers have finished. Their work is saved as these commits: w1 {tip}. For each one run `git merge --squash <sha>` and commit the result as one normal commit; resolve conflicts yourself and never create merge commits. When everything is committed on your branch, write the file .tmgr/done and stop.\r")
    );
}

#[test]
fn a_multi_repo_worker_without_commits_is_not_failed() {
    let (fx, _) = working_multi_team();
    finish_workers(&fx);
    assert_eq!(team(&fx).phase, TeamPhase::Finishing);
    let told = fx.runs.sent.borrow().last().cloned().unwrap();
    assert!(told.1.starts_with("No worker left commits"), "{}", told.1);
    assert!(!told.1.contains("did not finish cleanly"), "{}", told.1);
    assert!(team(&fx).members.iter().all(|m| m.tip.is_none()));
}

#[test]
fn a_failed_later_import_removes_the_branches_already_imported() {
    let (fx, second) = working_multi_team();
    let id = pickup(&fx).id;
    let w1 = team(&fx).members[0].clone();
    commit_in(&Path::new(&w1.cwd).join("backend"), "api.txt");
    commit_in(&Path::new(&w1.cwd).join("frontend"), "ui.txt");
    git_in(Path::new(&second), &["branch", &format!("queue-team/{id}")]);
    finish_workers(&fx);
    let branch = format!("queue-team/{id}/w1");
    assert!(!has_branch(&fx.repo, &branch));
    assert!(!has_branch(&second, &branch));
    assert_eq!(fx.audit_events("team_import_failed"), 1);
    let told = fx.runs.sent.borrow().last().cloned().unwrap();
    assert!(told.1.contains("w1 did not finish cleanly"), "{}", told.1);
}

#[test]
fn a_worker_whose_clones_are_not_the_ones_of_the_run_is_counted_failed() {
    let (fx, _) = working_multi_team();
    let id = pickup(&fx).id;
    let w1 = team(&fx).members[0].clone();
    commit_in(&Path::new(&w1.cwd).join("backend"), "api.txt");
    let mut st = state::load(&fx.paths).unwrap();
    st.pickups[0].team.as_mut().unwrap().members[0].clones.clear();
    state::save(&fx.paths, &mut st).unwrap();
    finish_workers(&fx);
    assert!(!has_branch(&fx.repo, &format!("queue-team/{id}/w1")));
    let told = fx.runs.sent.borrow().last().cloned().unwrap();
    assert!(told.1.contains("w1 did not finish cleanly"), "{}", told.1);
}
