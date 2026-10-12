#![cfg(not(feature = "runner"))]

use assert_cmd::Command;

#[test]
fn runner_commands_do_not_exist_without_the_feature() {
    for args in [
        ["run", "--help"],
        ["agents", "--help"],
        ["attach", "--help"],
    ] {
        let out = Command::cargo_bin("tmgr")
            .unwrap()
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
