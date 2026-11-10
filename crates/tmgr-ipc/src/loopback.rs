use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine;

const BODY_CAP: usize = 1024 * 1024;
const HEADER_CAP: usize = 64 * 1024;
const LSOF_DEADLINE: Duration = Duration::from_secs(1);
const LSOF_UID_DEADLINE: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpErr {
    Refused,
    Timeout,
    Other,
}

pub fn parse_url(s: &str) -> Option<SocketAddr> {
    let rest = s.strip_prefix("http://")?;
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    let (ip, port) = match rest.strip_prefix("[::1]:") {
        Some(p) => (IpAddr::V6(Ipv6Addr::LOCALHOST), p),
        None => (
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            rest.strip_prefix("127.0.0.1:")?,
        ),
    };
    if port.is_empty() || port.len() > 8 || !port.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let port: u32 = port.parse().ok()?;
    if !(1..=65535).contains(&port) {
        return None;
    }
    Some(SocketAddr::new(ip, port as u16))
}

pub fn canonical_url(addr: SocketAddr) -> String {
    match addr {
        SocketAddr::V4(a) => format!("http://{}:{}", a.ip(), a.port()),
        SocketAddr::V6(a) => format!("http://[{}]:{}", a.ip(), a.port()),
    }
}

pub fn valid_session_id(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn basic_header(password: &str) -> String {
    let raw = format!("opencode:{password}");
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(raw)
    )
}

fn clean(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
}

fn map_io(e: &std::io::Error) -> HttpErr {
    match e.kind() {
        std::io::ErrorKind::ConnectionRefused => HttpErr::Refused,
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => HttpErr::Timeout,
        _ => HttpErr::Other,
    }
}

fn remaining(end: Instant) -> Result<Duration, HttpErr> {
    let left = end.saturating_duration_since(Instant::now());
    if left.is_zero() {
        Err(HttpErr::Timeout)
    } else {
        Ok(left)
    }
}

pub fn request(
    addr: SocketAddr,
    method: &str,
    path: &str,
    auth: Option<&str>,
    body: Option<&[u8]>,
    deadline: Duration,
) -> Result<(u16, Vec<u8>), HttpErr> {
    if !clean(method)
        || method.contains(' ')
        || !clean(path)
        || path.contains(' ')
        || !path.starts_with('/')
        || auth.is_some_and(|a| !clean(a))
    {
        return Err(HttpErr::Other);
    }
    let end = Instant::now() + deadline;
    let mut stream = TcpStream::connect_timeout(&addr, remaining(end)?).map_err(|e| map_io(&e))?;
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n");
    if let Some(a) = auth {
        head.push_str("Authorization: ");
        head.push_str(a);
        head.push_str("\r\n");
    }
    if let Some(b) = body {
        head.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            b.len()
        ));
    }
    head.push_str("\r\n");
    let mut out = head.into_bytes();
    if let Some(b) = body {
        out.extend_from_slice(b);
    }
    let mut sent = 0;
    while sent < out.len() {
        stream
            .set_write_timeout(Some(remaining(end)?))
            .map_err(|e| map_io(&e))?;
        match stream.write(&out[sent..]) {
            Ok(0) => return Err(HttpErr::Other),
            Ok(n) => sent += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(map_io(&e)),
        }
    }
    let mut raw = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        stream
            .set_read_timeout(Some(remaining(end)?))
            .map_err(|e| map_io(&e))?;
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if raw.len() > BODY_CAP + HEADER_CAP {
                    return Err(HttpErr::Other);
                }
                if response_complete(&raw, method)? {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(map_io(&e)),
        }
    }
    parse_response(&raw, method)
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

struct Head {
    status: u16,
    length: Option<usize>,
    chunked: bool,
    body_start: usize,
}

fn no_body(status: u16, method: &str) -> bool {
    method.eq_ignore_ascii_case("HEAD") || matches!(status, 204 | 304)
}

