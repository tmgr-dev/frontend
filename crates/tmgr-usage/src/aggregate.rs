use chrono::{DateTime, FixedOffset, Local, NaiveDate, TimeZone, Utc};
use rusqlite::types::Value;
use std::collections::BTreeMap;

use crate::error::Result;
use crate::model::{Tokens, Tool};
use crate::store::{from_ms, ms, Store};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GroupBy {
    Day,
    Tool,
    Model,
    Account,
    Project,
    Branch,
    Session,
}

#[derive(Debug, Clone, Copy)]
pub enum Tz {
    Fixed(FixedOffset),
    Local,
}

impl Tz {
    pub fn offset_min(&self, at: DateTime<Utc>) -> i32 {
        let secs = match self {
            Tz::Fixed(off) => off.local_minus_utc(),
            Tz::Local => Local
                .offset_from_utc_datetime(&at.naive_utc())
                .local_minus_utc(),
        };
        secs / 60
    }

    pub fn day(&self, ts: DateTime<Utc>) -> NaiveDate {
        match self {
            Tz::Fixed(off) => ts.with_timezone(off).date_naive(),
            Tz::Local => Local.from_utc_datetime(&ts.naive_utc()).date_naive(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Filters {
    pub tools: Vec<Tool>,
    pub accounts: Vec<String>,
    pub models: Vec<String>,
    pub project_hashes: Vec<String>,
    pub branches: Vec<String>,
    pub sessions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Query {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub tz: Tz,
    pub group_by: Vec<GroupBy>,
    pub filters: Filters,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Row {
    pub keys: Vec<String>,
    pub labels: Vec<String>,
    pub tokens: Tokens,
    pub cost_usd: f64,
    pub events: u64,
    pub unpriced_events: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub rows: Vec<Row>,
    pub totals: Row,
}

struct Fact {
    day: String,
    tool: String,
    account: String,
    model: String,
    project_hash: String,
    project_name: String,
    branch: String,
    session: String,
    tokens: Tokens,
    cost: f64,
    events: u64,
    unpriced: u64,
}

fn in_clause(col: &str, vals: &[String], sql: &mut String, params: &mut Vec<Value>) {
    if vals.is_empty() {
        return;
    }
    let marks = vec!["?"; vals.len()].join(",");
    sql.push_str(&format!(" AND {col} IN ({marks})"));
    params.extend(vals.iter().cloned().map(Value::Text));
}

impl Store {
    pub fn aggregate(&self, q: &Query) -> Result<Report> {
        let floor = self.rollup_floor_ms()?;
        let mut facts = Vec::new();
        let ev_from = ms(q.from).max(floor);

        let mut sql = String::from(
            "SELECT e.ts, e.tool, e.account, e.model, COALESCE(e.project_hash, ''),
                    COALESCE(e.project_name, ''), COALESCE(e.git_branch, ''), s.tool_session_id,
                    e.input, e.output, e.cache_read, e.cache_write_5m, e.cache_write_1h, e.reasoning,
                    e.cost_usd, e.cost_basis
             FROM usage_event e JOIN usage_session s ON s.id = e.session_id
             WHERE e.ts >= ? AND e.ts < ?",
        );
        let mut params = vec![Value::Integer(ev_from), Value::Integer(ms(q.to))];
        let f = &q.filters;
        let tools: Vec<String> = f.tools.iter().map(|t| t.as_str().to_string()).collect();
        in_clause("e.tool", &tools, &mut sql, &mut params);
        in_clause("e.account", &f.accounts, &mut sql, &mut params);
        in_clause("e.model", &f.models, &mut sql, &mut params);
        in_clause("e.project_hash", &f.project_hashes, &mut sql, &mut params);
        in_clause("e.git_branch", &f.branches, &mut sql, &mut params);
        in_clause("s.tool_session_id", &f.sessions, &mut sql, &mut params);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params))?;
        while let Some(r) = rows.next()? {
            let ts = from_ms(r.get(0)?);
            let cost: Option<f64> = r.get(14)?;
            let basis: String = r.get(15)?;
            facts.push(Fact {
                day: q.tz.day(ts).format("%Y-%m-%d").to_string(),
                tool: r.get(1)?,
                account: r.get(2)?,
                model: r.get(3)?,
                project_hash: r.get(4)?,
                project_name: r.get(5)?,
                branch: r.get(6)?,
                session: r.get(7)?,
                tokens: Tokens {
                    input: r.get::<_, i64>(8)? as u64,
                    output: r.get::<_, i64>(9)? as u64,
                    cache_read: r.get::<_, i64>(10)? as u64,
                    cache_write_5m: r.get::<_, i64>(11)? as u64,
                    cache_write_1h: r.get::<_, i64>(12)? as u64,
                    reasoning: r.get::<_, i64>(13)? as u64,
                },
                cost: cost.unwrap_or(0.0),
                events: 1,
                unpriced: u64::from(basis == "unpriced"),
            });
        }
        drop(rows);
        drop(stmt);

        if ms(q.from) < floor && f.branches.is_empty() && f.sessions.is_empty() {
            self.rollup_facts(q, &mut facts)?;
        }

        let mut grouped: BTreeMap<Vec<String>, Row> = BTreeMap::new();
        let mut totals = Row::default();
        for fact in facts {
            let (keys, labels) = group_keys(&q.group_by, &fact);
            let row = grouped.entry(keys.clone()).or_insert_with(|| Row {
                keys,
                labels,
                ..Row::default()
            });
            for r in [row, &mut totals] {
                r.tokens.add(&fact.tokens);
                r.cost_usd += fact.cost;
                r.events += fact.events;
                r.unpriced_events += fact.unpriced;
            }
        }
        Ok(Report {
            rows: grouped.into_values().collect(),
            totals,
        })
    }

    fn rollup_facts(&self, q: &Query, out: &mut Vec<Fact>) -> Result<()> {
        let first = q.tz.day(q.from).format("%Y-%m-%d").to_string();
        let last =
            q.tz.day(q.to - chrono::Duration::milliseconds(1))
                .format("%Y-%m-%d")
                .to_string();
        let mut sql = String::from(
            "SELECT day, tool, account, model, project_hash, project_name, input, output, cache_read,
                    cache_write_5m, cache_write_1h, reasoning, cost_usd, events, unpriced_events
             FROM usage_day_rollup WHERE day >= ? AND day <= ?",
        );
        let mut params = vec![Value::Text(first), Value::Text(last)];
        let f = &q.filters;
        let tools: Vec<String> = f.tools.iter().map(|t| t.as_str().to_string()).collect();
        in_clause("tool", &tools, &mut sql, &mut params);
        in_clause("account", &f.accounts, &mut sql, &mut params);
        in_clause("model", &f.models, &mut sql, &mut params);
        in_clause("project_hash", &f.project_hashes, &mut sql, &mut params);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params))?;
        while let Some(r) = rows.next()? {
            out.push(Fact {
                day: r.get(0)?,
                tool: r.get(1)?,
                account: r.get(2)?,
                model: r.get(3)?,
                project_hash: r.get(4)?,
                project_name: r.get(5)?,
                branch: String::new(),
                session: String::new(),
                tokens: Tokens {
                    input: r.get::<_, i64>(6)? as u64,
                    output: r.get::<_, i64>(7)? as u64,
                    cache_read: r.get::<_, i64>(8)? as u64,
                    cache_write_5m: r.get::<_, i64>(9)? as u64,
                    cache_write_1h: r.get::<_, i64>(10)? as u64,
                    reasoning: r.get::<_, i64>(11)? as u64,
                },
                cost: r.get(12)?,
                events: r.get::<_, i64>(13)? as u64,
                unpriced: r.get::<_, i64>(14)? as u64,
            });
        }
        Ok(())
    }
}

fn group_keys(by: &[GroupBy], f: &Fact) -> (Vec<String>, Vec<String>) {
    let mut keys = Vec::with_capacity(by.len());
    let mut labels = Vec::with_capacity(by.len());
    for g in by {
        let (k, l) = match g {
            GroupBy::Day => (f.day.clone(), f.day.clone()),
            GroupBy::Tool => (f.tool.clone(), f.tool.clone()),
            GroupBy::Model => (f.model.clone(), f.model.clone()),
            GroupBy::Account => (f.account.clone(), f.account.clone()),
            GroupBy::Project => (f.project_hash.clone(), f.project_name.clone()),
            GroupBy::Branch => (f.branch.clone(), f.branch.clone()),
            GroupBy::Session => (f.session.clone(), f.session.clone()),
        };
        keys.push(k);
        labels.push(l);
    }
    (keys, labels)
}
