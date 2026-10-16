use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use tmgr_usage::task::validate_json;

fn now() -> DateTime<Utc> {
    "2026-10-16T10:00:00Z".parse().unwrap()
}

fn tokens(n: u64) -> Value {
    json!({"input": n, "output": n, "cache_read": n, "cache_write_5m": n, "cache_write_1h": n, "reasoning": n})
}

fn sample() -> Value {
    json!({
        "schema": "task_usage.v1",
        "source_id": "k3J9x0Qe2bVt",
        "generated_at": "2026-10-16T09:59:00Z",
        "task": {"workspace": "tmgrdev", "ref": "TM-573", "id": null},
        "period": {"from": "2026-10-16T08:00:00Z", "to": "2026-10-16T09:00:00Z"},
        "totals": {"cost_usd": 18.4, "events": 412, "unpriced_events": 0, "tokens": tokens(1), "active_ms": 9300000, "turns": 31, "turn_ms": 7800000},
        "limits": [{"tool": "claude", "window": "week", "used_pct": 6.0, "approximate": true}],
        "nodes": [
            {"id": "n0", "parent_id": null, "depth": 0, "tool": "claude", "agent_type": "main", "model": "claude-opus-5-5", "origin": "cli", "link": null,
             "started_at": "2026-10-16T08:00:00Z", "ended_at": "2026-10-16T09:00:00Z", "duration_ms": 7800000, "cost_usd": 14.9, "events": 300, "tokens": tokens(2),
             "tools": [{"name": "Bash", "count": 42, "errors": 1, "duration_ms": 61000}]},
            {"id": "n1", "parent_id": "n0", "depth": 1, "tool": "codex", "agent_type": "codex", "model": "gpt-6-astra", "origin": "exec", "link": "likely",
             "started_at": "2026-10-16T08:10:00Z", "ended_at": "2026-10-16T08:20:00Z", "duration_ms": 1000, "cost_usd": 2.1, "events": 10, "tokens": tokens(0), "tools": []}
        ],
        "context": {
            "tools": [{"name": "Read", "count": 80, "errors": 0, "duration_ms": 9000}],
            "skills": [{"name": "superpowers:brainstorming", "count": 1}],
            "skills_available": 214,
            "plugins": [{"name": "superpowers", "count": 3}],
            "mcp": [{"server": "tmgr", "tool": "update_task", "count": 5, "errors": 0}],
            "hooks": [{"event": "PostToolUse", "name": "token-goat", "tool": "Read", "count": 80, "errors": 0, "duration_ms": 4000}],
            "slash_commands": [{"name": "tm", "count": 1}],
            "permission_modes": [{"mode": "bypassPermissions", "count": 1}],
            "compactions": 1, "away": 2,
            "file_edits": {"count": 17, "extensions": [{"name": "rs", "count": 15}]},
            "prs": [{"repo": "tmgr-dev/frontend", "number": 497}],
            "agents": [{"tool": "claude", "version": "2.1.90", "entrypoint": "cli", "count": 4}]
        },
        "truncated": {"nodes": false, "lists": false}
    })
}

fn changed(f: impl FnOnce(&mut Value)) -> Value {
    let mut v = sample();
    f(&mut v);
    v
}

fn accepted(label: &str, v: Value) {
    if let Err(e) = validate_json(&v, now()) {
        panic!("{label} should be accepted: {e:?}");
    }
}

fn rejected(label: &str, v: Value) {
    assert!(
        validate_json(&v, now()).is_err(),
        "{label} should be rejected"
    );
}

