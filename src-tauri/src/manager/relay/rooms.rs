//! Sealed `rooms` index sent by a runner to owner devices: display data only, no trust decision reads it.

use serde::{Deserialize, Serialize};

use crate::manager::backend::{RoomKind, RoomRef};

const MAX_ROOMS: usize = 200;
const MAX_FOLLOWERS: usize = 200;
const MAX_LABEL: usize = 64;
const MAX_TITLE: usize = 256;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexRoom {
  pub room: RoomRef,
  pub title: String,
  #[serde(default)]
  pub state: Option<String>,
  #[serde(default)]
  pub context_pct: Option<u8>,
  #[serde(default)]
  pub last_ts: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoomsIndex {
  pub v: u32,
  pub machine: String,
  pub machine_label: String,
  pub role: String,
  #[serde(default)]
  pub followers: Vec<String>,
  pub rooms: Vec<IndexRoom>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredRooms {
  pub from: String,
  pub index: RoomsIndex,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MachineIndex {
  pub machine: String,
  pub label: String,
  pub role: String,
  pub followers: Vec<String>,
}

pub fn valid_machine(id: &str) -> bool {
  id.starts_with("mx_") && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

pub fn valid_index_msg_id(id: &str) -> bool {
  id.len() == 34 && id.starts_with("i_") && id[2..].bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

impl RoomsIndex {
  pub fn is_valid(&self) -> bool {
    let label_ok = (1..=MAX_LABEL).contains(&self.machine_label.chars().count()) && !self.machine_label.chars().any(char::is_control);
    let role_ok = match self.role.as_str() {
      "top" => self.followers.len() <= MAX_FOLLOWERS && self.followers.iter().all(|f| valid_machine(f)),
      "sub" => self.followers.is_empty(),
      _ => false,
    };
    self.v == 1
      && valid_machine(&self.machine)
      && label_ok
      && role_ok
      && self.rooms.len() <= MAX_ROOMS
      && self.rooms.iter().all(|r| {
        let shape = match r.room.kind {
          RoomKind::Manager => r.room.session.is_none(),
          RoomKind::Session => r.room.session.is_some(),
        };
        shape && r.room.machine == self.machine && r.title.len() <= MAX_TITLE
      })
  }
}
