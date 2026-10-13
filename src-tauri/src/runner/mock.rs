use std::io::Write;
use std::os::unix::net::UnixStream;
use std::thread::{self, JoinHandle};

use serde_json::{json, Value};

use super::conn::{proof, read_frame, write_json, Frame};

pub struct Mock {
  pub secret: String,
  pub bad_server_proof: bool,
  pub reject_proto: bool,
  pub welcome_proto: u64,
  pub sessions: Vec<Value>,
}

impl Mock {
  pub fn new(secret: &str) -> Mock {
    Mock { secret: secret.into(), bad_server_proof: false, reject_proto: false, welcome_proto: 1, sessions: Vec::new() }
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

pub fn spawn(mut s: UnixStream, mock: Mock) -> JoinHandle<Log> {
  thread::spawn(move || {
    let mut log = Log::default();
    let Ok(Some(Frame::Json(hello))) = read_frame(&mut s) else { return log };
    log.hello = hello.clone();
    if mock.reject_proto {
      let _ = write_json(&mut s, &json!({"type": "error", "code": "proto_mismatch", "message": "server speaks protocol 2"}));
      return log;
    }
    let nonce_c = hello["nonce_c"].as_str().unwrap_or_default().to_owned();
    let nonce_s = "ab".repeat(32);
    let mut proof_s = proof(&mock.secret, "tmgr-runner-server", &nonce_c, &nonce_s);
    if mock.bad_server_proof {
      proof_s = "00".repeat(32);
    }
    let _ = write_json(&mut s, &json!({"type": "challenge", "nonce_s": nonce_s, "proof_s": proof_s}));
    let Ok(Some(Frame::Json(auth))) = read_frame(&mut s) else { return log };
    if auth["proof_c"].as_str() != Some(&proof(&mock.secret, "tmgr-runner-client", &nonce_s, &nonce_c)) {
      let _ = write_json(&mut s, &json!({"type": "error", "code": "unauthorized", "message": "bad proof"}));
      return log;
    }
    let _ = write_json(
      &mut s,
      &json!({"type": "welcome", "proto": mock.welcome_proto, "runner_version": "0.1.0", "caps": ["runs.create"]}),
    );
    loop {
      match read_frame(&mut s) {
        Ok(Some(Frame::Json(v))) if v["type"] == "request" => {
          log.requests.push(v.clone());
          let id = v["id"].clone();
          let reply = match v["method"].as_str().unwrap_or_default() {
            "runner.status" => Ok(json!({"pid": 1, "version": "0.1.0", "proto": 1, "uptime_s": 3, "sessions": mock.sessions.len()})),
            "sessions.list" => Ok(Value::Array(mock.sessions.clone())),
            "runs.create" => {
              let mut sess = session("s_newsessio2", json!({"kind": "starting", "since_ms": 1, "detail": null, "exit_code": null}));
              sess["task"] = json!({"workspace_id": v["params"]["task"]["workspace_id"], "task_id": v["params"]["task"]["task_id"]});
              sess["profile"] = v["params"]["profile"].clone();
              Ok(sess)
            }
            "sessions.kill" | "attach.open" | "attach.set" => Ok(Value::Null),
            other => Err(json!({"code": "unknown_method", "message": other})),
          };
          let msg = match reply {
            Ok(ok) => json!({"type": "response", "id": id, "ok": ok}),
            Err(err) => json!({"type": "response", "id": id, "err": err}),
          };
          if write_json(&mut s, &msg).is_err() {
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
    let Ok(Some(Frame::Json(hello))) = read_frame(&mut s) else { return };
    let nonce_c = hello["nonce_c"].as_str().unwrap_or_default().to_owned();
    let nonce_s = "ab".repeat(32);
    let proof_s = proof(&secret, "tmgr-runner-server", &nonce_c, &nonce_s);
    let _ = write_json(&mut s, &json!({"type": "challenge", "nonce_s": nonce_s, "proof_s": proof_s}));
    let Ok(Some(Frame::Json(_))) = read_frame(&mut s) else { return };
    let _ = write_json(&mut s, &json!({"type": "welcome", "proto": 1, "runner_version": "0.1.0", "caps": []}));
    script(&mut s);
  })
}
