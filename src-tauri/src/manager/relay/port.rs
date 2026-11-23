//! Contract between the composite Manager backend and the relay engine (rooms of other machines).
//! The engine is built on the sans-IO crate `tmgr-chat-relay`; HTTP and key storage sit behind ports.

use serde::{Deserialize, Serialize};

use super::rooms::MachineIndex;
use crate::manager::backend::{ChatMessage, HistoryParams, RoomInfo, RoomRef, SearchParams, SendParams};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
  Trusted,
  Pending,
  Revoked,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustDevice {
  pub id: String,
  pub kind: String,
  pub label: String,
  pub fp: String,
  pub state: DeviceState,
  pub is_self: bool,
  #[serde(default)]
  pub kind_verified: bool,
  #[serde(default)]
  pub can_confirm_kind: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingJoin {
  pub device_id: String,
  pub label: String,
  pub fp: String,
  #[serde(default)]
  pub kind: Option<String>,
  pub requested_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingRevoke {
  pub statement_id: String,
  pub revoker_label: String,
  pub revoker_fp: String,
  pub target_id: String,
  pub target_label: String,
  pub target_fp: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReceiptState {
  Delivered,
  Queued,
  Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Receipt {
  pub id: String,
  pub state: ReceiptState,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SeqGap {
  pub from_device: String,
  pub expected: u64,
  pub got: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustAlert {
  pub kind: String,
  pub device_id: String,
  pub at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TrustState {
  pub registered: bool,
  pub self_fp: Option<String>,
  pub devices: Vec<TrustDevice>,
  pub pending_joins: Vec<PendingJoin>,
  #[serde(default)]
  pub pending_revokes: Vec<PendingRevoke>,
  #[serde(default)]
  pub key_mismatch: Vec<String>,
  #[serde(default)]
  pub seq_gaps: Vec<SeqGap>,
  #[serde(default)]
  pub alerts: Vec<TrustAlert>,
  #[serde(default)]
  pub gate_blockers: Vec<String>,
  #[serde(default)]
  pub auto_join_error: Option<String>,
  #[serde(default)]
  pub joining: bool,
  #[serde(default)]
  pub can_join_with_code: bool,
}

#[allow(dead_code)]
pub trait RelayPort: Send + Sync {
  fn rooms(&self) -> Result<Vec<RoomInfo>, String>;
  fn history(&self, params: &HistoryParams) -> Result<Vec<ChatMessage>, String>;
  fn search(&self, params: &SearchParams) -> Result<Vec<ChatMessage>, String>;
  fn send(&self, params: SendParams) -> Result<ChatMessage, String>;
  fn owns(&self, room: &RoomRef) -> bool;
  fn trust_state(&self) -> Result<TrustState, String>;
  fn decide_join(&self, device_id: &str, accept: bool) -> Result<(), String>;
  fn revoke(&self, device_id: &str) -> Result<(), String>;
  fn confirm_device_kind(&self, _device_id: &str, _kind: &str) -> Result<(), String> {
    Err("chat sync is not set up".into())
  }
  fn decide_revoke(&self, _statement_id: &str, _accept: bool) -> Result<(), String> {
    Err("chat sync is not set up".into())
  }
  fn open_join_code(&self) -> Result<String, String> {
    Err("chat sync is not set up".into())
  }
  fn join_with_code(&self, _code: &str) -> Result<(), String> {
    Err("chat sync is not set up".into())
  }
  fn close_join_code(&self, _expires_at_ms: u64) -> Result<(), String> {
    Ok(())
  }
  fn expect_join(&self, _device_id: &str, _fp: &str) -> Result<(), String> {
    Err("chat sync is not set up".into())
  }
  fn machine_indexes(&self) -> Vec<MachineIndex> {
    vec![]
  }
  fn reset_identity(&self) -> Result<(), String>;
  fn poll_once(&self) -> Result<Vec<ChatMessage>, String>;
  fn take_receipts(&self) -> Vec<Receipt> {
    vec![]
  }
  fn flush(&self) {}
}

pub struct NoRelay;

impl RelayPort for NoRelay {
  fn rooms(&self) -> Result<Vec<RoomInfo>, String> {
    Ok(vec![])
  }
  fn history(&self, _: &HistoryParams) -> Result<Vec<ChatMessage>, String> {
    Err("unknown room".into())
  }
  fn search(&self, _: &SearchParams) -> Result<Vec<ChatMessage>, String> {
    Ok(vec![])
  }
  fn send(&self, _: SendParams) -> Result<ChatMessage, String> {
    Err("chat sync is not set up".into())
  }
  fn owns(&self, _: &RoomRef) -> bool {
    false
  }
  fn trust_state(&self) -> Result<TrustState, String> {
    Ok(TrustState::default())
  }
  fn decide_join(&self, _: &str, _: bool) -> Result<(), String> {
    Err("chat sync is not set up".into())
  }
  fn revoke(&self, _: &str) -> Result<(), String> {
    Err("chat sync is not set up".into())
  }
  fn reset_identity(&self) -> Result<(), String> {
    Err("chat sync is not set up".into())
  }
  fn poll_once(&self) -> Result<Vec<ChatMessage>, String> {
    Ok(vec![])
  }
}
