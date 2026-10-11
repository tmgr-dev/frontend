use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::tray;

pub const CHECK_FOR_UPDATES_ID: &str = "check_for_updates";
pub const NAV_BACK_ID: &str = "nav_back";
pub const NAV_FORWARD_ID: &str = "nav_forward";

/// Starts from Tauri's default macOS app menu (App/File/Edit/View/Window/Help) so every stock
/// item keeps working, and adds "Check for Updates…" to the top of File.
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
  let menu = Menu::default(app)?;
  let file_menu = menu.items()?.into_iter().find_map(|item| {
    item
      .as_submenu()
      .filter(|submenu| submenu.text().ok().as_deref() == Some("File"))
      .cloned()
  });
  if let Some(file_menu) = file_menu {
    let check_for_updates =
      MenuItemBuilder::with_id(CHECK_FOR_UPDATES_ID, "Check for Updates…").build(app)?;
    file_menu.insert(&PredefinedMenuItem::separator(app)?, 0)?;
    file_menu.insert(&check_for_updates, 0)?;
  }
  let view_menu = menu.items()?.into_iter().find_map(|item| {
    item
      .as_submenu()
      .filter(|submenu| submenu.text().ok().as_deref() == Some("View"))
      .cloned()
  });
  if let Some(view_menu) = view_menu {
    let back = MenuItemBuilder::with_id(NAV_BACK_ID, "Back").build(app)?;
    let forward = MenuItemBuilder::with_id(NAV_FORWARD_ID, "Forward").build(app)?;
    view_menu.insert(&PredefinedMenuItem::separator(app)?, 0)?;
    view_menu.insert(&forward, 0)?;
    view_menu.insert(&back, 0)?;
  }
  Ok(menu)
}

fn nav_event_name(id: &str) -> Option<&'static str> {
  match id {
    NAV_BACK_ID => Some("menu://nav-back"),
    NAV_FORWARD_ID => Some("menu://nav-forward"),
    _ => None,
  }
}

pub fn on_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
  if id == CHECK_FOR_UPDATES_ID {
    tray::show_main(app);
    let _ = app.emit("menu://check-for-updates", ());
  }
  if let Some(event) = nav_event_name(id) {
    let focused = app
      .webview_windows()
      .into_values()
      .find(|window| window.is_focused().unwrap_or(false));
    if let Some(window) = focused {
      let _ = app.emit_to(window.label(), event, ());
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn nav_ids_map_to_events() {
    assert_eq!(nav_event_name(NAV_BACK_ID), Some("menu://nav-back"));
    assert_eq!(nav_event_name(NAV_FORWARD_ID), Some("menu://nav-forward"));
    assert_eq!(nav_event_name(CHECK_FOR_UPDATES_ID), None);
  }
}
