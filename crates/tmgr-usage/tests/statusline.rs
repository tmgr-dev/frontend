mod common;
use chrono::{TimeZone, Utc};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tmgr_usage::limits::{LimitSource, LimitWindow};
use tmgr_usage::statusline::{
    config_dir_tag, parse_snapshot, plan_install, plan_uninstall, read_snapshots, run_hook,
    run_shell_chain, write_snapshot,
};
use tmgr_usage::UsageError;

const EXE: &str = "/opt/tmgr/bin/tmgr";

fn input(session: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "session_id": session,
        "transcript_path": "/home/SECRET-TRANSCRIPT/t.jsonl",
        "cwd": "/home/SECRET-CWD",
        "workspace": {"current_dir": "/home/SECRET-WS", "repo": "SECRET-REPO"},
        "prompt": "SECRET-PROMPT-1",
        "pr": {"url": "SECRET-PULL"},
        "version": "9.9.9",
        "model": {"id": "claude-test-opus-1", "display_name": "Opus", "extra": "SECRET-MODEL"},
        "cost": {"total_cost_usd": 1.25, "total_duration_ms": 5, "note": "SECRET-COST"},
        "rate_limits": {
            "five_hour": {"used_percentage": 23.5, "resets_at": 1791000000, "note": "SECRET-RL"},
            "seven_day": {"used_percentage": 41.0, "resets_at": 1791500000}
        }
    }))
    .unwrap()
}

fn all_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            all_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn snapshot_keeps_only_whitelisted_fields() {
    let tmp = tempfile::tempdir().unwrap();
    let snap = write_snapshot(tmp.path(), Some("acct1"), &input("sess-1")).unwrap();
    assert_eq!(snap.session_id, "sess-1");
    assert_eq!(snap.model.as_deref(), Some("claude-test-opus-1"));
    assert_eq!(snap.cost_total_usd, Some(1.25));
    assert_eq!(snap.five_hour.as_ref().unwrap().used_pct, 23.5);
    assert_eq!(
        snap.seven_day
            .as_ref()
            .unwrap()
            .resets_at
            .unwrap()
            .timestamp(),
        1791500000
    );

    let mut files = Vec::new();
    all_files(tmp.path(), &mut files);
    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("claude/acct1/sess-1.json"));
    let bytes = std::fs::read(&files[0]).unwrap();
    for needle in [
        "SECRET",
        "transcript",
        "cwd",
        "workspace",
        "prompt",
        "SECRET-PULL",
        "9.9.9",
        "extra",
    ] {
        assert!(
            !bytes.windows(needle.len()).any(|w| w == needle.as_bytes()),
            "snapshot leaked a non-whitelisted field"
        );
    }
    let keys: Value = serde_json::from_slice(&bytes).unwrap();
    let mut names: Vec<&String> = keys.as_object().unwrap().keys().collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "config_dir_tag",
            "cost_total_usd",
            "five_hour",
            "model",
            "observed_at",
            "session_id",
            "seven_day"
        ]
    );
}

#[cfg(unix)]
#[test]
fn files_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    write_snapshot(tmp.path(), Some("a"), &input("s")).unwrap();
    let mode = |p: PathBuf| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(tmp.path().join("claude")), 0o700);
    assert_eq!(mode(tmp.path().join("claude/a")), 0o700);
    assert_eq!(mode(tmp.path().join("claude/a/s.json")), 0o600);
}

#[test]
fn session_id_and_tag_are_sanitised() {
    let tmp = tempfile::tempdir().unwrap();
    let snap = write_snapshot(
        tmp.path(),
        Some("../../evil/tag"),
        &input("../../etc/pass wd"),
    )
    .unwrap();
    assert_eq!(snap.session_id, "etcpasswd");
    assert_eq!(snap.config_dir_tag.as_deref(), Some("eviltag"));
    let mut files = Vec::new();
    all_files(tmp.path(), &mut files);
    assert_eq!(files.len(), 1);
    assert!(files[0].starts_with(tmp.path().join("claude/eviltag")));

    let long = "a".repeat(200);
    let snap = write_snapshot(tmp.path(), None, &input(&long)).unwrap();
    assert_eq!(snap.session_id.len(), 64);
    assert_eq!(snap.config_dir_tag.as_deref(), Some("default"));

    for bad in ["", "...", "///"] {
        let r = write_snapshot(tmp.path(), None, &input(bad));
        assert!(matches!(r, Err(UsageError::Invalid(_))));
    }
}

