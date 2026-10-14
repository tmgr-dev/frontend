mod common;
use common::*;
use std::io::Write;
use std::path::Path;
use tmgr_usage::aggregate::{Filters, Query, Tz};
use tmgr_usage::claude::ClaudeCollector;
use tmgr_usage::codex::CodexCollector;
use tmgr_usage::collector::{Collector, LogFile, ParseOutcome};
use tmgr_usage::{
    scan_account, scan_with, Account, ParsedRecord, ParserState, Result, Store, Tool,
};

struct Versioned<C> {
    inner: C,
    version: u32,
    tweak: fn(&mut ParsedRecord, &mut u32) -> bool,
}

impl<C: Collector> Collector for Versioned<C> {
    fn tool(&self) -> Tool {
        self.inner.tool()
    }
    fn parser_version(&self) -> u32 {
        self.version
    }
    fn discover(&self, account: &Account) -> Vec<LogFile> {
        self.inner.discover(account)
    }
    fn parse(
        &self,
        file: &LogFile,
        from: u64,
        state: &mut ParserState,
        sink: &mut dyn FnMut(ParsedRecord),
    ) -> Result<ParseOutcome> {
        let mut seen = 0;
        let tweak = self.tweak;
        self.inner.parse(file, from, state, &mut |mut rec| {
            if tweak(&mut rec, &mut seen) {
                sink(rec);
            }
        })
    }
}

fn keep(_: &mut ParsedRecord, _: &mut u32) -> bool {
    true
}

fn wrong_cache_split(rec: &mut ParsedRecord, _: &mut u32) -> bool {
    if let ParsedRecord::Event(ev) = rec {
        ev.tokens.input += ev.tokens.cache_read;
        ev.tokens.cache_read = 0;
    }
    true
}

fn drop_first_event(rec: &mut ParsedRecord, seen: &mut u32) -> bool {
    if matches!(rec, ParsedRecord::Event(_)) {
        *seen += 1;
        return *seen > 1;
    }
    true
}

fn claude_line(msg: &str, ts: &str, input: u64, cache_read: u64) -> String {
    format!(
        r#"{{"type":"assistant","sessionId":"s","timestamp":"{ts}","cwd":"/work/alpha","requestId":"req-{msg}","message":{{"id":"{msg}","model":"claude-test-opus-1","usage":{{"input_tokens":{input},"output_tokens":10,"cache_read_input_tokens":{cache_read},"cache_creation_input_tokens":0}}}}}}"#
    )
}

fn totals(store: &Store) -> tmgr_usage::Row {
    store
        .aggregate(&Query {
            from: chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 9, 1, 0, 0, 0).unwrap(),
            to: chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 11, 1, 0, 0, 0).unwrap(),
            tz: Tz::Fixed(chrono::FixedOffset::east_opt(0).unwrap()),
            group_by: vec![],
            filters: Filters::default(),
        })
        .unwrap()
        .totals
}

fn stored_version(dir: &Path, tool: &str) -> i64 {
    rusqlite::Connection::open(dir.join("usage.db"))
        .unwrap()
        .query_row(
            "SELECT parser_version FROM usage_source WHERE tool = ?1",
            [tool],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn a_parser_version_bump_corrects_stored_rows_without_duplicates() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("claude");
    let file = home.join("projects/p/s.jsonl");
    write_lines(
        &file,
        &[
            claude_line("m1", "2026-10-01T10:00:00Z", 100, 1000),
            claude_line("m2", "2026-10-01T10:05:00Z", 100, 1000),
        ],
    );
    let account = claude_account(&home);
    let mut store = open_store(tmp.path());
    let v1 = Versioned {
        inner: ClaudeCollector,
        version: 1,
        tweak: wrong_cache_split,
    };
    scan_with(&mut store, &v1, &account, &book(), now()).unwrap();
    let t = totals(&store);
    assert_eq!(
        (t.tokens.input, t.tokens.cache_read, t.events),
        (2200, 0, 2)
    );

    let v2 = Versioned {
        inner: ClaudeCollector,
        version: 2,
        tweak: keep,
    };
    let r = scan_with(&mut store, &v2, &account, &book(), now()).unwrap();
    assert_eq!((r.events_new, r.events_updated), (0, 2));
    let t = totals(&store);
    assert_eq!(
        (t.tokens.input, t.tokens.cache_read, t.events),
        (200, 2000, 2)
    );
    let first = store
        .link_events(
            chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 10, 1, 0, 0, 0).unwrap(),
            chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 10, 2, 0, 0, 0).unwrap(),
        )
        .unwrap();
    assert_eq!(first[0].ts.to_rfc3339(), "2026-10-01T10:00:00+00:00");
    assert!(first.iter().all(|e| e.cost_usd.is_some()));

    let again = scan_with(&mut store, &v2, &account, &book(), now()).unwrap();
    assert_eq!((again.files, again.events_updated), (0, 0));
}

