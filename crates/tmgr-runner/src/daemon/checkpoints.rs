use super::*;
use crate::checkpoint::{CpError, Store, DEFAULT_KEEP};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::SystemTime;
use tmgr_ipc::api::{
    CheckpointDiffParams, CheckpointEnableParams, CheckpointList, CheckpointRestoreParams,
    CheckpointStatus,
};

const KEEP_RANGE: std::ops::RangeInclusive<u32> = 3..=1000;
const MARKER_DIR: &str = ".cache/tmgr-turn";

pub(super) struct Cp {
    lock: Mutex<()>,
    pending: AtomicBool,
    busy: AtomicBool,
    removed: AtomicBool,
    mark: Mutex<Option<SystemTime>>,
    status: Mutex<Option<CheckpointStatus>>,
    homes: PathBuf,
}

impl Cp {
    pub(super) fn new(homes: PathBuf) -> Cp {
        Cp {
            lock: Mutex::new(()),
            pending: AtomicBool::new(false),
            busy: AtomicBool::new(false),
            removed: AtomicBool::new(false),
            mark: Mutex::new(None),
            status: Mutex::new(None),
            homes,
        }
    }
}

fn cp_err(e: CpError) -> ApiErr {
    match e {
        CpError::NotGit(m) | CpError::Refused(m) => api(ErrorCode::InvalidParams, m),
        CpError::Failed(m) => api(ErrorCode::Internal, m),
    }
}

fn overlaps(a: &Path, b: &Path) -> bool {
    let a = a.canonicalize().unwrap_or_else(|_| a.to_path_buf());
    let b = b.canonicalize().unwrap_or_else(|_| b.to_path_buf());
    a.starts_with(&b) || b.starts_with(&a)
}

fn working(s: &Session) -> bool {
    matches!(s.state().kind, StateKind::Working | StateKind::Starting)
}

impl Session {
    fn cp_keep(&self) -> Option<u32> {
        lock(&self.inner).meta.checkpoints
    }

    fn checkpoint_store(&self) -> Option<Store> {
        let i = lock(&self.inner);
        Some(Store {
            session: self.id.clone(),
            sdir: self.dir.clone(),
            cwd: PathBuf::from(&i.meta.cwd),
            keep: i.meta.checkpoints?,
        })
    }

    pub(super) fn cp_info(&self, i: &Inner) -> Option<CheckpointStatus> {
        i.meta.checkpoints?;
        Some(lock(&self.cp.status).clone().unwrap_or(CheckpointStatus {
            available: false,
            note: None,
            count: 0,
        }))
    }

    pub(super) fn cp_refresh(&self) {
        if let Some(store) = self.checkpoint_store() {
            *lock(&self.cp.status) = Some(store.status());
        }
    }

    pub(super) fn cp_note(&self, states: &[State]) {
        let due = states
            .iter()
            .any(|s| matches!(s.kind, StateKind::Waiting | StateKind::Exited));
        if due && self.cp_keep().is_some() {
            self.cp.pending.store(true, Ordering::SeqCst);
        }
    }

    fn marker_mtime(&self) -> Option<SystemTime> {
        let rel = Path::new(MARKER_DIR).join(&self.id);
        let real = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|h| h.is_absolute());
        [Some(self.cp.homes.join(&self.id)), real]
            .into_iter()
            .flatten()
            .filter_map(|base| fs::symlink_metadata(base.join(&rel)).ok()?.modified().ok())
            .max()
    }

    pub(super) fn cp_tick(self: &Arc<Self>) {
        if self.cp_keep().is_none() {
            return;
        }
        if let Some(t) = self.marker_mtime() {
            let mut seen = lock(&self.cp.mark);
            if *seen != Some(t) {
                *seen = Some(t);
                self.cp.pending.store(true, Ordering::SeqCst);
            }
        }
        self.cp_spawn();
    }

    pub(super) fn cp_spawn(self: &Arc<Self>) {
        if !self.cp.pending.load(Ordering::SeqCst) || self.cp.busy.swap(true, Ordering::SeqCst) {
            return;
        }
        let s = self.clone();
        thread::spawn(move || loop {
            while s.cp.pending.load(Ordering::SeqCst) {
                let _ = catch_unwind(AssertUnwindSafe(|| s.cp_snapshot()));
            }
            s.cp.busy.store(false, Ordering::SeqCst);
            if !s.cp.pending.load(Ordering::SeqCst) || s.cp.busy.swap(true, Ordering::SeqCst) {
                break;
            }
        });
    }

    fn cp_snapshot(&self) {
        let _g = lock(&self.cp.lock);
        self.cp.pending.store(false, Ordering::SeqCst);
        if self.cp.removed.load(Ordering::SeqCst) {
            return;
        }
        let Some(store) = self.checkpoint_store() else {
            return;
        };
        if let Err(e) = store.snapshot() {
            log(&format!("session {}: checkpoint not saved: {e:?}", self.id));
        }
        *lock(&self.cp.status) = Some(store.status());
    }

    pub(super) fn cp_forget(&self) {
        let _g = lock(&self.cp.lock);
        if !self.cp.removed.swap(true, Ordering::SeqCst) {
            if let Some(store) = self.checkpoint_store() {
                store.remove_all();
            }
        }
        if let Some(h) = std::env::var_os("HOME").map(PathBuf::from) {
            if h.is_absolute() {
                let _ = fs::remove_file(h.join(MARKER_DIR).join(&self.id));
            }
        }
    }
}

