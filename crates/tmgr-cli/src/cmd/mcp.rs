use crate::cli::VERSION;
use crate::cmd::notifyenv::{Failure, NotifyEnv};
use serde_json::{json, Map, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tmgr_core::notify::{
    build_push, format_alarm, format_failure, valid_alarm_id, AlarmRequest, Fail, WaitOptions,
};
use tmgr_core::Url;

const SUPPORTED: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const MAX_LINE: u64 = 1024 * 1024;
const DRAIN_AFTER_EOF: Duration = Duration::from_secs(15);
const PUSH_TIMEOUT: Duration = Duration::from_secs(5);

pub const CLI_HELP: &str = "tmgr: command line for TMGR tasks, time and agent work.
Tasks:   tmgr task show TM-512 | tmgr task list -p TM | tmgr add \"title\" -p TM
Timer:   tmgr start TM-512 | tmgr stop | tmgr done
Agent:   tmgr agent start TM-512 --agent NAME | tmgr agent update --commit \"sha subject\" | tmgr agent finish --status succeeded --summary TEXT
Phone:   tmgr notify \"text\" [--title T] [--priority high] | tmgr alarm \"text\" | tmgr alarm status [ID]
Add --json for machine-readable output (schema tmgr.v1). `tmgr --help` lists everything.";

fn tool_defs() -> Value {
    json!([
        {
            "name": "notify_user",
            "description": "Send a phone push notification to the user via TMGR agent notifications.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "minLength": 1, "maxLength": 120, "description": "Notification title"},
                    "body": {"type": "string", "maxLength": 1000, "description": "Notification body"},
                    "priority": {"type": "string", "enum": ["low", "normal", "high"], "description": "Priority, default normal"},
                    "link": {"type": "string", "format": "uri", "description": "Absolute http(s) URL to open on tap"}
                },
                "required": ["title"],
                "additionalProperties": false
            }
        },
        {
            "name": "alarm",
            "description": "Only for incidents that need the human NOW (production down, data loss, security). Rings the owner's phone: silent push to the app alarm, then a voice call if not acknowledged. Prefer notify_user for everything else. Calls repeat up to callAttempts times (default 3) until acknowledged. You decide when/whether to retry; one call = one escalation.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "minLength": 1, "maxLength": 120, "description": "Alarm title, read aloud on the call"},
                    "message": {"type": "string", "minLength": 1, "maxLength": 500, "description": "What happened and what is needed, read aloud on the call"},
                    "ackTimeoutSeconds": {"type": "integer", "exclusiveMinimum": 0, "description": "Seconds to wait for an app acknowledgement before calling"},
                    "deliveryTimeoutSeconds": {"type": "integer", "exclusiveMinimum": 0, "description": "Seconds to wait for the app to receive the alarm before calling"},
                    "call": {"type": "boolean", "description": "Voice-call fallback (default true). With false the alarm ends expired if not acknowledged in the app"},
                    "callAttempts": {"type": "integer", "minimum": 1, "maximum": 5, "description": "Voice-call attempts until acknowledged (1-5, default 3)"},
                    "waitForResult": {"type": "boolean", "description": "Wait for a final status (default true); false returns {id, status} immediately"},
                    "maxWaitSeconds": {"type": "number", "exclusiveMinimum": 0, "maximum": 3600, "description": "Overall wait cap in seconds when waiting (default 600). On timeout the last status is returned with timedOut=true; re-check with alarm_status"}
                },
                "required": ["title", "message"],
                "additionalProperties": false
            }
        },
        {
            "name": "alarm_status",
            "description": "Check an alarm created by the alarm tool. With waitSeconds (max 50) it long-polls until the status changes. Use it to re-check after the alarm call timed out or was started with waitForResult=false.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {"type": "string", "minLength": 1, "description": "Alarm id returned by the alarm tool"},
                    "waitSeconds": {"type": "integer", "minimum": 0, "maximum": 50, "description": "Long-poll seconds, default 0"}
                },
                "required": ["id"],
                "additionalProperties": false
            }
        },
        {
            "name": "tmgr_cli",
            "description": "Short help for the tmgr command line tool: tasks, timer, agent work runs and notifications. Prefer the CLI over many tool calls.",
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        }
    ])
}

