use crate::cmd::decisions::{
    self, agent_name, check_checked, default_wait, event_text, fact_checked, parent_pid,
    parse_check, parse_fact, pending_json, send_card, source, wait_loop, Sent, Spec, Waited, KINDS,
    MAX_CONTEXT, MAX_TITLE,
};
use crate::cmd::mcp::{arg_err, bool_arg, int_arg, str_arg, text_result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tmgr_ipc::decisions::{Client, LineComment};

static CLIENT: Mutex<String> = Mutex::new(String::new());
static CURSORS: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);

const MAX_REPLY: usize = 16 * 1024;

pub fn note_client(name: Option<&str>) {
    if let (Some(n), Ok(mut c)) = (name, CLIENT.lock()) {
        *c = n.chars().take(100).collect();
    }
}

fn client_agent() -> &'static str {
    agent_name(
        CLIENT
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default()
            .as_str(),
    )
}

fn cursor(card_id: &str) -> u64 {
    CURSORS
        .lock()
        .ok()
        .and_then(|m| m.as_ref().and_then(|m| m.get(card_id).copied()))
        .unwrap_or(0)
}

fn advance(card_id: &str, seq: u64) {
    if let Ok(mut m) = CURSORS.lock() {
        let e = m
            .get_or_insert_with(HashMap::new)
            .entry(card_id.to_string())
            .or_insert(0);
        *e = (*e).max(seq);
    }
}

pub fn is_decisions_tool(name: &str) -> bool {
    matches!(name, "ask_owner" | "get_answer" | "reply_owner")
}

pub fn defs() -> Vec<Value> {
    let wait = json!({"type": "integer", "minimum": 0, "maximum": 3600, "description": "Seconds to wait for the owner (default 600; 0 only checks). If the MCP client times out tool calls early (Codex: tool_timeout_sec, default 60), use a smaller value and call get_answer again."});
    vec![
        json!({
            "name": "ask_owner",
            "description": "Ask the owner a question and wait for the answer in the TMGR desktop Decisions inbox. Call it for decisions, reviews and release gates: anything where you would otherwise guess or stop and wait for the human. Give a short title, the question, and the options when the answer is a choice (set recommended to the option you advise). Add facts, checks, repo/base/head (the owner sees the diff and can comment on lines), task and pr when useful. Returns JSON: an event with type \"answer\" (choice, choice_label, note, free_text, line_comments; deferred=true means the owner postponed the decision: continue other work and call get_answer later; dismissed=true means they declined to answer) or type \"message\" (an interim owner comment, chat message or fix request: address it, then call reply_owner with your reply). If nothing arrives in time it returns {\"status\":\"pending\",\"card_id\":...}: continue other work and call get_answer with that card_id later. The card is queued even when the desktop is not running.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "minLength": 1, "maxLength": MAX_TITLE, "description": "One-line headline of the card"},
                    "question": {"type": "string", "description": "What you need decided"},
                    "context": {"type": "string", "maxLength": MAX_CONTEXT, "description": "Background in Markdown"},
                    "kind": {"type": "string", "enum": KINDS, "description": "Default decision"},
                    "blocking": {"type": "boolean", "description": "Whether you cannot continue without the answer (default true)"},
                    "options": {
                        "type": "array",
                        "maxItems": 9,
                        "items": {"anyOf": [
                            {"type": "string"},
                            {"type": "object", "properties": {"label": {"type": "string"}, "recommended": {"type": "boolean"}}, "required": ["label"], "additionalProperties": false}
                        ]},
                        "description": "Choices, as labels or {label, recommended}"
                    },
                    "recommended": {"type": "integer", "minimum": 1, "maximum": 9, "description": "1-based number of the recommended option"},
                    "facts": {"type": "array", "maxItems": 50, "items": {"anyOf": [
                        {"type": "string", "description": "STATE:TEXT with STATE ok, bad or wait"},
                        {"type": "object", "properties": {"state": {"type": "string", "enum": ["ok", "bad", "wait"]}, "text": {"type": "string"}, "link": {"type": "string"}}, "required": ["state", "text"], "additionalProperties": false}
                    ]}},
                    "checks": {"type": "array", "maxItems": 50, "items": {"anyOf": [
                        {"type": "string", "description": "STATE:TEXT with STATE ok, bad or todo"},
                        {"type": "object", "properties": {"state": {"type": "string", "enum": ["ok", "bad", "todo"]}, "text": {"type": "string"}}, "required": ["state", "text"], "additionalProperties": false}
                    ]}},
                    "repo": {"type": "string", "description": "Local git repository to show the diff of"},
                    "base": {"type": "string", "description": "Base ref of the diff (with repo)"},
                    "head": {"type": "string", "description": "Head ref of the diff (default: the working tree)"},
                    "task": {"type": "string", "description": "Task reference or URL"},
                    "pr": {"type": "string", "description": "Pull request URL"},
                    "waitSeconds": wait
                },
                "required": ["title"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "get_answer",
            "description": "Collect the owner's answer to a card created by ask_owner that came back pending (or was deferred). Waits up to waitSeconds for the next event after the ones already returned in this session: an \"answer\" (final unless deferred=true) or a \"message\" (an interim owner comment: address it, then call reply_owner). Returns {\"status\":\"pending\",\"card_id\":...} when nothing new arrived.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "card_id": {"type": "string", "description": "card_id from ask_owner"},
                    "waitSeconds": wait
                },
                "required": ["card_id"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "reply_owner",
            "description": "Answer an interim \"message\" event from the owner (a thread comment, chat message or fix request) on a card, then wait for the next event like get_answer. Use text for a general reply and line_replies to answer comments on specific lines of the diff.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "card_id": {"type": "string", "description": "card_id of the card"},
                    "text": {"type": "string", "maxLength": MAX_REPLY, "description": "Your reply"},
                    "line_replies": {"type": "array", "maxItems": 50, "items": {
                        "type": "object",
                        "properties": {"file": {"type": "string"}, "line": {"type": "integer", "minimum": 1}, "text": {"type": "string"}},
                        "required": ["file", "line", "text"],
                        "additionalProperties": false
                    }},
                    "waitSeconds": wait
                },
                "required": ["card_id"],
                "additionalProperties": false
            }
        }),
    ]
}

