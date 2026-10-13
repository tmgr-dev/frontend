mod api;
mod bundle;
pub(crate) mod commands;
mod conn;
mod dir;
mod flag;
mod folder;
#[cfg(all(test, unix))]
mod live;
#[cfg(all(test, unix))]
mod mock;
mod pump;
mod render;
mod sanitize;
mod task_lookup;

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use tmgr_ipc::api::{method, ClientMsg, Resize, Scroll};

use api::{find_session, list_sessions, session_from_value, state_from_value, Detail, RunnerSession, RunnerState, StartRun, TaskRef};
use conn::{input_frame, write_frame, write_json, Conn, ConnError, Welcome, MAX_INPUT, UPDATE_RUNNER};
use dir::Sock;
use flag::Flag;
use pump::{run_attach, run_watch, Coalescer, Shared, TermFrame, WatchEvent};

pub type Connector = Arc<dyn Fn() -> Result<(Conn, Welcome), ConnError> + Send + Sync>;

const MAX_COLS: u32 = tmgr_ipc::api::limits::MAX_COLS as u32;
const MAX_ROWS: u32 = tmgr_ipc::api::limits::MAX_ROWS as u32;
const LIST_THROTTLE: Duration = Duration::from_millis(500);
const WATCH_FLUSH: Duration = Duration::from_millis(100);
const TERM_WRITE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RunnerStatus {
  pub state: &'static str,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub version: Option<String>,
  pub caps: Vec<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TermInfo {
  pub session: String,
  pub name: String,
  pub task: Option<TaskRef>,
  pub state: RunnerState,
}

struct WatchHandle {
  stop: Arc<AtomicBool>,
  sock: Sock,
}

struct TermHandle {
  shared: Arc<Shared>,
  writer: Mutex<Sock>,
  sock: Sock,
}

struct Inner {
  flag: Flag,
  connector: Connector,
  watchers: Mutex<HashMap<u64, WatchHandle>>,
  terms: Mutex<HashMap<String, Arc<TermHandle>>>,
  next_watch: AtomicU64,
  disable_gen: AtomicU64,
  closes: Mutex<HashMap<String, u64>>,
}

#[derive(Clone)]
pub struct Runner(Arc<Inner>);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
  m.lock().unwrap_or_else(|e| e.into_inner())
}

fn err(e: ConnError) -> String {
  e.message().to_owned()
}

pub fn clamp_size(cols: u32, rows: u32) -> (u16, u16) {
  (cols.clamp(1, MAX_COLS) as u16, rows.clamp(1, MAX_ROWS) as u16)
}

impl Runner {
  pub fn new(flag_path: PathBuf, connector: Connector) -> Runner {
    Runner(Arc::new(Inner {
      flag: Flag::load(flag_path),
      connector,
      watchers: Mutex::new(HashMap::new()),
      terms: Mutex::new(HashMap::new()),
      next_watch: AtomicU64::new(1),
      disable_gen: AtomicU64::new(0),
      closes: Mutex::new(HashMap::new()),
    }))
  }

  pub fn with_default_connector(flag_path: PathBuf) -> Runner {
    Runner::new(flag_path, Arc::new(dir::connect))
  }

  pub fn enabled(&self) -> bool {
    self.0.flag.get()
  }

  fn gate(&self) -> Result<(), String> {
    if self.enabled() {
      Ok(())
    } else {
      Err("disabled".to_owned())
    }
  }

  pub fn set_enabled(&self, on: bool) -> Result<(), String> {
    self.0.flag.set(on).map_err(|e| e.to_string())?;
    if !on {
      self.0.disable_gen.fetch_add(1, Ordering::SeqCst);
      self.stop_all();
    }
    Ok(())
  }

  fn generation(&self, sid: Option<&str>) -> (u64, u64) {
    let closes = sid.map_or(0, |s| lock(&self.0.closes).get(s).copied().unwrap_or(0));
    (self.0.disable_gen.load(Ordering::SeqCst), closes)
  }

