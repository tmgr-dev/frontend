use super::fmt::{count, money, span};
use super::runs::{self, WantedTask};
use super::summary::linking_json;
use super::{fail, scan, Usage};
use crate::cli_usage::TaskArgs;
use crate::ctx::Ctx;
use crate::out::{Output, Style};
use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use tmgr_core::error::{Error, Result};
use tmgr_core::refs::{parse_task_ref, TaskRef};
use tmgr_core::text::sanitize_line;
use tmgr_usage::bind::Linker;
use tmgr_usage::facts::{StoredAgent, StoredFact};
use tmgr_usage::limits::LimitSnapshot;
use tmgr_usage::link::{Link, LinkEvent, LinkMethod, LinkReport, NodeMark};
use tmgr_usage::pricing::PriceBook;
use tmgr_usage::task::{
    build_with, facts_as_events, BuildExtras, LimitKind, LinkKind, Node, TaskIdent, TaskInput,
    TaskUsageAggregate,
};
use tmgr_usage::{Tokens, Tool};

const LABEL_WIDTH: usize = 8;
const TOP_NAMES: usize = 6;

pub struct Corpus {
    pub now: DateTime<Utc>,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub source_id: String,
    pub events: Vec<LinkEvent>,
    pub tokens: Vec<Tokens>,
    pub facts: Vec<StoredFact>,
    pub agents: Vec<StoredAgent>,
    pub limits: Vec<LimitSnapshot>,
    pub book: PriceBook,
    pub set: runs::RunSet,
    pub report: LinkReport,
    pub fact_report: LinkReport,
}

pub struct Bucket {
    pub workspace: String,
    pub task_ref: String,
    pub id: Option<u64>,
    pub id_mismatch: bool,
    pub events: Vec<usize>,
    pub facts: Vec<usize>,
}

impl Corpus {
    pub fn load(ctx: &Ctx, u: &mut Usage, days: i64, no_scan: bool) -> Result<Corpus> {
        let now = Utc::now();
        let (from, to) = (now - Duration::days(days), now + Duration::minutes(1));
        if !no_scan {
            scan(ctx, u, now)?;
        }
        let (events, tokens): (Vec<_>, Vec<_>) = u
            .store
            .link_events_with_tokens(from, to)
            .map_err(fail)?
            .into_iter()
            .unzip();
        let facts = u.store.facts(from, to).map_err(fail)?;
        let agents = u.store.agents().map_err(fail)?;
        let set = runs::collect(ctx, from, to, now);
        let fact_events = facts_as_events(&facts, &events);
        let earlier = runs::earlier_facts(&u.store, &set, from, to)?;
        let linker = Linker::new(
            &set.windows,
            &set.ids,
            &events,
            earlier.as_deref().unwrap_or(&facts),
            &agents,
            now,
        )
        .with_roles(set.roles.clone());
        let report = linker.attribute(&events);
        let fact_report = linker.attribute(&fact_events);
        Ok(Corpus {
            now,
            from,
            to,
            source_id: u.store.install_id().map_err(fail)?,
            limits: u.store.current_limits(now).map_err(fail)?,
            book: u.book.clone(),
            events,
            tokens,
            facts,
            agents,
            set,
            report,
            fact_report,
        })
    }

    pub fn aggregate(
        &self,
        task: TaskIdent,
        linked_events: &[usize],
        linked_facts: &[usize],
    ) -> Option<TaskUsageAggregate> {
        let runs: BTreeSet<&str> = [
            (&self.report, linked_events),
            (&self.fact_report, linked_facts),
        ]
        .into_iter()
        .flat_map(|(report, linked)| {
            let linked: BTreeSet<usize> = linked.iter().copied().collect();
            report
                .links
                .iter()
                .filter(move |l| linked.contains(&l.event_index))
                .map(|l| l.run_ref.as_str())
                .collect::<Vec<_>>()
        })
        .collect();
        let marks: Vec<NodeMark> = self
            .report
            .marks
            .iter()
            .chain(&self.fact_report.marks)
            .filter(|m| runs.contains(m.run_ref.as_str()))
            .cloned()
            .collect();
        build_with(
            &TaskInput {
                source_id: self.source_id.clone(),
                generated_at: self.now,
                task,
                from: self.from,
                to: self.to,
                events: &self.events,
                tokens: &self.tokens,
                linked_events,
                facts: &self.facts,
                linked_facts,
                agents: &self.agents,
                limits: &self.limits,
            },
            &BuildExtras {
                marks: &marks,
                book: Some(&self.book),
            },
        )
    }

