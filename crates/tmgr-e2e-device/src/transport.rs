use serde_json::Value;
use std::io::Read;
use std::time::Duration;
use tmgr_chat_relay::api::{Auth, Request};
use tmgr_chat_relay::engine::Transport;

pub struct HttpTransport {
    origin: String,
    token: String,
}

pub fn origin_of(api: &str) -> String {
    let t = api.trim_end_matches('/');
    t.strip_suffix("/api").unwrap_or(t).to_owned()
}

impl HttpTransport {
    pub fn new(api: &str, token: &str) -> Self {
        Self {
            origin: origin_of(api),
            token: token.to_owned(),
        }
    }
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("tmgr-e2e-device")
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())
}

pub fn current_user_id(api: &str, token: &str) -> Result<u64, String> {
    let url = format!("{}/api/user", origin_of(api));
    let body: Value = client()?
        .get(url)
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    let u = body.get("data").unwrap_or(&body);
    u["id"]
        .as_u64()
        .ok_or_else(|| "no user id in /api/user".into())
}

impl Transport for HttpTransport {
    fn call(&self, req: &Request, device_token: Option<&str>) -> Result<String, String> {
        let bearer = match req.auth {
            Auth::Login => Some(self.token.as_str()),
            Auth::Device => device_token,
            Auth::DeviceOrLogin => device_token.or(Some(self.token.as_str())),
            Auth::LoginOrOwnDevice => Some(self.token.as_str()),
        }
        .filter(|t| !t.is_empty())
        .ok_or("not signed in to the relay")?;
        let method = reqwest::Method::from_bytes(req.method.as_str().as_bytes())
            .map_err(|e| e.to_string())?;
        let mut b = client()?
            .request(method, format!("{}{}", self.origin, req.path))
            .bearer_auth(bearer)
            .header(reqwest::header::ACCEPT, "application/json");
        if let Some(body) = &req.body {
            b = b
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.to_string());
        }
        let resp = b.send().map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        let retry = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok());
        let mut raw = Vec::new();
        resp.take(4 << 20)
            .read_to_end(&mut raw)
            .map_err(|e| e.to_string())?;
        let body = String::from_utf8_lossy(&raw).into_owned();
        if (200..300).contains(&status) {
            return Ok(body);
        }
        if let (429, Some(secs)) = (status, retry) {
            let mut map = match serde_json::from_str::<Value>(&body) {
                Ok(Value::Object(m)) => m,
                _ => Default::default(),
            };
            map.insert("retry_after".into(), secs.into());
            return Err(Value::Object(map).to_string());
        }
        Err(if body.trim().is_empty() {
            format!("the relay answered HTTP {status}")
        } else {
            body
        })
    }
}
