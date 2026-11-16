use crate::lines::for_each_line;
use crate::model::{Agent, Builder, Session, ToolCall};
use regex::Regex;
use serde_json::Value;
use std::io::BufRead;
use std::sync::OnceLock;

const NOISE: [&str; 6] = [
    "<environment_context>",
    "# AGENTS.md",
    "<user_instructions>",
    "<INSTRUCTIONS>",
    "<permissions",
    "<turn_aborted>",
];

fn patch_file() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?m)^\*\*\* (?:Update|Add|Delete) File: (.+)$").expect("patch"))
}

fn exit_code() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)exit code:?\s*(-?\d+)").expect("exit"))
}

pub fn parse(reader: impl BufRead, fallback_id: &str) -> Session {
    let mut b = Builder::new(Agent::Codex, fallback_id.to_string());
    let skipped = for_each_line(reader, |line| {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return false;
        };
        let p = &v["payload"];
        match v["type"].as_str() {
            Some("session_meta") => {
                if let Some(id) = p["id"].as_str() {
                    b.session.id = id.to_string();
                }
                b.session.cwd = p["cwd"].as_str().map(str::to_string);
                b.session.branch = p["git"]["branch"].as_str().map(str::to_string);
            }
            Some("response_item") => item(&mut b, p),
            _ => {}
        }
        true
    });
    b.session.skipped = skipped;
    b.session
}

fn item(b: &mut Builder, p: &Value) {
    match p["type"].as_str() {
        Some("message") => message(b, p),
        Some("function_call") => {
            let name = p["name"].as_str().unwrap_or("tool").to_string();
            let args: Value = p["arguments"]
                .as_str()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(Value::Null);
            if name == "update_plan" {
                b.session.plan = plan(&args);
            }
            let command = command_of(&args);
            b.tool(call_id(p), shell_call(name, command));
        }
        Some("local_shell_call") => {
            let command = command_of(&p["action"]);
            b.tool(call_id(p), shell_call("shell".into(), command));
        }
        Some("custom_tool_call") => {
            let name = p["name"].as_str().unwrap_or("tool").to_string();
            let input = p["input"].as_str().unwrap_or_default();
            let edited = patch_file()
                .captures_iter(input)
                .map(|c| c[1].trim().to_string())
                .collect::<Vec<_>>();
            let summary = if edited.is_empty() {
                input.lines().next().unwrap_or_default().to_string()
            } else {
                edited.join(", ")
            };
            b.tool(
                call_id(p),
                ToolCall {
                    name,
                    summary,
                    edited,
                    ..ToolCall::default()
                },
            );
        }
        Some("function_call_output" | "custom_tool_call_output") => {
            let (out, failed) = output(&p["output"]);
            b.result(&call_id(p), out, failed);
        }
        _ => {}
    }
}

fn shell_call(name: String, command: Option<String>) -> ToolCall {
    let edited = command
        .as_deref()
        .map(|c| {
            patch_file()
                .captures_iter(c)
                .map(|m| m[1].trim().to_string())
                .collect()
        })
        .unwrap_or_default();
    ToolCall {
        summary: command.clone().unwrap_or_else(|| name.clone()),
        command,
        name,
        edited,
        ..ToolCall::default()
    }
}

fn call_id(p: &Value) -> String {
    p["call_id"].as_str().unwrap_or_default().to_string()
}

fn message(b: &mut Builder, p: &Value) {
    let text: String = p["content"]
        .as_array()
        .map(|a| a.iter().filter_map(|c| c["text"].as_str()).collect())
        .unwrap_or_default();
    match p["role"].as_str() {
        Some("user") => {
            let t = text.trim();
            if !t.is_empty() && !NOISE.iter().any(|n| t.starts_with(n)) {
                b.user(t.to_string());
            }
        }
        Some("assistant") => b.text(text),
        _ => {}
    }
}

fn command_of(args: &Value) -> Option<String> {
    let c = args.get("command").or_else(|| args.get("cmd"))?;
    match c {
        Value::String(s) => Some(s.clone()),
        Value::Array(a) => {
            let parts: Vec<&str> = a.iter().filter_map(Value::as_str).collect();
            match parts.as_slice() {
                [_, "-lc" | "-c", script] => Some((*script).to_string()),
                _ => Some(parts.join(" ")),
            }
        }
        _ => None,
    }
}

fn output(v: &Value) -> (String, bool) {
    let raw = match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    if let Ok(obj) = serde_json::from_str::<Value>(&raw) {
        if let Some(out) = obj["output"].as_str() {
            let code = obj["metadata"]["exit_code"].as_i64().unwrap_or(0);
            return (out.to_string(), code != 0);
        }
    }
    let failed = exit_code()
        .captures(&raw)
        .and_then(|c| c[1].parse::<i64>().ok())
        .is_some_and(|c| c != 0);
    (raw, failed)
}

fn plan(args: &Value) -> Option<String> {
    let items = args["plan"].as_array()?;
    let lines: Vec<String> = items
        .iter()
        .map(|s| {
            format!(
                "[{}] {}",
                s["status"].as_str().unwrap_or("pending"),
                s["step"].as_str().unwrap_or_default()
            )
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}
