use assert_cmd::Command;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tmgr_link::audit::Audit;
use tmgr_link::leader::Client;
use tmgr_link::proto::Reply;
use tmgr_link::scope::Scope;
use tmgr_link::store::{Leader, Paths, Store};

const ENR: &str = "0123456789abcdef0123456789abcdef";

struct Fx {
    _tmp: tempfile::TempDir,
    home: PathBuf,
    repo: PathBuf,
    paths: Paths,
}

fn fx() -> Fx {
    let tmp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(tmp.path()).unwrap();
    let home = root.join("home");
    let repo = root.join("repo");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&repo).unwrap();
    let paths = Paths {
        config: home.join(".config").join("tmgr"),
        state: home.join(".local").join("state").join("tmgr"),
        home: home.clone(),
    };
    Fx {
        _tmp: tmp,
        home,
        repo,
        paths,
    }
}

impl Fx {
    fn trust_leader(&self, name: &str, scope: Scope) {
        Store::update(&self.paths, |s| {
            s.leaders.insert(
                name.into(),
                Leader {
                    key: "AAAA".into(),
                    fingerprint: "SHA256:fixture".into(),
                    added: "2026-10-13T00:00:00Z".into(),
                    scope,
                    enrollment: ENR.into(),
                    authorized_keys: String::new(),
                },
            );
            Ok(())
        })
        .unwrap();
    }

    fn init_identity(&self) -> tmgr_link::leader::Identity {
        let cred =
            tmgr_link::leader::credential_store(&self.paths, Some(tmgr_core::creds::Backend::File));
        tmgr_link::leader::init(&self.paths, &cred, false).unwrap()
    }

    fn scope(&self) -> Scope {
        Scope {
            repos: vec![self.repo.to_str().unwrap().into()],
            agents: vec!["codex".into()],
            models: vec![],
            max: 2,
            from: vec![],
            allow_attach_input: false,
            ..Default::default()
        }
    }

    fn tmgr(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_tmgr"));
        c.env("HOME", &self.home)
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_STATE_HOME")
            .env_remove("XDG_RUNTIME_DIR")
            .env_remove("TMGR_RUNNER_DIR")
            .env_remove("TMGR_RUN_TOKEN")
            .env_remove("TMGR_RUN_ID")
            .env_remove("TMGR_SESSION_ID")
            .env("TMGR_RUNNER_DIR", self.home.join("no-runner"))
            .env("TMGR_CREDENTIAL_STORE", "file")
            .env("NO_COLOR", "1");
        for (name, _) in std::env::vars_os() {
            if tmgr_local::in_agent_session([name.to_string_lossy()]) {
                c.env_remove(name);
            }
        }
        c
    }

    fn worker(&self, name: &str) -> std::process::Command {
        let mut c =
            std::process::Command::new(std::path::PathBuf::from(env!("CARGO_BIN_EXE_tmgr")));
        c.args(["worker", "--link", name, "--enrollment", ENR])
            .env("HOME", &self.home)
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_STATE_HOME")
            .env_remove("XDG_RUNTIME_DIR")
            .env("TMGR_RUNNER_DIR", self.home.join("no-runner"))
            .env("SSH_CONNECTION", "192.0.2.5 5555 192.0.2.1 22");
        c
    }

    fn client(&self, name: &str) -> Client {
        Client::spawn(self.worker(name), Duration::from_secs(30)).unwrap()
    }
}

fn err_text(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

#[test]
fn trust_is_refused_inside_agent_sessions_with_exit_5() {
    let f = fx();
    for var in ["TMGR_RUN_TOKEN", "TMGR_RUN_ID"] {
        let out = f
            .tmgr()
            .env(var, "x")
            .args([
                "link",
                "trust",
                "ssh-ed25519",
                "AAAA",
                "--as",
                "leader",
                "--name",
                "laptop",
                "--yes",
            ])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(5), "{var}");
        assert!(err_text(&out).contains("privileged change refused inside an agent session"));
    }
    assert!(!f.paths.links_toml().exists());
}