fn parse_head(raw: &[u8]) -> Result<Option<Head>, HttpErr> {
    let Some(split) = find(raw, b"\r\n\r\n") else {
        if raw.len() > HEADER_CAP {
            return Err(HttpErr::Other);
        }
        return Ok(None);
    };
    if split > HEADER_CAP {
        return Err(HttpErr::Other);
    }
    let head = std::str::from_utf8(&raw[..split]).map_err(|_| HttpErr::Other)?;
    let mut lines = head.split("\r\n");
    let status_line = lines.next().ok_or(HttpErr::Other)?;
    let mut parts = status_line.split(' ');
    if !parts.next().is_some_and(|v| v.starts_with("HTTP/1.")) {
        return Err(HttpErr::Other);
    }
    let status: u16 = parts
        .next()
        .and_then(|c| c.parse().ok())
        .filter(|c| (100..=599).contains(c))
        .ok_or(HttpErr::Other)?;
    let mut length: Option<usize> = None;
    let mut chunked = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Err(HttpErr::Other);
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            let n = value.parse().map_err(|_| HttpErr::Other)?;
            if length.is_some_and(|prev| prev != n) {
                return Err(HttpErr::Other);
            }
            length = Some(n);
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            let last = value.rsplit(',').next().unwrap_or("").trim();
            if !last.eq_ignore_ascii_case("chunked") {
                return Err(HttpErr::Other);
            }
            chunked = true;
        }
    }
    Ok(Some(Head {
        status,
        length,
        chunked,
        body_start: split + 4,
    }))
}

fn response_complete(raw: &[u8], method: &str) -> Result<bool, HttpErr> {
    let Some(h) = parse_head(raw)? else {
        return Ok(false);
    };
    if no_body(h.status, method) {
        return Ok(true);
    }
    let body = &raw[h.body_start..];
    Ok(if h.chunked {
        decode_chunked(body).is_ok()
    } else if let Some(n) = h.length {
        n > BODY_CAP || body.len() >= n
    } else {
        false
    })
}

fn parse_response(raw: &[u8], method: &str) -> Result<(u16, Vec<u8>), HttpErr> {
    let h = parse_head(raw)?.ok_or(HttpErr::Other)?;
    if no_body(h.status, method) {
        return Ok((h.status, Vec::new()));
    }
    let body = &raw[h.body_start..];
    let body = if h.chunked {
        decode_chunked(body)?
    } else if let Some(n) = h.length {
        if n > BODY_CAP || body.len() < n {
            return Err(HttpErr::Other);
        }
        body[..n].to_vec()
    } else {
        body.to_vec()
    };
    if body.len() > BODY_CAP {
        return Err(HttpErr::Other);
    }
    Ok((h.status, body))
}

fn decode_chunked(mut data: &[u8]) -> Result<Vec<u8>, HttpErr> {
    let mut out = Vec::new();
    loop {
        let eol = find(data, b"\r\n").ok_or(HttpErr::Other)?;
        let size_line = std::str::from_utf8(&data[..eol]).map_err(|_| HttpErr::Other)?;
        let size_hex = size_line.split(';').next().unwrap_or("").trim();
        if size_hex.is_empty() || size_hex.len() > 8 {
            return Err(HttpErr::Other);
        }
        let size = usize::from_str_radix(size_hex, 16).map_err(|_| HttpErr::Other)?;
        data = &data[eol + 2..];
        if size == 0 {
            return Ok(out);
        }
        if out.len() + size > BODY_CAP || data.len() < size + 2 || &data[size..size + 2] != b"\r\n"
        {
            return Err(HttpErr::Other);
        }
        out.extend_from_slice(&data[..size]);
        data = &data[size + 2..];
    }
}

