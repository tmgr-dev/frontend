/// Every app command is behind a permission, granted only to the app's own windows in
/// capabilities/default.json. Without this list, any webview (a plugin window included) could call them.
const APP_COMMANDS: &[&str] = &[
  "capture_screenshot",
  "capture_selection",
  "take_capture",
  "reveal_download",
  "local_db_backup",
  "local_db_execute",
  "local_db_select",
  "local_export_write",
  "local_reveal",
  "local_file_write",
  "local_workspace_create",
  "local_workspace_set_schema",
  "local_workspaces_list",
  "plugins_dev_list",
  "plugins_dev_reveal",
  "plugins_safe_mode",
  "plugin_pick_file",
  "plugin_fetch",
  "hide_quick_add",
  "open_quick_add",
  "take_quick_add",
  "tray_update",
  "plugin_page_put",
  "plugin_window_open",
  "plugin_window_call",
  "plugin_window_reply",
  "plugin_windows_close",
  "plugin_github_release",
  "plugin_install",
  "plugins_installed_list",
  "plugin_uninstall",
];

fn main() {
  tauri_build::try_build(
    tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(APP_COMMANDS)),
  )
  .expect("failed to run tauri-build");
}
