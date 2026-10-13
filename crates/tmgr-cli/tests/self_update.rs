use assert_cmd::Command;

fn tmgr(home: &std::path::Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_tmgr"));
    c.env_clear()
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("xdg"))
        .env("NO_COLOR", "1");
    c
}

#[test]
fn build_without_update_key_refuses_with_exit_5() {
    let home = tempfile::tempdir().unwrap();
    let out = tmgr(home.path()).arg("self-update").assert().code(5);
    let stderr = String::from_utf8(out.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("this build has no update key; install a signed release"));
    assert!(!home.path().join(".local/state/tmgr/update.json").exists());
}

#[test]
fn check_refuses_the_same_way_and_json_carries_the_code() {
    let home = tempfile::tempdir().unwrap();
    let out = tmgr(home.path())
        .args(["self-update", "--check", "--json"])
        .assert()
        .code(5);
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "forbidden");
    assert_eq!(v["error"]["exit"], 5);
}

#[test]
fn flags_parse() {
    let home = tempfile::tempdir().unwrap();
    tmgr(home.path())
        .args(["self-update", "--channel", "beta", "--check"])
        .assert()
        .code(5);
    tmgr(home.path())
        .args(["self-update", "--channel", "nightly"])
        .assert()
        .code(2);
}
