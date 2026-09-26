use std::sync::Mutex;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Runtime, State, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "quick-add";

#[derive(Default)]
pub struct QuickAddStore(pub Mutex<Option<Value>>);

fn window<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<tauri::WebviewWindow<R>> {
  if let Some(window) = app.get_webview_window(LABEL) {
    return Ok(window);
  }
  WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("quick-add".into()))
    .title("Quick add")
    .inner_size(620.0, 132.0)
    .resizable(false)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .center()
    .build()
}

#[tauri::command]
pub fn open_quick_add<R: Runtime>(
  app: AppHandle<R>,
  store: State<'_, QuickAddStore>,
  payload: Option<Value>,
) -> Result<(), String> {
  *store.0.lock().unwrap() = Some(payload.unwrap_or(Value::Null));
  let window = window(&app).map_err(|e| e.to_string())?;
  let _ = window.center();
  window.show().map_err(|e| e.to_string())?;
  let _ = window.set_focus();
  let _ = app.emit_to(LABEL, "quick-add://open", ());
  log::info!("[quick-add] open");
  Ok(())
}

#[tauri::command]
pub fn take_quick_add(store: State<'_, QuickAddStore>) -> Option<Value> {
  store.0.lock().unwrap().take()
}

#[tauri::command]
pub fn hide_quick_add<R: Runtime>(app: AppHandle<R>) {
  if let Some(window) = app.get_webview_window(LABEL) {
    let _ = window.hide();
  }
}