fn text_result(text: impl Into<String>, is_error: bool) -> Value {
    let mut m = Map::new();
    m.insert(
        "content".into(),
        json!([{"type": "text", "text": text.into()}]),
    );
    if is_error {
        m.insert("isError".into(), json!(true));
    }
    Value::Object(m)
}

fn arg_err(msg: &str) -> Value {
    text_result(format!("Invalid arguments: {msg}"), true)
}

fn str_arg<'a>(a: &'a Value, key: &str, min: usize, max: usize) -> Result<Option<&'a str>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            let n = s.chars().count();
            if n < min || n > max {
                Err(format!("{key} must be {min} to {max} characters"))
            } else {
                Ok(Some(s))
            }
        }
        Some(_) => Err(format!("{key} must be a string")),
    }
}

fn int_arg(a: &Value, key: &str, min: i64, max: i64) -> Result<Option<i64>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match v.as_i64() {
            Some(n) if (min..=max).contains(&n) => Ok(Some(n)),
            _ => Err(format!("{key} must be an integer from {min} to {max}")),
        },
    }
}

fn bool_arg(a: &Value, key: &str) -> Result<Option<bool>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(format!("{key} must be a boolean")),
    }
}

fn failure_text(f: Failure, alarm_id: Option<&str>) -> Value {
    match f {
        Failure::Net(fail, _) => text_result(format_failure(&fail, alarm_id), true),
        Failure::Other(e) => text_result(format!("error: {}", e.message), true),
    }
}

fn notify_user(env: &NotifyEnv, a: &Value) -> Value {
    let parsed = (|| {
        let title = str_arg(a, "title", 1, 120)?.ok_or("title is required")?;
        let body = str_arg(a, "body", 0, 1000)?;
        let priority = str_arg(a, "priority", 0, 10)?;
        if priority.is_some_and(|p| !matches!(p, "low" | "normal" | "high")) {
            return Err("priority must be low, normal or high".to_string());
        }
        let link = str_arg(a, "link", 1, 2000)?;
        if let Some(l) = link {
            let ok = Url::parse(l).is_ok_and(|u| matches!(u.scheme(), "http" | "https"));
            if !ok {
                return Err("link must be an absolute http(s) URL".to_string());
            }
        }
        Ok(build_push(title, body, priority, link, None))
    })();
    let payload = match parsed {
        Ok(p) => p,
        Err(m) => return arg_err(&m),
    };
    let mut warnings = Vec::new();
    match env.with_http_raw(&mut warnings, |h| h.push(&payload, PUSH_TIMEOUT)) {
        Ok((outcome, _)) => text_result(outcome.status, false),
        Err(f) => failure_text(f, None),
    }
}

fn alarm(env: &NotifyEnv, a: &Value, progress: &mut dyn FnMut(&str)) -> Value {
    let parsed = (|| {
        let title = str_arg(a, "title", 1, 120)?.ok_or("title is required")?;
        let message = str_arg(a, "message", 1, 500)?.ok_or("message is required")?;
        let req = AlarmRequest {
            title: title.to_string(),
            message: message.to_string(),
            ack_timeout_seconds: int_arg(a, "ackTimeoutSeconds", 1, i64::MAX)?.map(|n| n as u64),
            delivery_timeout_seconds: int_arg(a, "deliveryTimeoutSeconds", 1, i64::MAX)?
                .map(|n| n as u64),
            call: bool_arg(a, "call")?,
            call_attempts: int_arg(a, "callAttempts", 1, 5)?.map(|n| n as u8),
        };
        let wait = bool_arg(a, "waitForResult")?.unwrap_or(true);
        let max_wait = match a.get("maxWaitSeconds") {
            None | Some(Value::Null) => 600.0,
            Some(v) => match v.as_f64() {
                Some(n) if n > 0.0 && n <= 3600.0 => n,
                _ => {
                    return Err(
                        "maxWaitSeconds must be a number above 0 and at most 3600".to_string()
                    )
                }
            },
        };
        Ok((req, wait, max_wait))
    })();
    let (req, wait, max_wait) = match parsed {
        Ok(p) => p,
        Err(m) => return arg_err(&m),
    };
    let mut warnings = Vec::new();
    let (created, session) = match env.with_http_raw(&mut warnings, |h| h.create_alarm(&req)) {
        Ok(v) => v,
        Err(f) => return failure_text(f, None),
    };
    if !wait {
        return text_result(
            json!({"id": created.id, "status": created.status}).to_string(),
            false,
        );
    }
    let opts = WaitOptions {
        max_wait: Duration::from_secs_f64(max_wait),
        ..WaitOptions::default()
    };
    match session.http.wait_alarm(&created.id, &opts, &mut |al| {
        progress(&format!("alarm {}: {}", al.id, al.status));
    }) {
        Ok((alarm, timed_out)) => text_result(format_alarm(&alarm, timed_out), false),
        Err(f) => text_result(format_failure(&f, Some(&created.id)), true),
    }
}

