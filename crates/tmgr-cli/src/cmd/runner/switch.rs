use super::ops::{clean, with_session};
use super::ops::{login_preflight, run_create_run};
use super::run_task::{launch_pin, plan_switch, prepare, start_switch, Launch, Started};
use super::{connect, emit, privilege, rerr, runner_config, Opts, PermissionMode};
use clap::ValueEnum;
use serde_json::json;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::limits::MAX_RUN_PROMPT;
use tmgr_ipc::api::{caps, SessionInfo};
use tmgr_runner::daemon;
use tmgr_runner::switch::{self as lib, SwitchError, Target};

const SCREEN_LINES: u32 = 400;
const NOTE_CHARS: usize = 40_000;

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum, Debug)]
pub enum SwitchTarget {
    #[value(name = "claude-code")]
    ClaudeCode,
    Codex,
    #[value(name = "opencode")]
    OpenCode,
    Agy,
}

impl From<SwitchTarget> for Target {
    fn from(t: SwitchTarget) -> Target {
        match t {
            SwitchTarget::ClaudeCode => Target::ClaudeCode,
            SwitchTarget::Codex => Target::Codex,
            SwitchTarget::OpenCode => Target::OpenCode,
            SwitchTarget::Agy => Target::Agy,
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

pub(super) fn carried_mode(old: &SessionInfo, keep_bypass: bool) -> PermissionMode {
    match old.permission_mode.as_deref() {
        Some("accept-edits") => PermissionMode::AcceptEdits,
        Some("plan") => PermissionMode::Plan,
        Some("bypass") if keep_bypass => PermissionMode::Bypass,
        _ => PermissionMode::Ask,
    }
}

fn was_flagged(old: &SessionInfo) -> bool {
    old.permission_mode.as_deref() == Some("bypass") || old.no_sandbox
}

fn supports(profile: &str, mode: PermissionMode) -> bool {
    let dir = tmgr_runner::profile::user_dir();
    let (set, _) = tmgr_runner::profile::ProfileSet::load(dir.as_deref());
    set.lookup(profile)
        .is_ok_and(|p| p.permission_args(mode.as_str()).is_ok())
}

fn fitted(old: &SessionInfo, target: Target, keep_bypass: bool) -> Result<(PermissionMode, bool)> {
    let wanted = carried_mode(old, keep_bypass);
    if supports(target.profile(), wanted) {
        return Ok((wanted, false));
    }
    if wanted == PermissionMode::Bypass {
        return Err(
            Error::usage(format!("{} cannot run in bypass mode", target.profile()))
                .hint("drop --keep-bypass to continue in ask mode"),
        );
    }
    Ok((PermissionMode::Ask, true))
}

fn cut_bytes(note: &str) -> String {
    let mut out = String::new();
    for c in note.chars().take(NOTE_CHARS) {
        if out.len() + c.len_utf8() > MAX_RUN_PROMPT {
            break;
        }
        out.push(c);
    }
    out
}

struct Plain {
    pins: (tmgr_ipc::api::ExePin, String),
}

fn plain_check(
    c: &super::Client,
    target: Target,
    mode: PermissionMode,
    no_sandbox: bool,
) -> Result<Plain> {
    if !c.welcome().caps.iter().any(|h| h == caps::RUN_PROMPT_V1) {
        return Err(Error::new(
            Kind::Conflict,
            "the running runner is older than this tmgr and cannot start a session with a handoff note",
        )
        .hint(format!("{} runner restart", super::bin())));
    }
    if !no_sandbox {
        login_preflight(target.profile())?;
    }
    let pins = launch_pin(target.profile(), None, mode.as_str())?;
    Ok(Plain { pins })
}

fn start_plain(
    old: &SessionInfo,
    target: Target,
    (mode, no_sandbox): (PermissionMode, bool),
    plain: Plain,
    (size, note): ((u16, u16), &str),
) -> Result<Started> {
    let cfg = runner_config()?;
    daemon::start_background(&cfg).map_err(rerr)?;
    let mut client = connect(&cfg)?;
    let mut req = run_create_run(
        target.profile().to_string(),
        old.cwd.clone(),
        None,
        None,
        size,
        false,
    );
    req.permission_mode = Some(mode.as_str().to_string());
    req.no_sandbox = privilege::request(no_sandbox);
    req.exe_pin = Some(plain.pins.0);
    req.argv_sha256 = Some(plain.pins.1);
    req.parent_session = Some(old.id.clone());
    req.prompt = Some(cut_bytes(note));
    let info = client.create_run(req).map_err(rerr)?;
    Ok(Started {
        info,
        cwd: old.cwd.clone(),
        fetch: String::new(),
    })
}

enum Route {
    Task(Box<super::run_task::SwitchPlan>),
    Plain(Plain),
}

pub(super) fn switch(o: &Opts, a: &super::SwitchArgs) -> Result<i32> {
    let (mut c, s) = with_session(&a.session)?;
    let target = lib::check(&s, a.to.map(Target::from)).map_err(err)?;
    let (mode, mode_fallback) = fitted(&s, target, a.keep_bypass)?;
    let no_sandbox = a.keep_bypass && s.no_sandbox;
    privilege::check_caps(
        &c.welcome().caps,
        mode == PermissionMode::Bypass,
        no_sandbox,
    )?;
    let reference = s.task.map(|t| format!("#{}", t.task_id));
    let launch = Launch {
        model: None,
        name: None,
        base: None,
        size: super::ops::terminal_size(),
    };
    let route = match (s.task, reference.as_deref()) {
        (Some(task), Some(reference)) => {
            let prepared = prepare(o.ws.as_deref(), None, reference, target.profile(), mode)?;
            let plan = plan_switch(prepared, &launch, &s.cwd, no_sandbox)?;
            if plan.task() != task {
                return Err(Error::usage(
                    "the task of this session was not found in the selected workspace",
                )
                .hint("pass --ws local:<code> for the workspace of the session's task"));
            }
            Route::Task(Box::new(plan))
        }
        _ => Route::Plain(plain_check(&c, target, mode, no_sandbox)?),
    };
    let screen = if lib::has_log(&s) {
        lib::ensure_readable(&s).map_err(err)?;
        Vec::new()
    } else {
        if !c.welcome().caps.iter().any(|h| h == caps::SESSION_TEXT_V1) {
            return Err(Error::new(
                Kind::Conflict,
                "the running runner is older than this tmgr and cannot read the screen of a session",
            )
            .hint(format!("{} runner restart", super::bin())));
        }
        c.text(&s.id, Some(SCREEN_LINES)).map_err(rerr)?.lines
    };
    if !o.json && !o.quiet {
        match s.task {
            Some(task) => eprintln!(
                "Continue task #{} in {} (new session, same folder, {} permission mode)",
                task.task_id,
                target.profile(),
                mode.as_str()
            ),
            None => eprintln!(
                "Continue in {} (new session, same folder, {} permission mode)",
                target.profile(),
                mode.as_str()
            ),
        }
    }
    privilege::confirm(mode == PermissionMode::Bypass, no_sandbox, false, a.yes)?;
    crate::interact::confirm(
        &format!(
            "Stop {} and continue the work in {}?",
            clean(&s.name),
            target.profile()
        ),
        true,
        a.yes,
    )?;
    let sessions = c.list().map_err(rerr)?;
    let resume_hint = format!("Resume the old one with `{} resume {}`", super::bin(), s.id);
    lib::stop(&mut c, &s, target).map_err(|e| {
        Error::new(
            Kind::Conflict,
            format!(
                "{e}. The old session got an interrupt and then a terminate signal and may have stopped in the middle of a turn. {resume_hint}"
            ),
        )
    })?;
    let after_stop = |e: String| {
        Error::new(
            Kind::Conflict,
            format!("the old session stopped, but the new agent did not start: {e}. {resume_hint}"),
        )
    };
    let note = lib::prepare_note(&s, &sessions, target, reference.as_deref(), &screen)
        .map_err(|e| after_stop(e.to_string()))?;
    let started = match route {
        Route::Task(plan) => start_switch(*plan, &launch, (&s.id, &note.text)),
        Route::Plain(plain) => start_plain(
            &s,
            target,
            (mode, no_sandbox),
            plain,
            (launch.size, &note.text),
        ),
    }
    .map_err(|e| after_stop(e.to_string()))?;
    let warning = lib::export_all(&started.info.id, &note.exports);
    let dropped = was_flagged(&s) && !a.keep_bypass;
    report(o, (&s, target), &started, (warning, dropped, mode_fallback))
}

fn report(
    o: &Opts,
    (old, target): (&SessionInfo, Target),
    started: &Started,
    (warning, dropped, mode_fallback): (Option<String>, bool, bool),
) -> Result<i32> {
    let bin = super::bin();
    if let Some(w) = &warning {
        eprintln!("warning: {w}");
    }
    let safe_note = dropped.then_some("the old session ran with bypass or without the sandbox; the new one starts in safe mode and will ask for approvals (pass --keep-bypass to carry it over)");
    let mode_note = mode_fallback.then(|| {
        format!(
            "{} has no {} mode; the new session starts in ask mode",
            target.profile(),
            old.permission_mode.as_deref().unwrap_or("ask")
        )
    });
    if !o.json {
        for n in safe_note.iter().copied().chain(mode_note.as_deref()) {
            eprintln!("{n}");
        }
    }
    let human = format!(
        "switched {} to {}: new session {} in {} ({} permission mode)\nattach with: {bin} attach {}",
        clean(&old.name),
        target.profile(),
        started.info.id,
        clean(&started.cwd),
        started.info.permission_mode.as_deref().unwrap_or("ask"),
        started.info.id
    );
    Ok(emit(
        o,
        json!({
            "from": old.id,
            "to": started.info.id,
            "agent": target.profile(),
            "permission_mode": started.info.permission_mode,
            "warning": warning,
            "safe_mode_note": safe_note,
            "mode_note": mode_note,
            "session": started.info,
        }),
        human,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_ipc::api::{State, StateKind};

    fn old(mode: Option<&str>, no_sandbox: bool) -> SessionInfo {
        SessionInfo {
            id: "a".into(),
            name: "n".into(),
            argv: vec![],
            cwd: "/w".into(),
            profile: Some("claude-code".into()),
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
            task: None,
            bundle_sha256: None,
            origin: None,
            permission_mode: mode.map(str::to_string),
            resumed_from: None,
            human_s: None,
            no_sandbox,
            checkpoints: None,
            permission_cards: false,
            team: None,
            agent_session_id: None,
            limit: None,
            handoff_from: None,
            managed: false,
            manager: false,
        }
    }

    #[test]
    fn ordinary_modes_carry_over_silently() {
        assert_eq!(
            carried_mode(&old(Some("accept-edits"), false), false).as_str(),
            "accept-edits"
        );
        assert_eq!(
            carried_mode(&old(Some("plan"), false), false).as_str(),
            "plan"
        );
        assert_eq!(
            carried_mode(&old(Some("ask"), false), false).as_str(),
            "ask"
        );
        assert_eq!(carried_mode(&old(None, false), true).as_str(), "ask");
        assert_eq!(carried_mode(&old(Some("odd"), false), true).as_str(), "ask");
    }

    #[test]
    fn bypass_carries_only_with_the_explicit_flag() {
        let b = old(Some("bypass"), false);
        assert_eq!(carried_mode(&b, false).as_str(), "ask");
        assert_eq!(carried_mode(&b, true).as_str(), "bypass");
        assert!(was_flagged(&b));
        assert!(was_flagged(&old(Some("ask"), true)));
        assert!(!was_flagged(&old(Some("plan"), false)));
    }

    #[test]
    fn a_long_note_is_cut_on_a_character_boundary_within_the_byte_limit() {
        let note = "я".repeat(NOTE_CHARS);
        let cut = cut_bytes(&note);
        assert!(cut.len() <= MAX_RUN_PROMPT);
        assert!(cut.chars().all(|c| c == 'я'));
        assert_eq!(cut_bytes("short"), "short");
    }
}
