use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::collector::{
    for_each_line, rel_key, walk_files, Collector, LogFile, ParseOutcome, ParserState,
    MAX_LINE_BYTES,
};
use crate::error::Result;
use crate::model::{Account, ParsedRecord, Tokens, Tool, UsageEvent};

pub const PARSER_VERSION: u32 = 1;
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
}

#[derive(Deserialize)]
struct CacheCreation {
    ephemeral_5m_input_tokens: Option<u64>,
    ephemeral_1h_input_tokens: Option<u64>,
}

fn has_assistant_mark(line: &[u8]) -> bool {
    memchr::memmem::find(line, ASSISTANT_MARK).is_some()
}

fn into_event(line: Line, account: &str) -> Option<UsageEvent> {
    if line.kind.as_deref() != Some("assistant") {
        return None;
    }
    let msg = line.message?;
    let model = msg.model.filter(|m| !m.is_empty() && m != "<synthetic>")?;
    let usage = msg.usage?;
    let message_id = msg.id.filter(|i| !i.is_empty())?;
    let session = line.session_id.filter(|s| !s.is_empty())?;
    let ts = DateTime::parse_from_rfc3339(line.timestamp.as_deref()?)
        .ok()?
        .with_timezone(&Utc);
    let event_key = match line.request_id.as_deref().filter(|r| !r.is_empty()) {
        Some(req) => format!("{message_id}|{req}"),
        None => message_id,
    };
    let total_write = usage.cache_creation_input_tokens.unwrap_or(0);
    let (cw5, cw1) = match usage.cache_creation {
        Some(cc)
            if cc.ephemeral_5m_input_tokens.is_some() || cc.ephemeral_1h_input_tokens.is_some() =>
        {
            (
                cc.ephemeral_5m_input_tokens.unwrap_or(0),
                cc.ephemeral_1h_input_tokens.unwrap_or(0),
            )
        }
        _ => (total_write, 0),
    };
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
        speed: usage.speed,
        service_tier: usage.service_tier,
        git_branch: line.git_branch.filter(|b| !b.is_empty()),
        cwd: line.cwd.filter(|c| !c.is_empty()),
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
                    if let Some(ev) = into_event(line, &file.account) {
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
