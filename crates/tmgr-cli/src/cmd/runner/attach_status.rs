use super::attach_questions::{session_src, QuestionJob};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;
use tmgr_core::redact::redact_secrets;
use tmgr_runner::attach::{Format, SessionStatus, StatusScan};
use tmgr_runner::questions::transcript_path;

const MAX_SCANS: usize = 64;

pub(super) fn status_worker(
    jobs: Receiver<Vec<QuestionJob>>,
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
            if let Some(status) = status_for(job, &mut scans) {
                found.insert(job.id.clone(), status);
            }
        }
        if !send(found) {
            return;
        }
    }
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

    fn job(home: &std::path::Path, dir: &std::path::Path) -> QuestionJob {
        QuestionJob {
            id: "s_abcdefghjk".into(),
            src: super::super::attach_title::AgentSrc {
                profile: "claude-code".into(),
                format: Format::Claude,
                cwd: "/work/x".into(),
                pid: 0,
                default_name: "claude".into(),
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
        let handle =
            std::thread::spawn(move || status_worker(rx, move |found| out_tx.send(found).is_ok()));
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
        let handle =
            std::thread::spawn(move || status_worker(rx, move |found| out_tx.send(found).is_ok()));
        tx.send(vec![j]).unwrap();
        assert!(out_rx.recv().unwrap().is_empty());
        drop(tx);
        handle.join().unwrap();
    }
}
