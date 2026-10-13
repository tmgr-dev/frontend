use std::io;
use std::path::{Path, PathBuf};

use super::conn::{Conn, ConnError, Welcome};

#[cfg(unix)]
pub type Sock = std::os::unix::net::UnixStream;

#[cfg(not(unix))]
pub struct Sock;

#[cfg(not(unix))]
impl Sock {
  pub fn try_clone(&self) -> io::Result<Sock> {
    Err(io::ErrorKind::Unsupported.into())
  }
  pub fn shutdown(&self, _how: std::net::Shutdown) -> io::Result<()> {
    Ok(())
  }
  pub fn set_read_timeout(&self, _d: Option<std::time::Duration>) -> io::Result<()> {
    Ok(())
  }
}

#[cfg(not(unix))]
impl io::Read for Sock {
  fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
    Err(io::ErrorKind::Unsupported.into())
  }
}

#[cfg(not(unix))]
impl io::Write for Sock {
  fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
    Err(io::ErrorKind::Unsupported.into())
  }
  fn flush(&mut self) -> io::Result<()> {
    Ok(())
  }
}

#[cfg(unix)]
pub const CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
#[cfg(all(unix, not(target_os = "linux")))]
const MAX_SUN_PATH: usize = 103;

pub fn resolve_dir(
  override_dir: Option<PathBuf>,
  home: Option<PathBuf>,
  xdg_runtime: Option<PathBuf>,
  xdg_state: Option<PathBuf>,
) -> io::Result<PathBuf> {
  if let Some(p) = override_dir {
    return Ok(p);
  }
  let home = || home.clone().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"));
  if cfg!(target_os = "macos") {
    return Ok(home()?.join("Library/Application Support/dev.tmgr.cli/run"));
  }
  if let Some(p) = xdg_runtime {
    return Ok(p.join("tmgr"));
  }
  Ok(xdg_state.map_or_else(|| home().map(|h| h.join(".local/state")), Ok)?.join("tmgr/run"))
}

fn env_path(name: &str) -> Option<PathBuf> {
  std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

pub fn runner_dir() -> io::Result<PathBuf> {
  resolve_dir(env_path("TMGR_RUNNER_DIR"), env_path("HOME"), env_path("XDG_RUNTIME_DIR"), env_path("XDG_STATE_HOME"))
}

#[cfg(unix)]
fn euid() -> u32 {
  unsafe { libc::geteuid() }
}

#[cfg(unix)]
fn denied(msg: String) -> io::Error {
  io::Error::new(io::ErrorKind::PermissionDenied, msg)
}

#[cfg(unix)]
pub fn check_private_dir(path: &Path) -> io::Result<()> {
  use std::os::unix::fs::MetadataExt;
  let meta = std::fs::symlink_metadata(path)?;
  if meta.file_type().is_symlink() {
    return Err(denied("runner directory is a symlink".to_owned()));
  }
  if !meta.is_dir() {
    return Err(denied("runner directory is not a directory".to_owned()));
  }
  if meta.uid() != euid() {
    return Err(denied("runner directory is owned by another user".to_owned()));
  }
  if meta.mode() & 0o077 != 0 {
    return Err(denied("runner directory must not be accessible by group or others".to_owned()));
  }
  Ok(())
}

#[cfg(unix)]
pub fn read_secret(path: &Path) -> io::Result<String> {
  use std::os::unix::fs::MetadataExt;
  let meta = std::fs::symlink_metadata(path)?;
  if !meta.file_type().is_file() {
    return Err(denied("runner secret is not a regular file".to_owned()));
  }
  if meta.uid() != euid() {
    return Err(denied("runner secret is owned by another user".to_owned()));
  }
  if meta.mode() & 0o077 != 0 {
    return Err(denied("runner secret must not be accessible by group or others".to_owned()));
  }
  Ok(std::fs::read_to_string(path)?.trim().to_owned())
}

#[cfg(unix)]
fn peer_uid(stream: &std::os::unix::net::UnixStream) -> io::Result<u32> {
  use std::os::fd::AsRawFd;
  #[cfg(target_os = "linux")]
  {
    let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let rc = unsafe {
      libc::getsockopt(stream.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, (&mut cred as *mut libc::ucred).cast(), &mut len)
    };
    if rc != 0 {
      return Err(io::Error::last_os_error());
    }
    Ok(cred.uid)
  }
  #[cfg(target_os = "macos")]
  {
    let (mut uid, mut gid) = (0 as libc::uid_t, 0 as libc::gid_t);
    if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0 {
      return Err(io::Error::last_os_error());
    }
    Ok(uid)
  }
  #[cfg(not(any(target_os = "linux", target_os = "macos")))]
  {
    let _ = stream;
    Err(io::Error::new(io::ErrorKind::Unsupported, "peer credentials are not supported on this platform"))
  }
}

#[cfg(unix)]
fn open_stream(dir: &Path) -> io::Result<std::os::unix::net::UnixStream> {
  use std::os::unix::net::UnixStream;
  #[cfg(target_os = "linux")]
  {
    use std::os::linux::net::SocketAddrExt;
    let name = read_secret(&dir.join("runner.addr"))?;
    let addr = std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes())?;
    UnixStream::connect_addr(&addr)
  }
  #[cfg(not(target_os = "linux"))]
  {
    let path = dir.join("r.sock");
    if path.as_os_str().len() > MAX_SUN_PATH {
      return Err(io::Error::new(io::ErrorKind::InvalidInput, "runner directory path is too long for a unix socket"));
    }
    UnixStream::connect(path)
  }
}

