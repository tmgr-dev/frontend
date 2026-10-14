mod common;
use common::*;
use tmgr_usage::{PriceBook, Tokens};

fn tokens(input: u64, output: u64, cache_read: u64) -> Tokens {
    Tokens {
        input,
        output,
        cache_read,
        ..Tokens::default()
    }
}

fn close(a: Option<f64>, b: f64) {
    assert!((a.unwrap() - b).abs() < 1e-9, "{a:?} != {b}");
}

#[test]
fn lookup_normalises_names() {
    let b = book();
    for name in [
        "claude-test-opus-1",
        "Claude-Test-Opus-1",
        "anthropic/claude-test-opus-1",
        "claude-test-opus-1-20260101",
        "claude-test-opus-1-2026-01-01",
        "claude-test-opus-1[1m]",
        "claude-test-opus-1-latest",
        "anthropic.claude-test-opus-1-v1:0",
    ] {
        assert!(b.lookup(name).is_some(), "{name}");
    }
    assert!(b.lookup("claude-test-opus-2").is_none());
    assert!(b.lookup("").is_none());
}

#[test]
fn aliases_resolve_to_entries() {
    let mut b = book();
    b.set_alias("my-opus", "claude-test-opus-1");
    assert!(b.lookup("My-Opus-20260101").is_some());
}

#[test]
fn long_context_applies_to_the_whole_request_above_the_threshold() {
    let b = book();
    close(
        b.cost("claude-test-opus-1", &tokens(200_000, 0, 0), None),
        2.0,
    );
    close(
        b.cost("claude-test-opus-1", &tokens(200_001, 0, 0), None),
        200_001.0 * 2e-5,
    );
    close(
        b.cost("claude-test-opus-1", &tokens(100_000, 10, 100_000), None),
        100_000.0 * 1e-5 + 10.0 * 5e-5 + 100_000.0 * 1e-6,
    );
    close(
        b.cost("claude-test-opus-1", &tokens(100_000, 10, 100_001), None),
        100_000.0 * 2e-5 + 10.0 * 7.5e-5 + 100_001.0 * 2e-6,
    );
}

#[test]
fn one_hour_cache_writes_use_their_own_rate() {
    let b = book();
    let t = Tokens {
        cache_write_5m: 1000,
        cache_write_1h: 1000,
        ..Tokens::default()
    };
    close(
        b.cost("claude-test-opus-1", &t, None),
        1000.0 * 1.25e-5 + 1000.0 * 2e-5,
    );
}

#[test]
fn service_tiers_pick_their_keys_and_fall_back() {
    let json = r#"{"gpt-test": {"mode":"responses","input_cost_per_token":1e-6,"output_cost_per_token":8e-6,
        "cache_read_input_token_cost":1e-7,"input_cost_per_token_priority":2e-6,"output_cost_per_token_priority":16e-6,
        "input_cost_per_token_above_272k_tokens":2e-6,"output_cost_per_token_above_272k_tokens":12e-6}}"#;
    let b = PriceBook::from_litellm_json(json.as_bytes()).unwrap();
    let t = tokens(1000, 1000, 0);
    close(b.cost("gpt-test", &t, None), 1000.0 * 1e-6 + 1000.0 * 8e-6);
    close(
        b.cost("gpt-test", &t, Some("priority")),
        1000.0 * 2e-6 + 1000.0 * 16e-6,
    );
    close(
        b.cost("gpt-test", &t, Some("standard")),
        1000.0 * 1e-6 + 1000.0 * 8e-6,
    );
    let long = tokens(272_001, 10, 0);
    close(
        b.cost("gpt-test", &long, None),
        272_001.0 * 2e-6 + 10.0 * 12e-6,
    );
    close(
        b.cost("gpt-test", &tokens(10, 0, 100), None),
        10.0 * 1e-6 + 100.0 * 1e-7,
    );
}

#[test]
fn litellm_filter_keeps_only_wanted_models_and_cost_keys() {
    let json = r#"{
        "claude-a": {"mode":"chat","input_cost_per_token":1e-6,"output_cost_per_token":2e-6,"litellm_provider":"anthropic","supports_vision":true},
        "gpt-b": {"mode":"chat","input_cost_per_token":1e-6,"output_cost_per_token":2e-6},
        "o3": {"mode":"responses","input_cost_per_token":1e-6,"output_cost_per_token":2e-6},
        "codex-mini": {"input_cost_per_token":1e-6,"output_cost_per_token":2e-6},
        "gemini-x": {"mode":"chat","input_cost_per_token":1e-6,"output_cost_per_token":2e-6},
        "openai/gpt-c": {"mode":"chat","input_cost_per_token":1e-6,"output_cost_per_token":2e-6},
        "gpt-embed": {"mode":"embedding","input_cost_per_token":1e-6,"output_cost_per_token":2e-6},
        "gpt-noout": {"mode":"chat","input_cost_per_token":1e-6},
        "ollama": {"mode":"chat","input_cost_per_token":1e-6,"output_cost_per_token":2e-6}
    }"#;
    let b = PriceBook::from_litellm_json(json.as_bytes()).unwrap();
    let keys: Vec<_> = b.entries().keys().map(String::as_str).collect();
    assert_eq!(keys, ["claude-a", "codex-mini", "gpt-b", "o3"]);
    let entry = b.lookup("claude-a").unwrap();
    assert_eq!(entry.costs.len(), 2);
    assert!(PriceBook::from_litellm_json(b"{}").is_err());
    assert!(PriceBook::from_litellm_json(b"nope").is_err());
}

