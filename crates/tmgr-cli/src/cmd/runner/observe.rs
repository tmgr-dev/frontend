use super::ops::with_session;
use super::{connect, emit, rerr, runner_config, Opts, PeekArgs, SendArgs, WaitFor};
use serde_json::json;
use std::collections::HashMap;
use std::io::{self, Write};
use std::thread;
use std::time::{Duration, Instant};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{limits::MAX_TEXT_LINES, State, StateKind};
use tmgr_ipc::proto::ServerMsg;
use tmgr_runner::client::parse_event;
use tmgr_runner::sanitize::sanitize_text;
use tmgr_runner::wire::now_ms;

pub const EXIT_BLOCKED: i32 = 8;

const POLL: Duration = Duration::from_millis(100);
const LCS_CELLS: usize = 4_000_000;

fn clean(s: &str) -> String {
    sanitize_text(s).into_owned()
}

fn kind_name(k: StateKind) -> &'static str {
    match k {
        StateKind::Starting => "starting",
        StateKind::Working => "working",
        StateKind::Waiting => "waiting",
        StateKind::Blocked => "blocked",
        StateKind::Exited => "exited",
    }
}

fn clock(ms: u64) -> String {
    i64::try_from(ms)
        .ok()
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%H:%M:%S")
                .to_string()
        })
        .unwrap_or_default()
}

pub fn watch(o: &Opts) -> Result<i32> {
    let cfg = runner_config()?;
    let mut stream = connect(&cfg)?.attach(&[]).map_err(rerr)?;
    let mut names: HashMap<String, String> = connect(&cfg)?
        .list()
        .map_err(rerr)?
        .into_iter()
        .map(|s| (s.id, s.name))
        .collect();
    let stdout = io::stdout();
    loop {
        let msg = match stream.recv().map_err(rerr)? {
            Some(ServerMsg::Closed { reason }) => {
                return Err(rerr(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    reason,
                )))
            }
            Some(msg) => msg,
            None => return Err(rerr(io::ErrorKind::UnexpectedEof.into())),
        };
        let Some(ev) = parse_event(&msg) else {
            continue;
        };
        let line = if o.json {
            serde_json::to_string(&ev).map_err(|e| Error::internal(e.to_string()))?
        } else {
            if !names.contains_key(&ev.session) {
                if let Ok(list) = connect(&cfg).and_then(|mut c| c.list().map_err(rerr)) {
                    names.extend(list.into_iter().map(|s| (s.id, s.name)));
                }
            }
            let mut line = format!("{} {}", clock(ev.state.since_ms), clean(&ev.session));
            if let Some(name) = names.get(&ev.session) {
                line.push(' ');
                line.push_str(&tmgr_core::text::sanitize_line(name));
            }
            line.push(' ');
            line.push_str(kind_name(ev.state.kind));
            if let Some(d) = ev.state.detail.as_deref().filter(|d| !d.is_empty()) {
                line.push_str(": ");
                line.push_str(&clean(d));
            }
            line
        };
        let mut w = stdout.lock();
        if writeln!(w, "{line}").and_then(|()| w.flush()).is_err() {
            return Ok(0);
        }
    }
}

fn trim_blank_end(lines: &[String]) -> &[String] {
    let end = lines
        .iter()
        .rposition(|l| !l.is_empty())
        .map_or(0, |i| i + 1);
    &lines[..end]
}

pub fn peek(o: &Opts, a: &PeekArgs) -> Result<i32> {
    let (mut c, s) = with_session(&a.session)?;
    let t = c
        .text(&s.id, a.lines.map(|_| MAX_TEXT_LINES))
        .map_err(rerr)?;
    let all = trim_blank_end(&t.lines);
    let lines = &all[a.lines.map_or(0, |n| all.len().saturating_sub(n as usize))..];
    if o.json {
        return Ok(emit(
            o,
            json!({"session": s.id, "rows": t.rows, "cols": t.cols, "lines": lines}),
            "",
        ));
    }
    let mut w = io::stdout().lock();
    for l in lines {
        if writeln!(w, "{}", clean(l)).is_err() {
            break;
        }
    }
    Ok(0)
}

