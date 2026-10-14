mod common;
use common::*;
use std::path::{Path, PathBuf};
use tmgr_usage::paths::{add_account, Env};
use tmgr_usage::{scan_account, Account, Tool, UsageError};

const SECRETS: [&str; 5] = [
    "SECRET-PROMPT-1",
    "SECRET-RESPONSE-1",
    "SECRET-TOOL-1",
    "SECRET-OUT-1",
    "SECRET-BASEINSTR",
];

fn all_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            all_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn assert_clean(data_dir: &Path) -> usize {
    let mut files = Vec::new();
    all_files(data_dir, &mut files);
    for file in &files {
        let bytes = std::fs::read(file).unwrap();
        for (i, secret) in SECRETS.iter().enumerate() {
            let hit = bytes.windows(secret.len()).any(|w| w == secret.as_bytes());
            assert!(!hit, "secret #{i} leaked into a data file");
        }
    }
    files.len()
}

#[test]
fn no_content_reaches_the_data_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("claude");
    copy_dir(&fixture_dir(), &home);
    let data = tmp.path().join("data");
    let env = Env {
        home: Some(tmp.path().to_path_buf()),
        vars: Default::default(),
    };
    add_account(
        &data,
        &env,
        Account {
            tool: Tool::Claude,
            label: "work".into(),
            root: home.clone(),
        },
    )
    .unwrap();

    let mut store = open_store(&data);
    let report = scan_account(&mut store, &claude_account(&home), &book(), now()).unwrap();
    assert_eq!(report.events_new, 8);

    let open_files = assert_clean(&data);
    assert!(open_files >= 2);

    store.prune(7, now()).unwrap();
    store.save_prices(&book(), "test", now()).unwrap();
    assert_clean(&data);
    drop(store);
    assert_clean(&data);
}

#[test]
fn the_fixtures_really_contain_the_secrets() {
    let mut files = Vec::new();
    all_files(&fixture_dir(), &mut files);
    for secret in SECRETS {
        let found = files.iter().any(|f| {
            let bytes = std::fs::read(f).unwrap();
            bytes.windows(secret.len()).any(|w| w == secret.as_bytes())
        });
        assert!(found, "fixtures lost a planted secret");
    }
}

#[test]
fn errors_never_carry_input_text() {
    let err: UsageError = serde_json::from_str::<u64>("\"SECRET-PROMPT-1\"")
        .unwrap_err()
        .into();
    assert!(!err.to_string().contains("SECRET"));
    assert!(!format!("{err:?}").contains("SECRET"));
}