#[test]
fn config_dir_tag_hides_the_path() {
    let tag = config_dir_tag(Some("/home/someone/.claude-work"));
    assert_eq!(tag.len(), 16);
    assert!(!tag.contains("someone"));
    assert_eq!(tag, config_dir_tag(Some("/home/someone/.claude-work")));
    assert_ne!(tag, config_dir_tag(Some("/home/someone/.claude")));
    assert_eq!(config_dir_tag(None), "default");
}

#[test]
fn oversized_and_malformed_input_is_refused_without_writing() {
    let tmp = tempfile::tempdir().unwrap();
    let big = format!(
        r#"{{"session_id":"s","pad":"{}"}}"#,
        "x".repeat(1024 * 1024)
    );
    assert!(write_snapshot(tmp.path(), None, big.as_bytes()).is_err());
    assert!(write_snapshot(tmp.path(), None, b"not json SECRET-PROMPT-1").is_err());
    let err = write_snapshot(tmp.path(), None, b"{\"session_id\": SECRET-PROMPT-1}").unwrap_err();
    assert!(!err.to_string().contains("SECRET"));
    assert!(std::fs::read_dir(tmp.path()).unwrap().next().is_none());
}

#[test]
fn input_without_rate_limits_still_writes() {
    let tmp = tempfile::tempdir().unwrap();
    let snap = write_snapshot(
        tmp.path(),
        None,
        br#"{"session_id":"s","model":{"display_name":"Opus"}}"#,
    )
    .unwrap();
    assert_eq!(snap.model.as_deref(), Some("Opus"));
    assert!(snap.five_hour.is_none() && snap.seven_day.is_none());
}

#[test]
fn parallel_sessions_write_atomically() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let mut handles = Vec::new();
    for s in 0..4 {
        let dir = dir.clone();
        handles.push(std::thread::spawn(move || {
            for i in 0..60 {
                let body = serde_json::to_vec(&json!({
                    "session_id": format!("sess-{s}"),
                    "cost": {"total_cost_usd": i as f64},
                    "rate_limits": {"five_hour": {"used_percentage": i as f64, "resets_at": 1791000000}}
                }))
                .unwrap();
                write_snapshot(&dir, Some("t"), &body).unwrap();
            }
        }));
    }
    let reader = {
        let dir = dir.clone();
        std::thread::spawn(move || {
            for _ in 0..100 {
                for snap in read_snapshots(&dir).unwrap() {
                    assert!(snap.session_id.starts_with("sess-"));
                }
            }
        })
    };
    for h in handles {
        h.join().unwrap();
    }
    reader.join().unwrap();
    let snaps = read_snapshots(&dir).unwrap();
    assert_eq!(snaps.len(), 4);
    assert!(snaps.iter().all(|s| s.cost_total_usd == Some(59.0)));
    let mut files = Vec::new();
    all_files(&dir, &mut files);
    assert_eq!(files.len(), 4, "no temp files are left behind");
}

#[test]
fn read_snapshots_ignores_foreign_oversized_and_unreadable_files() {
    let tmp = tempfile::tempdir().unwrap();
    write_snapshot(tmp.path(), Some("a"), &input("good")).unwrap();
    let tag = tmp.path().join("claude/a");
    std::fs::write(tag.join("junk.json"), b"{not json").unwrap();
    std::fs::write(
        tag.join("foreign.json"),
        br#"{"session_id":"foreign","x":1}"#,
    )
    .unwrap();
    std::fs::write(tag.join("note.txt"), b"hello").unwrap();
    std::fs::write(tag.join("big.json"), vec![b' '; 100_000]).unwrap();
    std::fs::write(
        tag.join("renamed.json"),
        br#"{"session_id":"other","config_dir_tag":null,"model":null,"five_hour":null,"seven_day":null,"cost_total_usd":null,"observed_at":"2026-10-14T10:00:00Z"}"#,
    )
    .unwrap();
    std::fs::create_dir_all(tmp.path().join("claude/bad tag")).unwrap();
    let snaps = read_snapshots(tmp.path()).unwrap();
    assert_eq!(snaps.len(), 1);
    assert_eq!(snaps[0].session_id, "good");
    assert_eq!(snaps[0].config_dir_tag.as_deref(), Some("a"));
    assert!(read_snapshots(&tmp.path().join("missing"))
        .unwrap()
        .is_empty());
}

