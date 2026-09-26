use std::fs;
use std::io::Read;
use std::path::Path;

use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::DialogExt;

pub const MAX_PICK_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Serialize, Debug)]
pub struct PickedFile {
  name: String,
  size: u64,
  base64: String,
}

/// A plain file of at most MAX_PICK_BYTES, enforced on the read itself.
pub fn read_limited(path: &Path) -> Result<Vec<u8>, String> {
  let file = fs::File::open(path).map_err(|e| e.to_string())?;
  if !file.metadata().map_err(|e| e.to_string())?.is_file() {
    return Err("not a file".into());
  }
  let mut bytes = Vec::new();
  file.take(MAX_PICK_BYTES + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
  if bytes.len() as u64 > MAX_PICK_BYTES {
    return Err("the file is larger than 5 MB".into());
  }
  Ok(bytes)
}

/// The user chooses the file in a system dialog; that choice is the consent. Nothing but the chosen
/// file's name and bytes reaches the plugin, not its path.
#[tauri::command]
pub async fn plugin_pick_file<R: Runtime>(app: AppHandle<R>, title: String) -> Result<Option<PickedFile>, String> {
  let title: String = title.chars().take(120).collect();
  let picked = tauri::async_runtime::spawn_blocking(move || app.dialog().file().set_title(title).blocking_pick_file())
    .await
    .map_err(|e| e.to_string())?;
  let Some(picked) = picked else { return Ok(None) };
  let path = picked.into_path().map_err(|e| e.to_string())?;
  let bytes = tauri::async_runtime::spawn_blocking({
    let path = path.clone();
    move || read_limited(&path)
  })
  .await
  .map_err(|e| e.to_string())??;
  Ok(Some(PickedFile {
    name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
    size: bytes.len() as u64,
    base64: base64::engine::general_purpose::STANDARD.encode(bytes),
  }))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reads_small_files_and_refuses_big_ones_and_folders() {
    let dir = std::env::temp_dir().join(format!("tmgr-pick-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("small.txt"), "hi").unwrap();
    fs::write(dir.join("big.bin"), vec![0u8; (MAX_PICK_BYTES + 1) as usize]).unwrap();
    assert_eq!(read_limited(&dir.join("small.txt")).unwrap(), b"hi");
    assert!(read_limited(&dir.join("big.bin")).is_err());
    assert!(read_limited(&dir).is_err());
    let _ = fs::remove_dir_all(&dir);
  }
}
