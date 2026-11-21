//! Real HTTP implementation of `Transport`. The wire sits behind `HttpClient` so tests never touch the network.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tmgr_chat_relay::api::{Auth, Request};

use super::ports::Transport;

const BODY_LIMIT: usize = 4 << 20;
const UNREACHABLE: &str = "relay unreachable";

pub struct HttpResponse {
  pub status: u16,
  pub retry_after: Option<String>,
  pub body: String,
}

pub trait HttpClient: Send + Sync {
  fn send(&self, method: &str, url: &str, headers: &[(&str, String)], body: Option<String>) -> Result<HttpResponse, String>;
}

pub trait LoginToken: Send + Sync {
  fn token(&self) -> Option<String>;
}

pub struct NoLogin;

impl LoginToken for NoLogin {
  fn token(&self) -> Option<String> {
    None
  }
}

pub struct ReqwestClient;

impl HttpClient for ReqwestClient {
  fn send(&self, method: &str, url: &str, headers: &[(&str, String)], body: Option<String>) -> Result<HttpResponse, String> {
    let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|_| UNREACHABLE.to_owned())?;
    crate::local_move::ensure_tls_provider();
    let (url, headers) = (url.to_owned(), headers.to_vec());
    tauri::async_runtime::block_on(async move {
      let unreachable = |_| UNREACHABLE.to_owned();
      let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(unreachable)?;
      let mut req = client.request(method, url).header(reqwest::header::ACCEPT, "application/json");
      for (name, value) in headers {
        req = req.header(name, value);
      }
      if let Some(b) = body {
        req = req.header(reqwest::header::CONTENT_TYPE, "application/json").body(b);
      }
      let mut resp = req.send().await.map_err(unreachable)?;
      let status = resp.status().as_u16();
      let retry_after = resp.headers().get(reqwest::header::RETRY_AFTER).and_then(|v| v.to_str().ok()).map(str::to_owned);
      let mut out = Vec::new();
      while out.len() < BODY_LIMIT {
        match resp.chunk().await.map_err(unreachable)? {
          Some(chunk) => out.extend_from_slice(&chunk),
          None => break,
        }
      }
      Ok(HttpResponse { status, retry_after, body: String::from_utf8_lossy(&out).into_owned() })
    })
  }
}

pub struct HttpTransport {
  base_url: String,
  login: Arc<dyn LoginToken>,
  client: Arc<dyn HttpClient>,
}

