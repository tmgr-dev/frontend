mod common;
use chrono::{TimeZone, Utc};
use common::*;
use tmgr_usage::aggregate::{Filters, GroupBy, Query, Tz};
use tmgr_usage::scan_account;

#[test]
fn advisor_iterations_are_counted_under_their_own_model() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("claude/projects/p");
    std::fs::create_dir_all(&dir).unwrap();
    let line = r#"{"type":"assistant","sessionId":"s1","timestamp":"2026-10-01T10:00:00Z","cwd":"/w/p","requestId":"r1","message":{"id":"m1","model":"claude-sonnet-5-5","usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":0,"cache_creation_input_tokens":0,"iterations":[{"type":"message","input_tokens":10,"output_tokens":20},{"type":"advisor_message","model":"claude-opus-5-5","input_tokens":1000,"output_tokens":100,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}]}}}"#;
    std::fs::write(dir.join("s1.jsonl"), format!("{line}\n{line}\n")).unwrap();
    let mut store = open_store(tmp.path());
    let account = claude_account(&tmp.path().join("claude"));
    let report = scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!(report.events_new, 2);
    let q = Query {
        from: Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap(),
        to: Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap(),
        tz: Tz::Local,
        group_by: vec![GroupBy::Model],
        filters: Filters::default(),
    };
    let rep = store.aggregate(&q).unwrap();
    let find = |m: &str| rep.rows.iter().find(|r| r.keys[0] == m).unwrap();
    assert_eq!(find("claude-opus-5-5").tokens.input, 1000);
    assert_eq!(find("claude-sonnet-5-5").tokens.input, 10);
    assert_eq!(rep.totals.events, 2);
}
