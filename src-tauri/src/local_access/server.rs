#![cfg(unix)]

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Body, Bytes, Frame, Incoming};
use hyper::header::HeaderMap;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::Serialize;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::watch;

use super::tokens::{chmod, StoredToken, TokenStore};
use super::{BridgeReply, BridgeRequest, Cursor, EventBus, EventEntry, Replay};

const MAX_BODY_BYTES: usize = 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 5 * 1024 * 1024;
const BRIDGE_TIMEOUT: Duration = Duration::from_secs(15);
const RATE_LIMIT_GET_PER_SEC: u32 = 50;
const RATE_LIMIT_WRITE_PER_SEC: u32 = 10;
const HEALTH_PATH: &str = "/api/local/health";
const EVENTS_PATH: &str = "/api/local/events";
const GRANT_PATH: &str = "/api/local/_grant";
const SSE_PING_INTERVAL: Duration = Duration::from_secs(15);
const GRANT_CACHE_TTL: Duration = Duration::from_secs(1);
const MAX_SSE_PER_TOKEN: usize = 4;

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
type ResponseBody = BoxBody<Bytes, std::convert::Infallible>;

fn boxed(body: Full<Bytes>) -> ResponseBody {
  BodyExt::boxed(body)
}

/// Emits `local-access://request` to the main window and resolves once `local_access_reply` answers
/// it (or the caller times out). Injectable so tests can stand in a fake window.
pub type BridgeFn = Arc<dyn Fn(BridgeRequest) -> BoxFuture<BridgeReply> + Send + Sync>;

pub struct ServerDeps {
  pub tokens: Arc<TokenStore>,
  pub workspace_exists: Arc<dyn Fn(&str) -> Option<i64> + Send + Sync>,
  pub ready: Arc<dyn Fn() -> bool + Send + Sync>,
  pub bridge: BridgeFn,
  pub app_version: String,
  pub now_epoch: Arc<dyn Fn() -> i64 + Send + Sync>,
  pub events: Arc<EventBus>,
}

#[derive(Default)]
struct Window {
  start: Option<Instant>,
  get: u32,
  write: u32,
}

/// Per-token, 1-second sliding windows: 50 reads / 10 writes, same as plugins and cloud personas.
#[derive(Default)]
struct RateLimiter {
  windows: Mutex<HashMap<String, Window>>,
}

impl RateLimiter {
  fn check(&self, token_id: &str, is_get: bool) -> bool {
    let mut windows = self.windows.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    let window = windows.entry(token_id.to_string()).or_default();
    if window.start.is_none_or(|start| now.duration_since(start) >= Duration::from_secs(1)) {
      window.start = Some(now);
      window.get = 0;
      window.write = 0;
    }
    if is_get {
      window.get += 1;
      window.get <= RATE_LIMIT_GET_PER_SEC
    } else {
      window.write += 1;
      window.write <= RATE_LIMIT_WRITE_PER_SEC
    }
  }
}

fn is_human_token_header(headers: &HeaderMap) -> bool {
  headers.contains_key("x-smart-device-token") || headers.contains_key("authorization")
}

/// At most 4 open SSE connections per token; an SSE connection itself already consumed one GET
/// against `RateLimiter` above.
#[derive(Default)]
struct SseConnLimiter {
  counts: Mutex<HashMap<String, usize>>,
}

impl SseConnLimiter {
  fn try_acquire(&self, token_id: &str) -> bool {
    let mut counts = self.counts.lock().unwrap_or_else(|e| e.into_inner());
    let entry = counts.entry(token_id.to_string()).or_insert(0);
    if *entry >= MAX_SSE_PER_TOKEN {
      false
    } else {
      *entry += 1;
      true
    }
  }

  fn release(&self, token_id: &str) {
    let mut counts = self.counts.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = counts.get_mut(token_id) {
      *entry = entry.saturating_sub(1);
      if *entry == 0 {
        counts.remove(token_id);
      }
    }
  }
}

struct SseGuard {
  limiter: Arc<SseConnLimiter>,
  token_id: String,
}

impl Drop for SseGuard {
  fn drop(&mut self) {
    self.limiter.release(&self.token_id);
  }
}

/// Backs the SSE response body with an mpsc channel so the writer task can push frames as events
/// arrive, without pulling in a separate `Stream` combinator crate.
struct ChannelBody {
  rx: tokio::sync::mpsc::UnboundedReceiver<Bytes>,
}

impl Body for ChannelBody {
  type Data = Bytes;
  type Error = std::convert::Infallible;

  fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
    match self.rx.poll_recv(cx) {
      Poll::Ready(Some(bytes)) => Poll::Ready(Some(Ok(Frame::data(bytes)))),
      Poll::Ready(None) => Poll::Ready(None),
      Poll::Pending => Poll::Pending,
    }
  }
}

fn sse_response(rx: tokio::sync::mpsc::UnboundedReceiver<Bytes>) -> Response<ResponseBody> {
  Response::builder()
    .status(200)
    .header("Content-Type", "text/event-stream")
    .header("Cache-Control", "no-cache")
    .body(BodyExt::boxed(ChannelBody { rx }))
    .unwrap()
}

fn sse_event_frame(boot: &str, entry: &EventEntry) -> Bytes {
  Bytes::from(format!("id: {boot}:{}\nevent: {}\ndata: {}\n\n", entry.seq, entry.event_type, entry.json))
}

fn sse_reset_frame(cursor: &str) -> Bytes {
  Bytes::from(format!("event: reset\ndata: {{\"cursor\":\"{cursor}\"}}\n\n"))
}

