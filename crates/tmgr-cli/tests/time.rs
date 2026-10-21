#![cfg(feature = "runner")]

mod common;
use common::*;
use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

struct Home {
    tmp: tempfile::TempDir,
}

impl Home {
    fn new() -> Home {
        Home {
            tmp: tempfile::tempdir().unwrap(),
        }
    }

    fn run_dir(&self) -> PathBuf {
        self.tmp.path().join("run")
    }

    fn cmd(&self) -> assert_cmd::Command {
        let mut c = assert_cmd::Command::new(env!("CARGO_BIN_EXE_tmgr"));
        c.env_clear()
            .env("HOME", self.tmp.path())
            .env("XDG_CONFIG_HOME", self.tmp.path().join("xdg"))
            .env("TMGR_CREDENTIAL_STORE", "file")
            .env("TMGR_RUNNER_DIR", self.run_dir())
            .env("NO_COLOR", "1");
        c
    }

    fn write(&self, name: &str, lines: &[serde_json::Value]) {
        let dir = self.run_dir().join("time");
        fs::create_dir_all(&dir).unwrap();
        for d in [self.run_dir(), dir.clone()] {
            fs::set_permissions(d, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
        fs::write(dir.join(name), body).unwrap();
    }

    fn text(&self, args: &[&str]) -> String {
        let out = self.cmd().args(args).output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn chunk(
    id: &str,
    session: &str,
    name: &str,
    cwd: &str,
    task: Option<(i64, i64)>,
    end: u64,
    typing: u64,
    reading: u64,
) -> serde_json::Value {
    let mut v = json!({
        "id": id, "session": session, "name": name, "cwd": cwd,
        "start_s": end - typing - reading, "end_s": end, "typing_s": typing, "reading_s": reading
    });
    if let Some((w, t)) = task {
        v["task"] = json!({"workspace_id": w, "task_id": t});
    }
    v
}

fn seed(h: &Home) {
    let n = now();
    h.write(
        "intervals.jsonl",
        &[
            chunk(
                "s_a-0000001",
                "s_a",
                "api",
                "/work/api",
                Some((56, 10175)),
                n - 100,
                20,
                40,
            ),
            chunk(
                "s_a-0000002",
                "s_a",
                "api",
                "/work/api",
                Some((56, 10175)),
                n - 50,
                10,
                20,
            ),
            chunk(
                "s_b-0000001",
                "s_b",
                "web",
                "/work/web",
                Some((56, 10175)),
                n - 80,
                5,
                15,
            ),
            chunk(
                "s_c-0000001",
                "s_c",
                "shell",
                "/work/api",
                None,
                n - 60,
                0,
                90,
            ),
            chunk(
                "s_d-0000001",
                "s_d",
                "old",
                "/work/old",
                Some((-2, 7)),
                n - 3 * 86_400,
                30,
                30,
            ),
        ],
    );
    h.write(
        "sent.jsonl",
        &[
            json!({"id": "s_a-0000001", "status": "created", "counted_s": 60, "at_s": n}),
            json!({"id": "s_d-0000001", "status": "local", "at_s": n}),
        ],
    );
}

fn rows(h: &Home, args: &[&str]) -> Vec<serde_json::Value> {
    let mut a = vec!["--json", "time"];
    a.extend_from_slice(args);
    let out = h.cmd().args(a).output().unwrap();
    let v = stdout_json(&out);
    assert_valid("time", &v);
    v["data"]["rows"].as_array().unwrap().clone()
}

#[test]
fn by_session_lists_each_session_with_totals_and_send_state() {
    let h = Home::new();
    seed(&h);
    let text = h.text(&["time"]);
    let line = |name: &str| {
        text.lines()
            .find(|l| l.starts_with(name))
            .unwrap()
            .to_string()
    };
    assert!(text.lines().next().unwrap().starts_with("NAME"));
    assert!(line("api").contains("56:10175"));
    assert!(line("api").contains("1m 30s"));
    assert!(line("api").ends_with("pending"));
    assert!(line("web").ends_with("pending"));
    assert!(line("shell").ends_with("not bound"));
    assert!(line("old").ends_with("local"));
    assert!(text.contains("Total "));
    assert!(text.contains("Sending to tasks: on - idle cap 120 s"));
}

#[test]
fn grouping_by_folder_and_by_task_adds_up_sessions() {
    let h = Home::new();
    seed(&h);
    let by_folder = rows(&h, &["--by", "folder"]);
    let api = by_folder
        .iter()
        .find(|r| r["folder"] == "/work/api")
        .unwrap();
    assert_eq!(api["sessions"], 2);
    assert_eq!(api["total_s"], 60 + 30 + 90);
    let by_task = rows(&h, &["--by", "task"]);
    let t = by_task
        .iter()
        .find(|r| r["tasks"] == json!([{"workspace_id": 56, "task_id": 10175}]))
        .unwrap();
    assert_eq!(t["sessions"], 2);
    assert_eq!(t["total_s"], 60 + 30 + 20);
    assert_eq!(t["typing_s"], 20 + 10 + 5);
    assert_eq!(by_task.len(), 3);
}

#[test]
fn today_leaves_out_older_time() {
    let h = Home::new();
    seed(&h);
    assert_eq!(rows(&h, &[]).len(), 4);
    let today = rows(&h, &["--today"]);
    assert!(today.iter().all(|r| r["name"] != "old"));
    assert!(rows(&h, &["--since", "1d"])
        .iter()
        .all(|r| r["name"] != "old"));
    assert_eq!(rows(&h, &["--since", "7d"]).len(), 4);
    assert!(
        h.cmd()
            .args(["time", "--since", "nope"])
            .output()
            .unwrap()
            .status
            .code()
            == Some(2)
    );
}

#[test]
fn json_carries_totals_and_settings() {
    let h = Home::new();
    seed(&h);
    let out = h.cmd().args(["--json", "time"]).output().unwrap();
    let v = stdout_json(&out);
    assert_eq!(v["data"]["totals"]["total_s"], 60 + 30 + 20 + 90 + 60);
    assert_eq!(v["data"]["sync"], true);
    assert_eq!(v["data"]["idle_s"], 120);
}

#[test]
fn an_empty_runner_dir_prints_a_friendly_line() {
    let h = Home::new();
    let text = h.text(&["time"]);
    assert!(text.starts_with("No time recorded yet"));
}

#[test]
fn sync_and_idle_are_stored_for_the_runner() {
    let h = Home::new();
    assert!(h.text(&["time", "sync"]).contains("is on"));
    assert!(h.text(&["time", "sync", "off"]).contains("is off"));
    assert!(h.text(&["time", "sync"]).contains("is off"));
    assert!(h.text(&["time", "idle", "300"]).contains("300 s"));
    let saved = fs::read_to_string(Path::new(&h.run_dir()).join("time/settings.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(v, json!({"sync": false, "idle_s": 300}));
    assert!(h
        .text(&["time"])
        .contains("Sending to tasks: off - idle cap 300 s"));
    for bad in ["29", "3601", "0"] {
        let out = h.cmd().args(["time", "idle", bad]).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{bad}");
    }
}

#[test]
fn sync_off_shows_unsent_time_as_skipped() {
    let h = Home::new();
    seed(&h);
    h.text(&["time", "sync", "off"]);
    let text = h.text(&["time"]);
    let web = text.lines().find(|l| l.starts_with("web")).unwrap();
    assert!(web.ends_with("skipped"));
}
