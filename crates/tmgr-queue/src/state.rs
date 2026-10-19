use crate::cloud::CloudClaim;
use crate::paths::{read_private, Paths};
use crate::rules::ProfilePin;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tmgr_core::config::{ensure_private_dir, write_atomic};
use tmgr_core::error::{Error, Result};
use tmgr_ipc::api::{RunBundle, TaskRef};

const KEEP_RELEASED: usize = 200;
pub const MAX_PUBLISH_ATTEMPTS: u32 = 3;

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickupClone {
    pub name: String,
    pub dev: u64,
    pub ino: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Held {
    pub at: String,
    #[serde(default)]
    pub shas: BTreeMap<String, String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub findings: usize,
}

const HELD_PREFIX: &str = "held: ";

pub fn held_error(text: &str) -> Error {
    Error::conflict(format!("{HELD_PREFIX}{text}"))
}

pub fn is_held(e: &Error) -> bool {
    e.message.starts_with(HELD_PREFIX)
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
    #[serde(default)]
    pub run_clean: bool,
    #[serde(default = "one")]
    pub attempt: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud: Option<CloudClaim>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
    #[serde(default)]
    pub publish_attempts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publish_error: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clones: Vec<PickupClone>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held: Option<Held>,
}

fn one() -> u32 {
    1
}

impl Pickup {
    pub fn start_key(&self) -> String {
        format!("{}-{}", self.id, self.attempt)
    }

    pub fn awaits_publish(&self) -> bool {
        self.cloud.is_some()
            && self.status == Status::Started
            && self.run_exited
            && self.run_clean
            && self.pr_url.is_none()
            && self.publish_attempts < MAX_PUBLISH_ATTEMPTS
            && self.held.is_none()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_is_recognised_by_its_prefix_only() {
        assert!(is_held(&held_error("the leak gate found 1 finding(s)")));
        assert!(!is_held(&Error::conflict("push refused")));
        assert!(!is_held(&Error::internal("x held: y")));
    }

    #[test]
    fn a_hold_survives_the_state_file_and_a_short_one_still_loads() {
        let held = Held {
            at: "t".into(),
            shas: [("o/r".to_string(), "abc".to_string())].into(),
            categories: vec!["secret".into()],
            findings: 1,
        };
        let text = serde_json::to_string(&held).unwrap();
        assert_eq!(serde_json::from_str::<Held>(&text).unwrap(), held);
        let short: Held = serde_json::from_str(r#"{"at":"t"}"#).unwrap();
        assert!(short.shas.is_empty() && short.categories.is_empty());
    }
}
