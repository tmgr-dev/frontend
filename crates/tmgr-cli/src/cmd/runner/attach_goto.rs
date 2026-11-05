use super::attach_consent::Reads;
use super::attach_questions::QuestionJob;
use super::{connect, rerr};
use std::time::Duration;
use tmgr_ipc::api::{caps, limits, RestoredRun};
use tmgr_runner::attach::{
    find_question, goto_target, locate, Format, GotoMark, ScrollText, Source,
};
use tmgr_runner::daemon::RunnerConfig;
use tmgr_runner::resume::{id_from_log, valid_resume_id};
use tmgr_runner::sanitize::sanitize_text;

const FORK_WAIT: Duration = Duration::from_secs(30);
pub(super) const FORK_PASTE_WAIT: Duration = Duration::from_secs(90);

pub(super) type Found = (usize, usize, GotoMark);

pub(super) fn goto_call(
    cfg: &RunnerConfig,
    session: &str,
    question: &str,
) -> Result<Option<Found>, String> {
    let mut client = connect(cfg).map_err(|e| e.to_string())?;
    let _ = client.set_timeouts(Duration::from_secs(10));
    let text = client
        .text(session, Some(limits::MAX_TEXT_LINES))
        .map_err(|e| sanitize_text(&e.to_string()).into_owned())?;
    let Some(span) = find_question(&text.lines, question) else {
        return Ok(None);
    };
    let (offset, mark) = goto_target(
        text.lines.len(),
        usize::from(text.rows),
        text.scroll_max as usize,
        span,
        session,
    );
    Ok(Some((offset, text.scroll_max as usize, mark)))
}

pub(super) fn text_call(cfg: &RunnerConfig, session: &str) -> Result<ScrollText, String> {
    let mut client = connect(cfg).map_err(|e| e.to_string())?;
    let _ = client.set_timeouts(Duration::from_secs(10));
    let text = client
        .text(session, Some(limits::MAX_TEXT_LINES))
        .map_err(|e| sanitize_text(&e.to_string()).into_owned())?;
    Ok(ScrollText {
        session: session.to_string(),
        lines: text.lines,
        cols: usize::from(text.cols),
        wrapped: text.wrapped,
        scroll_max: text.scroll_max as usize,
        rows: usize::from(text.rows),
    })
}

pub(super) fn fork_name(names: &[&str], base: &str) -> String {
    let root = match base.rsplit_once("-fork-") {
        Some((head, n))
            if !head.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) =>
        {
            head
        }
        _ => base,
    };
    (1..)
        .map(|n| format!("{root}-fork-{n}"))
        .find(|c| !names.contains(&c.as_str()))
        .unwrap_or_else(|| format!("{root}-fork"))
}

pub(super) fn can_fork(profile: &str) -> bool {
    matches!(profile, "claude-code" | "codex")
}

fn codex_rollout_id(home: &std::path::Path, cwd: &str) -> Option<String> {
    let path = tmgr_runner::questions::tail::locate_codex_rollout(home, None, cwd)?;
    let stem = path.file_stem()?.to_str()?;
    let id = stem.get(stem.len().checked_sub(36)?..)?;
    valid_resume_id(id).then(|| id.to_string())
}

pub(super) fn conversation_id(job: &QuestionJob) -> Option<String> {
    let log_id = id_from_log(&job.log, &job.src.profile);
    if job.src.format == Format::Codex {
        return log_id.or_else(|| codex_rollout_id(&job.sandbox_home, &job.src.cwd));
    }
    let (path, id) = locate(&Source {
        home: &job.sandbox_home,
        format: Format::Claude,
        cwd: &job.src.cwd,
        pid: if job.exited { 0 } else { job.pid },
        log_id: log_id.as_deref(),
    })?;
    id.or_else(|| {
        let stem = path.file_stem()?.to_str()?;
        valid_resume_id(stem).then(|| stem.to_string())
    })
}

