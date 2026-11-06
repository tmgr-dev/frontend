use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{WebviewUrl, WebviewWindowBuilder};

macro_rules! probes {
  ($($name:ident),*) => {
    $(#[tauri::command] fn $name() -> bool { true })*
    fn handler() -> impl Fn(tauri::ipc::Invoke<tauri::test::MockRuntime>) -> bool + Send + Sync + 'static {
      tauri::generate_handler![$($name),*]
    }
  };
}
probes!(
  idle_supported,
  runner_enabled,
  llm_chat,
  local_access_event,
  decisions_list,
  persona_cache_clear_for_user,
  persona_llm_clear_for_user,
  skill_library_read,
  skill_library_write,
  skill_library_export,
  skill_library_import_pick,
  skill_library_migrate_local
);

fn call(label: &str, cmd: &str) -> Result<(), String> {
  let app = mock_builder().invoke_handler(handler()).build(tauri::generate_context!()).unwrap();
  let window = WebviewWindowBuilder::new(&app, label, WebviewUrl::default()).build().unwrap();
  let request = InvokeRequest {
    cmd: cmd.into(),
    callback: CallbackFn(0),
    error: CallbackFn(1),
    url: "tauri://localhost".parse().unwrap(),
    body: InvokeBody::default(),
    headers: Default::default(),
    invoke_key: INVOKE_KEY.into(),
  };
  get_ipc_response(&window, request).map(|_| ()).map_err(|e| e.to_string())
}

#[test]
fn the_acl_decides_per_command_and_window() {
  assert!(call("main", "idle_supported").is_ok());
  assert!(call("main", "runner_enabled").is_ok());
  assert!(call("task-1", "llm_chat").is_ok());
  assert!(call("task-1", "runner_enabled").unwrap_err().contains("not allowed on window"));
  assert!(call("page-1", "llm_chat").unwrap_err().contains("not allowed on window"));
  assert!(call("decisions", "decisions_list").is_ok());
  assert!(call("main", "decisions_list").unwrap_err().contains("not allowed on window"));
  assert!(call("task-1", "decisions_list").unwrap_err().contains("not allowed on window"));
  assert!(call("decisions", "runner_enabled").unwrap_err().contains("not allowed on window"));
  assert!(call("main", "local_access_event").unwrap_err().contains("Command not found"));

  for command in [
    "skill_library_read",
    "skill_library_write",
    "skill_library_export",
    "skill_library_import_pick",
    "skill_library_migrate_local",
  ] {
    assert!(call("main", command).is_ok(), "{command}");
    assert!(call("task-1", command).unwrap_err().contains("not allowed on window"), "{command}");
    assert!(call("decisions", command).unwrap_err().contains("not allowed on window"), "{command}");
  }
}

#[test]
fn sign_out_cleanup_is_allowed_for_the_main_window_only() {
  for cmd in ["persona_cache_clear_for_user", "persona_llm_clear_for_user"] {
    assert!(call("main", cmd).is_ok(), "{cmd} on main");
    for label in ["task-1", "page-1", "decisions", "quick-add"] {
      assert!(call(label, cmd).unwrap_err().contains("not allowed on window"), "{cmd} on {label}");
    }
  }
}
