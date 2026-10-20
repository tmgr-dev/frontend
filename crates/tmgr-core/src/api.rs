use crate::creds::{CredentialStore, Tokens};
use crate::error::{Error, Kind, Result};
use crate::models::{ApiComment, ApiTask, Category, DeviceCode, DeviceInfo, Status, Workspace};
use crate::redact::Secret;
use crate::text::{sanitize_line, truncate_chars};
use reqwest::blocking::{Client as Http, Response};
use reqwest::header::{HeaderValue, AUTHORIZATION};
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::io::Read;
use std::time::Duration;

pub const DEFAULT_SCOPES: [&str; 7] = [
    "workspaces:read",
    "tasks:read",
    "tasks:write",
    "tasks:create",
    "comments:create",
    "time:read",
    "time:write",
];

const MAX_BODY: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct Warning {
    pub code: String,
    pub message: String,
}

impl Warning {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Auth {
    None,
    Bearer,
}

pub enum DevicePoll {
    Pending,
    SlowDown,
    Expired,
    Denied,
    Granted(Tokens, Vec<String>),
}

pub struct TaskPage {
    pub items: Vec<ApiTask>,
    pub current_page: u64,
    pub last_page: u64,
}

pub struct Client {
    http: Http,
    base: String,
    host: String,
    tokens: RefCell<Option<Tokens>>,
    store: Option<CredentialStore>,
    verbose: bool,
    warnings: RefCell<Vec<Warning>>,
    workspace_header: Cell<Option<i64>>,
    local: Option<crate::local::Local>,
    get_retries: Cell<u32>,
    retry_pause: Cell<Duration>,
}

impl Client {
    pub fn new(
        server: &str,
        user_agent: &str,
        tokens: Option<Tokens>,
        store: Option<CredentialStore>,
        verbose: bool,
    ) -> Result<Self> {
        let http = Http::builder()
            .user_agent(user_agent)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| Error::internal("cannot initialise the HTTP client"))?;
        let host = server
            .split_once("://")
            .map(|(_, r)| r)
            .unwrap_or(server)
            .to_string();
        Ok(Self {
            http,
            base: server.trim_end_matches('/').to_string(),
            host,
            tokens: RefCell::new(tokens),
            store,
            verbose,
            warnings: RefCell::new(Vec::new()),
            workspace_header: Cell::new(None),
            local: None,
            get_retries: Cell::new(0),
            retry_pause: Cell::new(Duration::from_millis(250)),
        })
    }

    /// A client whose requests go to a local workspace instead of the server.
    pub fn for_local(local: crate::local::Local, user_agent: &str, verbose: bool) -> Result<Self> {
        let mut client = Client::new("local://workspace", user_agent, None, None, verbose)?;
        client.local = Some(local);
        Ok(client)
    }

    pub fn local(&self) -> Option<&crate::local::Local> {
        self.local.as_ref()
    }

    /// The id of the user this client acts as: the local identity, or the logged-in device's user.
    pub fn current_user_id(&self) -> Result<Option<i64>> {
        match &self.local {
            Some(local) => Ok(Some(local.user().id)),
            None => Ok(self.current_device()?.user.id),
        }
    }

    pub fn set_get_retries(&self, retries: u32, pause: Duration) {
        self.get_retries.set(retries);
        self.retry_pause.set(pause);
    }

    pub fn take_warnings(&self) -> Vec<Warning> {
        std::mem::take(&mut self.warnings.borrow_mut())
    }

    pub fn has_tokens(&self) -> bool {
        self.tokens.borrow().is_some()
    }

    pub fn token_expires_at(&self) -> Option<String> {
        self.tokens
            .borrow()
            .as_ref()
            .and_then(|t| t.expires_at.clone())
    }

    pub fn set_workspace_header(&self, workspace_id: Option<i64>) {
        self.workspace_header.set(workspace_id);
    }

    pub fn set_tokens(&self, tokens: Tokens) {
        *self.tokens.borrow_mut() = Some(tokens);
    }

