#![cfg(feature = "runner")]

use assert_cmd::Command;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const HINT: &str = "Titles and questions are off: run tmgr usage consent allow";
const C1: &str = "11111111-0000-4000-8000-000000000001";
const X1: &str = "22222222-0000-4000-8000-000000000002";

struct Rig {
    tmp: tempfile::TempDir,
    runner: tempfile::TempDir,
}

impl Rig {
    fn new() -> Rig {
        let rig = Rig {
            tmp: tempfile::tempdir().unwrap(),
            runner: tempfile::Builder::new()
                .prefix("sl")
                .tempdir_in("/tmp")
                .unwrap(),
        };
        std::fs::create_dir_all(rig.home()).unwrap();
        rig
    }

    fn home(&self) -> PathBuf {
        self.tmp.path().join("home")
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_tmgr"));
        c.env_clear()
            .env("HOME", self.home())
            .env("TMGR_USAGE_DIR", self.tmp.path().join("usage"))
            .env("TMGR_RUNNER_DIR", self.runner.path())
            .env("TMGR_CREDENTIAL_STORE", "file")
            .env("NO_COLOR", "1")
            .args(args);
        c
    }

    fn consent(&self, what: &str) {
        self.cmd(&["usage", "consent", what]).assert().success();
    }

    fn write(&self, rel: &str, body: &str) {
        let p = self.home().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn seed(&self, user_text: &str) {
        let claude = json!({"type":"user","timestamp":"2026-11-05T10:00:00Z","cwd":"/w/claude","gitBranch":"main",
            "message":{"content":user_text}});
        self.write(
            &format!(".claude/projects/-w-claude/{C1}.jsonl"),
            &format!("{claude}\n"),
        );
        let meta = json!({"type":"session_meta","payload":{"id":X1,"timestamp":"2026-11-05T10:00:00Z",
            "cwd":"/w/codex","source":"cli","git":{"branch":"dev"}}});
        let msg = json!({"type":"event_msg","payload":{"type":"user_message","message":"codex says zebra"}});
        self.write(
            &format!(".codex/sessions/2026/11/05/rollout-2026-11-05T10-00-00-{X1}.jsonl"),
            &format!("{meta}\n{msg}\n"),
        );
    }

    fn json(&self, args: &[&str]) -> Value {
        let out = self
            .cmd(args)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        serde_json::from_slice(&out).unwrap()
    }

    fn index(&self) -> PathBuf {
        self.home().join(".config/tmgr/sessions-index.sqlite")
    }
}

fn ids(v: &Value) -> Vec<String> {
    v["data"]["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect()
}

fn hash_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.insert(
                    p.clone(),
                    Sha256::digest(std::fs::read(&p).unwrap()).to_vec(),
                );
            }
        }
    }
    out
}

#[test]
fn granted_lists_both_tools_with_content() {
    let rig = Rig::new();
    rig.seed("please fix the login bug");
    rig.consent("allow");
    let v = rig.json(&["--json", "session", "list"]);
    let mut got = ids(&v);
    got.sort();
    assert_eq!(got, [C1, X1]);
    let rows = v["data"]["sessions"].as_array().unwrap();
    let claude = rows.iter().find(|r| r["id"] == C1).unwrap();
    assert_eq!(claude["agent"], "claude-code");
    assert_eq!(claude["cwd"], "/w/claude");
    assert_eq!(claude["first_message"], "please fix the login bug");
    assert_eq!(claude["readable"], true);
    let codex = rows.iter().find(|r| r["id"] == X1).unwrap();
    assert_eq!(codex["agent"], "codex");
    assert_eq!(codex["first_message"], "codex says zebra");
    assert!(v["data"]["hints"].as_array().unwrap().is_empty());
    let notes = v["data"]["notes"].as_array().unwrap();
    assert!(
        notes[0]
            .as_str()
            .unwrap()
            .contains("tmgr sessions are not listed"),
        "{notes:?}"
    );
}

#[test]
fn the_agent_filter_and_limit_apply() {
    let rig = Rig::new();
    rig.seed("hello");
    rig.consent("allow");
    assert_eq!(
        ids(&rig.json(&["--json", "session", "list", "--agent", "codex"])),
        [X1]
    );
    assert_eq!(
        ids(&rig.json(&["--json", "session", "list", "--agent", "claude"])),
        [C1]
    );
    assert_eq!(
        ids(&rig.json(&["--json", "session", "list", "--limit", "1"])).len(),
        1
    );
    rig.cmd(&["session", "list", "--limit", "0"])
        .assert()
        .failure();
}