  fn still_current(&self, sid: Option<&str>, g: (u64, u64)) -> Result<(), String> {
    if !self.enabled() || self.0.disable_gen.load(Ordering::SeqCst) != g.0 {
      return Err("disabled".to_owned());
    }
    if sid.is_some_and(|s| lock(&self.0.closes).get(s).copied().unwrap_or(0) != g.1) {
      return Err("detached".to_owned());
    }
    Ok(())
  }

  fn connect(&self) -> Result<(Conn, Welcome), String> {
    (self.0.connector)().map_err(err)
  }

  pub fn status(&self) -> RunnerStatus {
    let status = |state, version, caps, message| RunnerStatus { state, version, caps, message };
    if !self.enabled() {
      return status("disabled", None, Vec::new(), None);
    }
    match (self.0.connector)() {
      Ok((_, w)) if !w.can_create_runs() => status("incompatible", Some(w.runner_version), w.caps, Some(UPDATE_RUNNER.to_owned())),
      Ok((_, w)) => status("ready", Some(w.runner_version), w.caps, None),
      Err(ConnError::Incompatible(m)) => status("incompatible", None, Vec::new(), Some(m)),
      Err(e) => status("absent", None, Vec::new(), Some(e.message().to_owned())),
    }
  }

  pub fn profiles(&self) -> Result<Vec<Value>, String> {
    self.gate()?;
    Ok(api::PROFILES.iter().map(|(id, label)| json!({"id": id, "label": label})).collect())
  }

  pub fn sessions(&self) -> Result<Vec<RunnerSession>, String> {
    self.gate()?;
    list_sessions(&mut self.connect()?.0).map_err(err)
  }

  pub fn start_run(&self, r: &StartRun) -> Result<RunnerSession, String> {
    self.gate()?;
    let (mut conn, welcome) = self.connect()?;
    if !welcome.can_create_runs() {
      return Err(UPDATE_RUNNER.to_owned());
    }
    api::create_run(&mut conn, r, &welcome).map_err(err)
  }

  pub fn config_path(&self) -> &std::path::Path {
    self.0.flag.path()
  }

  pub fn kill(&self, sid: &str) -> Result<(), String> {
    self.gate()?;
    api::check_session_id(sid)?;
    api::kill_session(&mut self.connect()?.0, sid).map_err(err)
  }

  pub fn session_exists(&self, sid: &str) -> Result<bool, String> {
    self.gate()?;
    api::check_session_id(sid)?;
    Ok(find_session(&mut self.connect()?.0, sid).map_err(err)?.is_some())
  }

  pub fn watch(&self, sink: impl Fn(WatchEvent) -> bool + Send + Sync + 'static) -> Result<u64, String> {
    self.gate()?;
    let generation = self.generation(None);
    let (mut conn, _) = self.connect()?;
    conn.call(method::ATTACH_OPEN, json!({"sessions": []})).map_err(err)?;
    let _ = conn.stream.set_read_timeout(None);
    let sock = conn.stream.try_clone().map_err(|e| e.to_string())?;
    let id = self.0.next_watch.fetch_add(1, Ordering::SeqCst);
    let stop = Arc::new(AtomicBool::new(false));
    let pending = Arc::new(AtomicBool::new(false));
    let co = Arc::new(Coalescer::default());
    let sink = Arc::new(sink);
    {
      let mut watchers = lock(&self.0.watchers);
      if let Err(e) = self.still_current(None, generation) {
        drop(watchers);
        stop.store(true, Ordering::SeqCst);
        let _ = sock.shutdown(std::net::Shutdown::Both);
        return Err(e);
      }
      watchers.insert(id, WatchHandle { stop: stop.clone(), sock });
    }

    let (s, p, k, c, me) = (sink.clone(), pending.clone(), stop.clone(), co.clone(), self.clone());
    let mut reader = conn.stream;
    let spawned = std::thread::Builder::new().name("runner-watch".into()).spawn(move || {
      run_watch(&mut reader, &*s, &c, &p, &k);
      k.store(true, Ordering::SeqCst);
      lock(&me.0.watchers).remove(&id);
    });
    if spawned.is_err() {
      self.unwatch(id);
      return Err("could not start watcher".to_owned());
    }

    let me = self.clone();
    let lister = std::thread::Builder::new().name("runner-watch-list".into()).spawn(move || {
      let mut last_list = Instant::now();
      loop {
        std::thread::sleep(WATCH_FLUSH);
        if stop.load(Ordering::SeqCst) || !me.enabled() {
          break;
        }
        for ev in co.drain() {
          if !sink(ev) {
            me.unwatch(id);
            return;
          }
        }
        if last_list.elapsed() >= LIST_THROTTLE && pending.swap(false, Ordering::SeqCst) {
          last_list = Instant::now();
          if let Ok(sessions) = me.sessions() {
            if !sink(WatchEvent::Sessions { sessions }) {
              me.unwatch(id);
              return;
            }
          }
        }
      }
    });
    if lister.is_err() {
      self.unwatch(id);
      return Err("could not start watcher".to_owned());
    }
    Ok(id)
  }

