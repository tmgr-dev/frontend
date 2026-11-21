#![allow(dead_code)]

use super::handoff::{supports, switch_err};
use super::*;
use crate::handoff_store::HandoffStore;
use crate::switch::{self as sw, Carry, SwitchError, Target};

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

struct SwitchGuard<'a> {
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
    pub(crate) fn switch_session(
        self: &Arc<Self>,
        old_id: &str,
        to: Target,
        opts: SwitchOpts,
    ) -> Result<SwitchOut, ApiErr> {
        let _guard = self.switch_guard(old_id)?;
        let plan = self.switch_preflight(old_id, to, opts)?;
        self.stop_and_wait(&plan.session, plan.target, INT_WAIT, TERM_WAIT)?;
        let old = plan.session.info();
        let screen = if sw::has_log(&old) {
            Vec::new()
        } else {
            plan.session.text(Some(SCREEN_LINES)).lines
        };
        let sessions: Vec<SessionInfo> = self.all().iter().map(|x| x.info()).collect();
        let inputs = sw::prepare_inputs(&old, &sessions, plan.target, None, &screen)
            .map_err(switch_err)?;
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
        let new_session = match self.started_from(&old.id, handoff.packet.created_ms) {
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

    fn switch_guard(&self, id: &str) -> Result<SwitchGuard<'_>, ApiErr> {
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

    fn stop_and_wait(
        &self,
        s: &Arc<Session>,
        target: Target,
        int_wait: Duration,
        term_wait: Duration,
    ) -> Result<(), ApiErr> {
        if s.is_exited() {
            return Ok(());
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
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            if s.is_exited() {
                return Ok(());
            }
        }
        Err(switch_err(SwitchError::Conflict(format!(
            "{} is still running",
            s.info().name
        ))))
    }

    fn started_from(&self, old_id: &str, after_ms: u64) -> Option<SessionInfo> {
        self.all()
            .iter()
            .map(|s| s.info())
            .find(|i| i.handoff_from.as_deref() == Some(old_id) && i.created_ms >= after_ms)
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
        self.create_run_as(req, false, None, None, None)
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
            let e = err(&rt, SwitchOpts { keep_bypass: true, manual });
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
        assert!(rt.started_from(OLD, 100).is_some());
        assert!(rt.started_from(OLD, 101).is_none());
        assert!(rt.started_from("s_cccccccccc", 0).is_none());
    }
}
