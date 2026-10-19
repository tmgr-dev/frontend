use std::io::{self, Write};
use std::time::Duration;

use chrono::{FixedOffset, Utc};
use serde_json::Value;
use tmgr_core::error::{Error, Result};

use super::control::{self, Hosted};
use super::render;
use crate::cmd::runner::rerr;
use crate::out::Output;

const POLL: Duration = Duration::from_secs(1);
const WINDOW: u32 = tmgr_ipc::api::limits::MAX_AUTOPILOT_LOG;

#[derive(Default)]
pub struct Follow {
    seen: Vec<String>,
}

impl Follow {
    pub fn new(start: &[Value]) -> Follow {
        Follow {
            seen: start.iter().map(Value::to_string).collect(),
        }
    }

    pub fn fresh(&mut self, now: Vec<Value>) -> Vec<Value> {
        let cur: Vec<String> = now.iter().map(Value::to_string).collect();
        let most = self.seen.len().min(cur.len());
        let skip = (0..=most)
            .rev()
            .find(|k| self.seen[self.seen.len() - k..] == cur[..*k])
            .unwrap_or(0);
        self.seen = cur;
        now.into_iter().skip(skip).collect()
    }
}

pub fn poll(
    host: &mut dyn Hosted,
    follow: &mut Follow,
    out: &mut dyn Write,
    tz: FixedOffset,
) -> io::Result<()> {
    for line in follow.fresh(host.log(WINDOW)?) {
        writeln!(out, "{}", render::log_line(&line, tz))?;
    }
    out.flush()
}

pub fn run(queue_line: &str, json: bool) -> Result<Output> {
    if json {
        return Err(Error::usage("--tail cannot be combined with --json"));
    }
    let tz = control::local_offset();
    let mut client = control::connect_running().unwrap_or(None);
    let mut follow = client
        .as_mut()
        .and_then(|c| c.log(WINDOW).ok())
        .map_or_else(Follow::default, |lines| Follow::new(&lines));
    let header = match client.as_mut() {
        Some(c) => control::status_output(Some(c as &mut dyn Hosted), Utc::now(), tz)
            .map(|(o, _)| o.human)
            .unwrap_or_else(|_| render::not_running_text()),
        None => "The runner is not running, so no background loop is working.".into(),
    };
    println!("{queue_line}\n{header}\n\nNew events (Ctrl+C to stop):");
    let mut stdout = io::stdout();
    loop {
        std::thread::sleep(POLL);
        if client.is_none() {
            client = control::connect_running().unwrap_or(None);
        }
        let Some(c) = client.as_mut() else { continue };
        if let Err(e) = poll(c, &mut follow, &mut stdout, tz) {
            if e.kind() == io::ErrorKind::BrokenPipe {
                return Err(rerr(e));
            }
            client = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn line(n: i64) -> Value {
        json!({"at": "2026-10-19T17:52:00Z", "kind": "picked", "task": format!("TM-{n}"), "text": "x"})
    }

    fn lines(range: std::ops::RangeInclusive<i64>) -> Vec<Value> {
        range.map(line).collect()
    }

    #[test]
    fn only_lines_after_the_last_seen_ones_are_new() {
        let mut f = Follow::new(&lines(1..=3));
        assert!(f.fresh(lines(1..=3)).is_empty());
        assert_eq!(f.fresh(lines(1..=5)), lines(4..=5));
        assert!(f.fresh(lines(1..=5)).is_empty());
    }

    #[test]
    fn a_sliding_window_reports_each_new_line_once() {
        let mut f = Follow::new(&lines(1..=4));
        assert_eq!(f.fresh(lines(3..=6)), lines(5..=6));
        assert_eq!(f.fresh(lines(4..=7)), lines(7..=7));
    }

    #[test]
    fn identical_lines_are_counted_by_position() {
        let a = line(1);
        let mut f = Follow::new(&[a.clone()]);
        assert_eq!(f.fresh(vec![a.clone(), a.clone()]), vec![a.clone()]);
        assert_eq!(
            f.fresh(vec![a.clone(), a.clone(), a.clone(), a.clone()]),
            vec![a.clone(), a.clone()]
        );
    }

    #[test]
    fn without_a_baseline_everything_is_new_and_an_empty_log_stays_quiet() {
        let mut f = Follow::default();
        assert!(f.fresh(vec![]).is_empty());
        assert_eq!(f.fresh(lines(1..=2)), lines(1..=2));
    }

    #[test]
    fn a_log_that_started_over_is_shown_in_full() {
        let mut f = Follow::new(&lines(1..=3));
        assert_eq!(f.fresh(lines(10..=11)), lines(10..=11));
    }

    struct Fake(Vec<Vec<Value>>);

    impl Hosted for Fake {
        fn status(&mut self) -> io::Result<tmgr_ipc::api::AutopilotStatus> {
            unreachable!()
        }
        fn control(&mut self, _: tmgr_ipc::api::AutopilotCommand) -> io::Result<u64> {
            unreachable!()
        }
        fn log(&mut self, tail: u32) -> io::Result<Vec<Value>> {
            assert_eq!(tail, WINDOW);
            Ok(self.0.remove(0))
        }
    }

    #[test]
    fn poll_prints_new_events_as_log_lines() {
        let tz = FixedOffset::east_opt(0).unwrap();
        let mut host = Fake(vec![lines(1..=2), lines(1..=3)]);
        let mut f = Follow::new(&lines(1..=1));
        let mut out = Vec::new();
        poll(&mut host, &mut f, &mut out, tz).unwrap();
        poll(&mut host, &mut f, &mut out, tz).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "17:52 picked TM-2 x\n17:52 picked TM-3 x\n"
        );
    }
}