  pub fn unwatch(&self, id: u64) {
    if let Some(w) = lock(&self.0.watchers).remove(&id) {
      w.stop.store(true, Ordering::SeqCst);
      let _ = w.sock.shutdown(std::net::Shutdown::Both);
    }
  }

  pub fn term_attach(
    &self,
    sid: &str,
    cols: u32,
    rows: u32,
    sink: impl Fn(TermFrame) -> bool + Send + Sync + 'static,
  ) -> Result<TermInfo, String> {
    self.gate()?;
    api::check_session_id(sid)?;
    let generation = self.generation(Some(sid));
    let found = find_session(&mut self.connect()?.0, sid).map_err(err)?.ok_or_else(|| "session not found".to_owned())?;
    let session = session_from_value(&found).ok_or_else(|| "session not found".to_owned())?;
    let state = state_from_value(&found["state"], Detail::Text).unwrap_or(session.state.clone());

    let (mut conn, _) = self.connect()?;
    conn.call(method::ATTACH_OPEN, json!({"sessions": [sid]})).map_err(err)?;
    let _ = conn.stream.set_read_timeout(None);
    conn.stream.set_write_timeout(Some(TERM_WRITE_TIMEOUT)).map_err(|e| e.to_string())?;
    let (cols, rows) = clamp_size(cols, rows);
    write_json(&mut conn.stream, &ClientMsg::Resize(Resize { session: sid.to_owned(), cols, rows })).map_err(|e| e.to_string())?;
    let handle = Arc::new(TermHandle {
      shared: Arc::new(Shared::new(sink)),
      writer: Mutex::new(conn.stream.try_clone().map_err(|e| e.to_string())?),
      sock: conn.stream.try_clone().map_err(|e| e.to_string())?,
    });
    let old = {
      let mut terms = lock(&self.0.terms);
      if let Err(e) = self.still_current(Some(sid), generation) {
        drop(terms);
        Self::close_term(&handle);
        return Err(e);
      }
      terms.insert(sid.to_owned(), handle.clone())
    };
    if let Some(old) = old {
      Self::close_term(&old);
    }

    let mut reader = conn.stream;
    let (me, h, sid_owned) = (self.clone(), handle.clone(), sid.to_owned());
    let spawned = std::thread::Builder::new().name("runner-term".into()).spawn(move || {
      run_attach(&mut reader, &sid_owned, &h.shared);
      let mut terms = lock(&me.0.terms);
      if terms.get(&sid_owned).is_some_and(|cur| Arc::ptr_eq(cur, &h)) {
        terms.remove(&sid_owned);
      }
    });
    if spawned.is_err() {
      self.term_detach(sid);
      return Err("could not start terminal".to_owned());
    }
    Ok(TermInfo { session: session.id, name: session.name, task: session.task, state })
  }

  fn close_term(t: &TermHandle) {
    t.shared.stopped.store(true, Ordering::SeqCst);
    if let Ok(mut w) = t.writer.try_lock() {
      let _ = write_json(&mut *w, &ClientMsg::Detach);
    }
    let _ = t.sock.shutdown(std::net::Shutdown::Both);
  }

