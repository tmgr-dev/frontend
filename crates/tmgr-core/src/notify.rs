use crate::config::normalize_server;
use crate::creds::Tokens;
use crate::error::{Error, Kind, Result};
use crate::redact::Secret;
use crate::text::{sanitize, sanitize_line, truncate_chars};
use reqwest::blocking::Client as Http;
use reqwest::header::{HeaderValue, AUTHORIZATION, RETRY_AFTER};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const PUSH_PATH: &str = "/api/notifications/push";
pub const ALARMS_PATH: &str = "/api/alarms";
pub const TOKEN_PREFIX: &str = "tmgrn_";
pub const NOTIFY_SCOPE: &str = "notify:send";
pub const MINT_LABEL: &str = "tmgr cli";
pub const ENV_TOKEN: &str = "TMGR_NOTIFY_TOKEN";
pub const ENV_URL: &str = "TMGR_URL";
pub const ENV_STOP_MIN: &str = "TMGR_NOTIFY_STOP_MIN_MINUTES";
pub const DEFAULT_STOP_MIN_MINUTES: f64 = 5.0;

const TITLE_MAX: usize = 120;
const BODY_MAX: usize = 1000;
const SOURCE_MAX: usize = 120;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_POLL_SECONDS: u64 = 50;
const POLL_GRACE: Duration = Duration::from_secs(10);
const MAX_RETRY_AFTER: Duration = Duration::from_secs(10);
const MAX_BODY: u64 = 64 * 1024;
const MAX_ENV_FILE: u64 = 16 * 1024;
const TERMINAL: [&str; 6] = [
    "acknowledged",
    "no_answer",
    "busy",
    "failed",
    "call_unavailable",
    "expired",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fail {
    Unauthorized,
    RateLimited(Option<u64>),
    Validation(String),
    NotFound,
    Server(u16),
    Network { timeout: bool },
}

impl Fail {
    pub fn transient(&self) -> bool {
        match self {
            Fail::RateLimited(_) | Fail::Network { .. } => true,
            Fail::Server(s) => *s >= 500,
            _ => false,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Fail::Unauthorized => "unauthorized, check TMGR_NOTIFY_TOKEN".to_string(),
            Fail::RateLimited(Some(s)) => format!("rate limited (retry after {s}s)"),
            Fail::RateLimited(None) => "rate limited".to_string(),
            Fail::Validation(m) => m.clone(),
            Fail::NotFound => "alarm not found".to_string(),
            Fail::Server(s) => format!("unexpected status {s}"),
            Fail::Network { timeout: true } => "request timed out".to_string(),
            Fail::Network { timeout: false } => "cannot reach the server".to_string(),
        }
    }

    pub fn into_error(self, token_hint: &str) -> Error {
        match self {
            Fail::Unauthorized => {
                Error::auth("the notify token was rejected").hint(token_hint.to_string())
            }
            Fail::RateLimited(r) => {
                let e = Error::internal(self_rate_message(r));
                match r {
                    Some(s) => e.hint(format!("retry in {s}s")),
                    None => e,
                }
            }
            Fail::Validation(m) => Error::usage(m),
            Fail::NotFound => Error::not_found("alarm not found"),
            Fail::Server(s) if (502..=504).contains(&s) => {
                Error::new(Kind::Offline, format!("server unavailable (HTTP {s})"))
            }
            Fail::Server(s) => Error::internal(format!("server error (HTTP {s})")),
            Fail::Network { timeout } => Error::new(
                Kind::Offline,
                if timeout {
                    "cannot reach the server (timed out)"
                } else {
                    "cannot reach the server"
                },
            )
            .hint("check your connection, or `tmgr config get server`"),
        }
    }
}

fn self_rate_message(r: Option<u64>) -> String {
    match r {
        Some(s) => format!("rate limited by the server (retry after {s}s)"),
        None => "rate limited by the server".to_string(),
    }
}

pub fn format_failure(fail: &Fail, alarm_id: Option<&str>) -> String {
    let suffix = alarm_id
        .map(|id| format!(" (alarm {id} was created; re-check with alarm_status)"))
        .unwrap_or_default();
    match fail {
        Fail::RateLimited(_) => format!("{}{suffix}", fail.describe()),
        _ => format!("error: {}{suffix}", fail.describe()),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PushPayload {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

pub fn build_push(
    title: &str,
    body: Option<&str>,
    priority: Option<&str>,
    link: Option<&str>,
    source: Option<&str>,
) -> PushPayload {
    let lower = |s: &str| s.to_ascii_lowercase();
    PushPayload {
        title: truncate_chars(sanitize_line(title).trim(), TITLE_MAX).0,
        body: body
            .map(|b| truncate_chars(sanitize(b).trim(), BODY_MAX).0)
            .filter(|b| !b.is_empty()),
        priority: priority
            .map(lower)
            .filter(|p| matches!(p.as_str(), "low" | "normal" | "high")),
        link: link
            .filter(|l| {
                let l = lower(l);
                l.starts_with("http://") || l.starts_with("https://")
            })
            .map(|l| sanitize_line(l).trim().to_string()),
        source: source.map(|s| truncate_chars(&sanitize_line(s), SOURCE_MAX).0),
    }
}

pub fn first_line(text: Option<&str>, fallback: &str) -> String {
    text.and_then(|t| t.lines().map(str::trim).find(|l| !l.is_empty()))
        .map(|l| truncate_chars(&sanitize_line(l), BODY_MAX).0)
        .filter(|l| !l.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

pub fn project_name(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| truncate_chars(&sanitize_line(n), 60).0)
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "project".to_string())
}

const NOT_NEEDS_INPUT: [&str; 3] = [
    "auth_success",
    "elicitation_complete",
    "elicitation_response",
];

pub fn notification_push(input: &Value) -> Option<PushPayload> {
    if input
        .get("notification_type")
        .and_then(Value::as_str)
        .is_some_and(|t| NOT_NEEDS_INPUT.contains(&t))
    {
        return None;
    }
    let project = project_name(input.get("cwd").and_then(Value::as_str).unwrap_or(""));
    let body = first_line(
        input.get("message").and_then(Value::as_str),
        "Claude Code needs your attention.",
    );
    Some(build_push(
        &format!("Claude Code · {project} · needs you"),
        Some(&body),
        Some("high"),
        None,
        Some(&format!("claude-code:{project}")),
    ))
}

pub fn stop_push(
    input: &Value,
    started_ms: Option<i64>,
    now_ms: i64,
    min_minutes: f64,
) -> Option<PushPayload> {
    // An unknown start (no prompt hook record, transcript not read) is sent: only a known short
    // turn is filtered out (D-026).
    if started_ms.is_some_and(|started| ((now_ms - started) as f64 / 60_000.0) < min_minutes) {
        return None;
    }
    let project = project_name(input.get("cwd").and_then(Value::as_str).unwrap_or(""));
    let body = first_line(
        input.get("last_assistant_message").and_then(Value::as_str),
        "Claude Code finished.",
    );
    Some(build_push(
        &format!("Claude Code · {project} · done"),
        Some(&body),
        Some("normal"),
        None,
        Some(&format!("claude-code:{project}")),
    ))
}

pub fn codex_push(input: &Value) -> Option<PushPayload> {
    if input.get("type").and_then(Value::as_str) != Some("agent-turn-complete") {
        return None;
    }
    let project = project_name(input.get("cwd").and_then(Value::as_str).unwrap_or(""));
    let body = first_line(
        input.get("last-assistant-message").and_then(Value::as_str),
        "Codex finished.",
    );
    Some(build_push(
        &format!("Codex · {project}"),
        Some(&body),
        Some("normal"),
        None,
        Some(&format!("codex:{project}")),
    ))
}

pub fn last_user_prompt_ms(jsonl: &str) -> Option<i64> {
    let mut last = None;
    for line in jsonl.lines() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if entry.get("type").and_then(Value::as_str) != Some("user") {
            continue;
        }
        let message = entry.get("message");
        if message.and_then(|m| m.get("role")).and_then(Value::as_str) != Some("user") {
            continue;
        }
        let real = match message.and_then(|m| m.get("content")) {
            Some(Value::String(_)) => true,
            Some(Value::Array(blocks)) => blocks
                .iter()
                .all(|b| b.get("type").and_then(Value::as_str) != Some("tool_result")),
            _ => false,
        };
        if !real {
            continue;
        }
        if let Some(ms) = entry
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|d| d.timestamp_millis())
        {
            last = Some(ms);
        }
    }
    last
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PushOutcome {
    pub status: String,
    pub channels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Alarm {
    pub id: String,
    pub status: String,
    pub delivered_at: Option<String>,
    pub acknowledged_at: Option<String>,
    pub ack_channel: Option<String>,
    pub call_status: Option<String>,
    pub call_attempts: Option<i64>,
    pub call_attempts_made: Option<i64>,
    pub created_at: Option<String>,
}

impl Alarm {
    pub fn terminal(&self) -> bool {
        TERMINAL.contains(&self.status.as_str())
    }

    pub fn parse(v: &Value) -> Option<Alarm> {
        let root = v.get("data").filter(|d| d.is_object()).unwrap_or(v);
        let id = match root.get("id")? {
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
            _ => return None,
        };
        let text = |camel: &str, snake: &str| {
            root.get(camel)
                .or_else(|| root.get(snake))
                .and_then(Value::as_str)
                .map(|s| truncate_chars(&sanitize_line(s), 100).0)
        };
        let int = |camel: &str, snake: &str| {
            root.get(camel)
                .or_else(|| root.get(snake))
                .and_then(Value::as_i64)
        };
        Some(Alarm {
            id: truncate_chars(&sanitize_line(&id), 40).0,
            status: truncate_chars(&sanitize_line(root.get("status")?.as_str()?), 40).0,
            delivered_at: text("deliveredAt", "delivered_at"),
            acknowledged_at: text("acknowledgedAt", "acknowledged_at"),
            ack_channel: text("ackChannel", "ack_channel"),
            call_status: text("callStatus", "call_status"),
            call_attempts: int("callAttempts", "call_attempts"),
            call_attempts_made: int("callAttemptsMade", "call_attempts_made"),
            created_at: text("createdAt", "created_at"),
        })
    }
}

pub fn format_alarm(alarm: &Alarm, timed_out: bool) -> String {
    let mut parts = vec![format!("alarm {}: {}", alarm.id, alarm.status)];
    if let Some(c) = &alarm.ack_channel {
        parts.push(format!("channel: {c}"));
    }
    parts.push(format!(
        "call: {}",
        alarm.call_status.as_deref().unwrap_or("none")
    ));
    if let (Some(total), Some(made)) = (alarm.call_attempts, alarm.call_attempts_made) {
        parts.push(format!("attempts: {made}/{total}"));
    }
    if timed_out {
        parts.push("timedOut: true (still not final, re-check with alarm_status)".to_string());
    }
    parts.join(", ")
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AlarmRequest {
    pub title: String,
    pub message: String,
    #[serde(rename = "ackTimeoutSeconds", skip_serializing_if = "Option::is_none")]
    pub ack_timeout_seconds: Option<u64>,
    #[serde(
        rename = "deliveryTimeoutSeconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub delivery_timeout_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call: Option<bool>,
    #[serde(rename = "callAttempts", skip_serializing_if = "Option::is_none")]
    pub call_attempts: Option<u8>,
}

pub struct WaitOptions {
    pub max_wait: Duration,
    pub poll_seconds: u64,
    pub poll_delay: Duration,
    pub retry_delays: Vec<Duration>,
}

impl Default for WaitOptions {
    fn default() -> Self {
        Self {
            max_wait: Duration::from_secs(600),
            poll_seconds: MAX_POLL_SECONDS,
            poll_delay: Duration::from_secs(1),
            retry_delays: vec![
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(4),
            ],
        }
    }
}

pub struct NotifyHttp {
    http: Http,
    base: String,
    token: Secret,
}

impl NotifyHttp {
    pub fn new(server: &str, token: &str, user_agent: &str) -> Result<Self> {
        let http = Http::builder()
            .user_agent(user_agent)
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| Error::internal("cannot initialise the HTTP client"))?;
        Ok(Self {
            http,
            base: server.trim_end_matches('/').to_string(),
            token: Secret::new(token),
        })
    }

    fn send(
        &self,
        post: bool,
        path_and_query: &str,
        body: Option<&Value>,
        timeout: Duration,
    ) -> std::result::Result<(u16, Option<u64>, Value), Fail> {
        let url = format!("{}{}", self.base, path_and_query);
        let mut req = if post {
            self.http.post(&url)
        } else {
            self.http.get(&url)
        };
        req = req.timeout(timeout);
        if let Some(b) = body {
            req = req.json(b);
        }
        let mut value = HeaderValue::from_str(&format!("Bearer {}", self.token.expose()))
            .map_err(|_| Fail::Unauthorized)?;
        value.set_sensitive(true);
        let resp = req
            .header(AUTHORIZATION, value)
            .send()
            .map_err(|e| Fail::Network {
                timeout: e.is_timeout(),
            })?;
        let status = resp.status().as_u16();
        let retry = resp
            .headers()
            .get(RETRY_AFTER)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.trim().parse::<u64>().ok());
        let mut buf = Vec::new();
        let _ = resp.take(MAX_BODY).read_to_end(&mut buf);
        let json = serde_json::from_slice::<Value>(&buf).unwrap_or_else(|_| {
            if buf.is_empty() {
                Value::Null
            } else {
                Value::String(String::from_utf8_lossy(&buf).to_string())
            }
        });
        Ok((status, retry, json))
    }

    fn classify(status: u16, retry: Option<u64>, body: &Value) -> Fail {
        match status {
            401 => Fail::Unauthorized,
            429 => Fail::RateLimited(retry),
            404 => Fail::NotFound,
            400 | 422 => {
                let raw = body
                    .get("message")
                    .and_then(Value::as_str)
                    .or_else(|| body.as_str())
                    .unwrap_or("validation error");
                Fail::Validation(truncate_chars(&sanitize_line(raw), 200).0)
            }
            s => Fail::Server(s),
        }
    }

    pub fn push(
        &self,
        payload: &PushPayload,
        timeout: Duration,
    ) -> std::result::Result<PushOutcome, Fail> {
        let body = serde_json::to_value(payload).map_err(|_| Fail::Server(0))?;
        let (status, retry, json) = self.send(true, PUSH_PATH, Some(&body), timeout)?;
        if status != 202 && status != 200 {
            return Err(Self::classify(status, retry, &json));
        }
        let data = json.get("data").unwrap_or(&json);
        Ok(PushOutcome {
            status: data
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("sent")
                .to_string(),
            channels: data
                .get("channels")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|c| c.as_str().map(sanitize_line))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    pub fn create_alarm(&self, req: &AlarmRequest) -> std::result::Result<Alarm, Fail> {
        let body = serde_json::to_value(req).map_err(|_| Fail::Server(0))?;
        let (status, retry, json) = self.send(true, ALARMS_PATH, Some(&body), DEFAULT_TIMEOUT)?;
        if !matches!(status, 200..=202) {
            return Err(Self::classify(status, retry, &json));
        }
        Alarm::parse(&json).ok_or(Fail::Server(status))
    }

    pub fn get_alarm(&self, id: &str, wait_seconds: u64) -> std::result::Result<Alarm, Fail> {
        let wait = wait_seconds.min(MAX_POLL_SECONDS);
        let path = format!("{ALARMS_PATH}/{id}?waitSeconds={wait}");
        let timeout = Duration::from_secs(wait) + POLL_GRACE;
        let (status, retry, json) = self.send(false, &path, None, timeout)?;
        if !matches!(status, 200..=202) {
            return Err(Self::classify(status, retry, &json));
        }
        Alarm::parse(&json).ok_or(Fail::Server(status))
    }

    pub fn wait_alarm(
        &self,
        id: &str,
        opts: &WaitOptions,
        on_update: &mut dyn FnMut(&Alarm),
    ) -> std::result::Result<(Alarm, bool), Fail> {
        let deadline = Instant::now() + opts.max_wait;
        let poll = opts.poll_seconds.min(MAX_POLL_SECONDS);
        let mut retries = 0usize;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let wait = poll.min(remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0));
            let started = Instant::now();
            match self.get_alarm(id, wait) {
                Err(f) => {
                    if !f.transient() || retries >= opts.retry_delays.len() {
                        return Err(f);
                    }
                    let delay = match f {
                        Fail::RateLimited(Some(s)) => Duration::from_secs(s).min(MAX_RETRY_AFTER),
                        _ => opts.retry_delays[retries],
                    };
                    retries += 1;
                    std::thread::sleep(delay);
                }
                Ok(alarm) => {
                    retries = 0;
                    on_update(&alarm);
                    if alarm.terminal() {
                        return Ok((alarm, false));
                    }
                    if Instant::now() >= deadline {
                        return Ok((alarm, true));
                    }
                    if started.elapsed() < opts.poll_delay {
                        std::thread::sleep(opts.poll_delay);
                    }
                }
            }
        }
    }
}

pub fn valid_alarm_id(id: &str) -> bool {
    (1..=18).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())
}

pub fn parse_duration_seconds(raw: &str) -> Option<u64> {
    let s = raw.trim().to_ascii_lowercase();
    let (num, mult) = match s.chars().last()? {
        's' => (&s[..s.len() - 1], 1),
        'm' => (&s[..s.len() - 1], 60),
        'h' => (&s[..s.len() - 1], 3600),
        c if c.is_ascii_digit() => (s.as_str(), 1),
        _ => return None,
    };
    let n: u64 = num.parse().ok()?;
    (n > 0).then(|| n.checked_mul(mult)).flatten()
}

#[derive(Debug, PartialEq, Eq)]
pub enum LegacyEnv {
    Missing,
    Insecure,
    Values(BTreeMap<String, String>),
}

pub fn parse_env_file(content: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let mut value = value.trim();
        for q in ['"', '\''] {
            if value.len() >= 2 && value.starts_with(q) && value.ends_with(q) {
                value = &value[1..value.len() - 1];
            }
        }
        if !key.is_empty() {
            out.insert(key.to_string(), value.to_string());
        }
    }
    out
}

pub fn legacy_env_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())?;
    Some(home.join(".config").join("tmgr-notify").join("env"))
}

