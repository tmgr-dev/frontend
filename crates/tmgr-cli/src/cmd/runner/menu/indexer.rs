use crate::cmd::runner::attach_consent::Reads;
use crate::cmd::runner::catalog::{
    self,
    index::{self, Index},
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use tmgr_usage::paths::Env;

const STARTING: u8 = 0;
const READY: u8 = 1;
const DONE: u8 = 2;
const BUSY: u8 = 3;
const FAILED: u8 = 4;

#[derive(Default)]
struct Shared {
    state: AtomicU8,
    stop: AtomicBool,
    done: AtomicUsize,
    total: AtomicUsize,
}

struct Finish<'a>(&'a Shared);

impl Drop for Finish<'_> {
    fn drop(&mut self) {
        if !matches!(self.0.state.load(Ordering::SeqCst), DONE | BUSY) {
            self.0.state.store(FAILED, Ordering::SeqCst);
        }
    }
}

pub struct Indexer {
    shared: Arc<Shared>,
    path: PathBuf,
}

impl Indexer {
    pub fn start(path: PathBuf, env: Env) -> Indexer {
        let shared = Arc::new(Shared::default());
        let worker = shared.clone();
        let file = path.clone();
        std::thread::spawn(move || {
            let _finish = Finish(&worker);
            let state = match work(&worker, &file, &env) {
                Ok(true) => BUSY,
                Ok(false) => DONE,
                Err(()) => FAILED,
            };
            worker.state.store(state, Ordering::SeqCst);
        });
        Indexer { shared, path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn ready(&self) -> bool {
        matches!(self.state(), READY | DONE | BUSY)
    }

    pub fn complete(&self) -> bool {
        matches!(self.state(), DONE | BUSY)
    }

    pub fn stamp(&self) -> (u8, usize) {
        (self.state(), self.shared.done.load(Ordering::SeqCst))
    }

    fn state(&self) -> u8 {
        self.shared.state.load(Ordering::SeqCst)
    }

    pub fn status(&self) -> Option<String> {
        match self.state() {
            STARTING => Some("Preparing the search index...".into()),
            READY => Some(format!(
                "Indexing sessions {}/{}, results may be partial",
                self.shared.done.load(Ordering::SeqCst),
                self.shared.total.load(Ordering::SeqCst)
            )),
            BUSY => {
                Some("Another tmgr is updating the search index, results may be partial".into())
            }
            FAILED => Some("Search index unavailable, filtering by title and folder only".into()),
            _ => None,
        }
    }
}

impl Drop for Indexer {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
    }
}

fn work(shared: &Shared, path: &Path, env: &Env) -> Result<bool, ()> {
    let reads = Reads::of(env);
    index::enforce_consent(path, reads.claude, reads.codex).map_err(|_| ())?;
    let ix = Index::open(path).map_err(|_| ())?;
    shared.state.store(READY, Ordering::SeqCst);
    let entries = catalog::scan(env, None);
    shared.total.store(
        entries.iter().filter(|e| e.readable).count(),
        Ordering::SeqCst,
    );
    let out = ix
        .update_while(
            &entries,
            |done, _| shared.done.store(done, Ordering::SeqCst),
            || !shared.stop.load(Ordering::SeqCst),
        )
        .map_err(|_| ())?;
    Ok(out.busy)
}
