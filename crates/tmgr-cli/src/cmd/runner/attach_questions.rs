use super::attach_title::AgentSrc;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use tmgr_runner::questions::haiku::find_claude;
use tmgr_runner::questions::{transcript_path, Entry, Scanner, SessionSrc, Store};
use tmgr_runner::resume::id_from_log;
use tmgr_runner::wire::now_ms;

const MAX_ENTRIES: usize = 400;

pub(super) struct QuestionJob {
    pub id: String,
    pub src: AgentSrc,
    pub pid: u32,
    pub sandbox_home: PathBuf,
    pub log: PathBuf,
    pub exited: bool,
    pub settled: bool,
}

fn session_src(job: &QuestionJob) -> Option<SessionSrc> {
    if !std::fs::symlink_metadata(&job.sandbox_home).is_ok_and(|m| m.is_dir()) {
        return None;
    }
    let mut src = SessionSrc {
        id: job.id.clone(),
        format: job.src.format,
        home: job.sandbox_home.clone(),
        cwd: job.src.cwd.clone(),
        pid: if job.exited { 0 } else { job.pid },
        log_id: None,
        settled: job.settled,
    };
    if job.exited || transcript_path(&src).is_none() {
        src.log_id = id_from_log(&job.log, &job.src.profile);
    }
    Some(src)
}

pub(super) fn claude_binary() -> Option<PathBuf> {
    find_claude(
        std::env::var_os("PATH").as_deref(),
        std::env::var_os("HOME").map(PathBuf::from).as_deref(),
    )
}

pub(super) fn questioner(
    jobs: Receiver<Vec<QuestionJob>>,
    runner_dir: PathBuf,
    claude: Option<PathBuf>,
    mut send: impl FnMut(Vec<Entry>) -> bool,
) {
    let store = Store::new(&runner_dir);
    let mut scanner = Scanner::new(store.clone(), claude, std::env::temp_dir());
    for batch in jobs {
        if !batch.is_empty() {
            if let Ok(Some(_lock)) = store.try_lock() {
                let sources: Vec<SessionSrc> = batch.iter().filter_map(session_src).collect();
                scanner.scan(&sources, now_ms() / 1000);
            }
        }
        let mut entries = store.entries(None);
        entries.truncate(MAX_ENTRIES);
        if !send(entries) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::mpsc;
    use tmgr_runner::attach::Format;

    const U: &str = "2f0e1c3a-5b7d-4e8f-9a1b-3c4d5e6f7a8b";

    fn job(home: &std::path::Path, dir: &std::path::Path, settled: bool) -> QuestionJob {
        QuestionJob {
            id: "s_abcdefghjk".into(),
            src: AgentSrc {
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
            settled,
        }
    }

    #[test]
    fn the_worker_finds_questions_in_the_session_home_and_reports_the_store() {
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
        writeln!(f, r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"Done. Should I also update the docs?"}}]}}}}"#).unwrap();
        std::fs::write(
            runner.path().join("output.log"),
            format!("claude --resume {U}\n"),
        )
        .unwrap();

        let (tx, rx) = mpsc::sync_channel(1);
        let (out_tx, out_rx) = mpsc::channel();
        let dir = runner.path().to_path_buf();
        let handle = std::thread::spawn(move || {
            questioner(rx, dir, None, move |found| out_tx.send(found).is_ok())
        });
        tx.send(vec![job(home.path(), runner.path(), true)])
            .unwrap();
        let found = out_rx.recv().unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].q.text, "Should I also update the docs?");
        assert_eq!(found[0].q.session, "s_abcdefghjk");
        tx.send(Vec::new()).unwrap();
        assert_eq!(out_rx.recv().unwrap().len(), 1);
        drop(tx);
        handle.join().unwrap();
    }

    #[test]
    fn a_missing_home_is_skipped() {
        let runner = tempfile::tempdir().unwrap();
        let missing = runner.path().join("nope");
        assert!(session_src(&job(&missing, runner.path(), true)).is_none());
    }
}
