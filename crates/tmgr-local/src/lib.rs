mod context;
mod db;
mod dispatch;
mod error;
mod journal;
mod js;
mod markdown;
mod persona;
mod router;
mod routes;
mod serialize;
mod workspace;

#[cfg(test)]
mod testing;

pub use context::{Actor, Context, User};
pub use db::{open_read, open_write, MIN_SCHEMA_VERSION, SCHEMA_VERSION};
pub use dispatch::{dispatch, Response};
pub use error::{Error, Result};
pub use journal::{
    agent_session_detected, comments_digest, confirm_task, content_sha, in_agent_session,
    provenance, provenance_in, provenance_observed, row_hash, snapshot, store_checkpoint,
    verify_chain, verify_chain_full, ChainStatus, HumanContent, JournalRow, Observed, Provenance,
    Snapshot, WriteOrigin, AGENT_SESSION_MARKERS, CHECKPOINT_KEY, GENESIS_HASH,
};
pub use workspace::{find, list_in, local_id, root, LocalWorkspace, Manifest, ROOT_ENV};
