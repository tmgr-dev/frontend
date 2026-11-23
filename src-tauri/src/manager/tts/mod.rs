pub mod external;
pub mod g2p;
pub mod host;
pub mod pack;
pub mod ru;
pub mod ru_rules;
pub mod wav;

#[cfg_attr(not(any(target_os = "macos", target_os = "linux", all(target_os = "windows", target_arch = "x86_64"))), path = "synth_stub.rs")]
mod synth;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::LABEL;
use pack::Lang;

const MAX_TEXT_CHARS: usize = 2000;
const PROGRESS_EVENT: &str = "manager://tts-progress";

#[derive(Default)]
pub struct TtsState {
  stopped_through: Arc<AtomicU64>,
  last_run: AtomicU64,
  engines: Mutex<HashMap<Lang, Arc<synth::Engine>>>,
  install: Mutex<Option<(Lang, Arc<AtomicBool>)>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
  lang: &'static str,
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
  lang: &'static str,
  received: u64,
  total: u64,
}

fn dir_of<R: Runtime>(app: &AppHandle<R>, lang: Lang) -> Result<PathBuf, String> {
  app.path().app_data_dir().map(|d| pack::pack_dir(&d, lang)).map_err(|e| e.to_string())
}

fn engine_of<R: Runtime>(app: &AppHandle<R>, lang: Lang) -> Result<Arc<synth::Engine>, String> {
  let state = app.state::<TtsState>();
  if let Some(engine) = state.engines.lock().map_err(|e| e.to_string())?.get(&lang) {
    return Ok(engine.clone());
  }
  let dir = dir_of(app, lang)?;
  if !pack::is_installed(&dir, pack::spec(lang).files) {
    return Err(format!("the {} voice is not installed", lang.label()));
  }
  let loaded = Arc::new(synth::Engine::load(&dir, lang)?);
  let mut engines = state.engines.lock().map_err(|e| e.to_string())?;
  Ok(engines.entry(lang).or_insert(loaded).clone())
}

fn english_for_latin<R: Runtime>(app: &AppHandle<R>, lang: Lang, text: &str) -> Option<Arc<synth::Engine>> {
  if lang == Lang::En || !text.chars().any(|c| c.is_ascii_alphabetic()) {
    return None;
  }
  engine_of(app, Lang::En).ok()
}

#[tauri::command]
pub async fn tts_status<R: Runtime>(app: AppHandle<R>, lang: Option<String>) -> Result<Status, String> {
  let lang = Lang::parse(lang.as_deref())?;
  let spec = pack::spec(lang);
  let dir = dir_of(&app, lang)?;
  let installing = app.state::<TtsState>().install.lock().map(|i| i.as_ref().is_some_and(|(l, _)| *l == lang)).unwrap_or(false);
  Ok(Status {
    lang: lang.code(),
    supported: synth::SUPPORTED,
    installed: synth::SUPPORTED && pack::is_installed(&dir, spec.files),
    installing,
    size_bytes: pack::total_size(spec.files),
    sample_rate: spec.sample_rate,
  })
}

#[tauri::command]
pub async fn tts_install<R: Runtime>(app: AppHandle<R>, lang: Option<String>) -> Result<(), String> {
  let lang = Lang::parse(lang.as_deref())?;
  if !synth::SUPPORTED {
    return Err("voices are not available on this platform".to_owned());
  }
  let spec = pack::spec(lang);
  let dir = dir_of(&app, lang)?;
  let cancel = Arc::new(AtomicBool::new(false));
  {
    let state = app.state::<TtsState>();
    let mut slot = state.install.lock().map_err(|e| e.to_string())?;
    if slot.is_some() {
      return Err("a voice is already being installed".to_owned());
    }
    *slot = Some((lang, cancel.clone()));
  }
  let last_percent = AtomicU64::new(u64::MAX);
  let emitter = app.clone();
  let result = pack::install(&dir, spec.base_url, spec.files, &cancel, &move |received, total| {
    let percent = (received * 100).checked_div(total).unwrap_or(0);
    if last_percent.swap(percent, Ordering::Relaxed) != percent {
      let _ = emitter.emit_to(LABEL, PROGRESS_EVENT, Progress { lang: lang.code(), received, total });
    }
  })
  .await;
  if let Ok(mut slot) = app.state::<TtsState>().install.lock() {
    *slot = None;
  }
  result
}

