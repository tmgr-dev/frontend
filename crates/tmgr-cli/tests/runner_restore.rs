#![cfg(all(unix, feature = "runner"))]

use serde_json::Value;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tmgr_ipc::api::{CwdPin, Origin};
use tmgr_runner::restore::{self, Intent};
use tmgr_runner::wire::now_ms;

const BIN: &str = env!("CARGO_BIN_EXE_tmgr");
const A: &str = "s_aaaaaaaaaa";
const L: &str = "s_llllllllll";

fn private_dir(path: &Path) {
    fs::create_dir_all(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

struct Env {
    tmp: tempfile::TempDir,
    run: PathBuf,
    homes: PathBuf,
    work: PathBuf,
}

impl Env {
    fn new() -> Env {
        let tmp = tempfile::Builder::new()
            .prefix("rr")
            .tempdir_in("/tmp")
            .unwrap();
        private_dir(tmp.path());
        let (run, homes, work) = (
            tmp.path().join("r"),
            tmp.path().join("h"),
            tmp.path().join("w"),
        );
        for d in [&run, &homes, &work] {
            private_dir(d);
        }
        Env {
            tmp,
            run,
            homes,
            work,
        }
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        c.args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.tmp.path())
            .env("TMGR_RUNNER_DIR", &self.run)
            .env("TMGR_RUNNER_HOMES", &self.homes)
            .env("XDG_CONFIG_HOME", self.tmp.path().join("c"))
            .env("TMGR_AGENTS_DIR", self.tmp.path().join("a"))
            .env("NO_COLOR", "1")
            .stdin(Stdio::null());
        c
    }

    fn tmgr(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }

    fn record(&self, id: &str, origin: Option<Origin>) {
        let m = fs::metadata(&self.work).unwrap();
        restore::record(
            &restore::store(&self.homes),
            &Intent {
                v: 1,
                session: id.into(),
                profile: "codex".into(),
                model: Some("m1".into()),
                name: "codex".into(),
                cwd: fs::canonicalize(&self.work).unwrap().display().to_string(),
                cwd_pin: CwdPin {
                    dev: m.dev(),
                    ino: m.ino(),
                },
                task: None,
                bundle_sha256: None,
                origin,
                permission_mode: None,
                clone_of: None,
                agent_work: false,
                keyed: Some(false),
                resume_decl: None,
                exe_pin: None,
                argv_sha256: None,
                bypass: false,
                no_sandbox: false,
                boot: None,
                pids: Vec::new(),
                created_ms: now_ms(),
                cols: 80,
                rows: 24,
            },
            None,
        )
        .unwrap();
    }

    fn has(&self, id: &str) -> bool {
        restore::load(&restore::store(&self.homes), id).is_ok()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = self.tmgr(&["runner", "stop"]);
    }
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!("{e}: {}", String::from_utf8_lossy(&out.stdout));
    })
}

#[test]
fn listing_never_restores_without_a_terminal() {
    let e = Env::new();
    e.record(A, None);
    e.record(
        L,
        Some(Origin {
            link: "laptop".into(),
            generation: 1,
        }),
    );
    let v = json(&e.tmgr(&["--json", "agents", "restore"]));
    assert_eq!(v["ok"], true, "{v}");
    let rows = v["data"]["sessions"].as_array().unwrap();
    let status = |id: &str| {
        rows.iter()
            .find(|r| r["intent"]["session"] == id)
            .map(|r| r["status"].as_str().unwrap().to_string())
    };
    assert_eq!(status(A).as_deref(), Some("restorable"));
    assert_eq!(status(L).as_deref(), Some("link"));

    let out = e.tmgr(&["agents", "restore"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains(A), "{text}");
    assert!(text.contains("tmgr agents restore <id>"), "{text}");
    assert!(e.has(A));

    let out = e.tmgr(&["agents"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("1 session stopped by a reboot"), "{text}");
}

#[test]
fn restoring_needs_a_terminal_and_a_human() {
    let e = Env::new();
    e.record(A, None);
    let out = e.tmgr(&["agents", "restore", A]);
    assert_eq!(out.status.code(), Some(10));
    let out = e
        .cmd(&["agents", "restore", A])
        .env("TMGR_RUN_TOKEN", "x")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    assert!(e.has(A));
}

#[test]
fn link_records_are_not_restorable_here() {
    let e = Env::new();
    e.record(
        L,
        Some(Origin {
            link: "laptop".into(),
            generation: 1,
        }),
    );
    let out = e.tmgr(&["agents", "restore", L]);
    assert_eq!(out.status.code(), Some(5));
    assert!(String::from_utf8_lossy(&out.stderr).contains("leader"));
}

#[test]
fn discard_removes_the_record() {
    let e = Env::new();
    e.record(A, None);
    let out = e
        .cmd(&["agents", "restore", A, "--discard"])
        .env("TMGR_RUN_ID", "x")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    assert!(e.has(A));
    assert!(e
        .tmgr(&["agents", "restore", A, "--discard"])
        .status
        .success());
    assert!(!e.has(A));
    assert_eq!(
        e.tmgr(&["agents", "restore", A, "--discard"]).status.code(),
        Some(3)
    );
}
