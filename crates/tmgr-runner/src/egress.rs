use std::io::{self, Read, Write};
use std::net::{
    IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs,
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const BASE_ALLOW: &[&str] = &[
    "github.com",
    "codeload.github.com",
    "objects.githubusercontent.com",
    "raw.githubusercontent.com",
];

const MAX_HEAD: usize = 8 * 1024;
const MAX_CONNECTIONS: usize = 64;
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);
const TICK: Duration = Duration::from_millis(500);
const MAX_REFUSED: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostPattern {
    Exact(String),
    Suffix(String),
}

impl HostPattern {
    pub fn parse(s: &str) -> Result<HostPattern, String> {
        let s = s.trim().to_ascii_lowercase();
        let (suffix, name) = match s.strip_prefix("*.") {
            Some(rest) => (true, rest),
            None => (false, s.as_str()),
        };
        if !valid_name(name) {
            return Err(format!("{s:?} is not a valid host pattern"));
        }
        Ok(if suffix {
            HostPattern::Suffix(format!(".{name}"))
        } else {
            HostPattern::Exact(name.to_string())
        })
    }

    pub fn matches(&self, host: &str) -> bool {
        match self {
            HostPattern::Exact(h) => host == h,
            HostPattern::Suffix(suffix) => {
                host.len() > suffix.len() && host.ends_with(suffix.as_str())
            }
        }
    }
}

fn valid_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 253 || name.parse::<IpAddr>().is_ok() {
        return false;
    }
    let labels: Vec<&str> = name.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|l| {
            (1..=63).contains(&l.len())
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        })
        && !labels
            .last()
            .is_some_and(|tld| tld.bytes().all(|c| c.is_ascii_digit()))
}

pub fn allowlist(profile_allow: &[String]) -> Result<Vec<HostPattern>, String> {
    BASE_ALLOW
        .iter()
        .map(|s| s.to_string())
        .chain(profile_allow.iter().cloned())
        .map(|s| HostPattern::parse(&s))
        .collect()
}

pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => public_v4(v4),
        IpAddr::V6(v6) => public_v6(v6),
    }
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || a == 0
        || (a == 100 && (64..128).contains(&b))
        || a >= 240
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113))
}

fn public_v6(ip: Ipv6Addr) -> bool {
    let s = ip.segments();
    let embedded =
        |hi: u16, lo: u16| Ipv4Addr::new((hi >> 8) as u8, hi as u8, (lo >> 8) as u8, lo as u8);
    if let Some(v4) = ip.to_ipv4_mapped() {
        return public_v4(v4);
    }
    if s[..6] == [0; 6] {
        return public_v4(embedded(s[6], s[7])) && s[6] != 0;
    }
    if s[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
        return public_v4(embedded(s[6], s[7]));
    }
    if s[0] == 0x2002 {
        return public_v4(embedded(s[1], s[2]));
    }
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (s[0] & 0xfe00) == 0xfc00
        || (s[0] & 0xffc0) == 0xfe80
        || (s[0] & 0xffc0) == 0xfec0
        || (s[0] == 0x2001 && s[1] == 0x0db8)
        || (s[0] == 0x2001 && s[1] == 0))
}

pub trait Resolver: Send + Sync {
    fn resolve(&self, host: &str, port: u16) -> io::Result<Vec<SocketAddr>>;
}

pub struct Dns;

impl Resolver for Dns {
    fn resolve(&self, host: &str, port: u16) -> io::Result<Vec<SocketAddr>> {
        Ok((host, port).to_socket_addrs()?.collect())
    }
}

pub struct Strict<R>(pub R);

impl<R: Resolver> Resolver for Strict<R> {
    fn resolve(&self, host: &str, port: u16) -> io::Result<Vec<SocketAddr>> {
        let addrs = self.0.resolve(host, port)?;
        if addrs.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "no address"));
        }
        if addrs.iter().any(|a| !is_public(a.ip())) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "address is not public",
            ));
        }
        Ok(addrs)
    }
}

struct Shared {
    allow: Vec<HostPattern>,
    resolver: Box<dyn Resolver>,
    stop: AtomicBool,
    active: AtomicUsize,
    refused: Mutex<Vec<String>>,
}

