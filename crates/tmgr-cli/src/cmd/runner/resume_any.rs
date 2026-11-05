use super::attach_consent::Reads;
use super::attach_goto::{fork_call, fork_name};
use super::attach_questions::QuestionJob;
use super::attach_title::agent_src;
use super::catalog::{self, Entry, Live};
use super::ops::{clean, elevated, list_or_empty, resolve, run_create_run, terminal_size};
use super::{attach, connect, emit, rerr, runner_config, Opts, PermissionMode, ResumeArgs};
use std::io::IsTerminal;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{CwdPin, ExternalResume, RestoredRun, RunCreateReq, SessionInfo, StateKind};
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
    Ok(resolve(&list_or_empty(&cfg)?, key).is_ok())
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
    eprintln!(
        "{} this conversation?\n  agent    {}\n  id       {}\n  folder   {}",
        if p.fork { "Fork" } else { "Resume" },
        p.profile,
        p.id,
        clean(&p.cwd)
    );
    let bypass = a.mode == Some(PermissionMode::Bypass);
    super::privilege::confirm(bypass, true, false, false)?;
    let again = catalog::find(&env, &a.session).ok_or_else(|| {
        Error::not_found("the conversation disappeared while waiting for confirmation")
    })?;
    check_readable(&again, Reads::of(&env).hint())?;
    check_live(&again, a.fork)?;
    let cfg = runner_config()?;
    daemon::start_background(&cfg).map_err(rerr)?;
    let mut client = connect(&cfg)?;
    let caps = client.welcome().caps.clone();
    super::privilege::check_caps(&caps, bypass, true)?;
    super::privilege::check_external_cap(&caps)?;
    let info = client
        .create_run(request(&p, a.mode, terminal_size()))
        .map_err(rerr)?;
    attach_or_report(o, &cfg, a, &info)
}

fn tmgr_fork_refusal(s: &SessionInfo, a: &ResumeArgs) -> Option<Error> {
    if a.mode.is_some() || a.no_sandbox {
        return Some(
            Error::usage("--fork of a tmgr session keeps the settings it was started with")
                .hint("repeat without --mode and --no-sandbox"),
        );
    }
    if s.profile.as_deref() != Some("claude-code") {
        return Some(Error::new(
            Kind::Forbidden,
            "only Claude Code sessions can be forked from a tmgr session; forking a Codex session is not supported yet",
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

pub(super) fn fork_tmgr(o: &Opts, a: &ResumeArgs, s: &SessionInfo) -> Result<i32> {
    if let Some(e) = tmgr_fork_refusal(s, a) {
        return Err(e);
    }
    let src = agent_src(s).ok_or_else(|| Error::usage("this session has no agent to fork"))?;
    let cfg = runner_config()?;
    let job = QuestionJob {
        id: s.id.clone(),
        pid: src.pid,
        src,
        sandbox_home: cfg.homes.join(&s.id),
        log: cfg.dir.join("sessions").join(&s.id).join("output.log"),
        exited: s.state.kind == StateKind::Exited,
        settled: !matches!(s.state.kind, StateKind::Working | StateKind::Starting),
    };
    let sessions = connect(&cfg)?.list().map_err(rerr)?;
    let names: Vec<&str> = sessions.iter().map(|x| x.name.as_str()).collect();
    let name = fork_name(&names, &s.name);
    let run = fork_call(&cfg, &job, &name, terminal_size(), Reads::current())
        .map_err(|e| Error::new(Kind::Conflict, e))?;
    attach_or_report(o, &cfg, a, &run.session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::runner::catalog::testkit::{entry, uid};
    use crate::cmd::runner::catalog::Agent;

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
        }
    }

    #[test]
    fn forking_a_tmgr_session_refuses_flags_codex_and_unsandboxed_sessions() {
        let claude = session("claude-code", false, None);
        assert!(tmgr_fork_refusal(&claude, &args(None, false)).is_none());
        for a in [args(Some(PermissionMode::Bypass), false), args(None, true)] {
            let e = tmgr_fork_refusal(&claude, &a).unwrap();
            assert!(e.message.contains("keeps the settings"), "{}", e.message);
        }
        let e = tmgr_fork_refusal(&session("codex", false, None), &args(None, false)).unwrap();
        assert!(e.message.contains("Codex"), "{}", e.message);
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
}
