use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

use crate::error::Result;
use crate::model::{clean_name, project_hash, AgentMeta, Fact, FactEnd, FactKind, Tool};
use crate::store::{from_ms, ms, Store};

pub const MAX_NAME: usize = 128;

#[derive(Debug, Clone, PartialEq)]
pub struct StoredFact {
    pub tool: Tool,
    pub account: String,
    pub tool_session_id: String,
    pub agent_id: Option<String>,
    pub ts: DateTime<Utc>,
    pub kind: FactKind,
    pub name: String,
    pub detail: Option<String>,
    pub target: Option<String>,
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
    pub value: Option<u64>,
    pub is_error: bool,
    pub git_branch: Option<String>,
    pub project_hash: Option<String>,
    pub call_id: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StoredAgent {
    pub tool: Option<Tool>,
    pub account: String,
    pub tool_session_id: String,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub parent_agent_id: Option<String>,
    pub spawn_tool_use_id: Option<String>,
    pub spawn_depth: Option<u32>,
    pub originator: Option<String>,
    pub source: Option<String>,
    pub version: Option<String>,
    pub entrypoint: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub project_hash: Option<String>,
}

fn opt_name(v: &Option<String>) -> Option<String> {
    v.as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| clean_name(s, MAX_NAME))
}

/// `owner/name` with each part kept to `[A-Za-z0-9_.-]`, or `_` when it is not a repository name.
pub fn clean_repo(raw: &str) -> String {
    let part = |p: &str, max: usize| {
        let p: String = p
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-') {
                    c
                } else {
                    '_'
                }
            })
            .take(max)
            .collect();
        (!p.is_empty() && !p.starts_with('.')).then_some(p)
    };
    match raw.split_once('/') {
        Some((owner, name)) if !name.contains('/') => match (part(owner, 64), part(name, 100)) {
            (Some(o), Some(n)) => format!("{o}/{n}"),
            _ => "_".into(),
        },
        _ => "_".into(),
    }
}

fn fact_key(tool: Tool, f: &Fact) -> String {
    format!("{}|{}", tool.as_str(), f.key)
}

