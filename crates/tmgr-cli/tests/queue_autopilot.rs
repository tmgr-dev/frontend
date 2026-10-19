#![cfg(feature = "runner")]

mod common;

use common::{assert_valid, stdout_json, text};
use serde_json::Value;
use std::process::Output;
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_tmgr");

struct Fx {
    home: TempDir,
    runner: TempDir,
}

fn private_tmp(prefix: &str) -> TempDir {
    let dir = tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in("/tmp")
        .unwrap();
    std::fs::set_permissions(
        dir.path(),
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();
    dir
}

impl Fx {
    fn new() -> Fx {
        Fx {
            home: private_tmp("apc-h"),
            runner: private_tmp("apc-r"),
        }
    }

    fn run(&self, args: &[&str], extra_env: &[(&str, &str)]) -> Output {
        let t = self.home.path();
        let mut c = assert_cmd::Command::new(BIN);
        c.current_dir(t)
            .env_clear()
            .env("HOME", t)
            .env("XDG_CONFIG_HOME", t.join("xdg"))
            .env("XDG_STATE_HOME", t.join("state"))
            .env("TMGR_CREDENTIAL_STORE", "file")
            .env("TMGR_SERVER", "http://127.0.0.1:9")
            .env("TMGR_RUNNER_DIR", self.runner.path())
            .env("NO_COLOR", "1")
            .env("PATH", "/usr/bin:/bin");
        for (k, v) in extra_env {
            c.env(k, v);
        }
        c.args(args).output().unwrap()
    }
}

#[test]
fn starting_without_a_terminal_exits_10_even_with_yes() {
    let fx = Fx::new();
    for args in [
        vec!["queue", "autopilot"],
        vec!["queue", "autopilot", "start"],
        vec!["--yes", "queue", "autopilot", "--rule", "tm"],
        vec!["--yes", "queue", "autopilot", "start", "--no-usage-limit"],
    ] {
        let out = fx.run(&args, &[]);
        assert_eq!(
            out.status.code(),
            Some(10),
            "{args:?}: {}",
            text(&out.stderr)
        );
    }
}

#[test]
fn an_agent_session_cannot_start_or_resume() {
    let fx = Fx::new();
    for var in ["TMGR_RUN_TOKEN", "TMGR_RUN_ID"] {
        for args in [
            vec!["queue", "autopilot"],
            vec!["--yes", "queue", "autopilot", "start"],
            vec!["queue", "autopilot", "resume"],
        ] {
            let out = fx.run(&args, &[(var, "x")]);
            assert_eq!(
                out.status.code(),
                Some(5),
                "{var} {args:?}: {}",
                text(&out.stderr)
            );
        }
    }
}

#[test]
fn status_without_a_runner_says_so_and_needs_no_terminal() {
    let fx = Fx::new();
    let out = fx.run(&["queue", "autopilot", "status"], &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stdout).starts_with("Autopilot is not running."));
    let out = fx.run(&["--json", "queue", "autopilot", "status"], &[]);
    assert_eq!(out.status.code(), Some(0));
    let v: Value = stdout_json(&out);
    assert_eq!(v["data"]["running"], false);
    assert_valid("queue-autopilot-status", &v);
}

#[test]
fn watch_with_json_is_a_usage_error() {
    let fx = Fx::new();
    let out = fx.run(&["--json", "queue", "autopilot", "status", "--watch"], &[]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn pause_stop_and_log_without_a_runner_say_autopilot_is_not_running() {
    let fx = Fx::new();
    for args in [
        vec!["queue", "autopilot", "pause"],
        vec!["queue", "autopilot", "stop"],
        vec!["queue", "autopilot", "log"],
    ] {
        let out = fx.run(&args, &[]);
        assert_eq!(
            out.status.code(),
            Some(6),
            "{args:?}: {}",
            text(&out.stderr)
        );
        assert!(text(&out.stderr).contains("autopilot is not running"));
    }
}

#[test]
fn the_help_lists_every_subcommand_and_flag() {
    let fx = Fx::new();
    let out = fx.run(&["queue", "autopilot", "--help"], &[]);
    let help = text(&out.stdout);
    for word in [
        "start",
        "status",
        "pause",
        "resume",
        "stop",
        "log",
        "--rule",
        "--tasks",
        "--max-concurrent",
        "--usage-soft",
        "--usage-hard",
        "--no-usage-limit",
        "--resume-after-reset",
        "--max-cost",
        "--max-hours",
        "--max-tasks",
    ] {
        assert!(help.contains(word), "{word} missing in\n{help}");
    }
}