pub fn read_legacy_env(path: &Path) -> LegacyEnv {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return LegacyEnv::Missing,
        Err(_) => return LegacyEnv::Insecure,
    };
    let Ok(meta) = file.metadata() else {
        return LegacyEnv::Insecure;
    };
    let euid = unsafe { libc::geteuid() };
    if !meta.is_file()
        || meta.mode() & 0o077 != 0
        || meta.uid() != euid
        || meta.len() > MAX_ENV_FILE
    {
        return LegacyEnv::Insecure;
    }
    let mut text = String::new();
    if file.take(MAX_ENV_FILE).read_to_string(&mut text).is_err() {
        return LegacyEnv::Insecure;
    }
    LegacyEnv::Values(parse_env_file(&text))
}

pub fn server_from_env_url(raw: &str) -> Result<String> {
    let mut base = raw.trim().trim_end_matches('/').to_string();
    if base.to_ascii_lowercase().ends_with("/api") {
        base.truncate(base.len() - 4);
    }
    normalize_server(&base)
}

pub fn stop_min_minutes(raw: Option<&str>) -> f64 {
    raw.and_then(|r| r.trim().parse::<f64>().ok())
        .filter(|n| n.is_finite() && *n >= 0.0)
        .unwrap_or(DEFAULT_STOP_MIN_MINUTES)
}

