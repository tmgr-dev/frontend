use super::*;
use crate::handoff_store::HandoffStore;
use crate::switch::{self as sw, SwitchError, Target};
use tmgr_ipc::api::{HandoffFinishReq, HandoffFinishResp, HandoffPrepareReq, HandoffPrepareResp};
use tmgr_session::packet::HandoffPacket;

const SCREEN_LINES: u32 = 400;
const PACKET_FILE: &str = "handoff.json";

fn switch_err(e: SwitchError) -> ApiErr {
    match e {
        SwitchError::Usage(m) => api(ErrorCode::InvalidParams, m),
        SwitchError::NotFound(m) => api(ErrorCode::NotFound, m),
        SwitchError::Conflict(m) => api(ErrorCode::Busy, m),
        SwitchError::Internal(m) => api(ErrorCode::Internal, m),
    }
}

fn supports(profile: &str, mode: &str) -> bool {
    let (set, _) = ProfileSet::load(profile::user_dir().as_deref());
    set.lookup(profile)
        .is_ok_and(|p| p.permission_args(mode).is_ok())
}

impl Runner {
    fn handoff_session(&self, id: &str) -> Result<Arc<Session>, ApiErr> {
        if !valid_session_id(id) {
            return Err(api(ErrorCode::InvalidParams, "invalid session id"));
        }
        self.session(id)
            .ok_or_else(|| api(ErrorCode::NotFound, "no such session"))
    }

    pub(super) fn handoff_dispatch(
        &self,
        conn: &Conn,
        m: &str,
        p: &Value,
    ) -> Result<Value, ApiErr> {
        if conn.client != CLIENT_CLI {
            return Err(api(
                ErrorCode::Forbidden,
                "handoff methods are accepted only from the tmgr CLI",
            ));
        }
        match m {
            method::HANDOFF_PREPARE => self.handoff_prepare(params(p)?).map(|r| to_json(&r)),
            _ => self.handoff_finish(params(p)?).map(|r| to_json(&r)),
        }
    }

    fn handoff_prepare(&self, req: HandoffPrepareReq) -> Result<HandoffPrepareResp, ApiErr> {
        let s = self.handoff_session(&req.session)?;
        let old = s.info();
        let target = Target::of_profile(Some(&req.to)).ok_or_else(|| {
            api(
                ErrorCode::InvalidParams,
                format!("{} is not a switch target", req.to),
            )
        })?;
        let target = sw::check(&old, Some(target)).map_err(switch_err)?;
        let carried = sw::carry(&old, target, req.keep_bypass, supports).map_err(switch_err)?;
        let screen = if sw::has_log(&old) {
            Vec::new()
        } else {
            s.text(Some(SCREEN_LINES)).lines
        };
        let sessions: Vec<SessionInfo> = self.all().iter().map(|x| x.info()).collect();
        let inputs = sw::prepare_inputs(&old, &sessions, target, req.task_ref.as_deref(), &screen)
            .map_err(switch_err)?;
        let store = HandoffStore::open(&self.cfg.dir)
            .map_err(|e| api(ErrorCode::Internal, e.to_string()))?;
        let mut random = [0u8; 10];
        getrandom::fill(&mut random).map_err(|e| api(ErrorCode::Internal, e.to_string()))?;
        let h = sw::build_handoff(
            &store,
            &self.chat,
            &old,
            &inputs,
            target,
            &carried,
            now_ms(),
            random,
        )
        .map_err(switch_err)?;
        Ok(HandoffPrepareResp {
            packet_id: h.packet_id,
            prompt: h.prompt,
            mode: carried.mode,
            fell_back: carried.fell_back,
            bypass_dropped: carried.bypass_dropped,
        })
    }

