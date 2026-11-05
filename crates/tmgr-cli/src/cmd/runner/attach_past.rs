use super::attach_new::{privileged_ask, task_err, SpawnErr};
use super::catalog::{self, Entry};
use super::menu::Pick;
use super::resume_any::{
    create_external, fork_job, fork_refusal, fork_run, fork_source, plan, plan_header,
    recheck_external, resume_flagged, tmgr_resume_checks_with, tmgr_resume_header, Plan,
};
use super::{connect, rerr, PermissionMode};
use std::time::Duration;
use tmgr_core::error::{Error, Result};
use tmgr_ipc::api::{RestoredRun, SessionInfo};
use tmgr_link::privileged;
use tmgr_runner::attach::PrivAsk;
use tmgr_runner::daemon::RunnerConfig;
use tmgr_usage::paths::Env;

use super::attach_consent::Reads;

pub(super) const START_WAIT: Duration = Duration::from_secs(30);

type Asked = std::result::Result<PrivAsk, (String, Option<String>)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shape {
    External,
    Resume,
    ResumeFlagged,
    Fork,
}

pub(super) fn shape(p: &Pick) -> Shape {
    match (p.tmgr, p.fork) {
        (false, _) => Shape::External,
        (true, true) => Shape::Fork,
        (true, false) if p.bypass || p.no_sandbox => Shape::ResumeFlagged,
        (true, false) => Shape::Resume,
    }
}

pub(super) enum Go {
    External {
        query: String,
        plan: Plan,
        mode: Option<PermissionMode>,
    },
    Tmgr {
        session: Box<SessionInfo>,
        known: Option<String>,
        mode: Option<PermissionMode>,
        no_sandbox: bool,
    },
}

pub(super) struct Ready {
    pub go: Go,
    pub lines: Vec<String>,
    pub ask: PrivAsk,
}

pub(super) struct Deps<'a> {
    pub guard: &'a dyn Fn() -> Result<()>,
    pub entry: &'a dyn Fn(&str) -> (Option<Entry>, Option<&'static str>),
    pub session: &'a dyn Fn(&str) -> Result<SessionInfo>,
    pub known: &'a dyn Fn(&str) -> Result<Vec<String>>,
    pub ask: &'a dyn Fn(bool, bool) -> Asked,
}

fn asked(e: (String, Option<String>)) -> SpawnErr {
    SpawnErr {
        message: e.0,
        hint: e.1,
    }
}

fn mode_of(bypass: bool) -> Option<PermissionMode> {
    bypass.then_some(PermissionMode::Bypass)
}

fn external_ready(
    found: (Option<Entry>, Option<&'static str>),
    pick: &Pick,
    ask: &dyn Fn(bool, bool) -> Asked,
) -> std::result::Result<Ready, SpawnErr> {
    let (entry, denial) = found;
    let entry = entry.ok_or_else(|| {
        task_err(Error::not_found(
            "no recorded conversation matches this session",
        ))
    })?;
    let p = plan(&entry, pick.fork, pick.no_sandbox, denial).map_err(task_err)?;
    let mut lines = p.warnings.clone();
    lines.extend(plan_header(&p));
    let ask = ask(pick.bypass, true).map_err(asked)?;
    Ok(Ready {
        go: Go::External {
            query: pick.id.clone(),
            plan: p,
            mode: mode_of(pick.bypass),
        },
        lines,
        ask,
    })
}

fn tmgr_ready(
    session: SessionInfo,
    pick: &Pick,
    known: &dyn Fn(&str) -> Result<Vec<String>>,
    ask: &dyn Fn(bool, bool) -> Asked,
) -> std::result::Result<Ready, SpawnErr> {
    let (warnings, known) =
        tmgr_resume_checks_with(&session, pick.no_sandbox, known).map_err(task_err)?;
    let mut lines = warnings;
    lines.extend(tmgr_resume_header(&session, pick.no_sandbox));
    let ask = ask(pick.bypass, pick.no_sandbox).map_err(asked)?;
    Ok(Ready {
        go: Go::Tmgr {
            session: Box::new(session),
            known,
            mode: mode_of(pick.bypass),
            no_sandbox: pick.no_sandbox,
        },
        lines,
        ask,
    })
}

pub(super) fn prepare_with(pick: &Pick, deps: &Deps) -> std::result::Result<Ready, SpawnErr> {
    (deps.guard)().map_err(task_err)?;
    match shape(pick) {
        Shape::External => external_ready((deps.entry)(&pick.id), pick, deps.ask),
        Shape::ResumeFlagged => {
            let session = (deps.session)(&pick.id).map_err(task_err)?;
            tmgr_ready(session, pick, deps.known, deps.ask)
        }
        Shape::Resume | Shape::Fork => Err(SpawnErr {
            message: "nothing to confirm".into(),
            hint: None,
        }),
    }
}

fn session_by_id(cfg: &RunnerConfig, id: &str) -> Result<SessionInfo> {
    let sessions = connect(cfg)?.list().map_err(rerr)?;
    sessions
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| Error::not_found("that session is no longer known to the runner"))
}

