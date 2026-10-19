use crate::crypto::{content_sha256, message, verify};
use crate::fsio;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tmgr_ipc::api::CloudApproval;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub message: String,
    pub fix: String,
}

impl Refusal {
    pub fn new(message: impl Into<String>, fix: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            fix: fix.into(),
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.fix)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedKey {
    pub key_id: String,
    pub public_key: String,
    #[serde(default)]
    pub device_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub server: String,
    pub user_id: i64,
    pub workspace_id: i64,
    pub workspace_code: String,
    pub allowed_at: String,
    pub expires_at: Option<String>,
    pub keys: Vec<PinnedKey>,
    #[serde(default)]
    pub repos: BTreeMap<String, String>,
}

impl Entry {
    pub fn expiry(&self) -> Option<DateTime<Utc>> {
        self.expires_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&Utc))
    }

    pub fn expired(&self, now: DateTime<Utc>) -> bool {
        match (&self.expires_at, self.expiry()) {
            (None, _) => false,
            (Some(_), None) => true,
            (Some(_), Some(at)) => at <= now,
        }
    }

    pub fn allow_command(&self) -> String {
        format!("tmgr runner allow {}", self.workspace_code)
    }

    pub fn key(&self, key_id: &str) -> Option<&PinnedKey> {
        self.keys.iter().find(|k| k.key_id == key_id)
    }

    pub fn repo(&self, category_id: i64) -> Option<&str> {
        self.repos.get(&category_id.to_string()).map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowFile {
    pub version: u32,
    pub entries: Vec<Entry>,
}

impl Default for AllowFile {
    fn default() -> Self {
        Self {
            version: 1,
            entries: Vec::new(),
        }
    }
}

pub fn allow_path() -> Option<PathBuf> {
    fsio::config_dir().map(|d| d.join("runner-allow.json"))
}

pub fn rollback_path() -> Option<PathBuf> {
    fsio::state_dir().map(|d| d.join("cloud-approvals.json"))
}

impl AllowFile {
    pub fn load() -> Result<Self, String> {
        let path = allow_path().ok_or("cannot locate the config directory")?;
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Result<Self, String> {
        match fsio::read_opt(path).map_err(|e| format!("{}: {e}", path.display()))? {
            None => Ok(Self::default()),
            Some(text) => {
                let f: Self = serde_json::from_str(&text)
                    .map_err(|e| format!("{} is not valid: {e}", path.display()))?;
                if f.version != 1 {
                    return Err(format!(
                        "{} has version {}, this tmgr reads version 1",
                        path.display(),
                        f.version
                    ));
                }
                Ok(f)
            }
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = allow_path().ok_or("cannot locate the config directory")?;
        self.save_to(&path)
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        let body = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        fsio::write_private(path, &body).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn find(&self, server: &str, user_id: i64, workspace_id: i64) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.server == server && e.user_id == user_id && e.workspace_id == workspace_id)
    }

    pub fn find_live(
        &self,
        server: &str,
        user_id: i64,
        workspace_id: i64,
        workspace_code: &str,
        now: DateTime<Utc>,
    ) -> Result<&Entry, Refusal> {
        let fix = format!("tmgr runner allow {workspace_code}");
        let Some(e) = self.find(server, user_id, workspace_id) else {
            return Err(Refusal::new(
                format!("workspace {workspace_code} is not allowed to run agents on this machine"),
                fix,
            ));
        };
        if e.expired(now) {
            return Err(Refusal::new(
                format!(
                    "the permission for workspace {} expired on {}",
                    e.workspace_code,
                    e.expires_at.as_deref().unwrap_or("an unreadable date")
                ),
                e.allow_command(),
            ));
        }
        Ok(e)
    }

    pub fn upsert(&mut self, entry: Entry) {
        self.entries.retain(|e| {
            !(e.server == entry.server
                && e.user_id == entry.user_id
                && e.workspace_id == entry.workspace_id)
        });
        self.entries.push(entry);
    }

    pub fn remove(&mut self, server: &str, user_id: i64, workspace_id: i64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| {
            !(e.server == server && e.user_id == user_id && e.workspace_id == workspace_id)
        });
        self.entries.len() != before
    }
}

pub fn check_claim(
    entry: &Entry,
    claim: &CloudApproval,
    title: &str,
    body: &str,
) -> Result<(), Refusal> {
    let code = &entry.workspace_code;
    let allow = entry.allow_command();
    if claim.kind != "author" && claim.kind != "confirm" {
        return Err(Refusal::new("the approval has an unknown kind", &allow));
    }
    if claim.host != entry.server
        || claim.user_id != entry.user_id
        || claim.workspace_id != entry.workspace_id
    {
        return Err(Refusal::new(
            format!("the approval was not made for this account and server in workspace {code}"),
            &allow,
        ));
    }
    if content_sha256(title, body) != claim.content_sha256 {
        return Err(Refusal::new(
            "the task changed since it was approved",
            "open the task in the web app and click Confirm for agent runs",
        ));
    }
    let Some(key) = entry.key(&claim.key_id) else {
        return Err(Refusal::new(
            "the approval is signed by a key this machine does not trust",
            format!("{allow} to re-pin your keys"),
        ));
    };
    let msg = message(
        &claim.host,
        claim.user_id,
        claim.workspace_id,
        claim.task_id,
        claim.category_id,
        &claim.content_sha256,
        &claim.key_id,
        claim.signed_at,
    );
    if !verify(&key.public_key, &msg, &claim.signature) {
        return Err(Refusal::new(
            "the approval signature is not valid",
            "open the task in the web app and click Confirm for agent runs",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rollback {
    pub version: u32,
    pub tasks: BTreeMap<String, i64>,
}

impl Default for Rollback {
    fn default() -> Self {
        Self {
            version: 1,
            tasks: BTreeMap::new(),
        }
    }
}

impl Rollback {
    pub fn key(server: &str, user_id: i64, workspace_id: i64, task_id: i64) -> String {
        format!("{server}|{user_id}|{workspace_id}|{task_id}")
    }

    pub fn load() -> Result<Self, String> {
        let path = rollback_path().ok_or("cannot locate the state directory")?;
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Result<Self, String> {
        match fsio::read_opt(path).map_err(|e| format!("{}: {e}", path.display()))? {
            None => Ok(Self::default()),
            Some(text) => serde_json::from_str(&text)
                .map_err(|e| format!("{} is not valid: {e}", path.display())),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = rollback_path().ok_or("cannot locate the state directory")?;
        self.save_to(&path)
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        let body = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        fsio::write_private(path, &body).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn seen(&self, key: &str) -> Option<i64> {
        self.tasks.get(key).copied()
    }

    pub fn record(&mut self, key: String, signed_at: i64) {
        let e = self.tasks.entry(key).or_insert(signed_at);
        *e = (*e).max(signed_at);
    }
}
