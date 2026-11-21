//! Pure verifier; the caller burns a challenge by its nonce (one winner per card), never by signature bytes, because ECDSA signatures are malleable.
//! `req_sha256` is always hashed from `SpawnProposal::canonical_bytes`; never hash a re-serialized `serde_json::Value`.

mod error;
mod key_statement;
mod keystore;
mod proposal;
mod tokens;
mod verify;
mod wire;

pub use error::Refusal;
pub use key_statement::{
    build_spawn_key_statement, device_fp, supersedes, verify_spawn_key_statement, DeviceKeys,
    SpawnKeyStatement,
};
pub use keystore::{
    ApproveError, DeviceFp, KeyBook, KeyState, KeyView, OfferOutcome, RevokeError, RevokedKey,
};
pub use proposal::{sha256_hex, SpawnMode, SpawnProposal};
pub use tokens::{b64_decode_strict, b64_encode, is_hex_lower, is_target_id, is_token};
pub use verify::{
    signed_string, verify, Approved, ChallengeRecord, PinnedKey, TrustStatus, TrustView,
    MAX_SKEW_MS, MAX_TTL_MS,
};
pub use wire::SignedSpawn;
