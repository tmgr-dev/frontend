#![cfg(all(unix, feature = "runner"))]

use assert_cmd::Command;
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_tmgr");
const REQUIRED: &str =
    "error: the following required arguments were not provided:\nhint: tmgr --help\n";

fn cmd(home: &TempDir, args: &[&str], extra: &[(&str, &str)]) -> Command {
    let t = home.path();
    let mut c = Command::new(BIN);
    c.env_clear()
        .current_dir(t)
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", t)
        .env("XDG_CONFIG_HOME", t.join("c"))
        .env("TMGR_RUNNER_DIR", t.join("r"))
        .env("TMGR_AGENTS_DIR", t.join("a"))
        .env("TMGR_CREDENTIAL_STORE", "file")
        .env("NO_COLOR", "1")
        .args(args)
        .write_stdin("");
    for (k, v) in extra {
        c.env(k, v);
    }
    c
}

#[test]
fn bare_run_without_a_tty_keeps_the_required_group_error() {
    let home = tempfile::tempdir().unwrap();
    let out = cmd(&home, &["run"], &[]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(err.trim_end(), REQUIRED.trim_end());
}

#[test]
fn bare_run_with_an_agent_marker_keeps_the_same_error() {
    let plain = tempfile::tempdir().unwrap();
    let expected = cmd(&plain, &["run"], &[]).output().unwrap();
    for marker in ["TMGR_RUN_ID", "TMGR_RUN_TOKEN"] {
        let home = tempfile::tempdir().unwrap();
        let out = cmd(&home, &["run"], &[(marker, "1")]).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{marker}");
        assert_eq!(out.stderr, expected.stderr, "{marker}");
        assert!(out.stdout.is_empty(), "{marker}");
    }
}

#[test]
fn run_with_a_flag_is_not_intercepted() {
    let home = tempfile::tempdir().unwrap();
    let out = cmd(&home, &["run", "--agent", "nope-agent", "--no-attach"], &[])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("required arguments"), "{err}");
    assert!(err.contains("nope-agent"), "{err}");
    cmd(&home, &["runner", "stop"], &[]).output().unwrap();
}

#[test]
fn bare_run_with_json_without_a_tty_keeps_the_usage_error() {
    let home = tempfile::tempdir().unwrap();
    let out = cmd(&home, &["--json", "run"], &[]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn the_attach_flag_of_resume_is_accepted_and_hidden_from_help() {
    let home = tempfile::tempdir().unwrap();
    let out = cmd(&home, &["resume", "s_aaaaaaaaaa", "--attach"], &[])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("unexpected argument"), "{err}");
    let help = cmd(&home, &["resume", "--help"], &[]).output().unwrap();
    assert!(!String::from_utf8_lossy(&help.stdout).contains("--attach"));
    cmd(&home, &["runner", "stop"], &[]).output().unwrap();
}
