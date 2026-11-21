use super::*;
use tmgr_ipc::api::{ManagerUnrestrictedReq, NoSandbox};

const PIN_INTERVAL: Duration = Duration::from_secs(2);

pub(super) struct PinGrant(());

fn wipe(s: String) {
    let mut b = s.into_bytes();
    b.fill(0);
    std::hint::black_box(&b);
}

fn refuse(code: ErrorCode, msg: &str) -> ApiErr {
    api(code, msg)
}

fn structural(run: &RunCreateReq) -> Result<(), ApiErr> {
    if !run.manager {
        return Err(refuse(
            ErrorCode::InvalidParams,
            "an unrestricted start is only for a manager",
        ));
    }
    if run.origin.is_some() {
        return Err(refuse(
            ErrorCode::Forbidden,
            "an unrestricted manager is not started for a link",
        ));
    }
    if run.no_sandbox.is_some() {
        return Err(refuse(
            ErrorCode::InvalidParams,
            "no_sandbox is set by the runner for an unrestricted manager, not by the client",
        ));
    }
    if !matches!(run.permission_mode.as_deref(), None | Some("ask")) {
        return Err(refuse(
            ErrorCode::InvalidParams,
            "the permission mode of an unrestricted manager is set by the runner",
        ));
    }
    if run.profile != "claude-code" {
        return Err(refuse(
            ErrorCode::InvalidParams,
            "an unrestricted manager runs only with claude-code",
        ));
    }
    if run.spawn_card.is_some()
        || run.external_resume.is_some()
        || run.parent_session.is_some()
        || run.trust_cwd.is_some()
        || run.trust_via_repo.is_some()
        || run.cloud.is_some()
        || run.permission_cards
    {
        return Err(refuse(
            ErrorCode::InvalidParams,
            "an unrestricted manager takes no extra run options",
        ));
    }
    Ok(())
}

fn unrestricted_run(mut run: RunCreateReq) -> RunCreateReq {
    run.no_sandbox = Some(NoSandbox::default());
    run.permission_mode = Some("bypass".into());
    run
}

impl Runner {
    pub(super) fn manager_unrestricted(
        self: &Arc<Self>,
        client: &str,
        p: &Value,
    ) -> Result<SessionInfo, ApiErr> {
        let (run, grant) = self.authorize_unrestricted(client, p)?;
        let (info, ..) = self.create_run_as(run, true, None, None, None, Some(&grant))?;
        log(&format!(
            "manager unrestricted start: session {}, profile claude-code, mode: bypass, sandbox: off, pin ok",
            info.id
        ));
        Ok(info)
    }

    fn authorize_unrestricted(
        &self,
        client: &str,
        p: &Value,
    ) -> Result<(RunCreateReq, PinGrant), ApiErr> {
        if client != CLIENT_CLI {
            return Err(refuse(
                ErrorCode::Forbidden,
                "manager.unrestricted is accepted only from the tmgr CLI",
            ));
        }
        let ManagerUnrestrictedReq { run, pin } = params(p)?;
        let checked = self.precheck_unrestricted(run);
        let run = match checked {
            Ok(run) => run,
            Err(e) => {
                wipe(pin);
                return Err(e);
            }
        };
        let verdict = (|| {
            let Some(check) = self.cfg.pin_check.clone() else {
                return Err(refuse(
                    ErrorCode::Forbidden,
                    "this runner cannot check the follower PIN",
                ));
            };
            {
                let mut last = lock(&self.mgr.pin_last);
                if last.is_some_and(|t| t.elapsed() < PIN_INTERVAL) {
                    return Err(refuse(ErrorCode::Busy, "try again in a moment"));
                }
                *last = Some(Instant::now());
            }
            check(&pin).map_err(|e| {
                refuse(
                    ErrorCode::Internal,
                    &format!("cannot check the follower PIN: {e}"),
                )
            })
        })();
        wipe(pin);
        match verdict? {
            PinVerdict::Ok => Ok((unrestricted_run(run), PinGrant(()))),
            PinVerdict::Wrong { left } => {
                log("manager unrestricted start refused: pin wrong");
                Err(refuse(
                    ErrorCode::Forbidden,
                    &format!("wrong PIN, {left} tries left"),
                ))
            }
            PinVerdict::Locked => {
                log("manager unrestricted start refused: pin locked");
                Err(refuse(
                    ErrorCode::Forbidden,
                    "the PIN is locked after too many wrong tries; reset it with `tmgr link pin reset`",
                ))
            }
            PinVerdict::Unset => Err(refuse(
                ErrorCode::Forbidden,
                "no follower PIN is set; set one with `tmgr link pin set`",
            )),
        }
    }