fn is_loopback(host: &str) -> bool {
  matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

/// The origin only: request paths already carry `/api/chat-relay`.
pub(crate) fn checked_origin(base: &str) -> Result<String, String> {
  let url = url::Url::parse(base).map_err(|_| "the relay address is not valid".to_owned())?;
  let host = url.host_str().ok_or_else(|| "the relay address is not valid".to_owned())?;
  let secure = url.scheme() == "https" || (url.scheme() == "http" && is_loopback(host));
  if !secure || !url.username().is_empty() || url.password().is_some() {
    return Err("the relay address is not one this app trusts".to_owned());
  }
  Ok(match url.port() {
    Some(port) => format!("{}://{host}:{port}", url.scheme()),
    None => format!("{}://{host}", url.scheme()),
  })
}

impl HttpTransport {
  pub fn new(base_url: &str, login: Arc<dyn LoginToken>) -> Result<Self, String> {
    Self::with_client(base_url, login, Arc::new(ReqwestClient))
  }

  pub fn with_client(base_url: &str, login: Arc<dyn LoginToken>, client: Arc<dyn HttpClient>) -> Result<Self, String> {
    Ok(Self { base_url: checked_origin(base_url)?, login, client })
  }

  fn bearer(&self, auth: Auth, device_token: Option<&str>) -> Result<String, String> {
    let device = device_token.map(str::to_owned);
    let token = match auth {
      Auth::Login => self.login.token(),
      Auth::Device => device,
      Auth::DeviceOrLogin => device.or_else(|| self.login.token()),
      Auth::LoginOrOwnDevice => self.login.token().or(device),
    };
    token.filter(|t| !t.is_empty()).ok_or_else(|| "not signed in to the relay".to_owned())
  }
}

fn failure(resp: HttpResponse) -> String {
  if resp.status == 429 {
    if let Some(secs) = resp.retry_after.as_deref().and_then(|v| v.trim().parse::<u64>().ok()) {
      let mut body = match serde_json::from_str::<Value>(&resp.body) {
        Ok(Value::Object(map)) => map,
        _ => Default::default(),
      };
      body.insert("retry_after".into(), secs.into());
      return Value::Object(body).to_string();
    }
  }
  if resp.body.trim().is_empty() {
    format!("the relay answered HTTP {}", resp.status)
  } else {
    resp.body
  }
}

impl Transport for HttpTransport {
  fn call(&self, req: &Request, device_token: Option<&str>) -> Result<String, String> {
    let bearer = self.bearer(req.auth, device_token)?;
    let url = format!("{}{}", self.base_url, req.path);
    let headers = [("Authorization", format!("Bearer {bearer}"))];
    let body = req.body.as_ref().map(Value::to_string);
    let resp = self.client.send(req.method.as_str(), &url, &headers, body).map_err(|_| UNREACHABLE.to_owned())?;
    if (200..300).contains(&resp.status) {
      Ok(resp.body)
    } else {
      Err(failure(resp))
    }
  }
}

#[cfg(test)]
mod tests {
  use std::sync::Mutex;

  use super::*;

  type Sent = (String, String, Vec<(String, String)>, Option<String>);

  struct Fake {
    reply: Mutex<Option<Result<HttpResponse, String>>>,
    sent: Mutex<Vec<Sent>>,
  }

  impl Fake {
    fn new(reply: Result<HttpResponse, String>) -> Arc<Self> {
      Arc::new(Self { reply: Mutex::new(Some(reply)), sent: Mutex::default() })
    }
  }

  impl HttpClient for Fake {
    fn send(&self, method: &str, url: &str, headers: &[(&str, String)], body: Option<String>) -> Result<HttpResponse, String> {
      let h = headers.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
      self.sent.lock().unwrap().push((method.into(), url.into(), h, body));
      self.reply.lock().unwrap().take().unwrap()
    }
  }

  struct Login(Option<&'static str>);

  impl LoginToken for Login {
    fn token(&self) -> Option<String> {
      self.0.map(str::to_owned)
    }
  }

  fn resp(status: u16, retry_after: Option<&str>, body: &str) -> Result<HttpResponse, String> {
    Ok(HttpResponse { status, retry_after: retry_after.map(str::to_owned), body: body.into() })
  }

  fn transport(login: Option<&'static str>, fake: &Arc<Fake>) -> HttpTransport {
    HttpTransport::with_client("https://api.example.com/api/", Arc::new(Login(login)), fake.clone()).unwrap()
  }

  fn auth_of(fake: &Fake) -> String {
    fake.sent.lock().unwrap()[0].2[0].1.clone()
  }

  #[test]
  fn builds_url_and_returns_the_2xx_body() {
    let fake = Fake::new(resp(200, None, r#"{"data":[]}"#));
    let req = tmgr_chat_relay::api::list_devices();
    assert_eq!(transport(None, &fake).call(&req, Some("dev-tok")).unwrap(), r#"{"data":[]}"#);
    let sent = fake.sent.lock().unwrap();
    assert_eq!(sent[0].0, "GET");
    assert_eq!(sent[0].1, "https://api.example.com/api/chat-relay/devices");
    assert_eq!(sent[0].3, None);
  }

  #[test]
  fn sends_the_json_body() {
    let fake = Fake::new(resp(200, None, "{}"));
    let req = tmgr_chat_relay::api::put_push_token(Some("abc")).unwrap();
    transport(None, &fake).call(&req, Some("d")).unwrap();
    assert_eq!(fake.sent.lock().unwrap()[0].3.as_deref(), Some(r#"{"push_token":"abc"}"#));
  }

  fn call_with(auth: Auth, login: Option<&'static str>, device: Option<&str>) -> (Result<String, String>, Arc<Fake>) {
    let fake = Fake::new(resp(200, None, "{}"));
    let req = Request { method: tmgr_chat_relay::api::Method::Get, path: "/api/chat-relay/x".into(), auth, body: None };
    (transport(login, &fake).call(&req, device), fake)
  }

  #[test]
  fn auth_header_follows_the_auth_variant() {
    let (r, f) = call_with(Auth::Login, Some("L"), Some("D"));
    assert!(r.is_ok());
    assert_eq!(auth_of(&f), "Bearer L");
    let (_, f) = call_with(Auth::Device, Some("L"), Some("D"));
    assert_eq!(auth_of(&f), "Bearer D");
    let (_, f) = call_with(Auth::DeviceOrLogin, Some("L"), Some("D"));
    assert_eq!(auth_of(&f), "Bearer D");
    let (_, f) = call_with(Auth::DeviceOrLogin, Some("L"), None);
    assert_eq!(auth_of(&f), "Bearer L");
    let (_, f) = call_with(Auth::LoginOrOwnDevice, Some("L"), Some("D"));
    assert_eq!(auth_of(&f), "Bearer L");
    let (_, f) = call_with(Auth::LoginOrOwnDevice, None, Some("D"));
    assert_eq!(auth_of(&f), "Bearer D");
  }

  #[test]
  fn missing_credentials_never_reach_the_wire() {
    for (auth, login, device) in [(Auth::Login, None, Some("D")), (Auth::Device, Some("L"), None), (Auth::DeviceOrLogin, None, None)] {
      let (r, f) = call_with(auth, login, device);
      assert_eq!(r.unwrap_err(), "not signed in to the relay");
      assert!(f.sent.lock().unwrap().is_empty());
    }
  }

  #[test]
  fn rate_limit_merges_retry_after_into_the_body() {
    let fake = Fake::new(resp(429, Some("7"), r#"{"error":"rate_limited"}"#));
    let req = tmgr_chat_relay::api::list_devices();
    let err = transport(None, &fake).call(&req, Some("d")).unwrap_err();
    let v: Value = serde_json::from_str(&err).unwrap();
    assert_eq!(v["retry_after"], 7);
    assert_eq!(v["error"], "rate_limited");
  }

  #[test]
  fn rate_limit_with_an_unreadable_body_still_carries_retry_after() {
    let fake = Fake::new(resp(429, Some("3"), "slow down"));
    let req = tmgr_chat_relay::api::list_devices();
    assert_eq!(transport(None, &fake).call(&req, Some("d")).unwrap_err(), r#"{"retry_after":3}"#);
  }

  #[test]
  fn rate_limit_without_a_usable_header_passes_the_body() {
    let fake = Fake::new(resp(429, Some("Wed, 21 Oct"), r#"{"error":"x"}"#));
    let req = tmgr_chat_relay::api::list_devices();
    assert_eq!(transport(None, &fake).call(&req, Some("d")).unwrap_err(), r#"{"error":"x"}"#);
  }

  #[test]
  fn conflict_and_other_errors_pass_through() {
    let fake = Fake::new(resp(409, None, r#"{"error":"duplicate"}"#));
    let req = tmgr_chat_relay::api::list_devices();
    assert_eq!(transport(None, &fake).call(&req, Some("d")).unwrap_err(), r#"{"error":"duplicate"}"#);
    let fake = Fake::new(resp(503, None, ""));
    assert_eq!(transport(None, &fake).call(&req, Some("d")).unwrap_err(), "the relay answered HTTP 503");
  }

  #[test]
  fn network_error_is_mapped_without_the_cause() {
    let fake = Fake::new(Err("connect to https://api.example.com with Bearer SECRET failed".into()));
    let req = tmgr_chat_relay::api::list_devices();
    let err = transport(None, &fake).call(&req, Some("SECRET")).unwrap_err();
    assert_eq!(err, "relay unreachable");
  }

  #[test]
  fn tokens_do_not_leak_into_errors() {
    for reply in [resp(401, None, ""), resp(500, None, "boom"), Err("x".into())] {
      let fake = Fake::new(reply);
      let req = tmgr_chat_relay::api::list_devices();
      let err = transport(Some("LOGINSECRET"), &fake).call(&req, Some("DEVSECRET")).unwrap_err();
      assert!(!err.contains("SECRET"));
    }
  }

  #[test]
  fn refuses_untrusted_base_urls() {
    let login: Arc<dyn LoginToken> = Arc::new(NoLogin);
    let fake = Fake::new(resp(200, None, ""));
    let ok = |u: &str| HttpTransport::with_client(u, login.clone(), fake.clone()).is_ok();
    assert!(ok("https://api.tmgr.dev/api/"));
    assert!(ok("http://127.0.0.1:8080"));
    assert!(ok("http://localhost:8080/api/"));
    assert!(ok("http://[::1]:8080"));
    assert!(!ok("http://api.tmgr.dev"));
    assert!(!ok("http://127.0.0.1.evil.com"));
    assert!(!ok("ftp://localhost"));
    assert!(!ok("https://user:pw@api.tmgr.dev"));
    assert!(!ok("not a url"));
  }

  #[test]
  fn base_origin_keeps_the_port_and_drops_the_path() {
    assert_eq!(checked_origin("http://localhost:8080/api/").unwrap(), "http://localhost:8080");
    assert_eq!(checked_origin("https://api.tmgr.dev/api/").unwrap(), "https://api.tmgr.dev");
  }
}
