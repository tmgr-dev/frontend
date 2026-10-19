use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use chrono::{DateTime, FixedOffset, Local, Utc};
use serde_json::{json, Value};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{AutopilotCommand, AutopilotStatus};

use super::model::{RunState, StatusDoc};
use super::render;
use crate::cmd::runner::{connect, rerr, runner_config};
use crate::out::Output;

const WATCH_EVERY: Duration = Duration::from_secs(2);
const IO_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_TAIL: u32 = 50;

pub trait Hosted {
    fn status(&mut self) -> io::Result<AutopilotStatus>;
    fn control(&mut self, command: AutopilotCommand) -> io::Result<u64>;
    fn log(&mut self, tail: u32) -> io::Result<Vec<Value>>;
}

impl Hosted for tmgr_runner::client::Client {
    fn status(&mut self) -> io::Result<AutopilotStatus> {
        self.autopilot_status()
    }

    fn control(&mut self, command: AutopilotCommand) -> io::Result<u64> {
        self.autopilot_control(command)
    }

    fn log(&mut self, tail: u32) -> io::Result<Vec<Value>> {
        self.autopilot_log(tail)
    }
}

/// The runner client, or `None` when no runner is running (nothing is started for a status).
pub fn connect_running() -> Result<Option<tmgr_runner::client::Client>> {
    let cfg = runner_config()?;
    match connect(&cfg) {
        Ok(client) => {
            client.set_timeouts(IO_TIMEOUT).map_err(rerr)?;
            Ok(Some(client))
        }
        Err(e) if e.kind == Kind::Offline => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn local_offset() -> FixedOffset {
    *Local::now().offset()
}

pub fn status_output(
    host: Option<&mut dyn Hosted>,
    now: DateTime<Utc>,
    tz: FixedOffset,
) -> Result<(Output, bool)> {
    let Some(host) = host else {
        return Ok((
            Output::new(json!({"running": false}), render::not_running_text()),
            true,
        ));
    };
    let st = host.status().map_err(rerr)?;
    if st.status.is_null() {
        return Ok((
            Output::new(json!({"running": st.running}), render::not_running_text()),
            !st.running,
        ));
    }
    let doc: StatusDoc = serde_json::from_value(st.status.clone())
        .map_err(|_| Error::internal("the autopilot status file is unreadable"))?;
    let mut data = st.status;
    data["running"] = json!(st.running);
    let finished = !st.running || doc.state == RunState::Stopped;
    Ok((
        Output::new(data, render::status_text(&doc, st.running, now, tz)),
        finished,
    ))
}

pub fn status(watch: bool, json: bool) -> Result<Output> {
    if !watch {
        return status_once();
    }
    if json {
        return Err(Error::usage("--watch cannot be combined with --json"));
    }
    let tty = io::stdout().is_terminal();
    loop {
        let (out, finished) = status_once_with_end()?;
        if tty {
            print!("\x1b[2J\x1b[H");
        }
        println!("{}", out.human.trim_end_matches('\n'));
        let _ = io::stdout().flush();
        if finished {
            return Ok(Output::new(out.data, String::new()));
        }
        std::thread::sleep(WATCH_EVERY);
    }
}

fn status_once() -> Result<Output> {
    status_once_with_end().map(|(o, _)| o)
}

fn status_once_with_end() -> Result<(Output, bool)> {
    let mut client = connect_running()?;
    let host = client.as_mut().map(|c| c as &mut dyn Hosted);
    status_output(host, Utc::now(), local_offset())
}

fn requires_runner<T>(client: Option<T>) -> Result<T> {
    client.ok_or_else(|| Error::new(Kind::Conflict, "autopilot is not running"))
}

pub fn control_output(host: &mut dyn Hosted, command: AutopilotCommand) -> Result<Output> {
    let seq = host.control(command).map_err(rerr)?;
    let (word, done) = match command {
        AutopilotCommand::Pause => (
            "pause",
            "Autopilot will pause: no new tasks start, running ones finish.",
        ),
        AutopilotCommand::Resume => ("resume", "Autopilot will resume."),
        AutopilotCommand::Stop => ("stop", "Autopilot will stop and turn the queue off."),
        AutopilotCommand::Suspend => (
            "suspend",
            "Autopilot will suspend: the queue stays on and nothing is interrupted.",
        ),
    };
    Ok(Output::new(
        json!({"command": word, "seq": seq}),
        format!("{done}\nWatch it with `tmgr queue autopilot status --watch`."),
    ))
}

pub fn control(command: AutopilotCommand) -> Result<Output> {
    let mut client = requires_runner(connect_running()?)?;
    control_output(&mut client, command)
}

pub fn log_output(host: &mut dyn Hosted, tail: u32, tz: FixedOffset) -> Result<Output> {
    if !(1..=tmgr_ipc::api::limits::MAX_AUTOPILOT_LOG).contains(&tail) {
        return Err(Error::usage(format!(
            "--tail must be 1 to {}",
            tmgr_ipc::api::limits::MAX_AUTOPILOT_LOG
        )));
    }
    let lines = host.log(tail).map_err(rerr)?;
    let human = if lines.is_empty() {
        "The autopilot log is empty.".to_string()
    } else {
        lines
            .iter()
            .map(|l| render::log_line(l, tz))
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(Output::new(json!({"lines": lines}), human))
}

pub fn log(tail: u32) -> Result<Output> {
    let mut client = requires_runner(connect_running()?)?;
    log_output(&mut client, tail, local_offset())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[derive(Default)]
    struct Fake {
        status: Option<AutopilotStatus>,
        sent: Vec<AutopilotCommand>,
        lines: Vec<Value>,
        asked: Vec<u32>,
        fail: Option<String>,
    }

    impl Hosted for Fake {
        fn status(&mut self) -> io::Result<AutopilotStatus> {
            Ok(self.status.clone().unwrap())
        }

        fn control(&mut self, c: AutopilotCommand) -> io::Result<u64> {
            if let Some(f) = &self.fail {
                return Err(io::Error::other(f.clone()));
            }
            self.sent.push(c);
            Ok(self.sent.len() as u64)
        }

        fn log(&mut self, tail: u32) -> io::Result<Vec<Value>> {
            self.asked.push(tail);
            Ok(self.lines.clone())
        }
    }

    fn tz() -> FixedOffset {
        FixedOffset::east_opt(0).unwrap()
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 19, 20, 3, 0).unwrap()
    }

    #[test]
    fn pause_resume_and_stop_send_one_command_each() {
        let mut f = Fake::default();
        for c in [
            AutopilotCommand::Pause,
            AutopilotCommand::Resume,
            AutopilotCommand::Stop,
        ] {
            let o = control_output(&mut f, c).unwrap();
            assert!(o.human.contains("status --watch"));
        }
        assert_eq!(
            f.sent,
            [
                AutopilotCommand::Pause,
                AutopilotCommand::Resume,
                AutopilotCommand::Stop
            ]
        );
        let o = control_output(&mut f, AutopilotCommand::Pause).unwrap();
        assert_eq!(o.data, json!({"command": "pause", "seq": 4}));
    }

    #[test]
    fn a_refusal_of_the_runner_keeps_its_exit_code() {
        let mut f = Fake {
            fail: Some("not_running: no autopilot is running".into()),
            ..Fake::default()
        };
        let e = control_output(&mut f, AutopilotCommand::Stop)
            .err()
            .unwrap();
        assert_eq!(e.kind, Kind::Conflict);
        let mut f = Fake {
            fail: Some("forbidden: this client may not do that".into()),
            ..Fake::default()
        };
        let e = control_output(&mut f, AutopilotCommand::Stop)
            .err()
            .unwrap();
        assert_eq!(e.kind, Kind::Forbidden);
    }

    #[test]
    fn log_prints_the_tail_as_text_and_json() {
        let mut f = Fake {
            lines: vec![
                json!({"at": "2026-10-19T17:52:00Z", "kind": "pr", "task": "TM-1", "text": "opened"}),
            ],
            ..Fake::default()
        };
        let o = log_output(&mut f, 20, tz()).unwrap();
        assert_eq!(f.asked, [20]);
        assert_eq!(o.human, "17:52 pr TM-1 opened");
        assert_eq!(o.data["lines"].as_array().unwrap().len(), 1);
        assert!(log_output(&mut f, 0, tz()).is_err());
        assert!(log_output(&mut f, 5000, tz()).is_err());
        f.lines.clear();
        assert_eq!(
            log_output(&mut f, 5, tz()).unwrap().human,
            "The autopilot log is empty."
        );
    }

    #[test]
    fn no_runner_or_no_status_means_not_running() {
        let (o, finished) = status_output(None, now(), tz()).unwrap();
        assert!(finished);
        assert_eq!(o.data, json!({"running": false}));
        assert!(o.human.starts_with("Autopilot is not running."));
        let mut f = Fake {
            status: Some(AutopilotStatus {
                running: false,
                pid: None,
                status: Value::Null,
            }),
            ..Fake::default()
        };
        let (o, finished) = status_output(Some(&mut f), now(), tz()).unwrap();
        assert!(finished);
        assert_eq!(o.data, json!({"running": false}));
    }

    #[test]
    fn status_json_is_the_status_file_plus_the_daemon_flag() {
        let doc = super::super::render::tests::doc();
        let mut f = Fake {
            status: Some(AutopilotStatus {
                running: true,
                pid: Some(4242),
                status: serde_json::to_value(&doc).unwrap(),
            }),
            ..Fake::default()
        };
        let (o, finished) = status_output(Some(&mut f), now(), tz()).unwrap();
        assert!(!finished);
        assert_eq!(o.data["running"], true);
        assert_eq!(o.data["tasks"]["total"], 100);
        assert!(o.human.starts_with("Autopilot running"));
    }

    #[test]
    fn a_stopped_status_ends_the_watch() {
        let mut doc = super::super::render::tests::doc();
        doc.state = RunState::Stopped;
        let mut f = Fake {
            status: Some(AutopilotStatus {
                running: true,
                pid: Some(1),
                status: serde_json::to_value(&doc).unwrap(),
            }),
            ..Fake::default()
        };
        assert!(status_output(Some(&mut f), now(), tz()).unwrap().1);
    }

    fn envelope(data: &Value) -> Value {
        json!({"schema": "tmgr.v1", "ok": true, "data": data, "warnings": []})
    }

    fn status_schema() -> Value {
        serde_json::from_str(include_str!(
            "../../../schemas/tmgr.v1/queue-autopilot-status.json"
        ))
        .unwrap()
    }

    #[test]
    fn status_json_validates_against_the_published_schema() {
        let schema = status_schema();
        let mut f = Fake {
            status: Some(AutopilotStatus {
                running: true,
                pid: Some(4242),
                status: serde_json::to_value(super::super::render::tests::doc()).unwrap(),
            }),
            ..Fake::default()
        };
        let (o, _) = status_output(Some(&mut f), now(), tz()).unwrap();
        jsonschema::validate(&schema, &envelope(&o.data)).unwrap();
        let (o, _) = status_output(None, now(), tz()).unwrap();
        jsonschema::validate(&schema, &envelope(&o.data)).unwrap();
        let broken = json!({"running": true, "state": "flying"});
        assert!(jsonschema::validate(&schema, &envelope(&broken)).is_err());
    }

    #[test]
    fn a_status_with_a_plan_validates_and_stays_additive() {
        use crate::cmd::autopilot::model::{
            NotPlanned, PlanDoc, PlanRow, Slots, TaskBrief, TriageRow,
        };
        let schema = status_schema();
        let mut doc = super::super::render::tests::doc();
        doc.runs[0].team_size = Some(3);
        doc.runs[0].team_mode = Some("eco".into());
        doc.runs[0].agents = Some(3);
        doc.runs[0].workspace_code = Some("tm".into());
        doc.runs[0].workspace_id = Some(7);
        doc.runs[0].session_id = Some("s1".into());
        doc.agents_planned = Some(Slots { running: 3, max: 6 });
        doc.plan = Some(PlanDoc {
            profile: "claude-code".into(),
            items: vec![PlanRow {
                task: "TM-12".into(),
                position: 1,
                mode: "eco".into(),
                state: "queued".into(),
                ..PlanRow::default()
            }],
            ..PlanDoc::default()
        });
        doc.triage = vec![TriageRow {
            task: "TM-2".into(),
            verdict: "blocked".into(),
            ..TriageRow::default()
        }];
        doc.not_planned = vec![NotPlanned::default()];
        doc.needs_feedback_tasks = Some(vec![TaskBrief::default()]);
        let value = serde_json::to_value(&doc).unwrap();
        for key in [
            "plan",
            "triage",
            "not_planned",
            "agents_planned",
            "needs_feedback_tasks",
        ] {
            assert!(value.get(key).is_some(), "{key}");
        }
        let mut f = Fake {
            status: Some(AutopilotStatus {
                running: true,
                pid: Some(1),
                status: value,
            }),
            ..Fake::default()
        };
        let (o, _) = status_output(Some(&mut f), now(), tz()).unwrap();
        jsonschema::validate(&schema, &envelope(&o.data)).unwrap();
        assert!(o.human.contains("Plan "));
        let bad = json!({"running": true, "plan": {"state": "later"}});
        assert!(jsonschema::validate(&schema, &envelope(&bad)).is_err());
        let old = serde_json::to_value(super::super::render::tests::doc()).unwrap();
        for key in [
            "plan",
            "triage",
            "not_planned",
            "agents_planned",
            "queued_tasks",
        ] {
            assert!(old.get(key).is_none(), "{key} leaks into an old status");
        }
    }

    #[test]
    fn a_garbled_status_file_is_an_error_not_a_guess() {
        let mut f = Fake {
            status: Some(AutopilotStatus {
                running: true,
                pid: Some(1),
                status: json!({"state": 7}),
            }),
            ..Fake::default()
        };
        assert!(status_output(Some(&mut f), now(), tz()).is_err());
    }
}
