use super::attach_consent::Reads;
use super::attach_goto::{can_fork, fork_call, fork_name};
use super::attach_questions::QuestionJob;
use super::attach_title::{agent_src, AgentSrc};
use super::catalog::{self, Entry, Live};
use super::ops::{clean, elevated, list_or_empty, resolve, run_create_run, terminal_size};
use super::{attach, connect, emit, rerr, runner_config, Opts, PermissionMode, ResumeArgs};
use std::io::IsTerminal;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::Duration;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{
    CwdPin, ExternalResume, RestoredRun, ResumeParams, RunCreateReq, SessionInfo, StateKind,
};
use tmgr_runner::client::Client;
use tmgr_runner::daemon::{self, RunnerConfig};
use tmgr_runner::resume::valid_resume_id;
use tmgr_usage::paths::Env;

#[derive(Debug)]
pub(super) struct Plan {
    pub profile: &'static str,
    pub id: String,
    pub cwd: String,
    pub pin: CwdPin,
    pub fork: bool,
    pub warnings: Vec<String>,
}

pub(super) fn is_tmgr_session(key: &str) -> Result<bool> {
    let cfg = runner_config()?;
    Ok(names_tmgr_session(&list_or_empty(&cfg)?, key))
}

fn names_tmgr_session(sessions: &[SessionInfo], key: &str) -> bool {
    !valid_resume_id(key) && resolve(sessions, key).is_ok()
}

fn check_readable(e: &Entry, denial: Option<&str>) -> Result<()> {
    if e.readable {
        return Ok(());
    }
    Err(Error::new(
        Kind::Forbidden,
        "session files of this agent are not readable",
    )
    .hint(denial.unwrap_or("run tmgr usage consent allow")))
}

fn check_live(e: &Entry, fork: bool) -> Result<Vec<String>> {
    let bin = super::bin();
    let mut warnings = Vec::new();
    match e.live {
        Live::Yes if !fork => {
            return Err(Error::new(
                Kind::Conflict,
                "this session is running in another terminal",
            )
            .hint(format!(
                "resume would write to the same conversation; use --fork: {bin} resume {} --fork --no-sandbox",
                e.id
            )))
        }
        Live::Recent if !fork => warnings.push(
            "warning: this session was active a moment ago or its state cannot be told, so it may still be running; resuming it can interleave two writers".to_string(),
        ),
        _ => {}
    }
    Ok(warnings)
}

pub(super) fn check_known(id: &str, fork: bool) -> Result<Vec<String>> {
    let env = Env::from_process();
    let e = catalog::find(&env, id).ok_or_else(|| {
        Error::not_found(format!(
            "the conversation of this session was not found: {}",
            clean(id)
        ))
    })?;
    check_readable(&e, Reads::of(&env).hint())?;
    check_live(&e, fork)
}

pub(super) fn conversation_line(s: &SessionInfo) -> String {
    match s.agent_session_id.as_deref().filter(|i| valid_resume_id(i)) {
        Some(id) => format!("  conversation  {id}"),
        None => "  conversation  taken from its recorded output".to_string(),
    }
}

pub(super) fn plan(e: &Entry, fork: bool, no_sandbox: bool, denial: Option<&str>) -> Result<Plan> {
    let bin = super::bin();
    check_readable(e, denial)?;
    if !no_sandbox {
        return Err(Error::usage(
            "an ordinary session lives in your real home folder, which the sandbox cannot see",
        )
        .hint(format!(
            "repeat with --no-sandbox: {bin} resume {} --no-sandbox",
            e.id
        )));
    }
    let warnings = check_live(e, fork)?;
    let recorded = e.cwd.as_deref().ok_or_else(|| {
        Error::not_found("the folder of this session was not recorded, so it cannot be started")
    })?;
    let gone = || {
        Error::not_found(format!(
            "the folder of this session no longer exists: {}",
            clean(recorded)
        ))
        .hint("restore the folder, then run the command again")
    };
    let real = std::fs::canonicalize(recorded).map_err(|_| gone())?;
    if !real.is_dir() {
        return Err(gone());
    }
    if real != Path::new(recorded) {
        return Err(Error::new(
            Kind::Conflict,
            format!(
                "the recorded folder {} now resolves to {}, and the agent finds its conversations by folder",
                clean(recorded),
                clean(&real.display().to_string())
            ),
        )
        .hint("restore the folder at its recorded path, then run the command again"));
    }
    let cwd = real
        .to_str()
        .ok_or_else(|| Error::usage("the folder of this session is not valid text"))?
        .to_string();
    let meta = std::fs::metadata(&real).map_err(|_| gone())?;
    Ok(Plan {
        profile: e.agent.profile(),
        id: e.id.clone(),
        cwd,
        pin: CwdPin {
            dev: meta.dev(),
            ino: meta.ino(),
        },
        fork,
        warnings,
    })
}