  fn term(&self, sid: &str) -> Result<Arc<TermHandle>, String> {
    self.gate()?;
    lock(&self.0.terms).get(sid).cloned().ok_or_else(|| "not attached".to_owned())
  }

  pub fn term_ack(&self, sid: &str, seq: u64) -> Result<(), String> {
    self.term(sid)?.shared.ack(seq);
    Ok(())
  }

  pub fn term_input(&self, sid: &str, data: &str) -> Result<(), String> {
    if data.len() > MAX_INPUT {
      return Err("input too large".to_owned());
    }
    let t = self.term(sid)?;
    let mut w = lock(&t.writer);
    write_frame(&mut *w, &input_frame(sid, data.as_bytes())).map_err(|e| e.to_string())?;
    w.flush().map_err(|e| e.to_string())
  }

  pub fn term_resize(&self, sid: &str, cols: u32, rows: u32) -> Result<(), String> {
    let t = self.term(sid)?;
    let (cols, rows) = clamp_size(cols, rows);
    Self::send(&t, &ClientMsg::Resize(Resize { session: sid.to_owned(), cols, rows }))
  }

  fn send(t: &TermHandle, msg: &ClientMsg) -> Result<(), String> {
    let mut w = lock(&t.writer);
    write_json(&mut *w, msg).map_err(|e| e.to_string())
  }

  pub fn term_scroll(&self, sid: &str, offset: u32) -> Result<(), String> {
    let t = self.term(sid)?;
    Self::send(&t, &ClientMsg::Scroll(Scroll { session: sid.to_owned(), offset }))
  }

  pub fn term_detach(&self, sid: &str) {
    *lock(&self.0.closes).entry(sid.to_owned()).or_insert(0) += 1;
    if let Some(t) = lock(&self.0.terms).remove(sid) {
      Self::close_term(&t);
    }
  }

  pub fn window_destroyed(&self, label: &str) {
    if let Ok(sid) = api::session_from_label(label) {
      self.term_detach(&sid);
    }
  }

  fn stop_all(&self) {
    let watchers: Vec<_> = lock(&self.0.watchers).drain().collect();
    for (_, w) in watchers {
      w.stop.store(true, Ordering::SeqCst);
      let _ = w.sock.shutdown(std::net::Shutdown::Both);
    }
    let terms: Vec<_> = lock(&self.0.terms).drain().collect();
    for (_, t) in terms {
      t.shared.emit(TermFrame::Closed { reason: "disabled".to_owned() });
      Self::close_term(&t);
    }
  }
}

#[cfg(all(test, unix))]
mod tests {
  use super::*;
  use crate::runner::conn::Frame;
  use crate::runner::mock::{self, session, Mock};
  use std::os::unix::net::UnixStream;
  use std::sync::atomic::AtomicUsize;

  const SID: &str = "s_abcdefghij";

  type Servers = Arc<Mutex<Vec<std::thread::JoinHandle<mock::Log>>>>;

  fn mock_runner(on: bool) -> (Runner, Arc<AtomicUsize>, Servers, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let servers: Servers = Arc::default();
    let (c, s) = (calls.clone(), servers.clone());
    let connector: Connector = Arc::new(move || {
      c.fetch_add(1, Ordering::SeqCst);
      let (a, b) = UnixStream::pair().unwrap();
      a.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
      let mut m = Mock::new("secret");
      m.sessions = vec![session(SID, json!({"kind": "working", "since_ms": 1, "detail": null}))];
      s.lock().unwrap().push(mock::spawn(b, m));
      Conn::handshake(a, "secret", "desktop")
    });
    let runner = Runner::new(dir.path().join("runner.json"), connector);
    if on {
      runner.set_enabled(true).unwrap();
    }
    (runner, calls, servers, dir)
  }

