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
