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
