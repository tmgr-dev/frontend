use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CLOUD_PREFIX: &str = "tmgrp_";
const LOCAL_PREFIX: &str = "tmgrl_";
const SECRET_BODY_LEN: usize = 43;
const ID_PREFIX: &str = "lt_";
const ID_BODY_LEN: usize = 16;
const PREFIX_SHOW_LEN: usize = 10;
pub const ISSUE_LIMIT_PER_HOUR: usize = 10;
const HOUR_SECS: i64 = 3600;
const LAST_USED_THROTTLE_SECS: i64 = 60;

const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Where a token's secret lives. Real tokens go to the OS keychain; tests use an in-memory store
/// so the pure token logic never touches the real keychain.
pub trait SecretStore: Send + Sync {
  fn set(&self, id: &str, secret: &str) -> Result<(), String>;
  fn get(&self, id: &str) -> Result<Option<String>, String>;
  fn delete(&self, id: &str) -> Result<(), String>;
}

pub const KEYCHAIN_SERVICE: &str = "dev.tmgr.local-access";

pub struct KeychainSecrets;

impl SecretStore for KeychainSecrets {
  fn set(&self, id: &str, secret: &str) -> Result<(), String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, id).map_err(|e| e.to_string())?.set_password(secret).map_err(|e| e.to_string())
  }

  fn get(&self, id: &str) -> Result<Option<String>, String> {
    match keyring::Entry::new(KEYCHAIN_SERVICE, id).map_err(|e| e.to_string())?.get_password() {
      Ok(secret) => Ok(Some(secret)),
      Err(keyring::Error::NoEntry) => Ok(None),
      Err(e) => Err(e.to_string()),
    }
  }

  fn delete(&self, id: &str) -> Result<(), String> {
    match keyring::Entry::new(KEYCHAIN_SERVICE, id).map_err(|e| e.to_string())?.delete_credential() {
      Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
      Err(e) => Err(e.to_string()),
    }
  }
}

/// For tests: never used in the shipped app, which always talks to the real keychain.
#[derive(Default)]
#[allow(dead_code)]
pub struct MemorySecrets(Mutex<HashMap<String, String>>);

#[allow(dead_code)]
impl MemorySecrets {
  pub fn new() -> Self {
    Self::default()
  }
}

impl SecretStore for MemorySecrets {
  fn set(&self, id: &str, secret: &str) -> Result<(), String> {
    self.0.lock().map_err(|e| e.to_string())?.insert(id.to_string(), secret.to_string());
    Ok(())
  }

  fn get(&self, id: &str) -> Result<Option<String>, String> {
    Ok(self.0.lock().map_err(|e| e.to_string())?.get(id).cloned())
  }

  fn delete(&self, id: &str) -> Result<(), String> {
    self.0.lock().map_err(|e| e.to_string())?.remove(id);
    Ok(())
  }
}

/// Unix seconds, formatted the same way `local_workspaces::chrono_like_now` does.
pub fn epoch_to_iso(secs: i64) -> String {
  let days = secs.div_euclid(86_400);
  let rem = secs.rem_euclid(86_400);
  let (y, m, d) = crate::local_workspaces::civil_from_days(days);
  format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, (rem % 3600) / 60, rem % 60)
}

/// Divides a big-endian byte buffer by 62 repeatedly to get its base62 digits, padded on the left
/// to `out_len`. 62^43 > 2^256, so 43 digits always hold a 32-byte number without truncation.
fn base62_fixed(bytes: &[u8], out_len: usize) -> String {
  let mut num = bytes.to_vec();
  let mut digits = Vec::with_capacity(out_len);
  while digits.len() < out_len {
    let mut remainder: u32 = 0;
    for byte in num.iter_mut() {
      let cur = (remainder << 8) | (*byte as u32);
      *byte = (cur / 62) as u8;
      remainder = cur % 62;
    }
    digits.push(ALPHABET[remainder as usize]);
  }
  digits.reverse();
  String::from_utf8(digits).expect("base62 alphabet is ASCII")
}

fn random_bytes<const N: usize>() -> Result<[u8; N], String> {
  let mut bytes = [0u8; N];
  getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
  Ok(bytes)
}

/// Uniform base62 characters via rejection sampling (256 = 4*62 + 8, so bytes >= 248 are redrawn).
fn random_base62(len: usize) -> Result<String, String> {
  let mut out = Vec::with_capacity(len);
  while out.len() < len {
    let [byte] = random_bytes::<1>()?;
    if byte < 248 {
      out.push(ALPHABET[(byte % 62) as usize]);
    }
  }
  Ok(String::from_utf8(out).expect("base62 alphabet is ASCII"))
}

