use super::*;
use crate::manager as core;
use tmgr_ipc::manager::SpoolResp;

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
                    for req in reqs {
                        let resp = match core::execute(&host, &self.mgr, &req) {
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

struct ManagerHost<'a> {
    rt: &'a Arc<Runner>,
    me: &'a str,
}

impl core::Host for ManagerHost<'_> {
    fn me(&self) -> &str {
        self.me
    }

    fn machine(&self) -> String {
        "local".into()
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

    fn trusted_dir(&self, cwd: &str) -> Result<PathBuf, String> {
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
        Ok(dir.path.clone())
    }

    fn spawn(&self, spec: core::SpawnSpec) -> Result<SessionInfo, String> {
        let (set, _) = ProfileSet::load(profile::user_dir().as_deref());
        if !set
            .get(&spec.profile)
            .is_some_and(|p| p.source == profile::Source::Builtin)
        {
            return Err("profile must be a built-in agent".into());
        }
        let req: RunCreateReq = serde_json::from_value(serde_json::json!({
            "profile": spec.profile,
            "cwd": spec.cwd.display().to_string(),
            "name": spec.name,
            "permission_mode": spec.mode,
            "parent_session": spec.parent,
        }))
        .map_err(|e| e.to_string())?;
        self.rt
            .create_run_as(req, false, None, None, None)
            .map(|(info, ..)| info)
            .map_err(|e| e.msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const MGR: &str = "s_aaaaaaaaaa";
    const OTHER: &str = "s_bbbbbbbbbb";

    fn runner(t: &Path) -> Arc<Runner> {
        Arc::new(Runner {
            rename_lock: Mutex::new(()),
            cfg: RunnerConfig {
                dir: t.join("run"),
                homes: t.join("homes"),
                self_exe: PathBuf::from("tmgr"),
                quiet_ms: 1000,
                version: "test".into(),
                max_sessions: 4,
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
            mgr: core::State::default(),
        })
    }

    fn add(rt: &Arc<Runner>, t: &Path, id: &str, manager: bool) {
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
}
