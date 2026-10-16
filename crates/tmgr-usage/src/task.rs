use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::facts::{clean_repo, StoredAgent, StoredFact, MAX_NAME};
use crate::limits::{LimitSnapshot, LimitWindow};
use crate::link::LinkEvent;
use crate::model::{clean_name, FactKind, Tokens, Tool};

pub const SCHEMA: &str = "task_usage.v1";
pub const MAX_NODES: usize = 200;
pub const MAX_LIST: usize = 100;
pub const MAX_LISTS_TOTAL: usize = 500;
pub const MAX_DEPTH: usize = 32;

const CODEX_WINDOW_SLACK_SECS: i64 = 5;
const CODEX_BACKGROUND_MINUTES: i64 = 10;
const MAX_CHAIN: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskIdent {
    pub workspace: String,
    #[serde(rename = "ref")]
    pub task_ref: String,
    pub id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Period {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Totals {
    pub cost_usd: f64,
    pub events: u64,
    pub unpriced_events: u64,
    pub tokens: Tokens,
    pub active_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiting_ms: Option<u64>,
    pub turns: u64,
    pub turn_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitKind {
    FiveHour,
    Week,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LimitShare {
    pub tool: Tool,
    pub window: LimitKind,
    pub used_pct: f64,
    pub approximate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkKind {
    Exact,
    Likely,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolStat {
    pub name: String,
    pub count: u64,
    pub errors: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Node {
    pub id: String,
    pub parent_id: Option<String>,
    pub depth: usize,
    pub tool: Tool,
    pub agent_type: String,
    pub model: String,
    pub origin: Option<String>,
    pub link: Option<LinkKind>,
    pub started_at: String,
    pub ended_at: String,
    pub duration_ms: u64,
    pub cost_usd: f64,
    pub events: u64,
    pub tokens: Tokens,
    pub tools: Vec<ToolStat>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NamedCount {
    pub name: String,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct McpStat {
    pub server: String,
    pub tool: String,
    pub count: u64,
    pub errors: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HookStat {
    pub event: String,
    pub name: String,
    pub tool: Option<String>,
    pub count: u64,
    pub errors: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModeCount {
    pub mode: String,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileEdits {
    pub count: u64,
    pub extensions: Vec<NamedCount>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PrRef {
    pub repo: String,
    pub number: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AgentStat {
    pub tool: Tool,
    pub version: String,
    pub entrypoint: String,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Context {
    pub tools: Vec<ToolStat>,
    pub skills: Vec<NamedCount>,
    pub skills_available: u64,
    pub plugins: Vec<NamedCount>,
    pub mcp: Vec<McpStat>,
    pub hooks: Vec<HookStat>,
    pub slash_commands: Vec<NamedCount>,
    pub permission_modes: Vec<ModeCount>,
    pub compactions: u64,
    pub away: u64,
    pub file_edits: FileEdits,
    pub prs: Vec<PrRef>,
    pub agents: Vec<AgentStat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Truncated {
    pub nodes: bool,
    pub lists: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskUsageAggregate {
    pub schema: &'static str,
    pub source_id: String,
    pub generated_at: String,
    pub task: TaskIdent,
    pub period: Period,
    pub totals: Totals,
    pub limits: Vec<LimitShare>,
    pub nodes: Vec<Node>,
    pub context: Context,
    pub truncated: Truncated,
}

pub struct TaskInput<'a> {
    pub source_id: String,
    pub generated_at: DateTime<Utc>,
    pub task: TaskIdent,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub events: &'a [LinkEvent],
    pub tokens: &'a [Tokens],
    pub linked_events: &'a [usize],
    pub facts: &'a [StoredFact],
    pub linked_facts: &'a [usize],
    pub agents: &'a [StoredAgent],
    pub limits: &'a [LimitSnapshot],
}

pub fn clean_generic(raw: &str) -> String {
    if raw.trim().is_empty() {
        "unknown".into()
    } else {
        clean_name(raw, MAX_NAME)
    }
}

pub fn clean_model(raw: &str) -> String {
    let (base, suffix) = match raw.rsplit_once('@') {
        Some((b, s))
            if (1..=16).contains(&s.len())
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')) =>
        {
            (b, Some(s))
        }
        _ => (raw, None),
    };
    let mut out: String = base
        .chars()
        .map(|c| match c {
            '/' => ':',
            c if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '+' | '-') => c,
            _ => '_',
        })
        .take(111)
        .collect();
    while out.contains("..") {
        out = out.replace("..", "_");
    }
    while out.starts_with('.') {
        out.replace_range(..1, "_");
    }
    if out.is_empty() {
        return "unknown".into();
    }
    if let Some(s) = suffix {
        out.push('@');
        out.push_str(s);
    }
    out
}

pub fn clean_version(raw: &str) -> String {
    let mut out: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-') {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    while out.starts_with('.') {
        out.replace_range(..1, "_");
    }
    if out.is_empty() {
        "unknown".into()
    } else {
        out
    }
}

pub fn clean_extension(raw: &str) -> String {
    let ext = raw.trim_start_matches('.');
    if ext.is_empty() {
        "none".into()
    } else if ext.len() <= 16 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        ext.to_string()
    } else {
        "other".into()
    }
}

type Branches<'a> = Vec<(DateTime<Utc>, &'a str)>;

/// Facts as link events (no cost, no tokens) so they are attributed to runs by the same rules.
/// Facts keep no branch; each takes the branch of its session's latest event at or before it.
pub fn facts_as_events(facts: &[StoredFact], events: &[LinkEvent]) -> Vec<LinkEvent> {
    let mut parents: HashMap<(Tool, &str), &str> = HashMap::new();
    let mut branches: HashMap<(Tool, &str, &str), Branches> = HashMap::new();
    for e in events {
        if let Some(p) = e.parent_session_id.as_deref() {
            parents.insert((e.tool, e.tool_session_id.as_str()), p);
        }
        if let Some(b) = e.git_branch.as_deref() {
            branches
                .entry((e.tool, e.account.as_str(), e.tool_session_id.as_str()))
                .or_default()
                .push((e.ts, b));
        }
    }
    for list in branches.values_mut() {
        list.sort_by_key(|(ts, _)| *ts);
    }
    let branch_at = |f: &StoredFact| {
        let list = branches.get(&(f.tool, f.account.as_str(), f.tool_session_id.as_str()))?;
        let i = list.partition_point(|(ts, _)| *ts <= f.ts);
        list.get(i.saturating_sub(1)).map(|(_, b)| b.to_string())
    };
    facts
        .iter()
        .enumerate()
        .map(|(i, f)| LinkEvent {
            id: i as i64,
            tool: f.tool,
            account: f.account.clone(),
            tool_session_id: f.tool_session_id.clone(),
            parent_session_id: parents
                .get(&(f.tool, f.tool_session_id.as_str()))
                .map(|p| p.to_string()),
            ts: f.ts,
            model: String::new(),
            git_branch: branch_at(f),
            project_hash: f.project_hash.clone(),
            cost_usd: None,
            tokens: 0,
            agent_id: f.agent_id.clone(),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Key {
    tool: Tool,
    account: String,
    session: String,
    agent: Option<String>,
}

struct Raw<'a> {
    key: Key,
    events: Vec<usize>,
    facts: Vec<usize>,
    meta: Option<&'a StoredAgent>,
    parent: Option<usize>,
    link: Option<LinkKind>,
    spawns: Vec<(DateTime<Utc>, usize)>,
}

struct Cand<'a> {
    node: usize,
    fact: usize,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: Option<&'a str>,
}

struct Graph<'a> {
    nodes: Vec<Raw<'a>>,
    index: HashMap<Key, usize>,
    main_of: HashMap<(Tool, String), usize>,
}

impl<'a> Graph<'a> {
    fn node(&mut self, key: Key) -> usize {
        if let Some(&i) = self.index.get(&key) {
            return i;
        }
        if key.agent.is_some() {
            self.node(Key {
                agent: None,
                ..key.clone()
            });
        } else {
            self.main_of
                .insert((key.tool, key.session.clone()), self.nodes.len());
        }
        let i = self.nodes.len();
        self.index.insert(key.clone(), i);
        self.nodes.push(Raw {
            key,
            events: Vec::new(),
            facts: Vec::new(),
            meta: None,
            parent: None,
            link: None,
            spawns: Vec::new(),
        });
        i
    }

    fn is_ancestor(&self, ancestor: usize, of: usize) -> bool {
        let mut cur = self.nodes[of].parent;
        for _ in 0..MAX_CHAIN {
            match cur {
                Some(p) if p == ancestor => return true,
                Some(p) => cur = self.nodes[p].parent,
                None => return false,
            }
        }
        false
    }

    fn inherited<T>(&self, idx: usize, pick: impl Fn(&StoredAgent) -> Option<T>) -> Option<T> {
        let mut cur = Some(idx);
        for _ in 0..MAX_CHAIN {
            let i = cur?;
            if let Some(v) = self.nodes[i].meta.and_then(&pick) {
                return Some(v);
            }
            cur = self.nodes[i].parent;
        }
        None
    }
}

fn is_codex_call(f: &StoredFact) -> bool {
    f.tool == Tool::Claude
        && f.kind == FactKind::ToolCall
        && f.detail
            .as_deref()
            .is_some_and(|d| d.to_ascii_lowercase().contains("codex"))
}

fn call_window(f: &StoredFact) -> (DateTime<Utc>, DateTime<Utc>) {
    let start = f.ts - Duration::seconds(CODEX_WINDOW_SLACK_SECS);
    let end = match (f.target.as_deref(), f.ended_at) {
        (Some("background"), _) | (_, None) => f.ts + Duration::minutes(CODEX_BACKGROUND_MINUTES),
        (_, Some(e)) => e.max(f.ts) + Duration::seconds(CODEX_WINDOW_SLACK_SECS),
    };
    (start, end)
}

fn codex_from_claude(meta: &StoredAgent) -> bool {
    let originator = meta.originator.as_deref().unwrap_or("");
    originator.eq_ignore_ascii_case("Claude_Code")
        || originator.eq_ignore_ascii_case("codex_exec")
        || meta.source.as_deref() == Some("exec")
}

fn build_graph<'a>(inp: &TaskInput<'a>) -> Graph<'a> {
    let mut g = Graph {
        nodes: Vec::new(),
        index: HashMap::new(),
        main_of: HashMap::new(),
    };
    let mut codex_parent: HashMap<usize, &str> = HashMap::new();
    for (i, e) in inp.events.iter().enumerate() {
        let idx = g.node(Key {
            tool: e.tool,
            account: e.account.clone(),
            session: e.tool_session_id.clone(),
            agent: e.agent_id.clone(),
        });
        g.nodes[idx].events.push(i);
        if let (Tool::Codex, Some(p)) = (e.tool, e.parent_session_id.as_deref()) {
            codex_parent.entry(idx).or_insert(p);
        }
    }
    let mut calls: HashMap<(Tool, &str, &str, &str), usize> = HashMap::new();
    for (i, f) in inp.facts.iter().enumerate() {
        let idx = g.node(Key {
            tool: f.tool,
            account: f.account.clone(),
            session: f.tool_session_id.clone(),
            agent: f.agent_id.clone(),
        });
        g.nodes[idx].facts.push(i);
        if f.kind == FactKind::ToolCall && !f.call_id.is_empty() {
            calls.insert((f.tool, &f.account, &f.tool_session_id, &f.call_id), idx);
        }
    }
    for a in inp.agents {
        let Some(tool) = a.tool else { continue };
        let key = Key {
            tool,
            account: a.account.clone(),
            session: a.tool_session_id.clone(),
            agent: a.agent_id.clone(),
        };
        if let Some(&i) = g.index.get(&key) {
            g.nodes[i].meta = Some(a);
        }
    }

    for idx in 0..g.nodes.len() {
        let key = g.nodes[idx].key.clone();
        let parent = match (key.tool, &key.agent) {
            (Tool::Claude, Some(_)) => {
                let meta = g.nodes[idx].meta;
                meta.and_then(|m| m.spawn_tool_use_id.as_deref())
                    .and_then(|id| calls.get(&(Tool::Claude, &key.account, &key.session, id)))
                    .copied()
                    .or_else(|| {
                        let pid = meta?.parent_agent_id.clone()?;
                        g.index
                            .get(&Key {
                                agent: Some(pid),
                                ..key.clone()
                            })
                            .copied()
                    })
                    .or_else(|| g.main_of.get(&(Tool::Claude, key.session.clone())).copied())
            }
            (Tool::Codex, None) => codex_parent
                .get(&idx)
                .and_then(|p| g.main_of.get(&(Tool::Codex, p.to_string())))
                .copied(),
            _ => None,
        };
        g.nodes[idx].parent = parent.filter(|&p| p != idx);
    }

    link_codex_to_claude(&mut g, inp);

    for idx in 0..g.nodes.len() {
        let mut cur = g.nodes[idx].parent;
        for _ in 0..MAX_CHAIN {
            match cur {
                Some(p) if p == idx => {
                    g.nodes[idx].parent = None;
                    g.nodes[idx].link = None;
                    break;
                }
                Some(p) => cur = g.nodes[p].parent,
                None => break,
            }
        }
    }
    g
}

fn link_codex_to_claude(g: &mut Graph<'_>, inp: &TaskInput<'_>) {
    let cands: Vec<Cand<'_>> = inp
        .facts
        .iter()
        .enumerate()
        .filter(|(_, f)| is_codex_call(f))
        .filter_map(|(fact, f)| {
            let node = *g.index.get(&Key {
                tool: f.tool,
                account: f.account.clone(),
                session: f.tool_session_id.clone(),
                agent: f.agent_id.clone(),
            })?;
            let (start, end) = call_window(f);
            Some(Cand {
                node,
                fact,
                start,
                end,
                project: f.project_hash.as_deref(),
            })
        })
        .collect();
    if cands.is_empty() {
        return;
    }
    for idx in 0..g.nodes.len() {
        let n = &g.nodes[idx];
        if n.key.tool != Tool::Codex || n.key.agent.is_some() || n.parent.is_some() {
            continue;
        }
        let Some(meta) = n.meta.filter(|m| codex_from_claude(m)) else {
            continue;
        };
        let started = meta
            .started_at
            .or_else(|| n.events.iter().map(|&i| inp.events[i].ts).min())
            .or_else(|| n.facts.iter().map(|&i| inp.facts[i].ts).min());
        let Some(started) = started else { continue };
        let project = meta.project_hash.as_deref().or_else(|| {
            n.events
                .iter()
                .find_map(|&i| inp.events[i].project_hash.as_deref())
        });
        let mut hits: Vec<&Cand<'_>> = cands
            .iter()
            .filter(|c| c.start <= started && started <= c.end)
            .collect();
        if hits.is_empty() {
            continue;
        }
        hits.sort_by(|a, b| {
            (a.end - a.start)
                .cmp(&(b.end - b.start))
                .then(b.start.cmp(&a.start))
                .then(a.node.cmp(&b.node))
        });
        let chain = hits.windows(2).all(|w| {
            let (a, b) = (w[0], w[1]);
            b.start <= a.start
                && a.end <= b.end
                && (a.node == b.node || g.is_ancestor(b.node, a.node))
        });
        let same_project = matches!((project, hits[0].project), (Some(a), Some(b)) if a == b);
        let parent = hits[0].node;
        g.nodes[idx].parent = Some(parent);
        g.nodes[idx].spawns = cands
            .iter()
            .filter(|c| c.node == parent)
            .map(|c| (c.start, c.fact))
            .collect();
        g.nodes[idx].spawns.sort();
        g.nodes[idx].link = Some(if chain && same_project {
            LinkKind::Exact
        } else {
            LinkKind::Likely
        });
    }
}

struct Picked {
    included: Vec<bool>,
    events: Vec<Vec<usize>>,
    facts: Vec<Vec<usize>>,
}

fn pick(g: &Graph<'_>, inp: &TaskInput<'_>) -> Picked {
    let n = g.nodes.len();
    let linked_ev: HashSet<usize> = inp.linked_events.iter().copied().collect();
    let linked_fa: HashSet<usize> = inp.linked_facts.iter().copied().collect();
    let mut included = vec![false; n];
    let mut take_all = vec![false; n];
    for (i, node) in g.nodes.iter().enumerate() {
        if node.events.iter().any(|e| linked_ev.contains(e))
            || node.facts.iter().any(|f| linked_fa.contains(f))
        {
            let mut cur = Some(i);
            for _ in 0..MAX_CHAIN {
                let Some(c) = cur else { break };
                included[c] = true;
                cur = g.nodes[c].parent;
            }
        }
    }
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, node) in g.nodes.iter().enumerate() {
        if let Some(p) = node.parent {
            children[p].push(i);
        }
    }
    let mut gate: Vec<Gate> = vec![None; n];
    let selected = |i: usize, take_all: &[bool], gate: &[Gate]| -> (Vec<usize>, Vec<usize>) {
        let keep =
            |items: &[usize], linked: &HashSet<usize>, ts: &dyn Fn(usize) -> DateTime<Utc>| {
                items
                    .iter()
                    .copied()
                    .filter(|x| {
                        linked.contains(x)
                            || (take_all[i]
                                && gate[i].as_ref().is_none_or(|calls| {
                                    let t = ts(*x);
                                    calls
                                        .iter()
                                        .rev()
                                        .find(|(start, _)| *start <= t)
                                        .or(calls.first())
                                        .is_some_and(|(_, picked)| *picked)
                                }))
                    })
                    .collect::<Vec<_>>()
            };
        (
            keep(&g.nodes[i].events, &linked_ev, &|x| inp.events[x].ts),
            keep(&g.nodes[i].facts, &linked_fa, &|x| inp.facts[x].ts),
        )
    };
    let mut queue: Vec<usize> = (0..n).filter(|&i| included[i]).collect();
    while let Some(i) = queue.pop() {
        for &c in &children[i] {
            if g.nodes[c].key.tool == Tool::Codex && !take_all[c] {
                take_all[c] = true;
                included[c] = true;
                gate[c] = if g.nodes[i].key.tool == Tool::Claude {
                    let (_, fa) = selected(i, &take_all, &gate);
                    let picked: HashSet<usize> = fa.into_iter().collect();
                    Some(
                        g.nodes[c]
                            .spawns
                            .iter()
                            .map(|(start, fact)| (*start, picked.contains(fact)))
                            .collect(),
                    )
                } else if take_all[i] {
                    gate[i].clone()
                } else {
                    None
                };
                queue.push(c);
            }
        }
    }
    let mut events = Vec::with_capacity(n);
    let mut facts = Vec::with_capacity(n);
    for i in 0..n {
        let (ev, fa) = selected(i, &take_all, &gate);
        events.push(ev);
        facts.push(fa);
    }
    let mut live = vec![false; n];
    for i in 0..n {
        if events[i].is_empty() && facts[i].is_empty() {
            continue;
        }
        let mut cur = Some(i);
        for _ in 0..MAX_CHAIN {
            let Some(c) = cur else { break };
            live[c] = true;
            cur = g.nodes[c].parent;
        }
    }
    for i in 0..n {
        if g.nodes[i].key.tool == Tool::Codex && !live[i] {
            included[i] = false;
        }
    }
    Picked {
        events,
        facts,
        included,
    }
}

type Span = (DateTime<Utc>, DateTime<Utc>);
type Gate = Option<Vec<(DateTime<Utc>, bool)>>;
type HookKey = (String, String, Option<String>);

fn own_span(inp: &TaskInput<'_>, events: &[usize], facts: &[usize]) -> Option<Span> {
    let mut span: Option<Span> = None;
    let mut widen = |t: DateTime<Utc>| {
        span = Some(match span {
            Some((a, b)) => (a.min(t), b.max(t)),
            None => (t, t),
        });
    };
    for &i in events {
        widen(inp.events[i].ts);
    }
    for &i in facts {
        let f = &inp.facts[i];
        widen(f.ts);
        if let Some(e) = f.ended_at {
            widen(e);
        }
    }
    span
}

fn tree_depths(g: &Graph<'_>, included: &[bool]) -> Vec<usize> {
    (0..g.nodes.len())
        .map(|i| {
            let mut d = 0;
            let mut cur = g.nodes[i].parent;
            while let Some(p) = cur.filter(|&p| included[p]) {
                d += 1;
                cur = g.nodes[p].parent;
                if d > MAX_CHAIN {
                    break;
                }
            }
            d
        })
        .collect()
}

fn ms_between(a: DateTime<Utc>, b: DateTime<Utc>) -> u64 {
    (b - a).num_milliseconds().max(0) as u64
}

fn union_ms(mut spans: Vec<Span>) -> u64 {
    spans.sort();
    let mut total = 0;
    let mut cur: Option<Span> = None;
    for (a, b) in spans {
        cur = match cur {
            Some((ca, cb)) if a <= cb => Some((ca, cb.max(b))),
            Some((ca, cb)) => {
                total += ms_between(ca, cb);
                Some((a, b))
            }
            None => Some((a, b)),
        };
    }
    if let Some((a, b)) = cur {
        total += ms_between(a, b);
    }
    total
}

fn round6(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}

fn tools_of<'a>(facts: impl Iterator<Item = &'a StoredFact>) -> Vec<ToolStat> {
    let mut map: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
    for f in facts.filter(|f| f.kind == FactKind::ToolCall) {
        let e = map.entry(clean_generic(&f.name)).or_default();
        e.0 += 1;
        e.1 += f.is_error as u64;
        e.2 += f.duration_ms.unwrap_or(0);
    }
    let mut out: Vec<ToolStat> = map
        .into_iter()
        .map(|(name, (count, errors, duration_ms))| ToolStat {
            name,
            count,
            errors,
            duration_ms,
        })
        .collect();
    out.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
    out
}

fn counted(map: BTreeMap<String, u64>) -> Vec<NamedCount> {
    let mut out: Vec<NamedCount> = map
        .into_iter()
        .map(|(name, count)| NamedCount { name, count })
        .collect();
    out.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
    out
}

fn mcp_parts(name: &str) -> Option<(String, String)> {
    let rest = name.strip_prefix("mcp__")?;
    let (server, tool) = rest.split_once("__")?;
    (!server.is_empty() && !tool.is_empty()).then(|| (server.to_string(), tool.to_string()))
}

fn plugin_prefix(name: &str) -> Option<String> {
    let (prefix, rest) = name.split_once(':')?;
    (!prefix.is_empty() && !rest.is_empty()).then(|| clean_generic(prefix))
}

fn context_of(facts: &[&StoredFact], agents: BTreeMap<(Tool, String, String), u64>) -> Context {
    let mut skills: BTreeMap<String, u64> = BTreeMap::new();
    let mut plugins: BTreeMap<String, u64> = BTreeMap::new();
    let mut slash: BTreeMap<String, u64> = BTreeMap::new();
    let mut modes: BTreeMap<String, u64> = BTreeMap::new();
    let mut exts: BTreeMap<String, u64> = BTreeMap::new();
    let mut mcp: BTreeMap<(String, String), (u64, u64)> = BTreeMap::new();
    let mut hooks: BTreeMap<HookKey, (u64, u64, u64)> = BTreeMap::new();
    let mut prs: BTreeMap<(String, u64), ()> = BTreeMap::new();
    let (mut available, mut compactions, mut away, mut edits) = (0u64, 0u64, 0u64, 0u64);
    for f in facts {
        match f.kind {
            FactKind::ToolCall => {
                if let Some((server, tool)) = mcp_parts(&f.name) {
                    let e = mcp
                        .entry((clean_generic(&server), clean_generic(&tool)))
                        .or_default();
                    e.0 += 1;
                    e.1 += f.is_error as u64;
                }
                if matches!(f.name.as_str(), "Agent" | "Task") {
                    if let Some(p) = f.detail.as_deref().and_then(plugin_prefix) {
                        *plugins.entry(p).or_default() += 1;
                    }
                }
            }
            FactKind::Skill => {
                *skills.entry(clean_generic(&f.name)).or_default() += 1;
                if let Some(p) = plugin_prefix(&f.name) {
                    *plugins.entry(p).or_default() += 1;
                }
            }
            FactKind::SkillListing => available = available.max(f.value.unwrap_or(0)),
            FactKind::Hook => {
                let key = (
                    clean_generic(f.detail.as_deref().unwrap_or("unknown")),
                    clean_generic(&f.name),
                    f.target.as_deref().map(clean_generic),
                );
                let e = hooks.entry(key).or_default();
                e.0 += 1;
                e.1 += f.is_error as u64;
                e.2 += f.duration_ms.unwrap_or(0);
            }
            FactKind::Away => away += 1,
            FactKind::Compact => compactions += 1,
            FactKind::PermissionMode => {
                *modes.entry(clean_generic(&f.name)).or_default() += 1;
            }
            FactKind::Pr => {
                let repo = clean_repo(&f.name);
                if let (true, Some(n)) = (repo != "_", f.value) {
                    prs.insert((repo, n), ());
                }
            }
            FactKind::FileEdit => {
                edits += 1;
                *exts.entry(clean_extension(&f.name)).or_default() += 1;
            }
            FactKind::Slash => *slash.entry(clean_generic(&f.name)).or_default() += 1,
            FactKind::Turn => {}
        }
    }
    let mut mcp: Vec<McpStat> = mcp
        .into_iter()
        .map(|((server, tool), (count, errors))| McpStat {
            server,
            tool,
            count,
            errors,
        })
        .collect();
    mcp.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then(a.server.cmp(&b.server))
            .then(a.tool.cmp(&b.tool))
    });
    let mut hooks: Vec<HookStat> = hooks
        .into_iter()
        .map(
            |((event, name, tool), (count, errors, duration_ms))| HookStat {
                event,
                name,
                tool,
                count,
                errors,
                duration_ms,
            },
        )
        .collect();
    hooks.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then(a.name.cmp(&b.name))
            .then(a.event.cmp(&b.event))
    });
    let mut permission_modes: Vec<ModeCount> = modes
        .into_iter()
        .map(|(mode, count)| ModeCount { mode, count })
        .collect();
    permission_modes.sort_by(|a, b| b.count.cmp(&a.count).then(a.mode.cmp(&b.mode)));
    let mut agents: Vec<AgentStat> = agents
        .into_iter()
        .map(|((tool, version, entrypoint), count)| AgentStat {
            tool,
            version,
            entrypoint,
            count,
        })
        .collect();
    agents.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then(a.tool.cmp(&b.tool))
            .then(a.version.cmp(&b.version))
    });
    Context {
        tools: tools_of(facts.iter().copied()),
        skills: counted(skills),
        skills_available: available,
        plugins: counted(plugins),
        mcp,
        hooks,
        slash_commands: counted(slash),
        permission_modes,
        compactions,
        away,
        file_edits: FileEdits {
            count: edits,
            extensions: counted(exts),
        },
        prs: prs
            .into_keys()
            .map(|(repo, number)| PrRef { repo, number })
            .collect(),
        agents,
    }
}

fn limit_shares(inp: &TaskInput<'_>, selected: &[usize]) -> Vec<LimitShare> {
    let mut sums: BTreeMap<(Tool, LimitKind), f64> = BTreeMap::new();
    for snap in inp.limits {
        let (kind, len) = match snap.window {
            LimitWindow::Session => (LimitKind::FiveHour, Duration::hours(5)),
            LimitWindow::Weekly => (LimitKind::Week, Duration::days(7)),
            LimitWindow::Model(_) => continue,
        };
        let Some(reset) = snap.resets_at else {
            continue;
        };
        let start = reset - len;
        let in_window = |e: &&LinkEvent| {
            e.tool == snap.tool && e.account == snap.account && e.ts >= start && e.ts < reset
        };
        let cost = |e: &LinkEvent| e.cost_usd.unwrap_or(0.0);
        let total: f64 = inp.events.iter().filter(in_window).map(cost).sum();
        let mine: f64 = selected
            .iter()
            .map(|&i| &inp.events[i])
            .filter(in_window)
            .map(cost)
            .sum();
        if total > 0.0 && mine > 0.0 {
            *sums.entry((snap.tool, kind)).or_default() += snap.used_pct * (mine / total).min(1.0);
        }
    }
    sums.into_iter()
        .map(|((tool, window), pct)| LimitShare {
            tool,
            window,
            used_pct: (pct * 10.0).round() / 10.0,
            approximate: true,
        })
        .filter(|l| l.used_pct > 0.0)
        .collect()
}

trait Capped {
    fn size(&self) -> usize;
    fn cut(&mut self, to: usize);
}

impl<T> Capped for Vec<T> {
    fn size(&self) -> usize {
        self.len()
    }

    fn cut(&mut self, to: usize) {
        self.truncate(to);
    }
}

fn cap_lists(lists: &mut [&mut dyn Capped]) -> bool {
    let mut cut = false;
    for l in lists.iter_mut() {
        if l.size() > MAX_LIST {
            l.cut(MAX_LIST);
            cut = true;
        }
    }
    while lists.iter().map(|l| l.size()).sum::<usize>() > MAX_LISTS_TOTAL {
        let longest = lists
            .iter()
            .enumerate()
            .max_by_key(|(_, l)| l.size())
            .map(|(i, _)| i)
            .unwrap_or(0);
        let size = lists[longest].size();
        lists[longest].cut(size - 1);
        cut = true;
    }
    cut
}

fn dominant_model(models: &BTreeMap<String, u64>) -> String {
    models
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
        .map(|(m, _)| clean_model(m))
        .unwrap_or_else(|| "unknown".into())
}

struct Built {
    span: Span,
    tokens: Tokens,
    cost: f64,
    events: u64,
    model: String,
    tools: Vec<ToolStat>,
}

pub fn build(inp: &TaskInput<'_>) -> Option<TaskUsageAggregate> {
    if inp.linked_events.is_empty() && inp.linked_facts.is_empty() {
        return None;
    }
    let g = build_graph(inp);
    let picked = pick(&g, inp);
    let depths = tree_depths(&g, &picked.included);
    let mut order: Vec<usize> = (0..g.nodes.len()).filter(|&i| picked.included[i]).collect();
    order.sort_by(|&a, &b| depths[b].cmp(&depths[a]));

    let mut spans: Vec<Option<Span>> = vec![None; g.nodes.len()];
    for &i in &order {
        spans[i] = own_span(inp, &picked.events[i], &picked.facts[i])
            .or_else(|| g.nodes[i].meta.and_then(|m| m.started_at).map(|t| (t, t)))
            .or_else(|| {
                order
                    .iter()
                    .filter(|&&c| g.nodes[c].parent == Some(i))
                    .filter_map(|&c| spans[c])
                    .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
            })
            .or(Some((inp.from, inp.from)));
    }

    let mut built: HashMap<usize, Built> = HashMap::new();
    let mut totals_tokens = Tokens::default();
    let (mut total_cost, mut total_events, mut unpriced) = (0.0f64, 0u64, 0u64);
    let mut all_events: Vec<usize> = Vec::new();
    for &i in &order {
        let mut tokens = Tokens::default();
        let mut cost = 0.0;
        let mut models: BTreeMap<String, u64> = BTreeMap::new();
        for &e in &picked.events[i] {
            let ev = &inp.events[e];
            tokens.add(&inp.tokens[e]);
            match ev.cost_usd {
                Some(c) => cost += c,
                None => unpriced += 1,
            }
            *models.entry(ev.model.clone()).or_default() +=
                inp.tokens[e].prompt_total() + inp.tokens[e].output + 1;
        }
        totals_tokens.add(&tokens);
        total_cost += cost;
        total_events += picked.events[i].len() as u64;
        all_events.extend(&picked.events[i]);
        built.insert(
            i,
            Built {
                span: spans[i].unwrap_or((inp.from, inp.from)),
                tokens,
                cost,
                events: picked.events[i].len() as u64,
                model: dominant_model(&models),
                tools: tools_of(picked.facts[i].iter().map(|&f| &inp.facts[f])),
            },
        );
    }

    let mut kids: HashMap<Option<usize>, Vec<usize>> = HashMap::new();
    for &i in &order {
        let parent = g.nodes[i].parent.filter(|&p| picked.included[p]);
        kids.entry(parent).or_default().push(i);
    }
    for list in kids.values_mut() {
        list.sort_by(|&a, &b| {
            (built[&a].span.0, &g.nodes[a].key).cmp(&(built[&b].span.0, &g.nodes[b].key))
        });
    }
    let mut dfs: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = kids.get(&None).cloned().unwrap_or_default();
    stack.reverse();
    while let Some(i) = stack.pop() {
        dfs.push(i);
        if let Some(c) = kids.get(&Some(i)) {
            stack.extend(c.iter().rev());
        }
    }

    let mut truncated_nodes = dfs.iter().any(|&i| depths[i] > MAX_DEPTH);
    dfs.retain(|&i| depths[i] <= MAX_DEPTH);
    if dfs.len() > MAX_NODES {
        let mut drop_order: Vec<usize> = dfs.clone();
        drop_order.sort_by(|&a, &b| {
            depths[b]
                .cmp(&depths[a])
                .then(built[&a].cost.total_cmp(&built[&b].cost))
                .then(b.cmp(&a))
        });
        let dropped: HashSet<usize> = drop_order[..dfs.len() - MAX_NODES]
            .iter()
            .copied()
            .collect();
        dfs.retain(|i| !dropped.contains(i));
        truncated_nodes = true;
    }

    let ids: HashMap<usize, String> = dfs
        .iter()
        .enumerate()
        .map(|(n, &i)| (i, format!("n{n}")))
        .collect();
    let mut nodes: Vec<Node> = dfs
        .iter()
        .map(|&i| {
            let b = built.remove(&i).expect("built node");
            let raw = &g.nodes[i];
            let (agent_type, origin) = match raw.key.tool {
                Tool::Claude => (
                    match (
                        &raw.key.agent,
                        raw.meta.and_then(|m| m.agent_type.as_deref()),
                    ) {
                        (None, _) => "main".to_string(),
                        (Some(_), Some(t)) => clean_generic(t),
                        (Some(_), None) => "agent".to_string(),
                    },
                    g.inherited(i, |m| m.entrypoint.clone()),
                ),
                Tool::Codex => ("codex".to_string(), raw.meta.and_then(|m| m.source.clone())),
            };
            Node {
                id: ids[&i].clone(),
                parent_id: raw.parent.and_then(|p| ids.get(&p)).cloned(),
                depth: depths[i],
                tool: raw.key.tool,
                agent_type,
                model: b.model,
                origin: origin.map(|o| clean_generic(&o)),
                link: raw.link,
                started_at: secs(b.span.0),
                ended_at: secs(b.span.1),
                duration_ms: ms_between(b.span.0, b.span.1),
                cost_usd: round6(b.cost),
                events: b.events,
                tokens: b.tokens,
                tools: b.tools,
            }
        })
        .collect();

    let mut agent_counts: BTreeMap<(Tool, String, String), u64> = BTreeMap::new();
    for &i in &order {
        let raw = &g.nodes[i];
        let version = g
            .inherited(i, |m| m.version.clone())
            .map_or_else(|| "unknown".into(), |v| clean_version(&v));
        let entry = match raw.key.tool {
            Tool::Claude => g.inherited(i, |m| m.entrypoint.clone()),
            Tool::Codex => raw.meta.and_then(|m| m.source.clone()),
        }
        .map_or_else(|| "unknown".into(), |e| clean_generic(&e));
        *agent_counts
            .entry((raw.key.tool, version, entry))
            .or_default() += 1;
    }
    let facts: Vec<&StoredFact> = order
        .iter()
        .flat_map(|&i| picked.facts[i].iter().map(|&f| &inp.facts[f]))
        .collect();
    let mut context = context_of(&facts, agent_counts);

    let turns = facts.iter().filter(|f| f.kind == FactKind::Turn).count() as u64;
    let turn_ms = facts
        .iter()
        .filter(|f| f.kind == FactKind::Turn)
        .map(|f| f.duration_ms.unwrap_or(0))
        .sum();
    let all_spans: Vec<Span> = order.iter().filter_map(|&i| spans[i]).collect();
    let (period_from, period_to) = all_spans
        .iter()
        .fold(None, |acc: Option<Span>, s| {
            Some(acc.map_or(*s, |a| (a.0.min(s.0), a.1.max(s.1))))
        })
        .unwrap_or((inp.from, inp.to));
    let limits = limit_shares(inp, &all_events);

    let mut lists: Vec<&mut dyn Capped> = vec![
        &mut context.tools,
        &mut context.skills,
        &mut context.plugins,
        &mut context.mcp,
        &mut context.hooks,
        &mut context.slash_commands,
        &mut context.permission_modes,
        &mut context.file_edits.extensions,
        &mut context.prs,
        &mut context.agents,
    ];
    for n in &mut nodes {
        lists.push(&mut n.tools);
    }
    let truncated_lists = cap_lists(&mut lists);

    Some(TaskUsageAggregate {
        schema: SCHEMA,
        source_id: inp.source_id.clone(),
        generated_at: secs(inp.generated_at),
        task: TaskIdent {
            workspace: clean_generic(&inp.task.workspace),
            task_ref: clean_generic(&inp.task.task_ref),
            id: inp.task.id,
        },
        period: Period {
            from: secs(period_from),
            to: secs(period_to),
        },
        totals: Totals {
            cost_usd: round6(total_cost),
            events: total_events,
            unpriced_events: unpriced,
            tokens: totals_tokens,
            active_ms: union_ms(all_spans),
            waiting_ms: None,
            turns,
            turn_ms,
        },
        limits,
        nodes,
        context,
        truncated: Truncated {
            nodes: truncated_nodes,
            lists: truncated_lists,
        },
    })
}

fn secs(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(sec: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 16, 8, 0, 0).unwrap() + Duration::seconds(sec)
    }

    #[derive(Default)]
    struct Fx {
        events: Vec<LinkEvent>,
        tokens: Vec<Tokens>,
        facts: Vec<StoredFact>,
        agents: Vec<StoredAgent>,
        limits: Vec<LimitSnapshot>,
        linked_events: Vec<usize>,
        linked_facts: Vec<usize>,
    }

    impl Fx {
        fn event(&mut self, tool: Tool, session: &str, agent: Option<&str>, at: i64, cost: f64) {
            self.events.push(LinkEvent {
                id: self.events.len() as i64,
                tool,
                account: tool.as_str().into(),
                tool_session_id: session.into(),
                parent_session_id: None,
                ts: t(at),
                model: "claude-opus-5-5".into(),
                git_branch: None,
                project_hash: Some("p1".into()),
                cost_usd: Some(cost),
                tokens: 10,
                agent_id: agent.map(String::from),
            });
            self.tokens.push(Tokens {
                input: 10,
                output: 5,
                cache_read: 100,
                ..Tokens::default()
            });
        }

        fn child_thread(&mut self, session: &str, parent: &str, at: i64) {
            self.event(Tool::Codex, session, None, at, 0.5);
            self.events.last_mut().unwrap().parent_session_id = Some(parent.into());
        }

        fn fact(
            &mut self,
            tool: Tool,
            session: &str,
            agent: Option<&str>,
            kind: FactKind,
            name: &str,
            at: i64,
        ) -> &mut StoredFact {
            self.facts.push(StoredFact {
                tool,
                account: tool.as_str().into(),
                tool_session_id: session.into(),
                agent_id: agent.map(String::from),
                ts: t(at),
                kind,
                name: name.into(),
                detail: None,
                target: None,
                ended_at: None,
                duration_ms: None,
                value: None,
                is_error: false,
                project_hash: Some("p1".into()),
                call_id: String::new(),
            });
            self.facts.last_mut().unwrap()
        }

        #[allow(clippy::too_many_arguments)]
        fn call(
            &mut self,
            session: &str,
            agent: Option<&str>,
            name: &str,
            detail: Option<&str>,
            id: &str,
            from: i64,
            to: Option<i64>,
        ) {
            let f = self.fact(Tool::Claude, session, agent, FactKind::ToolCall, name, from);
            f.detail = detail.map(String::from);
            f.call_id = id.into();
            f.ended_at = to.map(t);
        }

        fn claude_agent(&mut self, session: &str, agent: Option<&str>) -> &mut StoredAgent {
            self.agents.push(StoredAgent {
                tool: Some(Tool::Claude),
                account: "claude".into(),
                tool_session_id: session.into(),
                agent_id: agent.map(String::from),
                version: Some("2.1.90".into()),
                entrypoint: Some("cli".into()),
                ..StoredAgent::default()
            });
            self.agents.last_mut().unwrap()
        }

        fn codex_agent(&mut self, session: &str, originator: &str, source: &str, at: i64) {
            self.agents.push(StoredAgent {
                tool: Some(Tool::Codex),
                account: "codex".into(),
                tool_session_id: session.into(),
                originator: Some(originator.into()),
                source: Some(source.into()),
                version: Some("0.150.0".into()),
                started_at: Some(t(at)),
                project_hash: Some("p1".into()),
                ..StoredAgent::default()
            });
        }

        fn link_all(&mut self) {
            self.linked_events = (0..self.events.len()).collect();
            self.linked_facts = (0..self.facts.len()).collect();
        }

        fn build(&self) -> Option<TaskUsageAggregate> {
            build(&TaskInput {
                source_id: "src_1".into(),
                generated_at: t(100_000),
                task: TaskIdent {
                    workspace: "tmgrdev".into(),
                    task_ref: "TM-1".into(),
                    id: Some(7),
                },
                from: t(-1000),
                to: t(100_000),
                events: &self.events,
                tokens: &self.tokens,
                linked_events: &self.linked_events,
                facts: &self.facts,
                linked_facts: &self.linked_facts,
                agents: &self.agents,
                limits: &self.limits,
            })
        }
    }

    fn by_type<'a>(a: &'a TaskUsageAggregate, ty: &str) -> &'a Node {
        a.nodes.iter().find(|n| n.agent_type == ty).unwrap()
    }

    fn parent_of<'a>(a: &'a TaskUsageAggregate, n: &Node) -> &'a Node {
        let id = n.parent_id.as_deref().unwrap();
        a.nodes.iter().find(|p| p.id == id).unwrap()
    }

    #[test]
    fn nothing_linked_builds_nothing() {
        let mut fx = Fx::default();
        fx.event(Tool::Claude, "s1", None, 0, 1.0);
        assert!(fx.build().is_none());
    }

    #[test]
    fn subagents_hang_under_the_spawning_call_then_the_parent_agent_then_the_main_session() {
        let mut fx = Fx::default();
        fx.event(Tool::Claude, "s1", None, 0, 1.0);
        fx.event(Tool::Claude, "s1", Some("a1"), 10, 0.5);
        fx.event(Tool::Claude, "s1", Some("a2"), 20, 0.25);
        fx.event(Tool::Claude, "s1", Some("a3"), 30, 0.125);
        fx.claude_agent("s1", None);
        fx.call("s1", None, "Agent", Some("Explore"), "tu1", 5, Some(60));
        fx.call("s1", Some("a1"), "Agent", Some("Plan"), "tu2", 12, Some(40));
        let a = fx.claude_agent("s1", Some("a1"));
        a.agent_type = Some("Explore".into());
        a.spawn_tool_use_id = Some("tu1".into());
        let a = fx.claude_agent("s1", Some("a2"));
        a.agent_type = Some("Plan".into());
        a.spawn_tool_use_id = Some("tu2".into());
        let a = fx.claude_agent("s1", Some("a3"));
        a.agent_type = Some("Review".into());
        a.parent_agent_id = Some("a1".into());
        fx.claude_agent("s1", Some("a4")).agent_type = Some("Orphan".into());
        fx.event(Tool::Claude, "s1", Some("a4"), 40, 0.1);
        fx.link_all();
        let a = fx.build().unwrap();
        assert_eq!(a.nodes.len(), 5);
        let explore = by_type(&a, "Explore");
        assert_eq!(parent_of(&a, explore).agent_type, "main");
        let plan = by_type(&a, "Plan");
        assert_eq!(parent_of(&a, plan).agent_type, "Explore");
        assert_eq!(plan.depth, 2);
        let review = by_type(&a, "Review");
        assert_eq!(parent_of(&a, review).agent_type, "Explore");
        let orphan = by_type(&a, "Orphan");
        assert_eq!(parent_of(&a, orphan).agent_type, "main");
        for (i, n) in a.nodes.iter().enumerate() {
            assert_eq!(n.id, format!("n{i}"));
            if let Some(p) = &n.parent_id {
                let pi: usize = p[1..].parse().unwrap();
                assert!(pi < i, "{} before {}", p, n.id);
            }
        }
        assert_eq!(a.nodes[0].origin.as_deref(), Some("cli"));
        assert_eq!(explore.origin.as_deref(), Some("cli"));
        assert!((a.totals.cost_usd - 1.975).abs() < 1e-9);
        assert_eq!(a.totals.events, 5);
    }

    #[test]
    fn a_codex_child_thread_follows_its_parent_session_and_takes_all_its_usage() {
        let mut fx = Fx::default();
        fx.event(Tool::Codex, "cx1", None, 0, 2.0);
        fx.child_thread("cx2", "cx1", 100);
        fx.child_thread("cx2", "cx1", 110);
        fx.codex_agent("cx1", "codex-tui", "cli", 0);
        fx.linked_events = vec![0];
        let a = fx.build().unwrap();
        assert_eq!(a.nodes.len(), 2);
        assert_eq!(a.nodes[1].parent_id.as_deref(), Some("n0"));
        assert_eq!(a.nodes[1].link, None);
        assert_eq!(a.nodes[1].events, 2);
        assert_eq!(a.nodes[0].origin.as_deref(), Some("cli"));
        assert_eq!(a.nodes[0].agent_type, "codex");
        assert_eq!(a.totals.events, 3);
    }

    #[test]
    fn a_codex_session_spanning_two_tasks_is_split_by_the_parent_link_windows() {
        let mut fx = Fx::default();
        fx.event(Tool::Claude, "s1", None, 55, 1.0);
        fx.event(Tool::Claude, "s1", None, 205, 1.0);
        fx.claude_agent("s1", None);
        fx.call("s1", None, "Bash", Some("codex"), "tu1", 50, Some(100));
        fx.call("s1", None, "Bash", Some("codex"), "tu2", 150, Some(210));
        fx.event(Tool::Codex, "cx1", None, 60, 0.5);
        fx.event(Tool::Codex, "cx1", None, 200, 0.5);
        fx.codex_agent("cx1", "Claude_Code", "vscode", 60);
        fx.linked_events = vec![0];
        fx.linked_facts = vec![0];
        let a = fx.build().unwrap();
        assert_eq!(a.totals.events, 2);
        fx.linked_events = vec![1];
        fx.linked_facts = vec![1];
        let b = fx.build().unwrap();
        assert_eq!(b.totals.events, 2);
        assert_eq!(a.totals.events + b.totals.events, fx.events.len() as u64);
    }

    #[test]
    fn a_codex_event_between_two_tasks_calls_counts_for_one_task_only() {
        let mut fx = Fx::default();
        fx.event(Tool::Claude, "s1", None, 0, 1.0);
        fx.event(Tool::Claude, "s1", None, 1200, 1.0);
        fx.event(Tool::Claude, "s1", None, 300, 1.0);
        fx.claude_agent("s1", None);
        fx.call("s1", None, "Bash", Some("codex"), "tu1", 0, Some(60));
        fx.call("s1", None, "Bash", Some("codex"), "tu2", 300, Some(660));
        fx.call("s1", None, "Bash", Some("codex"), "tu3", 1200, Some(1260));
        fx.event(Tool::Codex, "cx1", None, 30, 0.5);
        fx.event(Tool::Codex, "cx1", None, 600, 0.5);
        fx.event(Tool::Codex, "cx1", None, 1230, 0.5);
        fx.codex_agent("cx1", "Claude_Code", "vscode", 30);
        fx.linked_events = vec![0, 1];
        fx.linked_facts = vec![0, 2];
        let a = fx.build().unwrap();
        fx.linked_events = vec![2];
        fx.linked_facts = vec![1];
        let b = fx.build().unwrap();
        assert_eq!(a.totals.events, 4);
        assert_eq!(b.totals.events, 2);
        assert_eq!(a.totals.events + b.totals.events, fx.events.len() as u64);
    }

    #[test]
    fn codex_events_after_the_spawning_call_stay_with_that_task() {
        let mut fx = Fx::default();
        fx.event(Tool::Claude, "s1", None, 0, 1.0);
        fx.claude_agent("s1", None);
        fx.call("s1", None, "Bash", Some("codex"), "tu1", 10, Some(20));
        fx.event(Tool::Codex, "cx1", None, 15, 0.5);
        fx.event(Tool::Codex, "cx1", None, 900, 0.5);
        fx.codex_agent("cx1", "Claude_Code", "vscode", 15);
        fx.linked_events = vec![0];
        fx.linked_facts = vec![0];
        let a = fx.build().unwrap();
        assert_eq!(a.totals.events, 3);
    }

    #[test]
    fn a_codex_session_outside_every_call_window_is_not_linked() {
        let mut fx = Fx::default();
        fx.event(Tool::Claude, "s1", None, 0, 1.0);
        fx.claude_agent("s1", None);
        fx.call("s1", None, "Bash", Some("codex"), "tu1", 10, Some(60));
        fx.event(Tool::Codex, "cx1", None, 5000, 0.5);
        fx.codex_agent("cx1", "Claude_Code", "vscode", 5000);
        fx.linked_events = vec![0];
        fx.linked_facts = vec![0];
        let a = fx.build().unwrap();
        assert_eq!(a.nodes.len(), 1);
        assert_eq!(a.totals.events, 1);
    }

    #[test]
    fn codex_children_with_nothing_selected_for_the_task_are_dropped() {
        let mut fx = Fx::default();
        fx.event(Tool::Claude, "s1", None, 0, 1.0);
        fx.claude_agent("s1", None);
        fx.call("s1", None, "Bash", Some("codex"), "tu1", 0, Some(60));
        fx.call("s1", None, "Bash", Some("codex"), "tu2", 3000, Some(3060));
        fx.event(Tool::Codex, "cx1", None, 3020, 0.5);
        fx.codex_agent("cx1", "Claude_Code", "vscode", 3010);
        fx.linked_events = vec![0];
        fx.linked_facts = vec![0];
        let a = fx.build().unwrap();
        assert_eq!(a.nodes.len(), 1);
        assert_eq!(a.totals.events, 1);
        fx.linked_events = vec![];
        fx.linked_facts = vec![1];
        let b = fx.build().unwrap();
        assert_eq!(b.nodes.len(), 2);
        assert_eq!(b.totals.events, 1);
    }

    fn rescue_fixture() -> Fx {
        let mut fx = Fx::default();
        fx.event(Tool::Claude, "s1", None, 0, 1.0);
        fx.event(Tool::Claude, "s1", Some("a1"), 20, 0.5);
        fx.claude_agent("s1", None);
        let a = fx.claude_agent("s1", Some("a1"));
        a.agent_type = Some("codex:codex-rescue".into());
        a.spawn_tool_use_id = Some("tu1".into());
        fx.call(
            "s1",
            None,
            "Agent",
            Some("codex:codex-rescue"),
            "tu1",
            10,
            Some(600),
        );
        fx.call(
            "s1",
            Some("a1"),
            "Bash",
            Some("codex"),
            "tu2",
            30,
            Some(500),
        );
        fx.event(Tool::Codex, "cx1", None, 60, 0.0);
        fx.event(Tool::Codex, "cx1", None, 90, 0.0);
        fx.codex_agent("cx1", "Claude_Code", "vscode", 60);
        fx.fact(Tool::Codex, "cx1", None, FactKind::ToolCall, "exec", 70);
        fx.linked_events = vec![0, 1];
        fx.linked_facts = vec![0, 1];
        fx
    }

    #[test]
    fn a_codex_session_inside_one_call_chain_with_the_same_repository_is_exact() {
        let a = rescue_fixture().build().unwrap();
        let codex = by_type(&a, "codex");
        assert_eq!(codex.link, Some(LinkKind::Exact));
        let parent = parent_of(&a, codex);
        assert_eq!(parent.agent_type, "codex:codex-rescue");
        assert_eq!(codex.events, 2);
        assert_eq!(codex.tools[0].name, "exec");
        assert_eq!(
            a.context.plugins,
            vec![NamedCount {
                name: "codex".into(),
                count: 1
            }]
        );
    }

    #[test]
    fn a_codex_session_in_another_repository_or_inside_overlapping_calls_is_likely() {
        let mut fx = rescue_fixture();
        fx.agents.last_mut().unwrap().project_hash = Some("other".into());
        fx.events[2].project_hash = Some("other".into());
        let a = fx.build().unwrap();
        assert_eq!(by_type(&a, "codex").link, Some(LinkKind::Likely));

        let mut fx = rescue_fixture();
        fx.call("s1", Some("a1"), "Bash", Some("codex"), "tu3", 40, Some(80));
        fx.call(
            "s1",
            Some("a1"),
            "Bash",
            Some("codex"),
            "tu4",
            50,
            Some(120),
        );
        fx.linked_facts = (0..fx.facts.len()).collect();
        let a = fx.build().unwrap();
        let codex = by_type(&a, "codex");
        assert_eq!(codex.link, Some(LinkKind::Likely));
        assert_eq!(parent_of(&a, codex).agent_type, "codex:codex-rescue");
    }

    #[test]
    fn background_calls_stretch_the_window_and_unrelated_codex_sessions_stay_roots() {
        let mut fx = rescue_fixture();
        fx.facts[1].ended_at = Some(t(35));
        fx.facts[1].target = Some("background".into());
        let a = fx.build().unwrap();
        assert!(by_type(&a, "codex").link.is_some());

        let mut fx = rescue_fixture();
        fx.agents.last_mut().unwrap().originator = Some("codex-tui".into());
        fx.agents.last_mut().unwrap().source = Some("cli".into());
        fx.linked_events = vec![0, 1, 2, 3];
        let a = fx.build().unwrap();
        let codex = by_type(&a, "codex");
        assert_eq!(codex.parent_id, None);
        assert_eq!(codex.link, None);

        let mut fx = rescue_fixture();
        fx.agents.last_mut().unwrap().started_at = Some(t(5000));
        fx.events[2].ts = t(5000);
        fx.events[3].ts = t(5001);
        fx.linked_events = vec![0, 1, 2, 3];
        let a = fx.build().unwrap();
        assert!(a.nodes.iter().any(|n| n.tool == Tool::Codex));
        assert!(a
            .nodes
            .iter()
            .filter(|n| n.tool == Tool::Codex)
            .all(|n| n.parent_id.is_none()));
    }

    #[test]
    fn only_linked_nodes_their_ancestors_and_codex_children_are_included() {
        let mut fx = Fx::default();
        fx.event(Tool::Claude, "s1", None, 0, 1.0);
        fx.event(Tool::Claude, "s1", Some("a1"), 10, 0.5);
        fx.event(Tool::Claude, "s1", Some("a2"), 20, 0.25);
        fx.event(Tool::Claude, "s2", None, 30, 9.0);
        fx.claude_agent("s1", Some("a1")).agent_type = Some("Kept".into());
        fx.claude_agent("s1", Some("a2")).agent_type = Some("Dropped".into());
        fx.linked_events = vec![1];
        let a = fx.build().unwrap();
        let types: Vec<&str> = a.nodes.iter().map(|n| n.agent_type.as_str()).collect();
        assert_eq!(types, vec!["main", "Kept"]);
        assert_eq!(a.nodes[0].events, 0);
        assert_eq!(a.totals.events, 1);
        assert!((a.totals.cost_usd - 0.5).abs() < 1e-9);

        let mut fx = rescue_fixture();
        fx.linked_events = vec![0];
        fx.linked_facts = vec![];
        let a = fx.build().unwrap();
        let codex = a.nodes.iter().find(|n| n.tool == Tool::Codex);
        assert!(codex.is_none(), "{:?}", a.nodes);
    }

    #[test]
    fn totals_context_and_period_come_from_included_facts() {
        let mut fx = Fx::default();
        fx.event(Tool::Claude, "s1", None, 0, 1.0);
        fx.event(Tool::Claude, "s1", None, 3600, 1.0);
        fx.claude_agent("s1", None);
        fx.call("s1", None, "mcp__tmgr__update_task", None, "c1", 5, Some(6));
        fx.call("s1", None, "Bash", None, "c2", 6, Some(8));
        fx.facts[1].is_error = true;
        fx.facts[1].duration_ms = Some(2000);
        fx.fact(
            Tool::Claude,
            "s1",
            None,
            FactKind::Skill,
            "superpowers:brainstorming",
            7,
        );
        fx.fact(Tool::Claude, "s1", None, FactKind::SkillListing, "", 7)
            .value = Some(214);
        let h = fx.fact(Tool::Claude, "s1", None, FactKind::Hook, "token-goat", 8);
        h.detail = Some("PostToolUse".into());
        h.target = Some("Read".into());
        h.duration_ms = Some(40);
        fx.fact(Tool::Claude, "s1", None, FactKind::Turn, "", 9)
            .duration_ms = Some(5000);
        fx.fact(Tool::Claude, "s1", None, FactKind::Compact, "", 9);
        fx.fact(Tool::Claude, "s1", None, FactKind::Away, "", 9);
        fx.fact(
            Tool::Claude,
            "s1",
            None,
            FactKind::PermissionMode,
            "bypassPermissions",
            9,
        );
        fx.fact(Tool::Claude, "s1", None, FactKind::Slash, "tm", 9);
        fx.fact(Tool::Claude, "s1", None, FactKind::FileEdit, "rs", 9);
        fx.fact(Tool::Claude, "s1", None, FactKind::FileEdit, "a/b", 9);
        for _ in 0..2 {
            let p = fx.fact(
                Tool::Claude,
                "s1",
                None,
                FactKind::Pr,
                "tmgr-dev/frontend",
                9,
            );
            p.value = Some(497);
        }
        fx.link_all();
        let a = fx.build().unwrap();
        assert_eq!(a.totals.tokens.cache_read, 200);
        assert_eq!(a.totals.turns, 1);
        assert_eq!(a.totals.turn_ms, 5000);
        assert_eq!(a.totals.active_ms, 3_600_000);
        assert_eq!(a.totals.waiting_ms, None);
        assert_eq!(a.period.from, "2026-10-16T08:00:00Z");
        assert_eq!(a.period.to, "2026-10-16T09:00:00Z");
        let c = &a.context;
        assert_eq!(c.skills_available, 214);
        assert_eq!(c.plugins[0].name, "superpowers");
        assert_eq!(
            c.mcp,
            vec![McpStat {
                server: "tmgr".into(),
                tool: "update_task".into(),
                count: 1,
                errors: 0
            }]
        );
        assert_eq!(c.hooks[0].tool.as_deref(), Some("Read"));
        assert_eq!(c.hooks[0].event, "PostToolUse");
        assert_eq!((c.compactions, c.away), (1, 1));
        assert_eq!(c.permission_modes[0].mode, "bypassPermissions");
        assert_eq!(c.file_edits.count, 2);
        let exts: Vec<&str> = c
            .file_edits
            .extensions
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(exts, vec!["other", "rs"]);
        assert_eq!(
            c.prs,
            vec![PrRef {
                repo: "tmgr-dev/frontend".into(),
                number: 497
            }]
        );
        let bash = c.tools.iter().find(|t| t.name == "Bash").unwrap();
        assert_eq!((bash.errors, bash.duration_ms), (1, 2000));
        assert_eq!(
            c.agents,
            vec![AgentStat {
                tool: Tool::Claude,
                version: "2.1.90".into(),
                entrypoint: "cli".into(),
                count: 1
            }]
        );
        let json = serde_json::to_value(&a).unwrap();
        assert!(json["totals"].get("waiting_ms").is_none());
        assert_eq!(json["schema"], "task_usage.v1");
        assert_eq!(json["task"]["ref"], "TM-1");
    }

    #[test]
    fn limits_are_the_tasks_share_of_the_window_and_stay_approximate() {
        let mut fx = Fx::default();
        fx.event(Tool::Claude, "s1", None, 0, 1.0);
        fx.event(Tool::Claude, "s2", None, 10, 3.0);
        fx.linked_events = vec![0];
        fx.limits.push(LimitSnapshot {
            tool: Tool::Claude,
            account: "claude".into(),
            window: LimitWindow::Weekly,
            used_pct: 40.0,
            resets_at: Some(t(86_400)),
            plan: None,
            observed_at: t(50),
            source: crate::limits::LimitSource::Statusline,
            tool_session_id: "s".into(),
        });
        let a = fx.build().unwrap();
        assert_eq!(
            a.limits,
            vec![LimitShare {
                tool: Tool::Claude,
                window: LimitKind::Week,
                used_pct: 10.0,
                approximate: true
            }]
        );
        fx.limits[0].resets_at = Some(t(-86_400 * 30));
        assert!(fx.build().unwrap().limits.is_empty());
    }

    #[test]
    fn names_never_carry_paths_emails_or_odd_characters() {
        assert_eq!(clean_generic("/etc/passwd"), "other");
        assert_eq!(clean_generic("me@host"), "other");
        assert_eq!(clean_generic("../x"), "other");
        assert_eq!(clean_generic("~/x"), "other");
        assert_eq!(clean_generic("Claude Code"), "Claude_Code");
        assert_eq!(clean_generic(""), "unknown");
        assert_eq!(clean_generic(&"a".repeat(300)), "other");
        assert_eq!(clean_model("anthropic/claude-x@1"), "anthropic:claude-x@1");
        assert_eq!(clean_model("a b\\c"), "a_b_c");
        assert_eq!(clean_model("../x"), "_:x");
        assert_eq!(
            clean_model("claude-opus-4@20250514"),
            "claude-opus-4@20250514"
        );
        assert_eq!(clean_model("alice@example.com"), "alice_example.com");
        assert_eq!(clean_model("a@b@c"), "a_b@c");
        assert_eq!(clean_version("2.1.90"), "2.1.90");
        assert_eq!(clean_version("1.0/rc@2"), "1.0_rc_2");
        assert_eq!(clean_version(&"9".repeat(80)).len(), 64);
        assert_eq!(clean_extension(".rs"), "rs");
        assert_eq!(clean_extension(""), "none");
        assert_eq!(clean_extension("a/b"), "other");
        assert_eq!(clean_extension(&"x".repeat(17)), "other");

        let mut fx = Fx::default();
        fx.event(Tool::Claude, "s1", None, 0, 1.0);
        fx.events[0].model = "vendor/model@1 /tmp/x".into();
        fx.call(
            "s1",
            None,
            "/usr/bin/evil",
            Some("me@host"),
            "c1",
            1,
            Some(2),
        );
        fx.call("s1", None, "mcp__a/b__c@d", None, "c2", 1, Some(2));
        fx.link_all();
        let a = fx.build().unwrap();
        let text = serde_json::to_string(&a).unwrap();
        assert!(!text.contains("/usr") && !text.contains("me@host") && !text.contains("/tmp"));
        assert_eq!(a.nodes[0].model, "vendor:model_1_:tmp:x");
    }

    #[test]
    fn nodes_are_capped_deepest_and_cheapest_first_and_lists_are_bounded() {
        let mut fx = Fx::default();
        fx.event(Tool::Claude, "s1", None, 0, 1.0);
        fx.claude_agent("s1", None);
        for i in 0..250 {
            let id = format!("a{i}");
            fx.event(
                Tool::Claude,
                "s1",
                Some(&id),
                10 + i,
                0.01 * (i as f64 + 1.0),
            );
            fx.claude_agent("s1", Some(&id)).agent_type = Some(format!("T{i}"));
        }
        for i in 0..120 {
            fx.call(
                "s1",
                None,
                &format!("Tool{i}"),
                None,
                &format!("c{i}"),
                5,
                Some(6),
            );
        }
        fx.link_all();
        let a = fx.build().unwrap();
        assert_eq!(a.nodes.len(), MAX_NODES);
        assert!(a.truncated.nodes && a.truncated.lists);
        assert_eq!(a.nodes[0].agent_type, "main");
        assert!(a
            .nodes
            .iter()
            .all(|n| n.agent_type == "main" || n.cost_usd >= 0.51));
        assert!(a.context.tools.len() <= MAX_LIST);
        assert_eq!(a.totals.events, 251);

        let mut fx = Fx::default();
        fx.event(Tool::Claude, "s1", None, 0, 1.0);
        for i in 0..600 {
            fx.fact(
                Tool::Claude,
                "s1",
                None,
                FactKind::Slash,
                &format!("c{i}"),
                1,
            );
            fx.fact(
                Tool::Claude,
                "s1",
                None,
                FactKind::Skill,
                &format!("p:s{i}"),
                1,
            );
            fx.fact(
                Tool::Claude,
                "s1",
                None,
                FactKind::Hook,
                &format!("h{i}"),
                1,
            );
            fx.fact(
                Tool::Claude,
                "s1",
                None,
                FactKind::Skill,
                &format!("q{i}:s"),
                1,
            );
            fx.fact(
                Tool::Claude,
                "s1",
                None,
                FactKind::Slash,
                &format!("d{i}"),
                1,
            );
        }
        fx.link_all();
        let a = fx.build().unwrap();
        let c = &a.context;
        let total = c.tools.len()
            + c.skills.len()
            + c.plugins.len()
            + c.hooks.len()
            + c.slash_commands.len()
            + c.mcp.len();
        assert!(total <= MAX_LISTS_TOTAL, "{total}");
        assert!(c.skills.len() <= MAX_LIST && c.hooks.len() <= MAX_LIST);
        assert!(a.truncated.lists);
    }

    #[test]
    fn facts_become_link_events_that_keep_session_and_parent() {
        let mut fx = Fx::default();
        fx.child_thread("cx2", "cx1", 100);
        fx.fact(Tool::Codex, "cx2", None, FactKind::ToolCall, "exec", 105);
        fx.fact(Tool::Claude, "s1", Some("a1"), FactKind::Skill, "x", 105);
        let evs = facts_as_events(&fx.facts, &fx.events);
        assert_eq!(evs[0].parent_session_id.as_deref(), Some("cx1"));
        assert_eq!(evs[0].cost_usd, None);
        assert_eq!(evs[1].agent_id.as_deref(), Some("a1"));
        assert_eq!(evs[1].tool_session_id, "s1");
    }
}
