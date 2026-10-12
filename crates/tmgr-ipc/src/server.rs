use crate::auth;
use crate::frame::{read_frame, write_json, Frame};
use crate::peer::{peer_cred, PeerCred};
use crate::proto::{Auth, Challenge, ErrorBody, Handshake, Hello, Welcome, PROTO_VERSION};
use std::io::{self, Read};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

pub const IDENTITY_FAILED: &str = "runner identity check failed (stale or foreign socket)";

struct DeadlineRead<'a> {
    stream: &'a UnixStream,
    deadline: Instant,
}

impl Read for DeadlineRead<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let _ = self.stream.set_read_timeout(Some(left));
        (&*self.stream).read(buf)
    }
}

pub fn read_frame_by(stream: &UnixStream, deadline: Instant) -> io::Result<Option<Frame>> {
    let r = read_frame(&mut DeadlineRead { stream, deadline });
    let _ = stream.set_read_timeout(None);
    r
}

pub fn accept_check(stream: &UnixStream) -> io::Result<PeerCred> {
    let cred = peer_cred(stream)?;
    if cred.uid != crate::euid() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "peer belongs to another user",
        ));
    }
    Ok(cred)
}

fn read_handshake(stream: &UnixStream, deadline: Instant) -> io::Result<Handshake> {
    match read_frame_by(stream, deadline)? {
        Some(Frame::Json(v)) => serde_json::from_value(v)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("bad handshake: {e}"))),
        Some(Frame::Bin(_)) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "binary frame in handshake",
        )),
        None => Err(io::ErrorKind::UnexpectedEof.into()),
    }
}

fn reject(stream: &UnixStream, kind: io::ErrorKind, code: &str, message: &str) -> io::Error {
    let body = ErrorBody {
        code: code.into(),
        message: message.into(),
    };
    let _ = write_json(&mut &*stream, &Handshake::Error(body));
    io::Error::new(kind, format!("{code}: {message}"))
}

pub fn handshake_server(
    stream: &UnixStream,
    expected_secret: &str,
    runner_version: &str,
) -> io::Result<Hello> {
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    let _ = stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT));
    let Handshake::Hello(hello) = read_handshake(stream, deadline)? else {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "expected hello"));
    };
    if hello.proto != PROTO_VERSION {
        return Err(reject(
            stream,
            io::ErrorKind::InvalidData,
            "proto_mismatch",
            &format!(
                "server speaks protocol {PROTO_VERSION}, client {}",
                hello.proto
            ),
        ));
    }
    let nonce_s = auth::nonce();
    let challenge = Challenge {
        proof_s: auth::proof(expected_secret, auth::SERVER, &hello.nonce_c, &nonce_s),
        nonce_s: nonce_s.clone(),
    };
    write_json(&mut &*stream, &Handshake::Challenge(challenge))?;
    let Handshake::Auth(Auth { proof_c }) = read_handshake(stream, deadline)? else {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "expected auth"));
    };
    if !auth::verify(
        expected_secret,
        auth::CLIENT,
        &nonce_s,
        &hello.nonce_c,
        &proof_c,
    ) {
        return Err(reject(
            stream,
            io::ErrorKind::PermissionDenied,
            "unauthorized",
            "bad proof",
        ));
    }
    let welcome = Welcome {
        proto: PROTO_VERSION,
        runner_version: runner_version.into(),
        caps: Vec::new(),
    };
    write_json(&mut &*stream, &Handshake::Welcome(welcome))?;
    let _ = stream.set_write_timeout(None);
    Ok(hello)
}

pub fn handshake_client(stream: &UnixStream, client: &str, secret: &str) -> io::Result<Welcome> {
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    let _ = stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT));
    let nonce_c = auth::nonce();
    let hello = Hello {
        proto: PROTO_VERSION,
        client: client.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        pid: std::process::id(),
        nonce_c: nonce_c.clone(),
    };
    write_json(&mut &*stream, &Handshake::Hello(hello))?;
    let remote_error = |e: ErrorBody| {
        let kind = if e.code == "unauthorized" {
            io::ErrorKind::PermissionDenied
        } else {
            io::ErrorKind::InvalidData
        };
        io::Error::new(kind, format!("{}: {}", e.code, e.message))
    };
    let challenge = match read_handshake(stream, deadline)? {
        Handshake::Challenge(c) => c,
        Handshake::Error(e) => return Err(remote_error(e)),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected handshake message",
            ))
        }
    };
    if !auth::verify(
        secret,
        auth::SERVER,
        &nonce_c,
        &challenge.nonce_s,
        &challenge.proof_s,
    ) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            IDENTITY_FAILED,
        ));
    }
    let proof_c = auth::proof(secret, auth::CLIENT, &challenge.nonce_s, &nonce_c);
    write_json(&mut &*stream, &Handshake::Auth(Auth { proof_c }))?;
    match read_handshake(stream, deadline)? {
        Handshake::Welcome(w) => {
            let _ = stream.set_write_timeout(None);
            Ok(w)
        }
        Handshake::Error(e) => Err(remote_error(e)),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unexpected handshake message",
        )),
    }
}
