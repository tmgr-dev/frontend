use super::attach_consent::Reads;
use super::attach_questions::{session_src, QuestionJob};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;
use tmgr_core::redact::redact_secrets;
use tmgr_runner::attach::{peek_lines, Format, SessionStatus, StatusScan};
use tmgr_runner::questions::transcript_path;

const MAX_SCANS: usize = 64;

pub(super) fn status_worker(
    jobs: Receiver<Vec<QuestionJob>>,
    consent: impl Fn() -> Reads,
    mut send: impl FnMut(HashMap<String, SessionStatus>) -> bool,
) {
    let mut scans: HashMap<String, StatusScan> = HashMap::new();
    for batch in jobs {
        if scans.len() > MAX_SCANS {
            let keep: HashSet<&str> = batch.iter().map(|j| j.id.as_str()).collect();
            scans.retain(|id, _| keep.contains(id.as_str()));
        }
        let mut found = HashMap::new();
        for job in batch.iter().filter(|j| j.src.format == Format::Claude) {
            if let Some(status) = status_if_allowed(job, consent(), &mut scans) {
                found.insert(job.id.clone(), status);
            }
        }
        if !send(found) {
            return;
        }
    }
}

pub(super) struct PeekJob {
    pub job: QuestionJob,
    pub sub: String,
    pub agent: Option<String>,
}

pub(super) fn peek_worker(
    jobs: Receiver<PeekJob>,
    consent: impl Fn() -> Reads,
    mut send: impl FnMut(String, String, Vec<String>) -> bool,
) {
    for PeekJob { job, sub, agent } in jobs {
        let allowed = consent().allows(job.src.format);
        let lines = allowed
            .then_some(&job)
            .and_then(session_src)
            .and_then(|src| {
                let path = transcript_path(&src)?;
                Some(peek_lines(&src.home, &path, &sub, agent.as_deref()))
            })
            .unwrap_or_default()
            .into_iter()
            .map(|l| redact_secrets(&l))
            .collect();
        if !send(job.id.clone(), sub, lines) {
            return;
        }
    }
}

fn status_if_allowed(
    job: &QuestionJob,
    reads: Reads,
    scans: &mut HashMap<String, StatusScan>,
) -> Option<SessionStatus> {
    if !reads.allows(job.src.format) {
        scans.remove(&job.id);
        return None;
    }
    status_for(job, scans)
}