fn wait_arg(a: &Value) -> Result<Duration, String> {
    Ok(int_arg(a, "waitSeconds", 0, 3600)?
        .map(|n| Duration::from_secs(n as u64))
        .unwrap_or_else(default_wait))
}

fn items<'a>(a: &'a Value, key: &str) -> Result<&'a [Value], String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(v)) => Ok(v),
        Some(_) => Err(format!("{key} must be an array")),
    }
}

fn parse_spec(a: &Value) -> Result<Spec, String> {
    let title = str_arg(a, "title", 1, MAX_TITLE)?.ok_or("title is required")?;
    let mut spec = Spec {
        title: title.to_string(),
        question: str_arg(a, "question", 0, MAX_CONTEXT)?.map(str::to_string),
        context: str_arg(a, "context", 0, MAX_CONTEXT)?.map(str::to_string),
        kind: str_arg(a, "kind", 0, 20)?.unwrap_or("decision").to_string(),
        blocking: bool_arg(a, "blocking")?.unwrap_or(true),
        recommended: int_arg(a, "recommended", 1, 9)?.map(|n| n as usize),
        repo: str_arg(a, "repo", 1, 4096)?.map(str::to_string),
        base: str_arg(a, "base", 1, 300)?.map(str::to_string),
        head: str_arg(a, "head", 1, 300)?.map(str::to_string),
        task: str_arg(a, "task", 1, 2000)?.map(str::to_string),
        pr: str_arg(a, "pr", 1, 2000)?.map(str::to_string),
        ..Spec::default()
    };
    for (i, o) in items(a, "options")?.iter().enumerate() {
        match o {
            Value::String(s) => spec.options.push(s.clone()),
            Value::Object(_) => {
                let label = str_arg(o, "label", 1, MAX_TITLE)?.ok_or("option label is required")?;
                spec.options.push(label.to_string());
                if bool_arg(o, "recommended")? == Some(true) && spec.recommended.is_none() {
                    spec.recommended = Some(i + 1);
                }
            }
            _ => return Err("options must be strings or {label, recommended}".into()),
        }
    }
    for f in items(a, "facts")? {
        spec.facts.push(match f {
            Value::String(s) => parse_fact(s)?,
            Value::Object(_) => fact_checked(
                serde_json::from_value(f.clone()).map_err(|_| "facts need state and text")?,
            )?,
            _ => return Err("facts must be strings or objects".into()),
        });
    }
    for c in items(a, "checks")? {
        spec.checks.push(match c {
            Value::String(s) => parse_check(s)?,
            Value::Object(_) => check_checked(
                serde_json::from_value(c.clone()).map_err(|_| "checks need state and text")?,
            )?,
            _ => return Err("checks must be strings or objects".into()),
        });
    }
    Ok(spec)
}

