pub mod attach;
pub mod audit;
pub mod authkeys;
pub mod fence;
pub mod follower;
pub mod grammar;
pub mod leader;
pub mod ownership;
pub mod privileged;
pub mod proto;
pub mod registry;
pub mod runner;
pub mod runs;
pub mod scope;
pub mod sshd;
pub mod store;
pub mod taskref;
pub mod worker;

use tmgr_core::error::{Error, Kind};

pub(crate) fn forbidden(message: impl AsRef<str>) -> Error {
    Error::new(Kind::Forbidden, message)
}

pub(crate) fn link_err(message: impl AsRef<str>) -> Error {
    Error::new(Kind::Link, message)
}

pub(crate) fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
