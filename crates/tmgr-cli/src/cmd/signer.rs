use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use p256::ecdsa::signature::Signer as _;
use p256::ecdsa::{Signature, SigningKey};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tmgr_core::api::{map_error, unwrap_data, Client};
use tmgr_core::approval::{
    approval_message, content_sha256, context_sha256, page_sha256, persona_sha256, repos_sha256,
    Payload,
};
#[cfg(debug_assertions)]
use tmgr_core::creds::KeyringError;
use tmgr_core::creds::{classify, Failure, Keychain, OsKeychain, STORE_ENV};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_link::privileged;

const SCOPE: &str = "agent-approvals:write";
const CHUNK: usize = 100;
const KIND: &str = "confirm";

pub fn host_of(server: &str) -> Result<String> {
    let url = tmgr_core::Url::parse(server)
        .map_err(|_| Error::usage("the server address is not a valid URL"))?;
    let host = url
        .host_str()
        .ok_or_else(|| Error::usage("the server address has no host"))?
        .to_ascii_lowercase();
    Ok(match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host,
    })
}

pub fn account(host: &str, user_id: i64) -> String {
    format!("approval-key:{host}:{user_id}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnKey {
    pub key_id: String,
    pub public_key: String,
}

#[derive(Debug, Clone)]
pub struct SignTask {
    pub id: i64,
    pub workspace_id: i64,
    pub category_id: i64,
    pub title: String,
    pub description: String,
    pub repos_sha256: Option<String>,
    pub offer: Offer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferPersona {
    pub id: String,
    pub name: String,
    pub instructions: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferPage {
    pub id: i64,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Offer {
    pub repositories: Option<Vec<String>>,
    pub persona: Option<OfferPersona>,
    pub context: Option<Vec<OfferPage>>,
}

const MAX_SAFE_ID: i64 = 9_007_199_254_740_991;

fn offer_of(data: &Value) -> std::result::Result<Offer, String> {
    let repositories = match data.get("repositories") {
        None | Some(Value::Null) => None,
        Some(list) => Some(
            serde_json::from_value(list.clone()).map_err(|_| "invalid_repositories".to_string())?,
        ),
    };
    let persona = match data.get("persona") {
        None | Some(Value::Null) => None,
        Some(p) => {
            let text = |k: &str| p.get(k).and_then(Value::as_str).map(str::to_string);
            Some(OfferPersona {
                id: text("id").ok_or("invalid_persona")?,
                name: text("name").ok_or("invalid_persona")?,
                instructions: text("instructions").ok_or("invalid_persona")?,
            })
        }
    };
    let context = match data.get("context") {
        None | Some(Value::Null) => None,
        Some(c) => {
            let pages = c
                .get("pages")
                .and_then(Value::as_array)
                .ok_or("context_changed")?;
            let mut out = Vec::new();
            for page in pages {
                let text = |k: &str| page.get(k).and_then(Value::as_str).map(str::to_string);
                let id = page
                    .get("id")
                    .and_then(Value::as_i64)
                    .ok_or("context_changed")?;
                if !(0..=MAX_SAFE_ID).contains(&id) {
                    return Err("invalid_context_page".to_string());
                }
                out.push(OfferPage {
                    id,
                    title: text("title").ok_or("context_changed")?,
                    body: text("body").ok_or("context_changed")?,
                });
            }
            Some(out)
        }
    };
    Ok(Offer {
        repositories,
        persona,
        context,
    })
}

fn coverage(offer: &Offer) -> std::result::Result<(Option<String>, Option<String>), String> {
    let persona = match &offer.persona {
        Some(p) => Some(
            persona_sha256(&p.id, &p.name, &p.instructions)
                .map_err(|_| "invalid_persona".to_string())?,
        ),
        None => None,
    };
    let context = match &offer.context {
        Some(pages) => {
            let hashes: Vec<String> = pages
                .iter()
                .map(|p| page_sha256(p.id, &p.title, &p.body))
                .collect();
            Some(context_sha256(&hashes).map_err(|_| "context_changed".to_string())?)
        }
        None => None,
    };
    Ok((persona, context))
}

pub const OFFER_OFFLINE: &str = "offline";

pub fn read_offer(client: &Client, task_id: i64) -> std::result::Result<Offer, String> {
    let (status, value) = client
        .call(
            "GET",
            &format!("/api/tasks/{task_id}/agent-approval"),
            None,
            true,
        )
        .map_err(|e| {
            if e.kind == tmgr_core::error::Kind::Offline {
                OFFER_OFFLINE.to_string()
            } else {
                "failed".to_string()
            }
        })?;
    if !(200..300).contains(&status) {
        return Err(error_code(&value).unwrap_or("failed").to_string());
    }
    offer_of(&unwrap_data(value))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signed {
    pub task_id: i64,
    pub ok: bool,
    pub code: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    key_id: String,
    scalar_b64: String,
}

fn public_of(key: &SigningKey) -> String {
    STANDARD.encode(key.verifying_key().to_encoded_point(false).as_bytes())
}

fn parse_stored(blob: &str) -> Result<(Stored, SigningKey)> {
    let corrupt = || Error::internal("the stored approval key is corrupt");
    let stored: Stored = serde_json::from_str(blob).map_err(|_| corrupt())?;
    let bytes = STANDARD.decode(&stored.scalar_b64).map_err(|_| corrupt())?;
    let key = SigningKey::from_slice(&bytes).map_err(|_| corrupt())?;
    Ok((stored, key))
}

fn no_keychain() -> Error {
    Error::new(
        Kind::Forbidden,
        "the CLI keeps its approval key only in the OS keychain, and none is available",
    )
    .hint("confirm tasks in the web app instead")
}

fn keychain_refused() -> Error {
    Error::internal("the OS keychain refused access (locked or denied)")
        .hint("unlock the keychain and retry")
}

fn forced_file() -> bool {
    std::env::var(STORE_ENV).as_deref() == Ok("file")
}

#[cfg(debug_assertions)]
type KeyResult<T> = std::result::Result<T, KeyringError>;

#[cfg(debug_assertions)]
struct FileKeychain(std::path::PathBuf);

#[cfg(debug_assertions)]
impl FileKeychain {
    fn load(&self) -> std::collections::BTreeMap<String, String> {
        std::fs::read(&self.0)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn store(&self, map: &std::collections::BTreeMap<String, String>) -> KeyResult<()> {
        let body =
            serde_json::to_vec(map).map_err(|e| KeyringError::PlatformFailure(Box::new(e)))?;
        std::fs::write(&self.0, body).map_err(|e| KeyringError::PlatformFailure(Box::new(e)))
    }
}

#[cfg(debug_assertions)]
impl Keychain for FileKeychain {
    fn get(&self, account: &str) -> KeyResult<String> {
        self.load().remove(account).ok_or(KeyringError::NoEntry)
    }

    fn set(&self, account: &str, blob: &str) -> KeyResult<()> {
        let mut map = self.load();
        map.insert(account.to_string(), blob.to_string());
        self.store(&map)
    }

    fn delete(&self, account: &str) -> KeyResult<()> {
        let mut map = self.load();
        map.remove(account);
        self.store(&map)
    }
}

#[cfg(debug_assertions)]
fn test_keychain() -> Option<Box<dyn Keychain>> {
    let path = std::env::var_os("TMGR_TEST_KEYCHAIN_FILE")?;
    Some(Box::new(FileKeychain(path.into())))
}

#[cfg(not(debug_assertions))]
fn test_keychain() -> Option<Box<dyn Keychain>> {
    None
}

fn default_keychain() -> (Box<dyn Keychain>, bool) {
    match test_keychain() {
        Some(k) => (k, false),
        None => (Box::new(OsKeychain), forced_file()),
    }
}

fn read_stored(keychain: &dyn Keychain, host: &str, user_id: i64) -> Result<Option<String>> {
    match keychain.get(&account(host, user_id)) {
        Ok(blob) => Ok(Some(blob)),
        Err(e) => match classify(&e, cfg!(target_os = "linux")) {
            Failure::NoEntry => Ok(None),
            Failure::StoreAbsent => Err(no_keychain()),
            Failure::Refused => Err(keychain_refused()),
        },
    }
}

/// The CLI key of this machine for a server and user; never creates one and never reads a file.
pub fn own_key(server: &str, user_id: i64) -> Result<Option<OwnKey>> {
    let (keychain, file_forced) = default_keychain();
    if file_forced {
        return Ok(None);
    }
    own_key_in(&*keychain, &host_of(server)?, user_id)
}

pub fn own_key_in(keychain: &dyn Keychain, host: &str, user_id: i64) -> Result<Option<OwnKey>> {
    let Some(blob) = read_stored(keychain, host, user_id)? else {
        return Ok(None);
    };
    let (stored, key) = parse_stored(&blob)?;
    if stored.key_id.is_empty() {
        return Ok(None);
    }
    Ok(Some(OwnKey {
        key_id: stored.key_id,
        public_key: public_of(&key),
    }))
}

pub fn device_name() -> String {
    let os = match std::env::consts::OS {
        "macos" => "macOS",
        "linux" => "Linux",
        "windows" => "Windows",
        _ => "unknown",
    };
    format!("CLI ({os})")
}

fn error_code(body: &Value) -> Option<&str> {
    body.get("code")
        .or_else(|| body.pointer("/error/code"))
        .or_else(|| body.pointer("/data/code"))
        .and_then(Value::as_str)
}

fn key_rejected(code: Option<&str>) -> bool {
    matches!(code, Some("key_unknown" | "key_revoked"))
}

struct Covered {
    repos: Option<String>,
    persona: Option<String>,
    context: Option<String>,
}

struct Prepared {
    task: SignTask,
    sha: String,
    covered: Covered,
}

enum Upload {
    Done(Vec<Signed>),
    KeyRejected,
}

pub struct Signer<'a> {
    client: &'a Client,
    keychain: Box<dyn Keychain>,
    host: String,
    user_id: i64,
    file_forced: bool,
    guard: Box<dyn Fn() -> Result<()>>,
}

impl<'a> Signer<'a> {
    pub fn new(client: &'a Client, server: &str, user_id: i64) -> Result<Self> {
        let (keychain, file_forced) = default_keychain();
        Self::with_keychain(client, server, user_id, keychain, file_forced)
    }

    pub fn with_keychain(
        client: &'a Client,
        server: &str,
        user_id: i64,
        keychain: Box<dyn Keychain>,
        file_forced: bool,
    ) -> Result<Self> {
        Ok(Self {
            client,
            keychain,
            host: host_of(server)?,
            user_id,
            file_forced,
            guard: Box::new(privileged::guard),
        })
    }

    #[cfg(test)]
    pub(crate) fn with_guard(mut self, guard: impl Fn() -> Result<()> + 'static) -> Self {
        self.guard = Box::new(guard);
        self
    }

    pub fn ensure_key(&self) -> Result<OwnKey> {
        (self.guard)()?;
        self.ready_key().map(|(own, _)| own)
    }

    fn ready_key(&self) -> Result<(OwnKey, SigningKey)> {
        if self.file_forced {
            return Err(no_keychain());
        }
        let pending = match read_stored(&*self.keychain, &self.host, self.user_id)? {
            Some(blob) => {
                let (stored, key) = parse_stored(&blob)?;
                if !stored.key_id.is_empty() {
                    let own = OwnKey {
                        key_id: stored.key_id,
                        public_key: public_of(&key),
                    };
                    return Ok((own, key));
                }
                key
            }
            None => self.save(&generate(), "")?,
        };
        self.register(pending)
    }

    fn save(&self, key: &SigningKey, key_id: &str) -> Result<SigningKey> {
        let blob = serde_json::to_string(&Stored {
            key_id: key_id.to_string(),
            scalar_b64: STANDARD.encode(key.to_bytes()),
        })
        .map_err(|_| Error::internal("cannot serialize the approval key"))?;
        self.keychain
            .set(&account(&self.host, self.user_id), &blob)
            .map_err(|e| match classify(&e, cfg!(target_os = "linux")) {
                Failure::StoreAbsent => no_keychain(),
                _ => keychain_refused(),
            })?;
        Ok(key.clone())
    }

    fn register(&self, key: SigningKey) -> Result<(OwnKey, SigningKey)> {
        let public_key = public_of(&key);
        let (status, value) = self.client.call(
            "POST",
            "/api/agent-runs/keys",
            Some(&json!({"public_key": public_key, "device_name": device_name()})),
            true,
        )?;
        if !(200..300).contains(&status) {
            return Err(map_error(status, &value, SCOPE));
        }
        let key_id = unwrap_data(value)
            .get("key_id")
            .and_then(Value::as_str)
            .filter(|k| !k.is_empty())
            .map(str::to_string)
            .ok_or_else(|| Error::internal("unexpected answer to the key registration"))?;
        self.save(&key, &key_id)?;
        Ok((OwnKey { key_id, public_key }, key))
    }

    fn rotate_key(&self) -> Result<(OwnKey, SigningKey)> {
        let fresh = self.save(&generate(), "")?;
        self.register(fresh)
    }

    pub fn sign_tasks(&self, tasks: &[SignTask]) -> Result<Vec<Signed>> {
        (self.guard)()?;
        let (mut own, mut key) = self.ready_key()?;
        let (prepared, mut results) = self.prepare(tasks);
        if prepared.is_empty() {
            return Ok(results);
        }
        let signed_at = chrono::Utc::now().timestamp();
        let mut rotated = false;
        loop {
            match self.upload(&own, &key, &prepared, signed_at)? {
                Upload::Done(done) => {
                    results.extend(done);
                    return Ok(results);
                }
                Upload::KeyRejected if !rotated => {
                    (own, key) = self.rotate_key()?;
                    rotated = true;
                }
                Upload::KeyRejected => {
                    return Err(Error::new(
                        Kind::Conflict,
                        "the server does not accept the CLI approval key",
                    )
                    .hint("run tmgr login and allow agent approvals, then retry"))
                }
            }
        }
    }

    fn prepare(&self, tasks: &[SignTask]) -> (Vec<Prepared>, Vec<Signed>) {
        let mut ready = Vec::new();
        let mut refused = Vec::new();
        for task in tasks {
            match self.coverage(task) {
                Ok(covered) => ready.push(Prepared {
                    sha: content_sha256(&task.title, &task.description),
                    covered,
                    task: task.clone(),
                }),
                Err(code) => refused.push(Signed {
                    task_id: task.id,
                    ok: false,
                    code: Some(code),
                }),
            }
        }
        (ready, refused)
    }

    fn coverage(&self, task: &SignTask) -> std::result::Result<Covered, String> {
        let offered = read_offer(self.client, task.id)?;
        let repos = match &offered.repositories {
            None => None,
            Some(names) => {
                Some(repos_sha256(names).map_err(|_| "invalid_repositories".to_string())?)
            }
        };
        if repos != task.repos_sha256 {
            return Err("repositories_changed".to_string());
        }
        if offered.persona != task.offer.persona {
            return Err("persona_changed".to_string());
        }
        if offered.context != task.offer.context {
            return Err("context_changed".to_string());
        }
        let (persona, context) = coverage(&offered)?;
        Ok(Covered {
            repos,
            persona,
            context,
        })
    }

    fn upload(
        &self,
        own: &OwnKey,
        key: &SigningKey,
        prepared: &[Prepared],
        signed_at: i64,
    ) -> Result<Upload> {
        let mut done = Vec::new();
        for (n, chunk) in prepared.chunks(CHUNK).enumerate() {
            let items: Vec<Value> = chunk
                .iter()
                .map(|p| self.item(own, key, p, signed_at))
                .collect();
            let (status, value) = self.client.call(
                "POST",
                "/api/agent-runs/approvals/bulk",
                Some(&json!({ "items": items })),
                true,
            )?;
            if !(200..300).contains(&status) {
                if n == 0 && key_rejected(error_code(&value)) {
                    return Ok(Upload::KeyRejected);
                }
                return Err(map_error(status, &value, SCOPE));
            }
            let answers = match unwrap_data(value) {
                Value::Array(a) => a,
                _ => return Err(Error::internal("unexpected answer to the bulk approval")),
            };
            let outcomes: Vec<Signed> =
                chunk.iter().map(|p| outcome(p.task.id, &answers)).collect();
            if n == 0
                && !outcomes.is_empty()
                && outcomes.iter().all(|o| key_rejected(o.code.as_deref()))
            {
                return Ok(Upload::KeyRejected);
            }
            done.extend(outcomes);
        }
        Ok(Upload::Done(done))
    }

    fn item(&self, own: &OwnKey, key: &SigningKey, p: &Prepared, signed_at: i64) -> Value {
        let message = approval_message(&Payload {
            host: self.host.clone(),
            user_id: self.user_id,
            workspace_id: p.task.workspace_id,
            task_id: p.task.id,
            category_id: p.task.category_id,
            content_sha256: p.sha.clone(),
            repos_sha256: p.covered.repos.clone(),
            persona_sha256: p.covered.persona.clone(),
            context_sha256: p.covered.context.clone(),
            key_id: own.key_id.clone(),
            signed_at,
        });
        let signature: Signature = key.sign(message.as_bytes());
        let mut item = json!({
            "task_id": p.task.id,
            "kind": KIND,
            "key_id": own.key_id,
            "host": self.host,
            "category_id": p.task.category_id,
            "content_sha256": p.sha,
            "signed_at": signed_at,
            "signature": STANDARD.encode(signature.to_bytes()),
        });
        for (name, hash) in [
            ("repos_sha256", &p.covered.repos),
            ("persona_sha256", &p.covered.persona),
            ("context_sha256", &p.covered.context),
        ] {
            if let Some(hash) = hash {
                item[name] = json!(hash);
            }
        }
        item
    }
}

fn outcome(task_id: i64, answers: &[Value]) -> Signed {
    let found = answers
        .iter()
        .find(|a| a.get("task_id").and_then(Value::as_i64) == Some(task_id));
    match found {
        Some(a) if a.get("ok").and_then(Value::as_bool) == Some(true) => Signed {
            task_id,
            ok: true,
            code: None,
        },
        Some(a) => Signed {
            task_id,
            ok: false,
            code: Some(
                a.get("code")
                    .and_then(Value::as_str)
                    .unwrap_or("failed")
                    .to_string(),
            ),
        },
        None => Signed {
            task_id,
            ok: false,
            code: Some("no_answer".into()),
        },
    }
}

fn generate() -> SigningKey {
    loop {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).expect("system random source");
        if let Ok(key) = SigningKey::from_slice(&bytes) {
            return key;
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(debug_assertions))]
    #[test]
    fn release_builds_have_no_test_keychain() {
        std::env::set_var("TMGR_TEST_KEYCHAIN_FILE", "/nonexistent");
        assert!(super::test_keychain().is_none());
    }

    #[cfg(debug_assertions)]
    #[test]
    fn the_test_keychain_round_trips_and_reports_a_missing_entry() {
        let dir = tempfile::tempdir().unwrap();
        let k = super::FileKeychain(dir.path().join("k.json"));
        assert!(matches!(k.get("a"), Err(keyring::Error::NoEntry)));
        k.set("a", "1").unwrap();
        assert_eq!(k.get("a").unwrap(), "1");
        k.delete("a").unwrap();
        assert!(matches!(k.get("a"), Err(keyring::Error::NoEntry)));
    }

    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tmgr_core::approval::{check_claim, verify, CloudApproval, Entry, PinnedKey};
    use tmgr_core::redact::Secret;
    use wiremock::matchers::{method, path, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[derive(Clone, Default)]
    struct Fake {
        map: Arc<Mutex<HashMap<String, String>>>,
        absent: bool,
    }

    impl Keychain for Fake {
        fn get(&self, account: &str) -> keyring::Result<String> {
            if self.absent {
                return Err(keyring::Error::NoDefaultStore);
            }
            self.map
                .lock()
                .unwrap()
                .get(account)
                .cloned()
                .ok_or(keyring::Error::NoEntry)
        }

        fn set(&self, account: &str, blob: &str) -> keyring::Result<()> {
            if self.absent {
                return Err(keyring::Error::NoDefaultStore);
            }
            self.map.lock().unwrap().insert(account.into(), blob.into());
            Ok(())
        }

        fn delete(&self, account: &str) -> keyring::Result<()> {
            self.map.lock().unwrap().remove(account);
            Ok(())
        }
    }

    fn task(id: i64, title: &str) -> SignTask {
        SignTask {
            id,
            workspace_id: 56,
            category_id: 127,
            title: title.into(),
            description: "body".into(),
            repos_sha256: None,
            offer: Offer::default(),
        }
    }

    async fn on_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        tokio::task::spawn_blocking(f).await.unwrap()
    }

    fn signer<'a>(c: &'a Client, uri: &str, fake: &Fake) -> Signer<'a> {
        Signer::with_keychain(c, uri, 1, Box::new(fake.clone()), false)
            .unwrap()
            .with_guard(|| Ok(()))
    }

    async fn mount_register(server: &MockServer, key_id: &str) {
        Mock::given(method("POST"))
            .and(path("/api/agent-runs/keys"))
            .respond_with(ResponseTemplate::new(201).set_body_json(
                json!({"key_id": key_id, "device_name": "CLI (macOS)", "created_at": "2026-10-19T00:00:00Z"}),
            ))
            .mount(server)
            .await;
    }

    async fn mount_repos(server: &MockServer, repos: Value) {
        Mock::given(method("GET"))
            .and(path_regex(r"^/api/tasks/\d+/agent-approval$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"repositories": repos})))
            .mount(server)
            .await;
    }

    async fn mount_bulk_ok(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/api/agent-runs/approvals/bulk"))
            .respond_with(|req: &wiremock::Request| {
                let body: Value = serde_json::from_slice(&req.body).unwrap();
                let answers: Vec<Value> = body["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|i| json!({"task_id": i["task_id"], "ok": true}))
                    .collect();
                ResponseTemplate::new(200).set_body_json(answers)
            })
            .mount(server)
            .await;
    }

    async fn bodies(server: &MockServer, p: &str) -> Vec<Value> {
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == p && r.method.as_str() == "POST")
            .map(|r| serde_json::from_slice(&r.body).unwrap())
            .collect()
    }

    fn claim_of(item: &Value, user_id: i64, workspace_id: i64) -> CloudApproval {
        CloudApproval {
            host: item["host"].as_str().unwrap().into(),
            user_id,
            workspace_id,
            task_id: item["task_id"].as_i64().unwrap(),
            category_id: item["category_id"].as_i64().unwrap(),
            content_sha256: item["content_sha256"].as_str().unwrap().into(),
            key_id: item["key_id"].as_str().unwrap().into(),
            signed_at: item["signed_at"].as_i64().unwrap(),
            signature: item["signature"].as_str().unwrap().into(),
            kind: item["kind"].as_str().unwrap().into(),
            repos_sha256: item["repos_sha256"].as_str().map(str::to_string),
            persona_sha256: item["persona_sha256"].as_str().map(str::to_string),
            context_sha256: item["context_sha256"].as_str().map(str::to_string),
        }
    }

    fn pinned(host: &str, own: &OwnKey) -> Entry {
        Entry {
            server: host.into(),
            user_id: 1,
            workspace_id: 56,
            workspace_code: "tmgrdev".into(),
            allowed_at: "2026-10-19T00:00:00Z".into(),
            expires_at: None,
            keys: vec![PinnedKey {
                key_id: own.key_id.clone(),
                public_key: own.public_key.clone(),
                device_name: "CLI".into(),
            }],
            repos: Default::default(),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn first_use_generates_registers_and_stores_in_the_keychain_once() {
        let server = MockServer::start().await;
        mount_register(&server, "key-1").await;
        let fake = Fake::default();
        let uri = server.uri();
        let f = fake.clone();
        let (a, b) = on_thread(move || {
            let c = client_for(&uri);
            let s = signer(&c, &uri, &f);
            (s.ensure_key().unwrap(), s.ensure_key().unwrap())
        })
        .await;
        assert_eq!(a, b);
        assert_eq!(a.key_id, "key-1");
        let raw = STANDARD.decode(&a.public_key).unwrap();
        assert_eq!((raw.len(), raw[0]), (65, 4));
        let regs = bodies(&server, "/api/agent-runs/keys").await;
        assert_eq!(regs.len(), 1);
        assert_eq!(regs[0]["public_key"], a.public_key);
        assert!(regs[0]["device_name"]
            .as_str()
            .unwrap()
            .starts_with("CLI ("));
        let host = host_of(&server.uri()).unwrap();
        let blob = fake.map.lock().unwrap()[&format!("approval-key:{host}:1")].clone();
        let stored: Value = serde_json::from_str(&blob).unwrap();
        assert_eq!(stored["key_id"], "key-1");
        assert!(stored["scalar_b64"].as_str().unwrap().len() >= 43);
    }

    fn client_for(uri: &str) -> Client {
        let tokens = tmgr_core::creds::Tokens {
            access: Secret::new("acc"),
            refresh: None,
            expires_at: None,
        };
        Client::new(uri, "test", Some(tokens), None, false).unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn signed_approvals_pass_check_claim_and_a_changed_task_fails() {
        let server = MockServer::start().await;
        mount_register(&server, "key-1").await;
        mount_repos(&server, json!(null)).await;
        mount_bulk_ok(&server).await;
        let fake = Fake::default();
        let uri = server.uri();
        let f = fake.clone();
        let (own, results) = on_thread(move || {
            let c = client_for(&uri);
            let s = signer(&c, &uri, &f);
            let r = s.sign_tasks(&[task(10, "Fix it")]).unwrap();
            (s.ensure_key().unwrap(), r)
        })
        .await;
        assert_eq!(
            results,
            vec![Signed {
                task_id: 10,
                ok: true,
                code: None
            }]
        );
        let items = bodies(&server, "/api/agent-runs/approvals/bulk").await;
        let item = &items[0]["items"][0];
        assert_eq!(item["kind"], "confirm");
        assert!(item.get("repos_sha256").is_none());
        let claim = claim_of(item, 1, 56);
        let entry = pinned(&claim.host, &own);
        check_claim(&entry, &claim, "Fix it", "body").unwrap();
        assert!(check_claim(&entry, &claim, "Fix it!", "body").is_err());
        assert!(check_claim(&entry, &claim, "Fix it", "body and more").is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_repository_set_makes_a_v2_signature_that_check_claim_accepts() {
        let server = MockServer::start().await;
        mount_register(&server, "key-1").await;
        mount_repos(&server, json!(["acme/web", "acme/api"])).await;
        mount_bulk_ok(&server).await;
        let fake = Fake::default();
        let uri = server.uri();
        let f = fake.clone();
        let own = on_thread(move || {
            let c = client_for(&uri);
            let s = signer(&c, &uri, &f);
            let mut t = task(11, "T");
            t.repos_sha256 = Some(repos_sha256(&["acme/api".into(), "acme/web".into()]).unwrap());
            let r = s.sign_tasks(&[t]).unwrap();
            assert!(r[0].ok, "{r:?}");
            s.ensure_key().unwrap()
        })
        .await;
        let items = bodies(&server, "/api/agent-runs/approvals/bulk").await;
        let claim = claim_of(&items[0]["items"][0], 1, 56);
        assert_eq!(
            claim.repos_sha256.as_deref(),
            Some(
                repos_sha256(&["acme/api".into(), "acme/web".into()])
                    .unwrap()
                    .as_str()
            )
        );
        check_claim(&pinned(&claim.host, &own), &claim, "T", "body").unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_task_whose_repositories_differ_from_the_ones_shown_is_refused() {
        let server = MockServer::start().await;
        mount_register(&server, "key-1").await;
        mount_repos(&server, json!(["acme/web", "acme/other"])).await;
        mount_bulk_ok(&server).await;
        let fake = Fake::default();
        let uri = server.uri();
        let f = fake.clone();
        let results = on_thread(move || {
            let c = client_for(&uri);
            let s = signer(&c, &uri, &f);
            let mut shown = task(11, "T");
            shown.repos_sha256 =
                Some(repos_sha256(&["acme/api".into(), "acme/web".into()]).unwrap());
            s.sign_tasks(&[shown, task(12, "none shown")]).unwrap()
        })
        .await;
        assert!(results
            .iter()
            .all(|r| !r.ok && r.code.as_deref() == Some("repositories_changed")));
        assert!(bodies(&server, "/api/agent-runs/approvals/bulk")
            .await
            .is_empty());
    }

    fn offer_fixture() -> (Offer, Value) {
        let persona = OfferPersona {
            id: "993c30a6-3528-42bf-9a70-507841cebc08".into(),
            name: "Agent".into(),
            instructions: "Follow the rules.\r\nOpen a PR.".into(),
        };
        let pages = vec![OfferPage {
            id: 125,
            title: "Rules".into(),
            body: "- Take only Backlog tasks.".into(),
        }];
        let served = json!({
            "repositories": null,
            "persona": {"id": persona.id, "name": persona.name, "instructions": persona.instructions, "sha256": "x"},
            "context": {"pages": [{"id": 125, "slug": "r", "version": 1, "title": "Rules", "body": "- Take only Backlog tasks.", "sha256": "x"}], "sha256": "x"},
        });
        (
            Offer {
                repositories: None,
                persona: Some(persona),
                context: Some(pages),
            },
            served,
        )
    }

    async fn mount_offer(server: &MockServer, served: Value) {
        Mock::given(method("GET"))
            .and(path_regex(r"^/api/tasks/\d+/agent-approval$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(served))
            .mount(server)
            .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_persona_and_rules_offer_makes_a_v3_signature_that_check_claim_accepts() {
        let (offer, served) = offer_fixture();
        let server = MockServer::start().await;
        mount_register(&server, "key-1").await;
        mount_offer(&server, served).await;
        mount_bulk_ok(&server).await;
        let uri = server.uri();
        let fake = Fake::default();
        let own = on_thread(move || {
            let c = client_for(&uri);
            let s = signer(&c, &uri, &fake);
            let mut t = task(13, "T");
            t.offer = offer;
            let r = s.sign_tasks(&[t]).unwrap();
            assert!(r[0].ok, "{r:?}");
            s.ensure_key().unwrap()
        })
        .await;
        let items = bodies(&server, "/api/agent-runs/approvals/bulk").await;
        let item = &items[0]["items"][0];
        let claim = claim_of(item, 1, 56);
        let (offer, _) = offer_fixture();
        let (persona, context) = coverage(&offer).unwrap();
        assert_eq!(claim.persona_sha256, persona);
        assert_eq!(claim.context_sha256, context);
        assert!(persona.is_some() && context.is_some());
        check_claim(&pinned(&claim.host, &own), &claim, "T", "body").unwrap();
        let mut other = claim.clone();
        other.persona_sha256 = Some("0".repeat(64));
        assert!(check_claim(&pinned(&claim.host, &own), &other, "T", "body").is_err());
    }

    #[test]
    fn page_ids_beyond_the_javascript_safe_integer_are_refused() {
        let page = |id: i64| json!({"context": {"pages": [{"id": id, "title": "t", "body": "b"}]}});
        assert!(offer_of(&page(9_007_199_254_740_991)).is_ok());
        assert_eq!(
            offer_of(&page(9_007_199_254_740_992)).unwrap_err(),
            "invalid_context_page"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn persona_or_rules_changed_since_shown_are_refused_and_nothing_is_sent() {
        let (offer, mut served) = offer_fixture();
        served["persona"]["instructions"] = json!("Changed.");
        let server = MockServer::start().await;
        mount_register(&server, "key-1").await;
        mount_offer(&server, served.clone()).await;
        mount_bulk_ok(&server).await;
        let uri = server.uri();
        let fake = Fake::default();
        let shown = offer.clone();
        let results = on_thread(move || {
            let c = client_for(&uri);
            let mut t = task(13, "T");
            t.offer = shown;
            let mut u = task(14, "U");
            u.offer = Offer {
                repositories: None,
                persona: t.offer.persona.clone(),
                context: None,
            };
            signer(&c, &uri, &fake).sign_tasks(&[t, u]).unwrap()
        })
        .await;
        let codes: Vec<_> = results.iter().map(|r| r.code.as_deref()).collect();
        assert_eq!(
            codes,
            vec![Some("persona_changed"), Some("persona_changed")]
        );
        assert!(bodies(&server, "/api/agent-runs/approvals/bulk")
            .await
            .is_empty());

        let (_, mut served) = offer_fixture();
        served["context"]["pages"][0]["body"] = json!("Changed.");
        let server = MockServer::start().await;
        mount_register(&server, "key-1").await;
        mount_offer(&server, served).await;
        let uri = server.uri();
        let fake = Fake::default();
        let results = on_thread(move || {
            let c = client_for(&uri);
            let mut t = task(13, "T");
            t.offer = offer;
            signer(&c, &uri, &fake).sign_tasks(&[t]).unwrap()
        })
        .await;
        assert_eq!(results[0].code.as_deref(), Some("context_changed"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn more_than_a_hundred_tasks_go_in_chunks_of_a_hundred() {
        let server = MockServer::start().await;
        mount_register(&server, "key-1").await;
        mount_repos(&server, json!(null)).await;
        mount_bulk_ok(&server).await;
        let uri = server.uri();
        let fake = Fake::default();
        let results = on_thread(move || {
            let c = client_for(&uri);
            let tasks: Vec<SignTask> = (1..=101).map(|i| task(i, "T")).collect();
            signer(&c, &uri, &fake).sign_tasks(&tasks).unwrap()
        })
        .await;
        assert_eq!(results.len(), 101);
        assert!(results.iter().all(|r| r.ok));
        let sizes: Vec<usize> = bodies(&server, "/api/agent-runs/approvals/bulk")
            .await
            .iter()
            .map(|b| b["items"].as_array().unwrap().len())
            .collect();
        assert_eq!(sizes, vec![100, 1]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn invalid_repositories_and_server_refusals_are_reported_per_task() {
        let server = MockServer::start().await;
        mount_register(&server, "key-1").await;
        Mock::given(method("GET"))
            .and(path("/api/tasks/1/agent-approval"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"repositories": []})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/tasks/2/agent-approval"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({"code": "not_found"})))
            .mount(&server)
            .await;
        mount_repos(&server, json!(null)).await;
        Mock::given(method("POST"))
            .and(path("/api/agent-runs/approvals/bulk"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!([{"task_id": 3, "ok": false, "code": "setting_off"}])),
            )
            .mount(&server)
            .await;
        let uri = server.uri();
        let fake = Fake::default();
        let results = on_thread(move || {
            let c = client_for(&uri);
            signer(&c, &uri, &fake)
                .sign_tasks(&[task(1, "a"), task(2, "b"), task(3, "c")])
                .unwrap()
        })
        .await;
        let code = |id: i64| {
            results
                .iter()
                .find(|r| r.task_id == id)
                .unwrap()
                .code
                .clone()
        };
        assert_eq!(code(1).as_deref(), Some("invalid_repositories"));
        assert_eq!(code(2).as_deref(), Some("not_found"));
        assert_eq!(code(3).as_deref(), Some("setting_off"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_rejected_key_is_registered_again_once() {
        let server = MockServer::start().await;
        mount_register(&server, "key-1").await;
        mount_repos(&server, json!(null)).await;
        Mock::given(method("POST"))
            .and(path("/api/agent-runs/approvals/bulk"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!([{"task_id": 5, "ok": false, "code": "key_unknown"}])),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
        mount_bulk_ok(&server).await;
        let uri = server.uri();
        let fake = Fake::default();
        let results = on_thread(move || {
            let c = client_for(&uri);
            signer(&c, &uri, &fake).sign_tasks(&[task(5, "a")]).unwrap()
        })
        .await;
        assert!(results[0].ok);
        assert_eq!(bodies(&server, "/api/agent-runs/keys").await.len(), 2);
        assert_eq!(
            bodies(&server, "/api/agent-runs/approvals/bulk")
                .await
                .len(),
            2
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_key_rejected_twice_is_an_error_not_a_loop() {
        let server = MockServer::start().await;
        mount_register(&server, "key-1").await;
        mount_repos(&server, json!(null)).await;
        Mock::given(method("POST"))
            .and(path("/api/agent-runs/approvals/bulk"))
            .respond_with(ResponseTemplate::new(422).set_body_json(json!({"code": "key_unknown"})))
            .mount(&server)
            .await;
        let uri = server.uri();
        let fake = Fake::default();
        let err = on_thread(move || {
            let c = client_for(&uri);
            signer(&c, &uri, &fake)
                .sign_tasks(&[task(5, "a")])
                .unwrap_err()
        })
        .await;
        assert_eq!(err.exit_code(), 6);
        assert_eq!(bodies(&server, "/api/agent-runs/keys").await.len(), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn without_a_tty_nothing_is_signed_generated_or_sent() {
        let server = MockServer::start().await;
        let uri = server.uri();
        let fake = Fake::default();
        let f = fake.clone();
        let err = on_thread(move || {
            let c = client_for(&uri);
            let s = Signer::with_keychain(&c, &uri, 1, Box::new(f), false).unwrap();
            s.sign_tasks(&[task(1, "a")]).unwrap_err()
        })
        .await;
        assert_eq!(err.exit_code(), 10);
        assert!(server.received_requests().await.unwrap().is_empty());
        assert!(fake.map.lock().unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn in_an_agent_session_the_signer_refuses_even_on_a_tty() {
        let server = MockServer::start().await;
        let uri = server.uri();
        let fake = Fake::default();
        let f = fake.clone();
        let err = on_thread(move || {
            let c = client_for(&uri);
            let s = Signer::with_keychain(&c, &uri, 1, Box::new(f), false)
                .unwrap()
                .with_guard(|| privileged::check(true, true));
            let first = s.sign_tasks(&[task(1, "a")]).unwrap_err();
            assert!(s.ensure_key().is_err());
            first
        })
        .await;
        assert_eq!(err.exit_code(), 5);
        assert!(server.received_requests().await.unwrap().is_empty());
        assert!(fake.map.lock().unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn no_os_keychain_or_the_file_store_means_refusal_and_no_registration() {
        let server = MockServer::start().await;
        mount_register(&server, "key-1").await;
        let uri = server.uri();
        let (absent, forced) = on_thread(move || {
            let c = client_for(&uri);
            let gone = Fake {
                absent: true,
                ..Fake::default()
            };
            let a = Signer::with_keychain(&c, &uri, 1, Box::new(gone), false)
                .unwrap()
                .with_guard(|| Ok(()))
                .ensure_key()
                .unwrap_err();
            let b = Signer::with_keychain(&c, &uri, 1, Box::new(Fake::default()), true)
                .unwrap()
                .with_guard(|| Ok(()))
                .ensure_key()
                .unwrap_err();
            (a, b)
        })
        .await;
        for e in [absent, forced] {
            assert_eq!(e.exit_code(), 5);
            assert!(e.message.contains("OS keychain"), "{}", e.message);
        }
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[test]
    fn own_key_reads_without_creating_and_skips_unregistered_keys() {
        let fake = Fake::default();
        assert_eq!(own_key_in(&fake, "h", 1).unwrap(), None);
        assert!(fake.map.lock().unwrap().is_empty());
        let key = generate();
        let blob = |id: &str| {
            serde_json::to_string(&Stored {
                key_id: id.into(),
                scalar_b64: STANDARD.encode(key.to_bytes()),
            })
            .unwrap()
        };
        fake.set(&account("h", 1), &blob("")).unwrap();
        assert_eq!(own_key_in(&fake, "h", 1).unwrap(), None);
        fake.set(&account("h", 1), &blob("k9")).unwrap();
        let own = own_key_in(&fake, "h", 1).unwrap().unwrap();
        assert_eq!(
            (own.key_id.as_str(), own.public_key),
            ("k9", public_of(&key))
        );
        let gone = Fake {
            absent: true,
            ..Fake::default()
        };
        assert!(own_key_in(&gone, "h", 1).is_err());
    }

    #[test]
    fn the_fixture_key_has_the_webcrypto_public_key_and_its_signatures_verify() {
        let f: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../tmgr-core/tests/fixtures/agent-approval-fixtures.json"
        )))
        .unwrap();
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let d = URL_SAFE_NO_PAD
            .decode(f["key"]["private_jwk"]["d"].as_str().unwrap())
            .unwrap();
        let key = SigningKey::from_slice(&d).unwrap();
        assert_eq!(public_of(&key), f["key"]["public_key"].as_str().unwrap());
        let sig: Signature = key.sign(b"message");
        assert!(verify(
            f["key"]["public_key"].as_str().unwrap(),
            "message",
            &STANDARD.encode(sig.to_bytes())
        ));
        assert_eq!(sig.to_bytes().len(), 64);
    }

    #[test]
    fn host_keeps_non_default_ports_only() {
        assert_eq!(
            host_of("https://API.tmgr.dev:443/").unwrap(),
            "api.tmgr.dev"
        );
        assert_eq!(host_of("http://localhost:8080").unwrap(), "localhost:8080");
        assert!(host_of("nonsense").is_err());
    }
}