  #[test]
  fn disabled_flag_refuses_everything_without_touching_the_socket() {
    let (r, calls, _s, _d) = mock_runner(false);
    assert!(!r.enabled());
    assert_eq!(r.status().state, "disabled");
    assert_eq!(r.profiles().unwrap_err(), "disabled");
    assert_eq!(r.sessions().unwrap_err(), "disabled");
    let run = StartRun { workspace_id: -1, task_id: 1, profile: "codex".into(), cwd: "/work/project".into(), name: "t".into(), cwd_pin: None, bundle: None };
    assert_eq!(r.start_run(&run).unwrap_err(), "disabled");
    assert_eq!(r.kill(SID).unwrap_err(), "disabled");
    assert_eq!(r.session_exists(SID).unwrap_err(), "disabled");
    assert_eq!(r.watch(|_| true).unwrap_err(), "disabled");
    assert_eq!(r.term_attach(SID, 80, 24, |_| true).unwrap_err(), "disabled");
    assert_eq!(r.term_input(SID, "x").unwrap_err(), "disabled");
    assert_eq!(r.term_ack(SID, 1).unwrap_err(), "disabled");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
  }

  #[test]
  fn enabled_runner_reports_ready_lists_and_kills() {
    let (r, calls, servers, _d) = mock_runner(true);
    let st = r.status();
    assert_eq!((st.state, st.version.as_deref()), ("ready", Some("0.1.0")));
    assert!(st.caps.iter().any(|c| c == "runs.create.v1"));
    assert_eq!(r.profiles().unwrap().len(), 2);
    assert_eq!(r.sessions().unwrap()[0].id, SID);
    r.kill(SID).unwrap();
    assert_eq!(r.kill("../x").unwrap_err(), "invalid session");
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    let mut logs = Vec::new();
    for h in servers.lock().unwrap().drain(..) {
      logs.push(h.join().unwrap());
    }
    assert!(logs.iter().any(|l| l.requests.iter().any(|q| q["method"] == "sessions.kill")));
  }

  fn runner_with_caps(caps: &[&str]) -> (Runner, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let caps: Vec<String> = caps.iter().map(|c| c.to_string()).collect();
    let connector: Connector = Arc::new(move || {
      let (a, b) = UnixStream::pair().unwrap();
      let mut m = Mock::new("secret");
      m.caps = Some(caps.clone());
      mock::spawn(b, m);
      Conn::handshake(a, "secret", "desktop")
    });
    let r = Runner::new(dir.path().join("runner.json"), connector);
    r.set_enabled(true).unwrap();
    (r, dir)
  }

  #[test]
  fn a_runner_without_runs_create_is_incompatible_and_refuses_runs() {
    let (r, _d) = runner_with_caps(&["sessions.v1", "attach.v1"]);
    let st = r.status();
    assert_eq!((st.state, st.message.as_deref()), ("incompatible", Some("Update the TMGR runner")));
    let run = StartRun { workspace_id: -1, task_id: 1, profile: "codex".into(), cwd: "/work/project".into(), name: "t".into(), cwd_pin: None, bundle: None };
    assert_eq!(r.start_run(&run).unwrap_err(), "Update the TMGR runner");
    assert!(r.sessions().unwrap().is_empty());
    let (ok, _d2) = runner_with_caps(&["runs.create.v1"]);
    assert_eq!(ok.status().state, "ready");
    assert_eq!(ok.start_run(&run).unwrap().task, Some(TaskRef { workspace_id: -1, task_id: 1 }));
  }

  #[test]
  fn start_run_sends_pin_only_with_the_cap() {
    let log = |caps: &[&str]| {
      let dir = tempfile::tempdir().unwrap();
      let servers = Arc::new(Mutex::new(Vec::new()));
      let (c, s) = (caps.iter().map(|c| c.to_string()).collect::<Vec<_>>(), servers.clone());
      let connector: Connector = Arc::new(move || {
        let (a, b) = UnixStream::pair().unwrap();
        let mut m = Mock::new("secret");
        m.caps = Some(c.clone());
        m.folder_id = Some((1, 2));
        s.lock().unwrap().push(mock::spawn(b, m));
        Conn::handshake(a, "secret", "desktop")
      });
      let r = Runner::new(dir.path().join("runner.json"), connector);
      r.set_enabled(true).unwrap();
      let run = StartRun { workspace_id: -1, task_id: 1, profile: "codex".into(), cwd: "/work/project".into(), name: "t".into(), cwd_pin: Some((1, 3)), bundle: None };
      let result = r.start_run(&run);
      drop(r);
      let logs: Vec<_> = servers.lock().unwrap().drain(..).map(|h| h.join().unwrap()).collect();
      (result, logs)
    };
    let (ok, logs) = log(&["runs.create.v1"]);
    assert!(ok.is_ok());
    let sent = logs.iter().flat_map(|l| &l.requests).find(|q| q["method"] == "runs.create").unwrap();
    assert!(sent["params"].get("cwd_pin").is_none());
    let (rejected, _) = log(&["runs.create.v1", "cwd_pin.v1"]);
    assert_eq!(rejected.unwrap_err(), "invalid_params: approved folder changed");
  }

