use super::attach_questions::{session_src, QuestionJob};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::Duration;
use tmgr_core::agentwork::{self, Progress, RunView};
use tmgr_core::api::Client;
use tmgr_runner::attach::{
    clamp_summary, clean_reply, compose, digest, plain_summary, prompt_input, scrub, Digest,
    PostTarget, SUMMARY_PROMPT,
};
use tmgr_runner::questions::{haiku, transcript_path};

const TIMEOUT: Duration = Duration::from_secs(60);
const COMMENT_HEAD: &str = "**Session summary** (written by tmgr from the session transcript)";

pub(super) struct SummaryJob {
    pub key: String,
    pub job: QuestionJob,
    pub claude: Option<PathBuf>,
    pub workdir: PathBuf,
}

pub(super) struct Generated {
    pub key: String,
    pub text: String,
    pub plain: bool,
    pub tainted: bool,
}

pub(super) fn generate(job: &SummaryJob, home: &str) -> Generated {
    let key = job.key.clone();
    let failed = |text: &str| Generated {
        key: key.clone(),
        text: text.to_string(),
        plain: true,
        tainted: false,
    };
    let Some(src) = session_src(&job.job) else {
        return failed("The session transcript is not available.");
    };
    let Some(path) = transcript_path(&src) else {
        return failed("The session transcript is not available.");
    };
    let Some(d) = digest(&src.home, &path, &job.job.src.cwd, home) else {
        return failed("The session transcript cannot be read.");
    };
    summarize(&key, &d, job.claude.as_deref(), &job.workdir)
}

pub(super) fn summarize(key: &str, d: &Digest, claude: Option<&Path>, workdir: &Path) -> Generated {
    let plain = || Generated {
        key: key.to_string(),
        text: plain_summary(d),
        plain: true,
        tainted: false,
    };
    let Some(bin) = claude.filter(|_| !d.is_empty()) else {
        return plain();
    };
    match haiku::complete(bin, workdir, SUMMARY_PROMPT, &prompt_input(d), TIMEOUT) {
        Ok(reply) => {
            let (prose, tainted) = clean_reply(&reply);
            if prose.trim().is_empty() {
                return plain();
            }
            Generated {
                key: key.to_string(),
                text: compose(Some(&prose), d),
                plain: false,
                tainted,
            }
        }
        Err(_) => plain(),
    }
}

pub(super) fn summarizer(jobs: Receiver<SummaryJob>, mut send: impl FnMut(Generated) -> bool) {
    let home = std::env::var("HOME").unwrap_or_default();
    for job in jobs {
        if !send(generate(&job, &home)) {
            return;
        }
    }
}

pub(super) trait Poster {
    fn run(&self, task_id: u64, run_id: u64) -> Result<RunView, String>;
    fn set_summary(&self, run_id: u64, text: &str) -> Result<(), String>;
    fn comment(&self, task_id: u64, text: &str) -> Result<(), String>;
}

pub(super) struct ClientPoster<'a>(pub &'a Client);

impl Poster for ClientPoster<'_> {
    fn run(&self, task_id: u64, run_id: u64) -> Result<RunView, String> {
        agentwork::get_run(self.0, task_id, run_id, true).map_err(|e| e.to_string())
    }

    fn set_summary(&self, run_id: u64, text: &str) -> Result<(), String> {
        let progress = Progress {
            summary: Some(text.to_string()),
            ..Progress::default()
        };
        agentwork::update(self.0, run_id, &progress, true)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn comment(&self, task_id: u64, text: &str) -> Result<(), String> {
        self.0.add_comment(task_id, text).map_err(|e| e.to_string())
    }
}

pub(super) fn post(p: &dyn Poster, t: &PostTarget, text: &str) -> Result<String, String> {
    let text = clamp_summary(&scrub(text));
    if text.trim().is_empty() {
        return Err("the summary is empty".into());
    }
    let run = p
        .run(t.task_id, t.run_id)
        .map_err(|e| format!("cannot check the agent work run: {e}"))?;
    if run.task_id != Some(t.task_id) {
        return Err("the run does not belong to this task".into());
    }
    let running = run.status == "running";
    if running {
        p.set_summary(t.run_id, &text)
            .map_err(|e| format!("cannot update the agent work: {e}"))?;
    }
    p.comment(t.task_id, &format!("{COMMENT_HEAD}\n\n{text}"))
        .map_err(|e| {
            if running {
                format!("agent work updated, comment failed: {e}")
            } else {
                format!("cannot add the comment: {e}")
            }
        })?;
    Ok(if running {
        format!("summary posted to {}", t.task_key)
    } else {
        format!(
            "summary posted to {} as a comment (the run is finished)",
            t.task_key
        )
    })
}

