use crate::audit::Audit;
use crate::grammar;
use crate::proto::{self, Channel, FrameError};
use crate::runner::{Backend, BackendError};
use crate::scope::Scope;
use crate::sha256_hex;
use crate::store::{Paths, Store};
use crate::taskref::{LocalRefs, TaskResolver};
use serde_json::{json, Value};
use std::ffi::OsString;
use std::net::IpAddr;
use tmgr_core::error::Result;

pub const MAX_REQUESTS: u32 = 256;
pub const REQUESTS_PER_MINUTE: u32 = 120;
pub const MAX_WORKERS: usize = 4;

#[derive(Debug, Clone)]
pub struct Limits {
    pub max_requests: u32,
    pub per_minute: u32,
    pub max_workers: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_requests: MAX_REQUESTS,
            per_minute: REQUESTS_PER_MINUTE,
            max_workers: MAX_WORKERS,
        }
    }
}

pub type Connector<'a> = &'a (dyn Fn() -> std::result::Result<Box<dyn Backend>, BackendError> + 'a);

pub struct Runtime<'a> {
    pub connect: Connector<'a>,
    pub resolver: &'a dyn TaskResolver,
}

fn no_runner() -> std::result::Result<Box<dyn Backend>, BackendError> {
    Err(BackendError::Unavailable("runner not installed".into()))
}

impl Runtime<'static> {
    pub fn none() -> Runtime<'static> {
        Runtime {
            connect: &no_runner,
            resolver: &LocalRefs,
        }
    }
}

pub fn check_argv(args: &[OsString]) -> Result<(String, String)> {
    let bad =
        || crate::forbidden("tmgr worker accepts exactly: tmgr worker --link NAME --enrollment ID");
    if args.len() != 6 || args[1] != "worker" || args[2] != "--link" || args[4] != "--enrollment" {
        return Err(bad());
    }
    let name = args[3].to_str().ok_or_else(bad)?;
    grammar::link_name(name).map_err(|_| bad())?;
    let enrollment = args[5].to_str().ok_or_else(bad)?;
    if !grammar::enrollment_id(enrollment) {
        return Err(bad());
    }
    Ok((name.to_string(), enrollment.to_string()))
}

pub struct Watchdog(#[allow(dead_code)] std::sync::mpsc::Sender<()>);

pub fn watchdog(after: std::time::Duration, on_expire: impl FnOnce() + Send + 'static) -> Watchdog {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        if let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(after) {
            on_expire();
        }
    });
    Watchdog(tx)
}

pub struct Slot(#[allow(dead_code)] std::fs::File);

impl Slot {
    pub fn acquire(paths: &Paths, name: &str, max: usize) -> Result<Option<Slot>> {
        use std::os::unix::fs::OpenOptionsExt;
        let dir = paths.state.join("link-slots");
        tmgr_core::config::ensure_private_dir(&dir)?;
        for i in 0..max {
            let f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .open(dir.join(format!("{name}.{i}.lock")))?;
            if f.try_lock().is_ok() {
                return Ok(Some(Slot(f)));
            }
        }
        Ok(None)
    }
}

#[derive(Debug, Default, Clone)]
pub struct Env {
    pub ssh_connection: Option<String>,
    pub original_command: Option<Vec<u8>>,
}

impl Env {
    pub fn from_process() -> Env {
        use std::os::unix::ffi::OsStrExt;
        Env {
            ssh_connection: std::env::var("SSH_CONNECTION").ok(),
            original_command: std::env::var_os("SSH_ORIGINAL_COMMAND")
                .map(|v| v.as_bytes().to_vec()),
        }
    }
}

pub fn client_ip(ssh_connection: &str) -> Option<IpAddr> {
    let fields: Vec<&str> = ssh_connection.split_whitespace().collect();
    if fields.len() != 4 {
        return None;
    }
    fields[0].parse().ok()
}

pub fn cidr_contains(cidr: &str, ip: IpAddr) -> bool {
    let (addr, prefix) = match cidr.split_once('/') {
        Some((a, p)) => (a, p.parse::<u32>().ok()),
        None => (cidr, None),
    };
    let Ok(net) = addr.parse::<IpAddr>() else {
        return false;
    };
    let (a, b, bits) = match (net, ip) {
        (IpAddr::V4(n), IpAddr::V4(i)) => (u32::from(n) as u128, u32::from(i) as u128, 32u32),
        (IpAddr::V6(n), IpAddr::V6(i)) => (u128::from(n), u128::from(i), 128u32),
        _ => return false,
    };
    let prefix = prefix.unwrap_or(bits);
    if prefix > bits {
        return false;
    }
    if prefix == 0 {
        return true;
    }
    let shift = bits - prefix;
    (a >> shift) == (b >> shift)
}

#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    pub requests: u32,
}