#[tauri::command]
pub fn tts_cancel_install<R: Runtime>(app: AppHandle<R>, lang: Option<String>) {
  let Ok(lang) = Lang::parse(lang.as_deref()) else { return };
  if let Ok(slot) = app.state::<TtsState>().install.lock() {
    if let Some((installing, cancel)) = slot.as_ref() {
      if *installing == lang {
        cancel.store(true, Ordering::SeqCst);
      }
    }
  }
}

#[tauri::command]
pub async fn tts_remove<R: Runtime>(app: AppHandle<R>, lang: Option<String>) -> Result<(), String> {
  let lang = Lang::parse(lang.as_deref())?;
  let state = app.state::<TtsState>();
  if state.install.lock().map_err(|e| e.to_string())?.is_some() {
    return Err("a voice is being installed".to_owned());
  }
  state.stopped_through.fetch_max(state.last_run.load(Ordering::SeqCst), Ordering::SeqCst);
  let dir = dir_of(&app, lang)?;
  state.engines.lock().map_err(|e| e.to_string())?.remove(&lang);
  tauri::async_runtime::spawn_blocking(move || pack::remove(&dir)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn tts_check<R: Runtime>(app: AppHandle<R>, chunks: Vec<String>, lang: Option<String>) -> Result<Check, String> {
  let lang = Lang::parse(lang.as_deref())?;
  tauri::async_runtime::spawn_blocking(move || {
    let engine = engine_of(&app, lang)?;
    let latin = english_for_latin(&app, lang, &chunks.join(" "));
    let mut total = g2p::Phonemized::default();
    for chunk in &chunks {
      let p = engine.phonemize(chunk, latin.as_ref().and_then(|e| e.english()));
      total.words += p.words;
      total.oov.extend(p.oov);
      total.guessed.extend(p.guessed);
    }
    Ok(Check { ok: !total.too_many_unknown(), oov: total.oov })
  })
  .await
  .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn tts_synthesize<R: Runtime>(app: AppHandle<R>, text: String, run: u64, lang: Option<String>) -> Result<Response, String> {
  let lang = Lang::parse(lang.as_deref())?;
  if text.chars().count() > MAX_TEXT_CHARS {
    return Err("the text is too long for one speech request".to_owned());
  }
  let state = app.state::<TtsState>();
  if state.last_run.fetch_max(run, Ordering::SeqCst) > run {
    return Ok(Response::new(Vec::new()));
  }
  let stopped_through = state.stopped_through.clone();
  let samples = tauri::async_runtime::spawn_blocking(move || {
    let engine = engine_of(&app, lang)?;
    let latin = english_for_latin(&app, lang, &text);
    engine.synthesize(&text, run, &stopped_through, latin.as_ref().and_then(|e| e.english()))
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

#[cfg(test)]
mod tests {
  #[test]
  fn every_tts_command_is_granted_to_the_manager_window() {
    let build = include_str!("../../../build.rs");
    let capability = include_str!("../../../capabilities/manager.json");
    let commands: Vec<&str> = build.lines().filter_map(|l| l.trim().strip_prefix('"')).filter_map(|l| l.strip_suffix("\",")).filter(|c| c.starts_with("tts_")).collect();
    assert!(commands.len() >= 12, "{commands:?}");
    for command in commands {
      let permission = format!("\"allow-{}\"", command.replace('_', "-"));
      assert!(capability.contains(&permission), "{permission} missing in manager.json");
    }
  }
}
