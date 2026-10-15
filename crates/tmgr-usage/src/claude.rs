use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;

use crate::collector::{
    for_each_line, rel_key, walk_files, Collector, LogFile, ParseOutcome, ParserState,
    MAX_LINE_BYTES,
};
use crate::error::Result;
use crate::model::{
    Account, AgentMeta, Fact, FactEnd, FactKind, ParsedRecord, Tokens, Tool, UsageEvent,
};

pub const PARSER_VERSION: u32 = 3;
const MAX_META_BYTES: u64 = 64 * 1024;
const MAX_SLASH_BYTES: usize = 128;
const INTERPRETERS: [&str; 12] = [
    "node", "python", "python3", "bash", "sh", "zsh", "npx", "bun", "deno", "uv", "uvx", "run",
];
const MARKS: [&[u8]; 8] = [
    b"\"type\":\"assistant\"",
    b"\"tool_result\"",
    b"<command-name>",
    b"\"type\":\"hook_",
    b"\"type\":\"skill_listing\"",
    b"\"type\":\"system\"",
    b"\"type\":\"permission-mode\"",
    b"\"type\":\"pr-link\"",
];

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
    uuid: Option<String>,
    #[serde(rename = "agentId")]
    agent_id: Option<String>,
    version: Option<String>,
    entrypoint: Option<String>,
    message: Option<Message>,
    attachment: Option<Attachment>,
    subtype: Option<String>,
    #[serde(rename = "durationMs")]
    duration_ms: Option<u64>,
    #[serde(rename = "messageCount")]
    message_count: Option<u64>,
    #[serde(rename = "compactMetadata")]
    compact: Option<CompactMeta>,
    #[serde(rename = "permissionMode")]
    permission_mode: Option<String>,
    #[serde(rename = "prNumber")]
    pr_number: Option<u64>,
    #[serde(rename = "prRepository")]
    pr_repository: Option<String>,
}

#[derive(Deserialize)]
struct CompactMeta {
    #[serde(rename = "preTokens")]
    pre_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct Attachment {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "hookEvent")]
    hook_event: Option<String>,
    #[serde(rename = "hookName")]
    hook_name: Option<String>,
    #[serde(rename = "durationMs")]
    duration_ms: Option<u64>,
    #[serde(rename = "exitCode")]
    exit_code: Option<i64>,
    #[serde(rename = "skillCount")]
    skill_count: Option<u64>,
    command: Option<String>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    model: Option<String>,
    usage: Option<Usage>,
    content: Option<Content>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Blocks(Vec<Block>),
    Other(serde::de::IgnoredAny),
}