pub(crate) struct Conn<'a> {
    pub(crate) paths: &'a Paths,
    pub(crate) audit: Audit,
    pub(crate) name: &'a str,
    pub(crate) enrollment: &'a str,
    client: IpAddr,
    pub(crate) fingerprint: String,
    pub(crate) rt: &'a Runtime<'a>,
}

impl Conn<'_> {
    pub(crate) fn record(
        &self,
        method: &str,
        args: Value,
        decision: &str,
        reason: &str,
    ) -> Result<()> {
        self.audit.append(
            "worker.request",
            json!({
                "link": self.name,
                "fp": self.fingerprint,
                "method": method,
                "args": args,
                "decision": decision,
                "reason": reason,
            }),
        )
    }

    pub(crate) fn current(&self) -> Result<std::result::Result<Scope, &'static str>> {
        Ok(self.current_gen()?.map(|(scope, _)| scope))
    }

    pub(crate) fn current_gen(&self) -> Result<std::result::Result<(Scope, u64), &'static str>> {
        let store = Store::load(self.paths)?;
        if crate::follower::is_pending(self.paths, self.name) {
            return Ok(Err("link blocked after a failed rollback"));
        }
        let Some(leader) = store.leaders.get(self.name) else {
            return Ok(Err("link revoked"));
        };
        if leader.enrollment != self.enrollment {
            return Ok(Err("link re-enrolled"));
        }
        if !leader.scope.from.is_empty()
            && !leader
                .scope
                .from
                .iter()
                .any(|c| cidr_contains(c, self.client))
        {
            return Ok(Err("client address outside from"));
        }
        Ok(Ok((leader.scope.clone(), store.generation(self.name))))
    }
}

pub(crate) fn say(chan: &mut dyn Channel, payload: Vec<u8>) -> bool {
    chan.send(&payload).is_ok()
}

pub fn serve(
    paths: &Paths,
    name: &str,
    enrollment: &str,
    env: &Env,
    chan: &mut dyn Channel,
    limits: &Limits,
) -> Result<Outcome> {
    serve_with(paths, name, enrollment, env, chan, limits, &Runtime::none())
}

pub fn serve_with(
    paths: &Paths,
    name: &str,
    enrollment: &str,
    env: &Env,
    chan: &mut dyn Channel,
    limits: &Limits,
    rt: &Runtime,
) -> Result<Outcome> {
    grammar::link_name(name)?;
    let refused = || crate::forbidden("refused");
    if !grammar::enrollment_id(enrollment) {
        return Err(refused());
    }
    let ssh_connection = env.ssh_connection.as_deref().ok_or_else(refused)?;
    let client = client_ip(ssh_connection).ok_or_else(refused)?;
    let store = Store::load(paths)?;
    let leader = store.leaders.get(name).ok_or_else(refused)?;
    let audit = Audit::new(&paths.audit_dir());
    if crate::follower::is_pending(paths, name) {
        let _ = audit.append(
            "worker.refused",
            json!({"link": name, "fp": leader.fingerprint, "reason": "link blocked after a failed rollback"}),
        );
        return Err(refused());
    }
    if leader.enrollment != enrollment {
        let _ = audit.append(
            "worker.refused",
            json!({"link": name, "fp": leader.fingerprint, "reason": "enrollment mismatch"}),
        );
        return Err(refused());
    }
    if !leader.scope.from.is_empty() && !leader.scope.from.iter().any(|c| cidr_contains(c, client))
    {
        let _ = audit.append(
            "worker.refused",
            json!({"link": name, "fp": leader.fingerprint, "reason": "client address outside from"}),
        );
        return Err(refused());
    }
    let Some(_slot) = Slot::acquire(paths, name, limits.max_workers)? else {
        let _ = audit.append(
            "worker.busy",
            json!({"link": name, "fp": leader.fingerprint, "max_workers": limits.max_workers}),
        );
        return Err(refused());
    };
    let Ok(_registered) = crate::registry::register(paths, name, enrollment) else {
        let _ = audit.append(
            "worker.refused",
            json!({"link": name, "fp": leader.fingerprint, "reason": "cannot register the worker process"}),
        );
        return Err(refused());
    };
    let conn = Conn {
        paths,
        audit,
        name,
        enrollment,
        client,
        fingerprint: leader.fingerprint.clone(),
        rt,
    };
    let mut state = State { backend: None };
    let max_requests = limits.max_requests;
    let mut recent: std::collections::VecDeque<std::time::Instant> = Default::default();
    let original = env.original_command.as_deref();
    conn.audit.append(
        "worker.connect",
        json!({
            "link": name,
            "fp": conn.fingerprint,
            "client": client.to_string(),
            "orig_cmd_len": original.map_or(0, <[u8]>::len),
            "orig_cmd_sha256": original.map(sha256_hex),
        }),
    )?;
    let mut requests = 0u32;
    loop {
        let frame = match chan.recv() {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                let code = match e {
                    FrameError::Oversize(_) => "frame_too_large",
                    FrameError::Timeout => "timeout",
                    _ => "bad_frame",
                };
                let _ = conn.audit.append(
                    "worker.frame_rejected",
                    json!({"link": name, "reason": e.to_string()}),
                );
                say(chan, proto::error(None, code, "the connection is closed"));
                break;
            }
        };
        let now = std::time::Instant::now();
        while recent
            .front()
            .is_some_and(|t| now.duration_since(*t) >= std::time::Duration::from_secs(60))
        {
            recent.pop_front();
        }
        if recent.len() >= limits.per_minute as usize {
            let _ = conn.audit.append(
                "worker.rate_limited",
                json!({"link": name, "fp": conn.fingerprint, "per_minute": limits.per_minute}),
            );
            say(
                chan,
                proto::error(None, "rate_limited", "too many requests"),
            );
            break;
        }
        recent.push_back(now);
        requests += 1;
        if requests > max_requests {
            let _ = conn.audit.append(
                "worker.request_cap",
                json!({"link": name, "cap": max_requests}),
            );
            say(
                chan,
                proto::error(None, "too_many_requests", "request limit reached"),
            );
            requests -= 1;
            break;
        }
        match handle(&conn, &mut state, chan, &frame) {
            Step::Reply(r) => {
                if !say(chan, r) {
                    break;
                }
            }
            Step::Close(r) => {
                say(chan, r);
                break;
            }
            Step::Done => break,
        }
    }
    Ok(Outcome { requests })
}