#[test]
fn trust_without_a_terminal_exits_10_and_yes_never_applies() {
    let f = fx();
    for extra in [&[][..], &["--yes"][..], &["-y"][..]] {
        let mut args = vec![
            "link",
            "trust",
            "ssh-ed25519",
            "AAAA",
            "--as",
            "leader",
            "--name",
            "laptop",
        ];
        args.extend_from_slice(extra);
        let out = f.tmgr().args(&args).output().unwrap();
        assert_eq!(out.status.code(), Some(10), "{extra:?}");
    }
    let out = f
        .tmgr()
        .args(["link", "add", "spark", "--ssh", "u@example.com", "--yes"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10));
    let json = f
        .tmgr()
        .args([
            "link",
            "trust",
            "ssh-ed25519",
            "AAAA",
            "--as",
            "leader",
            "--name",
            "l",
            "--json",
        ])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(v["error"]["code"], "needs_confirmation");
    assert!(!f.paths.authorized_keys().exists());
}

#[test]
fn widening_a_scope_needs_a_terminal_but_narrowing_does_not() {
    let f = fx();
    f.trust_leader("laptop", f.scope());
    let out = f
        .tmgr()
        .args(["link", "scope", "laptop", "--max", "9", "--yes"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10));
    let out = f
        .tmgr()
        .env("TMGR_RUN_TOKEN", "x")
        .args(["link", "scope", "laptop", "--max", "9"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    assert_eq!(
        Store::load(&f.paths).unwrap().leaders["laptop"].scope.max,
        2
    );
    let out = f
        .tmgr()
        .env("TMGR_RUN_TOKEN", "x")
        .args(["link", "scope", "laptop", "--max", "1", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", err_text(&out));
    assert_eq!(
        Store::load(&f.paths).unwrap().leaders["laptop"].scope.max,
        1
    );
    let shown = f.tmgr().args(["link", "scope", "laptop"]).output().unwrap();
    assert!(String::from_utf8_lossy(&shown.stdout).contains("max 1 running"));
}

#[test]
fn attach_input_is_a_widening_that_needs_a_terminal() {
    let f = fx();
    f.trust_leader("laptop", f.scope());
    let input = |f: &Fx| {
        Store::load(&f.paths).unwrap().leaders["laptop"]
            .scope
            .allow_attach_input
    };
    for extra in [
        &["--allow-attach-input"][..],
        &["--allow-attach-input", "true", "--yes"][..],
        &["--allow-attach-input=true"][..],
    ] {
        let mut args = vec!["link", "scope", "laptop"];
        args.extend_from_slice(extra);
        let out = f.tmgr().args(&args).output().unwrap();
        assert_eq!(out.status.code(), Some(10), "{extra:?}: {}", err_text(&out));
        assert!(!input(&f));
    }
    let out = f
        .tmgr()
        .env("TMGR_RUN_TOKEN", "x")
        .args(["link", "scope", "laptop", "--allow-attach-input"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    assert!(!input(&f));
    let shown = f.tmgr().args(["link", "scope", "laptop"]).output().unwrap();
    assert!(String::from_utf8_lossy(&shown.stdout).contains("attach input: forbidden"));

    f.trust_leader(
        "laptop",
        Scope {
            allow_attach_input: true,
            ..f.scope()
        },
    );
    let out = f
        .tmgr()
        .env("TMGR_RUN_TOKEN", "x")
        .args(["link", "scope", "laptop", "--allow-attach-input", "false"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", err_text(&out));
    assert!(!input(&f));
}

#[test]
fn revoke_with_an_unreadable_runs_file_exits_1_and_says_so() {
    let f = fx();
    f.trust_leader("laptop", f.scope());
    let dir = f.paths.link_dir();
    std::fs::create_dir_all(&dir).unwrap();
    tmgr_core::config::write_atomic(&dir.join("runs.json"), b"\x00 not json", 0o600).unwrap();
    let out = f
        .tmgr()
        .args(["link", "revoke", "laptop", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", err_text(&out));
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &err_text(&out);
    assert!(text.contains("containment incomplete"), "{text}");
    assert!(
        text.contains("tmgr link revoke laptop --halt-runs"),
        "{text}"
    );
    assert!(Store::load(&f.paths).unwrap().leaders.is_empty());
}

#[test]
fn revoke_with_an_unreadable_worker_registry_exits_1_and_says_so() {
    let f = fx();
    f.trust_leader("laptop", f.scope());
    let dir = f.paths.link_dir();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("workers"), b"").unwrap();
    let out = f
        .tmgr()
        .args(["link", "revoke", "laptop"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", err_text(&out));
    let text = err_text(&out);
    assert!(text.contains("worker registry"), "{text}");
    assert!(text.contains("not a directory"), "{text}");
    assert!(
        text.contains("tmgr link revoke laptop --halt-runs"),
        "{text}"
    );
    assert!(Store::load(&f.paths).unwrap().leaders.is_empty());
}

#[test]
fn revoke_with_an_unknown_worker_record_exits_1_and_says_so() {
    let f = fx();
    f.trust_leader("laptop", f.scope());
    let workers = f.paths.link_dir().join("workers");
    std::fs::create_dir_all(&workers).unwrap();
    std::fs::write(workers.join("laptop.4242.json"), b"not json").unwrap();
    let out = f
        .tmgr()
        .args(["link", "revoke", "laptop"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", err_text(&out));
    assert!(
        err_text(&out).contains("could not be checked"),
        "{}",
        err_text(&out)
    );
}

#[test]
fn turning_remote_control_off_with_an_unknown_worker_record_saves_off_and_fails() {
    let f = fx();
    f.trust_leader(
        "laptop",
        Scope {
            remote_control: true,
            ..f.scope()
        },
    );
    let workers = f.paths.link_dir().join("workers");
    std::fs::create_dir_all(&workers).unwrap();
    std::fs::write(workers.join("laptop.4242.json"), b"not json").unwrap();
    let out = f
        .tmgr()
        .args(["link", "scope", "laptop", "--remote-control", "off"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", err_text(&out));
    assert!(
        err_text(&out).contains("could not be checked"),
        "{}",
        err_text(&out)
    );
    let store = Store::load(&f.paths).unwrap();
    assert!(!store.leaders["laptop"].scope.remote_control);
}

#[test]
fn running_off_again_with_an_unknown_worker_record_still_fails() {
    let f = fx();
    f.trust_leader("laptop", f.scope());
    let workers = f.paths.link_dir().join("workers");
    std::fs::create_dir_all(&workers).unwrap();
    std::fs::write(workers.join("laptop.4242.json"), b"not json").unwrap();
    let out = f
        .tmgr()
        .args(["link", "scope", "laptop", "--include-bypass", "off"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", err_text(&out));
    assert!(err_text(&out).contains("saved"), "{}", err_text(&out));
}

#[test]
fn an_unknown_worker_record_only_warns_on_an_unrelated_scope_change() {
    let f = fx();
    f.trust_leader(
        "laptop",
        Scope {
            allow_attach_input: true,
            ..f.scope()
        },
    );
    let workers = f.paths.link_dir().join("workers");
    std::fs::create_dir_all(&workers).unwrap();
    std::fs::write(workers.join("laptop.4242.json"), b"not json").unwrap();
    let out = f
        .tmgr()
        .args(["link", "scope", "laptop", "--allow-attach-input", "false"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", err_text(&out));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("could not be checked"), "{text}");
}

#[test]
fn an_unknown_worker_record_does_not_fail_a_scope_change_with_both_flags_on() {
    let f = fx();
    f.trust_leader(
        "laptop",
        Scope {
            remote_control: true,
            include_bypass: true,
            allow_attach_input: true,
            ..f.scope()
        },
    );
    let workers = f.paths.link_dir().join("workers");
    std::fs::create_dir_all(&workers).unwrap();
    std::fs::write(workers.join("laptop.4242.json"), b"not json").unwrap();
    let out = f
        .tmgr()
        .args(["link", "scope", "laptop", "--allow-attach-input", "false"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", err_text(&out));
}

#[test]
fn revoke_is_always_allowed_and_audited() {
    let f = fx();
    f.trust_leader("laptop", f.scope());
    let out = f
        .tmgr()
        .env("TMGR_RUN_TOKEN", "x")
        .args(["link", "revoke", "laptop"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", err_text(&out));
    assert!(Store::load(&f.paths).unwrap().leaders.is_empty());
    let audit = Audit::new(&f.paths.audit_dir());
    let tail = audit.tail(2).unwrap();
    assert_eq!(tail[0]["event"], "link.revoke");
    assert_eq!(tail[1]["event"], "link.revoke.contain");
    let again = f
        .tmgr()
        .args(["link", "revoke", "laptop"])
        .output()
        .unwrap();
    assert_eq!(again.status.code(), Some(0), "{}", err_text(&again));
    let unknown = f
        .tmgr()
        .args(["link", "revoke", "never-trusted"])
        .output()
        .unwrap();
    assert_eq!(unknown.status.code(), Some(3));
    let retry_halt = f
        .tmgr()
        .args(["link", "revoke", "laptop", "--halt-runs"])
        .output()
        .unwrap();
    assert_eq!(
        retry_halt.status.code(),
        Some(1),
        "{}",
        err_text(&retry_halt)
    );
    assert!(err_text(&retry_halt).contains("containment incomplete"));
    let bad = f
        .tmgr()
        .args(["link", "revoke", "Bad Name"])
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(2));
}

#[test]
fn worker_refuses_everything_but_the_exact_shape() {
    let f = fx();
    f.trust_leader("laptop", f.scope());
    let cases: Vec<Vec<&str>> = vec![
        vec!["worker", "--link", "laptop", "--enrollment", ENR, "--json"],
        vec!["--json", "worker", "--link", "laptop", "--enrollment", ENR],
        vec!["worker", "--link=laptop", "--enrollment", ENR],
        vec!["worker", "--link", "Laptop", "--enrollment", ENR],
        vec!["worker", "--enrollment", ENR, "--link", "laptop"],
        vec!["worker", "--link", "laptop", "--enrollment", "short"],
    ];
    for args in cases {
        let out = f
            .tmgr()
            .env("SSH_CONNECTION", "192.0.2.5 5555 192.0.2.1 22")
            .args(&args)
            .output()
            .unwrap();
        assert_ne!(out.status.code(), Some(0), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
    }
    let out = f.tmgr().args(["worker"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn worker_needs_ssh_connection_and_a_trusted_link() {
    let f = fx();
    f.trust_leader("laptop", f.scope());
    let out = f
        .tmgr()
        .args(["worker", "--link", "laptop", "--enrollment", ENR])
        .env_remove("SSH_CONNECTION")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    assert!(out.stdout.is_empty());
    assert!(err_text(&out).trim() == "tmgr worker: refused");
    let out = f
        .tmgr()
        .args(["worker", "--link", "stranger", "--enrollment", ENR])
        .env("SSH_CONNECTION", "192.0.2.5 5555 192.0.2.1 22")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    assert!(out.stdout.is_empty());
}

fn reply_err(r: Reply) -> (String, String) {
    match r {
        Reply::Err { code, message } => (code, message),
        Reply::Ok(v) => panic!("unexpected ok {v}"),
    }
}

#[test]
fn leader_talks_to_the_worker_over_a_pipe_pair() {
    let f = fx();
    f.trust_leader("laptop", f.scope());
    let mut c = f.client("laptop");
    let hello = c.hello().unwrap();
    assert_eq!(hello["role"], "follower");
    assert_eq!(
        hello["methods"],
        json!([
            "hello",
            "runs.list",
            "runs.get",
            "runs.create",
            "runs.halt",
            "attach.open"
        ])
    );

    let repo = f.repo.to_str().unwrap();
    let ok = c
        .call(
            "runs.create",
            json!({"task":"w1:512","repo":repo,"agent":"codex"}),
        )
        .unwrap();
    assert_eq!(
        reply_err(ok),
        ("not_available".into(), "runner not running".into())
    );
    let listed = c.call("runs.list", Value::Null).unwrap();
    assert_eq!(reply_err(listed).0, "not_available");
    let wrong_agent = c
        .call(
            "runs.create",
            json!({"task":"w1:512","repo":repo,"agent":"claude-code"}),
        )
        .unwrap();
    assert_eq!(reply_err(wrong_agent).0, "forbidden");
    let outside = c
        .call(
            "runs.create",
            json!({"task":"w1:512","repo":"/","agent":"codex"}),
        )
        .unwrap();
    assert_eq!(reply_err(outside).0, "forbidden");
    let bypass = c
        .call(
            "runs.create",
            json!({"task":"w1:530","repo":repo,"agent":"codex","mode":"bypass"}),
        )
        .unwrap();
    assert_eq!(reply_err(bypass).1, "bypass not allowed");
    let on = c
        .call(
            "runs.create",
            json!({"task":"w1:530","repo":repo,"agent":"codex","on":"spark"}),
        )
        .unwrap();
    assert_eq!(reply_err(on).0, "forbidden");
    for method in [
        "shell.exec",
        "files.read",
        "config.set",
        "link.trust",
        "runs.send",
    ] {
        let r = c.call(method, json!({"cmd":"id"})).unwrap();
        assert_eq!(reply_err(r).0, "unknown_method", "{method}");
    }
    let unknown_run = c.call("runs.halt", json!({"run":"r-1"})).unwrap();
    assert_eq!(reply_err(unknown_run).0, "not_found");
    drop(c);

    let audit = Audit::new(&f.paths.audit_dir());
    let log = audit.tail(100).unwrap();
    assert_eq!(log[0]["event"], "worker.connect");
    assert_eq!(log[0]["client"], "192.0.2.5");
    let accepted = log.iter().filter(|e| e["decision"] == "accepted").count();
    assert_eq!(accepted, 3);
    assert!(audit.verify().unwrap().is_ok());

    let out = f
        .tmgr()
        .args(["link", "audit", "--verify", "--json"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["ok"], true);
    let human = f
        .tmgr()
        .args(["link", "audit", "--tail", "3"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(
        text.contains("from laptop") && text.contains("rejected"),
        "{text}"
    );
}

#[test]
fn a_revoked_link_stops_answering_on_the_next_request() {
    let f = fx();
    f.trust_leader("laptop", f.scope());
    let mut c = f.client("laptop");
    c.hello().unwrap();
    let out = f
        .tmgr()
        .args(["link", "revoke", "laptop"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let json_out = f
        .tmgr()
        .args(["link", "audit", "--tail", "5", "--json"])
        .output()
        .unwrap();
    let audit: Value = serde_json::from_slice(&json_out.stdout).unwrap();
    let contain = audit["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["event"] == "link.revoke.contain")
        .unwrap();
    assert_eq!(contain["workers_signalled"], 1);
    assert_eq!(contain["workers_surviving"], 0);
    let left = std::fs::read_dir(f.paths.link_dir().join("workers")).map_or(0, |d| d.count());
    assert_eq!(left, 0);
    match c.call("runs.list", Value::Null) {
        Ok(r) => assert_eq!(reply_err(r).0, "revoked"),
        Err(e) => assert_eq!(e.exit_code(), 9),
    }
    assert!(c.call("hello", json!({})).is_err());
}

#[test]
fn raw_garbage_and_oversize_never_crash_the_worker() {
    use std::io::{Read, Write};
    use std::process::Stdio;
    let f = fx();
    f.trust_leader("laptop", f.scope());
    let mut child = f
        .worker("laptop")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let garbage = b"\x00\x00\x00\x05hello";
    stdin.write_all(garbage).unwrap();
    stdin.write_all(&[0x7f, 0xff, 0xff, 0xff]).unwrap();
    stdin.write_all(&[0u8; 32]).unwrap();
    drop(stdin);
    let mut out = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut out).unwrap();
    let status = child.wait().unwrap();
    assert!(status.success());
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("bad_request") && text.contains("frame_too_large"),
        "{text}"
    );
}

#[test]
fn audit_verify_detects_tampering_through_the_cli() {
    let f = fx();
    f.trust_leader("laptop", f.scope());
    let mut c = f.client("laptop");
    for _ in 0..3 {
        c.hello().unwrap();
    }
    drop(c);
    let ok = f
        .tmgr()
        .args(["link", "audit", "--verify"])
        .output()
        .unwrap();
    assert_eq!(ok.status.code(), Some(0));
    let file = Audit::new(&f.paths.audit_dir()).current();
    let text = std::fs::read_to_string(&file).unwrap();
    let mut lines: Vec<&str> = text.lines().collect();
    lines.remove(1);
    std::fs::write(&file, format!("{}\n", lines.join("\n"))).unwrap();
    let bad = f
        .tmgr()
        .args(["link", "audit", "--verify"])
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(1));
    assert!(err_text(&bad).contains("audit chain broken"));
}

#[test]
fn init_is_privileged_and_the_identity_never_shows_the_private_key() {
    let f = fx();
    let out = f.tmgr().args(["link", "init", "--yes"]).output().unwrap();
    assert_eq!(out.status.code(), Some(10));
    let out = f
        .tmgr()
        .env("TMGR_RUN_TOKEN", "x")
        .args(["link", "init"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    assert!(!f.paths.public_key().exists());
    let id = f.init_identity();
    let list = f.tmgr().args(["link", "list", "--json"]).output().unwrap();
    let text = String::from_utf8_lossy(&list.stdout).to_string();
    assert!(!text.contains("PRIVATE") && !text.contains("BEGIN"));
    let l: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(l["data"]["identity"]["fingerprint"], id.fingerprint);
    let rotate = f
        .tmgr()
        .args(["link", "init", "--rotate"])
        .output()
        .unwrap();
    assert_eq!(rotate.status.code(), Some(10));
}

#[test]
fn list_on_a_fresh_machine_prints_one_hint() {
    let f = fx();
    let out = f.tmgr().args(["link", "list"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("No links."));
}

#[test]
fn link_and_worker_are_hidden_from_help_but_documented_as_experimental() {
    let f = fx();
    let out = f.tmgr().arg("--help").output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(!text
        .lines()
        .any(|l| l.trim_start().starts_with("link ") || l.trim_start().starts_with("worker ")));
    for cmd in ["link", "worker"] {
        let out = f.tmgr().args([cmd, "--help"]).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("experimental"), "{cmd}: {text}");
    }
}

#[test]
fn files_written_by_the_cli_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let f = fx();
    f.init_identity();
    f.trust_leader("laptop", f.scope());
    let mut c = f.client("laptop");
    c.hello().unwrap();
    drop(c);
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&f.paths.links_toml()), 0o600);
    assert_eq!(mode(&f.paths.config), 0o700);
    assert_eq!(mode(&f.paths.audit_dir()), 0o700);
    assert_eq!(mode(&Audit::new(&f.paths.audit_dir()).current()), 0o600);
    assert_eq!(mode(&f.paths.config.join("credentials.json")), 0o600);
}

fn scope_flags(f: &Fx) -> (bool, bool) {
    let s = Store::load(&f.paths).unwrap().leaders["laptop"]
        .scope
        .clone();
    (s.remote_control, s.include_bypass)
}

fn links_bytes(f: &Fx) -> Vec<u8> {
    std::fs::read(f.paths.links_toml()).unwrap()
}

#[test]
fn enabling_remote_control_or_include_bypass_is_refused_in_an_agent_session_with_exit_5() {
    let f = fx();
    f.trust_leader("laptop", f.scope());
    let before = links_bytes(&f);
    for flag in ["--remote-control", "--include-bypass"] {
        let out = f
            .tmgr()
            .env("TMGR_SESSION_ID", "s_x")
            .args(["link", "scope", "laptop", flag, "on", "--yes"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(5), "{flag}: {}", err_text(&out));
        assert_eq!(links_bytes(&f), before, "{flag}");
    }
}

#[test]
fn enabling_is_refused_under_every_agent_session_marker() {
    let f = fx();
    f.trust_leader("laptop", f.scope());
    let before = links_bytes(&f);
    for marker in [
        "CLAUDECODE",
        "CLAUDE_CODE_ENTRYPOINT",
        "CODEX_SANDBOX_MODE",
        "TMGR_AGENT_SESSION",
    ] {
        let out = f
            .tmgr()
            .env(marker, "1")
            .args(["link", "scope", "laptop", "--remote-control", "on"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(5), "{marker}: {}", err_text(&out));
        assert_eq!(links_bytes(&f), before, "{marker}");
    }
}

#[test]
fn a_mixed_call_that_turns_one_flag_on_is_still_guarded() {
    let f = fx();
    f.trust_leader(
        "laptop",
        Scope {
            include_bypass: true,
            ..f.scope()
        },
    );
    let before = links_bytes(&f);
    for args in [
        &["--remote-control", "on", "--include-bypass", "off"][..],
        &["--remote-control", "on", "--max", "1"][..],
    ] {
        let mut a = vec!["link", "scope", "laptop"];
        a.extend_from_slice(args);
        let out = f
            .tmgr()
            .env("TMGR_SESSION_ID", "s_x")
            .args(&a)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(5), "{args:?}");
        let out = f.tmgr().args(&a).output().unwrap();
        assert_eq!(out.status.code(), Some(10), "{args:?}");
        assert_eq!(links_bytes(&f), before, "{args:?}");
    }
}

#[test]
fn enabling_remote_control_without_a_terminal_exits_10_and_changes_nothing() {
    let f = fx();
    f.trust_leader("laptop", f.scope());
    let before = links_bytes(&f);
    for flag in ["--remote-control", "--include-bypass"] {
        let out = f
            .tmgr()
            .args(["link", "scope", "laptop", flag, "on", "--yes"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(10), "{flag}: {}", err_text(&out));
        assert_eq!(links_bytes(&f), before, "{flag}");
    }
}

#[test]
fn turning_remote_control_or_include_bypass_off_works_from_an_agent_session_without_a_terminal() {
    let f = fx();
    f.trust_leader(
        "laptop",
        Scope {
            remote_control: true,
            include_bypass: true,
            ..f.scope()
        },
    );
    for (flag, want) in [
        ("--include-bypass", (true, false)),
        ("--remote-control", (false, false)),
    ] {
        let out = f
            .tmgr()
            .env("TMGR_SESSION_ID", "s_x")
            .args(["link", "scope", "laptop", flag, "off"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0), "{flag}: {}", err_text(&out));
        assert_eq!(scope_flags(&f), want, "{flag}");
    }
}

#[test]
fn old_links_files_parse_as_off_and_new_links_start_off() {
    let f = fx();
    f.trust_leader("laptop", f.scope());
    let raw = String::from_utf8(links_bytes(&f)).unwrap();
    assert!(!raw.contains("remote_control") && !raw.contains("include_bypass"));
    assert_eq!(scope_flags(&f), (false, false));
    let out = f
        .tmgr()
        .args(["link", "scope", "laptop", "--json"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["scope"]["remote_control"], false);
    assert_eq!(v["data"]["scope"]["include_bypass"], false);
    assert_eq!(v["data"]["scope"]["allow_bypass"], false);
}

#[test]
fn trust_has_no_remote_control_flags() {
    let f = fx();
    for flag in ["--remote-control", "--include-bypass"] {
        let out = f
            .tmgr()
            .args([
                "link",
                "trust",
                "ssh-ed25519",
                "AAAA",
                "--as",
                "leader",
                "--name",
                "l",
                flag,
                "on",
            ])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{flag}");
        assert!(err_text(&out).contains("unexpected argument"), "{flag}");
    }
}

#[test]
fn remote_control_values_must_be_explicit_on_or_off() {
    let f = fx();
    f.trust_leader("laptop", f.scope());
    let before = links_bytes(&f);
    for args in [
        &["--remote-control", "maybe"][..],
        &["--remote-control"][..],
        &["--include-bypass", "true"][..],
    ] {
        let mut a = vec!["link", "scope", "laptop"];
        a.extend_from_slice(args);
        let out = f.tmgr().args(&a).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", err_text(&out));
        assert_eq!(links_bytes(&f), before);
    }
}

fn fake_attached_worker(f: &Fx, bypass: bool) -> std::process::Child {
    let child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .unwrap();
    let dir = f.paths.link_dir().join("workers");
    std::fs::create_dir_all(&dir).unwrap();
    let rec = json!({
        "pid": child.id(),
        "start": tmgr_link::registry::process_start(child.id()).unwrap(),
        "enrollment": ENR,
        "attached": {"run": "s_abc", "input": true, "remote": true, "bypass": bypass},
    });
    std::fs::write(
        dir.join(format!("laptop.{}.json", child.id())),
        rec.to_string(),
    )
    .unwrap();
    child
}

#[test]
fn turning_remote_control_off_stops_remote_attach_workers_and_audits_it() {
    let f = fx();
    f.trust_leader(
        "laptop",
        Scope {
            remote_control: true,
            ..f.scope()
        },
    );
    let mut worker = fake_attached_worker(&f, false);
    let out = f
        .tmgr()
        .args([
            "link",
            "scope",
            "laptop",
            "--remote-control",
            "off",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", err_text(&out));
    assert!(worker.wait().is_ok_and(|s| !s.success()));
    let entries = Audit::new(&f.paths.audit_dir()).tail(10).unwrap();
    let closed = entries
        .iter()
        .find(|e| e["event"] == "link.scope.closed")
        .expect("closed entry");
    assert_eq!(closed["link"], "laptop");
    assert_eq!(closed["signalled"], 1);
    assert_eq!(closed["survivors"], 0);
}

#[test]
fn turning_only_include_bypass_off_stops_bypass_attaches_and_spares_the_rest() {
    let f = fx();
    f.trust_leader(
        "laptop",
        Scope {
            remote_control: true,
            include_bypass: true,
            ..f.scope()
        },
    );
    let mut bypass = fake_attached_worker(&f, true);
    let mut plain = fake_attached_worker(&f, false);
    let out = f
        .tmgr()
        .args(["link", "scope", "laptop", "--include-bypass", "off"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", err_text(&out));
    assert!(bypass.wait().is_ok_and(|s| !s.success()));
    assert!(plain.try_wait().unwrap().is_none());
    plain.kill().unwrap();
    plain.wait().unwrap();
}
