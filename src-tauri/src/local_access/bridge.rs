//! `TMGR mcp` subcommand: a stdio<->socket bridge for MCP clients (Codex, Claude Code) that only
//! speak stdio, run before Tauri initializes so a second app instance never starts.

use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct BridgeArgs {
  pub token_id: Option<String>,
  pub socket_path: Option<PathBuf>,
}

/// `--token-id <id>` and `--socket <path>`; unknown flags are ignored so future flags don't break old callers.
pub fn parse_args(args: &[String]) -> BridgeArgs {
  let mut result = BridgeArgs::default();
  let mut i = 0;
  while i < args.len() {
    match args[i].as_str() {
      "--token-id" if i + 1 < args.len() => {
        result.token_id = Some(args[i + 1].clone());
        i += 2;
      }
      "--socket" if i + 1 < args.len() => {
        result.socket_path = Some(PathBuf::from(&args[i + 1]));
        i += 2;
      }
      _ => i += 1,
    }
  }
  result
}

/// macOS: `~/Library/Application Support/dev.tmgr.desktop`. Elsewhere: `$XDG_DATA_HOME` (falling
/// back to `~/.local/share`) `/dev.tmgr.desktop` — same identifier Tauri's own `app_data_dir` uses.
pub fn app_data_dir_from_env(home: &str, xdg_data_home: Option<&str>) -> PathBuf {
  if cfg!(target_os = "macos") {
    Path::new(home).join("Library/Application Support/dev.tmgr.desktop")
  } else {
    let base = xdg_data_home
      .filter(|s| !s.is_empty())
      .map(PathBuf::from)
      .unwrap_or_else(|| Path::new(home).join(".local/share"));
    base.join("dev.tmgr.desktop")
  }
}

fn app_data_dir() -> Result<PathBuf, String> {
  let home = std::env::var("HOME").map_err(|_| "HOME is not set".to_string())?;
  let xdg = std::env::var("XDG_DATA_HOME").ok();
  Ok(app_data_dir_from_env(&home, xdg.as_deref()))
}

/// A raw HTTP/1.1 POST /mcp request. `Connection: close` so the server closes after one response
/// and reading to EOF on the client side is enough to know the response is complete.
pub fn build_http_request(token: &str, body: &str) -> Vec<u8> {
  let mut request = format!(
    "POST /mcp HTTP/1.1\r\nHost: localhost\r\nX-Persona-Token: {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
    body.as_bytes().len()
  )
  .into_bytes();
  request.extend_from_slice(body.as_bytes());
  request
}

/// Splits the head from the body on the blank line and reads the status code; ignores headers
/// otherwise since the caller only needs status + body.
pub fn parse_http_response(raw: &[u8]) -> Result<(u16, String), String> {
  let text = String::from_utf8_lossy(raw);
  let mut sections = text.splitn(2, "\r\n\r\n");
  let head = sections.next().filter(|h| !h.is_empty()).ok_or("empty response")?;
  let body = sections.next().unwrap_or("");
  let status_line = head.lines().next().ok_or("missing status line")?;
  let status = status_line
    .split_whitespace()
    .nth(1)
    .and_then(|code| code.parse::<u16>().ok())
    .ok_or_else(|| format!("bad status line: {status_line}"))?;
  Ok((status, body.to_string()))
}

/// The `id` of a JSON-RPC request, if it has one; a notification (no `id` member) returns `None`,
/// so its transport failure never gets an answer.
pub fn extract_request_id(line: &str) -> Option<serde_json::Value> {
  serde_json::from_str::<serde_json::Value>(line).ok().and_then(|value| value.get("id").cloned())
}

fn transport_error(id: serde_json::Value) -> String {
  serde_json::json!({
    "jsonrpc": "2.0",
    "id": id,
    "error": { "code": -32000, "message": "TMGR is not running (APP_NOT_RUNNING)" }
  })
  .to_string()
}

#[cfg(unix)]
fn resolve_token(args: &BridgeArgs) -> Result<String, String> {
  if let Some(id) = &args.token_id {
    let entry = keyring::Entry::new(super::tokens::KEYCHAIN_SERVICE, id).map_err(|e| e.to_string())?;
    return entry.get_password().map_err(|e| e.to_string());
  }
  if let Ok(token) = std::env::var("TMGR_LOCAL_TOKEN") {
    if !token.is_empty() {
      return Ok(token);
    }
  }
  Err("no token: pass --token-id <id> or set TMGR_LOCAL_TOKEN".to_string())
}