pub fn notify_account(login_account: &str) -> String {
    format!("{login_account}:notify")
}

#[derive(Debug)]
pub struct NotifyCred {
    pub token: Secret,
    pub id: Option<String>,
}

impl NotifyCred {
    // The credential store keeps one Tokens blob per account: the notify token goes in `access`
    // and the id needed to revoke it in `refresh`.
    pub fn to_tokens(&self) -> Tokens {
        Tokens {
            access: self.token.clone(),
            refresh: self.id.as_deref().map(Secret::new),
            expires_at: None,
        }
    }

    pub fn from_tokens(t: &Tokens) -> NotifyCred {
        NotifyCred {
            token: t.access.clone(),
            id: t.refresh.as_ref().map(|r| r.expose().to_string()),
        }
    }
}

pub fn parse_minted(body: &Value) -> Result<NotifyCred> {
    let data = body.get("data").unwrap_or(body);
    let token = data
        .get("token")
        .and_then(Value::as_str)
        .filter(|t| t.starts_with(TOKEN_PREFIX) && t.len() <= 256)
        .ok_or_else(|| Error::internal("unexpected notify token response from the server"))?;
    let id = data.get("id").and_then(|i| match i {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) if s.bytes().all(|b| b.is_ascii_digit()) => Some(s.clone()),
        _ => None,
    });
    Ok(NotifyCred {
        token: Secret::new(token),
        id,
    })
}

