use crate::cmd::runner::attach_consent::Reads;
use crate::cmd::runner::catalog::{
    self,
    index::{self, Index},
    Agent,
};
use std::cell::RefCell;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tmgr_usage::consent;
use tmgr_usage::paths::{self, Env};

const REASK: Duration = Duration::from_millis(500);

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

fn allows(env: &Env, agent: Agent) -> bool {
    let reads = Reads::of(env);
    match agent {
        Agent::Claude => reads.claude,
        Agent::Codex => reads.codex,
    }
}

fn work(shared: &Shared, path: &Path, env: &Env) -> Result<bool, ()> {
    let dir = paths::ensure_data_dir(env).map_err(|_| ())?;
    let ix = {
        let _lock = consent::scan_lock(&dir).map_err(|_| ())?;
        let reads = Reads::of(env);
        if !reads.any() {
            return Err(());
        }
        index::enforce_consent(path, reads.claude, reads.codex).map_err(|_| ())?;
        Index::open(path).map_err(|_| ())?
    };
    shared.state.store(READY, Ordering::SeqCst);
    let entries = catalog::scan(env, None);
    shared.total.store(
        entries.iter().filter(|e| e.readable).count(),
        Ordering::SeqCst,
    );
    let held = RefCell::new(None);
    let out = ix
        .update_while(
            &entries,
            |done, _| shared.done.store(done, Ordering::SeqCst),
            |e| {
                *held.borrow_mut() = None;
                if shared.stop.load(Ordering::SeqCst) {
                    return false;
                }
                *held.borrow_mut() = consent::scan_lock(&dir).ok();
                held.borrow().is_some() && allows(env, e.agent)
            },
        )
        .map_err(|_| ())?;
    drop(held);
    if !Reads::of(env).any() {
        let _ = consent::remove_index_files(path);
    }
    Ok(out.busy)
}

pub struct Answer {
    pub query: String,
    pub stamp: (u8, usize),
    pub ids: Option<Vec<String>>,
}

type Asked = (String, (u8, usize), Instant);

struct Request {
    query: String,
    stamp: (u8, usize),
    complete: bool,
}

pub struct Searcher {
    requests: Sender<Request>,
    answer: Arc<Mutex<Option<Answer>>>,
    asked: RefCell<Option<Asked>>,
}

impl Searcher {
    pub fn start(path: PathBuf, env: Env) -> Searcher {
        let (requests, inbox) = mpsc::channel::<Request>();
        let answer = Arc::new(Mutex::new(None));
        let out = answer.clone();
        std::thread::spawn(move || {
            let mut open: Option<(Index, u64)> = None;
            while let Ok(mut req) = inbox.recv() {
                while let Ok(next) = inbox.try_recv() {
                    req = next;
                }
                let ids = lookup(&mut open, &path, &env, &req);
                if let Ok(mut slot) = out.lock() {
                    *slot = Some(Answer {
                        query: req.query,
                        stamp: req.stamp,
                        ids,
                    });
                }
            }
        });
        Searcher {
            requests,
            answer,
            asked: RefCell::new(None),
        }
    }

    pub fn latest(&self) -> Option<Answer> {
        let slot = self.answer.lock().ok()?;
        slot.as_ref().map(|a| Answer {
            query: a.query.clone(),
            stamp: a.stamp,
            ids: a.ids.clone(),
        })
    }

    pub fn ask(&self, query: &str, stamp: (u8, usize), complete: bool) {
        let mut asked = self.asked.borrow_mut();
        let repeat = asked
            .as_ref()
            .is_some_and(|(q, s, at)| q == query && (*s == stamp || at.elapsed() < REASK));
        if repeat {
            return;
        }
        *asked = Some((query.to_string(), stamp, Instant::now()));
        let _ = self.requests.send(Request {
            query: query.to_string(),
            stamp,
            complete,
        });
    }
}

fn lookup(
    open: &mut Option<(Index, u64)>,
    path: &Path,
    env: &Env,
    req: &Request,
) -> Option<Vec<String>> {
    let inode = std::fs::metadata(path).ok().map(|m| m.ino());
    if !Reads::of(env).any() || inode.is_none() {
        *open = None;
        return None;
    }
    if open.as_ref().map(|(_, i)| *i) != inode {
        *open = Index::open_read_only(path).ok().zip(inode);
    }
    let (ix, _) = open.as_ref()?;
    super::search_ids(ix, &req.query, req.complete)
}
