use crate::ctx::Ctx;
use serde_json::Value;
use std::io::Read;
use std::time::Duration;
use tmgr_chat_relay::api::{Auth, Request};
use tmgr_chat_relay::engine::Transport;
use tmgr_core::text::{sanitize_line, truncate_chars};

const BODY_LIMIT: u64 = 4 << 20;
const MESSAGE_MAX: usize = 200;
const UNREACHABLE: &str = "relay unreachable";

/// A failed call is `Err(<response body>)`; a 429 with a numeric Retry-After gets `"retry_after": <secs>` merged into that JSON.
pub struct HttpRelayTransport {
    pub base_url: String,
    pub login_token: Option<String>,
}

pub fn factory() -> tmgr_runner::chat::relay::pump::TransportFactory {
    std::sync::Arc::new(|server| {
        Ok(Box::new(HttpRelayTransport {
            base_url: server.to_owned(),
            login_token: None,
        }))
    })
}

fn is_loopback(rest: &str) -> bool {
    ["127.0.0.1", "localhost", "[::1]"].iter().any(|h| {
        rest.strip_prefix(h)
            .is_some_and(|t| t.is_empty() || t.starts_with(':') || t.starts_with('/'))
    })
}

fn origin(base: &str) -> Result<String, String> {
    let base = base.trim_end_matches('/');
    let secure = match base.split_once("://") {
        Some(("https", rest)) => !rest.is_empty(),
        Some(("http", rest)) => is_loopback(rest),
        _ => false,
    };
    if !secure || base.contains('@') {
        return Err("the relay address is not one this CLI trusts".into());
    }
    Ok(base.to_owned())
}

pub(crate) const STATUS_KEY: &str = "http_status";

pub(crate) fn failure(status: u16, retry_after: Option<u64>, body: String) -> String {
    let mut object = match serde_json::from_str::<Value>(&body) {
        Ok(Value::Object(mut map)) => {
            map.remove(STATUS_KEY);
            Some(map)
        }
        _ => None,
    };
    if let (429, Some(secs)) = (status, retry_after) {
        let mut map = object.unwrap_or_default();
        map.insert("retry_after".into(), secs.into());
        return Value::Object(map).to_string();
    }
    if (400..500).contains(&status) {
        let mut map = object.unwrap_or_default();
        if map.is_empty() {
            let text = truncate_chars(&sanitize_line(body.trim()), MESSAGE_MAX).0;
            let message = if text.trim().is_empty() {
                format!("the relay answered HTTP {status}")
            } else {
                text
            };
            map.insert("message".into(), message.into());
        }
        map.insert(STATUS_KEY.into(), status.into());
        return Value::Object(map).to_string();
    }
    if let Some(map) = object.take().filter(|_| body.contains(STATUS_KEY)) {
        return Value::Object(map).to_string();
    }
    let text = truncate_chars(&sanitize_line(body.trim()), MESSAGE_MAX).0;
    if text.trim().is_empty() {
        format!("the relay answered HTTP {status}")
    } else {
        text
    }
}

impl HttpRelayTransport {
    pub(crate) fn bearer(&self, auth: Auth, device: Option<&str>) -> Result<String, String> {
        let login = self.login_token.as_deref();
        let token = match auth {
            Auth::Login => login,
            Auth::Device => device,
            Auth::DeviceOrLogin => device.or(login),
            Auth::LoginOrOwnDevice => login.or(device),
        };
        token
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| "not signed in to the relay".to_owned())
    }
}

