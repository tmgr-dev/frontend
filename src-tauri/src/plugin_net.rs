use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::LazyLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::Url;
use tokio::sync::Semaphore;

const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 5 * 1024 * 1024;
const METHODS: [&str; 5] = ["GET", "POST", "PUT", "PATCH", "DELETE"];

#[derive(Deserialize)]
pub struct FetchRequest {
  url: String,
  method: Option<String>,
  headers: Option<Vec<(String, String)>>,
  body: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct FetchResponse {
  status: u16,
  headers: Vec<(String, String)>,
  body: String,
}

/// Plugins may only talk to this machine: plain http to a loopback address. Returns where to connect.
pub fn loopback_target(url: &Url) -> Result<SocketAddr, String> {
  if url.scheme() != "http" {
    return Err("plugins may only use http:// on this computer".into());
  }
  if !url.username().is_empty() || url.password().is_some() {
    return Err("credentials in the URL are not allowed".into());
  }
  let ip = match url.host() {
    Some(url::Host::Domain("localhost")) => IpAddr::V4(Ipv4Addr::LOCALHOST),
    Some(url::Host::Ipv4(ip)) if ip.is_loopback() => IpAddr::V4(ip),
    Some(url::Host::Ipv6(ip)) if ip.is_loopback() => IpAddr::V6(Ipv6Addr::LOCALHOST),
    _ => return Err("plugins may only connect to localhost".into()),
  };
  let port = url.port_or_known_default().ok_or("no port")?;
  Ok(SocketAddr::new(ip, port))
}

/// Headers the client sets itself, or that could change how the request is framed or routed.
pub fn forbidden_header(name: &str) -> bool {
  matches!(
    name.to_ascii_lowercase().as_str(),
    "host"
      | "cookie"
      | "connection"
      | "content-length"
      | "transfer-encoding"
      | "te"
      | "trailer"
      | "upgrade"
      | "expect"
      | "keep-alive"
      | "proxy-authorization"
      | "proxy-connection"
  )
}

/// At most four plugin requests at once, whatever plugins ask for.
static IN_FLIGHT: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(4));

pub async fn fetch(request: FetchRequest) -> Result<FetchResponse, String> {
  let _permit = IN_FLIGHT.acquire().await.map_err(|e| e.to_string())?;
  let url = Url::parse(&request.url).map_err(|e| e.to_string())?;
  let target = loopback_target(&url)?;
  let method = request.method.unwrap_or_else(|| "GET".into()).to_uppercase();
  if !METHODS.contains(&method.as_str()) {
    return Err(format!("method {method} is not allowed"));
  }
  let body = request.body.unwrap_or_default();
  if body.len() > MAX_REQUEST_BYTES {
    return Err("request body is larger than 1 MB".into());
  }
  // reqwest shares its TLS setup with the updater and needs a crypto provider even for plain http.
  if rustls::crypto::CryptoProvider::get_default().is_none() {
    let _ = rustls::crypto::ring::default_provider().install_default();
  }
  let host = url.host_str().unwrap_or("localhost").to_string();
  // No DNS, no proxy, no redirects: the connection can only ever reach the loopback address above.
  let client = reqwest::Client::builder()
    .no_proxy()
    .redirect(reqwest::redirect::Policy::none())
    .timeout(Duration::from_secs(30))
    .resolve(&host, target)
    .build()
    .map_err(|e| e.to_string())?;
  let mut builder = client.request(method.parse().map_err(|_| "bad method")?, url);
  for (name, value) in request.headers.unwrap_or_default().into_iter().take(32) {
    if forbidden_header(&name) {
      continue;
    }
    builder = builder.header(name, value);
  }
  if !body.is_empty() {
    builder = builder.body(body);
  }
  let mut response = builder.send().await.map_err(|e| e.to_string())?;
  let status = response.status().as_u16();
  let headers = response
    .headers()
    .iter()
    .take(64)
    .filter_map(|(name, value)| Some((name.to_string(), value.to_str().ok()?.to_string())))
    .collect();
  let mut bytes = Vec::new();
  while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
    if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
      return Err("response is larger than 5 MB".into());
    }
    bytes.extend_from_slice(&chunk);
  }
  Ok(FetchResponse { status, headers, body: String::from_utf8_lossy(&bytes).into_owned() })
}

#[tauri::command]
pub async fn plugin_fetch(request: FetchRequest) -> Result<FetchResponse, String> {
  fetch(request).await
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::{Read, Write};
  use std::net::TcpListener;

  fn target(url: &str) -> Result<SocketAddr, String> {
    loopback_target(&Url::parse(url).unwrap())
  }

  #[test]
  fn drops_headers_that_change_framing_or_routing() {
    for name in ["Host", "content-length", "Transfer-Encoding", "TE", "Upgrade", "Expect", "Connection"] {
      assert!(forbidden_header(name), "{name}");
    }
    assert!(!forbidden_header("Content-Type"));
    assert!(!forbidden_header("Authorization"));
  }

  #[test]
  fn only_plain_http_to_this_computer() {
    assert_eq!(target("http://localhost:11434/api").unwrap(), "127.0.0.1:11434".parse().unwrap());
    assert!(target("http://127.0.0.1:8080").is_ok());
    assert!(target("http://[::1]:8080").is_ok());
    assert!(target("https://localhost:8080").is_err());
    assert!(target("http://example.com").is_err());
    assert!(target("http://192.0.2.1").is_err());
    assert!(target("http://localhost.example.com").is_err());
    assert!(target("http://user:pass@localhost:1").is_err());
    assert!(target("file:///etc/passwd").is_err());
  }

  fn serve_once(reply: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
      if let Ok((mut stream, _)) = listener.accept() {
        let mut buf = [0u8; 4096];
        let _ = stream.read(&mut buf);
        let _ = stream.write_all(reply.as_bytes());
      }
    });
    port
  }

  fn request(url: String) -> FetchRequest {
    FetchRequest { url, method: None, headers: None, body: None }
  }

  #[test]
  fn fetches_from_localhost_and_does_not_follow_redirects() {
    let runtime = tauri::async_runtime::block_on(async {
      let ok = serve_once("HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: text/plain\r\n\r\nhello");
      let answer = fetch(request(format!("http://localhost:{ok}/"))).await.unwrap();
      let redirect = serve_once("HTTP/1.1 302 Found\r\nLocation: http://example.com/\r\nContent-Length: 0\r\n\r\n");
      let moved = fetch(request(format!("http://127.0.0.1:{redirect}/"))).await.unwrap();
      (answer, moved)
    });
    assert_eq!(runtime.0.status, 200);
    assert_eq!(runtime.0.body, "hello");
    assert_eq!(runtime.1.status, 302);
  }
}
