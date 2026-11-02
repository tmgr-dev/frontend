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
fn a_multi_repo_or_non_claude_team_is_refused_with_a_reason_and_consumed() {
    let fx = cloud_fx();
    fx.cloud.tasks.replace(vec![signed(1, "do it")]);
    fx.cloud.claim_repos.replace(vec![
        crate::clones::ClaimRepo {
            full_name: "o/a".into(),
            path: fx.repo.clone(),
        },
        crate::clones::ClaimRepo {
            full_name: "o/b".into(),
            path: fx.repo.clone(),
        },
    ]);
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
