#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tmgr_ipc::api::{
    CreateReq, RunCreateReq, SessionInfo, StateKind, CLIENT_DESKTOP, CLIENT_WORKER,
};
use tmgr_runner::client::Client;
use tmgr_runner::restore;

const HOST: &str = env!("CARGO_BIN_EXE_tmgr-runner-host");
const U1: &str = "2f0e1c3a-5b7d-4e8f-9a1b-3c4d5e6f7a8b";
const U2: &str = "0199a213-81c0-7800-8aa1-bbab2a035a53";

const AGENT: &str = r#"#!/bin/sh
trap 'kill $! 2>/dev/null; exit 0' HUP TERM
echo "args:$*" >> ./log
if [ -f ./announce ]; then cat ./announce; rm -f ./announce; fi
if [ -f "$HOME/marker" ]; then echo "home:kept" >> ./log; else echo x > "$HOME/marker"; echo "home:new" >> ./log; fi
if [ -f ./exit-now ]; then exit 0; fi
echo ready
sleep 600 &
wait
"#;

const RAW: &str = r#"#!/bin/sh
trap 'kill $! 2>/dev/null; exit 0' HUP TERM
echo "raw:$LC_RAWTEST:$*" >> ./log
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
    env: Vec<(String, String)>,
    runner: Option<Child>,
}

impl H {
    fn new(resume: &str) -> H {
        let tmp = tempfile::Builder::new()
            .prefix("rm")
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
        fs::write(work.join("raw.sh"), RAW).unwrap();
        fs::write(
            agents.join("fake.toml"),
            format!(
                "id = \"fake\"\nname = \"Fake\"\ncommand = [\"/bin/sh\", {:?}]\n{resume}\n[status]\nblocked = ['(?i)do you want to proceed']\n",
                script.display().to_string()
            ),
        )
        .unwrap();
        H {
            tmp,
            run,
            homes,
            work,
            env: Vec::new(),
            runner: None,
        }
    }

    fn full() -> H {
        H::new("[resume]\nargs = [\"--resumed\"]\nby_id = [\"--resume-id\", \"{resume_id}\"]\n")
    }