    fn precheck_unrestricted(&self, run: RunCreateReq) -> Result<RunCreateReq, ApiErr> {
        structural(&run)?;
        let probe = unrestricted_run(run.clone());
        crate::manager::check_create(&probe, true, self.live_manager().is_some(), true)
            .map_err(|(c, m)| api(c, m))?;
        Ok(run)
    }
}

#[cfg(test)]
mod tests {
    use super::super::manager::tests::{add, runner_with};
    use super::*;
    use serde_json::json;
    use std::sync::atomic::AtomicUsize;

    struct Env {
        _t: tempfile::TempDir,
        rt: Arc<Runner>,
        calls: Arc<AtomicUsize>,
    }

    fn env(verdict: Option<Result<PinVerdict, String>>) -> Env {
        let t = tempfile::Builder::new()
            .prefix("un")
            .tempdir_in("/tmp")
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let check: Option<PinCheck> = verdict.map(|v| {
            Arc::new(move |_: &str| {
                seen.fetch_add(1, Ordering::SeqCst);
                v.clone()
            }) as PinCheck
        });
        let rt = runner_with(t.path(), check);
        Env { _t: t, rt, calls }
    }

    fn body(extra: Value) -> Value {
        let mut run = json!({"profile": "claude-code", "cwd": "/tmp", "manager": true});
        run.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        json!({"run": run, "pin": "correct horse"})
    }

    fn ask(e: &Env, client: &str, p: Value) -> Result<(RunCreateReq, PinGrant), ApiErr> {
        e.rt.authorize_unrestricted(client, &p)
    }

    fn refused(r: Result<(RunCreateReq, PinGrant), ApiErr>) -> ApiErr {
        match r {
            Ok(_) => panic!("expected a refusal"),
            Err(e) => e,
        }
    }

    fn ok_env() -> Env {
        env(Some(Ok(PinVerdict::Ok)))
    }

