use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use tmgr_core::error::Result;
use tmgr_ipc::api::AutopilotStatus;
use tmgr_queue::rules::QueueFile;
use tmgr_queue::Paths;

use super::control::connect_running;
use super::model::{Answers, Finish, LoopKind, Settings, QUEUE_SETTINGS_ID};
use super::start::{agent_tool, hosted_client, start_hosted};
use super::store::Store;
use crate::cmd::runner::rerr;
use crate::out::Output;

pub enum Hosting {
    Started(u32),
    Running,
    DrainRunning,
    Skipped,
}

pub fn settings_for(file: &QueueFile, now: DateTime<Utc>) -> Settings {
    let mut tools: Vec<_> = file
        .rules
        .iter()
        .filter_map(|r| agent_tool(&r.profile))
        .collect();
    tools.dedup();
    let one_tool = tools.len() == 1 && file.rules.iter().all(|r| agent_tool(&r.profile).is_some());
    let slots: u32 = file.rules.iter().map(|r| r.max_concurrent).sum();
    Settings {
        id: QUEUE_SETTINGS_ID.into(),
        created_at: now,
        answers: Answers {
            usage_guard: false,
            max_concurrent: slots.max(1),
            ..Answers::default()
        },
        rule: "all".into(),
        agent: tools
            .first()
            .filter(|_| one_tool)
            .map(|t| t.as_str().to_string())
            .unwrap_or_default(),
        mode: "auto".into(),
        finish: Finish::PrOnly,
        tasks: Default::default(),
        signed: Vec::new(),
        windows: Vec::new(),
        kind: LoopKind::Queue,
        resume_queue: false,
        order: Vec::new(),
        dry_run: false,
    }
}

#[cfg(debug_assertions)]
fn off_for_tests() -> bool {
    std::env::var_os("TMGR_TEST_NO_QUEUE_LOOP").is_some()
}

#[cfg(not(debug_assertions))]
fn off_for_tests() -> bool {
    false
}

pub fn save_settings(paths: &Paths) -> Result<()> {
    let file = tmgr_queue::rules::load(paths)?.file;
    Store::from_env()?.save_settings(&settings_for(&file, Utc::now()))
}

pub fn ensure_running(paths: &Paths) -> Result<Hosting> {
    if off_for_tests() {
        return Ok(Hosting::Skipped);
    }
    save_settings(paths)?;
    let mut client = hosted_client()?;
    let status = client.autopilot_status().map_err(rerr)?;
    if status.running {
        return Ok(match status.status["settings_id"].as_str() {
            Some(id) if id != QUEUE_SETTINGS_ID => Hosting::DrainRunning,
            _ => Hosting::Running,
        });
    }
    drop(client);
    start_hosted(QUEUE_SETTINGS_ID).map(Hosting::Started)
}

pub fn summarize(st: Option<&AutopilotStatus>) -> (Value, String) {
    let Some(st) = st else {
        return (json!({"running": null}), "Background loop: unknown.".into());
    };
    let id = st.status["settings_id"].as_str().unwrap_or("");
    let pid = st.pid.map(|p| format!(" (pid {p})")).unwrap_or_default();
    let text = if st.running && id == QUEUE_SETTINGS_ID {
        format!("Background loop: running{pid}.")
    } else if st.running {
        format!("Background loop: a drain autopilot is running{pid}.")
    } else {
        match (
            st.status["state"].as_str(),
            st.status["stop_reason"].as_str(),
        ) {
            (Some(state), Some(why)) => {
                format!("Background loop: not running (last state {state}: {why}).")
            }
            (Some(state), None) => format!("Background loop: not running (last state {state})."),
            _ => "Background loop: not running.".into(),
        }
    };
    (
        json!({"running": st.running, "pid": st.pid, "kind": if id == QUEUE_SETTINGS_ID { "queue" } else { "drain" }}),
        text,
    )
}

pub fn view() -> (Value, String) {
    match connect_running() {
        Ok(Some(mut client)) => summarize(client.autopilot_status().ok().as_ref()),
        Ok(None) => (
            json!({"running": false}),
            "Background loop: not running (the runner is off).".into(),
        ),
        Err(_) => summarize(None),
    }
}