fn sse_revoked_frame() -> Bytes {
  Bytes::from_static(b"event: revoked\ndata: {}\n\n")
}

fn sse_ping_frame() -> Bytes {
  Bytes::from_static(b": ping\n\n")
}

struct GrantCache {
  permissions: Vec<String>,
  fetched_at: Instant,
}

/// Asks the main window for the persona's current grant over the same bridge normal requests use,
/// with a synthetic internal path the HTTP entrypoint refuses from the outside (see `handle`).
async fn fetch_grant(deps: &ServerDeps, token: &StoredToken, next_id: &AtomicU64) -> Result<Vec<String>, ()> {
  let request_id = next_id.fetch_add(1, Ordering::Relaxed) + 1;
  let request = BridgeRequest {
    id: request_id,
    workspace_code: token.workspace_code.clone(),
    workspace_id: token.workspace_id,
    persona_uuid: token.persona_uuid.clone(),
    persona_name: token.persona_name.clone(),
    token_id: token.id.clone(),
    method: "GET".into(),
    path: GRANT_PATH.into(),
    body: None,
  };
  let reply = tokio::time::timeout(BRIDGE_TIMEOUT, (deps.bridge)(request)).await.map_err(|_| ())?;
  if reply.status != 200 {
    return Err(());
  }
  let value: serde_json::Value = serde_json::from_str(&reply.body).map_err(|_| ())?;
  Ok(
    value
      .get("permissions")
      .and_then(|p| p.as_array())
      .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
      .unwrap_or_default(),
  )
}

async fn cached_grant(
  cache: &mut Option<GrantCache>,
  deps: &ServerDeps,
  token: &StoredToken,
  next_id: &AtomicU64,
) -> Result<Vec<String>, ()> {
  if let Some(cached) = cache {
    if cached.fetched_at.elapsed() < GRANT_CACHE_TTL {
      return Ok(cached.permissions.clone());
    }
  }
  let permissions = fetch_grant(deps, token, next_id).await?;
  *cache = Some(GrantCache { permissions: permissions.clone(), fetched_at: Instant::now() });
  Ok(permissions)
}

/// One SSE connection: replays buffered events past the client's cursor, then follows the live
/// buffer, checking the token and grant again before every batch so a revoke closes the stream.
async fn handle_sse(
  deps: Arc<ServerDeps>,
  sse_limiter: Arc<SseConnLimiter>,
  next_id: Arc<AtomicU64>,
  token: StoredToken,
  secret: String,
  cursor: Option<Cursor>,
  mut conn_shutdown: watch::Receiver<bool>,
) -> Response<ResponseBody> {
  if !sse_limiter.try_acquire(&token.id) {
    return json_error_retry(429, "RATE_LIMITED", "Too many concurrent event streams", 1);
  }

  let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Bytes>();
  let mut notify_rx = deps.events.subscribe();
  let workspace_code = token.workspace_code.clone();
  let boot = deps.events.boot().to_string();
  let replay = deps.events.replay(&workspace_code, cursor);
  let mut last_seq = deps.events.current_seq(&workspace_code);

  tauri::async_runtime::spawn(async move {
    let _guard = SseGuard { limiter: sse_limiter, token_id: token.id.clone() };
    let mut grant_cache: Option<GrantCache> = None;
    let permissions = match cached_grant(&mut grant_cache, &deps, &token, &next_id).await {
      Ok(permissions) => permissions,
      Err(()) => {
        let _ = tx.send(sse_revoked_frame());
        return;
      }
    };

    match replay {
      Replay::Reset { cursor } => {
        if tx.send(sse_reset_frame(&cursor)).is_err() {
          return;
        }
      }
      Replay::Events(entries) => {
        for entry in entries {
          if permissions.contains(&entry.permission) && tx.send(sse_event_frame(&boot, &entry)).is_err() {
            return;
          }
        }
      }
    }

    let mut ping = tokio::time::interval(SSE_PING_INTERVAL);
    ping.tick().await; // the first tick fires immediately; skip it so ping really means "every 15s"
    loop {
      tokio::select! {
        _ = ping.tick() => {
          if tx.send(sse_ping_frame()).is_err() {
            break;
          }
        }
        changed = notify_rx.recv() => {
          match changed {
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
              if deps.tokens.verify(&secret, (deps.now_epoch)()).is_err() {
                let _ = tx.send(sse_revoked_frame());
                break;
              }
              let permissions = match cached_grant(&mut grant_cache, &deps, &token, &next_id).await {
                Ok(permissions) => permissions,
                Err(()) => {
                  let _ = tx.send(sse_revoked_frame());
                  break;
                }
              };
              let mut closed = false;
              for entry in deps.events.since(&workspace_code, last_seq) {
                last_seq = entry.seq;
                if permissions.contains(&entry.permission) && tx.send(sse_event_frame(&boot, &entry)).is_err() {
                  closed = true;
                  break;
                }
              }
              if closed {
                break;
              }
            }
          }
        }
        changed = conn_shutdown.changed() => {
          if changed.is_err() || *conn_shutdown.borrow() {
            break;
          }
        }
      }
    }
  });

  sse_response(rx)
}

#[derive(Serialize)]
struct ErrorBody<'a> {
  message: &'a str,
  code: &'a str,
}

fn json_error(status: u16, code: &str, message: &str) -> Response<ResponseBody> {
  json_response(status, &ErrorBody { message, code }, &[])
}

