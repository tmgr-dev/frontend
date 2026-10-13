mod api;
pub(crate) mod commands;
mod conn;
mod dir;
mod flag;
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
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};

use api::{find_session, list_sessions, session_from_value, state_from_value, Detail, RunnerSession, RunnerState, StartRun, TaskRef};
use conn::{input_frame, write_frame, write_json, Conn, ConnError, Welcome, MAX_INPUT};
use dir::Sock;
use flag::Flag;
use pump::{run_attach, run_watch, Shared, TermFrame, WatchEvent};

pub type Connector = Arc<dyn Fn() -> Result<(Conn<Sock>, Welcome), ConnError> + Send + Sync>;

const MAX_COLS: u32 = 500;
const MAX_ROWS: u32 = 200;
const LIST_THROTTLE: Duration = Duration::from_millis(500);

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
      self.stop_all();
    }
    Ok(())
  }

  fn connect(&self) -> Result<(Conn<Sock>, Welcome), String> {
    (self.0.connector)().map_err(err)
  }

  pub fn status(&self) -> RunnerStatus {
    let status = |state, version, caps, message| RunnerStatus { state, version, caps, message };
    if !self.enabled() {
      return status("disabled", None, Vec::new(), None);
    }
    match (self.0.connector)() {
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
    api::create_run(&mut self.connect()?.0, r).map_err(err)
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
    let (mut conn, _) = self.connect()?;
    conn.call("attach.open", json!({"sessions": []})).map_err(err)?;
    let _ = conn.stream.set_read_timeout(None);
    let sock = conn.stream.try_clone().map_err(|e| e.to_string())?;
    let id = self.0.next_watch.fetch_add(1, Ordering::SeqCst);
    let stop = Arc::new(AtomicBool::new(false));
    let pending = Arc::new(AtomicBool::new(false));
    let sink = Arc::new(sink);
    lock(&self.0.watchers).insert(id, WatchHandle { stop: stop.clone(), sock });

    let (s, p, k, me) = (sink.clone(), pending.clone(), stop.clone(), self.clone());
    let mut reader = conn.stream;
    let spawned = std::thread::Builder::new().name("runner-watch".into()).spawn(move || {
      run_watch(&mut reader, &*s, &p, &k);
      k.store(true, Ordering::SeqCst);
      lock(&me.0.watchers).remove(&id);
    });
    if spawned.is_err() {
      lock(&self.0.watchers).remove(&id);
      return Err("could not start watcher".to_owned());
    }

    let me = self.clone();
    let lister = std::thread::Builder::new().name("runner-watch-list".into()).spawn(move || loop {
      std::thread::sleep(LIST_THROTTLE);
      if stop.load(Ordering::SeqCst) || !me.enabled() {
        break;
      }
      if pending.swap(false, Ordering::SeqCst) {
        if let Ok(sessions) = me.sessions() {
          if !sink(WatchEvent::Sessions { sessions }) {
            break;
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
    let found = find_session(&mut self.connect()?.0, sid).map_err(err)?.ok_or_else(|| "session not found".to_owned())?;
    let session = session_from_value(&found).ok_or_else(|| "session not found".to_owned())?;
    let state = state_from_value(&found["state"], Detail::Text).unwrap_or(session.state.clone());

    let (mut conn, _) = self.connect()?;
    conn.call("attach.open", json!({"sessions": [sid]})).map_err(err)?;
    let _ = conn.stream.set_read_timeout(None);
    let (cols, rows) = clamp_size(cols, rows);
    write_json(&mut conn.stream, &json!({"type": "resize", "session": sid, "cols": cols, "rows": rows}))
      .map_err(|e| e.to_string())?;
    let handle = Arc::new(TermHandle {
      shared: Arc::new(Shared::new(sink)),
      writer: Mutex::new(conn.stream.try_clone().map_err(|e| e.to_string())?),
      sock: conn.stream.try_clone().map_err(|e| e.to_string())?,
    });
    let old = lock(&self.0.terms).insert(sid.to_owned(), handle.clone());
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
    let _ = Self::send(t, &json!({"type": "detach"}));
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
    Self::send(&t, &json!({"type": "resize", "session": sid, "cols": cols, "rows": rows}))
  }

  fn send(t: &TermHandle, msg: &Value) -> Result<(), String> {
    let mut w = lock(&t.writer);
    write_json(&mut *w, msg).map_err(|e| e.to_string())
  }

  pub fn term_scroll(&self, sid: &str, offset: u32) -> Result<(), String> {
    let t = self.term(sid)?;
    Self::send(&t, &json!({"type": "scroll", "session": sid, "offset": offset}))
  }

  pub fn term_detach(&self, sid: &str) {
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
    let run = StartRun { workspace_id: -1, task_id: 1, profile: "codex".into() };
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
    assert_eq!(st.caps, vec!["runs.create".to_owned()]);
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

  #[test]
  fn size_clamping() {
    assert_eq!(clamp_size(0, 0), (1, 1));
    assert_eq!(clamp_size(100000, 100000), (500, 200));
    assert_eq!(clamp_size(120, 32), (120, 32));
  }
}