pub(super) fn post_real(t: &PostTarget, text: &str) -> Result<String, String> {
    use clap::Parser;
    let cli = crate::cli::Cli::parse_from(["tmgr"]);
    let ctx = crate::ctx::Ctx::new(&cli).map_err(|e| e.to_string())?;
    let client = ctx
        .client_for_run(t.workspace.as_deref())
        .map_err(|e| e.to_string())?;
    post(&ClientPoster(&client), t, text)
}

fn marker_name(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(120)
        .collect()
}

pub(super) fn claim(dir: &Path, key: &str) -> bool {
    use std::os::unix::fs::OpenOptionsExt;
    if tmgr_core::config::ensure_private_dir(dir).is_err() {
        return false;
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join(marker_name(key)))
        .is_ok()
}

pub(super) fn unclaim(dir: &Path, key: &str) {
    let _ = std::fs::remove_file(dir.join(marker_name(key)));
}

pub(super) fn print_summary(
    o: &super::Opts,
    key: &str,
    plain: bool,
) -> tmgr_core::error::Result<i32> {
    use super::attach_title::agent_src;
    use tmgr_core::error::Error;
    use tmgr_ipc::api::StateKind;
    let cfg = super::runner_config()?;
    let sessions = super::connect(&cfg)?.list().map_err(super::rerr)?;
    let info = super::ops::resolve(&sessions, key)?;
    let src = agent_src(info)
        .filter(|s| s.format == tmgr_runner::attach::Format::Claude)
        .ok_or_else(|| Error::usage("summaries are available for Claude sessions"))?;
    let job = SummaryJob {
        key: info.id.clone(),
        job: QuestionJob {
            id: info.id.clone(),
            src,
            pid: info.child_pid,
            sandbox_home: cfg.homes.join(&info.id),
            log: cfg.dir.join("sessions").join(&info.id).join("output.log"),
            exited: info.state.kind == StateKind::Exited,
            settled: true,
        },
        claude: None,
        workdir: PathBuf::new(),
    };
    let workdir = cfg.dir.join("summary-cwd");
    let job = SummaryJob {
        claude: super::attach_questions::claude_binary()
            .filter(|_| !plain && tmgr_ipc::dirs::ensure_private_dir(&workdir).is_ok()),
        workdir,
        ..job
    };
    let done = generate(&job, &std::env::var("HOME").unwrap_or_default());
    let human = done.text.clone();
    Ok(super::emit(
        o,
        serde_json::json!({ "summary": done.text, "plain": done.plain }),
        human,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    struct Fake {
        status: String,
        task: Option<u64>,
        fail_run: bool,
        fail_comment: bool,
        calls: RefCell<Vec<String>>,
    }

    impl Fake {
        fn new(status: &str) -> Fake {
            Fake {
                status: status.into(),
                task: Some(7),
                fail_run: false,
                fail_comment: false,
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl Poster for Fake {
        fn run(&self, _: u64, _: u64) -> Result<RunView, String> {
            self.calls.borrow_mut().push("run".into());
            if self.fail_run {
                return Err("nope".into());
            }
            Ok(RunView {
                task_id: self.task,
                status: self.status.clone(),
                ..RunView::default()
            })
        }

        fn set_summary(&self, run_id: u64, text: &str) -> Result<(), String> {
            self.calls
                .borrow_mut()
                .push(format!("summary {run_id} {text}"));
            Ok(())
        }

        fn comment(&self, task_id: u64, text: &str) -> Result<(), String> {
            self.calls
                .borrow_mut()
                .push(format!("comment {task_id} {text}"));
            if self.fail_comment {
                Err("down".into())
            } else {
                Ok(())
            }
        }
    }

    fn target() -> PostTarget {
        PostTarget {
            task_id: 7,
            run_id: 9,
            workspace: None,
            task_key: "TM-7".into(),
        }
    }

    #[test]
    fn a_running_run_gets_the_summary_and_one_marked_comment() {
        let f = Fake::new("running");
        let msg = post(&f, &target(), "- did it").unwrap();
        let calls = f.calls.borrow();
        assert_eq!(calls[0], "run");
        assert_eq!(calls[1], "summary 9 - did it");
        assert!(calls[2].starts_with("comment 7 **Session summary** (written by tmgr"));
        assert!(calls[2].ends_with("- did it"));
        assert_eq!(calls.len(), 3);
        assert_eq!(msg, "summary posted to TM-7");
    }

    #[test]
    fn a_finished_run_is_not_patched_only_commented() {
        let f = Fake::new("succeeded");
        let msg = post(&f, &target(), "- did it").unwrap();
        let calls = f.calls.borrow();
        assert_eq!(calls.len(), 2);
        assert!(calls[1].starts_with("comment 7"));
        assert!(msg.contains("finished"));
    }

    #[test]
    fn nothing_is_written_when_the_run_cannot_be_checked_or_is_another_tasks() {
        let mut f = Fake::new("running");
        f.fail_run = true;
        assert!(post(&f, &target(), "x").is_err());
        assert_eq!(f.calls.borrow().len(), 1);
        let mut g = Fake::new("running");
        g.task = Some(8);
        assert!(post(&g, &target(), "x").is_err());
        assert_eq!(g.calls.borrow().len(), 1);
    }

    #[test]
    fn a_failed_comment_is_reported() {
        let mut f = Fake::new("running");
        f.fail_comment = true;
        assert!(post(&f, &target(), "x")
            .unwrap_err()
            .contains("comment failed"));
    }

    #[test]
    fn secrets_are_masked_again_at_post_time() {
        let f = Fake::new("running");
        post(&f, &target(), "- used api_key=\"abcdef123456789\" here").unwrap();
        assert!(f
            .calls
            .borrow()
            .iter()
            .all(|c| !c.contains("abcdef123456789")));
    }

    #[test]
    fn an_empty_summary_is_refused_before_any_call() {
        let f = Fake::new("running");
        assert!(post(&f, &target(), "  \n ").is_err());
        assert!(f.calls.borrow().is_empty());
    }

    #[test]
    fn a_claim_is_taken_once_and_can_be_released() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("summaries");
        assert!(claim(&dir, "9-abc-1"));
        assert!(!claim(&dir, "9-abc-1"));
        unclaim(&dir, "9-abc-1");
        assert!(claim(&dir, "9-abc-1"));
    }

    fn fake_claude(dir: &Path, body: &str) -> PathBuf {
        let src = dir.join("claude.src");
        std::fs::write(&src, format!("#!/bin/sh\n{body}\n")).unwrap();
        let bin = dir.join("claude");
        assert!(Command::new("cp")
            .arg(&src)
            .arg(&bin)
            .status()
            .unwrap()
            .success());
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    fn digest_with_work() -> Digest {
        Digest {
            prompts: vec!["fix the login bug".into()],
            replies: vec!["done".into()],
            files: vec!["src/login.rs".into()],
            commits: vec!["fix login".into()],
            prs: vec!["https://github.com/o/r/pull/3".into()],
        }
    }

    #[test]
    fn haiku_prose_and_the_facts_are_joined() {
        let d = tempfile::tempdir().unwrap();
        let bin = fake_claude(
            d.path(),
            "cat >/dev/null; printf -- '- fixed login\\n- tests left'",
        );
        let g = summarize("k", &digest_with_work(), Some(&bin), d.path());
        assert!(!g.plain && !g.tainted);
        assert!(g
            .text
            .starts_with("- fixed login\n- tests left\nFiles changed (1): src/login.rs"));
        assert!(g.text.contains("PR: https://github.com/o/r/pull/3"));
    }

    #[test]
    fn the_transcript_reaches_claude_on_stdin_with_fixed_flags() {
        let d = tempfile::tempdir().unwrap();
        let out = d.path().join("seen");
        let bin = fake_claude(
            d.path(),
            &format!(
                "cat > {o}.in; echo \"$@\" > {o}.args; echo '- ok'",
                o = out.display()
            ),
        );
        summarize("k", &digest_with_work(), Some(&bin), d.path());
        let stdin = std::fs::read_to_string(format!("{}.in", out.display())).unwrap();
        assert!(stdin.starts_with("BEGIN SESSION"));
        assert!(stdin.contains("fix the login bug") && stdin.trim_end().ends_with("END SESSION"));
        let args = std::fs::read_to_string(format!("{}.args", out.display())).unwrap();
        assert!(
            args.contains("--model haiku")
                && args.contains("--safe-mode")
                && args.contains("--tools")
        );
    }

    #[test]
    fn a_reply_with_links_is_cleaned_and_flagged() {
        let d = tempfile::tempdir().unwrap();
        let bin = fake_claude(
            d.path(),
            "cat >/dev/null; echo '- see [x](https://evil.example/?d=1) @bob'",
        );
        let g = summarize("k", &digest_with_work(), Some(&bin), d.path());
        assert!(g.tainted);
        assert!(!g.text.contains("evil.example") && !g.text.contains('@'));
    }

    #[test]
    fn without_claude_or_on_failure_a_plain_list_is_used() {
        let d = tempfile::tempdir().unwrap();
        let g = summarize("k", &digest_with_work(), None, d.path());
        assert!(g.plain);
        assert!(g.text.starts_with("Files changed (1): src/login.rs"));
        let bin = fake_claude(d.path(), "cat >/dev/null; exit 3");
        let g = summarize("k", &digest_with_work(), Some(&bin), d.path());
        assert!(g.plain);
    }

    #[test]
    fn an_empty_digest_never_calls_claude() {
        let d = tempfile::tempdir().unwrap();
        let marker = d.path().join("ran");
        let bin = fake_claude(d.path(), &format!("touch {}", marker.display()));
        let g = summarize("k", &Digest::default(), Some(&bin), d.path());
        assert!(g.plain && !marker.exists());
    }
}