pub(super) fn request(p: &Plan, mode: Option<PermissionMode>, size: (u16, u16)) -> RunCreateReq {
    let mut req = run_create_run(
        p.profile.to_string(),
        p.cwd.clone(),
        None,
        None,
        size,
        false,
    );
    req.cwd_pin = Some(p.pin);
    req.external_resume = Some(ExternalResume {
        id: p.id.clone(),
        fork: p.fork,
    });
    elevated(req, mode, true)
}

fn scrub(s: &mut String) {
    *s = clean(s);
}

pub(super) fn clean_info(info: &SessionInfo) -> SessionInfo {
    let mut i = info.clone();
    for s in [&mut i.id, &mut i.name, &mut i.cwd] {
        scrub(s);
    }
    i.argv.iter_mut().for_each(scrub);
    for s in [
        &mut i.profile,
        &mut i.permission_mode,
        &mut i.resumed_from,
        &mut i.agent_session_id,
    ]
    .into_iter()
    .flatten()
    {
        scrub(s);
    }
    i
}

pub(super) fn clean_run(r: &RestoredRun) -> RestoredRun {
    let mut r = r.clone();
    r.session = clean_info(&r.session);
    if let Some(v) = r.via.as_mut() {
        scrub(v);
    }
    r
}

fn report(o: &Opts, info: &SessionInfo, verb: &str) -> i32 {
    let bin = super::bin();
    let info = clean_info(info);
    let conversation = info
        .agent_session_id
        .as_deref()
        .map(|id| format!(" conversation {id}"))
        .unwrap_or_default();
    let human = format!(
        "{verb} {} ({}){conversation}; attach with: {bin} attach {}",
        info.name, info.id, info.id
    );
    emit(o, &info, human)
}

fn attach_or_report(
    o: &Opts,
    cfg: &RunnerConfig,
    a: &ResumeArgs,
    info: &SessionInfo,
) -> Result<i32> {
    if !a.no_attach && !o.json && std::io::stdout().is_terminal() {
        return attach::run(cfg, &info.id);
    }
    Ok(report(o, info, if a.fork { "forked" } else { "resumed" }))
}

pub(super) fn plan_header(p: &Plan) -> Vec<String> {
    vec![
        format!(
            "{} this conversation?",
            if p.fork { "Fork" } else { "Resume" }
        ),
        format!("  agent    {}", p.profile),
        format!("  id       {}", p.id),
        format!("  folder   {}", clean(&p.cwd)),
    ]
}

pub(super) fn recheck_entry(again: Option<Entry>, denial: Option<&str>, fork: bool) -> Result<()> {
    let again = again.ok_or_else(|| {
        Error::not_found("the conversation disappeared while waiting for confirmation")
    })?;
    check_readable(&again, denial)?;
    check_live(&again, fork)?;
    Ok(())
}

pub(super) fn recheck_external(env: &Env, session: &str, fork: bool) -> Result<()> {
    recheck_entry(catalog::find(env, session), Reads::of(env).hint(), fork)
}

