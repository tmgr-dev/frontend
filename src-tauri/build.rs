/// Every app command is behind a permission, granted only to the app's own windows in
/// capabilities/default.json. Without this list, any webview (a plugin window included) could call them.
const APP_COMMANDS: &[&str] = &[
  "idle_supported",
  "capture_screenshot",
  "capture_selection",
  "take_capture",
  "reveal_download",
  "local_db_backup",
  "local_db_batch",
  "local_db_execute",
  "local_db_select",
  "local_export_write",
  "local_reveal",
  "local_identity_set",
  "local_identity_clear",
  "local_profile_get",
  "local_profile_create",
  "local_profile_update",
  "local_profile_link",
  "local_profile_unlink",
  "local_backup_now",
  "local_backup_list",
  "local_backup_status",
  "local_backup_reveal",
  "local_file_write",
  "local_move_freeze",
  "local_move_unfreeze",
  "local_source_revision",
  "local_move_snapshot",
  "local_move_build_bundle",
  "local_move_put",
  "local_move_archive",
  "local_workspace_remove",
  "local_workspace_create_demo",
  "local_workspace_delete_demo",
  "local_move_export_file",
  "local_move_pick_export_target",
  "local_move_accept_lost",
  "local_move_clear_decision",
  "local_move_add_refused",
  "local_workspace_create",
  "local_workspace_set_schema",
  "local_workspaces_list",
  "plugins_dev_list",
  "plugins_dev_fingerprints",
  "plugins_dev_reveal",
  "plugins_safe_mode",
  "plugin_pick_file",
  "skills_scan",
  "git_track_update",
  "git_track_pick_folder",
  "skills_pick_folder",
  "skills_read_files",
  "skill_library_read",
  "skill_library_write",
  "skill_library_export",
  "skill_library_import_pick",
  "skill_library_migrate_local",
  "plugin_fetch",
  "plugin_image_fetch",
  "plugin_notify",
  "plugin_open_tg_link",
  "hide_quick_add",
  "open_quick_add",
  "open_task_window",
  "focus_task_window",
  "open_page_window",
  "focus_page_window",
  "take_quick_add",
  "tray_update",
  "dnd_update",
  "mark_update_relaunch",
  "relaunch_after_update",
  "plugin_page_put",
  "plugin_window_open",
  "plugin_window_call",
  "plugin_window_reply",
  "plugin_windows_close",
  "plugin_github_release",
  "plugin_catalog",
  "plugin_catalog_refresh",
  "plugin_install",
  "plugins_installed_list",
  "plugin_uninstall",
  "persona_cache_put",
  "persona_cache_get",
  "persona_cache_clear_for_user",
  "persona_cache_migrate_local",
  "persona_llm_clear_for_user",
  "persona_llm_migrate_local",
  "llm_config_set",
  "llm_config_get",
  "llm_chat",
  "llm_cancel",
  "llm_detect",
  "llm_lan_confirm",
  "llm_models",
  "llm_test",
  "llm_complete",
  "agents_detect",
  "agent_connect_run",
  "local_token_issue",
  "local_token_list",
  "local_token_revoke",
  "local_token_revoke_all",
  "local_token_copy",
  "local_access_status",
  "local_access_set_enabled",
  "local_access_ready",
  "local_access_reply",
  "runner_enabled",
  "runner_allowed_list",
  "runner_set_enabled",
  "runner_status",
  "runner_profiles",
  "runner_sessions",
  "runner_start_run",
  "runner_project_folder",
  "runner_pick_folder",
  "runner_halt",
  "runner_autopilot_status",
  "runner_autopilot_control",
  "runner_open_terminal",
  "runner_watch",
  "runner_unwatch",
  "runner_term_attach",
  "runner_term_ack",
  "runner_term_input",
  "runner_term_resize",
  "runner_term_scroll",
  "runner_term_detach",
  "runner_term_open_link",
  "usage_scan",
  "usage_consent",
  "usage_consent_set",
  "usage_forget",
  "usage_overview",
  "usage_tasks",
  "usage_task",
  "decisions_list",
  "decisions_get",
  "decisions_answer",
  "decisions_message",
  "decisions_diff",
  "decisions_open_link",
  "decisions_pending_task_comments",
  "decisions_mark_task_comment",
  "phones_begin_pairing",
  "phones_open_window",
  "phones_list",
  "phones_pairing",
  "phones_confirm",
  "phones_cancel",
  "phones_revoke",
];

fn build_widget_reload() {
  use std::path::PathBuf;
  use std::process::Command;

  println!("cargo:rerun-if-changed=widget-reload/TMGRWidgetReload.swift");
  println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");
  println!("cargo:rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");
  let min = std::env::var("MACOSX_DEPLOYMENT_TARGET").unwrap_or_else(|_| "13.3".into());
  let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
    Ok("aarch64") => "arm64",
    _ => "x86_64",
  };
  let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
  let lib = out.join("libtmgr_widget_reload.a");
  let status = Command::new("xcrun")
    .args(["swiftc", "-emit-library", "-static", "-parse-as-library", "-module-name", "TMGRWidgetReload"])
    .args(["-target", &format!("{arch}-apple-macos{min}")])
    .args(["-Xfrontend", "-disable-autolink-framework", "-Xfrontend", "WidgetKit"])
    .arg("widget-reload/TMGRWidgetReload.swift")
    .arg("-o")
    .arg(&lib)
    .status()
    .expect("failed to run swiftc");
  assert!(status.success(), "swiftc failed");
  let sdk = Command::new("xcrun").args(["--show-sdk-path"]).output().expect("xcrun failed");
  let sdk = String::from_utf8(sdk.stdout).unwrap();
  println!("cargo:rustc-link-search=native={}", out.display());
  println!("cargo:rustc-link-search=native={}/usr/lib/swift", sdk.trim());
  println!("cargo:rustc-link-lib=static=tmgr_widget_reload");
  println!("cargo:rustc-link-arg=-Wl,-weak_framework,WidgetKit");
  println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
}

fn main() {
  if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
    build_widget_reload();
  }
  tauri_build::try_build(
    tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(APP_COMMANDS)),
  )
  .expect("failed to run tauri-build");
}
