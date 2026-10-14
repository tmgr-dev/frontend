mod common;
use common::*;
use tmgr_usage::codex::CodexCollector;
use tmgr_usage::limits::LimitWindow;
use tmgr_usage::link::{attribute, LinkReport};
use tmgr_usage::statusline::{plan_install, read_snapshots, write_snapshot};
use tmgr_usage::{scan_account, Account, Collector, Tool, UsageError};

#[test]
fn stubs_return_empty_or_unsupported() {
    assert!(matches!(
        write_snapshot(std::path::Path::new("."), None, b"{}"),
        Err(UsageError::Unsupported)
    ));
    assert!(matches!(
        plan_install("{}", "tmgr", None),
        Err(UsageError::Unsupported)
    ));
    let tmp = tempfile::tempdir().unwrap();
    assert!(read_snapshots(tmp.path()).unwrap().is_empty());
    assert_eq!(attribute(&[], &[]), LinkReport::default());
}

#[test]
fn codex_account_scans_nothing_until_implemented() {
    let tmp = tempfile::tempdir().unwrap();
    let account = Account {
        tool: Tool::Codex,
        label: "codex".into(),
        root: tmp.path().to_path_buf(),
    };
    assert!(CodexCollector.discover(&account).is_empty());
    let mut store = open_store(tmp.path());
    let report = scan_account(&mut store, &account, &book(), now()).unwrap();
    assert_eq!(report.files, 0);
}

#[test]
fn limit_window_keys_round_trip() {
    for w in [
        LimitWindow::Session,
        LimitWindow::Weekly,
        LimitWindow::Model("fable".into()),
    ] {
        assert_eq!(LimitWindow::from_key(&w.as_key()), Some(w));
    }
    assert_eq!(LimitWindow::from_key("nope"), None);
}
