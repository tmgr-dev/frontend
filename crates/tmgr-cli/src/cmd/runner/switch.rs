use super::ops::{clean, with_session};
use super::run_task::{execute_switch, prepare, Handoff, Launch, Started};
use super::{emit, rerr, Opts, PermissionMode};
use clap::ValueEnum;
use serde_json::json;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::SessionInfo;
use tmgr_runner::switch::{self as lib, SwitchError, Target};

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum, Debug)]
pub enum SwitchTarget {
    #[value(name = "claude-code")]
    ClaudeCode,
    Codex,
}

impl From<SwitchTarget> for Target {
    fn from(t: SwitchTarget) -> Target {
        match t {
            SwitchTarget::ClaudeCode => Target::ClaudeCode,
            SwitchTarget::Codex => Target::Codex,
        }
    }
}

fn err(e: SwitchError) -> Error {
    match e {
        SwitchError::Usage(m) => Error::usage(m),
        SwitchError::NotFound(m) => Error::not_found(m),
        SwitchError::Conflict(m) => Error::new(Kind::Conflict, m),
        SwitchError::Internal(m) => Error::internal(m),
    }
}

pub(super) fn switch(o: &Opts, a: &super::SwitchArgs) -> Result<i32> {
    let (mut c, s) = with_session(&a.session)?;
    let target = lib::check(&s, a.to.map(Target::from)).map_err(err)?;
    let reference = s
        .task
        .map(|t| format!("#{}", t.task_id))
        .ok_or_else(|| Error::usage("the session has no task"))?;
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
    lib::ensure_readable(&s).map_err(err)?;
    lib::stop(&mut c, &s, target).map_err(err)?;
    let note = lib::prepare_note(&s, &sessions, target, &reference).map_err(err)?;
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
            note: &note.text,
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
    let warning = lib::export_all(&started.info.id, &note.exports);
    report(o, &s, target, &started, warning)
}

fn report(
    o: &Opts,
    old: &SessionInfo,
    target: Target,
    started: &Started,
    warning: Option<String>,
) -> Result<i32> {
    let bin = super::bin();
    if let Some(w) = &warning {
        eprintln!("warning: {w}");
    }
    let safe_note = (old.permission_mode.as_deref() == Some("bypass") || old.no_sandbox)
        .then_some("the old session ran with bypass or without the sandbox; the new one starts in safe mode and will ask for approvals");
    if let (Some(n), false) = (safe_note, o.json) {
        eprintln!("{n}");
    }
    let human = format!(
        "switched {} to {}: new session {} in {}\nattach with: {bin} attach {}",
        clean(&old.name),
        target.profile(),
        started.info.id,
        clean(&started.cwd),
        started.info.id
    );
    Ok(emit(
        o,
        json!({
            "from": old.id,
            "to": started.info.id,
            "agent": target.profile(),
            "warning": warning,
            "safe_mode_note": safe_note,
            "session": started.info,
        }),
        human,
    ))
}
