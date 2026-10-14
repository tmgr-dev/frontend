use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashSet;

use crate::collector::{
    for_each_line, rel_key, walk_files, Collector, LogFile, ParseOutcome, ParserState,
    MAX_LINE_BYTES,
};
use crate::error::Result;
use crate::limits::{LimitSnapshot, LimitSource, LimitWindow};
use crate::model::{Account, ParsedRecord, SessionMeta, Tokens, Tool, UsageEvent};

pub const PARSER_VERSION: u32 = 2;
const MAX_PENDING: usize = 32;
const MAX_IDS: usize = 256;
const MARKERS: [&[u8]; 8] = [
    b"session_meta",
    b"turn_context",
    b"token_count",
    b"task_started",
    b"thread_settings_applied",
    b"token_usage_record",
    b"compacted",
    b"inter_agent_communication",
];
const SKIP_HEAD: &[u8] = b"\"type\":\"response_item\"";

pub struct CodexCollector;

fn num<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<u64, D::Error> {
    Ok(Option::<u64>::deserialize(d)?.unwrap_or(0))
}

#[derive(Deserialize, Serialize, Clone, Copy, Default, PartialEq, Eq, Debug)]
#[serde(default)]
struct Raw {
    #[serde(deserialize_with = "num")]
    input_tokens: u64,
    #[serde(deserialize_with = "num")]
    cached_input_tokens: u64,
    #[serde(deserialize_with = "num")]
    cache_write_input_tokens: u64,
    #[serde(deserialize_with = "num")]
    output_tokens: u64,
    #[serde(deserialize_with = "num")]
    reasoning_output_tokens: u64,
}

impl Raw {
    fn is_zero(&self) -> bool {
        *self == Raw::default()
    }

    fn decreased_from(&self, p: &Raw) -> bool {
        self.input_tokens < p.input_tokens
            || self.cached_input_tokens < p.cached_input_tokens
            || self.cache_write_input_tokens < p.cache_write_input_tokens
            || self.output_tokens < p.output_tokens
            || self.reasoning_output_tokens < p.reasoning_output_tokens
    }

    fn minus(&self, p: &Raw) -> Raw {
        Raw {
            input_tokens: self.input_tokens - p.input_tokens,
            cached_input_tokens: self.cached_input_tokens - p.cached_input_tokens,
            cache_write_input_tokens: self.cache_write_input_tokens - p.cache_write_input_tokens,
            output_tokens: self.output_tokens - p.output_tokens,
            reasoning_output_tokens: self.reasoning_output_tokens - p.reasoning_output_tokens,
        }
    }

    fn tokens(&self) -> Tokens {
        Tokens {
            input: self
                .input_tokens
                .saturating_sub(self.cached_input_tokens)
                .saturating_sub(self.cache_write_input_tokens),
            output: self.output_tokens,
            cache_read: self.cached_input_tokens,
            cache_write_5m: self.cache_write_input_tokens,
            cache_write_1h: 0,
            reasoning: self.reasoning_output_tokens,
        }
    }
}

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: Option<String>,
    timestamp: Option<String>,
    trigger_turn: Option<bool>,
    payload: Option<Payload>,
}

#[derive(Deserialize, Default)]
struct Payload {
    #[serde(rename = "type")]
    kind: Option<String>,
    id: Option<String>,
    session_id: Option<String>,
    parent_thread_id: Option<String>,
    cwd: Option<String>,
    git: Option<Git>,
    model: Option<String>,
    info: Option<Info>,
    rate_limits: Option<RateLimits>,
    thread_settings: Option<Settings>,
    response_id: Option<String>,
    usage: Option<Raw>,
    compaction_response_id: Option<String>,
    trigger_turn: Option<bool>,
}

#[derive(Deserialize)]
struct Git {
    branch: Option<String>,
}

#[derive(Deserialize)]
struct Settings {
    service_tier: Option<String>,
}

#[derive(Deserialize)]
struct Info {
    total_token_usage: Option<Raw>,
    last_token_usage: Option<Raw>,
}