#[cfg(unix)]
fn resolve_socket(args: &BridgeArgs) -> Result<PathBuf, String> {
  if let Some(path) = &args.socket_path {
    return Ok(path.clone());
  }
  if let Ok(path) = std::env::var("TMGR_LOCAL_SOCKET") {
    if !path.is_empty() {
      return Ok(PathBuf::from(path));
    }
  }
  let data_dir = app_data_dir()?;
  let socket_path_file = data_dir.join("local-access").join("socket-path");
  if let Ok(contents) = std::fs::read_to_string(&socket_path_file) {
    let trimmed = contents.trim();
    if !trimmed.is_empty() {
      return Ok(PathBuf::from(trimmed));
    }
  }
  Ok(super::server::resolve_socket_path(&data_dir))
}

#[cfg(unix)]
fn send_request(socket_path: &Path, token: &str, line: &str) -> Result<(u16, String), String> {
  use std::os::unix::net::UnixStream;
  let mut stream = UnixStream::connect(socket_path).map_err(|e| e.to_string())?;
  stream.write_all(&build_http_request(token, line)).map_err(|e| e.to_string())?;
  stream.shutdown(std::net::Shutdown::Write).ok();
  let mut raw = Vec::new();
  stream.read_to_end(&mut raw).map_err(|e| e.to_string())?;
  parse_http_response(&raw)
}

/// Reads newline-delimited JSON-RPC from `stdin`, POSTs each line to `/mcp` over the socket, and
/// writes the response body as one line to `stdout`. Never prints the token.
#[cfg(unix)]
pub fn run<R: BufRead, W: Write, E: Write>(socket_path: &Path, token: &str, stdin: R, mut stdout: W, mut stderr: E) {
  for line in stdin.lines() {
    let Ok(line) = line else { break };
    let line = line.trim();
    if line.is_empty() {
      continue;
    }
    match send_request(socket_path, token, line) {
      Ok((202, _)) => {}
      Ok((_, body)) => {
        if !body.is_empty() {
          let _ = writeln!(stdout, "{body}");
        }
      }
      Err(error) => {
        let _ = writeln!(stderr, "[tmgr mcp] {error}");
        if let Some(id) = extract_request_id(line) {
          let _ = writeln!(stdout, "{}", transport_error(id));
        }
      }
    }
  }
}

/// Called from `main()` before Tauri initializes. Runs and exits the process when invoked as
/// `TMGR mcp ...`; otherwise returns immediately so normal startup continues.
#[cfg(unix)]
pub fn maybe_run_and_exit() {
  let args: Vec<String> = std::env::args().collect();
  if args.len() < 2 || args[1] != "mcp" {
    return;
  }
  let parsed = parse_args(&args[2..]);
  let token = match resolve_token(&parsed) {
    Ok(token) => token,
    Err(error) => {
      eprintln!("[tmgr mcp] {error}");
      std::process::exit(1);
    }
  };
  let socket_path = match resolve_socket(&parsed) {
    Ok(path) => path,
    Err(error) => {
      eprintln!("[tmgr mcp] {error}");
      std::process::exit(1);
    }
  };
  let stdin = std::io::stdin();
  let stdout = std::io::stdout();
  let stderr = std::io::stderr();
  run(&socket_path, &token, stdin.lock(), stdout.lock(), stderr.lock());
  std::process::exit(0);
}

