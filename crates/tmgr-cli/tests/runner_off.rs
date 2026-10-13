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
            .env("TMGR_CREDENTIAL_STORE", "file")
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
