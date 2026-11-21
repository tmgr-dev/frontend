use std::collections::BTreeMap;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tmgr_ipc::api::{EventMsg, Line, ServerMsg, Snapshot};

use super::api::{state_from_ipc, Detail, RunnerSession, RunnerState, TaskRef};
use super::conn::{read_frame, Frame};
use super::render::{cap_line, cap_snapshot, empty_line, render_diff, render_full, ScreenPush, MAX_ROWS};
use super::sanitize;

pub const MAX_UNACKED: u64 = 4;
const MAX_WATCH_SESSIONS: usize = 1024;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TermFrame {
  Data { seq: u64, data: String },
  State { state: RunnerState },
  Scroll { offset: usize, max: usize },
  Closed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WatchEvent {
  State { session: String, state: RunnerState, #[serde(skip_serializing_if = "Option::is_none")] task: Option<TaskRef> },
  Sessions { sessions: Vec<RunnerSession> },
  Down { message: String },
}

#[derive(Default)]
pub struct PumpCore {
  snap: Option<Snapshot>,
  seq: u64,
  acked: u64,
  dirty: bool,
  sent_scroll: (usize, usize),
  pending_scroll: Option<(usize, usize)>,
  sent_state: Option<RunnerState>,
  pending_state: Option<RunnerState>,
}

impl PumpCore {
  fn unacked(&self) -> u64 {
    self.seq - self.acked
  }

  fn caught_up(&self) -> bool {
    !self.dirty && self.unacked() == 0
  }

  fn apply(&mut self, push: &ScreenPush) -> Option<Vec<(usize, Line)>> {
    if push.full {
      let mut snap = cap_snapshot(push.snapshot.as_ref()?);
      snap.lines.resize(snap.rows as usize, empty_line());
      snap.scroll_offset = push.scroll_offset;
      snap.scroll_max = push.scroll_max;
      self.snap = Some(snap);
      return Some(Vec::new());
    }
    let snap = self.snap.as_mut()?;
    let rows = snap.rows as usize;
    let mut applied = Vec::new();
    for (i, line) in &push.lines {
      if *i >= MAX_ROWS || *i >= rows {
        continue;
      }
      let line = cap_line(line);
      snap.lines[*i] = line.clone();
      applied.push((*i, line));
    }
    snap.cursor = push.cursor;
    if let Some(m) = push.modes {
      snap.modes = m;
    }
    snap.scroll_offset = push.scroll_offset;
    snap.scroll_max = push.scroll_max;
    Some(applied)
  }

  fn flush(&mut self, out: &mut Vec<TermFrame>) {
    if let Some((offset, max)) = self.pending_scroll.take() {
      self.sent_scroll = (offset, max);
      out.push(TermFrame::Scroll { offset, max });
    }
    if let Some(state) = self.pending_state.take() {
      if self.sent_state.as_ref() != Some(&state) {
        self.sent_state = Some(state.clone());
        out.push(TermFrame::State { state });
      }
    }
  }

  pub fn on_state(&mut self, state: RunnerState) -> Vec<TermFrame> {
    self.pending_state = Some(state);
    let mut out = Vec::new();
    if self.caught_up() {
      self.flush(&mut out);
    }
    out
  }

  pub fn finish(&mut self) -> Vec<TermFrame> {
    let mut out = Vec::new();
    self.flush(&mut out);
    out
  }

  pub fn on_push(&mut self, push: &ScreenPush) -> Vec<TermFrame> {
    let Some(lines) = self.apply(push) else { return Vec::new() };
    let mut out = Vec::new();
    let scroll = (push.scroll_offset, push.scroll_max);
    self.pending_scroll = (scroll != self.sent_scroll).then_some(scroll);
    if self.caught_up() {
      self.flush(&mut out);
    }
    if self.dirty || self.unacked() >= MAX_UNACKED {
      self.dirty = true;
      return out;
    }
    let snap = self.snap.as_ref().expect("applied above");
    let data = if push.full {
      render_full(snap)
    } else {
      render_diff(&lines, push.cursor, push.modes, snap.rows as usize, snap.cols as usize)
    };
    self.seq += 1;
    out.push(TermFrame::Data { seq: self.seq, data });
    out
  }

  pub fn on_ack(&mut self, seq: u64) -> Vec<TermFrame> {
    self.acked = self.acked.max(seq.min(self.seq));
    let mut out = Vec::new();
    if !self.dirty {
      if self.unacked() == 0 {
        self.flush(&mut out);
      }
      return out;
    }
    if self.unacked() >= MAX_UNACKED {
      return out;
    }
    self.dirty = false;
    self.flush(&mut out);
    if let Some(snap) = self.snap.as_ref() {
      self.seq += 1;
      out.push(TermFrame::Data { seq: self.seq, data: render_full(snap) });
    }
    out
  }

  #[cfg(test)]
  fn retained_chars(&self) -> usize {
    self.snap.as_ref().map_or(0, |s| s.lines.iter().flat_map(|l| &l.spans).map(|sp| sp.t.chars().count()).sum())
  }
}

#[derive(Default)]
pub struct Coalescer {
  latest: Mutex<BTreeMap<String, (RunnerState, Option<TaskRef>)>>,
}

impl Coalescer {
  pub fn push(&self, session: &str, state: RunnerState, task: Option<TaskRef>) {
    let mut latest = self.latest.lock().unwrap_or_else(|e| e.into_inner());
    if latest.len() < MAX_WATCH_SESSIONS || latest.contains_key(session) {
      latest.insert(session.to_owned(), (state, task));
    }
  }

  pub fn drain(&self) -> Vec<WatchEvent> {
    let latest = std::mem::take(&mut *self.latest.lock().unwrap_or_else(|e| e.into_inner()));
    latest.into_iter().map(|(session, (state, task))| WatchEvent::State { session, state, task }).collect()
  }
}

pub struct Shared {
  core: Mutex<PumpCore>,
  sink: Box<dyn Fn(TermFrame) -> bool + Send + Sync>,
  pub stopped: AtomicBool,
}

impl Shared {
  pub fn new(sink: impl Fn(TermFrame) -> bool + Send + Sync + 'static) -> Shared {
    Shared { core: Mutex::new(PumpCore::default()), sink: Box::new(sink), stopped: AtomicBool::new(false) }
  }

  fn with_core(&self, f: impl FnOnce(&mut PumpCore) -> Vec<TermFrame>) {
    let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
    for frame in f(&mut core) {
      if !(self.sink)(frame) {
        self.stopped.store(true, Ordering::SeqCst);
      }
    }
  }

  pub fn emit(&self, frame: TermFrame) {
    self.with_core(|_| vec![frame]);
  }

  pub fn ack(&self, seq: u64) {
    self.with_core(|c| c.on_ack(seq));
  }

  pub fn push(&self, push: &ScreenPush) {
    self.with_core(|c| c.on_push(push));
  }

  pub fn state(&self, state: RunnerState) {
    self.with_core(|c| c.on_state(state));
  }

  pub fn finish(&self, reason: String) {
    self.with_core(|c| {
      let mut out = c.finish();
      out.push(TermFrame::Closed { reason });
      out
    });
  }
}

fn closed_reason(reason: &str) -> String {
  let r = sanitize::text(reason, 200);
  if r.is_empty() {
    "closed".to_owned()
  } else {
    r
  }
}

fn next_message<R: Read>(reader: &mut R) -> std::io::Result<Option<ServerMsg>> {
  loop {
    match read_frame(reader)? {
      Some(Frame::Json(v)) => {
        if let Ok(msg) = serde_json::from_value::<ServerMsg>(v) {
          return Ok(Some(msg));
        }
      }
      Some(Frame::Bin(_)) => {}
      None => return Ok(None),
    }
  }
}

pub fn run_attach<R: Read>(reader: &mut R, sid: &str, shared: &Shared) {
  let reason = loop {
    if shared.stopped.load(Ordering::SeqCst) {
      return;
    }
    let msg = match next_message(reader) {
      Ok(Some(m)) => m,
      Ok(None) => break "connection closed".to_owned(),
      Err(e) => break sanitize::text(&e.to_string(), 200),
    };
    match msg {
      ServerMsg::Screen(v) => {
        if let Ok(push) = serde_json::from_value::<ScreenPush>(v) {
          if push.session == sid {
            shared.push(&push);
          }
        }
      }
      ServerMsg::Event(v) => {
        if let Ok(ev) = serde_json::from_value::<EventMsg>(v) {
          if ev.session == sid {
            shared.state(state_from_ipc(&ev.state, Detail::Text));
          }
        }
      }
      ServerMsg::Closed { reason } => break closed_reason(&reason),
      ServerMsg::Response(_) | ServerMsg::Unknown => {}
    }
  };
  if !shared.stopped.swap(true, Ordering::SeqCst) {
    shared.finish(reason);
  }
}

pub fn run_watch<R: Read>(reader: &mut R, sink: &dyn Fn(WatchEvent) -> bool, co: &Coalescer, pending: &AtomicBool, stop: &AtomicBool) {
  let message = loop {
    if stop.load(Ordering::SeqCst) {
      return;
    }
    let msg = match next_message(reader) {
      Ok(Some(m)) => m,
      Ok(None) => break "Agent runner closed the connection".to_owned(),
      Err(e) => break sanitize::text(&e.to_string(), 200),
    };
    match msg {
      ServerMsg::Event(v) => {
        let Ok(ev) = serde_json::from_value::<EventMsg>(v) else { continue };
        if !super::conn::valid_session_id(&ev.session) {
          continue;
        }
        pending.store(true, Ordering::SeqCst);
        co.push(&ev.session, state_from_ipc(&ev.state, Detail::ExitOnly), ev.task.map(TaskRef::from));
      }
      ServerMsg::Closed { reason } => break closed_reason(&reason),
      ServerMsg::Screen(_) | ServerMsg::Response(_) | ServerMsg::Unknown => {}
    }
  };
  if !stop.swap(true, Ordering::SeqCst) {
    for ev in co.drain() {
      sink(ev);
    }
    sink(WatchEvent::Down { message });
  }
}

#[cfg(all(test, unix))]
mod tests {
  use super::*;
  use crate::runner::api::StateKind;
  use crate::runner::conn::write_json;
  use crate::runner::mock::session;
  use serde_json::json;
  use std::os::unix::net::UnixStream;
  use std::sync::{Arc, Mutex};

  const SID: &str = "s_abcdefghij";

  fn full(n: usize) -> ScreenPush {
    serde_json::from_value(json!({
      "session": SID, "full": true,
      "snapshot": {"r": 2, "c": 80, "l": [[{"a": {"f": null, "b": null, "x": 0}, "t": format!("push{n}")}], []],
                   "cu": [0, 1], "m": {"ac": false, "bp": false, "alt": false, "hc": false}, "so": 0, "sm": 0},
      "lines": [], "cursor": [0, 1], "modes": {"ac": false, "bp": false, "alt": false, "hc": false},
      "scroll_offset": 0, "scroll_max": 0
    }))
    .unwrap()
  }

  fn diff(n: usize) -> ScreenPush {
    serde_json::from_value(json!({
      "session": SID, "full": false,
      "lines": [[0, [{"a": {"f": null, "b": null, "x": 0}, "t": format!("push{n}")}]]],
      "cursor": [0, 1], "modes": {"ac": false, "bp": false, "alt": false, "hc": false},
      "scroll_offset": 0, "scroll_max": 0
    }))
    .unwrap()
  }

  fn data(frames: &[TermFrame]) -> Vec<(u64, String)> {
    frames
      .iter()
      .filter_map(|f| match f {
        TermFrame::Data { seq, data } => Some((*seq, data.clone())),
        _ => None,
      })
      .collect()
  }

  #[test]
  fn ten_pushes_without_acks_send_four_frames_then_one_full_redraw() {
    let mut core = PumpCore::default();
    let mut sent = core.on_push(&full(0));
    for n in 1..10 {
      sent.extend(core.on_push(&diff(n)));
    }
    let d = data(&sent);
    assert_eq!(d.len(), 4);
    assert_eq!(d.iter().map(|(s, _)| *s).collect::<Vec<_>>(), vec![1, 2, 3, 4]);
    assert!(d[0].1.starts_with("\x1b[H\x1b[2J"));
    assert!(d[1].1.starts_with("\x1b[1;1H\x1b[2K"));

    let redraw = core.on_ack(1);
    let r = data(&redraw);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].0, 5);
    assert!(r[0].1.starts_with("\x1b[H\x1b[2J"));
    assert!(r[0].1.contains("push9"));
    assert!(!r[0].1.contains("push8"));

    assert!(core.on_ack(2).is_empty());
    assert_eq!(data(&core.on_push(&diff(10))).len(), 1);
    assert!(core.on_push(&diff(11)).is_empty());
  }

  #[test]
  fn unsaturated_stream_sends_every_push_and_acks_free_slots() {
    let mut core = PumpCore::default();
    assert_eq!(data(&core.on_push(&full(0))).len(), 1);
    for n in 1..=3 {
      assert_eq!(data(&core.on_push(&diff(n))).len(), 1);
    }
    assert!(core.on_push(&diff(4)).is_empty());
    assert_eq!(data(&core.on_ack(4)).len(), 1);
    assert_eq!(data(&core.on_push(&diff(5))).len(), 1);
    assert!(core.on_ack(999).len() <= 1);
  }

  #[test]
  fn diff_without_a_base_is_ignored_and_scroll_is_reported_on_change() {
    let mut core = PumpCore::default();
    assert!(core.on_push(&diff(1)).is_empty());
    let mut p = full(0);
    p.scroll_offset = 3;
    p.scroll_max = 40;
    let out = core.on_push(&p);
    assert_eq!(out[0], TermFrame::Scroll { offset: 3, max: 40 });
    let again = core.on_push(&p);
    assert!(again.iter().all(|f| matches!(f, TermFrame::Data { .. })));
  }

  fn state(kind: StateKind, since: u64) -> RunnerState {
    RunnerState { kind, since_ms: since, detail: None, exit_code: None }
  }

  fn scrolls(frames: &[TermFrame]) -> usize {
    frames.iter().filter(|f| matches!(f, TermFrame::Scroll { .. })).count()
  }

  fn scrolled(n: usize) -> ScreenPush {
    let mut p = diff(n);
    p.scroll_offset = n;
    p.scroll_max = 5000;
    p
  }

  #[test]
  fn retained_text_is_bounded() {
    let big = Line { spans: vec![tmgr_ipc::api::Span { a: serde_json::from_value(json!({"f": null, "b": null, "x": 0})).unwrap(), t: "x".repeat(900 * 1024) }] };
    let snapshot = Snapshot {
      rows: 200,
      cols: 80,
      lines: vec![big.clone(); 200],
      cursor: None,
      modes: tmgr_ipc::api::Modes { app_cursor: false, bracketed_paste: false, alt_screen: false, hide_cursor: false, mouse: Default::default() },
      scroll_offset: 0,
      scroll_max: 0,
    };
    let mut push = full(0);
    push.snapshot = Some(snapshot);
    let mut core = PumpCore::default();
    core.on_push(&push);
    assert!(core.retained_chars() <= 200 * 500, "{}", core.retained_chars());

    let mut d = diff(1);
    d.lines = (0..400).map(|i| (i, big.clone())).chain([(5000, big.clone())]).collect();
    core.on_push(&d);
    assert!(core.retained_chars() <= 200 * 500);
    assert_eq!(core.snap.as_ref().unwrap().lines.len(), 200);
  }

  #[test]
  fn diff_lines_beyond_the_snapshot_rows_are_ignored() {
    let mut core = PumpCore::default();
    core.on_push(&full(0));
    let mut d = diff(1);
    d.lines = vec![(1, d.lines[0].1.clone()), (2, d.lines[0].1.clone()), (150, d.lines[0].1.clone())];
    let out = core.on_push(&d);
    assert_eq!(core.snap.as_ref().unwrap().lines.len(), 2);
    let TermFrame::Data { data, .. } = &out[0] else { panic!("{out:?}") };
    assert!(data.contains("\x1b[2;1H") && !data.contains("\x1b[3;1H") && !data.contains("\x1b[151;1H"));
  }

  #[test]
  fn a_flood_of_scroll_changes_without_acks_emits_one_scroll_frame() {
    let mut core = PumpCore::default();
    let mut sent = core.on_push(&full(0));
    for n in 1..=1000 {
      sent.extend(core.on_push(&scrolled(n)));
    }
    assert!(scrolls(&sent) <= 1, "{}", scrolls(&sent));
    assert_eq!(data(&sent).len(), 4);
    let flushed = core.on_ack(1);
    assert_eq!(scrolls(&flushed), 1);
    assert!(flushed.iter().position(|f| matches!(f, TermFrame::Scroll { .. })) < flushed.iter().position(|f| matches!(f, TermFrame::Data { .. })));
    assert!(flushed.contains(&TermFrame::Scroll { offset: 1000, max: 5000 }));
    assert_eq!(scrolls(&core.on_ack(5)), 0);
  }

  #[test]
  fn states_are_deduped_and_held_while_data_is_in_flight() {
    let mut core = PumpCore::default();
    assert_eq!(core.on_state(state(StateKind::Working, 1)).len(), 1);
    assert!(core.on_state(state(StateKind::Working, 1)).is_empty());
    core.on_push(&full(0));
    for n in 0..1000 {
      assert!(core.on_state(state(StateKind::Waiting, n)).is_empty());
    }
    let out = core.on_ack(1);
    assert_eq!(out, vec![TermFrame::State { state: state(StateKind::Waiting, 999) }]);
    assert!(core.on_ack(1).is_empty());
    core.on_push(&diff(2));
    assert!(core.on_state(state(StateKind::Exited, 5)).is_empty());
    assert_eq!(core.finish(), vec![TermFrame::State { state: state(StateKind::Exited, 5) }]);
  }

  #[test]
  fn thousand_state_events_coalesce_into_one_per_session() {
    let co = Coalescer::default();
    for n in 0..1000u64 {
      co.push(if n % 2 == 0 { SID } else { "s_bcdefghijk" }, state(StateKind::Working, n), None);
    }
    let ev = co.drain();
    assert_eq!(ev.len(), 2);
    assert!(ev.contains(&WatchEvent::State { session: SID.into(), state: state(StateKind::Working, 998), task: None }));
    assert!(co.drain().is_empty());
    for n in 0..5000 {
      co.push(&format!("s_{n}"), state(StateKind::Working, 1), None);
    }
    assert!(co.drain().len() <= 1024);
  }

  #[test]
  fn frames_serialize_in_the_contract_shape() {
    let v = serde_json::to_value(TermFrame::Data { seq: 2, data: "x".into() }).unwrap();
    assert_eq!(v, json!({"type": "data", "seq": 2, "data": "x"}));
    let v = serde_json::to_value(TermFrame::Scroll { offset: 1, max: 2 }).unwrap();
    assert_eq!(v, json!({"type": "scroll", "offset": 1, "max": 2}));
    let v = serde_json::to_value(TermFrame::Closed { reason: "r".into() }).unwrap();
    assert_eq!(v, json!({"type": "closed", "reason": "r"}));
  }

  fn collect() -> (Arc<Mutex<Vec<TermFrame>>>, Shared) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let l = log.clone();
    let shared = Shared::new(move |f| {
      l.lock().unwrap().push(f);
      true
    });
    (log, shared)
  }

  #[test]
  fn attach_loop_filters_by_session_and_reports_closed() {
    let (mut a, mut b) = UnixStream::pair().unwrap();
    let (log, shared) = collect();
    let writer = std::thread::spawn(move || {
      write_json(&mut b, &json!({"type": "screen", "session": "s_zzzzzzzzzz", "full": false, "lines": []})).unwrap();
      let f = json!({
        "type": "screen", "session": SID, "full": true,
        "snapshot": {"r": 1, "c": 10, "l": [[]], "cu": null, "m": {"ac": false, "bp": false, "alt": false, "hc": false}, "so": 0, "sm": 0},
        "scroll_offset": 0, "scroll_max": 0
      });
      write_json(&mut b, &f).unwrap();
      write_json(
        &mut b,
        &json!({"type": "event", "session": "s_zzzzzzzzzz", "state": {"kind": "working", "since_ms": 1, "detail": null}}),
      )
      .unwrap();
      write_json(
        &mut b,
        &json!({"type": "event", "session": SID, "state": {"kind": "blocked", "since_ms": 9, "detail": "Allow?\u{1b}[2J"}}),
      )
      .unwrap();
      write_json(&mut b, &json!({"type": "closed", "reason": "runner stopped"})).unwrap();
    });
    run_attach(&mut a, SID, &shared);
    writer.join().unwrap();
    let frames = log.lock().unwrap().clone();
    assert_eq!(frames.len(), 3, "{frames:?}");
    assert!(matches!(frames[0], TermFrame::Data { seq: 1, .. }));
    match &frames[1] {
      TermFrame::State { state } => assert_eq!(state.detail.as_deref(), Some("Allow?[2J")),
      other => panic!("{other:?}"),
    }
    assert_eq!(frames[2], TermFrame::Closed { reason: "runner stopped".into() });
  }

  #[test]
  fn attach_loop_reports_eof_as_closed_once() {
    let (mut a, b) = UnixStream::pair().unwrap();
    drop(b);
    let (log, shared) = collect();
    run_attach(&mut a, SID, &shared);
    assert_eq!(log.lock().unwrap().len(), 1);
    assert!(matches!(log.lock().unwrap()[0], TermFrame::Closed { .. }));
  }

  #[test]
  fn attach_loop_stays_silent_when_stopped_locally() {
    let (mut a, b) = UnixStream::pair().unwrap();
    let (log, shared) = collect();
    shared.stopped.store(true, Ordering::SeqCst);
    drop(b);
    run_attach(&mut a, SID, &shared);
    assert!(log.lock().unwrap().is_empty());
  }

  #[test]
  fn watch_loop_drops_detail_text_and_marks_pending() {
    let (mut a, mut b) = UnixStream::pair().unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let e = events.clone();
    let pending = AtomicBool::new(false);
    let stop = AtomicBool::new(false);
    let co = Coalescer::default();
    let w = std::thread::spawn(move || {
      let mut s = session(SID, json!({}));
      s["state"] = json!({"kind": "blocked", "since_ms": 5, "detail": "Delete everything?"});
      write_json(&mut b, &json!({"type": "event", "session": SID, "state": s["state"]})).unwrap();
      write_json(&mut b, &json!({"type": "event", "session": "bad", "state": s["state"]})).unwrap();
      write_json(
        &mut b,
        &json!({"type": "event", "session": SID, "state": {"kind": "exited", "since_ms": 6, "detail": "failed"}}),
      )
      .unwrap();
    });
    run_watch(
      &mut a,
      &|ev| {
        e.lock().unwrap().push(ev);
        true
      },
      &co,
      &pending,
      &stop,
    );
    w.join().unwrap();
    assert!(pending.load(Ordering::SeqCst));
    let ev = events.lock().unwrap().clone();
    assert_eq!(ev.len(), 2, "{ev:?}");
    match &ev[0] {
      WatchEvent::State { state, .. } => assert_eq!(state.detail.as_deref(), Some("failed")),
      other => panic!("{other:?}"),
    }
    assert!(matches!(ev[1], WatchEvent::Down { .. }));
  }

  #[test]
  fn full_snapshot_with_fewer_lines_than_rows_is_padded_with_empty_lines() {
    let mut core = PumpCore::default();
    let mut p = full(0);
    let snap = p.snapshot.as_mut().unwrap();
    snap.rows = 5;
    snap.lines.truncate(1);
    let out = core.on_push(&p);
    assert_eq!(core.snap.as_ref().unwrap().lines.len(), 5);
    let TermFrame::Data { data, .. } = &out[0] else { panic!("{out:?}") };
    assert!(data.contains("\x1b[5;1H"), "{data:?}");
    let mut d = diff(1);
    d.lines = vec![(4, d.lines[0].1.clone())];
    assert!(matches!(core.on_push(&d)[0], TermFrame::Data { .. }));
  }

  fn fixture(name: &str) -> serde_json::Value {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/tmgr-ipc/tests/fixtures");
    serde_json::from_slice(&std::fs::read(dir.join(name)).unwrap()).unwrap()
  }

  #[test]
  fn golden_screen_pushes_render() {
    let mut core = PumpCore::default();
    let full: ScreenPush = serde_json::from_value(fixture("screen_push_full.json")).unwrap();
    let out = core.on_push(&full);
    let TermFrame::Data { data, .. } = &out[0] else { panic!("{out:?}") };
    assert!(data.starts_with("\x1b[H\x1b[2J") && data.contains("hi"), "{data:?}");
    let diff: ScreenPush = serde_json::from_value(fixture("screen_push_diff.json")).unwrap();
    assert_eq!(diff.scroll_max, 9);
    core.on_push(&diff);
    assert!(core.on_ack(2).contains(&TermFrame::Scroll { offset: 2, max: 9 }));
  }

  #[test]
  fn golden_event_keeps_the_task_for_the_watch() {
    let (mut a, mut b) = UnixStream::pair().unwrap();
    let mut ev = fixture("event_msg.json");
    ev["type"] = json!("event");
    ev["future_field"] = json!({"x": 1});
    let w = std::thread::spawn(move || {
      write_json(&mut b, &json!({"type": "brand_new_push", "x": 1})).unwrap();
      write_json(&mut b, &ev).unwrap();
    });
    let seen = Mutex::new(Vec::new());
    run_watch(&mut a, &|e| { seen.lock().unwrap().push(e); true }, &Coalescer::default(), &AtomicBool::new(false), &AtomicBool::new(false));
    w.join().unwrap();
    assert!(matches!(seen.lock().unwrap().last(), Some(WatchEvent::Down { .. })));
    let co = Coalescer::default();
    co.push(SID, state(StateKind::Blocked, 1), Some(TaskRef { workspace_id: -3, task_id: 412 }));
    let v = serde_json::to_value(co.drain()).unwrap();
    assert_eq!(v[0]["task"], json!({"workspaceId": -3, "taskId": 412}));
  }

  #[test]
  fn attach_loop_ignores_unknown_pushes_and_extra_fields() {
    let (mut a, mut b) = UnixStream::pair().unwrap();
    let (log, shared) = collect();
    let w = std::thread::spawn(move || {
      write_json(&mut b, &json!({"type": "hologram", "session": SID, "v": 1})).unwrap();
      let mut push = fixture("screen_push_full.json");
      push["type"] = json!("screen");
      push["unknown"] = json!([1, 2, 3]);
      write_json(&mut b, &push).unwrap();
      write_json(&mut b, &json!({"type": "closed", "reason": "bye"})).unwrap();
    });
    run_attach(&mut a, SID, &shared);
    w.join().unwrap();
    let frames = log.lock().unwrap().clone();
    assert!(matches!(frames[0], TermFrame::Data { seq: 1, .. }), "{frames:?}");
    assert_eq!(frames.last(), Some(&TermFrame::Closed { reason: "bye".into() }));
  }

  #[test]
  fn watch_detail_text_is_dropped_for_non_exit_states() {
    let (mut a, mut b) = UnixStream::pair().unwrap();
    let co = Coalescer::default();
    let w = std::thread::spawn(move || {
      write_json(&mut b, &json!({"type": "event", "session": SID, "state": {"kind": "blocked", "since_ms": 5, "detail": "Delete everything?"}})).unwrap();
    });
    let seen = Mutex::new(Vec::new());
    run_watch(&mut a, &|ev| { seen.lock().unwrap().push(ev); true }, &co, &AtomicBool::new(false), &AtomicBool::new(false));
    w.join().unwrap();
    let first = seen.lock().unwrap()[0].clone();
    match first {
      WatchEvent::State { state, .. } => assert_eq!(state.detail, None),
      other => panic!("{other:?}"),
    }
  }
}
