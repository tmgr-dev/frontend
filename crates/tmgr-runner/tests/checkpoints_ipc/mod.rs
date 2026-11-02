use super::*;
use tmgr_ipc::api::{CheckpointKind, CheckpointList, DiffAgainst};

const TURNS: &str = "echo v1 > f.txt; echo ready1; read a\n\
    echo v2 > f.txt; echo ready2; read b\nexit 0\n";

fn fx() -> Harness {
    let mut h = Harness::new();
    let home = h.tmp.path().join("home");
    fs::create_dir(&home).unwrap();
    h.real_home = Some(home);
    h.start(300);
    init_repo(&h.work);
    h
}

fn refs(h: &Harness) -> String {
    git_in(&h.work, &["for-each-ref", "refs/tmgr"])
}

fn wait_items(c: &mut Client, id: &str, want: usize) -> CheckpointList {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let l = c.checkpoints_list(id).unwrap();
        if l.items.len() >= want || Instant::now() >= deadline {
            return l;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn wait_waiting(c: &mut Client, id: &str) {
    let s = wait_state(c, id, 15, |s| s.kind == StateKind::Waiting);
    assert_eq!(s.state.kind, StateKind::Waiting);
}

fn read_f(h: &Harness) -> String {
    fs::read_to_string(h.work.join("f.txt")).unwrap_or_default()
}

#[test]
fn checkpoints_are_off_by_default_and_leave_no_refs() {
    if !sessions_available() {
        return;
    }
    let h = fx();
    let mut c = h.client();
    let info = c.create(h.sh(TURNS)).unwrap();
    wait_waiting(&mut c, &info.id);
    let listed = c
        .list()
        .unwrap()
        .into_iter()
        .find(|s| s.id == info.id)
        .unwrap();
    assert!(listed.checkpoints.is_none());
    assert!(err_text(c.checkpoints_list(&info.id)).starts_with("invalid_params"));
    assert!(
        err_text(c.checkpoints_diff(&info.id, 1, DiffAgainst::Previous))
            .starts_with("invalid_params")
    );
    assert!(err_text(c.checkpoints_restore(&info.id, 1)).starts_with("invalid_params"));
    c.send(&info.id, "go\rgo\r").unwrap();
    wait_exited(&mut c, &info.id);
    assert_eq!(refs(&h), "");
    assert!(!h
        .run
        .join("sessions")
        .join(&info.id)
        .join("checkpoints.json")
        .exists());
}

#[test]
fn enable_validates_keep_and_unknown_sessions() {
    if !sessions_available() {
        return;
    }
    let h = fx();
    let mut c = h.client();
    let info = c.create(h.sh(TURNS)).unwrap();
    for keep in [0, 2, 1001] {
        let e = err_text(c.checkpoints_enable(&info.id, Some(keep)));
        assert!(e.starts_with("invalid_params"), "{keep}: {e}");
    }
    assert!(err_text(c.checkpoints_enable("s_zzzzzzzzzz", None)).starts_with("not_found"));
    assert!(err_text(c.checkpoints_enable("../x", None)).starts_with("invalid_params"));
    let listed = c
        .list()
        .unwrap()
        .into_iter()
        .find(|s| s.id == info.id)
        .unwrap();
    assert!(listed.checkpoints.is_none());
    c.checkpoints_enable(&info.id, Some(3)).unwrap();
    c.checkpoints_enable(&info.id, Some(1000)).unwrap();
    let listed = c
        .list()
        .unwrap()
        .into_iter()
        .find(|s| s.id == info.id)
        .unwrap();
    assert!(listed.checkpoints.is_some());
}

#[test]
fn enabling_takes_a_start_checkpoint_and_each_turn_adds_one() {
    if !sessions_available() {
        return;
    }
    let h = fx();
    let mut c = h.client();
    let info = c.create(h.sh(TURNS)).unwrap();
    c.checkpoints_enable(&info.id, None).unwrap();
    wait_waiting(&mut c, &info.id);
    let first = wait_items(&mut c, &info.id, 1);
    assert_eq!(first.items[0].kind, CheckpointKind::Start);
    assert!(first.status.available);
    c.send(&info.id, "go\r").unwrap();
    let second = wait_items(&mut c, &info.id, 2);
    assert_eq!(second.items.len(), 2);
    assert_eq!(second.items[1].kind, CheckpointKind::Turn);
    let listed = c
        .list()
        .unwrap()
        .into_iter()
        .find(|s| s.id == info.id)
        .unwrap();
    assert_eq!(listed.checkpoints.map(|s| s.count), Some(2));
    assert!(refs(&h).contains("refs/tmgr/checkpoints/"));
}

#[test]
fn restore_is_refused_while_the_session_is_working() {
    if !sessions_available() {
        return;
    }
    let h = fx();
    let mut c = h.client();
    let info = c
        .create(h.sh("while :; do echo busy; sleep 0.1; done\n"))
        .unwrap();
    c.checkpoints_enable(&info.id, None).unwrap();
    let e = err_text(c.checkpoints_restore(&info.id, 1));
    assert!(e.starts_with("busy"), "{e}");
    let _ = c.kill(&info.id, "kill");
}

#[test]
fn restore_is_refused_while_a_session_in_an_overlapping_folder_works() {
    if !sessions_available() {
        return;
    }
    let h = fx();
    let sub = h.work.join("sub");
    fs::create_dir(&sub).unwrap();
    let mut c = h.client();
    let idle = c.create(h.sh(TURNS)).unwrap();
    c.checkpoints_enable(&idle.id, None).unwrap();
    wait_waiting(&mut c, &idle.id);
    wait_items(&mut c, &idle.id, 1);
    let mut busy = h.req(&["/bin/sh", "-c", "while :; do echo busy; sleep 0.1; done"]);
    busy.cwd = sub.display().to_string();
    let other = c.create(busy).unwrap();
    let e = err_text(c.checkpoints_restore(&idle.id, 1));
    assert!(e.starts_with("busy"), "{e}");
    let _ = c.kill(&other.id, "kill");
}

fn restore_with_another_session_in(h: &Harness, cwd: &Path) -> (Result<(), String>, String) {
    restore_from_with_another_session_in(h, &h.work, cwd)
}

fn restore_from_with_another_session_in(
    h: &Harness,
    own_cwd: &Path,
    cwd: &Path,
) -> (Result<(), String>, String) {
    let mut c = h.client();
    let mut own = h.sh(TURNS);
    own.cwd = own_cwd.display().to_string();
    let own = c.create(own).unwrap();
    c.checkpoints_enable(&own.id, None).unwrap();
    wait_waiting(&mut c, &own.id);
    wait_items(&mut c, &own.id, 1);
    let mut req = h.req(&["/bin/sh", "-c", "read a\n"]);
    req.cwd = cwd.display().to_string();
    let other = c.create(req).unwrap();
    let res = c
        .checkpoints_restore(&own.id, 1)
        .map(drop)
        .map_err(|e| e.to_string());
    let _ = c.kill(&other.id, "kill");
    let _ = c.kill(&own.id, "kill");
    (res, other.id)
}

#[test]
fn restore_is_refused_while_another_live_session_shares_the_folder() {
    if !sessions_available() {
        return;
    }
    let h = fx();
    let (res, other) = restore_with_another_session_in(&h, &h.work);
    let e = res.unwrap_err();
    assert!(e.starts_with("busy"), "{e}");
    assert!(e.contains(&other), "{e}");
}

#[test]
fn restore_is_refused_while_another_live_session_runs_in_a_subfolder() {
    if !sessions_available() {
        return;
    }
    let h = fx();
    let sub = h.work.join("sub");
    fs::create_dir(&sub).unwrap();
    let (res, other) = restore_with_another_session_in(&h, &sub);
    let e = res.unwrap_err();
    assert!(e.starts_with("busy"), "{e}");
    assert!(e.contains(&other), "{e}");
}

#[test]
fn restore_is_refused_while_another_live_session_runs_in_a_parent_folder() {
    if !sessions_available() {
        return;
    }
    let h = fx();
    let nested = h.work.join("nested");
    fs::create_dir(&nested).unwrap();
    init_repo(&nested);
    let (res, other) = restore_from_with_another_session_in(&h, &nested, &h.work);
    let e = res.unwrap_err();
    assert!(e.starts_with("busy"), "{e}");
    assert!(e.contains(&other), "{e}");
}

#[test]
fn restore_goes_ahead_with_a_live_session_in_a_separate_checkout() {
    if !sessions_available() {
        return;
    }
    let h = fx();
    let name = h.work.file_name().unwrap().to_string_lossy().into_owned();
    let checkout = h.work.with_file_name(format!("{name}-worktrees")).join("s");
    fs::create_dir_all(&checkout).unwrap();
    let (res, _) = restore_with_another_session_in(&h, &checkout);
    res.unwrap();
}

#[test]
fn restore_goes_ahead_once_the_other_session_has_exited() {
    if !sessions_available() {
        return;
    }
    let h = fx();
    let mut c = h.client();
    let own = c.create(h.sh(TURNS)).unwrap();
    c.checkpoints_enable(&own.id, None).unwrap();
    wait_waiting(&mut c, &own.id);
    wait_items(&mut c, &own.id, 1);
    let other = c.create(h.sh("exit 0\n")).unwrap();
    let start = Instant::now();
    while c
        .list()
        .unwrap()
        .iter()
        .any(|s| s.id == other.id && s.state.kind != StateKind::Exited)
    {
        assert!(start.elapsed() < Duration::from_secs(20), "never exited");
        thread::sleep(Duration::from_millis(50));
    }
    c.checkpoints_restore(&own.id, 1).unwrap();
    let _ = c.kill(&own.id, "kill");
}

#[test]
fn restore_round_trips_over_ipc_and_is_undoable() {
    if !sessions_available() {
        return;
    }
    let h = fx();
    let mut c = h.client();
    let info = c.create(h.sh(TURNS)).unwrap();
    wait_waiting(&mut c, &info.id);
    c.checkpoints_enable(&info.id, None).unwrap();
    let first = wait_items(&mut c, &info.id, 1);
    let start = first.items[0].n;
    assert_eq!(read_f(&h), "v1\n");
    c.send(&info.id, "go\r").unwrap();
    wait_items(&mut c, &info.id, 2);
    wait_waiting(&mut c, &info.id);
    assert_eq!(read_f(&h), "v2\n");
    let preview = c
        .checkpoints_diff(&info.id, start, DiffAgainst::Current)
        .unwrap();
    assert_eq!(preview.n, start);
    assert!(preview.modified >= 1);
    let done = c.checkpoints_restore(&info.id, start).unwrap();
    assert_eq!(done.n, start);
    assert!(done.left_as_is.is_empty());
    assert_eq!(read_f(&h), "v1\n");
    let kinds: Vec<_> = c
        .checkpoints_list(&info.id)
        .unwrap()
        .items
        .iter()
        .map(|i| i.kind)
        .collect();
    assert_eq!(kinds.last(), Some(&CheckpointKind::Restored));
    c.checkpoints_restore(&info.id, done.before_n).unwrap();
    assert_eq!(read_f(&h), "v2\n");
}

#[test]
fn restore_of_an_unknown_number_is_a_clear_error() {
    if !sessions_available() {
        return;
    }
    let h = fx();
    let mut c = h.client();
    let info = c.create(h.sh(TURNS)).unwrap();
    c.checkpoints_enable(&info.id, None).unwrap();
    wait_waiting(&mut c, &info.id);
    wait_items(&mut c, &info.id, 1);
    let e = err_text(c.checkpoints_restore(&info.id, 999));
    assert!(e.starts_with("invalid_params"), "{e}");
    assert_eq!(read_f(&h), "v1\n");
}

#[test]
fn only_the_cli_may_call_checkpoint_methods() {
    if !sessions_available() {
        return;
    }
    let h = fx();
    let mut c = h.client();
    let info = c.create(h.sh(TURNS)).unwrap();
    c.checkpoints_enable(&info.id, None).unwrap();
    wait_waiting(&mut c, &info.id);
    wait_items(&mut c, &info.id, 1);
    for who in ["worker", "desktop"] {
        let mut o = Client::connect_as(&h.run, who).unwrap();
        for e in [
            err_text(o.checkpoints_enable(&info.id, None)),
            err_text(o.checkpoints_list(&info.id)),
            err_text(o.checkpoints_diff(&info.id, 1, DiffAgainst::Previous)),
            err_text(o.checkpoints_restore(&info.id, 1)),
        ] {
            assert!(e.starts_with("forbidden"), "{who}: {e}");
        }
    }
    assert_eq!(read_f(&h), "v1\n");
}

#[test]
fn removing_a_session_clears_its_refs_and_manifest() {
    if !sessions_available() {
        return;
    }
    let h = fx();
    let mut c = h.client();
    let info = c.create(h.sh(TURNS)).unwrap();
    c.checkpoints_enable(&info.id, None).unwrap();
    wait_waiting(&mut c, &info.id);
    wait_items(&mut c, &info.id, 1);
    c.send(&info.id, "go\r").unwrap();
    c.send(&info.id, "go\r").unwrap();
    wait_exited(&mut c, &info.id);
    assert!(refs(&h).contains("refs/tmgr/checkpoints/"));
    c.remove(&info.id).unwrap();
    assert_eq!(refs(&h), "");
    assert!(!h.run.join("sessions").join(&info.id).exists());
}

#[test]
fn resume_keeps_the_checkpoint_history_under_the_new_id() {
    if !sessions_available() {
        return;
    }
    let h = fx();
    let mut c = h.client();
    let info = c.create(h.sh(TURNS)).unwrap();
    c.checkpoints_enable(&info.id, Some(7)).unwrap();
    wait_waiting(&mut c, &info.id);
    wait_items(&mut c, &info.id, 1);
    c.send(&info.id, "go\r").unwrap();
    c.send(&info.id, "go\r").unwrap();
    wait_exited(&mut c, &info.id);
    let before = wait_items(&mut c, &info.id, 2).items.len();
    let resumed = c.resume(&info.id, 80, 24).unwrap();
    let new_id = resumed.session.id.clone();
    assert_ne!(new_id, info.id);
    let listed = c
        .list()
        .unwrap()
        .into_iter()
        .find(|s| s.id == new_id)
        .unwrap();
    assert!(listed.checkpoints.is_some());
    let after = c.checkpoints_list(&new_id).unwrap();
    assert!(after.items.len() >= before);
    assert!(refs(&h).contains(&format!("refs/tmgr/checkpoints/{new_id}/")));
    assert!(!refs(&h).contains(&format!("refs/tmgr/checkpoints/{}/", info.id)));
    assert!(err_text(c.checkpoints_list(&info.id)).starts_with("not_found"));
}

#[test]
fn a_repo_that_would_run_programs_still_checkpoints_without_running_them() {
    if !sessions_available() {
        return;
    }
    let h = fx();
    let marker = h.tmp.path().join("fsmonitor-ran");
    let script = h.script(
        "fsm.sh",
        &format!("#!/bin/sh\ntouch {}\n", marker.display()),
    );
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    git_in(&h.work, &["config", "core.fsmonitor", &script]);
    let mut c = h.client();
    let info = c.create(h.sh(TURNS)).unwrap();
    c.checkpoints_enable(&info.id, None).unwrap();
    wait_waiting(&mut c, &info.id);
    let l = wait_items(&mut c, &info.id, 1);
    assert!(l.status.available);
    assert!(!marker.exists());
}

#[test]
fn outside_a_git_repo_the_session_reports_unavailable() {
    if !sessions_available() {
        return;
    }
    let mut h = Harness::new();
    h.start(300);
    let mut c = h.client();
    let info = c.create(h.sh(TURNS)).unwrap();
    c.checkpoints_enable(&info.id, None).unwrap();
    wait_waiting(&mut c, &info.id);
    thread::sleep(Duration::from_millis(800));
    let l = c.checkpoints_list(&info.id).unwrap();
    assert!(!l.status.available);
    assert!(l.status.note.is_some());
    assert!(l.items.is_empty());
}

#[test]
fn the_turn_marker_requests_a_snapshot() {
    if !sessions_available() {
        return;
    }
    let h = fx();
    let mut c = h.client();
    let marker_cmd = "d=\"$HOME/.cache/tmgr-turn\"; mkdir -p \"$d\"; \
        echo ready1; read a; echo v2 > f.txt; : > \"$d/$TMGR_SESSION_ID\"; \
        while :; do echo busy; sleep 0.2; done\n";
    let info = c.create(h.sh(marker_cmd)).unwrap();
    c.checkpoints_enable(&info.id, None).unwrap();
    wait_waiting(&mut c, &info.id);
    wait_items(&mut c, &info.id, 1);
    c.send(&info.id, "go\r").unwrap();
    let l = wait_items(&mut c, &info.id, 2);
    assert!(l.items.len() >= 2, "{l:?}");
    let _ = c.kill(&info.id, "kill");
}
