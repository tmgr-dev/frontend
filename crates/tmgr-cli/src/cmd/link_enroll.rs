use serde::Deserialize;
use serde_json::{json, Value};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tmgr_chat_relay::engine::OLD_CODE_MSG;
use tmgr_chat_relay::fields::{check_device_id, check_fp};
use tmgr_chat_relay::trust::decode_join_code;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::text::{sanitize_line, truncate_chars};
use tmgr_ipc::mgr_rpc::valid_machine_id;
use tmgr_link::enroll::{Enroller, Identity};
use tmgr_link::proto::Reply;
use tmgr_runner::mgr_rpc::{pairing, policy, top};

const FIRST_LINE_WAIT: Duration = Duration::from_secs(30);
const CALL_WAIT: Duration = Duration::from_secs(60);
const NOT_SAME_USER: &str = "the follower is not logged in as the same user, or the code expired";
const MAX_REPLY_CHARS: usize = 200;

pub trait JoinSpawner {
    fn first_line(&self, code: &str, wait: Duration) -> std::result::Result<String, String>;
}

pub struct ExeSpawner {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub scratch: PathBuf,
}

impl ExeSpawner {
    pub fn chat_join(scratch: PathBuf) -> std::io::Result<ExeSpawner> {
        Ok(ExeSpawner {
            program: std::env::current_exe()?,
            args: vec!["chat".into(), "join".into(), "--json".into()],
            scratch,
        })
    }
}

