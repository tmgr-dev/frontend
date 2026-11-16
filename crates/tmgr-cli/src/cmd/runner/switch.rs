use super::ops::{clean, wait_exit, with_session};
use super::run_task::{execute_switch, prepare, Handoff, Launch, Started};
use super::{connect, emit, rerr, runner_config, Opts, PermissionMode};
use clap::ValueEnum;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::SessionInfo;
use tmgr_ipc::dirs::homes_dir;
use tmgr_session::git;
use tmgr_session::handoff::{build, Opts as NoteOpts};
use tmgr_session::locate::{export_masked, find, load, newest, Roots};
use tmgr_session::{Agent, Session};
use tmgr_usage::paths::Env;

const MAX_CHAIN: usize = 5;

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum, Debug)]
pub enum SwitchTarget {
    #[value(name = "claude-code")]
    ClaudeCode,
    Codex,
}

impl SwitchTarget {
    fn profile(self) -> &'static str {
        match self {
            SwitchTarget::ClaudeCode => "claude-code",
            SwitchTarget::Codex => "codex",
        }
    }

    fn agent(self) -> Agent {
        match self {
            SwitchTarget::ClaudeCode => Agent::Claude,
            SwitchTarget::Codex => Agent::Codex,
        }
    }
}

pub(super) fn current(profile: Option<&str>) -> Option<SwitchTarget> {
    match profile? {
        "claude-code" => Some(SwitchTarget::ClaudeCode),
        "codex" => Some(SwitchTarget::Codex),
        _ => None,
    }
}

pub(super) fn target_for(profile: Option<&str>, to: Option<SwitchTarget>) -> Result<SwitchTarget> {
    let from = current(profile).ok_or_else(|| {
        Error::usage("only claude-code and codex sessions can be switched")
            .hint("the session's agent profile is neither")
    })?;
    let other = match from {
        SwitchTarget::ClaudeCode => SwitchTarget::Codex,
        SwitchTarget::Codex => SwitchTarget::ClaudeCode,
    };
    match to {
        Some(t) if t == from => Err(Error::usage(format!(
            "the session already runs {}",
            from.profile()
        ))),
        Some(t) => Ok(t),
        None => Ok(other),
    }
}

pub(super) fn refusal(s: &SessionInfo) -> Option<&'static str> {
    let task = match s.task {
        Some(t) => t,
        None => return Some("only a session started for a task can be switched"),
    };
    if task.workspace_id >= 0 {
        return Some("the switch works for tasks of local workspaces only");
    }
    if s.origin.is_some() {
        return Some("a session started from a link or the queue cannot be switched");
    }
    if s.team.is_some() {
        return Some("a team member cannot be switched");
    }
    None
}

pub(super) fn ancestors<'a>(
    sessions: &'a [SessionInfo],
    from: &'a SessionInfo,
) -> Vec<&'a SessionInfo> {
    let mut out = Vec::new();
    let mut next = from.handoff_from.as_deref();
    while let Some(id) = next {
        if out.len() >= MAX_CHAIN {
            break;
        }
        let Some(s) = sessions.iter().find(|s| s.id == id) else {
            break;
        };
        if out.iter().any(|o: &&SessionInfo| o.id == s.id) {
            break;
        }
        out.push(s);
        next = s.handoff_from.as_deref();
    }
    out.reverse();
    out
}

fn roots_of(s: &SessionInfo) -> Result<Roots> {
    if s.no_sandbox {
        return Ok(Roots::from_env(&Env::from_process()));
    }
    let home = homes_dir()
        .map_err(|e| Error::internal(format!("the runner homes folder is unavailable: {e}")))?
        .join(&s.id);
    if !home.is_dir() {
        return Err(Error::not_found(format!(
            "the folder of session {} is gone, so its conversation cannot be read",
            clean(&s.id)
        )));
    }
    Ok(Roots::for_home(&home))
}

fn log_of(s: &SessionInfo, agent: Agent) -> Result<PathBuf> {
    let roots = roots_of(s)?;
    if let Some(id) = s
        .agent_session_id
        .as_deref()
        .filter(|i| tmgr_runner::resume::valid_resume_id(i))
    {
        if let Ok((_, path)) = find(&roots, id) {
            return Ok(path);
        }
    }
    newest(&roots, agent).ok_or_else(|| {
        Error::not_found(format!(
            "no conversation file of session {} was found",
            clean(&s.id)
        ))
    })
}