#[derive(Deserialize)]
struct RateLimits {
    plan_type: Option<String>,
    primary: Option<Window>,
    secondary: Option<Window>,
}

#[derive(Deserialize)]
struct Window {
    used_percent: Option<f64>,
    window_minutes: Option<u64>,
    resets_at: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Pending {
    id: String,
    ts: DateTime<Utc>,
    usage: Raw,
    model: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct State {
    meta_seen: bool,
    sid: Option<String>,
    child: bool,
    replay: bool,
    prev: Option<Raw>,
    model: Option<String>,
    cwd: Option<String>,
    branch: Option<String>,
    tier: Option<String>,
    ordinal: u64,
    compacted: Vec<String>,
    emitted: Vec<String>,
    pending: Vec<Pending>,
    #[serde(default)]
    emitted_usage: Vec<(String, Raw)>,
    last_record: Option<String>,
}

fn push_bounded<T>(v: &mut Vec<T>, item: T, max: usize) {
    v.push(item);
    if v.len() > max {
        v.remove(0);
    }
}

fn normalize_tier(raw: &str) -> Option<String> {
    match raw {
        "priority" | "fast" => Some("priority".into()),
        "flex" => Some("flex".into()),
        "default" | "standard" => Some("default".into()),
        _ => None,
    }
}

fn parse_ts(s: Option<&str>) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s?)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

fn classify(minutes: u64) -> LimitWindow {
    match minutes {
        300 => LimitWindow::Session,
        10080 => LimitWindow::Weekly,
        m => LimitWindow::Model(format!("window:{m}m")),
    }
}

fn stem(file: &LogFile) -> String {
    file.path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".into())
}

struct Ctx<'a> {
    file: &'a LogFile,
    sink: &'a mut dyn FnMut(ParsedRecord),
}

impl State {
    fn session(&self, file: &LogFile) -> String {
        self.sid.clone().unwrap_or_else(|| stem(file))
    }

    fn event(
        &self,
        file: &LogFile,
        key: String,
        ts: DateTime<Utc>,
        model: Option<String>,
        raw: &Raw,
    ) -> UsageEvent {
        UsageEvent {
            account: file.account.clone(),
            tool_session_id: self.session(file),
            event_key: key,
            ts,
            model: model.unwrap_or_else(|| "unknown".into()),
            tokens: raw.tokens(),
            speed: None,
            service_tier: self.tier.clone(),
            git_branch: self.branch.clone(),
            cwd: self.cwd.clone(),
            is_sidechain: self.child,
        }
    }

    fn mark_emitted(&mut self, id: &str) {
        if !self.emitted.iter().any(|e| e == id) {
            push_bounded(&mut self.emitted, id.to_string(), MAX_IDS);
        }
    }

