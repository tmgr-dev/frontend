use serde::{Deserialize, Serialize};
use tauri::ipc::Response;
use tauri::{AppHandle, Runtime};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSettings {
  pub mode: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalHealth {
  pub healthy: bool,
  pub last_error: Option<String>,
}

#[tauri::command]
pub async fn tts_settings_get<R: Runtime>(_app: AppHandle<R>) -> Result<ExternalSettings, String> {
  Ok(ExternalSettings::default())
}

#[tauri::command]
pub async fn tts_settings_set<R: Runtime>(_app: AppHandle<R>, settings: ExternalSettings) -> Result<ExternalSettings, String> {
  Ok(settings)
}

#[tauri::command]
pub async fn tts_external_synthesize<R: Runtime>(_app: AppHandle<R>, _text: String, _run: u64) -> Result<Response, String> {
  Err("external voice is not implemented yet".to_owned())
}

#[tauri::command]
pub async fn tts_external_test<R: Runtime>(_app: AppHandle<R>) -> Result<Response, String> {
  Err("external voice is not implemented yet".to_owned())
}

#[tauri::command]
pub async fn tts_external_health<R: Runtime>(_app: AppHandle<R>) -> Result<ExternalHealth, String> {
  Ok(ExternalHealth { healthy: true, last_error: None })
}
