use super::switch::SwitchOpts;
use super::*;
use crate::manager as core;
use crate::switch::Target;
use tmgr_ipc::manager::{tool, SpoolReq, SpoolResp};

impl Runner {
    pub(super) fn live_manager(&self) -> Option<Arc<Session>> {
        self.all()
            .into_iter()
            .find(|s| lock(&s.inner).meta.manager && !s.is_exited())
    }

    pub(super) fn check_manager_create(
        &self,
        client: &str,
        req: &RunCreateReq,
    ) -> Result<(), ApiErr> {
        core::check_create(req, client == CLIENT_CLI, false).map_err(|(c, m)| api(c, m))
    }

    pub(super) fn manager_pass(self: &Arc<Self>) {
        if let Some(m) = self.live_manager() {
            let home = self.cfg.homes.join(&m.id);
            match core::take_pending(&home, now_ms()) {
                Ok(reqs) => {
                    self.mgr.warned.store(false, Ordering::SeqCst);
                    let host = ManagerHost {
                        rt: self,
                        me: &m.id,
                    };
                    let link = crate::manager_remote::Live {
                        dir: &self.cfg.dir,
                        chat: &self.chat,
                    };
                    for req in reqs {
                        if goes_off_thread(&host, &req) {
                            self.answer_remote_async(&m.id, home.clone(), req);
                            continue;
                        }
                        let resp =
                            match crate::manager_remote::execute(&link, &host, &self.mgr, &req) {
                                Ok(v) => SpoolResp::ok(&req.id, v),
                                Err(e) => SpoolResp::err(&req.id, e),
                            };
                        if let Err(e) = core::respond(&home, &resp) {
                            log(&format!("manager {}: response not written: {e}", m.id));
                        }
                    }
                }
                Err(e) => {
                    if !self.mgr.warned.swap(true, Ordering::SeqCst) {
                        log(&format!("manager {}: spool refused: {e}", m.id));
                    }
                }
            }
        }
        self.mgr.deliver_prompts(now_ms(), |p| {
            match self.session(&p.session).filter(|s| !s.is_exited()) {
                None => core::Delivery::Done,
                Some(s) => match s.input_at_prompt(&core::prompt_bytes(&p.text), Instant::now()) {
                    Ok(()) => core::Delivery::Done,
                    Err(_) => core::Delivery::Retry,
                },
            }
        });
    }
}

impl Runner {
    fn answer_remote_async(self: &Arc<Self>, manager: &str, home: PathBuf, req: SpoolReq) {
        let rt = Arc::clone(self);
        let id = manager.to_string();
        let req_id = req.id.clone();
        let thread_home = home.clone();
        let spawned = std::thread::Builder::new()
            .name("manager-call".into())
            .spawn(move || {
                let host = ManagerHost { rt: &rt, me: &id };
                let link = crate::manager_remote::Live {
                    dir: &rt.cfg.dir,
                    chat: &rt.chat,
                };
                let resp = guarded(&req.id, || {
                    crate::manager_remote::execute(&link, &host, &rt.mgr, &req)
                });
                if let Err(e) = core::respond(&thread_home, &resp) {
                    log(&format!("manager {id}: response not written: {e}"));
                }
            });
        if let Err(e) = spawned {
            log(&format!("manager {manager}: call thread not started: {e}"));
            let resp = SpoolResp::err(&req_id, "the call could not be started");
            if let Err(e) = core::respond(&home, &resp) {
                log(&format!("manager {manager}: response not written: {e}"));
            }
        }
    }
}

fn goes_off_thread(host: &ManagerHost, req: &SpoolReq) -> bool {
    use core::Host;
    match req.tool.as_str() {
        tool::SWITCH_AGENT => true,
        tool::SPAWN_SESSION | tool::PROPOSE_SPAWN => req
            .args
            .get("machine")
            .and_then(Value::as_str)
            .is_some_and(|m| m != host.machine()),
        _ => false,
    }
}

fn guarded(req_id: &str, run: impl FnOnce() -> Result<Value, String>) -> SpoolResp {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)) {
        Ok(Ok(v)) => SpoolResp::ok(req_id, v),
        Ok(Err(e)) => SpoolResp::err(req_id, e),
        Err(_) => SpoolResp::err(req_id, "the call failed unexpectedly"),
    }
}

fn builtin<'a>(set: &'a ProfileSet, id: &str) -> Result<&'a Profile, String> {
    set.get(id)
        .filter(|p| p.source == profile::Source::Builtin)
        .ok_or_else(|| "profile must be a built-in agent".into())
}

