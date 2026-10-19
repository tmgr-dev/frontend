use std::io::Write;
use std::os::unix::net::UnixStream;
use std::thread::{self, JoinHandle};

use serde_json::{json, Value};
use tmgr_ipc::api::{caps, Challenge, ErrorBody, ErrorCode, Handshake, Response, RunCreateReq, ServerMsg, Welcome};
use tmgr_ipc::frame::{read_frame, write_json, Frame};
use tmgr_ipc::{auth, server};

pub struct Mock {
  pub secret: String,
  pub bad_server_proof: bool,
  pub reject_proto: bool,
  pub welcome_proto: u32,
  pub caps: Option<Vec<String>>,
  pub busy: bool,
  pub folder_id: Option<(u64, u64)>,
  pub sessions: Vec<Value>,
}

impl Mock {
  pub fn new(secret: &str) -> Mock {
    Mock { secret: secret.into(), bad_server_proof: false, reject_proto: false, welcome_proto: 1, caps: None, busy: false, folder_id: None, sessions: Vec::new() }
  }

  fn standard(&self) -> bool {
    !self.bad_server_proof && !self.reject_proto && self.welcome_proto == 1 && self.caps.is_none()
  }
}

#[derive(Default)]
pub struct Log {
  pub hello: Value,
  pub requests: Vec<Value>,
  pub frames: Vec<Frame>,
}

pub fn session(id: &str, state: Value) -> Value {
  json!({
    "id": id, "name": "claude", "argv": ["claude"], "cwd": "/tmp/x", "profile": "claude-code",
    "state": state, "created_ms": 1000, "cols": 120, "rows": 32, "shim_pid": 1, "child_pid": 2,
    "task": {"workspace_id": -1, "task_id": 7}
  })
}

fn handshake(s: &mut UnixStream, mock: &Mock) -> Option<Value> {
  if mock.standard() {
    let hello = server::handshake_server(s, &mock.secret, "0.1.0").ok()?;
    return serde_json::to_value(hello).ok();
  }
  let Ok(Some(Frame::Json(hello))) = read_frame(s) else { return None };
  if mock.reject_proto {
    let body = ErrorBody { code: ErrorCode::ProtoMismatch, message: "server speaks protocol 2".into() };
    let _ = write_json(s, &Handshake::Error(body));
    return Some(hello);
  }
  let nonce_c = hello["nonce_c"].as_str().unwrap_or_default().to_owned();
  let nonce_s = "ab".repeat(32);
  let mut proof_s = auth::proof(&mock.secret, auth::SERVER, &nonce_c, &nonce_s);
  if mock.bad_server_proof {
    proof_s = "00".repeat(32);
  }
  let _ = write_json(s, &Handshake::Challenge(Challenge { nonce_s: nonce_s.clone(), proof_s }));
  let Ok(Some(Frame::Json(auth_msg))) = read_frame(s) else { return None };
  if auth_msg["proof_c"].as_str() != Some(&auth::proof(&mock.secret, auth::CLIENT, &nonce_s, &nonce_c)) {
    let body = ErrorBody { code: ErrorCode::Unauthorized, message: "bad proof".into() };
    let _ = write_json(s, &Handshake::Error(body));
    return None;
  }
  let caps = mock.caps.clone().unwrap_or_else(|| caps::ALL.map(String::from).to_vec());
  let _ = write_json(s, &Handshake::Welcome(Welcome { proto: mock.welcome_proto, runner_version: "0.1.0".into(), caps }));
  Some(hello)
}

fn err(code: ErrorCode, message: &str) -> ErrorBody {
  ErrorBody { code, message: message.into() }
}