pub(super) fn create_external(
    cfg: &RunnerConfig,
    p: &Plan,
    mode: Option<PermissionMode>,
    size: (u16, u16),
    wait: Option<Duration>,
) -> Result<SessionInfo> {
    daemon::start_background(cfg).map_err(rerr)?;
    let mut client = connect(cfg)?;
    if let Some(wait) = wait {
        let _ = client.set_timeouts(wait);
    }
    let caps = client.welcome().caps.clone();
    super::privilege::check_caps(&caps, mode == Some(PermissionMode::Bypass), true)?;
    super::privilege::check_external_cap(&caps)?;
    client.create_run(request(p, mode, size)).map_err(rerr)
}

pub(super) fn tmgr_resume_checks_with(
    s: &SessionInfo,
    no_sandbox: bool,
    check: &dyn Fn(&str) -> Result<Vec<String>>,
) -> Result<(Vec<String>, Option<String>)> {
    if s.state.kind != StateKind::Exited {
        return Err(Error::new(
            Kind::Conflict,
            format!("{} is still running", clean(&s.name)),
        ));
    }
    let known = s
        .agent_session_id
        .clone()
        .filter(|id| no_sandbox && valid_resume_id(id));
    let warnings = match &known {
        Some(id) => check(id)?,
        None => Vec::new(),
    };
    Ok((warnings, known))
}

pub(super) fn tmgr_resume_checks(
    s: &SessionInfo,
    no_sandbox: bool,
) -> Result<(Vec<String>, Option<String>)> {
    tmgr_resume_checks_with(s, no_sandbox, &|id| check_known(id, false))
}

pub(super) fn tmgr_resume_header(s: &SessionInfo, no_sandbox: bool) -> Vec<String> {
    let mut lines = vec![
        "Resume this session?".to_string(),
        format!("  session  {}", clean(&s.id)),
        format!("  name     {}", clean(&s.name)),
        format!("  folder   {}", clean(&s.cwd)),
    ];
    if no_sandbox {
        lines.push(conversation_line(s));
    }
    lines
}

pub(super) fn resume_gate(
    known: Option<&str>,
    have: &[String],
    mode: Option<PermissionMode>,
    no_sandbox: bool,
    recheck: &dyn Fn(&str) -> Result<Vec<String>>,
) -> Result<()> {
    if let Some(id) = known {
        recheck(id)?;
    }
    super::privilege::check_caps(have, mode == Some(PermissionMode::Bypass), no_sandbox)?;
    super::privilege::check_external_cap(have)
}

pub(super) fn resume_flagged(
    c: &mut Client,
    s: &SessionInfo,
    known: Option<&str>,
    mode: Option<PermissionMode>,
    no_sandbox: bool,
) -> Result<RestoredRun> {
    let have = c.welcome().caps.clone();
    resume_gate(known, &have, mode, no_sandbox, &|id| check_known(id, false))?;
    c.resume_with(ResumeParams {
        id: s.id.clone(),
        cols: s.cols,
        rows: s.rows,
        permission_mode: mode.map(|m| m.as_str().to_string()),
        no_sandbox: super::privilege::request(no_sandbox),
    })
    .map_err(rerr)
}

pub(super) fn external(o: &Opts, a: &ResumeArgs) -> Result<i32> {
    let env = Env::from_process();
    let entry = catalog::find(&env, &a.session).ok_or_else(|| {
        Error::not_found(format!(
            "no tmgr session or recorded conversation matches {}",
            clean(&a.session)
        ))
        .hint(format!("{} session list", super::bin()))
    })?;
    let p = plan(&entry, a.fork, a.no_sandbox, Reads::of(&env).hint())?;
    if !o.json && !o.quiet {
        for w in &p.warnings {
            eprintln!("{w}");
        }
    }
    eprintln!("{}", plan_header(&p).join("\n"));
    let bypass = a.mode == Some(PermissionMode::Bypass);
    super::privilege::confirm(bypass, true, false, false)?;
    recheck_external(&env, &a.session, a.fork)?;
    let cfg = runner_config()?;
    let info = create_external(&cfg, &p, a.mode, terminal_size(), None)?;
    attach_or_report(o, &cfg, a, &info)
}

fn tmgr_fork_refusal(s: &SessionInfo, a: &ResumeArgs) -> Option<Error> {
    fork_refusal(s, a.mode.is_some() || a.no_sandbox)
}