fn read_new(file: &mut std::fs::File, buf: &mut Vec<u8>) {
    let mut chunk = [0u8; 1024];
    while let Ok(n) = file.read(&mut chunk) {
        if n == 0 || buf.len() > 64 * 1024 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

fn first_complete_line(buf: &[u8]) -> Option<String> {
    let end = buf.iter().position(|b| *b == b'\n')?;
    Some(String::from_utf8_lossy(&buf[..end]).trim().to_string())
}

impl JoinSpawner for ExeSpawner {
    fn first_line(&self, code: &str, wait: Duration) -> std::result::Result<String, String> {
        let out_path = self.scratch.join(format!(
            ".enroll-{}-{}.out",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&out_path)
            .map_err(|e| format!("cannot start the join: {e}"))?;
        let mut reader = std::fs::File::open(&out_path).map_err(|e| e.to_string())?;
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::from(out))
            .stderr(Stdio::null());
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let spawned = cmd.spawn();
        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                let _ = std::fs::remove_file(&out_path);
                return Err(format!("cannot start the join: {e}"));
            }
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(format!("{code}\n").as_bytes());
        }
        let deadline = Instant::now() + wait;
        let mut buf = Vec::new();
        let result = loop {
            read_new(&mut reader, &mut buf);
            if let Some(line) = first_complete_line(&buf) {
                break Ok(line);
            }
            if let Ok(Some(status)) = child.try_wait() {
                read_new(&mut reader, &mut buf);
                break first_complete_line(&buf).ok_or_else(|| {
                    format!(
                        "chat join on the follower failed (exit {}): {NOT_SAME_USER}",
                        status.code().unwrap_or(-1)
                    )
                });
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                break Err("the follower did not register with the chat relay in time".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let _ = std::fs::remove_file(&out_path);
        if result.is_ok() {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        result
    }
}

#[derive(Deserialize)]
struct JoinLine {
    id: String,
    device_id: String,
    fp: String,
}

fn parse_identity(line: &str) -> std::result::Result<Identity, String> {
    let bad = || "the join reported an invalid identity".to_string();
    let j: JoinLine = serde_json::from_str(line).map_err(|_| bad())?;
    if !valid_machine_id(&j.id)
        || check_device_id(&j.device_id).is_err()
        || check_fp(&j.fp).is_err()
    {
        return Err(bad());
    }
    Ok(Identity {
        id: j.id,
        device_id: j.device_id,
        fp: j.fp,
    })
}

pub fn enroll_with(
    dir: &Path,
    spawner: &dyn JoinSpawner,
    code: &str,
    leader_machine: &str,
) -> std::result::Result<Identity, String> {
    let decoded = decode_join_code(code).map_err(|_| "that is not a valid join code")?;
    if decoded.version < 2 {
        return Err(OLD_CODE_MSG.into());
    }
    let own = tmgr_runner::chat::machine_id(dir).map_err(|e| format!("machine id: {e}"))?;
    let line = spawner.first_line(code, FIRST_LINE_WAIT)?;
    let identity = parse_identity(&line)?;
    if identity.id != own {
        return Err("the join reported a different machine".into());
    }
    policy::enroll_leader(dir, leader_machine)?;
    Ok(identity)
}

pub struct CliEnroller {
    dir: PathBuf,
    spawner: ExeSpawner,
}

impl CliEnroller {
    pub fn new() -> Option<CliEnroller> {
        let dir = tmgr_ipc::dirs::runner_dir().ok()?;
        let spawner = ExeSpawner::chat_join(dir.clone()).ok()?;
        Some(CliEnroller { dir, spawner })
    }
}

impl Enroller for CliEnroller {
    fn enroll(&self, code: &str, leader_machine: &str) -> std::result::Result<Identity, String> {
        enroll_with(&self.dir, &self.spawner, code, leader_machine)
    }
}

fn kind_of(code: &str) -> Kind {
    match code {
        "forbidden" | "revoked" => Kind::Forbidden,
        "bad_params" | "unknown_method" | "bad_request" => Kind::Usage,
        "refused" | "busy" | "rate_limited" | "too_many_requests" => Kind::Conflict,
        "not_available" | "audit_unavailable" => Kind::Offline,
        _ => Kind::Link,
    }
}

pub fn precheck(hello: &Value) -> Result<()> {
    let supported = hello
        .get("methods")
        .and_then(Value::as_array)
        .is_some_and(|m| m.iter().any(|x| x == tmgr_link::enroll::METHOD));
    if !supported {
        return Err(
            Error::conflict("the follower's tmgr is too old for chat enrolment")
                .hint("update tmgr on the follower"),
        );
    }
    if hello.pointer("/scope/chat_enroll") != Some(&Value::Bool(true)) {
        return Err(Error::new(
            Kind::Forbidden,
            "the follower's scope does not allow chat enrolment",
        )
        .hint("on the follower run: tmgr link scope <leader name> --chat-enroll"));
    }
    Ok(())
}

pub fn reply_data(reply: Reply) -> Result<Value> {
    match reply {
        Reply::Ok(v) => Ok(v),
        Reply::Err { code, message } => {
            let text = truncate_chars(&sanitize_line(&message), MAX_REPLY_CHARS).0;
            let err = Error::new(kind_of(&code), text);
            Err(match code.as_str() {
                "forbidden" => err.hint(
                    "what a leader may do is set on the follower: tmgr link scope <leader name> --chat-enroll",
                ),
                _ => err,
            })
        }
    }
}

pub fn finish(dir: &Path, follower: &str, reply: &Value) -> Result<Value> {
    let identity = reply
        .as_object()
        .and_then(|_| {
            let line = json!({
                "id": reply.get("id")?,
                "device_id": reply.get("device_id")?,
                "fp": reply.get("fp")?,
            });
            parse_identity(&line.to_string()).ok()
        })
        .ok_or_else(|| Error::new(Kind::Link, "the follower sent an invalid identity"))?;
    let line = json!({"id": identity.id, "device_id": identity.device_id, "fp": identity.fp});
    pairing::ensure_machine(dir, &line.to_string(), follower).map_err(Error::conflict)?;
    top::apply_top(dir, Some(true))
        .map_err(|e| Error::internal(format!("cannot update the top manager flag: {e}")))?;
    Ok(json!({
        "id": identity.id,
        "device_id": identity.device_id,
        "fp": identity.fp,
        "follower": follower,
    }))
}

pub fn leader_call(
    client: &mut tmgr_link::leader::Client,
    code: &str,
    leader_machine: &str,
) -> Result<Value> {
    let hello = client.hello()?;
    precheck(&hello)?;
    let reply = client.call_within(
        tmgr_link::enroll::METHOD,
        json!({"code": code, "leader_machine": leader_machine}),
        CALL_WAIT,
    )?;
    reply_data(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::os::unix::fs::PermissionsExt;
    use tmgr_chat_relay::trust::{encode_join_code, JoinCode};
    use tmgr_runner::mgr_rpc::policy::{Policy, ENROLL_ALLOW, POLICY_FILE};
    use tmgr_runner::mgr_rpc::Machines;

    const LEADER: &str = "mx_0123456789abcdef";
    const DEV: &str = "11111111-2222-3333-4444-555555555555";

    fn dir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("en")
            .tempdir_in("/tmp")
            .unwrap()
    }

    fn code(version: u8) -> String {
        encode_join_code(&JoinCode {
            version,
            kind: (version == 2).then(|| "desktop".to_string()),
            user_id: 42,
            device_id: "00000000-0000-4000-8000-00000000000a".into(),
            fp: "ab".repeat(32),
            secret: [9u8; 32],
            exp_ms: 1_800_000_600_000,
        })
        .unwrap()
    }

    struct Fake {
        calls: RefCell<Vec<String>>,
        reply: std::result::Result<String, String>,
    }

    impl JoinSpawner for Fake {
        fn first_line(&self, code: &str, _: Duration) -> std::result::Result<String, String> {
            self.calls.borrow_mut().push(code.to_string());
            self.reply.clone()
        }
    }

    fn fake(line: std::result::Result<String, String>) -> Fake {
        Fake {
            calls: RefCell::default(),
            reply: line,
        }
    }

    fn id_line(id: &str, extra: &str) -> String {
        format!(
            r#"{{"id":"{id}","device_id":"{DEV}","fp":"{}"{extra}}}"#,
            "cd".repeat(32)
        )
    }

    #[test]
    fn a_good_join_writes_the_policy_and_returns_the_identity() {
        let d = dir();
        let own = tmgr_runner::chat::machine_id(d.path()).unwrap();
        let f = fake(Ok(id_line(&own, r#","already_joined":false"#)));
        let c = code(2);
        let i = enroll_with(d.path(), &f, &c, LEADER).unwrap();
        assert_eq!((i.id.as_str(), i.device_id.as_str()), (own.as_str(), DEV));
        assert_eq!(f.calls.borrow().as_slice(), std::slice::from_ref(&c));
        let p = Policy::try_load(d.path()).unwrap();
        assert!(p.accepts(LEADER));
        assert!(ENROLL_ALLOW.iter().all(|op| p.allows(*op)));
        let mode = std::fs::metadata(d.path().join(POLICY_FILE))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        enroll_with(d.path(), &f, &c, LEADER).unwrap();
        assert_eq!(Policy::try_load(d.path()).unwrap(), p);
    }

    #[test]
    fn a_bad_code_never_reaches_the_child_and_writes_nothing() {
        let d = dir();
        let f = fake(Ok(String::new()));
        for bad in ["nope", "tmgr-chat-join:v2:@@@", &code(1)] {
            let e = enroll_with(d.path(), &f, bad, LEADER).unwrap_err();
            assert!(!e.contains("@@@"), "{e}");
        }
        assert!(f.calls.borrow().is_empty());
        assert!(!d.path().join(POLICY_FILE).exists());
    }

    #[test]
    fn a_failed_child_or_a_bad_identity_writes_no_policy() {
        let d = dir();
        let own = tmgr_runner::chat::machine_id(d.path()).unwrap();
        let cases = [
            Err("boom".to_string()),
            Ok("not json".to_string()),
            Ok(id_line("mx_ABC", "")),
            Ok(id_line("mx_00000000000000ff", "")),
            Ok(format!(r#"{{"id":"{own}","device_id":"x","fp":"y"}}"#)),
        ];
        for r in cases {
            let f = fake(r);
            assert!(enroll_with(d.path(), &f, &code(2), LEADER).is_err());
        }
        assert!(!d.path().join(POLICY_FILE).exists());
    }

    #[test]
    fn an_invalid_existing_policy_is_an_error_and_stays() {
        let d = dir();
        let own = tmgr_runner::chat::machine_id(d.path()).unwrap();
        std::fs::write(d.path().join(POLICY_FILE), "accept_from = [").unwrap();
        std::fs::set_permissions(
            d.path().join(POLICY_FILE),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        let f = fake(Ok(id_line(&own, "")));
        assert!(enroll_with(d.path(), &f, &code(2), LEADER).is_err());
        assert_eq!(
            std::fs::read_to_string(d.path().join(POLICY_FILE)).unwrap(),
            "accept_from = ["
        );
    }

    fn script(body: &str, scratch: &Path) -> ExeSpawner {
        ExeSpawner {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), body.into(), "sh".into()],
            scratch: scratch.to_path_buf(),
        }
    }

    #[test]
    fn the_real_spawner_feeds_the_code_on_stdin_only_and_returns_the_first_line() {
        let d = dir();
        let seen = d.path().join("seen");
        let body = format!(
            "IFS= read -r c; printf '%s' \"$c\" > {}; echo '{{\"id\":1}}'; sleep 1; echo late",
            seen.display()
        );
        let s = script(&body, d.path());
        let line = s
            .first_line("tmgr-chat-join:v2:CODE", Duration::from_secs(5))
            .unwrap();
        assert_eq!(line, r#"{"id":1}"#);
        assert_eq!(
            std::fs::read_to_string(&seen).unwrap(),
            "tmgr-chat-join:v2:CODE"
        );
        let leftovers = std::fs::read_dir(d.path())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".enroll")
            })
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn a_child_that_exits_before_printing_gives_the_clear_error() {
        let d = dir();
        let s = script("cat >/dev/null; exit 3", d.path());
        let e = s.first_line("x", Duration::from_secs(5)).unwrap_err();
        assert!(e.contains("not logged in as the same user"), "{e}");
        assert!(e.contains("exit 3"), "{e}");
    }

    #[test]
    fn a_silent_child_times_out_and_is_stopped() {
        let d = dir();
        let s = script("sleep 30", d.path());
        let started = Instant::now();
        let e = s.first_line("x", Duration::from_millis(300)).unwrap_err();
        assert!(e.contains("in time"), "{e}");
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn precheck_needs_the_method_and_the_scope_flag() {
        let ok = json!({"methods": ["hello", "chat.enroll"], "scope": {"chat_enroll": true}});
        assert!(precheck(&ok).is_ok());
        let old = json!({"methods": ["hello"], "scope": {}});
        assert!(precheck(&old).unwrap_err().to_string().contains("too old"));
        for scope in [json!({"chat_enroll": false}), json!({})] {
            let h = json!({"methods": ["chat.enroll"], "scope": scope});
            let e = precheck(&h).unwrap_err();
            assert!(e.to_string().contains("does not allow"), "{e}");
        }
    }

    #[test]
    fn follower_errors_map_to_clear_kinds() {
        let e = reply_data(Reply::Err {
            code: "forbidden".into(),
            message: "no".into(),
        })
        .unwrap_err();
        assert_eq!(e.kind, Kind::Forbidden);
        assert!(e.hint.is_some());
        let e = reply_data(Reply::Err {
            code: "refused".into(),
            message: "the follower is not logged in".into(),
        })
        .unwrap_err();
        assert_eq!(e.kind, Kind::Conflict);
        assert!(reply_data(Reply::Ok(json!({"id": 1}))).is_ok());
    }

    #[test]
    fn finishing_writes_the_row_makes_the_leader_top_and_is_idempotent() {
        let d = dir();
        let reply = json!({"id": LEADER, "device_id": DEV, "fp": "cd".repeat(32)});
        let v = finish(d.path(), "studio", &reply).unwrap();
        assert_eq!(
            v,
            json!({"id": LEADER, "device_id": DEV, "fp": "cd".repeat(32), "follower": "studio"})
        );
        assert!(top::is_top(d.path()));
        let text = std::fs::read_to_string(d.path().join("machines.json")).unwrap();
        let m = Machines::parse(&text).unwrap();
        assert_eq!(m.machine_for(DEV, &"cd".repeat(32)), Some(LEADER));
        assert_eq!(m.rows(), [(LEADER, "studio")]);
        finish(d.path(), "studio", &reply).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.path().join("machines.json")).unwrap(),
            text
        );
    }

    #[test]
    fn a_repaired_follower_replaces_its_row() {
        let d = dir();
        let r1 = json!({"id": LEADER, "device_id": DEV, "fp": "cd".repeat(32)});
        finish(d.path(), "studio", &r1).unwrap();
        let dev2 = "99999999-2222-3333-4444-555555555555";
        let r2 = json!({"id": LEADER, "device_id": dev2, "fp": "ef".repeat(32)});
        finish(d.path(), "studio", &r2).unwrap();
        let m = Machines::load(d.path());
        assert_eq!(m.machine_for(dev2, &"ef".repeat(32)), Some(LEADER));
        assert_eq!(m.machine_for(DEV, &"cd".repeat(32)), None);
    }

    #[test]
    fn an_invalid_reply_writes_nothing() {
        let d = dir();
        for r in [
            json!({}),
            json!({"id": "x", "device_id": DEV, "fp": "cd".repeat(32)}),
            json!({"id": LEADER, "device_id": "x", "fp": "cd".repeat(32)}),
            json!([1]),
        ] {
            assert!(finish(d.path(), "studio", &r).is_err(), "{r}");
        }
        assert!(!d.path().join("machines.json").exists());
        assert!(!top::is_top(d.path()));
    }
}
