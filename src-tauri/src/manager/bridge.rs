use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tmgr_ipc::chat::ChatActivity;

use super::backend::ChatMessage;
pub use crate::runner::Push;

const SEEN_CAP: usize = 512;
const BACKOFF_MIN: Duration = Duration::from_millis(500);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

pub trait Feed: Send {
  fn next_push(&mut self) -> Result<Option<Push>, String>;
}

pub trait Source: Send + Sync {
  fn open(&self) -> Result<Box<dyn Feed>, String>;
  fn enabled(&self) -> bool {
    true
  }
}

pub fn parse_push(push: &Value) -> Option<ChatMessage> {
  serde_json::from_value(push.get("message")?.clone()).ok()
}

#[derive(Default)]
struct Seen {
  order: VecDeque<String>,
  ids: HashSet<String>,
}

impl Seen {
  fn first_time(&mut self, id: &str) -> bool {
    if !self.ids.insert(id.to_owned()) {
      return false;
    }
    self.order.push_back(id.to_owned());
    if self.order.len() > SEEN_CAP {
      if let Some(old) = self.order.pop_front() {
        self.ids.remove(&old);
      }
    }
    true
  }
}

struct Backoff(Duration);

impl Backoff {
  fn new() -> Self {
    Backoff(BACKOFF_MIN)
  }

  fn reset(&mut self) {
    self.0 = BACKOFF_MIN;
  }

  fn next(&mut self) -> Duration {
    let now = self.0;
    self.0 = (self.0 * 2).min(BACKOFF_MAX);
    now
  }
}

pub fn run(source: &dyn Source, emit: &dyn Fn(ChatMessage), activity: &dyn Fn(ChatActivity), resync: &dyn Fn(), stop: &AtomicBool, sleep: &dyn Fn(Duration)) {
  let mut seen = Seen::default();
  let mut backoff = Backoff::new();
  let mut connected_before = false;
  while !stop.load(Ordering::SeqCst) {
    let opened = if source.enabled() { source.open() } else { Err("disabled".to_owned()) };
    if let Ok(mut feed) = opened {
      backoff.reset();
      if connected_before {
        resync();
      }
      connected_before = true;
      loop {
        if stop.load(Ordering::SeqCst) {
          return;
        }
        if !source.enabled() {
          break;
        }
        match feed.next_push() {
          Ok(Some(Push::Chat(push))) => {
            if let Some(message) = parse_push(&push) {
              if seen.first_time(&message.id) {
                emit(message);
              }
            }
          }
          Ok(Some(Push::Activity(event))) => activity(event),
          Ok(None) | Err(_) => break,
        }
      }
    }
    if stop.load(Ordering::SeqCst) {
      return;
    }
    sleep(backoff.next());
  }
}

pub fn spawn(source: Arc<dyn Source>, emit: impl Fn(ChatMessage) + Send + 'static, activity: impl Fn(ChatActivity) + Send + 'static, resync: impl Fn() + Send + 'static) {
  let spawned = std::thread::Builder::new().name("manager-bridge".into()).spawn(move || {
    run(&*source, &emit, &activity, &resync, &AtomicBool::new(false), &std::thread::sleep);
  });
  if let Err(e) = spawned {
    log::error!("[manager] live bridge did not start: {e}");
  }
}

pub struct RunnerSource(pub crate::runner::Runner);

impl Source for RunnerSource {
  fn open(&self) -> Result<Box<dyn Feed>, String> {
    Ok(Box::new(self.0.chat_subscribe()?))
  }

  fn enabled(&self) -> bool {
    self.0.enabled()
  }
}

