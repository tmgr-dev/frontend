use super::attach_consent::Reads;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use tmgr_ipc::api::SessionInfo;
use tmgr_runner::attach::{
    locate, locate_unsandboxed_claude, AgentTitle, Format, Source, TitleScan,
};
use tmgr_runner::resume::id_from_log;
use tmgr_runner::sanitize::sanitize_text;

const MAX_SCANS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AgentSrc {
    pub profile: String,
    pub format: Format,
    pub cwd: String,
    pub pid: u32,
    pub default_name: String,
    pub no_sandbox: bool,
    pub created_ms: u64,
}

pub(super) fn agent_src(s: &SessionInfo) -> Option<AgentSrc> {
    let profile = s.profile.as_deref()?;
    Some(AgentSrc {
        profile: profile.to_string(),
        format: Format::of(profile)?,
        cwd: s.cwd.clone(),
        pid: s.child_pid,
        default_name: default_name(s.argv.first().map_or("", String::as_str)),
        no_sandbox: s.no_sandbox,
        created_ms: s.created_ms,
    })
}

pub(super) fn title_real_home(src: &AgentSrc, home: Option<PathBuf>) -> Option<PathBuf> {
    home.filter(|_| src.no_sandbox && src.format == Format::Claude)
}

pub(super) fn default_name(argv0: &str) -> String {
    let base = Path::new(argv0)
        .file_name()
        .map_or_else(|| argv0.to_string(), |n| n.to_string_lossy().into_owned());
    let name: String = sanitize_text(&base).chars().take(64).collect();
    if name.is_empty() {
        "session".into()
    } else {
        name
    }
}

pub(super) struct TitleJob {
    pub id: String,
    pub src: AgentSrc,
    pub pid: u32,
    pub sandbox_home: PathBuf,
    pub log: PathBuf,
    pub real_home: Option<PathBuf>,
}

pub(super) fn titler(
    jobs: Receiver<Vec<TitleJob>>,
    consent: impl Fn() -> Reads,
    mut send: impl FnMut(Vec<(String, Option<AgentTitle>)>) -> bool,
) {
    let mut scans: HashMap<String, TitleScan> = HashMap::new();
    for batch in jobs {
        if scans.len() > MAX_SCANS {
            let keep: HashSet<&str> = batch.iter().map(|j| j.id.as_str()).collect();
            scans.retain(|id, _| keep.contains(id.as_str()));
        }
        let found = batch
            .iter()
            .map(|job| (job.id.clone(), title_for(job, &mut scans, consent())))
            .collect();
        if !send(found) {
            return;
        }
    }
}

fn title_for(
    job: &TitleJob,
    scans: &mut HashMap<String, TitleScan>,
    reads: Reads,
) -> Option<AgentTitle> {
    if !reads.allows(job.src.format) {
        scans.remove(&job.id);
        return None;
    }
    if let Some(home) = job.real_home.as_deref() {
        let Some((path, id)) =
            locate_unsandboxed_claude(home, &job.src.cwd, job.pid, job.src.created_ms)
        else {
            return retained_title(job, scans);
        };
        return scan_title(job, home, path, Some(id), scans);
    }
    if job.src.no_sandbox {
        return None;
    }
    let home = job.sandbox_home.as_path();
    if !std::fs::symlink_metadata(home).is_ok_and(|m| m.is_dir()) {
        scans.remove(&job.id);
        return None;
    }
    let exit_id = (job.pid == 0)
        .then(|| id_from_log(&job.log, &job.src.profile))
        .flatten();
    let mut source = Source {
        home,
        format: job.src.format,
        cwd: &job.src.cwd,
        pid: job.pid,
        log_id: exit_id.as_deref(),
    };
    let mut found = locate(&source);
    let log_id;
    if found.is_none() && exit_id.is_none() {
        log_id = id_from_log(&job.log, &job.src.profile);
        source.log_id = log_id.as_deref();
        found = locate(&source);
    }
    let Some((path, id)) = found else {
        return retained_title(job, scans);
    };
    scan_title(job, home, path, id, scans)
}

