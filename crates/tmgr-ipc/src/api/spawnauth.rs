use serde::{Deserialize, Serialize};
use tmgr_spawn_auth::SpawnProposal;

/// Params of `spawn.challenge` (`client: "cli"` or `"desktop"` only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnChallengeParams {
    pub card_id: String,
    pub proposal: SpawnProposal,
}

/// `req` is the canonical request bytes (base64 standard, padded); the signer hashes them exactly as received.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnChallengeResult {
    pub nonce: String,
    pub issued_ms: u64,
    pub exp_ms: u64,
    pub target_id: String,
    pub target_fp: String,
    pub req: String,
    pub req_sha256: String,
}

/// Params of `spawn.card` (`client: "cli"` or `"desktop"` only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnCardParams {
    pub card_id: String,
}

/// `state` is one of `issued`, `approved`, `claimed`, `spawned`, `refused`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnCardResult {
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exp_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpawnDecision {
    Approve,
    Reject,
    Edit,
}

/// The replacement values of `decision: "edit"`; profile, target and prompt stay as proposed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnDecideEdit {
    pub cwd: String,
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
}

/// Params of `spawn.decide` (`client: "desktop"` only); `req_sha256` is the hash of the request bytes the owner saw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnDecideParams {
    pub card_id: String,
    pub req_sha256: String,
    pub decision: SpawnDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit: Option<SpawnDecideEdit>,
}

/// `state` is `approved`, `rejected` or `edited`; an edit also returns the new card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnDecideResult {
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exp_ms: Option<u64>,
}

/// One spawn key of a phone, `state` is `pending` or `approved`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnKeyInfo {
    pub device_fp: String,
    pub device_id: String,
    pub spawn_key_fp: String,
    pub state: String,
    pub issued_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_at_ms: Option<u64>,
}

/// Result of `spawn.keys.list` (`client: "cli"` only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnKeysList {
    pub keys: Vec<SpawnKeyInfo>,
}

/// Params of `spawn.keys.approve` (`client: "cli"` only); both fingerprints may be prefixes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnKeysApproveParams {
    pub device_fp: String,
    pub spawn_key_fp: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnKeysApproveResult {
    pub device_fp: String,
}

/// Params of `spawn.keys.revoke` (`client: "cli"` only); `device_fp` may be a prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnKeysRevokeParams {
    pub device_fp: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnKeysRevokeResult {
    pub device_fp: String,
    pub device_id: String,
    pub spawn_key_fp: String,
    pub state: String,
}
