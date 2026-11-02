#![cfg(unix)]

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use keyring::credential::{Credential, CredentialApi, CredentialBuilderApi};
use tauri::test::{mock_builder, MockRuntime};
use tauri::{App, Manager, WebviewUrl, WebviewWindowBuilder};
use tmgr_lib::local_identity;
use tmgr_lib::persona_llm::{llm_chat, llm_config_get, llm_config_set};

type Store = Arc<Mutex<HashMap<(String, String), Vec<u8>>>>;

static STORE: LazyLock<Store> = LazyLock::new(Store::default);
static SERIAL: Mutex<()> = Mutex::new(());
static HOME: LazyLock<tempfile::TempDir> = LazyLock::new(|| {
  let dir = tempfile::tempdir().unwrap();
  std::env::set_var("HOME", dir.path());
  std::env::set_var("XDG_DATA_HOME", dir.path().join("data"));
  keyring::set_default_credential_builder(Box::new(MemoryBuilder));
  dir
});

struct MemoryBuilder;
struct MemoryCredential(String, String);

impl CredentialBuilderApi for MemoryBuilder {
  fn build(&self, _target: Option<&str>, service: &str, user: &str) -> keyring::Result<Box<Credential>> {
    Ok(Box::new(MemoryCredential(service.into(), user.into())))
  }
  fn as_any(&self) -> &dyn Any {
    self
  }
}

impl CredentialApi for MemoryCredential {
  fn set_secret(&self, secret: &[u8]) -> keyring::Result<()> {
    STORE.lock().unwrap().insert((self.0.clone(), self.1.clone()), secret.to_vec());
    Ok(())
  }
  fn get_secret(&self) -> keyring::Result<Vec<u8>> {
    STORE.lock().unwrap().get(&(self.0.clone(), self.1.clone())).cloned().ok_or(keyring::Error::NoEntry)
  }
  fn delete_credential(&self) -> keyring::Result<()> {
    STORE.lock().unwrap().remove(&(self.0.clone(), self.1.clone())).map(|_| ()).ok_or(keyring::Error::NoEntry)
  }
  fn as_any(&self) -> &dyn Any {
    self
  }
}

const SERVICE: &str = "dev.tmgr.persona-llm";
const IDENTITY: i64 = 7;
const OTHER: i64 = 8;

struct Fixture {
  app: App<MockRuntime>,
  window: tauri::Window<MockRuntime>,
  _guard: MutexGuard<'static, ()>,
}

fn identity_file() -> std::path::PathBuf {
  HOME.path().join(".tmgr.dev").join("cloud-identity.json")
}

fn fixture(identity: Option<i64>) -> Fixture {
  let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
  LazyLock::force(&HOME);
  STORE.lock().unwrap().clear();
  let _ = std::fs::remove_file(identity_file());
  if let Some(id) = identity {
    local_identity::set_at(&identity_file(), id, "Tester").unwrap();
  }
  let app = mock_builder().build(tauri::generate_context!()).unwrap();
  WebviewWindowBuilder::new(&app, "main", WebviewUrl::default()).build().unwrap();
  let _ = std::fs::remove_file(app.path().app_data_dir().unwrap().join("llm_config.json"));
  let window = app.get_webview_window("main").unwrap().as_ref().window();
  Fixture { app, window, _guard: guard }
}

fn plant_key(user_id: i64) {
  STORE.lock().unwrap().insert((SERVICE.into(), format!("api-key:{user_id}")), b"sk-planted".to_vec());
}

fn has_key(user_id: i64) -> bool {
  STORE.lock().unwrap().contains_key(&(SERVICE.to_string(), format!("api-key:{user_id}")))
}

fn config_get(f: &Fixture, user_id: i64) -> serde_json::Value {
  serde_json::to_value(llm_config_get(f.app.handle().clone(), f.window.clone(), user_id).unwrap()).unwrap()
}

fn chat(f: &Fixture, user_id: i64) -> Result<(), String> {
  tauri::async_runtime::block_on(llm_chat(f.app.handle().clone(), user_id, "req".into(), vec![], None))
}

#[test]
fn config_set_refuses_another_accounts_slot() {
  let f = fixture(Some(IDENTITY));
  let result = llm_config_set(
    f.app.handle().clone(),
    f.window.clone(),
    OTHER,
    "https://api.example.com/v1".into(),
    "m".into(),
    Some("sk-new".into()),
    false,
  );
  assert!(result.unwrap_err().contains("another account"));
  assert!(!has_key(OTHER));
}

#[test]
fn config_get_hides_another_accounts_key() {
  let f = fixture(Some(IDENTITY));
  plant_key(OTHER);
  let out = config_get(&f, OTHER);
  assert_eq!(out["has_api_key"], false);
  assert_eq!(out["slot_state"], "other_account");
}

#[test]
fn config_get_reports_own_key() {
  let f = fixture(Some(IDENTITY));
  plant_key(IDENTITY);
  let out = config_get(&f, IDENTITY);
  assert_eq!(out["has_api_key"], true);
  assert_eq!(out["slot_state"], "ok");
}

#[test]
fn config_get_without_identity_is_not_ready() {
  let f = fixture(None);
  plant_key(IDENTITY);
  let out = config_get(&f, IDENTITY);
  assert_eq!(out["has_api_key"], false);
  assert_eq!(out["slot_state"], "not_ready");
}

#[test]
fn chat_refuses_another_accounts_slot() {
  let f = fixture(Some(IDENTITY));
  assert!(chat(&f, OTHER).unwrap_err().contains("belongs to another account"));
}

#[test]
fn chat_lets_the_current_account_through_to_the_config_check() {
  let f = fixture(Some(IDENTITY));
  assert_eq!(chat(&f, IDENTITY).unwrap_err(), "Set an LLM base URL first");
}

#[test]
fn chat_refuses_a_key_for_a_remote_http_server() {
  let f = fixture(Some(IDENTITY));
  plant_key(IDENTITY);
  let dir = f.app.path().app_data_dir().unwrap();
  std::fs::create_dir_all(&dir).unwrap();
  std::fs::write(
    dir.join("llm_config.json"),
    r#"{"base_url":"http://remote.invalid/v1","model":"m","key_origins":{"7":"http://remote.invalid:80"}}"#,
  )
  .unwrap();
  assert!(chat(&f, IDENTITY).unwrap_err().contains("plain HTTP"));
}