pub fn after_enable(paths: &Paths, mut out: Output) -> Output {
    let (loop_state, note): (Value, String) = match ensure_running(paths) {
        Ok(Hosting::Started(pid)) => (
            json!({"state": "started", "pid": pid}),
            format!("The queue now runs by itself in the background (loop pid {pid}). Watch it with `tmgr queue status --tail`; turn it off with `tmgr queue disable`."),
        ),
        Ok(Hosting::Running) => (
            json!({"state": "running"}),
            "The queue now runs by itself in the background. Watch it with `tmgr queue status --tail`; turn it off with `tmgr queue disable`.".into(),
        ),
        Ok(Hosting::DrainRunning) => (
            json!({"state": "drain"}),
            "A drain autopilot is running and works through the queue. Watch it with `tmgr queue autopilot status --watch`.".into(),
        ),
        Ok(Hosting::Skipped) => return out,
        Err(e) => (
            json!({"state": "failed", "error": e.message}),
            format!(
                "The background loop did not start: {}\nThe queue is on. Fix that and run `tmgr queue enable` again, or drive it with `tmgr queue run-once`.",
                e.message
            ),
        ),
    };
    out.data["loop"] = loop_state;
    out.human = format!("{}\n{note}", out.human);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_queue::rules::Rule;

    fn rule(name: &str, profile: &str, max: u32) -> Rule {
        Rule {
            name: name.into(),
            workspace: "local:personal".into(),
            project: None,
            status_type: "backlog".into(),
            persona: "dev".into(),
            profile: profile.into(),
            model: None,
            repos: vec!["/tmp/repo".into()],
            max_concurrent: max,
            hours: None,
            trusted_authors: vec!["human".into()],
            auto: true,
            mode: None,
        }
    }

    fn file(rules: Vec<Rule>) -> QueueFile {
        QueueFile {
            rules,
            ..QueueFile::default()
        }
    }

    #[test]
    fn the_queue_loop_has_no_guard_no_caps_and_the_sum_of_the_rule_slots() {
        let s = settings_for(
            &file(vec![rule("a", "x", 2), rule("b", "y", 3)]),
            Utc::now(),
        );
        assert_eq!(s.id, "queue");
        assert_eq!(s.kind, LoopKind::Queue);
        assert!(!s.answers.usage_guard);
        assert_eq!(s.answers.max_concurrent, 5);
        assert!(s.answers.max_tasks.is_none() && s.answers.max_hours.is_none());
        assert!(s.answers.max_cost_usd.is_none());
        assert!(s.tasks.is_empty() && s.windows.is_empty());
        assert_eq!(s.rule, "all");
    }

    fn status(running: bool, id: &str, state: &str, why: Option<&str>) -> AutopilotStatus {
        AutopilotStatus {
            running,
            pid: running.then_some(77),
            status: json!({"settings_id": id, "state": state, "stop_reason": why}),
        }
    }

    #[test]
    fn the_status_line_tells_the_queue_loop_from_a_drain_and_from_nothing() {
        let line = |s| summarize(Some(&s)).1;
        assert_eq!(
            line(status(true, "queue", "running", None)),
            "Background loop: running (pid 77)."
        );
        assert_eq!(
            line(status(true, "ap-1", "running", None)),
            "Background loop: a drain autopilot is running (pid 77)."
        );
        assert_eq!(
            line(status(
                false,
                "queue",
                "stopped",
                Some("the queue was turned off")
            )),
            "Background loop: not running (last state stopped: the queue was turned off)."
        );
        assert_eq!(
            line(AutopilotStatus {
                running: false,
                pid: None,
                status: Value::Null
            }),
            "Background loop: not running."
        );
        assert_eq!(summarize(None).1, "Background loop: unknown.");
        assert_eq!(summarize(None).0["running"], Value::Null);
    }

    #[test]
    fn no_rules_still_leaves_one_slot() {
        assert_eq!(
            settings_for(&file(vec![]), Utc::now())
                .answers
                .max_concurrent,
            1
        );
    }
}