pub(super) fn fork_refusal(s: &SessionInfo, flagged: bool) -> Option<Error> {
    if flagged {
        return Some(
            Error::usage("--fork of a tmgr session keeps the settings it was started with")
                .hint("repeat without --mode and --no-sandbox"),
        );
    }
    if !s.profile.as_deref().is_some_and(can_fork) {
        return Some(Error::new(
            Kind::Forbidden,
            "only Claude Code and Codex sessions can be forked from a tmgr session",
        ));
    }
    if s.no_sandbox {
        let hint = s
            .agent_session_id
            .as_deref()
            .filter(|id| tmgr_runner::resume::valid_resume_id(id))
            .map(|id| {
                format!(
                    "fork its conversation by id: {} resume {id} --fork --no-sandbox",
                    super::bin()
                )
            });
        let e = Error::new(
            Kind::Forbidden,
            "a session started with --no-sandbox cannot be forked by its tmgr id",
        );
        return Some(match hint {
            Some(h) => e.hint(h),
            None => e,
        });
    }
    None
}

pub(super) fn fork_source(s: &SessionInfo) -> Result<AgentSrc> {
    agent_src(s).ok_or_else(|| Error::usage("this session has no agent to fork"))
}

pub(super) fn fork_job(cfg: &RunnerConfig, s: &SessionInfo, src: AgentSrc) -> QuestionJob {
    QuestionJob {
        id: s.id.clone(),
        pid: src.pid,
        src,
        sandbox_home: cfg.homes.join(&s.id),
        log: cfg.dir.join("sessions").join(&s.id).join("output.log"),
        exited: s.state.kind == StateKind::Exited,
        settled: !matches!(s.state.kind, StateKind::Working | StateKind::Starting),
    }
}

pub(super) fn fork_run(
    cfg: &RunnerConfig,
    job: &QuestionJob,
    s: &SessionInfo,
    size: (u16, u16),
) -> Result<(RestoredRun, String)> {
    let sessions = connect(cfg)?.list().map_err(rerr)?;
    let names: Vec<&str> = sessions.iter().map(|x| x.name.as_str()).collect();
    let name = fork_name(&names, &s.name);
    let recorded = s.agent_session_id.as_deref();
    fork_call(cfg, job, &name, size, (Reads::current(), recorded))
        .map_err(|e| Error::new(Kind::Conflict, e))
}

