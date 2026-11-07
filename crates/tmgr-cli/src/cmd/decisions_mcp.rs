use crate::cmd::decisions::{
    self, agent_name, check_checked, default_wait, event_text, fact_checked, parent_pid,
    parse_check, parse_fact, pending_json, send_card_full, source, wait_cards, SentFull, Spec,
    Waited, KINDS, MAX_CONTEXT, MAX_TITLE,
};
use crate::cmd::mcp::{arg_err, bool_arg, int_arg, str_arg, text_result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tmgr_ipc::decisions::{
    Cancel, Client, DiffSpec, Found, LineComment, PatchIn, WaitCard, MAX_PATCH_BYTES,
    MAX_WAIT_CARDS, OUTCOME_SIMILAR,
};
use tmgr_ipc::decisions::{MAX_DIFFS, MAX_PATCHES};

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

const FIND_UNAVAILABLE: &str = "error: the Decisions inbox could not be reached or is too old for find_answers (update the desktop app)";
const SIMILAR_HINT: &str = "An earlier card on this task looks like the same question. Reuse its answer (dismissed=true means the owner declined to answer, not a choice), or if it is still open call get_answer with its card_id; if your question really differs, call ask_owner again with force_new:true and a new key.";

pub fn is_decisions_tool(name: &str) -> bool {
    matches!(
        name,
        "ask_owner" | "get_answer" | "reply_owner" | "find_answers"
    )
}

pub fn defs() -> Vec<Value> {
    let wait = json!({"type": "integer", "minimum": 0, "maximum": 3600, "description": "Seconds to wait for the owner (default 600; 0 only checks). If the MCP client times out tool calls early (Codex: tool_timeout_sec, default 60), use a smaller value and call get_answer again."});
    vec![
        json!({
            "name": "ask_owner",
            "description": "Before asking, call find_answers for the same task: the owner often already answered. If ask_owner returns status similar_answered, use that answer instead of asking again. Ask the owner a question and wait for the answer in the TMGR desktop Decisions inbox. Call it for decisions, reviews and release gates: anything where you would otherwise guess or stop and wait for the human. Give a short title, the question, and the options when the answer is a choice (set recommended to the option you advise). Add facts, checks, repo/base/head or repos (the owner sees the diffs and can comment on lines), patches (diff text you hold, no repository needed), task, prs, commits, tags and links when useful. Line comments on a card with 2 or more diff groups (repos plus patches) arrive as file = \"<label>:<path>\" where the label is the repository folder name (a second one gets -2) or the patch label; with one group the file is the plain path. Keep the repos order when you resend so the labels stay stable. To resend a question instead of piling up cards, pass key or replaces. key names one question for good: the same key from the same agent and working directory updates the still-open card in place (same card_id, queue position and messages), and if the owner already answered or dismissed it nothing is reopened and the existing answer is returned; a new round of the question needs a new key. A deliberate new round of a question the owner already answered needs force_new:true together with a new key. The key is scoped to (agent, working directory, key), so a resend from another worktree needs replaces (the card_id returned the first time), which wins over key. Always use the card_id the call returns. Returns JSON: an event with type \"answer\" (choice, choice_label, note, free_text, line_comments; deferred=true means the owner postponed the decision: continue other work and call get_answer later; dismissed=true means they declined to answer) or type \"message\" (an interim owner comment, chat message or fix request: address it, then call reply_owner with your reply). If nothing arrives in time it returns {\"status\":\"pending\",\"card_id\":...}: continue other work and call get_answer with that card_id later. The card is queued even when the desktop is not running.",
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
                        {"type": "object", "properties": {"state": {"type": "string", "enum": ["ok", "bad", "wait"]}, "text": {"type": "string"}, "link": {"type": "string", "description": "http(s) URL"}}, "required": ["state", "text"], "additionalProperties": false}
                    ]}},
                    "checks": {"type": "array", "maxItems": 50, "items": {"anyOf": [
                        {"type": "string", "description": "STATE:TEXT with STATE ok, bad or todo"},
                        {"type": "object", "properties": {"state": {"type": "string", "enum": ["ok", "bad", "todo"]}, "text": {"type": "string"}}, "required": ["state", "text"], "additionalProperties": false}
                    ]}},
                    "repo": {"type": "string", "description": "Local git repository to show the diff of"},
                    "base": {"type": "string", "description": "Base ref of the diff (with repo)"},
                    "head": {"type": "string", "description": "Head ref of the diff (default: the working tree)"},
                    "task": {"type": "string", "description": "Task reference or http(s) URL"},
                    "pr": {"type": "string", "description": "Pull request http(s) URL or reference"},
                    "repos": {"type": "array", "maxItems": MAX_DIFFS, "items": {
                        "type": "object",
                        "properties": {
                            "repo": {"type": "string", "description": "Local git repository"},
                            "base": {"type": "string", "description": "Base ref"},
                            "head": {"type": "string", "description": "Head ref (default: the working tree)"}
                        },
                        "required": ["repo"],
                        "additionalProperties": false
                    }, "description": "More repositories to show; with repo, the repo field comes first. At most 8 repositories in all. Keep the order when you resend."},
                    "patches": {"type": "array", "maxItems": MAX_PATCHES, "items": {
                        "type": "object",
                        "properties": {
                            "label": {"type": "string", "description": "Name shown for the patch and used in line comment file names (default patch)"},
                            "text": {"type": "string", "description": "Unified diff text"}
                        },
                        "required": ["text"],
                        "additionalProperties": false
                    }, "description": "Diffs given as text. At most 8, 1 MiB in total; a bigger card is refused, never cut."},
                    "prs": {"type": "array", "items": {"type": "string"}, "description": "Pull requests: http(s) URL or owner/repo#123"},
                    "commits": {"type": "array", "items": {"type": "string"}, "description": "Commits: URL or owner/repo@<7-40 hex sha>"},
                    "tags": {"type": "array", "items": {"type": "string"}, "description": "Release tags: URL or owner/repo@tag"},
                    "links": {"type": "array", "items": {"type": "string"}, "description": "Other links: [LABEL=]URL. Only http and https; at most 50 links in all."},
                    "replaces": {"type": "string", "description": "card_id of an earlier card to update in place (wins over key). Needed to resend from another worktree. If that card is already answered or dismissed the existing answer is returned."},
                    "key": {"type": "string", "maxLength": 200, "description": "Names one question for good. The same key from the same agent and working directory updates the open card instead of creating another; a new round needs a new key, and with force_new:true. Scope is (agent, working directory, key)."},
                    "force_new": {"type": "boolean", "description": "Set true for a deliberate new round of a question that earlier cards on the same task already asked. Without it, ask_owner returns the similar earlier card (status similar_answered) instead of creating a new one. Pass it with a new key."},
                    "waitSeconds": wait
                },
                "required": ["title"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "get_answer",
            "description": "Collect the owner's answer to a card created by ask_owner that came back pending (or was deferred). Waits up to waitSeconds for the next event after the ones already returned in this session: an \"answer\" (final unless deferred=true) or a \"message\" (an interim owner comment: address it, then call reply_owner). Pass card_ids (up to 64) to cover several cards with one wait: it returns the first event from any of them, and the event JSON includes its card_id. Returns {\"status\":\"pending\",\"card_id\":...} when nothing new arrived (with card_ids: {\"status\":\"pending\",\"card_ids\":[...]}).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "card_id": {"type": "string", "description": "card_id from ask_owner"},
                    "card_ids": {"type": "array", "minItems": 1, "maxItems": MAX_WAIT_CARDS, "items": {"type": "string"}, "description": "Several card_ids to wait on at once; merged with card_id. Either card_id or card_ids is required."},
                    "waitSeconds": wait
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "find_answers",
            "description": "Look up the owner's past answers in the TMGR desktop Decisions inbox: answered and dismissed cards, newest first, filtered by task (TM-123 or a task URL), by key prefix or by text (query: every word must occur in the title, question, choice, free text or note). Each result gives the choice, free text, note and state (dismissed=true means the owner declined to answer). Call it before ask_owner: the owner often already answered the same question. Returns {\"answers\":[...]}.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task": {"type": "string", "minLength": 1, "maxLength": 2000, "description": "Task reference (TM-123) or http(s) URL"},
                    "key": {"type": "string", "minLength": 1, "maxLength": 200, "description": "Prefix of the card key"},
                    "query": {"type": "string", "minLength": 1, "maxLength": 2000, "description": "Words that must all occur in the card text"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50, "description": "How many cards to return (default 10)"}
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "reply_owner",
            "description": "Answer an interim \"message\" event from the owner (a thread comment, chat message or fix request) on a card, then wait for the next event like get_answer. Use line_replies to answer comments on specific lines of the diff. A text-only reply goes to the general chat, unless the owner has no unanswered chat message and exactly one line thread is waiting for you: then it is posted in that thread. If two or more line threads are waiting the call is refused with the list; answer them with line_replies.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "card_id": {"type": "string", "description": "card_id of the card"},
                    "text": {"type": "string", "maxLength": MAX_REPLY, "description": "Your reply. Goes to the general chat, or to the single line thread waiting for you when the chat has no unanswered owner message; refused when two or more line threads are waiting (use line_replies)"},
                    "line_replies": {"type": "array", "maxItems": 50, "items": {
                        "type": "object",
                        "properties": {"file": {"type": "string"}, "line": {"type": "integer", "minimum": 1}, "side": {"type": "string", "enum": ["old", "new"], "description": "old = the deleted line, new = the added or unchanged line; omit to join the thread already on that line"}, "text": {"type": "string"}},
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
        force_new: Some(bool_arg(a, "force_new")?.unwrap_or(false)),
        repo: str_arg(a, "repo", 1, 4096)?.map(str::to_string),
        base: str_arg(a, "base", 1, 300)?.map(str::to_string),
        head: str_arg(a, "head", 1, 300)?.map(str::to_string),
        task: str_arg(a, "task", 1, 2000)?.map(str::to_string),
        prs: str_arg(a, "pr", 1, 2000)?
            .map(str::to_string)
            .into_iter()
            .collect(),
        replaces: str_arg(a, "replaces", 1, 64)?.map(str::to_string),
        key: str_arg(a, "key", 1, 200)?.map(str::to_string),
        ..Spec::default()
    };
    for r in items(a, "repos")? {
        spec.diffs.push(DiffSpec {
            repo: str_arg(r, "repo", 1, 4096)?
                .ok_or("repos need repo")?
                .to_string(),
            base: str_arg(r, "base", 1, 300)?.map(str::to_string),
            head: str_arg(r, "head", 1, 300)?.map(str::to_string),
        });
    }
    for p in items(a, "patches")? {
        spec.patches.push(PatchIn {
            label: str_arg(p, "label", 0, MAX_TITLE)?
                .unwrap_or_default()
                .to_string(),
            text: str_arg(p, "text", 1, MAX_PATCH_BYTES)?
                .ok_or("patches need text")?
                .to_string(),
        });
    }
    for (key, list) in [
        ("prs", &mut spec.prs),
        ("commits", &mut spec.commits),
        ("tags", &mut spec.tags),
        ("links", &mut spec.links),
    ] {
        for v in items(a, key)? {
            let s = v.as_str().ok_or_else(|| format!("{key} must be strings"))?;
            list.push(s.to_string());
        }
    }
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

fn card_ids_arg(a: &Value) -> Result<Vec<String>, String> {
    let mut ids: Vec<String> = Vec::new();
    let mut add = |id: &str| {
        if !tmgr_ipc::decisions::valid_card_id(id) {
            return Err("card_id is not a card id".to_string());
        }
        if !ids.iter().any(|i| i == id) {
            ids.push(id.to_string());
        }
        Ok(())
    };
    if let Some(id) = str_arg(a, "card_id", 1, 64)? {
        add(id)?;
    }
    match a.get("card_ids") {
        None | Some(Value::Null) => {}
        Some(Value::Array(list)) => {
            for v in list {
                let id = v.as_str().ok_or("card_ids must be strings")?;
                add(id)?;
            }
        }
        Some(_) => return Err("card_ids must be an array".into()),
    }
    if ids.is_empty() {
        return Err("card_id or card_ids is required".into());
    }
    if ids.len() > MAX_WAIT_CARDS {
        return Err(format!("at most {MAX_WAIT_CARDS} cards can be waited on"));
    }
    Ok(ids)
}

fn event_result(card_ids: &[String], waited: Waited) -> Value {
    match waited {
        Waited::Event(key, e) => {
            advance(&key, e.seq);
            text_result(
                serde_json::to_string(&*e).unwrap_or_else(|_| event_text(&e)),
                false,
            )
        }
        Waited::Pending { unreachable } => {
            let v = match card_ids {
                [id] => pending_json(id, unreachable),
                _ => {
                    let mut v = json!({"status": "pending", "card_ids": card_ids});
                    if unreachable {
                        v["note"] = Value::String(decisions::PENDING_NOTE.to_string());
                    }
                    v
                }
            };
            text_result(v.to_string(), false)
        }
        Waited::Refused(m) => text_result(format!("error: {m}"), true),
        Waited::Cancelled => text_result("error: cancelled", true),
    }
}

fn collect(
    card_ids: &[String],
    total: Duration,
    cancel: Option<&Cancel>,
    progress: &mut dyn FnMut(&str),
) -> Value {
    let cards: Vec<WaitCard> = card_ids
        .iter()
        .map(|id| WaitCard {
            card_id: id.clone(),
            since: cursor(id),
        })
        .collect();
    let waited = wait_cards(&cards, total, false, cancel, &mut || {
        progress("waiting for the owner")
    });
    event_result(card_ids, waited)
}

fn similar_answer(similar: &[Found]) -> Value {
    let best = &similar[0];
    let already_answered =
        best.dismissed || matches!(best.state.as_str(), "answered" | "dismissed");
    json!({
        "status": "similar_answered",
        "already_answered": already_answered,
        "card_id": best.card_id,
        "similar_answered": similar,
        "hint": SIMILAR_HINT,
    })
}

fn find_args(a: &Value) -> Result<(Option<&str>, Option<&str>, Option<&str>, u32), String> {
    Ok((
        str_arg(a, "task", 1, 2000)?,
        str_arg(a, "key", 1, 200)?,
        str_arg(a, "query", 1, 2000)?,
        int_arg(a, "limit", 1, 50)?.unwrap_or(10) as u32,
    ))
}

fn find_answers(a: &Value) -> Value {
    let (task, key, query, limit) = match find_args(a) {
        Ok(p) => p,
        Err(m) => return arg_err(&m),
    };
    let found = tmgr_ipc::decisions::decisions_dir()
        .and_then(|d| Client::open_in(&d))
        .and_then(|c| c.find(task, key, query, Some(limit)));
    match found {
        Ok(answers) => text_result(json!({"answers": answers}).to_string(), false),
        Err(_) => text_result(FIND_UNAVAILABLE, true),
    }
}

fn ask_owner(a: &Value, cancel: Option<&Cancel>, progress: &mut dyn FnMut(&str)) -> Value {
    let parsed = parse_spec(a).and_then(|s| {
        let wait = wait_arg(a)?;
        let card = s.into_card("mcp", source(client_agent(), Some(parent_pid())))?;
        Ok((card, wait))
    });
    let (card, wait) = match parsed {
        Ok(p) => p,
        Err(m) => return arg_err(&m),
    };
    match send_card_full(card) {
        Ok(SentFull::Live(r))
            if r.outcome.as_deref() == Some(OUTCOME_SIMILAR) && !r.similar.is_empty() =>
        {
            text_result(similar_answer(&r.similar).to_string(), false)
        }
        Ok(SentFull::Live(r)) => collect(&[r.card_id.unwrap_or_default()], wait, cancel, progress),
        Ok(SentFull::Spooled(id)) => text_result(pending_json(&id, true).to_string(), false),
        Err(e) => text_result(format!("error: {e}"), true),
    }
}

fn get_answer(a: &Value, cancel: Option<&Cancel>, progress: &mut dyn FnMut(&str)) -> Value {
    let parsed = card_ids_arg(a).and_then(|ids| Ok((ids, wait_arg(a)?)));
    match parsed {
        Ok((ids, wait)) => collect(&ids, wait, cancel, progress),
        Err(m) => arg_err(&m),
    }
}

fn reply_owner(a: &Value, cancel: Option<&Cancel>, progress: &mut dyn FnMut(&str)) -> Value {
    let parsed = (|| {
        let id = card_id_arg(a)?;
        let text = str_arg(a, "text", 1, MAX_REPLY)?.map(str::to_string);
        let mut lines = Vec::new();
        for r in items(a, "line_replies")? {
            let file = str_arg(r, "file", 1, 4096)?.ok_or("line_replies need file")?;
            let line = int_arg(r, "line", 1, u32::MAX as i64)?.ok_or("line_replies need line")?;
            let text = str_arg(r, "text", 1, MAX_REPLY)?.ok_or("line_replies need text")?;
            let side = match str_arg(r, "side", 1, 3)? {
                None => None,
                Some(s @ ("old" | "new")) => Some(s.to_string()),
                Some(_) => return Err("line_replies side must be old or new".to_string()),
            };
            lines.push(LineComment {
                file: file.to_string(),
                line: line as u32,
                side,
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
        Ok(()) => collect(&[id], wait, cancel, progress),
        Err(e) if decisions::is_refusal(&e) => text_result(format!("error: {e}"), true),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => text_result(
            "error: the Decisions inbox did not accept the connection (unauthorized); the reply was not delivered, try again".to_string(),
            true,
        ),
        Err(_) => text_result(
            format!(
                "error: {}; the reply was not delivered",
                decisions::PENDING_NOTE
            ),
            true,
        ),
    }
}

pub fn call(
    name: &str,
    args: &Value,
    cancel: Option<&Cancel>,
    progress: &mut dyn FnMut(&str),
) -> Value {
    match name {
        "find_answers" => find_answers(args),
        "ask_owner" => ask_owner(args, cancel, progress),
        "get_answer" => get_answer(args, cancel, progress),
        _ => reply_owner(args, cancel, progress),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "c_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "c_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn a_card_is_required() {
        assert_eq!(
            card_ids_arg(&json!({})).unwrap_err(),
            "card_id or card_ids is required"
        );
        assert!(card_ids_arg(&json!({"card_ids": []})).is_err());
    }

    #[test]
    fn card_id_and_card_ids_are_merged_and_deduped() {
        let ids = card_ids_arg(&json!({"card_id": B, "card_ids": [A, B, A]})).unwrap();
        assert_eq!(ids, [B, A]);
        assert_eq!(card_ids_arg(&json!({"card_id": A})).unwrap(), [A]);
    }

    #[test]
    fn at_most_64_cards_after_the_merge() {
        let id = |i: usize| format!("c_{i:032x}");
        let many: Vec<String> = (0..64).map(id).collect();
        assert_eq!(card_ids_arg(&json!({"card_ids": many})).unwrap().len(), 64);
        let mut over = many.clone();
        over.push(id(64));
        assert!(card_ids_arg(&json!({"card_ids": over})).is_err());
        assert!(card_ids_arg(&json!({"card_id": id(64), "card_ids": many})).is_err());
    }

    #[test]
    fn bad_ids_are_refused() {
        assert!(card_ids_arg(&json!({"card_ids": [A, "nope"]})).is_err());
        assert!(card_ids_arg(&json!({"card_id": "nope"})).is_err());
        assert!(card_ids_arg(&json!({"card_ids": [1]})).is_err());
        assert!(card_ids_arg(&json!({"card_ids": "c_x"})).is_err());
    }

    fn found(state: &str, dismissed: bool) -> Found {
        Found {
            card_id: A.into(),
            title: "Ship it?".into(),
            question: Some("Release 1.2?".into()),
            state: state.into(),
            choice_label: None,
            free_text: None,
            note: None,
            dismissed,
            key: None,
            task: Some("TM-1".into()),
            created_ms: 1,
        }
    }

    #[test]
    fn find_answers_is_a_decisions_tool_with_its_schema() {
        assert!(is_decisions_tool("find_answers"));
        let def = defs()
            .into_iter()
            .find(|d| d["name"] == "find_answers")
            .expect("find_answers is listed");
        let props = &def["inputSchema"]["properties"];
        for key in ["task", "key", "query"] {
            assert_eq!(props[key]["type"], "string", "{key}");
        }
        assert_eq!(props["limit"]["type"], "integer");
        assert_eq!(props["limit"]["minimum"], 1);
        assert_eq!(props["limit"]["maximum"], 50);
        assert_eq!(def["inputSchema"]["additionalProperties"], false);
    }

    #[test]
    fn ask_owner_takes_force_new() {
        let def = defs()
            .into_iter()
            .find(|d| d["name"] == "ask_owner")
            .unwrap();
        assert_eq!(
            def["inputSchema"]["properties"]["force_new"]["type"],
            "boolean"
        );
        assert!(def["description"].as_str().unwrap().starts_with(
            "Before asking, call find_answers for the same task: the owner often already answered."
        ));
    }

    #[test]
    fn force_new_defaults_to_a_similarity_check() {
        let spec = parse_spec(&json!({"title": "t"})).unwrap();
        assert_eq!(spec.force_new, Some(false));
        let spec = parse_spec(&json!({"title": "t", "force_new": true})).unwrap();
        assert_eq!(spec.force_new, Some(true));
        assert!(parse_spec(&json!({"title": "t", "force_new": "yes"})).is_err());
    }

    #[test]
    fn find_args_are_bounded_and_default_to_ten() {
        let empty = json!({});
        let (task, key, query, limit) = find_args(&empty).unwrap();
        assert_eq!((task, key, query, limit), (None, None, None, 10));
        let given = json!({"task": "TM-1", "limit": 50});
        let (task, _, _, limit) = find_args(&given).unwrap();
        assert_eq!((task, limit), (Some("TM-1"), 50));
        assert!(find_args(&json!({"limit": 0})).is_err());
        assert!(find_args(&json!({"limit": 51})).is_err());
        assert!(find_args(&json!({"query": ""})).is_err());
    }

    #[test]
    fn a_similar_answer_reports_the_best_match_and_its_state() {
        let answered = similar_answer(&[found("answered", false), found("open", false)]);
        assert_eq!(answered["status"], "similar_answered");
        assert_eq!(answered["already_answered"], true);
        assert_eq!(answered["card_id"], A);
        assert_eq!(answered["similar_answered"].as_array().unwrap().len(), 2);
        assert!(answered["hint"]
            .as_str()
            .unwrap()
            .contains("force_new:true"));

        let dismissed = similar_answer(&[found("answered", true)]);
        assert_eq!(dismissed["already_answered"], true);
        assert_eq!(dismissed["similar_answered"][0]["dismissed"], true);

        let open = similar_answer(&[found("open", false)]);
        assert_eq!(open["already_answered"], false);
        assert!(open["hint"].as_str().unwrap().contains("get_answer"));
    }
}