pub fn mint_body() -> Value {
    json!({ "label": MINT_LABEL })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_payloads_are_bounded_and_clean() {
        let p = build_push(
            &format!("a\u{1b}[31m{}", "t".repeat(300)),
            Some(&"b".repeat(2000)),
            Some("HIGH"),
            Some("javascript:alert(1)"),
            None,
        );
        assert_eq!(p.title.chars().count(), 120);
        assert!(!p.title.contains('\u{1b}'));
        assert_eq!(p.body.unwrap().chars().count(), 1000);
        assert_eq!(p.priority.as_deref(), Some("high"));
        assert!(p.link.is_none());
        let p = build_push(
            "t",
            None,
            Some("urgent"),
            Some("https://example.com/x"),
            None,
        );
        assert!(p.priority.is_none());
        assert_eq!(p.link.as_deref(), Some("https://example.com/x"));
    }

    #[test]
    fn first_line_skips_blank_lines_and_falls_back() {
        assert_eq!(first_line(Some("\n\n  hello  \nworld"), "x"), "hello");
        assert_eq!(first_line(Some("  \n"), "fallback"), "fallback");
        assert_eq!(first_line(None, "fallback"), "fallback");
    }

    #[test]
    fn notification_hook_ignores_types_that_do_not_need_input() {
        let ok = serde_json::json!({"cwd": "/work/api", "message": "Claude needs your permission to use Bash"});
        let p = notification_push(&ok).unwrap();
        assert_eq!(p.title, "Claude Code · api · needs you");
        assert_eq!(p.priority.as_deref(), Some("high"));
        assert_eq!(p.source.as_deref(), Some("claude-code:api"));
        let quiet = serde_json::json!({"notification_type": "auth_success"});
        assert!(notification_push(&quiet).is_none());
    }

    #[test]
    fn stop_hook_skips_only_a_known_short_turn() {
        let input =
            serde_json::json!({"cwd": "/work/api", "last_assistant_message": "Done.\nmore"});
        let now = 10_000_000;
        let unknown = stop_push(&input, None, now, 5.0).unwrap();
        assert_eq!(unknown.title, "Claude Code · api · done");
        assert!(stop_push(&input, Some(now - 4 * 60_000), now, 5.0).is_none());
        let p = stop_push(&input, Some(now - 6 * 60_000), now, 5.0).unwrap();
        assert_eq!(p.title, "Claude Code · api · done");
        assert_eq!(p.body.as_deref(), Some("Done."));
        assert!(stop_push(&input, Some(now), now, 0.0).is_some());
    }

    #[test]
    fn codex_hook_handles_only_finished_turns() {
        let done = serde_json::json!({"type": "agent-turn-complete", "cwd": "/w/app", "last-assistant-message": "ok"});
        assert_eq!(codex_push(&done).unwrap().title, "Codex · app");
        assert!(codex_push(&serde_json::json!({"type": "other"})).is_none());
    }

    #[test]
    fn transcript_fallback_finds_the_last_real_user_prompt() {
        let t = concat!(
            "{\"type\":\"user\",\"timestamp\":\"2026-10-12T10:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n",
            "garbage\n",
            "{\"type\":\"user\",\"timestamp\":\"2026-10-12T10:05:00Z\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\"}]}}\n",
        );
        let ms = last_user_prompt_ms(t).unwrap();
        assert_eq!(
            ms,
            chrono::DateTime::parse_from_rfc3339("2026-10-12T10:00:00Z")
                .unwrap()
                .timestamp_millis()
        );
        assert!(last_user_prompt_ms("").is_none());
    }

    #[test]
    fn alarm_views_parse_camel_and_snake_case() {
        let a = Alarm::parse(&json!({"data": {"id": 7, "status": "calling", "ackChannel": "app", "callStatus": "ringing", "callAttempts": 3, "callAttemptsMade": 1}})).unwrap();
        assert_eq!(a.id, "7");
        assert_eq!(
            format_alarm(&a, false),
            "alarm 7: calling, channel: app, call: ringing, attempts: 1/3"
        );
        assert!(format_alarm(&a, true).ends_with("(still not final, re-check with alarm_status)"));
        assert!(!a.terminal());
        let b = Alarm::parse(&json!({"id": "8", "status": "acknowledged", "ack_channel": "call"}))
            .unwrap();
        assert!(b.terminal());
        assert_eq!(b.ack_channel.as_deref(), Some("call"));
        assert!(Alarm::parse(&json!({"status": "x"})).is_none());
    }

    #[test]
    fn failures_read_like_the_typescript_tool() {
        assert_eq!(
            format_failure(&Fail::RateLimited(Some(3)), None),
            "rate limited (retry after 3s)"
        );
        assert_eq!(
            format_failure(&Fail::Unauthorized, Some("9")),
            "error: unauthorized, check TMGR_NOTIFY_TOKEN (alarm 9 was created; re-check with alarm_status)"
        );
        assert!(Fail::Server(503).transient());
        assert!(!Fail::Server(400).transient());
        assert!(!Fail::Unauthorized.transient());
    }

    #[test]
    fn durations_accept_units() {
        assert_eq!(parse_duration_seconds("5m"), Some(300));
        assert_eq!(parse_duration_seconds("90s"), Some(90));
        assert_eq!(parse_duration_seconds("2h"), Some(7200));
        assert_eq!(parse_duration_seconds("45"), Some(45));
        for bad in ["", "0", "-1", "m", "1d", "1.5m"] {
            assert_eq!(parse_duration_seconds(bad), None, "{bad}");
        }
    }

    #[test]
    fn alarm_ids_are_digits_only() {
        assert!(valid_alarm_id("12"));
        for bad in ["", "1/../2", "a", "1?x=2", &"1".repeat(40)] {
            assert!(!valid_alarm_id(bad), "{bad}");
        }
    }

    #[test]
    fn env_files_parse_quotes_and_comments() {
        let m = parse_env_file(
            "# c\nTMGR_NOTIFY_TOKEN=\"tmgrn_x\"\n\nTMGR_URL='https://a.example'\nbad\n",
        );
        assert_eq!(m["TMGR_NOTIFY_TOKEN"], "tmgrn_x");
        assert_eq!(m["TMGR_URL"], "https://a.example");
    }

    #[test]
    fn legacy_env_must_be_private_and_owned() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("env");
        assert_eq!(read_legacy_env(&p), LegacyEnv::Missing);
        std::fs::write(&p, "TMGR_NOTIFY_TOKEN=tmgrn_a\n").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(read_legacy_env(&p), LegacyEnv::Insecure);
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
        match read_legacy_env(&p) {
            LegacyEnv::Values(v) => assert_eq!(v["TMGR_NOTIFY_TOKEN"], "tmgrn_a"),
            other => panic!("{other:?}"),
        }
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&p, &link).unwrap();
        assert_eq!(read_legacy_env(&link), LegacyEnv::Insecure);
    }

    #[test]
    fn env_urls_lose_a_trailing_api_and_must_be_https_or_loopback() {
        assert_eq!(
            server_from_env_url("https://api.tmgr.dev/api/").unwrap(),
            "https://api.tmgr.dev"
        );
        assert!(server_from_env_url("http://evil.example").is_err());
        assert!(server_from_env_url("http://127.0.0.1:8080").is_ok());
    }

    #[test]
    fn stop_threshold_defaults_to_five_minutes() {
        assert_eq!(stop_min_minutes(None), 5.0);
        assert_eq!(stop_min_minutes(Some("0")), 0.0);
        assert_eq!(stop_min_minutes(Some("x")), 5.0);
        assert_eq!(stop_min_minutes(Some("-2")), 5.0);
    }

    #[test]
    fn minted_tokens_must_look_like_notify_tokens() {
        let c = parse_minted(&json!({"data": {"id": 12, "token": "tmgrn_abc"}})).unwrap();
        assert_eq!(c.id.as_deref(), Some("12"));
        assert!(parse_minted(&json!({"data": {"token": "eyJhbGci"}})).is_err());
        let round = NotifyCred::from_tokens(&c.to_tokens());
        assert_eq!(round.token.expose(), "tmgrn_abc");
        assert_eq!(round.id.as_deref(), Some("12"));
    }
}
