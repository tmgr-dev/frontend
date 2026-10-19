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

    pub fn update<T>(f: impl FnOnce(&mut Self) -> Result<T, String>) -> Result<T, String> {
        let path = allow_path().ok_or("cannot locate the config directory")?;
        Self::update_at(&path, f)
    }

    pub fn update_at<T>(
        path: &Path,
        f: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut lock_name = path.as_os_str().to_owned();
        lock_name.push(".lock");
        let _lock = fsio::Lock::exclusive(Path::new(&lock_name))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let mut file = Self::load_from(path)?;
        let out = f(&mut file)?;
        file.save_to(path)?;
        Ok(out)
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
    #[serde(default)]
    pub hashes: BTreeMap<String, String>,
}

impl Default for Rollback {
    fn default() -> Self {
        Self {
            version: 1,
            tasks: BTreeMap::new(),
            hashes: BTreeMap::new(),
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

    pub fn seen(&self, key: &str) -> Option<(i64, Option<&str>)> {
        let at = self.tasks.get(key).copied()?;
        Some((at, self.hashes.get(key).map(String::as_str)))
    }

    pub fn record(&mut self, key: String, signed_at: i64, content_sha256: &str) {
        let newer = self.tasks.get(&key).is_none_or(|cur| signed_at > *cur);
        let unhashed = self.tasks.get(&key) == Some(&signed_at) && !self.hashes.contains_key(&key);
        if newer || unhashed {
            self.tasks.insert(key.clone(), signed_at);
            self.hashes.insert(key, content_sha256.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    fn entry(ws: i64) -> Entry {
        Entry {
            server: "s".into(),
            user_id: 1,
            workspace_id: ws,
            workspace_code: format!("w{ws}"),
            allowed_at: "2025-10-01T00:00:00Z".into(),
            expires_at: None,
            keys: vec![],
            repos: BTreeMap::new(),
        }
    }

    #[test]
    fn concurrent_updates_do_not_overwrite_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("allow.json");
        AllowFile::update_at(&path, |f| {
            f.upsert(entry(1));
            f.upsert(entry(2));
            Ok(())
        })
        .unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let (p1, b1) = (path.clone(), barrier.clone());
        let slow = std::thread::spawn(move || {
            AllowFile::update_at(&p1, |f| {
                b1.wait();
                std::thread::sleep(std::time::Duration::from_millis(300));
                f.upsert(entry(3));
                Ok(())
            })
        });
        barrier.wait();
        AllowFile::update_at(&path, |f| {
            assert!(f.remove("s", 1, 1), "must see the first writer's result");
            Ok(())
        })
        .unwrap();
        slow.join().unwrap().unwrap();
        let ids: Vec<i64> = AllowFile::load_from(&path)
            .unwrap()
            .entries
            .iter()
            .map(|e| e.workspace_id)
            .collect();
        assert_eq!(ids, [2, 3]);
    }

    #[test]
    fn an_equal_stamp_with_another_hash_is_remembered_as_the_first_one() {
        let mut r = Rollback::default();
        r.record("k".into(), 10, "aaa");
        assert_eq!(r.seen("k"), Some((10, Some("aaa"))));
        r.record("k".into(), 10, "bbb");
        assert_eq!(r.seen("k"), Some((10, Some("aaa"))));
        r.record("k".into(), 9, "ccc");
        assert_eq!(r.seen("k"), Some((10, Some("aaa"))));
        r.record("k".into(), 11, "ddd");
        assert_eq!(r.seen("k"), Some((11, Some("ddd"))));
    }
}
