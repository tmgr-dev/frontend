mod common;
use common::*;
use std::io::Write;
use tmgr_usage::claude::ClaudeCollector;
use tmgr_usage::collector::{Collector, LogFile, ParseOutcome};
use tmgr_usage::{
    scan_account, scan_in_chunks, scan_with, Account, ParsedRecord, ParserState, Result, Tool,
    UsageError,
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

struct SlowParse {
    parsing: std::sync::mpsc::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

impl Collector for SlowParse {
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
        self.parsing.send(()).unwrap();
        self.release.recv().unwrap();
        ClaudeCollector.parse(file, from, state, sink)
    }
}

/// Starts a scan in another thread that blocks inside `parse` until `release` is sent.
fn slow_scan(
    dir: &std::path::Path,
    account: &Account,
) -> (
    std::thread::JoinHandle<tmgr_usage::ScanReport>,
    std::sync::mpsc::Sender<()>,
) {
    let (parsing_tx, parsing) = std::sync::mpsc::channel();
    let (release, release_rx) = std::sync::mpsc::channel();
    let (dir, account) = (dir.to_path_buf(), account.clone());
    let slow = std::thread::spawn(move || {
        let mut store = open_store(&dir);
        let c = SlowParse {
            parsing: parsing_tx,
            release: release_rx,
        };
        scan_with(&mut store, &c, &account, &book(), now()).unwrap()
    });
    parsing.recv().unwrap();
    (slow, release)
}

fn stored_offsets(dir: &std::path::Path) -> u64 {
    rusqlite::Connection::open(dir.join("usage.db"))
        .unwrap()
        .query_row("SELECT COUNT(*) FROM usage_file_state", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn reading_turned_off_during_a_parse_is_saved_at_once_and_stores_nothing() {
    use tmgr_usage::consent::{ConsentChange, ReadLogs};
    let (tmp, home, _store) = setup();
    write_lines(
        &projects_file(&home, "s.jsonl"),
        &[assistant_line("s", T, "m1", 10, 1)],
    );
    let (slow, release) = slow_scan(tmp.path(), &claude_account(&home));
    let other = open_store(tmp.path());
    let started = std::time::Instant::now();
    let deny = ConsentChange {
        state: Some(ReadLogs::Denied),
        ..ConsentChange::default()
    };
    let c = other.change_log_consent(&deny, now()).unwrap();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(c.state, ReadLogs::Denied);
    release.send(()).unwrap();
    let late = slow.join().unwrap();
    assert_eq!((late.events_new, late.files), (0, 0));
    assert_eq!(other.event_count().unwrap(), 0);
    assert_eq!(
        stored_offsets(tmp.path()),
        0,
        "the read position did not move"
    );
}

#[test]
fn racing_scans_store_a_file_once() {
    let (tmp, home, _store) = setup();
    write_lines(
        &projects_file(&home, "s.jsonl"),
        &[assistant_line("s", T, "m1", 10, 1)],
    );
    let account = claude_account(&home);
    let (slow, release) = slow_scan(tmp.path(), &account);
    let mut racing = open_store(tmp.path());
    let first = scan_account(&mut racing, &account, &book(), now()).unwrap();
    assert_eq!(first.events_new, 1);
    release.send(()).unwrap();
    let late = slow.join().unwrap();
    assert_eq!((late.events_new, late.files_unchanged), (0, 1));
    assert_eq!(racing.event_count().unwrap(), 1);
}

/// Counts the chunks and fails the one numbered `fail_at` (1-based; 0 = never).
struct Chunks {
    calls: std::cell::Cell<u32>,
    fail_at: u32,
}

impl Collector for Chunks {
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
        self.parse_until(file, from, u64::MAX, state, sink)
    }
    fn parse_until(
        &self,
        file: &LogFile,
        from: u64,
        until: u64,
        state: &mut ParserState,
        sink: &mut dyn FnMut(ParsedRecord),
    ) -> Result<ParseOutcome> {
        self.calls.set(self.calls.get() + 1);
        if self.calls.get() == self.fail_at {
            return Err(UsageError::Invalid("simulated crash".into()));
        }
        ClaudeCollector.parse_until(file, from, until, state, sink)
    }
}

fn six_lines(home: &std::path::Path) {
    let lines: Vec<String> = (1..=6)
        .map(|i| assistant_line("s", T, &format!("m{i}"), 10, 1))
        .collect();
    write_lines(&projects_file(home, "s.jsonl"), &lines);
}

#[test]
fn a_large_backlog_is_stored_in_several_chunks() {
    let (_tmp, home, mut store) = setup();
    six_lines(&home);
    let c = Chunks {
        calls: std::cell::Cell::new(0),
        fail_at: 0,
    };
    let account = claude_account(&home);
    let r = scan_in_chunks(&mut store, &c, &account, &book(), now(), 1).unwrap();
    assert_eq!((r.events_new, r.files, r.errors), (6, 1, 0));
    assert!(c.calls.get() >= 6, "{} chunks", c.calls.get());
    let again = scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!((again.files_unchanged, again.events_new), (1, 0));
}

#[test]
fn an_interrupted_scan_resumes_from_the_last_stored_chunk() {
    let (_tmp, home, mut store) = setup();
    six_lines(&home);
    let account = claude_account(&home);
    let c = Chunks {
        calls: std::cell::Cell::new(0),
        fail_at: 3,
    };
    let r = scan_in_chunks(&mut store, &c, &account, &book(), now(), 1).unwrap();
    assert_eq!((r.events_new, r.errors, r.files), (2, 1, 0));
    assert_eq!(store.event_count().unwrap(), 2);
    let resumed = scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!((resumed.events_new, resumed.events_updated), (4, 0));
    assert_eq!(store.event_count().unwrap(), 6);
    let again = scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!((again.files_unchanged, again.events_new), (1, 0));
}

#[cfg(unix)]
#[test]
fn an_unreadable_log_dir_is_a_scan_error_and_a_missing_root_is_not() {
    use std::os::unix::fs::PermissionsExt;
    let (_tmp, home, mut store) = setup();
    let locked = home.join("projects/p");
    write_lines(
        &locked.join("s.jsonl"),
        &[assistant_line("s", T, "m1", 10, 1)],
    );
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let readable_anyway = std::fs::read_dir(&locked).is_ok();
    let report = scan_account(&mut store, &claude_account(&home), &book(), now());
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
    if readable_anyway {
        return;
    }
    let report = report.unwrap();
    assert_eq!((report.errors, report.files), (1, 0));
    let gone = home.join("not-installed");
    let report = scan_account(&mut store, &claude_account(&gone), &book(), now()).unwrap();
    assert_eq!(report.errors, 0);
}