#[test]
fn bundled_snapshot_prices_known_models() {
    let b = PriceBook::bundled();
    assert!(b.len() > 50);
    let t = tokens(1_000_000, 1_000_000, 1_000_000);
    for model in [
        "claude-opus-4-5",
        "claude-sonnet-4-5-20250929",
        "gpt-5",
        "gpt-5-codex",
        "o3",
    ] {
        let cost = b.cost(model, &t, None).unwrap_or_else(|| panic!("{model}"));
        assert!(cost > 0.0);
    }
    assert!(b.cost("totally-unknown-model", &t, None).is_none());
}

const FAST_PRICES: &str = r#"{
  "claude-test-fast-1": {
    "mode": "chat",
    "input_cost_per_token": 1e-5,
    "output_cost_per_token": 5e-5,
    "provider_specific_entry": {"us": 1.1, "fast": 2.0}
  },
  "claude-test-slow-1": {
    "mode": "chat",
    "input_cost_per_token": 1e-5,
    "output_cost_per_token": 5e-5,
    "provider_specific_entry": {"us": 1.1}
  }
}"#;

fn event(model: &str, speed: Option<&str>) -> tmgr_usage::UsageEvent {
    tmgr_usage::UsageEvent {
        account: "claude".into(),
        tool_session_id: "s".into(),
        event_key: "k".into(),
        ts: now(),
        model: model.into(),
        tokens: tokens(1000, 100, 0),
        speed: speed.map(str::to_string),
        service_tier: None,
        git_branch: None,
        cwd: None,
        is_sidechain: false,
    }
}

#[test]
fn fast_mode_multiplies_the_whole_event_cost_and_regional_pricing_is_ignored() {
    let b = PriceBook::from_litellm_json(FAST_PRICES.as_bytes()).unwrap();
    let base = 1000.0 * 1e-5 + 100.0 * 5e-5;
    close(b.price(&event("claude-test-fast-1", None)).0, base);
    close(
        b.price(&event("claude-test-fast-1", Some("standard"))).0,
        base,
    );
    close(
        b.price(&event("claude-test-fast-1", Some("fast"))).0,
        base * 2.0,
    );
    close(b.price(&event("claude-test-slow-1", Some("fast"))).0, base);
}

#[test]
fn the_bundled_snapshot_knows_the_fast_multiplier() {
    let b = PriceBook::bundled();
    let fast = b.price(&event("claude-opus-5", Some("fast"))).0.unwrap();
    let plain = b.price(&event("claude-opus-5", None)).0.unwrap();
    assert!((fast / plain - 2.0).abs() < 1e-9);
}

#[test]
fn reprice_applies_the_fast_multiplier_to_stored_fast_events() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("claude");
    let file = home.join("projects/p/s1.jsonl");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(
        &file,
        concat!(
            r#"{"type":"assistant","sessionId":"s1","timestamp":"2026-10-14T10:00:00Z","cwd":"/w/a","requestId":"r1","message":{"id":"m1","model":"claude-test-fast-1","usage":{"input_tokens":1000,"output_tokens":100,"speed":"fast"}}}"#,
            "\n"
        ),
    )
    .unwrap();
    let plain = FAST_PRICES.replace(
        r#""provider_specific_entry": {"us": 1.1, "fast": 2.0}"#,
        r#""x": 1"#,
    );
    let before = PriceBook::from_litellm_json(plain.as_bytes()).unwrap();
    let mut store = open_store(tmp.path());
    tmgr_usage::scan_account(&mut store, &claude_account(&home), &before, now()).unwrap();
    let cost = |s: &tmgr_usage::Store| -> f64 {
        s.link_events(
            now() - chrono::Duration::days(30),
            now() + chrono::Duration::days(1),
        )
        .unwrap()[0]
            .cost_usd
            .unwrap()
    };
    let base = 1000.0 * 1e-5 + 100.0 * 5e-5;
    assert!((cost(&store) - base).abs() < 1e-9);
    let after = PriceBook::from_litellm_json(FAST_PRICES.as_bytes()).unwrap();
    assert_eq!(store.reprice(&after).unwrap(), 1);
    assert!((cost(&store) - base * 2.0).abs() < 1e-9);
}
