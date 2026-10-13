use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;

use super::api::{state_from_value, Detail, RunnerSession, RunnerState};
use super::conn::{read_frame, Frame};
use super::render::{render_diff, render_full, Line, ScreenPush, Snapshot};
use super::sanitize;

pub const MAX_UNACKED: u64 = 4;

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
  State { session: String, state: RunnerState },
  Sessions { sessions: Vec<RunnerSession> },
  Down { message: String },
}

#[derive(Default)]
pub struct PumpCore {
  snap: Option<Snapshot>,
  seq: u64,
  acked: u64,
  dirty: bool,
  scroll: (usize, usize),
}

impl PumpCore {
  fn unacked(&self) -> u64 {
    self.seq - self.acked
  }

  fn apply(&mut self, push: &ScreenPush) -> bool {
    if push.full {
      let Some(mut snap) = push.snapshot.clone() else { return false };
      let rows = snap.r as usize;
      if snap.l.len() < rows {
        snap.l.resize(rows, Line::default());
      }
      snap.so = push.scroll_offset;
      snap.sm = push.scroll_max;
      self.snap = Some(snap);
      return true;
    }
    let Some(snap) = self.snap.as_mut() else { return false };
    for (i, line) in &push.lines {
      if *i >= super::render::MAX_ROWS {
        continue;
      }
      if *i >= snap.l.len() {
        snap.l.resize(*i + 1, Line::default());
      }
      snap.l[*i] = line.clone();
    }
    snap.cu = push.cursor;
    if let Some(m) = push.modes {
      snap.m = m;
    }
    snap.so = push.scroll_offset;
    snap.sm = push.scroll_max;
    true
  }

  pub fn on_push(&mut self, push: &ScreenPush) -> Vec<TermFrame> {
    if !self.apply(push) {
      return Vec::new();
    }
    let mut out = Vec::new();
    let scroll = (push.scroll_offset, push.scroll_max);
    if self.scroll != scroll {
      self.scroll = scroll;
      out.push(TermFrame::Scroll { offset: scroll.0, max: scroll.1 });
    }
    if self.dirty || self.unacked() >= MAX_UNACKED {
      self.dirty = true;
      return out;
    }
    let snap = self.snap.as_ref().expect("applied above");
    let data = if push.full {
      render_full(snap)
    } else {
      render_diff(&push.lines, push.cursor, push.modes, snap.r as usize, snap.c as usize)
    };
    self.seq += 1;
    out.push(TermFrame::Data { seq: self.seq, data });
    out
  }

  pub fn on_ack(&mut self, seq: u64) -> Vec<TermFrame> {
    self.acked = self.acked.max(seq.min(self.seq));
    if !self.dirty || self.unacked() >= MAX_UNACKED {
      return Vec::new();
    }
    self.dirty = false;
    let Some(snap) = self.snap.as_ref() else { return Vec::new() };
    self.seq += 1;
    vec![TermFrame::Data { seq: self.seq, data: render_full(snap) }]
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
}

fn closed_reason(v: &Value) -> String {
  let r = sanitize::text(v.get("reason").and_then(Value::as_str).unwrap_or(""), 200);
  if r.is_empty() {
    "closed".to_owned()
  } else {
    r
  }
}

pub fn run_attach<R: Read>(reader: &mut R, sid: &str, shared: &Shared) {
  let reason = loop {
    if shared.stopped.load(Ordering::SeqCst) {
      return;
    }
    let v = match read_frame(reader) {
      Ok(Some(Frame::Json(v))) => v,
      Ok(Some(Frame::Bin(_))) => continue,
      Ok(None) => break "connection closed".to_owned(),
      Err(e) => break sanitize::text(&e.to_string(), 200),
    };
    match v.get("type").and_then(Value::as_str) {
      Some("screen") => {
        if v.get("session").and_then(Value::as_str) != Some(sid) {
          continue;
        }
        if let Ok(push) = serde_json::from_value::<ScreenPush>(v) {
          shared.push(&push);
        }
      }
      Some("event") => {
        if v.get("session").and_then(Value::as_str) != Some(sid) {
          continue;
        }
        if let Some(state) = v.get("state").and_then(|s| state_from_value(s, Detail::Text)) {
          shared.emit(TermFrame::State { state });
        }
      }
      Some("closed") => break closed_reason(&v),
      _ => {}
    }
  };
  if !shared.stopped.swap(true, Ordering::SeqCst) {
    shared.emit(TermFrame::Closed { reason });
  }
}

pub fn run_watch<R: Read>(reader: &mut R, sink: &dyn Fn(WatchEvent) -> bool, pending: &AtomicBool, stop: &AtomicBool) {
  let message = loop {
    if stop.load(Ordering::SeqCst) {
      return;
    }
    let v = match read_frame(reader) {
      Ok(Some(Frame::Json(v))) => v,
      Ok(Some(Frame::Bin(_))) => continue,
      Ok(None) => break "Agent runner closed the connection".to_owned(),
      Err(e) => break sanitize::text(&e.to_string(), 200),
    };
    match v.get("type").and_then(Value::as_str) {
      Some("event") => {
        let session = v.get("session").and_then(Value::as_str).unwrap_or_default();
        if !super::conn::valid_session_id(session) {
          continue;
        }
        if let Some(state) = v.get("state").and_then(|s| state_from_value(s, Detail::ExitOnly)) {
          pending.store(true, Ordering::SeqCst);
          if !sink(WatchEvent::State { session: session.to_owned(), state }) {
            return;
          }
        }
      }
      Some("closed") => break closed_reason(&v),
      _ => {}
    }
  };
  if !stop.swap(true, Ordering::SeqCst) {
    sink(WatchEvent::Down { message });
  }
}

#[cfg(all(test, unix))]
mod tests {
  use super::*;
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
      &pending,
      &stop,
    );
    w.join().unwrap();
    assert!(pending.load(Ordering::SeqCst));
    let ev = events.lock().unwrap().clone();
    assert_eq!(ev.len(), 3, "{ev:?}");
    match &ev[0] {
      WatchEvent::State { state, .. } => assert_eq!(state.detail, None),
      other => panic!("{other:?}"),
    }
    match &ev[1] {
      WatchEvent::State { state, .. } => assert_eq!(state.detail.as_deref(), Some("failed")),
      other => panic!("{other:?}"),
    }
    assert!(matches!(ev[2], WatchEvent::Down { .. }));
  }
}