    fn send_raw(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
        auth: Auth,
    ) -> Result<(u16, Value)> {
        if self.local.is_some() {
            return Err(Error::usage("this is not available for local workspaces")
                .hint("use a cloud workspace: --ws <code>"));
        }
        let url = format!("{}{}", self.base, path);
        if self.verbose {
            eprintln!("> {method} {path}");
        }
        let is_get = method == Method::GET;
        let mut req = self.http.request(method, &url).query(query);
        if let Some(b) = body {
            req = req.json(b);
        }
        if auth == Auth::Bearer {
            let tokens = self.tokens.borrow();
            let token = tokens.as_ref().ok_or_else(Error::not_logged_in)?;
            let mut value = HeaderValue::from_str(&format!("Bearer {}", token.access.expose()))
                .map_err(|_| Error::auth("stored token is malformed").hint("tmgr login"))?;
            value.set_sensitive(true);
            req = req.header(AUTHORIZATION, value);
            if let Some(id) = self.workspace_header.get() {
                req = req.header("X-Workspace-Id", id.to_string());
            }
        }
        let mut attempt = 0;
        let resp = loop {
            let again = if is_get && attempt < self.get_retries.get() {
                req.try_clone()
            } else {
                None
            };
            let sent = match again {
                Some(copy) => copy.send(),
                None => break req.send().map_err(|e| self.network_error(&e))?,
            };
            match sent {
                Ok(resp) => break resp,
                Err(e) if e.is_timeout() => return Err(self.network_error(&e)),
                Err(_) => {
                    std::thread::sleep(self.retry_pause.get() * 3u32.pow(attempt));
                    attempt += 1;
                }
            }
        };
        let status = resp.status().as_u16();
        if self.verbose {
            eprintln!("< {status}");
        }
        let retry_after = (status == 429)
            .then(|| resp.headers().get("retry-after"))
            .flatten()
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok());
        let mut value = read_json(resp)?;
        if let (Some(secs), Value::Object(m)) = (retry_after, &mut value) {
            m.insert("retry_after_s".into(), Value::from(secs));
        }
        Ok((status, value))
    }

    fn network_error(&self, e: &reqwest::Error) -> Error {
        let why = if e.is_timeout() {
            "timed out"
        } else if e.is_connect() {
            "connection failed"
        } else {
            "request failed"
        };
        Error::new(Kind::Offline, format!("cannot reach {} ({why})", self.host))
            .hint("check your connection, or `tmgr config get server`")
    }

    pub(crate) fn request(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
        scope: &str,
    ) -> Result<Value> {
        let (status, value) = self.request_status(method, path, query, body)?;
        if (200..300).contains(&status) {
            Ok(value)
        } else if self.local.is_some() {
            Err(crate::local::status_error(status, &value))
        } else {
            Err(map_error(status, &value, scope))
        }
    }

    pub(crate) fn request_status(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<(u16, Value)> {
        if let Some(local) = &self.local {
            return self.local_send(local, method.as_str(), path, query, body);
        }
        let sent_access = self.current_access()?;
        let (mut status, mut value) =
            self.send_raw(method.clone(), path, query, body, Auth::Bearer)?;
        if status == 401 && self.recover(&sent_access)? {
            (status, value) = self.send_raw(method, path, query, body, Auth::Bearer)?;
        }
        Ok((status, value))
    }

    pub fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        refresh: bool,
    ) -> Result<(u16, Value)> {
        let method = Method::from_bytes(method.as_bytes())
            .map_err(|_| Error::internal("invalid HTTP method"))?;
        if let Some(local) = &self.local {
            return self.local_send(local, method.as_str(), path, &[], body);
        }
        let sent_access = self.current_access()?;
        let (mut status, mut value) =
            self.send_raw(method.clone(), path, &[], body, Auth::Bearer)?;
        if status == 401 && refresh && self.recover(&sent_access)? {
            (status, value) = self.send_raw(method, path, &[], body, Auth::Bearer)?;
        }
        Ok((status, value))
    }

    pub fn call_guarded(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        allow_resend: &dyn Fn() -> bool,
    ) -> Result<Option<(u16, Value)>> {
        let method = Method::from_bytes(method.as_bytes())
            .map_err(|_| Error::internal("invalid HTTP method"))?;
        if let Some(local) = &self.local {
            return self
                .local_send(local, method.as_str(), path, &[], body)
                .map(Some);
        }
        let sent_access = self.current_access()?;
        let (mut status, mut value) =
            self.send_raw(method.clone(), path, &[], body, Auth::Bearer)?;
        if status == 401 && self.recover(&sent_access)? {
            if !allow_resend() {
                return Ok(None);
            }
            (status, value) = self.send_raw(method, path, &[], body, Auth::Bearer)?;
        }
        Ok(Some((status, value)))
    }

    fn local_send(
        &self,
        local: &crate::local::Local,
        method: &str,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<(u16, Value)> {
        if self.verbose {
            eprintln!(
                "> {method} {path} (local {})",
                local.workspace().manifest.code
            );
        }
        let (status, value) = local.send(method, path, query, body)?;
        if self.verbose {
            eprintln!("< {status}");
        }
        Ok((status, value))
    }

    fn current_access(&self) -> Result<String> {
        self.tokens
            .borrow()
            .as_ref()
            .map(|t| t.access.expose().to_string())
            .ok_or_else(Error::not_logged_in)
    }

    fn recover(&self, failed_access: &str) -> Result<bool> {
        let guard = match &self.store {
            Some(store) => Some(store.lock()?),
            None => None,
        };
        if let Some(store) = &self.store {
            if let Some((stored, _)) = store.load()? {
                let rotated = stored.access.expose() != failed_access;
                self.set_tokens(stored);
                if rotated {
                    return Ok(true);
                }
            }
        }
        let refresh = self
            .tokens
            .borrow()
            .as_ref()
            .and_then(|t| t.refresh.clone())
            .ok_or_else(session_expired)?;
        let (status, body) = self.send_raw(
            Method::POST,
            "/api/auth/refresh",
            &[],
            Some(&json!({ "refreshToken": refresh.expose() })),
            Auth::None,
        )?;
        if !(200..300).contains(&status) {
            return if status >= 500 {
                Err(map_error(status, &body, "auth"))
            } else {
                Err(session_expired())
            };
        }
        let data = unwrap_data(body);
        let access = data
            .get("token")
            .or_else(|| data.get("access_token"))
            .and_then(Value::as_str)
            .ok_or_else(|| Error::internal("unexpected refresh response from the server"))?;
        let new_refresh = data
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(Secret::new)
            .unwrap_or(refresh);
        let expires_at = data
            .get("expires_in")
            .and_then(Value::as_i64)
            .map(expires_in_to_rfc3339);
        let tokens = Tokens {
            access: Secret::new(access),
            refresh: Some(new_refresh),
            expires_at,
        };
        if let (Some(store), Some(guard)) = (&self.store, &guard) {
            match store.save_locked(guard, &tokens) {
                Ok(saved) => {
                    if let Some(w) = saved.warning {
                        self.warnings
                            .borrow_mut()
                            .push(Warning::new("credential_store_fallback", w));
                    }
                }
                Err(_) => self.warnings.borrow_mut().push(Warning::new(
                    "credentials_not_saved",
                    "could not save the refreshed token; the next command may need `tmgr login`",
                )),
            }
        }
        self.set_tokens(tokens);
        Ok(true)
    }

    pub fn device_code(
        &self,
        client_name: &str,
        host_label: &str,
        scopes: &[String],
    ) -> Result<DeviceCode> {
        let (status, body) = self.send_raw(
            Method::POST,
            "/api/auth/device/code",
            &[],
            Some(&json!({
                "client_name": client_name,
                "host_label": host_label,
                "scopes": scopes,
            })),
            Auth::None,
        )?;
        if !(200..300).contains(&status) {
            return Err(map_error(status, &body, "auth"));
        }
        parse(unwrap_data(body))
    }

    pub fn device_token(&self, device_code: &str) -> Result<DevicePoll> {
        let (status, body) = self.send_raw(
            Method::POST,
            "/api/auth/device/token",
            &[],
            Some(&json!({ "device_code": device_code })),
            Auth::None,
        )?;
        if (200..300).contains(&status) {
            let data = unwrap_data(body);
            let access = data
                .get("access_token")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::internal("unexpected token response from the server"))?;
            let refresh = data
                .get("refresh_token")
                .and_then(Value::as_str)
                .map(Secret::new);
            let expires_at = data
                .get("expires_in")
                .and_then(Value::as_i64)
                .map(expires_in_to_rfc3339);
            let scopes = data
                .get("scopes")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            return Ok(DevicePoll::Granted(
                Tokens {
                    access: Secret::new(access),
                    refresh,
                    expires_at,
                },
                scopes,
            ));
        }
        match body.get("error").and_then(Value::as_str) {
            Some("authorization_pending") => Ok(DevicePoll::Pending),
            Some("slow_down") => Ok(DevicePoll::SlowDown),
            Some("expired_token") => Ok(DevicePoll::Expired),
            Some("access_denied") => Ok(DevicePoll::Denied),
            _ => Err(map_error(status, &body, "auth")),
        }
    }

    pub fn current_device(&self) -> Result<DeviceInfo> {
        parse(unwrap_data(self.request(
            Method::GET,
            "/api/auth/devices/current",
            &[],
            None,
            "any",
        )?))
    }

    pub fn logout(&self) -> Result<()> {
        let path = "/api/auth/devices/current/logout";
        let sent = self.current_access()?;
        let (mut status, mut body) = self.send_raw(Method::POST, path, &[], None, Auth::Bearer)?;
        if status == 401 && self.recover(&sent)? {
            (status, body) = self.send_raw(Method::POST, path, &[], None, Auth::Bearer)?;
        }
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(map_error(status, &body, "any"))
        }
    }

    pub fn workspaces(&self) -> Result<Vec<Workspace>> {
        items(self.request(Method::GET, "/api/workspaces", &[], None, "workspaces:read")?)
    }

    pub fn statuses(&self, workspace_id: i64) -> Result<Vec<Status>> {
        items(self.request(
            Method::GET,
            &format!("/api/workspaces/{workspace_id}/statuses"),
            &[],
            None,
            "workspaces:read",
        )?)
    }

    pub fn categories(&self, workspace_id: i64) -> Result<Vec<Category>> {
        items(self.request(
            Method::GET,
            "/api/project_categories",
            &[("workspace_id", workspace_id.to_string())],
            None,
            "workspaces:read",
        )?)
    }

    pub fn task_by_id(&self, id: u64) -> Result<ApiTask> {
        parse(unwrap_data(self.request(
            Method::GET,
            &format!("/api/tasks/{id}"),
            &[],
            None,
            "tasks:read",
        )?))
    }

    pub fn task_by_key(&self, key: &str, workspace_id: i64) -> Result<ApiTask> {
        if crate::refs::parse_key(key).is_none() {
            return Err(Error::usage("invalid task key"));
        }
        parse(unwrap_data(self.request(
            Method::GET,
            &format!("/api/tasks/by-key/{key}"),
            &[("workspace_id", workspace_id.to_string())],
            None,
            "tasks:read",
        )?))
    }

    pub fn comments(&self, task_id: u64) -> Result<Vec<ApiComment>> {
        items(self.request(
            Method::GET,
            &format!("/api/tasks/{task_id}/comments"),
            &[],
            None,
            "tasks:read",
        )?)
    }

    pub fn tasks_page(
        &self,
        workspace_id: i64,
        category_id: Option<u64>,
        status_id: Option<u64>,
        page: u64,
        per_page: u64,
    ) -> Result<TaskPage> {
        let mut query = vec![
            ("workspace_id", workspace_id.to_string()),
            ("page", page.to_string()),
            ("per_page", per_page.to_string()),
            ("fields", "card".to_string()),
        ];
        if let Some(c) = category_id {
            query.push(("project_category_id", c.to_string()));
        }
        if let Some(s) = status_id {
            query.push(("status_id", s.to_string()));
        }
        let body = self.request(Method::GET, "/api/tasks", &query, None, "tasks:read")?;
        let meta = body.get("meta").cloned().unwrap_or(Value::Null);
        let current_page = meta
            .get("current_page")
            .and_then(Value::as_u64)
            .unwrap_or(page);
        let last_page = meta
            .get("last_page")
            .and_then(Value::as_u64)
            .unwrap_or(current_page);
        Ok(TaskPage {
            items: items(body)?,
            current_page,
            last_page,
        })
    }

    pub fn time_summary_total(&self, workspace_id: i64, from: &str, to: &str) -> Result<i64> {
        let body = self.request(
            Method::GET,
            &format!("/api/workspaces/{workspace_id}/time-summary"),
            &[
                ("from", from.to_string()),
                ("to", to.to_string()),
                ("group_by", "task".to_string()),
            ],
            None,
            "time:read",
        )?;
        let data = unwrap_data(body);
        data.get("total_s")
            .or_else(|| data.get("total_seconds"))
            .and_then(Value::as_i64)
            .ok_or_else(|| Error::internal("unexpected time summary response"))
    }

    pub fn create_task(&self, body: &Value) -> Result<ApiTask> {
        parse(unwrap_data(self.request(
            Method::POST,
            "/api/tasks",
            &[],
            Some(body),
            "tasks:create",
        )?))
    }

    pub fn running_tasks(&self) -> Result<Vec<ApiTask>> {
        let tasks: Vec<ApiTask> =
            items(self.request(Method::GET, "/api/tasks/runned", &[], None, "tasks:read")?)?;
        Ok(tasks
            .into_iter()
            .filter(|t| t.start_time.is_some_and(|s| s > 0))
            .collect())
    }

    pub fn start_countdown(&self, task_id: u64) -> Result<ApiTask> {
        parse(unwrap_data(self.request(
            Method::POST,
            &format!("/api/tasks/{task_id}/countdown"),
            &[],
            None,
            "time:write",
        )?))
    }

    pub fn stop_countdown(&self, task_id: u64) -> Result<ApiTask> {
        parse(unwrap_data(self.request(
            Method::DELETE,
            &format!("/api/tasks/{task_id}/countdown"),
            &[],
            None,
            "time:write",
        )?))
    }

    pub fn set_task_status(&self, task_id: u64, status_id: u64) -> Result<()> {
        self.request(
            Method::PUT,
            &format!("/api/tasks/{task_id}/{status_id}"),
            &[],
            None,
            "tasks:write",
        )
        .map(|_| ())
    }

    pub fn agent_work(&self, task_id: u64) -> Result<Value> {
        Ok(unwrap_data(self.request(
            Method::GET,
            &format!("/api/tasks/{task_id}/agent-work"),
            &[],
            None,
            "tasks:read",
        )?))
    }

    pub fn add_comment(&self, task_id: u64, message: &str) -> Result<()> {
        self.request(
            Method::POST,
            &format!("/api/tasks/{task_id}/comments"),
            &[],
            Some(&json!({ "message": message })),
            "comments:create",
        )
        .map(|_| ())
    }
}