pub fn generate_secret() -> Result<String, String> {
  let bytes = random_bytes::<32>()?;
  Ok(format!("{LOCAL_PREFIX}{}", base62_fixed(&bytes, SECRET_BODY_LEN)))
}

pub fn generate_id() -> Result<String, String> {
  Ok(format!("{ID_PREFIX}{}", random_base62(ID_BODY_LEN)?))
}

pub fn hash_secret(secret: &str) -> String {
  let mut hasher = Sha256::new();
  hasher.update(secret.as_bytes());
  hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// `tmgrl_` followed by exactly 43 alphanumeric characters.
fn is_well_formed(secret: &str) -> bool {
  secret
    .strip_prefix(LOCAL_PREFIX)
    .is_some_and(|body| body.len() == SECRET_BODY_LEN && body.bytes().all(|b| b.is_ascii_alphanumeric()))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredToken {
  pub id: String,
  pub prefix: String,
  pub sha256: String,
  pub persona_uuid: String,
  pub persona_name: String,
  pub workspace_id: i64,
  pub workspace_code: String,
  pub label: String,
  pub created_at: String,
  pub created_at_epoch: i64,
  pub expires_at: String,
  pub expires_at_epoch: i64,
  pub last_used_at: Option<String>,
  pub last_used_epoch: Option<i64>,
  pub revoked_at: Option<String>,
  pub plugin_id: Option<String>,
}

/// The shape sent over IPC: never the hash, never the internal epoch bookkeeping.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInfo {
  pub id: String,
  pub prefix: String,
  pub persona_uuid: String,
  pub persona_name: String,
  pub workspace_code: String,
  pub workspace_id: i64,
  pub label: String,
  pub created_at: String,
  pub expires_at: String,
  pub last_used_at: Option<String>,
  pub revoked_at: Option<String>,
  pub plugin_id: Option<String>,
}

impl From<&StoredToken> for TokenInfo {
  fn from(token: &StoredToken) -> Self {
    Self {
      id: token.id.clone(),
      prefix: token.prefix.clone(),
      persona_uuid: token.persona_uuid.clone(),
      persona_name: token.persona_name.clone(),
      workspace_code: token.workspace_code.clone(),
      workspace_id: token.workspace_id,
      label: token.label.clone(),
      created_at: token.created_at.clone(),
      expires_at: token.expires_at.clone(),
      last_used_at: token.last_used_at.clone(),
      revoked_at: token.revoked_at.clone(),
      plugin_id: token.plugin_id.clone(),
    }
  }
}

pub struct IssueParams {
  pub persona_uuid: String,
  pub persona_name: String,
  pub workspace_code: String,
  pub workspace_id: i64,
  pub label: String,
  pub expires_in_days: u32,
  pub plugin_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueError {
  BadLabel,
  BadExpiry,
  RateLimited,
  Internal(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenError {
  CloudToken,
  Invalid,
  Revoked,
  Expired,
}

impl TokenError {
  pub fn code(&self) -> &'static str {
    match self {
      TokenError::CloudToken => "CLOUD_TOKEN",
      TokenError::Invalid => "TOKEN_INVALID",
      TokenError::Revoked => "TOKEN_REVOKED",
      TokenError::Expired => "TOKEN_EXPIRED",
    }
  }

  pub fn message(&self) -> &'static str {
    match self {
      TokenError::CloudToken => "This is a cloud persona token; the local socket only accepts local tokens",
      TokenError::Invalid => "The token is not recognized",
      TokenError::Revoked => "The token has been revoked",
      TokenError::Expired => "The token has expired",
    }
  }
}

fn tokens_path(dir: &Path) -> PathBuf {
  dir.join("tokens.json")
}

fn load_tokens(dir: &Path) -> Result<Vec<StoredToken>, String> {
  let path = tokens_path(dir);
  if !path.exists() {
    return Ok(Vec::new());
  }
  let bytes = fs::read(&path).map_err(|e| e.to_string())?;
  if bytes.is_empty() {
    return Ok(Vec::new());
  }
  serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

#[cfg(unix)]
pub(crate) fn chmod(path: &Path, mode: u32) -> Result<(), String> {
  use std::os::unix::fs::PermissionsExt;
  fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|e| e.to_string())
}

#[cfg(not(unix))]
pub(crate) fn chmod(_path: &Path, _mode: u32) -> Result<(), String> {
  Ok(())
}

/// Holds every local token for the app: one `tokens.json` under the local-access dir, plus the
/// secret store (keychain in the app, in-memory in tests). Never keeps the secret itself in memory
/// longer than the call that generated or fetched it.
pub struct TokenStore {
  dir: PathBuf,
  secrets: Arc<dyn SecretStore>,
  tokens: Mutex<Vec<StoredToken>>,
}

impl TokenStore {
  pub fn open(dir: PathBuf, secrets: Arc<dyn SecretStore>) -> Result<Self, String> {
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    chmod(&dir, 0o700)?;
    let tokens = load_tokens(&dir)?;
    Ok(Self { dir, secrets, tokens: Mutex::new(tokens) })
  }

  fn persist(&self, tokens: &[StoredToken]) -> Result<(), String> {
    let path = tokens_path(&self.dir);
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(tokens).map_err(|e| e.to_string())?;
    fs::write(&tmp, &json).map_err(|e| e.to_string())?;
    chmod(&tmp, 0o600)?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
  }

  pub fn list(&self, workspace_code: Option<&str>) -> Result<Vec<TokenInfo>, String> {
    let tokens = self.tokens.lock().map_err(|e| e.to_string())?;
    let mut out: Vec<TokenInfo> = tokens
      .iter()
      .filter(|t| workspace_code.is_none_or(|code| t.workspace_code == code))
      .map(TokenInfo::from)
      .collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
  }

  pub fn issue(&self, params: IssueParams, now_epoch: i64) -> Result<(TokenInfo, String), IssueError> {
    if params.label.is_empty() || params.label.chars().count() > 60 {
      return Err(IssueError::BadLabel);
    }
    if !(1..=365).contains(&params.expires_in_days) {
      return Err(IssueError::BadExpiry);
    }
    let mut tokens = self.tokens.lock().map_err(|e| IssueError::Internal(e.to_string()))?;
    let recent = tokens.iter().filter(|t| now_epoch - t.created_at_epoch < HOUR_SECS).count();
    if recent >= ISSUE_LIMIT_PER_HOUR {
      return Err(IssueError::RateLimited);
    }
    let secret = generate_secret().map_err(IssueError::Internal)?;
    let id = generate_id().map_err(IssueError::Internal)?;
    let prefix: String = secret.chars().take(PREFIX_SHOW_LEN).collect();
    let expires_at_epoch = now_epoch + params.expires_in_days as i64 * 86_400;
    let stored = StoredToken {
      id: id.clone(),
      prefix,
      sha256: hash_secret(&secret),
      persona_uuid: params.persona_uuid,
      persona_name: params.persona_name,
      workspace_id: params.workspace_id,
      workspace_code: params.workspace_code,
      label: params.label,
      created_at: epoch_to_iso(now_epoch),
      created_at_epoch: now_epoch,
      expires_at: epoch_to_iso(expires_at_epoch),
      expires_at_epoch,
      last_used_at: None,
      last_used_epoch: None,
      revoked_at: None,
      plugin_id: params.plugin_id,
    };
    self.secrets.set(&id, &secret).map_err(IssueError::Internal)?;
    tokens.push(stored.clone());
    if let Err(e) = self.persist(&tokens) {
      tokens.pop();
      let _ = self.secrets.delete(&id);
      return Err(IssueError::Internal(e));
    }
    Ok((TokenInfo::from(&stored), secret))
  }

  pub fn revoke(&self, id: &str, now_epoch: i64) -> Result<bool, String> {
    let mut tokens = self.tokens.lock().map_err(|e| e.to_string())?;
    let Some(token) = tokens.iter_mut().find(|t| t.id == id) else { return Ok(false) };
    if token.revoked_at.is_some() {
      return Ok(false);
    }
    token.revoked_at = Some(epoch_to_iso(now_epoch));
    self.persist(&tokens)?;
    drop(tokens);
    self.delete_secret(id);
    Ok(true)
  }

  /// At least one filter is required so this can never turn into "revoke every token".
  pub fn revoke_all(
    &self,
    persona_uuid: Option<&str>,
    plugin_id: Option<&str>,
    workspace_code: Option<&str>,
    now_epoch: i64,
  ) -> Result<usize, String> {
    if persona_uuid.is_none() && plugin_id.is_none() && workspace_code.is_none() {
      return Err("at least one filter is required".into());
    }
    let mut tokens = self.tokens.lock().map_err(|e| e.to_string())?;
    let mut revoked_ids = Vec::new();
    for token in tokens.iter_mut() {
      if token.revoked_at.is_some() {
        continue;
      }
      if persona_uuid.is_some_and(|uuid| token.persona_uuid != uuid) {
        continue;
      }
      if plugin_id.is_some_and(|id| token.plugin_id.as_deref() != Some(id)) {
        continue;
      }
      if workspace_code.is_some_and(|code| token.workspace_code != code) {
        continue;
      }
      token.revoked_at = Some(epoch_to_iso(now_epoch));
      revoked_ids.push(token.id.clone());
    }
    if !revoked_ids.is_empty() {
      self.persist(&tokens)?;
      drop(tokens);
      for id in &revoked_ids {
        self.delete_secret(id);
      }
    }
    Ok(revoked_ids.len())
  }

  fn delete_secret(&self, id: &str) {
    if let Err(error) = self.secrets.delete(id) {
      log::warn!("[local-access] could not delete the stored secret of token {id}: {error}");
    }
  }

  pub fn copy_secret(&self, id: &str, now_epoch: i64) -> Result<Option<String>, String> {
    {
      let tokens = self.tokens.lock().map_err(|e| e.to_string())?;
      match tokens.iter().find(|t| t.id == id) {
        Some(token) if token.revoked_at.is_none() && token.expires_at_epoch > now_epoch => {}
        _ => return Ok(None),
      }
    }
    self.secrets.get(id)
  }

  /// Checked in this order: cloud prefix, format/unknown hash, revoked, expired. Workspace
  /// existence, rate limiting and readiness are the caller's job (they need app state this store
  /// doesn't have). On success, `last_used_at` is bumped and persisted at most once a minute.
  pub fn verify(&self, secret: &str, now_epoch: i64) -> Result<StoredToken, TokenError> {
    if secret.starts_with(CLOUD_PREFIX) {
      return Err(TokenError::CloudToken);
    }
    if !is_well_formed(secret) {
      return Err(TokenError::Invalid);
    }
    let hash = hash_secret(secret);
    let mut tokens = self.tokens.lock().map_err(|_| TokenError::Invalid)?;
    let Some(idx) = tokens.iter().position(|t| t.sha256 == hash) else { return Err(TokenError::Invalid) };
    if tokens[idx].revoked_at.is_some() {
      return Err(TokenError::Revoked);
    }
    if tokens[idx].expires_at_epoch <= now_epoch {
      return Err(TokenError::Expired);
    }
    if tokens[idx].last_used_epoch.is_none_or(|last| now_epoch - last >= LAST_USED_THROTTLE_SECS) {
      tokens[idx].last_used_epoch = Some(now_epoch);
      tokens[idx].last_used_at = Some(epoch_to_iso(now_epoch));
      // Best-effort: a slow disk should not fail a request that already authenticated.
      let _ = self.persist(&tokens);
    }
    Ok(tokens[idx].clone())
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MigrateStatus {
  None,
  Moved,
  Already,
  Conflict,
}

#[derive(Debug, Serialize)]
pub struct LlmMigrateOut {
  pub status: MigrateStatus,
}

pub fn validate_user_id(id: i64) -> Result<i64, String> {
  if id > 0 && id <= 9_007_199_254_740_991 {
    Ok(id)
  } else {
    Err("invalid user id".into())
  }
}

pub fn migrate_secret<S: SecretStore>(store: &S, src: &str, dst: &str) -> Result<MigrateStatus, String> {
  let Some(secret) = store.get(src).map_err(|_| "could not read the saved key")? else {
    return Ok(MigrateStatus::None);
  };
  match store.get(dst).map_err(|_| "could not read the destination key")? {
    Some(existing) if existing == secret => {
      store.delete(src).map_err(|_| "could not remove the old key")?;
      Ok(MigrateStatus::Already)
    }
    Some(_) => Ok(MigrateStatus::Conflict),
    None => {
      let written = store.set(dst, &secret).is_ok() && store.get(dst).ok().flatten().as_deref() == Some(secret.as_str());
      if !written {
        let _ = store.delete(dst);
        return Err("could not write the key to the new account".into());
      }
      store.delete(src).map_err(|_| "could not remove the old key")?;
      Ok(MigrateStatus::Moved)
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tmgr-local-access-test-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
  }

  fn store(name: &str) -> TokenStore {
    TokenStore::open(temp_dir(name), Arc::new(MemorySecrets::new())).unwrap()
  }

  fn issue_params(workspace_code: &str) -> IssueParams {
    IssueParams {
      persona_uuid: "p-1".into(),
      persona_name: "Reviewer".into(),
      workspace_code: workspace_code.into(),
      workspace_id: -1,
      label: "My laptop".into(),
      expires_in_days: 90,
      plugin_id: None,
    }
  }

  #[test]
  fn secret_matches_the_contract_regex() {
    let secret = generate_secret().unwrap();
    assert!(secret.starts_with("tmgrl_"));
    let body = &secret["tmgrl_".len()..];
    assert_eq!(body.len(), 43);
    assert!(body.bytes().all(|b| b.is_ascii_alphanumeric()));
  }

  #[test]
  fn base62_fixed_pads_all_zero_and_all_ff_to_the_same_length() {
    assert_eq!(base62_fixed(&[0u8; 32], 43).len(), 43);
    assert_eq!(base62_fixed(&[0xffu8; 32], 43).len(), 43);
    assert_eq!(base62_fixed(&[0u8; 32], 43), "0".repeat(43));
  }

  #[test]
  fn id_matches_the_contract_shape() {
    let id = generate_id().unwrap();
    assert!(id.starts_with("lt_"));
    assert_eq!(id.len(), "lt_".len() + 16);
  }

  #[test]
  fn hashing_is_deterministic_and_secret_specific() {
    let a = hash_secret("tmgrl_aaaa");
    let b = hash_secret("tmgrl_aaaa");
    let c = hash_secret("tmgrl_bbbb");
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_eq!(a.len(), 64);
  }

  #[test]
  fn verify_rejects_a_cloud_token_before_looking_at_storage() {
    let store = store("cloud");
    let err = store.verify("tmgrp_whatever", 0).unwrap_err();
    assert_eq!(err, TokenError::CloudToken);
  }

  #[test]
  fn verify_rejects_bad_format_and_unknown_secrets() {
    let store = store("invalid");
    assert_eq!(store.verify("not-a-token", 0).unwrap_err(), TokenError::Invalid);
    let unknown = generate_secret().unwrap();
    assert_eq!(store.verify(&unknown, 0).unwrap_err(), TokenError::Invalid);
  }

  #[test]
  fn verify_rejects_a_revoked_token() {
    let store = store("revoked");
    let (info, secret) = store.issue(issue_params("local-personal"), 1000).unwrap();
    store.revoke(&info.id, 2000).unwrap();
    assert_eq!(store.verify(&secret, 3000).unwrap_err(), TokenError::Revoked);
  }

  #[test]
  fn verify_rejects_an_expired_token() {
    let store = store("expired");
    let (_, secret) = store.issue(issue_params("local-personal"), 1000).unwrap();
    let expires_at = 1000 + 90 * 86_400;
    assert!(store.verify(&secret, expires_at - 1).is_ok());
    assert_eq!(store.verify(&secret, expires_at).unwrap_err(), TokenError::Expired);
  }

  #[test]
  fn verify_updates_last_used_at_at_most_once_a_minute() {
    let store = store("throttle");
    let (_, secret) = store.issue(issue_params("local-personal"), 1000).unwrap();
    let first = store.verify(&secret, 1005).unwrap();
    assert_eq!(first.last_used_epoch, Some(1005));
    let still_throttled = store.verify(&secret, 1010).unwrap();
    assert_eq!(still_throttled.last_used_epoch, Some(1005));
    let bumped = store.verify(&secret, 1005 + 60).unwrap();
    assert_eq!(bumped.last_used_epoch, Some(1005 + 60));
  }

  #[test]
  fn issue_rejects_bad_label_and_expiry() {
    let store = store("bad-params");
    let mut params = issue_params("local-personal");
    params.label = "".into();
    assert_eq!(store.issue(params, 0).unwrap_err(), IssueError::BadLabel);
    let mut params = issue_params("local-personal");
    params.label = "x".repeat(61);
    assert_eq!(store.issue(params, 0).unwrap_err(), IssueError::BadLabel);
    let mut params = issue_params("local-personal");
    params.expires_in_days = 0;
    assert_eq!(store.issue(params, 0).unwrap_err(), IssueError::BadExpiry);
    let mut params = issue_params("local-personal");
    params.expires_in_days = 366;
    assert_eq!(store.issue(params, 0).unwrap_err(), IssueError::BadExpiry);
  }

  #[test]
  fn issue_is_limited_to_ten_per_rolling_hour() {
    let store = store("rate-limit");
    for i in 0..ISSUE_LIMIT_PER_HOUR {
      store.issue(issue_params("local-personal"), i as i64).unwrap();
    }
    assert_eq!(store.issue(issue_params("local-personal"), 100).unwrap_err(), IssueError::RateLimited);
    // An hour later the earliest issues have rolled out of the window.
    assert!(store.issue(issue_params("local-personal"), HOUR_SECS + 1).is_ok());
  }

  #[test]
  fn revoke_all_requires_a_filter() {
    let store = store("revoke-all-filter");
    assert!(store.revoke_all(None, None, None, 0).is_err());
  }

  #[test]
  fn revoke_all_filters_by_persona_plugin_and_workspace() {
    let store = store("revoke-all");
    let mut a = issue_params("ws-a");
    a.persona_uuid = "persona-a".into();
    let (a, _) = store.issue(a, 0).unwrap();
    let mut b = issue_params("ws-b");
    b.persona_uuid = "persona-a".into();
    let (b, _) = store.issue(b, 0).unwrap();
    let mut c = issue_params("ws-a");
    c.persona_uuid = "persona-c".into();
    c.plugin_id = Some("plugin-x".into());
    let (c, _) = store.issue(c, 0).unwrap();

    let revoked = store.revoke_all(Some("persona-a"), None, Some("ws-a"), 10).unwrap();
    assert_eq!(revoked, 1);
    let tokens = store.list(None).unwrap();
    let by_id = |id: &str| tokens.iter().find(|t| t.id == id).unwrap();
    assert!(by_id(&a.id).revoked_at.is_some());
    assert!(by_id(&b.id).revoked_at.is_none());
    assert!(by_id(&c.id).revoked_at.is_none());

    let revoked = store.revoke_all(None, Some("plugin-x"), None, 20).unwrap();
    assert_eq!(revoked, 1);
    let tokens = store.list(None).unwrap();
    assert!(tokens.iter().find(|t| t.id == c.id).unwrap().revoked_at.is_some());
  }

  #[test]
  fn tokens_file_and_dir_are_private() {
    let dir = temp_dir("perms");
    let store = TokenStore::open(dir.clone(), Arc::new(MemorySecrets::new())).unwrap();
    store.issue(issue_params("local-personal"), 0).unwrap();
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      let dir_mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
      assert_eq!(dir_mode, 0o700);
      let file_mode = fs::metadata(tokens_path(&dir)).unwrap().permissions().mode() & 0o777;
      assert_eq!(file_mode, 0o600);
    }
  }

  #[test]
  fn list_never_includes_the_hash_and_sorts_newest_first() {
    let store = store("list");
    store.issue(issue_params("local-personal"), 100).unwrap();
    let (newest, _) = store.issue(issue_params("local-personal"), 200).unwrap();
    let listed = store.list(None).unwrap();
    assert_eq!(listed[0].id, newest.id);
    let json = serde_json::to_string(&listed[0]).unwrap();
    assert!(!json.contains("sha256"));
  }

  #[test]
  fn issued_secret_is_never_returned_again() {
    let store = store("secret-once");
    let (info, secret) = store.issue(issue_params("local-personal"), 0).unwrap();
    assert_eq!(store.copy_secret(&info.id, 0).unwrap(), Some(secret.clone()));
    let listed = store.list(None).unwrap();
    let json = serde_json::to_string(&listed[0]).unwrap();
    assert!(!json.contains(&secret));
    assert!(json.contains(&listed[0].prefix));
  }

  struct FailingDelete(MemorySecrets);

  impl SecretStore for FailingDelete {
    fn set(&self, id: &str, secret: &str) -> Result<(), String> {
      self.0.set(id, secret)
    }
    fn get(&self, id: &str) -> Result<Option<String>, String> {
      self.0.get(id)
    }
    fn delete(&self, _id: &str) -> Result<(), String> {
      Err("keychain unavailable".into())
    }
  }

  fn store_with_secrets(name: &str, secrets: Arc<dyn SecretStore>) -> TokenStore {
    TokenStore::open(temp_dir(name), secrets).unwrap()
  }

  #[test]
  fn revoke_removes_the_stored_secret() {
    let secrets = Arc::new(MemorySecrets::new());
    let store = store_with_secrets("revoke-deletes", secrets.clone());
    let (info, _) = store.issue(issue_params("local-personal"), 0).unwrap();
    assert!(secrets.get(&info.id).unwrap().is_some());
    assert!(store.revoke(&info.id, 10).unwrap());
    assert!(secrets.get(&info.id).unwrap().is_none());
  }

  #[test]
  fn revoke_all_removes_the_stored_secrets_of_the_revoked_tokens_only() {
    let secrets = Arc::new(MemorySecrets::new());
    let store = store_with_secrets("revoke-all-deletes", secrets.clone());
    let (a, _) = store.issue(issue_params("ws-a"), 0).unwrap();
    let (b, _) = store.issue(issue_params("ws-b"), 0).unwrap();
    assert_eq!(store.revoke_all(None, None, Some("ws-a"), 10).unwrap(), 1);
    assert!(secrets.get(&a.id).unwrap().is_none());
    assert!(secrets.get(&b.id).unwrap().is_some());
  }

  #[test]
  fn revocation_succeeds_when_the_secret_cannot_be_deleted() {
    let store = store_with_secrets("revoke-delete-fails", Arc::new(FailingDelete(MemorySecrets::new())));
    let (a, secret) = store.issue(issue_params("ws-a"), 0).unwrap();
    let (b, _) = store.issue(issue_params("ws-b"), 0).unwrap();
    assert!(store.revoke(&a.id, 10).unwrap());
    assert_eq!(store.verify(&secret, 20).unwrap_err(), TokenError::Revoked);
    assert_eq!(store.revoke_all(None, None, Some("ws-b"), 10).unwrap(), 1);
    assert!(store.copy_secret(&b.id, 20).unwrap().is_none());
  }

  #[test]
  fn copy_secret_only_returns_secrets_of_live_tokens() {
    let secrets = Arc::new(MemorySecrets::new());
    let store = store_with_secrets("copy-live-only", secrets.clone());
    let (live, live_secret) = store.issue(issue_params("ws-a"), 1000).unwrap();
    let expires_at = 1000 + 90 * 86_400;
    assert!(store.copy_secret(&live.id, 2000).unwrap().is_some_and(|secret| secret == live_secret));
    assert!(store.copy_secret(&live.id, expires_at).unwrap().is_none());
    assert!(store.copy_secret("lt_missing", 2000).unwrap().is_none());

    let (revoked, _) = store.issue(issue_params("ws-b"), 1000).unwrap();
    store.revoke(&revoked.id, 1500).unwrap();
    secrets.set(&revoked.id, "leftover").unwrap();
    assert!(store.copy_secret(&revoked.id, 2000).unwrap().is_none());
  }
}

#[cfg(test)]
mod migrate_tests {
  use super::*;
  use std::sync::atomic::{AtomicBool, Ordering::SeqCst};

  #[derive(Default)]
  struct Flaky {
    inner: MemorySecrets,
    fail_get_dst: AtomicBool,
    fail_set: AtomicBool,
    drop_set: AtomicBool,
    garble_set: AtomicBool,
    fail_delete_src: AtomicBool,
  }

  impl SecretStore for Flaky {
    fn set(&self, id: &str, secret: &str) -> Result<(), String> {
      if self.fail_set.load(SeqCst) {
        return Err("set failed".into());
      }
      if self.drop_set.load(SeqCst) {
        return Ok(());
      }
      if self.garble_set.load(SeqCst) {
        return self.inner.set(id, "garbled");
      }
      self.inner.set(id, secret)
    }

    fn get(&self, id: &str) -> Result<Option<String>, String> {
      if id == DST && self.fail_get_dst.load(SeqCst) {
        return Err("get failed".into());
      }
      self.inner.get(id)
    }

    fn delete(&self, id: &str) -> Result<(), String> {
      if id == SRC && self.fail_delete_src.load(SeqCst) {
        return Err("delete failed".into());
      }
      self.inner.delete(id)
    }
  }

  const SRC: &str = "api-key:-1";
  const DST: &str = "api-key:7";
  const KEY: &str = "sk-secret-value";

  fn run(s: &Flaky) -> Result<MigrateStatus, String> {
    migrate_secret(s, SRC, DST)
  }

  fn with_src() -> Flaky {
    let s = Flaky::default();
    s.inner.set(SRC, KEY).unwrap();
    s
  }

  #[test]
  fn none_when_source_is_empty() {
    let s = Flaky::default();
    assert_eq!(run(&s).unwrap(), MigrateStatus::None);
    assert_eq!(s.inner.get(DST).unwrap(), None);
  }

  #[test]
  fn moves_when_destination_is_empty() {
    let s = with_src();
    assert_eq!(run(&s).unwrap(), MigrateStatus::Moved);
    assert_eq!(s.inner.get(SRC).unwrap(), None);
    assert_eq!(s.inner.get(DST).unwrap().as_deref(), Some(KEY));
    assert_eq!(run(&s).unwrap(), MigrateStatus::None);
  }

  #[test]
  fn already_when_destination_equals_source_and_removes_source() {
    let s = with_src();
    s.inner.set(DST, KEY).unwrap();
    assert_eq!(run(&s).unwrap(), MigrateStatus::Already);
    assert_eq!(s.inner.get(SRC).unwrap(), None);
    assert_eq!(s.inner.get(DST).unwrap().as_deref(), Some(KEY));
  }

  #[test]
  fn conflict_changes_nothing() {
    let s = with_src();
    s.inner.set(DST, "other").unwrap();
    assert_eq!(run(&s).unwrap(), MigrateStatus::Conflict);
    assert_eq!(s.inner.get(SRC).unwrap().as_deref(), Some(KEY));
    assert_eq!(s.inner.get(DST).unwrap().as_deref(), Some("other"));
  }

  #[test]
  fn destination_read_error_changes_nothing_and_rerun_moves() {
    let s = with_src();
    s.fail_get_dst.store(true, SeqCst);
    assert!(run(&s).is_err());
    assert_eq!(s.inner.get(SRC).unwrap().as_deref(), Some(KEY));
    assert_eq!(s.inner.get(DST).unwrap(), None);
    s.fail_get_dst.store(false, SeqCst);
    assert_eq!(run(&s).unwrap(), MigrateStatus::Moved);
  }

  #[test]
  fn failed_write_keeps_source_and_rerun_moves() {
    let s = with_src();
    s.fail_set.store(true, SeqCst);
    let err = run(&s).unwrap_err();
    assert!(!err.contains(KEY));
    assert_eq!(s.inner.get(SRC).unwrap().as_deref(), Some(KEY));
    assert_eq!(s.inner.get(DST).unwrap(), None);
    s.fail_set.store(false, SeqCst);
    assert_eq!(run(&s).unwrap(), MigrateStatus::Moved);
    assert_eq!(s.inner.get(DST).unwrap().as_deref(), Some(KEY));
  }

  #[test]
  fn silently_dropped_write_is_an_error_and_keeps_source() {
    let s = with_src();
    s.drop_set.store(true, SeqCst);
    assert!(run(&s).is_err());
    assert_eq!(s.inner.get(SRC).unwrap().as_deref(), Some(KEY));
    assert_eq!(s.inner.get(DST).unwrap(), None);
    s.drop_set.store(false, SeqCst);
    assert_eq!(run(&s).unwrap(), MigrateStatus::Moved);
  }

  #[test]
  fn read_back_mismatch_removes_destination_keeps_source() {
    let s = with_src();
    s.garble_set.store(true, SeqCst);
    let err = run(&s).unwrap_err();
    assert!(!err.contains(KEY) && !err.contains("garbled"));
    assert_eq!(s.inner.get(SRC).unwrap().as_deref(), Some(KEY));
    assert_eq!(s.inner.get(DST).unwrap(), None);
    s.garble_set.store(false, SeqCst);
    assert_eq!(run(&s).unwrap(), MigrateStatus::Moved);
  }

  #[test]
  fn failed_source_delete_keeps_both_and_rerun_is_already() {
    let s = with_src();
    s.fail_delete_src.store(true, SeqCst);
    let err = run(&s).unwrap_err();
    assert!(!err.contains(KEY));
    assert_eq!(s.inner.get(SRC).unwrap().as_deref(), Some(KEY));
    assert_eq!(s.inner.get(DST).unwrap().as_deref(), Some(KEY));
    assert_eq!(run(&s).unwrap_err(), err);
    s.fail_delete_src.store(false, SeqCst);
    assert_eq!(run(&s).unwrap(), MigrateStatus::Already);
    assert_eq!(s.inner.get(SRC).unwrap(), None);
  }

  #[test]
  fn user_id_bounds() {
    assert!(validate_user_id(1).is_ok());
    assert!(validate_user_id(9_007_199_254_740_991).is_ok());
    for bad in [0, -1, i64::MIN, 9_007_199_254_740_992, i64::MAX] {
      assert!(validate_user_id(bad).is_err(), "{bad}");
    }
  }

  #[test]
  fn status_serialises_lowercase() {
    let json = |s| serde_json::to_string(&s).unwrap();
    assert_eq!(json(MigrateStatus::None), "\"none\"");
    assert_eq!(json(MigrateStatus::Moved), "\"moved\"");
    assert_eq!(json(MigrateStatus::Already), "\"already\"");
    assert_eq!(json(MigrateStatus::Conflict), "\"conflict\"");
  }

  #[test]
  fn serialised_result_and_errors_never_contain_the_key() {
    let mut seen = Vec::new();
    for case in 0..6 {
      let s = with_src();
      match case {
        1 => s.inner.set(DST, KEY).unwrap(),
        2 => s.inner.set(DST, "other").unwrap(),
        3 => s.fail_set.store(true, SeqCst),
        4 => s.garble_set.store(true, SeqCst),
        5 => s.fail_delete_src.store(true, SeqCst),
        _ => {}
      }
      seen.push(match run(&s) {
        Ok(status) => serde_json::to_string(&LlmMigrateOut { status }).unwrap(),
        Err(e) => e,
      });
    }
    assert_eq!(seen[0], r#"{"status":"moved"}"#);
    assert_eq!(seen[2], r#"{"status":"conflict"}"#);
    for out in &seen {
      assert!(!out.contains(KEY) && !out.contains("other") && !out.contains("garbled"), "{out}");
    }
  }
}
