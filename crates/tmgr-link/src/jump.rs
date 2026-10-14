use crate::grammar::{self, PubKey};
use crate::leader::{self, SshTarget};
use crate::store::{Paths, Store};
use std::ffi::OsString;
use std::io::Read;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tmgr_core::creds::CredentialStore;
use tmgr_core::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hop {
    pub name: String,
    pub target: SshTarget,
}

pub fn hop_for(store: &Store, jump: &str) -> Result<Hop> {
    grammar::link_name(jump)?;
    let f = store.followers.get(jump).ok_or_else(|| {
        Error::not_found(format!("no pinned link named {jump} to jump through"))
            .hint("tmgr link list")
    })?;
    if store.jumps.contains_key(jump) {
        return Err(Error::usage(format!(
            "{jump} is itself reached through a jump host; only one hop is supported"
        )));
    }
    grammar::parse_target(&format!("{}@{}", f.user, f.host))?;
    grammar::parse_port(u32::from(f.port))?;
    Ok(Hop {
        name: jump.to_string(),
        target: SshTarget {
            user: f.user.clone(),
            host: f.host.clone(),
            port: f.port,
        },
    })
}

pub fn resolve(store: &Store, link: &str) -> Result<Option<Hop>> {
    store.jumps.get(link).map(|j| hop_for(store, j)).transpose()
}

pub fn referenced_by(store: &Store, jump: &str) -> Vec<String> {
    store
        .jumps
        .iter()
        .filter(|(_, j)| j.as_str() == jump)
        .map(|(n, _)| n.clone())
        .collect()
}

fn token(s: &str) -> Result<&str> {
    let ok = !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._/:@=,+-".contains(&b));
    if ok {
        Ok(s)
    } else {
        Err(Error::usage(
            "a value for the jump hop contains characters that cannot be passed to ssh safely",
        )
        .hint("use paths without spaces, quotes, % or shell characters (HOME, XDG_CONFIG_HOME, XDG_RUNTIME_DIR)"))
    }
}