#[test]
fn an_older_binary_neither_resets_nor_lowers_a_newer_parser_version() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("claude");
    let file = home.join("projects/p/s.jsonl");
    write_lines(
        &file,
        &[claude_line("m1", "2026-10-01T10:00:00Z", 100, 1000)],
    );
    let account = claude_account(&home);
    let mut store = open_store(tmp.path());
    let v2 = Versioned {
        inner: ClaudeCollector,
        version: 2,
        tweak: keep,
    };
    scan_with(&mut store, &v2, &account, &book(), now()).unwrap();
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap();
    writeln!(
        f,
        "{}",
        claude_line("m2", "2026-10-01T10:05:00Z", 100, 1000)
    )
    .unwrap();
    let v1 = Versioned {
        inner: ClaudeCollector,
        version: 1,
        tweak: wrong_cache_split,
    };
    let r = scan_with(&mut store, &v1, &account, &book(), now()).unwrap();
    assert_eq!((r.events_new, r.events_updated), (1, 0));
    assert_eq!(stored_version(tmp.path(), "claude"), 2);
    let t = totals(&store);
    assert_eq!((t.tokens.cache_read, t.events), (1000, 2));
}

#[test]
fn a_codex_version_bump_drops_events_the_new_parser_no_longer_emits() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("codex");
    let file = root.join("sessions/2026/10/14/rollout-a.jsonl");
    write_lines(
        &file,
        &[
            r#"{"timestamp":"2026-10-14T10:00:00Z","type":"session_meta","payload":{"id":"s1","cwd":"/w/a"}}"#.into(),
            r#"{"timestamp":"2026-10-14T10:00:01Z","type":"turn_context","payload":{"model":"gpt-a"}}"#.into(),
            r#"{"timestamp":"2026-10-14T10:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"output_tokens":1},"last_token_usage":null}}}"#.into(),
            r#"{"timestamp":"2026-10-14T10:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":30,"output_tokens":5},"last_token_usage":null}}}"#.into(),
        ],
    );
    let account = Account {
        tool: Tool::Codex,
        label: "codex".into(),
        root,
    };
    let mut store = open_store(tmp.path());
    let old = Versioned {
        inner: CodexCollector,
        version: 10,
        tweak: keep,
    };
    scan_with(&mut store, &old, &account, &book(), now()).unwrap();
    assert_eq!(store.event_count().unwrap(), 2);
    let new = Versioned {
        inner: CodexCollector,
        version: 11,
        tweak: drop_first_event,
    };
    scan_with(&mut store, &new, &account, &book(), now()).unwrap();
    assert_eq!(store.event_count().unwrap(), 1);
    let plain = scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!(plain.errors, 0);
}

#[test]
fn concurrent_scans_of_one_data_dir_never_fail_or_double_count() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("claude");
    for f in 0..40 {
        let lines: Vec<String> = (0..3)
            .map(|i| claude_line(&format!("m{f}-{i}"), "2026-10-01T10:00:00Z", 10, 0))
            .collect();
        write_lines(&home.join(format!("projects/p/s{f}.jsonl")), &lines);
    }
    drop(open_store(tmp.path()));
    let db = tmp.path().join("usage.db");
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let (db, home) = (db.clone(), home.clone());
            std::thread::spawn(move || {
                let mut store = Store::open(&db).unwrap();
                let account = claude_account(&home);
                let mut errors = 0;
                let mut new = 0;
                for _ in 0..4 {
                    let r = scan_account(&mut store, &account, &book(), now()).unwrap();
                    errors += r.errors;
                    new += r.events_new;
                }
                (errors, new)
            })
        })
        .collect();
    let results: Vec<(u64, u64)> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert!(results.iter().all(|r| r.0 == 0), "{results:?}");
    assert_eq!(results.iter().map(|r| r.1).sum::<u64>(), 120);
    let store = open_store(tmp.path());
    assert_eq!(store.event_count().unwrap(), 120);
}
