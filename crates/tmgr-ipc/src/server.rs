use crate::frame::{read_frame, write_json, Frame};
use crate::peer::{peer_cred, PeerCred};
use crate::proto::{ErrorBody, Handshake, Hello, Welcome, PROTO_VERSION};
use crate::secret;
use std::io;
use std::os::unix::net::UnixStream;
use std::time::Duration;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

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

fn read_handshake(stream: &UnixStream) -> io::Result<Handshake> {
    let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
    let frame = read_frame(&mut &*stream);
    let _ = stream.set_read_timeout(None);
    match frame? {
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
    let Handshake::Hello(hello) = read_handshake(stream)? else {
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
    if !secret::eq(&hello.secret, expected_secret) {
        return Err(reject(
            stream,
            io::ErrorKind::PermissionDenied,
            "unauthorized",
            "bad secret",
        ));
    }
    let welcome = Welcome {
        proto: PROTO_VERSION,
        runner_version: runner_version.into(),
        caps: Vec::new(),
    };
    write_json(&mut &*stream, &Handshake::Welcome(welcome))?;
    Ok(hello)
}

pub fn handshake_client(stream: &UnixStream, client: &str, secret: &str) -> io::Result<Welcome> {
    let hello = Hello {
        proto: PROTO_VERSION,
        client: client.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        pid: std::process::id(),
        secret: secret.into(),
    };
    write_json(&mut &*stream, &Handshake::Hello(hello))?;
    match read_handshake(stream)? {
        Handshake::Welcome(w) => Ok(w),
        Handshake::Error(e) => {
            let kind = if e.code == "unauthorized" {
                io::ErrorKind::PermissionDenied
            } else {
                io::ErrorKind::InvalidData
            };
            Err(io::Error::new(kind, format!("{}: {}", e.code, e.message)))
        }
        Handshake::Hello(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unexpected hello",
        )),
    }
}