fn ssh_binary() -> Result<PathBuf> {
    let found = leader::tool("ssh", "/usr/bin/ssh");
    if found.is_absolute() {
        return Ok(found);
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .filter(|d| d.is_absolute())
        .map(|d| d.join("ssh"))
        .find(|p| p.is_file())
        .ok_or_else(|| crate::link_err("cannot find an absolute path to ssh for the jump hop"))
}

fn forward_dest(dest: &SshTarget) -> Result<()> {
    grammar::parse_target(&format!("{}@{}", dest.user, dest.host))?;
    if dest.host.contains(':') {
        return Err(
            Error::usage("an IPv6 literal cannot be reached through --jump")
                .hint("use a host name"),
        );
    }
    grammar::parse_port(u32::from(dest.port))?;
    Ok(())
}

fn hop_args(hop: &Hop, key: &Path, known_hosts: &Path, dest: &SshTarget) -> Result<Vec<OsString>> {
    forward_dest(dest)?;
    leader::build_args(
        &hop.target,
        key,
        known_hosts,
        &hop.name,
        "none",
        Some((&dest.host, dest.port)),
    )
}

pub fn proxy_command(
    hop: &Hop,
    key: &Path,
    known_hosts: &Path,
    dest: &SshTarget,
) -> Result<String> {
    if !key.is_absolute() || !known_hosts.is_absolute() {
        return Err(Error::usage("jump hop paths must be absolute"));
    }
    let ssh = ssh_binary()?;
    let mut parts = vec![token(
        ssh.to_str()
            .ok_or_else(|| Error::usage("the ssh path is not valid text"))?,
    )?
    .to_string()];
    for a in hop_args(hop, key, known_hosts, dest)? {
        let a = a
            .into_string()
            .map_err(|_| Error::usage("a jump hop value is not valid text"))?;
        parts.push(token(&a)?.to_string());
    }
    Ok(parts.join(" "))
}

struct Relay {
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
    thread: Option<std::thread::JoinHandle<String>>,
}

impl Relay {
    fn start(listener: TcpListener, args: Vec<OsString>) -> Relay {
        let stop = Arc::new(AtomicBool::new(false));
        let child: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
        let (s, c) = (Arc::clone(&stop), Arc::clone(&child));
        let thread = std::thread::spawn(move || relay_main(listener, args, &s, &c));
        Relay {
            stop,
            child,
            thread: Some(thread),
        }
    }

    fn finish(mut self) -> String {
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(mut c) = self.child.lock() {
            if let Some(ch) = c.as_mut() {
                let _ = ch.kill();
            }
        }
        self.thread
            .take()
            .and_then(|t| t.join().ok())
            .unwrap_or_default()
    }
}

fn relay_main(
    listener: TcpListener,
    args: Vec<OsString>,
    stop: &AtomicBool,
    slot: &Mutex<Option<Child>>,
) -> String {
    let _ = listener.set_nonblocking(true);
    let deadline = Instant::now() + Duration::from_secs(20);
    let sock = loop {
        match listener.accept() {
            Ok((s, _)) => break s,
            Err(_) if stop.load(Ordering::SeqCst) || Instant::now() > deadline => {
                return String::new()
            }
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    let _ = sock.set_nonblocking(false);
    let spawned = Command::new(leader::tool("ssh", "/usr/bin/ssh"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = spawned else {
        return String::new();
    };
    let (Some(mut stdin), Some(mut stdout), Some(mut stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return String::new();
    };
    if let Ok(mut s) = slot.lock() {
        *s = Some(child);
    }
    let err_text = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = (&mut stderr).take(8192).read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).to_string()
    });
    let Ok(mut inbound) = sock.try_clone() else {
        return String::new();
    };
    let up = std::thread::spawn(move || {
        let _ = std::io::copy(&mut inbound, &mut stdin);
    });
    let mut outbound = sock;
    let _ = std::io::copy(&mut stdout, &mut outbound);
    let _ = outbound.shutdown(std::net::Shutdown::Both);
    let _ = up.join();
    if let Ok(mut s) = slot.lock() {
        if let Some(ch) = s.as_mut() {
            let _ = ch.kill();
            let _ = ch.wait();
        }
    }
    err_text.join().unwrap_or_default()
}

pub fn scan_host_via(
    paths: &Paths,
    cred: &CredentialStore,
    hop: &Hop,
    host: &str,
    port: u16,
) -> Result<PubKey> {
    let dest = SshTarget {
        user: "scan".into(),
        host: host.into(),
        port,
    };
    let key = leader::materialise(cred)?;
    let args = hop_args(hop, &key.path, &paths.known_hosts(), &dest)?;
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let local = listener.local_addr()?.port();
    let relay = Relay::start(listener, args);
    let found = leader::scan_host("127.0.0.1", local);
    let stderr = relay.finish();
    found.map_err(|_| {
        if stderr.trim().is_empty() {
            crate::link_err(format!(
                "could not read an ed25519 host key from {host}:{port} through {}",
                hop.name
            ))
            .hint("check the address, the jump link (tmgr link test), and that the jump host allows forwarding to it")
        } else {
            crate::link_err(format!(
                "{} (through {})",
                leader::classify_stderr(&stderr, false),
                hop.name
            ))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Follower;

    fn follower(host: &str) -> Follower {
        Follower {
            user: "deploy".into(),
            host: host.into(),
            port: 22,
            host_key_fingerprint: "SHA256:x".into(),
            added: "2026-10-13T00:00:00Z".into(),
        }
    }

    fn store() -> Store {
        let mut s = Store::default();
        s.followers
            .insert("bastion".into(), follower("jump.example.com"));
        s.followers.insert("inner".into(), follower("192.0.2.5"));
        s.jumps.insert("inner".into(), "bastion".into());
        s
    }

    fn dest() -> SshTarget {
        SshTarget {
            user: "dev".into(),
            host: "target.example.com".into(),
            port: 2222,
        }
    }

    fn hop() -> Hop {
        hop_for(&store(), "bastion").unwrap()
    }

    #[test]
    fn the_hop_argv_is_the_isolated_set_plus_stdio_forwarding() {
        let args: Vec<String> = hop_args(
            &hop(),
            Path::new("/run/k/key"),
            Path::new("/home/u/.config/tmgr/link_known_hosts"),
            &dest(),
        )
        .unwrap()
        .into_iter()
        .map(|a| a.into_string().unwrap())
        .collect();
        let expected = "-F none -T -a -x \
            -o ClearAllForwardings=yes -o BatchMode=yes -o IdentitiesOnly=yes -o IdentityAgent=none \
            -o IdentityFile=/run/k/key -o ControlMaster=no -o ControlPath=none -o PermitLocalCommand=no \
            -o ProxyCommand=none -o KnownHostsCommand=none \
            -o UserKnownHostsFile=/home/u/.config/tmgr/link_known_hosts -o GlobalKnownHostsFile=none \
            -o HostKeyAlias=tmgr-link-bastion -o StrictHostKeyChecking=yes -o RequestTTY=no \
            -W target.example.com:2222 -p 22 deploy@jump.example.com";
        assert_eq!(
            args.join(" "),
            expected.split_whitespace().collect::<Vec<_>>().join(" ")
        );
    }

    #[test]
    fn the_outer_argv_carries_the_whole_hop_in_one_proxy_command_and_its_own_alias() {
        let key = Path::new("/run/k/key");
        let kh = Path::new("/home/u/.config/tmgr/link_known_hosts");
        let proxy = proxy_command(&hop(), key, kh, &dest()).unwrap();
        let hop_argv: Vec<String> = hop_args(&hop(), key, kh, &dest())
            .unwrap()
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect();
        assert_eq!(
            proxy,
            format!("{} {}", ssh_binary().unwrap().display(), hop_argv.join(" "))
        );
        assert!(proxy.starts_with('/'), "{proxy}");
        assert!(proxy.contains(" -W target.example.com:2222 -p 22 deploy@jump.example.com"));
        assert!(proxy.contains("HostKeyAlias=tmgr-link-bastion"));
        assert!(proxy.contains("IdentitiesOnly=yes") && proxy.contains("IdentityAgent=none"));
        assert!(proxy.contains("StrictHostKeyChecking=yes") && proxy.contains("-F none"));
        assert!(!proxy.contains('%') && !proxy.contains('"') && !proxy.contains('\''));
        let outer: Vec<String> = leader::build_args(&dest(), key, kh, "inner", &proxy, None)
            .unwrap()
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect();
        let i = outer
            .iter()
            .position(|a| a.starts_with("ProxyCommand="))
            .unwrap();
        assert_eq!(outer[i], format!("ProxyCommand={proxy}"));
        assert_eq!(
            outer
                .iter()
                .filter(|a| a.starts_with("ProxyCommand="))
                .count(),
            1
        );
        assert!(outer.contains(&"HostKeyAlias=tmgr-link-inner".to_string()));
        assert!(outer.contains(&"StrictHostKeyChecking=yes".to_string()));
        assert_eq!(
            &outer[outer.len() - 3..],
            ["-p", "2222", "dev@target.example.com"]
        );
    }

    #[test]
    fn unsafe_characters_never_reach_the_proxy_command() {
        let kh = Path::new("/h/kh");
        for bad in [
            "/a b/key",
            "/a%d/key",
            "/a\"b/key",
            "/a'b/key",
            "/a;b/key",
            "/a$b/key",
            "/a`b/key",
            "/a|b/key",
            "/a&b/key",
            "/a\nb/key",
            "/a(b)/key",
            "/a*b/key",
            "/a~b/key",
            "/a\\b/key",
            "/a<b/key",
            "/a#b/key",
        ] {
            let r = proxy_command(&hop(), Path::new(bad), kh, &dest());
            assert!(r.is_err(), "{bad} was accepted");
        }
        assert!(proxy_command(&hop(), Path::new("/k"), Path::new("/h d/kh"), &dest()).is_err());
        assert!(proxy_command(&hop(), Path::new("relative/key"), kh, &dest()).is_err());
        assert!(proxy_command(&hop(), Path::new("/k"), Path::new("kh"), &dest()).is_err());
    }

    #[test]
    fn hostile_destinations_and_hops_are_rejected() {
        let key = Path::new("/k");
        let kh = Path::new("/kh");
        for host in [
            "a b",
            "a;b",
            "a$(x)",
            "-oProxyCommand=x",
            "a'b",
            "a%h",
            "",
            "::1",
            "[::1]",
        ] {
            let d = SshTarget {
                host: host.into(),
                ..dest()
            };
            assert!(proxy_command(&hop(), key, kh, &d).is_err(), "{host}");
        }
        let d = SshTarget { port: 0, ..dest() };
        assert!(proxy_command(&hop(), key, kh, &d).is_err());
        let d = SshTarget {
            user: "a b".into(),
            ..dest()
        };
        assert!(proxy_command(&hop(), key, kh, &d).is_err());
        let mut s = store();
        s.followers.get_mut("bastion").unwrap().host = "x;y".into();
        assert!(hop_for(&s, "bastion").is_err());
        s.followers.get_mut("bastion").unwrap().host = "jump.example.com".into();
        s.followers.get_mut("bastion").unwrap().user = "-oFoo".into();
        assert!(hop_for(&s, "bastion").is_err());
        assert!(hop_for(&store(), "Bad Name").is_err());
    }

    #[test]
    fn only_one_hop_and_only_existing_links() {
        let s = store();
        assert_eq!(hop_for(&s, "inner").unwrap_err().exit_code(), 2);
        assert!(hop_for(&s, "inner")
            .unwrap_err()
            .message
            .contains("only one hop"));
        assert_eq!(hop_for(&s, "ghost").unwrap_err().exit_code(), 3);
        assert_eq!(resolve(&s, "inner").unwrap().unwrap().name, "bastion");
        assert!(resolve(&s, "bastion").unwrap().is_none());
        let mut t = s.clone();
        t.jumps.insert("inner".into(), "ghost".into());
        assert!(resolve(&t, "inner").is_err());
    }

    #[test]
    fn references_to_a_jump_link_are_listed() {
        let s = store();
        assert_eq!(referenced_by(&s, "bastion"), vec!["inner".to_string()]);
        assert!(referenced_by(&s, "inner").is_empty());
    }

    #[test]
    fn the_store_round_trips_jumps_and_stays_compatible_without_them() {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        Store::update(&paths, |s| {
            *s = store();
            s.peer_keys.insert("inner".into(), "AAAA".into());
            Ok(())
        })
        .unwrap();
        let loaded = Store::load(&paths).unwrap();
        assert_eq!(
            loaded.jumps.get("inner").map(String::as_str),
            Some("bastion")
        );
        assert_eq!(
            loaded.peer_keys.get("inner").map(String::as_str),
            Some("AAAA")
        );
        let plain = toml::to_string(&Store::default()).unwrap();
        assert!(!plain.contains("jumps") && !plain.contains("peer_keys"));
    }
}