pub(crate) fn upsert_fact(conn: &Connection, tool: Tool, f: &Fact) -> Result<()> {
    let Some(kind) = f.kind else {
        return Ok(());
    };
    conn.prepare_cached(
        "INSERT INTO usage_fact
           (fact_key, tool, account, tool_session_id, agent_id, ts, kind, name, detail, target,
            duration_ms, value, is_error, git_branch, project_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(fact_key) DO UPDATE SET
           ts = excluded.ts, kind = excluded.kind, name = excluded.name,
           detail = excluded.detail, target = excluded.target,
           duration_ms = COALESCE(excluded.duration_ms, duration_ms),
           value = excluded.value,
           is_error = MAX(is_error, excluded.is_error),
           git_branch = excluded.git_branch, project_hash = excluded.project_hash",
    )?
    .execute(params![
        fact_key(tool, f),
        tool.as_str(),
        f.account,
        f.tool_session_id,
        f.agent_id.clone().unwrap_or_default(),
        ms(f.ts),
        kind.as_str(),
        match kind {
            FactKind::Pr => clean_repo(&f.name),
            _ => clean_name(&f.name, MAX_NAME),
        },
        opt_name(&f.detail),
        opt_name(&f.target),
        f.duration_ms.map(|d| d as i64),
        f.value.map(|v| v as i64),
        f.is_error,
        f.git_branch,
        f.cwd.as_deref().map(project_hash),
    ])?;
    Ok(())
}

pub(crate) fn end_fact(conn: &Connection, tool: Tool, end: &FactEnd) -> Result<()> {
    conn.prepare_cached(
        "UPDATE usage_fact SET ended_at = ?2,
           duration_ms = MAX(0, ?2 - ts),
           is_error = MAX(is_error, ?3)
         WHERE fact_key = ?1",
    )?
    .execute(params![
        format!("{}|{}", tool.as_str(), end.key),
        ms(end.ended_at),
        end.is_error,
    ])?;
    Ok(())
}

pub(crate) fn upsert_agent(conn: &Connection, tool: Tool, a: &AgentMeta) -> Result<()> {
    conn.prepare_cached(
        "INSERT INTO usage_agent
           (tool, account, tool_session_id, agent_id, agent_type, parent_agent_id,
            spawn_tool_use_id, spawn_depth, originator, source, version, entrypoint, started_at,
            project_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         ON CONFLICT(tool, account, tool_session_id, agent_id) DO UPDATE SET
           agent_type = COALESCE(excluded.agent_type, agent_type),
           parent_agent_id = COALESCE(excluded.parent_agent_id, parent_agent_id),
           spawn_tool_use_id = COALESCE(excluded.spawn_tool_use_id, spawn_tool_use_id),
           spawn_depth = COALESCE(excluded.spawn_depth, spawn_depth),
           originator = COALESCE(excluded.originator, originator),
           source = COALESCE(excluded.source, source),
           version = COALESCE(excluded.version, version),
           entrypoint = COALESCE(excluded.entrypoint, entrypoint),
           started_at = CASE WHEN excluded.started_at IS NULL THEN started_at
                             WHEN started_at IS NULL OR excluded.started_at < started_at
                               THEN excluded.started_at
                             ELSE started_at END,
           project_hash = COALESCE(project_hash, excluded.project_hash)",
    )?
    .execute(params![
        tool.as_str(),
        a.account,
        a.tool_session_id,
        a.agent_id.clone().unwrap_or_default(),
        opt_name(&a.agent_type),
        a.parent_agent_id,
        a.spawn_tool_use_id,
        a.spawn_depth,
        opt_name(&a.originator),
        opt_name(&a.source),
        opt_name(&a.version),
        opt_name(&a.entrypoint),
        a.started_at.map(ms),
        a.cwd.as_deref().map(project_hash),
    ])?;
    Ok(())
}

fn none_if_empty(s: String) -> Option<String> {
    (!s.is_empty()).then_some(s)
}

impl Store {
    pub fn record_fact(&self, tool: Tool, fact: &Fact) -> Result<()> {
        upsert_fact(&self.conn, tool, fact)
    }

    pub fn end_fact(&self, tool: Tool, end: &FactEnd) -> Result<()> {
        end_fact(&self.conn, tool, end)
    }

    pub fn record_agent(&self, tool: Tool, agent: &AgentMeta) -> Result<()> {
        upsert_agent(&self.conn, tool, agent)
    }

    /// Facts with `ts` in `[from, to)`, oldest first.
    pub fn facts(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<StoredFact>> {
        let mut stmt = self.conn.prepare(
            "SELECT tool, account, tool_session_id, agent_id, ts, kind, name, detail, target,
                    ended_at, duration_ms, value, is_error, git_branch, project_hash, fact_key
             FROM usage_fact WHERE ts >= ?1 AND ts < ?2 ORDER BY ts, id",
        )?;
        let rows = stmt.query_map([ms(from), ms(to)], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(5)?,
                StoredFact {
                    tool: Tool::Claude,
                    account: r.get(1)?,
                    tool_session_id: r.get(2)?,
                    agent_id: none_if_empty(r.get(3)?),
                    ts: from_ms(r.get(4)?),
                    kind: FactKind::ToolCall,
                    name: r.get(6)?,
                    detail: r.get(7)?,
                    target: r.get(8)?,
                    ended_at: r.get::<_, Option<i64>>(9)?.map(from_ms),
                    duration_ms: r.get::<_, Option<i64>>(10)?.map(|v| v.max(0) as u64),
                    value: r.get::<_, Option<i64>>(11)?.map(|v| v.max(0) as u64),
                    is_error: r.get(12)?,
                    git_branch: r.get(13)?,
                    project_hash: r.get(14)?,
                    call_id: r.get::<_, String>(15)?,
                },
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (tool, kind, mut f) = row?;
            let (Ok(tool), Some(kind)) = (Tool::parse(&tool), FactKind::parse(&kind)) else {
                continue;
            };
            f.tool = tool;
            f.kind = kind;
            f.call_id = f
                .call_id
                .rsplit_once('|')
                .map(|(_, id)| id.to_string())
                .unwrap_or_default();
            out.push(f);
        }
        Ok(out)
    }

    /// Every known agent node: Claude main sessions and subagents, Codex sessions.
    pub fn agents(&self) -> Result<Vec<StoredAgent>> {
        let mut stmt = self.conn.prepare(
            "SELECT tool, account, tool_session_id, agent_id, agent_type, parent_agent_id,
                    spawn_tool_use_id, spawn_depth, originator, source, version, entrypoint,
                    started_at, project_hash
             FROM usage_agent ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(StoredAgent {
                tool: Tool::parse(&r.get::<_, String>(0)?).ok(),
                account: r.get(1)?,
                tool_session_id: r.get(2)?,
                agent_id: none_if_empty(r.get(3)?),
                agent_type: r.get(4)?,
                parent_agent_id: r.get(5)?,
                spawn_tool_use_id: r.get(6)?,
                spawn_depth: r.get::<_, Option<i64>>(7)?.map(|v| v.max(0) as u32),
                originator: r.get(8)?,
                source: r.get(9)?,
                version: r.get(10)?,
                entrypoint: r.get(11)?,
                started_at: r.get::<_, Option<i64>>(12)?.map(from_ms),
                project_hash: r.get(13)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}
