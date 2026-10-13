use serde_json::json;
use std::io;
#[cfg(target_os = "macos")]
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::thread;
use tmgr_ipc::endpoint::{bind, connect, Endpoint};
use tmgr_ipc::frame::{read_frame, write_json, Frame};
use tmgr_ipc::peer::peer_cred;
use tmgr_ipc::server::{accept_check, handshake_client, handshake_server};

const SID: &str = "s_abcdefghij";

struct Dir {
    _t: tempfile::TempDir,
    path: PathBuf,
}

impl Dir {
    fn path(&self) -> &Path {
        &self.path
    }
}

fn short_dir() -> Dir {
    let t = tempfile::Builder::new()
        .prefix("tr")
        .tempdir_in("/tmp")
        .unwrap();
    std::fs::set_permissions(
        t.path(),
        <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700),
    )
    .unwrap();
    let path = t.path().join("r");
    Dir { _t: t, path }
}

fn runner(dir: &Path) -> Endpoint {
    Endpoint::new_runner(dir).unwrap()
}

fn serve_once(ep: &Endpoint, secret: &'static str) -> thread::JoinHandle<io::Result<()>> {
    let l = bind(ep).unwrap();
    thread::spawn(move || {
        let (s, _) = l.accept()?;
        accept_check(&s)?;
        handshake_server(&s, secret, "9.9.9").map(|_| ())
    })
}

#[test]
fn handshake_ok() {
    let t = short_dir();
    let ep = runner(t.path());
    let h = serve_once(&ep, "right");
    let s = connect(&ep).unwrap();
    let w = handshake_client(&s, "cli", "right").unwrap();
    assert_eq!(w.runner_version, "9.9.9");
    assert_eq!(w.proto, 1);
    h.join().unwrap().unwrap();
}

#[test]
fn wrong_secret_fails_in_both_directions() {
    let t = short_dir();
    let ep = runner(t.path());
    let h = serve_once(&ep, "right");
    let s = connect(&ep).unwrap();
    let e = handshake_client(&s, "cli", "wrong").unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
    assert!(e.to_string().contains("identity check failed"), "{e}");
    assert!(h.join().unwrap().is_err());

    let t = short_dir();
    let ep = runner(t.path());
    let h = serve_once(&ep, "wrong");
    let s = connect(&ep).unwrap();
    let e = handshake_client(&s, "cli", "right").unwrap_err();
    assert!(e.to_string().contains("identity check failed"), "{e}");
    assert!(h.join().unwrap().is_err());
}

#[test]
fn client_without_the_secret_is_unauthorized() {
    let t = short_dir();
    let ep = runner(t.path());
    let h = serve_once(&ep, "right");
    let mut s = connect(&ep).unwrap();
    write_json(
        &mut s,
        &json!({"type": "hello", "proto": 1, "client": "x", "version": "x", "pid": 1, "nonce_c": "aa"}),
    )
    .unwrap();
    let Some(Frame::Json(c)) = read_frame(&mut s).unwrap() else {
        panic!("no challenge")
    };
    assert_eq!(c["type"], "challenge");
    write_json(&mut s, &json!({"type": "auth", "proof_c": "00"})).unwrap();
    let Some(Frame::Json(v)) = read_frame(&mut s).unwrap() else {
        panic!("no reply")
    };
    assert_eq!(v["code"], "unauthorized");
    let e = h.join().unwrap().unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
}

#[test]
fn fake_server_is_rejected_and_learns_no_secret() {
    use std::io::Read;
    const SECRET: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let t = short_dir();
    let ep = runner(t.path());
    let l = bind(&ep).unwrap();
    let fake = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let Some(Frame::Json(h)) = read_frame(&mut s).unwrap() else {
            panic!("no hello")
        };
        let nonce_c = h["nonce_c"].as_str().unwrap().to_string();
        write_json(
            &mut s,
            &json!({"type": "challenge", "nonce_s": "11".repeat(32), "proof_s": "22".repeat(32)}),
        )
        .unwrap();
        let mut rest = Vec::new();
        s.read_to_end(&mut rest).unwrap();
        (h.to_string(), nonce_c, rest)
    });
    let s = connect(&ep).unwrap();
    let e = handshake_client(&s, "cli", SECRET).unwrap_err();
    assert!(e.to_string().contains("identity check failed"), "{e}");
    drop(s);
    let (hello, nonce_c, rest) = fake.join().unwrap();
    assert!(!hello.contains(SECRET));
    assert_eq!(nonce_c.len(), 64);
    assert!(rest.is_empty(), "client sent {} more bytes", rest.len());
}