impl Runner {
    pub(super) fn checkpoints_call(&self, m: &str, p: &Value) -> Result<Value, ApiErr> {
        let id = session_id_param(p)?;
        let s = self
            .session(&id)
            .ok_or_else(|| api(ErrorCode::NotFound, "no such session"))?;
        if m == method::CHECKPOINTS_ENABLE {
            return self.cp_enable(&s, params(p)?);
        }
        let store = s.checkpoint_store().ok_or_else(|| {
            api(
                ErrorCode::InvalidParams,
                "checkpoints are off for this session",
            )
        })?;
        match m {
            method::CHECKPOINTS_LIST => Ok(to_json(&CheckpointList {
                status: store.status(),
                items: store.list(),
            })),
            method::CHECKPOINTS_DIFF => {
                let q: CheckpointDiffParams = params(p)?;
                let _g = lock(&s.cp.lock);
                store
                    .diff(q.n, q.against)
                    .map(|d| to_json(&d))
                    .map_err(cp_err)
            }
            method::CHECKPOINTS_RESTORE => {
                let q: CheckpointRestoreParams = params(p)?;
                self.cp_check_idle(&s)?;
                let _g = lock(&s.cp.lock);
                self.cp_check_idle(&s)?;
                let out = store.restore(q.n);
                *lock(&s.cp.status) = Some(store.status());
                out.map(|r| to_json(&r)).map_err(cp_err)
            }
            other => Err(api(
                ErrorCode::UnknownMethod,
                format!("unknown method {other}"),
            )),
        }
    }

    fn cp_enable(&self, s: &Arc<Session>, q: CheckpointEnableParams) -> Result<Value, ApiErr> {
        let keep = q.keep.unwrap_or(DEFAULT_KEEP);
        if !KEEP_RANGE.contains(&keep) {
            return Err(api(ErrorCode::InvalidParams, "keep must be from 3 to 1000"));
        }
        lock(&s.inner).meta.checkpoints = Some(keep);
        s.persist();
        s.cp_refresh();
        s.cp.pending.store(true, Ordering::SeqCst);
        s.cp_spawn();
        let store = s.checkpoint_store();
        Ok(to_json(&CheckpointList {
            status: store
                .as_ref()
                .map(Store::status)
                .unwrap_or(CheckpointStatus {
                    available: false,
                    note: None,
                    count: 0,
                }),
            items: store.map(|st| st.list()).unwrap_or_default(),
        }))
    }

    fn cp_check_idle(&self, s: &Session) -> Result<(), ApiErr> {
        if working(s) {
            return Err(api(
                ErrorCode::Busy,
                "the session is working; wait until it is idle to restore",
            ));
        }
        let cwd = PathBuf::from(&lock(&s.inner).meta.cwd);
        for o in self.all() {
            if o.id == s.id || o.is_exited() {
                continue;
            }
            let (name, other) = {
                let i = lock(&o.inner);
                (i.meta.name.clone(), PathBuf::from(&i.meta.cwd))
            };
            if overlaps(&cwd, &other) {
                return Err(api(
                    ErrorCode::Busy,
                    format!(
                        "session {name} ({}) is running in {}, which overlaps this folder; stop it before restoring",
                        o.id,
                        other.display()
                    ),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn carry_checkpoints(&self, old: &str, new: &str) {
        let (Some(os), Some(ns)) = (self.session(old), self.session(new)) else {
            return;
        };
        let (Some(keep), Some(from)) = (os.cp_keep(), os.checkpoint_store()) else {
            return;
        };
        let _g = lock(&os.cp.lock);
        let _n = lock(&ns.cp.lock);
        lock(&ns.inner).meta.checkpoints = Some(keep);
        let Some(to) = ns.checkpoint_store() else {
            return;
        };
        match from.transfer(&to) {
            Ok(()) => os.cp.removed.store(true, Ordering::SeqCst),
            Err(e) => log(&format!(
                "session {old}: checkpoints not carried to {new}: {e:?}"
            )),
        }
        ns.persist();
        ns.cp_refresh();
    }
}

#[cfg(test)]
mod tests {
    use super::overlaps;
    use std::fs;

    #[test]
    fn overlap_covers_the_same_folder_and_nesting_either_way_through_symlinks() {
        let t = tempfile::tempdir().unwrap();
        let repo = t.path().join("repo");
        let sub = repo.join("sub");
        let sibling = t.path().join("repo-worktrees/s");
        fs::create_dir_all(&sub).unwrap();
        fs::create_dir_all(&sibling).unwrap();
        let link = t.path().join("link");
        std::os::unix::fs::symlink(&sub, &link).unwrap();
        assert!(overlaps(&repo, &repo));
        assert!(overlaps(&repo, &sub));
        assert!(overlaps(&sub, &repo));
        assert!(overlaps(&repo, &link));
        assert!(overlaps(&link, &repo));
        assert!(!overlaps(&repo, &sibling));
        assert!(!overlaps(&sibling, &repo));
    }
}