fn session_expired() -> Error {
    Error::auth("session expired or revoked").hint("tmgr login")
}

fn read_json(resp: Response) -> Result<Value> {
    let mut buf = Vec::new();
    resp.take(MAX_BODY)
        .read_to_end(&mut buf)
        .map_err(|_| Error::new(Kind::Offline, "connection lost while reading the response"))?;
    if buf.iter().all(u8::is_ascii_whitespace) {
        return Ok(Value::Null);
    }
    Ok(serde_json::from_slice(&buf).unwrap_or(Value::Null))
}

pub fn unwrap_data(v: Value) -> Value {
    match v {
        Value::Object(mut m) if m.contains_key("data") && !m.contains_key("title") => {
            m.remove("data").unwrap_or(Value::Null)
        }
        other => other,
    }
}

pub(crate) fn items<T: DeserializeOwned>(v: Value) -> Result<Vec<T>> {
    match unwrap_data(v) {
        Value::Array(a) => a.into_iter().map(parse).collect(),
        Value::Null => Ok(Vec::new()),
        _ => Err(unexpected()),
    }
}

pub(crate) fn parse<T: DeserializeOwned>(v: Value) -> Result<T> {
    serde_json::from_value(v).map_err(|_| unexpected())
}

pub(crate) fn unexpected() -> Error {
    Error::internal("unexpected response from the server")
}

