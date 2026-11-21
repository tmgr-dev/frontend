use serde::{Deserialize, Serialize};

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
}
