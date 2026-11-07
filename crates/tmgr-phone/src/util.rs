use crate::{Error, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use sha2::{Digest, Sha256};

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn h(x: &str) -> String {
    hex(&Sha256::digest(x.as_bytes()))
}

pub fn b64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

pub fn b64_decode(s: &str) -> Result<Vec<u8>> {
    STANDARD.decode(s).map_err(|_| Error::BadFormat)
}
