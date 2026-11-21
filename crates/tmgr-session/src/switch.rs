use crate::git;
use crate::handoff::{build, build_screen, Opts as NoteOpts};
use crate::locate::{export_counted, find, load, newest, Roots};
use crate::{Agent, Session};
use std::path::{Path, PathBuf};
use tmgr_ipc::api::SessionInfo;
use tmgr_usage::paths::Env;

const MAX_CHAIN: usize = 5;
const UNSUPPORTED: &str = "only claude-code, codex, opencode and agy sessions can be switched";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchError {
    Usage(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
}

impl std::fmt::Display for SwitchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (SwitchError::Usage(m)
        | SwitchError::NotFound(m)
        | SwitchError::Conflict(m)
        | SwitchError::Internal(m)) = self;
        f.write_str(m)
    }
}

impl std::error::Error for SwitchError {}

type Result<T> = std::result::Result<T, SwitchError>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    ClaudeCode,
    Codex,
    OpenCode,
    Agy,
}

impl Target {
    pub fn profile(self) -> &'static str {
        match self {
            Target::ClaudeCode => "claude-code",
            Target::Codex => "codex",
            Target::OpenCode => "opencode",
            Target::Agy => "agy",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Target::ClaudeCode => "claude",
            other => other.profile(),
        }
    }

    pub fn agent(self) -> Option<Agent> {
        match self {
            Target::ClaudeCode => Some(Agent::Claude),
            Target::Codex => Some(Agent::Codex),
            Target::OpenCode | Target::Agy => None,
        }
    }

    pub fn rules(self) -> &'static [&'static str] {
        match self {
            Target::ClaudeCode => &["CLAUDE.md"],
            Target::Codex | Target::OpenCode => &["AGENTS.md"],
            Target::Agy => &["AGENTS.md", "GEMINI.md"],
        }
    }

    pub fn of_profile(profile: Option<&str>) -> Option<Target> {
        match profile? {
            "claude-code" => Some(Target::ClaudeCode),
            "codex" => Some(Target::Codex),
            "opencode" => Some(Target::OpenCode),
            "agy" => Some(Target::Agy),
            _ => None,
        }
    }
}

pub fn target_for(profile: Option<&str>, to: Option<Target>) -> Result<Target> {
    let from = Target::of_profile(profile).ok_or_else(|| SwitchError::Usage(UNSUPPORTED.into()))?;
    match (to, from) {
        (Some(t), _) if t == from => Err(SwitchError::Usage(format!(
            "the session already runs {}",
            from.profile()
        ))),
        (Some(t), _) => Ok(t),
        (None, Target::ClaudeCode) => Ok(Target::Codex),
        (None, Target::Codex) => Ok(Target::ClaudeCode),
        (None, _) => Err(SwitchError::Usage(format!(
            "pass --to to say which agent to continue in; a {} session has no conversation log to pick a default from",
            from.profile()
        ))),
    }
}

pub fn refusal(s: &SessionInfo) -> Option<&'static str> {
    if s.task.is_some_and(|t| t.workspace_id >= 0) {
        return Some("the switch works for tasks of local workspaces only");
    }
    if s.origin.is_some() {
        return Some("a session started from a link or the queue cannot be switched");
    }
    if s.managed {
        return Some("a session started by the queue or as part of a batch cannot be switched");
    }
    if s.team.is_some() {
        return Some("a team member cannot be switched");
    }
    None
}

pub fn ancestors<'a>(sessions: &'a [SessionInfo], from: &'a SessionInfo) -> Vec<&'a SessionInfo> {
    let mut out: Vec<&SessionInfo> = Vec::new();
    let mut next = from.handoff_from.as_deref();
    while let Some(id) = next {
        if out.len() >= MAX_CHAIN {
            break;
        }
        let Some(s) = sessions.iter().find(|s| s.id == id) else {
            break;
        };
        if out.iter().any(|o| o.id == s.id) {
            break;
        }
        out.push(s);
        next = s.handoff_from.as_deref();
    }
    out.reverse();
    out
}

fn is_uuid(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => matches!(b, b'0'..=b'9' | b'a'..=b'f'),
        })
}

#[cfg(unix)]
fn homes_dir() -> std::io::Result<PathBuf> {
    tmgr_ipc::dirs::homes_dir()
}

#[cfg(not(unix))]
fn homes_dir() -> std::io::Result<PathBuf> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "the runner is not available on this platform",
    ))
}

