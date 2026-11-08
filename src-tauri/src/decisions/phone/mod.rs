mod apply;
mod audit;
mod pairing;
mod pins;
mod poller;
mod projection;
mod relay;

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};
use tmgr_ipc::decisions::Event;

use super::hub::Hub;
use super::push::RouteOutcome;
use audit::Audit;
use pairing::Pending;
use pins::{Keychain, Pins};
use relay::{HttpRelay, Relay};

pub use projection::{eligible, project};

pub const PHONES_LABEL: &str = "decisions-phones";
const MAIN_LABEL: &str = "main";

pub trait Host: Send + Sync {
  fn notify(&self, title: &str, body: &str);
  fn route(&self, card_id: &str, event: &Event, bound: Option<&str>) -> RouteOutcome;
  fn changed(&self);
  fn open_window(&self);
}

pub type Clock = Box<dyn Fn() -> i64 + Send + Sync>;

#[derive(Default)]
struct Synced {
  sig: String,
  at: i64,
  ids: Option<std::collections::HashSet<String>>,
}

pub struct Phones {
  pins: Pins,
  relay: Arc<dyn Relay>,
  hub: Arc<Hub>,
  host: Arc<dyn Host>,
  audit: Audit,
  now: Clock,
  pairing: Mutex<Option<Pending>>,
  synced: Mutex<HashMap<String, Synced>>,
  failures: AtomicU64,
  failed_ids: Mutex<apply::FailedIds>,
  last_alert: AtomicI64,
  dirty_since: AtomicI64,
  kick: (Mutex<bool>, Condvar),
  poll_errors: Mutex<HashMap<String, String>>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct PhoneView {
  pub id: String,
  pub name: String,
  pub fingerprint: String,
  pub paired_at_ms: i64,
  pub last_used_ms: i64,
  pub host: String,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct PairingView {
  pub state: pairing::PairState,
  pub qr_svg_data_url: String,
  pub expires_ms: i64,
  pub attempts_left: u8,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub phone_name: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<String>,
}

impl Phones {
  pub fn new(
    store: Arc<dyn pins::PinStore>,
    relay: Arc<dyn Relay>,
    hub: Arc<Hub>,
    host: Arc<dyn Host>,
    audit: Audit,
    now: Clock,
  ) -> Arc<Phones> {
    Arc::new(Phones {
      pins: Pins::new(store),
      relay,
      hub,
      host,
      audit,
      now,
      pairing: Mutex::new(None),
      synced: Mutex::new(HashMap::new()),
      failures: AtomicU64::new(0),
      failed_ids: Mutex::new(apply::FailedIds::default()),
      last_alert: AtomicI64::new(i64::MIN),
      dirty_since: AtomicI64::new(0),
      kick: (Mutex::new(true), Condvar::new()),
      poll_errors: Mutex::new(HashMap::new()),
    })
  }

  #[cfg(test)]
  pub fn verification_failures(&self) -> u64 {
    self.failures.load(Ordering::SeqCst)
  }

  pub fn mark_dirty(&self) {
    let _ = self.dirty_since.compare_exchange(0, (self.now)().max(1), Ordering::SeqCst, Ordering::SeqCst);
  }

  fn wake(&self) {
    *self.kick.0.lock().unwrap_or_else(|p| p.into_inner()) = true;
    self.kick.1.notify_all();
  }

  pub fn list(&self) -> Result<Vec<PhoneView>, String> {
    let now = (self.now)();
    Ok(
      self
        .pins
        .list()?
        .into_iter()
        .filter(|p| !p.expired(now))
        .map(|p| PhoneView {
          id: p.mailbox,
          name: p.phone_name,
          fingerprint: p.phone_fp,
          paired_at_ms: p.paired_ms,
          last_used_ms: p.last_used_ms,
          host: p.api_base,
        })
        .collect(),
    )
  }

  pub fn revoke(&self, id: &str) -> Result<pins::Pin, String> {
    let pin = self.pins.get(id)?.ok_or_else(|| "that phone is not paired".to_owned())?;
    self.pins.delete(id)?;
    self.synced.lock().unwrap_or_else(|p| p.into_inner()).remove(id);
    self.audit.append((self.now)(), "revoke", &[("mailbox", id), ("phone_fp", &pin.phone_fp)]);
    self.host.changed();
    Ok(pin)
  }
}

struct TauriHost<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Host for TauriHost<R> {
  fn notify(&self, title: &str, body: &str) {
    crate::plugin_notify::plugin_notify(self.0.clone(), title.to_owned(), body.to_owned(), None);
  }

  fn route(&self, card_id: &str, event: &Event, bound: Option<&str>) -> RouteOutcome {
    super::push::route(&self.0, card_id, event, bound)
  }

  fn changed(&self) {
    let _ = self.0.emit_to(PHONES_LABEL, "phones://changed", ());
  }

  fn open_window(&self) {
    open_phones_window(&self.0);
  }
}

pub fn open_phones_window<R: Runtime>(app: &AppHandle<R>) {
  if let Some(window) = app.get_webview_window(PHONES_LABEL) {
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
    return;
  }
  let builder = WebviewWindowBuilder::new(app, PHONES_LABEL, WebviewUrl::App("decisions-phones.html".into()))
    .title(crate::native_i18n::tr(crate::native_i18n::Key::PairedPhones))
    .inner_size(560.0, 760.0)
    .min_inner_size(480.0, 560.0)
    .resizable(true)
    .disable_drag_drop_handler()
    .center();
  match crate::with_app_webview_handlers(builder, app).build() {
    Ok(window) => {
      let _ = window.show();
      let _ = window.set_focus();
    }
    Err(e) => log::error!("[phones] window failed: {e}"),
  }
}

pub fn setup<R: Runtime>(app: &AppHandle<R>, hub: Arc<Hub>, dir: &Path) {
  let phones = Phones::new(
    Arc::new(Keychain),
    Arc::new(HttpRelay),
    hub,
    Arc::new(TauriHost(app.clone())),
    Audit::new(dir.join("phone-audit.jsonl")),
    Box::new(super::store::now_ms),
  );
  app.manage(phones.clone());
  phones.start();
}

pub fn on_hub_change<R: Runtime>(app: &AppHandle<R>) {
  if let Some(p) = app.try_state::<Arc<Phones>>() {
    p.mark_dirty();
  }
}

fn phones_of<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<Phones>, String> {
  app.try_state::<Arc<Phones>>().map(|p| p.inner().clone()).ok_or_else(|| "phone pairing is not running".to_owned())
}

fn main_only<R: Runtime>(window: &tauri::Window<R>) -> Result<(), String> {
  if window.label() == MAIN_LABEL {
    Ok(())
  } else {
    Err("this command is only available to the main window".to_owned())
  }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn phones_begin_pairing<R: Runtime>(
  app: AppHandle<R>,
  window: tauri::Window<R>,
  api_base: String,
  user_id: i64,
  mailbox_id: String,
  desktop_token: String,
  phone_token: String,
  desktop_name: String,
) -> Result<(), String> {
  main_only(&window)?;
  let phones = phones_of(&app)?;
  super::blocking(move || {
    phones.begin_pairing(&api_base, user_id, &mailbox_id, &desktop_token, &phone_token, &desktop_name)
  })
  .await
}

#[tauri::command]
pub fn phones_open_window<R: Runtime>(app: AppHandle<R>, window: tauri::Window<R>) -> Result<(), String> {
  main_only(&window)?;
  open_phones_window(&app);
  Ok(())
}

#[tauri::command]
pub async fn phones_list<R: Runtime>(app: AppHandle<R>) -> Result<Vec<PhoneView>, String> {
  let phones = phones_of(&app)?;
  super::blocking(move || phones.list()).await
}

#[tauri::command]
pub async fn phones_pairing<R: Runtime>(app: AppHandle<R>) -> Result<Option<PairingView>, String> {
  let phones = phones_of(&app)?;
  super::blocking(move || Ok(phones.pairing_view())).await
}

#[tauri::command]
pub async fn phones_confirm<R: Runtime>(app: AppHandle<R>, code: String) -> Result<(), String> {
  let phones = phones_of(&app)?;
  super::blocking(move || phones.confirm(&code)).await
}

#[tauri::command]
pub async fn phones_cancel<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
  let phones = phones_of(&app)?;
  super::blocking(move || {
    phones.cancel();
    Ok(())
  })
  .await
}

#[tauri::command]
pub async fn phones_revoke<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), String> {
  let phones = phones_of(&app)?;
  let pin = super::blocking({
    let phones = phones.clone();
    move || phones.revoke(&id)
  })
  .await?;
  std::thread::spawn(move || phones.cleanup_revoked(&pin));
  Ok(())
}

#[cfg(test)]
mod tests;
