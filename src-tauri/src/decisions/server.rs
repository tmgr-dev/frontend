use std::io;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tmgr_ipc::decisions::{
  prepare_server_dir, read_line_limited, socket_endpoint, token_ok, write_line, Op, Request, Response, MAX_WAIT_MS,
  PROTO, SOCK, UNAUTHORIZED,
};
use tmgr_ipc::endpoint;
use tmgr_ipc::server::accept_check;

use super::hub::Hub;

const READ_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_CONNECTIONS: usize = 256;

pub struct Server {
  stop: Arc<AtomicBool>,
  sock: PathBuf,
}

impl Server {
  pub fn start(hub: Arc<Hub>, dir: &Path) -> io::Result<Server> {
    tmgr_ipc::dirs::ensure_private_dir(dir)?;
    let listener = endpoint::bind(&socket_endpoint(dir))?;
    let token = prepare_server_dir(dir)?;
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let active = Arc::new(AtomicUsize::new(0));
    std::thread::Builder::new().name("decisions-accept".into()).spawn(move || {
      for conn in listener.incoming() {
        if flag.load(Ordering::SeqCst) {
          break;
        }
        let Ok(stream) = conn else {
          std::thread::sleep(Duration::from_millis(50));
          continue;
        };
        if active.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
          active.fetch_sub(1, Ordering::SeqCst);
          continue;
        }
        let (hub, token, active) = (hub.clone(), token.clone(), active.clone());
        let spawned = std::thread::Builder::new().name("decisions-conn".into()).spawn(move || {
          handle(stream, &hub, &token);
          active.fetch_sub(1, Ordering::SeqCst);
        });
        if spawned.is_err() {
          log::warn!("[decisions] could not start a connection thread");
        }
      }
    })?;
    Ok(Server { stop, sock: dir.join(SOCK) })
  }
}

impl Drop for Server {
  fn drop(&mut self) {
    self.stop.store(true, Ordering::SeqCst);
    let _ = UnixStream::connect(&self.sock);
    let _ = std::fs::remove_file(&self.sock);
  }
}

fn handle(stream: UnixStream, hub: &Arc<Hub>, token: &str) {
  if accept_check(&stream).is_err() {
    return;
  }
  let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
  let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
  let resp = match read_line_limited(&stream) {
    Ok(Some(line)) => match serde_json::from_slice::<Request>(&line) {
      Ok(req) if !token_ok(token, &req.token) => Response::err(UNAUTHORIZED),
      Ok(req) if req.v != PROTO => Response::err("unsupported protocol version"),
      Ok(req) => dispatch(hub, req.op),
      Err(_) => Response::err("bad request"),
    },
    Ok(None) => return,
    Err(_) => Response::err("bad request"),
  };
  let _ = write_line(&mut &stream, &resp);
}

