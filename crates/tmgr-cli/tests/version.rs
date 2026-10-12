use assert_cmd::Command;

fn tmgr() -> Command {
    Command::cargo_bin("tmgr").unwrap()
}

#[test]
fn version_flag_prints_name_and_version() {
    let out = tmgr().arg("--version").assert().success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert_eq!(stdout.trim(), format!("tmgr {}", env!("CARGO_PKG_VERSION")));
}

#[test]
fn version_json_prints_envelope() {
    let out = tmgr().args(["version", "--json"]).assert().success();
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["schema"], "tmgr.v1");
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(!v["data"]["commit"].as_str().unwrap().is_empty());
    assert!(!v["data"]["target"].as_str().unwrap().is_empty());
}

#[test]
fn version_without_json_is_human_readable() {
    let out = tmgr().arg("version").assert().success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(stdout.starts_with(&format!("tmgr {}", env!("CARGO_PKG_VERSION"))));
}