    pub fn buckets(&self) -> Vec<Bucket> {
        let workspace_of: HashMap<&str, &str> = self
            .set
            .windows
            .iter()
            .map(|w| (w.run_ref.as_str(), w.workspace.as_str()))
            .collect();
        let mut by_task: BTreeMap<(String, String), Bucket> = BTreeMap::new();
        for (links, facts) in [(&self.report.links, false), (&self.fact_report.links, true)] {
            for l in links {
                let (Some(meta), Some(ws)) = (
                    self.set.meta.get(&l.run_ref),
                    workspace_of.get(l.run_ref.as_str()),
                ) else {
                    continue;
                };
                let plain = meta.task_ref.rsplit('/').next().unwrap_or(&meta.task_ref);
                let bucket = by_task
                    .entry((ws.to_string(), plain.to_ascii_uppercase()))
                    .or_insert_with(|| Bucket {
                        workspace: ws.to_string(),
                        task_ref: plain.to_string(),
                        id: None,
                        id_mismatch: false,
                        events: Vec::new(),
                        facts: Vec::new(),
                    });
                match (bucket.id, meta.task_id) {
                    (Some(a), Some(b)) if a != b => bucket.id_mismatch = true,
                    (None, found) => bucket.id = found,
                    _ => {}
                }
                if facts {
                    bucket.facts.push(l.event_index);
                } else {
                    bucket.events.push(l.event_index);
                }
            }
        }
        by_task.into_values().collect()
    }
}

pub fn run(ctx: &Ctx, args: &TaskArgs) -> Result<Output> {
    let want = match parse_task_ref(&args.reference)? {
        TaskRef::Id(id) => WantedTask::Id(id),
        TaskRef::Key(k) => WantedTask::Key(k),
    };
    let mut u = Usage::open()?;
    let c = Corpus::load(ctx, &mut u, args.days, args.no_scan)?;
    let (set, report, fact_report) = (&c.set, &c.report, &c.fact_report);
    let matching = |links: &[Link]| -> Vec<usize> {
        links
            .iter()
            .filter(|l| set.task_matches(&l.run_ref, &want))
            .map(|l| l.event_index)
            .collect()
    };
    let linked_events = matching(&report.links);
    let linked_facts = matching(&fact_report.links);
    let mut linking = linking_json(set, report);
    linking["linked_facts"] = json!(linked_facts.len());

    let label = match &want {
        WantedTask::Id(id) => format!("#{id}"),
        WantedTask::Key(k) => k.clone(),
    };
    let matched_runs: BTreeSet<&str> = report
        .links
        .iter()
        .chain(&fact_report.links)
        .filter(|l| set.task_matches(&l.run_ref, &want))
        .map(|l| l.run_ref.as_str())
        .collect();
    let workspaces: BTreeSet<&str> = set
        .windows
        .iter()
        .filter(|w| matched_runs.contains(w.run_ref.as_str()))
        .map(|w| w.workspace.as_str())
        .collect();
    if workspaces.len() > 1 {
        return Err(Error::usage(format!(
            "{} matches tasks in several workspaces ({})",
            sanitize_line(&label),
            workspaces.iter().copied().collect::<Vec<_>>().join(", ")
        ))
        .hint("pick one with --ws CODE"));
    }
    let meta = matched_runs.iter().find_map(|r| set.meta.get(*r));
    let task = TaskIdent {
        workspace: workspaces.iter().next().copied().unwrap_or("").to_string(),
        task_ref: match (&want, meta) {
            (WantedTask::Key(k), _) => k.clone(),
            (_, Some(m)) => m.task_ref.rsplit('/').next().unwrap_or("").to_string(),
            _ => label.clone(),
        },
        id: meta.and_then(|m| m.task_id),
    };
    let title = meta
        .and_then(|m| m.title.as_deref())
        .map(sanitize_line)
        .filter(|t| !t.trim().is_empty());

    let aggregate = c.aggregate(task, &linked_events, &linked_facts);
    let Some(aggregate) = aggregate else {
        return Ok(Output::new(
            json!({"basis": "api_equivalent", "aggregate": Value::Null, "title": Value::Null, "linking": linking}),
            format!(
                "No usage linked to {} in this period.\nLinking needs run windows: tmgr agent start records the session; see tmgr usage --by task for unattributed usage.",
                sanitize_line(&label)
            ),
        ));
    };
    if aggregate.totals.unpriced_events > 0 {
        ctx.warn(
            "usage_unpriced",
            format!(
                "{} events use models without a known price and are left out of the cost (tmgr usage pricing update)",
                aggregate.totals.unpriced_events
            ),
        );
    }
    let by_task_id = report
        .links
        .iter()
        .chain(&fact_report.links)
        .any(|l| l.method == LinkMethod::AgentWorkTask && set.task_matches(&l.run_ref, &want));
    let human = render(
        &aggregate,
        title.as_deref(),
        args.tree,
        by_task_id,
        &ctx.style,
    );
    Ok(Output::new(
        json!({"basis": "api_equivalent", "aggregate": aggregate, "title": title, "linking": linking}),
        human,
    ))
}