pub(crate) enum Step {
    Reply(Vec<u8>),
    Close(Vec<u8>),
    Done,
}

pub(crate) struct State {
    pub(crate) backend: Option<Box<dyn Backend>>,
}

fn handle(conn: &Conn, state: &mut State, chan: &mut dyn Channel, frame: &[u8]) -> Step {
    let audit_down = || Step::Close(proto::error(None, "audit_unavailable", "request refused"));
    let req = match proto::parse_request(frame) {
        Ok(r) => r,
        Err(why) => {
            if conn.record("?", json!({}), "rejected", why).is_err() {
                return audit_down();
            }
            return Step::Reply(proto::error(None, "bad_request", why));
        }
    };
    let id = Some(req.id);
    let scope = match conn.current() {
        Ok(Ok(s)) => s,
        Ok(Err(reason)) => {
            let _ = conn.record(&req.method, json!({}), "rejected", reason);
            let code = if reason == "client address outside from" {
                "forbidden"
            } else {
                "revoked"
            };
            return Step::Close(proto::error(id, code, "this link is no longer trusted"));
        }
        Err(_) => return Step::Close(proto::error(id, "unavailable", "request refused")),
    };
    let method = req.method.as_str();
    match method {
        "hello" => {
            let want = req.params.get("protocol").and_then(Value::as_u64);
            if want.is_some_and(|p| p != u64::from(proto::PROTOCOL)) {
                if conn
                    .record(method, json!({}), "rejected", "protocol mismatch")
                    .is_err()
                {
                    return audit_down();
                }
                return Step::Reply(proto::error(
                    id,
                    "protocol_mismatch",
                    "unsupported protocol",
                ));
            }
            if conn.record(method, json!({}), "accepted", "").is_err() {
                return audit_down();
            }
            Step::Reply(proto::ok(
                id,
                json!({
                    "protocol": proto::PROTOCOL,
                    "role": "follower",
                    "version": env!("CARGO_PKG_VERSION"),
                    "methods": proto::METHODS,
                }),
            ))
        }
        "runs.list" | "runs.get" | "runs.create" | "runs.halt" | "attach.open" => {
            crate::runs::run_method(conn, state, chan, &req, &scope)
        }
        other => {
            let shown = tmgr_core::text::truncate_chars(
                &other
                    .chars()
                    .filter(|c| !c.is_control())
                    .collect::<String>(),
                32,
            )
            .0;
            if conn
                .record(&shown, json!({}), "rejected", "unknown method")
                .is_err()
            {
                return audit_down();
            }
            Step::Reply(proto::error(id, "unknown_method", "unknown method"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::Framed;
    use crate::store::Leader;
    use std::io::Cursor;

    const ENR: &str = "0123456789abcdef0123456789abcdef";

    fn lim(n: u32) -> Limits {
        Limits {
            max_requests: n,
            ..Limits::default()
        }
    }

    fn args(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    #[test]
    fn argv_must_have_the_exact_shape() {
        assert_eq!(
            check_argv(&args(&[
                "/x/tmgr",
                "worker",
                "--link",
                "laptop",
                "--enrollment",
                ENR
            ]))
            .unwrap(),
            ("laptop".to_string(), ENR.to_string())
        );
        for bad in [
            vec!["tmgr", "worker"],
            vec!["tmgr", "worker", "--link"],
            vec!["tmgr", "worker", "--link", "laptop"],
            vec!["tmgr", "worker", "--link", "laptop", "--enrollment"],
            vec![
                "tmgr",
                "worker",
                "--link",
                "laptop",
                "--enrollment",
                "short",
            ],
            vec![
                "tmgr",
                "worker",
                "--link",
                "laptop",
                "--enrollment",
                &"A".repeat(32),
            ],
            vec!["tmgr", "worker", "--enrollment", ENR, "--link", "laptop"],
            vec![
                "tmgr",
                "worker",
                "--link",
                "laptop",
                "--enrollment",
                ENR,
                "--json",
            ],
            vec![
                "tmgr",
                "--json",
                "worker",
                "--link",
                "laptop",
                "--enrollment",
                ENR,
            ],
            vec!["tmgr", "worker", "--link=laptop", "x"],
            vec!["tmgr", "worker", "-l", "laptop"],
            vec!["tmgr", "worker", "--link", "Laptop", "--enrollment", ENR],
            vec!["tmgr", "worker", "--link", "a b", "--enrollment", ENR],
            vec!["tmgr", "worker", "--link", "-x", "--enrollment", ENR],
            vec!["tmgr", "link", "--link", "laptop", "--enrollment", ENR],
        ] {
            assert_eq!(
                check_argv(&args(&bad)).unwrap_err().exit_code(),
                5,
                "{bad:?}"
            );
        }
    }

    #[test]
    fn ssh_connection_is_parsed_strictly() {
        assert_eq!(
            client_ip("192.0.2.5 5555 192.0.2.1 22"),
            Some("192.0.2.5".parse().unwrap())
        );
        assert_eq!(
            client_ip("2001:db8::5 5555 2001:db8::1 22")
                .unwrap()
                .to_string(),
            "2001:db8::5"
        );
        assert!(client_ip("").is_none());
        assert!(client_ip("garbage").is_none());
        assert!(client_ip("a b c d").is_none());
    }

    #[test]
    fn cidr_membership() {
        let ip: IpAddr = "192.0.2.77".parse().unwrap();
        assert!(cidr_contains("192.0.2.0/24", ip));
        assert!(cidr_contains("192.0.2.77", ip));
        assert!(cidr_contains("0.0.0.0/0", ip));
        assert!(!cidr_contains("198.51.100.0/24", ip));
        assert!(!cidr_contains("2001:db8::/32", ip));
        assert!(cidr_contains(
            "2001:db8::/32",
            "2001:db8::9".parse().unwrap()
        ));
        assert!(!cidr_contains("192.0.2.0/33", ip));
    }

    struct Fx {
        _d: tempfile::TempDir,
        paths: Paths,
        repo: String,
    }

    fn fx() -> Fx {
        let d = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(d.path()).unwrap();
        let paths = Paths::under(&root);
        let repo = root.join("repo");
        std::fs::create_dir(&repo).unwrap();
        let repo = repo.to_str().unwrap().to_string();
        Store::update(&paths, |s| {
            s.leaders.insert(
                "laptop".into(),
                Leader {
                    key: "AAAA".into(),
                    fingerprint: "SHA256:test".into(),
                    added: "2026-10-13T00:00:00Z".into(),
                    scope: Scope {
                        repos: vec![repo.clone()],
                        agents: vec!["codex".into()],
                        models: vec![],
                        max: 2,
                        from: vec![],
                        allow_attach_input: false,
                    },
                    enrollment: ENR.into(),
                    authorized_keys: String::new(),
                },
            );
            Ok(())
        })
        .unwrap();
        Fx { _d: d, paths, repo }
    }

    fn env() -> Env {
        Env {
            ssh_connection: Some("192.0.2.5 5555 192.0.2.1 22".into()),
            original_command: None,
        }
    }

    fn frames(items: &[Value]) -> Vec<u8> {
        let mut out = Vec::new();
        for v in items {
            let b = serde_json::to_vec(v).unwrap();
            out.extend_from_slice(&(b.len() as u32).to_be_bytes());
            out.extend_from_slice(&b);
        }
        out
    }

    fn run(f: &Fx, input: Vec<u8>, cap: u32) -> (Result<Outcome>, Vec<Value>) {
        let mut out = Vec::new();
        let result = {
            let mut chan = Framed::new(Cursor::new(input), &mut out);
            serve(&f.paths, "laptop", ENR, &env(), &mut chan, &lim(cap))
        };
        let mut replies = Vec::new();
        let mut cur = Cursor::new(out);
        while let Ok(Some(frame)) = proto::read_frame(&mut cur, proto::MAX_FRAME) {
            replies.push(serde_json::from_slice(&frame).unwrap());
        }
        (result, replies)
    }

    fn events(f: &Fx) -> Vec<Value> {
        Audit::new(&f.paths.audit_dir()).tail(100).unwrap()
    }

    #[test]
    fn hello_lists_only_the_worker_methods() {
        let f = fx();
        let (r, replies) = run(
            &f,
            frames(&[json!({"id":1,"method":"hello","params":{"protocol":1}})]),
            10,
        );
        assert_eq!(r.unwrap().requests, 1);
        assert_eq!(replies[0]["result"]["protocol"], 1);
        assert_eq!(replies[0]["result"]["methods"], json!(proto::METHODS));
        assert_eq!(replies[0]["id"], 1);
    }

    #[test]
    fn run_methods_are_validated_audited_and_not_available() {
        let f = fx();
        let ok = json!({"id":2,"method":"runs.create","params":{"task":"w1:512","repo":f.repo,"agent":"codex"}});
        let bad_agent = json!({"id":3,"method":"runs.create","params":{"task":"w1:512","repo":f.repo,"agent":"claude-code"}});
        let bypass = json!({"id":4,"method":"runs.create","params":{"task":"w1:530","repo":f.repo,"agent":"codex","mode":"bypass"}});
        let list = json!({"id":5,"method":"runs.list"});
        let (_, replies) = run(&f, frames(&[ok, bad_agent, bypass, list]), 10);
        assert_eq!(replies[0]["error"], "not_available");
        assert_eq!(replies[0]["message"], "runner not installed");
        assert_eq!(replies[1]["error"], "forbidden");
        assert_eq!(replies[2]["message"], "bypass not allowed");
        assert_eq!(replies[3]["error"], "not_available");
        let ev = events(&f);
        let decisions: Vec<_> = ev
            .iter()
            .filter(|e| e["event"] == "worker.request")
            .map(|e| e["decision"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            decisions,
            ["accepted", "failed", "rejected", "rejected", "accepted", "failed"]
        );
        assert_eq!(ev[1]["fp"], "SHA256:test");
        assert_eq!(ev[1]["args"]["task"], "w1:512");
        assert!(Audit::new(&f.paths.audit_dir()).verify().unwrap().is_ok());
    }

    #[test]
    fn unknown_and_forbidden_method_families_get_an_error_never_a_panic() {
        let f = fx();
        let methods = [
            "shell.exec",
            "files.read",
            "config.set",
            "link.trust",
            "queue.add",
            "runs.send",
            "events.subscribe",
            "",
            "hello\u{1b}[2J",
        ];
        let items: Vec<Value> = methods
            .iter()
            .enumerate()
            .map(|(i, m)| json!({"id":i,"method":m,"params":{"cmd":"id"}}))
            .collect();
        let (r, replies) = run(&f, frames(&items), 100);
        assert_eq!(r.unwrap().requests as usize, methods.len());
        for (i, rep) in replies.iter().enumerate() {
            assert!(
                rep["error"] == "unknown_method"
                    || rep["error"] == "bad_params"
                    || rep["error"] == "protocol_mismatch"
                    || i == 9,
                "{rep}"
            );
        }
        assert_eq!(replies.len(), methods.len());
    }

    #[test]
    fn garbage_requests_are_answered_and_the_stream_continues() {
        let f = fx();
        let mut input = Vec::new();
        for raw in [
            &b"not json"[..],
            b"[1]",
            br#"{"id":"x","method":"hello"}"#,
            br#"{"id":1,"method":"hello","extra":1}"#,
        ] {
            input.extend_from_slice(&(raw.len() as u32).to_be_bytes());
            input.extend_from_slice(raw);
        }
        input.extend(frames(&[json!({"id":9,"method":"hello"})]));
        let (r, replies) = run(&f, input, 100);
        assert_eq!(r.unwrap().requests, 5);
        assert!(replies[..4].iter().all(|r| r["error"] == "bad_request"));
        assert_eq!(replies[4]["result"]["role"], "follower");
    }

    #[test]
    fn oversize_frames_close_the_connection() {
        let f = fx();
        let mut input = (proto::MAX_FRAME as u32 + 1).to_be_bytes().to_vec();
        input.extend(vec![b'x'; 64]);
        input.extend(frames(&[json!({"id":1,"method":"hello"})]));
        let (r, replies) = run(&f, input, 10);
        assert_eq!(r.unwrap().requests, 0);
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0]["error"], "frame_too_large");
        assert!(events(&f)
            .iter()
            .any(|e| e["event"] == "worker.frame_rejected"));
    }

    #[test]
    fn truncated_and_empty_frames_close_the_connection() {
        let f = fx();
        let (_, replies) = run(&f, vec![0, 0, 0, 9, b'{'], 10);
        assert_eq!(replies[0]["error"], "bad_frame");
        let (_, replies) = run(&f, vec![0, 0, 0, 0], 10);
        assert_eq!(replies[0]["error"], "bad_frame");
    }

    #[test]
    fn the_request_cap_closes_the_connection() {
        let f = fx();
        let items: Vec<Value> = (0..5).map(|i| json!({"id":i,"method":"hello"})).collect();
        let (r, replies) = run(&f, frames(&items), 3);
        assert_eq!(r.unwrap().requests, 3);
        assert_eq!(replies.len(), 4);
        assert_eq!(replies[3]["error"], "too_many_requests");
    }

    #[test]
    fn missing_ssh_connection_or_untrusted_link_is_refused_without_audit() {
        let f = fx();
        let mut sink = Vec::new();
        let mut chan = Framed::new(Cursor::new(vec![]), &mut sink);
        let no_env = Env::default();
        assert_eq!(
            serve(&f.paths, "laptop", ENR, &no_env, &mut chan, &lim(5))
                .unwrap_err()
                .exit_code(),
            5
        );
        let bogus = Env {
            ssh_connection: Some("nonsense".into()),
            original_command: None,
        };
        assert!(serve(&f.paths, "laptop", ENR, &bogus, &mut chan, &lim(5)).is_err());
        assert!(serve(&f.paths, "stranger", ENR, &env(), &mut chan, &lim(5)).is_err());
        assert!(serve(&f.paths, "Bad Name", ENR, &env(), &mut chan, &lim(5)).is_err());
        assert!(sink.is_empty());
        assert!(events(&f).is_empty());
    }

    #[test]
    fn the_client_address_must_match_from() {
        let f = fx();
        Store::update(&f.paths, |s| {
            s.leaders.get_mut("laptop").unwrap().scope.from = vec!["198.51.100.0/24".into()];
            Ok(())
        })
        .unwrap();
        let mut sink = Vec::new();
        let mut chan = Framed::new(Cursor::new(vec![]), &mut sink);
        assert!(serve(&f.paths, "laptop", ENR, &env(), &mut chan, &lim(5)).is_err());
        assert_eq!(events(&f)[0]["event"], "worker.refused");
    }

    #[test]
    fn original_command_is_logged_as_length_and_hash_only() {
        let f = fx();
        let mut e = env();
        e.original_command = Some(b"cat /etc/passwd".to_vec());
        let mut sink = Vec::new();
        let mut chan = Framed::new(Cursor::new(vec![]), &mut sink);
        serve(&f.paths, "laptop", ENR, &e, &mut chan, &lim(5)).unwrap();
        let ev = &events(&f)[0];
        assert_eq!(ev["event"], "worker.connect");
        assert_eq!(ev["orig_cmd_len"], 15);
        assert_eq!(ev["orig_cmd_sha256"], sha256_hex(b"cat /etc/passwd"));
        let raw = std::fs::read_to_string(Audit::new(&f.paths.audit_dir()).current()).unwrap();
        assert!(!raw.contains("passwd"));
        assert!(sink.is_empty());
    }

    #[test]
    fn revocation_takes_effect_on_the_next_request() {
        let f = fx();
        struct Revoking<'a>(&'a Paths, Vec<Vec<u8>>);
        impl Channel for Revoking<'_> {
            fn recv(&mut self) -> std::result::Result<Option<Vec<u8>>, FrameError> {
                Ok(if self.1.is_empty() {
                    None
                } else {
                    Some(self.1.remove(0))
                })
            }
            fn send(&mut self, payload: &[u8]) -> std::result::Result<(), FrameError> {
                if payload.windows(6).any(|w| w == b"\"role\"") {
                    Store::update(self.0, |s| {
                        s.leaders.remove("laptop");
                        Ok(())
                    })
                    .unwrap();
                }
                Ok(())
            }
        }
        let reqs = vec![
            serde_json::to_vec(&json!({"id":1,"method":"hello"})).unwrap(),
            serde_json::to_vec(&json!({"id":2,"method":"runs.list"})).unwrap(),
        ];
        let mut chan = Revoking(&f.paths, reqs);
        let out = serve(&f.paths, "laptop", ENR, &env(), &mut chan, &lim(10)).unwrap();
        assert_eq!(out.requests, 2);
        let last = events(&f).pop().unwrap();
        assert_eq!(last["reason"], "link revoked");
    }

    #[test]
    fn a_different_enrollment_is_refused_and_a_re_trust_kills_old_sessions() {
        let f = fx();
        let mut sink = Vec::new();
        let mut chan = Framed::new(Cursor::new(vec![]), &mut sink);
        let other = "f".repeat(32);
        assert!(serve(&f.paths, "laptop", &other, &env(), &mut chan, &lim(5)).is_err());
        assert_eq!(events(&f)[0]["reason"], "enrollment mismatch");
        struct Retrust<'a>(&'a Paths, Vec<Vec<u8>>);
        impl Channel for Retrust<'_> {
            fn recv(&mut self) -> std::result::Result<Option<Vec<u8>>, FrameError> {
                Ok(if self.1.is_empty() {
                    None
                } else {
                    Some(self.1.remove(0))
                })
            }
            fn send(&mut self, payload: &[u8]) -> std::result::Result<(), FrameError> {
                if payload.windows(6).any(|w| w == b"\"role\"") {
                    Store::update(self.0, |s| {
                        s.leaders.get_mut("laptop").unwrap().enrollment = "e".repeat(32);
                        Ok(())
                    })
                    .unwrap();
                }
                Ok(())
            }
        }
        let reqs = vec![
            serde_json::to_vec(&json!({"id":1,"method":"hello"})).unwrap(),
            serde_json::to_vec(&json!({"id":2,"method":"runs.list"})).unwrap(),
        ];
        let mut chan = Retrust(&f.paths, reqs);
        serve(&f.paths, "laptop", ENR, &env(), &mut chan, &lim(10)).unwrap();
        assert_eq!(events(&f).pop().unwrap()["reason"], "link re-enrolled");
    }

    #[test]
    fn from_is_rechecked_on_every_request() {
        let f = fx();
        struct Narrow<'a>(&'a Paths, Vec<Vec<u8>>);
        impl Channel for Narrow<'_> {
            fn recv(&mut self) -> std::result::Result<Option<Vec<u8>>, FrameError> {
                Ok(if self.1.is_empty() {
                    None
                } else {
                    Some(self.1.remove(0))
                })
            }
            fn send(&mut self, payload: &[u8]) -> std::result::Result<(), FrameError> {
                if payload.windows(6).any(|w| w == b"\"role\"") {
                    Store::update(self.0, |s| {
                        s.leaders.get_mut("laptop").unwrap().scope.from =
                            vec!["198.51.100.0/24".into()];
                        Ok(())
                    })
                    .unwrap();
                }
                Ok(())
            }
        }
        let reqs = vec![
            serde_json::to_vec(&json!({"id":1,"method":"hello"})).unwrap(),
            serde_json::to_vec(&json!({"id":2,"method":"runs.list"})).unwrap(),
        ];
        let mut chan = Narrow(&f.paths, reqs);
        serve(&f.paths, "laptop", ENR, &env(), &mut chan, &lim(10)).unwrap();
        assert_eq!(
            events(&f).pop().unwrap()["reason"],
            "client address outside from"
        );
    }

    #[test]
    fn the_per_minute_rate_limit_disconnects_with_one_audited_entry() {
        let f = fx();
        let items: Vec<Value> = (0..6).map(|i| json!({"id":i,"method":"hello"})).collect();
        let limits = Limits {
            per_minute: 3,
            ..Limits::default()
        };
        let mut out = Vec::new();
        let result = {
            let mut chan = Framed::new(Cursor::new(frames(&items)), &mut out);
            serve(&f.paths, "laptop", ENR, &env(), &mut chan, &limits)
        };
        assert_eq!(result.unwrap().requests, 3);
        let mut cur = Cursor::new(out);
        let mut replies = Vec::new();
        while let Ok(Some(fr)) = proto::read_frame(&mut cur, proto::MAX_FRAME) {
            replies.push(serde_json::from_slice::<Value>(&fr).unwrap());
        }
        assert_eq!(replies.len(), 4);
        assert_eq!(replies[3]["error"], "rate_limited");
        let n = events(&f)
            .iter()
            .filter(|e| e["event"] == "worker.rate_limited")
            .count();
        assert_eq!(n, 1);
    }

    #[test]
    fn concurrent_workers_per_link_are_capped() {
        let f = fx();
        let first = Slot::acquire(&f.paths, "laptop", 2).unwrap().unwrap();
        let second = Slot::acquire(&f.paths, "laptop", 2).unwrap().unwrap();
        assert!(Slot::acquire(&f.paths, "laptop", 2).unwrap().is_none());
        let limits = Limits {
            max_workers: 2,
            ..Limits::default()
        };
        let mut sink = Vec::new();
        let mut chan = Framed::new(Cursor::new(vec![]), &mut sink);
        assert!(serve(&f.paths, "laptop", ENR, &env(), &mut chan, &limits).is_err());
        assert!(events(&f).iter().any(|e| e["event"] == "worker.busy"));
        drop(first);
        assert!(Slot::acquire(&f.paths, "laptop", 2).unwrap().is_some());
        drop(second);
        assert!(Slot::acquire(&f.paths, "other", 1).unwrap().is_some());
    }

    #[test]
    fn the_canonical_repo_is_audited() {
        let f = fx();
        let dotted = format!("{}/../repo", f.repo);
        let req = json!({"id":1,"method":"runs.create","params":{"task":"w1:1","repo":dotted,"agent":"codex"}});
        let _ = run(&f, frames(&[req]), 5);
        let ev = events(&f)
            .into_iter()
            .find(|e| e["event"] == "worker.request")
            .unwrap();
        assert_eq!(ev["args"]["repo_canonical"], f.repo);
    }

    #[test]
    fn the_watchdog_fires_after_the_lifetime_and_can_be_cancelled() {
        let fired = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let f = std::sync::Arc::clone(&fired);
        let wd = watchdog(std::time::Duration::from_millis(100), move || {
            f.store(true, std::sync::atomic::Ordering::SeqCst)
        });
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert!(fired.load(std::sync::atomic::Ordering::SeqCst));
        drop(wd);
        let fired2 = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let f2 = std::sync::Arc::clone(&fired2);
        let wd = watchdog(std::time::Duration::from_millis(200), move || {
            f2.store(true, std::sync::atomic::Ordering::SeqCst)
        });
        drop(wd);
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert!(!fired2.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn a_pending_link_is_refused_at_connect_and_mid_session() {
        let f = fx();
        let marker = f.paths.pending_marker("laptop");
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(&marker, "pending").unwrap();
        let mut sink = Vec::new();
        let mut chan = Framed::new(Cursor::new(vec![]), &mut sink);
        assert!(serve(&f.paths, "laptop", ENR, &env(), &mut chan, &lim(5)).is_err());
        assert_eq!(
            events(&f)[0]["reason"],
            "link blocked after a failed rollback"
        );
        std::fs::remove_file(&marker).unwrap();
        struct Block<'a>(&'a Paths, Vec<Vec<u8>>);
        impl Channel for Block<'_> {
            fn recv(&mut self) -> std::result::Result<Option<Vec<u8>>, FrameError> {
                Ok(if self.1.is_empty() {
                    None
                } else {
                    Some(self.1.remove(0))
                })
            }
            fn send(&mut self, payload: &[u8]) -> std::result::Result<(), FrameError> {
                if payload.windows(6).any(|w| w == b"\"role\"") {
                    let m = self.0.pending_marker("laptop");
                    std::fs::create_dir_all(m.parent().unwrap()).unwrap();
                    std::fs::write(m, "pending").unwrap();
                }
                Ok(())
            }
        }
        let reqs = vec![
            serde_json::to_vec(&json!({"id":1,"method":"hello"})).unwrap(),
            serde_json::to_vec(&json!({"id":2,"method":"runs.list"})).unwrap(),
        ];
        let mut chan = Block(&f.paths, reqs);
        serve(&f.paths, "laptop", ENR, &env(), &mut chan, &lim(10)).unwrap();
        assert_eq!(
            events(&f).pop().unwrap()["reason"],
            "link blocked after a failed rollback"
        );
    }
}
