use tauri::utils::config::BackgroundThrottlingPolicy;
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

fn is_code(value: &str, max: usize) -> bool {
  !value.is_empty()
    && value.len() <= max
    && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn page_window_label(workspace_code: &str, page_id: u64) -> Result<String, String> {
  if !is_code(workspace_code, 64) {
    return Err("invalid workspace code".to_owned());
  }
  Ok(format!("page-{workspace_code}-{page_id}"))
}

fn focus_existing<R: Runtime>(app: &AppHandle<R>, label: &str) -> Option<tauri::WebviewWindow<R>> {
  let window = app.get_webview_window(label)?;
  let _ = window.unminimize();
  let _ = window.show();
  let _ = window.set_focus();
  Some(window)
}

#[tauri::command]
pub fn focus_page_window<R: Runtime>(
  app: AppHandle<R>,
  page_id: u64,
  workspace_code: String,
) -> Result<bool, String> {
  let label = page_window_label(&workspace_code, page_id)?;
  Ok(focus_existing(&app, &label).is_some())
}

#[tauri::command]
pub async fn open_page_window<R: Runtime>(
  app: AppHandle<R>,
  page_id: u64,
  workspace_code: String,
  slug: String,
  title: Option<String>,
) -> Result<(), String> {
  let label = page_window_label(&workspace_code, page_id)?;
  if !is_code(&slug, 120) {
    return Err("invalid page slug".to_owned());
  }
  let title = title.as_deref().map(str::trim).filter(|t| !t.is_empty());

  if let Some(window) = focus_existing(&app, &label) {
    if let Some(title) = title {
      let _ = window.set_title(title);
    }
    return Ok(());
  }

  log::info!("[page-window] open {label}");
  let url = WebviewUrl::App(format!("{workspace_code}/page-window/{slug}").into());
  let builder = WebviewWindowBuilder::new(&app, &label, url)
    .title(title.unwrap_or("Page"))
    .inner_size(900.0, 860.0)
    .min_inner_size(520.0, 500.0)
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
  use super::{is_code, page_window_label};

  #[test]
  fn builds_label() {
    assert_eq!(page_window_label("my-ws_1", 42).unwrap(), "page-my-ws_1-42");
  }

  #[test]
  fn rejects_invalid_workspace_codes() {
    assert!(page_window_label("", 1).is_err());
    assert!(page_window_label("a/b", 1).is_err());
    assert!(page_window_label("../x", 1).is_err());
    assert!(page_window_label(&"a".repeat(65), 1).is_err());
  }

  #[test]
  fn slug_is_a_single_url_segment() {
    assert!(!is_code("kontekst-vor\u{43a}space", 120));
    assert!(is_code("ivan-petrov-a1b2", 120));
    assert!(!is_code("a/b", 120));
    assert!(!is_code("a?x=1", 120));
    assert!(!is_code("", 120));
  }
}
