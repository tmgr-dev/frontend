#![cfg(feature = "runner")]

use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;

fn tmgr(home: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_tmgr"));
    c.env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("XDG_CONFIG_HOME", home.join("xdg"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("TMGR_CREDENTIAL_STORE", "file")
        .env("TMGR_RUNNER_DIR", home.join("run"))
        .env("TMGR_RUNNER_HOMES", home.join("homes"))
        .env("TMGR_AGENTS_DIR", home.join("agents"));
    c
}

fn tree_is_empty_of_service_files(home: &Path) -> bool {
    !home.join("Library").exists()
        && !home.join("xdg/systemd").exists()
        && !home.join(".config").exists()
}

#[test]
fn preview_prints_the_file_and_changes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let out = tmgr(home.path())
        .args(["--json", "runner", "install-service"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let d = &v["data"];
    let path = d["path"].as_str().unwrap();
    assert!(path.starts_with(home.path().to_str().unwrap()), "{path}");
    assert!(d["content"].as_str().unwrap().contains("dev.tmgr.runner"));
    assert!(d["commands"].as_array().unwrap().len() >= 2);
    assert!(["launchd", "systemd"].contains(&d["platform"].as_str().unwrap()));
    assert!(!Path::new(path).exists());
    assert!(tree_is_empty_of_service_files(home.path()));
}

#[test]
fn preview_human_output_names_the_path() {
    let home = tempfile::tempdir().unwrap();
    let out = tmgr(home.path())
        .args(["runner", "install-service"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains(home.path().to_str().unwrap()));
    assert!(text.contains("Nothing was changed"));
}

#[test]
fn install_without_a_terminal_exits_10_and_writes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let out = tmgr(home.path())
        .args(["runner", "install-service", "--install"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10));
    assert!(tree_is_empty_of_service_files(home.path()));
}

#[test]
fn install_inside_an_agent_session_exits_5() {
    let home = tempfile::tempdir().unwrap();
    let out = tmgr(home.path())
        .env("TMGR_RUN_TOKEN", "x")
        .args(["runner", "install-service", "--install"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    assert!(tree_is_empty_of_service_files(home.path()));
}

#[test]
fn uninstall_inside_an_agent_session_exits_5() {
    let home = tempfile::tempdir().unwrap();
    let out = tmgr(home.path())
        .env("TMGR_RUN_TOKEN", "x")
        .args(["runner", "uninstall-service"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
}

#[test]
fn uninstall_with_nothing_installed_exits_0() {
    let home = tempfile::tempdir().unwrap();
    let out = tmgr(home.path())
        .args(["runner", "uninstall-service"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("not installed"));
}