impl Feed for crate::runner::ChatFeed {
  fn next_push(&mut self) -> Result<Option<Push>, String> {
    crate::runner::ChatFeed::next_push(self)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;
  use std::sync::Mutex;

  struct Script(VecDeque<Result<Push, String>>);

  impl Feed for Script {
    fn next_push(&mut self) -> Result<Option<Push>, String> {
      match self.0.pop_front() {
        Some(Ok(v)) => Ok(Some(v)),
        Some(Err(e)) => Err(e),
        None => Ok(None),
      }
    }
  }

  type Opened = Result<Vec<Result<Push, String>>, String>;

  struct Fake {
    opens: Mutex<VecDeque<Opened>>,
    stop: Arc<AtomicBool>,
    on: Arc<AtomicBool>,
    opened: Mutex<u32>,
  }

  impl Source for Fake {
    fn enabled(&self) -> bool {
      self.on.load(Ordering::SeqCst)
    }

    fn open(&self) -> Result<Box<dyn Feed>, String> {
      *self.opened.lock().unwrap() += 1;
      match self.opens.lock().unwrap().pop_front() {
        Some(Ok(items)) => Ok(Box::new(Script(items.into()))),
        Some(Err(e)) => Err(e),
        None => {
          self.stop.store(true, Ordering::SeqCst);
          Err("done".into())
        }
      }
    }
  }

  fn push(id: &str) -> Push {
    Push::Chat(json!({"message": {
      "v": 1, "id": id, "room": {"kind": "manager", "machine": "mac"},
      "author": {"kind": "manager", "id": "m"}, "ts_ms": 1, "kind": "text", "body_md": "hi"
    }}))
  }

  fn working(current: &str) -> Push {
    Push::Activity(ChatActivity {
      v: 1,
      room: tmgr_ipc::chat::RoomRef { kind: tmgr_ipc::chat::RoomKind::Manager, machine: "mac".into(), session: None },
      state: tmgr_ipc::chat::ActivityState::Working,
      current: Some(current.into()),
      used: vec![current.into()],
      calls: Vec::new(),
    })
  }

  fn drive(opens: Vec<Result<Vec<Result<Push, String>>, String>>) -> (Vec<String>, Vec<Duration>) {
    let (got, sleeps, _) = drive_resync(opens);
    (got, sleeps)
  }

  fn drive_resync(opens: Vec<Result<Vec<Result<Push, String>>, String>>) -> (Vec<String>, Vec<Duration>, u32) {
    let stop = Arc::new(AtomicBool::new(false));
    let fake = Fake { opens: Mutex::new(opens.into()), stop: stop.clone(), on: Arc::new(AtomicBool::new(true)), opened: Mutex::new(0) };
    let got = Mutex::new(Vec::new());
    let sleeps = Mutex::new(Vec::new());
    let resyncs = Mutex::new(0);
    run(&fake, &|m| got.lock().unwrap().push(m.id), &|_| {}, &|| *resyncs.lock().unwrap() += 1, &stop, &|d| sleeps.lock().unwrap().push(d));
    (got.into_inner().unwrap(), sleeps.into_inner().unwrap(), resyncs.into_inner().unwrap())
  }

  #[test]
  fn resync_fires_once_per_successful_reconnect_and_never_for_the_first_connection() {
    let (_, _, none) = drive_resync(vec![Ok(vec![Ok(push("m_1"))])]);
    assert_eq!(none, 0);
    let (_, _, two) = drive_resync(vec![Ok(vec![]), Err("down".into()), Ok(vec![]), Ok(vec![])]);
    assert_eq!(two, 2);
  }

  #[test]
  fn a_disabled_source_is_never_opened_and_a_live_feed_is_dropped_once_disabled() {
    let stop = Arc::new(AtomicBool::new(false));
    let on = Arc::new(AtomicBool::new(false));
    let fake = Fake { opens: Mutex::new(vec![Ok(vec![Ok(push("m_1"))])].into()), stop: stop.clone(), on: on.clone(), opened: Mutex::new(0) };
    let sleeps = Mutex::new(0);
    let sleep = |_: Duration| {
      *sleeps.lock().unwrap() += 1;
      if *sleeps.lock().unwrap() >= 3 {
        stop.store(true, Ordering::SeqCst);
      }
    };
    run(&fake, &|_| {}, &|_| {}, &|| {}, &stop, &sleep);
    assert_eq!(*fake.opened.lock().unwrap(), 0);
    let got = Mutex::new(Vec::new());
    let stop2 = Arc::new(AtomicBool::new(false));
    let fake = Fake { opens: Mutex::new(vec![Ok(vec![Ok(push("m_1")), Ok(push("m_2"))])].into()), stop: stop2.clone(), on: on.clone(), opened: Mutex::new(0) };
    on.store(true, Ordering::SeqCst);
    run(&fake, &|m| {
      got.lock().unwrap().push(m.id);
      on.store(false, Ordering::SeqCst);
    }, &|_| {}, &|| {}, &stop2, &|_| stop2.store(true, Ordering::SeqCst));
    assert_eq!(*got.lock().unwrap(), ["m_1"]);
  }

  #[test]
  fn activity_frames_reach_the_activity_callback_and_leave_messages_alone() {
    let stop = Arc::new(AtomicBool::new(false));
    let fake = Fake { opens: Mutex::new(vec![Ok(vec![Ok(working("Read: a.ts")), Ok(push("m_1")), Ok(working("Bash: cargo test"))])].into()), stop: stop.clone(), on: Arc::new(AtomicBool::new(true)), opened: Mutex::new(0) };
    let messages = Mutex::new(Vec::new());
    let activity = Mutex::new(Vec::new());
    run(&fake, &|m| messages.lock().unwrap().push(m.id), &|a| activity.lock().unwrap().push(a.current.unwrap()), &|| {}, &stop, &|_| {});
    assert_eq!(*messages.lock().unwrap(), ["m_1"]);
    assert_eq!(*activity.lock().unwrap(), ["Read: a.ts", "Bash: cargo test"]);
  }

  #[test]
  fn activity_is_not_deduplicated_like_messages() {
    let stop = Arc::new(AtomicBool::new(false));
    let fake = Fake { opens: Mutex::new(vec![Ok(vec![Ok(working("x")), Ok(working("x"))])].into()), stop: stop.clone(), on: Arc::new(AtomicBool::new(true)), opened: Mutex::new(0) };
    let count = Mutex::new(0);
    run(&fake, &|_| {}, &|_| *count.lock().unwrap() += 1, &|| {}, &stop, &|_| {});
    assert_eq!(*count.lock().unwrap(), 2);
  }

  #[test]
  fn parses_the_wire_push_and_ignores_garbage() {
    let Push::Chat(wire) = push("m_1") else { unreachable!() };
    assert_eq!(parse_push(&wire).unwrap().body_md, "hi");
    assert!(parse_push(&json!({"message": {"id": 5}})).is_none());
    assert!(parse_push(&json!({"nope": 1})).is_none());
  }

  #[test]
  fn emits_each_message_once_even_across_reconnects() {
    let (got, _) = drive(vec![
      Ok(vec![Ok(push("m_1")), Ok(push("m_1")), Ok(Push::Chat(json!({"x": 1}))), Ok(push("m_2"))]),
      Ok(vec![Ok(push("m_2")), Ok(push("m_3"))]),
    ]);
    assert_eq!(got, ["m_1", "m_2", "m_3"]);
  }

  #[test]
  fn reconnects_with_growing_backoff_that_resets_after_a_connection() {
    let (got, sleeps) = drive(vec![
      Err("down".into()),
      Err("down".into()),
      Err("down".into()),
      Ok(vec![Ok(push("m_1")), Err("closed".into())]),
      Err("down".into()),
    ]);
    assert_eq!(got, ["m_1"]);
    let ms: Vec<u128> = sleeps.iter().map(|d| d.as_millis()).collect();
    assert_eq!(&ms[..5], &[500, 1000, 2000, 500, 1000]);
  }

  #[test]
  fn backoff_is_capped() {
    let mut b = Backoff::new();
    let last = (0..20).map(|_| b.next()).last().unwrap();
    assert_eq!(last, BACKOFF_MAX);
  }

  #[test]
  fn seen_set_is_bounded() {
    let mut s = Seen::default();
    for i in 0..SEEN_CAP + 10 {
      assert!(s.first_time(&format!("m_{i}")));
    }
    assert!(s.first_time("m_0"));
    assert_eq!(s.ids.len(), s.order.len());
    assert!(s.ids.len() <= SEEN_CAP);
  }
}