fn status_for(job: &QuestionJob, scans: &mut HashMap<String, StatusScan>) -> Option<SessionStatus> {
    let Some(src) = session_src(job) else {
        scans.remove(&job.id);
        return None;
    };
    let path = transcript_path(&src)?;
    let scan = scans
        .entry(job.id.clone())
        .or_insert_with(|| StatusScan::new(src.home.clone(), path.clone()));
    if !scan.is_for(&path) {
        *scan = StatusScan::new(src.home.clone(), path);
    }
    scan.poll();
    let mut status = scan.status();
    status.activity = status.activity.map(|a| redact_secrets(&a));
    Some(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::mpsc;
    use tmgr_runner::attach::Format;

    const U: &str = "2f0e1c3a-5b7d-4e8f-9a1b-3c4d5e6f7a8b";

    const ALL: Reads = Reads {
        claude: true,
        codex: true,
    };
    const NONE: Reads = Reads {
        claude: false,
        codex: false,
    };

    fn job(home: &std::path::Path, dir: &std::path::Path) -> QuestionJob {
        QuestionJob {
            id: "s_abcdefghjk".into(),
            src: super::super::attach_title::AgentSrc {
                profile: "claude-code".into(),
                format: Format::Claude,
                cwd: "/work/x".into(),
                pid: 0,
                default_name: "claude".into(),
                no_sandbox: false,
                created_ms: 0,
            },
            pid: 0,
            sandbox_home: home.to_path_buf(),
            log: dir.join("output.log"),
            exited: true,
            settled: false,
        }
    }

    #[test]
    fn the_worker_reports_the_running_tool_and_redacts_bearer_tokens() {
        let runner = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let proj = home.path().join(".claude/projects/-work-x");
        std::fs::create_dir_all(&proj).unwrap();
        let mut f = std::fs::File::create(proj.join(format!("{U}.jsonl"))).unwrap();
        writeln!(
            f,
            r#"{{"type":"user","message":{{"role":"user","content":"go"}}}}"#
        )
        .unwrap();
        writeln!(f, r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"a","name":"Bash","input":{{"command":"curl -H 'Authorization: Bearer abc123' x"}}}}]}}}}"#).unwrap();
        std::fs::write(
            runner.path().join("output.log"),
            format!("claude --resume {U}\n"),
        )
        .unwrap();
        let (tx, rx) = mpsc::sync_channel(1);
        let (out_tx, out_rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            status_worker(rx, || ALL, move |found| out_tx.send(found).is_ok())
        });
        tx.send(vec![job(home.path(), runner.path())]).unwrap();
        let found = out_rx.recv().unwrap();
        let activity = found["s_abcdefghjk"].activity.clone().unwrap();
        assert!(activity.starts_with("Bash: curl"), "{activity}");
        assert!(!activity.contains("abc123"), "{activity}");
        drop(tx);
        handle.join().unwrap();
    }

    #[test]
    fn codex_sessions_report_nothing() {
        let runner = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let mut j = job(home.path(), runner.path());
        j.src.format = Format::Codex;
        let (tx, rx) = mpsc::sync_channel(1);
        let (out_tx, out_rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            status_worker(rx, || ALL, move |found| out_tx.send(found).is_ok())
        });
        tx.send(vec![j]).unwrap();
        assert!(out_rx.recv().unwrap().is_empty());
        drop(tx);
        handle.join().unwrap();
    }

    #[test]
    fn the_peek_worker_returns_the_subagent_lines() {
        let runner = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let proj = home.path().join(".claude/projects/-work-x");
        let subs = proj.join(U).join("subagents");
        std::fs::create_dir_all(&subs).unwrap();
        std::fs::write(proj.join(format!("{U}.jsonl")), "").unwrap();
        std::fs::write(subs.join("agent-z.meta.json"), r#"{"toolUseId":"tu1"}"#).unwrap();
        std::fs::write(
            subs.join("agent-z.jsonl"),
            r#"{"type":"user","message":{"role":"user","content":"look at Bearer abc123 now"}}"#,
        )
        .unwrap();
        std::fs::write(
            runner.path().join("output.log"),
            format!("claude --resume {U}\n"),
        )
        .unwrap();
        let (tx, rx) = mpsc::sync_channel(1);
        let (out_tx, out_rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            peek_worker(
                rx,
                || ALL,
                move |s, sub, lines| out_tx.send((s, sub, lines)).is_ok(),
            )
        });
        tx.send(PeekJob {
            job: job(home.path(), runner.path()),
            sub: "tu1".into(),
            agent: None,
        })
        .unwrap();
        let (session, sub, lines) = out_rx.recv().unwrap();
        assert_eq!((session.as_str(), sub.as_str()), ("s_abcdefghjk", "tu1"));
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("prompt: look at Bearer "), "{lines:?}");
        assert!(!lines[0].contains("abc123"), "{lines:?}");
        drop(tx);
        handle.join().unwrap();
    }

    fn tool_use(cmd: &str) -> String {
        format!(
            "{{\"type\":\"assistant\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"tool_use\",\"id\":\"a\",\"name\":\"Bash\",\"input\":{{\"command\":\"{cmd}\"}}}}]}}}}\n"
        )
    }

    fn session(runner: &std::path::Path, home: &std::path::Path) -> std::path::PathBuf {
        let proj = home.join(".claude/projects/-work-x");
        std::fs::create_dir_all(&proj).unwrap();
        let file = proj.join(format!("{U}.jsonl"));
        std::fs::write(&file, tool_use("ls")).unwrap();
        std::fs::write(runner.join("output.log"), format!("claude --resume {U}\n")).unwrap();
        file
    }

    fn append_line(file: &std::path::Path, line: &str) {
        let mut f = std::fs::OpenOptions::new().append(true).open(file).unwrap();
        f.write_all(line.as_bytes()).unwrap();
    }

    #[test]
    fn the_status_worker_reports_nothing_without_consent_and_stops_following_the_file() {
        use std::cell::Cell;
        let runner = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let file = session(runner.path(), home.path());
        let (tx, rx) = mpsc::channel();
        for _ in 0..4 {
            tx.send(vec![job(home.path(), runner.path())]).unwrap();
        }
        drop(tx);
        let state = Cell::new(NONE);
        let mut round = 0;
        let mut got = Vec::new();
        status_worker(
            rx,
            || state.get(),
            |found| {
                got.push(found.get("s_abcdefghjk").and_then(|s| s.activity.clone()));
                round += 1;
                match round {
                    1 => state.set(ALL),
                    2 => state.set(NONE),
                    _ => {}
                }
                if round == 2 {
                    append_line(&file, &tool_use("secret-after-denial"));
                }
                true
            },
        );
        assert_eq!(got[0], None);
        assert_eq!(got[1].as_deref(), Some("Bash: ls"));
        assert_eq!(got[2], None);
        assert_eq!(got[3], None);
    }

    #[test]
    fn a_denied_job_drops_its_scan_and_an_allowed_one_keeps_it() {
        let runner = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        session(runner.path(), home.path());
        let j = job(home.path(), runner.path());
        let mut scans = HashMap::new();
        assert!(status_if_allowed(&j, ALL, &mut scans).is_some());
        assert_eq!(scans.len(), 1);
        assert!(status_if_allowed(&j, NONE, &mut scans).is_none());
        assert!(scans.is_empty());
    }

    #[test]
    fn a_denial_during_a_batch_stops_the_next_file_from_being_read() {
        use std::cell::Cell;
        let runner = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        session(runner.path(), home.path());
        let mut second = job(home.path(), runner.path());
        second.id = "s_bcdefghjkm".into();
        let (tx, rx) = mpsc::channel();
        tx.send(vec![job(home.path(), runner.path()), second])
            .unwrap();
        drop(tx);
        let calls = Cell::new(0);
        let mut got = Vec::new();
        status_worker(
            rx,
            || {
                calls.set(calls.get() + 1);
                if calls.get() == 1 {
                    ALL
                } else {
                    NONE
                }
            },
            |found| {
                got.push(found);
                true
            },
        );
        assert_eq!(calls.get(), 2);
        assert!(got[0].contains_key("s_abcdefghjk"));
        assert!(!got[0].contains_key("s_bcdefghjkm"));
    }

    #[test]
    fn a_denied_peek_sends_empty_lines() {
        use std::cell::Cell;
        let runner = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        session(runner.path(), home.path());
        let proj = home.path().join(".claude/projects/-work-x");
        let subs = proj.join(U).join("subagents");
        std::fs::create_dir_all(&subs).unwrap();
        std::fs::write(subs.join("agent-z.meta.json"), r#"{"toolUseId":"tu1"}"#).unwrap();
        std::fs::write(
            subs.join("agent-z.jsonl"),
            r#"{"type":"user","message":{"role":"user","content":"look at this"}}"#,
        )
        .unwrap();
        let (tx, rx) = mpsc::channel();
        for _ in 0..3 {
            tx.send(PeekJob {
                job: job(home.path(), runner.path()),
                sub: "tu1".into(),
                agent: None,
            })
            .unwrap();
        }
        drop(tx);
        let state = Cell::new(NONE);
        let mut got = Vec::new();
        peek_worker(
            rx,
            || state.get(),
            |_, _, lines| {
                got.push(lines);
                state.set(if got.len() == 1 { ALL } else { NONE });
                true
            },
        );
        assert!(got[0].is_empty());
        assert_eq!(got[1].len(), 1);
        assert!(got[2].is_empty());
    }

    #[test]
    fn consent_for_codex_alone_does_not_open_a_claude_transcript() {
        let runner = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        session(runner.path(), home.path());
        let (tx, rx) = mpsc::channel();
        tx.send(vec![job(home.path(), runner.path())]).unwrap();
        drop(tx);
        let mut got = Vec::new();
        status_worker(
            rx,
            || Reads {
                claude: false,
                codex: true,
            },
            |found| {
                got.push(found);
                true
            },
        );
        assert!(got[0].is_empty());
    }
}