pub struct Proxy {
    pub port: u16,
    shared: Arc<Shared>,
    handle: Option<JoinHandle<()>>,
}

impl Proxy {
    pub fn start(allow: Vec<HostPattern>) -> io::Result<Proxy> {
        Proxy::start_with(allow, Box::new(Strict(Dns)))
    }

    pub fn start_with(allow: Vec<HostPattern>, resolver: Box<dyn Resolver>) -> io::Result<Proxy> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let shared = Arc::new(Shared {
            allow,
            resolver,
            stop: AtomicBool::new(false),
            active: AtomicUsize::new(0),
            refused: Mutex::new(Vec::new()),
        });
        let s = shared.clone();
        let handle = std::thread::Builder::new()
            .name("egress-proxy".into())
            .spawn(move || accept_loop(listener, s))?;
        Ok(Proxy {
            port,
            shared,
            handle: Some(handle),
        })
    }

    pub fn refused_hosts(&self) -> Vec<String> {
        self.shared
            .refused
            .lock()
            .map(|r| r.clone())
            .unwrap_or_default()
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], self.port)),
            Duration::from_secs(1),
        );
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn accept_loop(listener: TcpListener, shared: Arc<Shared>) {
    for conn in listener.incoming() {
        if shared.stop.load(Ordering::SeqCst) {
            break;
        }
        let Ok(stream) = conn else { continue };
        if shared.active.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            shared.active.fetch_sub(1, Ordering::SeqCst);
            let _ = reply(&stream, "503 Service Unavailable");
            continue;
        }
        let s = shared.clone();
        let spawned = std::thread::Builder::new()
            .name("egress-conn".into())
            .spawn(move || {
                handle(stream, &s);
                s.active.fetch_sub(1, Ordering::SeqCst);
            });
        if spawned.is_err() {
            shared.active.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

fn reply(mut stream: &TcpStream, status: &str) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
    )
}

enum Head {
    Parsed {
        method: String,
        target: String,
        rest: Vec<u8>,
    },
    TooLarge,
    Bad,
}

fn read_head(stream: &mut TcpStream) -> Head {
    let _ = stream.set_read_timeout(Some(HEAD_TIMEOUT));
    let mut buf = Vec::with_capacity(512);
    let mut chunk = [0u8; 1024];
    loop {
        let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
            if buf.len() > MAX_HEAD {
                return Head::TooLarge;
            }
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => return Head::Bad,
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    continue;
                }
            }
        };
        if end > MAX_HEAD {
            return Head::TooLarge;
        }
        let rest = buf[end + 4..].to_vec();
        let Ok(head) = std::str::from_utf8(&buf[..end]) else {
            return Head::Bad;
        };
        let line = head.lines().next().unwrap_or("");
        let mut parts = line.split(' ');
        return match (parts.next(), parts.next(), parts.next(), parts.next()) {
            (Some(m), Some(t), Some(v), None) if v.starts_with("HTTP/1.") && !m.is_empty() => {
                Head::Parsed {
                    method: m.to_string(),
                    target: t.to_string(),
                    rest,
                }
            }
            _ => Head::Bad,
        };
    }
}

enum Target {
    Host(String),
    Refused(String),
}

fn check_target(target: &str, allow: &[HostPattern]) -> Target {
    let Some((host, port)) = target.rsplit_once(':') else {
        return Target::Refused(target.to_string());
    };
    let host = host.to_ascii_lowercase();
    if port != "443" || host.parse::<IpAddr>().is_ok() || host.starts_with('[') {
        return Target::Refused(host);
    }
    if !valid_name(&host) || !allow.iter().any(|p| p.matches(&host)) {
        return Target::Refused(host);
    }
    Target::Host(host)
}

fn note_refused(shared: &Shared, host: &str) {
    let clean: String = host
        .chars()
        .take(100)
        .map(|c| if c.is_ascii_graphic() { c } else { '?' })
        .collect();
    eprintln!("tmgr egress: refused CONNECT to {clean}");
    if let Ok(mut r) = shared.refused.lock() {
        if r.len() < MAX_REFUSED {
            r.push(clean);
        }
    }
}

