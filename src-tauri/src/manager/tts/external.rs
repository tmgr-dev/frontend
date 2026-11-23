use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, Runtime};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use super::{host, wav, TtsState, MAX_TEXT_CHARS};

const FILE_NAME: &str = "tts-external.json";
const TEST_SENTENCE: &str = "This is a test of the external voice.";
const MAX_TEST_CHARS: usize = 300;
const MAX_AUDIO_BYTES: usize = 30 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 2048;
const MAX_ARGS: usize = 64;
const MAX_ARG_BYTES: usize = 4096;
const POLL: Duration = Duration::from_millis(50);
const HTTP_TOTAL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HttpSettings {
  pub base_url: String,
  pub model: String,
  pub voice: String,
  pub allow_remote: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CommandSettings {
  pub argv: Vec<String>,
  pub timeout_secs: u64,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExternalSettings {
  pub mode: String,
  pub http: HttpSettings,
  pub command: CommandSettings,
}

impl Default for CommandSettings {
  fn default() -> Self {
    Self { argv: Vec::new(), timeout_secs: 30 }
  }
}

impl Default for ExternalSettings {
  fn default() -> Self {
    Self { mode: "off".to_owned(), http: HttpSettings::default(), command: CommandSettings::default() }
  }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalHealth {
  pub configured: bool,
  pub mode: String,
  pub healthy: bool,
  pub last_error: Option<String>,
}

pub struct Health {
  healthy: bool,
  last_error: Option<String>,
}

impl Health {
  const fn new() -> Self {
    Self { healthy: true, last_error: None }
  }

  fn mark_healthy(&mut self) {
    self.healthy = true;
    self.last_error = None;
  }

  fn mark_failed(&mut self, error: String) {
    self.healthy = false;
    self.last_error = Some(error);
  }

  fn record<T>(&mut self, outcome: &Result<Option<T>, String>) {
    match outcome {
      Ok(Some(_)) => self.mark_healthy(),
      Ok(None) => {}
      Err(e) => self.mark_failed(e.clone()),
    }
  }
}

static HEALTH: Mutex<Health> = Mutex::new(Health::new());

fn with_health(f: impl FnOnce(&mut Health)) {
  if let Ok(mut health) = HEALTH.lock() {
    f(&mut health);
  }
}

fn validate_argv(argv: &[String]) -> Result<(), String> {
  if argv.is_empty() {
    return Err("the command is empty".to_owned());
  }
  if argv.len() > MAX_ARGS {
    return Err(format!("the command has more than {MAX_ARGS} arguments"));
  }
  if !Path::new(&argv[0]).is_absolute() {
    return Err("the program must be given as an absolute path".to_owned());
  }
  for arg in argv {
    if arg.len() >= MAX_ARG_BYTES {
      return Err("a command argument is too long".to_owned());
    }
    if arg.contains('\0') {
      return Err("a command argument contains a NUL character".to_owned());
    }
  }
  Ok(())
}

pub fn validate(settings: &ExternalSettings) -> Result<ExternalSettings, String> {
  let mut out = settings.clone();
  out.command.timeout_secs = out.command.timeout_secs.clamp(1, 120);
  out.http.base_url = out.http.base_url.trim().to_owned();
  match out.mode.as_str() {
    "off" => {}
    "http" if out.http.base_url.is_empty() => return Err("enter the server address".to_owned()),
    "command" if out.command.argv.is_empty() => return Err("enter the command".to_owned()),
    "http" | "command" => {}
    other => return Err(format!("unknown external voice mode: {other}")),
  }
  if !out.http.base_url.is_empty() {
    host::parse_base(&out.http.base_url, out.http.allow_remote)?;
  }
  if !out.command.argv.is_empty() {
    validate_argv(&out.command.argv)?;
  }
  Ok(out)
}

fn settings_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
  app.path().app_data_dir().map(|d| d.join(FILE_NAME)).map_err(|e| e.to_string())
}

fn load_from(path: &Path) -> (ExternalSettings, Option<String>) {
  let bytes = match std::fs::read(path) {
    Ok(b) => b,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (ExternalSettings::default(), None),
    Err(e) => return (ExternalSettings::default(), Some(format!("the external voice settings could not be read: {e}"))),
  };
  let parsed = serde_json::from_slice::<ExternalSettings>(&bytes).map_err(|e| e.to_string()).and_then(|s| validate(&s));
  match parsed {
    Ok(settings) => (settings, None),
    Err(e) => (ExternalSettings::default(), Some(format!("the saved external voice settings were rejected and the voice is turned off: {e}"))),
  }
}

fn save_to(path: &Path, settings: &ExternalSettings) -> Result<(), String> {
  if let Some(dir) = path.parent() {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
  }
  let tmp = path.with_extension("json.tmp");
  let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
  std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
  std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn quoted(argv: &[String]) -> String {
  argv.iter().map(|a| format!("{a:?}")).collect::<Vec<_>>().join(" ")
}

fn confirmation(old: &ExternalSettings, new: &ExternalSettings) -> Option<String> {
  let mut parts = Vec::new();
  if new.command.argv != old.command.argv && !new.command.argv.is_empty() {
    parts.push(format!("TMGR will run this program on your computer and pass the text of chat messages to it:\n\n{}", quoted(&new.command.argv)));
  }
  if new.http.allow_remote {
    let host = host::remote_host(&new.http.base_url);
    let turned_on = !old.http.allow_remote;
    if turned_on || (host.is_some() && host != host::remote_host(&old.http.base_url)) {
      parts.push(format!("Remote servers are allowed. The text of chat messages will be sent to:\n\n{}", new.http.base_url));
    }
  }
  if parts.is_empty() {
    None
  } else {
    parts.push("Only continue if you trust this.".to_owned());
    Some(parts.join("\n\n"))
  }
}

async fn wait_stopped(stopped: &AtomicU64, run: u64) {
  while stopped.load(Ordering::SeqCst) < run {
    tokio::time::sleep(POLL).await;
  }
}

async fn interruptible<T>(stopped: &AtomicU64, run: u64, work: impl Future<Output = Result<T, String>>) -> Result<Option<T>, String> {
  tokio::select! {
    r = work => r.map(Some),
    () = wait_stopped(stopped, run) => Ok(None),
  }
}

fn http_client(loopback: bool) -> Result<reqwest::Client, String> {
  if rustls::crypto::CryptoProvider::get_default().is_none() {
    let _ = rustls::crypto::ring::default_provider().install_default();
  }
  let local = [std::net::SocketAddr::from(([127, 0, 0, 1], 0)), std::net::SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, 0))];
  reqwest::Client::builder()
    .no_proxy()
    .redirect(reqwest::redirect::Policy::none())
    .connect_timeout(Duration::from_secs(if loopback { 2 } else { 10 }))
    .timeout(HTTP_TOTAL_TIMEOUT)
    .user_agent("TMGR-desktop-external-voice")
    .resolve_to_addrs("localhost", &local)
    .build()
    .map_err(|e| e.to_string())
}

async fn http_fetch(cfg: &HttpSettings, text: &str, limit: usize) -> Result<Vec<u8>, String> {
  let url = host::endpoint(&cfg.base_url, cfg.allow_remote)?;
  let mut body = serde_json::Map::new();
  if !cfg.model.is_empty() {
    body.insert("model".to_owned(), cfg.model.clone().into());
  }
  body.insert("input".to_owned(), text.into());
  if !cfg.voice.is_empty() {
    body.insert("voice".to_owned(), cfg.voice.clone().into());
  }
  body.insert("response_format".to_owned(), "wav".into());
  let payload = serde_json::to_vec(&body).map_err(|e| e.to_string())?;
  let client = http_client(host::is_loopback(&url))?;
  let mut response = client
    .post(url)
    .header(reqwest::header::CONTENT_TYPE, "application/json")
    .body(payload)
    .send()
    .await
    .map_err(|e| format!("the voice server could not be reached: {e}"))?;
  if !response.status().is_success() {
    return Err(format!("the voice server answered with HTTP {}", response.status()));
  }
  let mut bytes = Vec::new();
  while let Some(chunk) = response.chunk().await.map_err(|e| format!("the voice server connection failed: {e}"))? {
    if bytes.len() + chunk.len() > limit {
      return Err(too_much_audio());
    }
    bytes.extend_from_slice(&chunk);
  }
  Ok(bytes)
}

fn too_much_audio() -> String {
  "the voice returned more than 30 MB of audio".to_owned()
}

async fn read_capped(mut reader: impl AsyncRead + Unpin, limit: usize) -> Result<Vec<u8>, String> {
  let mut out = Vec::new();
  let mut buf = vec![0u8; 64 * 1024];
  loop {
    let n = reader.read(&mut buf).await.map_err(|e| e.to_string())?;
    if n == 0 {
      return Ok(out);
    }
    if out.len() + n > limit {
      return Err(too_much_audio());
    }
    out.extend_from_slice(&buf[..n]);
  }
}

async fn read_stderr(mut reader: impl AsyncRead + Unpin) -> Vec<u8> {
  let mut kept = Vec::new();
  let mut buf = vec![0u8; 8 * 1024];
  loop {
    match reader.read(&mut buf).await {
      Ok(0) | Err(_) => return kept,
      Ok(n) => {
        let room = MAX_STDERR_BYTES.saturating_sub(kept.len());
        kept.extend_from_slice(&buf[..n.min(room)]);
      }
    }
  }
}

async fn kill_tree(child: &mut tokio::process::Child, pid: Option<u32>) {
  #[cfg(unix)]
  if let Some(pgid) = pid.and_then(|p| i32::try_from(p).ok()) {
    unsafe {
      libc::kill(-pgid, libc::SIGKILL);
    }
  }
  #[cfg(not(unix))]
  let _ = pid;
  let _ = child.start_kill();
  let _ = child.wait().await;
}

enum Outcome {
  Finished(Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), String>),
  TimedOut,
  Stopped,
}