fn alarm_status(env: &NotifyEnv, a: &Value) -> Value {
    let parsed = (|| {
        let id = str_arg(a, "id", 1, 40)?.ok_or("id is required")?;
        if !valid_alarm_id(id) {
            return Err("id must be the numeric alarm id".to_string());
        }
        let wait = int_arg(a, "waitSeconds", 0, 50)?.unwrap_or(0) as u64;
        Ok((id.to_string(), wait))
    })();
    let (id, wait) = match parsed {
        Ok(p) => p,
        Err(m) => return arg_err(&m),
    };
    let mut warnings = Vec::new();
    match env.with_http_raw(&mut warnings, |h| h.get_alarm(&id, wait)) {
        Ok((alarm, _)) => text_result(format_alarm(&alarm, false), false),
        Err(Failure::Net(Fail::NotFound, _)) => text_result("error: alarm not found", true),
        Err(f) => failure_text(f, None),
    }
}

pub fn call_tool(
    env: Option<&NotifyEnv>,
    name: &str,
    args: &Value,
    progress: &mut dyn FnMut(&str),
) -> Value {
    if name == "tmgr_cli" {
        return text_result(CLI_HELP, false);
    }
    if !matches!(name, "notify_user" | "alarm" | "alarm_status") {
        return text_result(
            format!(
                "Unknown tool: {}",
                name.chars().take(60).collect::<String>()
            ),
            true,
        );
    }
    let Some(env) = env else {
        return text_result("error: cannot read the tmgr configuration", true);
    };
    let empty = json!({});
    let args = if args.is_object() { args } else { &empty };
    match name {
        "notify_user" => notify_user(env, args),
        "alarm" => alarm(env, args, progress),
        _ => alarm_status(env, args),
    }
}

pub fn negotiate(requested: Option<&str>) -> &'static str {
    requested
        .and_then(|r| SUPPORTED.iter().find(|s| **s == r))
        .copied()
        .unwrap_or(SUPPORTED[0])
}

fn ok(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn err(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

pub fn handle_inline(msg: &Value) -> Option<Value> {
    let method = msg.get("method")?.as_str()?;
    let id = msg.get("id")?;
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => ok(
            id,
            json!({
                "protocolVersion": negotiate(params.get("protocolVersion").and_then(Value::as_str)),
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "tmgr", "version": VERSION},
                "instructions": "Phone notifications for the user (notify_user, alarm, alarm_status) and short tmgr CLI help (tmgr_cli)."
            }),
        ),
        "ping" => ok(id, json!({})),
        "tools/list" => ok(id, json!({"tools": tool_defs()})),
        _ => err(id, -32601, "Method not found"),
    })
}

type Out = Arc<Mutex<std::io::Stdout>>;

fn send(out: &Out, v: &Value) {
    if let (Ok(mut w), Ok(text)) = (out.lock(), serde_json::to_string(v)) {
        let _ = writeln!(w, "{text}");
        let _ = w.flush();
    }
}

fn read_line(reader: &mut impl BufRead, buf: &mut Vec<u8>) -> Option<bool> {
    buf.clear();
    let n = reader
        .by_ref()
        .take(MAX_LINE + 1)
        .read_until(b'\n', buf)
        .ok()?;
    if n == 0 {
        return None;
    }
    if buf.len() as u64 > MAX_LINE && buf.last() != Some(&b'\n') {
        loop {
            let chunk = reader.fill_buf().ok()?;
            if chunk.is_empty() {
                break;
            }
            match chunk.iter().position(|b| *b == b'\n') {
                Some(i) => {
                    reader.consume(i + 1);
                    break;
                }
                None => {
                    let len = chunk.len();
                    reader.consume(len);
                }
            }
        }
        return Some(false);
    }
    Some(true)
}

