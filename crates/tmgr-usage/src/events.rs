use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::Result;
use crate::link::LinkEvent;
use crate::model::{project_hash, project_name, CostBasis, SessionMeta, Tokens, Tool, UsageEvent};
use crate::store::{from_ms, ms, Store};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upsert {
    New,
    Updated,
    Unchanged,
}

struct Stored {
    id: i64,
    ts: i64,
    session_id: i64,
    sid: String,
    model: String,
    tokens: Tokens,
    cost: Option<f64>,
    speed: Option<String>,
    tier: Option<String>,
    agent_id: Option<String>,
}

pub(crate) fn ensure_session(
    conn: &Connection,
    tool: Tool,
    account: &str,
    tool_session_id: &str,
    cwd: Option<&str>,
    git_branch: Option<&str>,
) -> Result<i64> {
    conn.prepare_cached(
        "INSERT OR IGNORE INTO usage_session
           (tool, account, tool_session_id, git_branch, cwd, project_name, project_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?
    .execute(params![
        tool.as_str(),
        account,
        tool_session_id,
        git_branch,
        cwd,
        cwd.map(project_name),
        cwd.map(project_hash),
    ])?;
    Ok(conn
        .prepare_cached(
            "SELECT id FROM usage_session WHERE tool = ?1 AND account = ?2 AND tool_session_id = ?3",
        )?
        .query_row(params![tool.as_str(), account, tool_session_id], |r| {
            r.get(0)
        })?)
}

fn refresh_session_range(conn: &Connection, session_id: i64) -> Result<()> {
    conn.prepare_cached(
        "UPDATE usage_session SET
           started_at = (SELECT MIN(ts) FROM usage_event WHERE session_id = ?1),
           ended_at = (SELECT MAX(ts) FROM usage_event WHERE session_id = ?1)
         WHERE id = ?1",
    )?
    .execute([session_id])?;
    Ok(())
}

pub(crate) fn upsert_session_meta(conn: &Connection, tool: Tool, meta: &SessionMeta) -> Result<()> {
    let id = ensure_session(
        conn,
        tool,
        &meta.account,
        &meta.tool_session_id,
        meta.cwd.as_deref(),
        meta.git_branch.as_deref(),
    )?;
    conn.execute(
        "UPDATE usage_session SET
           started_at = CASE WHEN ?2 IS NULL THEN started_at
                             WHEN started_at IS NULL OR ?2 < started_at THEN ?2 ELSE started_at END,
           parent_session_id = COALESCE(?3, parent_session_id),
           cwd = COALESCE(cwd, ?4),
           project_name = COALESCE(project_name, ?5),
           project_hash = COALESCE(project_hash, ?6),
           git_branch = COALESCE(git_branch, ?7)
         WHERE id = ?1",
        params![
            id,
            meta.started_at.map(ms),
            meta.parent_session_id,
            meta.cwd,
            meta.cwd.as_deref().map(project_name),
            meta.cwd.as_deref().map(project_hash),
            meta.git_branch,
        ],
    )?;
    Ok(())
}

pub(crate) fn purge_session_events(
    conn: &Connection,
    tool: Tool,
    account: &str,
    tool_session_id: &str,
) -> Result<()> {
    conn.prepare_cached(
        "DELETE FROM usage_event WHERE session_id IN
           (SELECT id FROM usage_session WHERE tool = ?1 AND account = ?2 AND tool_session_id = ?3)",
    )?
    .execute(params![tool.as_str(), account, tool_session_id])?;
    Ok(())
}

pub(crate) fn upsert_event(
    conn: &Connection,
    tool: Tool,
    ev: &UsageEvent,
    cost: Option<f64>,
    floor_ms: i64,
    overwrite: bool,
) -> Result<Upsert> {
    let ts = ms(ev.ts);
    if ts < floor_ms {
        return Ok(Upsert::Unchanged);
    }
    let key = format!("{}|{}", tool.as_str(), ev.event_key);
    let basis = if cost.is_some() {
        CostBasis::Computed
    } else {
        CostBasis::Unpriced
    };
    let existing: Option<Stored> = conn
        .prepare_cached(
            "SELECT e.id, e.ts, e.session_id, s.tool_session_id, e.model, e.input, e.output,
                    e.cache_read, e.cache_write_5m, e.cache_write_1h, e.reasoning, e.cost_usd,
                    e.speed, e.service_tier, e.agent_id
             FROM usage_event e JOIN usage_session s ON s.id = e.session_id
             WHERE e.event_key = ?1",
        )?
        .query_row([&key], |r| {
            Ok(Stored {
                id: r.get(0)?,
                ts: r.get(1)?,
                session_id: r.get(2)?,
                sid: r.get(3)?,
                model: r.get(4)?,
                tokens: Tokens {
                    input: r.get::<_, i64>(5)? as u64,
                    output: r.get::<_, i64>(6)? as u64,
                    cache_read: r.get::<_, i64>(7)? as u64,
                    cache_write_5m: r.get::<_, i64>(8)? as u64,
                    cache_write_1h: r.get::<_, i64>(9)? as u64,
                    reasoning: r.get::<_, i64>(10)? as u64,
                },
                cost: r.get(11)?,
                speed: r.get(12)?,
                tier: r.get(13)?,
                agent_id: r.get(14)?,
            })
        })
        .optional()?;
    let cwd = ev.cwd.as_deref();
    let Some(old) = existing else {
        let session_id = ensure_session(
            conn,
            tool,
            &ev.account,
            &ev.tool_session_id,
            ev.cwd.as_deref(),
            ev.git_branch.as_deref(),
        )?;
        conn.prepare_cached(
            "INSERT INTO usage_event
               (session_id, event_key, tool, account, ts, model, input, output, cache_read,
                cache_write_5m, cache_write_1h, reasoning, speed, service_tier, cost_usd, cost_basis,
                git_branch, cwd, project_name, project_hash, is_sidechain, agent_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
        )?
        .execute(params![
            session_id,
            key,
            tool.as_str(),
            ev.account,
            ts,
            ev.model,
            ev.tokens.input as i64,
            ev.tokens.output as i64,
            ev.tokens.cache_read as i64,
            ev.tokens.cache_write_5m as i64,
            ev.tokens.cache_write_1h as i64,
            ev.tokens.reasoning as i64,
            ev.speed,
            ev.service_tier,
            cost,
            basis.as_str(),
            ev.git_branch,
            cwd,
            cwd.map(project_name),
            cwd.map(project_hash),
            ev.is_sidechain,
            ev.agent_id,
        ])?;
        conn.prepare_cached(
            "UPDATE usage_session SET
               started_at = CASE WHEN started_at IS NULL OR ?2 < started_at THEN ?2 ELSE started_at END,
               ended_at = CASE WHEN ended_at IS NULL OR ?2 > ended_at THEN ?2 ELSE ended_at END
             WHERE id = ?1",
        )?
        .execute(params![session_id, ts])?;
        return Ok(Upsert::New);
    };

    let (id, old_ts, old_session) = (old.id, old.ts, old.session_id);
    let differs = old.model != ev.model
        || old.tokens != ev.tokens
        || old.cost != cost
        || old.speed != ev.speed
        || old.tier != ev.service_tier;
    let better = if overwrite {
        differs
    } else {
        ev.tokens.output > old.tokens.output
    };
    let earlier = ts < old_ts || (ts == old_ts && ev.tool_session_id < old.sid);
    if !better && !earlier {
        if old.agent_id.is_none() && ev.agent_id.is_some() && ev.tool_session_id == old.sid {
            conn.prepare_cached("UPDATE usage_event SET agent_id = ?2 WHERE id = ?1")?
                .execute(params![id, ev.agent_id])?;
            return Ok(Upsert::Updated);
        }
        return Ok(Upsert::Unchanged);
    }
    if better {
        conn.execute(
            "UPDATE usage_event SET model = ?2, input = ?3, output = ?4, cache_read = ?5,
               cache_write_5m = ?6, cache_write_1h = ?7, reasoning = ?8, speed = ?9, service_tier = ?10,
               cost_usd = ?11, cost_basis = ?12 WHERE id = ?1",
            params![
                id,
                ev.model,
                ev.tokens.input as i64,
                ev.tokens.output as i64,
                ev.tokens.cache_read as i64,
                ev.tokens.cache_write_5m as i64,
                ev.tokens.cache_write_1h as i64,
                ev.tokens.reasoning as i64,
                ev.speed,
                ev.service_tier,
                cost,
                basis.as_str(),
            ],
        )?;
    }
    if earlier {
        let session_id = ensure_session(
            conn,
            tool,
            &ev.account,
            &ev.tool_session_id,
            ev.cwd.as_deref(),
            ev.git_branch.as_deref(),
        )?;
        conn.execute(
            "UPDATE usage_event SET ts = ?2, session_id = ?3, account = ?4, git_branch = ?5, cwd = ?6,
               project_name = ?7, project_hash = ?8, is_sidechain = ?9, agent_id = ?10 WHERE id = ?1",
            params![
                id,
                ts,
                session_id,
                ev.account,
                ev.git_branch,
                cwd,
                cwd.map(project_name),
                cwd.map(project_hash),
                ev.is_sidechain,
                ev.agent_id,
            ],
        )?;
        refresh_session_range(conn, session_id)?;
        if old_session != session_id {
            refresh_session_range(conn, old_session)?;
        }
    }
    Ok(Upsert::Updated)
}

impl Store {
    pub fn link_events(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<LinkEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.tool, e.account, s.tool_session_id, e.ts, e.model, e.git_branch,
                    e.project_hash, e.cost_usd,
                    e.input + e.output + e.cache_read + e.cache_write_5m + e.cache_write_1h,
                    s.parent_session_id, e.agent_id
             FROM usage_event e JOIN usage_session s ON s.id = e.session_id
             WHERE e.ts >= ?1 AND e.ts < ?2 ORDER BY e.ts, e.id",
        )?;
        let rows = stmt.query_map([ms(from), ms(to)], |r| {
            Ok((
                LinkEvent {
                    id: r.get(0)?,
                    tool: Tool::Claude,
                    account: r.get(2)?,
                    tool_session_id: r.get(3)?,
                    parent_session_id: r.get(10)?,
                    ts: from_ms(r.get(4)?),
                    model: r.get(5)?,
                    git_branch: r.get(6)?,
                    project_hash: r.get(7)?,
                    cost_usd: r.get(8)?,
                    tokens: r.get::<_, i64>(9)? as u64,
                    agent_id: r.get(11)?,
                },
                r.get::<_, String>(1)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (mut ev, tool) = row?;
            ev.tool = Tool::parse(&tool)?;
            out.push(ev);
        }
        Ok(out)
    }
}