fn spawn_req(spec: &core::SpawnSpec) -> Result<RunCreateReq, String> {
    serde_json::from_value(serde_json::json!({
        "profile": spec.profile,
        "cwd": spec.cwd.display().to_string(),
        "name": spec.name,
        "permission_mode": spec.mode,
        "parent_session": spec.parent,
        "spawn_card": spec.card,
        "cwd_pin": spec.pin,
    }))
    .map_err(|e| e.to_string())
}

pub(super) struct ManagerHost<'a> {
    pub(super) rt: &'a Arc<Runner>,
    pub(super) me: &'a str,
}

impl core::Host for ManagerHost<'_> {
    fn me(&self) -> &str {
        self.me
    }

    fn machine(&self) -> String {
        self.rt.chat.machine().to_string()
    }

    fn now_ms(&self) -> u64 {
        now_ms()
    }

    fn sessions(&self) -> Vec<SessionInfo> {
        let mut list: Vec<SessionInfo> = self.rt.all().iter().map(|s| s.info()).collect();
        list.sort_by(|a, b| (a.created_ms, &a.id).cmp(&(b.created_ms, &b.id)));
        list
    }

    fn text(&self, id: &str, lines: u32) -> Result<Vec<String>, String> {
        let s = self.rt.session(id).ok_or("no such session")?;
        Ok(s.text(Some(lines)).lines)
    }

    fn send(&self, id: &str, text: &str) -> Result<(), String> {
        let s = self.rt.live(id).map_err(|e| e.msg)?;
        s.input_at_prompt(&core::prompt_bytes(text), Instant::now())
            .map_err(|e| e.msg)
    }

    fn trusted_dir(&self, cwd: &str) -> Result<core::TrustedDir, String> {
        let bad = || "cwd must be an existing absolute directory".to_string();
        if !Path::new(cwd).is_absolute() {
            return Err(bad());
        }
        let dir =
            bundle::open_path(fs::canonicalize(cwd).map_err(|_| bad())?).map_err(|_| bad())?;
        if !folders::is_trusted(&self.rt.cfg.homes, &dir.path, dir.id()) {
            return Err(
                "cwd is not a trusted folder; the owner must trust it with tmgr runner trust"
                    .into(),
            );
        }
        let (dev, ino) = dir.id();
        Ok(core::TrustedDir {
            path: dir.path.clone(),
            pin: CwdPin { dev, ino },
        })
    }

    fn precheck_spawn(&self, spec: &core::SpawnSpec) -> Result<(), String> {
        let (set, _) = ProfileSet::load(profile::user_dir().as_deref());
        let prof = builtin(&set, &spec.profile)?;
        plan_mode(
            prof,
            &spawn_req(spec)?,
            &None,
            false,
            true,
            sandbox::network_restriction_enforced(),
        )
        .map(|_| ())
        .map_err(|e| e.msg)
    }

    fn spawn(&self, spec: core::SpawnSpec) -> Result<SessionInfo, String> {
        let (set, _) = ProfileSet::load(profile::user_dir().as_deref());
        builtin(&set, &spec.profile)?;
        self.rt
            .create_run_as(spawn_req(&spec)?, false, None, None, None)
            .map(|(info, ..)| info)
            .map_err(|e| e.msg)
    }

    fn switch(&self, id: &str, to: &str) -> Result<core::SwitchResult, String> {
        let target = Target::of_profile(Some(to)).ok_or("to must be a built-in agent")?;
        let old = self.rt.session(id).ok_or("no such session")?.info();
        let trusted = self.trusted_dir(&old.cwd).map_err(|e| {
            format!("{e}; the owner switches sessions of untrusted folders from the CLI or the app")
        })?;
        if fs::canonicalize(&old.cwd).ok().as_deref() != Some(trusted.path.as_path()) {
            return Err(
                "cwd is not a trusted folder; the owner switches it from the CLI or the app".into(),
            );
        }
        self.rt
            .switch_session(
                id,
                target,
                SwitchOpts {
                    keep_bypass: false,
                    manual: false,
                },
            )
            .map(|o| core::SwitchResult {
                session: o.new_session,
                packet_id: o.packet_id,
                warnings: o.warnings,
            })
            .map_err(|e| e.msg)
    }

    fn verify_spawn(
        &self,
        approval: &tmgr_ipc::manager::SignedSpawn,
        proposal: &tmgr_spawn_auth::SpawnProposal,
        proposer: Option<&str>,
    ) -> Result<core::Verdict, String> {
        self.rt.verify_spawn(approval, proposal, proposer)
    }

    fn spawn_done(&self, card_id: &str, session: &str) {
        self.rt.spawn_done(card_id, session);
    }

    fn propose_spawn(
        &self,
        proposal: &tmgr_spawn_auth::SpawnProposal,
        proposer: Option<&str>,
        prompt: Option<&str>,
    ) -> Result<core::Proposed, String> {
        self.rt.propose_spawn_by(proposal, proposer, prompt)
    }

    fn approved_spawn(
        &self,
        card_id: &str,
        proposer: Option<&str>,
    ) -> Option<tmgr_spawn_auth::SignedSpawn> {
        self.rt.approved_spawn(card_id, proposer)
    }

    fn proposer(&self) -> Option<String> {
        (self.me != "-").then(|| self.me.to_string())
    }

    fn remote_caller(&self) -> Option<&str> {
        None
    }

    fn spawned_for(&self, session: &str, proposer: &str) -> bool {
        self.rt.spawned_for(session, proposer)
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::manager::Host;
    use serde_json::json;

    const MGR: &str = "s_aaaaaaaaaa";
    const OTHER: &str = "s_bbbbbbbbbb";

    pub(in crate::daemon) fn runner(t: &Path) -> Arc<Runner> {
        Arc::new(Runner {
            rename_lock: Mutex::new(()),
            switch_lock: Mutex::new(HashSet::new()),
            chat: Arc::new(crate::chat::Chat::open(&t.join("run"))),
            cfg: RunnerConfig {
                dir: t.join("run"),
                homes: t.join("homes"),
                self_exe: PathBuf::from("tmgr"),
                quiet_ms: 1000,
                version: "test".into(),
                max_sessions: 4,
                relay: None,
            },
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
            mgr: Arc::new(core::State::default()),
            spawn_gate: spawnauth::SpawnGate::default(),
        })
    }

    pub(in crate::daemon) fn add(rt: &Arc<Runner>, t: &Path, id: &str, manager: bool) {
        let meta: Meta = serde_json::from_value(json!({
            "id": id, "name": id, "argv": ["claude"], "cwd": t,
            "profile": "claude-code", "created_ms": 0, "cols": 100, "rows": 24,
            "shim_pid": 1, "child_pid": 2, "manager": manager,
        }))
        .unwrap();
        let sdir = t.join("sessions").join(id);
        fs::create_dir_all(&sdir).unwrap();
        let s = rt.new_session(meta, None, &sdir);
        lock(&rt.sessions).insert(id.to_string(), s);
    }

    fn plant(home: &Path, tool: &str) -> String {
        let stem = format!("{}-1-ab", now_ms());
        let d = core::req_dir(home);
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join(format!("{stem}.json")),
            json!({"v":1,"id":stem,"tool":tool,"args":{}}).to_string(),
        )
        .unwrap();
        stem
    }

    fn pair() -> (tempfile::TempDir, Arc<Runner>) {
        let t = tempfile::Builder::new()
            .prefix("mh")
            .tempdir_in("/tmp")
            .unwrap();
        let rt = runner(t.path());
        add(&rt, t.path(), MGR, true);
        add(&rt, t.path(), OTHER, false);
        (t, rt)
    }

    #[test]
    fn only_the_recorded_managers_spool_is_read() {
        let (t, rt) = pair();
        let homes = t.path().join("homes");
        let mine = plant(&homes.join(MGR), "list_sessions");
        let foreign = plant(&homes.join(OTHER), "list_sessions");
        rt.manager_pass();
        let answered = homes
            .join(MGR)
            .join(format!(".cache/tmgr-manager/resp/{mine}.json"));
        let v: Value = serde_json::from_slice(&fs::read(answered).unwrap()).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["result"].as_array().unwrap().len(), 2);
        let theirs = core::req_dir(&homes.join(OTHER)).join(format!("{foreign}.json"));
        assert!(theirs.exists());
        assert!(!homes.join(OTHER).join(".cache/tmgr-manager/resp").exists());
    }

    fn reply(t: &Path, stem: &str) -> Value {
        let p = t
            .join("homes")
            .join(MGR)
            .join(format!(".cache/tmgr-manager/resp/{stem}.json"));
        serde_json::from_slice(&fs::read(p).unwrap()).unwrap()
    }

    #[test]
    fn the_remote_tools_are_refused_for_a_sub_and_answered_for_the_top() {
        let (t, rt) = pair();
        let home = t.path().join("homes").join(MGR);
        let ask = |tool: &str| {
            let stem = plant(&home, tool);
            rt.manager_pass();
            reply(t.path(), &stem)
        };
        assert_eq!(ask("list_machines")["error"], "not available");
        assert_eq!(ask("__caps")["result"], json!({"top": false}));

        crate::mgr_rpc::top::apply_top(&t.path().join("run"), Some(true)).unwrap();
        assert_eq!(ask("__caps")["result"], json!({"top": true}));
        let v = ask("list_machines");
        assert_eq!(v["ok"], true);
        assert_eq!(v["result"], json!({"machines": [], "relay_ready": false}));
    }

    #[test]
    fn a_switch_request_is_answered_from_its_own_thread() {
        let (t, rt) = pair();
        let home = t.path().join("homes").join(MGR);
        let stem = format!("{}-1-ab", now_ms());
        let d = core::req_dir(&home);
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join(format!("{stem}.json")),
            json!({"v":1,"id":stem,"tool":"switch_agent","args":{"session": MGR, "to": "codex"}})
                .to_string(),
        )
        .unwrap();
        rt.manager_pass();
        let resp = home.join(format!(".cache/tmgr-manager/resp/{stem}.json"));
        let end = Instant::now() + Duration::from_secs(5);
        while !resp.exists() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        let v: Value = serde_json::from_slice(&fs::read(resp).unwrap()).unwrap();
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("itself"));
    }

    #[test]
    fn calls_naming_a_foreign_machine_go_through_the_remote_router_off_the_spool_thread() {
        let (t, rt) = pair();
        let home = t.path().join("homes").join(MGR);
        let host = ManagerHost { rt: &rt, me: MGR };
        let own = core::Host::machine(&host);
        let req = |tool: &str, machine: &str| SpoolReq {
            v: 1,
            id: "1-1-a".into(),
            tool: tool.into(),
            args: json!({"machine": machine}),
        };
        assert!(goes_off_thread(&host, &req("switch_agent", &own)));
        assert!(goes_off_thread(
            &host,
            &req("spawn_session", "mx_00000000000000bb")
        ));
        assert!(goes_off_thread(
            &host,
            &req("propose_spawn", "mx_00000000000000bb")
        ));
        assert!(!goes_off_thread(&host, &req("spawn_session", &own)));
        assert!(!goes_off_thread(
            &host,
            &req("list_sessions", "mx_00000000000000bb")
        ));
        let stem = format!("{}-1-ab", now_ms());
        let d = core::req_dir(&home);
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join(format!("{stem}.json")),
            json!({"v":1,"id":stem,"tool":"propose_spawn","args":{"machine":"mx_00000000000000bb","profile":"codex","cwd":"/tmp"}})
                .to_string(),
        )
        .unwrap();
        rt.manager_pass();
        let resp = home.join(format!(".cache/tmgr-manager/resp/{stem}.json"));
        let end = Instant::now() + Duration::from_secs(5);
        while !resp.exists() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        let v: Value = serde_json::from_slice(&fs::read(resp).unwrap()).unwrap();
        assert!(v["error"].as_str().unwrap().contains("not available"));
    }

    #[test]
    fn a_switch_of_an_untrusted_cwd_is_refused_and_starts_nothing() {
        let (t, rt) = pair();
        let home = t.path().join("homes").join(MGR);
        let stem = plant(&home, "switch_agent");
        let d = core::req_dir(&home);
        fs::write(
            d.join(format!("{stem}.json")),
            json!({"v":1,"id":stem,"tool":"switch_agent","args":{"session": OTHER, "to": "codex"}})
                .to_string(),
        )
        .unwrap();
        rt.manager_pass();
        let resp = home.join(format!(".cache/tmgr-manager/resp/{stem}.json"));
        let end = Instant::now() + Duration::from_secs(5);
        while !resp.exists() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        let v: Value = serde_json::from_slice(&fs::read(resp).unwrap()).unwrap();
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("trusted folder"));
        assert_eq!(rt.all().len(), 2);
    }

    #[test]
    fn a_trusted_cwd_passes_the_trust_check() {
        let (t, rt) = pair();
        let homes = t.path().join("homes");
        fs::create_dir_all(&homes).unwrap();
        fs::set_permissions(&homes, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
        fs::set_permissions(
            t.path(),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .unwrap();
        folders::add(&homes, t.path(), None, 0).unwrap();
        let host = ManagerHost { rt: &rt, me: MGR };
        let e = core::Host::switch(&host, OTHER, "codex")
            .err()
            .unwrap_or_default();
        assert!(!e.contains("trusted folder"), "{e}");
    }

    #[test]
    fn a_panic_in_the_switch_becomes_an_error_response() {
        let r = guarded("rid", || panic!("boom"));
        assert!(!r.ok);
        assert_eq!(r.id, "rid");
        assert!(r.error.unwrap().contains("unexpectedly"));
        assert!(guarded("rid", || Ok(json!(1))).ok);
    }

    #[test]
    fn without_a_manager_no_spool_is_read() {
        let t = tempfile::Builder::new()
            .prefix("mh")
            .tempdir_in("/tmp")
            .unwrap();
        let rt = runner(t.path());
        add(&rt, t.path(), OTHER, false);
        let homes = t.path().join("homes");
        let stem = plant(&homes.join(OTHER), "list_sessions");
        assert!(rt.live_manager().is_none());
        rt.manager_pass();
        assert!(core::req_dir(&homes.join(OTHER))
            .join(format!("{stem}.json"))
            .exists());
    }

    #[test]
    fn a_manager_spool_that_is_a_symlink_is_refused_and_answers_nothing() {
        let (t, rt) = pair();
        let homes = t.path().join("homes");
        let elsewhere = t.path().join("elsewhere");
        fs::create_dir_all(elsewhere.join("req")).unwrap();
        let stem = format!("{}-1-ab", now_ms());
        fs::write(
            elsewhere.join("req").join(format!("{stem}.json")),
            json!({"v":1,"id":stem,"tool":"list_sessions"}).to_string(),
        )
        .unwrap();
        fs::create_dir_all(homes.join(MGR).join(".cache")).unwrap();
        std::os::unix::fs::symlink(&elsewhere, homes.join(MGR).join(".cache/tmgr-manager"))
            .unwrap();
        rt.manager_pass();
        assert!(elsewhere.join("req").join(format!("{stem}.json")).exists());
        assert!(!elsewhere.join("resp").exists());
    }

    #[test]
    fn a_second_manager_create_is_refused_while_one_lives() {
        let (_t, rt) = pair();
        assert_eq!(rt.live_manager().unwrap().id, MGR);
        let req: RunCreateReq = serde_json::from_value(json!({
            "profile": "claude-code", "cwd": "/tmp", "manager": true
        }))
        .unwrap();
        let e = rt.create_run(req.clone(), true).unwrap_err();
        assert_eq!(e.code, ErrorCode::Busy);
        assert!(rt.check_manager_create("desktop", &req).is_err());
        let mut other = req;
        other.profile = "opencode".into();
        assert_eq!(
            rt.check_manager_create(CLIENT_CLI, &other)
                .unwrap_err()
                .code,
            ErrorCode::InvalidParams
        );
    }

    #[test]
    fn a_folder_swapped_after_the_trust_check_is_refused_at_spawn() {
        use std::os::unix::fs::MetadataExt;
        let (t, rt) = pair();
        let dir = fs::canonicalize(t.path()).unwrap().join("proj");
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(t.path().join("homes")).unwrap();
        folders::add(&t.path().join("homes"), &dir, None, 1).unwrap();
        let host = ManagerHost { rt: &rt, me: MGR };
        let trusted = host.trusted_dir(dir.to_str().unwrap()).unwrap();
        let meta = fs::metadata(&dir).unwrap();
        assert_eq!((trusted.pin.dev, trusted.pin.ino), (meta.dev(), meta.ino()));
        let spec = |pin| core::SpawnSpec {
            profile: "claude-code".into(),
            cwd: dir.clone(),
            name: None,
            mode: "ask".into(),
            parent: Some(MGR.into()),
            card: None,
            pin: Some(pin),
        };
        let moved = t.path().join("proj-moved");
        fs::rename(&dir, &moved).unwrap();
        fs::create_dir_all(&dir).unwrap();
        let err = host.spawn(spec(trusted.pin)).unwrap_err();
        assert_eq!(err, "approved folder changed");
    }

    #[test]
    fn precheck_refuses_what_the_run_plan_would_refuse() {
        let (t, rt) = pair();
        let host = ManagerHost { rt: &rt, me: MGR };
        let spec = |profile: &str, mode: &str, card: Option<&str>| core::SpawnSpec {
            profile: profile.into(),
            cwd: t.path().to_path_buf(),
            name: None,
            mode: mode.into(),
            parent: Some(MGR.into()),
            card: card.map(String::from),
            pin: None,
        };
        host.precheck_spawn(&spec("claude-code", "bypass", Some("card_1")))
            .unwrap();
        host.precheck_spawn(&spec("claude-code", "ask", None))
            .unwrap();
        assert!(host
            .precheck_spawn(&spec("opencode", "bypass", Some("card_1")))
            .unwrap_err()
            .contains("cannot run in mode bypass"));
        assert!(host
            .precheck_spawn(&spec("codex", "plan", None))
            .unwrap_err()
            .contains("plan"));
        assert!(host
            .precheck_spawn(&spec("claude-code", "bypass", None))
            .is_err());
    }
}
