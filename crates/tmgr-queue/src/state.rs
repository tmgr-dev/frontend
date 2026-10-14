use crate::paths::{read_private, Paths};
use crate::rules::ProfilePin;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tmgr_core::config::{ensure_private_dir, write_atomic};
use tmgr_core::error::{Error, Result};
use tmgr_ipc::api::{RunBundle, TaskRef};

const KEEP_RELEASED: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    PendingApproval,
    Approved,
    Starting,
    UnknownStart,
    Started,
    Failed,
    Expired,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::PendingApproval => "pending_approval",
            Status::Approved => "approved",
            Status::Starting => "starting",
            Status::UnknownStart => "unknown_start",
            Status::Started => "started",
            Status::Failed => "failed",
            Status::Expired => "expired",
        }
    }

    pub fn is_live(self) -> bool {
        matches!(
            self,
            Status::Starting | Status::UnknownStart | Status::Started
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pickup {
    pub id: String,
    pub rule: String,
    pub task: TaskRef,
    pub key: String,
    pub status: Status,
    pub bundle: RunBundle,
    pub bundle_sha256: String,
    pub context_tainted: bool,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default)]
    pub released: bool,
    #[serde(default)]
    pub persona_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_pin: Option<ProfilePin>,
    #[serde(default)]
    pub run_exited: bool,
    #[serde(default = "one")]
    pub attempt: u32,
}

fn one() -> u32 {
    1
}

impl Pickup {
    pub fn start_key(&self) -> String {
        format!("{}-{}", self.id, self.attempt)
    }

    pub fn occupies(&self) -> bool {
        self.status.is_live() && !self.run_exited
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub pickups: Vec<Pickup>,
    #[serde(default)]
    pub skips: BTreeMap<String, String>,
}

pub fn load(paths: &Paths) -> Result<State> {
    match read_private(&paths.state())? {
        None => Ok(State {
            version: 1,
            ..State::default()
        }),
        Some(text) => serde_json::from_str(&text).map_err(|e| {
            Error::internal(format!("queue-state.json is damaged: {e}"))
                .hint("move it aside; open pickups will be proposed again")
        }),
    }
}

pub fn save(paths: &Paths, state: &mut State) -> Result<()> {
    ensure_private_dir(&paths.dir)?;
    state.version = 1;
    let released = state
        .pickups
        .iter()
        .filter(|p| p.released && !p.status.is_live())
        .count();
    if released > KEEP_RELEASED {
        let mut drop = released - KEEP_RELEASED;
        state.pickups.retain(|p| {
            if drop > 0 && p.released && !p.occupies() {
                drop -= 1;
                false
            } else {
                true
            }
        });
    }
    let bytes = serde_json::to_vec_pretty(state).map_err(|e| Error::internal(e.to_string()))?;
    write_atomic(&paths.state(), &bytes, 0o600)
}

impl State {
    pub fn live_count(&self, rule: Option<&str>) -> usize {
        self.pickups
            .iter()
            .filter(|p| p.occupies() && rule.is_none_or(|r| p.rule == r))
            .count()
    }
}
