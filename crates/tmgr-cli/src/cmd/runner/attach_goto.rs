use super::attach_questions::QuestionJob;
use super::{connect, rerr};
use std::time::Duration;
use tmgr_ipc::api::{caps, RestoredRun};
use tmgr_runner::attach::{find_question, goto_target, locate, Format, GotoMark, Source};
use tmgr_runner::daemon::RunnerConfig;
use tmgr_runner::resume::{id_from_log, valid_resume_id};
use tmgr_runner::sanitize::sanitize_text;

const GOTO_LINES: u32 = 20_000;
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
        .text(session, Some(GOTO_LINES))
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

pub(super) fn conversation_id(job: &QuestionJob) -> Option<String> {
    let log_id = id_from_log(&job.log, &job.src.profile);
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
) -> Result<RestoredRun, String> {
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
}
