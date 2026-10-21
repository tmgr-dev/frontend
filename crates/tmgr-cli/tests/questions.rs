#![cfg(feature = "runner")]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tmgr_runner::questions::{Origin, Question, SessionFile, Store};

struct Home {
    tmp: tempfile::TempDir,
}

impl Home {
    fn new() -> Home {
        let h = Home {
            tmp: tempfile::tempdir().unwrap(),
        };
        std::fs::create_dir_all(h.run_dir()).unwrap();
        std::fs::set_permissions(h.run_dir(), std::fs::Permissions::from_mode(0o700)).unwrap();
        h
    }

    fn run_dir(&self) -> PathBuf {
        self.tmp.path().join("run")
    }

    fn cmd(&self) -> assert_cmd::Command {
        let mut c = assert_cmd::Command::new(env!("CARGO_BIN_EXE_tmgr"));
        c.env_clear()
            .env("HOME", self.tmp.path())
            .env("XDG_CONFIG_HOME", self.tmp.path().join("xdg"))
            .env("TMGR_CREDENTIAL_STORE", "file")
            .env("TMGR_RUNNER_DIR", self.run_dir())
            .env("NO_COLOR", "1");
        c
    }

    fn seed(&self) {
        let q = |id: &str, text: &str, t: u64| Question {
            id: id.into(),
            session: "s_abcdefghjk".into(),
            text: text.into(),
            asked_s: t,
            origin: Origin::Haiku,
            user: String::new(),
            reply: String::new(),
        };
        let file = SessionFile {
            questions: vec![
                q("aaaa11112222", "Should I update the docs?", 4_000_000_000),
                q("bbbb33334444", "Keep the old flag?", 4_000_000_100),
            ],
            ..SessionFile::default()
        };
        Store::new(&self.run_dir())
            .save("s_abcdefghjk", &file, 4_000_000_200)
            .unwrap();
    }

    fn json(&self, args: &[&str]) -> serde_json::Value {
        let out = self.cmd().args(args).output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
}

#[test]
fn lists_answers_and_ignores_without_a_runner() {
    let h = Home::new();
    h.seed();
    let v = h.json(&["--json", "questions"]);
    assert_eq!(v["data"]["questions"].as_array().unwrap().len(), 2);
    assert_eq!(v["data"]["haiku"], true);
    h.json(&["--json", "questions", "answer", "aaaa"]);
    h.json(&["--json", "questions", "ignore", "bbbb"]);
    let v = h.json(&["--json", "questions"]);
    assert!(v["data"]["questions"].as_array().unwrap().is_empty());
    let v = h.json(&["--json", "questions", "--status", "ignored"]);
    assert_eq!(v["data"]["questions"][0]["id"], "bbbb33334444");
    let v = h.json(&[
        "--json",
        "questions",
        "--status",
        "all",
        "--session",
        "s_abcdefghjk",
    ]);
    assert_eq!(v["data"]["questions"].as_array().unwrap().len(), 2);
}

#[test]
fn haiku_can_be_turned_off() {
    let h = Home::new();
    let v = h.json(&["--json", "questions", "haiku", "off"]);
    assert_eq!(v["data"]["haiku"], false);
    let v = h.json(&["--json", "questions"]);
    assert_eq!(v["data"]["haiku"], false);
}

#[test]
fn an_unknown_id_fails() {
    let h = Home::new();
    h.seed();
    let out = h
        .cmd()
        .args(["questions", "answer", "zzzz"])
        .output()
        .unwrap();
    assert!(!out.status.success());
}