#[test]
fn trickling_handshake_is_cut_off_by_the_absolute_deadline() {
    use std::io::Write;
    use std::time::{Duration, Instant};
    let t = short_dir();
    let ep = runner(t.path());
    let h = serve_once(&ep, "right");
    let mut s = connect(&ep).unwrap();
    let started = Instant::now();
    let frame = br#"{"type":"hello","proto":1,"client":"x","version":"x","pid":1,"nonce_c":"aa"}"#;
    let mut bytes = (frame.len() as u32 + 1).to_be_bytes().to_vec();
    bytes.push(0);
    bytes.extend_from_slice(frame);
    for b in bytes {
        if s.write_all(&[b]).is_err() {
            break;
        }
        thread::sleep(Duration::from_secs(1));
        if started.elapsed() > Duration::from_secs(8) {
            break;
        }
    }
    let e = h.join().unwrap().unwrap_err();
    assert!(
        matches!(
            e.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ),
        "{e}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(7),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn wrong_proto_is_mismatch() {
    let t = short_dir();
    let ep = runner(t.path());
    let h = serve_once(&ep, "right");
    let mut s = connect(&ep).unwrap();
    write_json(
        &mut s,
        &json!({"type": "hello", "proto": 2, "client": "cli", "version": "x", "pid": 1, "nonce_c": "aa"}),
    )
    .unwrap();
    let Some(Frame::Json(v)) = read_frame(&mut s).unwrap() else {
        panic!("no reply")
    };
    assert_eq!(v["type"], "error");
    assert_eq!(v["code"], "proto_mismatch");
    assert!(h.join().unwrap().is_err());
}

#[test]
fn non_hello_first_message_is_rejected() {
    let t = short_dir();
    let ep = runner(t.path());
    let h = serve_once(&ep, "right");
    let mut s = connect(&ep).unwrap();
    write_json(&mut s, &json!({"type": "request", "id": 1, "method": "x"})).unwrap();
    assert!(h.join().unwrap().is_err());
}

#[test]
fn peer_cred_reports_our_uid_and_pid() {
    let t = short_dir();
    let ep = runner(t.path());
    let l = bind(&ep).unwrap();
    let c = connect(&ep).unwrap();
    let (s, _) = l.accept().unwrap();
    let uid = unsafe { libc::geteuid() };
    for side in [&c, &s] {
        let cred = peer_cred(side).unwrap();
        assert_eq!(cred.uid, uid);
        assert_eq!(cred.pid, Some(std::process::id() as i32));
    }
    assert_eq!(accept_check(&s).unwrap().uid, uid);
}

#[test]
fn session_endpoints_and_id_validation() {
    let t = short_dir();
    let ep = Endpoint::new_session(t.path(), SID).unwrap();
    let l = bind(&ep).unwrap();
    assert_eq!(Endpoint::load_session(t.path(), SID).unwrap(), ep);
    let c = connect(&ep).unwrap();
    let (s, _) = l.accept().unwrap();
    drop((c, s));
    for bad in ["../x", "S_abcdefghij", "s_short"] {
        assert_eq!(
            Endpoint::new_session(t.path(), bad).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            Endpoint::load_session(t.path(), bad).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
}

#[test]
fn load_runner_matches_new_runner() {
    let t = short_dir();
    let ep = runner(t.path());
    assert_eq!(Endpoint::load_runner(t.path()).unwrap(), ep);
}

#[cfg(not(target_os = "linux"))]
mod pathname {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;

    fn path_of(ep: &Endpoint) -> std::path::PathBuf {
        match ep {
            Endpoint::Path(p) => p.clone(),
            Endpoint::Abstract(_) => unreachable!(),
        }
    }

    #[test]
    fn socket_is_0600() {
        let t = short_dir();
        let ep = runner(t.path());
        let _l = bind(&ep).unwrap();
        let mode = fs::metadata(path_of(&ep)).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn stale_socket_is_replaced() {
        let t = short_dir();
        let ep = runner(t.path());
        drop(UnixListener::bind(path_of(&ep)).unwrap());
        assert!(path_of(&ep).exists());
        let h = serve_once(&ep, "right");
        let s = connect(&ep).unwrap();
        handshake_client(&s, "cli", "right").unwrap();
        h.join().unwrap().unwrap();
    }

    #[test]
    fn live_server_is_addr_in_use() {
        let t = short_dir();
        let ep = runner(t.path());
        let _l = bind(&ep).unwrap();
        assert_eq!(bind(&ep).unwrap_err().kind(), io::ErrorKind::AddrInUse);
    }

    #[test]
    fn regular_file_is_not_replaced() {
        let t = short_dir();
        let ep = runner(t.path());
        fs::write(path_of(&ep), "x").unwrap();
        assert!(bind(&ep).is_err());
        assert!(path_of(&ep).exists());
    }

    #[test]
    fn open_parent_dir_refused_on_connect() {
        let t = short_dir();
        let ep = runner(t.path());
        let _l = bind(&ep).unwrap();
        fs::set_permissions(t.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            connect(&ep).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn long_path_has_clear_error() {
        let t = short_dir();
        let long = t.path().join("x".repeat(100));
        let e = Endpoint::new_runner(&long).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidInput);
        assert!(e.to_string().contains("TMGR_RUNNER_DIR"), "{e}");
        let e = bind(&Endpoint::Path(long.join("r.sock"))).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(
            Endpoint::new_session(&long, SID).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn connect_refused_without_server() {
        let t = short_dir();
        let ep = runner(t.path());
        let _ = UnixStream::connect(path_of(&ep));
        assert!(connect(&ep).is_err());
    }
}

#[cfg(target_os = "linux")]
mod abstract_sockets {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn runner_addr_file_is_private_and_named() {
        let t = short_dir();
        let ep = runner(t.path());
        let Endpoint::Abstract(name) = &ep else {
            panic!("expected abstract")
        };
        let uid = unsafe { libc::geteuid() };
        assert!(name.starts_with(&format!("tmgr-{uid}-")));
        assert_eq!(name.len(), format!("tmgr-{uid}-").len() + 16);
        let mode = fs::metadata(t.path().join("runner.addr"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn addr_file_replaced_atomically() {
        let t = short_dir();
        let a = runner(t.path());
        let b = runner(t.path());
        assert_ne!(a, b);
        assert_eq!(Endpoint::load_runner(t.path()).unwrap(), b);
    }

    #[test]
    fn live_abstract_name_is_addr_in_use() {
        let t = short_dir();
        let ep = runner(t.path());
        let _l = bind(&ep).unwrap();
        assert_eq!(bind(&ep).unwrap_err().kind(), io::ErrorKind::AddrInUse);
    }

    #[test]
    fn loose_addr_file_refused() {
        let t = short_dir();
        runner(t.path());
        let p = t.path().join("runner.addr");
        fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(Endpoint::load_runner(t.path()).is_err());
    }
}
