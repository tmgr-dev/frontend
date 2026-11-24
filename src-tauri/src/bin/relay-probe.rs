use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tmgr_lib::relay_probe::{build_engine, EngineSpec, KeyStore, LoginToken, RelayPort, Slot};

struct FileStore(PathBuf);

impl FileStore {
  fn path(&self, slot: Slot) -> PathBuf {
    self.0.join(format!("{slot:?}.json").to_lowercase())
  }
}

impl KeyStore for FileStore {
  fn load(&self, slot: Slot) -> Result<Option<String>, String> {
    match std::fs::read_to_string(self.path(slot)) {
      Ok(v) => Ok(Some(v)),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
      Err(e) => Err(e.to_string()),
    }
  }

  fn save(&self, slot: Slot, value: &str) -> Result<(), String> {
    let tmp = self.0.join(".tmp");
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp).map_err(|e| e.to_string())?;
    f.write_all(value.as_bytes()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, self.path(slot)).map_err(|e| e.to_string())
  }

  fn clear(&self) -> Result<(), String> {
    for slot in [Slot::Identity, Slot::State, Slot::Log] {
      let _ = std::fs::remove_file(self.path(slot));
    }
    Ok(())
  }
}

struct TokenFile(PathBuf);

impl LoginToken for TokenFile {
  fn token(&self) -> Option<String> {
    std::fs::read_to_string(&self.0).ok().map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
  }
}

struct StderrLog;

impl log::Log for StderrLog {
  fn enabled(&self, m: &log::Metadata) -> bool {
    m.level() <= log::Level::Info
  }
  fn log(&self, r: &log::Record) {
    if self.enabled(r.metadata()) {
      emit(json!({"event": "log", "level": r.level().as_str(), "text": r.args().to_string()}));
    }
  }
  fn flush(&self) {}
}

fn now_ms() -> u64 {
  SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn emit(mut v: Value) {
  v["t_ms"] = json!(now_ms());
  let mut out = std::io::stdout().lock();
  let _ = writeln!(out, "{v}");
  let _ = out.flush();
}

fn room_of(spec: &str) -> Result<Value, String> {
  match spec.split(':').collect::<Vec<_>>().as_slice() {
    ["manager", m] => Ok(json!({"kind": "manager", "machine": m})),
    ["session", m, s] => Ok(json!({"kind": "session", "machine": m, "session": s})),
    _ => Err("room must be manager:<machine> or session:<machine>:<id>".into()),
  }
}

fn command(eng: &dyn RelayPort, cmd: &Value) -> Result<(), String> {
  match cmd["cmd"].as_str().unwrap_or("") {
    "join" => emit(json!({"event": "join_code", "code": eng.open_join_code()?})),
    "send" => {
      let room = room_of(cmd["room"].as_str().unwrap_or(""))?;
      let params = serde_json::from_value(json!({"room": room, "body_md": cmd["text"], "reply_to": null})).map_err(|e| e.to_string())?;
      let started = now_ms();
      let sent = eng.send(params)?;
      eng.flush();
      emit(json!({"event": "post_start", "msg_id": sent.id, "post_start_ms": started}));
    }
    "state" => emit(json!({"event": "state", "trust": serde_json::to_value(eng.trust_state()?).map_err(|e| e.to_string())?,
      "rooms": serde_json::to_value(eng.rooms()?).map_err(|e| e.to_string())?})),
    other => return Err(format!("unknown command {other}")),
  }
  Ok(())
}

fn main() {
  let args: Vec<String> = std::env::args().collect();
  if args.len() != 6 {
    eprintln!("usage: relay-probe <state-dir> <api-base> <token-file> <user-id> <local-machine>\nstdin: JSON lines {{\"cmd\":\"join\"|\"send\"|\"state\", \"room\", \"text\"}}");
    std::process::exit(2);
  }
  let _ = rustls::crypto::ring::default_provider().install_default();
  let _ = log::set_logger(&StderrLog).map(|()| log::set_max_level(log::LevelFilter::Info));
  let dir = PathBuf::from(&args[1]);
  std::fs::create_dir_all(&dir).expect("state dir");
  let spec = EngineSpec {
    user_id: args[4].parse().expect("user id"),
    api_base: args[2].clone(),
    local_machine: args[5].clone(),
    login: Arc::new(TokenFile(PathBuf::from(&args[3]))),
  };
  let eng = build_engine(spec, FileStore(dir)).unwrap_or_else(|e| {
    eprintln!("engine: {e}");
    std::process::exit(1);
  });
  let (tx, rx) = mpsc::channel::<Value>();
  std::thread::spawn(move || {
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
      match serde_json::from_str(&line) {
        Ok(v) => {
          if tx.send(v).is_err() {
            return;
          }
        }
        Err(e) => emit(json!({"event": "bad_command", "error": e.to_string()})),
      }
    }
  });
  let poll = Duration::from_millis(std::env::var("PROBE_POLL_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(1000));
  let mut alerts: HashSet<String> = HashSet::new();
  emit(json!({"event": "listening"}));
  loop {
    while let Ok(cmd) = rx.try_recv() {
      if let Err(e) = command(&*eng, &cmd) {
        emit(json!({"event": "command_failed", "cmd": cmd, "error": e}));
      }
    }
    match eng.poll_once() {
      Ok(msgs) => {
        for m in msgs {
          emit(json!({"event": "message", "msg_id": m.id, "room": m.room, "author": m.author, "kind": m.kind, "text": m.body_md}));
        }
      }
      Err(e) => emit(json!({"event": "poll_error", "error": e})),
    }
    for r in eng.take_receipts() {
      emit(json!({"event": "receipt", "msg_id": r.id, "state": r.state, "reason": r.reason}));
    }
    if let Ok(st) = eng.trust_state() {
      for j in &st.pending_joins {
        let ok = eng.decide_join(&j.device_id, true);
        emit(json!({"event": "join_approved", "device_id": j.device_id, "kind": j.kind, "error": ok.err()}));
      }
      for a in serde_json::to_value(&st.alerts).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default() {
        if alerts.insert(a.to_string()) {
          emit(json!({"event": "alert", "alert": a}));
        }
      }
    }
    std::thread::sleep(poll);
  }
}
