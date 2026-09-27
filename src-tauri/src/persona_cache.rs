use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};

/// Prompt/skills cache, keyed by persona uuid, outside the workspace folder: the prompt is the owner's data, not workspace data.
fn cache_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
  let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("persona-cache");
  fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
  Ok(dir)
}

fn cache_path<R: Runtime>(app: &AppHandle<R>, uuid: &str) -> Result<PathBuf, String> {
  if uuid.is_empty() || uuid.len() > 64 || !uuid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
    return Err("invalid persona uuid".into());
  }
  Ok(cache_dir(app)?.join(format!("{uuid}.json")))
}

#[tauri::command]
pub fn persona_cache_put<R: Runtime>(app: AppHandle<R>, uuid: String, data: Value) -> Result<(), String> {
  let path = cache_path(&app, &uuid)?;
  fs::write(path, serde_json::to_vec(&data).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn persona_cache_get<R: Runtime>(app: AppHandle<R>, uuid: String) -> Result<Option<Value>, String> {
  let path = cache_path(&app, &uuid)?;
  if !path.exists() {
    return Ok(None);
  }
  let bytes = fs::read(&path).map_err(|e| e.to_string())?;
  serde_json::from_slice(&bytes).map(Some).map_err(|e| e.to_string())
}