fn agent_of(s: &SessionInfo) -> Result<Agent> {
    current(s.profile.as_deref())
        .map(SwitchTarget::agent)
        .ok_or_else(|| Error::usage("only claude-code and codex sessions can be switched"))
}

pub(super) fn lookup_hint(old: &Session) -> String {
    let dir = match old.agent {
        Agent::Claude => "~/.claude/projects/handoff/",
        Agent::Codex => "~/.codex/sessions/handoff/",
    };
    format!(
        "A masked copy of session {id} is under your home in {dir}; `tmgr session` finds it there. If `tmgr` is not available, read that file directly.",
        id = old.id
    )
}

pub(super) fn rules_files(cwd: &Path, target: Agent, source: Agent) -> Vec<String> {
    let mut out: Vec<String> = [target.rules_file(), source.rules_file()]
        .iter()
        .filter(|f| cwd.join(f).is_file())
        .map(|f| (*f).to_string())
        .collect();
    out.dedup();
    out
}

fn task_ref(s: &SessionInfo) -> Result<String> {
    s.task
        .map(|t| format!("#{}", t.task_id))
        .ok_or_else(|| Error::usage("the session has no task"))
}

pub(super) fn switch(o: &Opts, a: &super::SwitchArgs) -> Result<i32> {
    let (mut c, s) = with_session(&a.session)?;
    if let Some(why) = refusal(&s) {
        return Err(Error::usage(why));
    }
    let target = target_for(s.profile.as_deref(), a.to)?;
    let source = agent_of(&s)?;
    let reference = task_ref(&s)?;
    let prepared = prepare(
        o.ws.as_deref(),
        None,
        &reference,
        target.profile(),
        PermissionMode::Ask,
    )?;
    if !o.json && !o.quiet {
        eprintln!("{}", prepared.summary());
    }
    crate::interact::confirm(
        &format!(
            "Stop {} and continue the task in {}?",
            clean(&s.name),
            target.profile()
        ),
        true,
        a.yes,
    )?;
    let sessions = c.list().map_err(rerr)?;
    let chain = ancestors(&sessions, &s);
    let old_log = log_of(&s, source);

    if s.state.kind != tmgr_ipc::api::StateKind::Exited {
        let reason = format!("handed to {}", target.agent().name());
        c.kill_with_reason(&s.id, "int", Some(&reason))
            .map_err(rerr)?;
        if !wait_exit(&mut c, &s.id, Duration::from_secs(10))? {
            c.kill_with_reason(&s.id, "term", Some(&reason))
                .map_err(rerr)?;
            if !wait_exit(&mut c, &s.id, Duration::from_secs(5))? {
                return Err(Error::new(
                    Kind::Conflict,
                    format!("{} is still running", clean(&s.name)),
                )
                .hint(format!("{} halt --kill", super::bin())));
            }
        }
    }

    let old_log = old_log?;
    let session = load(source, &old_log).map_err(Error::internal)?;
    let mut previous = Vec::new();
    let mut exports: Vec<(Agent, PathBuf)> = Vec::new();
    for p in &chain {
        if let Ok(agent) = agent_of(p) {
            if let Ok(path) = log_of(p, agent) {
                if let Ok(prev) = load(agent, &path) {
                    previous.push(format!("{} ({})", prev.id, agent.name()));
                    exports.push((agent, path));
                }
            }
        }
    }
    exports.push((source, old_log));
    let cwd = Path::new(&s.cwd);
    let note = build(
        &session,
        &NoteOpts {
            target: target.agent(),
            task: Some(reference.clone()),
            previous,
            git: git::collect(cwd),
            rules_files: rules_files(cwd, target.agent(), source),
            lookup_hint: Some(lookup_hint(&session)),
        },
    );

    let started = execute_switch(
        prepared,
        &Launch {
            model: None,
            name: None,
            base: None,
            size: super::ops::terminal_size(),
        },
        &Handoff {
            old_id: &s.id,
            old_cwd: &s.cwd,
            note: &note,
        },
    )
    .map_err(|e| {
        Error::new(
            Kind::Conflict,
            format!(
                "the old session stopped, but the new agent did not start: {e}. Resume the old one with `{} resume {}`",
                super::bin(),
                s.id
            ),
        )
    })?;
    let warning = export_all(&started, &exports);
    report(o, &s, target, &started, warning)
}