pub(super) fn fork_tmgr(o: &Opts, a: &ResumeArgs, s: &SessionInfo) -> Result<i32> {
    if let Some(e) = tmgr_fork_refusal(s, a) {
        return Err(e);
    }
    let src = fork_source(s)?;
    let cfg = runner_config()?;
    let job = fork_job(&cfg, s, src);
    let (run, conversation) = fork_run(&cfg, &job, s, terminal_size())?;
    if !o.json && !o.quiet {
        eprintln!("Forked conversation {conversation}");
    }
    attach_or_report(o, &cfg, a, &run.session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::runner::catalog::testkit::{entry, uid};
    use crate::cmd::runner::catalog::Agent;
    use tmgr_ipc::api::caps;

    fn at(dir: &Path, agent: Agent) -> Entry {
        let mut e = entry(agent);
        e.cwd = Some(std::fs::canonicalize(dir).unwrap().display().to_string());
        e
    }

    fn fixture() -> (tempfile::TempDir, Entry) {
        let tmp = tempfile::tempdir().unwrap();
        let e = at(tmp.path(), Agent::Claude);
        (tmp, e)
    }

    #[test]
    fn a_session_without_no_sandbox_is_refused_with_a_hint_to_repeat_with_it() {
        let (_t, e) = fixture();
        let err = plan(&e, false, false, None).unwrap_err();
        assert!(err.message.contains("real home folder"), "{}", err.message);
        assert!(err.message.contains("sandbox cannot see"));
        let hint = err.hint.unwrap();
        assert!(
            hint.contains("--no-sandbox") && hint.contains(&e.id),
            "{hint}"
        );
    }

    #[test]
    fn a_session_whose_files_are_not_readable_is_refused_with_the_consent_hint() {
        let (_t, mut e) = fixture();
        e.readable = false;
        let off = Reads::default().hint();
        let err = plan(&e, false, true, off).unwrap_err();
        assert_eq!(err.hint.as_deref(), off);
        let err = plan(&e, true, true, off).unwrap_err();
        assert_eq!(err.hint.as_deref(), off);
    }

    #[test]
    fn a_live_session_is_refused_for_resume_and_allowed_for_fork() {
        let (_t, mut e) = fixture();
        e.live = Live::Yes;
        let err = plan(&e, false, true, None).unwrap_err();
        assert!(err.message.contains("running"));
        assert!(err.hint.unwrap().contains("--fork"));
        assert!(plan(&e, true, true, None).unwrap().warnings.is_empty());
    }

    #[test]
    fn a_recently_active_session_warns_and_continues() {
        let (_t, mut e) = fixture();
        e.live = Live::Recent;
        let p = plan(&e, false, true, None).unwrap();
        assert_eq!(p.warnings.len(), 1);
        assert!(p.warnings[0].starts_with("warning:"));
        assert!(plan(&e, true, true, None).unwrap().warnings.is_empty());
    }

    #[test]
    fn a_missing_folder_is_refused_naming_it_and_never_replaced() {
        let (tmp, mut e) = fixture();
        let gone = tmp.path().join("gone\u{1b}[2Jx");
        e.cwd = Some(gone.display().to_string());
        let err = plan(&e, false, true, None).unwrap_err();
        assert_eq!(err.kind, Kind::NotFound);
        assert!(err.message.contains("gone"), "{}", err.message);
        assert!(!err.message.contains('\u{1b}'));
        e.cwd = None;
        assert_eq!(
            plan(&e, false, true, None).unwrap_err().kind,
            Kind::NotFound
        );
    }

    #[test]
    fn a_folder_that_is_a_file_is_refused() {
        let (tmp, mut e) = fixture();
        let f = std::fs::canonicalize(tmp.path()).unwrap().join("f");
        std::fs::write(&f, b"x").unwrap();
        e.cwd = Some(f.display().to_string());
        assert_eq!(
            plan(&e, false, true, None).unwrap_err().kind,
            Kind::NotFound
        );
    }

    #[test]
    fn a_recorded_folder_that_resolves_elsewhere_is_refused_naming_both_paths() {
        let (tmp, mut e) = fixture();
        let real = std::fs::canonicalize(tmp.path()).unwrap();
        let link = real.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        e.cwd = Some(link.display().to_string());
        let err = plan(&e, false, true, None).unwrap_err();
        assert_eq!(err.kind, Kind::Conflict);
        assert!(err.message.contains("link"), "{}", err.message);
        assert!(err.message.contains("now resolves to"));
    }

    #[test]
    fn the_plan_carries_the_resolved_folder_the_profile_and_the_id() {
        for agent in [Agent::Claude, Agent::Codex] {
            let tmp = tempfile::tempdir().unwrap();
            let mut e = at(tmp.path(), agent);
            e.id = uid(7);
            for fork in [false, true] {
                let p = plan(&e, fork, true, None).unwrap();
                assert_eq!(p.profile, agent.profile());
                assert_eq!(p.id, uid(7));
                assert_eq!(p.cwd, e.cwd.clone().unwrap());
                assert_eq!(p.fork, fork);
            }
        }
    }

    #[test]
    fn the_request_matches_what_the_daemon_accepts_for_every_agent_fork_and_mode() {
        for agent in [Agent::Claude, Agent::Codex] {
            for fork in [false, true] {
                for mode in [None, Some(PermissionMode::Bypass)] {
                    let tmp = tempfile::tempdir().unwrap();
                    let e = at(tmp.path(), agent);
                    let p = plan(&e, fork, true, None).unwrap();
                    let r = request(&p, mode, (100, 30));
                    assert_eq!(r.profile, agent.profile());
                    assert_eq!(r.cwd, p.cwd);
                    assert_eq!(
                        r.external_resume,
                        Some(ExternalResume {
                            id: e.id.clone(),
                            fork
                        })
                    );
                    assert!(r.no_sandbox.is_some());
                    assert_eq!(r.permission_mode.as_deref(), mode.map(|m| m.as_str()));
                    assert_eq!((r.cols, r.rows), (100, 30));
                    assert!(r.task.is_none() && r.origin.is_none() && r.start_key.is_none());
                    assert!(r.clones.is_empty() && r.cloud.is_none() && !r.unattended);
                    assert!(r.trust_cwd.is_none() && r.team.is_none() && !r.agent_work);
                }
            }
        }
    }

    fn session(profile: &str, no_sandbox: bool, agent_id: Option<&str>) -> SessionInfo {
        serde_json::from_value(serde_json::json!({
            "id": "s_aaaaaaaaaa", "name": "w", "argv": ["/bin/sh"], "cwd": "/w",
            "profile": profile, "state": {"kind": "exited", "since_ms": 1},
            "created_ms": 1, "cols": 80, "rows": 24, "shim_pid": 1, "child_pid": 2,
            "no_sandbox": no_sandbox, "agent_session_id": agent_id,
        }))
        .unwrap()
    }

    fn args(mode: Option<PermissionMode>, no_sandbox: bool) -> ResumeArgs {
        ResumeArgs {
            session: "s_aaaaaaaaaa".into(),
            fork: true,
            mode,
            no_sandbox,
            no_attach: false,
            attach: false,
        }
    }

    #[test]
    fn forking_a_tmgr_session_refuses_flags_other_agents_and_unsandboxed_sessions() {
        let claude = session("claude-code", false, None);
        assert!(tmgr_fork_refusal(&claude, &args(None, false)).is_none());
        for a in [args(Some(PermissionMode::Bypass), false), args(None, true)] {
            let e = tmgr_fork_refusal(&claude, &a).unwrap();
            assert!(e.message.contains("keeps the settings"), "{}", e.message);
        }
        assert!(tmgr_fork_refusal(&session("codex", false, None), &args(None, false)).is_none());
        let e = tmgr_fork_refusal(&session("codex", false, None), &args(None, true)).unwrap();
        assert!(e.message.contains("keeps the settings"), "{}", e.message);
        for other in ["gemini", "claude", "Codex"] {
            let e = tmgr_fork_refusal(&session(other, false, None), &args(None, false)).unwrap();
            assert!(
                e.message.contains("only Claude Code and Codex"),
                "{}",
                e.message
            );
        }
        let e = tmgr_fork_refusal(&session("codex", true, None), &args(None, false)).unwrap();
        assert!(e.message.contains("--no-sandbox") && e.hint.is_none());
        let id = uid(4);
        let e = tmgr_fork_refusal(&session("codex", true, Some(&id)), &args(None, false)).unwrap();
        let hint = e.hint.unwrap();
        assert!(
            hint.contains(&id) && hint.contains("--fork --no-sandbox"),
            "{hint}"
        );
        let e = tmgr_fork_refusal(&session("claude-code", true, None), &args(None, false)).unwrap();
        assert!(e.message.contains("--no-sandbox") && e.hint.is_none());
        let id = uid(3);
        let e = tmgr_fork_refusal(&session("claude-code", true, Some(&id)), &args(None, false))
            .unwrap();
        assert!(e.hint.unwrap().contains(&id));
        let e = tmgr_fork_refusal(
            &session("claude-code", true, Some("x\u{1b}[2J")),
            &args(None, false),
        )
        .unwrap();
        assert!(e.hint.is_none());
    }

    #[test]
    fn the_request_pins_the_folder_the_plan_inspected() {
        use std::os::unix::fs::MetadataExt;
        let (tmp, e) = fixture();
        let p = plan(&e, false, true, None).unwrap();
        let meta = std::fs::metadata(tmp.path()).unwrap();
        assert_eq!(
            request(&p, None, (80, 24)).cwd_pin,
            Some(CwdPin {
                dev: meta.dev(),
                ino: meta.ino()
            })
        );
    }

    #[test]
    fn printed_session_data_loses_c1_and_bidi_characters() {
        let mut info = session("claude-code", true, Some("x\u{9b}y"));
        info.name = "a\u{9b}b\u{202e}c".into();
        info.cwd = "/w\u{2066}".into();
        info.argv = vec!["a\u{9b}".into()];
        let c = clean_info(&info);
        let text = serde_json::to_string(&c).unwrap();
        for bad in ['\u{9b}', '\u{202e}', '\u{2066}'] {
            assert!(!text.contains(bad), "{text}");
        }
        assert_eq!(c.name, "a?b?c");
    }

    #[test]
    fn a_live_or_unknown_state_is_refused_or_warned_for_a_known_conversation() {
        let (_t, mut e) = fixture();
        e.live = Live::Yes;
        assert!(check_live(&e, false).is_err());
        assert!(check_live(&e, true).unwrap().is_empty());
        e.live = Live::Recent;
        assert!(check_live(&e, false).unwrap()[0].contains("cannot be told"));
    }

    #[test]
    fn a_tmgr_session_named_like_a_conversation_id_never_captures_it() {
        let mut named = session("claude-code", false, None);
        named.name = uid(1);
        assert!(!names_tmgr_session(std::slice::from_ref(&named), &uid(1)));
        assert!(names_tmgr_session(&[named], "s_aaaaaaaaaa"));
    }

    #[test]
    fn the_plan_header_names_the_agent_the_id_and_the_folder() {
        let (_t, e) = fixture();
        let p = plan(&e, false, true, None).unwrap();
        let lines = plan_header(&p);
        assert_eq!(lines[0], "Resume this conversation?");
        assert!(lines[1].starts_with("  agent    ") && lines[1].contains(p.profile));
        assert!(lines[2].starts_with("  id       ") && lines[2].contains(&p.id));
        assert!(lines[3].starts_with("  folder   ") && lines[3].contains(&p.cwd));
        let p = plan(&e, true, true, None).unwrap();
        assert_eq!(plan_header(&p)[0], "Fork this conversation?");
    }

    #[test]
    fn a_conversation_that_went_live_while_waiting_is_refused_for_resume_and_allowed_for_fork() {
        let (_t, mut e) = fixture();
        assert!(recheck_entry(Some(e.clone()), None, false).is_ok());
        e.live = Live::Yes;
        let err = recheck_entry(Some(e.clone()), None, false).unwrap_err();
        assert_eq!(err.kind, Kind::Conflict);
        assert!(err.hint.unwrap().contains("--fork"));
        assert!(recheck_entry(Some(e), None, true).is_ok());
    }

    #[test]
    fn a_conversation_that_vanished_or_lost_consent_while_waiting_is_refused() {
        let err = recheck_entry(None, None, false).unwrap_err();
        assert_eq!(err.kind, Kind::NotFound);
        assert!(err.message.contains("disappeared while waiting"));
        let (_t, mut e) = fixture();
        e.readable = false;
        let off = Reads::default().hint();
        let err = recheck_entry(Some(e), off, true).unwrap_err();
        assert_eq!(err.kind, Kind::Forbidden);
        assert_eq!(err.hint.as_deref(), off);
    }

    #[test]
    fn a_running_tmgr_session_is_refused_before_anything_else() {
        let mut s = session("claude-code", true, Some(&uid(1)));
        s.state.kind = StateKind::Working;
        let called = std::cell::Cell::new(0);
        let known = |_: &str| {
            called.set(called.get() + 1);
            Ok(Vec::new())
        };
        let err = tmgr_resume_checks_with(&s, true, &known).unwrap_err();
        assert_eq!(err.kind, Kind::Conflict);
        assert!(err.message.contains("is still running"));
        assert_eq!(called.get(), 0);
    }

    #[test]
    fn the_known_conversation_is_checked_only_for_no_sandbox_and_a_valid_id() {
        let id = uid(2);
        let called = std::cell::RefCell::new(Vec::new());
        let known = |id: &str| {
            called.borrow_mut().push(id.to_string());
            Ok(vec!["warning: x".to_string()])
        };
        let s = session("claude-code", true, Some(&id));
        let (warnings, got) = tmgr_resume_checks_with(&s, true, &known).unwrap();
        assert_eq!(warnings, ["warning: x"]);
        assert_eq!(got.as_deref(), Some(id.as_str()));
        assert_eq!(*called.borrow(), std::slice::from_ref(&id));
        let (warnings, got) = tmgr_resume_checks_with(&s, false, &known).unwrap();
        assert!(warnings.is_empty() && got.is_none());
        let bad = session("claude-code", true, Some("x\u{1b}[2J"));
        let (_, got) = tmgr_resume_checks_with(&bad, true, &known).unwrap();
        assert!(got.is_none());
        assert_eq!(called.borrow().len(), 1);
        let refuse = |_: &str| Err(Error::new(Kind::Conflict, "live"));
        assert!(tmgr_resume_checks_with(&s, true, &refuse).is_err());
    }

    #[test]
    fn the_tmgr_resume_header_names_session_name_folder_and_the_conversation() {
        let mut s = session("claude-code", true, Some(&uid(3)));
        s.name = "work\u{1b}[2J".into();
        let plain = tmgr_resume_header(&s, false);
        assert_eq!(plain[0], "Resume this session?");
        assert!(plain[1].contains("s_aaaaaaaaaa") && plain[2].contains("work?[2J"));
        assert!(plain[3].contains("/w") && plain.len() == 4);
        let flagged = tmgr_resume_header(&s, true);
        assert_eq!(flagged[4], conversation_line(&s));
    }

    #[test]
    fn the_gate_after_confirmation_rechecks_the_conversation_and_the_runner_caps() {
        let id = uid(4);
        let caps = vec![
            caps::MANUAL_BYPASS_V1.to_string(),
            caps::NO_SANDBOX_V1.to_string(),
            caps::EXTERNAL_RESUME_V1.to_string(),
        ];
        let ok = |_: &str| Ok(Vec::new());
        assert!(resume_gate(Some(&id), &caps, Some(PermissionMode::Bypass), true, &ok).is_ok());
        let live = |_: &str| {
            Err(Error::new(
                Kind::Conflict,
                "this session is running in another terminal",
            ))
        };
        let err = resume_gate(Some(&id), &caps, None, true, &live).unwrap_err();
        assert_eq!(err.kind, Kind::Conflict);
        let err = resume_gate(None, &caps[..2], None, true, &live).unwrap_err();
        assert!(err.hint.unwrap().ends_with("runner restart"));
        let err =
            resume_gate(None, &caps[1..], Some(PermissionMode::Bypass), true, &ok).unwrap_err();
        assert!(err.hint.unwrap().ends_with("runner restart"));
        let err = resume_gate(None, &caps[..2], None, true, &ok).unwrap_err();
        assert!(err.message.contains("resume a conversation by id"));
    }

    #[test]
    fn fork_refusal_for_flags_matches_the_cli_wrapper() {
        let claude = session("claude-code", false, None);
        assert!(fork_refusal(&claude, false).is_none());
        let e = fork_refusal(&claude, true).unwrap();
        assert!(e.message.contains("keeps the settings"));
    }

    #[test]
    fn the_fork_job_is_built_from_the_session_and_the_runner_folders() {
        let cfg = RunnerConfig {
            dir: "/run".into(),
            homes: "/homes".into(),
            self_exe: "/tmgr".into(),
            quiet_ms: 0,
            version: "0".into(),
            max_sessions: 1,
        };
        let s = session("claude-code", false, None);
        let job = fork_job(&cfg, &s, fork_source(&s).unwrap());
        assert_eq!(job.id, "s_aaaaaaaaaa");
        assert!(job.exited && job.settled);
        assert_eq!(job.sandbox_home, Path::new("/homes/s_aaaaaaaaaa"));
        assert_eq!(job.log, Path::new("/run/sessions/s_aaaaaaaaaa/output.log"));
        let bare = session("shell", false, None);
        assert!(fork_source(&bare).is_err());
    }
}