fn line(style: &Style, label: &str, text: String) -> String {
    format!("{}{text}", style.muted(&format!("{label:<LABEL_WIDTH$}")))
}

fn thousands(n: u64) -> String {
    let mut out: Vec<char> = Vec::new();
    for (i, c) in n.to_string().chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.iter().rev().collect()
}

fn seconds_text(ms: u64) -> String {
    format!("{:.1}s", ms as f64 / 1000.0)
}

fn times(name: &str, n: u64) -> String {
    format!("{} ×{n}", sanitize_line(name))
}

fn join<T>(items: impl Iterator<Item = T>, sep: &str, f: impl Fn(T) -> String) -> String {
    items.map(f).collect::<Vec<_>>().join(sep)
}

fn node_line(n: &Node, style: &Style) -> String {
    let label = match (n.tool, n.agent_type.as_str()) {
        (Tool::Claude, "main") => "Claude Code".to_string(),
        (Tool::Codex, _) => "Codex".to_string(),
        (_, other) => sanitize_line(other),
    };
    let mut parts = vec![style.bold(&label)];
    if let Some(role) = &n.role {
        parts.push(style.muted(&sanitize_line(role)));
    }
    if n.model != "unknown" {
        let model = n.model.strip_prefix("claude-").unwrap_or(&n.model);
        parts.push(style.muted(&sanitize_line(model)));
    }
    if n.tool == Tool::Codex {
        if let Some(o) = &n.origin {
            parts.push(style.muted(&sanitize_line(o)));
        }
    }
    if n.events > 0 || n.duration_ms > 0 {
        let (active, wall) = ((n.active_ms / 1000) as i64, (n.duration_ms / 1000) as i64);
        parts.push(if active < wall {
            format!("active {} · wall {}", span(active), span(wall))
        } else {
            span(wall)
        });
    }
    if n.cost_usd > 0.0 {
        parts.push(style.good(&money(n.cost_usd)));
    }
    let mut text = parts.join(&style.muted(" · "));
    let tools = join(n.tools.iter().take(4), " ", |t| times(&t.name, t.count));
    if !tools.is_empty() {
        text.push_str(&format!("   {}", style.muted(&tools)));
    }
    if n.link == Some(LinkKind::Likely) {
        text.push_str(&format!("  {}", style.muted("(likely)")));
    }
    text
}

type Kids<'a> = BTreeMap<Option<&'a str>, Vec<&'a Node>>;

