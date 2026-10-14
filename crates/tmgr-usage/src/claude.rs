use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::collector::{
    for_each_line, rel_key, walk_files, Collector, LogFile, ParseOutcome, ParserState,
    MAX_LINE_BYTES,
};
use crate::error::Result;
use crate::model::{Account, ParsedRecord, Tokens, Tool, UsageEvent};

pub const PARSER_VERSION: u32 = 2;
const ASSISTANT_MARK: &[u8] = b"\"type\":\"assistant\"";

pub struct ClaudeCollector;

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    timestamp: Option<String>,
    cwd: Option<String>,
    #[serde(rename = "gitBranch")]
    git_branch: Option<String>,
    #[serde(rename = "isSidechain")]
    is_sidechain: Option<bool>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    model: Option<String>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_creation: Option<CacheCreation>,
    speed: Option<String>,
    service_tier: Option<String>,
    iterations: Option<Vec<Iteration>>,
}

#[derive(Deserialize)]
struct Iteration {
    #[serde(rename = "type")]
    kind: Option<String>,
    model: Option<String>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_creation: Option<CacheCreation>,
}

#[derive(Deserialize)]
struct CacheCreation {
    ephemeral_5m_input_tokens: Option<u64>,
    ephemeral_1h_input_tokens: Option<u64>,
}

fn has_assistant_mark(line: &[u8]) -> bool {
    memchr::memmem::find(line, ASSISTANT_MARK).is_some()
}

fn cache_writes(total: Option<u64>, cc: Option<CacheCreation>) -> (u64, u64) {
    match cc {
        Some(cc)
            if cc.ephemeral_5m_input_tokens.is_some() || cc.ephemeral_1h_input_tokens.is_some() =>
        {
            (
                cc.ephemeral_5m_input_tokens.unwrap_or(0),
                cc.ephemeral_1h_input_tokens.unwrap_or(0),
            )
        }
        _ => (total.unwrap_or(0), 0),
    }
}

fn into_events(line: Line, account: &str) -> Vec<UsageEvent> {
    let Some(main) = into_event(&line, account) else {
        return Vec::new();
    };
    let mut out = vec![main];
    let iterations = line
        .message
        .and_then(|m| m.usage)
        .and_then(|u| u.iterations)
        .unwrap_or_default();
    for (idx, it) in iterations.into_iter().enumerate() {
        let Some(kind) = it.kind.filter(|k| k != "message") else {
            continue;
        };
        let Some(model) = it.model.filter(|m| !m.is_empty() && m != "<synthetic>") else {
            continue;
        };
        let (cw5, cw1) = cache_writes(it.cache_creation_input_tokens, it.cache_creation);
        let mut ev = out[0].clone();
        ev.event_key = format!("{}|iter{idx}:{kind}", ev.event_key);
        ev.model = model;
        ev.tokens = Tokens {
            input: it.input_tokens.unwrap_or(0),
            output: it.output_tokens.unwrap_or(0),
            cache_read: it.cache_read_input_tokens.unwrap_or(0),
            cache_write_5m: cw5,
            cache_write_1h: cw1,
            reasoning: 0,
        };
        out.push(ev);
    }
    out
}

fn into_event(line: &Line, account: &str) -> Option<UsageEvent> {
    if line.kind.as_deref() != Some("assistant") {
        return None;
    }
    let msg = line.message.as_ref()?;
    let model = msg
        .model
        .clone()
        .filter(|m| !m.is_empty() && m != "<synthetic>")?;
    let usage = msg.usage.as_ref()?;
    let message_id = msg.id.clone().filter(|i| !i.is_empty())?;
    let session = line.session_id.clone().filter(|s| !s.is_empty())?;
    let ts = DateTime::parse_from_rfc3339(line.timestamp.as_deref()?)
        .ok()?
        .with_timezone(&Utc);
    let event_key = match line.request_id.as_deref().filter(|r| !r.is_empty()) {
        Some(req) => format!("{message_id}|{req}"),
        None => message_id,
    };
    let (cw5, cw1) = cache_writes(
        usage.cache_creation_input_tokens,
        usage.cache_creation.as_ref().map(|c| CacheCreation {
            ephemeral_5m_input_tokens: c.ephemeral_5m_input_tokens,
            ephemeral_1h_input_tokens: c.ephemeral_1h_input_tokens,
        }),
    );
    Some(UsageEvent {
        account: account.to_string(),
        tool_session_id: session,
        event_key,
        ts,
        model,
        tokens: Tokens {
            input: usage.input_tokens.unwrap_or(0),
            output: usage.output_tokens.unwrap_or(0),
            cache_read: usage.cache_read_input_tokens.unwrap_or(0),
            cache_write_5m: cw5,
            cache_write_1h: cw1,
            reasoning: 0,
        },
        speed: usage.speed.clone(),
        service_tier: usage.service_tier.clone(),
        git_branch: line.git_branch.clone().filter(|b| !b.is_empty()),
        cwd: line.cwd.clone().filter(|c| !c.is_empty()),
        is_sidechain: line.is_sidechain.unwrap_or(false),
    })
}

impl Collector for ClaudeCollector {
    fn tool(&self) -> Tool {
        Tool::Claude
    }

    fn parser_version(&self) -> u32 {
        PARSER_VERSION
    }

    fn discover(&self, account: &Account) -> Vec<LogFile> {
        walk_files(&account.root.join("projects"), "jsonl")
            .into_iter()
            .map(|path| LogFile {
                rel: rel_key(&account.root, &path),
                path,
                account: account.label.clone(),
                tool: Tool::Claude,
            })
            .collect()
    }

    fn parse(
        &self,
        file: &LogFile,
        from: u64,
        _state: &mut ParserState,
        sink: &mut dyn FnMut(ParsedRecord),
    ) -> Result<ParseOutcome> {
        let mut bad_lines = 0;
        let scan = for_each_line(&file.path, from, MAX_LINE_BYTES, &mut |raw| {
            if !has_assistant_mark(raw) {
                return;
            }
            match serde_json::from_slice::<Line>(raw) {
                Ok(line) => {
                    for ev in into_events(line, &file.account) {
                        sink(ParsedRecord::Event(ev));
                    }
                }
                Err(_) => bad_lines += 1,
            }
        })?;
        Ok(ParseOutcome {
            next_offset: scan.next_offset,
            oversized: scan.oversized,
            bad_lines,
        })
    }
}
