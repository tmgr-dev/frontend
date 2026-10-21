use crate::ctx::credential_store;
use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tmgr_core::api::Client;
use tmgr_core::config::{self, Config};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::time_entries::{self, Interval, PostResult, Posted, MAX_BATCH};
use tmgr_ipc::dirs::ensure_private_dir;
use tmgr_runner::humantime::{self, Record, SentRecord, Settings};
use tmgr_runner::wire::write_atomic;

const EXPIRE_S: u64 = 29 * 24 * 3600;
const BACKOFF_FIRST_S: u64 = 60;
const BACKOFF_MAX_S: u64 = 30 * 60;
pub const LOGIN_HINT: &str = "run tmgr login again (needs time:write)";

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SendState {
    pub failures: u32,
    pub next_try_s: u64,
    pub last_enabled: BTreeMap<String, bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl SendState {
    pub fn load(runner_dir: &Path) -> SendState {
        std::fs::read(humantime::time_dir(runner_dir).join("send.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save(&self, runner_dir: &Path) -> std::io::Result<()> {
        let bytes = serde_json::to_vec(self).map_err(std::io::Error::other)?;
        write_atomic(&humantime::time_dir(runner_dir).join("send.json"), &bytes)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FlushReport {
    pub busy: bool,
    pub backed_off: bool,
    pub not_logged_in: bool,
    pub created: usize,
    pub duplicate: usize,
    pub conflict: usize,
    pub skipped: usize,
    pub expired: usize,
    pub invalid: usize,
    pub local: usize,
    pub error: Option<String>,
    pub hint: Option<String>,
}

pub trait Server {
    fn enabled(&self, workspace_id: i64) -> Result<bool>;
    fn post(&self, task_id: u64, intervals: &[Interval]) -> Result<Posted>;
}

impl Server for Client {
    fn enabled(&self, workspace_id: i64) -> Result<bool> {
        time_entries::session_time_enabled(self, workspace_id)
    }

    fn post(&self, task_id: u64, intervals: &[Interval]) -> Result<Posted> {
        time_entries::post(self, task_id, intervals)
    }
}

struct Lock {
    _file: File,
}

impl Lock {
    fn try_acquire(runner_dir: &Path) -> std::io::Result<Option<Lock>> {
        let dir = humantime::time_dir(runner_dir);
        ensure_private_dir(&dir)?;
        let f = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(dir.join("send.lock"))?;
        let rc = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        Ok((rc == 0).then_some(Lock { _file: f }))
    }
}

pub fn flush(
    runner_dir: &Path,
    now_s: u64,
    connect: &dyn Fn() -> Result<Client>,
) -> Result<FlushReport> {
    flush_with(runner_dir, now_s, &|| {
        connect().map(|c| Box::new(c) as Box<dyn Server>)
    })
}

pub fn flush_default(runner_dir: &Path) -> Result<FlushReport> {
    let now_s = Utc::now().timestamp().max(0) as u64;
    flush(runner_dir, now_s, &default_client)
}

fn default_client() -> Result<Client> {
    let dir = config::config_dir()?;
    let cfg = Config::load(&dir)?;
    let over = std::env::var("TMGR_SERVER").ok().filter(|s| !s.is_empty());
    let server = cfg.effective_server(over.as_deref())?;
    let store = credential_store(&dir, &cfg, &server);
    let (tokens, _) = store.load()?.ok_or_else(Error::not_logged_in)?;
    Client::new(
        &server,
        &crate::ctx::Ctx::user_agent(),
        Some(tokens),
        Some(store),
        false,
    )
}

fn sent(id: &str, status: &str, counted_s: Option<u64>, at_s: u64) -> SentRecord {
    SentRecord {
        id: id.to_string(),
        status: status.to_string(),
        counted_s,
        at_s,
    }
}

fn backoff_s(failures: u32) -> u64 {
    BACKOFF_FIRST_S
        .saturating_mul(1u64 << failures.saturating_sub(1).min(16))
        .min(BACKOFF_MAX_S)
}

struct Run<'a> {
    dir: &'a Path,
    now_s: u64,
    report: FlushReport,
    state: SendState,
    recorded: HashSet<String>,
}

impl Run<'_> {
    fn record(&mut self, items: Vec<SentRecord>) -> Result<()> {
        for i in &items {
            self.recorded.insert(i.id.clone());
            match i.status.as_str() {
                "created" => self.report.created += 1,
                "duplicate" => self.report.duplicate += 1,
                "conflict" => self.report.conflict += 1,
                "skipped" => self.report.skipped += 1,
                "expired" => self.report.expired += 1,
                "invalid" => self.report.invalid += 1,
                "local" => self.report.local += 1,
                _ => {}
            }
        }
        humantime::append_sent(self.dir, &items)?;
        Ok(())
    }

    fn mark(&mut self, records: &[&Record], status: &str) -> Result<()> {
        let at = self.now_s;
        self.record(
            records
                .iter()
                .map(|r| sent(&r.id, status, None, at))
                .collect(),
        )
    }

    fn fail(&mut self, e: &Error) {
        if e.kind == Kind::Auth {
            self.report.not_logged_in = true;
            self.state.hint = Some("run tmgr login".to_string());
        } else {
            self.state.failures += 1;
            self.state.next_try_s = self.now_s + backoff_s(self.state.failures);
            if e.kind == Kind::Forbidden {
                self.report.hint = Some(LOGIN_HINT.to_string());
                self.state.hint = Some(LOGIN_HINT.to_string());
            }
        }
        self.report.error = Some(e.message.clone());
    }
}

fn interval_of(r: &Record) -> Option<Interval> {
    let seconds = r.typing_s + r.reading_s;
    if seconds == 0
        || r.end_s <= r.start_s
        || seconds > r.end_s - r.start_s
        || time_entries::validate_interval_id(&r.id).is_err()
    {
        return None;
    }
    let at = |s: u64| Utc.timestamp_opt(s as i64, 0).single();
    Some(Interval {
        client_interval_id: r.id.clone(),
        started_at: at(r.start_s)?,
        ended_at: at(r.end_s)?,
        seconds,
    })
}

pub fn flush_with(
    runner_dir: &Path,
    now_s: u64,
    connect: &dyn Fn() -> Result<Box<dyn Server>>,
) -> Result<FlushReport> {
    let Some(_lock) = Lock::try_acquire(runner_dir)? else {
        return Ok(FlushReport {
            busy: true,
            ..FlushReport::default()
        });
    };
    let done = humantime::read_sent(runner_dir);
    let records = humantime::read_records(runner_dir);
    let pending: Vec<&Record> = records
        .iter()
        .filter(|r| r.task.is_some() && !done.contains_key(&r.id))
        .collect();
    let mut run = Run {
        dir: runner_dir,
        now_s,
        report: FlushReport::default(),
        state: SendState::load(runner_dir),
        recorded: HashSet::new(),
    };
    if pending.is_empty() {
        return Ok(run.report);
    }
    if !Settings::load(runner_dir).sync {
        run.mark(&pending, "skipped")?;
        return Ok(run.report);
    }
    let mut sendable = Vec::new();
    let (mut local, mut expired) = (Vec::new(), Vec::new());
    for r in pending {
        let ws = r.task.map_or(0, |t| t.workspace_id);
        if ws <= 0 {
            local.push(r);
        } else if r.start_s + EXPIRE_S < now_s {
            expired.push(r);
        } else {
            sendable.push(r);
        }
    }
    run.mark(&local, "local")?;
    run.mark(&expired, "expired")?;
    if sendable.is_empty() {
        return Ok(run.report);
    }
    if run.state.next_try_s > now_s {
        run.report.backed_off = true;
        return Ok(run.report);
    }
    let server = match connect() {
        Ok(s) => s,
        Err(e) => {
            run.fail(&e);
            run.state.save(runner_dir)?;
            return Ok(run.report);
        }
    };
    let outcome = send_all(&mut run, server.as_ref(), sendable);
    if let Err(e) = outcome {
        run.fail(&e);
    } else {
        run.state.failures = 0;
        run.state.next_try_s = 0;
        run.state.hint = None;
    }
    run.state.save(runner_dir)?;
    Ok(run.report)
}

fn send_all(run: &mut Run, server: &dyn Server, sendable: Vec<&Record>) -> Result<()> {
    let mut by_ws: BTreeMap<i64, BTreeMap<i64, Vec<&Record>>> = BTreeMap::new();
    for r in sendable {
        let t = r.task.expect("sendable records are bound");
        by_ws
            .entry(t.workspace_id)
            .or_default()
            .entry(t.task_id)
            .or_default()
            .push(r);
    }
    for (ws, tasks) in by_ws {
        let all: Vec<&Record> = tasks.values().flatten().copied().collect();
        let enabled = server.enabled(ws)?;
        run.state.last_enabled.insert(ws.to_string(), enabled);
        if !enabled {
            run.mark(&all, "skipped")?;
            continue;
        }
        'tasks: for (task_id, mut recs) in tasks {
            recs.sort_by_key(|r| r.start_s);
            let mut queue: Vec<&Record> = Vec::new();
            for r in recs {
                if interval_of(r).is_some() {
                    queue.push(r);
                } else {
                    run.mark(&[r], "invalid")?;
                }
            }
            let Ok(task_id) = u64::try_from(task_id) else {
                run.mark(&queue, "invalid")?;
                continue;
            };
            while !queue.is_empty() {
                let take = queue.len().min(MAX_BATCH);
                let batch: Vec<&Record> = queue[..take].to_vec();
                let intervals: Vec<Interval> =
                    batch.iter().filter_map(|r| interval_of(r)).collect();
                match server.post(task_id, &intervals)? {
                    Posted::Done(result) => {
                        record_result(run, &batch, &result)?;
                        queue.drain(..take);
                    }
                    Posted::Invalid(index) => {
                        run.mark(&[batch[index]], "invalid")?;
                        queue.remove(index);
                    }
                    Posted::Disabled => {
                        run.state.last_enabled.insert(ws.to_string(), false);
                        let rest: Vec<&Record> = all
                            .iter()
                            .filter(|r| !run.recorded.contains(&r.id))
                            .copied()
                            .collect();
                        run.mark(&rest, "skipped")?;
                        break 'tasks;
                    }
                }
            }
        }
    }
    Ok(())
}

fn record_result(run: &mut Run, batch: &[&Record], result: &PostResult) -> Result<()> {
    let by_id: HashMap<&str, _> = result
        .entries
        .iter()
        .map(|e| (e.client_interval_id.as_str(), e))
        .collect();
    let at = run.now_s;
    let items = batch
        .iter()
        .filter_map(|r| {
            let e = by_id.get(r.id.as_str())?;
            matches!(e.status.as_str(), "created" | "duplicate" | "conflict")
                .then(|| sent(&r.id, &e.status, e.counted_seconds, at))
        })
        .collect();
    run.record(items)
}

const FIRST_DELAY: Duration = Duration::from_secs(10);
const INTERVAL: Duration = Duration::from_secs(300);
const FINAL_WAIT: Duration = Duration::from_secs(3);

pub struct Background {
    stop: mpsc::Sender<()>,
    dir: PathBuf,
}

impl Background {
    pub fn start(dir: PathBuf) -> Background {
        let (stop, rx) = mpsc::channel::<()>();
        let worker_dir = dir.clone();
        thread::spawn(move || {
            let mut wait = FIRST_DELAY;
            while let Err(mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(wait) {
                let _ = flush_default(&worker_dir);
                wait = INTERVAL;
            }
        });
        Background { stop, dir }
    }

    pub fn finish(self) {
        let _ = self.stop.send(());
        let (done, wait) = mpsc::channel();
        thread::spawn(move || {
            let _ = flush_default(&self.dir);
            let _ = done.send(());
        });
        let _ = wait.recv_timeout(FINAL_WAIT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::rc::Rc;
    use tmgr_core::time_entries::EntryResult;
    use tmgr_ipc::api::TaskRef;

    const NOW: u64 = 2_000_000_000;

    #[derive(Default)]
    struct Fake {
        enabled: RefCell<HashMap<i64, bool>>,
        posts: RefCell<Vec<(u64, Vec<String>)>>,
        script: RefCell<VecDeque<Result<Posted>>>,
        connects: Cell<usize>,
    }

    impl Server for Rc<Fake> {
        fn enabled(&self, workspace_id: i64) -> Result<bool> {
            Ok(*self.enabled.borrow().get(&workspace_id).unwrap_or(&true))
        }

        fn post(&self, task_id: u64, intervals: &[Interval]) -> Result<Posted> {
            let ids: Vec<String> = intervals
                .iter()
                .map(|i| i.client_interval_id.clone())
                .collect();
            self.posts.borrow_mut().push((task_id, ids.clone()));
            if let Some(next) = self.script.borrow_mut().pop_front() {
                return next;
            }
            Ok(done(&ids, "created"))
        }
    }

    fn done(ids: &[String], status: &str) -> Posted {
        Posted::Done(PostResult {
            task_id: None,
            common_time: None,
            entries: ids
                .iter()
                .map(|id| EntryResult {
                    client_interval_id: id.clone(),
                    status: status.into(),
                    id: None,
                    seconds: None,
                    counted_seconds: Some(30),
                })
                .collect(),
        })
    }

    fn rec(name: &str, ws: i64, task: i64, start_s: u64) -> Record {
        Record {
            id: format!("{name}-{start_s}"),
            session: name.into(),
            name: name.into(),
            cwd: "/work".into(),
            task: Some(TaskRef {
                workspace_id: ws,
                task_id: task,
            }),
            start_s,
            end_s: start_s + 30,
            typing_s: 10,
            reading_s: 20,
        }
    }

    fn recent(name: &str, ws: i64, task: i64, n: u64) -> Record {
        rec(name, ws, task, NOW - 10_000 + n * 40)
    }

    fn dir_with(records: &[Record]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        humantime::append_records(d.path(), records).unwrap();
        d
    }

    fn go(dir: &Path, now: u64, fake: &Rc<Fake>) -> FlushReport {
        flush_with(dir, now, &|| {
            fake.connects.set(fake.connects.get() + 1);
            Ok(Box::new(fake.clone()) as Box<dyn Server>)
        })
        .unwrap()
    }

    fn status_of(dir: &Path, id: &str) -> Option<String> {
        humantime::read_sent(dir).get(id).map(|s| s.status.clone())
    }

    #[test]
    fn nothing_pending_never_connects() {
        let mut unbound = recent("a", 1, 1, 0);
        unbound.task = None;
        let d = dir_with(&[unbound.clone()]);
        let fake = Rc::new(Fake::default());
        assert_eq!(go(d.path(), NOW, &fake), FlushReport::default());
        assert_eq!(fake.connects.get(), 0);
        assert!(status_of(d.path(), &unbound.id).is_none());
    }

    #[test]
    fn local_sync_off_skips_without_connecting() {
        let r = recent("a", 1, 1, 0);
        let d = dir_with(&[r.clone()]);
        Settings {
            sync: false,
            ..Settings::default()
        }
        .save(d.path())
        .unwrap();
        let fake = Rc::new(Fake::default());
        let report = go(d.path(), NOW, &fake);
        assert_eq!(report.skipped, 1);
        assert_eq!(fake.connects.get(), 0);
        assert_eq!(status_of(d.path(), &r.id).as_deref(), Some("skipped"));
    }

    #[test]
    fn a_server_toggle_that_is_off_posts_nothing_and_skips() {
        let r = recent("a", 7, 1, 0);
        let d = dir_with(&[r.clone()]);
        let fake = Rc::new(Fake::default());
        fake.enabled.borrow_mut().insert(7, false);
        let report = go(d.path(), NOW, &fake);
        assert_eq!(report.skipped, 1);
        assert!(fake.posts.borrow().is_empty());
        assert_eq!(status_of(d.path(), &r.id).as_deref(), Some("skipped"));
        assert_eq!(SendState::load(d.path()).last_enabled["7"], false);
    }

    #[test]
    fn disabled_on_post_skips_the_whole_workspace() {
        let recs = [
            recent("a", 7, 1, 0),
            recent("b", 7, 2, 1),
            recent("c", 8, 3, 2),
        ];
        let d = dir_with(&recs);
        let fake = Rc::new(Fake::default());
        fake.script.borrow_mut().push_back(Ok(Posted::Disabled));
        let report = go(d.path(), NOW, &fake);
        assert_eq!(report.skipped, 2);
        assert_eq!(report.created, 1);
        assert_eq!(status_of(d.path(), &recs[0].id).as_deref(), Some("skipped"));
        assert_eq!(status_of(d.path(), &recs[1].id).as_deref(), Some("skipped"));
        assert_eq!(status_of(d.path(), &recs[2].id).as_deref(), Some("created"));
    }

    #[test]
    fn offline_keeps_the_queue_backs_off_and_retries_the_same_ids() {
        let r = recent("a", 1, 5, 0);
        let d = dir_with(&[r.clone()]);
        let fake = Rc::new(Fake::default());
        fake.script
            .borrow_mut()
            .push_back(Err(Error::new(Kind::Offline, "down")));
        let report = go(d.path(), NOW, &fake);
        assert!(report.error.is_some());
        assert!(status_of(d.path(), &r.id).is_none());
        let state = SendState::load(d.path());
        assert_eq!((state.failures, state.next_try_s), (1, NOW + 60));

        let early = go(d.path(), NOW + 10, &fake);
        assert!(early.backed_off);
        assert_eq!(fake.connects.get(), 1);

        fake.script
            .borrow_mut()
            .push_back(Ok(done(&[r.id.clone()], "duplicate")));
        let retry = go(d.path(), NOW + 61, &fake);
        assert_eq!(retry.duplicate, 1);
        let posts = fake.posts.borrow();
        assert_eq!(posts[0], posts[1]);
        assert_eq!(status_of(d.path(), &r.id).as_deref(), Some("duplicate"));
        let state = SendState::load(d.path());
        assert_eq!((state.failures, state.next_try_s), (0, 0));
    }

    #[test]
    fn backoff_doubles_up_to_thirty_minutes() {
        let got: Vec<u64> = (1..=8).map(backoff_s).collect();
        assert_eq!(got, [60, 120, 240, 480, 960, 1800, 1800, 1800]);
        assert_eq!(backoff_s(200), 1800);
    }

    #[test]
    fn a_conflict_is_recorded_and_never_resent() {
        let r = recent("a", 1, 5, 0);
        let d = dir_with(&[r.clone()]);
        let fake = Rc::new(Fake::default());
        fake.script
            .borrow_mut()
            .push_back(Ok(done(&[r.id.clone()], "conflict")));
        assert_eq!(go(d.path(), NOW, &fake).conflict, 1);
        go(d.path(), NOW + 1, &fake);
        assert_eq!(fake.posts.borrow().len(), 1);
        assert_eq!(fake.connects.get(), 1);
    }

    #[test]
    fn an_invalid_entry_is_dropped_and_the_rest_resent() {
        let recs = [
            recent("a", 1, 5, 0),
            recent("a", 1, 5, 1),
            recent("a", 1, 5, 2),
        ];
        let d = dir_with(&recs);
        let fake = Rc::new(Fake::default());
        fake.script.borrow_mut().push_back(Ok(Posted::Invalid(1)));
        let report = go(d.path(), NOW, &fake);
        assert_eq!((report.invalid, report.created), (1, 2));
        assert_eq!(status_of(d.path(), &recs[1].id).as_deref(), Some("invalid"));
        let posts = fake.posts.borrow();
        assert_eq!(posts[0].1.len(), 3);
        assert_eq!(posts[1].1, vec![recs[0].id.clone(), recs[2].id.clone()]);
    }

    #[test]
    fn local_and_old_records_are_closed_without_the_network() {
        let local = recent("a", -3, 1, 0);
        let old = rec("b", 1, 1, NOW - 30 * 24 * 3600);
        let d = dir_with(&[local.clone(), old.clone()]);
        let fake = Rc::new(Fake::default());
        let report = go(d.path(), NOW, &fake);
        assert_eq!((report.local, report.expired), (1, 1));
        assert_eq!(fake.connects.get(), 0);
        assert_eq!(status_of(d.path(), &local.id).as_deref(), Some("local"));
        assert_eq!(status_of(d.path(), &old.id).as_deref(), Some("expired"));
    }

    #[test]
    fn a_task_is_sent_in_batches_of_one_hundred() {
        let recs: Vec<_> = (0..250).map(|n| recent("a", 1, 5, n)).collect();
        let d = dir_with(&recs);
        let fake = Rc::new(Fake::default());
        let report = go(d.path(), NOW, &fake);
        assert_eq!(report.created, 250);
        let sizes: Vec<usize> = fake.posts.borrow().iter().map(|p| p.1.len()).collect();
        assert_eq!(sizes, [100, 100, 50]);
    }

    #[test]
    fn a_second_flush_while_one_runs_is_skipped() {
        let d = dir_with(&[recent("a", 1, 5, 0)]);
        let held = Lock::try_acquire(d.path()).unwrap().unwrap();
        let fake = Rc::new(Fake::default());
        let report = go(d.path(), NOW, &fake);
        assert!(report.busy);
        assert_eq!(fake.connects.get(), 0);
        drop(held);
        assert_eq!(go(d.path(), NOW, &fake).created, 1);
    }

    #[test]
    fn a_forbidden_answer_keeps_the_queue_and_asks_for_a_new_login() {
        let r = recent("a", 1, 5, 0);
        let d = dir_with(&[r.clone()]);
        let fake = Rc::new(Fake::default());
        fake.script
            .borrow_mut()
            .push_back(Err(Error::new(Kind::Forbidden, "no")));
        let report = go(d.path(), NOW, &fake);
        assert_eq!(report.hint.as_deref(), Some(LOGIN_HINT));
        assert!(status_of(d.path(), &r.id).is_none());
        let state = SendState::load(d.path());
        assert_eq!(state.hint.as_deref(), Some(LOGIN_HINT));
        assert_eq!(state.next_try_s, NOW + 60);
    }

    #[test]
    fn not_logged_in_keeps_the_queue_without_backoff() {
        let r = recent("a", 1, 5, 0);
        let d = dir_with(&[r.clone()]);
        let report = flush_with(d.path(), NOW, &|| Err(Error::not_logged_in())).unwrap();
        assert!(report.not_logged_in);
        assert!(status_of(d.path(), &r.id).is_none());
        assert_eq!(SendState::load(d.path()).next_try_s, 0);
    }
}
