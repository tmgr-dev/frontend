use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use tmgr_ipc::api::sha256_hex;

pub fn norm(s: &str) -> String {
    s.replace("\r\n", "\n")
}

pub fn content_sha256(title: &str, description: &str) -> String {
    let t = sha256_hex(norm(title).as_bytes());
    let d = sha256_hex(norm(description).as_bytes());
    sha256_hex(format!("tmgr-task-content:v1:{t}:{d}").as_bytes())
}

#[allow(clippy::too_many_arguments)]
pub fn message(
    host: &str,
    user_id: i64,
    workspace_id: i64,
    task_id: i64,
    category_id: i64,
    content_sha256: &str,
    key_id: &str,
    signed_at: i64,
) -> String {
    format!(
        "tmgr-agent-approval:v1:{host}:{user_id}:{workspace_id}:{task_id}:{category_id}:{content_sha256}:{key_id}:{signed_at}"
    )
}

pub fn verify(public_key_b64: &str, msg: &str, signature_b64: &str) -> bool {
    let Ok(key) = STANDARD.decode(public_key_b64) else {
        return false;
    };
    let Ok(sig) = STANDARD.decode(signature_b64) else {
        return false;
    };
    let Ok(key) = VerifyingKey::from_sec1_bytes(&key) else {
        return false;
    };
    let Ok(sig) = Signature::from_slice(&sig) else {
        return false;
    };
    key.verify(msg.as_bytes(), &sig).is_ok()
}

pub fn fingerprint(public_key_b64: &str) -> String {
    let raw = STANDARD.decode(public_key_b64).unwrap_or_default();
    Sha256::digest(&raw)
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn valid_public_key(public_key_b64: &str) -> bool {
    STANDARD
        .decode(public_key_b64)
        .ok()
        .is_some_and(|k| VerifyingKey::from_sec1_bytes(&k).is_ok())
}
