use std::fs;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Runtime, State};

use crate::local_access::LocalAccessState;
use crate::local_db::LocalDbs;
use crate::local_move::{begin_operation, Moving};
use crate::local_workspaces::{self, LocalWorkspace};

const SENTINEL: &str = ".demo";
pub const DEMO_REFUSED: &str = "This is a demo workspace";

pub fn is_demo_dir(dir: &Path) -> bool {
  dir.join(SENTINEL).is_file()
}

pub fn refuse_if_demo(root: &Path, code: &str) -> Result<(), String> {
  if is_demo_dir(&root.join(code)) {
    Err(DEMO_REFUSED.to_owned())
  } else {
    Ok(())
  }
}

pub fn refuse_workspace(workspace: &LocalWorkspace) -> Result<(), String> {
  if workspace.demo {
    Err(DEMO_REFUSED.to_owned())
  } else {
    Ok(())
  }
}

pub fn create_demo_in(root: &Path, name: &str, now: &str) -> Result<LocalWorkspace, String> {
  let created = local_workspaces::create_in(root, name, now)?;
  fs::write(Path::new(&created.path).join(SENTINEL), b"").map_err(|e| e.to_string())?;
  local_workspaces::list_in(root)
    .into_iter()
    .find(|w| w.manifest.code == created.manifest.code)
    .ok_or_else(|| "workspace was not written".to_owned())
}

#[tauri::command]
pub fn local_workspace_create_demo<R: Runtime>(app: AppHandle<R>, name: String) -> Result<LocalWorkspace, String> {
  let created = create_demo_in(&local_workspaces::root(&app)?, &name, &local_workspaces::chrono_like_now())?;
  log::info!("[local] created demo workspace {}", created.manifest.code);
  Ok(created)
}

pub fn trash_dir(dir: &Path) -> Result<(), String> {
  #[allow(unused_mut)]
  let mut context = trash::TrashContext::default();
  #[cfg(target_os = "macos")]
  {
    use trash::macos::{DeleteMethod, TrashContextExtMacos};
    context.set_delete_method(DeleteMethod::NsFileManager);
  }
  context.delete(dir).map_err(|e| e.to_string())
}

pub fn delete_demo_in(
  root: &Path,
  code: &str,
  trash: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<PathBuf, String> {
  let workspace = local_workspaces::list_in(root)
    .into_iter()
    .find(|w| w.manifest.code == code)
    .ok_or_else(|| format!("local workspace {code} not found"))?;
  let dir = PathBuf::from(&workspace.path);
  if !is_demo_dir(&dir) {
    return Err("Only the demo workspace can be deleted this way".to_owned());
  }
  trash(&dir)?;
  Ok(dir)
}

#[tauri::command]
pub async fn local_workspace_delete_demo<R: Runtime>(
  app: AppHandle<R>,
  dbs: State<'_, LocalDbs>,
  moving: State<'_, Moving>,
  access: State<'_, LocalAccessState>,
  code: String,
) -> Result<(), String> {
  let root = local_workspaces::root(&app)?;
  let _operation = begin_operation(&moving, &code, false).await?;
  dbs.forget(&code);
  let tokens = access.token_store.clone();
  let now = crate::tray::now_secs();
  let removed = {
    let (root, code) = (root.clone(), code.clone());
    tauri::async_runtime::spawn_blocking(move || {
      let dir = delete_demo_in(&root, &code, trash_dir)?;
      if let Err(error) = tokens.revoke_all(None, None, Some(&code), now) {
        log::error!("[local] could not revoke local access tokens of {code}: {error}");
      }
      Ok::<_, String>(dir)
    })
    .await
    .map_err(|e| e.to_string())??
  };
  log::info!("[local] moved demo workspace {code} to the trash ({})", removed.display());
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;

  fn temp_root(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tmgr-demo-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
  }

  fn workspace(root: &Path, name: &str, demo: bool) -> PathBuf {
    let ws = local_workspaces::create_in(root, name, "now").unwrap();
    let dir = PathBuf::from(&ws.path);
    if demo {
      fs::write(dir.join(SENTINEL), b"").unwrap();
    }
    dir
  }

  #[test]
  fn only_a_marked_workspace_is_a_demo() {
    let root = temp_root("mark");
    let real = workspace(&root, "Mine", false);
    let demo = workspace(&root, "Demo", true);
    assert!(!is_demo_dir(&real));
    assert!(is_demo_dir(&demo));
    let listed = local_workspaces::list_in(&root);
    assert!(listed.iter().find(|w| w.manifest.code == "demo").unwrap().demo);
    assert!(!listed.iter().find(|w| w.manifest.code == "mine").unwrap().demo);
    assert!(!is_demo_dir(&root.join("missing")));
    assert!(refuse_if_demo(&root, "demo").is_err());
    assert!(refuse_if_demo(&root, "mine").is_ok());
    let _ = fs::remove_dir_all(&root);
  }

  #[test]
  fn a_created_demo_is_marked() {
    let root = temp_root("create");
    let demo = create_demo_in(&root, "Demo: Launch week", "now").unwrap();
    assert!(demo.demo);
    assert_eq!(demo.manifest.code, "demo-launch-week");
    assert!(refuse_if_demo(&root, "demo-launch-week").is_err());
    assert!(refuse_workspace(&demo).is_err());
    let _ = fs::remove_dir_all(&root);
  }

  #[test]
  fn delete_refuses_a_real_workspace_and_trashes_a_demo() {
    let root = temp_root("delete");
    workspace(&root, "Mine", false);
    workspace(&root, "Demo", true);
    let mut trashed = Vec::new();
    assert!(delete_demo_in(&root, "mine", |_| panic!("must not trash a real workspace")).is_err());
    assert!(delete_demo_in(&root, "nope", |_| panic!("unknown")).is_err());
    let dir = delete_demo_in(&root, "demo", |dir| {
      trashed.push(dir.to_path_buf());
      Ok(())
    })
    .unwrap();
    assert_eq!(trashed, vec![dir]);
    let _ = fs::remove_dir_all(&root);
  }
}
