use serde_json::Value;
use tmgr_phone::b64_decode;

use super::{Core, RelayEngine};
use crate::engine::{ChatSink, KeyStore, Transport};
use crate::wire::Opened;

pub const PART_BYTES: usize = 737_280;
pub const MAX_RELAY_BYTES: u64 = 6 * 1024 * 1024;
pub const MAX_RELAY_PARTS: u32 = 9;
pub const MAX_LOCAL_BYTES: u64 = 25 * 1024 * 1024;
pub const MAX_LOCAL_PARTS: u32 = 36;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobPart {
    pub sha256: String,
    pub size: u64,
    pub part: u32,
    pub parts: u32,
    pub data: Vec<u8>,
}

pub fn parts_for(size: u64) -> u64 {
    size.div_ceil(PART_BYTES as u64)
}

impl BlobPart {
    pub fn parse(payload: &Value, max_size: u64) -> Result<BlobPart, &'static str> {
        const BAD: &str = "malformed blob payload";
        let obj = payload.as_object().ok_or(BAD)?;
        if obj.get("v").and_then(Value::as_u64) != Some(1) {
            return Err(BAD);
        }
        let sha256 = obj.get("sha256").and_then(Value::as_str).ok_or(BAD)?;
        let size = obj.get("size").and_then(Value::as_u64).ok_or(BAD)?;
        let part =
            u32::try_from(obj.get("part").and_then(Value::as_u64).ok_or(BAD)?).map_err(|_| BAD)?;
        let parts =
            u32::try_from(obj.get("parts").and_then(Value::as_u64).ok_or(BAD)?).map_err(|_| BAD)?;
        let data = obj.get("data").and_then(Value::as_str).ok_or(BAD)?;
        if !valid_sha(sha256) || size == 0 || size > max_size {
            return Err(BAD);
        }
        if u64::from(parts) != parts_for(size) || part >= parts {
            return Err(BAD);
        }
        if data.len() > PART_BYTES.div_ceil(3) * 4 {
            return Err(BAD);
        }
        let data = b64_decode(data).map_err(|_| BAD)?;
        let expected = if part + 1 == parts {
            size - PART_BYTES as u64 * u64::from(parts - 1)
        } else {
            PART_BYTES as u64
        };
        if data.len() as u64 != expected {
            return Err(BAD);
        }
        Ok(BlobPart {
            sha256: sha256.to_owned(),
            size,
            part,
            parts,
            data,
        })
    }
}

pub fn valid_sha(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

impl<T: Transport, K: KeyStore, S: ChatSink> RelayEngine<T, K, S> {
    pub(super) fn apply_blob(&self, c: &mut Core, o: &Opened) -> Result<(), String> {
        let sender_ok = c.st.joined
            && o.trusted
            && !c
                .st
                .pending_revokes
                .iter()
                .any(|p| p.target_fp == o.from_fp)
            && matches!(c.st.kind_of_trusted(&o.from), Some("desktop" | "mobile"));
        if !sender_ok {
            c.drop_reason = Some("blob from a sender that may not send files");
            return Ok(());
        }
        match BlobPart::parse(&o.payload, MAX_RELAY_BYTES) {
            Ok(part) => self.s.deliver_blob(&o.from, part),
            Err(reason) => {
                c.drop_reason = Some(reason);
                Ok(())
            }
        }
    }
}