fn roots_of(s: &SessionInfo) -> Result<Roots> {
    if s.no_sandbox {
        return Ok(Roots::from_env(&Env::from_process()));
    }
    let home = homes_dir()
        .map_err(|e| SwitchError::Internal(format!("the runner homes folder is unavailable: {e}")))?
        .join(&s.id);
    if !home.is_dir() {
        return Err(SwitchError::NotFound(format!(
            "the folder of session {} is gone, so its conversation cannot be read",
            s.id
        )));
    }
    Ok(Roots::for_home(&home))
}

fn log_of(s: &SessionInfo, agent: Agent) -> Result<PathBuf> {
    let roots = roots_of(s)?;
    if s.no_sandbox {
        let id = s.agent_session_id.as_deref().filter(|i| is_uuid(i));
        return id
            .and_then(|id| find(&roots, id).ok())
            .map(|(_, path)| path)
            .ok_or_else(|| {
                SwitchError::NotFound(format!(
                    "the conversation of session {} cannot be told apart from your other sessions",
                    s.id
                ))
            });
    }
    if let Some(id) = s.agent_session_id.as_deref().filter(|i| is_uuid(i)) {
        if let Ok((_, path)) = find(&roots, id) {
            return Ok(path);
        }
    }
    newest(&roots, agent).ok_or_else(|| {
        SwitchError::NotFound(format!(
            "no conversation file of session {} was found",
            s.id
        ))
    })
}

fn agent_of(s: &SessionInfo) -> Result<Agent> {
    Target::of_profile(s.profile.as_deref())
        .and_then(|t| t.agent())
        .ok_or_else(|| SwitchError::Usage(UNSUPPORTED.into()))
}

pub fn has_log(s: &SessionInfo) -> bool {
    agent_of(s).is_ok()
}

fn lookup_hint(old: &Session) -> String {
    let dir = match old.agent {
        Agent::Claude => "~/.claude/projects/handoff/",
        Agent::Codex => "~/.codex/sessions/handoff/",
    };
    format!(
        "A masked copy of session {id} is under your home in {dir}; `tmgr session` finds it there. If `tmgr` is not available, read that file directly.",
        id = old.id
    )
}

pub fn rules_files(cwd: &Path, target: Target, source: Target) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in target.rules().iter().chain(source.rules()) {
        if cwd.join(f).is_file() && !out.iter().any(|o| o == f) {
            out.push((*f).to_string());
        }
    }
    out
}

#[derive(Debug)]
pub struct Note {
    pub text: String,
    pub exports: Vec<(Agent, PathBuf)>,
    pub task_ref: Option<String>,
}

pub fn check(s: &SessionInfo, to: Option<Target>) -> Result<Target> {
    if let Some(why) = refusal(s) {
        return Err(SwitchError::Usage(why.into()));
    }
    target_for(s.profile.as_deref(), to)
}

pub fn ensure_readable(s: &SessionInfo) -> Result<()> {
    let Ok(agent) = agent_of(s) else {
        return Target::of_profile(s.profile.as_deref())
            .map(drop)
            .ok_or_else(|| SwitchError::Usage(UNSUPPORTED.into()));
    };
    let path = log_of(s, agent)?;
    load(agent, &path).map(drop).map_err(SwitchError::Internal)
}

pub fn prepare_note(
    s: &SessionInfo,
    sessions: &[SessionInfo],
    target: Target,
    task_ref: Option<&str>,
    screen: &[String],
) -> Result<Note> {
    let source_target = Target::of_profile(s.profile.as_deref())
        .ok_or_else(|| SwitchError::Usage(UNSUPPORTED.into()))?;
    let Some(source) = source_target.agent() else {
        let cwd = Path::new(&s.cwd);
        let text = build_screen(
            source_target.name(),
            &s.id,
            screen,
            &NoteOpts {
                target: target.name(),
                task: task_ref.map(str::to_string),
                previous: Vec::new(),
                git: git::collect(cwd),
                rules_files: rules_files(cwd, target, source_target),
                lookup_hint: None,
            },
            &s.cwd,
        );
        return Ok(Note {
            text,
            exports: Vec::new(),
            task_ref: task_ref.map(str::to_string),
        });
    };
    let own_log = log_of(s, source)?;
    let session = load(source, &own_log).map_err(SwitchError::Internal)?;
    let mut previous = Vec::new();
    let mut exports: Vec<(Agent, PathBuf)> = Vec::new();
    for p in ancestors(sessions, s) {
        let Ok(agent) = agent_of(p) else { continue };
        let Ok(path) = log_of(p, agent) else { continue };
        let Ok(prev) = load(agent, &path) else {
            continue;
        };
        previous.push(format!("{} ({})", prev.id, agent.name()));
        exports.push((agent, path));
    }
    exports.push((source, own_log));
    let cwd = Path::new(&s.cwd);
    let text = build(
        &session,
        &NoteOpts {
            target: target.name(),
            task: task_ref.map(str::to_string),
            previous,
            git: git::collect(cwd),
            rules_files: rules_files(cwd, target, source_target),
            lookup_hint: Some(lookup_hint(&session)),
        },
    );
    Ok(Note {
        text,
        exports,
        task_ref: task_ref.map(str::to_string),
    })
}

