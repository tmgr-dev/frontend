use tauri::{AppHandle, Runtime};

const TITLE_MAX: usize = 120;
const BODY_MAX: usize = 400;

#[cfg(target_os = "macos")]
mod mac {
  use std::sync::atomic::{AtomicUsize, Ordering};

  use tauri::{AppHandle, Emitter, Runtime};

  use crate::tray;

  use super::{BODY_MAX, TITLE_MAX};

  /// Caps blocked waiter threads: mac-notification-sys 0.6.15 has no wait timeout of its own.
  const MAX_WAITING_CLICKS: usize = 20;
  static WAITING: AtomicUsize = AtomicUsize::new(0);

  /// Mirrors tauri-plugin-notification's own `set_application` call; ignores `AlreadySet` (it is a shared global).
  fn ensure_application_set<R: Runtime>(app: &AppHandle<R>) {
    let identifier = app.config().identifier.clone();
    let bundle = if tauri::is_dev() { "com.apple.Terminal".to_string() } else { identifier };
    let _ = mac_notification_sys::set_application(&bundle);
  }

  pub fn notify<R: Runtime>(app: AppHandle<R>, title: String, body: String, token: Option<String>) {
    let title: String = title.chars().take(TITLE_MAX).collect();
    let body: String = body.chars().take(BODY_MAX).collect();
    let wait_for_click = token.is_some()
      && WAITING
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| (n < MAX_WAITING_CLICKS).then_some(n + 1))
        .is_ok();
    std::thread::spawn(move || {
      ensure_application_set(&app);
      let mut notification = mac_notification_sys::Notification::new();
      notification.title(&title).message(&body);
      if !wait_for_click {
        let _ = notification.send();
        return;
      }
      notification.wait_for_click(true);
      let response = notification.send();
      WAITING.fetch_sub(1, Ordering::SeqCst);
      if let (Ok(mac_notification_sys::NotificationResponse::Click), Some(token)) = (response, token) {
        tray::show_main(&app);
        let _ = app.emit_to("main", "plugins://notification-click", token);
      }
    });
  }
}

#[cfg(not(target_os = "macos"))]
mod fallback {
  use tauri::{AppHandle, Runtime};
  use tauri_plugin_notification::NotificationExt;

  use super::{BODY_MAX, TITLE_MAX};

  /// No click path outside macOS yet: shown without an action, same as any other desktop notification.
  pub fn notify<R: Runtime>(app: AppHandle<R>, title: String, body: String, _token: Option<String>) {
    let title: String = title.chars().take(TITLE_MAX).collect();
    let body: String = body.chars().take(BODY_MAX).collect();
    let _ = app.notification().builder().title(title).body(body).show();
  }
}

/// Shows a plugin's notification natively; `token` is opaque and handed back verbatim on a macOS click.
#[tauri::command]
pub fn plugin_notify<R: Runtime>(app: AppHandle<R>, title: String, body: String, token: Option<String>) {
  #[cfg(target_os = "macos")]
  mac::notify(app, title, body, token);
  #[cfg(not(target_os = "macos"))]
  fallback::notify(app, title, body, token);
}
