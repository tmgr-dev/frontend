#![cfg(unix)]

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::header::HeaderMap;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::Serialize;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::watch;

use super::tokens::{chmod, TokenStore};

const MAX_BODY_BYTES: usize = 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 5 * 1024 * 1024;
const BRIDGE_TIMEOUT: Duration = Duration::from_secs(15);
const RATE_LIMIT_GET_PER_SEC: u32 = 50;
const RATE_LIMIT_WRITE_PER_SEC: u32 = 10;
const HEALTH_PATH: &str = "/api/local/health";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeRequest {
  pub id: u64,
  pub workspace_code: String,
  pub workspace_id: i64,
  pub persona_uuid: String,
  pub persona_name: String,
  pub token_id: String,
  pub method: String,
  pub path: String,
  pub body: Option<String>,
}

#[derive(Clone, Debug)]
pub struct BridgeReply {
  pub status: u16,
  pub body: String,
}

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

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

#[derive(Serialize)]
struct ErrorBody<'a> {
  message: &'a str,
  code: &'a str,
}

fn json_error(status: u16, code: &str, message: &str) -> Response<Full<Bytes>> {
  json_response(status, &ErrorBody { message, code }, &[])
}

fn json_error_retry(status: u16, code: &str, message: &str, retry_after_secs: u32) -> Response<Full<Bytes>> {
  json_response(status, &ErrorBody { message, code }, &[("Retry-After", retry_after_secs.to_string())])
}

fn json_response(status: u16, body: &impl Serialize, extra_headers: &[(&str, String)]) -> Response<Full<Bytes>> {
  let mut builder = Response::builder()
    .status(StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR))
    .header("Content-Type", "application/json");
  for (name, value) in extra_headers {
    builder = builder.header(*name, value);
  }
  builder.body(Full::new(Bytes::from(serde_json::to_vec(body).unwrap_or_default()))).unwrap()
}

async fn read_body_limited(body: Incoming) -> Result<Vec<u8>, ()> {
  match Limited::new(body, MAX_BODY_BYTES).collect().await {
    Ok(collected) => Ok(collected.to_bytes().to_vec()),
    Err(_) => Err(()),
  }
}

fn health_response(deps: &ServerDeps, headers: &HeaderMap) -> Response<Full<Bytes>> {
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

async fn handle(
  req: Request<Incoming>,
  deps: Arc<ServerDeps>,
  limiter: Arc<RateLimiter>,
  next_id: Arc<AtomicU64>,
) -> Response<Full<Bytes>> {
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

fn json_raw_response(status: u16, body: String) -> Response<Full<Bytes>> {
  Response::builder()
    .status(StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR))
    .header("Content-Type", "application/json")
    .body(Full::new(Bytes::from(body)))
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
}

impl Server {
  pub fn new(deps: ServerDeps) -> Self {
    Self { deps: Arc::new(deps), limiter: Arc::new(RateLimiter::default()), next_id: Arc::new(AtomicU64::new(0)) }
  }

  /// Runs until `shutdown` carries `true`, then removes the socket file. Bind happens inside this
  /// async call (never synchronously in `setup()`), since `UnixListener::bind` needs a running
  /// reactor. Every connection watches the same signal, so the kill switch does not leave existing
  /// keep-alive connections answering requests after the listener itself is gone.
  pub async fn serve(&self, socket_path: &Path, mut shutdown: watch::Receiver<bool>) -> Result<(), String> {
    prepare_socket_path(socket_path).await?;
    let listener = UnixListener::bind(socket_path).map_err(|e| format!("bind {}: {e}", socket_path.display()))?;
    chmod(socket_path, 0o600).map_err(|e| format!("chmod {}: {e}", socket_path.display()))?;
    loop {
      tokio::select! {
        _ = shutdown.changed() => {
          if *shutdown.borrow() {
            break;
          }
        }
        accepted = listener.accept() => {
          let Ok((stream, _)) = accepted else { continue };
          let deps = self.deps.clone();
          let limiter = self.limiter.clone();
          let next_id = self.next_id.clone();
          let mut conn_shutdown = shutdown.clone();
          tauri::async_runtime::spawn(async move {
            let io = TokioIo::new(stream);
            let svc = service_fn(move |req| {
              let deps = deps.clone();
              let limiter = limiter.clone();
              let next_id = next_id.clone();
              async move { Ok::<_, std::convert::Infallible>(handle(req, deps, limiter, next_id).await) }
            });
            let conn = hyper::server::conn::http1::Builder::new().serve_connection(io, svc);
            let mut conn = std::pin::pin!(conn);
            tokio::select! {
              result = conn.as_mut() => { let _ = result; }
              _ = conn_shutdown.changed() => {
                if *conn_shutdown.borrow() {
                  conn.as_mut().graceful_shutdown();
                  let _ = conn.await;
                }
              }
            }
          });
        }
      }
    }
    let _ = std::fs::remove_file(socket_path);
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
    let dir = std::env::temp_dir().join(format!("tmgr-local-access-server-test-{}", std::process::id()));
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
      Box::pin(async move {
        let echoed = serde_json::json!({
          "personaUuid": req.persona_uuid,
          "method": req.method,
          "path": req.path,
        });
        BridgeReply { status: 200, body: echoed.to_string() }
      })
    });
    let deps = ServerDeps {
      tokens: Arc::new(store),
      workspace_exists: Arc::new(|_| Some(-1)),
      ready: Arc::new(|| true),
      bridge,
      app_version: "0.0.0-test".into(),
      now_epoch: Arc::new(|| 0),
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

    let server = Server::new(deps);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let socket_path_clone = socket_path.clone();
    let handle = tokio::spawn(async move { server.serve(&socket_path_clone, shutdown_rx).await });

    // Give the listener a moment to bind.
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

    // No token → 401 TOKEN_MISSING.
    let mut sender = connect().await;
    let req = Request::builder().method("GET").uri("/api/tasks").body(Empty::<Bytes>::new()).unwrap();
    let res = send(&mut sender, req).await;
    assert_eq!(res.status(), 401);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&body).unwrap()["code"], "TOKEN_MISSING");

    // Cloud token prefix → 401 CLOUD_TOKEN.
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

    // A human-token header next to it → 400 HUMAN_TOKEN, even with no persona token at all.
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

    // A valid token bridges through with the actor from the token and no headers forwarded.
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
    assert!(!json.to_string().contains("should-never-arrive"));

    // 429 after 11 writes in the same second.
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

    // The kill switch closes connections that are already open, not just future ones.
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
}
