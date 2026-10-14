mod common;
use common::*;
use std::io::Write;
use tmgr_usage::claude::ClaudeCollector;
use tmgr_usage::collector::{Collector, LogFile, ParseOutcome};
use tmgr_usage::{
    scan_account, scan_with, Account, ParsedRecord, ParserState, Result, Tool, UsageError,
};

const T: &str = "2026-10-01T10:00:00Z";

fn setup() -> (tempfile::TempDir, std::path::PathBuf, tmgr_usage::Store) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("claude");
    std::fs::create_dir_all(home.join("projects")).unwrap();
    let store = open_store(tmp.path());
    (tmp, home, store)
}

fn append(path: &std::path::Path, text: &str) {
    let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    f.write_all(text.as_bytes()).unwrap();
}

#[test]
fn second_scan_without_changes_reads_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("claude");
    copy_dir(&fixture_dir(), &home);
    let mut store = open_store(tmp.path());
    let account = claude_account(&home);
    scan_account(&mut store, &account, &book(), now()).unwrap();
    let again = scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!(again.files, 0);
    assert_eq!(again.files_unchanged, 3);
    assert_eq!(again.events_new, 0);
    assert_eq!(again.events_updated, 0);
    assert_eq!(again.bytes, 0);
}

#[test]
fn append_parses_only_the_new_lines() {
    let (_tmp, home, mut store) = setup();
    let file = projects_file(&home, "a.jsonl");
    write_lines(&file, &[assistant_line("s", T, "m1", 1, 1)]);
    let account = claude_account(&home);
    let first = scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!(first.events_new, 1);
    let size_before = std::fs::metadata(&file).unwrap().len();
    append(&file, &format!("{}\n", assistant_line("s", T, "m2", 2, 2)));
    let second = scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!((second.files, second.events_new), (1, 1));
    assert_eq!(
        second.bytes,
        std::fs::metadata(&file).unwrap().len() - size_before
    );
    assert_eq!(store.event_count().unwrap(), 2);
}

#[test]
fn partial_trailing_line_waits_for_its_newline() {
    let (_tmp, home, mut store) = setup();
    let file = projects_file(&home, "a.jsonl");
    let full = assistant_line("s", T, "m2", 2, 2);
    let (head, tail) = full.split_at(full.len() / 2);
    write_lines(&file, &[assistant_line("s", T, "m1", 1, 1)]);
    append(&file, head);
    let account = claude_account(&home);
    let first = scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!((first.events_new, first.errors), (1, 0));
    append(&file, &format!("{tail}\n"));
    let second = scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!((second.events_new, second.errors), (1, 0));
}

#[test]
fn oversized_line_is_skipped_and_counted_once() {
    let (_tmp, home, mut store) = setup();
    let file = projects_file(&home, "a.jsonl");
    let big = format!(
        r#"{{"type":"assistant","pad":"{}"}}"#,
        "x".repeat(4 * 1024 * 1024 + 10)
    );
    write_lines(&file, &[big, assistant_line("s", T, "m1", 1, 1)]);
    let account = claude_account(&home);
    let report = scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!((report.skipped_oversized, report.events_new), (1, 1));
    let again = scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!((again.skipped_oversized, again.events_new), (0, 0));
}

#[test]
fn malformed_lines_are_counted_without_stopping_the_scan() {
    let (_tmp, home, mut store) = setup();
    let file = projects_file(&home, "a.jsonl");
    write_lines(
        &file,
        &[
            r#"{"type":"assistant","message":"SECRET-BAD"#.to_string() + "\"}}x",
            assistant_line("s", T, "m1", 1, 1),
        ],
    );
    let report = scan_account(&mut store, &claude_account(&home), &book(), now()).unwrap();
    assert_eq!(
        (report.bad_lines, report.errors, report.events_new),
        (1, 0, 1)
    );
}

#[test]
fn rotated_file_is_rescanned_without_double_counting() {
    let (_tmp, home, mut store) = setup();
    let file = projects_file(&home, "a.jsonl");
    write_lines(
        &file,
        &[
            assistant_line("s", T, "m1", 1, 1),
            assistant_line("s", T, "m2", 2, 2),
            assistant_line("s", T, "m3", 3, 3),
        ],
    );
    let account = claude_account(&home);
    assert_eq!(
        scan_account(&mut store, &account, &book(), now())
            .unwrap()
            .events_new,
        3
    );

    write_lines(
        &file,
        &[
            assistant_line("s", T, "m1", 1, 1),
            assistant_line("s", T, "m4", 4, 4),
        ],
    );
    let shrunk = scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!(shrunk.events_new, 1);

    let longer: Vec<String> = ["m5", "m6", "m7", "m8", "m9"]
        .iter()
        .map(|m| assistant_line("other", T, m, 9, 9))
        .collect();
    write_lines(&file, &longer);
    let grown = scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!(grown.events_new, 5);
    assert_eq!(store.event_count().unwrap(), 9);
}

struct FailAfterParse;

impl Collector for FailAfterParse {
    fn tool(&self) -> Tool {
        Tool::Claude
    }
    fn parser_version(&self) -> u32 {
        ClaudeCollector.parser_version()
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
        ClaudeCollector.parse(file, from, state, sink)?;
        Err(UsageError::Invalid("simulated crash".into()))
    }
}

#[test]
fn events_and_offset_commit_together() {
    let (_tmp, home, mut store) = setup();
    let file = projects_file(&home, "a.jsonl");
    write_lines(
        &file,
        &[
            assistant_line("s", T, "m1", 1, 1),
            assistant_line("s", T, "m2", 2, 2),
        ],
    );
    let account = claude_account(&home);
    let failed = scan_with(&mut store, &FailAfterParse, &account, &book(), now()).unwrap();
    assert_eq!((failed.errors, failed.files), (1, 0));
    assert_eq!(store.event_count().unwrap(), 0);
    let healed = scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!((healed.events_new, healed.errors), (2, 0));
}

#[cfg(unix)]
#[test]
fn symlinks_are_not_followed() {
    let (tmp, home, mut store) = setup();
    let outside = tmp.path().join("outside");
    write_lines(
        &outside.join("x.jsonl"),
        &[assistant_line("s", T, "m1", 1, 1)],
    );
    let projects = home.join("projects");
    std::os::unix::fs::symlink(&outside, projects.join("linked")).unwrap();
    std::os::unix::fs::symlink(outside.join("x.jsonl"), projects.join("x.jsonl")).unwrap();
    let report = scan_account(&mut store, &claude_account(&home), &book(), now()).unwrap();
    assert_eq!((report.files, report.events_new), (0, 0));
}

#[test]
fn missing_account_root_scans_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = open_store(tmp.path());
    let account = claude_account(&tmp.path().join("nope"));
    let report = scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!(report.files, 0);
}
