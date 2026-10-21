use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use tmgr_ipc::api::SessionInfo;
use tmgr_runner::attach::{locate, AgentTitle, Format, Source, TitleScan};
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
}

pub(super) fn agent_src(s: &SessionInfo) -> Option<AgentSrc> {
    let profile = s.profile.as_deref()?;
    Some(AgentSrc {
        profile: profile.to_string(),
        format: Format::of(profile)?,
        cwd: s.cwd.clone(),
        pid: s.child_pid,
        default_name: default_name(s.argv.first().map_or("", String::as_str)),
    })
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
}

pub(super) fn titler(
    jobs: Receiver<Vec<TitleJob>>,
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
            .map(|job| (job.id.clone(), title_for(job, &mut scans)))
            .collect();
        if !send(found) {
            return;
        }
    }
}

fn title_for(job: &TitleJob, scans: &mut HashMap<String, TitleScan>) -> Option<AgentTitle> {
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
        return scans.get_mut(&job.id).and_then(|scan| {
            scan.poll();
            scan.title(&job.src.default_name)
        });
    };
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
            },
            pid: 4242,
            sandbox_home: dir.join("homes").join("s1"),
            log: dir.join("output.log"),
        }
    }

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
        let t = title_for(&j, &mut scans).unwrap();
        assert_eq!(
            (t.text.as_str(), t.auto, t.base.as_str()),
            ("Auto", true, "claude")
        );
        std::fs::write(
            &file,
            "{\"type\":\"ai-title\",\"aiTitle\":\"Auto\"}\n{\"type\":\"custom-title\",\"customTitle\":\"Mine\"}\n",
        )
        .unwrap();
        let t = title_for(&j, &mut scans).unwrap();
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
        let t = title_for(&gone, &mut scans).unwrap();
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
        assert_eq!(title_for(&j, &mut scans).unwrap().text, "Review the PR");
    }

    #[test]
    fn a_missing_or_symlinked_sandbox_home_gives_no_title() {
        let dir = tempfile::tempdir().unwrap();
        let mut scans = HashMap::new();
        let j = job(dir.path(), Format::Codex, "codex", "/w");
        assert!(title_for(&j, &mut scans).is_none());
        let real = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(real.path().join(".codex")).unwrap();
        std::fs::write(
            real.path().join(".codex/session_index.jsonl"),
            "{\"id\":\"a\",\"thread_name\":\"someone else\"}\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("homes")).unwrap();
        std::os::unix::fs::symlink(real.path(), &j.sandbox_home).unwrap();
        assert!(title_for(&j, &mut scans).is_none());
    }

    #[test]
    fn titler_batches_are_sent_and_stop_when_the_receiver_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(vec![job(dir.path(), Format::Claude, "claude-code", "/w")])
            .unwrap();
        drop(tx);
        let mut got = Vec::new();
        titler(rx, |found| {
            got.push(found);
            true
        });
        assert_eq!(got.len(), 1);
        assert_eq!(got[0][0].0, "s1");
        assert!(got[0][0].1.is_none());
    }
}