fn walk(
    node: &Node,
    kids: &Kids<'_>,
    prefix: &str,
    place: Option<bool>,
    style: &Style,
    all: bool,
    out: &mut Vec<String>,
) {
    let glyph = match place {
        None => "",
        Some(true) => "└─ ",
        Some(false) => "├─ ",
    };
    out.push(format!("{prefix}{glyph}{}", node_line(node, style)));
    if !all {
        return;
    }
    let below = match place {
        None => String::new(),
        Some(last) => format!("{prefix}{}", if last { "   " } else { "│  " }),
    };
    let children = kids.get(&Some(node.id.as_str())).map_or(&[][..], |c| c);
    for (i, c) in children.iter().enumerate() {
        let last = i + 1 == children.len();
        walk(c, kids, &below, Some(last), style, all, out);
    }
}

fn tree_lines(a: &TaskUsageAggregate, style: &Style, all: bool) -> Vec<String> {
    let mut kids: Kids<'_> = BTreeMap::new();
    for n in &a.nodes {
        kids.entry(n.parent_id.as_deref()).or_default().push(n);
    }
    let mut out = Vec::new();
    for root in kids.get(&None).map_or(&[][..], |r| r) {
        walk(root, &kids, "", None, style, all, &mut out);
    }
    out
}

fn summary_lines(a: &TaskUsageAggregate, style: &Style) -> Vec<String> {
    let mut out = Vec::new();
    let t = &a.totals;
    out.push(line(
        style,
        "Time",
        format!("agents {} working", span((t.active_ms / 1000) as i64)),
    ));
    out.push(line(
        style,
        "Cost",
        format!("AI {} (API-equivalent)", style.good(&money(t.cost_usd))),
    ));
    let k = &t.tokens;
    let prompt = k.prompt_total();
    let mut tokens = vec![format!("input {}", count(k.input))];
    tokens.push(if prompt > 0 {
        format!(
            "cache read {} ({}%)",
            count(k.cache_read),
            (k.cache_read as f64 * 100.0 / prompt as f64).round()
        )
    } else {
        format!("cache read {}", count(k.cache_read))
    });
    tokens.push(format!(
        "cache write {}",
        count(k.cache_write_5m + k.cache_write_1h)
    ));
    tokens.push(format!("output {}", count(k.output)));
    if k.reasoning > 0 {
        tokens.push(format!("reasoning {}", count(k.reasoning)));
    }
    out.push(line(style, "Tokens", tokens.join(" · ")));
    if !a.limits.is_empty() {
        let text = join(a.limits.iter(), " · ", |l| {
            let tool = match l.tool {
                Tool::Claude => "Claude",
                Tool::Codex => "Codex",
            };
            let window = match l.window {
                LimitKind::FiveHour => "5h",
                LimitKind::Week => "week",
            };
            format!("{tool} {window} −{}%", l.used_pct)
        });
        out.push(line(style, "Limits", text));
    }
    out
}

