use crate::wire::write_atomic;
use crate::worklog;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use tmgr_ipc::api::TaskRef;

const FILE: &str = "agentwork.json";
const HALTED: &str = "agentwork.halted";
pub const UPDATE_EVERY_MS: u64 = 10 * 60 * 1000;
const RETRY_MS: u64 = 60 * 1000;
const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_REPLY: u64 = 4096;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct State {
    pub workspace: String,
    pub task_id: i64,
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub session: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<i64>,
    #[serde(default)]
    pub finished: bool,
    #[serde(default)]
    pub last_update_ms: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hold_success: bool,
}

#[derive(Clone, Debug)]
pub struct Intent {
    pub task: TaskRef,
    pub agent: String,
    pub model: Option<String>,
    pub hold_success: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Exit {
    pub code: Option<i32>,
    pub interrupted: bool,
}

pub fn retry_ms() -> u64 {
    std::env::var("TMGR_TEST_AGENT_WORK_RETRY_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(RETRY_MS)
}

pub fn wanted(dir: &Path) -> bool {
    dir.join(FILE).is_file()
}

fn load(dir: &Path) -> Option<State> {
    serde_json::from_slice(&std::fs::read(dir.join(FILE)).ok()?).ok()
}

fn save(dir: &Path, s: &State) -> io::Result<()> {
    write_atomic(&dir.join(FILE), &serde_json::to_vec(s)?)
}

pub fn begin(
    dir: &Path,
    task: TaskRef,
    agent: &str,
    model: Option<&str>,
    session: &str,
    hold_success: bool,
) -> io::Result<()> {
    save(
        dir,
        &State {
            workspace: task.workspace_id.to_string(),
            task_id: task.task_id,
            agent: agent.to_string(),
            model: model.map(str::to_string),
            session: session.to_string(),
            run_id: None,
            finished: false,
            last_update_ms: 0,
            hold_success,
        },
    )
}

pub fn mark_halted(dir: &Path, reason: Option<&str>) {
    if !wanted(dir) {
        return;
    }
    let path = dir.join(HALTED);
    if reason.is_none() && path.exists() {
        return;
    }
    let _ = write_atomic(&path, reason.unwrap_or("1").as_bytes());
}

fn halt_reason(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join(HALTED)).ok()?;
    let text = text.trim();
    (!text.is_empty() && text != "1").then(|| text.to_string())
}

pub fn status_of(exit: Exit, halted: bool) -> &'static str {
    if halted || exit.interrupted {
        "cancelled"
    } else if exit.code == Some(0) {
        "succeeded"
    } else {
        "failed"
    }
}

fn minutes(s: u64) -> String {
    match s / 60 {
        0 => format!("{s}s"),
        m if m < 60 => format!("{m}m"),
        m => format!("{}h {}m", m / 60, m % 60),
    }
}

fn call(helper: &Path, request: &Value) -> io::Result<Value> {
    let mut child = Command::new(helper)
        .arg("__agent-work")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let mut stdout = child.stdout.take().expect("piped stdout");
    let body = request.to_string();
    thread::spawn(move || {
        let _ = stdin.write_all(body.as_bytes());
    });
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut out = Vec::new();
        let _ = (&mut stdout).take(MAX_REPLY).read_to_end(&mut out);
        let _ = tx.send(out);
    });
    let deadline = Instant::now() + TIMEOUT;
    let timed_out = |child: &mut std::process::Child| {
        let _ = child.kill();
        let _ = child.wait();
        io::Error::new(io::ErrorKind::TimedOut, "agent work helper timed out")
    };
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => return Err(timed_out(&mut child)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        }
    };
    let left = deadline.saturating_duration_since(Instant::now());
    let out = rx
        .recv_timeout(left.max(Duration::from_millis(100)))
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "agent work helper kept its output open",
            )
        })?;
    let reply: Value = serde_json::from_slice(&out).unwrap_or(Value::Null);
    if !status.success() {
        let why = reply["error"]
            .as_str()
            .unwrap_or("agent work helper failed");
        return Err(io::Error::other(why.to_string()));
    }
    Ok(reply)
}

fn commit_lines(summary: &tmgr_ipc::api::WorkSummary) -> Vec<String> {
    summary
        .commits
        .iter()
        .map(|c| format!("{} {}", c.sha, c.subject).trim().to_string())
        .collect()
}

