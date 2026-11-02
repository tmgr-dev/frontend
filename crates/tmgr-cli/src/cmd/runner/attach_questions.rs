use super::attach_consent::Reads;
use super::attach_title::AgentSrc;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use tmgr_runner::questions::haiku::find_claude;
use tmgr_runner::questions::{transcript_path, Entry, Scanner, SessionSrc, Status, Store};
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

pub(super) fn session_src(job: &QuestionJob) -> Option<SessionSrc> {
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
    consent: impl Fn() -> Reads + Send + Sync + 'static,
    mut send: impl FnMut(Vec<Entry>, Reads) -> bool,
) {
    let store = Store::new(&runner_dir);
    let workdir = store.dir().join("haiku-cwd");
    let claude = claude.filter(|_| tmgr_ipc::dirs::ensure_private_dir(&workdir).is_ok());
    let consent = Arc::new(consent);
    let gate = consent.clone();
    let mut scanner = Scanner::new(store.clone(), claude, workdir, move |f| gate().allows(f));
    for batch in jobs {
        let reads = consent();
        scanner.forget_denied();
        if !batch.is_empty() && reads.any() {
            if let Ok(Some(_lock)) = store.try_lock() {
                let sources: Vec<SessionSrc> = batch
                    .iter()
                    .filter(|j| reads.allows(j.src.format))
                    .filter_map(session_src)
                    .collect();
                scanner.scan(&sources, now_ms() / 1000);
            }
        }
        let mut entries = store.entries(None);
        entries.sort_by_key(|e| e.status != Status::Open);
        entries.truncate(MAX_ENTRIES);
        if !send(entries, reads) {
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

    const ALL: Reads = Reads {
        claude: true,
        codex: true,
    };

    fn job(home: &std::path::Path, dir: &std::path::Path, settled: bool) -> QuestionJob {
        QuestionJob {
            id: "s_abcdefghjk".into(),
            src: AgentSrc {
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
            settled,
        }
    }

    #[test]
    fn the_worker_finds_questions_in_the_session_home_and_reports_the_store() {
        let runner = tempfile::tempdir().unwrap();
        std::fs::set_permissions(
            runner.path(),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .unwrap();
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
            questioner(
                rx,
                dir,
                None,
                || ALL,
                move |found, _| out_tx.send(found).is_ok(),
            )
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

    fn private(dir: &std::path::Path) {
        std::fs::set_permissions(dir, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    }

    fn transcript(home: &std::path::Path, runner: &std::path::Path, text: &str) {
        let proj = home.join(".claude/projects/-work-x");
        std::fs::create_dir_all(&proj).unwrap();
        let mut f = std::fs::File::create(proj.join(format!("{U}.jsonl"))).unwrap();
        writeln!(
            f,
            r#"{{"type":"user","message":{{"role":"user","content":"go"}}}}"#
        )
        .unwrap();
        writeln!(f, r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"{text}"}}]}}}}"#).unwrap();
        std::fs::write(runner.join("output.log"), format!("claude --resume {U}\n")).unwrap();
    }

    #[test]
    fn without_consent_nothing_new_is_stored_but_stored_questions_are_still_sent() {
        use tmgr_runner::questions::{Origin, Question, SessionFile};
        let runner = tempfile::tempdir().unwrap();
        private(runner.path());
        let home = tempfile::tempdir().unwrap();
        transcript(
            home.path(),
            runner.path(),
            "Done. Should I also update the docs?",
        );
        let store = Store::new(runner.path());
        let old = Question {
            id: "aaaa11112222".into(),
            session: "s_zzzzzzzzzz".into(),
            text: "Kept from before?".into(),
            asked_s: now_ms() / 1000,
            origin: Origin::Rule,
            user: String::new(),
            reply: String::new(),
            item: String::new(),
            pos: 0,
            turn: 0,
        };
        let file = SessionFile {
            questions: vec![old],
            ..SessionFile::default()
        };
        store.save("s_zzzzzzzzzz", &file, now_ms() / 1000).unwrap();

        let (tx, rx) = mpsc::sync_channel(1);
        let (out_tx, out_rx) = mpsc::channel();
        let dir = runner.path().to_path_buf();
        let handle = std::thread::spawn(move || {
            questioner(rx, dir, None, Reads::default, move |found, reads| {
                out_tx.send((found, reads)).is_ok()
            })
        });
        tx.send(vec![job(home.path(), runner.path(), true)])
            .unwrap();
        let (found, reads) = out_rx.recv().unwrap();
        assert_eq!(reads, Reads::default());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].q.text, "Kept from before?");
        assert!(!Store::new(runner.path())
            .dir()
            .join("s_abcdefghjk.json")
            .exists());
        drop(tx);
        handle.join().unwrap();
    }

    #[test]
    fn a_tool_without_consent_is_not_scanned_while_the_other_is() {
        let runner = tempfile::tempdir().unwrap();
        private(runner.path());
        let home = tempfile::tempdir().unwrap();
        transcript(
            home.path(),
            runner.path(),
            "Done. Should I also update the docs?",
        );
        let only_codex = || Reads {
            claude: false,
            codex: true,
        };
        let (tx, rx) = mpsc::sync_channel(1);
        let (out_tx, out_rx) = mpsc::channel();
        let dir = runner.path().to_path_buf();
        let handle = std::thread::spawn(move || {
            questioner(rx, dir, None, only_codex, move |found, reads| {
                out_tx.send((found, reads)).is_ok()
            })
        });
        tx.send(vec![job(home.path(), runner.path(), true)])
            .unwrap();
        let (found, reads) = out_rx.recv().unwrap();
        assert!(found.is_empty());
        assert_eq!(reads, only_codex());
        drop(tx);
        handle.join().unwrap();
    }
}