#[test]
fn to_limits_maps_windows_and_drops_expired_ones() {
    let snap = parse_snapshot(
        Some("a"),
        &input("s"),
        Utc.with_ymd_and_hms(2026, 10, 14, 10, 0, 0).unwrap(),
    )
    .unwrap();
    let at = Utc.timestamp_opt(1790990000, 0).unwrap();
    let limits = snap.to_limits_at("work", at);
    assert_eq!(limits.len(), 2);
    assert_eq!(limits[0].window, LimitWindow::Session);
    assert_eq!(limits[0].source, LimitSource::Statusline);
    assert_eq!(limits[0].account, "work");
    assert_eq!(limits[0].tool_session_id, "s");
    assert_eq!(limits[1].window, LimitWindow::Weekly);

    let later = Utc.timestamp_opt(1791200000, 0).unwrap();
    let limits = snap.to_limits_at("work", later);
    assert_eq!(limits.len(), 1);
    assert_eq!(limits[0].window, LimitWindow::Weekly);
    assert!(snap.to_limits("work").is_empty());
}

#[test]
fn hook_writes_the_snapshot_and_returns_only_the_chain_output() {
    let tmp = tempfile::tempdir().unwrap();
    let mut seen: Vec<(String, Vec<u8>)> = Vec::new();
    let out = run_hook(
        &input("s"),
        Some("a"),
        tmp.path(),
        Some("echo hi"),
        &mut |c, i| {
            seen.push((c.to_string(), i.to_vec()));
            b"line\n".to_vec()
        },
    );
    assert_eq!(out, b"line\n");
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, "echo hi");
    assert_eq!(seen[0].1, input("s"));
    assert_eq!(read_snapshots(tmp.path()).unwrap().len(), 1);

    let out = run_hook(&input("s2"), Some("a"), tmp.path(), None, &mut |_, _| {
        panic!("no chain")
    });
    assert!(out.is_empty());
}

#[test]
fn hook_never_fails_on_bad_input_or_unwritable_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let blocker = tmp.path().join("file");
    std::fs::write(&blocker, b"x").unwrap();
    let out = run_hook(b"garbage", None, &blocker, Some("c"), &mut |_, _| {
        b"ok".to_vec()
    });
    assert_eq!(out, b"ok");
    let out = run_hook(
        &input("s"),
        None,
        &blocker.join("sub"),
        None,
        &mut |_, _| vec![],
    );
    assert!(out.is_empty());
}

#[cfg(unix)]
#[test]
fn shell_chain_pipes_stdin_and_times_out() {
    assert_eq!(run_shell_chain("cat", b"abc"), b"abc");
    assert_eq!(run_shell_chain("echo out; echo err 1>&2", b""), b"out\n");
    assert!(run_shell_chain("exit 3", b"x").is_empty());
    let start = std::time::Instant::now();
    assert!(run_shell_chain("echo a; sleep 30", b"").is_empty());
    assert!(start.elapsed() < std::time::Duration::from_secs(5));
}

