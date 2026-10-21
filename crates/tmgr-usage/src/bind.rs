use chrono::{DateTime, Duration, Utc};
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::facts::{StoredAgent, StoredFact};
use crate::link::{attribute_full, Attribution, LinkEvent, LinkReport, OrchWindow, RunWindow};
use crate::model::{FactKind, Tool};

pub const SLACK_MINUTES: i64 = 2;
const WINDOW_GAP_MINUTES: i64 = 30;
const MAX_CHAIN: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunIds {
    pub run_id: u64,
    pub task_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeKey {
    pub tool: Tool,
    pub account: String,
    pub session: String,
    pub agent: Option<String>,
}

impl NodeKey {
    pub fn of_event(e: &LinkEvent) -> NodeKey {
        NodeKey {
            tool: e.tool,
            account: e.account.clone(),
            session: e.tool_session_id.clone(),
            agent: e.agent_id.clone(),
        }
    }

    fn of_fact(f: &StoredFact) -> NodeKey {
        NodeKey {
            tool: f.tool,
            account: f.account.clone(),
            session: f.tool_session_id.clone(),
            agent: f.agent_id.clone(),
        }
    }

    pub fn main(&self) -> NodeKey {
        NodeKey {
            agent: None,
            ..self.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentBinding {
    pub tool: Tool,
    pub account: String,
    pub tool_session_id: String,
    pub agent_id: Option<String>,
    pub run_index: usize,
    pub from: DateTime<Utc>,
    pub to: Option<DateTime<Utc>>,
    pub exact: bool,
}

impl AgentBinding {
    pub fn node(&self) -> NodeKey {
        NodeKey {
            tool: self.tool,
            account: self.account.clone(),
            session: self.tool_session_id.clone(),
            agent: self.agent_id.clone(),
        }
    }
}

struct Span {
    from: DateTime<Utc>,
    finished: Option<DateTime<Utc>>,
    exact: bool,
}

#[allow(clippy::too_many_arguments)]
fn matched_runs(
    marker: &str,
    value: u64,
    ts: DateTime<Utc>,
    runs: &[RunWindow],
    ids: &[RunIds],
    all: usize,
    now: DateTime<Utc>,
    slack: Duration,
) -> Vec<usize> {
    match marker {
        "agent_work_start" => (0..all)
            .filter(|&i| ids[i].task_id == Some(value) && (runs[i].started_at - ts).abs() <= slack)
            .collect(),
        "agent_work_update" | "agent_work_finish" => (0..all)
            .filter(|&i| {
                ids[i].run_id == value
                    && ts >= runs[i].started_at - slack
                    && ts <= runs[i].ended_at.unwrap_or(now) + slack
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// `ids` runs parallel to `runs`.
pub fn agent_bindings(
    facts: &[StoredFact],
    runs: &[RunWindow],
    ids: &[RunIds],
    now: DateTime<Utc>,
) -> Vec<AgentBinding> {
    let slack = Duration::minutes(SLACK_MINUTES);
    let all = runs.len().min(ids.len());
    let mut spans: BTreeMap<(NodeKey, usize), Span> = BTreeMap::new();
    for f in facts {
        if f.tool != Tool::Claude || f.kind != FactKind::ToolCall {
            continue;
        }
        let (Some(marker), Some(value)) = (f.target.as_deref(), f.value) else {
            continue;
        };
        let matched = matched_runs(marker, value, f.ts, runs, ids, all, now, slack);
        let exact = marker != "agent_work_start";
        for run in matched {
            let span = spans.entry((NodeKey::of_fact(f), run)).or_insert(Span {
                from: f.ts,
                finished: None,
                exact: false,
            });
            span.from = span.from.min(f.ts);
            span.exact |= exact;
            if marker == "agent_work_finish" {
                span.finished = span.finished.max(Some(f.ts));
            }
        }
    }
    spans
        .into_iter()
        .map(|((node, run_index), span)| AgentBinding {
            tool: node.tool,
            account: node.account,
            tool_session_id: node.session,
            agent_id: node.agent,
            run_index,
            from: span.from,
            to: match (span.finished, runs[run_index].ended_at) {
                (Some(f), Some(e)) => Some(f.min(e)),
                (f, e) => f.or(e),
            },
            exact: span.exact,
        })
        .collect()
}

/// Same parent rules as the task tree: the spawning call, then the parent agent, then the session's main node.
pub fn node_parents(
    events: &[LinkEvent],
    facts: &[StoredFact],
    agents: &[StoredAgent],
) -> HashMap<NodeKey, NodeKey> {
    let mut known: HashSet<NodeKey> = events.iter().map(NodeKey::of_event).collect();
    let mut calls: HashMap<(Tool, &str, &str, &str), NodeKey> = HashMap::new();
    for f in facts {
        let key = NodeKey::of_fact(f);
        if f.kind == FactKind::ToolCall && !f.call_id.is_empty() {
            calls.insert(
                (f.tool, &f.account, &f.tool_session_id, &f.call_id),
                key.clone(),
            );
        }
        known.insert(key);
    }
    let metas: HashMap<NodeKey, &StoredAgent> = agents
        .iter()
        .filter_map(|a| {
            Some((
                NodeKey {
                    tool: a.tool?,
                    account: a.account.clone(),
                    session: a.tool_session_id.clone(),
                    agent: a.agent_id.clone(),
                },
                a,
            ))
        })
        .collect();
    let mut parents = HashMap::new();
    for key in &known {
        if key.tool != Tool::Claude || key.agent.is_none() {
            continue;
        }
        let meta = metas.get(key);
        let parent = meta
            .and_then(|m| m.spawn_tool_use_id.as_deref())
            .and_then(|id| calls.get(&(key.tool, key.account.as_str(), key.session.as_str(), id)))
            .cloned()
            .or_else(|| {
                let candidate = NodeKey {
                    agent: Some(meta?.parent_agent_id.clone()?),
                    ..key.clone()
                };
                known.contains(&candidate).then_some(candidate)
            })
            .unwrap_or_else(|| key.main());
        if &parent != key {
            parents.insert(key.clone(), parent);
        }
    }
    parents
}

pub fn facts_from(runs: &[RunWindow], from: DateTime<Utc>) -> Option<DateTime<Utc>> {
    runs.iter()
        .map(|w| w.started_at - Duration::minutes(SLACK_MINUTES))
        .min()
        .filter(|e| *e < from)
}

fn ancestors(key: &NodeKey, parents: &HashMap<NodeKey, NodeKey>) -> Vec<NodeKey> {
    let mut out = Vec::new();
    let mut cur = key.clone();
    for _ in 0..MAX_CHAIN {
        let Some(next) = parents
            .get(&cur)
            .cloned()
            .or_else(|| cur.agent.is_some().then(|| cur.main()))
        else {
            break;
        };
        if next == *key || out.contains(&next) {
            break;
        }
        out.push(next.clone());
        cur = next;
    }
    out
}

fn task_key(run: &RunWindow) -> String {
    format!("{}\u{1}{}", run.workspace, run.task_ref)
}

/// Windows of a task per ancestor of its bound nodes: runs of descendant activity or own calls, split at gaps over 30 minutes.
pub fn orchestrator_windows(
    events: &[LinkEvent],
    facts: &[StoredFact],
    runs: &[RunWindow],
    ids: &[RunIds],
    bindings: &[AgentBinding],
    parents: &HashMap<NodeKey, NodeKey>,
    now: DateTime<Utc>,
) -> Vec<OrchWindow> {
    type Key = (NodeKey, String);
    let mut stamps: HashMap<NodeKey, Vec<DateTime<Utc>>> = HashMap::new();
    for e in events {
        stamps.entry(NodeKey::of_event(e)).or_default().push(e.ts);
    }
    for f in facts {
        stamps.entry(NodeKey::of_fact(f)).or_default().push(f.ts);
    }
    let mut by_node: HashMap<NodeKey, Vec<&AgentBinding>> = HashMap::new();
    for b in bindings.iter().filter(|b| b.run_index < runs.len()) {
        by_node.entry(b.node()).or_default().push(b);
    }
    let mut spans: BTreeMap<Key, Vec<DateTime<Utc>>> = BTreeMap::new();
    let mut ordered: Vec<(&NodeKey, &Vec<DateTime<Utc>>)> = stamps.iter().collect();
    ordered.sort_by(|a, b| a.0.cmp(b.0));
    for (x, list) in ordered {
        let mut chain = vec![x.clone()];
        chain.extend(ancestors(x, parents));
        for (i, d) in chain.iter().enumerate().take(chain.len() - 1) {
            for b in by_node.get(d).into_iter().flatten() {
                let inside: Vec<DateTime<Utc>> = list
                    .iter()
                    .copied()
                    .filter(|t| *t >= b.from && b.to.map_or(*t <= now, |e| *t <= e))
                    .collect();
                if inside.is_empty() {
                    continue;
                }
                for o in &chain[i + 1..] {
                    spans
                        .entry((o.clone(), task_key(&runs[b.run_index])))
                        .or_default()
                        .extend(&inside);
                }
            }
        }
    }
    let slack = Duration::minutes(SLACK_MINUTES);
    let all = runs.len().min(ids.len());
    for f in facts {
        if f.tool != Tool::Claude || f.kind != FactKind::ToolCall {
            continue;
        }
        let (Some(marker), Some(value)) = (f.target.as_deref(), f.value) else {
            continue;
        };
        for r in matched_runs(marker, value, f.ts, runs, ids, all, now, slack) {
            let key = (NodeKey::of_fact(f), task_key(&runs[r]));
            if let Some(list) = spans.get_mut(&key) {
                list.push(f.ts);
            }
        }
    }
    let max_gap = Duration::minutes(WINDOW_GAP_MINUTES);
    let mut out = Vec::new();
    for ((node, task), mut anchors) in spans {
        anchors.sort();
        anchors.dedup();
        let task_runs: Vec<usize> = (0..runs.len())
            .filter(|&i| task_key(&runs[i]) == task)
            .collect();
        let mut start = anchors[0];
        let mut last = start;
        for &at in &anchors[1..] {
            if at - last > max_gap {
                out.push(OrchWindow {
                    node: node.clone(),
                    task: task.clone(),
                    runs: task_runs.clone(),
                    from: start,
                    to: last,
                });
                start = at;
            }
            last = at;
        }
        out.push(OrchWindow {
            node,
            task,
            runs: task_runs,
            from: start,
            to: last,
        });
    }
    out
}

pub struct Linker<'a> {
    runs: &'a [RunWindow],
    bindings: Vec<AgentBinding>,
    parents: HashMap<NodeKey, NodeKey>,
    windows: Vec<OrchWindow>,
    roles: Vec<Option<String>>,
    now: DateTime<Utc>,
}

impl<'a> Linker<'a> {
    pub fn new(
        runs: &'a [RunWindow],
        ids: &[RunIds],
        events: &[LinkEvent],
        facts: &[StoredFact],
        agents: &[StoredAgent],
        now: DateTime<Utc>,
    ) -> Self {
        let bindings = agent_bindings(facts, runs, ids, now);
        let parents = node_parents(events, facts, agents);
        let windows = orchestrator_windows(events, facts, runs, ids, &bindings, &parents, now);
        Linker {
            runs,
            bindings,
            parents,
            windows,
            roles: Vec::new(),
            now,
        }
    }

    /// `roles` runs parallel to the runs; a role is only used for runs where it is known.
    pub fn with_roles(mut self, roles: Vec<Option<String>>) -> Self {
        self.roles = roles;
        self
    }

    pub fn windows(&self) -> &[OrchWindow] {
        &self.windows
    }

    pub fn attribute(&self, events: &[LinkEvent]) -> LinkReport {
        attribute_full(
            events,
            self.runs,
            &self.bindings,
            &self.parents,
            Attribution {
                windows: &self.windows,
                roles: &self.roles,
            },
            self.now,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::{attribute_at, attribute_with_bindings, LinkMethod};
    use chrono::TimeZone;

    fn t(min: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 14, 9, 0, 0).unwrap() + Duration::minutes(min)
    }

    fn now() -> DateTime<Utc> {
        t(600)
    }

    fn run(r: &str, from: i64, to: Option<i64>) -> RunWindow {
        RunWindow {
            run_ref: r.into(),
            task_ref: format!("T-{r}"),
            workspace: "w".into(),
            session_id: None,
            branch: None,
            project_hash: None,
            started_at: t(from),
            ended_at: to.map(t),
        }
    }

    fn ids(run_id: u64, task_id: u64) -> RunIds {
        RunIds {
            run_id,
            task_id: Some(task_id),
        }
    }

    fn call(agent: Option<&str>, marker: &str, value: u64, at: i64) -> StoredFact {
        StoredFact {
            tool: Tool::Claude,
            account: "claude".into(),
            tool_session_id: "s".into(),
            agent_id: agent.map(String::from),
            ts: t(at),
            kind: FactKind::ToolCall,
            name: "mcp__tmgr__x".into(),
            detail: None,
            target: Some(marker.into()),
            ended_at: None,
            duration_ms: None,
            value: Some(value),
            is_error: false,
            project_hash: None,
            call_id: format!("{marker}{value}{at}"),
        }
    }

    fn ev(agent: Option<&str>, at: i64) -> LinkEvent {
        LinkEvent {
            id: at,
            tool: Tool::Claude,
            account: "claude".into(),
            tool_session_id: "s".into(),
            parent_session_id: None,
            ts: t(at),
            model: "m".into(),
            git_branch: None,
            project_hash: None,
            cost_usd: Some(1.0),
            tokens: 10,
            agent_id: agent.map(String::from),
        }
    }

    fn node(agent: Option<&str>) -> NodeKey {
        NodeKey {
            tool: Tool::Claude,
            account: "claude".into(),
            session: "s".into(),
            agent: agent.map(String::from),
        }
    }

    fn tree(pairs: &[(&str, Option<&str>)]) -> HashMap<NodeKey, NodeKey> {
        pairs
            .iter()
            .map(|(child, parent)| (node(Some(child)), node(*parent)))
            .collect()
    }

    fn bindings(facts: &[StoredFact], runs: &[RunWindow], ids: &[RunIds]) -> Vec<AgentBinding> {
        agent_bindings(facts, runs, ids, now())
    }

    #[test]
    fn a_run_id_call_binds_exactly_inside_the_window_and_ends_at_the_finish() {
        let runs = [run("a", 10, Some(60))];
        let facts = [
            call(Some("l"), "agent_work_update", 7, 20),
            call(Some("l"), "agent_work_finish", 7, 50),
            call(Some("l"), "agent_work_update", 7, 200),
        ];
        let b = bindings(&facts, &runs, &[ids(7, 1)]);
        assert_eq!(b.len(), 1);
        assert!(b[0].exact);
        assert_eq!((b[0].from, b[0].to), (t(20), Some(t(50))));
        assert_eq!(b[0].agent_id.as_deref(), Some("l"));
    }

    #[test]
    fn calls_that_match_no_run_bind_nothing() {
        let runs = [run("a", 10, Some(60)), run("b", 10, Some(60))];
        let far = call(Some("l"), "agent_work_finish", 7, 300);
        let start_far = call(Some("l"), "agent_work_start", 1, 20);
        assert!(bindings(&[far, start_far], &runs, &[ids(7, 1), ids(8, 2)]).is_empty());
    }

    #[test]
    fn a_call_matching_several_runs_binds_all_of_them_and_stays_ambiguous() {
        let runs = [run("a", 10, Some(60)), run("b", 11, Some(60))];
        let facts = [call(Some("l"), "agent_work_start", 1, 11)];
        let b = bindings(&facts, &runs, &[ids(7, 1), ids(8, 1)]);
        assert_eq!(b.len(), 2);
        let mut fallback = run("c", 0, Some(100));
        fallback.session_id = Some("s".into());
        let all = [runs[0].clone(), runs[1].clone(), fallback];
        let r =
            attribute_with_bindings(&[ev(Some("l"), 20)], &all, &b, &tree(&[("l", None)]), now());
        assert!(r.links.is_empty());
        assert_eq!(r.ambiguous[0].candidates, vec!["a", "b"]);
    }

    #[test]
    fn a_finish_call_within_slack_cannot_extend_the_binding_past_the_run_end() {
        let runs = [run("a", 10, Some(60))];
        let facts = [call(Some("l"), "agent_work_finish", 7, 61)];
        let b = bindings(&facts, &runs, &[ids(7, 1)]);
        assert_eq!(b[0].to, Some(t(60)));
        let r = attribute_with_bindings(
            &[ev(Some("l"), 60)],
            &runs,
            &b,
            &tree(&[("l", None)]),
            now(),
        );
        assert!(r.links.is_empty());
    }

    #[test]
    fn a_start_call_binds_likely_when_the_run_started_within_two_minutes() {
        let runs = [run("a", 10, None)];
        let facts = [call(Some("l"), "agent_work_start", 1, 11)];
        let b = bindings(&facts, &runs, &[ids(7, 1)]);
        assert_eq!(b.len(), 1);
        assert!(!b[0].exact);
        assert_eq!((b[0].from, b[0].to), (t(11), None));
        let late = [call(Some("l"), "agent_work_start", 1, 13)];
        assert!(bindings(&late, &runs, &[ids(7, 1)]).is_empty());
    }

    #[test]
    fn a_later_run_id_call_makes_the_binding_exact() {
        let runs = [run("a", 10, Some(60))];
        let facts = [
            call(Some("l"), "agent_work_start", 1, 10),
            call(Some("l"), "agent_work_update", 7, 30),
        ];
        let b = bindings(&facts, &runs, &[ids(7, 1)]);
        assert!(b[0].exact);
        assert_eq!(b[0].from, t(10));
        assert_eq!(b[0].to, Some(t(60)));
    }

    #[test]
    fn parents_follow_the_spawning_call_then_the_parent_agent_then_the_main_node() {
        let mut spawn = call(Some("l"), "x", 0, 1);
        spawn.call_id = "tu".into();
        let agents = [
            StoredAgent {
                tool: Some(Tool::Claude),
                account: "claude".into(),
                tool_session_id: "s".into(),
                agent_id: Some("w".into()),
                spawn_tool_use_id: Some("tu".into()),
                ..StoredAgent::default()
            },
            StoredAgent {
                tool: Some(Tool::Claude),
                account: "claude".into(),
                tool_session_id: "s".into(),
                agent_id: Some("k".into()),
                parent_agent_id: Some("l".into()),
                ..StoredAgent::default()
            },
        ];
        let events = [ev(Some("w"), 2), ev(Some("k"), 3), ev(Some("l"), 1)];
        let p = node_parents(&events, &[spawn], &agents);
        assert_eq!(p[&node(Some("w"))], node(Some("l")));
        assert_eq!(p[&node(Some("k"))], node(Some("l")));
        assert_eq!(p[&node(Some("l"))], node(None));
    }

    fn bind(agent: Option<&str>, run_index: usize, from: i64, to: Option<i64>) -> AgentBinding {
        AgentBinding {
            tool: Tool::Claude,
            account: "claude".into(),
            tool_session_id: "s".into(),
            agent_id: agent.map(String::from),
            run_index,
            from: t(from),
            to: to.map(t),
            exact: true,
        }
    }

    #[test]
    fn without_windows_workers_inherit_the_lead_binding_and_the_orchestrator_stays_out() {
        let runs = [run("a", 0, Some(100))];
        let b = [bind(Some("l"), 0, 10, Some(50))];
        let parents = tree(&[("w", Some("l")), ("l", None)]);
        let events = [
            ev(Some("l"), 20),
            ev(Some("w"), 30),
            ev(None, 30),
            ev(Some("w"), 60),
        ];
        let r = attribute_with_bindings(&events, &runs, &b, &parents, now());
        let linked: Vec<usize> = r.links.iter().map(|l| l.event_index).collect();
        assert_eq!(linked, vec![0, 1]);
        assert!(r
            .links
            .iter()
            .all(|l| l.method == LinkMethod::AgentWorkRun && l.confidence == 0.95));
        assert_eq!(r.unattributed, vec![2, 3]);
    }

    #[test]
    fn a_main_binding_does_not_override_a_subagent_binding() {
        let runs = [run("a", 0, Some(100)), run("b", 0, Some(100))];
        let b = [bind(None, 0, 0, None), bind(Some("l"), 1, 10, Some(50))];
        let parents = tree(&[("l", None)]);
        let events = [ev(Some("l"), 20), ev(None, 20), ev(Some("l"), 70)];
        let r = attribute_with_bindings(&events, &runs, &b, &parents, now());
        let by_event: Vec<(usize, &str)> = r
            .links
            .iter()
            .map(|l| (l.event_index, l.run_ref.as_str()))
            .collect();
        assert_eq!(by_event, vec![(0, "b"), (1, "a"), (2, "a")]);
    }

    #[test]
    fn a_session_id_run_beats_a_main_binding_but_not_a_subagent_binding() {
        let mut by_session = run("s", 0, Some(100));
        by_session.session_id = Some("s".into());
        let runs = [by_session, run("a", 0, Some(100)), run("b", 0, Some(100))];
        let b = [bind(None, 1, 0, None), bind(Some("l"), 2, 0, None)];
        let parents = tree(&[("l", None)]);
        let events = [ev(None, 20), ev(Some("l"), 20)];
        let r = attribute_with_bindings(&events, &runs, &b, &parents, now());
        assert_eq!(r.links[0].run_ref, "s");
        assert_eq!(r.links[0].method, LinkMethod::SessionId);
        assert_eq!(r.links[1].run_ref, "b");
    }

    #[test]
    fn overlapping_bindings_on_one_node_are_ambiguous_and_do_not_fall_through() {
        let mut fallback = run("c", 0, Some(100));
        fallback.session_id = Some("s".into());
        let runs = [run("a", 0, Some(100)), run("b", 0, Some(100)), fallback];
        let b = [
            bind(Some("l"), 0, 10, Some(50)),
            bind(Some("l"), 1, 30, Some(70)),
        ];
        let parents = tree(&[("l", None)]);
        let events = [ev(Some("l"), 20), ev(Some("l"), 40), ev(Some("l"), 60)];
        let r = attribute_with_bindings(&events, &runs, &b, &parents, now());
        assert_eq!(r.links.len(), 2);
        assert_eq!(r.links[0].run_ref, "a");
        assert_eq!(r.links[1].run_ref, "b");
        assert_eq!(r.ambiguous.len(), 1);
        assert_eq!(r.ambiguous[0].event_index, 1);
        assert_eq!(r.ambiguous[0].candidates, vec!["a", "b"]);
        assert_eq!(r.ambiguous_totals.events, 1);
    }

    #[test]
    fn two_leads_bound_to_two_tasks_link_each_event_once() {
        let runs = [run("a", 0, Some(100)), run("b", 0, Some(100))];
        let b = [bind(Some("x"), 0, 5, None), bind(Some("y"), 1, 5, None)];
        let parents = tree(&[("x", None), ("y", None)]);
        let events = [ev(Some("x"), 10), ev(Some("y"), 10), ev(Some("y"), 20)];
        let r = attribute_with_bindings(&events, &runs, &b, &parents, now());
        let refs: Vec<&str> = r.links.iter().map(|l| l.run_ref.as_str()).collect();
        assert_eq!(refs, vec!["a", "b", "b"]);
        assert!(r.ambiguous.is_empty());
    }

    #[test]
    fn a_likely_binding_uses_the_task_method() {
        let runs = [run("a", 0, Some(100))];
        let mut b = bind(Some("l"), 0, 5, None);
        b.exact = false;
        let r = attribute_with_bindings(
            &[ev(Some("l"), 10)],
            &runs,
            &[b],
            &tree(&[("l", None)]),
            now(),
        );
        assert_eq!(r.links[0].method, LinkMethod::AgentWorkTask);
        assert_eq!(r.links[0].confidence, 0.75);
    }

    #[test]
    fn without_bindings_the_result_is_the_old_one() {
        let mut a = run("a", 0, Some(60));
        a.branch = Some("feat".into());
        let mut e = ev(Some("l"), 5);
        e.git_branch = Some("feat".into());
        let events = [e, ev(None, 7)];
        let plain = attribute_at(&events, std::slice::from_ref(&a), now());
        let with = attribute_with_bindings(&events, &[a], &[], &HashMap::new(), now());
        assert_eq!(plain, with);
    }

    #[test]
    fn a_parent_cycle_inherits_nothing() {
        let runs = [run("a", 0, Some(100))];
        let b = [bind(None, 0, 0, None)];
        let parents = tree(&[("l", Some("w")), ("w", Some("l"))]);
        let r = attribute_with_bindings(&[ev(Some("l"), 20)], &runs, &b, &parents, now());
        assert!(r.links.is_empty());
        assert_eq!(r.unattributed, vec![0]);
    }

    fn linker<'a>(
        runs: &'a [RunWindow],
        run_ids: &[RunIds],
        events: &[LinkEvent],
        facts: &[StoredFact],
    ) -> Linker<'a> {
        Linker::new(runs, run_ids, events, facts, &[], now())
    }

    #[test]
    fn the_orchestrator_is_attributed_inside_the_window_of_its_bound_descendants() {
        let runs = [run("a", 10, Some(100))];
        let facts = [call(Some("l"), "agent_work_start", 1, 10)];
        let events = [
            ev(None, 5),
            ev(Some("l"), 20),
            ev(None, 15),
            ev(None, 30),
            ev(Some("l"), 40),
            ev(None, 50),
        ];
        let l = linker(&runs, &[ids(7, 1)], &events, &facts);
        let r = l.attribute(&events);
        let linked: Vec<(usize, LinkMethod)> =
            r.links.iter().map(|k| (k.event_index, k.method)).collect();
        assert_eq!(
            linked,
            vec![
                (1, LinkMethod::AgentWorkTask),
                (2, LinkMethod::OrchestratorWindow),
                (3, LinkMethod::OrchestratorWindow),
                (4, LinkMethod::AgentWorkTask),
            ]
        );
        assert_eq!(r.unattributed, vec![0, 5]);
        let mark = r.marks.iter().find(|m| m.node == node(None)).unwrap();
        assert_eq!(mark.role.as_deref(), Some("orchestrator"));
        assert!(mark.likely);
        assert!(r.marks.iter().all(|m| m.node != node(Some("l"))));
    }

    #[test]
    fn the_window_stretches_to_the_orchestrators_own_calls_naming_the_task() {
        let runs = [run("a", 10, Some(100))];
        let facts = [
            call(Some("l"), "agent_work_start", 1, 10),
            call(None, "agent_work_update", 7, 40),
        ];
        let events = [ev(Some("l"), 20), ev(None, 35)];
        let l = linker(&runs, &[ids(7, 1)], &events, &facts);
        let w = l.windows().iter().find(|w| w.node == node(None)).unwrap();
        assert_eq!((w.from, w.to), (t(10), t(40)));
    }

    #[test]
    fn a_lead_resumed_hours_later_does_not_pull_the_orchestrator_events_in_between() {
        let runs = [run("a", 10, Some(600))];
        let facts = [call(Some("l"), "agent_work_start", 1, 10)];
        let events = [
            ev(Some("l"), 20),
            ev(None, 15),
            ev(None, 100),
            ev(Some("l"), 200),
            ev(Some("l"), 210),
            ev(None, 205),
        ];
        let l = linker(&runs, &[ids(7, 1)], &events, &facts);
        let windows: Vec<_> = l
            .windows()
            .iter()
            .filter(|w| w.node == node(None))
            .map(|w| (w.from, w.to))
            .collect();
        assert_eq!(windows, vec![(t(10), t(20)), (t(200), t(210))]);
        let r = l.attribute(&events);
        let orchestrator: Vec<usize> = r
            .links
            .iter()
            .filter(|k| k.method == LinkMethod::OrchestratorWindow)
            .map(|k| k.event_index)
            .collect();
        assert_eq!(orchestrator, vec![1, 5]);
        assert_eq!(r.unattributed, vec![2]);
        let counted = r.links.len() + r.ambiguous.len() + r.unattributed.len() + r.shared.len();
        assert_eq!(counted, events.len());
    }

    #[test]
    fn overlapping_windows_of_two_tasks_share_the_events_and_nothing_is_double_counted() {
        let runs = [run("a", 10, Some(100)), run("b", 20, Some(100))];
        let facts = [
            call(Some("l1"), "agent_work_start", 1, 10),
            call(Some("l2"), "agent_work_start", 2, 20),
        ];
        let events = [
            ev(Some("l1"), 12),
            ev(Some("l1"), 30),
            ev(Some("l2"), 22),
            ev(Some("l2"), 40),
            ev(None, 15),
            ev(None, 25),
            ev(None, 35),
            ev(None, 45),
        ];
        let l = linker(&runs, &[ids(7, 1), ids(8, 2)], &events, &facts);
        let r = l.attribute(&events);
        let by_event = |i: usize| r.links.iter().find(|k| k.event_index == i);
        assert_eq!(by_event(4).unwrap().run_ref, "a");
        assert!(by_event(5).is_none() && r.shared.contains(&5));
        assert_eq!(by_event(6).unwrap().run_ref, "b");
        assert_eq!(r.unattributed.last(), Some(&7));
        assert_eq!(r.shared, vec![5]);
        let counted = r.links.len() + r.ambiguous.len() + r.unattributed.len() + r.shared.len();
        assert_eq!(counted, events.len());
        let cost: f64 = r.per_task.iter().map(|t| t.cost_usd).sum::<f64>()
            + r.ambiguous_totals.cost_usd
            + r.unattributed_totals.cost_usd
            + r.shared_totals.cost_usd;
        assert_eq!(cost, events.len() as f64);
    }

    #[test]
    fn an_agent_session_id_matches_only_that_node_and_a_bare_one_keeps_its_children() {
        let mut exact = run("x", 0, Some(100));
        exact.session_id = Some("s/l".into());
        let mut bare = run("y", 0, Some(100));
        bare.session_id = Some("s".into());
        let events = [ev(Some("l"), 10), ev(Some("m"), 10), ev(None, 10)];
        let only_exact =
            attribute_with_bindings(&events, &[exact.clone()], &[], &HashMap::new(), now());
        let linked: Vec<usize> = only_exact.links.iter().map(|k| k.event_index).collect();
        assert_eq!(linked, vec![0]);
        let both = attribute_with_bindings(&events, &[exact, bare], &[], &HashMap::new(), now());
        let by_event: Vec<(usize, &str)> = both
            .links
            .iter()
            .map(|k| (k.event_index, k.run_ref.as_str()))
            .collect();
        assert_eq!(by_event, vec![(0, "x"), (1, "y"), (2, "y")]);
    }

    #[test]
    fn a_run_with_a_role_marks_the_node_it_is_linked_to() {
        let mut orchestrator = run("o", 0, Some(100));
        orchestrator.session_id = Some("s".into());
        let events = [ev(None, 10), ev(Some("w"), 10)];
        let runs = [orchestrator];
        let roles = vec![Some("orchestrator".to_string())];
        let r = crate::link::attribute_full(
            &events,
            &runs,
            &[],
            &HashMap::new(),
            crate::link::Attribution {
                windows: &[],
                roles: &roles,
            },
            now(),
        );
        assert_eq!(r.marks.len(), 1);
        assert_eq!(r.marks[0].node, node(None));
        assert_eq!(r.marks[0].role.as_deref(), Some("orchestrator"));
        assert!(!r.marks[0].likely);
    }
}
