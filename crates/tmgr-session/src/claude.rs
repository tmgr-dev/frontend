use crate::lines::for_each_line;
use crate::model::{todos_of, Agent, Builder, Session, ToolCall};
use serde_json::Value;
use std::io::BufRead;

const NOISE: [&str; 4] = [
    "<system-reminder>",
    "<local-command-",
    "Caveat:",
    "<command-message>",
];

pub fn parse(reader: impl BufRead, fallback_id: &str) -> Session {
    let mut b = Builder::new(Agent::Claude, fallback_id.to_string());
    let mut id_set = false;
    let skipped = for_each_line(reader, |line| {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return false;
        };
        if v["isSidechain"].as_bool() == Some(true) {
            return true;
        }
        if !id_set {
            if let Some(id) = v["sessionId"].as_str() {
                b.session.id = id.to_string();
                id_set = true;
            }
        }
        if b.session.cwd.is_none() {
            b.session.cwd = v["cwd"].as_str().map(str::to_string);
        }
        if let Some(branch) = v["gitBranch"].as_str().filter(|s| !s.is_empty()) {
            b.session.branch = Some(branch.to_string());
        }
        match v["type"].as_str() {
            Some("user") => user(&mut b, &v),
            Some("assistant") => assistant(&mut b, &v),
            _ => {}
        }
        true
    });
    b.session.skipped = skipped;
    b.session
}

fn user(b: &mut Builder, v: &Value) {
    if v["isMeta"].as_bool() == Some(true) {
        return;
    }
    let content = &v["message"]["content"];
    if let Some(s) = content.as_str() {
        push_user(b, s);
        return;
    }
    let Some(blocks) = content.as_array() else {
        return;
    };
    let mut text = String::new();
    for block in blocks {
        match block["type"].as_str() {
            Some("tool_result") => {
                let id = block["tool_use_id"].as_str().unwrap_or_default();
                let failed = block["is_error"].as_bool() == Some(true);
                b.result(id, flatten(&block["content"]), failed);
            }
            Some("text") => {
                if let Some(t) = block["text"].as_str() {
                    text.push_str(t);
                }
            }
            _ => {}
        }
    }
    push_user(b, &text);
}

fn push_user(b: &mut Builder, text: &str) {
    let t = text.trim();
    if t.is_empty() || NOISE.iter().any(|n| t.starts_with(n)) {
        return;
    }
    b.user(t.to_string());
}

fn assistant(b: &mut Builder, v: &Value) {
    let Some(blocks) = v["message"]["content"].as_array() else {
        return;
    };
    for block in blocks {
        match block["type"].as_str() {
            Some("text") => {
                if let Some(t) = block["text"].as_str() {
                    b.text(t.to_string());
                }
            }
            Some("tool_use") => {
                let name = block["name"].as_str().unwrap_or("tool").to_string();
                let id = block["id"].as_str().unwrap_or_default().to_string();
                let input = &block["input"];
                if name == "TodoWrite" {
                    b.session.plan = todo_plan(input);
                    if let Some(t) = todos_of(&input["todos"], "content", "status") {
                        b.session.todos = t;
                    }
                }
                b.tool(id, tool_call(name, input));
            }
            _ => {}
        }
    }
}

fn tool_call(name: String, input: &Value) -> ToolCall {
    let path = ["file_path", "notebook_path", "path"]
        .iter()
        .find_map(|k| input[*k].as_str())
        .map(str::to_string);
    let command = input["command"].as_str().map(str::to_string);
    let pattern = input["pattern"].as_str().map(str::to_string);
    let searching = matches!(name.as_str(), "Grep" | "Glob");
    let mut call = ToolCall {
        summary: command
            .clone()
            .or_else(|| pattern.clone().filter(|_| searching))
            .or_else(|| path.clone())
            .or(pattern)
            .or_else(|| {
                ["query", "url", "description", "prompt"]
                    .iter()
                    .find_map(|k| input[*k].as_str().map(str::to_string))
            })
            .unwrap_or_default(),
        command,
        name,
        ..ToolCall::default()
    };
    if let Some(p) = path {
        match call.name.as_str() {
            "Read" => call.read.push(p),
            "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => call.edited.push(p),
            _ => {}
        }
    }
    call
}

fn todo_plan(input: &Value) -> Option<String> {
    let items = input["todos"].as_array()?;
    let lines: Vec<String> = items
        .iter()
        .map(|t| {
            format!(
                "[{}] {}",
                t["status"].as_str().unwrap_or("pending"),
                t["content"].as_str().unwrap_or_default()
            )
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn flatten(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}
