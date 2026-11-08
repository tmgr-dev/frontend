use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::json;
use sha2::{Digest, Sha256};

const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

pub struct Audit {
  path: PathBuf,
  tip: Mutex<Option<String>>,
}

fn sha(s: &str) -> String {
  tmgr_phone::hex(&Sha256::digest(s.as_bytes()))
}

impl Audit {
  pub fn new(path: PathBuf) -> Audit {
    Audit { path, tip: Mutex::new(None) }
  }

  fn load_tip(&self) -> String {
    std::fs::read_to_string(&self.path)
      .ok()
      .and_then(|t| t.lines().last().map(sha))
      .unwrap_or_else(|| GENESIS.to_owned())
  }

  pub fn append(&self, now_ms: i64, event: &str, fields: &[(&str, &str)]) {
    let mut tip = self.tip.lock().unwrap_or_else(|p| p.into_inner());
    let prev = tip.clone().unwrap_or_else(|| self.load_tip());
    let mut entry = json!({ "t": now_ms, "event": event, "prev": prev });
    for (k, v) in fields {
      entry[*k] = json!(v);
    }
    let line = entry.to_string();
    let written =
      OpenOptions::new().create(true).append(true).mode(0o600).open(&self.path).and_then(|mut f| writeln!(f, "{line}"));
    match written {
      Ok(()) => *tip = Some(sha(&line)),
      Err(e) => log::warn!("[phones] audit write failed: {e}"),
    }
  }
}
