use crate::audit::Audit;
use crate::grammar;
use crate::proto::{self, Channel, FrameError};
use crate::scope::{self, Scope};
use crate::sha256_hex;
use crate::store::{Paths, Store};
use serde_json::{json, Value};
use std::ffi::OsString;
use std::net::IpAddr;
use tmgr_core::error::Result;

pub const MAX_REQUESTS: u32 = 256;

pub fn check_argv(args: &[OsString]) -> Result<String> {
    let bad = || crate::forbidden("tmgr worker accepts exactly: tmgr worker --link NAME");
    if args.len() != 4 || args[1] != "worker" || args[2] != "--link" {
        return Err(bad());
    }
    let name = args[3].to_str().ok_or_else(bad)?;
    grammar::link_name(name).map_err(|_| bad())?;
    Ok(name.to_string())
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

struct Conn<'a> {
    paths: &'a Paths,
    audit: Audit,
    name: &'a str,
    fingerprint: String,
}

impl Conn<'_> {
    fn record(&self, method: &str, args: Value, decision: &str, reason: &str) -> Result<()> {
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

    fn scope(&self) -> Result<Option<Scope>> {
        Ok(Store::load(self.paths)?
            .leaders
            .get(self.name)
            .map(|l| l.scope.clone()))
    }
}

fn say(chan: &mut dyn Channel, payload: Vec<u8>) -> bool {
    chan.send(&payload).is_ok()
}

pub fn serve(
    paths: &Paths,
    name: &str,
    env: &Env,
    chan: &mut dyn Channel,
    max_requests: u32,
) -> Result<Outcome> {
    grammar::link_name(name)?;
    let refused = || crate::forbidden("refused");
    let ssh_connection = env.ssh_connection.as_deref().ok_or_else(refused)?;
    let client = client_ip(ssh_connection).ok_or_else(refused)?;
    let store = Store::load(paths)?;
    let leader = store.leaders.get(name).ok_or_else(refused)?;
    let audit = Audit::new(&paths.audit_dir());
    if !leader.scope.from.is_empty() && !leader.scope.from.iter().any(|c| cidr_contains(c, client))
    {
        let _ = audit.append(
            "worker.refused",
            json!({"link": name, "fp": leader.fingerprint, "reason": "client address outside from"}),
        );
        return Err(refused());
    }
    let conn = Conn {
        paths,
        audit,
        name,
        fingerprint: leader.fingerprint.clone(),
    };
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
        match handle(&conn, &frame) {
            Step::Reply(r) => {
                if !say(chan, r) {
                    break;
                }
            }
            Step::Close(r) => {
                say(chan, r);
                break;
            }
        }
    }
    Ok(Outcome { requests })
}

enum Step {
    Reply(Vec<u8>),
    Close(Vec<u8>),
}

fn handle(conn: &Conn, frame: &[u8]) -> Step {
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
    let scope = match conn.scope() {
        Ok(Some(s)) => s,
        Ok(None) => {
            let _ = conn.record(&req.method, json!({}), "rejected", "link revoked");
            return Step::Close(proto::error(
                id,
                "revoked",
                "this link is no longer trusted",
            ));
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
        "runs.list" | "runs.get" | "runs.create" | "runs.halt" => {
            let args = scope::audit_args(&req.params);
            match scope::validate_run_method(method, &req.params, &scope, 0) {
                Ok(()) => {
                    if conn
                        .record(method, args, "accepted", "runner not installed")
                        .is_err()
                    {
                        return audit_down();
                    }
                    Step::Reply(proto::error(id, "not_available", "runner not installed"))
                }
                Err(reject) => {
                    if conn
                        .record(method, args, "rejected", &reject.reason)
                        .is_err()
                    {
                        return audit_down();
                    }
                    Step::Reply(proto::error(id, reject.code, &reject.reason))
                }
            }
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

    fn args(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    #[test]
    fn argv_must_have_the_exact_shape() {
        assert_eq!(
            check_argv(&args(&["/x/tmgr", "worker", "--link", "laptop"])).unwrap(),
            "laptop"
        );
        for bad in [
            vec!["tmgr", "worker"],
            vec!["tmgr", "worker", "--link"],
            vec!["tmgr", "worker", "--link", "laptop", "--json"],
            vec!["tmgr", "--json", "worker", "--link", "laptop"],
            vec!["tmgr", "worker", "--link=laptop", "x"],
            vec!["tmgr", "worker", "-l", "laptop"],
            vec!["tmgr", "worker", "--link", "Laptop"],
            vec!["tmgr", "worker", "--link", "a b"],
            vec!["tmgr", "worker", "--link", "-x"],
            vec!["tmgr", "link", "--link", "laptop"],
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
                        max: 2,
                        from: vec![],
                    },
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
            serve(&f.paths, "laptop", &env(), &mut chan, cap)
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
        let ok = json!({"id":2,"method":"runs.create","params":{"task":"TM-512","repo":f.repo,"agent":"codex"}});
        let bad_agent = json!({"id":3,"method":"runs.create","params":{"task":"TM-512","repo":f.repo,"agent":"claude-code"}});
        let bypass = json!({"id":4,"method":"runs.create","params":{"task":"TM-530","repo":f.repo,"agent":"codex","mode":"bypass"}});
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
        assert_eq!(decisions, ["accepted", "rejected", "rejected", "accepted"]);
        assert_eq!(ev[1]["fp"], "SHA256:test");
        assert_eq!(ev[1]["args"]["task"], "TM-512");
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
            "attach.open",
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
            serve(&f.paths, "laptop", &no_env, &mut chan, 5)
                .unwrap_err()
                .exit_code(),
            5
        );
        let bogus = Env {
            ssh_connection: Some("nonsense".into()),
            original_command: None,
        };
        assert!(serve(&f.paths, "laptop", &bogus, &mut chan, 5).is_err());
        assert!(serve(&f.paths, "stranger", &env(), &mut chan, 5).is_err());
        assert!(serve(&f.paths, "Bad Name", &env(), &mut chan, 5).is_err());
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
        assert!(serve(&f.paths, "laptop", &env(), &mut chan, 5).is_err());
        assert_eq!(events(&f)[0]["event"], "worker.refused");
    }

    #[test]
    fn original_command_is_logged_as_length_and_hash_only() {
        let f = fx();
        let mut e = env();
        e.original_command = Some(b"cat /etc/passwd".to_vec());
        let mut sink = Vec::new();
        let mut chan = Framed::new(Cursor::new(vec![]), &mut sink);
        serve(&f.paths, "laptop", &e, &mut chan, 5).unwrap();
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
        let out = serve(&f.paths, "laptop", &env(), &mut chan, 10).unwrap();
        assert_eq!(out.requests, 2);
        let last = events(&f).pop().unwrap();
        assert_eq!(last["reason"], "link revoked");
    }
}
