mod common;
use chrono::{DateTime, TimeZone, Utc};
use common::*;
use std::path::Path;
use tmgr_usage::claude::hook_script_name;
use tmgr_usage::facts::StoredFact;
use tmgr_usage::{scan_account, FactKind, Store};

const SECRETS: [&str; 13] = [
    "SECRET-HOOK-CMD",
    "SECRET-STDOUT",
    "SECRET-STDERR",
    "SECRET-ARG",
    "SECRET-DESC",
    "SECRET-SNIPPET",
    "SECRET-PROMPT",
    "SECRET-RESPONSE",
    "SECRET-TOOL-RESULT",
    "SECRET-HOOK-CONTENT",
    "SECRET-URL",
    "/Users/someone",
    "some one",
];

fn wide() -> (DateTime<Utc>, DateTime<Utc>) {
    (
        Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 0).unwrap(),
    )
}

fn setup() -> (tempfile::TempDir, std::path::PathBuf, Store) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("claude");
    copy_dir(&ctx_fixture_dir(), &home);
    let store = open_store(tmp.path());
    (tmp, home, store)
}

fn facts(store: &Store, kind: FactKind) -> Vec<StoredFact> {
    let (from, to) = wide();
    store
        .facts(from, to)
        .unwrap()
        .into_iter()
        .filter(|f| f.kind == kind)
        .collect()
}

fn by_id<'a>(list: &'a [StoredFact], id: &str) -> &'a StoredFact {
    list.iter().find(|f| f.call_id == id).unwrap()
}

fn total_facts(store: &Store) -> usize {
    let (from, to) = wide();
    store.facts(from, to).unwrap().len()
}

#[test]
fn hook_script_names() {
    for (cmd, want) in [
        ("FOO=secret node /x/y/hook.js --key abc", "hook.js"),
        ("node /Users/someone/x/hook.js", "hook.js"),
        ("\"/Users/some one/bin/stop.sh\" arg", "stop.sh"),
        ("'/opt/my tools/run-it.py'", "run-it.py"),
        ("env A=1 B=2 bash ./scripts/pre.sh", "pre.sh"),
        ("sudo uvx tool-name --flag", "tool-name"),
        ("python3 -c \"print('SECRET')\"", "inline"),
        ("bash -c 'curl http://x/y'", "inline"),
        ("/x/.hidden", "inline"),
        ("C:\\tools\\hook.cmd /v", "hook.cmd"),
        ("$HOME/.claude/hooks/a.sh", "a.sh"),
        ("/x/we!rd name", "inline"),
        ("", "inline"),
        ("node", "inline"),
        ("FOO=bar", "inline"),
    ] {
        assert_eq!(hook_script_name(cmd), want, "{cmd}");
    }
    assert_eq!(
        hook_script_name(&format!("/x/{}", "a".repeat(65))),
        "inline"
    );
}

#[test]
fn facts_agents_and_events_are_extracted() {
    let (_tmp, home, mut store) = setup();
    let report = scan_account(&mut store, &claude_account(&home), &book(), now()).unwrap();
    assert_eq!(report.events_new, 4);
    assert_eq!(report.bad_lines, 0);

    let calls = facts(&store, FactKind::ToolCall);
    assert_eq!(calls.len(), 9);
    let bash = by_id(&calls, "tb1");
    assert_eq!(bash.name, "Bash");
    assert_eq!(bash.detail.as_deref(), Some("codex"));
    assert_eq!(bash.duration_ms, Some(2000));
    assert!(!bash.is_error);
    assert_eq!(bash.git_branch.as_deref(), Some("feat"));
    let edit = by_id(&calls, "te1");
    assert!(edit.is_error);
    assert_eq!(edit.duration_ms, Some(3000));
    let agent = by_id(&calls, "ta1");
    assert_eq!(agent.detail.as_deref(), Some("Explore"));
    assert_eq!(agent.target.as_deref(), Some("background"));
    assert_eq!(agent.duration_ms, Some(18000));
    let sub = by_id(&calls, "tsr1");
    assert_eq!(sub.agent_id.as_deref(), Some("a1"));
    assert!(sub.is_error);
    assert_eq!(by_id(&calls, "tr1").duration_ms, None);
    assert_eq!(by_id(&calls, "tr1").agent_id, None);

    let skills = facts(&store, FactKind::Skill);
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].name, "superpowers:brainstorming");

    let mut exts: Vec<_> = facts(&store, FactKind::FileEdit)
        .into_iter()
        .map(|f| f.name)
        .collect();
    exts.sort();
    assert_eq!(exts, ["ipynb", "none", "other", "rs"]);

    let hooks = facts(&store, FactKind::Hook);
    assert_eq!(hooks.len(), 2);
    let ok = hooks.iter().find(|h| h.name == "hook.js").unwrap();
    assert_eq!(ok.detail.as_deref(), Some("PostToolUse"));
    assert_eq!(ok.target.as_deref(), Some("Read"));
    assert_eq!(ok.duration_ms, Some(40));
    assert_eq!(ok.value, Some(0));
    assert!(!ok.is_error);
    let bad = hooks.iter().find(|h| h.name == "stop.sh").unwrap();
    assert_eq!(bad.detail.as_deref(), Some("Stop"));
    assert_eq!(bad.target, None);
    assert_eq!(bad.value, Some(2));
    assert!(bad.is_error);

    assert_eq!(facts(&store, FactKind::SkillListing)[0].value, Some(12));
    let turn = &facts(&store, FactKind::Turn)[0];
    assert_eq!((turn.duration_ms, turn.value), (Some(5000), Some(7)));
    assert_eq!(facts(&store, FactKind::Away).len(), 1);
    assert_eq!(facts(&store, FactKind::Compact)[0].value, Some(123456));
    let prs = facts(&store, FactKind::Pr);
    assert_eq!(prs.len(), 1);
    assert_eq!(
        (prs[0].name.as_str(), prs[0].value),
        ("acme/widgets", Some(42))
    );
    let slash = facts(&store, FactKind::Slash);
    assert_eq!(slash.len(), 1);
    assert_eq!(slash[0].name, "tm:go");

    let modes: Vec<_> = facts(&store, FactKind::PermissionMode)
        .into_iter()
        .map(|f| f.name)
        .collect();
    assert_eq!(modes, ["default", "bypassPermissions", "default"]);

    let agents = store.agents().unwrap();
    assert_eq!(agents.len(), 2);
    let main = agents.iter().find(|a| a.agent_id.is_none()).unwrap();
    assert_eq!(main.tool_session_id, "m1");
    assert_eq!(main.version.as_deref(), Some("2.1.90"));
    assert_eq!(main.entrypoint.as_deref(), Some("cli"));
    assert_eq!(main.agent_type, None);
    let node = agents
        .iter()
        .find(|a| a.agent_id.as_deref() == Some("a1"))
        .unwrap();
    assert_eq!(node.tool_session_id, "m1");
    assert_eq!(node.agent_type.as_deref(), Some("Explore"));
    assert_eq!(node.spawn_tool_use_id.as_deref(), Some("ta1"));
    assert_eq!(node.spawn_depth, Some(1));
    assert_eq!(node.parent_agent_id, None);

    let conn = rusqlite::Connection::open(home.parent().unwrap().join("usage.db")).unwrap();
    let mut stmt = conn
        .prepare("SELECT agent_id, COUNT(*) FROM usage_event GROUP BY agent_id ORDER BY agent_id")
        .unwrap();
    let rows: Vec<(Option<String>, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows, [(None, 3), (Some("a1".into()), 1)]);
}

