use tauri::utils::config::BackgroundThrottlingPolicy;
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

fn task_window_label(workspace_code: &str, task_id: u64) -> Result<String, String> {
  let valid = !workspace_code.is_empty()
    && workspace_code.len() <= 64
    && workspace_code.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
  if !valid {
    return Err("invalid workspace code".to_owned());
  }
  Ok(format!("task-{workspace_code}-{task_id}"))
}

#[tauri::command]
pub async fn open_task_window<R: Runtime>(
  app: AppHandle<R>,
  task_id: u64,
  workspace_code: String,
  title: Option<String>,
) -> Result<(), String> {
  let label = task_window_label(&workspace_code, task_id)?;
  let title = title.as_deref().map(str::trim).filter(|t| !t.is_empty());

  if let Some(window) = app.get_webview_window(&label) {
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
    if let Some(title) = title {
      let _ = window.set_title(title);
    }
    return Ok(());
  }

  log::info!("[task-window] open {label}");
  let url = WebviewUrl::App(format!("{workspace_code}/task-window/{task_id}").into());
  let builder = WebviewWindowBuilder::new(&app, &label, url)
    .title(title.unwrap_or("Task"))
    .inner_size(760.0, 860.0)
    .min_inner_size(480.0, 500.0)
    .resizable(true)
    .disable_drag_drop_handler()
    .background_throttling(BackgroundThrottlingPolicy::Disabled)
    .center();
  let window = crate::with_app_webview_handlers(builder, &app)
    .build()
    .map_err(|e| e.to_string())?;
  let _ = window.show();
  let _ = window.set_focus();
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::task_window_label;

  #[test]
  fn builds_label() {
    assert_eq!(task_window_label("my-ws_1", 42).unwrap(), "task-my-ws_1-42");
  }

  #[test]
  fn rejects_invalid_chars() {
    assert!(task_window_label("a/b", 1).is_err());
    assert!(task_window_label("a b", 1).is_err());
    assert!(task_window_label("../x", 1).is_err());
    assert!(task_window_label("wß", 1).is_err());
  }

  #[test]
  fn rejects_empty() {
    assert!(task_window_label("", 1).is_err());
  }

  #[test]
  fn rejects_too_long() {
    assert!(task_window_label(&"a".repeat(65), 1).is_err());
    assert!(task_window_label(&"a".repeat(64), 1).is_ok());
  }
}
