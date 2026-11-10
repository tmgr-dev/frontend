use super::attach_goto::can_fork;
use super::attach_shell::is_shell_session;
use tmgr_ipc::api::{SessionInfo, StateKind};
use tmgr_runner::attach::{NewRequest, Place, SessionRow, SpawnKind};
use tmgr_runner::resume::valid_resume_id;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Launch {
    kind: Option<SpawnKind>,
    cwd: String,
    bypass: bool,
    no_sandbox: bool,
    permission_cards: bool,
    conversation: bool,
}

impl Launch {
    pub(super) fn of(s: &SessionInfo) -> Launch {
        let kind = if is_shell_session(&s.argv, &s.name) {
            Some(SpawnKind::Shell)
        } else {
            s.profile
                .clone()
                .map(|profile| SpawnKind::Agent { profile })
        };
        Launch {
            kind,
            cwd: s.cwd.clone(),
            bypass: s.permission_mode.as_deref() == Some("bypass"),
            no_sandbox: s.no_sandbox,
            permission_cards: s.permission_cards,
            conversation: s.agent_session_id.as_deref().is_some_and(valid_resume_id),
        }
    }

    pub(super) fn bypass(&self) -> bool {
        self.bypass
    }

    pub(super) fn no_sandbox(&self) -> bool {
        self.no_sandbox
    }

    pub(super) fn flagged(&self) -> bool {
        self.bypass || self.no_sandbox
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ForkStep {
    Native,
    Fresh(Box<NewRequest>),
    Unsupported,
}

pub(super) fn fork_step(launch: &Launch) -> ForkStep {
    let native = matches!(&launch.kind, Some(SpawnKind::Agent { profile }) if can_fork(profile))
        && !launch.flagged()
        && launch.conversation;
    match (&launch.kind, native) {
        (_, true) => ForkStep::Native,
        (Some(kind), false) => ForkStep::Fresh(Box::new(NewRequest {
            kind: kind.clone(),
            folder: launch.cwd.clone(),
            name: None,
            place: Place::Right,
            task: None,
            worktree: false,
            checkpoints: None,
            permission_cards: launch.permission_cards,
            bypass: launch.bypass,
            no_sandbox: launch.no_sandbox,
        })),
        (None, false) => ForkStep::Unsupported,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum RestartStep {
    Ignore,
    Resume,
    Confirm,
    StopThenResume,
    Flagged,
}

pub(super) fn restart_step(row: Option<&SessionRow>, resuming: bool, flagged: bool) -> RestartStep {
    match row {
        None => RestartStep::Ignore,
        Some(_) if resuming => RestartStep::Ignore,
        Some(_) if flagged => RestartStep::Flagged,
        Some(r) if r.exited() => RestartStep::Resume,
        Some(r) if r.state.kind == StateKind::Working => RestartStep::Confirm,
        Some(_) => RestartStep::StopThenResume,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch(kind: Option<SpawnKind>) -> Launch {
        Launch {
            kind,
            cwd: "/work/app".into(),
            bypass: false,
            no_sandbox: false,
            permission_cards: true,
            conversation: true,
        }
    }

    fn agent(profile: &str) -> Option<SpawnKind> {
        Some(SpawnKind::Agent {
            profile: profile.into(),
        })
    }

    #[test]
    fn claude_and_codex_with_a_conversation_fork_natively() {
        assert_eq!(fork_step(&launch(agent("claude-code"))), ForkStep::Native);
        assert_eq!(fork_step(&launch(agent("codex"))), ForkStep::Native);
    }

    #[test]
    fn everything_else_forks_fresh() {
        let mut no_id = launch(agent("claude-code"));
        no_id.conversation = false;
        let mut open = launch(agent("codex"));
        open.no_sandbox = true;
        let mut bypass = launch(agent("claude-code"));
        bypass.bypass = true;
        for l in [
            no_id,
            open,
            bypass,
            launch(Some(SpawnKind::Shell)),
            launch(agent("custom")),
        ] {
            assert!(matches!(fork_step(&l), ForkStep::Fresh(_)), "{l:?}");
        }
        assert_eq!(fork_step(&launch(None)), ForkStep::Unsupported);
    }

    #[test]
    fn a_fresh_fork_copies_the_launch_config_without_a_task() {
        let mut l = launch(agent("codex"));
        l.conversation = false;
        l.bypass = true;
        l.no_sandbox = true;
        let ForkStep::Fresh(req) = fork_step(&l) else {
            panic!("expected a fresh fork");
        };
        assert_eq!(req.kind, agent("codex").unwrap());
        assert_eq!(req.folder, "/work/app");
        assert_eq!(req.place, Place::Right);
        assert!(req.bypass && req.no_sandbox && req.permission_cards);
        assert!(req.task.is_none() && !req.worktree && req.name.is_none());
        assert!(req.checkpoints.is_none());
    }
}