pub(super) fn fork_call(
    cfg: &RunnerConfig,
    job: &QuestionJob,
    name: &str,
    (cols, rows): (u16, u16),
    reads: Reads,
) -> Result<RestoredRun, String> {
    if let Some(hint) = reads.denial(job.src.format) {
        return Err(hint.to_string());
    }
    let clean = |s: String| sanitize_text(&s).into_owned();
    let agent_id = conversation_id(job)
        .ok_or_else(|| "cannot tell which conversation this session has".to_string())?;
    let mut client = connect(cfg).map_err(|e| clean(e.to_string()))?;
    if !client
        .welcome()
        .caps
        .iter()
        .any(|c| c == caps::SESSIONS_FORK_V1)
    {
        return Err("this runner cannot fork sessions yet, restart it".into());
    }
    let _ = client.set_timeouts(FORK_WAIT);
    client
        .fork(&job.id, &agent_id, Some(name), (cols, rows))
        .map_err(|e| clean(rerr(e).to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fork_names_count_up_and_do_not_nest() {
        assert_eq!(fork_name(&["api"], "api"), "api-fork-1");
        assert_eq!(fork_name(&["api", "api-fork-1"], "api"), "api-fork-2");
        assert_eq!(fork_name(&["api-fork-1"], "api-fork-1"), "api-fork-2");
        assert_eq!(fork_name(&[], "x-fork-"), "x-fork--fork-1");
        assert_eq!(fork_name(&[], "-fork-3"), "-fork-3-fork-1");
    }

    #[test]
    fn a_fork_needs_consent_and_fails_before_the_conversation_is_looked_up() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = RunnerConfig {
            dir: dir.path().join("none"),
            homes: dir.path().join("homes"),
            self_exe: dir.path().join("tmgr"),
            quiet_ms: 0,
            version: "0".into(),
            max_sessions: 1,
        };
        let job = QuestionJob {
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
            sandbox_home: dir.path().join("home"),
            log: dir.path().join("output.log"),
            exited: true,
            settled: true,
        };
        let err = fork_call(&cfg, &job, "x", (80, 24), Reads::default()).unwrap_err();
        assert_eq!(err, Reads::default().hint().unwrap());
        let off = Reads {
            claude: false,
            codex: true,
        };
        let err = fork_call(&cfg, &job, "x", (80, 24), off).unwrap_err();
        assert!(err.starts_with("Claude Code titles"), "{err}");
        let err = fork_call(
            &cfg,
            &job,
            "x",
            (80, 24),
            Reads {
                claude: true,
                codex: false,
            },
        )
        .unwrap_err();
        assert_eq!(err, "cannot tell which conversation this session has");
    }
    const X1: &str = "0199a213-81c0-7800-8aa1-bbab2a035a53";
    const X2: &str = "2f0e1c3a-5b7d-4e8f-9a1b-3c4d5e6f7a8b";

    fn codex_job(dir: &std::path::Path) -> QuestionJob {
        QuestionJob {
            id: "s_abcdefghjk".into(),
            src: super::super::attach_title::AgentSrc {
                profile: "codex".into(),
                format: Format::Codex,
                cwd: "/work/x".into(),
                pid: 0,
                default_name: "codex".into(),
                no_sandbox: false,
                created_ms: 0,
            },
            pid: 0,
            sandbox_home: dir.join("home"),
            log: dir.join("output.log"),
            exited: false,
            settled: true,
        }
    }

    fn rollout(home: &std::path::Path, ts: &str, id: &str, cwd: &str) {
        let day = home.join(".codex/sessions/2026/11/05");
        std::fs::create_dir_all(&day).unwrap();
        let meta = serde_json::json!({"type":"session_meta","payload":{"id":id,"cwd":cwd}});
        std::fs::write(
            day.join(format!("rollout-{ts}-{id}.jsonl")),
            format!("{meta}\n"),
        )
        .unwrap();
    }

    #[test]
    fn only_claude_code_and_codex_sessions_can_be_forked() {
        assert!(can_fork("claude-code") && can_fork("codex"));
        for p in ["", "claude", "Codex", "gemini", "codex "] {
            assert!(!can_fork(p), "{p:?}");
        }
    }

    #[test]
    fn a_codex_conversation_is_the_exit_screen_id_or_the_newest_rollout_of_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let job = codex_job(dir.path());
        assert_eq!(conversation_id(&job), None);
        rollout(&job.sandbox_home, "2026-11-05T09-00-00", X2, "/work/other");
        assert_eq!(conversation_id(&job), None);
        rollout(&job.sandbox_home, "2026-11-05T10-00-00", X1, "/work/x");
        assert_eq!(conversation_id(&job).as_deref(), Some(X1));
        std::fs::write(
            &job.log,
            format!("To continue this session, run codex resume {X2}\r\n"),
        )
        .unwrap();
        assert_eq!(conversation_id(&job).as_deref(), Some(X2));
    }
}