#[test]
fn without_consent_rows_are_metadata_only_and_the_hint_prints_once() {
    for denied in [false, true] {
        let rig = Rig::new();
        rig.seed("confidential words");
        if denied {
            rig.consent("deny");
        }
        let out = rig
            .cmd(&["session", "list"])
            .assert()
            .success()
            .get_output()
            .clone();
        let text = String::from_utf8(out.stdout).unwrap();
        assert_eq!(text.matches(HINT).count(), 1, "{text}");
        assert!(text.contains(C1) && text.contains(X1), "{text}");
        assert!(
            !text.contains("confidential") && !text.contains("zebra"),
            "{text}"
        );
        assert!(text.contains("-w-claude"), "{text}");
        let v = rig.json(&["--json", "session", "list"]);
        assert!(v["data"]["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["readable"] == false && r["cwd"].is_null()));
        let search = rig
            .cmd(&["session", "list", "--search", "confidential"])
            .assert()
            .success()
            .get_output()
            .clone();
        assert!(!rig.index().exists(), "no index without consent");
        assert!(!String::from_utf8(search.stdout).unwrap().contains(C1));
    }
}

#[test]
fn search_builds_a_private_index_and_finds_text() {
    let rig = Rig::new();
    rig.seed("rotate the Кириллица tokens in TM-845");
    rig.consent("allow");
    let v = rig.json(&["--json", "session", "list", "--search", "tm-845 кириллица"]);
    assert_eq!(ids(&v), [C1]);
    assert!(v["data"]["sessions"][0]["snippet"]
        .as_str()
        .unwrap()
        .contains("TM-845"));
    let v = rig.json(&[
        "--json", "session", "list", "--search", "zebra", "--agent", "claude",
    ]);
    assert!(ids(&v).is_empty());
    let v = rig.json(&["--json", "session", "list", "--search", "zebra"]);
    assert_eq!(ids(&v), [X1]);
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&rig.index()), 0o600);
    let human = rig
        .cmd(&["session", "list", "--search", "zebra"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(String::from_utf8(human).unwrap().contains(X1));
}

#[test]
fn denying_consent_deletes_the_index() {
    let rig = Rig::new();
    rig.seed("indexed words");
    rig.consent("allow");
    rig.json(&["--json", "session", "list", "--search", "indexed"]);
    assert!(rig.index().exists());
    rig.consent("deny");
    for s in ["", "-wal", "-shm"] {
        let p = PathBuf::from(format!("{}{s}", rig.index().display()));
        assert!(!p.exists(), "{s}");
    }
}

#[test]
fn listing_and_searching_leave_session_files_untouched() {
    let rig = Rig::new();
    rig.seed("immutable");
    rig.write(".claude/sessions/1.json", "{}");
    rig.consent("allow");
    let before = hash_tree(&rig.home().join(".claude"));
    let codex = hash_tree(&rig.home().join(".codex"));
    rig.json(&["--json", "session", "list", "--search", "immutable"]);
    rig.json(&["--json", "session", "list"]);
    assert_eq!(hash_tree(&rig.home().join(".claude")), before);
    assert_eq!(hash_tree(&rig.home().join(".codex")), codex);
}

#[test]
fn escape_sequences_never_reach_the_output() {
    let rig = Rig::new();
    let evil = "x\u{1b}]0;pwn\u{7}y\u{202e}z\nw\u{9b}";
    let claude = json!({"type":"user","cwd":format!("/w/{evil}"),"gitBranch":evil,"message":{"content":evil}});
    rig.write(
        &format!(".claude/projects/-w-{evil}/{C1}.jsonl"),
        &format!("{claude}\n"),
    );
    rig.write(
        &format!(".codex/sessions/2026/11/05/rollout-2026-11-05T10-00-00-{X1}.jsonl"),
        &format!(
            "{}\n{}\n",
            json!({"type":"session_meta","payload":{"id":X1,"cwd":format!("/w/{evil}"),"source":"cli"}}),
            json!({"type":"event_msg","payload":{"type":"user_message","message":evil}})
        ),
    );
    rig.write(
        ".codex/session_index.jsonl",
        &format!("{}\n", json!({"id":X1,"thread_name":evil})),
    );
    let check = |text: &str| {
        for bad in ['\u{1b}', '\u{7}', '\u{202e}', '\u{9b}'] {
            assert!(!text.contains(bad), "{bad:?} in {text:?}");
        }
    };
    for granted in [false, true] {
        if granted {
            rig.consent("allow");
        }
        for args in [
            vec!["session", "list"],
            vec!["--json", "session", "list"],
            vec!["--json", "session", "list", "--search", "pwn"],
            vec!["session", "list", "--search", "pwn"],
        ] {
            let out = rig.cmd(&args).assert().success().get_output().clone();
            let stdout = String::from_utf8(out.stdout).unwrap();
            if args.len() <= 3 {
                assert!(stdout.contains(C1) && stdout.contains(X1), "{stdout}");
            }
            check(&stdout);
            check(&String::from_utf8(out.stderr).unwrap());
        }
    }
}

#[test]
fn inside_an_agent_session_listing_is_refused() {
    let rig = Rig::new();
    rig.seed("secret");
    rig.consent("allow");
    for var in ["TMGR_RUN_TOKEN", "TMGR_RUN_ID", "TMGR_SESSION_ID"] {
        for args in [
            vec!["session", "list"],
            vec!["session", "list", "--search", "secret"],
        ] {
            let out = rig
                .cmd(&args)
                .env(var, "x")
                .assert()
                .code(5)
                .get_output()
                .clone();
            let text = String::from_utf8_lossy(&out.stdout).to_string()
                + &String::from_utf8_lossy(&out.stderr);
            assert!(!text.contains(C1), "{text}");
        }
    }
}