fn dispatch(hub: &Arc<Hub>, op: Op) -> Response {
  let done = |r: Result<Response, String>| r.unwrap_or_else(Response::err);
  match op {
    Op::Ping => Response { ok: true, version: Some(env!("CARGO_PKG_VERSION").into()), ..Default::default() },
    Op::Create { card } => {
      done(hub.create(*card).map(|(id, outcome)| Response {
        ok: true,
        card_id: Some(id),
        outcome: Some(outcome.to_owned()),
        ..Default::default()
      }))
    }
    Op::Wait { card_id, since, timeout_ms, final_only } => {
      let timeout = Duration::from_millis(timeout_ms.min(MAX_WAIT_MS));
      done(hub.wait(&card_id, since, timeout, final_only).map(|(card_id, event)| Response {
        ok: true,
        card_id: Some(card_id),
        event,
        ..Default::default()
      }))
    }
    Op::Reply { card_id, text, line_replies } => done(hub.reply(&card_id, text, line_replies).map(|card_id| Response {
      ok: true,
      card_id: Some(card_id),
      ..Default::default()
    })),
    Op::CloseSession { agent, session } => {
      done(hub.close_session(&agent, &session).map(|n| Response { ok: true, closed: Some(n), ..Default::default() }))
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::Write;
  use std::sync::Mutex;
  use std::time::Instant;
  use tmgr_ipc::decisions::{spool, CardIn, Client, Opt, PatchIn, Source, TOKEN};

  use crate::decisions::hub::Change;
  use crate::decisions::store::{AnswerIn, MessageIn, Store};

  struct Rig {
    _tmp: tempfile::TempDir,
    dir: PathBuf,
    hub: Arc<Hub>,
    changes: Arc<Mutex<Vec<usize>>>,
    updates: Arc<Mutex<usize>>,
    _server: Server,
  }

  fn rig() -> Rig {
    let tmp = tempfile::Builder::new().prefix("dq").tempdir_in("/tmp").unwrap();
    let dir = tmp.path().join("d");
    tmgr_ipc::dirs::ensure_private_dir(&dir).unwrap();
    let changes = Arc::new(Mutex::new(Vec::new()));
    let seen = changes.clone();
    let updates = Arc::new(Mutex::new(0));
    let updated = updates.clone();
    let sink: crate::decisions::hub::Sink = Box::new(move |c, n| match c {
      Change::Created { .. } => seen.lock().unwrap().push(n),
      Change::Updated => *updated.lock().unwrap() += 1,
    });
    let hub = Arc::new(Hub::new(Store::open(&dir.join("decisions.db")).unwrap(), sink).with_spool(dir.clone()));
    hub.ingest_spool(&dir);
    let server = Server::start(hub.clone(), &dir).unwrap();
    Rig { _tmp: tmp, dir, hub, changes, updates, _server: server }
  }

  fn card(title: &str) -> CardIn {
    CardIn {
      title: title.into(),
      source: Source { agent: "claude".into(), session: Some("s1".into()), ..Default::default() },
      options: vec![Opt { id: "a".into(), label: "Ship".into(), recommended: true }],
      ..Default::default()
    }
  }

  fn answer_a() -> AnswerIn {
    AnswerIn { choice: Some("a".into()), ..Default::default() }
  }

  fn raw(dir: &Path, line: &str) -> String {
    let mut s = UnixStream::connect(dir.join(SOCK)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    s.write_all(line.as_bytes()).unwrap();
    s.write_all(b"\n").unwrap();
    String::from_utf8(read_line_limited(&s).unwrap().unwrap_or_default()).unwrap()
  }

  #[test]
  fn ping_create_and_reject_bad_tokens() {
    let r = rig();
    let c = Client::open_in(&r.dir).unwrap();
    assert_eq!(c.ping().unwrap().version.as_deref(), Some(env!("CARGO_PKG_VERSION")));
    let id = c.create(card("Ship?")).unwrap();
    assert_eq!(r.hub.get(&id).unwrap().title, "Ship?");
    assert_eq!(r.changes.lock().unwrap().as_slice(), [1]);
    assert_eq!(c.create(CardIn { id: Some(id.clone()), ..card("Ship?") }).unwrap(), id);
    assert_eq!(r.changes.lock().unwrap().len(), 1);

    let bad = raw(&r.dir, r#"{"v":1,"token":"nope","op":"ping"}"#);
    assert!(bad.contains("unauthorized"), "{bad}");
    let tok = std::fs::read_to_string(r.dir.join(TOKEN)).unwrap();
    let wrong_len = raw(&r.dir, &format!(r#"{{"v":1,"token":"{}","op":"ping"}}"#, &tok.trim()[..8]));
    assert!(wrong_len.contains("unauthorized"));
    let missing = raw(&r.dir, r#"{"v":1,"op":"ping"}"#);
    assert!(missing.contains("bad request"));
    let garbage = raw(&r.dir, "not json");
    assert!(garbage.contains("bad request"));
    let version = raw(&r.dir, &format!(r#"{{"v":9,"token":"{}","op":"ping"}}"#, tok.trim()));
    assert!(version.contains("unsupported"));
    assert!(r.hub.list().unwrap().len() == 1);
  }

  #[test]
  fn second_start_fails_without_locking_clients_out() {
    let r = rig();
    let second = Server::start(r.hub.clone(), &r.dir);
    assert!(second.is_err());
    let c = Client::open_in(&r.dir).unwrap();
    let id = c.create(card("Still here")).unwrap();
    assert_eq!(r.hub.get(&id).unwrap().title, "Still here");
  }

  #[test]
  fn create_errors_come_back_as_responses() {
    let r = rig();
    let c = Client::open_in(&r.dir).unwrap();
    assert!(c.create(CardIn { title: " ".into(), ..Default::default() }).is_err());
    assert!(c.reply("c_00000000000000000000000000000000", Some("x".into()), vec![]).is_err());
  }

  #[test]
  fn wait_times_out_with_null() {
    let r = rig();
    let c = Client::open_in(&r.dir).unwrap();
    let id = c.create(card("Ship?")).unwrap();
    let t = Instant::now();
    assert!(c.wait(&id, 0, Duration::from_millis(150), false).unwrap().is_none());
    assert!(t.elapsed() >= Duration::from_millis(140));
    assert!(c.wait("c_00000000000000000000000000000000", 0, Duration::ZERO, false).is_err());
  }

  #[test]
  fn wait_wakes_on_answer_and_final_repeats() {
    let r = rig();
    let c = Client::open_in(&r.dir).unwrap();
    let id = c.create(card("Ship?")).unwrap();
    let (hub, id2) = (r.hub.clone(), id.clone());
    let t = std::thread::spawn(move || {
      std::thread::sleep(Duration::from_millis(150));
      hub.answer(&id2, answer_a()).unwrap();
    });
    let t0 = Instant::now();
    let e = c.wait(&id, 0, Duration::from_secs(10), false).unwrap().unwrap();
    assert!(t0.elapsed() < Duration::from_secs(5));
    assert!(e.is_final());
    assert_eq!(e.choice_label.as_deref(), Some("Ship"));
    t.join().unwrap();
    for since in [0, 1, 9] {
      assert_eq!(c.wait(&id, since, Duration::from_millis(50), true).unwrap().unwrap().seq, 1);
    }
  }

  fn ask_at(r: &Rig, id: &str, text: &str, thread: Option<(&str, u32)>) {
    let m = MessageIn {
      text: text.into(),
      file: thread.map(|(f, _)| f.into()),
      line: thread.map(|(_, l)| l),
      ..Default::default()
    };
    r.hub.message(id, m).unwrap();
  }

  fn last_message(r: &Rig, id: &str) -> (String, Option<String>, Option<u32>) {
    let m = r.hub.get(id).unwrap().messages.pop().unwrap();
    (m.text, m.file, m.line)
  }

  #[test]
  fn text_replies_route_by_pending_thread_through_the_client() {
    let r = rig();
    let c = Client::open_in(&r.dir).unwrap();
    let id = c.create(card("Review")).unwrap();

    ask_at(&r, &id, "why?", Some(("web:src/a.vue", 7)));
    c.reply(&id, Some("because".into()), vec![]).unwrap();
    assert_eq!(last_message(&r, &id), ("because".into(), Some("web:src/a.vue".into()), Some(7)));

    c.reply(&id, Some("hello".into()), vec![]).unwrap();
    assert_eq!(last_message(&r, &id), ("hello".into(), None, None));

    ask_at(&r, &id, "one", Some(("a.rs", 3)));
    ask_at(&r, &id, "two", Some(("b.rs", 9)));
    let before = r.hub.get(&id).unwrap().messages.len();
    let line = tmgr_ipc::decisions::LineComment { file: "c.rs".into(), line: 1, text: "x".into() };
    let err = c.reply(&id, Some("hm".into()), vec![line]).unwrap_err().to_string();
    assert!(err.contains("2 line threads are waiting: a.rs:3, b.rs:9; answer them with line_replies"), "{err}");
    assert_eq!(r.hub.get(&id).unwrap().messages.len(), before);

    ask_at(&r, &id, "in chat", None);
    c.reply(&id, Some("chat wins".into()), vec![]).unwrap();
    assert_eq!(last_message(&r, &id), ("chat wins".into(), None, None));
  }

  #[test]
  fn messages_follow_the_since_cursor() {
    let r = rig();
    let c = Client::open_in(&r.dir).unwrap();
    let id = c.create(card("Review")).unwrap();
    r.hub.message(&id, MessageIn { text: "one".into(), ..Default::default() }).unwrap();
    r.hub.message(&id, MessageIn { text: "two".into(), ..Default::default() }).unwrap();
    let first = c.wait(&id, 0, Duration::from_millis(50), false).unwrap().unwrap();
    assert_eq!((first.seq, first.text.as_deref()), (1, Some("one")));
    let second = c.wait(&id, first.seq, Duration::from_millis(50), false).unwrap().unwrap();
    assert_eq!((second.seq, second.text.as_deref()), (2, Some("two")));
    assert!(c.wait(&id, 2, Duration::from_millis(50), false).unwrap().is_none());
    assert!(c.wait(&id, 0, Duration::from_millis(50), true).unwrap().is_none());
    c.reply(&id, Some("done".into()), vec![]).unwrap();
    assert_eq!(r.hub.get(&id).unwrap().messages.len(), 3);
  }

  #[test]
  fn deferred_unblocks_then_the_final_answer_wins() {
    let r = rig();
    let c = Client::open_in(&r.dir).unwrap();
    let id = c.create(card("Ship?")).unwrap();
    r.hub.answer(&id, AnswerIn { deferred: true, ..Default::default() }).unwrap();
    let d = c.wait(&id, 0, Duration::from_millis(50), false).unwrap().unwrap();
    assert!(d.deferred);
    assert!(c.wait(&id, d.seq, Duration::from_millis(50), true).unwrap().is_none());
    r.hub.answer(&id, answer_a()).unwrap();
    let f = c.wait(&id, d.seq, Duration::from_millis(50), true).unwrap().unwrap();
    assert!(f.is_final() && f.seq == 2);
  }

  #[test]
  fn close_session_over_the_wire() {
    let r = rig();
    let c = Client::open_in(&r.dir).unwrap();
    let hook = CardIn { kind: "stopped".into(), reply_via: "hook".into(), ..card("done") };
    let id = c.create(hook).unwrap();
    assert_eq!(c.close_session("claude", "s1").unwrap(), 1);
    assert_eq!(r.hub.get(&id).unwrap().state, "closed");
  }

  #[test]
  fn spooled_cards_are_taken_in_once() {
    let r = rig();
    let id = spool(&r.dir, &card("while away")).unwrap();
    assert_eq!(r.hub.ingest_spool(&r.dir), 1);
    assert_eq!(r.hub.get(&id).unwrap().title, "while away");
    assert_eq!(r.hub.ingest_spool(&r.dir), 0);
  }

  #[test]
  fn spool_file_survives_a_store_error_and_invalid_cards_are_dropped() {
    let r = rig();
    let inbox = r.dir.join("inbox");
    let id = spool(&r.dir, &card("retry me")).unwrap();
    let file = inbox.join(format!("{id}.json"));
    let db = rusqlite::Connection::open(r.dir.join("decisions.db")).unwrap();
    db.execute_batch("ALTER TABLE cards RENAME TO cards_off").unwrap();
    assert_eq!(r.hub.ingest_spool(&r.dir), 0);
    assert!(file.exists());
    db.execute_batch("ALTER TABLE cards_off RENAME TO cards").unwrap();
    assert_eq!(r.hub.ingest_spool(&r.dir), 1);
    assert!(!file.exists());
    assert_eq!(r.hub.get(&id).unwrap().title, "retry me");

    let bad = spool(&r.dir, &CardIn { title: " ".into(), ..Default::default() }).unwrap();
    let bad_file = inbox.join(format!("{bad}.json"));
    assert_eq!(r.hub.ingest_spool(&r.dir), 0);
    assert!(!bad_file.exists());
  }

  #[test]
  fn liveness_marks_dead_sessions() {
    let r = rig();
    let c = Client::open_in(&r.dir).unwrap();
    let mut a = card("alive");
    a.source.pid = Some(111);
    let mut b = card("dead");
    b.source.pid = Some(222);
    let (ida, idb) = (c.create(a).unwrap(), c.create(b).unwrap());
    r.hub.check_liveness(|pid| pid == 111);
    assert!(!r.hub.get(&ida).unwrap().session_gone);
    assert!(r.hub.get(&idb).unwrap().session_gone);
  }

  fn resend(title: &str, target: &str) -> CardIn {
    CardIn { replaces: Some(target.into()), ..card(title) }
  }

  #[test]
  fn resend_over_the_wire_updates_in_place_without_a_notification() {
    let r = rig();
    let c = Client::open_in(&r.dir).unwrap();
    let (a, outcome) = c.create_outcome(card("first")).unwrap();
    assert_eq!(outcome.as_deref(), Some("created"));
    let before = *r.updates.lock().unwrap();
    let b = tmgr_ipc::decisions::new_card_id();
    let (canon, outcome) = c.create_outcome(CardIn { id: Some(b.clone()), ..resend("second", &a) }).unwrap();
    assert_eq!((canon.as_str(), outcome.as_deref()), (a.as_str(), Some("updated")));
    assert_eq!(r.changes.lock().unwrap().len(), 1);
    assert_eq!(*r.updates.lock().unwrap(), before + 1);
    assert_eq!(r.hub.get(&a).unwrap().title, "second");
    assert_eq!(r.hub.list().unwrap().len(), 1);
    let (again, _) = c.create_outcome(CardIn { id: Some(b), ..resend("third", &a) }).unwrap();
    assert_eq!(again, a);
    assert_eq!(*r.updates.lock().unwrap(), before + 1);
  }

  #[test]
  fn waiters_on_the_old_and_the_new_id_both_get_the_answer() {
    let r = rig();
    let c = Client::open_in(&r.dir).unwrap();
    let a = c.create(card("first")).unwrap();
    let b = tmgr_ipc::decisions::new_card_id();
    assert_eq!(c.create(CardIn { id: Some(b.clone()), ..resend("second", &a) }).unwrap(), a);
    let waiters: Vec<_> = [a.clone(), b]
      .into_iter()
      .map(|id| {
        let dir = r.dir.clone();
        std::thread::spawn(move || Client::open_in(&dir).unwrap().wait(&id, 0, Duration::from_secs(10), true).unwrap())
      })
      .collect();
    std::thread::sleep(Duration::from_millis(150));
    r.hub.answer(&a, answer_a()).unwrap();
    for w in waiters {
      let e = w.join().unwrap().unwrap();
      assert!(e.is_final());
      assert_eq!(e.card_id, a);
    }
  }

  #[test]
  fn reply_and_wait_resolve_aliases_to_the_canonical_id() {
    let r = rig();
    let c = Client::open_in(&r.dir).unwrap();
    let a = c.create(card("first")).unwrap();
    let b = tmgr_ipc::decisions::new_card_id();
    c.create(CardIn { id: Some(b.clone()), ..resend("second", &a) }).unwrap();
    let reply = r.hub.reply(&b, Some("hi".into()), vec![]).unwrap();
    assert_eq!(reply, a);
    c.reply(&b, Some("again".into()), vec![]).unwrap();
    assert_eq!(r.hub.get(&a).unwrap().messages.len(), 2);
    r.hub.message(&b, MessageIn { text: "q".into(), ..Default::default() }).unwrap();
    let (id, event) = r.hub.wait(&b, 0, Duration::from_millis(50), false).unwrap();
    assert_eq!((id, event.unwrap().card_id), (a.clone(), a));
  }

  #[test]
  fn resend_to_an_answered_card_returns_the_answer_at_once() {
    let r = rig();
    let c = Client::open_in(&r.dir).unwrap();
    let a = c.create(card("first")).unwrap();
    r.hub.answer(&a, answer_a()).unwrap();
    let b = tmgr_ipc::decisions::new_card_id();
    let (canon, outcome) = c.create_outcome(CardIn { id: Some(b.clone()), ..resend("second", &a) }).unwrap();
    assert_eq!((canon.as_str(), outcome.as_deref()), (a.as_str(), Some("final")));
    assert_eq!(r.hub.list().unwrap().len(), 1);
    let e = c.wait(&b, 0, Duration::from_millis(50), true).unwrap().unwrap();
    assert_eq!((e.card_id.as_str(), e.choice.as_deref()), (a.as_str(), Some("a")));
  }

  fn quiet_hub(dir: &Path) -> Arc<Hub> {
    let sink: crate::decisions::hub::Sink = Box::new(|_, _| {});
    Arc::new(Hub::new(Store::open(&dir.join("decisions.db")).unwrap(), sink).with_spool(dir.to_path_buf()))
  }

  fn private_dir() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::Builder::new().prefix("dq").tempdir_in("/tmp").unwrap();
    let dir = tmp.path().join("d");
    tmgr_ipc::dirs::ensure_private_dir(&dir).unwrap();
    (tmp, dir)
  }

  fn is_unauthorized(e: &io::Error) -> bool {
    e.to_string().contains("unauthorized")
  }

  #[test]
  fn a_held_client_survives_a_restart_with_a_rotated_token() {
    let (_tmp, dir) = private_dir();
    let hub = quiet_hub(&dir);
    let first = Server::start(hub.clone(), &dir).unwrap();
    let held = Client::open_in(&dir).unwrap();
    held.ping().unwrap();
    let old = std::fs::read_to_string(dir.join(TOKEN)).unwrap();
    drop(first);
    let _second = Server::start(hub, &dir).unwrap();
    assert_ne!(old, std::fs::read_to_string(dir.join(TOKEN)).unwrap());
    held.ping().unwrap();
    held.create(card("after the restart")).unwrap();
  }

  #[test]
  fn clients_opened_during_restarts_never_see_unauthorized() {
    let (_tmp, dir) = private_dir();
    let hub = quiet_hub(&dir);
    let mut server = Some(Server::start(hub.clone(), &dir).unwrap());
    let id = Client::open_in(&dir).unwrap().create(card("stress")).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let counts = Arc::new([AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0)]);
    let workers: Vec<_> = (0..6)
      .map(|n| {
        let (dir, id, stop, counts) = (dir.clone(), id.clone(), stop.clone(), counts.clone());
        std::thread::spawn(move || {
          while !stop.load(Ordering::SeqCst) {
            let r = Client::open_in(&dir).and_then(|c| {
              if n % 2 == 0 {
                c.ping().map(|_| ())
              } else {
                c.wait(&id, 0, Duration::from_millis(5), false).map(|_| ())
              }
            });
            let slot = match r {
              Ok(()) => 0,
              Err(e) if is_unauthorized(&e) => 1,
              Err(_) => 2,
            };
            counts[slot].fetch_add(1, Ordering::SeqCst);
          }
        })
      })
      .collect();
    for i in 0..60u64 {
      drop(server.take());
      std::thread::sleep(Duration::from_millis(i % 4));
      server = Some(Server::start(hub.clone(), &dir).unwrap());
      std::thread::sleep(Duration::from_millis(3));
    }
    stop.store(true, Ordering::SeqCst);
    for w in workers {
      w.join().unwrap();
    }
    let n = |i: usize| counts[i].load(Ordering::SeqCst);
    eprintln!("stress: ok={} unauthorized={} other_errors={}", n(0), n(1), n(2));
    assert_eq!(n(1), 0);
    assert!(n(0) > 0);
  }

  #[test]
  fn cards_aliases_patches_and_answers_survive_a_restart() {
    let (_tmp, dir) = private_dir();
    let hub = quiet_hub(&dir);
    let server = Server::start(hub.clone(), &dir).unwrap();
    let held = Client::open_in(&dir).unwrap();
    let patches = vec![PatchIn { label: "one".into(), text: "diff one".into() }, PatchIn { label: "two".into(), text: "diff two".into() }];
    let a = held.create(CardIn { patches: patches.clone(), ..card("first") }).unwrap();
    let b = tmgr_ipc::decisions::new_card_id();
    assert_eq!(held.create(CardIn { id: Some(b.clone()), patches: patches.clone(), ..resend("second", &a) }).unwrap(), a);
    assert_eq!(hub.get_with_patches(&a).unwrap().1, patches);
    let answered = held.create(card("answered early")).unwrap();
    hub.answer(&answered, answer_a()).unwrap();
    let old_token = std::fs::read_to_string(dir.join(TOKEN)).unwrap();

    drop(server);
    drop(hub);
    std::thread::sleep(Duration::from_millis(200));
    let hub = quiet_hub(&dir);
    hub.ingest_spool(&dir);
    let _server = Server::start(hub.clone(), &dir).unwrap();
    assert_ne!(old_token, std::fs::read_to_string(dir.join(TOKEN)).unwrap());

    assert_eq!(hub.get(&b).unwrap().id, a);
    let (view, kept) = hub.get_with_patches(&a).unwrap();
    assert_eq!(view.title, "second");
    assert_eq!(kept, patches);
    let early = held.wait(&answered, 0, Duration::from_secs(5), true).unwrap().unwrap();
    assert!(early.is_final());
    assert_eq!(early.card_id, answered);

    let waiters: Vec<_> = [a.clone(), b]
      .into_iter()
      .map(|id| {
        let dir = dir.clone();
        std::thread::spawn(move || Client::open_in(&dir).unwrap().wait(&id, 0, Duration::from_secs(10), true).unwrap())
      })
      .collect();
    std::thread::sleep(Duration::from_millis(150));
    hub.answer(&a, answer_a()).unwrap();
    for w in waiters {
      let e = w.join().unwrap().unwrap();
      assert!(e.is_final());
      assert_eq!(e.card_id, a);
    }
    let late = held.wait(&a, 0, Duration::from_secs(5), true).unwrap().unwrap();
    assert!(late.is_final());
  }

  fn stamp(dir: &Path, id: &str, secs: u64) {
    let file = std::fs::File::options().write(true).open(dir.join("inbox").join(format!("{id}.json"))).unwrap();
    file.set_modified(std::time::UNIX_EPOCH + Duration::from_secs(secs)).unwrap();
  }

  #[test]
  fn spooled_resends_apply_in_order_and_redelivery_is_idempotent() {
    let r = rig();
    let a = spool(&r.dir, &card("first")).unwrap();
    let b_id = tmgr_ipc::decisions::new_card_id();
    let b_card = CardIn { id: Some(b_id.clone()), ..resend("second", &a) };
    spool(&r.dir, &b_card).unwrap();
    stamp(&r.dir, &a, 1_000);
    stamp(&r.dir, &b_id, 2_000);
    assert_eq!(r.hub.ingest_spool(&r.dir), 1);
    assert_eq!(r.hub.list().unwrap().len(), 1);
    assert_eq!(r.hub.get(&a).unwrap().title, "second");
    assert_eq!(r.hub.get(&b_id).unwrap().id, a);

    let c = Client::open_in(&r.dir).unwrap();
    c.create(resend("third", &a)).unwrap();
    spool(&r.dir, &b_card).unwrap();
    assert_eq!(r.hub.ingest_spool(&r.dir), 0);
    assert_eq!(r.hub.get(&a).unwrap().title, "third");
    assert_eq!(r.changes.lock().unwrap().len(), 0);
  }

  #[test]
  fn wait_for_a_spooled_unknown_id_drains_the_spool_first() {
    let r = rig();
    let c = Client::open_in(&r.dir).unwrap();
    assert!(c.wait("c_00000000000000000000000000000000", 0, Duration::ZERO, false).is_err());
    let id = spool(&r.dir, &card("while away")).unwrap();
    assert!(c.wait(&id, 0, Duration::from_millis(50), false).unwrap().is_none());
    assert_eq!(r.hub.get(&id).unwrap().title, "while away");
    assert_eq!(r.hub.ingest_spool(&r.dir), 0);
  }
}