pub fn added_lines(before: &[String], after: &[String]) -> Vec<String> {
    let (a, b) = (trim_blank_end(before), trim_blank_end(after));
    let pre = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let (a, b) = (&a[pre..], &b[pre..]);
    let suf = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (a, b) = (&a[..a.len() - suf], &b[..b.len() - suf]);
    let mut matched = vec![false; b.len()];
    if a.len().saturating_mul(b.len()) <= LCS_CELLS {
        let w = b.len() + 1;
        let mut t = vec![0u32; (a.len() + 1) * w];
        for i in (0..a.len()).rev() {
            for j in (0..b.len()).rev() {
                t[i * w + j] = if a[i] == b[j] {
                    t[(i + 1) * w + j + 1] + 1
                } else {
                    t[(i + 1) * w + j].max(t[i * w + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < a.len() && j < b.len() {
            if a[i] == b[j] {
                matched[j] = true;
                i += 1;
                j += 1;
            } else if t[(i + 1) * w + j] >= t[i * w + j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    } else {
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for l in a {
            *seen.entry(l).or_default() += 1;
        }
        for (j, l) in b.iter().enumerate() {
            if let Some(n) = seen.get_mut(l.as_str()).filter(|n| **n > 0) {
                *n -= 1;
                matched[j] = true;
            }
        }
    }
    let added: Vec<String> = b
        .iter()
        .zip(&matched)
        .filter(|(_, m)| !**m)
        .map(|(l, _)| l.clone())
        .collect();
    let start = added
        .iter()
        .position(|l| !l.is_empty())
        .unwrap_or(added.len());
    trim_blank_end(&added[start..]).to_vec()
}

fn finished(s: &State, wait: WaitFor, sent_ms: u64) -> bool {
    match wait {
        WaitFor::Exit => s.kind == StateKind::Exited,
        WaitFor::Idle => {
            matches!(
                s.kind,
                StateKind::Waiting | StateKind::Blocked | StateKind::Exited
            ) && s.since_ms >= sent_ms
        }
    }
}

pub fn send_wait(o: &Opts, a: &SendArgs, wait: WaitFor) -> Result<i32> {
    let (mut c, s) = with_session(&a.session)?;
    let before = c.text(&s.id, Some(MAX_TEXT_LINES)).map_err(rerr)?.lines;
    let mut text = a.text.clone();
    if !a.no_enter {
        text.push('\r');
    }
    let started = Instant::now();
    let sent_ms = now_ms();
    c.send(&s.id, &text).map_err(rerr)?;
    let state = loop {
        let list = c.list().map_err(rerr)?;
        let Some(now) = list.into_iter().find(|x| x.id == s.id) else {
            return Err(Error::not_found(format!(
                "session {} was removed",
                clean(&s.name)
            )));
        };
        if finished(&now.state, wait, sent_ms) {
            break now.state;
        }
        if started.elapsed() >= a.timeout {
            let what = match wait {
                WaitFor::Idle => "to be idle",
                WaitFor::Exit => "to exit",
            };
            return Err(Error::new(
                Kind::Conflict,
                format!(
                    "timed out after {}s waiting for {} {what}",
                    a.timeout.as_secs(),
                    clean(&s.name)
                ),
            ));
        }
        thread::sleep(POLL);
    };
    let after = c.text(&s.id, Some(MAX_TEXT_LINES)).map_err(rerr)?.lines;
    let output = added_lines(&before, &after);
    let human = output
        .iter()
        .map(|l| clean(l))
        .collect::<Vec<_>>()
        .join("\n");
    emit(
        o,
        json!({"session": s.id, "state": state, "output": output}),
        human,
    );
    if !o.json {
        let note = match (state.kind, state.detail.as_deref()) {
            (StateKind::Blocked, d) => Some(
                format!("blocked: {}", clean(d.unwrap_or("")))
                    .trim_end()
                    .to_string(),
            ),
            (StateKind::Exited, _) => Some(format!(
                "exited with code {}",
                state.exit_code.map_or("?".into(), |c| c.to_string())
            )),
            _ => None,
        };
        if let Some(n) = note {
            eprintln!("{n}");
        }
    }
    Ok(if state.kind == StateKind::Blocked {
        EXIT_BLOCKED
    } else {
        0
    })
}

#[cfg(test)]
mod tests {
    use super::added_lines;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn appended_lines_are_the_diff() {
        let got = added_lines(&v(&["$ a", "x"]), &v(&["$ a", "x", "got:1", "$"]));
        assert_eq!(got, v(&["got:1", "$"]));
    }

    #[test]
    fn redrawn_lines_count_as_new_and_kept_lines_do_not() {
        let before = v(&["header", "one", "two", "> "]);
        let after = v(&["header", "two", "three", "> typed"]);
        assert_eq!(added_lines(&before, &after), v(&["three", "> typed"]));
    }

    #[test]
    fn blank_edges_and_identical_screens() {
        assert!(added_lines(&v(&["a", ""]), &v(&["a"])).is_empty());
        assert_eq!(
            added_lines(&v(&[]), &v(&["", "a", "", "b", ""])),
            v(&["a", "", "b"])
        );
    }

    #[test]
    fn scrolled_screens_still_diff() {
        let before: Vec<String> = (0..30).map(|i| format!("l{i}")).collect();
        let after: Vec<String> = (5..40).map(|i| format!("l{i}")).collect();
        assert_eq!(added_lines(&before, &after).len(), 10);
    }
}