fn json_error_retry(status: u16, code: &str, message: &str, retry_after_secs: u32) -> Response<ResponseBody> {
  json_response(status, &ErrorBody { message, code }, &[("Retry-After", retry_after_secs.to_string())])
}

fn json_response(status: u16, body: &impl Serialize, extra_headers: &[(&str, String)]) -> Response<ResponseBody> {
  let mut builder = Response::builder()
    .status(StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR))
    .header("Content-Type", "application/json");
  for (name, value) in extra_headers {
    builder = builder.header(*name, value);
  }
  builder.body(boxed(Full::new(Bytes::from(serde_json::to_vec(body).unwrap_or_default())))).unwrap()
}

async fn read_body_limited(body: Incoming) -> Result<Vec<u8>, ()> {
  match Limited::new(body, MAX_BODY_BYTES).collect().await {
    Ok(collected) => Ok(collected.to_bytes().to_vec()),
    Err(_) => Err(()),
  }
}

fn health_response(deps: &ServerDeps, headers: &HeaderMap) -> Response<ResponseBody> {
  let ready = (deps.ready)();
  let mut body = serde_json::json!({
    "ok": true,
    "ready": ready,
    "appVersion": deps.app_version,
    "api": "local-1",
  });
  if let Some(secret) = headers.get("x-persona-token").and_then(|v| v.to_str().ok()) {
    match deps.tokens.verify(secret, (deps.now_epoch)()) {
      Ok(token) => {
        body["token"] = serde_json::Value::String("valid".into());
        body["workspace"] = serde_json::json!({ "id": token.workspace_id, "code": token.workspace_code });
        body["persona"] = serde_json::json!({ "id": token.persona_uuid, "name": token.persona_name });
      }
      Err(err) => body["token"] = serde_json::Value::String(err.code().into()),
    }
  }
  json_response(200, &body, &[])
}

#[allow(clippy::too_many_arguments)]
async fn handle(
  req: Request<Incoming>,
  deps: Arc<ServerDeps>,
  limiter: Arc<RateLimiter>,
  next_id: Arc<AtomicU64>,
  sse_limiter: Arc<SseConnLimiter>,
  conn_shutdown: watch::Receiver<bool>,
) -> Response<ResponseBody> {
  let method = req.method().as_str().to_string();
  let path_only = req.uri().path().to_string();
  let path_and_query =
    req.uri().path_and_query().map(|p| p.as_str().to_string()).unwrap_or_else(|| path_only.clone());
  let headers = req.headers().clone();

  let body = match read_body_limited(req.into_body()).await {
    Ok(bytes) => bytes,
    Err(()) => return json_error(413, "TOO_LARGE", "Request body is larger than 1 MB"),
  };

  if path_only == HEALTH_PATH && method.eq_ignore_ascii_case("GET") {
    return health_response(&deps, &headers);
  }

  // Only the SSE handler below may ask this of the window, and it does so through the bridge
  // directly, never through this HTTP entrypoint: any request that reaches here is external.
  if path_only == GRANT_PATH {
    return json_error(404, "NOT_FOUND", "Not found");
  }

  if is_human_token_header(&headers) {
    return json_error(400, "HUMAN_TOKEN", "This socket accepts only a persona token");
  }

  let Some(secret) = headers.get("x-persona-token").and_then(|v| v.to_str().ok()) else {
    return json_error(401, "TOKEN_MISSING", "X-Persona-Token is required");
  };

  let now = (deps.now_epoch)();
  let token = match deps.tokens.verify(secret, now) {
    Ok(token) => token,
    Err(err) => return json_error(401, err.code(), err.message()),
  };

  if (deps.workspace_exists)(&token.workspace_code).is_none() {
    return json_error(409, "WORKSPACE_GONE", "The local workspace for this token is gone");
  }

  let is_get = method.eq_ignore_ascii_case("GET");
  if !limiter.check(&token.id, is_get) {
    return json_error_retry(429, "RATE_LIMITED", "Too many requests", 1);
  }

  if !(deps.ready)() {
    return json_error_retry(503, "APP_NOT_READY", "The app is not ready yet", 2);
  }

  if path_only == EVENTS_PATH && is_get {
    let cursor = extract_cursor(&headers, &path_and_query);
    let secret = secret.to_string();
    return handle_sse(deps, sse_limiter, next_id, token, secret, cursor, conn_shutdown).await;
  }

  let request_id = next_id.fetch_add(1, Ordering::Relaxed) + 1;
  let bridge_request = BridgeRequest {
    id: request_id,
    workspace_code: token.workspace_code.clone(),
    workspace_id: token.workspace_id,
    persona_uuid: token.persona_uuid.clone(),
    persona_name: token.persona_name.clone(),
    token_id: token.id.clone(),
    method,
    path: path_and_query,
    body: (!body.is_empty()).then(|| String::from_utf8_lossy(&body).into_owned()),
  };
  match tokio::time::timeout(BRIDGE_TIMEOUT, (deps.bridge)(bridge_request)).await {
    Ok(reply) if reply.body.len() > MAX_RESPONSE_BYTES => {
      json_error(500, "INTERNAL", "The app's response was too large to send back")
    }
    Ok(reply) => json_raw_response(reply.status, reply.body),
    Err(_) => json_error(504, "TIMEOUT", "The app did not answer in time"),
  }
}