    fn handle(&mut self, line: Line, ctx: &mut Ctx<'_>) {
        let ts = parse_ts(line.timestamp.as_deref());
        let kind = line.kind.as_deref().unwrap_or("");
        match kind {
            "session_meta" => self.session_meta(line.payload, ts, ctx),
            "turn_context" => {
                if let Some(p) = line.payload {
                    if let Some(m) = p.model.filter(|m| !m.is_empty()) {
                        self.model = Some(m);
                    }
                    if let Some(c) = p.cwd.filter(|c| !c.is_empty()) {
                        self.cwd = Some(c);
                    }
                }
            }
            "inter_agent_communication_metadata" | "inter_agent_communication" => {
                let trigger = line
                    .trigger_turn
                    .or(line.payload.and_then(|p| p.trigger_turn));
                if trigger == Some(true) {
                    self.replay = false;
                }
            }
            "compacted" => self.compacted(line.payload, ctx),
            "token_usage_record" => self.usage_record(line.payload, ts, ctx),
            "event_msg" => {
                let Some(p) = line.payload else { return };
                match p.kind.as_deref() {
                    Some("task_started") => self.replay = false,
                    Some("inter_agent_communication_metadata" | "inter_agent_communication") => {
                        if p.trigger_turn == Some(true) {
                            self.replay = false;
                        }
                    }
                    Some("thread_settings_applied") => {
                        if let Some(t) = p.thread_settings.and_then(|s| s.service_tier) {
                            self.tier = normalize_tier(&t);
                        }
                    }
                    Some("token_count") => self.token_count(p, ts, ctx),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn session_meta(
        &mut self,
        payload: Option<Payload>,
        ts: Option<DateTime<Utc>>,
        ctx: &mut Ctx<'_>,
    ) {
        let Some(p) = payload else { return };
        if self.meta_seen {
            return;
        }
        self.meta_seen = true;
        self.sid = p.id.or(p.session_id).filter(|s| !s.is_empty());
        self.cwd = p.cwd.filter(|c| !c.is_empty());
        self.branch = p.git.and_then(|g| g.branch).filter(|b| !b.is_empty());
        let parent = p.parent_thread_id.filter(|s| !s.is_empty());
        if parent.is_some() {
            self.child = true;
            self.replay = true;
        }
        (ctx.sink)(ParsedRecord::Session(SessionMeta {
            account: ctx.file.account.clone(),
            tool_session_id: self.session(ctx.file),
            started_at: ts,
            cwd: self.cwd.clone(),
            git_branch: self.branch.clone(),
            parent_session_id: parent,
        }));
    }

    fn token_count(&mut self, p: Payload, ts: Option<DateTime<Utc>>, ctx: &mut Ctx<'_>) {
        if let (false, Some(rl), Some(ts)) = (self.replay, p.rate_limits.as_ref(), ts) {
            self.limits(rl, ts, ctx);
        }
        let Some(info) = p.info else { return };
        let delta = match (info.total_token_usage, self.prev) {
            (Some(t), Some(prev)) => {
                self.prev = Some(t);
                if t == prev {
                    Raw::default()
                } else if t.decreased_from(&prev) {
                    t
                } else {
                    t.minus(&prev)
                }
            }
            (Some(t), None) => {
                self.prev = Some(t);
                info.last_token_usage.unwrap_or(t)
            }
            (None, _) => info.last_token_usage.unwrap_or_default(),
        };
        if self.replay {
            return;
        }
        self.ordinal += 1;
        if delta.is_zero() {
            return;
        }
        if let Some(id) = self.last_record.clone() {
            let covered = self.pending.iter().any(|r| r.id == id && r.usage == delta);
            if covered {
                self.pending.retain(|r| r.id != id);
                self.mark_emitted(&id);
            } else if self
                .emitted_usage
                .iter()
                .any(|(i, u)| *i == id && *u == delta)
            {
                self.last_record = None;
                return;
            }
        }
        let Some(ts) = ts else { return };
        let key = format!("{}|{}", self.session(ctx.file), self.ordinal);
        let ev = self.event(ctx.file, key, ts, self.model.clone(), &delta);
        (ctx.sink)(ParsedRecord::Event(ev));
    }

    fn limits(&self, rl: &RateLimits, observed_at: DateTime<Utc>, ctx: &mut Ctx<'_>) {
        for w in [&rl.primary, &rl.secondary].into_iter().flatten() {
            let (Some(used), Some(minutes)) = (w.used_percent, w.window_minutes) else {
                continue;
            };
            (ctx.sink)(ParsedRecord::Limit(LimitSnapshot {
                tool: Tool::Codex,
                account: ctx.file.account.clone(),
                window: classify(minutes),
                used_pct: used,
                resets_at: w.resets_at.and_then(|s| Utc.timestamp_opt(s, 0).single()),
                plan: rl.plan_type.clone(),
                observed_at,
                source: LimitSource::Rollout,
                tool_session_id: self.session(ctx.file),
            }));
        }
    }

    fn compacted(&mut self, payload: Option<Payload>, ctx: &mut Ctx<'_>) {
        if self.replay {
            return;
        }
        let Some(id) = payload
            .and_then(|p| p.compaction_response_id)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        else {
            return;
        };
        if !self.compacted.contains(&id) {
            push_bounded(&mut self.compacted, id.clone(), MAX_IDS);
        }
        if let Some(pos) = self.pending.iter().position(|r| r.id == id) {
            let rec = self.pending.remove(pos);
            self.emit_record(rec, ctx);
        }
    }

    fn usage_record(
        &mut self,
        payload: Option<Payload>,
        ts: Option<DateTime<Utc>>,
        ctx: &mut Ctx<'_>,
    ) {
        if self.replay {
            return;
        }
        let (Some(p), Some(ts)) = (payload, ts) else {
            return;
        };
        let Some(id) = p
            .response_id
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        else {
            return;
        };
        let Some(usage) = p.usage.filter(|u| !u.is_zero()) else {
            return;
        };
        if self.emitted.contains(&id) {
            return;
        }
        let rec = Pending {
            id: id.clone(),
            ts,
            usage,
            model: p
                .model
                .filter(|m| !m.is_empty())
                .or_else(|| self.model.clone()),
        };
        self.last_record = Some(id.clone());
        if self.compacted.contains(&id) {
            self.emit_record(rec, ctx);
        } else {
            self.pending.retain(|r| r.id != id);
            push_bounded(&mut self.pending, rec, MAX_PENDING);
        }
    }

    fn emit_record(&mut self, rec: Pending, ctx: &mut Ctx<'_>) {
        if self.emitted.contains(&rec.id) {
            return;
        }
        self.mark_emitted(&rec.id);
        push_bounded(
            &mut self.emitted_usage,
            (rec.id.clone(), rec.usage),
            MAX_IDS,
        );
        let key = format!("{}|compact:{}", self.session(ctx.file), rec.id);
        let ev = self.event(ctx.file, key, rec.ts, rec.model, &rec.usage);
        (ctx.sink)(ParsedRecord::Event(ev));
    }
}

fn relevant(raw: &[u8]) -> bool {
    let head = &raw[..raw.len().min(160)];
    if memchr::memmem::find(head, SKIP_HEAD).is_some() {
        return false;
    }
    MARKERS
        .iter()
        .any(|m| memchr::memmem::find(raw, m).is_some())
}

fn strip_root(rel: &str) -> &str {
    rel.split_once('/').map_or(rel, |(_, rest)| rest)
}

impl Collector for CodexCollector {
    fn tool(&self) -> Tool {
        Tool::Codex
    }

    fn parser_version(&self) -> u32 {
        PARSER_VERSION
    }

    fn discover(&self, account: &Account) -> Vec<LogFile> {
        let to_file = |path: std::path::PathBuf| LogFile {
            rel: rel_key(&account.root, &path),
            path,
            account: account.label.clone(),
            tool: Tool::Codex,
        };
        let active: Vec<LogFile> = walk_files(&account.root.join("sessions"), "jsonl")
            .into_iter()
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("rollout-"))
            })
            .map(to_file)
            .collect();
        let seen: HashSet<String> = active
            .iter()
            .map(|f| strip_root(&f.rel).to_string())
            .collect();
        let archived = walk_files(&account.root.join("archived_sessions"), "jsonl")
            .into_iter()
            .map(to_file)
            .filter(|f| !seen.contains(strip_root(&f.rel)));
        active.iter().cloned().chain(archived).collect()
    }

    fn parse(
        &self,
        file: &LogFile,
        from: u64,
        state: &mut ParserState,
        sink: &mut dyn FnMut(ParsedRecord),
    ) -> Result<ParseOutcome> {
        let mut st: State = if from == 0 {
            State::default()
        } else {
            state.get().unwrap_or_default()
        };
        let mut bad_lines = 0;
        let mut ctx = Ctx { file, sink };
        let scan = for_each_line(&file.path, from, MAX_LINE_BYTES, &mut |raw| {
            if !relevant(raw) {
                return;
            }
            match serde_json::from_slice::<Line>(raw) {
                Ok(line) => st.handle(line, &mut ctx),
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