#[cfg(not(unix))]
pub fn maybe_run_and_exit() {
  let args: Vec<String> = std::env::args().collect();
  if args.len() < 2 || args[1] != "mcp" {
    return;
  }
  eprintln!("[tmgr mcp] not supported on this platform yet");
  std::process::exit(1);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_token_id_and_socket_flags() {
    let args = parse_args(&["--token-id".into(), "lt_abc".into(), "--socket".into(), "/tmp/x.sock".into()]);
    assert_eq!(args.token_id, Some("lt_abc".into()));
    assert_eq!(args.socket_path, Some(PathBuf::from("/tmp/x.sock")));
  }

  #[test]
  fn ignores_unknown_flags_and_a_dangling_flag_without_a_value() {
    let args = parse_args(&["--weird".into(), "--token-id".into()]);
    assert_eq!(args, BridgeArgs::default());
  }

  #[test]
  fn app_data_dir_on_macos_ignores_xdg_data_home() {
    if cfg!(target_os = "macos") {
      let dir = app_data_dir_from_env("/Users/me", Some("/custom"));
      assert_eq!(dir, PathBuf::from("/Users/me/Library/Application Support/dev.tmgr.desktop"));
    }
  }

  #[test]
  fn app_data_dir_on_linux_prefers_xdg_data_home_then_falls_back() {
    if !cfg!(target_os = "macos") {
      let dir = app_data_dir_from_env("/home/me", Some("/custom"));
      assert_eq!(dir, PathBuf::from("/custom/dev.tmgr.desktop"));
      let fallback = app_data_dir_from_env("/home/me", None);
      assert_eq!(fallback, PathBuf::from("/home/me/.local/share/dev.tmgr.desktop"));
    }
  }

  #[test]
  fn builds_a_well_formed_http_request_with_content_length() {
    let req = build_http_request("tmgrl_secret", "{\"a\":1}");
    let text = String::from_utf8(req).unwrap();
    assert!(text.starts_with("POST /mcp HTTP/1.1\r\n"));
    assert!(text.contains("X-Persona-Token: tmgrl_secret\r\n"));
    assert!(text.contains("Content-Length: 7\r\n"));
    assert!(text.ends_with("{\"a\":1}"));
  }

  #[test]
  fn parses_status_and_body_from_a_raw_response() {
    let raw = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"ok\":true}";
    let (status, body) = parse_http_response(raw).unwrap();
    assert_eq!(status, 200);
    assert_eq!(body, "{\"ok\":true}");
  }

  #[test]
  fn parses_an_empty_202_body() {
    let raw = b"HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\n\r\n";
    let (status, body) = parse_http_response(raw).unwrap();
    assert_eq!(status, 202);
    assert_eq!(body, "");
  }

  #[test]
  fn rejects_a_response_with_no_status_line() {
    assert!(parse_http_response(b"").is_err());
  }

  #[test]
  fn extracts_the_request_id_and_skips_notifications() {
    assert_eq!(extract_request_id(r#"{"jsonrpc":"2.0","id":5,"method":"ping"}"#), Some(serde_json::json!(5)));
    assert_eq!(extract_request_id(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#), None);
    assert_eq!(extract_request_id("not json"), None);
  }

  #[cfg(unix)]
  #[test]
  fn round_trips_a_request_against_a_fake_unix_socket_server() {
    use std::os::unix::net::UnixListener;
    let dir = std::env::temp_dir().join(format!("tmgr-bridge-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let socket_path = dir.join("fake.sock");
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path).unwrap();

    let server = std::thread::spawn(move || {
      let (mut stream, _) = listener.accept().unwrap();
      let mut buf = [0u8; 4096];
      let n = stream.read(&mut buf).unwrap();
      let request = String::from_utf8_lossy(&buf[..n]);
      assert!(request.contains("X-Persona-Token: tmgrl_test"));
      assert!(request.ends_with("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}"));
      let response = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}";
      stream.write_all(response).unwrap();
    });

    let (status, body) =
      send_request(&socket_path, "tmgrl_test", r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#).unwrap();
    server.join().unwrap();
    assert_eq!(status, 200);
    assert_eq!(body, r#"{"jsonrpc":"2.0","id":1,"result":{}}"#);

    let _ = std::fs::remove_dir_all(&dir);
  }

  #[cfg(unix)]
  #[test]
  fn run_writes_a_json_rpc_transport_error_for_a_request_but_nothing_for_a_notification() {
    let missing_socket = std::env::temp_dir().join(format!("tmgr-bridge-missing-{}.sock", std::process::id()));
    let input = format!(
      "{{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}}\n{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}}\n"
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    run(&missing_socket, "tmgrl_test", input.as_bytes(), &mut stdout, &mut stderr);
    let out = String::from_utf8(stdout).unwrap();
    assert_eq!(out.lines().count(), 1);
    let value: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(value["id"], 7);
    assert_eq!(value["error"]["code"], -32000);
    assert!(!String::from_utf8(stderr).unwrap().is_empty());
  }
}