  #[test]
  fn unreachable_runner_is_absent() {
    let dir = tempfile::tempdir().unwrap();
    let connector: Connector = Arc::new(|| Err(ConnError::Absent("Agent runner is not running".into())));
    let r = Runner::new(dir.path().join("runner.json"), connector);
    r.set_enabled(true).unwrap();
    let st = r.status();
    assert_eq!(st.state, "absent");
    assert_eq!(st.message.as_deref(), Some("Agent runner is not running"));
    let connector: Connector = Arc::new(|| Err(ConnError::Incompatible("proto".into())));
    let r = Runner::new(dir.path().join("runner.json"), connector);
    assert_eq!(r.status().state, "incompatible");
  }

  #[test]
  fn terminal_attach_writes_input_resize_scroll_and_detaches() {
    let (r, _calls, servers, _d) = mock_runner(true);
    assert_eq!(r.term_attach("s_zzzzzzzzzz", 80, 24, |_| true).unwrap_err(), "session not found");
    let info = r.term_attach(SID, 9999, 0, |_| true).unwrap();
    assert_eq!(info.session, SID);
    r.term_input(SID, "ls\r").unwrap();
    assert_eq!(r.term_input(SID, &"a".repeat(MAX_INPUT + 1)).unwrap_err(), "input too large");
    r.term_input(SID, &"a".repeat(MAX_INPUT)).unwrap();
    r.term_resize(SID, 0, 9999).unwrap();
    r.term_scroll(SID, 12).unwrap();
    r.term_ack(SID, 1).unwrap();
    r.term_detach(SID);
    assert_eq!(r.term_input(SID, "x").unwrap_err(), "not attached");
    let mut logs = Vec::new();
    for h in servers.lock().unwrap().drain(..) {
      logs.push(h.join().unwrap());
    }
    let log = logs.into_iter().find(|l| l.requests.iter().any(|q| q["method"] == "attach.open")).unwrap();
    assert_eq!(log.requests[0]["params"], json!({"sessions": [SID]}));
    let jsons: Vec<_> = log.frames.iter().filter_map(|f| if let Frame::Json(v) = f { Some(v.clone()) } else { None }).collect();
    assert_eq!(jsons[0], json!({"type": "resize", "session": SID, "cols": 500, "rows": 1}));
    assert_eq!(jsons[1], json!({"type": "resize", "session": SID, "cols": 1, "rows": 200}));
    assert_eq!(jsons[2], json!({"type": "scroll", "session": SID, "offset": 12}));
    assert_eq!(jsons[3], json!({"type": "detach"}));
    let inputs: Vec<_> = log.frames.iter().filter_map(|f| if let Frame::Bin(b) = f { Some(b) } else { None }).collect();
    assert_eq!((inputs[0].tag, inputs[0].session.as_str(), inputs[0].data.as_slice()), (3, SID, &b"ls\r"[..]));
    assert_eq!(inputs[1].data.len(), MAX_INPUT);
  }

