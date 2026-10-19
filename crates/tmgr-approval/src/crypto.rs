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

pub const MAX_REPOS: usize = 5;

pub fn valid_repo_name(name: &str) -> bool {
    let Some((owner, repo)) = name.split_once('/') else {
        return false;
    };
    let lower_alnum = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    owner.len() <= 39
        && owner.chars().next().is_some_and(lower_alnum)
        && owner.chars().all(|c| lower_alnum(c) || c == '-')
        && (1..=100).contains(&repo.len())
        && repo
            .chars()
            .all(|c| lower_alnum(c) || matches!(c, '.' | '_' | '-'))
        && !repo.ends_with(".git")
        && repo != "."
        && repo != ".."
}

/// The sorted set the signature covers; refuses instead of normalising so every client hashes the same bytes.
pub fn canonical_repos(names: &[String]) -> Result<Vec<String>, String> {
    if names.is_empty() || names.len() > MAX_REPOS {
        return Err(format!("a repository set has 1 to {MAX_REPOS} names"));
    }
    if let Some(bad) = names.iter().find(|n| !valid_repo_name(n)) {
        return Err(format!(
            "{bad:?} is not a lowercase owner/name of a GitHub repository"
        ));
    }
    let mut out = names.to_vec();
    out.sort();
    if out.windows(2).any(|w| w[0] == w[1]) {
        return Err("a repository set lists a name twice".into());
    }
    Ok(out)
}

pub fn repos_sha256(names: &[String]) -> Result<String, String> {
    let joined = canonical_repos(names)?.join("\n");
    Ok(sha256_hex(format!("tmgr-repos:v1:{joined}").as_bytes()))
}

pub fn valid_sha256_hex(s: &str) -> bool {
    s.len() == 64
        && s.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

#[allow(clippy::too_many_arguments)]
pub fn message_v2(
    host: &str,
    user_id: i64,
    workspace_id: i64,
    task_id: i64,
    category_id: i64,
    content_sha256: &str,
    repos_sha256: &str,
    key_id: &str,
    signed_at: i64,
) -> String {
    format!(
        "tmgr-agent-approval:v2:{host}:{user_id}:{workspace_id}:{task_id}:{category_id}:{content_sha256}:{repos_sha256}:{key_id}:{signed_at}"
    )
}

pub const MAX_PERSONA_BYTES: usize = 32768;
pub const MAX_CONTEXT_PAGES: usize = 5;
pub const MAX_CONTEXT_BYTES: usize = 65536;

pub fn valid_persona_id(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_digit() || (b'a'..=b'f').contains(c),
        })
}

pub fn persona_sha256(id: &str, name: &str, instructions: &str) -> Result<String, String> {
    if !valid_persona_id(id) {
        return Err("the persona id is not a lowercase UUID".into());
    }
    let n = sha256_hex(norm(name).as_bytes());
    let i = sha256_hex(norm(instructions).as_bytes());
    Ok(sha256_hex(
        format!("tmgr-persona:v1:{id}:{n}:{i}").as_bytes(),
    ))
}

pub fn page_sha256(id: i64, title: &str, body: &str) -> String {
    let t = sha256_hex(norm(title).as_bytes());
    let b = sha256_hex(norm(body).as_bytes());
    sha256_hex(format!("tmgr-page:v1:{id}:{t}:{b}").as_bytes())
}

pub fn context_sha256(page_hashes: &[String]) -> Result<String, String> {
    if page_hashes.is_empty() || page_hashes.len() > MAX_CONTEXT_PAGES {
        return Err(format!(
            "the workspace context has 1 to {MAX_CONTEXT_PAGES} pages"
        ));
    }
    if page_hashes.iter().any(|h| !valid_sha256_hex(h)) {
        return Err("a page hash is not 64 lowercase hex characters".into());
    }
    Ok(sha256_hex(
        format!("tmgr-context:v1:{}", page_hashes.join("\n")).as_bytes(),
    ))
}

#[allow(clippy::too_many_arguments)]
pub fn message_v3(
    host: &str,
    user_id: i64,
    workspace_id: i64,
    task_id: i64,
    category_id: i64,
    content_sha256: &str,
    repos_sha256: Option<&str>,
    persona_sha256: Option<&str>,
    context_sha256: Option<&str>,
    key_id: &str,
    signed_at: i64,
) -> String {
    let repos = repos_sha256.unwrap_or("-");
    let persona = persona_sha256.unwrap_or("-");
    let context = context_sha256.unwrap_or("-");
    format!(
        "tmgr-agent-approval:v3:{host}:{user_id}:{workspace_id}:{task_id}:{category_id}:{content_sha256}:{repos}:{persona}:{context}:{key_id}:{signed_at}"
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    pub host: String,
    pub user_id: i64,
    pub workspace_id: i64,
    pub task_id: i64,
    pub category_id: i64,
    pub content_sha256: String,
    pub repos_sha256: Option<String>,
    pub persona_sha256: Option<String>,
    pub context_sha256: Option<String>,
    pub key_id: String,
    pub signed_at: i64,
}

/// v3 only when a persona or the context is covered, else v2 when repositories are, else v1.
pub fn approval_message(p: &Payload) -> String {
    if p.persona_sha256.is_some() || p.context_sha256.is_some() {
        return message_v3(
            &p.host,
            p.user_id,
            p.workspace_id,
            p.task_id,
            p.category_id,
            &p.content_sha256,
            p.repos_sha256.as_deref(),
            p.persona_sha256.as_deref(),
            p.context_sha256.as_deref(),
            &p.key_id,
            p.signed_at,
        );
    }
    match &p.repos_sha256 {
        Some(repos) => message_v2(
            &p.host,
            p.user_id,
            p.workspace_id,
            p.task_id,
            p.category_id,
            &p.content_sha256,
            repos,
            &p.key_id,
            p.signed_at,
        ),
        None => message(
            &p.host,
            p.user_id,
            p.workspace_id,
            p.task_id,
            p.category_id,
            &p.content_sha256,
            &p.key_id,
            p.signed_at,
        ),
    }
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
