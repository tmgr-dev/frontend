use super::handoff::{supports, switch_err};
use super::*;
use crate::handoff_store::HandoffStore;
use crate::switch::{self as sw, Carry, SwitchError, Target};
use tmgr_ipc::api::{SwitchReq, SwitchResp};

const INT_WAIT: Duration = Duration::from_secs(10);
const TERM_WAIT: Duration = Duration::from_secs(5);
const SCREEN_LINES: u32 = 400;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SwitchOpts {
    pub keep_bypass: bool,
    pub manual: bool,
}

#[derive(Debug)]
pub(crate) struct SwitchOut {
    pub new_session: SessionInfo,
    pub packet_id: String,
    pub warnings: Vec<String>,
}

struct Plan {
    session: Arc<Session>,
    target: Target,
    carry: Carry,
}

pub(super) struct SwitchGuard<'a> {
    rt: &'a Runner,
    id: String,
}

impl Drop for SwitchGuard<'_> {
    fn drop(&mut self) {
        lock(&self.rt.switch_lock).remove(&self.id);
    }
}

fn refuse(m: impl Into<String>) -> ApiErr {
    api(ErrorCode::InvalidParams, m)
}

impl Runner {
    pub(super) fn switch_dispatch(
        self: &Arc<Self>,
        conn: &Conn,
        p: &Value,
    ) -> Result<Value, ApiErr> {
        if conn.client != CLIENT_CLI && conn.client != CLIENT_DESKTOP {
            return Err(api(
                ErrorCode::Forbidden,
                "sessions.switch is accepted only from the tmgr CLI or the desktop app",
            ));
        }
        if p.get("permission_mode").and_then(Value::as_str) == Some("bypass")
            || p.get("keep_bypass").and_then(Value::as_bool) == Some(true)
        {
            return Err(refuse("bypass is never carried by a switch"));
        }
        let req: SwitchReq = params(p)?;
        let to = Target::of_profile(Some(&req.to))
            .ok_or_else(|| refuse("to must be a built-in claude-code, codex, opencode or agy"))?;
        let out = self.switch_session(
            &req.session,
            to,
            SwitchOpts {
                keep_bypass: false,
                manual: false,
            },
        )?;
        Ok(to_json(&SwitchResp {
            new_session: out.new_session,
            packet_id: out.packet_id,
            warnings: out.warnings,
        }))
    }

    pub(crate) fn switch_session(
        self: &Arc<Self>,
        old_id: &str,
        to: Target,
        opts: SwitchOpts,
    ) -> Result<SwitchOut, ApiErr> {
        let _guard = self.switch_guard(old_id)?;
        let plan = self.switch_preflight(old_id, to, opts)?;
        let ran = self.stop_and_wait(&plan.session, plan.target, INT_WAIT, TERM_WAIT)?;
        let old = plan.session.info();
        if !ran {
            self.require_packet(&old, plan.target)?;
        }
        let screen = if sw::has_log(&old) {
            Vec::new()
        } else {
            plan.session.text(Some(SCREEN_LINES)).lines
        };
        let sessions: Vec<SessionInfo> = self.all().iter().map(|x| x.info()).collect();
        let inputs =
            sw::prepare_inputs(&old, &sessions, plan.target, None, &screen).map_err(switch_err)?;
        let store = HandoffStore::open(&self.cfg.dir)
            .map_err(|e| api(ErrorCode::Internal, e.to_string()))?;
        let mut random = [0u8; 10];
        getrandom::fill(&mut random).map_err(|e| api(ErrorCode::Internal, e.to_string()))?;
        let handoff = sw::build_handoff(
            &store,
            &self.chat,
            &old,
            &inputs,
            plan.target,
            &plan.carry,
            now_ms(),
            random,
        )
        .map_err(switch_err)?;
        let new_session = match self.started_from(
            &store,
            &old.id,
            plan.target.profile(),
            handoff.packet.created_ms,
        ) {
            Some(existing) => existing,
            None => self.start_switched(&old, &plan, &handoff.prompt)?,
        };
        let value = serde_json::to_value(&handoff.packet)
            .map_err(|e| api(ErrorCode::Internal, e.to_string()))?;
        let mut warnings = self.finish_handoff(
            &store,
            &new_session.id,
            &value,
            &handoff.packet,
            plan.target,
        );
        if plan.carry.fell_back {
            warnings.push(format!(
                "{} cannot run the old permission mode; the new session started in ask mode",
                plan.target.profile()
            ));
        }
        if plan.carry.bypass_dropped {
            warnings.push("bypass was not carried to the new session".into());
        }
        Ok(SwitchOut {
            new_session,
            packet_id: handoff.packet_id,
            warnings,
        })
    }