pub(super) fn prepare(cfg: &RunnerConfig, pick: &Pick) -> std::result::Result<Ready, SpawnErr> {
    let find = |id: &str| {
        let env = Env::from_process();
        (catalog::find(&env, id), Reads::of(&env).hint())
    };
    prepare_with(
        pick,
        &Deps {
            guard: &privileged::guard,
            entry: &find,
            session: &|id| session_by_id(cfg, id),
            known: &|id| super::resume_any::check_known(id, false),
            ask: &privileged_ask,
        },
    )
}

fn checked<T>(
    guard: &dyn Fn() -> Result<()>,
    recheck: &dyn Fn() -> Result<()>,
    create: impl FnOnce() -> Result<T>,
) -> Result<T> {
    guard()?;
    recheck()?;
    create()
}

pub(super) fn run_external(
    cfg: &RunnerConfig,
    query: &str,
    p: &Plan,
    mode: Option<PermissionMode>,
    size: (u16, u16),
) -> std::result::Result<SessionInfo, SpawnErr> {
    checked(
        &privileged::guard,
        &|| recheck_external(&Env::from_process(), query, p.fork),
        || create_external(cfg, p, mode, size, Some(START_WAIT)),
    )
    .map_err(task_err)
}

pub(super) fn run_tmgr_resume(
    cfg: &RunnerConfig,
    session: &SessionInfo,
    known: Option<&str>,
    mode: Option<PermissionMode>,
    no_sandbox: bool,
) -> std::result::Result<RestoredRun, SpawnErr> {
    let go = || {
        privileged::guard()?;
        let mut client = connect(cfg)?;
        let _ = client.set_timeouts(START_WAIT);
        resume_flagged(&mut client, session, known, mode, no_sandbox)
    };
    go().map_err(task_err)
}

pub(super) fn run_fork(
    cfg: &RunnerConfig,
    id: &str,
    size: (u16, u16),
) -> std::result::Result<(RestoredRun, String), SpawnErr> {
    let go = || {
        privileged::guard()?;
        let s = session_by_id(cfg, id)?;
        if let Some(e) = fork_refusal(&s, false) {
            return Err(e);
        }
        let job = fork_job(cfg, &s, fork_source(&s)?);
        fork_run(cfg, &job, &s, size)
    };
    go().map_err(task_err)
}