fn lsof(args: &[String], deadline: Duration) -> Result<String, ()> {
    let bin = ["/usr/sbin/lsof", "/usr/bin/lsof"]
        .into_iter()
        .find(|p| std::path::Path::new(p).exists())
        .ok_or(())?;
    let mut child = Command::new(bin)
        .env_clear()
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ())?;
    let mut stdout = child.stdout.take().ok_or(())?;
    let reader = thread::spawn(move || {
        let mut s = String::new();
        let _ = (&mut stdout).take(BODY_CAP as u64).read_to_string(&mut s);
        s
    });
    let end = Instant::now() + deadline;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if Instant::now() < end => thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(());
            }
        }
    };
    let out = reader.join().map_err(|_| ())?;
    match status.code() {
        Some(0) => Ok(out),
        Some(1) if out.is_empty() => Ok(out),
        _ => Err(()),
    }
}

fn lsof_args(who: [&str; 2], addr: SocketAddr) -> Vec<String> {
    let family = if addr.is_ipv4() { "-i4TCP" } else { "-i6TCP" };
    [
        "-nP",
        "-w",
        "-a",
        who[0],
        who[1],
        &format!("{family}:{}", addr.port()),
        "-sTCP:LISTEN",
        "-Fpn",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn local_matches(name: &str, addr: SocketAddr) -> bool {
    let local = name.split("->").next().unwrap_or("");
    let Some(host) = local.strip_suffix(&format!(":{}", addr.port())) else {
        return false;
    };
    match addr {
        SocketAddr::V4(_) => matches!(host, "127.0.0.1" | "*"),
        SocketAddr::V6(_) => matches!(host, "[::1]" | "*"),
    }
}

#[allow(clippy::result_unit_err)]
pub fn listener_pid(pid: u32, addr: SocketAddr) -> Result<bool, ()> {
    let out = lsof(&lsof_args(["-p", &pid.to_string()], addr), LSOF_DEADLINE)?;
    let mut current: Option<u32> = None;
    for line in out.lines() {
        if let Some(p) = line.strip_prefix('p') {
            current = p.parse().ok();
        } else if let Some(n) = line.strip_prefix('n') {
            if current == Some(pid) && local_matches(n, addr) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[allow(clippy::result_unit_err)]
pub fn listener_owned(uid: u32, addr: SocketAddr) -> Result<bool, ()> {
    let out = lsof(&lsof_args(["-u", &uid.to_string()], addr), LSOF_UID_DEADLINE)?;
    Ok(out
        .lines()
        .filter_map(|l| l.strip_prefix('n'))
        .any(|n| local_matches(n, addr)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc;

    const D: Duration = Duration::from_secs(5);

    #[test]
    fn parse_url_accepts() {
        let a = parse_url("http://127.0.0.1:4096").unwrap();
        assert_eq!(a, "127.0.0.1:4096".parse().unwrap());
        assert_eq!(parse_url("http://127.0.0.1:4096/"), Some(a));
        let b = parse_url("http://[::1]:80/").unwrap();
        assert_eq!(b, "[::1]:80".parse().unwrap());
        assert!(parse_url("http://127.0.0.1:65535").is_some());
        assert!(parse_url("http://127.0.0.1:1").is_some());
    }

    #[test]
    fn parse_url_rejects() {
        for s in [
            "",
            "http://localhost:4096",
            "http://0.0.0.0:4096",
            "http://127.0.0.2:4096",
            "http://127.1.2.3:4096",
            "https://127.0.0.1:4096",
            "http://user@127.0.0.1:4096",
            "http://user:pw@127.0.0.1:4096",
            "http://127.0.0.1:4096/x",
            "http://127.0.0.1:4096//",
            "http://127.0.0.1:4096/?a=1",
            "http://127.0.0.1:4096?a=1",
            "http://127.0.0.1:4096#f",
            "http://127.0.0.1:4096 ",
            " http://127.0.0.1:4096",
            "http://127.0.0.1: 4096",
            "http://127.0.0.1:4096\n",
            "http://127.0.0.1:0",
            "http://127.0.0.1:65536",
            "http://127.0.0.1:+80",
            "http://127.0.0.1:-1",
            "http://127.0.0.1:",
            "http://127.0.0.1",
            "http://[::1]",
            "http://[::2]:80",
            "http://[::]:80",
            "http://[::1]:0",
            "http://::1:80",
            "127.0.0.1:4096",
            "HTTP://127.0.0.1:4096",
            "http://127.0.0.1:4096:5",
            "http://127.0.0.1:0x50",
        ] {
            assert!(parse_url(s).is_none(), "{s:?}");
        }
    }

    #[test]
    fn canonical_round_trip() {
        for s in ["http://127.0.0.1:4096", "http://[::1]:8080"] {
            let a = parse_url(s).unwrap();
            assert_eq!(canonical_url(a), s);
            assert_eq!(parse_url(&canonical_url(a)), Some(a));
        }
        assert_eq!(
            parse_url("http://127.0.0.1:0080/")
                .map(canonical_url)
                .as_deref(),
            Some("http://127.0.0.1:80")
        );
    }

    #[test]
    fn session_ids() {
        assert!(valid_session_id("ses_abc-123"));
        assert!(valid_session_id(&"a".repeat(64)));
        for s in ["", "a/b", "a.b", "..", "a b", "é", "a\n"] {
            assert!(!valid_session_id(s), "{s:?}");
        }
        assert!(!valid_session_id(&"a".repeat(65)));
    }

    #[test]
    fn basic_known_vector() {
        assert_eq!(basic_header("secret"), "Basic b3BlbmNvZGU6c2VjcmV0");
    }

    fn serve(reply: Vec<u8>) -> (SocketAddr, mpsc::Receiver<Vec<u8>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = s.read(&mut buf).unwrap();
                got.extend_from_slice(&buf[..n]);
                if let Some(i) = find(&got, b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&got[..i]).to_lowercase();
                    let want = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if got.len() >= i + 4 + want {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let _ = s.write_all(&reply);
            let _ = tx.send(got);
        });
        (addr, rx)
    }

    #[test]
    fn sends_exact_request_with_body_and_auth() {
        let (addr, rx) = serve(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n".to_vec());
        let r = request(
            addr,
            "POST",
            "/session/s1/message",
            Some("Basic abc"),
            Some(b"{\"a\":1}"),
            D,
        );
        assert_eq!(r, Ok((204, Vec::new())));
        let got = String::from_utf8(rx.recv().unwrap()).unwrap();
        let want = format!(
            "POST /session/s1/message HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nAuthorization: Basic abc\r\nContent-Type: application/json\r\nContent-Length: 7\r\n\r\n{{\"a\":1}}"
        );
        assert_eq!(got, want);
    }

    #[test]
    fn sends_no_auth_no_body() {
        let (addr, rx) = serve(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".to_vec());
        let r = request(addr, "GET", "/health", None, None, D);
        assert_eq!(r, Ok((200, b"ok".to_vec())));
        let got = String::from_utf8(rx.recv().unwrap()).unwrap();
        assert_eq!(
            got,
            format!("GET /health HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n")
        );
    }

    #[test]
    fn decodes_chunked() {
        let (addr, _rx) = serve(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6;x=1\r\n world\r\n0\r\n\r\n".to_vec(),
        );
        assert_eq!(
            request(addr, "GET", "/", None, None, D),
            Ok((200, b"hello world".to_vec()))
        );
    }

    #[test]
    fn rejects_malformed_chunked() {
        let (addr, _rx) = serve(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\nhi\r\n0\r\n\r\n".to_vec(),
        );
        assert_eq!(
            request(addr, "GET", "/", None, None, D),
            Err(HttpErr::Other)
        );
    }

    #[test]
    fn body_over_cap_is_other() {
        let mut reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            BODY_CAP + 1
        )
        .into_bytes();
        reply.extend(std::iter::repeat_n(b'x', BODY_CAP + 1));
        let (addr, _rx) = serve(reply);
        assert_eq!(
            request(addr, "GET", "/", None, None, D),
            Err(HttpErr::Other)
        );
    }

    #[test]
    fn body_at_cap_is_ok() {
        let mut reply =
            format!("HTTP/1.1 200 OK\r\nContent-Length: {BODY_CAP}\r\n\r\n").into_bytes();
        reply.extend(std::iter::repeat_n(b'x', BODY_CAP));
        let (addr, _rx) = serve(reply);
        let (code, body) = request(addr, "GET", "/", None, None, D).unwrap();
        assert_eq!((code, body.len()), (200, BODY_CAP));
    }

    #[test]
    fn slow_server_hits_deadline() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let (tx, rx) = mpsc::channel::<()>();
        thread::spawn(move || {
            let (_s, _) = l.accept().unwrap();
            let _ = rx.recv_timeout(Duration::from_secs(10));
        });
        let t = Instant::now();
        let dl = Duration::from_millis(300);
        assert_eq!(
            request(addr, "GET", "/", None, None, dl),
            Err(HttpErr::Timeout)
        );
        assert!(
            t.elapsed() < dl * 2 + Duration::from_millis(200),
            "{:?}",
            t.elapsed()
        );
        drop(tx);
    }

    #[test]
    fn trickling_server_hits_overall_deadline() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            for _ in 0..50 {
                if s.write_all(b"x").is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
        });
        let t = Instant::now();
        let dl = Duration::from_millis(400);
        assert_eq!(
            request(addr, "GET", "/", None, None, dl),
            Err(HttpErr::Timeout)
        );
        assert!(t.elapsed() < dl * 2, "{:?}", t.elapsed());
    }

    #[test]
    fn redirect_is_returned_not_followed() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut n = 0;
            for s in l.incoming() {
                let mut s = s.unwrap();
                n += 1;
                let _ = tx.send(n);
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                let _ = s.write_all(
                    b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/x\r\nContent-Length: 0\r\n\r\n",
                );
            }
        });
        assert_eq!(
            request(addr, "GET", "/", Some("Basic x"), None, D),
            Ok((302, Vec::new()))
        );
        thread::sleep(Duration::from_millis(200));
        assert_eq!(rx.try_iter().count(), 1);
    }

    #[test]
    fn refused() {
        let refused = (0..10).any(|_| {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = l.local_addr().unwrap();
            drop(l);
            request(addr, "GET", "/", None, None, D) == Err(HttpErr::Refused)
        });
        assert!(refused);
    }

    #[test]
    fn rejects_header_injection() {
        let addr: SocketAddr = "127.0.0.1:9".parse().unwrap();
        assert_eq!(
            request(addr, "GET", "/a\r\nX: y", None, None, D),
            Err(HttpErr::Other)
        );
        assert_eq!(
            request(addr, "GET", "/a b", None, None, D),
            Err(HttpErr::Other)
        );
        assert_eq!(
            request(addr, "GET", "/", Some("x\r\nY: z"), None, D),
            Err(HttpErr::Other)
        );
        assert_eq!(
            request(addr, "G\nET", "/", None, None, D),
            Err(HttpErr::Other)
        );
    }

    fn lsof_available() -> bool {
        ["/usr/sbin/lsof", "/usr/bin/lsof"]
            .iter()
            .any(|p| std::path::Path::new(p).exists())
    }

    fn sa(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    #[test]
    fn listener_pid_own_process() {
        if !lsof_available() {
            eprintln!("lsof missing, skipping");
            return;
        }
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        assert_eq!(listener_pid(std::process::id(), addr), Ok(true));
        assert_eq!(
            listener_pid(std::process::id().wrapping_add(1), addr),
            Ok(false)
        );
        assert_eq!(listener_pid(1, addr), Ok(false));
    }

    #[test]
    fn listener_pid_ipv6() {
        if !lsof_available() {
            eprintln!("lsof missing, skipping");
            return;
        }
        let Ok(l) = TcpListener::bind("[::1]:0") else {
            return;
        };
        let addr = l.local_addr().unwrap();
        assert_eq!(listener_pid(std::process::id(), addr), Ok(true));
    }

    #[test]
    fn listener_pid_wrong_family_is_false() {
        if !lsof_available() {
            eprintln!("lsof missing, skipping");
            return;
        }
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let v6 = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port);
        assert_eq!(listener_pid(std::process::id(), v6), Ok(false));
    }

    #[test]
    fn listener_owned_own_uid() {
        if !lsof_available() {
            eprintln!("lsof missing, skipping");
            return;
        }
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let uid = unsafe { libc::getuid() };
        assert_eq!(listener_owned(uid, addr), Ok(true));
        assert_eq!(listener_owned(uid.wrapping_add(12345), addr), Ok(false));
    }

    #[test]
    fn listener_owned_requires_matching_family() {
        if !lsof_available() {
            eprintln!("lsof missing, skipping");
            return;
        }
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let uid = unsafe { libc::getuid() };
        assert_eq!(
            listener_owned(uid, SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)),
            Ok(true)
        );
        assert_eq!(
            listener_owned(uid, SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port)),
            Ok(false)
        );
    }

    #[test]
    fn local_matches_filters_address() {
        let v4 = sa("127.0.0.1:80");
        let v6 = sa("[::1]:80");
        assert!(local_matches("127.0.0.1:80", v4));
        assert!(local_matches("*:80", v4));
        assert!(!local_matches("192.0.2.7:80", v4));
        assert!(!local_matches("[::1]:80", v4));
        assert!(!local_matches("127.0.0.1:81", v4));
        assert!(local_matches("[::1]:80", v6));
        assert!(local_matches("*:80", v6));
        assert!(!local_matches("127.0.0.1:80", v6));
        assert!(!local_matches("[2001:db8::1]:80", v6));
    }

    #[test]
    fn no_content_response_returns_without_close() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let (tx, rx) = mpsc::channel::<()>();
        thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let _ = s.write_all(b"HTTP/1.1 204 No Content\r\n\r\n");
            let _ = rx.recv_timeout(Duration::from_secs(10));
        });
        let t = Instant::now();
        assert_eq!(
            request(addr, "POST", "/", None, Some(b"{}"), Duration::from_secs(5)),
            Ok((204, Vec::new()))
        );
        assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
        drop(tx);
    }

    fn open_socket_server(reply: &'static [u8]) -> (SocketAddr, mpsc::Sender<()>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let (tx, rx) = mpsc::channel::<()>();
        thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let _ = s.write_all(reply);
            let _ = rx.recv_timeout(Duration::from_secs(10));
        });
        (addr, tx)
    }

    #[test]
    fn content_length_response_returns_without_close() {
        let (addr, _tx) = open_socket_server(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
        let t = Instant::now();
        assert_eq!(
            request(addr, "GET", "/", None, None, Duration::from_secs(5)),
            Ok((200, b"ok".to_vec()))
        );
        assert!(t.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn chunked_response_returns_at_terminator_without_close() {
        let (addr, _tx) = open_socket_server(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nok\r\n0\r\n\r\n",
        );
        let t = Instant::now();
        assert_eq!(
            request(addr, "GET", "/", None, None, Duration::from_secs(5)),
            Ok((200, b"ok".to_vec()))
        );
        assert!(t.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn head_response_ignores_content_length() {
        let (addr, _tx) = open_socket_server(b"HTTP/1.1 200 OK\r\nContent-Length: 50\r\n\r\n");
        let t = Instant::now();
        assert_eq!(
            request(addr, "HEAD", "/", None, None, Duration::from_secs(5)),
            Ok((200, Vec::new()))
        );
        assert!(t.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn transfer_encoding_last_token_decides() {
        let (addr, _rx) = serve(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n2\r\nok\r\n0\r\n\r\n".to_vec(),
        );
        assert_eq!(
            request(addr, "GET", "/", None, None, D),
            Ok((200, b"ok".to_vec()))
        );
        let (addr, _rx) = serve(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked, gzip\r\n\r\n2\r\nok\r\n0\r\n\r\n".to_vec(),
        );
        assert_eq!(
            request(addr, "GET", "/", None, None, D),
            Err(HttpErr::Other)
        );
        let (addr, _rx) = serve(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: identity\r\n\r\nok".to_vec(),
        );
        assert_eq!(
            request(addr, "GET", "/", None, None, D),
            Err(HttpErr::Other)
        );
    }
}