    pub(super) fn switch_guard(&self, id: &str) -> Result<SwitchGuard<'_>, ApiErr> {
        if !lock(&self.switch_lock).insert(id.to_string()) {
            return Err(api(
                ErrorCode::Busy,
                "a switch of this session is already running",
            ));
        }
        Ok(SwitchGuard {
            rt: self,
            id: id.to_string(),
        })
    }

    fn switch_preflight(&self, old_id: &str, to: Target, opts: SwitchOpts) -> Result<Plan, ApiErr> {
        if !valid_session_id(old_id) {
            return Err(api(ErrorCode::InvalidParams, "invalid session id"));
        }
        let session = self
            .session(old_id)
            .ok_or_else(|| api(ErrorCode::NotFound, "no such session"))?;
        let old = session.info();
        if old.manager {
            return Err(refuse("the manager session cannot be switched"));
        }
        if old.task.is_some() {
            return Err(refuse(
                "a session linked to a task cannot be switched here; switch from the CLI",
            ));
        }
        let target = sw::check(&old, Some(to)).map_err(switch_err)?;
        if old.permission_mode.as_deref() == Some("bypass") || old.no_sandbox {
            let how = if opts.manual && opts.keep_bypass {
                "bypass is never carried by the daemon"
            } else {
                "sessions in bypass or no-sandbox mode cannot be switched here"
            };
            return Err(refuse(format!(
                "{how}; the owner switches those from the CLI or the app"
            )));
        }
        let carry = sw::carry(&old, target, false, supports).map_err(switch_err)?;
        let (set, _) = ProfileSet::load(profile::user_dir().as_deref());
        let prof = known_profile(&set, target.profile())?;
        if prof.source != profile::Source::Builtin {
            return Err(refuse(format!(
                "{} is overridden by a custom profile; switching supports built-in profiles only",
                target.profile()
            )));
        }
        if !Path::new(&old.cwd).is_dir() {
            return Err(refuse("the working directory of the session is gone"));
        }
        if session.is_exited() {
            self.require_packet(&old, target)?;
        }
        if sw::has_log(&old) {
            sw::ensure_readable(&old).map_err(switch_err)?;
        }
        let bin = self
            .cfg
            .self_exe
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("tmgr");
        let problem = std::env::var_os("HOME")
            .map(PathBuf::from)
            .and_then(|home| login::login_problem(prof, &home, bin));
        if let Some(p) = problem {
            return Err(refuse(format!(
                "{} is not ready to start: {p}",
                target.profile()
            )));
        }
        Ok(Plan {
            session,
            target,
            carry,
        })
    }

    fn require_packet(&self, old: &SessionInfo, target: Target) -> Result<(), ApiErr> {
        let store = HandoffStore::open(&self.cfg.dir)
            .map_err(|e| api(ErrorCode::Internal, e.to_string()))?;
        if store
            .find_for(&old.id, target.name(), old.state.since_ms)
            .is_none()
        {
            return Err(refuse(
                "the session has already exited and has no unfinished handoff to resume",
            ));
        }
        Ok(())
    }

    fn stop_and_wait(
        &self,
        s: &Arc<Session>,
        target: Target,
        int_wait: Duration,
        term_wait: Duration,
    ) -> Result<bool, ApiErr> {
        if s.is_exited() {
            return Ok(false);
        }
        if !s.has_shim() {
            return Err(switch_err(SwitchError::Conflict(format!(
                "{} cannot be stopped",
                s.info().name
            ))));
        }
        let reason = format!("handed to {}", target.name());
        for (sig, wait) in [(libc::SIGINT, int_wait), (libc::SIGTERM, term_wait)] {
            self.signal_session(s, sig, Some(&reason));
            let end = Instant::now() + wait;
            while Instant::now() < end {
                if s.is_exited() {
                    return Ok(true);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            if s.is_exited() {
                return Ok(true);
            }
        }
        Err(switch_err(SwitchError::Conflict(format!(
            "{} is still running",
            s.info().name
        ))))
    }

    fn started_from(
        &self,
        store: &HandoffStore,
        old_id: &str,
        target: &str,
        after_ms: u64,
    ) -> Option<SessionInfo> {
        self.all().iter().map(|s| s.info()).find(|i| {
            i.handoff_from.as_deref() == Some(old_id)
                && i.created_ms >= after_ms
                && i.profile.as_deref() == Some(target)
                && !store.is_successor(&i.id)
        })
    }

    fn start_switched(
        self: &Arc<Self>,
        old: &SessionInfo,
        plan: &Plan,
        prompt: &str,
    ) -> Result<SessionInfo, ApiErr> {
        let req: RunCreateReq = serde_json::from_value(serde_json::json!({
            "profile": plan.target.profile(),
            "cwd": old.cwd,
            "permission_mode": plan.carry.mode,
            "parent_session": old.id,
            "prompt": prompt,
        }))
        .map_err(|e| api(ErrorCode::Internal, e.to_string()))?;
        self.check_parent(&old.id, None)?;
        prompt_verdict(&req, true).map_err(|m| api(ErrorCode::InvalidParams, m))?;
        self.create_run_as(req, false, None, None, None, None)
            .map(|(info, ..)| info)
    }
}

#[cfg(test)]
mod tests {
    use super::super::handoff::tests::runner;
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const OLD: &str = "s_aaaaaaaaaa";

    fn add(rt: &Runner, root: &Path, id: &str, profile: &str, extra: Value) {
        let mut meta = serde_json::json!({
            "id": id, "name": "n", "argv": [profile], "cwd": root,
            "profile": profile, "created_ms": 0, "cols": 100, "rows": 24,
            "shim_pid": 1, "child_pid": 2,
        });
        meta.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let meta: Meta = serde_json::from_value(meta).unwrap();
        let sdir = root.join(id);
        fs::create_dir_all(&sdir).unwrap();
        fs::create_dir_all(rt.cfg.homes.join(id)).unwrap();
        let s = rt.new_session(meta, None, &sdir);
        lock(&rt.sessions).insert(id.into(), s);
    }

    fn kit(profile: &str, extra: Value) -> (tempfile::TempDir, Arc<Runner>) {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        add(&rt, t.path(), OLD, profile, extra);
        (t, rt)
    }

    fn exit(rt: &Runner, id: &str, since_ms: u64) {
        let s = rt.session(id).unwrap();
        lock(&s.inner).fixed = Some(State {
            kind: StateKind::Exited,
            since_ms,
            detail: None,
            exit_code: Some(0),
        });
    }

    fn err(rt: &Arc<Runner>, opts: SwitchOpts) -> ApiErr {
        rt.switch_session(OLD, Target::Codex, opts).unwrap_err()
    }

    fn refused(extra: Value, needle: &str) {
        let (_t, rt) = kit("opencode", extra);
        let e = err(&rt, SwitchOpts::default());
        assert_eq!(e.code, ErrorCode::InvalidParams, "{}", e.msg);
        assert!(e.msg.contains(needle), "{}", e.msg);
    }

    fn conn(client: &str) -> Arc<Conn> {
        let (a, _b) = std::os::unix::net::UnixStream::pair().unwrap();
        Arc::new(Conn {
            id: 1,
            w: Mutex::new(a),
            watched: Mutex::new(Vec::new()),
            scroll: Mutex::new(HashMap::new()),
            sizes: Mutex::new(HashMap::new()),
            closed: AtomicBool::new(false),
            attached: AtomicBool::new(false),
            client: client.into(),
            peer_pid: None,
            pending_rx: Mutex::new(None),
            chat_rx: Mutex::new(None),
            chat_subscribed: AtomicBool::new(false),
        })
    }

    fn call(rt: &Arc<Runner>, client: &str, p: Value) -> Result<Value, ApiErr> {
        rt.dispatch(
            &conn(client),
            &Request {
                id: 1,
                method: method::SESSIONS_SWITCH.into(),
                params: p,
            },
        )
    }

    #[test]
    fn sessions_switch_is_forbidden_for_workers_and_other_clients() {
        let (_t, rt) = kit("opencode", serde_json::json!({}));
        let p = serde_json::json!({"session": OLD, "to": "codex"});
        for client in [CLIENT_WORKER, "other", ""] {
            let e = call(&rt, client, p.clone()).unwrap_err();
            assert_eq!(e.code, ErrorCode::Forbidden, "{client}");
        }
    }

    #[test]
    fn sessions_switch_reaches_the_switch_for_cli_and_desktop() {
        let (_t, rt) = kit("opencode", serde_json::json!({}));
        for client in [CLIENT_CLI, CLIENT_DESKTOP] {
            let e = call(
                &rt,
                client,
                serde_json::json!({"session": "s_zzzzzzzzzz", "to": "codex"}),
            )
            .unwrap_err();
            assert_eq!(e.code, ErrorCode::NotFound, "{client}");
        }
        let e = call(&rt, CLIENT_CLI, serde_json::json!({"session": OLD})).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidParams);
        let e = call(
            &rt,
            CLIENT_CLI,
            serde_json::json!({"session": OLD, "to": "vim"}),
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidParams);
    }

    #[test]
    fn sessions_switch_refuses_bypass_params_and_bypass_sessions() {
        let (_t, rt) = kit("opencode", serde_json::json!({"permission_mode": "bypass"}));
        let p = serde_json::json!({"session": OLD, "to": "codex"});
        let e = call(&rt, CLIENT_CLI, p).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidParams);
        assert!(e.msg.contains("bypass"), "{}", e.msg);
        for extra in [
            serde_json::json!({"keep_bypass": true}),
            serde_json::json!({"permission_mode": "bypass"}),
        ] {
            let mut p = serde_json::json!({"session": OLD, "to": "codex"});
            p.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let e = call(&rt, CLIENT_DESKTOP, p).unwrap_err();
            assert!(e.msg.contains("never carried"), "{}", e.msg);
        }
    }

    #[test]
    fn the_manager_session_is_refused() {
        refused(serde_json::json!({"manager": true}), "manager");
    }

    #[test]
    fn team_members_are_refused() {
        refused(
            serde_json::json!({"team": {"role": {"worker": 1}, "of": "t1"}}),
            "team member",
        );
    }

    #[test]
    fn link_origin_sessions_are_refused() {
        refused(
            serde_json::json!({"origin": {"link": "l1", "generation": 1}}),
            "link",
        );
    }

    #[test]
    fn task_linked_sessions_are_refused() {
        refused(
            serde_json::json!({"task": {"workspace_id": -1, "task_id": 7}}),
            "switch from the CLI",
        );
    }

    #[test]
    fn bypass_and_no_sandbox_are_refused() {
        refused(serde_json::json!({"permission_mode": "bypass"}), "bypass");
        refused(serde_json::json!({"no_sandbox": true}), "bypass");
    }

    #[test]
    fn bypass_is_never_carried_even_with_keep_bypass() {
        for manual in [false, true] {
            let (_t, rt) = kit("opencode", serde_json::json!({"permission_mode": "bypass"}));
            let e = err(
                &rt,
                SwitchOpts {
                    keep_bypass: true,
                    manual,
                },
            );
            assert_eq!(e.code, ErrorCode::InvalidParams);
            assert!(e.msg.contains("bypass"), "{}", e.msg);
            assert!(rt.all().len() == 1, "no session was started");
        }
    }

    #[test]
    fn an_unknown_profile_is_refused() {
        let (_t, rt) = kit("custom", serde_json::json!({}));
        let e = err(&rt, SwitchOpts::default());
        assert_eq!(e.code, ErrorCode::InvalidParams);
    }

    #[test]
    fn an_unknown_session_is_not_found_and_releases_the_lock() {
        let (_t, rt) = kit("opencode", serde_json::json!({}));
        let e = rt
            .switch_session("s_zzzzzzzzzz", Target::Codex, SwitchOpts::default())
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::NotFound);
        assert!(rt.switch_guard("s_zzzzzzzzzz").is_ok());
    }

    #[test]
    fn an_exited_session_without_a_packet_is_refused() {
        let (_t, rt) = kit("opencode", serde_json::json!({}));
        exit(&rt, OLD, 0);
        let e = err(&rt, SwitchOpts::default());
        assert_eq!(e.code, ErrorCode::InvalidParams);
        assert!(e.msg.contains("unfinished handoff"), "{}", e.msg);
    }

    #[test]
    fn an_exited_session_with_a_stored_packet_passes_that_check() {
        let (t, rt) = kit("opencode", serde_json::json!({}));
        let store = HandoffStore::open(&rt.cfg.dir).unwrap();
        let id = format!("h_{}", "1".repeat(32));
        store
            .save(
                &id,
                &serde_json::json!({
                    "id": id, "created_ms": 10,
                    "from": {"session": OLD}, "to": {"agent": "codex"},
                }),
            )
            .unwrap();
        exit(&rt, OLD, 5);
        fs::set_permissions(t.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let msg = rt
            .switch_session(OLD, Target::Codex, SwitchOpts::default())
            .err()
            .map(|e| e.msg)
            .unwrap_or_default();
        assert!(!msg.contains("unfinished handoff"), "{msg}");
    }

    #[test]
    fn a_second_switch_of_the_same_session_is_busy() {
        let (_t, rt) = kit("opencode", serde_json::json!({}));
        let _held = rt.switch_guard(OLD).unwrap();
        let e = err(&rt, SwitchOpts::default());
        assert_eq!(e.code, ErrorCode::Busy);
    }

    #[test]
    fn the_lock_is_released_after_a_refusal() {
        let (_t, rt) = kit("opencode", serde_json::json!({"manager": true}));
        err(&rt, SwitchOpts::default());
        assert!(rt.switch_guard(OLD).is_ok());
    }

    #[test]
    fn stop_skips_an_exited_session() {
        let (_t, rt) = kit("opencode", serde_json::json!({}));
        exit(&rt, OLD, 0);
        let s = rt.session(OLD).unwrap();
        rt.stop_and_wait(&s, Target::Codex, Duration::ZERO, Duration::ZERO)
            .unwrap();
    }

    #[test]
    fn a_running_session_without_a_shim_cannot_be_stopped() {
        let (_t, rt) = kit("opencode", serde_json::json!({}));
        let s = rt.session(OLD).unwrap();
        let e = rt
            .stop_and_wait(&s, Target::Codex, Duration::ZERO, Duration::ZERO)
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::Busy);
    }

    #[test]
    fn started_from_finds_only_a_child_made_after_the_packet() {
        let (t, rt) = kit("opencode", serde_json::json!({}));
        add(
            &rt,
            t.path(),
            "s_bbbbbbbbbb",
            "codex",
            serde_json::json!({"handoff_from": OLD, "created_ms": 100}),
        );
        let store = HandoffStore::open(&rt.cfg.dir).unwrap();
        assert!(rt.started_from(&store, OLD, "codex", 100).is_some());
        assert!(rt.started_from(&store, OLD, "codex", 101).is_none());
        assert!(rt
            .started_from(&store, "s_cccccccccc", "codex", 0)
            .is_none());
    }

    #[test]
    fn started_from_ignores_a_child_of_another_profile() {
        let (t, rt) = kit("opencode", serde_json::json!({}));
        add(
            &rt,
            t.path(),
            "s_bbbbbbbbbb",
            "opencode",
            serde_json::json!({"handoff_from": OLD, "created_ms": 100}),
        );
        let store = HandoffStore::open(&rt.cfg.dir).unwrap();
        assert!(rt.started_from(&store, OLD, "codex", 0).is_none());
    }

    #[test]
    fn started_from_ignores_a_child_that_finished_another_packet() {
        let (t, rt) = kit("opencode", serde_json::json!({}));
        add(
            &rt,
            t.path(),
            "s_bbbbbbbbbb",
            "codex",
            serde_json::json!({"handoff_from": OLD, "created_ms": 100}),
        );
        let store = HandoffStore::open(&rt.cfg.dir).unwrap();
        store
            .mark_finished(&format!("h_{}", "2".repeat(32)), "s_bbbbbbbbbb")
            .unwrap();
        assert!(rt.started_from(&store, OLD, "codex", 0).is_none());
    }

    #[test]
    fn stop_reports_whether_the_session_was_running() {
        let (_t, rt) = kit("opencode", serde_json::json!({}));
        exit(&rt, OLD, 0);
        let s = rt.session(OLD).unwrap();
        let ran = rt
            .stop_and_wait(&s, Target::Codex, Duration::ZERO, Duration::ZERO)
            .unwrap();
        assert!(!ran);
    }

    #[test]
    fn handoff_methods_are_busy_while_a_switch_of_the_session_runs() {
        let (_t, rt) = kit("opencode", serde_json::json!({}));
        let _held = rt.switch_guard(OLD).unwrap();
        let e = rt
            .dispatch(
                &conn(CLIENT_CLI),
                &Request {
                    id: 1,
                    method: method::HANDOFF_PREPARE.into(),
                    params: serde_json::json!({"session": OLD, "to": "codex"}),
                },
            )
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::Busy);
    }
}