fn retained_title(job: &TitleJob, scans: &mut HashMap<String, TitleScan>) -> Option<AgentTitle> {
    scans.get_mut(&job.id).and_then(|scan| {
        scan.poll();
        scan.title(&job.src.default_name)
    })
}

fn scan_title(
    job: &TitleJob,
    home: &Path,
    path: PathBuf,
    id: Option<String>,
    scans: &mut HashMap<String, TitleScan>,
) -> Option<AgentTitle> {
    let scan = scans.entry(job.id.clone()).or_insert_with(|| {
        TitleScan::new(job.src.format, home.to_path_buf(), path.clone(), id.clone())
    });
    if !scan.is_for(&path, id.as_deref()) {
        *scan = TitleScan::new(job.src.format, home.to_path_buf(), path, id);
    }
    scan.poll();
    scan.title(&job.src.default_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_name_matches_the_runner() {
        assert_eq!(default_name("/home/u/.local/bin/claude"), "claude");
        assert_eq!(default_name("codex"), "codex");
        assert_eq!(default_name(""), "session");
        assert_eq!(default_name(&"a".repeat(100)).len(), 64);
    }

    fn job(dir: &Path, format: Format, profile: &str, cwd: &str) -> TitleJob {
        TitleJob {
            id: "s1".into(),
            src: AgentSrc {
                profile: profile.into(),
                format,
                cwd: cwd.into(),
                pid: 4242,
                default_name: "claude".into(),
                no_sandbox: false,
                checkpoints: None,
                created_ms: 0,
            },
            pid: 4242,
            sandbox_home: dir.join("homes").join("s1"),
            log: dir.join("output.log"),
            real_home: None,
        }
    }

    fn unsandboxed_job(dir: &Path, format: Format, real: &Path) -> TitleJob {
        let profile = if format == Format::Claude {
            "claude-code"
        } else {
            "codex"
        };
        let mut j = job(dir, format, profile, "/work/app");
        j.src.no_sandbox = true;
        j.real_home = title_real_home(&j.src, Some(real.to_path_buf()));
        j
    }

    fn plant(real: &Path, project: &str, pid: u32, pid_file_id: &str, body: &str) -> PathBuf {
        let sessions = real.join(".claude/sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(
            sessions.join(format!("{pid}.json")),
            format!("{{\"pid\":{pid},\"sessionId\":\"{pid_file_id}\"}}"),
        )
        .unwrap();
        let proj = real.join(".claude/projects").join(project);
        std::fs::create_dir_all(&proj).unwrap();
        let file = proj.join(format!("{U}.jsonl"));
        std::fs::write(&file, body).unwrap();
        file
    }

    const ALL: Reads = Reads {
        claude: true,
        codex: true,
    };

    const AUTO: &str = "{\"type\":\"ai-title\",\"aiTitle\":\"Auto\"}\n";

    const U: &str = "2f0e1c3a-5b7d-4e8f-9a1b-3c4d5e6f7a8b";

    #[test]
    fn titler_follows_a_sandbox_session_from_pid_file_to_exit_log() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("homes/s1");
        let proj = home.join(".claude/projects/-work-app");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::create_dir_all(home.join(".claude/sessions")).unwrap();
        std::fs::write(
            home.join(".claude/sessions/4242.json"),
            format!("{{\"pid\":4242,\"sessionId\":\"{U}\"}}"),
        )
        .unwrap();
        let file = proj.join(format!("{U}.jsonl"));
        std::fs::write(&file, "{\"type\":\"ai-title\",\"aiTitle\":\"Auto\"}\n").unwrap();
        let j = job(dir.path(), Format::Claude, "claude-code", "/work/app");
        let mut scans = HashMap::new();
        let t = title_for(&j, &mut scans, ALL).unwrap();
        assert_eq!(
            (t.text.as_str(), t.auto, t.base.as_str()),
            ("Auto", true, "claude")
        );
        std::fs::write(
            &file,
            "{\"type\":\"ai-title\",\"aiTitle\":\"Auto\"}\n{\"type\":\"custom-title\",\"customTitle\":\"Mine\"}\n",
        )
        .unwrap();
        let t = title_for(&j, &mut scans, ALL).unwrap();
        assert_eq!((t.text.as_str(), t.auto), ("Mine", false));
        std::fs::remove_file(home.join(".claude/sessions/4242.json")).unwrap();
        std::fs::write(
            dir.path().join("output.log"),
            format!("bye\r\nclaude --resume {U}\r\n"),
        )
        .unwrap();
        std::fs::write(
            proj.join("zzzzzzzz-5b7d-4e8f-9a1b-3c4d5e6f7a8b.jsonl"),
            "{\"type\":\"custom-title\",\"customTitle\":\"Newer decoy\"}\n",
        )
        .unwrap();
        let mut gone = job(dir.path(), Format::Claude, "claude-code", "/work/app");
        gone.pid = 0;
        let t = title_for(&gone, &mut scans, ALL).unwrap();
        assert_eq!(t.text, "Mine");
    }

    #[test]
    fn titler_reads_codex_from_the_sandbox_home_and_never_guesses_in_the_real_home() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("homes/s1/.codex");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            home.join("session_index.jsonl"),
            "{\"id\":\"a\",\"thread_name\":\"older\"}\n{\"id\":\"b\",\"thread_name\":\"Review the PR\"}\n",
        )
        .unwrap();
        let j = job(dir.path(), Format::Codex, "codex", "/w");
        let mut scans = HashMap::new();
        assert_eq!(
            title_for(&j, &mut scans, ALL).unwrap().text,
            "Review the PR"
        );
    }

    #[test]
    fn a_missing_or_symlinked_sandbox_home_gives_no_title() {
        let dir = tempfile::tempdir().unwrap();
        let mut scans = HashMap::new();
        let j = job(dir.path(), Format::Codex, "codex", "/w");
        assert!(title_for(&j, &mut scans, ALL).is_none());
        let real = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(real.path().join(".codex")).unwrap();
        std::fs::write(
            real.path().join(".codex/session_index.jsonl"),
            "{\"id\":\"a\",\"thread_name\":\"someone else\"}\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("homes")).unwrap();
        std::os::unix::fs::symlink(real.path(), &j.sandbox_home).unwrap();
        assert!(title_for(&j, &mut scans, ALL).is_none());
    }

    #[test]
    fn an_unsandboxed_claude_session_is_titled_from_the_real_home() {
        let dir = tempfile::tempdir().unwrap();
        let real = tempfile::tempdir().unwrap();
        let file = plant(real.path(), "-work-app", 4242, U, AUTO);
        let j = unsandboxed_job(dir.path(), Format::Claude, real.path());
        let mut scans = HashMap::new();
        let t = title_for(&j, &mut scans, ALL).unwrap();
        assert_eq!(
            (t.text.as_str(), t.auto, t.base.as_str()),
            ("Auto", true, "claude")
        );
        std::fs::write(
            &file,
            format!("{AUTO}{{\"type\":\"custom-title\",\"customTitle\":\"Mine\"}}\n"),
        )
        .unwrap();
        let t = title_for(&j, &mut scans, ALL).unwrap();
        assert_eq!((t.text.as_str(), t.auto), ("Mine", false));
    }

    #[test]
    fn an_unsandboxed_title_survives_the_pid_file_going_away_and_ignores_the_log() {
        let dir = tempfile::tempdir().unwrap();
        let real = tempfile::tempdir().unwrap();
        plant(real.path(), "-work-app", 4242, U, AUTO);
        let j = unsandboxed_job(dir.path(), Format::Claude, real.path());
        let mut scans = HashMap::new();
        assert_eq!(title_for(&j, &mut scans, ALL).unwrap().text, "Auto");
        std::fs::remove_file(real.path().join(".claude/sessions/4242.json")).unwrap();
        let mut gone = unsandboxed_job(dir.path(), Format::Claude, real.path());
        gone.pid = 0;
        assert_eq!(title_for(&gone, &mut scans, ALL).unwrap().text, "Auto");
        std::fs::write(
            dir.path().join("output.log"),
            format!("bye\r\nclaude --resume {U}\r\n"),
        )
        .unwrap();
        assert!(title_for(&gone, &mut HashMap::new(), ALL).is_none());
    }

    #[test]
    fn an_unsandboxed_session_without_a_pid_file_gets_no_title() {
        let dir = tempfile::tempdir().unwrap();
        let real = tempfile::tempdir().unwrap();
        let proj = real.path().join(".claude/projects/-work-app");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join(format!("{U}.jsonl")), AUTO).unwrap();
        let j = unsandboxed_job(dir.path(), Format::Claude, real.path());
        assert!(title_for(&j, &mut HashMap::new(), ALL).is_none());
    }

    #[test]
    fn an_unsandboxed_session_only_reads_its_own_working_folders_project() {
        let dir = tempfile::tempdir().unwrap();
        let real = tempfile::tempdir().unwrap();
        plant(real.path(), "-elsewhere", 4242, U, AUTO);
        let j = unsandboxed_job(dir.path(), Format::Claude, real.path());
        assert!(title_for(&j, &mut HashMap::new(), ALL).is_none());
        std::fs::write(
            real.path().join(".claude/sessions/4242.json"),
            format!("{{\"pid\":4242,\"sessionId\":\"{U}\",\"cwd\":\"/elsewhere\"}}"),
        )
        .unwrap();
        assert!(title_for(&j, &mut HashMap::new(), ALL).is_none());
    }

    #[test]
    fn an_unsandboxed_session_never_reads_a_transcript_in_the_sandbox_home() {
        let dir = tempfile::tempdir().unwrap();
        let real = tempfile::tempdir().unwrap();
        let j = unsandboxed_job(dir.path(), Format::Claude, real.path());
        plant(&j.sandbox_home, "-work-app", 4242, U, AUTO);
        assert!(title_for(&j, &mut HashMap::new(), ALL).is_none());
    }

    #[test]
    fn an_unsandboxed_session_without_a_real_home_ignores_its_per_run_home() {
        let dir = tempfile::tempdir().unwrap();
        let mut j = job(dir.path(), Format::Claude, "claude-code", "/work/app");
        plant(&j.sandbox_home, "-work-app", 4242, U, AUTO);
        assert_eq!(
            title_for(&j, &mut HashMap::new(), ALL).unwrap().text,
            "Auto"
        );
        j.src.no_sandbox = true;
        assert!(j.real_home.is_none());
        assert!(title_for(&j, &mut HashMap::new(), ALL).is_none());
    }

    #[test]
    fn a_sandboxed_session_never_reads_the_real_home() {
        let dir = tempfile::tempdir().unwrap();
        let real = tempfile::tempdir().unwrap();
        plant(real.path(), "-work-app", 4242, U, AUTO);
        let mut j = job(dir.path(), Format::Claude, "claude-code", "/work/app");
        j.real_home = title_real_home(&j.src, Some(real.path().to_path_buf()));
        assert!(j.real_home.is_none());
        std::fs::create_dir_all(&j.sandbox_home).unwrap();
        assert!(title_for(&j, &mut HashMap::new(), ALL).is_none());
    }

    #[test]
    fn only_an_unsandboxed_claude_session_with_a_home_gets_a_real_home() {
        let src = |format, no_sandbox| AgentSrc {
            profile: "p".into(),
            format,
            cwd: "/w".into(),
            pid: 1,
            default_name: "p".into(),
            no_sandbox,
            created_ms: 0,
        };
        let home = Some(PathBuf::from("/h"));
        assert_eq!(
            title_real_home(&src(Format::Codex, true), home.clone()),
            None
        );
        assert_eq!(title_real_home(&src(Format::Claude, true), None), None);
        assert_eq!(
            title_real_home(&src(Format::Claude, false), home.clone()),
            None
        );
        assert_eq!(
            title_real_home(&src(Format::Claude, true), home.clone()),
            home
        );
    }

    #[test]
    fn an_unsandboxed_codex_session_never_reads_the_real_home() {
        let dir = tempfile::tempdir().unwrap();
        let real = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(real.path().join(".codex")).unwrap();
        std::fs::write(
            real.path().join(".codex/session_index.jsonl"),
            "{\"id\":\"a\",\"thread_name\":\"someone else\"}\n",
        )
        .unwrap();
        let j = unsandboxed_job(dir.path(), Format::Codex, real.path());
        assert!(j.real_home.is_none());
        assert!(title_for(&j, &mut HashMap::new(), ALL).is_none());
    }

    #[test]
    fn agent_src_carries_no_sandbox_from_the_session() {
        let info = |no_sandbox: bool| -> SessionInfo {
            serde_json::from_value(serde_json::json!({
                "id": "s_abcdefghij", "name": "n", "argv": ["claude"], "cwd": "/w",
                "profile": "claude-code", "cols": 80, "rows": 24, "created_ms": 1,
                "shim_pid": 1, "child_pid": 2, "no_sandbox": no_sandbox,
                "state": {"kind": "working", "since_ms": 1}
            }))
            .unwrap()
        };
        assert!(agent_src(&info(true)).unwrap().no_sandbox);
        assert!(!agent_src(&info(false)).unwrap().no_sandbox);
    }

    #[test]
    fn a_stale_pid_file_from_before_the_session_gives_no_title() {
        let dir = tempfile::tempdir().unwrap();
        let real = tempfile::tempdir().unwrap();
        let mut j = unsandboxed_job(dir.path(), Format::Claude, real.path());
        plant(real.path(), "-work-app", 4242, U, AUTO);
        assert!(title_for(&j, &mut HashMap::new(), ALL).is_some());
        j.src.created_ms = now_ms() + 3_600_000;
        assert!(title_for(&j, &mut HashMap::new(), ALL).is_none());
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    #[test]
    fn titler_batches_are_sent_and_stop_when_the_receiver_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(vec![job(dir.path(), Format::Claude, "claude-code", "/w")])
            .unwrap();
        drop(tx);
        let mut got = Vec::new();
        titler(
            rx,
            || ALL,
            |found| {
                got.push(found);
                true
            },
        );
        assert_eq!(got.len(), 1);
        assert_eq!(got[0][0].0, "s1");
        assert!(got[0][0].1.is_none());
    }

    const NONE: Reads = Reads {
        claude: false,
        codex: false,
    };

    fn claude_only() -> Reads {
        Reads {
            claude: true,
            codex: false,
        }
    }

    fn codex_only() -> Reads {
        Reads {
            claude: false,
            codex: true,
        }
    }

    fn codex_job(dir: &Path) -> TitleJob {
        let home = dir.join("homes/s1/.codex");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            home.join("session_index.jsonl"),
            "{\"id\":\"b\",\"thread_name\":\"Review the PR\"}\n",
        )
        .unwrap();
        job(dir, Format::Codex, "codex", "/w")
    }

    #[test]
    fn without_consent_a_sandbox_session_gets_no_title() {
        let dir = tempfile::tempdir().unwrap();
        let j = job(dir.path(), Format::Claude, "claude-code", "/work/app");
        plant(&j.sandbox_home, "-work-app", 4242, U, AUTO);
        assert!(title_for(&j, &mut HashMap::new(), NONE).is_none());
        assert!(title_for(&j, &mut HashMap::new(), codex_only()).is_none());
        assert_eq!(
            title_for(&j, &mut HashMap::new(), claude_only())
                .unwrap()
                .text,
            "Auto"
        );
    }

    #[test]
    fn without_consent_an_unsandboxed_session_never_touches_the_real_home() {
        let dir = tempfile::tempdir().unwrap();
        let real = tempfile::tempdir().unwrap();
        let file = plant(real.path(), "-work-app", 4242, U, AUTO);
        let j = unsandboxed_job(dir.path(), Format::Claude, real.path());
        assert!(j.real_home.is_some());
        let mut scans = HashMap::new();
        assert!(title_for(&j, &mut scans, NONE).is_none());
        assert!(title_for(&j, &mut scans, codex_only()).is_none());
        assert!(scans.is_empty());
        std::fs::remove_file(&file).unwrap();
        assert!(title_for(&j, &mut scans, claude_only()).is_none());
    }

    #[test]
    fn consent_is_per_tool() {
        let dir = tempfile::tempdir().unwrap();
        let j = codex_job(dir.path());
        assert!(title_for(&j, &mut HashMap::new(), claude_only()).is_none());
        assert_eq!(
            title_for(&j, &mut HashMap::new(), codex_only())
                .unwrap()
                .text,
            "Review the PR"
        );
        let c = job(dir.path(), Format::Claude, "claude-code", "/work/app");
        plant(&c.sandbox_home, "-work-app", 4242, U, AUTO);
        assert!(title_for(&c, &mut HashMap::new(), codex_only()).is_none());
    }

    #[test]
    fn withdrawing_consent_clears_the_title_drops_the_scan_and_ignores_later_changes() {
        let dir = tempfile::tempdir().unwrap();
        let j = job(dir.path(), Format::Claude, "claude-code", "/work/app");
        let file = plant(&j.sandbox_home, "-work-app", 4242, U, AUTO);
        let mut scans = HashMap::new();
        assert_eq!(title_for(&j, &mut scans, ALL).unwrap().text, "Auto");
        assert_eq!(scans.len(), 1);
        assert!(title_for(&j, &mut scans, NONE).is_none());
        assert!(scans.is_empty());
        std::fs::write(
            &file,
            format!("{AUTO}{{\"type\":\"custom-title\",\"customTitle\":\"Mine\"}}\n"),
        )
        .unwrap();
        assert!(title_for(&j, &mut scans, NONE).is_none());
        assert!(scans.is_empty());
    }

    #[test]
    fn the_titler_reads_consent_fresh_for_every_batch() {
        use std::cell::Cell;
        let dir = tempfile::tempdir().unwrap();
        let j = || job(dir.path(), Format::Claude, "claude-code", "/work/app");
        plant(&j().sandbox_home, "-work-app", 4242, U, AUTO);
        let (tx, rx) = std::sync::mpsc::channel();
        for _ in 0..3 {
            tx.send(vec![j()]).unwrap();
        }
        drop(tx);
        let state = Cell::new(ALL);
        let mut titles = Vec::new();
        titler(
            rx,
            || state.get(),
            |found| {
                titles.push(found[0].1.as_ref().map(|t| t.text.clone()));
                state.set(if titles.len() == 1 { NONE } else { ALL });
                true
            },
        );
        assert_eq!(
            titles,
            [Some("Auto".to_string()), None, Some("Auto".to_string())]
        );
    }

    #[test]
    fn a_denial_during_a_batch_stops_the_next_job_from_being_read() {
        use std::cell::Cell;
        let dir = tempfile::tempdir().unwrap();
        let first = job(dir.path(), Format::Claude, "claude-code", "/work/app");
        plant(&first.sandbox_home, "-work-app", 4242, U, AUTO);
        let mut second = job(dir.path(), Format::Claude, "claude-code", "/work/app");
        second.id = "s2".into();
        second.sandbox_home = dir.path().join("homes").join("s2");
        plant(&second.sandbox_home, "-work-app", 4242, U, AUTO);
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(vec![first, second]).unwrap();
        drop(tx);
        let calls = Cell::new(0);
        let mut got = Vec::new();
        titler(
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
                got = found;
                true
            },
        );
        assert_eq!(calls.get(), 2);
        assert_eq!(got[0].1.as_ref().map(|t| t.text.as_str()), Some("Auto"));
        assert!(got[1].1.is_none());
    }
}