  #[test]
  fn turning_the_flag_off_closes_pumps_and_watchers() {
    let (r, _calls, _servers, _d) = mock_runner(true);
    let frames = Arc::new(Mutex::new(Vec::new()));
    let f = frames.clone();
    r.term_attach(SID, 80, 24, move |fr| {
      f.lock().unwrap().push(fr);
      true
    })
    .unwrap();
    let id = r.watch(|_| true).unwrap();
    assert!(id > 0);
    r.set_enabled(false).unwrap();
    assert_eq!(r.term_ack(SID, 1).unwrap_err(), "disabled");
    assert!(lock(&r.0.terms).is_empty() && lock(&r.0.watchers).is_empty());
    assert_eq!(frames.lock().unwrap()[0], TermFrame::Closed { reason: "disabled".into() });
  }

  fn slow_runner() -> (Runner, Arc<AtomicU64>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let delay = Arc::new(AtomicU64::new(0));
    let d = delay.clone();
    let connector: Connector = Arc::new(move || {
      std::thread::sleep(Duration::from_millis(d.load(Ordering::SeqCst)));
      let (a, b) = UnixStream::pair().unwrap();
      a.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
      let mut m = Mock::new("secret");
      m.sessions = vec![session(SID, json!({"kind": "working", "since_ms": 1, "detail": null}))];
      mock::spawn(b, m);
      Conn::handshake(a, "secret", "desktop")
    });
    let runner = Runner::new(dir.path().join("runner.json"), connector);
    runner.set_enabled(true).unwrap();
    (runner, delay, dir)
  }

  #[test]
  fn disabling_during_a_delayed_attach_leaves_no_handle() {
    let (r, delay, _d) = slow_runner();
    delay.store(300, Ordering::SeqCst);
    let me = r.clone();
    let attach = std::thread::spawn(move || me.term_attach(SID, 80, 24, |_| true));
    std::thread::sleep(Duration::from_millis(100));
    r.set_enabled(false).unwrap();
    assert_eq!(attach.join().unwrap().unwrap_err(), "disabled");
    assert!(lock(&r.0.terms).is_empty());
  }

  #[test]
  fn closing_the_window_during_a_delayed_attach_leaves_no_handle() {
    let (r, delay, _d) = slow_runner();
    delay.store(300, Ordering::SeqCst);
    let me = r.clone();
    let attach = std::thread::spawn(move || me.term_attach(SID, 80, 24, |_| true));
    std::thread::sleep(Duration::from_millis(100));
    r.window_destroyed(&format!("runner-{SID}"));
    assert_eq!(attach.join().unwrap().unwrap_err(), "detached");
    assert!(lock(&r.0.terms).is_empty());
    delay.store(0, Ordering::SeqCst);
    r.term_attach(SID, 80, 24, |_| true).unwrap();
    assert_eq!(lock(&r.0.terms).len(), 1);
  }

  #[test]
  fn disabling_during_a_delayed_watch_leaves_no_handle() {
    let (r, delay, _d) = slow_runner();
    delay.store(300, Ordering::SeqCst);
    let me = r.clone();
    let watch = std::thread::spawn(move || me.watch(|_| true));
    std::thread::sleep(Duration::from_millis(100));
    r.set_enabled(false).unwrap();
    assert_eq!(watch.join().unwrap().unwrap_err(), "disabled");
    assert!(lock(&r.0.watchers).is_empty());
  }

  #[test]
  fn detach_does_not_wait_for_a_busy_writer_and_attach_sets_a_write_timeout() {
    let (r, _delay, _d) = slow_runner();
    r.term_attach(SID, 80, 24, |_| true).unwrap();
    let t = r.term(SID).unwrap();
    assert_eq!(t.sock.write_timeout().unwrap(), Some(TERM_WRITE_TIMEOUT));
    let busy = lock(&t.writer);
    let me = r.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
      me.term_detach(SID);
      tx.send(()).unwrap();
    });
    rx.recv_timeout(Duration::from_secs(2)).expect("detach blocked on the writer");
    drop(busy);
    assert!(t.shared.stopped.load(Ordering::SeqCst));
    assert_eq!(r.term_input(SID, "x").unwrap_err(), "not attached");
  }

  #[test]
  fn size_clamping() {
    assert_eq!(clamp_size(0, 0), (1, 1));
    assert_eq!(clamp_size(100000, 100000), (500, 200));
    assert_eq!(clamp_size(120, 32), (120, 32));
  }
}
