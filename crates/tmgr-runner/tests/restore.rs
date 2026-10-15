#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tmgr_ipc::api::{CwdPin, Origin, RunCreateReq, SessionInfo, StateKind, CLIENT_WORKER};
use tmgr_runner::client::Client;
use tmgr_runner::restore::{self, Intent, Status};
use tmgr_runner::wire::now_ms;

const HOST: &str = env!("CARGO_BIN_EXE_tmgr-runner-host");

const AGENT: &str = r#"#!/bin/sh
echo "args:$*" >> ./log
if [ -f "$HOME/marker" ]; then kept=1; else kept=; fi
if [ -f ./change-seed ] && [ -z "$kept" ]; then echo '{"a":2}' > "$HOME/.fake/auth.json"; fi
if [ -n "$kept" ]; then echo "home:kept" >> ./log; else echo x > "$HOME/marker"; echo "home:new" >> ./log; fi
trap 'kill $! 2>/dev/null; exit 0' HUP TERM
if [ -f ./exit-now ]; then exit 0; fi
sleep 600 &
wait
"#;

fn private_dir(path: &Path) {
    fs::create_dir_all(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn sessions_available() -> bool {
    let abi = tmgr_runner::daemon::landlock_abi();
    if cfg!(target_os = "linux") && abi < 6 {
        eprintln!("skipped: Landlock ABI {abi} < 6");
        return false;
    }
    true
}

struct H {
    tmp: tempfile::TempDir,
    run: PathBuf,
    homes: PathBuf,
    work: PathBuf,
    agents: PathBuf,
    forget_ms: &'static str,
    runner: Option<Child>,
}

impl H {
    fn new(resume: bool) -> H {
        let tmp = tempfile::Builder::new()
            .prefix("rs")
            .tempdir_in("/tmp")
            .unwrap();
        private_dir(tmp.path());
        let (run, homes, work, agents) = (
            tmp.path().join("r"),
            tmp.path().join("h"),
            tmp.path().join("w"),
            tmp.path().join("a"),
        );
        for d in [&run, &homes, &work, &agents] {
            private_dir(d);
        }
        let script = work.join("agent.sh");
        fs::write(&script, AGENT).unwrap();
        let resume = if resume {
            "[resume]\nargs = [\"--resumed\"]\n"
        } else {
            ""
        };
        fs::write(
            agents.join("fake.toml"),
            format!(
                "id = \"fake\"\nname = \"Fake\"\ncommand = [\"/bin/sh\", {:?}]\n{resume}",
                script.display().to_string()
            ),
        )
        .unwrap();
        H {
            tmp,
            run,
            homes,
            work,
            agents,
            forget_ms: "300",
            runner: None,
        }
    }

    fn start(&mut self) {
        let err = fs::File::create(self.tmp.path().join("runner.err")).unwrap();
        self.runner = Some(
            Command::new(HOST)
                .args(["runner", "serve"])
                .env("HOME", self.tmp.path())
                .env("TMGR_RUNNER_DIR", &self.run)
                .env("TMGR_RUNNER_HOMES", &self.homes)
                .env("TMGR_AGENTS_DIR", &self.agents)
                .env("XDG_CONFIG_HOME", self.tmp.path().join("c"))
                .env("TMGR_T0_QUIET_MS", "5000")
                .env("TMGR_RESTORE_FORGET_MS", self.forget_ms)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(err)
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while Client::connect(&self.run).is_err() {
            assert!(Instant::now() < deadline, "runner did not start");
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn client(&self) -> Client {
        let c = Client::connect(&self.run).unwrap();
        c.set_timeouts(Duration::from_secs(60)).unwrap();
        c
    }

    fn store(&self) -> PathBuf {
        restore::store(&self.homes)
    }

    fn req(&self) -> RunCreateReq {
        RunCreateReq {
            profile: "fake".into(),
            task: None,
            cwd: self.work.display().to_string(),
            name: Some("fake-run".into()),
            model: None,
            cols: 80,
            rows: 24,
            bundle: None,
            bundle_sha256: None,
            cwd_pin: None,
            origin: None,
            permission_mode: None,
            start_key: None,
            clone_of: None,
            trust_cwd: None,
            agent_work: false,
        }
    }

    fn log(&self) -> String {
        fs::read_to_string(self.work.join("log")).unwrap_or_default()
    }

    fn wait_log(&self, needle: &str, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while self.log().matches(needle).count() < count {
            assert!(
                Instant::now() < deadline,
                "log never had {count}x {needle:?}: {:?}",
                self.log()
            );
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn reboot(&mut self, s: &SessionInfo, lose_runner_dir: bool) {
        if let Some(mut r) = self.runner.take() {
            let _ = r.kill();
            let _ = r.wait();
        }
        for pid in [s.child_pid, s.shim_pid] {
            if let Ok(p) = i32::try_from(pid) {
                if p > 1 {
                    unsafe {
                        libc::kill(-p, libc::SIGKILL);
                        libc::kill(p, libc::SIGKILL);
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(200));
        if lose_runner_dir {
            fs::remove_dir_all(&self.run).unwrap();
            private_dir(&self.run);
        }
        self.start();
    }

    fn entries(&self) -> Vec<restore::Entry> {
        restore::list(&self.store(), &self.homes, now_ms(), &|_| false)
    }
}

impl Drop for H {
    fn drop(&mut self) {
        if let Ok(mut c) = Client::connect(&self.run) {
            let _ = c.set_timeouts(Duration::from_secs(5));
            if let Ok(list) = c.list() {
                for s in list.iter().filter(|s| s.state.kind != StateKind::Exited) {
                    let _ = c.kill(&s.id, "kill");
                }
                thread::sleep(Duration::from_millis(300));
                for s in &list {
                    let _ = c.remove(&s.id);
                }
            }
            let _ = c.stop();
        }
        if let Some(mut r) = self.runner.take() {
            let _ = r.kill();
            let _ = r.wait();
        }
    }
}

fn restore_after_reboot(lose_runner_dir: bool) {
    let mut h = H::new(true);
    h.start();
    let first = h.client().create_run(h.req()).unwrap();
    h.wait_log("home:new", 1);
    let rec = restore::load(&h.store(), &first.id).unwrap();
    assert_eq!(rec.profile, "fake");
    assert_eq!(rec.origin, None);
    h.reboot(&first, lose_runner_dir);

    let e = h.entries();
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].status, Status::Restorable);
    assert!(e[0].resumable);

    let r = h.client().restore(&first.id).unwrap();
    assert!(r.resumed);
    assert_ne!(r.session.id, first.id);
    assert_eq!(r.session.argv.last().map(String::as_str), Some("--resumed"));
    h.wait_log("home:kept", 1);
    assert!(h.log().contains("args:--resumed"));
    assert!(restore::load(&h.store(), &first.id).is_err());
    assert!(restore::load(&h.store(), &r.session.id).is_ok());
    let ids: Vec<String> = h
        .client()
        .list()
        .unwrap()
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(ids, vec![r.session.id.clone()]);
    let again = h.client().restore(&first.id).unwrap_err().to_string();
    assert!(again.starts_with("not_found"), "{again}");
}

#[test]
fn restore_after_reboot_when_the_runner_dir_survives() {
    if sessions_available() {
        restore_after_reboot(false);
    }
}

#[test]
fn restore_after_reboot_when_the_runner_dir_is_gone() {
    if sessions_available() {
        restore_after_reboot(true);
    }
}

#[test]
fn profile_without_resume_starts_fresh_in_the_same_home() {
    if !sessions_available() {
        return;
    }
    let mut h = H::new(false);
    h.start();
    let first = h.client().create_run(h.req()).unwrap();
    h.wait_log("home:new", 1);
    h.reboot(&first, true);
    let r = h.client().restore(&first.id).unwrap();
    assert!(!r.resumed);
    h.wait_log("args:", 2);
    assert!(!h.log().contains("--resumed"));
}

#[test]
fn clean_exit_and_halt_drop_the_record() {
    if !sessions_available() {
        return;
    }
    let mut h = H::new(true);
    h.start();
    fs::write(h.work.join("exit-now"), b"").unwrap();
    let done = h.client().create_run(h.req()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while restore::load(&h.store(), &done.id).is_ok() {
        assert!(Instant::now() < deadline, "record kept after exit 0");
        thread::sleep(Duration::from_millis(50));
    }
    fs::remove_file(h.work.join("exit-now")).unwrap();
    h.client().remove(&done.id).unwrap();
    let halted = h.client().create_run(h.req()).unwrap();
    h.wait_log("home:new", 2);
    assert!(restore::load(&h.store(), &halted.id).is_ok());
    h.client().kill(&halted.id, "term").unwrap();
    assert!(restore::load(&h.store(), &halted.id).is_err());
}

fn term_and_wait_for_exit(h: &H, s: &SessionInfo) {
    unsafe { libc::kill(-(s.child_pid as i32), libc::SIGTERM) };
    let mut c = h.client();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let st = c
            .list()
            .unwrap()
            .into_iter()
            .find(|x| x.id == s.id)
            .unwrap()
            .state;
        if st.kind == StateKind::Exited {
            assert_eq!(st.exit_code, Some(0), "{st:?}");
            break;
        }
        assert!(Instant::now() < deadline, "agent did not exit on SIGTERM");
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn an_agent_that_exits_0_on_shutdown_stays_restorable() {
    if !sessions_available() {
        return;
    }
    let mut h = H::new(true);
    h.forget_ms = "10000";
    h.start();
    let first = h.client().create_run(h.req()).unwrap();
    h.wait_log("home:new", 1);
    term_and_wait_for_exit(&h, &first);
    h.reboot(&first, true);
    assert_eq!(h.entries()[0].status, Status::Restorable);
    assert!(h.client().restore(&first.id).unwrap().resumed);
}

#[test]
fn an_agent_that_exits_0_while_the_runner_lives_is_forgotten() {
    if !sessions_available() {
        return;
    }
    let mut h = H::new(true);
    h.start();
    let first = h.client().create_run(h.req()).unwrap();
    h.wait_log("home:new", 1);
    term_and_wait_for_exit(&h, &first);
    let deadline = Instant::now() + Duration::from_secs(10);
    while restore::load(&h.store(), &first.id).is_ok() {
        assert!(Instant::now() < deadline, "record kept");
        thread::sleep(Duration::from_millis(50));
    }
}

fn seeded(h: &H) {
    fs::write(h.tmp.path().join("seed.json"), "{\"a\":1}").unwrap();
    let script = h.work.join("agent.sh");
    fs::write(
        h.agents.join("fake.toml"),
        format!(
            "id = \"fake\"\nname = \"Fake\"\ncommand = [\"/bin/sh\", {:?}]\n[resume]\nargs = [\"--resumed\"]\n[[sandbox.seed]]\nfrom = \"~/seed.json\"\nto = \".fake/auth.json\"\nwrite_back = true\n",
            script.display().to_string()
        ),
    )
    .unwrap();
    fs::write(h.work.join("change-seed"), b"").unwrap();
}

fn restore_with_seeds(lose_runner_dir: bool) {
    let mut h = H::new(true);
    seeded(&h);
    h.start();
    let first = h.client().create_run(h.req()).unwrap();
    h.wait_log("home:new", 1);
    let old_home = h.homes.join(&first.id);
    assert_eq!(
        fs::read_to_string(old_home.join(".fake/auth.json"))
            .unwrap()
            .trim(),
        "{\"a\":2}"
    );
    h.reboot(&first, lose_runner_dir);
    let r = h.client().restore(&first.id).unwrap();
    h.wait_log("home:kept", 1);
    let new_home = h.homes.join(&r.session.id);
    assert_eq!(
        fs::read_to_string(new_home.join(".fake/auth.json")).unwrap(),
        "{\"a\":1}",
        "the new session gets a fresh seed, not the old copy"
    );
    assert!(new_home.join("marker").exists());
    let records: serde_json::Value = serde_json::from_slice(
        &fs::read(
            h.run
                .join("sessions")
                .join(&r.session.id)
                .join("seeds.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        records[0]["home"].as_str().map(PathBuf::from),
        Some(fs::canonicalize(&new_home).unwrap())
            .filter(|c| records[0]["home"] == c.display().to_string())
            .or(Some(new_home.clone()))
    );
    assert!(!h.run.join("sessions").join(&first.id).exists());
    let pending = tmgr_runner::seedguard::pending(&h.run);
    if lose_runner_dir {
        assert!(pending.is_empty());
    } else {
        assert_eq!(
            pending.len(),
            1,
            "the old copy became a sync candidate before the move"
        );
        assert_eq!(pending[0].session, first.id);
    }
}

#[test]
fn restore_stages_old_logins_then_seeds_the_moved_home_afresh() {
    if sessions_available() {
        restore_with_seeds(false);
    }
}

#[test]
fn restore_reseeds_the_moved_home_when_the_runner_dir_is_gone() {
    if sessions_available() {
        restore_with_seeds(true);
    }
}

#[test]
fn a_login_that_cannot_be_staged_stops_the_restore_before_the_home_moves() {
    if !sessions_available() {
        return;
    }
    let mut h = H::new(true);
    seeded(&h);
    h.start();
    let first = h.client().create_run(h.req()).unwrap();
    h.wait_log("home:new", 1);
    let cands = h.run.join("candidates");
    let _ = fs::create_dir(&cands);
    fs::set_permissions(&cands, fs::Permissions::from_mode(0o500)).unwrap();
    h.reboot(&first, false);
    let err = h.client().restore(&first.id).unwrap_err().to_string();
    fs::set_permissions(&cands, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(err.contains("could not be staged"), "{err}");
    assert!(h.homes.join(&first.id).join(".fake/auth.json").exists());
    assert_eq!(h.entries()[0].status, Status::Restorable);
    let r = h.client().restore(&first.id).unwrap();
    assert!(r.resumed);
    assert_eq!(tmgr_runner::seedguard::pending(&h.run).len(), 1);
}

#[test]
fn replaced_folder_is_refused_and_the_record_kept() {
    if !sessions_available() {
        return;
    }
    let mut h = H::new(true);
    h.start();
    let first = h.client().create_run(h.req()).unwrap();
    h.wait_log("home:new", 1);
    h.reboot(&first, true);
    fs::rename(&h.work, h.tmp.path().join("w-old")).unwrap();
    private_dir(&h.work);
    let err = h.client().restore(&first.id).unwrap_err().to_string();
    assert!(err.contains("approved folder changed"), "{err}");
    assert_eq!(h.entries()[0].status, Status::Restorable);
    assert!(h.homes.join(&first.id).is_dir());
}

#[test]
fn a_lost_runner_dir_with_the_agent_still_alive_is_not_restored() {
    if !sessions_available() {
        return;
    }
    let mut h = H::new(true);
    h.start();
    let first = h.client().create_run(h.req()).unwrap();
    h.wait_log("home:new", 1);
    if let Some(mut r) = h.runner.take() {
        let _ = r.kill();
        let _ = r.wait();
    }
    fs::remove_dir_all(&h.run).unwrap();
    private_dir(&h.run);
    h.start();
    let err = h.client().restore(&first.id).unwrap_err().to_string();
    assert!(err.starts_with("busy"), "{err}");
    assert_eq!(h.entries()[0].status, Status::Running);
    for pid in [first.child_pid, first.shim_pid] {
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
            libc::kill(pid as i32, libc::SIGKILL);
        }
    }
}

fn forged(h: &H, id: &str, edit: impl Fn(&mut Intent)) {
    let dir = fs::metadata(&h.work).unwrap();
    use std::os::unix::fs::MetadataExt;
    let mut i = Intent {
        v: 1,
        session: id.into(),
        profile: "fake".into(),
        model: None,
        name: "x".into(),
        cwd: fs::canonicalize(&h.work).unwrap().display().to_string(),
        cwd_pin: CwdPin {
            dev: dir.dev(),
            ino: dir.ino(),
        },
        task: None,
        bundle_sha256: None,
        origin: None,
        permission_mode: None,
        clone_of: None,
        agent_work: false,
        bypass: false,
        boot: None,
        pids: Vec::new(),
        created_ms: now_ms(),
        cols: 80,
        rows: 24,
    };
    edit(&mut i);
    restore::record(&h.store(), &i, None).unwrap();
}

#[test]
fn link_bypass_expired_and_bundle_less_records_are_refused() {
    let mut h = H::new(true);
    h.start();
    forged(&h, "s_aaaaaaaaaa", |i| {
        i.origin = Some(Origin {
            link: "laptop".into(),
            generation: 1,
        })
    });
    forged(&h, "s_bbbbbbbbbb", |i| i.bypass = true);
    forged(&h, "s_cccccccccc", |i| i.created_ms = 1);
    forged(&h, "s_dddddddddd", |i| {
        i.bundle_sha256 = Some("ab".repeat(32));
    });
    for (id, want) in [
        ("s_aaaaaaaaaa", "forbidden"),
        ("s_bbbbbbbbbb", "forbidden"),
        ("s_cccccccccc", "forbidden"),
        ("s_dddddddddd", "invalid_params"),
    ] {
        let err = h.client().restore(id).unwrap_err().to_string();
        assert!(err.starts_with(want), "{id}: {err}");
        assert!(restore::load(&h.store(), id).is_ok(), "{id} record lost");
    }
    assert!(h.client().list().unwrap().is_empty());
}

#[test]
fn only_the_cli_may_restore() {
    let mut h = H::new(true);
    h.start();
    forged(&h, "s_aaaaaaaaaa", |_| {});
    let mut w = Client::connect_as(&h.run, CLIENT_WORKER).unwrap();
    let err = w.restore("s_aaaaaaaaaa").unwrap_err().to_string();
    assert!(err.starts_with("forbidden"), "{err}");
    let err = h.client().restore("../etc").unwrap_err().to_string();
    assert!(err.starts_with("invalid_params"), "{err}");
}

fn trusted_profile(h: &H, seeded: bool) {
    let script = h.work.join("agent.sh");
    let extra = if seeded {
        "[[sandbox.seed]]\nfrom = \"~/seed.json\"\nto = \".claude.json\"\n\
         [trust]\nfile = \".claude.json\"\npath = [\"projects\", \"{cwd}\", \"ok\"]\n"
    } else {
        ""
    };
    fs::write(
        h.agents.join("fake.toml"),
        format!(
            "id = \"fake\"\nname = \"Fake\"\ncommand = [\"/bin/sh\", {:?}]\n[resume]\nargs = [\"--resumed\"]\n{extra}",
            script.display().to_string()
        ),
    )
    .unwrap();
}

#[test]
fn restore_drops_a_trust_entry_the_profile_no_longer_seeds() {
    if !sessions_available() {
        return;
    }
    let mut h = H::new(true);
    fs::write(h.tmp.path().join("seed.json"), "{\"a\":1}").unwrap();
    trusted_profile(&h, true);
    h.start();
    let mut req = h.req();
    req.trust_cwd = Some(true);
    let first = h.client().create_run(req).unwrap();
    h.wait_log("home:new", 1);
    assert!(
        fs::read_to_string(h.homes.join(&first.id).join(".claude.json"))
            .unwrap()
            .contains("\"ok\":true")
    );
    trusted_profile(&h, false);
    h.reboot(&first, false);
    let r = h.client().restore(&first.id).unwrap();
    h.wait_log("home:kept", 1);
    let new_home = h.homes.join(&r.session.id);
    assert!(new_home.join("marker").exists());
    assert!(!new_home.join(".claude.json").exists());
}