fn handle(mock: &Mock, method: &str, params: &Value) -> Result<Value, ErrorBody> {
  match method {
    "runner.status" => Ok(json!({"pid": 1, "version": "0.1.0", "proto": 1, "uptime_s": 3, "sessions": mock.sessions.len()})),
    "sessions.list" => Ok(Value::Array(mock.sessions.clone())),
    "runs.create" => {
      let req: RunCreateReq = serde_json::from_value(params.clone()).map_err(|e| err(ErrorCode::InvalidParams, &e.to_string()))?;
      if let (Some(pin), Some(actual)) = (req.cwd_pin, mock.folder_id) {
        if (pin.dev, pin.ino) != actual {
          return Err(err(ErrorCode::InvalidParams, "approved folder changed"));
        }
      }
      if req.bundle.is_some() != req.bundle_sha256.is_some() {
        return Err(err(ErrorCode::InvalidParams, "bundle and bundle_sha256 go together"));
      }
      if let (Some(b), Some(hash)) = (&req.bundle, &req.bundle_sha256) {
        if b.sha256() != *hash {
          return Err(err(ErrorCode::InvalidParams, "bundle hash mismatch"));
        }
        if b.launch.profile != req.profile || Some(b.task) != req.task || b.launch.cwd != req.cwd {
          return Err(err(ErrorCode::InvalidParams, "bundle launch differs from the request"));
        }
      }
      let mut sess = session("s_newsessio2", json!({"kind": "starting", "since_ms": 1, "detail": null, "exit_code": null}));
      sess["profile"] = json!(req.profile);
      sess["cwd"] = json!(req.cwd);
      sess["name"] = json!(req.name.unwrap_or_default());
      sess["task"] = serde_json::to_value(req.task).unwrap_or(Value::Null);
      if let Some(hash) = req.bundle_sha256 {
        sess["bundle_sha256"] = json!(hash);
      }
      Ok(sess)
    }
    "sessions.kill" => match params["signal"].as_str() {
      Some("term" | "kill" | "int" | "hup") => Ok(Value::Null),
      _ => Err(err(ErrorCode::InvalidParams, "bad signal")),
    },
    "sessions.send" | "sessions.remove" if mock.busy => Err(err(ErrorCode::Busy, "input queue is full")),
    "autopilot.status" => Ok(json!({"running": true, "pid": 4242, "status": {"rule": "all", "mode": "pr", "runs": [{"task": "TM-1"}]}})),
    "autopilot.control" => match params["command"].as_str() {
      Some("pause" | "resume" | "stop" | "suspend") => Ok(json!({"seq": 7})),
      _ => Err(err(ErrorCode::InvalidParams, "bad command")),
    },
    "attach.open" | "attach.set" => Ok(Value::Null),
    other => Err(err(ErrorCode::UnknownMethod, other)),
  }
}

pub fn spawn(mut s: UnixStream, mock: Mock) -> JoinHandle<Log> {
  thread::spawn(move || {
    let mut log = Log::default();
    let Some(hello) = handshake(&mut s, &mock) else { return log };
    log.hello = hello;
    if mock.reject_proto {
      return log;
    }
    loop {
      match read_frame(&mut s) {
        Ok(Some(Frame::Json(v))) if v["type"] == "request" => {
          log.requests.push(v.clone());
          let id = v["id"].as_u64().unwrap_or_default();
          let reply = handle(&mock, v["method"].as_str().unwrap_or_default(), &v["params"]);
          let (ok, err) = match reply {
            Ok(ok) => (Some(ok), None),
            Err(e) => (None, Some(e)),
          };
          if write_json(&mut s, &ServerMsg::Response(Response { id, ok, err })).is_err() {
            break;
          }
        }
        Ok(Some(other)) => log.frames.push(other),
        _ => break,
      }
    }
    let _ = s.flush();
    log
  })
}

pub fn spawn_scripted(mut s: UnixStream, secret: &str, script: impl FnOnce(&mut UnixStream) + Send + 'static) -> JoinHandle<()> {
  let secret = secret.to_owned();
  thread::spawn(move || {
    if server::handshake_server(&s, &secret, "0.1.0").is_err() {
      return;
    }
    script(&mut s);
  })
}