fn extract_cursor(headers: &HeaderMap, path_and_query: &str) -> Option<Cursor> {
  if let Some(raw) = headers.get("last-event-id").and_then(|v| v.to_str().ok()) {
    if let Some(cursor) = Cursor::parse(raw) {
      return Some(cursor);
    }
  }
  let query = path_and_query.split_once('?').map(|(_, q)| q).unwrap_or("");
  query.split('&').find_map(|pair| pair.strip_prefix("cursor=").and_then(Cursor::parse))
}

fn json_raw_response(status: u16, body: String) -> Response<ResponseBody> {
  Response::builder()
    .status(StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR))
    .header("Content-Type", "application/json")
    .body(boxed(Full::new(Bytes::from(body))))
    .unwrap()
}

/// `<app data dir>/local-access/local-access.sock`, unless that path is past `sun_path`'s ~104 byte
/// limit, in which case a short path under the OS temp dir is used instead.
pub fn resolve_socket_path(app_data_dir: &Path) -> PathBuf {
  let primary = app_data_dir.join("local-access").join("local-access.sock");
  if primary.as_os_str().len() <= 100 {
    return primary;
  }
  fallback_socket_path()
}

#[cfg(unix)]
fn fallback_socket_path() -> PathBuf {
  let uid = unsafe { libc::getuid() };
  std::env::temp_dir().join(format!("tmgr-{uid}")).join("local-access.sock")
}

fn file_ino(path: &Path) -> Option<u64> {
  use std::os::unix::fs::MetadataExt;
  std::fs::symlink_metadata(path).ok().map(|meta| meta.ino())
}

async fn prepare_socket_path(path: &Path) -> Result<(), String> {
  if path.exists() {
    match UnixStream::connect(path).await {
      // A live listener already owns this path: never steal another instance's socket.
      Ok(_) => return Err("another instance already owns the local access socket".into()),
      Err(_) => {
        let _ = std::fs::remove_file(path);
      }
    }
  }
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent).map_err(|e| format!("create_dir_all {}: {e}", parent.display()))?;
    chmod(parent, 0o700).map_err(|e| format!("chmod {}: {e}", parent.display()))?;
  }
  Ok(())
}

pub struct Server {
  deps: Arc<ServerDeps>,
  limiter: Arc<RateLimiter>,
  next_id: Arc<AtomicU64>,
  sse_limiter: Arc<SseConnLimiter>,
}

impl Server {
  /// `next_id` is shared with the caller so a restart never reissues an id a still-in-flight request
  /// from the previous run might resolve against.
  pub fn new(deps: ServerDeps, next_id: Arc<AtomicU64>) -> Self {
    Self {
      deps: Arc::new(deps),
      limiter: Arc::new(RateLimiter::default()),
      next_id,
      sse_limiter: Arc::new(SseConnLimiter::default()),
    }
  }

