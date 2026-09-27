use std::time::Duration;

use tauri::{AppHandle, Emitter, Runtime};

const TICK_MS: u64 = 30_000;

/// Wakes the plugin alarm scheduler even if the main window is hidden or the webview's own timers are
/// throttled: JS cannot be trusted to keep a setInterval running on its own in that state.
pub fn start<R: Runtime>(app: &AppHandle<R>) {
  let handle = app.clone();
  std::thread::spawn(move || loop {
    std::thread::sleep(Duration::from_millis(TICK_MS));
    let _ = handle.emit_to("main", "plugins://tick", ());
  });
}
