mod common;
use common::*;
use std::io::Write;
use std::path::Path;
use tmgr_usage::codex::CodexCollector;
use tmgr_usage::collector::{Collector, LogFile};
use tmgr_usage::model::{AgentMeta, Fact, FactEnd, FactKind};
use tmgr_usage::{scan_account, Account, ParsedRecord, ParserState, Tool};

const SECRETS: [&str; 4] = [
    "SECRET-ARG",
    "SECRET-OUTPUT",
    "SECRET-INSTR",
    "/Users/someone/x",
];

fn ts(n: u32) -> String {
    format!("2026-10-14T10:{:02}:{:02}Z", n / 60, n % 60)
}

fn line(t: u32, kind: &str, payload: &str) -> String {
    format!(
        r#"{{"timestamp":"{}","type":"{kind}","payload":{payload}}}"#,
        ts(t)
    )
}

fn meta(id: &str, originator: &str, source: &str, parent: Option<&str>) -> String {
    let parent = parent
        .map(|p| format!(r#","parent_thread_id":"{p}""#))
        .unwrap_or_default();
    line(
        0,
        "session_meta",
        &format!(
            r#"{{"id":"{id}","cwd":"/work/alpha","cli_version":"0.150.0","originator":"{originator}","source":{source},"git":{{"branch":"feat/x"}}{parent},"base_instructions":{{"text":"SECRET-INSTR /Users/someone/x"}}}}"#
        ),
    )
}

fn call(t: u32, id: &str, ns: Option<&str>, name: &str) -> String {
    let ns = ns
        .map(|n| format!(r#""namespace":"{n}","#))
        .unwrap_or_default();
    line(
        t,
        "response_item",
        &format!(
            r#"{{"type":"function_call","name":"{name}",{ns}"arguments":"{{\"cmd\":\"SECRET-ARG /Users/someone/x\"}}","call_id":"{id}"}}"#
        ),
    )
}

fn custom_call(t: u32, id: &str, name: &str) -> String {
    line(
        t,
        "response_item",
        &format!(
            r#"{{"type":"custom_tool_call","status":"completed","call_id":"{id}","name":"{name}","input":"SECRET-ARG /Users/someone/x"}}"#
        ),
    )
}

fn output(t: u32, kind: &str, id: &str) -> String {
    line(
        t,
        "response_item",
        &format!(
            r#"{{"type":"{kind}","call_id":"{id}","output":"SECRET-OUTPUT /Users/someone/x"}}"#
        ),
    )
}

fn end(t: u32, body: &str) -> String {
    line(t, "event_msg", body)
}

fn exec_end(t: u32, id: &str, code: i32) -> String {
    let status = if code == 0 { "completed" } else { "failed" };
    end(
        t,
        &format!(
            r#"{{"type":"exec_command_end","call_id":"{id}","exit_code":{code},"status":"{status}","aggregated_output":"SECRET-OUTPUT","command":["SECRET-ARG"],"cwd":"/Users/someone/x"}}"#
        ),
    )
}

fn task_complete(t: u32, id: &str, ms: u64) -> String {
    end(
        t,
        &format!(
            r#"{{"type":"task_complete","turn_id":"{id}","duration_ms":{ms},"last_agent_message":"SECRET-OUTPUT"}}"#
        ),
    )
}

fn task_started(t: u32) -> String {
    end(t, r#"{"type":"task_started","turn_id":"x"}"#)
}

fn compacted(t: u32, id: Option<&str>) -> String {
    let id = id
        .map(|i| format!(r#""compaction_response_id":"{i}","#))
        .unwrap_or_default();
    line(
        t,
        "compacted",
        &format!(r#"{{{id}"message":"SECRET-OUTPUT","replacement_history":[]}}"#),
    )
}

fn noise(t: u32) -> String {
    line(
        t,
        "response_item",
        r#"{"type":"message","role":"user","content":[{"type":"input_text","text":"SECRET-OUTPUT function_call"}]}"#,
    )
}

fn rollout() -> Vec<String> {
    vec![
        meta("s1", "Claude Code", r#""vscode""#, None),
        noise(1),
        call(2, "c1", None, "exec"),
        exec_end(4, "c1", 0),
        output(4, "function_call_output", "c1"),
        call(5, "c2", Some("mcp__serena__"), "find_symbol"),
        end(
            7,
            r#"{"type":"mcp_tool_call_end","call_id":"c2","result":{"Err":"SECRET-OUTPUT"}}"#,
        ),
        output(7, "function_call_output", "c2"),
        call(8, "c3", Some("mcp__exa"), "web_search_exa"),
        end(
            9,
            r#"{"type":"mcp_tool_call_end","call_id":"c3","result":{"Ok":{"content":"SECRET-OUTPUT"}}}"#,
        ),
        output(9, "function_call_output", "c3"),
        custom_call(10, "c4", "apply_patch"),
        end(
            11,
            r#"{"type":"patch_apply_end","call_id":"c4","success":false,"status":"failed","stdout":"SECRET-OUTPUT"}"#,
        ),
        output(12, "custom_tool_call_output", "c4"),
        call(13, "c5", Some("collaboration"), "send_message"),
        output(14, "function_call_output", "c5"),
        compacted(15, Some("resp-1")),
        compacted(16, None),
        task_complete(20, "turn-1", 1500),
    ]
}

struct Parsed {
    facts: Vec<Fact>,
    ends: Vec<FactEnd>,
    agents: Vec<AgentMeta>,
}

fn logfile(path: &Path) -> LogFile {
    LogFile {
        path: path.to_path_buf(),
        rel: "sessions/x.jsonl".into(),
        account: "codex".into(),
        tool: Tool::Codex,
    }
}

fn parse(lines: &[String]) -> Parsed {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-a.jsonl");
    write_lines(&path, lines);
    let mut out = Parsed {
        facts: vec![],
        ends: vec![],
        agents: vec![],
    };
    CodexCollector
        .parse(
            &logfile(&path),
            0,
            &mut ParserState::default(),
            &mut |r| match r {
                ParsedRecord::Fact(f) => out.facts.push(f),
                ParsedRecord::FactEnd(e) => out.ends.push(e),
                ParsedRecord::Agent(a) => out.agents.push(a),
                _ => {}
            },
        )
        .unwrap();
    out
}

fn names(p: &Parsed, kind: FactKind) -> Vec<String> {
    p.facts
        .iter()
        .filter(|f| f.kind == Some(kind))
        .map(|f| f.name.clone())
        .collect()
}

#[test]
fn calls_become_tool_facts_with_normalized_mcp_names() {
    let p = parse(&rollout());
    assert_eq!(
        names(&p, FactKind::ToolCall),
        [
            "exec",
            "mcp__serena__find_symbol",
            "mcp__exa__web_search_exa",
            "apply_patch",
            "send_message"
        ]
    );
    let first = &p.facts[0];
    assert_eq!(first.key, "s1|tool_call|c1");
    assert_eq!(first.cwd.as_deref(), Some("/work/alpha"));
    assert_eq!(first.git_branch.as_deref(), Some("feat/x"));
    assert!(p.facts.iter().all(|f| f.tool_session_id == "s1"));
}

#[test]
fn errors_come_only_from_structured_end_events() {
    let p = parse(&rollout());
    let err = |key: &str| {
        p.ends
            .iter()
            .filter(|e| e.key == format!("s1|tool_call|{key}"))
            .any(|e| e.is_error)
    };
    assert!(!err("c1") && !err("c3") && !err("c5"));
    assert!(err("c2") && err("c4"));
    let failed = parse(&[
        meta("s1", "codex-tui", r#""cli""#, None),
        call(1, "c1", None, "exec_command"),
        exec_end(2, "c1", 1),
    ]);
    assert!(failed.ends.iter().any(|e| e.is_error));
}

#[test]
fn turn_and_compact_facts_and_agent_meta() {
    let p = parse(&rollout());
    let turn = p
        .facts
        .iter()
        .find(|f| f.kind == Some(FactKind::Turn))
        .unwrap();
    assert_eq!(turn.key, "s1|turn|turn-1");
    assert_eq!(turn.duration_ms, Some(1500));
    let keys: Vec<_> = p
        .facts
        .iter()
        .filter(|f| f.kind == Some(FactKind::Compact))
        .map(|f| f.key.as_str())
        .collect();
    assert_eq!(keys, ["s1|compact|resp-1", "s1|compact|n2"]);
    assert_eq!(p.agents.len(), 1);
    let a = &p.agents[0];
    assert_eq!(a.originator.as_deref(), Some("Claude Code"));
    assert_eq!(a.source.as_deref(), Some("vscode"));
    assert_eq!(a.version.as_deref(), Some("0.150.0"));
    assert_eq!(a.agent_id, None);
    assert_eq!(a.tool_session_id, "s1");
}

#[test]
fn object_source_uses_its_single_key() {
    let p = parse(&[meta(
        "s2",
        "codex-tui",
        r#"{"subagent":{"thread_spawn":{"depth":1}}}"#,
        Some("s1"),
    )]);
    assert_eq!(p.agents[0].source.as_deref(), Some("subagent"));
    assert_eq!(p.agents[0].originator.as_deref(), Some("codex-tui"));
}

#[test]
fn replayed_prefix_of_a_child_session_adds_no_facts() {
    let p = parse(&[
        meta("child", "codex-tui", r#""cli""#, Some("parent")),
        call(1, "old", None, "exec"),
        output(2, "function_call_output", "old"),
        task_complete(3, "old-turn", 900),
        compacted(4, Some("old-resp")),
        task_started(5),
        call(6, "new", None, "wait"),
        output(8, "function_call_output", "new"),
        task_complete(9, "new-turn", 400),
    ]);
    assert_eq!(names(&p, FactKind::ToolCall), ["wait"]);
    assert_eq!(p.ends.len(), 1);
    assert_eq!(names(&p, FactKind::Turn).len(), 1);
    assert!(names(&p, FactKind::Compact).is_empty());
}

fn account(root: &Path) -> Account {
    Account {
        tool: Tool::Codex,
        label: "codex".into(),
        root: root.to_path_buf(),
    }
}

fn stored(store: &tmgr_usage::Store) -> Vec<tmgr_usage::facts::StoredFact> {
    store
        .facts(now() - chrono::Duration::days(30), now())
        .unwrap()
}

#[test]
fn facts_are_idempotent_incremental_and_private() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("codex");
    let path = root.join("sessions/2026/10/14/rollout-a.jsonl");
    let lines = rollout();
    write_lines(&path, &lines[..12]);
    let mut store = open_store(tmp.path());
    let acc = account(&root);
    scan_account(&mut store, &acc, &book(), now()).unwrap();
    scan_account(&mut store, &acc, &book(), now()).unwrap();
    assert_eq!(stored(&store).len(), 4);

    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    f.write_all(format!("{}\n", lines[12..].join("\n")).as_bytes())
        .unwrap();
    scan_account(&mut store, &acc, &book(), now()).unwrap();
    scan_account(&mut store, &acc, &book(), now()).unwrap();

    let facts = stored(&store);
    assert_eq!(facts.len(), 5 + 2 + 1);
    let by = |id: &str| facts.iter().find(|f| f.call_id == id).unwrap();
    assert_eq!(by("c1").name, "exec");
    assert_eq!(by("c1").duration_ms, Some(2000));
    assert!(!by("c1").is_error);
    assert!(by("c2").is_error);
    assert_eq!(by("c2").name, "mcp__serena__find_symbol");
    assert!(by("c4").is_error);
    assert_eq!(by("c5").duration_ms, Some(1000));
    assert_eq!(by("turn-1").duration_ms, Some(1500));

    let agents = store.agents().unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].originator.as_deref(), Some("Claude_Code"));

    drop(store);
    for entry in std::fs::read_dir(tmp.path()).unwrap() {
        let p = entry.unwrap().path();
        if p.is_file() {
            let bytes = std::fs::read(&p).unwrap();
            for s in SECRETS {
                assert!(
                    !bytes.windows(s.len()).any(|w| w == s.as_bytes()),
                    "{s} leaked into {p:?}"
                );
            }
        }
    }
}
