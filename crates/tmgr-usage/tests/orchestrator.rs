use chrono::{DateTime, Duration, TimeZone, Utc};
use std::collections::BTreeSet;
use tmgr_usage::bind::{Linker, RunIds};
use tmgr_usage::facts::{StoredAgent, StoredFact};
use tmgr_usage::link::{LinkEvent, LinkMethod, LinkReport, NodeMark, RunWindow};
use tmgr_usage::pricing::PriceEntry;
use tmgr_usage::task::{
    build_with, facts_as_events, validate_json, BuildExtras, LinkKind, TaskIdent, TaskInput,
    TaskUsageAggregate,
};
use tmgr_usage::{FactKind, PriceBook, Tokens, Tool};

fn at(sec: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 19, 12, 0, 0).unwrap() + Duration::seconds(sec)
}

fn min(m: i64) -> i64 {
    m * 60
}

#[derive(Default)]
struct World {
    events: Vec<LinkEvent>,
    tokens: Vec<Tokens>,
    facts: Vec<StoredFact>,
    agents: Vec<StoredAgent>,
    runs: Vec<RunWindow>,
    ids: Vec<RunIds>,
    roles: Vec<Option<String>>,
}

impl World {
    fn event(&mut self, agent: Option<&str>, sec: i64) {
        self.events.push(LinkEvent {
            id: self.events.len() as i64,
            tool: Tool::Claude,
            account: "claude".into(),
            tool_session_id: "s".into(),
            parent_session_id: None,
            ts: at(sec),
            model: "claude-opus-5-5".into(),
            git_branch: None,
            project_hash: None,
            cost_usd: Some(1.0),
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

    fn call(&mut self, agent: Option<&str>, name: &str, marker: Option<(&str, u64)>, sec: i64) {
        self.facts.push(StoredFact {
            tool: Tool::Claude,
            account: "claude".into(),
            tool_session_id: "s".into(),
            agent_id: agent.map(String::from),
            ts: at(sec),
            kind: FactKind::ToolCall,
            name: name.into(),
            detail: None,
            target: marker.map(|m| m.0.into()),
            ended_at: None,
            duration_ms: None,
            value: marker.map(|m| m.1),
            is_error: false,
            project_hash: None,
            call_id: format!("c{}", self.facts.len()),
        });
    }

    fn agent(&mut self, id: &str, kind: &str, parent: Option<&str>) {
        self.agents.push(StoredAgent {
            tool: Some(Tool::Claude),
            account: "claude".into(),
            tool_session_id: "s".into(),
            agent_id: Some(id.into()),
            agent_type: Some(kind.into()),
            parent_agent_id: parent.map(String::from),
            version: Some("2.1.90".into()),
            entrypoint: Some("cli".into()),
            ..StoredAgent::default()
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn run(
        &mut self,
        task: &str,
        run_id: u64,
        task_id: u64,
        from: i64,
        to: i64,
        session: Option<&str>,
        role: Option<&str>,
    ) {
        self.runs.push(RunWindow {
            run_ref: format!("w#{run_id}"),
            task_ref: task.into(),
            workspace: "w".into(),
            session_id: session.map(String::from),
            branch: None,
            project_hash: None,
            started_at: at(from),
            ended_at: Some(at(to)),
        });
        self.ids.push(RunIds {
            run_id,
            task_id: Some(task_id),
        });
        self.roles.push(role.map(String::from));
    }

    fn linker(&self) -> Linker<'_> {
        Linker::new(
            &self.runs,
            &self.ids,
            &self.events,
            &self.facts,
            &self.agents,
            at(100_000),
        )
        .with_roles(self.roles.clone())
    }

    fn reports(&self) -> (LinkReport, LinkReport) {
        let linker = self.linker();
        let fact_events = facts_as_events(&self.facts, &self.events);
        (
            linker.attribute(&self.events),
            linker.attribute(&fact_events),
        )
    }

    fn aggregate(
        &self,
        reports: &(LinkReport, LinkReport),
        task_ref: &str,
        id: Option<u64>,
    ) -> Option<TaskUsageAggregate> {
        let pick = |r: &LinkReport| -> Vec<usize> {
            r.links
                .iter()
                .filter(|l| l.task_ref == task_ref)
                .map(|l| l.event_index)
                .collect()
        };
        let runs: BTreeSet<&str> = self
            .runs
            .iter()
            .filter(|r| r.task_ref == task_ref)
            .map(|r| r.run_ref.as_str())
            .collect();
        let marks: Vec<NodeMark> = reports
            .0
            .marks
            .iter()
            .chain(&reports.1.marks)
            .filter(|m| runs.contains(m.run_ref.as_str()))
            .cloned()
            .collect();
        let mut book = PriceBook::empty();
        book.insert(
            "claude-opus-5-5",
            PriceEntry {
                costs: [
                    ("input_cost_per_token", 5e-6),
                    ("output_cost_per_token", 25e-6),
                    ("cache_read_input_token_cost", 5e-7),
                    ("cache_creation_input_token_cost", 6.25e-6),
                    ("cache_creation_input_token_cost_above_1hr", 10e-6),
                ]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            },
        );
        build_with(
            &TaskInput {
                source_id: "src_1".into(),
                generated_at: at(100_000),
                task: TaskIdent {
                    workspace: "w".into(),
                    task_ref: task_ref.into(),
                    id,
                },
                from: at(-100_000),
                to: at(100_000),
                events: &self.events,
                tokens: &self.tokens,
                linked_events: &pick(&reports.0),
                facts: &self.facts,
                linked_facts: &pick(&reports.1),
                agents: &self.agents,
                limits: &[],
            },
            &BuildExtras {
                marks: &marks,
                book: Some(&book),
            },
        )
    }
}

fn long_orchestration() -> World {
    let mut w = World::default();
    w.run("#10205", 1, 10205, min(110), min(122), None, None);
    w.run("#10205", 2, 10205, min(126) + 41, min(183), None, None);
    w.agent("l", "general-purpose", None);
    w.agent("p1", "Plan", Some("l"));
    w.agent("p2", "Plan", Some("l"));
    w.event(None, min(5));
    w.event(None, min(100));
    w.call(
        Some("l"),
        "mcp__tmgr__start_agent_work",
        Some(("agent_work_start", 10205)),
        min(110),
    );
    for m in 111..=121 {
        w.event(Some("l"), min(m));
    }
    w.event(Some("l"), min(121) + 17);
    w.event(None, min(115));
    w.event(None, min(123));
    w.event(None, min(124));
    w.event(Some("l"), min(126) + 41);
    w.call(
        Some("l"),
        "mcp__tmgr__start_agent_work",
        Some(("agent_work_start", 10205)),
        min(126) + 41,
    );
    for m in 128..=182 {
        w.event(Some("l"), min(m));
    }
    w.event(Some("p1"), min(140));
    w.event(Some("p1"), min(141));
    w.event(Some("p2"), min(150));
    w.event(None, min(150));
    w.call(
        Some("l"),
        "mcp__tmgr__finish_agent_work",
        Some(("agent_work_finish", 2)),
        min(182) + 30,
    );
    w.event(None, min(190));
    w
}

#[test]
fn a_long_lived_orchestrator_is_attributed_inside_the_task_window_and_costs_are_conserved() {
    let w = long_orchestration();
    let reports = w.reports();
    let a = w.aggregate(&reports, "#10205", Some(10205)).unwrap();

    let main = a.nodes.iter().find(|n| n.agent_type == "main").unwrap();
    assert_eq!(main.events, 4);
    assert_eq!(main.role.as_deref(), Some("orchestrator"));
    assert_eq!(main.link, Some(LinkKind::Likely));
    assert_eq!(main.active_ms, 60_000);

    let lead = a
        .nodes
        .iter()
        .find(|n| n.agent_type == "general-purpose")
        .unwrap();
    assert_eq!(lead.role, None);
    assert_eq!(lead.duration_ms, (min(72) + 30) as u64 * 1000);
    assert_eq!(lead.active_ms, (min(11) + 17 + min(55) + 49) as u64 * 1000);
    assert_eq!(a.nodes.iter().filter(|n| n.agent_type == "Plan").count(), 2);

    assert_eq!(a.totals.active_ms, lead.active_ms + main.active_ms);
    assert!(a.nodes.iter().all(|n| n.active_ms <= a.totals.active_ms));
    assert!(a.nodes.iter().all(|n| n.active_ms <= n.duration_ms));

    let linked: usize = reports.0.links.len();
    assert_eq!(
        linked + reports.0.unattributed.len() + reports.0.shared.len() + reports.0.ambiguous.len(),
        w.events.len()
    );
    assert_eq!(reports.0.unattributed, vec![0, 1, w.events.len() - 1]);
    assert_eq!(a.totals.cost_usd, linked as f64);
    let window_links = reports
        .0
        .links
        .iter()
        .filter(|l| l.method == LinkMethod::OrchestratorWindow)
        .count();
    assert_eq!(window_links, 4);
}

#[test]
fn an_orchestrator_driving_two_tasks_never_double_counts_the_overlap() {
    let mut w = World::default();
    w.run("T-A", 1, 1, min(10), min(40), None, None);
    w.run("T-B", 2, 2, min(20), min(60), None, None);
    w.agent("l1", "general-purpose", None);
    w.agent("l2", "general-purpose", None);
    w.call(
        Some("l1"),
        "mcp__tmgr__start_agent_work",
        Some(("agent_work_start", 1)),
        min(10),
    );
    w.call(
        Some("l2"),
        "mcp__tmgr__start_agent_work",
        Some(("agent_work_start", 2)),
        min(20),
    );
    for m in [12, 18, 25, 35] {
        w.event(Some("l1"), min(m));
    }
    for m in [22, 30, 45, 55] {
        w.event(Some("l2"), min(m));
    }
    for m in [0, 15, 28, 33, 50, 70] {
        w.event(None, min(m));
    }
    let reports = w.reports();
    let a = w.aggregate(&reports, "T-A", Some(1)).unwrap();
    let b = w.aggregate(&reports, "T-B", Some(2)).unwrap();

    let main_a = a.nodes.iter().find(|n| n.agent_type == "main").unwrap();
    let main_b = b.nodes.iter().find(|n| n.agent_type == "main").unwrap();
    assert_eq!(main_a.events, 1);
    assert_eq!(main_b.events, 1);
    assert_eq!(reports.0.shared.len(), 2);
    assert_eq!(reports.0.unattributed.len(), 2);

    let total_events = w.events.len();
    let accounted = reports.0.links.len()
        + reports.0.unattributed.len()
        + reports.0.shared.len()
        + reports.0.ambiguous.len();
    assert_eq!(accounted, total_events);
    assert_eq!(
        a.totals.cost_usd
            + b.totals.cost_usd
            + reports.0.shared_totals.cost_usd
            + reports.0.unattributed_totals.cost_usd,
        total_events as f64
    );
    let in_a: BTreeSet<usize> = reports
        .0
        .links
        .iter()
        .filter(|l| l.task_ref == "T-A")
        .map(|l| l.event_index)
        .collect();
    let in_b: BTreeSet<usize> = reports
        .0
        .links
        .iter()
        .filter(|l| l.task_ref == "T-B")
        .map(|l| l.event_index)
        .collect();
    assert!(in_a.is_disjoint(&in_b));
}

#[test]
fn an_orchestrator_with_its_own_run_is_linked_by_session_and_keeps_the_run_role() {
    let mut w = World::default();
    w.run(
        "T-O",
        1,
        9,
        min(0),
        min(200),
        Some("s"),
        Some("orchestrator"),
    );
    w.run("T-O", 2, 9, min(0), min(200), Some("s/l"), Some("worker"));
    w.agent("l", "general-purpose", None);
    w.event(None, min(10));
    w.event(Some("l"), min(20));
    w.event(Some("l"), min(21));
    w.event(None, min(30));
    let reports = w.reports();
    assert!(reports
        .0
        .links
        .iter()
        .all(|l| l.method == LinkMethod::SessionId));
    let by_event: Vec<(usize, &str)> = reports
        .0
        .links
        .iter()
        .map(|l| (l.event_index, l.run_ref.as_str()))
        .collect();
    assert_eq!(
        by_event,
        vec![(0, "w#1"), (1, "w#2"), (2, "w#2"), (3, "w#1")]
    );
    let a = w.aggregate(&reports, "T-O", Some(9)).unwrap();
    let main = a.nodes.iter().find(|n| n.agent_type == "main").unwrap();
    let lead = a
        .nodes
        .iter()
        .find(|n| n.agent_type == "general-purpose")
        .unwrap();
    assert_eq!(main.role.as_deref(), Some("orchestrator"));
    assert_eq!(main.link, None);
    assert_eq!(lead.role.as_deref(), Some("worker"));
    assert_eq!(a.totals.cost_usd, 4.0);
}

#[test]
fn the_whole_aggregate_matches_the_golden_json_and_passes_the_validator() {
    let w = long_orchestration();
    let reports = w.reports();
    let a = w.aggregate(&reports, "#10205", Some(10205)).unwrap();
    let value = serde_json::to_value(&a).unwrap();
    validate_json(&value, at(100_000)).unwrap();
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("golden/orchestrated.json")).unwrap();
    assert_eq!(value, golden);
}

#[test]
fn a_keyless_task_goes_out_as_hash_id_and_a_mismatching_id_is_rejected() {
    let w = long_orchestration();
    let reports = w.reports();
    let a = w.aggregate(&reports, "#10205", None).unwrap();
    assert_eq!(a.task.task_ref, "#10205");
    assert_eq!(a.task.id, Some(10205));
    let mut value = serde_json::to_value(&a).unwrap();
    validate_json(&value, at(100_000)).unwrap();
    value["task"]["id"] = serde_json::json!(7);
    assert!(validate_json(&value, at(100_000)).is_err());
    value["task"]["ref"] = serde_json::json!("#0");
    assert!(validate_json(&value, at(100_000)).is_err());
}

#[test]
fn only_models_with_a_full_price_set_get_a_rate() {
    let mut w = long_orchestration();
    w.events[3].model = "mystery-model".into();
    let reports = w.reports();
    let a = w.aggregate(&reports, "#10205", Some(10205)).unwrap();
    let models: Vec<&str> = a.rates.iter().map(|r| r.model.as_str()).collect();
    assert_eq!(models, vec!["claude-opus-5-5"]);
    let r = &a.rates[0];
    assert_eq!(
        (
            r.input,
            r.output,
            r.cache_read,
            r.cache_write_5m,
            r.cache_write_1h
        ),
        (5.0, 25.0, 0.5, 6.25, 10.0)
    );
    let json = serde_json::to_value(&a).unwrap();
    validate_json(&json, at(100_000)).unwrap();
}