    fn start(&mut self) {
        let err = fs::File::create(self.tmp.path().join("runner.err")).unwrap();
        self.runner = Some(
            Command::new(HOST)
                .args(["runner", "serve"])
                .envs(self.env.iter().cloned())
                .env("HOME", self.tmp.path())
                .env("TMGR_RUNNER_DIR", &self.run)
                .env("TMGR_RUNNER_HOMES", &self.homes)
                .env("TMGR_AGENTS_DIR", self.tmp.path().join("a"))
                .env("XDG_CONFIG_HOME", self.tmp.path().join("c"))
                .env("TMGR_T0_QUIET_MS", "5000")
                .env("TMGR_RESTORE_FORGET_MS", "300")
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

    fn stop_runner(&mut self) {
        if let Some(mut r) = self.runner.take() {
            let _ = r.kill();
            let _ = r.wait();
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

    fn sdir(&self, id: &str) -> PathBuf {
        self.run.join("sessions").join(id)
    }

    fn req(&self) -> RunCreateReq {
        RunCreateReq {
            clones: Vec::new(),
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
            trust_via_repo: None,
            agent_work: false,
            cloud: None,
            unattended: false,
            exe_pin: None,
            argv_sha256: None,
        }
    }

    fn raw_req(&self, argv: &[&str]) -> CreateReq {
        CreateReq {
            argv: argv.iter().map(|s| s.to_string()).collect(),
            cwd: self.work.display().to_string(),
            name: Some("rawone".into()),
            profile: None,
            cols: 80,
            rows: 24,
            env_pass: Vec::new(),
            task: None,
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

    fn state_of(&self, id: &str) -> Option<tmgr_ipc::api::State> {
        self.client()
            .list()
            .unwrap()
            .into_iter()
            .find(|s| s.id == id)
            .map(|s| s.state)
    }

    fn wait_exited(&self, id: &str) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if self
                .state_of(id)
                .is_some_and(|s| s.kind == StateKind::Exited)
            {
                return;
            }
            assert!(Instant::now() < deadline, "{id} did not exit");
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn stop(&self, id: &str) {
        self.client().kill(id, "term").unwrap();
        self.wait_exited(id);
    }

    fn lines(&self, id: &str) -> Vec<String> {
        self.client().text(id, Some(2000)).unwrap().lines
    }

    fn wait_line(&self, id: &str, needle: &str) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if self.lines(id).iter().any(|l| l.contains(needle)) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{needle:?} never shown: {:?}",
                self.lines(id)
            );
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn announce(&self, text: &str) {
        fs::write(self.work.join("announce"), text).unwrap();
    }

    fn started(&self) -> SessionInfo {
        let before = self.log().matches("home:").count();
        let s = self.client().create_run(self.req()).unwrap();
        self.wait_log("home:", before + 1);
        s
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
        self.stop_runner();
    }
}

fn claude_exit(id: &str) -> String {
    format!("\x1b[1mResume this session with:\x1b[0m\nclaude --resume {id}\n")
}

#[test]
fn a_clean_exit_resumes_by_id_from_the_relaunch_record() {
    if !sessions_available() {
        return;
    }
    let mut h = H::full();
    h.start();
    h.announce(&claude_exit(U1));
    fs::write(h.work.join("exit-now"), b"").unwrap();
    let first = h.client().create_run(h.req()).unwrap();
    h.wait_exited(&first.id);
    let deadline = Instant::now() + Duration::from_secs(15);
    while restore::load(&h.store(), &first.id).is_ok() {
        assert!(
            Instant::now() < deadline,
            "restore record kept after exit 0"
        );
        thread::sleep(Duration::from_millis(50));
    }
    assert!(h.sdir(&first.id).join("relaunch.json").is_file());
    fs::remove_file(h.work.join("exit-now")).unwrap();

    let r = h.client().resume(&first.id, 100, 30).unwrap();
    assert!(r.resumed);
    assert_eq!(r.via.as_deref(), Some("id"));
    assert_ne!(r.session.id, first.id);
    assert_eq!(r.session.resumed_from.as_deref(), Some(first.id.as_str()));
    assert_eq!(r.session.name, "fake-run");
    assert_eq!((r.session.cols, r.session.rows), (100, 30));
    h.wait_log("home:kept", 1);
    assert!(
        h.log().contains(&format!("args:--resume-id {U1}\n")),
        "{}",
        h.log()
    );
    let list = h.client().list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, r.session.id);
    assert_eq!(list[0].resumed_from.as_deref(), Some(first.id.as_str()));
    assert!(!h.sdir(&first.id).exists());
    assert!(!h.homes.join(&first.id).exists());
    assert!(h.sdir(&r.session.id).join("relaunch.json").is_file());
    assert!(restore::load(&h.store(), &r.session.id).is_ok());
    h.wait_line(&r.session.id, "-- resumed --");
    h.wait_line(&r.session.id, &format!("claude --resume {U1}"));

    let again = h.client().resume(&r.session.id, 80, 24).unwrap_err();
    assert!(again.to_string().starts_with("busy"), "{again}");
    let gone = h.client().resume(&first.id, 80, 24).unwrap_err();
    assert!(gone.to_string().starts_with("not_found"), "{gone}");
}

#[test]
fn a_killed_session_resumes_with_the_last_id_it_printed() {
    if !sessions_available() {
        return;
    }
    let mut h = H::full();
    h.start();
    h.announce(&format!("{}{}", claude_exit(U1), claude_exit(U2)));
    let first = h.started();
    h.stop(&first.id);
    assert!(restore::load(&h.store(), &first.id).is_err());
    let r = h.client().resume(&first.id, 80, 24).unwrap();
    assert_eq!(r.via.as_deref(), Some("id"));
    h.wait_log("home:kept", 1);
    assert!(
        h.log().contains(&format!("args:--resume-id {U2}\n")),
        "{}",
        h.log()
    );
}

#[test]
fn without_an_id_the_profiles_resume_args_run_and_without_a_home_it_starts_fresh() {
    if !sessions_available() {
        return;
    }
    let mut h = H::full();
    h.start();
    let first = h.started();
    h.stop(&first.id);
    let r = h.client().resume(&first.id, 80, 24).unwrap();
    assert!(r.resumed);
    assert_eq!(r.via.as_deref(), Some("continue"));
    h.wait_log("home:kept", 1);
    assert!(h.log().contains("args:--resumed\n"), "{}", h.log());

    h.stop(&r.session.id);
    fs::remove_dir_all(h.homes.join(&r.session.id)).unwrap();
    let again = h.client().resume(&r.session.id, 80, 24).unwrap();
    assert!(!again.resumed);
    assert_eq!(again.via.as_deref(), Some("fresh"));
    h.wait_log("home:new", 2);
    assert!(h.log().ends_with("args:\nhome:new\n"), "{}", h.log());
}

#[test]
fn a_profile_without_by_id_never_passes_an_id() {
    if !sessions_available() {
        return;
    }
    let mut h = H::new("[resume]\nargs = [\"--resumed\"]\n");
    h.start();
    h.announce(&claude_exit(U1));
    let first = h.started();
    h.stop(&first.id);
    let r = h.client().resume(&first.id, 80, 24).unwrap();
    assert_eq!(r.via.as_deref(), Some("continue"));
    h.wait_log("home:kept", 1);
    assert!(!h.log().contains("--resume-id"));
    assert!(h.log().contains("args:--resumed\n"));
}

#[test]
fn a_profile_without_resume_starts_fresh() {
    if !sessions_available() {
        return;
    }
    let mut h = H::new("");
    h.start();
    h.announce(&claude_exit(U1));
    let first = h.started();
    h.stop(&first.id);
    let r = h.client().resume(&first.id, 80, 24).unwrap();
    assert!(!r.resumed);
    assert_eq!(r.via.as_deref(), Some("fresh"));
}

type Edit = Box<dyn Fn(&mut restore::Intent)>;

fn rewrite_relaunch(h: &H, id: &str, edit: impl Fn(&mut restore::Intent)) {
    let (mut intent, bundle) = restore::load_relaunch(&h.sdir(id), id).unwrap();
    assert!(bundle.is_none());
    edit(&mut intent);
    restore::save_relaunch(&h.sdir(id), &intent, None).unwrap();
}

#[test]
fn link_bypass_expired_and_unpinned_queue_records_are_refused() {
    if !sessions_available() {
        return;
    }
    let mut h = H::full();
    h.start();
    h.announce(&claude_exit(U1));
    let cases: [(&str, Edit); 4] = [
        (
            "started over a link",
            Box::new(|i| {
                i.origin = Some(tmgr_ipc::api::Origin {
                    link: "laptop".into(),
                    generation: 1,
                })
            }),
        ),
        ("bypass", Box::new(|i| i.bypass = true)),
        ("older than 7 days", Box::new(|i| i.created_ms = 1)),
        ("queue the task again", Box::new(|i| i.keyed = Some(true))),
    ];
    for (needle, edit) in cases {
        let s = h.started();
        h.stop(&s.id);
        rewrite_relaunch(&h, &s.id, edit);
        let err = h.client().resume(&s.id, 80, 24).unwrap_err().to_string();
        assert!(err.starts_with("forbidden"), "{needle}: {err}");
        assert!(err.contains(needle), "{needle}: {err}");
        assert!(h.homes.join(&s.id).is_dir(), "{needle}: home lost");
        assert!(h.state_of(&s.id).is_some(), "{needle}: session lost");
        h.client().remove(&s.id).unwrap();
    }
}

#[test]
fn a_session_without_a_launch_record_cannot_be_resumed() {
    if !sessions_available() {
        return;
    }
    let mut h = H::full();
    h.start();
    let s = h.started();
    h.stop(&s.id);
    fs::remove_file(h.sdir(&s.id).join("relaunch.json")).unwrap();
    let err = h.client().resume(&s.id, 80, 24).unwrap_err().to_string();
    assert!(err.starts_with("not_found"), "{err}");
    assert!(h.state_of(&s.id).is_some());
}

#[test]
fn only_exited_sessions_known_ids_and_the_cli_may_resume() {
    if !sessions_available() {
        return;
    }
    let mut h = H::full();
    h.start();
    let s = h.started();
    let err = h.client().resume(&s.id, 80, 24).unwrap_err().to_string();
    assert!(err.starts_with("busy"), "{err}");
    let err = h
        .client()
        .resume("s_zzzzzzzzzz", 80, 24)
        .unwrap_err()
        .to_string();
    assert!(err.starts_with("not_found"), "{err}");
    let err = h.client().resume("../etc", 80, 24).unwrap_err().to_string();
    assert!(err.starts_with("invalid_params"), "{err}");
    h.stop(&s.id);
    for who in [CLIENT_WORKER, CLIENT_DESKTOP] {
        let mut c = Client::connect_as(&h.run, who).unwrap();
        let err = c.resume(&s.id, 80, 24).unwrap_err().to_string();
        assert!(err.starts_with("forbidden"), "{who}: {err}");
    }
    assert!(h.state_of(&s.id).is_some());
}

#[test]
fn a_second_resume_while_the_first_is_starting_is_busy() {
    if !sessions_available() {
        return;
    }
    let mut h = H::full();
    let go = h.tmp.path().join("go");
    fs::write(&go, b"").unwrap();
    h.env.push((
        "TMGR_TEST_HOLD_BEFORE_SPAWN".into(),
        go.display().to_string(),
    ));
    h.start();
    let first = h.started();
    h.stop(&first.id);
    fs::remove_file(&go).unwrap();
    let mut c = h.client();
    let id = first.id.clone();
    let worker = thread::spawn(move || c.resume(&id, 80, 24));
    let reached = go.with_extension("reached");
    let deadline = Instant::now() + Duration::from_secs(15);
    while !reached.exists() {
        assert!(Instant::now() < deadline, "the first resume never started");
        thread::sleep(Duration::from_millis(20));
    }
    let err = h
        .client()
        .resume(&first.id, 80, 24)
        .unwrap_err()
        .to_string();
    assert!(err.starts_with("busy"), "{err}");
    assert!(err.contains("already in progress"), "{err}");
    fs::write(&go, b"").unwrap();
    let done = worker.join().unwrap().unwrap();
    assert_ne!(done.session.id, first.id);
}

#[test]
fn a_replaced_folder_is_refused_and_nothing_is_lost() {
    if !sessions_available() {
        return;
    }
    let mut h = H::full();
    h.start();
    let s = h.started();
    h.stop(&s.id);
    fs::rename(&h.work, h.tmp.path().join("w-old")).unwrap();
    private_dir(&h.work);
    let err = h.client().resume(&s.id, 80, 24).unwrap_err().to_string();
    assert!(err.contains("approved folder changed"), "{err}");
    assert!(h.state_of(&s.id).is_some());
    assert!(h.homes.join(&s.id).is_dir());
    assert_eq!(h.client().list().unwrap().len(), 1);
}

#[test]
fn a_raw_session_reruns_its_command_with_the_same_name_and_env() {
    if !sessions_available() {
        return;
    }
    let mut h = H::full();
    h.env.push(("LC_RAWTEST".into(), "v1".into()));
    h.start();
    let mut req = h.raw_req(&[
        "/bin/sh",
        &h.work.join("raw.sh").display().to_string(),
        "a b",
    ]);
    req.env_pass = vec!["LC_RAWTEST".into()];
    let first = h.client().create(req).unwrap();
    h.wait_log("raw:v1:a b", 1);
    h.stop(&first.id);
    let r = h.client().resume(&first.id, 90, 20).unwrap();
    assert!(!r.resumed);
    assert_eq!(r.via.as_deref(), Some("rerun"));
    assert_eq!(r.session.name, "rawone");
    assert_eq!(r.session.profile, None);
    assert_eq!(r.session.resumed_from.as_deref(), Some(first.id.as_str()));
    assert_eq!((r.session.cols, r.session.rows), (90, 20));
    h.wait_log("raw:v1:a b", 2);
    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(h.sdir(&r.session.id).join("meta.json")).unwrap())
            .unwrap();
    assert_eq!(meta["env_pass"], serde_json::json!(["LC_RAWTEST"]));
    assert!(h.state_of(&first.id).is_none());
    assert_eq!(h.client().list().unwrap().len(), 1);
    h.wait_line(&r.session.id, "-- resumed --");
}

#[test]
fn a_raw_session_whose_folder_was_replaced_is_refused() {
    if !sessions_available() {
        return;
    }
    let mut h = H::full();
    h.start();
    let first = h
        .client()
        .create(h.raw_req(&["/bin/sh", &h.work.join("raw.sh").display().to_string()]))
        .unwrap();
    h.wait_log("raw:", 1);
    h.stop(&first.id);
    let script = fs::read(h.work.join("raw.sh")).unwrap();
    fs::rename(&h.work, h.tmp.path().join("w-old")).unwrap();
    private_dir(&h.work);
    fs::write(h.work.join("raw.sh"), script).unwrap();
    let err = h
        .client()
        .resume(&first.id, 80, 24)
        .unwrap_err()
        .to_string();
    assert!(err.starts_with("forbidden"), "{err}");
    assert!(h.state_of(&first.id).is_some());
}

fn not_blocked(h: &H, id: &str) {
    thread::sleep(Duration::from_millis(1500));
    let st = h.state_of(id).unwrap();
    assert_ne!(st.kind, StateKind::Blocked, "{st:?}");
}

#[test]
fn the_old_scrollback_survives_runner_restarts_and_never_blocks_the_new_session() {
    if !sessions_available() {
        return;
    }
    let mut h = H::full();
    h.start();
    h.announce(&format!("Do you want to proceed?\n{}", claude_exit(U1)));
    let first = h.started();
    h.stop(&first.id);
    let r = h.client().resume(&first.id, 80, 24).unwrap();
    let id = r.session.id.clone();
    h.wait_line(&id, "-- resumed --");
    h.wait_line(&id, "Do you want to proceed?");
    let prior = fs::read_to_string(h.sdir(&id).join("prior.txt")).unwrap();
    assert!(prior.contains("Do you want to proceed?"));
    assert!(!prior.contains("-- resumed --"));
    assert_eq!(
        fs::metadata(h.sdir(&id).join("prior.txt"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    not_blocked(&h, &id);

    h.stop_runner();
    h.start();
    h.wait_line(&id, "-- resumed --");
    h.wait_line(&id, "Do you want to proceed?");
    not_blocked(&h, &id);

    h.stop(&id);
    h.stop_runner();
    h.start();
    h.wait_line(&id, "-- resumed --");
    h.wait_line(&id, "Do you want to proceed?");
    let lines = h.lines(&id);
    let marker = lines
        .iter()
        .position(|l| l.contains("-- resumed --"))
        .unwrap();
    let before = lines
        .iter()
        .position(|l| l.contains("Do you want to proceed?"))
        .unwrap();
    assert!(before < marker, "{lines:?}");
}

#[test]
fn a_resume_works_when_the_runner_is_full() {
    if !sessions_available() {
        return;
    }
    let mut h = H::full();
    h.start();
    let first = h.started();
    h.stop(&first.id);
    for n in 0..15 {
        let mut req = h.raw_req(&["/bin/sleep", "600"]);
        req.name = Some(format!("sleeper-{n}"));
        h.client().create(req).unwrap();
    }
    assert_eq!(h.client().list().unwrap().len(), 16);
    let err = h
        .client()
        .create(h.raw_req(&["/bin/sleep", "600"]))
        .unwrap_err()
        .to_string();
    assert!(err.starts_with("limit"), "{err}");
    let r = h.client().resume(&first.id, 80, 24).unwrap();
    assert!(r.resumed);
    assert_eq!(h.client().list().unwrap().len(), 16);
}