#[derive(Deserialize)]
struct Block {
    #[serde(rename = "type")]
    kind: Option<String>,
    id: Option<String>,
    name: Option<String>,
    text: Option<String>,
    tool_use_id: Option<String>,
    is_error: Option<bool>,
    input: Option<ToolInput>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ToolInput {
    Known(Input),
    Other(serde::de::IgnoredAny),
}

#[derive(Deserialize)]
struct Input {
    subagent_type: Option<String>,
    run_in_background: Option<bool>,
    skill: Option<String>,
    command: Option<String>,
    file_path: Option<String>,
    notebook_path: Option<String>,
}

#[derive(Deserialize)]
struct SubagentMeta {
    #[serde(rename = "agentType")]
    agent_type: Option<String>,
    #[serde(rename = "parentAgentId")]
    parent_agent_id: Option<String>,
    #[serde(rename = "spawnDepth")]
    spawn_depth: Option<u32>,
    #[serde(rename = "toolUseId")]
    tool_use_id: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct ClaudeState {
    last_ts: Option<DateTime<Utc>>,
    mode: Option<String>,
    mode_changes: u64,
    agent_seen: bool,
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

fn wanted(line: &[u8]) -> bool {
    MARKS
        .iter()
        .any(|m| memchr::memmem::find(line, m).is_some())
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

fn into_events(line: Line, account: &str, agent: Option<&str>) -> Vec<UsageEvent> {
    let Some(main) = into_event(&line, account, agent) else {
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

fn into_event(line: &Line, account: &str, agent: Option<&str>) -> Option<UsageEvent> {
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
        agent_id: agent.map(str::to_string),
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
        state: &mut ParserState,
        sink: &mut dyn FnMut(ParsedRecord),
    ) -> Result<ParseOutcome> {
        let mut st: ClaudeState = if from == 0 {
            ClaudeState::default()
        } else {
            state.get().unwrap_or_default()
        };
        let file_agent = subagent_id_from_path(&file.path);
        let mut bad_lines = 0;
        let scan = for_each_line(&file.path, from, MAX_LINE_BYTES, &mut |raw| {
            if !wanted(raw) {
                return;
            }
            match serde_json::from_slice::<Line>(raw) {
                Ok(line) => handle_line(line, file, file_agent.as_deref(), &mut st, sink),
                Err(_) => bad_lines += 1,
            }
        })?;
        state.set(&st);
        Ok(ParseOutcome {
            next_offset: scan.next_offset,
            oversized: scan.oversized,
            bad_lines,
        })
    }
}

fn subagent_id_from_path(path: &Path) -> Option<String> {
    let in_subagents = path
        .parent()
        .and_then(|p| p.file_name())
        .is_some_and(|n| n == "subagents");
    if !in_subagents {
        return None;
    }
    path.file_stem()?
        .to_str()?
        .strip_prefix("agent-")
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

fn read_subagent_meta(path: &Path) -> Option<SubagentMeta> {
    let mut buf = Vec::new();
    std::fs::File::open(path.with_extension("meta.json"))
        .ok()?
        .take(MAX_META_BYTES)
        .read_to_end(&mut buf)
        .ok()?;
    serde_json::from_slice(&buf).ok()
}

fn parse_ts(raw: Option<&str>) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw?)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

fn handle_line(
    line: Line,
    file: &LogFile,
    file_agent: Option<&str>,
    st: &mut ClaudeState,
    sink: &mut dyn FnMut(ParsedRecord),
) {
    let agent: Option<String> = file_agent.map(|f| line.agent_id.clone().unwrap_or(f.to_string()));
    let session = line.session_id.clone().filter(|s| !s.is_empty());
    let ts = parse_ts(line.timestamp.as_deref());
    if let Some(t) = ts {
        st.last_ts = Some(t);
    }
    if !st.agent_seen {
        if let (Some(session), Some(ts)) = (&session, ts) {
            st.agent_seen = true;
            let meta = file_agent.and_then(|_| read_subagent_meta(&file.path));
            sink(ParsedRecord::Agent(AgentMeta {
                account: file.account.clone(),
                tool_session_id: session.clone(),
                agent_id: agent.clone(),
                agent_type: meta.as_ref().and_then(|m| m.agent_type.clone()),
                parent_agent_id: meta.as_ref().and_then(|m| m.parent_agent_id.clone()),
                spawn_tool_use_id: meta.as_ref().and_then(|m| m.tool_use_id.clone()),
                spawn_depth: meta.as_ref().and_then(|m| m.spawn_depth),
                version: line.version.clone().filter(|v| !v.is_empty()),
                entrypoint: line.entrypoint.clone().filter(|v| !v.is_empty()),
                started_at: Some(ts),
                cwd: line.cwd.clone().filter(|c| !c.is_empty()),
                ..AgentMeta::default()
            }));
        }
    }
    let Some(session) = session else {
        return;
    };
    let fact = |kind: FactKind, key_id: &str, ts: DateTime<Utc>| Fact {
        account: file.account.clone(),
        key: format!("{session}|{}|{key_id}", kind.as_str()),
        tool_session_id: session.clone(),
        agent_id: agent.clone(),
        ts,
        kind: Some(kind),
        git_branch: line.git_branch.clone().filter(|b| !b.is_empty()),
        cwd: line.cwd.clone().filter(|c| !c.is_empty()),
        ..Fact::default()
    };
    let uuid = line.uuid.clone().filter(|u| !u.is_empty());
    match line.kind.as_deref() {
        Some("assistant") => {
            if let Some(ts) = ts {
                if let Some(Content::Blocks(blocks)) =
                    line.message.as_ref().and_then(|m| m.content.as_ref())
                {
                    for block in blocks {
                        tool_use_facts(block, &fact, ts, sink);
                    }
                }
            }
            for ev in into_events(line, &file.account, agent.as_deref()) {
                sink(ParsedRecord::Event(ev));
            }
        }
        Some("user") => {
            let Some(ts) = ts else {
                return;
            };
            match line.message.as_ref().and_then(|m| m.content.as_ref()) {
                Some(Content::Blocks(blocks)) => {
                    for block in blocks {
                        if block.kind.as_deref() == Some("tool_result") {
                            if let Some(id) = block.tool_use_id.as_deref() {
                                sink(ParsedRecord::FactEnd(FactEnd {
                                    key: format!("{session}|tool_call|{id}"),
                                    ended_at: ts,
                                    is_error: block.is_error.unwrap_or(false),
                                }));
                            }
                        } else if let (Some(text), Some(uuid)) = (block.text.as_deref(), &uuid) {
                            slash_fact(text, uuid, &fact, ts, sink);
                        }
                    }
                }
                Some(Content::Text(text)) => {
                    if let Some(uuid) = &uuid {
                        slash_fact(text, uuid, &fact, ts, sink);
                    }
                }
                _ => {}
            }
        }
        Some("attachment") => {
            let (Some(ts), Some(uuid), Some(att)) = (ts, &uuid, line.attachment.as_ref()) else {
                return;
            };
            let kind = att.kind.as_deref().unwrap_or("");
            if kind == "skill_listing" {
                sink(ParsedRecord::Fact(Fact {
                    value: att.skill_count,
                    ..fact(FactKind::SkillListing, uuid, ts)
                }));
            } else if kind.starts_with("hook_") && kind != "hook_additional_context" {
                let named = matches!(
                    kind,
                    "hook_success" | "hook_non_blocking_error" | "hook_cancelled"
                );
                if !named && att.duration_ms.is_none() {
                    return;
                }
                let exit = att.exit_code.unwrap_or(0);
                sink(ParsedRecord::Fact(Fact {
                    name: att
                        .command
                        .as_deref()
                        .map(hook_script_name)
                        .unwrap_or_else(|| "inline".into()),
                    detail: att.hook_event.clone(),
                    target: att
                        .hook_name
                        .as_deref()
                        .and_then(|n| n.split_once(':'))
                        .map(|(_, m)| m.to_string())
                        .filter(|m| !m.is_empty()),
                    duration_ms: att.duration_ms,
                    value: u64::try_from(exit).ok(),
                    is_error: kind != "hook_success" || exit != 0,
                    ..fact(FactKind::Hook, uuid, ts)
                }));
            }
        }
        Some("system") => {
            let (Some(ts), Some(uuid)) = (ts, &uuid) else {
                return;
            };
            let record = match line.subtype.as_deref() {
                Some("turn_duration") => Fact {
                    duration_ms: line.duration_ms,
                    value: line.message_count,
                    ..fact(FactKind::Turn, uuid, ts)
                },
                Some("away_summary") => fact(FactKind::Away, uuid, ts),
                Some("compact_boundary") => Fact {
                    value: line.compact.as_ref().and_then(|c| c.pre_tokens),
                    ..fact(FactKind::Compact, uuid, ts)
                },
                _ => return,
            };
            sink(ParsedRecord::Fact(record));
        }
        Some("permission-mode") => {
            let (Some(mode), Some(ts)) = (line.permission_mode.as_deref(), st.last_ts) else {
                return;
            };
            if st.mode.as_deref() == Some(mode) {
                return;
            }
            st.mode_changes += 1;
            st.mode = Some(mode.to_string());
            sink(ParsedRecord::Fact(Fact {
                name: mode.to_string(),
                ..fact(FactKind::PermissionMode, &st.mode_changes.to_string(), ts)
            }));
        }
        Some("pr-link") => {
            let (Some(ts), Some(repo), Some(number)) =
                (ts, line.pr_repository.as_deref(), line.pr_number)
            else {
                return;
            };
            sink(ParsedRecord::Fact(Fact {
                name: repo.to_string(),
                value: Some(number),
                ..fact(FactKind::Pr, &format!("{repo}#{number}"), ts)
            }));
        }
        _ => {}
    }
}

fn tool_use_facts(
    block: &Block,
    fact: &dyn Fn(FactKind, &str, DateTime<Utc>) -> Fact,
    ts: DateTime<Utc>,
    sink: &mut dyn FnMut(ParsedRecord),
) {
    if block.kind.as_deref() != Some("tool_use") {
        return;
    }
    let (Some(id), Some(name)) = (
        block.id.as_deref().filter(|i| !i.is_empty()),
        block.name.as_deref().filter(|n| !n.is_empty()),
    ) else {
        return;
    };
    let input = match &block.input {
        Some(ToolInput::Known(i)) => Some(i),
        _ => None,
    };
    let detail = match name {
        "Agent" | "Task" => input.and_then(|i| i.subagent_type.clone()),
        "Bash" => input
            .and_then(|i| i.command.as_deref())
            .filter(|c| c.contains("codex exec") || c.contains("codex-companion"))
            .map(|_| "codex".to_string()),
        _ => None,
    };
    sink(ParsedRecord::Fact(Fact {
        name: name.to_string(),
        detail,
        target: input
            .and_then(|i| i.run_in_background)
            .filter(|b| *b)
            .map(|_| "background".to_string()),
        ..fact(FactKind::ToolCall, id, ts)
    }));
    match (name, input) {
        ("Skill", Some(i)) => {
            if let Some(skill) = i.skill.as_deref().filter(|s| !s.is_empty()) {
                sink(ParsedRecord::Fact(Fact {
                    name: skill.to_string(),
                    ..fact(FactKind::Skill, id, ts)
                }));
            }
        }
        ("Edit" | "Write" | "MultiEdit" | "NotebookEdit", i) => {
            let path = i.and_then(|i| i.file_path.as_deref().or(i.notebook_path.as_deref()));
            sink(ParsedRecord::Fact(Fact {
                name: file_extension(path),
                ..fact(FactKind::FileEdit, id, ts)
            }));
        }
        _ => {}
    }
}

fn slash_fact(
    text: &str,
    uuid: &str,
    fact: &dyn Fn(FactKind, &str, DateTime<Utc>) -> Fact,
    ts: DateTime<Utc>,
    sink: &mut dyn FnMut(ParsedRecord),
) {
    let Some(start) = text
        .find("<command-name>")
        .map(|i| i + "<command-name>".len())
    else {
        return;
    };
    let rest = &text[start..];
    let mut end = rest.find("</command-name>").unwrap_or(rest.len());
    end = end.min(MAX_SLASH_BYTES);
    while !rest.is_char_boundary(end) {
        end -= 1;
    }
    let name = rest[..end].trim().trim_start_matches('/');
    if name.is_empty() {
        return;
    }
    sink(ParsedRecord::Fact(Fact {
        name: name.to_string(),
        ..fact(FactKind::Slash, uuid, ts)
    }));
}

fn file_extension(path: Option<&str>) -> String {
    let Some(base) = path.and_then(|p| p.rsplit(['/', '\\']).next()) else {
        return "none".into();
    };
    match base.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => {
            let ext = ext.to_ascii_lowercase();
            if ext.len() <= 16
                && ext
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            {
                ext
            } else {
                "other".into()
            }
        }
        _ => "none".into(),
    }
}

fn shell_tokens(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in command.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                started = true;
            }
            None if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            None => {
                cur.push(c);
                started = true;
            }
        }
    }
    if started {
        out.push(cur);
    }
    out
}

pub fn hook_script_name(command: &str) -> String {
    const INLINE: &str = "inline";
    for token in shell_tokens(command) {
        if token.contains('=') || INTERPRETERS.contains(&token.as_str()) {
            continue;
        }
        if matches!(token.as_str(), "env" | "exec" | "sudo") {
            continue;
        }
        if token.starts_with('-') {
            if matches!(token.as_str(), "-c" | "-e" | "--eval") {
                return INLINE.into();
            }
            continue;
        }
        let base = token.rsplit(['/', '\\']).next().unwrap_or("");
        let valid = (1..=64).contains(&base.len())
            && !base.starts_with('.')
            && base
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        return if valid { base.into() } else { INLINE.into() };
    }
    INLINE.into()
}
