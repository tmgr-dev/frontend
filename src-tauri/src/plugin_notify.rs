use tauri::{AppHandle, Runtime};

const TITLE_MAX: usize = 120;
const BODY_MAX: usize = 400;
pub const OPEN_IN_DECISIONS: &str = "Open in Decisions";

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
  None,
  Plugin(String),
  Decision(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reply {
  Click,
  Button,
  Other,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
  Plugin(String),
  Decision(String),
}

pub fn outcome(target: Target, reply: Reply) -> Option<Outcome> {
  match (target, reply) {
    (Target::Plugin(token), Reply::Click) => Some(Outcome::Plugin(token)),
    (Target::Decision(id), Reply::Click | Reply::Button) => Some(Outcome::Decision(id)),
    _ => None,
  }
}

#[cfg(target_os = "macos")]
mod mac {
  use std::sync::atomic::{AtomicUsize, Ordering};

  use tauri::{AppHandle, Emitter, Runtime};

  use crate::tray;

  use super::{outcome, Outcome, Reply, Target, BODY_MAX, OPEN_IN_DECISIONS, TITLE_MAX};

  /// Caps blocked waiter threads: mac-notification-sys 0.6.15 has no wait timeout of its own.
  const MAX_WAITING_CLICKS: usize = 20;
  static WAITING: AtomicUsize = AtomicUsize::new(0);

  /// Mirrors tauri-plugin-notification's own `set_application` call; ignores `AlreadySet` (it is a shared global).
  fn ensure_application_set<R: Runtime>(app: &AppHandle<R>) {
    let identifier = app.config().identifier.clone();
    let bundle = if tauri::is_dev() { "com.apple.Terminal".to_string() } else { identifier };
    let _ = mac_notification_sys::set_application(&bundle);
  }

  pub fn notify<R: Runtime>(app: AppHandle<R>, title: String, body: String, target: Target) {
    let title: String = title.chars().take(TITLE_MAX).collect();
    let body: String = body.chars().take(BODY_MAX).collect();
    // `try_update` replaces it, but isn't stable at our MSRV (1.89).
    #[allow(deprecated)]
    let wait_for_click = target != Target::None
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
      if matches!(target, Target::Decision(_)) {
        notification.main_button(mac_notification_sys::MainButton::SingleAction(OPEN_IN_DECISIONS));
      }
      notification.wait_for_click(true);
      let response = notification.send();
      WAITING.fetch_sub(1, Ordering::SeqCst);
      let reply = match response {
        Ok(mac_notification_sys::NotificationResponse::Click) => Reply::Click,
        Ok(mac_notification_sys::NotificationResponse::ActionButton(_)) => Reply::Button,
        _ => Reply::Other,
      };
      match outcome(target, reply) {
        Some(Outcome::Plugin(token)) => {
          tray::show_main(&app);
          let _ = app.emit_to("main", "plugins://notification-click", token);
        }
        Some(Outcome::Decision(id)) => crate::decisions::focus_card(&app, &id),
        None => {}
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
  pub fn notify<R: Runtime>(app: AppHandle<R>, title: String, body: String, _target: super::Target) {
    let title: String = title.chars().take(TITLE_MAX).collect();
    let body: String = body.chars().take(BODY_MAX).collect();
    let _ = app.notification().builder().title(title).body(body).show();
  }
}

/// Shows a plugin's notification natively; `token` is opaque and handed back verbatim on a macOS click.
#[tauri::command]
pub fn plugin_notify<R: Runtime>(app: AppHandle<R>, title: String, body: String, token: Option<String>) {
  let target = token.map_or(Target::None, Target::Plugin);
  #[cfg(target_os = "macos")]
  mac::notify(app, title, body, target);
  #[cfg(not(target_os = "macos"))]
  fallback::notify(app, title, body, target);
}

pub fn notify_decision<R: Runtime>(app: AppHandle<R>, title: String, body: String, card_id: String) {
  let target = Target::Decision(card_id);
  #[cfg(target_os = "macos")]
  mac::notify(app, title, body, target);
  #[cfg(not(target_os = "macos"))]
  fallback::notify(app, title, body, target);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_decision_notification_opens_its_card_on_body_click_and_on_the_button() {
    let target = || Target::Decision("c1".to_owned());
    assert_eq!(outcome(target(), Reply::Click), Some(Outcome::Decision("c1".to_owned())));
    assert_eq!(outcome(target(), Reply::Button), Some(Outcome::Decision("c1".to_owned())));
    assert_eq!(outcome(target(), Reply::Other), None);
  }

  #[test]
  fn a_plugin_notification_reacts_only_to_a_body_click() {
    let target = || Target::Plugin("t".to_owned());
    assert_eq!(outcome(target(), Reply::Click), Some(Outcome::Plugin("t".to_owned())));
    assert_eq!(outcome(target(), Reply::Button), None);
    assert_eq!(outcome(Target::None, Reply::Click), None);
  }
}