fn from_text(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn what_the_server_accepts_is_accepted() {
    accepted("the server sample", sample());
    accepted(
        "generated_at one minute ahead",
        changed(|v| v["generated_at"] = "2026-10-16T10:01:00Z".into()),
    );
    accepted(
        "model with a suffix",
        changed(|v| v["nodes"][0]["model"] = "claude@2026".into()),
    );
    accepted(
        "cost at the cap",
        changed(|v| v["totals"]["cost_usd"] = 1_000_000.into()),
    );
    accepted(
        "events at 2^53",
        changed(|v| v["totals"]["events"] = (1u64 << 53).into()),
    );
    accepted(
        "pr number at Integer.MAX_VALUE",
        changed(|v| v["context"]["prs"][0]["number"] = 2_147_483_647u64.into()),
    );
    accepted(
        "six decimals",
        changed(|v| v["totals"]["cost_usd"] = from_text("0.123456")),
    );
    accepted("depth 32", changed(|v| v["nodes"][1]["depth"] = 32.into()));
    accepted(
        "names with dots, colons and plus",
        changed(|v| v["context"]["skills"][0]["name"] = "a.b:c+d-e_f".into()),
    );
    accepted(
        "no list entries at all",
        changed(|v| {
            for k in [
                "tools",
                "skills",
                "plugins",
                "mcp",
                "hooks",
                "slash_commands",
                "permission_modes",
                "prs",
                "agents",
            ] {
                v["context"][k] = json!([]);
            }
            v["limits"] = json!([]);
        }),
    );
}

#[test]
fn list_budgets_follow_the_server_counting() {
    let named = |n: usize| -> Value {
        (0..n)
            .map(|i| json!({"name": format!("s{i}"), "count": 1}))
            .collect()
    };
    let limit = json!({"tool": "claude", "window": "week", "used_pct": 1, "approximate": false});
    accepted(
        "500 context entries",
        changed(|v| {
            for k in ["skills", "plugins", "slash_commands"] {
                v["context"][k] = named(100);
            }
            v["context"]["tools"] = (0..100)
                .map(
                    |i| json!({"name": format!("t{i}"), "count": 1, "errors": 0, "duration_ms": 0}),
                )
                .collect();
            v["context"]["mcp"] = (0..100)
                .map(|i| json!({"server": "s", "tool": format!("m{i}"), "count": 1, "errors": 0}))
                .collect();
            v["limits"] = json!([]);
            v["context"]["hooks"] = json!([]);
            v["context"]["permission_modes"] = json!([]);
            v["context"]["prs"] = json!([]);
            v["context"]["agents"] = json!([]);
            v["context"]["file_edits"]["extensions"] = json!([]);
        }),
    );
    rejected(
        "limits count against the 500 context entries",
        changed(|v| {
            for k in ["skills", "plugins", "slash_commands"] {
                v["context"][k] = named(100);
            }
            v["context"]["tools"] = (0..100)
                .map(
                    |i| json!({"name": format!("t{i}"), "count": 1, "errors": 0, "duration_ms": 0}),
                )
                .collect();
            v["context"]["mcp"] = (0..100)
                .map(|i| json!({"server": "s", "tool": format!("m{i}"), "count": 1, "errors": 0}))
                .collect();
            v["limits"] = json!([limit]);
            v["context"]["hooks"] = json!([]);
            v["context"]["permission_modes"] = json!([]);
            v["context"]["prs"] = json!([]);
            v["context"]["agents"] = json!([]);
            v["context"]["file_edits"]["extensions"] = json!([]);
        }),
    );
    let node = |i: usize, tools: usize| -> Value {
        json!({"id": format!("n{i}"), "parent_id": null, "depth": 0, "tool": "claude", "agent_type": "main", "model": null, "origin": null, "link": null,
               "started_at": "2026-10-16T08:00:00Z", "ended_at": "2026-10-16T08:00:00Z", "duration_ms": 0, "cost_usd": 0, "events": 0, "tokens": tokens(0),
               "tools": (0..tools).map(|t| json!({"name": format!("t{t}"), "count": 1, "errors": 0, "duration_ms": 0})).collect::<Vec<_>>()})
    };
    accepted(
        "2000 node tool entries are outside the 500",
        changed(|v| v["nodes"] = (0..20).map(|i| node(i, 100)).collect()),
    );
    rejected(
        "more than 2000 node tool entries",
        changed(|v| v["nodes"] = (0..21).map(|i| node(i, 96)).collect()),
    );
}

#[test]
fn what_the_server_rejects_is_rejected() {
    rejected(
        "extra root field",
        changed(|v| v["prompt"] = "write me a poem".into()),
    );
    rejected(
        "extra totals field",
        changed(|v| v["totals"]["extra"] = 1.into()),
    );
    rejected(
        "extra token field",
        changed(|v| v["totals"]["tokens"]["thinking"] = 1.into()),
    );
    rejected(
        "extra node field",
        changed(|v| v["nodes"][0]["title"] = "secret".into()),
    );
    rejected(
        "extra tool field",
        changed(|v| v["context"]["tools"][0]["args"] = "x".into()),
    );
    for (label, value) in [
        ("1.5", from_text("1.5")),
        ("1.0", from_text("1.0")),
        ("string", json!("412")),
        ("negative", json!(-1)),
        ("beyond 2^53", json!((1u64 << 53) + 1)),
    ] {
        rejected(
            &format!("events {label}"),
            changed(|v| v["totals"]["events"] = value),
        );
    }
    rejected(
        "negative token",
        changed(|v| v["totals"]["tokens"]["input"] = (-5).into()),
    );
    for (label, value) in [
        ("seven decimals", from_text("0.1234567")),
        ("float noise", from_text("0.30000000000000004")),
        ("negative", json!(-0.5)),
        ("string", json!("18.4")),
        ("over the cap", json!(2_000_000)),
    ] {
        rejected(
            &format!("cost {label}"),
            changed(|v| v["totals"]["cost_usd"] = value),
        );
    }
    rejected(
        "used_pct with seven decimals",
        changed(|v| v["limits"][0]["used_pct"] = from_text("0.1234567")),
    );
    for bad in [
        "a/b",
        "@scope",
        ".hidden",
        "..",
        "/Users/x/.claude/hooks/a.sh",
        "a b",
        "",
        "a..b",
    ] {
        rejected(
            &format!("tool name {bad:?}"),
            changed(|v| v["context"]["tools"][0]["name"] = bad.into()),
        );
    }
    rejected(
        "agent_type with a slash",
        changed(|v| v["nodes"][0]["agent_type"] = "a/b".into()),
    );
    for bad in [
        "../model",
        "alice@example.com",
        "a@b@c",
        "a..b",
        ".x",
        "home/alice",
        "a@",
    ] {
        rejected(
            &format!("model {bad:?}"),
            changed(|v| v["nodes"][0]["model"] = bad.into()),
        );
    }
    rejected(
        "model of 112 characters",
        changed(|v| v["nodes"][0]["model"] = "m".repeat(112).into()),
    );
    rejected(
        "@ in a skill name",
        changed(|v| v["context"]["skills"][0]["name"] = "skill@2".into()),
    );
    for bad in ["1..2", ".1", "1/2", "1@2", ""] {
        rejected(
            &format!("version {bad:?}"),
            changed(|v| v["context"]["agents"][0]["version"] = bad.into()),
        );
    }
    rejected(
        "extension with a dot",
        changed(|v| v["context"]["file_edits"]["extensions"][0]["name"] = "tar.gz".into()),
    );
    for bad in [
        "a/b/c",
        "https://github.com/a/b",
        "a..b/c",
        "a/b..c",
        ".a/b",
        "a/.b",
        "alice@example.com/private",
        "a/",
    ] {
        rejected(
            &format!("repo {bad:?}"),
            changed(|v| v["context"]["prs"][0]["repo"] = bad.into()),
        );
    }
    rejected(
        "pr number 2^31",
        changed(|v| v["context"]["prs"][0]["number"] = 2_147_483_648u64.into()),
    );
    rejected(
        "pr number 0",
        changed(|v| v["context"]["prs"][0]["number"] = 0.into()),
    );
    rejected(
        "parent after child",
        changed(|v| v["nodes"][0]["parent_id"] = "n1".into()),
    );
    rejected(
        "unknown parent",
        changed(|v| v["nodes"][1]["parent_id"] = "n9".into()),
    );
    rejected(
        "duplicate node id",
        changed(|v| v["nodes"][1]["id"] = "n0".into()),
    );
    rejected("depth 33", changed(|v| v["nodes"][1]["depth"] = 33.into()));
    rejected(
        "generated_at ten minutes ahead",
        changed(|v| v["generated_at"] = "2026-10-16T10:10:00Z".into()),
    );
    rejected(
        "period.to ahead",
        changed(|v| v["period"]["to"] = "2026-10-16T10:10:00Z".into()),
    );
    rejected(
        "node started_at ahead",
        changed(|v| v["nodes"][0]["started_at"] = "2026-10-16T10:10:00Z".into()),
    );
    rejected(
        "period.to before from",
        changed(|v| v["period"]["to"] = "2026-10-16T07:00:00Z".into()),
    );
    rejected(
        "201 nodes",
        changed(|v| {
            v["nodes"] = (0..201)
                .map(|i| {
                    let mut n = sample()["nodes"][0].clone();
                    n["id"] = format!("n{i}").into();
                    n
                })
                .collect();
        }),
    );
    rejected(
        "schema v2",
        changed(|v| v["schema"] = "task_usage.v2".into()),
    );
    rejected("not an object", json!([]));
    for bad in ["a.b", "a b", "a:b", ""] {
        rejected(
            &format!("workspace {bad:?}"),
            changed(|v| v["task"]["workspace"] = bad.into()),
        );
    }
    for bad in [
        "TM5",
        "TM-",
        "TM-1x",
        "5-1",
        "T.M-1",
        "TM-1234567890123456789",
    ] {
        rejected(
            &format!("task ref {bad:?}"),
            changed(|v| v["task"]["ref"] = bad.into()),
        );
    }
    rejected("task id 0", changed(|v| v["task"]["id"] = 0.into()));
}

#[test]
fn the_ref_pattern_of_the_server_takes_either_letter_case() {
    accepted(
        "lower case ref",
        changed(|v| v["task"]["ref"] = "tm-5".into()),
    );
    accepted(
        "long ref",
        changed(|v| v["task"]["ref"] = "ABC123-123456789012345678".into()),
    );
}
