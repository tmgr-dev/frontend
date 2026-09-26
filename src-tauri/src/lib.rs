use std::path::{Path, PathBuf};

use tauri::webview::{DownloadEvent, NewWindowResponse};
use tauri::{Manager, Url, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;

fn is_app_url(url: &Url) -> bool {
  match url.scheme() {
    "tauri" | "about" | "blob" | "data" => true,
    "http" | "https" => matches!(url.host_str(), Some("tauri.localhost") | Some("localhost")),
    _ => false,
  }
}

fn is_external_url(url: &Url) -> bool {
  matches!(url.scheme(), "http" | "https" | "mailto" | "tel") && !is_app_url(url)
}

fn unique_download_path(dir: PathBuf, suggested: &Path, url: &Url) -> PathBuf {
  let name = suggested
    .file_name()
    .map(|n| n.to_string_lossy().into_owned())
    .or_else(|| url.path_segments().and_then(|mut s| s.next_back().map(str::to_owned)))
    .filter(|n| !n.is_empty())
    .unwrap_or_else(|| "download".to_owned());
  let candidate = dir.join(&name);
  if !candidate.exists() {
    return candidate;
  }
  let path = PathBuf::from(&name);
  let stem = path
    .file_stem()
    .map(|s| s.to_string_lossy().into_owned())
    .unwrap_or_else(|| name.clone());
  let ext = path
    .extension()
    .map(|e| format!(".{}", e.to_string_lossy()))
    .unwrap_or_default();
  (1..)
    .map(|i| dir.join(format!("{stem} ({i}){ext}")))
    .find(|p| !p.exists())
    .unwrap()
}

#[cfg(target_os = "macos")]
fn hide_traffic_lights(window: &WebviewWindow) {
  use objc2_app_kit::{NSWindow, NSWindowButton};
  let Ok(ns_window) = window.ns_window() else {
    return;
  };
  let ns_window = unsafe { &*(ns_window as *const NSWindow) };
  for kind in [
    NSWindowButton::CloseButton,
    NSWindowButton::MiniaturizeButton,
    NSWindowButton::ZoomButton,
  ] {
    if let Some(button) = ns_window.standardWindowButton(kind) {
      button.setHidden(true);
    }
  }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .plugin(tauri_plugin_window_state::Builder::default().build())
    .plugin(tauri_plugin_opener::init())
    .plugin(tauri_plugin_process::init())
    .plugin(tauri_plugin_updater::Builder::new().build())
    .setup(|app| {
      app.handle().plugin(
        tauri_plugin_log::Builder::default()
          .level(log::LevelFilter::Info)
          .build(),
      )?;

      let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .expect("main window config")
        .clone();
      let nav_handle = app.handle().clone();
      let popup_handle = app.handle().clone();
      let download_handle = app.handle().clone();

      let window = WebviewWindowBuilder::from_config(app.handle(), &config)?
        .on_navigation(move |url| {
          if is_app_url(url) {
            return true;
          }
          if is_external_url(url) {
            let _ = nav_handle.opener().open_url(url.as_str(), None::<&str>);
          }
          false
        })
        .on_new_window(move |url, _features| {
          if is_external_url(&url) {
            let _ = popup_handle.opener().open_url(url.as_str(), None::<&str>);
          }
          NewWindowResponse::Deny
        })
        .on_download(move |_webview, event| {
          if let DownloadEvent::Requested { url, destination } = event {
            if let Ok(dir) = download_handle.path().download_dir() {
              *destination = unique_download_path(dir, destination, &url);
            }
          }
          true
        })
        .build()?;

      #[cfg(target_os = "macos")]
      hide_traffic_lights(&window);
      #[cfg(not(target_os = "macos"))]
      let _ = window;

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
