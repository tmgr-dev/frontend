use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem};
use tauri::{AppHandle, Emitter, Runtime};

use crate::tray;

pub const CHECK_FOR_UPDATES_ID: &str = "check_for_updates";

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
  Ok(menu)
}

pub fn on_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
  if id == CHECK_FOR_UPDATES_ID {
    tray::show_main(app);
    let _ = app.emit("menu://check-for-updates", ());
  }
}