fn context_lines(a: &TaskUsageAggregate, style: &Style) -> Vec<String> {
    let c = &a.context;
    let mut out = Vec::new();
    if !c.tools.is_empty() {
        let mut grouped: BTreeMap<String, u64> = BTreeMap::new();
        for t in &c.tools {
            let name = match t.name.strip_prefix("mcp__") {
                Some(rest) => format!("mcp__{}", rest.split("__").next().unwrap_or(rest)),
                None => t.name.clone(),
            };
            *grouped.entry(name).or_default() += t.count;
        }
        let mut list: Vec<_> = grouped.into_iter().collect();
        list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        out.push(line(
            style,
            "Tools",
            join(list.iter().take(8), " · ", |(n, c)| times(n, *c)),
        ));
    }
    if !c.skills.is_empty() {
        let mut text = join(c.skills.iter().take(TOP_NAMES), " · ", |s| {
            times(&s.name, s.count)
        });
        if c.skills_available > 0 {
            text.push_str(&format!("  (of {} available)", c.skills_available));
        }
        out.push(line(style, "Skills", text));
    }
    if !c.plugins.is_empty() {
        out.push(line(
            style,
            "Plugins",
            join(c.plugins.iter().take(TOP_NAMES), " · ", |p| {
                times(&p.name, p.count)
            }),
        ));
    }
    if !c.mcp.is_empty() {
        let mut servers: BTreeMap<&str, u64> = BTreeMap::new();
        for m in &c.mcp {
            *servers.entry(m.server.as_str()).or_default() += m.count;
        }
        let mut list: Vec<_> = servers.into_iter().collect();
        list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        out.push(line(
            style,
            "MCP",
            join(list.iter().take(TOP_NAMES), " · ", |(n, c)| times(n, *c)),
        ));
    }
    if !c.hooks.is_empty() {
        let runs: u64 = c.hooks.iter().map(|h| h.count).sum();
        let ms: u64 = c.hooks.iter().map(|h| h.duration_ms).sum();
        let errors: u64 = c.hooks.iter().map(|h| h.errors).sum();
        let mut by_name: BTreeMap<&str, u64> = BTreeMap::new();
        for h in &c.hooks {
            *by_name.entry(h.name.as_str()).or_default() += h.count;
        }
        let top = by_name
            .into_iter()
            .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(a.0)));
        let mut text = format!(
            "{} runs · {} · {} errors",
            thousands(runs),
            seconds_text(ms),
            errors
        );
        if let Some((name, n)) = top {
            text.push_str(&format!("  (top: {})", times(name, n)));
        }
        out.push(line(style, "Hooks", text));
    }
    let mut mode: Vec<String> = Vec::new();
    if !c.permission_modes.is_empty() {
        mode.push(join(c.permission_modes.iter(), ", ", |m| {
            sanitize_line(&m.mode)
        }));
    }
    if c.compactions > 0 {
        mode.push(format!("Compactions {}", c.compactions));
    }
    if !c.prs.is_empty() {
        mode.push(format!(
            "PRs {}",
            join(c.prs.iter().take(3), ", ", |p| format!(
                "{}#{}",
                sanitize_line(&p.repo),
                p.number
            ))
        ));
    }
    if !mode.is_empty() {
        out.push(line(style, "Mode", mode.join(" · ")));
    }
    out
}

fn render(
    a: &TaskUsageAggregate,
    title: Option<&str>,
    tree: bool,
    by_task_id: bool,
    style: &Style,
) -> String {
    let mut head = style.bold(&sanitize_line(&a.task.task_ref));
    if let Some(t) = title {
        let t = t
            .strip_prefix(a.task.task_ref.as_str())
            .and_then(|rest| rest.strip_prefix(':'))
            .map_or(t, str::trim_start);
        head.push_str(&format!("  {t}"));
    }
    let mut summary = summary_lines(a, style);
    if by_task_id {
        summary.push(line(
            style,
            "Linking",
            "agent work calls (some by task id only: likely)".into(),
        ));
    }
    let mut blocks = vec![head, summary.join("\n")];
    let mut nodes = tree_lines(a, style, tree).join("\n");
    if a.truncated.nodes {
        nodes.push_str(&format!(
            "\n{}",
            style.muted("(some agents are left out of the tree)")
        ));
    }
    blocks.push(nodes);
    let context = context_lines(a, style);
    if !context.is_empty() {
        blocks.push(context.join("\n"));
    }
    blocks.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(duration_ms: u64, active_ms: u64, role: Option<&str>) -> Node {
        Node {
            id: "n0".into(),
            parent_id: None,
            depth: 0,
            tool: Tool::Claude,
            agent_type: "general-purpose".into(),
            model: "claude-sonnet-5-5".into(),
            origin: None,
            link: None,
            started_at: "2026-10-19T16:01:00Z".into(),
            ended_at: "2026-10-19T17:12:00Z".into(),
            duration_ms,
            active_ms,
            role: role.map(String::from),
            cost_usd: 2.11,
            events: 3,
            tokens: Tokens::default(),
            tools: Vec::new(),
        }
    }

    #[test]
    fn a_node_with_idle_time_shows_active_and_wall_and_its_role() {
        let style = Style::detect(true, false);
        let line = node_line(&node(4_260_000, 2_760_000, Some("lead")), &style);
        assert!(line.contains("lead"), "{line}");
        assert!(line.contains("active 46m · wall 1h 11m"), "{line}");
        let busy = node_line(&node(4_260_000, 4_260_000, None), &style);
        assert!(!busy.contains("active"), "{busy}");
        assert!(busy.contains("1h 11m"), "{busy}");
    }
}
