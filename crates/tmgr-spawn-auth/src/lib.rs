//! Pure verifier; the caller burns a challenge by its nonce (one winner per card), never by signature bytes, because ECDSA signatures are malleable.
//! `req_sha256` is always hashed from `SpawnProposal::canonical_bytes`; never hash a re-serialized `serde_json::Value`.

mod error;
mod key_statement;
mod keystore;
mod permit;
mod proposal;
mod tokens;
mod verify;
mod wire;

pub use error::Refusal;
pub use key_statement::{
    build_presence_key_statement, build_spawn_key_statement, device_fp, supersedes,
    verify_presence_key_statement, verify_spawn_key_statement, DeviceKeys, SpawnKeyStatement,
};
pub use keystore::{
    ApproveError, DeviceFp, KeyBook, KeyState, KeyView, OfferOutcome, RevokeError, RevokedKey,
};
pub use permit::{
    permit_signed_string, verify_permit, DesktopPolicy, PermitChoice, PermitKey, PermitOutcome,
    PermitRecord, PermitRefusal, PermitScreen, PermitSender, PermitSenderKind, PermittedAnswer,
    SignedPermit, MAX_PERMIT_TTL_MS,
};
pub use proposal::{sha256_hex, ProposalKind, SpawnMode, SpawnProposal};
pub use tokens::{b64_decode_strict, b64_encode, is_hex_lower, is_target_id, is_token};
pub use verify::{
    signed_string, verify, Approved, ChallengeRecord, PinnedKey, TrustStatus, TrustView,
    MAX_SKEW_MS, MAX_TTL_MS,
};
pub use wire::SignedSpawn;