async fn run_command(cfg: &CommandSettings, text: &str, timeout: Duration, stopped: &AtomicU64, run: u64, limit: usize) -> Result<Option<Vec<u8>>, String> {
  if stopped.load(Ordering::SeqCst) >= run {
    return Ok(None);
  }
  validate_argv(&cfg.argv)?;
  let mut command = tokio::process::Command::new(&cfg.argv[0]);
  command.args(&cfg.argv[1..]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
  #[cfg(unix)]
  command.process_group(0);
  #[cfg(windows)]
  command.creation_flags(0x0800_0000);
  let mut child = command.spawn().map_err(|e| format!("the command could not be started: {e}"))?;
  let pid = child.id();
  let (Some(mut stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
    kill_tree(&mut child, pid).await;
    return Err("the command pipes could not be opened".to_owned());
  };
  let outcome = {
    let io = async {
      let write = async move {
        match stdin.write_all(text.as_bytes()).await {
          Err(e) if e.kind() != std::io::ErrorKind::BrokenPipe => Err(e.to_string()),
          _ => Ok(()),
        }
      };
      let err = async { Ok::<_, String>(read_stderr(stderr).await) };
      let (_, out, err) = tokio::try_join!(write, read_capped(stdout, limit), err)?;
      let status = child.wait().await.map_err(|e| e.to_string())?;
      Ok((status, out, err))
    };
    tokio::select! {
      r = io => Outcome::Finished(r),
      () = tokio::time::sleep(timeout) => Outcome::TimedOut,
      () = wait_stopped(stopped, run) => Outcome::Stopped,
    }
  };
  match outcome {
    Outcome::Finished(Ok((status, out, _))) if status.success() => {
      kill_tree(&mut child, pid).await;
      Ok(Some(out))
    }
    Outcome::Finished(Ok((status, _, err))) => {
      kill_tree(&mut child, pid).await;
      let err = String::from_utf8_lossy(&err);
      let err = err.trim();
      Err(if err.is_empty() { format!("the command failed ({status})") } else { format!("the command failed ({status}): {err}") })
    }
    Outcome::Finished(Err(e)) => {
      kill_tree(&mut child, pid).await;
      Err(e)
    }
    Outcome::TimedOut => {
      kill_tree(&mut child, pid).await;
      Err(format!("the command did not finish within {} seconds", timeout.as_secs()))
    }
    Outcome::Stopped => {
      kill_tree(&mut child, pid).await;
      Ok(None)
    }
  }
}

fn payload(rate: u32, samples: &[f32]) -> Vec<u8> {
  let mut bytes = Vec::with_capacity(4 + samples.len() * 4);
  bytes.extend_from_slice(&rate.to_le_bytes());
  for s in samples {
    bytes.extend_from_slice(&s.to_le_bytes());
  }
  bytes
}

async fn synthesize_with(settings: &ExternalSettings, text: &str, stopped: &AtomicU64, run: u64, limit: usize) -> Result<Option<Vec<u8>>, String> {
  if stopped.load(Ordering::SeqCst) >= run {
    return Ok(None);
  }
  let raw = match settings.mode.as_str() {
    "http" => interruptible(stopped, run, http_fetch(&settings.http, text, limit)).await?,
    "command" => run_command(&settings.command, text, Duration::from_secs(settings.command.timeout_secs), stopped, run, limit).await?,
    _ => return Err("the external voice is not turned on".to_owned()),
  };
  let Some(raw) = raw else { return Ok(None) };
  let (rate, samples) = wav::parse(&raw)?;
  Ok(Some(payload(rate, &samples)))
}

fn load<R: Runtime>(app: &AppHandle<R>) -> Result<(ExternalSettings, Option<String>), String> {
  Ok(load_from(&settings_path(app)?))
}

#[tauri::command]
pub async fn tts_settings_get<R: Runtime>(app: AppHandle<R>) -> Result<ExternalSettings, String> {
  Ok(load(&app)?.0)
}

#[tauri::command]
pub async fn tts_settings_set<R: Runtime>(app: AppHandle<R>, settings: ExternalSettings) -> Result<ExternalSettings, String> {
  use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
  let new = validate(&settings)?;
  let path = settings_path(&app)?;
  let (old, _) = load_from(&path);
  if let Some(message) = confirmation(&old, &new) {
    let dialog_app = app.clone();
    let confirmed = tauri::async_runtime::spawn_blocking(move || {
      dialog_app
        .dialog()
        .message(message)
        .title("TMGR external voice")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom("Allow".to_owned(), "Cancel".to_owned()))
        .blocking_show()
    })
    .await
    .unwrap_or(false);
    if !confirmed {
      return Err("cancelled".to_owned());
    }
  }
  save_to(&path, &new)?;
  with_health(Health::mark_healthy);
  Ok(new)
}

async fn synthesize_checked<R: Runtime>(app: &AppHandle<R>, text: &str, stopped: &AtomicU64, run: u64) -> Result<Vec<u8>, String> {
  let (settings, load_error) = load(app)?;
  if let Some(e) = load_error {
    return Err(e);
  }
  let outcome = synthesize_with(&settings, text, stopped, run, MAX_AUDIO_BYTES).await;
  with_health(|h| h.record(&outcome));
  outcome.map(Option::unwrap_or_default)
}

#[tauri::command]
pub async fn tts_external_synthesize<R: Runtime>(app: AppHandle<R>, text: String, run: u64) -> Result<Response, String> {
  if text.chars().count() > MAX_TEXT_CHARS {
    return Err("the text is too long for one speech request".to_owned());
  }
  let state = app.state::<TtsState>();
  if state.last_run.fetch_max(run, Ordering::SeqCst) > run || run <= state.stopped_through.load(Ordering::SeqCst) {
    return Ok(Response::new(Vec::new()));
  }
  let stopped = state.stopped_through.clone();
  Ok(Response::new(synthesize_checked(&app, &text, &stopped, run).await?))
}

#[tauri::command]
pub async fn tts_external_test<R: Runtime>(app: AppHandle<R>, text: Option<String>) -> Result<Response, String> {
  let never = AtomicU64::new(0);
  let phrase = text.map(|t| t.trim().chars().take(MAX_TEST_CHARS).collect::<String>()).filter(|t| !t.is_empty());
  let result = synthesize_checked(&app, phrase.as_deref().unwrap_or(TEST_SENTENCE), &never, 1).await;
  if let Err(e) = &result {
    with_health(|h| h.mark_failed(e.clone()));
  }
  Ok(Response::new(result?))
}

#[tauri::command]
pub async fn tts_external_health<R: Runtime>(app: AppHandle<R>) -> Result<ExternalHealth, String> {
  let (settings, load_error) = load(&app)?;
  let (healthy, last_error) = HEALTH.lock().map(|h| (h.healthy, h.last_error.clone())).unwrap_or((true, None));
  Ok(ExternalHealth {
    configured: settings.mode != "off",
    mode: settings.mode,
    healthy: healthy && load_error.is_none(),
    last_error: load_error.or(last_error),
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio::net::TcpListener;

  fn wav_bytes(samples: &[i16]) -> Vec<u8> {
    let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let mut w = b"RIFF".to_vec();
    w.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    for v in [1u16, 1] {
      w.extend_from_slice(&v.to_le_bytes());
    }
    w.extend_from_slice(&22050u32.to_le_bytes());
    w.extend_from_slice(&44100u32.to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(data.len() as u32).to_le_bytes());
    w.extend_from_slice(&data);
    w
  }

  fn expected_payload() -> Vec<u8> {
    payload(22050, &[0.0, 0.5])
  }

  fn command_settings(argv: Vec<String>) -> ExternalSettings {
    ExternalSettings { mode: "command".to_owned(), command: CommandSettings { argv, timeout_secs: 30 }, ..ExternalSettings::default() }
  }

  fn http_settings(port: u16) -> ExternalSettings {
    ExternalSettings {
      mode: "http".to_owned(),
      http: HttpSettings { base_url: format!("http://127.0.0.1:{port}"), model: "m1".to_owned(), voice: "v1".to_owned(), allow_remote: false },
      ..ExternalSettings::default()
    }
  }

  #[test]
  fn defaults_and_partial_json() {
    let d = ExternalSettings::default();
    assert_eq!((d.mode.as_str(), d.command.timeout_secs, d.http.allow_remote), ("off", 30, false));
    let s: ExternalSettings = serde_json::from_str(r#"{"mode":"http","http":{"baseUrl":"http://localhost:1"}}"#).unwrap();
    assert_eq!(s.http.base_url, "http://localhost:1");
    assert_eq!(s.command.timeout_secs, 30);
    let s: ExternalSettings = serde_json::from_str("{}").unwrap();
    assert_eq!(s.mode, "off");
  }

  #[test]
  fn validation_rules() {
    let mut s = ExternalSettings::default();
    assert!(validate(&s).is_ok());
    s.command.timeout_secs = 0;
    assert_eq!(validate(&s).unwrap().command.timeout_secs, 1);
    s.command.timeout_secs = 9999;
    assert_eq!(validate(&s).unwrap().command.timeout_secs, 120);
    s.mode = "weird".to_owned();
    assert!(validate(&s).is_err());
    s.mode = "http".to_owned();
    assert!(validate(&s).is_err());
    s.http.base_url = "http://localhost:8000".to_owned();
    assert!(validate(&s).is_ok());
    s.http.base_url = "https://example.com".to_owned();
    assert!(validate(&s).is_err());
    s.http.allow_remote = true;
    assert!(validate(&s).is_ok());
    s.http.base_url = "http://example.com".to_owned();
    assert!(validate(&s).is_err());
    s.mode = "command".to_owned();
    s.http = HttpSettings::default();
    assert!(validate(&s).is_err());
  }

  #[test]
  fn argv_rules() {
    let abs = if cfg!(windows) { "C:\\tts.exe" } else { "/usr/bin/tts" };
    let ok = vec![abs.to_owned(), "--fast".to_owned()];
    assert!(validate_argv(&ok).is_ok());
    assert!(validate_argv(&[]).is_err());
    assert!(validate_argv(&["tts".to_owned()]).is_err());
    assert!(validate_argv(&["./tts".to_owned()]).is_err());
    assert!(validate_argv(&[abs.to_owned(), "a\0b".to_owned()]).is_err());
    assert!(validate_argv(&[abs.to_owned(), "x".repeat(4096)]).is_err());
    assert!(validate_argv(&[abs.to_owned(), "x".repeat(4095)]).is_ok());
    let many: Vec<String> = std::iter::once(abs.to_owned()).chain((0..64).map(|i| i.to_string())).collect();
    assert!(validate_argv(&many).is_err());
    assert!(validate_argv(&many[..64]).is_ok());
  }

  #[test]
  fn load_missing_file_is_default_without_error() {
    let dir = tempfile::tempdir().unwrap();
    let (s, e) = load_from(&dir.path().join(FILE_NAME));
    assert_eq!(s.mode, "off");
    assert!(e.is_none());
  }

  #[test]
  fn save_then_load_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join(FILE_NAME);
    let s = http_settings(8000);
    save_to(&path, &s).unwrap();
    let (loaded, e) = load_from(&path);
    assert!(e.is_none());
    assert!(loaded == s);
  }

  #[test]
  fn tampered_file_turns_the_voice_off_with_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    for json in [
      r#"{"mode":"http","http":{"baseUrl":"http://evil.example.com","allowRemote":true}}"#,
      r#"{"mode":"http","http":{"baseUrl":"https://evil.example.com","allowRemote":false}}"#,
      r#"{"mode":"command","command":{"argv":["relative/tts"]}}"#,
      r#"{"mode":"rm"}"#,
      "not json",
    ] {
      std::fs::write(&path, json).unwrap();
      let (s, e) = load_from(&path);
      assert_eq!(s.mode, "off", "{json}");
      assert!(s.http.base_url.is_empty(), "{json}");
      assert!(e.is_some(), "{json}");
    }
  }

  #[test]
  fn confirmation_is_needed_for_argv_and_remote_changes_only() {
    let old = ExternalSettings::default();
    assert!(confirmation(&old, &old).is_none());

    let cmd = command_settings(vec!["/usr/bin/tts".to_owned(), "a b".to_owned()]);
    let message = confirmation(&old, &cmd).unwrap();
    assert!(message.contains(r#""/usr/bin/tts" "a b""#), "{message}");
    assert!(confirmation(&cmd, &cmd).is_none());
    let mut other_mode = cmd.clone();
    other_mode.mode = "off".to_owned();
    assert!(confirmation(&cmd, &other_mode).is_none());
    let mut changed = cmd.clone();
    changed.command.argv.push("--x".to_owned());
    assert!(confirmation(&cmd, &changed).is_some());

    assert!(confirmation(&old, &http_settings(8000)).is_none());
    let mut remote = old.clone();
    remote.mode = "http".to_owned();
    remote.http = HttpSettings { base_url: "https://tts.example.com".to_owned(), allow_remote: true, ..HttpSettings::default() };
    let message = confirmation(&old, &remote).unwrap();
    assert!(message.contains("https://tts.example.com"), "{message}");
    assert!(confirmation(&remote, &remote).is_none());
    let mut moved = remote.clone();
    moved.http.base_url = "https://other.example.com".to_owned();
    assert!(confirmation(&remote, &moved).is_some());
    let mut model_only = remote.clone();
    model_only.http.model = "x".to_owned();
    assert!(confirmation(&remote, &model_only).is_none());
  }

  #[test]
  fn health_state_machine() {
    let mut h = Health::new();
    assert!(h.healthy);
    h.record::<()>(&Err("boom".to_owned()));
    assert!(!h.healthy);
    assert_eq!(h.last_error.as_deref(), Some("boom"));
    h.record::<()>(&Ok(None));
    assert!(!h.healthy);
    assert_eq!(h.last_error.as_deref(), Some("boom"));
    h.record(&Ok(Some(())));
    assert!(h.healthy);
    assert!(h.last_error.is_none());
    h.record::<()>(&Err("again".to_owned()));
    h.mark_healthy();
    assert!(h.healthy && h.last_error.is_none());
  }

  #[test]
  fn payload_layout() {
    let p = payload(22050, &[1.0]);
    assert_eq!(&p[..4], &22050u32.to_le_bytes());
    assert_eq!(&p[4..], &1.0f32.to_le_bytes());
  }

  struct Server {
    port: u16,
    request: std::sync::Arc<Mutex<Vec<u8>>>,
    task: tokio::task::JoinHandle<()>,
  }

  impl Drop for Server {
    fn drop(&mut self) {
      self.task.abort();
    }
  }

  async fn serve(delay: Duration, response: Vec<u8>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let request = std::sync::Arc::new(Mutex::new(Vec::new()));
    let seen = request.clone();
    let task = tokio::spawn(async move {
      loop {
        let Ok((mut socket, _)) = listener.accept().await else { return };
        let seen = seen.clone();
        let response = response.clone();
        tokio::spawn(async move {
          let mut data = Vec::new();
          let mut buf = [0u8; 4096];
          loop {
            let n = socket.read(&mut buf).await.unwrap_or(0);
            if n == 0 {
              return;
            }
            data.extend_from_slice(&buf[..n]);
            if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
              let head = String::from_utf8_lossy(&data[..end]).to_lowercase();
              let want = head.split("content-length:").nth(1).and_then(|v| v.split("\r\n").next()).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
              if data.len() >= end + 4 + want {
                break;
              }
            }
          }
          *seen.lock().unwrap() = data;
          tokio::time::sleep(delay).await;
          let _ = socket.write_all(&response).await;
          let _ = socket.shutdown().await;
        });
      }
    });
    Server { port, request, task }
  }

  fn ok_response(body: &[u8]) -> Vec<u8> {
    let mut r = format!("HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
    r.extend_from_slice(body);
    r
  }

  const NEVER: Duration = Duration::ZERO;

  #[tokio::test]
  async fn http_success_posts_the_expected_json() {
    let server = serve(NEVER, ok_response(&wav_bytes(&[0, 16384]))).await;
    let stopped = AtomicU64::new(0);
    let out = synthesize_with(&http_settings(server.port), "hello", &stopped, 1, MAX_AUDIO_BYTES).await.unwrap().unwrap();
    assert_eq!(out, expected_payload());
    let request = String::from_utf8(server.request.lock().unwrap().clone()).unwrap();
    assert!(request.starts_with("POST /v1/audio/speech HTTP/1.1"), "{request}");
    assert!(request.to_lowercase().contains("user-agent: tmgr-desktop-external-voice"), "{request}");
    let body: serde_json::Value = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(body, serde_json::json!({"model": "m1", "input": "hello", "voice": "v1", "response_format": "wav"}));
  }

  #[tokio::test]
  async fn http_omits_empty_model_and_voice() {
    let server = serve(NEVER, ok_response(&wav_bytes(&[0, 16384]))).await;
    let mut settings = http_settings(server.port);
    settings.http.model.clear();
    settings.http.voice.clear();
    synthesize_with(&settings, "hi", &AtomicU64::new(0), 1, MAX_AUDIO_BYTES).await.unwrap().unwrap();
    let request = String::from_utf8(server.request.lock().unwrap().clone()).unwrap();
    let body: serde_json::Value = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(body, serde_json::json!({"input": "hi", "response_format": "wav"}));
  }

  #[tokio::test]
  async fn http_error_status_is_an_error() {
    let server = serve(NEVER, b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()).await;
    let err = synthesize_with(&http_settings(server.port), "x", &AtomicU64::new(0), 1, MAX_AUDIO_BYTES).await.unwrap_err();
    assert!(err.contains("500"), "{err}");
  }

  #[tokio::test]
  async fn http_redirect_to_a_remote_host_is_not_followed() {
    let server = serve(NEVER, b"HTTP/1.1 302 Found\r\nLocation: https://example.invalid/v1/audio/speech\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()).await;
    let err = synthesize_with(&http_settings(server.port), "x", &AtomicU64::new(0), 1, MAX_AUDIO_BYTES).await.unwrap_err();
    assert!(err.contains("302"), "{err}");
  }

  #[tokio::test]
  async fn http_body_over_the_cap_is_rejected_without_content_length() {
    let mut response = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
    response.extend_from_slice(&wav_bytes(&vec![1i16; 500]));
    let server = serve(NEVER, response).await;
    let err = synthesize_with(&http_settings(server.port), "x", &AtomicU64::new(0), 1, 100).await.unwrap_err();
    assert!(err.contains("30 MB"), "{err}");
  }

  #[tokio::test]
  async fn http_non_wav_body_is_an_error() {
    let server = serve(NEVER, ok_response(b"<html>nope</html>")).await;
    assert!(synthesize_with(&http_settings(server.port), "x", &AtomicU64::new(0), 1, MAX_AUDIO_BYTES).await.is_err());
  }

  #[tokio::test]
  async fn http_stop_aborts_a_slow_request() {
    let server = serve(Duration::from_secs(20), ok_response(&wav_bytes(&[0, 16384]))).await;
    let stopped = std::sync::Arc::new(AtomicU64::new(0));
    let flag = stopped.clone();
    tokio::spawn(async move {
      tokio::time::sleep(Duration::from_millis(300)).await;
      flag.store(1, Ordering::SeqCst);
    });
    let started = std::time::Instant::now();
    let out = synthesize_with(&http_settings(server.port), "x", &stopped, 1, MAX_AUDIO_BYTES).await.unwrap();
    assert!(out.is_none());
    assert!(started.elapsed() < Duration::from_secs(5));
  }

  #[tokio::test]
  async fn stale_run_returns_empty_without_work() {
    let stopped = AtomicU64::new(5);
    assert!(synthesize_with(&http_settings(1), "x", &stopped, 5, MAX_AUDIO_BYTES).await.unwrap().is_none());
    assert!(synthesize_with(&command_settings(vec!["/nonexistent/tts".to_owned()]), "x", &stopped, 3, MAX_AUDIO_BYTES).await.unwrap().is_none());
  }

  #[tokio::test]
  async fn off_mode_is_an_error() {
    assert!(synthesize_with(&ExternalSettings::default(), "x", &AtomicU64::new(0), 1, MAX_AUDIO_BYTES).await.is_err());
  }

  #[tokio::test]
  async fn unreachable_server_fails_fast() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    assert!(synthesize_with(&http_settings(port), "x", &AtomicU64::new(0), 1, MAX_AUDIO_BYTES).await.is_err());
  }

  #[cfg(unix)]
  mod unix {
    use super::*;

    struct Fixture {
      dir: tempfile::TempDir,
    }

    impl Fixture {
      fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("out.wav"), wav_bytes(&[0, 16384])).unwrap();
        Self { dir }
      }

      fn p(&self, name: &str) -> String {
        self.dir.path().join(name).to_string_lossy().into_owned()
      }

      fn argv(&self, script: &str) -> Vec<String> {
        let path = self.p("run.sh");
        std::fs::write(&path, script).unwrap();
        vec!["/bin/sh".to_owned(), path]
      }

      fn settings(&self, script: &str) -> CommandSettings {
        CommandSettings { argv: self.argv(script), timeout_secs: 30 }
      }
    }

    fn alive(pid: i32) -> bool {
      unsafe { libc::kill(pid, 0) == 0 }
    }

    async fn nobody_alive(pids: &[i32]) -> bool {
      for _ in 0..60 {
        if pids.iter().all(|p| !alive(*p)) {
          return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
      }
      false
    }

    fn pids(path: &str) -> Vec<i32> {
      std::fs::read_to_string(path).unwrap_or_default().split_whitespace().filter_map(|p| p.parse().ok()).collect()
    }

    #[tokio::test]
    async fn success_receives_text_on_stdin() {
      let f = Fixture::new();
      let script = format!("cat > '{}'\ncat '{}'\n", f.p("in.txt"), f.p("out.wav"));
      let stopped = AtomicU64::new(0);
      let out = run_command(&f.settings(&script), "héllo wörld", Duration::from_secs(10), &stopped, 1, MAX_AUDIO_BYTES).await.unwrap().unwrap();
      assert_eq!(out, wav_bytes(&[0, 16384]));
      assert_eq!(std::fs::read_to_string(f.p("in.txt")).unwrap(), "héllo wörld");
    }

    #[tokio::test]
    async fn background_children_are_killed_after_a_successful_run() {
      let f = Fixture::new();
      let script = format!("cat >/dev/null\nsleep 60 >/dev/null 2>&1 &\necho $! > '{}'\ncat '{}'\n", f.p("pids.txt"), f.p("out.wav"));
      let stopped = AtomicU64::new(0);
      run_command(&f.settings(&script), "x", Duration::from_secs(10), &stopped, 1, MAX_AUDIO_BYTES).await.unwrap().unwrap();
      assert!(nobody_alive(&pids(&f.p("pids.txt"))).await);
    }

    #[tokio::test]
    async fn full_synthesis_through_a_command() {
      let f = Fixture::new();
      let script = format!("cat >/dev/null\ncat '{}'\n", f.p("out.wav"));
      let settings = command_settings(f.argv(&script));
      let out = synthesize_with(&settings, "x", &AtomicU64::new(0), 1, MAX_AUDIO_BYTES).await.unwrap().unwrap();
      assert_eq!(out, expected_payload());
    }

    #[tokio::test]
    async fn command_that_ignores_stdin_still_works() {
      let f = Fixture::new();
      let script = format!("cat '{}'\n", f.p("out.wav"));
      let text = "x".repeat(2000);
      let out = run_command(&f.settings(&script), &text, Duration::from_secs(10), &AtomicU64::new(0), 1, MAX_AUDIO_BYTES).await.unwrap();
      assert!(out.is_some());
    }

    #[tokio::test]
    async fn nonzero_exit_reports_stderr() {
      let f = Fixture::new();
      let err = run_command(&f.settings("echo 'model missing' >&2\nexit 3\n"), "x", Duration::from_secs(10), &AtomicU64::new(0), 1, MAX_AUDIO_BYTES).await.unwrap_err();
      assert!(err.contains("model missing") && err.contains('3'), "{err}");
    }

    #[tokio::test]
    async fn huge_stderr_is_drained_and_truncated() {
      let f = Fixture::new();
      let script = format!("head -c 300000 /dev/zero | tr '\\0' 'e' >&2\ncat '{}'\n", f.p("out.wav"));
      let out = run_command(&f.settings(&script), "x", Duration::from_secs(10), &AtomicU64::new(0), 1, MAX_AUDIO_BYTES).await.unwrap();
      assert!(out.is_some());
      let script = "head -c 300000 /dev/zero | tr '\\0' 'e' >&2\nexit 1\n";
      let err = run_command(&f.settings(script), "x", Duration::from_secs(10), &AtomicU64::new(0), 1, MAX_AUDIO_BYTES).await.unwrap_err();
      assert!(err.len() < 2300 && err.contains("eee"), "{}", err.len());
    }

    #[tokio::test]
    async fn timeout_kills_the_whole_process_group() {
      let f = Fixture::new();
      let script = format!("sleep 60 &\necho $! >> '{0}'\nsleep 60 &\necho $! >> '{0}'\nwait\n", f.p("pids"));
      let started = std::time::Instant::now();
      let err = run_command(&f.settings(&script), "x", Duration::from_millis(1500), &AtomicU64::new(0), 1, MAX_AUDIO_BYTES).await.unwrap_err();
      assert!(err.contains("did not finish"), "{err}");
      assert!(started.elapsed() < Duration::from_secs(10));
      let spawned = pids(&f.p("pids"));
      assert_eq!(spawned.len(), 2);
      assert!(nobody_alive(&spawned).await);
    }

    #[tokio::test]
    async fn stdout_over_the_cap_is_an_error_and_kills_the_command() {
      let f = Fixture::new();
      let script = format!("echo $$ > '{}'\nyes | head -c 100000\nsleep 60\n", f.p("pids"));
      let err = run_command(&f.settings(&script), "x", Duration::from_secs(10), &AtomicU64::new(0), 1, 100).await.unwrap_err();
      assert!(err.contains("30 MB"), "{err}");
      assert!(nobody_alive(&pids(&f.p("pids"))).await);
    }

    #[tokio::test]
    async fn stop_kills_a_slow_command_and_returns_empty() {
      let f = Fixture::new();
      let script = format!("sleep 60 &\necho $! > '{}'\nwait\n", f.p("pids"));
      let stopped = std::sync::Arc::new(AtomicU64::new(0));
      let flag = stopped.clone();
      tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(600)).await;
        flag.store(1, Ordering::SeqCst);
      });
      let started = std::time::Instant::now();
      let out = run_command(&f.settings(&script), "x", Duration::from_secs(30), &stopped, 1, MAX_AUDIO_BYTES).await.unwrap();
      assert!(out.is_none());
      assert!(started.elapsed() < Duration::from_secs(10));
      let spawned = pids(&f.p("pids"));
      assert_eq!(spawned.len(), 1);
      assert!(nobody_alive(&spawned).await);
    }

    #[tokio::test]
    async fn missing_program_is_an_error() {
      let settings = CommandSettings { argv: vec!["/nonexistent/tts-binary".to_owned()], timeout_secs: 5 };
      let err = run_command(&settings, "x", Duration::from_secs(5), &AtomicU64::new(0), 1, MAX_AUDIO_BYTES).await.unwrap_err();
      assert!(err.contains("could not be started"), "{err}");
    }

    #[tokio::test]
    async fn relative_program_is_rejected_at_run_time() {
      let settings = CommandSettings { argv: vec!["sh".to_owned()], timeout_secs: 5 };
      assert!(run_command(&settings, "x", Duration::from_secs(5), &AtomicU64::new(0), 1, MAX_AUDIO_BYTES).await.is_err());
    }
  }
}