  /// Binds and runs until `shutdown` carries `true`; `bound`, if given, carries the bind outcome so
  /// the caller only records "listening" once the socket truly is.
  pub async fn serve(
    &self,
    socket_path: &Path,
    mut shutdown: watch::Receiver<bool>,
    bound: Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
  ) -> Result<(), String> {
    let (listener, bound_ino) = match prepare_socket_path(socket_path)
      .await
      .and_then(|_| UnixListener::bind(socket_path).map_err(|e| format!("bind {}: {e}", socket_path.display())))
      .and_then(|listener| chmod(socket_path, 0o600).map(|_| listener))
    {
      Ok(listener) => {
        if let Some(tx) = bound {
          let _ = tx.send(Ok(()));
        }
        (listener, file_ino(socket_path))
      }
      Err(error) => {
        if let Some(tx) = bound {
          let _ = tx.send(Err(error.clone()));
        }
        return Err(error);
      }
    };
    loop {
      tokio::select! {
        changed = shutdown.changed() => {
          if changed.is_err() || *shutdown.borrow() {
            break;
          }
        }
        accepted = listener.accept() => {
          let Ok((stream, _)) = accepted else { continue };
          let deps = self.deps.clone();
          let limiter = self.limiter.clone();
          let next_id = self.next_id.clone();
          let sse_limiter = self.sse_limiter.clone();
          let mut conn_shutdown = shutdown.clone();
          let conn_shutdown_for_svc = conn_shutdown.clone();
          tauri::async_runtime::spawn(async move {
            let io = TokioIo::new(stream);
            let svc = service_fn(move |req| {
              let deps = deps.clone();
              let limiter = limiter.clone();
              let next_id = next_id.clone();
              let sse_limiter = sse_limiter.clone();
              let conn_shutdown = conn_shutdown_for_svc.clone();
              async move { Ok::<_, std::convert::Infallible>(handle(req, deps, limiter, next_id, sse_limiter, conn_shutdown).await) }
            });
            let conn = hyper::server::conn::http1::Builder::new().serve_connection(io, svc);
            let mut conn = std::pin::pin!(conn);
            tokio::select! {
              result = conn.as_mut() => { let _ = result; }
              changed = conn_shutdown.changed() => {
                if changed.is_err() || *conn_shutdown.borrow() {
                  conn.as_mut().graceful_shutdown();
                  let _ = conn.await;
                }
              }
            }
          });
        }
      }
    }
    // Only unlink the inode this call itself bound: a fast off/on toggle may already have a newer instance's listener at this path.
    if bound_ino.is_some() && file_ino(socket_path) == bound_ino {
      let _ = std::fs::remove_file(socket_path);
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::local_access::tokens::{IssueParams, MemorySecrets};
  use http_body_util::Empty;
  use hyper::header::{HeaderName, HeaderValue};

  #[test]
  fn human_token_headers_are_detected_case_insensitively() {
    let mut headers = HeaderMap::new();
    assert!(!is_human_token_header(&headers));
    headers.insert(HeaderName::from_static("authorization"), HeaderValue::from_static("Bearer x"));
    assert!(is_human_token_header(&headers));
    let mut headers = HeaderMap::new();
    headers.insert(HeaderName::from_static("x-smart-device-token"), HeaderValue::from_static("x"));
    assert!(is_human_token_header(&headers));
  }

  #[test]
  fn rate_limiter_allows_the_documented_burst_then_rejects() {
    let limiter = RateLimiter::default();
    for _ in 0..10 {
      assert!(limiter.check("t-1", false));
    }
    assert!(!limiter.check("t-1", false));
    for _ in 0..50 {
      assert!(limiter.check("t-2", true));
    }
    assert!(!limiter.check("t-2", true));
  }

  #[test]
  fn rate_limits_are_tracked_per_token() {
    let limiter = RateLimiter::default();
    for _ in 0..10 {
      assert!(limiter.check("a", false));
    }
    assert!(limiter.check("b", false));
  }

  #[test]
  fn socket_path_falls_back_when_the_primary_path_is_too_long() {
    let short = resolve_socket_path(Path::new("/data/dev.tmgr.desktop"));
    assert!(short.ends_with("local-access/local-access.sock"));
    let long = resolve_socket_path(Path::new(&"/data/".to_string().repeat(20)));
    assert!(long.to_string_lossy().contains("tmgr-"));
    assert_ne!(long, short);
  }

  fn test_deps(secret_holder: &mut Option<String>) -> (ServerDeps, String) {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tmgr-local-access-server-test-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = TokenStore::open(dir, Arc::new(MemorySecrets::new())).unwrap();
    let (info, secret) = store
      .issue(
        IssueParams {
          persona_uuid: "persona-1".into(),
          persona_name: "Reviewer".into(),
          workspace_code: "local-personal".into(),
          workspace_id: -1,
          label: "test".into(),
          expires_in_days: 90,
          plugin_id: None,
        },
        0,
      )
      .unwrap();
    *secret_holder = Some(secret.clone());
    let bridge: BridgeFn = Arc::new(|req: BridgeRequest| -> BoxFuture<BridgeReply> {
      Box::pin(async move { BridgeReply { status: 200, body: serde_json::to_string(&req).unwrap() } })
    });
    let deps = ServerDeps {
      tokens: Arc::new(store),
      workspace_exists: Arc::new(|_| Some(-1)),
      ready: Arc::new(|| true),
      bridge,
      app_version: "0.0.0-test".into(),
      now_epoch: Arc::new(|| 0),
      events: Arc::new(super::EventBus::new("boot-1".into())),
    };
    (deps, info.id)
  }

  async fn send(
    sender: &mut hyper::client::conn::http1::SendRequest<Empty<Bytes>>,
    req: Request<Empty<Bytes>>,
  ) -> Response<Incoming> {
    sender.send_request(req).await.unwrap()
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn end_to_end_over_a_real_socket() {
    let mut secret_holder = None;
    let (deps, _token_id) = test_deps(&mut secret_holder);
    let secret = secret_holder.unwrap();
    let socket_dir = std::env::temp_dir().join(format!("tmgr-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&socket_dir);
    let socket_path = socket_dir.join("sock");

    let server = Server::new(deps, Arc::new(AtomicU64::new(0)));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let socket_path_clone = socket_path.clone();
    let handle = tokio::spawn(async move { server.serve(&socket_path_clone, shutdown_rx, None).await });

    for _ in 0..50 {
      if socket_path.exists() || handle.is_finished() {
        break;
      }
      tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(socket_path.exists(), "socket was never created");

    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      let mode = std::fs::metadata(&socket_path).unwrap().permissions().mode() & 0o777;
      assert_eq!(mode, 0o600);
    }

    let connect = || {
      let path = socket_path.clone();
      async move {
        let stream = UnixStream::connect(&path).await.unwrap();
        let io = TokioIo::new(stream);
        let (sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
        tokio::spawn(async move {
          let _ = conn.await;
        });
        sender
      }
    };

    let mut sender = connect().await;
    let req = Request::builder().method("GET").uri("/api/tasks").body(Empty::<Bytes>::new()).unwrap();
    let res = send(&mut sender, req).await;
    assert_eq!(res.status(), 401);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&body).unwrap()["code"], "TOKEN_MISSING");

    let mut sender = connect().await;
    let req = Request::builder()
      .method("GET")
      .uri("/api/tasks")
      .header("X-Persona-Token", "tmgrp_whatever")
      .body(Empty::<Bytes>::new())
      .unwrap();
    let res = send(&mut sender, req).await;
    assert_eq!(res.status(), 401);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&body).unwrap()["code"], "CLOUD_TOKEN");

    let mut sender = connect().await;
    let req = Request::builder()
      .method("GET")
      .uri("/api/tasks")
      .header("Authorization", "Bearer human")
      .body(Empty::<Bytes>::new())
      .unwrap();
    let res = send(&mut sender, req).await;
    assert_eq!(res.status(), 400);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&body).unwrap()["code"], "HUMAN_TOKEN");

    let mut sender = connect().await;
    let req = Request::builder()
      .method("GET")
      .uri("/api/tasks?status_id=3")
      .header("X-Persona-Token", secret.as_str())
      .header("X-TMGR-Plugin", "should-never-arrive")
      .body(Empty::<Bytes>::new())
      .unwrap();
    let res = send(&mut sender, req).await;
    assert_eq!(res.status(), 200);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["personaUuid"], "persona-1");
    assert_eq!(json["path"], "/api/tasks?status_id=3");
    let mut keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
      keys,
      vec!["body", "id", "method", "path", "personaName", "personaUuid", "tokenId", "workspaceCode", "workspaceId"]
    );
    assert!(!json.to_string().contains("should-never-arrive"));

    let mut sender = connect().await;
    let mut statuses = Vec::new();
    for _ in 0..11 {
      let req = Request::builder()
        .method("POST")
        .uri("/api/tasks")
        .header("X-Persona-Token", secret.as_str())
        .body(Empty::<Bytes>::new())
        .unwrap();
      let res = send(&mut sender, req).await;
      statuses.push(res.status().as_u16());
      // The connection isn't ready for the next request until this body is fully drained.
      let _ = res.into_body().collect().await;
    }
    assert_eq!(&statuses[..10], &[200u16; 10]);
    assert_eq!(statuses[10], 429);

    let mut persistent = connect().await;
    let req = Request::builder().method("GET").uri("/api/local/health").body(Empty::<Bytes>::new()).unwrap();
    let res = send(&mut persistent, req).await;
    assert_eq!(res.status(), 200);
    let _ = res.into_body().collect().await;

    let _ = shutdown_tx.send(true);
    let mut closed = false;
    for _ in 0..50 {
      let req = Request::builder().method("GET").uri("/api/local/health").body(Empty::<Bytes>::new()).unwrap();
      if persistent.send_request(req).await.is_err() {
        closed = true;
        break;
      }
      tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(closed, "an already-open connection should be closed by the kill switch");

    let _ = handle.await;
    assert!(!socket_path.exists(), "socket file should be removed on shutdown");
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn a_second_instance_never_steals_a_live_socket_and_can_bind_after_the_first_shuts_down() {
    let mut secret_holder = None;
    let (deps_a, _) = test_deps(&mut secret_holder);
    let socket_dir = std::env::temp_dir().join(format!("tmgr-it-second-instance-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&socket_dir);
    let socket_path = socket_dir.join("sock");

    let server_a = Server::new(deps_a, Arc::new(AtomicU64::new(0)));
    let (shutdown_a_tx, shutdown_a_rx) = watch::channel(false);
    let (bound_a_tx, bound_a_rx) = tokio::sync::oneshot::channel();
    let path_a = socket_path.clone();
    let handle_a = tokio::spawn(async move { server_a.serve(&path_a, shutdown_a_rx, Some(bound_a_tx)).await });
    bound_a_rx.await.unwrap().expect("first instance should bind");

    let mut secret_holder_b = None;
    let (deps_b, _) = test_deps(&mut secret_holder_b);
    let server_b = Server::new(deps_b, Arc::new(AtomicU64::new(0)));
    let (_shutdown_b_tx, shutdown_b_rx) = watch::channel(false);
    let (bound_b_tx, bound_b_rx) = tokio::sync::oneshot::channel();
    let path_b = socket_path.clone();
    let handle_b = tokio::spawn(async move { server_b.serve(&path_b, shutdown_b_rx, Some(bound_b_tx)).await });
    assert!(bound_b_rx.await.unwrap().is_err(), "a second instance must not bind over a live socket");
    let _ = handle_b.await;

    let stream = UnixStream::connect(&socket_path).await.unwrap();
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
    tokio::spawn(async move {
      let _ = conn.await;
    });
    let req = Request::builder().method("GET").uri("/api/local/health").body(Empty::<Bytes>::new()).unwrap();
    let res = send(&mut sender, req).await;
    assert_eq!(res.status(), 200);
    let _ = res.into_body().collect().await;

    let _ = shutdown_a_tx.send(true);
    let _ = handle_a.await;
    assert!(!socket_path.exists());

    let mut secret_holder_c = None;
    let (deps_c, _) = test_deps(&mut secret_holder_c);
    let server_c = Server::new(deps_c, Arc::new(AtomicU64::new(0)));
    let (shutdown_c_tx, shutdown_c_rx) = watch::channel(false);
    let (bound_c_tx, bound_c_rx) = tokio::sync::oneshot::channel();
    let path_c = socket_path.clone();
    let handle_c = tokio::spawn(async move { server_c.serve(&path_c, shutdown_c_rx, Some(bound_c_tx)).await });
    bound_c_rx.await.unwrap().expect("a fresh instance can bind once the path is free again");
    let _ = shutdown_c_tx.send(true);
    let _ = handle_c.await;
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn a_dropped_shutdown_sender_stops_the_loop_instead_of_spinning() {
    let mut secret_holder = None;
    let (deps, _) = test_deps(&mut secret_holder);
    let socket_dir = std::env::temp_dir().join(format!("tmgr-it-dropped-sender-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&socket_dir);
    let socket_path = socket_dir.join("sock");

    let server = Server::new(deps, Arc::new(AtomicU64::new(0)));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let path = socket_path.clone();
    let handle = tokio::spawn(async move { server.serve(&path, shutdown_rx, None).await });
    for _ in 0..50 {
      if socket_path.exists() || handle.is_finished() {
        break;
      }
      tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(socket_path.exists());

    drop(shutdown_tx);
    let result = tokio::time::timeout(Duration::from_secs(1), handle).await;
    assert!(result.is_ok(), "serve must exit once its shutdown sender is dropped, not spin forever");
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn an_externally_unlinked_socket_lets_a_new_instance_bind_and_the_old_one_wont_delete_it() {
    let mut secret_holder_a = None;
    let (deps_a, _) = test_deps(&mut secret_holder_a);
    let socket_dir = std::env::temp_dir().join(format!("tmgr-it-external-unlink-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&socket_dir);
    let socket_path = socket_dir.join("sock");

    let server_a = Server::new(deps_a, Arc::new(AtomicU64::new(0)));
    let (shutdown_a_tx, shutdown_a_rx) = watch::channel(false);
    let (bound_a_tx, bound_a_rx) = tokio::sync::oneshot::channel();
    let path_a = socket_path.clone();
    let handle_a = tokio::spawn(async move { server_a.serve(&path_a, shutdown_a_rx, Some(bound_a_tx)).await });
    bound_a_rx.await.unwrap().expect("first instance should bind");

    // Simulates another process (or a stray cleanup) removing the file out from under a live server.
    std::fs::remove_file(&socket_path).unwrap();

    let mut secret_holder_b = None;
    let (deps_b, _) = test_deps(&mut secret_holder_b);
    let server_b = Server::new(deps_b, Arc::new(AtomicU64::new(0)));
    let (shutdown_b_tx, shutdown_b_rx) = watch::channel(false);
    let (bound_b_tx, bound_b_rx) = tokio::sync::oneshot::channel();
    let path_b = socket_path.clone();
    let handle_b = tokio::spawn(async move { server_b.serve(&path_b, shutdown_b_rx, Some(bound_b_tx)).await });
    bound_b_rx.await.unwrap().expect("a fresh bind at the recreated path must succeed");

    let _ = shutdown_a_tx.send(true);
    let _ = handle_a.await;
    assert!(socket_path.exists(), "A's shutdown must not delete B's socket file, since A no longer owns that inode");

    let stream = UnixStream::connect(&socket_path).await.unwrap();
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
    tokio::spawn(async move {
      let _ = conn.await;
    });
    let req = Request::builder().method("GET").uri("/api/local/health").body(Empty::<Bytes>::new()).unwrap();
    let res = send(&mut sender, req).await;
    assert_eq!(res.status(), 200);
    let _ = res.into_body().collect().await;

    let _ = shutdown_b_tx.send(true);
    let _ = handle_b.await;
  }

  struct GrantState {
    permissions: Mutex<Vec<String>>,
    fail: std::sync::atomic::AtomicBool,
  }

  fn sse_test_deps(secret_holder: &mut Option<String>, grant: Arc<GrantState>) -> (ServerDeps, Arc<super::EventBus>) {
    static COUNTER: AtomicU64 = AtomicU64::new(500_000);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tmgr-local-access-sse-test-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = TokenStore::open(dir, Arc::new(MemorySecrets::new())).unwrap();
    let (_info, secret) = store
      .issue(
        IssueParams {
          persona_uuid: "persona-1".into(),
          persona_name: "Reviewer".into(),
          workspace_code: "local-personal".into(),
          workspace_id: -1,
          label: "test".into(),
          expires_in_days: 90,
          plugin_id: None,
        },
        0,
      )
      .unwrap();
    *secret_holder = Some(secret);
    let events = Arc::new(super::EventBus::new("boot-sse".into()));
    let bridge: BridgeFn = {
      let grant = grant.clone();
      Arc::new(move |req: BridgeRequest| -> BoxFuture<BridgeReply> {
        let grant = grant.clone();
        Box::pin(async move {
          if req.path == GRANT_PATH {
            if grant.fail.load(Ordering::SeqCst) {
              return BridgeReply { status: 401, body: r#"{"message":"nope","code":"PERSONA_DISABLED"}"#.into() };
            }
            let permissions = grant.permissions.lock().unwrap().clone();
            return BridgeReply { status: 200, body: serde_json::json!({ "permissions": permissions }).to_string() };
          }
          BridgeReply { status: 200, body: "{}".into() }
        })
      })
    };
    let deps = ServerDeps {
      tokens: Arc::new(store),
      workspace_exists: Arc::new(|_| Some(-1)),
      ready: Arc::new(|| true),
      bridge,
      app_version: "0.0.0-test".into(),
      now_epoch: Arc::new(|| 0),
      events: events.clone(),
    };
    (deps, events)
  }

  async fn wait_for_socket(path: &Path) {
    for _ in 0..50 {
      if path.exists() {
        return;
      }
      tokio::time::sleep(Duration::from_millis(10)).await;
    }
  }

  async fn connect_client(path: &Path) -> hyper::client::conn::http1::SendRequest<Empty<Bytes>> {
    let stream = UnixStream::connect(path).await.unwrap();
    let io = TokioIo::new(stream);
    let (sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
    tokio::spawn(async move {
      let _ = conn.await;
    });
    sender
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn the_internal_grant_route_is_refused_from_outside_with_404() {
    let mut secret_holder = None;
    let grant = Arc::new(GrantState { permissions: Mutex::new(vec![]), fail: std::sync::atomic::AtomicBool::new(false) });
    let (deps, _events) = sse_test_deps(&mut secret_holder, grant);
    let socket_dir = std::env::temp_dir().join(format!("tmgr-it-sse-grant-404-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&socket_dir);
    let socket_path = socket_dir.join("sock");
    let server = Server::new(deps, Arc::new(AtomicU64::new(0)));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let path = socket_path.clone();
    let handle = tokio::spawn(async move { server.serve(&path, shutdown_rx, None).await });
    wait_for_socket(&socket_path).await;

    // No token at all: the route must be unreachable regardless, not merely unauthorized.
    let mut sender = connect_client(&socket_path).await;
    let req = Request::builder().method("GET").uri("/api/local/_grant").body(Empty::<Bytes>::new()).unwrap();
    let res = send(&mut sender, req).await;
    assert_eq!(res.status(), 404);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&body).unwrap()["code"], "NOT_FOUND");

    let _ = shutdown_tx.send(true);
    let _ = handle.await;
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn sse_stream_delivers_only_events_the_grant_permits() {
    let mut secret_holder = None;
    let grant = Arc::new(GrantState {
      permissions: Mutex::new(vec!["tasks:read".into()]),
      fail: std::sync::atomic::AtomicBool::new(false),
    });
    let (deps, events) = sse_test_deps(&mut secret_holder, grant);
    let secret = secret_holder.unwrap();
    let socket_dir = std::env::temp_dir().join(format!("tmgr-it-sse-filter-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&socket_dir);
    let socket_path = socket_dir.join("sock");
    let server = Server::new(deps, Arc::new(AtomicU64::new(0)));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let path = socket_path.clone();
    let handle = tokio::spawn(async move { server.serve(&path, shutdown_rx, None).await });
    wait_for_socket(&socket_path).await;

    let mut sender = connect_client(&socket_path).await;
    let req = Request::builder()
      .method("GET")
      .uri("/api/local/events")
      .header("X-Persona-Token", secret.as_str())
      .body(Empty::<Bytes>::new())
      .unwrap();
    let res = send(&mut sender, req).await;
    assert_eq!(res.status(), 200);
    let mut body = res.into_body();

    events.push("local-personal", "comment.created".into(), "comments:read".into(), r#"{"type":"comment.created"}"#.into());
    events.push("local-personal", "task.created".into(), "tasks:read".into(), r#"{"type":"task.created"}"#.into());

    let frame = tokio::time::timeout(Duration::from_secs(5), body.frame()).await.unwrap().unwrap().unwrap();
    let text = String::from_utf8_lossy(frame.data_ref().unwrap()).to_string();
    assert!(text.contains("event: task.created"), "expected the permitted event, got: {text}");
    assert!(!text.contains("comment.created"), "the denied event must not be forwarded: {text}");

    let _ = shutdown_tx.send(true);
    let _ = handle.await;
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn the_stream_sends_revoked_and_closes_when_the_grant_check_fails() {
    let mut secret_holder = None;
    let grant = Arc::new(GrantState { permissions: Mutex::new(vec![]), fail: std::sync::atomic::AtomicBool::new(true) });
    let (deps, _events) = sse_test_deps(&mut secret_holder, grant);
    let secret = secret_holder.unwrap();
    let socket_dir = std::env::temp_dir().join(format!("tmgr-it-sse-revoked-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&socket_dir);
    let socket_path = socket_dir.join("sock");
    let server = Server::new(deps, Arc::new(AtomicU64::new(0)));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let path = socket_path.clone();
    let handle = tokio::spawn(async move { server.serve(&path, shutdown_rx, None).await });
    wait_for_socket(&socket_path).await;

    let mut sender = connect_client(&socket_path).await;
    let req = Request::builder()
      .method("GET")
      .uri("/api/local/events")
      .header("X-Persona-Token", secret.as_str())
      .body(Empty::<Bytes>::new())
      .unwrap();
    let res = send(&mut sender, req).await;
    assert_eq!(res.status(), 200);
    let mut body = res.into_body();

    let frame = tokio::time::timeout(Duration::from_secs(5), body.frame()).await.unwrap().unwrap().unwrap();
    let text = String::from_utf8_lossy(frame.data_ref().unwrap()).to_string();
    assert!(text.contains("event: revoked"), "expected a revoked event, got: {text}");

    let next = tokio::time::timeout(Duration::from_secs(2), body.frame()).await.unwrap();
    assert!(next.is_none(), "the stream must close after revoking");

    let _ = shutdown_tx.send(true);
    let _ = handle.await;
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn a_fifth_concurrent_sse_connection_for_the_same_token_is_rate_limited() {
    let mut secret_holder = None;
    let grant = Arc::new(GrantState { permissions: Mutex::new(vec![]), fail: std::sync::atomic::AtomicBool::new(false) });
    let (deps, _events) = sse_test_deps(&mut secret_holder, grant);
    let secret = secret_holder.unwrap();
    let socket_dir = std::env::temp_dir().join(format!("tmgr-it-sse-limit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&socket_dir);
    let socket_path = socket_dir.join("sock");
    let server = Server::new(deps, Arc::new(AtomicU64::new(0)));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let path = socket_path.clone();
    let handle = tokio::spawn(async move { server.serve(&path, shutdown_rx, None).await });
    wait_for_socket(&socket_path).await;

    let mut statuses = Vec::new();
    let mut senders = Vec::new();
    for _ in 0..5 {
      let mut sender = connect_client(&socket_path).await;
      let req = Request::builder()
        .method("GET")
        .uri("/api/local/events")
        .header("X-Persona-Token", secret.as_str())
        .body(Empty::<Bytes>::new())
        .unwrap();
      let res = send(&mut sender, req).await;
      statuses.push(res.status().as_u16());
      senders.push(sender);
    }
    assert_eq!(&statuses[..4], &[200u16; 4]);
    assert_eq!(statuses[4], 429);

    let _ = shutdown_tx.send(true);
    let _ = handle.await;
  }
}