fn card_id_arg(a: &Value) -> Result<String, String> {
    let id = str_arg(a, "card_id", 1, 64)?.ok_or("card_id is required")?;
    if !tmgr_ipc::decisions::valid_card_id(id) {
        return Err("card_id is not a card id".into());
    }
    Ok(id.to_string())
}

fn event_result(card_id: &str, waited: Waited) -> Value {
    match waited {
        Waited::Event(e) => {
            advance(card_id, e.seq);
            text_result(
                serde_json::to_string(&*e).unwrap_or_else(|_| event_text(&e)),
                false,
            )
        }
        Waited::Pending { unreachable } => {
            text_result(pending_json(card_id, unreachable).to_string(), false)
        }
        Waited::Refused(m) => text_result(format!("error: {m}"), true),
    }
}

fn collect(card_id: &str, total: Duration, progress: &mut dyn FnMut(&str)) -> Value {
    let waited = wait_loop(card_id, cursor(card_id), total, false, &mut || {
        progress("waiting for the owner")
    });
    event_result(card_id, waited)
}

fn ask_owner(a: &Value, progress: &mut dyn FnMut(&str)) -> Value {
    let parsed = parse_spec(a).and_then(|s| {
        let wait = wait_arg(a)?;
        let card = s.into_card("mcp", source(client_agent(), Some(parent_pid())))?;
        Ok((card, wait))
    });
    let (card, wait) = match parsed {
        Ok(p) => p,
        Err(m) => return arg_err(&m),
    };
    match send_card(card) {
        Ok(Sent::Live(id)) => collect(&id, wait, progress),
        Ok(Sent::Spooled(id)) => text_result(pending_json(&id, true).to_string(), false),
        Err(e) => text_result(format!("error: {e}"), true),
    }
}

fn get_answer(a: &Value, progress: &mut dyn FnMut(&str)) -> Value {
    let parsed = card_id_arg(a).and_then(|id| Ok((id, wait_arg(a)?)));
    match parsed {
        Ok((id, wait)) => collect(&id, wait, progress),
        Err(m) => arg_err(&m),
    }
}

fn reply_owner(a: &Value, progress: &mut dyn FnMut(&str)) -> Value {
    let parsed = (|| {
        let id = card_id_arg(a)?;
        let text = str_arg(a, "text", 1, MAX_REPLY)?.map(str::to_string);
        let mut lines = Vec::new();
        for r in items(a, "line_replies")? {
            let file = str_arg(r, "file", 1, 4096)?.ok_or("line_replies need file")?;
            let line = int_arg(r, "line", 1, u32::MAX as i64)?.ok_or("line_replies need line")?;
            let text = str_arg(r, "text", 1, MAX_REPLY)?.ok_or("line_replies need text")?;
            lines.push(LineComment {
                file: file.to_string(),
                line: line as u32,
                text: text.to_string(),
            });
        }
        if text.is_none() && lines.is_empty() {
            return Err("text or line_replies is required".to_string());
        }
        Ok((id, text, lines, wait_arg(a)?))
    })();
    let (id, text, lines, wait) = match parsed {
        Ok(p) => p,
        Err(m) => return arg_err(&m),
    };
    let sent = tmgr_ipc::decisions::decisions_dir()
        .and_then(|d| Client::open_in(&d))
        .and_then(|c| c.reply(&id, text, lines));
    match sent {
        Ok(()) => collect(&id, wait, progress),
        Err(e) if decisions::is_refusal(&e) => text_result(format!("error: {e}"), true),
        Err(_) => text_result(
            format!(
                "error: {}; the reply was not delivered",
                decisions::PENDING_NOTE
            ),
            true,
        ),
    }
}

pub fn call(name: &str, args: &Value, progress: &mut dyn FnMut(&str)) -> Value {
    match name {
        "ask_owner" => ask_owner(args, progress),
        "get_answer" => get_answer(args, progress),
        _ => reply_owner(args, progress),
    }
}