fn handle(mut client: TcpStream, shared: &Shared) {
    let (method, target, rest) = match read_head(&mut client) {
        Head::Parsed {
            method,
            target,
            rest,
        } => (method, target, rest),
        Head::TooLarge => {
            let _ = reply(&client, "431 Request Header Fields Too Large");
            return;
        }
        Head::Bad => {
            let _ = reply(&client, "400 Bad Request");
            return;
        }
    };
    if method != "CONNECT" {
        let _ = reply(&client, "405 Method Not Allowed");
        return;
    }
    let host = match check_target(&target, &shared.allow) {
        Target::Host(h) => h,
        Target::Refused(h) => {
            note_refused(shared, &h);
            let _ = reply(&client, "403 Forbidden");
            return;
        }
    };
    let addrs = match shared.resolver.resolve(&host, 443) {
        Ok(a) if !a.is_empty() => a,
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            note_refused(shared, &host);
            let _ = reply(&client, "403 Forbidden");
            return;
        }
        _ => {
            let _ = reply(&client, "502 Bad Gateway");
            return;
        }
    };
    let Some(upstream) = addrs
        .iter()
        .find_map(|a| TcpStream::connect_timeout(a, CONNECT_TIMEOUT).ok())
    else {
        let _ = reply(&client, "502 Bad Gateway");
        return;
    };
    if write!(client, "HTTP/1.1 200 Connection Established\r\n\r\n").is_err() {
        return;
    }
    splice(client, upstream, rest, shared);
}

fn splice(client: TcpStream, upstream: TcpStream, early: Vec<u8>, shared: &Shared) {
    let (Ok(c2), Ok(u2)) = (client.try_clone(), upstream.try_clone()) else {
        return;
    };
    let last = Arc::new(Mutex::new(Instant::now()));
    let mut upstream_w = upstream.try_clone().ok();
    if let Some(w) = upstream_w.as_mut() {
        if !early.is_empty() && w.write_all(&early).is_err() {
            return;
        }
    }
    let stop = &shared.stop;
    std::thread::scope(|scope| {
        scope.spawn(|| pump(client, u2, &last, stop));
        scope.spawn(|| pump(upstream, c2, &last, stop));
    });
}