impl Transport for HttpRelayTransport {
    fn call(&self, req: &Request, device_token: Option<&str>) -> Result<String, String> {
        let url = format!("{}{}", origin(&self.base_url)?, req.path);
        let bearer = self.bearer(req.auth, device_token)?;
        let client = reqwest::blocking::Client::builder()
            .user_agent(Ctx::user_agent())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| UNREACHABLE.to_owned())?;
        let method = reqwest::Method::from_bytes(req.method.as_str().as_bytes())
            .map_err(|_| UNREACHABLE.to_owned())?;
        let mut builder = client
            .request(method, url)
            .bearer_auth(bearer)
            .header(reqwest::header::ACCEPT, "application/json");
        for (name, value) in &req.headers {
            builder = builder.header(*name, *value);
        }
        if let Some(body) = &req.body {
            builder = builder
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.to_string());
        }
        let resp = builder.send().map_err(|_| UNREACHABLE.to_owned())?;
        let status = resp.status().as_u16();
        let retry_after = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok());
        let mut raw = Vec::new();
        resp.take(BODY_LIMIT)
            .read_to_end(&mut raw)
            .map_err(|_| UNREACHABLE.to_owned())?;
        let body = String::from_utf8_lossy(&raw).into_owned();
        if (200..300).contains(&status) {
            return Ok(body);
        }
        Err(failure(status, retry_after, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use tmgr_chat_relay::api::Method;
    use tmgr_chat_relay::engine::{post_error, retry_after_secs};

    #[test]
    fn a_404_carries_its_status_and_a_readable_message_whatever_the_body() {
        for body in ["", "<html>", r#"{"error":"not_found"}"#] {
            let v: Value = serde_json::from_str(&failure(404, None, body.into())).unwrap();
            assert_eq!(v[STATUS_KEY], 404, "{body}");
        }
        let v: Value = serde_json::from_str(&failure(404, None, "<html>".into())).unwrap();
        assert_eq!(v["message"], "<html>");
        let v: Value = serde_json::from_str(&failure(404, None, String::new())).unwrap();
        assert_eq!(v["message"], "the relay answered HTTP 404");
    }

    #[test]
    fn a_status_claimed_by_the_body_is_never_trusted() {
        let forged = r#"{"error":"x","http_status":404}"#;
        for (status, real) in [(400, Some(400)), (409, Some(409)), (500, None), (502, None)] {
            let out = failure(status, None, forged.into());
            let v: Value = serde_json::from_str(&out).unwrap();
            assert_eq!(
                v.get(STATUS_KEY).and_then(Value::as_u64),
                real,
                "{status}: {out}"
            );
        }
        let out = failure(429, Some(3), forged.into());
        let v: Value = serde_json::from_str(&out).unwrap();
        assert!(v.get(STATUS_KEY).is_none(), "{out}");
        let v: Value = serde_json::from_str(&failure(403, None, forged.into())).unwrap();
        assert_eq!(v[STATUS_KEY], 403);
    }

    #[test]
    fn every_4xx_keeps_its_real_status_whatever_the_body() {
        for status in [400u16, 401, 403, 404, 409, 410, 422] {
            for body in [
                "",
                "<html>",
                r#"{"error":"nope"}"#,
                r#"{"http_status":200}"#,
            ] {
                let v: Value = serde_json::from_str(&failure(status, None, body.into())).unwrap();
                assert_eq!(v[STATUS_KEY], status, "{status} {body}");
            }
        }
    }

    #[test]
    fn a_non_json_body_is_the_message_for_every_4xx_bounded_and_without_escapes() {
        for status in [400u16, 401, 403, 404, 409] {
            let v: Value =
                serde_json::from_str(&failure(status, None, "plain\u{1b}[2J text".into())).unwrap();
            assert_eq!(v["message"], "plain [2J text", "{status}");
            let long = "x".repeat(1000);
            let v: Value = serde_json::from_str(&failure(status, None, long)).unwrap();
            assert_eq!(v["message"].as_str().unwrap().chars().count(), 200);
        }
        let v: Value = serde_json::from_str(&failure(409, None, String::new())).unwrap();
        assert_eq!(v["message"], "the relay answered HTTP 409");
        let v: Value =
            serde_json::from_str(&failure(409, None, r#"{"error":"x"}"#.into())).unwrap();
        assert!(v.get("message").is_none());
    }

    #[test]
    fn a_server_error_body_is_kept_as_it_was() {
        assert_eq!(failure(500, None, "boom".into()), "boom");
        assert_eq!(
            failure(502, None, String::new()),
            "the relay answered HTTP 502"
        );
    }

    #[test]
    fn a_long_plain_server_error_body_is_sanitized_and_cut() {
        let body = format!("bad\x1b[2Jgateway {}", "x".repeat(5000));
        let out = failure(502, None, body);
        assert!(!out.contains('\x1b'), "{out:?}");
        assert!(out.chars().count() <= MESSAGE_MAX, "{}", out.len());
        assert!(out.starts_with("bad"));
    }

    fn serve(reply: String) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut head = String::new();
            let mut len = 0usize;
            loop {
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
                head.push_str(&line);
            }
            let mut body = vec![0u8; len];
            r.read_exact(&mut body).unwrap();
            head.push_str(&String::from_utf8(body).unwrap());
            tx.send(head).unwrap();
            s.write_all(reply.as_bytes()).unwrap();
        });
        (base, rx)
    }

    fn reply(status: &str, headers: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    fn request(auth: Auth) -> Request {
        Request {
            method: Method::Post,
            path: "/api/chat-relay/x".into(),
            auth,
            body: Some(serde_json::json!({"a": 1})),
            headers: Vec::new(),
        }
    }

    fn transport(base: String) -> HttpRelayTransport {
        HttpRelayTransport {
            base_url: base,
            login_token: Some("LOGIN".into()),
        }
    }

    #[test]
    fn success_returns_body_and_sends_expected_request() {
        let (base, rx) = serve(reply("200 OK", "", "{ok}"));
        let out = transport(base)
            .call(&request(Auth::Device), Some("DEVICE"))
            .unwrap();
        assert_eq!(out, "{ok}");
        let sent = rx.recv().unwrap();
        assert!(sent.starts_with("POST /api/chat-relay/x HTTP/1.1"));
        assert!(sent
            .to_ascii_lowercase()
            .contains("authorization: bearer device"));
        assert!(sent.ends_with("{\"a\":1}"));
    }

    #[test]
    fn the_devices_request_carries_the_kinds_header() {
        let (base, rx) = serve(reply("200 OK", "", "{}"));
        transport(base)
            .call(&tmgr_chat_relay::api::list_devices(), Some("DEVICE"))
            .unwrap();
        let sent = rx.recv().unwrap().to_ascii_lowercase();
        assert!(sent.contains("x-chat-relay-kinds: terminal"), "{sent}");
    }

    #[test]
    fn auth_picks_login_or_device() {
        let t = transport("http://127.0.0.1:1".into());
        assert_eq!(t.bearer(Auth::Login, Some("D")).unwrap(), "LOGIN");
        assert_eq!(t.bearer(Auth::Device, Some("D")).unwrap(), "D");
        assert_eq!(t.bearer(Auth::DeviceOrLogin, Some("D")).unwrap(), "D");
        assert_eq!(t.bearer(Auth::DeviceOrLogin, None).unwrap(), "LOGIN");
        assert_eq!(
            t.bearer(Auth::LoginOrOwnDevice, Some("D")).unwrap(),
            "LOGIN"
        );
        assert!(t.bearer(Auth::Device, None).is_err());
    }

    #[test]
    fn non_2xx_returns_the_response_body() {
        let (base, _rx) = serve(reply("403 Forbidden", "", "{\"error\":\"scope\"}"));
        let err = transport(base)
            .call(&request(Auth::Login), None)
            .unwrap_err();
        assert_eq!(err, "{\"error\":\"scope\",\"http_status\":403}");
    }

    #[test]
    fn empty_error_body_names_the_status() {
        let (base, _rx) = serve(reply("502 Bad Gateway", "", ""));
        let err = transport(base)
            .call(&request(Auth::Login), None)
            .unwrap_err();
        assert_eq!(err, "the relay answered HTTP 502");
    }

    #[test]
    fn rate_limit_merges_retry_after_into_the_body() {
        let (base, _rx) = serve(reply(
            "429 Too Many Requests",
            "Retry-After: 7\r\n",
            "{\"error\":\"rate_limited\"}",
        ));
        let err = transport(base)
            .call(&request(Auth::Login), None)
            .unwrap_err();
        let v: Value = serde_json::from_str(&err).unwrap();
        assert_eq!(v["error"], "rate_limited");
        assert_eq!(v["retry_after"], 7);
        assert_eq!(retry_after_secs(&post_error(err)), Some(7));
    }

    #[test]
    fn rate_limit_with_a_non_json_body_still_carries_retry_after() {
        let (base, _rx) = serve(reply(
            "429 Too Many Requests",
            "Retry-After: 3\r\n",
            "slow down",
        ));
        let err = transport(base)
            .call(&request(Auth::Login), None)
            .unwrap_err();
        assert_eq!(err, "{\"retry_after\":3}");
        assert_eq!(retry_after_secs(&post_error(err)), Some(3));
    }

    #[test]
    fn full_queue_errors_map_to_the_engine_messages() {
        let (base, _rx) = serve(reply(
            "429 Too Many Requests",
            "Retry-After: 7\r\n",
            "{\"error\":\"recipient_full\"}",
        ));
        let err = transport(base)
            .call(&request(Auth::Login), None)
            .unwrap_err();
        assert_eq!(post_error(err), "the recipient's relay queue is full");
        let (base, _rx) = serve(reply(
            "413 Payload Too Large",
            "",
            "{\"error\":\"user_full\"}",
        ));
        let err = transport(base)
            .call(&request(Auth::Login), None)
            .unwrap_err();
        assert_eq!(post_error(err), "relay storage is full");
    }

    #[test]
    fn plain_http_to_remote_host_is_refused() {
        let t = transport("http://example.com".into());
        assert!(t.call(&request(Auth::Login), None).is_err());
        assert!(origin("https://tmgr.dev/").is_ok());
        assert!(origin("http://localhost:8080").is_ok());
        assert!(origin("http://127.0.0.1.evil.com").is_err());
    }
}
