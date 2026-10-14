use std::collections::HashMap;
use std::path::PathBuf;
use tmgr_usage::paths::*;
use tmgr_usage::{Account, Tool};

fn env(home: &std::path::Path, vars: &[(&str, &str)]) -> Env {
    Env {
        home: Some(home.to_path_buf()),
        vars: vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<HashMap<_, _>>(),
    }
}

#[test]
fn data_dir_precedence() {
    let home = PathBuf::from("/h");
    assert_eq!(
        data_dir(&env(
            &home,
            &[("TMGR_USAGE_DIR", "/a"), ("XDG_DATA_HOME", "/x")]
        ))
        .unwrap(),
        PathBuf::from("/a")
    );
    assert_eq!(
        data_dir(&env(&home, &[("XDG_DATA_HOME", "/x")])).unwrap(),
        PathBuf::from("/x/tmgr/usage")
    );
    assert_eq!(
        data_dir(&env(&home, &[("XDG_DATA_HOME", "")])).unwrap(),
        PathBuf::from("/h/.local/share/tmgr/usage")
    );
    assert!(data_dir(&Env::default()).is_err());
}

#[cfg(unix)]
#[test]
fn data_dir_is_created_private() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let dir = ensure_data_dir(&env(tmp.path(), &[])).unwrap();
    let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
}

#[test]
fn default_accounts_follow_env_and_existence() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
    let found = default_accounts(&env(tmp.path(), &[]));
    assert_eq!(found.len(), 1);
    assert_eq!(
        (found[0].tool, found[0].label.as_str()),
        (Tool::Claude, "claude")
    );
    assert_eq!(found[0].root, tmp.path().join(".claude"));

    let other = tmp.path().join("work-claude");
    let codex = tmp.path().join("cx");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::create_dir_all(&codex).unwrap();
    let vars = [
        ("CLAUDE_CONFIG_DIR", other.to_str().unwrap()),
        ("CODEX_HOME", codex.to_str().unwrap()),
    ];
    let found = default_accounts(&env(tmp.path(), &vars));
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].root, other);
    assert_eq!(found[1].root, codex);
}

#[test]
fn user_accounts_add_list_remove() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("data");
    std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
    let e = env(tmp.path(), &[]);
    let acc = |label: &str, root: &str| Account {
        tool: Tool::Claude,
        label: label.into(),
        root: PathBuf::from(root),
    };
    add_account(&dir, &e, acc("work", "/r/work")).unwrap();
    assert!(add_account(&dir, &e, acc("work", "/r/other")).is_err());
    assert!(add_account(&dir, &e, acc("claude", "/r/other")).is_err());
    assert!(add_account(&dir, &e, acc("codex", "/r/other")).is_err());
    assert!(add_account(&dir, &e, acc("dup-root", "/r/work")).is_err());
    assert!(add_account(&dir, &e, acc("bad label", "/r/x")).is_err());
    assert!(add_account(&dir, &e, acc("", "/r/x")).is_err());

    let all = all_accounts(&dir, &e).unwrap();
    assert_eq!(
        all.iter().map(|a| a.label.as_str()).collect::<Vec<_>>(),
        ["claude", "work"]
    );
    assert!(remove_account(&dir, "work").unwrap());
    assert!(!remove_account(&dir, "work").unwrap());
    assert!(list_user_accounts(&dir).unwrap().is_empty());
}