pub(super) fn spawn_notice(e: &SpawnErr) -> String {
    match &e.hint {
        Some(hint) => format!("{}: {hint}", e.message),
        None => e.message.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::runner::catalog::testkit::{entry, uid};
    use crate::cmd::runner::catalog::{Agent, Live};
    use std::cell::{Cell, RefCell};
    use tmgr_ipc::api::StateKind;

    fn pick(tmgr: bool, fork: bool, bypass: bool, no_sandbox: bool) -> Pick {
        Pick {
            id: uid(1),
            fork,
            tmgr,
            bypass,
            no_sandbox,
        }
    }

    fn at(dir: &std::path::Path) -> Entry {
        let mut e = entry(Agent::Claude);
        e.cwd = Some(std::fs::canonicalize(dir).unwrap().display().to_string());
        e
    }

    fn ask_ok(calls: &RefCell<Vec<(bool, bool)>>) -> impl Fn(bool, bool) -> Asked + '_ {
        move |bypass, no_sandbox| {
            calls.borrow_mut().push((bypass, no_sandbox));
            Ok(PrivAsk {
                word: "no-sandbox",
                prompt: "Type no-sandbox to continue:".into(),
                warnings: vec!["warning: no sandbox"],
            })
        }
    }

    #[test]
    fn a_confirmation_is_demanded_exactly_when_bypass_or_no_sandbox_is_set() {
        for fork in [false, true] {
            for bypass in [false, true] {
                for no_sandbox in [false, true] {
                    let s = shape(&pick(true, fork, bypass, no_sandbox));
                    if fork {
                        assert_eq!(s, Shape::Fork);
                    } else if bypass || no_sandbox {
                        assert_eq!(s, Shape::ResumeFlagged);
                    } else {
                        assert_eq!(s, Shape::Resume);
                    }
                }
            }
            assert_eq!(shape(&pick(false, fork, false, true)), Shape::External);
        }
    }

    #[test]
    fn an_external_pick_lists_warnings_then_details_and_asks_once_for_the_word() {
        let tmp = tempfile::tempdir().unwrap();
        let mut e = at(tmp.path());
        e.live = Live::Recent;
        let calls = RefCell::new(Vec::new());
        let ready = external_ready(
            (Some(e), None),
            &pick(false, false, true, true),
            &ask_ok(&calls),
        )
        .unwrap();
        assert_eq!(*calls.borrow(), [(true, true)]);
        assert!(ready.lines[0].starts_with("warning:"));
        assert_eq!(ready.lines[1], "Resume this conversation?");
        assert_eq!(ready.lines.len(), 5);
        assert_eq!(ready.ask.word, "no-sandbox");
        let Go::External { query, plan, mode } = ready.go else {
            panic!("external");
        };
        assert_eq!(query, uid(1));
        assert!(!plan.fork && mode == Some(PermissionMode::Bypass));
    }

    #[test]
    fn refusals_happen_before_any_prompt_and_keep_their_hints() {
        let tmp = tempfile::tempdir().unwrap();
        let calls = RefCell::new(Vec::new());
        let ask = ask_ok(&calls);
        let mut live = at(tmp.path());
        live.live = Live::Yes;
        let err = external_ready(
            (Some(live.clone()), None),
            &pick(false, false, false, true),
            &ask,
        )
        .err()
        .unwrap();
        assert!(err.hint.unwrap().contains("--fork"));
        assert!(external_ready((Some(live), None), &pick(false, true, false, true), &ask).is_ok());
        calls.borrow_mut().clear();
        let mut gone = at(tmp.path());
        gone.cwd = Some(tmp.path().join("gone").display().to_string());
        let mut hidden = at(tmp.path());
        hidden.readable = false;
        let off = Reads::default().hint();
        let cases = [
            (Some(gone), None, true),
            (Some(hidden), off, true),
            (Some(at(tmp.path())), None, false),
            (None, None, true),
        ];
        for (found, denial, no_sandbox) in cases {
            let err = external_ready(
                (found, denial),
                &pick(false, false, false, no_sandbox),
                &ask,
            )
            .err()
            .unwrap();
            assert!(!err.message.is_empty());
            if denial.is_some() {
                assert_eq!(err.hint.as_deref(), denial);
            }
        }
        assert!(calls.borrow().is_empty());
    }

    #[test]
    fn a_refused_privileged_prompt_keeps_its_message_and_hint() {
        let tmp = tempfile::tempdir().unwrap();
        let refuse = |_: bool, _: bool| -> Asked {
            Err((
                "privileged change refused inside an agent session".into(),
                Some("run it yourself in a normal terminal".into()),
            ))
        };
        let err = external_ready(
            (Some(at(tmp.path())), None),
            &pick(false, false, false, true),
            &refuse,
        )
        .err()
        .unwrap();
        assert!(err.message.contains("refused inside an agent session"));
        assert_eq!(
            err.hint.as_deref(),
            Some("run it yourself in a normal terminal")
        );
        assert!(spawn_notice(&err).ends_with("run it yourself in a normal terminal"));
    }

    fn exited(profile: &str, agent_id: Option<&str>) -> SessionInfo {
        serde_json::from_value(serde_json::json!({
            "id": uid(1), "name": "w", "argv": ["/bin/sh"], "cwd": "/w",
            "profile": profile, "state": {"kind": "exited", "since_ms": 1},
            "created_ms": 1, "cols": 80, "rows": 24, "shim_pid": 1, "child_pid": 2,
            "no_sandbox": false, "agent_session_id": agent_id,
        }))
        .unwrap()
    }

    #[test]
    fn a_flagged_tmgr_resume_refuses_a_running_session_before_the_prompt() {
        let calls = RefCell::new(Vec::new());
        let mut s = exited("claude-code", None);
        s.state.kind = StateKind::Working;
        let known = |_: &str| Ok(Vec::new());
        let err = tmgr_ready(s, &pick(true, false, true, false), &known, &ask_ok(&calls))
            .err()
            .unwrap();
        assert!(err.message.contains("is still running"));
        assert!(calls.borrow().is_empty());
    }

    #[test]
    fn a_flagged_tmgr_resume_shows_the_warnings_the_details_and_asks_with_its_flags() {
        let calls = RefCell::new(Vec::new());
        let id = uid(9);
        let s = exited("claude-code", Some(&id));
        let known = |_: &str| Ok(vec!["warning: may still be running".to_string()]);
        let ready =
            tmgr_ready(s, &pick(true, false, false, true), &known, &ask_ok(&calls)).unwrap();
        assert_eq!(*calls.borrow(), [(false, true)]);
        assert_eq!(ready.lines[0], "warning: may still be running");
        assert_eq!(ready.lines[1], "Resume this session?");
        assert!(ready.lines.last().unwrap().contains(&id));
        let Go::Tmgr {
            known,
            no_sandbox,
            mode,
            ..
        } = ready.go
        else {
            panic!("tmgr");
        };
        assert_eq!(known.as_deref(), Some(id.as_str()));
        assert!(no_sandbox && mode.is_none());
    }

    #[test]
    fn prepare_runs_the_guard_first_and_touches_nothing_when_it_refuses() {
        let touched = Cell::new(0);
        let guard = || Err(Error::new(tmgr_core::error::Kind::Forbidden, "no"));
        let entry_of = |_: &str| {
            touched.set(touched.get() + 1);
            (None, None)
        };
        let session = |_: &str| {
            touched.set(touched.get() + 1);
            Err(Error::not_found("x"))
        };
        let known = |_: &str| {
            touched.set(touched.get() + 1);
            Ok(Vec::new())
        };
        let ask = |_: bool, _: bool| -> Asked {
            touched.set(touched.get() + 1);
            Err(("x".into(), None))
        };
        let deps = Deps {
            guard: &guard,
            entry: &entry_of,
            session: &session,
            known: &known,
            ask: &ask,
        };
        for p in [
            pick(false, false, false, true),
            pick(true, false, true, false),
        ] {
            let err = prepare_with(&p, &deps).err().unwrap();
            assert_eq!(err.message, "no");
        }
        assert_eq!(touched.get(), 0);
    }

    #[test]
    fn prepare_has_nothing_to_confirm_for_a_plain_resume_or_a_fork() {
        let ok = || Ok(());
        let entry_of = |_: &str| (None, None);
        let session = |_: &str| Err(Error::not_found("x"));
        let known = |_: &str| Ok(Vec::new());
        let ask = |_: bool, _: bool| -> Asked { Err(("x".into(), None)) };
        let deps = Deps {
            guard: &ok,
            entry: &entry_of,
            session: &session,
            known: &known,
            ask: &ask,
        };
        for p in [
            pick(true, false, false, false),
            pick(true, true, false, false),
        ] {
            assert_eq!(
                prepare_with(&p, &deps).err().unwrap().message,
                "nothing to confirm"
            );
        }
    }

    #[test]
    fn the_checks_after_the_confirmation_run_in_order_and_stop_the_create() {
        let order = RefCell::new(Vec::new());
        let guard = |ok: bool| {
            let order = &order;
            move || {
                order.borrow_mut().push("guard");
                if ok {
                    Ok(())
                } else {
                    Err(Error::new(tmgr_core::error::Kind::Forbidden, "guard"))
                }
            }
        };
        let recheck = |ok: bool| {
            let order = &order;
            move || {
                order.borrow_mut().push("recheck");
                if ok {
                    Ok(())
                } else {
                    Err(Error::new(tmgr_core::error::Kind::Conflict, "went live"))
                }
            }
        };
        let create = || {
            order.borrow_mut().push("create");
            Ok(1)
        };
        assert_eq!(checked(&guard(true), &recheck(true), create).unwrap(), 1);
        assert_eq!(*order.borrow(), ["guard", "recheck", "create"]);
        order.borrow_mut().clear();
        let err = checked(&guard(true), &recheck(false), create).unwrap_err();
        assert_eq!(err.message, "went live");
        assert_eq!(*order.borrow(), ["guard", "recheck"]);
        order.borrow_mut().clear();
        assert!(checked(&guard(false), &recheck(true), create).is_err());
        assert_eq!(*order.borrow(), ["guard"]);
    }

    #[test]
    fn a_notice_without_a_hint_is_the_bare_message() {
        let e = SpawnErr {
            message: "m".into(),
            hint: None,
        };
        assert_eq!(spawn_notice(&e), "m");
    }
}
