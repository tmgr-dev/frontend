pub mod g2p;
pub mod pack;

#[cfg_attr(not(any(target_os = "macos", target_os = "linux", all(target_os = "windows", target_arch = "x86_64"))), path = "synth_stub.rs")]
mod synth;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::LABEL;

const MAX_TEXT_CHARS: usize = 2000;
const PROGRESS_EVENT: &str = "manager://tts-progress";

#[derive(Default)]
pub struct TtsState {
  stopped_through: Arc<AtomicU64>,
  last_run: AtomicU64,
  engine: Mutex<Option<Arc<synth::Engine>>>,
  install: Mutex<Option<Arc<AtomicBool>>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
  supported: bool,
  installed: bool,
  installing: bool,
  size_bytes: u64,
  sample_rate: i32,
}

#[derive(Serialize)]
pub struct Check {
  ok: bool,
  oov: Vec<String>,
}

#[derive(Clone, Serialize)]
struct Progress {
  received: u64,
  total: u64,
}

fn dir_of<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
  app.path().app_data_dir().map(|d| pack::pack_dir(&d)).map_err(|e| e.to_string())
}

fn engine_of<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<synth::Engine>, String> {
  let state = app.state::<TtsState>();
  let mut slot = state.engine.lock().map_err(|e| e.to_string())?;
  if let Some(engine) = slot.as_ref() {
    return Ok(engine.clone());
  }
  let dir = dir_of(app)?;
  if !pack::is_installed(&dir, &pack::FILES) {
    return Err("the English voice is not installed".to_owned());
  }
  let engine = Arc::new(synth::Engine::load(&dir)?);
  *slot = Some(engine.clone());
  Ok(engine)
}

#[tauri::command]
pub async fn tts_status<R: Runtime>(app: AppHandle<R>) -> Result<Status, String> {
  let dir = dir_of(&app)?;
  let installing = app.state::<TtsState>().install.lock().map(|i| i.is_some()).unwrap_or(false);
  Ok(Status {
    supported: synth::SUPPORTED,
    installed: synth::SUPPORTED && pack::is_installed(&dir, &pack::FILES),
    installing,
    size_bytes: pack::total_size(&pack::FILES),
    sample_rate: 22050,
  })
}

#[tauri::command]
pub async fn tts_install<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
  if !synth::SUPPORTED {
    return Err("English voice is not available on this platform".to_owned());
  }
  let dir = dir_of(&app)?;
  let cancel = Arc::new(AtomicBool::new(false));
  {
    let state = app.state::<TtsState>();
    let mut slot = state.install.lock().map_err(|e| e.to_string())?;
    if slot.is_some() {
      return Err("the English voice is already being installed".to_owned());
    }
    *slot = Some(cancel.clone());
  }
  let last_percent = AtomicU64::new(u64::MAX);
  let emitter = app.clone();
  let result = pack::install(&dir, pack::BASE_URL, &pack::FILES, &cancel, &move |received, total| {
    let percent = (received * 100).checked_div(total).unwrap_or(0);
    if last_percent.swap(percent, Ordering::Relaxed) != percent {
      let _ = emitter.emit_to(LABEL, PROGRESS_EVENT, Progress { received, total });
    }
  })
  .await;
  if let Ok(mut slot) = app.state::<TtsState>().install.lock() {
    *slot = None;
  }
  result
}

#[tauri::command]
pub fn tts_cancel_install<R: Runtime>(app: AppHandle<R>) {
  if let Ok(slot) = app.state::<TtsState>().install.lock() {
    if let Some(cancel) = slot.as_ref() {
      cancel.store(true, Ordering::SeqCst);
    }
  }
}

#[tauri::command]
pub async fn tts_remove<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
  let state = app.state::<TtsState>();
  if state.install.lock().map_err(|e| e.to_string())?.is_some() {
    return Err("the English voice is being installed".to_owned());
  }
  state.stopped_through.fetch_max(state.last_run.load(Ordering::SeqCst), Ordering::SeqCst);
  let dir = dir_of(&app)?;
  state.engine.lock().map_err(|e| e.to_string())?.take();
  tauri::async_runtime::spawn_blocking(move || pack::remove(&dir)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn tts_check<R: Runtime>(app: AppHandle<R>, chunks: Vec<String>) -> Result<Check, String> {
  tauri::async_runtime::spawn_blocking(move || {
    let engine = engine_of(&app)?;
    let mut total = g2p::Phonemized::default();
    for chunk in &chunks {
      let p = engine.phonemize(chunk);
      total.words += p.words;
      total.oov.extend(p.oov);
    }
    Ok(Check { ok: !total.too_many_unknown(), oov: total.oov })
  })
  .await
  .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn tts_synthesize<R: Runtime>(app: AppHandle<R>, text: String, run: u64) -> Result<Response, String> {
  if text.chars().count() > MAX_TEXT_CHARS {
    return Err("the text is too long for one speech request".to_owned());
  }
  let state = app.state::<TtsState>();
  if state.last_run.fetch_max(run, Ordering::SeqCst) > run {
    return Ok(Response::new(Vec::new()));
  }
  let stopped_through = state.stopped_through.clone();
  let samples = tauri::async_runtime::spawn_blocking(move || {
    let engine = engine_of(&app)?;
    engine.synthesize(&text, run, &stopped_through)
  })
  .await
  .map_err(|e| e.to_string())??;
  let mut bytes = Vec::with_capacity(samples.len() * 4);
  for s in samples {
    bytes.extend_from_slice(&s.to_le_bytes());
  }
  Ok(Response::new(bytes))
}

#[tauri::command]
pub fn tts_stop<R: Runtime>(app: AppHandle<R>, run: u64) {
  app.state::<TtsState>().stopped_through.fetch_max(run, Ordering::SeqCst);
}