fn pump(mut from: TcpStream, mut to: TcpStream, last: &Mutex<Instant>, stop: &AtomicBool) {
    let _ = from.set_read_timeout(Some(TICK));
    let mut buf = [0u8; 16 * 1024];
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        match from.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if let Ok(mut t) = last.lock() {
                    *t = Instant::now();
                }
                if to.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                let idle = last.lock().map(|t| t.elapsed()).unwrap_or(IDLE_TIMEOUT);
                if idle >= IDLE_TIMEOUT {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let _ = to.shutdown(Shutdown::Write);
    let _ = from.shutdown(Shutdown::Read);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    struct Fixed {
        to: SocketAddr,
        calls: Arc<AtomicUsize>,
    }

    impl Resolver for Fixed {
        fn resolve(&self, _host: &str, _port: u16) -> io::Result<Vec<SocketAddr>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![self.to])
        }
    }

    struct Raw(Vec<IpAddr>);

    impl Resolver for Raw {
        fn resolve(&self, _host: &str, port: u16) -> io::Result<Vec<SocketAddr>> {
            Ok(self.0.iter().map(|ip| SocketAddr::new(*ip, port)).collect())
        }
    }

    fn addr(s: &str) -> IpAddr {
        s.replace('_', ".").parse().unwrap()
    }

    fn pats(list: &[&str]) -> Vec<HostPattern> {
        list.iter()
            .map(|s| HostPattern::parse(s).unwrap())
            .collect()
    }

    fn upstream_echo() -> (SocketAddr, JoinHandle<()>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let h = std::thread::spawn(move || {
            if let Ok((mut s, _)) = l.accept() {
                let mut buf = [0u8; 64];
                while let Ok(n) = s.read(&mut buf) {
                    if n == 0 || s.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            }
        });
        (addr, h)
    }

    fn counted(to: SocketAddr) -> (Box<dyn Resolver>, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        (
            Box::new(Fixed {
                to,
                calls: calls.clone(),
            }),
            calls,
        )
    }

    fn talk(port: u16, request: &[u8]) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        s.write_all(request).unwrap();
        let mut out = String::new();
        let mut buf = [0u8; 256];
        while let Ok(n) = s.read(&mut buf) {
            if n == 0 {
                break;
            }
            out.push_str(&String::from_utf8_lossy(&buf[..n]));
            if out.contains("\r\n\r\n") {
                break;
            }
        }
        out
    }

    fn status(resp: &str) -> &str {
        resp.lines().next().unwrap_or("")
    }

    fn connect(port: u16, target: &str) -> String {
        talk(
            port,
            format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n").as_bytes(),
        )
    }

    #[test]
    fn patterns_match_exact_and_suffix_only() {
        let exact = HostPattern::parse("api.anthropic.com").unwrap();
        assert!(exact.matches("api.anthropic.com"));
        assert!(!exact.matches("x.api.anthropic.com"));
        assert!(!exact.matches("anthropic.com"));
        let wild = HostPattern::parse("*.github.com").unwrap();
        assert!(wild.matches("a.github.com"));
        assert!(wild.matches("a.b.github.com"));
        assert!(!wild.matches("github.com"));
        assert!(!wild.matches("evilgithub.com"));
        assert!(!wild.matches("a.github.com.evil.io"));
    }

    #[test]
    fn invalid_patterns_are_rejected() {
        for bad in [
            "",
            "localhost",
            "1.2.3.4",
            "*",
            "*.",
            "a..b.com",
            "-a.com",
            "a_b.com",
            "[::1]",
            "a.com/x",
            "a.com:443",
            "*.*.com",
            "1.2.3.4.5",
        ] {
            assert!(HostPattern::parse(bad).is_err(), "{bad}");
        }
        assert!(HostPattern::parse(&"10_0_0_1".replace('_', ".")).is_err());
        assert_eq!(
            HostPattern::parse("API.Example.COM").unwrap(),
            HostPattern::Exact("api.example.com".into())
        );
    }

    #[test]
    fn base_set_is_always_present() {
        let list = allowlist(&["api.openai.com".into()]).unwrap();
        for h in [
            "github.com",
            "codeload.github.com",
            "objects.githubusercontent.com",
            "raw.githubusercontent.com",
            "api.openai.com",
        ] {
            assert!(list.iter().any(|p| p.matches(h)), "{h}");
        }
        assert!(allowlist(&["bad host".into()]).is_err());
    }

    #[test]
    fn non_public_addresses_are_not_public() {
        for ip in [
            "0.0.0.0",
            "127.0.0.1",
            "127.255.0.2",
            "10_1_2_3",
            "172_16_0_1",
            "172_31_255_255",
            "192_168_1_1",
            "169.254.169.254",
            "100.64.0.1",
            "100.127.255.255",
            "224.0.0.1",
            "239.1.1.1",
            "255.255.255.255",
            "240.0.0.1",
            "192.0.0.8",
            "192.0.2.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.9",
            "0.1.2.3",
            "::",
            "::1",
            "ff02::1",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "fec0::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
            "::ffff:10_0_0_1",
            "::ffff:169.254.169.254",
            "::ffff:100.64.0.1",
            "::ffff:0.0.0.0",
            "::10_0_0_1",
            "::127.0.0.1",
            "64:ff9b::a00:1",
            "64:ff9b::7f00:1",
            "2002:7f00:1::1",
            "2002:a00:1::1",
        ] {
            assert!(!is_public(addr(ip)), "{ip}");
        }
    }

    #[test]
    fn global_addresses_are_public() {
        for ip in [
            "1.1.1.1",
            "8.8.8.8",
            "140.82.112.3",
            "100.63.255.255",
            "100.128.0.1",
            "172.15.0.1",
            "172.32.0.1",
            "2606:4700:4700::1111",
            "2a00:1450:4001::200e",
            "::ffff:8.8.8.8",
            "64:ff9b::808:808",
        ] {
            assert!(is_public(addr(ip)), "{ip}");
        }
    }

    #[test]
    fn strict_resolver_refuses_when_any_address_is_private() {
        let r = Strict(Raw(vec!["8.8.8.8".parse().unwrap(), addr("10_0_0_1")]));
        assert_eq!(
            r.resolve("a.example.com", 443).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        let ok = Strict(Raw(vec!["8.8.8.8".parse().unwrap()]));
        assert_eq!(
            ok.resolve("a.example.com", 443).unwrap(),
            vec!["8.8.8.8:443".parse().unwrap()]
        );
        assert!(Strict(Raw(vec![])).resolve("a.example.com", 443).is_err());
    }

    #[test]
    fn allowed_connect_reaches_the_upstream_and_splices_both_ways() {
        let (addr, h) = upstream_echo();
        let (resolver, calls) = counted(addr);
        let proxy = Proxy::start_with(pats(&["files.example.com"]), resolver).unwrap();
        let mut s = TcpStream::connect(("127.0.0.1", proxy.port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        s.write_all(
            b"CONNECT files.example.com:443 HTTP/1.1\r\nHost: files.example.com:443\r\n\r\nping",
        )
        .unwrap();
        let mut got = Vec::new();
        let mut buf = [0u8; 256];
        while !got.ends_with(b"ping") {
            let n = s.read(&mut buf).unwrap();
            assert!(n > 0, "closed early: {:?}", String::from_utf8_lossy(&got));
            got.extend_from_slice(&buf[..n]);
        }
        let text = String::from_utf8_lossy(&got);
        assert!(text.starts_with("HTTP/1.1 200"), "{text}");
        s.write_all(b"pong").unwrap();
        got.clear();
        while !got.ends_with(b"pong") {
            let n = s.read(&mut buf).unwrap();
            assert!(n > 0);
            got.extend_from_slice(&buf[..n]);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(proxy.refused_hosts().is_empty());
        drop(s);
        drop(proxy);
        h.join().unwrap();
    }

    #[test]
    fn upper_case_host_matches_the_allowlist() {
        let (addr, _h) = upstream_echo();
        let (resolver, _) = counted(addr);
        let proxy = Proxy::start_with(pats(&["files.example.com"]), resolver).unwrap();
        assert!(status(&connect(proxy.port, "FILES.Example.com:443")).contains("200"));
    }

    fn refusal(request: &[u8], allow: &[&str]) -> (String, usize, Vec<String>) {
        let (resolver, calls) = counted("127.0.0.1:9".parse().unwrap());
        let proxy = Proxy::start_with(pats(allow), resolver).unwrap();
        let resp = talk(proxy.port, request);
        let hosts = proxy.refused_hosts();
        (
            status(&resp).to_string(),
            calls.load(Ordering::SeqCst),
            hosts,
        )
    }

    #[test]
    fn non_connect_methods_are_405_without_resolution() {
        for m in [
            "GET http://files.example.com/ HTTP/1.1",
            "POST / HTTP/1.1",
            "GET / HTTP/1.1",
            "OPTIONS * HTTP/1.1",
            "connect files.example.com:443 HTTP/1.1",
        ] {
            let (st, calls, _) = refusal(
                format!("{m}\r\nHost: x\r\n\r\n").as_bytes(),
                &["files.example.com"],
            );
            assert_eq!(st, "HTTP/1.1 405 Method Not Allowed", "{m}");
            assert_eq!(calls, 0);
        }
    }

    #[test]
    fn only_port_443_is_allowed() {
        for t in [
            "files.example.com:80",
            "files.example.com:8443",
            "files.example.com",
            "files.example.com:0443x",
        ] {
            let (st, calls, hosts) = refusal(
                format!("CONNECT {t} HTTP/1.1\r\n\r\n").as_bytes(),
                &["files.example.com"],
            );
            assert_eq!(st, "HTTP/1.1 403 Forbidden", "{t}");
            assert_eq!(calls, 0);
            assert_eq!(hosts.len(), 1);
        }
    }

    #[test]
    fn hosts_outside_the_allowlist_are_403() {
        for t in [
            "evil.example.org:443",
            "example.com:443",
            "sub.files.example.com:443",
            "files.example.com.evil.io:443",
        ] {
            let (st, calls, hosts) = refusal(
                format!("CONNECT {t} HTTP/1.1\r\n\r\n").as_bytes(),
                &["files.example.com"],
            );
            assert_eq!(st, "HTTP/1.1 403 Forbidden", "{t}");
            assert_eq!(calls, 0);
            assert_eq!(hosts, vec![t.rsplit_once(':').unwrap().0.to_string()]);
        }
    }

    #[test]
    fn ip_literals_are_refused_even_when_a_wildcard_would_cover_them() {
        for t in [
            "1.1.1.1:443",
            "127.0.0.1:443",
            "[::1]:443",
            "[2606:4700:4700::1111]:443",
            "::1:443",
            "0x7f.1:443",
            "2130706433:443",
            "localhost:443",
        ] {
            let (st, calls, _) = refusal(
                format!("CONNECT {t} HTTP/1.1\r\n\r\n").as_bytes(),
                &["*.example.com", "files.example.com"],
            );
            assert_eq!(st, "HTTP/1.1 403 Forbidden", "{t}");
            assert_eq!(calls, 0);
        }
    }

    #[test]
    fn allowed_names_resolving_to_private_addresses_are_refused() {
        for ip in [
            "127.0.0.1",
            "10_0_0_5",
            "169.254.169.254",
            "100.64.0.9",
            "::1",
            "::ffff:192_168_0_1",
            "fd00::1",
            "fe80::1",
        ] {
            let proxy = Proxy::start_with(
                pats(&["files.example.com"]),
                Box::new(Strict(Raw(vec![addr(ip)]))),
            )
            .unwrap();
            let resp = connect(proxy.port, "files.example.com:443");
            assert_eq!(status(&resp), "HTTP/1.1 403 Forbidden", "{ip}");
            assert_eq!(proxy.refused_hosts(), vec!["files.example.com".to_string()]);
        }
    }

    #[test]
    fn unreachable_upstream_is_502() {
        let dead = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap()
        };
        let (resolver, _) = counted(dead);
        let proxy = Proxy::start_with(pats(&["files.example.com"]), resolver).unwrap();
        assert_eq!(
            status(&connect(proxy.port, "files.example.com:443")),
            "HTTP/1.1 502 Bad Gateway"
        );
    }

    #[test]
    fn oversized_heads_are_431() {
        let (resolver, calls) = counted("127.0.0.1:9".parse().unwrap());
        let proxy = Proxy::start_with(pats(&["files.example.com"]), resolver).unwrap();
        let mut req = b"CONNECT files.example.com:443 HTTP/1.1\r\nX-Pad: ".to_vec();
        req.extend(std::iter::repeat_n(b'a', MAX_HEAD + 100));
        let resp = talk(proxy.port, &req);
        assert_eq!(
            status(&resp),
            "HTTP/1.1 431 Request Header Fields Too Large"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn malformed_request_lines_are_400() {
        for r in [
            "\r\n\r\n",
            "CONNECT\r\n\r\n",
            "CONNECT a.example.com:443\r\n\r\n",
            "CONNECT a b c d\r\n\r\n",
            "CONNECT a.example.com:443 SPDY/3\r\n\r\n",
        ] {
            let (st, calls, _) = refusal(r.as_bytes(), &["files.example.com"]);
            assert_eq!(st, "HTTP/1.1 400 Bad Request", "{r:?}");
            assert_eq!(calls, 0);
        }
    }

    #[test]
    fn refused_hosts_are_logged_without_control_characters() {
        let (_, _, hosts) = refusal(
            b"CONNECT bad\x01host.example.org:443 HTTP/1.1\r\n\r\n",
            &["files.example.com"],
        );
        assert_eq!(hosts, vec!["bad?host.example.org".to_string()]);
    }

    #[test]
    fn dropping_the_proxy_stops_the_listener() {
        let (resolver, _) = counted("127.0.0.1:9".parse().unwrap());
        let proxy = Proxy::start_with(pats(&["files.example.com"]), resolver).unwrap();
        let port = proxy.port;
        drop(proxy);
        assert!(TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], port)),
            Duration::from_millis(500)
        )
        .is_err());
    }
}
