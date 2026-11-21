//! Contract between the composite Manager backend and the relay engine (rooms of other machines).
//! The engine is built on the sans-IO crate `tmgr-chat-relay`; HTTP and key storage sit behind ports.

use serde::{Deserialize, Serialize};

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
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingJoin {
  pub device_id: String,
  pub label: String,
  pub fp: String,
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
  fn decide_revoke(&self, _statement_id: &str, _accept: bool) -> Result<(), String> {
    Err("chat sync is not set up".into())
  }
  fn reset_identity(&self) -> Result<(), String>;
  fn poll_once(&self) -> Result<Vec<ChatMessage>, String>;
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