    #[test]
    fn only_the_cli_may_ask() {
        let e = ok_env();
        for client in ["desktop", "worker", "other"] {
            let err = refused(ask(&e, client, body(json!({}))));
            assert_eq!(err.code, ErrorCode::Forbidden, "{client}");
        }
        assert_eq!(e.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_doomed_request_never_reaches_the_checker() {
        let e = ok_env();
        let doomed = [
            json!({"manager": false}),
            json!({"origin": {"link": "l", "generation": 1}}),
            json!({"no_sandbox": {}}),
            json!({"permission_mode": "bypass"}),
            json!({"permission_mode": "accept-edits"}),
            json!({"profile": "codex"}),
            json!({"spawn_card": "card_1"}),
            json!({"parent_session": "s_aaaaaaaaaa"}),
            json!({"start_key": "k"}),
            json!({"unattended": true}),
        ];
        for extra in doomed {
            refused(ask(&e, CLIENT_CLI, body(extra.clone())));
            assert_eq!(e.calls.load(Ordering::SeqCst), 0, "{extra}");
        }
    }

    #[test]
    fn a_refused_codex_says_why() {
        let e = ok_env();
        let err = refused(ask(&e, CLIENT_CLI, body(json!({"profile": "codex"}))));
        assert!(err.msg.contains("only with claude-code"), "{}", err.msg);
    }

    #[test]
    fn a_live_manager_is_refused_before_the_checker() {
        let e = ok_env();
        add(&e.rt, e._t.path(), "s_aaaaaaaaaa", true);
        let err = refused(ask(&e, CLIENT_CLI, body(json!({}))));
        assert_eq!(err.code, ErrorCode::Busy);
        assert_eq!(e.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_runner_without_a_checker_refuses() {
        let e = env(None);
        let err = refused(ask(&e, CLIENT_CLI, body(json!({}))));
        assert!(
            err.msg.contains("cannot check the follower PIN"),
            "{}",
            err.msg
        );
    }

    #[test]
    fn the_verdicts_have_their_own_messages() {
        let cases = [
            (PinVerdict::Wrong { left: 3 }, "wrong PIN, 3 tries left"),
            (PinVerdict::Locked, "tmgr link pin reset"),
            (PinVerdict::Unset, "tmgr link pin set"),
        ];
        for (verdict, text) in cases {
            let e = env(Some(Ok(verdict)));
            let err = refused(ask(&e, CLIENT_CLI, body(json!({}))));
            assert_eq!(err.code, ErrorCode::Forbidden);
            assert!(err.msg.contains(text), "{}", err.msg);
            assert!(!err.msg.contains("correct horse"));
            assert_eq!(e.calls.load(Ordering::SeqCst), 1);
        }
        let e = env(Some(Err("disk".into())));
        let err = refused(ask(&e, CLIENT_CLI, body(json!({}))));
        assert_eq!(err.code, ErrorCode::Internal);
    }

    #[test]
    fn a_second_check_within_two_seconds_is_refused_without_calling_the_checker() {
        let e = env(Some(Ok(PinVerdict::Wrong { left: 4 })));
        refused(ask(&e, CLIENT_CLI, body(json!({}))));
        let err = refused(ask(&e, CLIENT_CLI, body(json!({}))));
        assert_eq!(err.code, ErrorCode::Busy);
        assert_eq!(err.msg, "try again in a moment");
        assert_eq!(e.calls.load(Ordering::SeqCst), 1);
        *lock(&e.rt.mgr.pin_last) = Some(Instant::now() - PIN_INTERVAL * 2);
        refused(ask(&e, CLIENT_CLI, body(json!({}))));
        assert_eq!(e.calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_good_pin_grants_the_unsandboxed_bypass_run() {
        let e = ok_env();
        let (run, _grant) = ask(&e, CLIENT_CLI, body(json!({}))).ok().unwrap();
        assert_eq!(e.calls.load(Ordering::SeqCst), 1);
        assert!(run.manager);
        assert_eq!(run.no_sandbox, Some(NoSandbox::default()));
        assert_eq!(run.permission_mode.as_deref(), Some("bypass"));
        let ask_mode = body(json!({"permission_mode": "ask"}));
        assert!(ask(&env(Some(Ok(PinVerdict::Ok))), CLIENT_CLI, ask_mode).is_ok());
    }

    #[test]
    fn the_session_record_of_an_unrestricted_manager_holds_no_pin() {
        let meta: Meta = serde_json::from_value(json!({
            "id": "s_aaaaaaaaaa", "name": "m", "argv": ["claude"], "cwd": "/tmp",
            "profile": "claude-code", "created_ms": 0, "cols": 100, "rows": 24,
            "shim_pid": 1, "child_pid": 2, "manager": true, "unrestricted": true,
            "no_sandbox": true, "permission_mode": "bypass",
        }))
        .unwrap();
        let text = serde_json::to_string(&meta).unwrap();
        assert!(text.contains("\"unrestricted\":true"));
        assert!(!text.contains("correct horse"));
        let plain: Meta = serde_json::from_value(json!({
            "id": "s_aaaaaaaaaa", "name": "m", "argv": ["claude"], "cwd": "/tmp",
            "profile": "claude-code", "created_ms": 0, "cols": 100, "rows": 24,
            "shim_pid": 1, "child_pid": 2,
        }))
        .unwrap();
        assert!(!serde_json::to_string(&plain)
            .unwrap()
            .contains("unrestricted"));
    }

    #[test]
    fn the_request_never_prints_its_pin() {
        let req: ManagerUnrestrictedReq = serde_json::from_value(body(json!({}))).unwrap();
        assert!(!format!("{req:?}").contains("correct horse"));
    }

    #[test]
    fn the_wire_method_is_refused_for_a_desktop_connection() {
        let e = ok_env();
        let err =
            e.rt.manager_unrestricted("desktop", &body(json!({})))
                .unwrap_err();
        assert_eq!(err.code, ErrorCode::Forbidden);
        assert_eq!(e.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn creating_with_a_grant_needs_a_manager_request() {
        let e = ok_env();
        let req: RunCreateReq = serde_json::from_value(
            json!({"profile": "claude-code", "cwd": "/tmp", "manager": false}),
        )
        .unwrap();
        let err =
            e.rt.create_run_as(req, true, None, None, None, Some(&PinGrant(())))
                .unwrap_err();
        assert!(err.msg.contains("only for a manager"), "{}", err.msg);
    }
}