fn assert_no_secrets(dir: &Path) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            assert_no_secrets(&path);
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        for secret in SECRETS {
            assert!(
                !bytes.windows(secret.len()).any(|w| w == secret.as_bytes()),
                "{secret} leaked into {}",
                path.file_name().unwrap().to_string_lossy()
            );
        }
    }
}

#[test]
fn no_secret_reaches_the_database() {
    let (tmp, home, mut store) = setup();
    scan_account(&mut store, &claude_account(&home), &book(), now()).unwrap();
    let data = tmp.path().to_path_buf();
    let db_files = |data: &Path| -> Vec<String> {
        std::fs::read_dir(data)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("usage.db"))
            .collect()
    };
    assert!(db_files(&data).iter().any(|n| n == "usage.db"));
    let planted = std::fs::read(ctx_fixture_dir().join("projects/-work-beta/m1.jsonl")).unwrap();
    for secret in &SECRETS[..11] {
        assert!(planted
            .windows(secret.len())
            .any(|w| w == secret.as_bytes()));
    }
    let snapshot = |data: &Path| {
        let copy = tempfile::tempdir().unwrap();
        for name in db_files(data) {
            std::fs::copy(data.join(&name), copy.path().join(&name)).unwrap();
        }
        copy
    };
    assert_no_secrets(snapshot(&data).path());
    drop(store);
    assert_no_secrets(snapshot(&data).path());
}

#[test]
fn rescans_do_not_duplicate_facts() {
    let (tmp, home, mut store) = setup();
    let account = claude_account(&home);
    scan_account(&mut store, &account, &book(), now()).unwrap();
    let before = total_facts(&store);
    assert_eq!(before, 9 + 1 + 4 + 2 + 1 + 1 + 1 + 1 + 1 + 1 + 3);
    scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!(total_facts(&store), before);

    let main = home.join("projects/-work-beta/m1.jsonl");
    let mut text = std::fs::read_to_string(&main).unwrap();
    text.push_str(
        r#"{"type":"permission-mode","permissionMode":"default","sessionId":"m1"}
{"type":"permission-mode","permissionMode":"plan","sessionId":"m1"}
{"type":"assistant","sessionId":"m1","uuid":"a9","timestamp":"2026-10-14T10:01:00Z","cwd":"/work/beta","gitBranch":"feat","message":{"id":"m9","model":"claude-test-opus-1","content":[{"type":"tool_use","id":"tn9","name":"Read","input":{}}],"usage":{"input_tokens":1,"output_tokens":1}},"requestId":"req-m9"}
{"type":"user","sessionId":"m1","uuid":"r9","timestamp":"2026-10-14T10:01:02Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tn9"}]}}
"#,
    );
    std::fs::write(&main, text).unwrap();
    scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!(total_facts(&store), before + 2);
    let modes: Vec<_> = facts(&store, FactKind::PermissionMode)
        .into_iter()
        .map(|f| f.name)
        .collect();
    assert_eq!(modes, ["default", "bypassPermissions", "default", "plan"]);
    let calls = facts(&store, FactKind::ToolCall);
    assert_eq!(by_id(&calls, "tn9").duration_ms, Some(2000));
    assert_eq!(store.agents().unwrap().len(), 2);

    let after = total_facts(&store);
    let conn = rusqlite::Connection::open(tmp.path().join("usage.db")).unwrap();
    conn.execute("UPDATE usage_source SET parser_version = 0", [])
        .unwrap();
    drop(conn);
    scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!(total_facts(&store), after);
    assert_eq!(store.agents().unwrap().len(), 2);
    assert_eq!(store.event_count().unwrap(), 5);
    let calls = facts(&store, FactKind::ToolCall);
    assert_eq!(by_id(&calls, "tb1").duration_ms, Some(2000));
}