#[cfg(unix)]
pub fn connect_in(dir: &Path) -> Result<(Conn<Sock>, Welcome), ConnError> {
  check_private_dir(dir)?;
  let secret = read_secret(&dir.join("client.secret"))?;
  let stream = open_stream(dir)?;
  if peer_uid(&stream)? != euid() {
    return Err(ConnError::Failed("runner socket belongs to another user".to_owned()));
  }
  stream.set_read_timeout(Some(CALL_TIMEOUT))?;
  stream.set_write_timeout(Some(CALL_TIMEOUT))?;
  Conn::handshake(stream, &secret, "desktop")
}

#[cfg(unix)]
pub fn connect() -> Result<(Conn<Sock>, Welcome), ConnError> {
  connect_in(&runner_dir()?)
}

#[cfg(not(unix))]
pub fn connect() -> Result<(Conn<Sock>, Welcome), ConnError> {
  Err(ConnError::Absent("Agent runner is not available on Windows yet".to_owned()))
}

#[cfg(all(test, unix))]
mod tests {
  use super::*;
  use std::fs;
  use std::os::unix::fs::{symlink, PermissionsExt};

  fn mode(p: &Path, m: u32) {
    fs::set_permissions(p, fs::Permissions::from_mode(m)).unwrap();
  }

  #[test]
  fn dir_resolution() {
    let p = |s: &str| Some(PathBuf::from(s));
    assert_eq!(resolve_dir(p("/x/run"), p("/h"), None, None).unwrap(), PathBuf::from("/x/run"));
    if cfg!(target_os = "macos") {
      assert_eq!(
        resolve_dir(None, p("/h"), p("/rt"), None).unwrap(),
        PathBuf::from("/h/Library/Application Support/dev.tmgr.cli/run")
      );
    } else {
      assert_eq!(resolve_dir(None, p("/h"), p("/rt"), None).unwrap(), PathBuf::from("/rt/tmgr"));
      assert_eq!(resolve_dir(None, p("/h"), None, p("/st")).unwrap(), PathBuf::from("/st/tmgr/run"));
      assert_eq!(resolve_dir(None, p("/h"), None, None).unwrap(), PathBuf::from("/h/.local/state/tmgr/run"));
    }
    assert!(resolve_dir(None, None, None, None).is_err() || !cfg!(target_os = "macos"));
  }

  #[test]
  fn private_dir_checks() {
    let t = tempfile::tempdir().unwrap();
    let d = t.path().join("run");
    fs::create_dir(&d).unwrap();
    mode(&d, 0o700);
    assert!(check_private_dir(&d).is_ok());
    mode(&d, 0o750);
    assert_eq!(check_private_dir(&d).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    mode(&d, 0o700);
    let link = t.path().join("link");
    symlink(&d, &link).unwrap();
    assert!(check_private_dir(&link).unwrap_err().to_string().contains("symlink"));
    let f = t.path().join("file");
    fs::write(&f, b"x").unwrap();
    assert!(check_private_dir(&f).is_err());
    assert_eq!(check_private_dir(&t.path().join("none")).unwrap_err().kind(), io::ErrorKind::NotFound);
  }

  #[test]
  fn secret_file_checks() {
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("client.secret");
    fs::write(&p, "abc\n").unwrap();
    mode(&p, 0o644);
    assert_eq!(read_secret(&p).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    mode(&p, 0o640);
    assert!(read_secret(&p).is_err());
    mode(&p, 0o600);
    assert_eq!(read_secret(&p).unwrap(), "abc");
    let l = t.path().join("l");
    symlink(&p, &l).unwrap();
    assert_eq!(read_secret(&l).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    assert!(read_secret(t.path()).is_err());
    assert_eq!(read_secret(&t.path().join("missing")).unwrap_err().kind(), io::ErrorKind::NotFound);
  }

  #[test]
  fn missing_runner_is_absent_and_loose_dir_is_not() {
    let t = tempfile::tempdir().unwrap();
    assert!(matches!(connect_in(&t.path().join("none")), Err(ConnError::Absent(_))));
    let d = t.path().join("run");
    fs::create_dir(&d).unwrap();
    mode(&d, 0o700);
    assert!(matches!(connect_in(&d), Err(ConnError::Absent(_))));
    fs::write(d.join("client.secret"), "s").unwrap();
    mode(&d.join("client.secret"), 0o600);
    assert!(matches!(connect_in(&d), Err(ConnError::Absent(_)) | Err(ConnError::Failed(_))));
    mode(&d, 0o755);
    assert!(matches!(connect_in(&d), Err(ConnError::Failed(_))));
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn connects_over_a_real_socket_in_a_temp_dir() {
    use crate::runner::mock::{self, Mock};
    let t = tempfile::tempdir().unwrap();
    let d = t.path().join("run");
    fs::create_dir(&d).unwrap();
    mode(&d, 0o700);
    fs::write(d.join("client.secret"), "topsecret\n").unwrap();
    mode(&d.join("client.secret"), 0o600);
    let listener = std::os::unix::net::UnixListener::bind(d.join("r.sock")).unwrap();
    let server = std::thread::spawn(move || {
      let (s, _) = listener.accept().unwrap();
      mock::spawn(s, Mock::new("topsecret")).join().unwrap()
    });
    let (mut conn, welcome) = connect_in(&d).unwrap();
    assert_eq!(welcome.caps, vec!["runs.create".to_owned()]);
    assert_eq!(conn.call("runner.status", serde_json::Value::Null).unwrap()["proto"], 1);
    drop(conn);
    assert_eq!(server.join().unwrap().hello["client"], "desktop");
  }
}