pub fn run(server_override: Option<String>, verbose: bool) -> i32 {
    let env = NotifyEnv::load(server_override, verbose).ok().map(Arc::new);
    let out: Out = Arc::new(Mutex::new(std::io::stdout()));
    let mut reader = BufReader::new(std::io::stdin());
    let mut buf = Vec::new();
    let mut workers: Vec<std::thread::JoinHandle<()>> = Vec::new();
    while let Some(complete) = read_line(&mut reader, &mut buf) {
        workers.retain(|w| !w.is_finished());
        if !complete {
            send(&out, &err(&Value::Null, -32600, "Request too large"));
            continue;
        }
        let text = String::from_utf8_lossy(&buf);
        if text.trim().is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(&text) else {
            send(&out, &err(&Value::Null, -32700, "Parse error"));
            continue;
        };
        if !msg.is_object() {
            send(&out, &err(&Value::Null, -32600, "Invalid Request"));
            continue;
        }
        if msg.get("method").and_then(Value::as_str) == Some("tools/call")
            && msg.get("id").is_some()
        {
            let out = out.clone();
            let env = env.clone();
            workers.push(std::thread::spawn(move || {
                tools_call(&out, env.as_deref(), &msg)
            }));
            continue;
        }
        if let Some(resp) = handle_inline(&msg) {
            send(&out, &resp);
        }
    }
    let deadline = std::time::Instant::now() + DRAIN_AFTER_EOF;
    while workers.iter().any(|w| !w.is_finished()) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    0
}

fn tools_call(out: &Out, env: Option<&NotifyEnv>, msg: &Value) {
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        send(out, &err(&id, -32602, "Invalid params: name is required"));
        return;
    };
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
    let token = params
        .get("_meta")
        .and_then(|m| m.get("progressToken"))
        .cloned();
    let mut step = 0u64;
    let mut progress = |message: &str| {
        let Some(t) = &token else { return };
        step += 1;
        send(
            out,
            &json!({"jsonrpc": "2.0", "method": "notifications/progress", "params": {"progressToken": t, "progress": step, "message": message}}),
        );
    };
    let result = call_tool(env, name, &args, &mut progress);
    send(out, &ok(&id, result));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_negotiated_down_to_a_supported_one() {
        assert_eq!(negotiate(Some("2025-03-26")), "2025-03-26");
        assert_eq!(negotiate(Some("2024-11-05")), "2024-11-05");
        assert_eq!(negotiate(Some("2031-01-01")), SUPPORTED[0]);
        assert_eq!(negotiate(None), SUPPORTED[0]);
    }

    #[test]
    fn the_tool_list_matches_the_typescript_package_plus_help() {
        let names: Vec<String> = tool_defs()
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names, ["notify_user", "alarm", "alarm_status", "tmgr_cli"]);
        for t in tool_defs().as_array().unwrap() {
            assert_eq!(t["inputSchema"]["type"], "object");
        }
    }

    #[test]
    fn notifications_get_no_reply_and_unknown_methods_get_an_error() {
        assert!(
            handle_inline(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
                .is_none()
        );
        let r = handle_inline(&json!({"jsonrpc": "2.0", "id": 5, "method": "nope"})).unwrap();
        assert_eq!(r["error"]["code"], -32601);
        let r = handle_inline(&json!({"jsonrpc": "2.0", "id": "a", "method": "ping"})).unwrap();
        assert_eq!(r["id"], "a");
    }

    #[test]
    fn help_needs_no_configuration_and_bad_arguments_are_tool_errors() {
        let r = call_tool(None, "tmgr_cli", &json!({}), &mut |_| {});
        assert!(r["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("tmgr agent start"));
        assert!(r.get("isError").is_none());
        let r = call_tool(None, "other", &json!({}), &mut |_| {});
        assert_eq!(r["isError"], true);
        let r = call_tool(None, "notify_user", &json!({}), &mut |_| {});
        assert_eq!(r["isError"], true);
    }
}