fn settings(v: Value) -> String {
    let mut s = serde_json::to_string_pretty(&v).unwrap();
    s.push('\n');
    s
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn install_into_empty_and_missing_statusline() {
    let plan = plan_install("", EXE, None).unwrap();
    let v = parse(&plan.new_settings_text);
    assert_eq!(v["statusLine"]["type"], "command");
    assert_eq!(
        v["statusLine"]["command"],
        "/opt/tmgr/bin/tmgr usage statusline-hook"
    );
    assert_eq!(plan.previous_command, None);
    assert_eq!(
        plan.installed_command,
        "/opt/tmgr/bin/tmgr usage statusline-hook"
    );
    assert!(plan.warnings.is_empty());

    let text = settings(json!({"model": "opus", "env": {"A": "1"}}));
    let plan = plan_install(&text, EXE, None).unwrap();
    let v = parse(&plan.new_settings_text);
    assert_eq!(v["model"], "opus");
    let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
    assert_eq!(keys, vec!["model", "env", "statusLine"]);
}

#[test]
fn install_chains_the_previous_command_and_keeps_other_keys() {
    let text = settings(json!({
        "theme": "dark",
        "statusLine": {"type": "command", "command": "~/bin/my line.sh --x 'a b'", "padding": 2, "refreshInterval": 5},
        "hooks": {"Stop": []}
    }));
    let plan = plan_install(&text, EXE, Some("ignored-because-user-has-one")).unwrap();
    assert_eq!(
        plan.previous_command.as_deref(),
        Some("~/bin/my line.sh --x 'a b'")
    );
    let v = parse(&plan.new_settings_text);
    let sl = &v["statusLine"];
    assert_eq!(sl["padding"], 2);
    assert_eq!(sl["refreshInterval"], 5);
    assert_eq!(sl["type"], "command");
    assert_eq!(sl["command"], plan.installed_command);
    let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
    assert_eq!(keys, vec!["theme", "statusLine", "hooks"]);
    let inner: Vec<&String> = sl.as_object().unwrap().keys().collect();
    assert_eq!(inner, vec!["type", "command", "padding", "refreshInterval"]);
    assert!(plan.installed_command.contains("--chain '"));
}

#[test]
fn explicit_chain_is_used_when_the_user_has_no_statusline() {
    let plan = plan_install("{}", EXE, Some("tmgr statusline claude")).unwrap();
    assert_eq!(
        plan.previous_command.as_deref(),
        Some("tmgr statusline claude")
    );
    assert!(plan
        .installed_command
        .ends_with("--chain 'tmgr statusline claude'"));
}

#[test]
fn install_is_idempotent_and_replaces_a_moved_exe() {
    let text = settings(json!({"statusLine": {"type": "command", "command": "echo it's mine"}}));
    let first = plan_install(&text, EXE, None).unwrap();
    let second = plan_install(&first.new_settings_text, EXE, None).unwrap();
    assert_eq!(second.new_settings_text, first.new_settings_text);
    assert_eq!(second.previous_command.as_deref(), Some("echo it's mine"));

    let moved = plan_install(&first.new_settings_text, "/usr/local/bin/tmgr", None).unwrap();
    assert_eq!(moved.previous_command.as_deref(), Some("echo it's mine"));
    assert!(moved
        .installed_command
        .starts_with("/usr/local/bin/tmgr usage statusline-hook --chain "));
    assert_eq!(
        moved.installed_command.matches("statusline-hook").count(),
        1
    );
    assert_eq!(
        shell_chain(&moved.installed_command).as_deref(),
        Some("echo it's mine")
    );
}

fn shell_chain(cmd: &str) -> Option<String> {
    let t = tmgr_usage::statusline::shell_split(cmd)?;
    (t.get(3).map(String::as_str) == Some("--chain")).then(|| t[4].clone())
}

#[test]
fn uninstall_restores_previous_command_or_removes_the_key() {
    let with_prev = settings(json!({
        "a": 1,
        "statusLine": {"type": "command", "command": "echo \"x y\"", "padding": 1}
    }));
    let installed = plan_install(&with_prev, EXE, None)
        .unwrap()
        .new_settings_text;
    let back = plan_uninstall(&installed).unwrap();
    assert_eq!(back.new_settings_text, with_prev);
    assert_eq!(back.previous_command.as_deref(), Some("echo \"x y\""));

    let without = settings(json!({"a": 1, "b": {"c": [1, 2]}}));
    let installed = plan_install(&without, EXE, None).unwrap().new_settings_text;
    assert_ne!(installed, without);
    let back = plan_uninstall(&installed).unwrap();
    assert_eq!(back.new_settings_text, without);
    assert_eq!(back.previous_command, None);
}

#[test]
fn uninstall_leaves_foreign_or_missing_statuslines_alone() {
    let foreign = settings(json!({"statusLine": {"type": "command", "command": "echo hi"}}));
    let plan = plan_uninstall(&foreign).unwrap();
    assert_eq!(plan.new_settings_text, foreign);
    assert!(!plan.warnings.is_empty());
    let none = settings(json!({"a": 1}));
    assert_eq!(plan_uninstall(&none).unwrap().new_settings_text, none);
    assert_eq!(plan_uninstall("").unwrap().new_settings_text, "");
}

#[test]
fn indentation_is_preserved() {
    let text = "{\n\t\"a\": 1\n}\n";
    let plan = plan_install(text, EXE, None).unwrap();
    assert!(plan.new_settings_text.contains("\n\t\"statusLine\""));
    let four = "{\n    \"a\": 1\n}";
    let plan = plan_install(four, EXE, None).unwrap();
    assert!(plan.new_settings_text.contains("\n    \"statusLine\""));
    assert!(!plan.new_settings_text.ends_with('\n'));
}

#[test]
fn odd_settings_are_refused_and_untouched() {
    for bad in [
        "not json",
        "[1,2]",
        "\"x\"",
        r#"{"statusLine": "echo hi"}"#,
        r#"{"statusLine": {"type": "prompt", "command": "x"}}"#,
        r#"{"statusLine": {"type": "command"}}"#,
        r#"{"statusLine": {"type": "command", "command": 5}}"#,
        r#"{"statusLine": null}"#,
    ] {
        assert!(
            matches!(plan_install(bad, EXE, None), Err(UsageError::Invalid(_))),
            "install should refuse"
        );
        assert!(plan_uninstall(bad).is_err(), "uninstall should refuse");
    }
}

#[test]
fn relative_exe_gets_a_warning() {
    let plan = plan_install("{}", "tmgr", None).unwrap();
    assert_eq!(plan.warnings.len(), 1);
}