fn export_all(started: &Started, exports: &[(Agent, PathBuf)]) -> Option<String> {
    let home = homes_dir().ok()?.join(&started.info.id);
    let mut failed = Vec::new();
    for (agent, path) in exports {
        if let Err(e) = export_masked(path, *agent, &home) {
            failed.push(e);
        }
    }
    (!failed.is_empty()).then(|| {
        format!(
            "the old conversation could not be copied for lookup ({}); the note still carries the essentials",
            failed.join("; ")
        )
    })
}

fn report(
    o: &Opts,
    old: &SessionInfo,
    target: SwitchTarget,
    started: &Started,
    warning: Option<String>,
) -> Result<i32> {
    let bin = super::bin();
    if let Some(w) = &warning {
        eprintln!("warning: {w}");
    }
    let human = format!(
        "switched {} to {}: new session {} in {}\nattach with: {bin} attach {}",
        clean(&old.name),
        target.profile(),
        started.info.id,
        clean(&started.cwd),
        started.info.id
    );
    let _ = (connect, runner_config);
    Ok(emit(
        o,
        json!({
            "from": old.id,
            "to": started.info.id,
            "agent": target.profile(),
            "warning": warning,
            "session": started.info,
        }),
        human,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_ipc::api::{Origin, State, StateKind, TaskRef};

    fn info(id: &str, profile: &str) -> SessionInfo {
        SessionInfo {
            id: id.into(),
            name: "n".into(),
            argv: vec![],
            cwd: "/w".into(),
            profile: Some(profile.into()),
            state: State {
                kind: StateKind::Exited,
                since_ms: 0,
                detail: None,
                exit_code: Some(0),
            },
            created_ms: 0,
            cols: 80,
            rows: 24,
            shim_pid: 1,
            child_pid: 2,
            task: Some(TaskRef {
                workspace_id: -3,
                task_id: 7,
            }),
            bundle_sha256: None,
            origin: None,
            permission_mode: None,
            resumed_from: None,
            human_s: None,
            no_sandbox: false,
            checkpoints: None,
            permission_cards: false,
            team: None,
            agent_session_id: None,
            limit: None,
            handoff_from: None,
        }
    }

    #[test]
    fn the_target_is_the_other_agent_unless_named() {
        assert_eq!(
            target_for(Some("claude-code"), None).unwrap(),
            SwitchTarget::Codex
        );
        assert_eq!(
            target_for(Some("codex"), None).unwrap(),
            SwitchTarget::ClaudeCode
        );
        assert!(target_for(Some("codex"), Some(SwitchTarget::Codex)).is_err());
        assert!(target_for(Some("shell"), None).is_err());
        assert!(target_for(None, None).is_err());
    }

    #[test]
    fn only_plain_local_task_sessions_can_switch() {
        assert!(refusal(&info("a", "codex")).is_none());
        let mut s = info("a", "codex");
        s.task = None;
        assert!(refusal(&s).is_some());
        let mut s = info("a", "codex");
        s.task = Some(TaskRef {
            workspace_id: 56,
            task_id: 1,
        });
        assert!(refusal(&s).unwrap().contains("local"));
        let mut s = info("a", "codex");
        s.origin = Some(Origin {
            link: "l".into(),
            generation: 1,
        });
        assert!(refusal(&s).is_some());
    }

    #[test]
    fn the_chain_lists_earlier_sessions_oldest_first_and_stops_at_loops() {
        let a = info("a", "claude-code");
        let mut b = info("b", "codex");
        b.handoff_from = Some("a".into());
        let mut c = info("c", "claude-code");
        c.handoff_from = Some("b".into());
        let all = vec![a.clone(), b.clone(), c.clone()];
        let ids: Vec<&str> = ancestors(&all, &c).iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
        let mut x = info("x", "codex");
        x.handoff_from = Some("y".into());
        let mut y = info("y", "claude-code");
        y.handoff_from = Some("x".into());
        let all = vec![x.clone(), y];
        assert_eq!(ancestors(&all, &x).len(), 2);
    }
}
