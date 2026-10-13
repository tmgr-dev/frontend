#![cfg(not(feature = "runner"))]

use assert_cmd::Command;

#[test]
fn runner_commands_do_not_exist_without_the_feature() {
    for args in [
        ["run", "--help"],
        ["agents", "--help"],
        ["attach", "--help"],
    ] {
        let home = tempfile::tempdir().unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_tmgr"))
            .env_clear()
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path().join("xdg"))
            .env("XDG_STATE_HOME", home.path().join("state"))
            .env("XDG_DATA_HOME", home.path().join("data"))
            .env("TMGR_CREDENTIAL_STORE", "file")
            .env("TMGR_RUNNER_DIR", home.path().join("run"))
            .env("TMGR_RUNNER_HOMES", home.path().join("homes"))
            .env("TMGR_AGENTS_DIR", home.path().join("agents"))
            .args(args)
            .output()
            .unwrap();
        assert!(!out.status.success(), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("unrecognized subcommand"),
            "{args:?}"
        );
    }
}