fn expires_in_to_rfc3339(seconds: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::seconds(seconds))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn scopes_hint(needed: &str) -> String {
    let mut scopes: Vec<&str> = DEFAULT_SCOPES.to_vec();
    if needed != "any" && !scopes.contains(&needed) {
        scopes.push(needed);
    }
    format!("tmgr login --scopes {}", scopes.join(","))
}

fn server_message(body: &Value) -> Option<String> {
    let raw = body
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| body.get("error").and_then(Value::as_str))
        .or_else(|| {
            body.get("error")
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
        })?;
    Some(truncate_chars(&sanitize_line(raw), 200).0)
}

pub fn map_error(status: u16, body: &Value, scope: &str) -> Error {
    let msg = server_message(body);
    match status {
        401 => Error::auth("not logged in or the session expired").hint("tmgr login"),
        403 => {
            let insufficient = body.get("error").and_then(Value::as_str)
                == Some("insufficient_scope")
                || msg
                    .as_deref()
                    .is_some_and(|m| m.contains("insufficient_scope"));
            if insufficient {
                Error::new(
                    Kind::Forbidden,
                    format!("this login does not allow that (needs scope {scope})"),
                )
                .hint(scopes_hint(scope))
            } else {
                Error::new(Kind::Forbidden, msg.unwrap_or_else(|| "forbidden".into()))
            }
        }
        404 => Error::not_found(msg.unwrap_or_else(|| "not found".into())),
        409 => Error::new(Kind::Conflict, msg.unwrap_or_else(|| "conflict".into())),
        400 | 422 => Error::usage(msg.unwrap_or_else(|| "the server rejected the request".into())),
        429 => Error::internal("rate limited by the server").hint("retry in a minute"),
        502..=504 => Error::new(Kind::Offline, format!("server unavailable (HTTP {status})")),
        _ => Error::internal(format!("server error (HTTP {status})")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_statuses_map_to_exit_codes() {
        let none = Value::Null;
        let cases = [
            (401, 4),
            (403, 5),
            (404, 3),
            (409, 6),
            (400, 2),
            (422, 2),
            (429, 1),
            (500, 1),
            (503, 7),
        ];
        for (status, exit) in cases {
            assert_eq!(
                map_error(status, &none, "tasks:read").exit_code(),
                exit,
                "{status}"
            );
        }
    }

    #[test]
    fn insufficient_scope_names_the_scope_and_hints_a_relogin() {
        let e = map_error(403, &json!({"error": "insufficient_scope"}), "tasks:create");
        assert_eq!(e.exit_code(), 5);
        assert!(e.message.contains("tasks:create"));
        assert_eq!(
            e.hint.as_deref(),
            Some("tmgr login --scopes workspaces:read,tasks:read,tasks:write,tasks:create,comments:create,time:read,time:write")
        );
        let e = map_error(403, &json!({"error": "insufficient_scope"}), "pages:read");
        assert!(e.hint.unwrap().ends_with(",pages:read"));
    }

    #[test]
    fn server_messages_are_sanitized_and_bounded() {
        let long = "x".repeat(500);
        let e = map_error(404, &json!({"message": format!("a\u{1b}[0m{long}")}), "any");
        assert!(!e.message.contains('\u{1b}'));
        assert!(e.message.chars().count() <= 200);
    }

    #[test]
    fn data_envelopes_and_bare_arrays_both_unwrap() {
        let a: Vec<Workspace> = items(json!({"data": [{"id": 1, "code": "p"}]})).unwrap();
        let b: Vec<Workspace> = items(json!([{"id": 1, "code": "p"}])).unwrap();
        assert_eq!(a[0].id, b[0].id);
        let task = unwrap_data(json!({"id": 1, "title": "t", "data": 5}));
        assert_eq!(task["id"], 1);
    }

    fn flaky_server(drop_first: usize) -> (String, std::thread::JoinHandle<usize>) {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut seen = 0;
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                seen += 1;
                if seen <= drop_first {
                    drop(stream);
                    continue;
                }
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}",
                );
                break;
            }
            seen
        });
        (url, handle)
    }

    fn client_for(url: &str) -> Client {
        let tokens = Tokens {
            access: Secret::new("t"),
            refresh: None,
            expires_at: None,
        };
        Client::new(url, "test", Some(tokens), None, false).unwrap()
    }

    #[test]
    fn a_get_is_retried_after_dropped_connections_when_asked() {
        let (url, server) = flaky_server(2);
        let client = client_for(&url);
        client.set_get_retries(3, Duration::from_millis(1));
        let (status, _) = client.call("GET", "/x", None, false).unwrap();
        assert_eq!(status, 200);
        assert_eq!(server.join().unwrap(), 3);
    }

    #[test]
    fn a_get_is_not_retried_by_default() {
        let (url, server) = flaky_server(1);
        let client = client_for(&url);
        let e = client.call("GET", "/x", None, false).unwrap_err();
        assert_eq!(e.kind, Kind::Offline);
        let _ = std::net::TcpStream::connect(url.trim_start_matches("http://"));
        server.join().unwrap();
    }

    #[test]
    fn retries_give_up_after_the_limit() {
        let (url, server) = flaky_server(10);
        let client = client_for(&url);
        client.set_get_retries(2, Duration::from_millis(1));
        let e = client.call("GET", "/x", None, false).unwrap_err();
        assert_eq!(e.kind, Kind::Offline);
        let _ = std::net::TcpStream::connect(url.trim_start_matches("http://"));
        for _ in 0..8 {
            let _ = std::net::TcpStream::connect(url.trim_start_matches("http://"));
        }
        let _ = server.join();
    }
}
