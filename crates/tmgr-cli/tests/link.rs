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
        }
    }

    fn tmgr(&self) -> Command {
        let mut c = Command::cargo_bin("tmgr").unwrap();
        c.env("HOME", &self.home)
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_STATE_HOME")
            .env_remove("TMGR_RUN_TOKEN")
            .env_remove("TMGR_RUN_ID")
            .env("TMGR_CREDENTIAL_STORE", "file")
            .env("NO_COLOR", "1");
        c
    }

    fn worker(&self, name: &str) -> std::process::Command {
        let mut c = std::process::Command::new(assert_cmd::cargo::cargo_bin("tmgr"));
        c.args(["worker", "--link", name, "--enrollment", ENR])
            .env("HOME", &self.home)
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_STATE_HOME")
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
    assert_eq!(audit.tail(1).unwrap()[0]["event"], "link.revoke");
    let again = f
        .tmgr()
        .args(["link", "revoke", "laptop"])
        .output()
        .unwrap();
    assert_eq!(again.status.code(), Some(3));
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
        json!(["hello", "runs.list", "runs.get", "runs.create", "runs.halt"])
    );

    let repo = f.repo.to_str().unwrap();
    let ok = c
        .call(
            "runs.create",
            json!({"task":"TM-512","repo":repo,"agent":"codex"}),
        )
        .unwrap();
    assert_eq!(
        reply_err(ok),
        ("not_available".into(), "runner not installed".into())
    );
    let listed = c.call("runs.list", Value::Null).unwrap();
    assert_eq!(reply_err(listed).0, "not_available");
    let wrong_agent = c
        .call(
            "runs.create",
            json!({"task":"TM-512","repo":repo,"agent":"claude-code"}),
        )
        .unwrap();
    assert_eq!(reply_err(wrong_agent).0, "forbidden");
    let outside = c
        .call(
            "runs.create",
            json!({"task":"TM-512","repo":"/","agent":"codex"}),
        )
        .unwrap();
    assert_eq!(reply_err(outside).0, "forbidden");
    let bypass = c
        .call(
            "runs.create",
            json!({"task":"TM-530","repo":repo,"agent":"codex","mode":"bypass"}),
        )
        .unwrap();
    assert_eq!(reply_err(bypass).1, "bypass not allowed");
    let on = c
        .call(
            "runs.create",
            json!({"task":"TM-530","repo":repo,"agent":"codex","on":"spark"}),
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
    let still_alive = c.call("runs.halt", json!({"run":"r-1"})).unwrap();
    assert_eq!(reply_err(still_alive).0, "not_available");
    drop(c);

    let audit = Audit::new(&f.paths.audit_dir());
    let log = audit.tail(100).unwrap();
    assert_eq!(log[0]["event"], "worker.connect");
    assert_eq!(log[0]["client"], "192.0.2.5");
    let accepted = log.iter().filter(|e| e["decision"] == "accepted").count();
    assert_eq!(accepted, 4);
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
    let r = c.call("runs.list", Value::Null).unwrap();
    assert_eq!(reply_err(r).0, "revoked");
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
