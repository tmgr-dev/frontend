#![cfg(all(unix, feature = "runner"))]

use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_tmgr");

fn private_dir(path: &Path) {
    fs::create_dir_all(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

struct Env {
    tmp: tempfile::TempDir,
    run: PathBuf,
}

impl Env {
    fn new() -> Env {
        let tmp = tempfile::Builder::new()
            .prefix("rz")
            .tempdir_in("/tmp")
            .unwrap();
        private_dir(tmp.path());
        let run = tmp.path().join("r");
        private_dir(&run);
        private_dir(&tmp.path().join("h"));
        Env { tmp, run }
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        c.args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.tmp.path())
            .env("TMGR_RUNNER_DIR", &self.run)
            .env("TMGR_RUNNER_HOMES", self.tmp.path().join("h"))
            .env("XDG_CONFIG_HOME", self.tmp.path().join("c"))
            .env("TMGR_AGENTS_DIR", self.tmp.path().join("a"))
            .env("NO_COLOR", "1")
            .stdin(Stdio::null());
        c
    }
}

fn error_of(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout)
        .or_else(|_| serde_json::from_slice(&out.stderr))
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stderr)))
}

#[test]
fn resume_is_refused_inside_any_session() {
    let e = Env::new();
    for key in ["TMGR_SESSION_ID", "TMGR_RUN_TOKEN", "TMGR_RUN_ID"] {
        let out = e
            .cmd(&["resume", "s_aaaaaaaaaa"])
            .env(key, "x")
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(5), "{key}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("agent session"));
    }
}

#[test]
fn resume_needs_a_terminal_and_never_accepts_yes() {
    let e = Env::new();
    let out = e.cmd(&["resume", "s_aaaaaaaaaa"]).output().unwrap();
    assert_eq!(out.status.code(), Some(10));
    let out = e
        .cmd(&["--json", "resume", "s_aaaaaaaaaa"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10));
    assert_eq!(error_of(&out)["ok"], false);
    assert!(!e.run.join("runner.pid").exists());
}

#[test]
fn resume_is_listed_at_the_top_level() {
    let e = Env::new();
    let out = e.cmd(&["resume", "--help"]).output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("<SESSION>"), "{text}");
    let top = e.cmd(&["--help"]).output().unwrap();
    assert!(String::from_utf8_lossy(&top.stdout).contains("resume"));
}