    fn handoff_finish(&self, req: HandoffFinishReq) -> Result<HandoffFinishResp, ApiErr> {
        self.handoff_session(&req.new_session)?;
        let store = HandoffStore::open(&self.cfg.dir)
            .map_err(|e| api(ErrorCode::Internal, e.to_string()))?;
        let value = store
            .get(&req.packet_id)
            .map_err(|e| api(ErrorCode::InvalidParams, e.to_string()))?
            .ok_or_else(|| api(ErrorCode::NotFound, "no such handoff packet"))?;
        let packet: HandoffPacket = serde_json::from_value(value.clone())
            .map_err(|e| api(ErrorCode::Internal, e.to_string()))?;
        let old = self.handoff_session(&packet.from.session)?.info();
        let target = Target::of_profile(Some(&packet.to.profile))
            .ok_or_else(|| api(ErrorCode::Internal, "the packet names an unknown target"))?;
        sw::announce(&self.chat, &old, &req.new_session, &packet, target).map_err(switch_err)?;
        let sessions: Vec<SessionInfo> = self.all().iter().map(|x| x.info()).collect();
        let mut warnings: Vec<String> = Vec::new();
        let home = self.cfg.homes.join(&req.new_session);
        match sw::prepare_inputs(&old, &sessions, target, None, &[]) {
            Ok(inputs) => warnings.extend(sw::export_all_at(&home, &inputs.exports)),
            Err(e) => warnings.push(e.to_string()),
        }
        let bytes = serde_json::to_vec_pretty(&value)
            .map_err(|e| api(ErrorCode::Internal, e.to_string()))?;
        if let Err(e) = write_atomic(&home.join(PACKET_FILE), &bytes) {
            warnings.push(format!("the handoff packet could not be copied: {e}"));
        }
        Ok(HandoffFinishResp {
            warning: (!warnings.is_empty()).then(|| warnings.join("; ")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixStream;

    const OLD: &str = "s_aaaaaaaaaa";
    const NEW: &str = "s_bbbbbbbbbb";

    fn runner(dir: &Path) -> Arc<Runner> {
        fs::set_permissions(dir, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
        let cfg = RunnerConfig {
            dir: dir.join("run"),
            homes: dir.join("homes"),
            self_exe: PathBuf::from("tmgr"),
            quiet_ms: 1000,
            version: "test".into(),
            max_sessions: 4,
            relay: None,
        };
        let chat_store = Arc::new(chat::Chat::open(&cfg.dir));
        Arc::new(Runner {
            rename_lock: Mutex::new(()),
            chat: chat_store.clone(),
            mgr: crate::manager::State::new(Box::new(chat_store)),
            cfg,
            secret: String::new(),
            started: Instant::now(),
            sessions: Mutex::new(HashMap::new()),
            hub: Arc::new(Hub::default()),
            create_lock: Mutex::new(()),
            key_lock: Mutex::new(()),
            external_lock: Mutex::new(()),
            layout_lock: Mutex::new(()),
            stop: AtomicBool::new(false),
            conns: Arc::new(AtomicUsize::new(0)),
            pending: Arc::new(AtomicUsize::new(0)),
            rejects: Mutex::new(Rejects::default()),
            client_config_dirs: Mutex::new(Vec::new()),
            config_dir_warned: AtomicBool::new(false),
            next_conn: AtomicU64::new(1),
            autopilot: autopilot::Host::default(),
            clock: Mutex::new(humantime::Clock::new(0)),
        })
    }

    fn add(rt: &Runner, root: &Path, id: &str, mode: Option<&str>) {
        let meta: Meta = serde_json::from_value(serde_json::json!({
            "id": id, "name": "n", "argv": ["opencode"], "cwd": root,
            "profile": "opencode", "created_ms": 0, "cols": 100, "rows": 24,
            "shim_pid": 1, "child_pid": 2, "permission_mode": mode,
        }))
        .unwrap();
        let sdir = root.join(id);
        fs::create_dir_all(&sdir).unwrap();
        fs::create_dir_all(rt.cfg.homes.join(id)).unwrap();
        let s = rt.new_session(meta, None, &sdir);
        lock(&rt.sessions).insert(id.into(), s);
    }

    fn conn(client: &str) -> Arc<Conn> {
        let (a, _b) = UnixStream::pair().unwrap();
        Arc::new(Conn {
            id: 1,
            w: Mutex::new(a),
            watched: Mutex::new(Vec::new()),
            scroll: Mutex::new(HashMap::new()),
            sizes: Mutex::new(HashMap::new()),
            closed: AtomicBool::new(false),
            attached: AtomicBool::new(false),
            client: client.into(),
            pending_rx: Mutex::new(None),
            chat_rx: Mutex::new(None),
        })
    }

    fn call(rt: &Arc<Runner>, client: &str, m: &str, p: Value) -> Result<Value, ApiErr> {
        rt.dispatch(
            &conn(client),
            &Request {
                id: 1,
                method: m.into(),
                params: p,
            },
        )
    }

    fn prepare(rt: &Arc<Runner>, keep_bypass: bool) -> HandoffPrepareResp {
        let req = HandoffPrepareReq {
            session: OLD.into(),
            to: "codex".into(),
            keep_bypass,
            task_ref: None,
        };
        serde_json::from_value(
            call(rt, CLIENT_CLI, method::HANDOFF_PREPARE, to_json(&req)).unwrap(),
        )
        .unwrap()
    }

    fn finish(rt: &Arc<Runner>, id: &str) -> Result<Value, ApiErr> {
        let req = HandoffFinishReq {
            packet_id: id.into(),
            new_session: NEW.into(),
        };
        call(rt, CLIENT_CLI, method::HANDOFF_FINISH, to_json(&req))
    }

    fn kit() -> (tempfile::TempDir, Arc<Runner>) {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        add(&rt, t.path(), OLD, None);
        add(&rt, t.path(), NEW, None);
        (t, rt)
    }

    #[test]
    fn only_the_cli_may_call_the_handoff_methods() {
        let (_t, rt) = kit();
        let prep = to_json(&HandoffPrepareReq {
            session: OLD.into(),
            to: "codex".into(),
            keep_bypass: false,
            task_ref: None,
        });
        let fin = to_json(&HandoffFinishReq {
            packet_id: "h_x".into(),
            new_session: NEW.into(),
        });
        for client in [CLIENT_DESKTOP, CLIENT_WORKER, "other", ""] {
            for (m, p) in [
                (method::HANDOFF_PREPARE, &prep),
                (method::HANDOFF_FINISH, &fin),
            ] {
                let e = call(&rt, client, m, p.clone()).unwrap_err();
                assert_eq!(e.code, ErrorCode::Forbidden, "{client} {m}");
            }
        }
    }

    #[test]
    fn prepare_stores_a_private_packet_and_a_rerun_reuses_it() {
        let (t, rt) = kit();
        let first = prepare(&rt, false);
        let path = t
            .path()
            .join("run/handoff")
            .join(format!("{}.json", first.packet_id));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!first.prompt.is_empty());
        let second = prepare(&rt, false);
        assert_eq!(second.packet_id, first.packet_id);
        let files = fs::read_dir(t.path().join("run/handoff")).unwrap().count();
        assert_eq!(files, 1);
    }

    #[test]
    fn bypass_is_never_carried_without_keep_bypass() {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        add(&rt, t.path(), OLD, Some("bypass"));
        let r = prepare(&rt, false);
        assert_eq!(r.mode, "ask");
        assert!(r.bypass_dropped);
    }

    #[test]
    fn finish_writes_both_messages_once_and_copies_the_packet() {
        let (t, rt) = kit();
        let id = prepare(&rt, false).packet_id;
        finish(&rt, &id).unwrap();
        finish(&rt, &id).unwrap();
        let count = |sid: &str| {
            rt.chat
                .history(&rt.chat.session_room(sid), None, None)
                .unwrap()
                .iter()
                .filter(|m| m.body_md.contains(&id))
                .count()
        };
        assert_eq!(count(OLD), 1);
        assert_eq!(count(NEW), 1);
        let copy = t.path().join("homes").join(NEW).join(PACKET_FILE);
        assert_eq!(
            fs::metadata(&copy).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let v: Value = serde_json::from_slice(&fs::read(&copy).unwrap()).unwrap();
        assert_eq!(v["id"], id.as_str());
    }

    #[test]
    fn finish_refuses_an_unknown_packet() {
        let (_t, rt) = kit();
        let e = finish(&rt, &format!("h_{}", "0".repeat(32))).unwrap_err();
        assert_eq!(e.code, ErrorCode::NotFound);
    }
}
