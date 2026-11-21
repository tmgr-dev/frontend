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
                    let host = ManagerHost { rt: self, me: &m.id };
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
        let dir = bundle::open_path(fs::canonicalize(cwd).map_err(|_| bad())?)
            .map_err(|_| bad())?;
        if !folders::is_trusted(&self.rt.cfg.homes, &dir.path, dir.id()) {
            return Err(
                "cwd is not a trusted folder; the owner must trust it with tmgr runner trust".into(),
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
