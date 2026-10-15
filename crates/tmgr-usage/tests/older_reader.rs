mod common;
use common::*;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use tmgr_usage::claude::{ClaudeCollector, PARSER_VERSION};
use tmgr_usage::collector::{Collector, LogFile, ParseOutcome};
use tmgr_usage::{scan_with, Account, ParsedRecord, ParserState, Result, Store, Tool};

struct OlderBinary;

impl Collector for OlderBinary {
    fn tool(&self) -> Tool {
        Tool::Claude
    }

    fn parser_version(&self) -> u32 {
        PARSER_VERSION
    }

    fn discover(&self, account: &Account) -> Vec<LogFile> {
        ClaudeCollector.discover(account)
    }

    fn parse(
        &self,
        file: &LogFile,
        from: u64,
        state: &mut ParserState,
        sink: &mut dyn FnMut(ParsedRecord),
    ) -> Result<ParseOutcome> {
        ClaudeCollector.parse(file, from, state, &mut |rec| {
            if let ParsedRecord::Event(mut ev) = rec {
                ev.agent_id = None;
                sink(ParsedRecord::Event(ev));
            }
        })
    }
}

fn jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            jsonl(&path, out);
        } else if path.extension().is_some_and(|e| e == "jsonl") {
            out.push(path);
        }
    }
}

fn split_fixture(home: &Path) -> Vec<(PathBuf, String)> {
    copy_dir(&fixture_dir(), home);
    let mut files = Vec::new();
    jsonl(home, &mut files);
    let mut tails = Vec::new();
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        let (head, tail) = lines.split_at(lines.len() / 2);
        let mut head = head.join("\n");
        if !head.is_empty() {
            head.push('\n');
        }
        std::fs::write(&path, head).unwrap();
        tails.push((path, format!("{}\n", tail.join("\n"))));
    }
    tails
}

fn append_tails(tails: &[(PathBuf, String)]) {
    for (path, tail) in tails {
        let mut text = std::fs::read_to_string(path).unwrap();
        text.push_str(tail);
        std::fs::write(path, text).unwrap();
    }
}

fn counts(db: &Path) -> (i64, i64, i64, i64) {
    let conn = Connection::open(db).unwrap();
    let q = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
    (
        q("SELECT COUNT(*) FROM usage_event"),
        q("SELECT COUNT(*) FROM usage_event WHERE agent_id IS NOT NULL"),
        q("SELECT COUNT(*) FROM usage_fact"),
        q("SELECT COUNT(*) FROM usage_agent"),
    )
}

fn context_rows(db: &Path) -> Vec<(i64, String, i64)> {
    let conn = Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare("SELECT source_id, file_key, byte_offset FROM usage_context_file_state")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

fn restore_context_rows(db: &Path, rows: &[(i64, String, i64)]) {
    let conn = Connection::open(db).unwrap();
    conn.execute("DELETE FROM usage_context_file_state", [])
        .unwrap();
    for (source, key, offset) in rows {
        conn.execute(
            "INSERT INTO usage_context_file_state VALUES (?1, ?2, ?3)",
            rusqlite::params![source, key, offset],
        )
        .unwrap();
    }
}

#[test]
fn lines_an_older_binary_read_are_read_again_for_facts_without_duplicate_events() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("claude");
    let tails = split_fixture(&home);
    let db = tmp.path().join("usage.db");
    let account = claude_account(&home);
    let mut store = Store::open(&db).unwrap();
    scan_with(&mut store, &ClaudeCollector, &account, &book(), now()).unwrap();
    let before_old = context_rows(&db);
    append_tails(&tails);
    scan_with(&mut store, &OlderBinary, &account, &book(), now()).unwrap();
    restore_context_rows(&db, &before_old);
    let after_old = counts(&db);
    scan_with(&mut store, &ClaudeCollector, &account, &book(), now()).unwrap();
    let mixed = counts(&db);

    let fresh_tmp = tempfile::tempdir().unwrap();
    let fresh_home = fresh_tmp.path().join("claude");
    copy_dir(&fixture_dir(), &fresh_home);
    let fresh_db = fresh_tmp.path().join("usage.db");
    let mut fresh = Store::open(&fresh_db).unwrap();
    scan_with(
        &mut fresh,
        &ClaudeCollector,
        &claude_account(&fresh_home),
        &book(),
        now(),
    )
    .unwrap();
    let expected = counts(&fresh_db);

    assert_eq!(after_old.0, expected.0);
    assert_eq!(mixed, expected);
}