/// Opens, updates or finishes the session's Agent work run; returns what it did, if anything.
pub fn step(
    helper: &Path,
    dir: &Path,
    now: u64,
    exit: Option<Exit>,
) -> io::Result<Option<&'static str>> {
    let Some(mut s) = load(dir) else {
        return Ok(None);
    };
    if s.finished {
        return Ok(None);
    }
    let summary = worklog::summary(dir);
    let Some(run_id) = s.run_id else {
        let reply = call(
            helper,
            &json!({
                "op": "start",
                "workspace": s.workspace,
                "task_id": s.task_id,
                "agent": s.agent,
                "model": s.model,
                "branch": summary.branch,
                "session_id": s.session,
            }),
        )?;
        let id = reply["run_id"]
            .as_i64()
            .filter(|id| *id > 0)
            .ok_or_else(|| io::Error::other("agent work helper answered without a run id"))?;
        s.run_id = Some(id);
        s.last_update_ms = now;
        save(dir, &s)?;
        return Ok(Some("start"));
    };
    if let Some(exit) = exit {
        let status = status_of(exit, dir.join(HALTED).exists());
        if status == "succeeded" && s.hold_success {
            call(
                helper,
                &json!({
                    "op": "update",
                    "workspace": s.workspace,
                    "task_id": s.task_id,
                    "run_id": run_id,
                    "branch": summary.branch,
                    "commits": commit_lines(&summary),
                }),
            )?;
            s.finished = true;
            save(dir, &s)?;
            return Ok(Some("hold"));
        }
        let reason = (status == "cancelled").then(|| halt_reason(dir)).flatten();
        let text = reason.unwrap_or_else(|| {
            format!(
                "{} by the runner after {} ({} working), {} commit(s)",
                match status {
                    "succeeded" => "Finished",
                    "cancelled" => "Stopped",
                    _ => "Failed",
                },
                minutes(summary.duration_s),
                minutes(summary.working_s),
                summary.commits.len()
            )
        });
        call(
            helper,
            &json!({
                "op": "finish",
                "workspace": s.workspace,
                "task_id": s.task_id,
                "run_id": run_id,
                "status": status,
                "summary": text,
                "commits": commit_lines(&summary),
                "branch": summary.branch,
            }),
        )?;
        s.finished = true;
        save(dir, &s)?;
        return Ok(Some("finish"));
    }
    if now.saturating_sub(s.last_update_ms) < UPDATE_EVERY_MS {
        return Ok(None);
    }
    call(
        helper,
        &json!({
            "op": "update",
            "workspace": s.workspace,
            "task_id": s.task_id,
            "run_id": run_id,
            "branch": summary.branch,
            "commits": commit_lines(&summary),
        }),
    )?;
    s.last_update_ms = now;
    save(dir, &s)?;
    Ok(Some("update"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_finish_status_follows_the_exit_and_a_halt() {
        let ok = Exit {
            code: Some(0),
            interrupted: false,
        };
        let bad = Exit {
            code: Some(3),
            interrupted: false,
        };
        let lost = Exit {
            code: None,
            interrupted: true,
        };
        assert_eq!(status_of(ok, false), "succeeded");
        assert_eq!(status_of(bad, false), "failed");
        assert_eq!(status_of(ok, true), "cancelled");
        assert_eq!(status_of(bad, true), "cancelled");
        assert_eq!(status_of(lost, false), "cancelled");
    }

    #[test]
    fn a_run_is_started_once_updated_every_ten_minutes_and_finished_once() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let calls = tmp.path().join("calls");
        let helper = tmp.path().join("helper.sh");
        std::fs::write(
            &helper,
            format!("#!/bin/sh\ncat >> {calls:?}\necho >> {calls:?}\necho '{{\"run_id\":5}}'\n"),
        )
        .unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
        let dir = tmp.path().join("s");
        std::fs::create_dir(&dir).unwrap();
        let task = TaskRef {
            workspace_id: -2,
            task_id: 7,
        };
        begin(&dir, task, "claude-code", Some("opus"), "s_abc", false).unwrap();
        let t0 = 1_000_000;
        let ended = Some(Exit {
            code: Some(0),
            interrupted: false,
        });
        assert_eq!(step(&helper, &dir, t0, None).unwrap(), Some("start"));
        assert_eq!(step(&helper, &dir, t0 + 1000, None).unwrap(), None);
        assert_eq!(
            step(&helper, &dir, t0 + UPDATE_EVERY_MS, None).unwrap(),
            Some("update")
        );
        assert_eq!(
            step(&helper, &dir, t0 + UPDATE_EVERY_MS + 5, ended).unwrap(),
            Some("finish")
        );
        assert_eq!(
            step(&helper, &dir, t0 + 3 * UPDATE_EVERY_MS, ended).unwrap(),
            None
        );
        let ops: Vec<String> = std::fs::read_to_string(&calls)
            .unwrap()
            .lines()
            .map(|l| {
                serde_json::from_str::<Value>(l).unwrap()["op"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(ops, ["start", "update", "finish"]);
    }

    #[test]
    fn a_held_run_stays_open_after_a_clean_exit_and_closes_on_failure() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let calls = tmp.path().join("calls");
        let helper = tmp.path().join("helper.sh");
        std::fs::write(
            &helper,
            format!("#!/bin/sh\ncat >> {calls:?}\necho >> {calls:?}\necho '{{\"run_id\":5}}'\n"),
        )
        .unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
        let task = TaskRef {
            workspace_id: 56,
            task_id: 7,
        };
        let ops = || -> Vec<String> {
            std::fs::read_to_string(&calls)
                .unwrap_or_default()
                .lines()
                .map(|l| {
                    serde_json::from_str::<Value>(l).unwrap()["op"]
                        .as_str()
                        .unwrap()
                        .to_string()
                })
                .collect()
        };
        let ok = Some(Exit {
            code: Some(0),
            interrupted: false,
        });
        let bad = Some(Exit {
            code: Some(1),
            interrupted: false,
        });
        for (exit, want) in [(ok, "hold"), (bad, "finish")] {
            let dir = tmp.path().join(want);
            std::fs::create_dir(&dir).unwrap();
            begin(&dir, task, "claude-code", None, "s_abc", true).unwrap();
            assert_eq!(step(&helper, &dir, 1, None).unwrap(), Some("start"));
            assert_eq!(step(&helper, &dir, 2, exit).unwrap(), Some(want));
            assert_eq!(step(&helper, &dir, 3, exit).unwrap(), None);
        }
        assert_eq!(ops(), ["start", "update", "start", "finish"]);
    }

    #[test]
    fn durations_read_as_seconds_minutes_or_hours() {
        assert_eq!(minutes(42), "42s");
        assert_eq!(minutes(600), "10m");
        assert_eq!(minutes(3 * 3600 + 5 * 60), "3h 5m");
    }
}