pub fn export_all(new_session: &str, exports: &[(Agent, PathBuf)]) -> Option<String> {
    let home = homes_dir().ok()?.join(new_session);
    let mut failed: Vec<String> = Vec::new();
    let mut skipped = 0;
    for (agent, path) in exports {
        match export_counted(path, *agent, &home) {
            Ok((_, n)) => skipped += n,
            Err(e) => failed.push(e),
        }
    }
    if !failed.is_empty() {
        return Some(format!(
            "the old conversation could not be copied for lookup ({}); the note still carries the essentials",
            failed.join("; ")
        ));
    }
    (skipped > 0).then(|| {
        format!("{skipped} unreadable lines of the old conversation were left out of the copy")
    })
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
            managed: false,
        }
    }

    #[test]
    fn the_target_is_the_other_agent_unless_named() {
        assert_eq!(
            target_for(Some("claude-code"), None).unwrap(),
            Target::Codex
        );
        assert_eq!(target_for(Some("codex"), None).unwrap(), Target::ClaudeCode);
        assert!(target_for(Some("codex"), Some(Target::Codex)).is_err());
        assert!(target_for(Some("shell"), None).is_err());
        assert!(target_for(None, None).is_err());
    }

    #[test]
    fn opencode_and_agy_are_targets_and_need_an_explicit_choice_as_sources() {
        assert_eq!(Target::of_profile(Some("opencode")), Some(Target::OpenCode));
        assert_eq!(Target::of_profile(Some("agy")), Some(Target::Agy));
        assert_eq!(Target::OpenCode.profile(), "opencode");
        assert_eq!(Target::Agy.profile(), "agy");
        assert_eq!(
            target_for(Some("claude-code"), Some(Target::Agy)).unwrap(),
            Target::Agy
        );
        assert_eq!(
            target_for(Some("codex"), Some(Target::OpenCode)).unwrap(),
            Target::OpenCode
        );
        assert_eq!(
            target_for(Some("agy"), Some(Target::ClaudeCode)).unwrap(),
            Target::ClaudeCode
        );
        assert!(matches!(
            target_for(Some("opencode"), None),
            Err(SwitchError::Usage(_))
        ));
        assert!(matches!(
            target_for(Some("agy"), None),
            Err(SwitchError::Usage(_))
        ));
        assert!(target_for(Some("agy"), Some(Target::Agy)).is_err());
    }

    #[test]
    fn rules_files_follow_the_target() {
        assert_eq!(Target::ClaudeCode.rules(), ["CLAUDE.md"]);
        assert_eq!(Target::Codex.rules(), ["AGENTS.md"]);
        assert_eq!(Target::OpenCode.rules(), ["AGENTS.md"]);
        assert_eq!(Target::Agy.rules(), ["AGENTS.md", "GEMINI.md"]);
        assert!(Target::OpenCode.agent().is_none());
        assert_eq!(Target::Codex.agent(), Some(Agent::Codex));
    }

    #[test]
    fn only_plain_local_sessions_can_switch() {
        assert!(refusal(&info("a", "codex")).is_none());
        let mut s = info("a", "codex");
        s.task = None;
        assert!(refusal(&s).is_none());
        s.managed = true;
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
    fn queue_and_batch_sessions_are_refused() {
        let mut s = info("a", "codex");
        s.managed = true;
        assert!(refusal(&s).unwrap().contains("queue"));
    }

    #[test]
    fn an_unsandboxed_session_without_a_conversation_id_is_refused() {
        let mut s = info("a", "codex");
        s.no_sandbox = true;
        assert!(log_of(&s, Agent::Codex).is_err());
    }

    #[test]
    fn an_unsandboxed_session_whose_conversation_is_gone_never_falls_back_to_the_newest() {
        let mut s = info("a", "codex");
        s.no_sandbox = true;
        s.agent_session_id = Some("99999999-aaaa-bbbb-cccc-000000000009".into());
        assert!(matches!(
            log_of(&s, Agent::Codex),
            Err(SwitchError::NotFound(_))
        ));
    }

    #[test]
    fn the_chain_lists_earlier_sessions_oldest_first_and_stops_at_loops() {
        let a = info("a", "claude-code");
        let mut b = info("b", "codex");
        b.handoff_from = Some("a".into());
        let mut c = info("c", "claude-code");
        c.handoff_from = Some("b".into());
        let all = vec![a, b, c.clone()];
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
