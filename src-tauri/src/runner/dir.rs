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
const MAX_SECRET_FILE: u64 = 256;

#[cfg(unix)]
fn read_private_file(path: &Path, what: &str) -> io::Result<String> {
  use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
  let file = std::fs::OpenOptions::new()
    .read(true)
    .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
    .open(path)
    .map_err(|e| if e.raw_os_error() == Some(libc::ELOOP) { denied(format!("{what} is a symlink")) } else { e })?;
  let meta = file.metadata()?;
  if !meta.file_type().is_file() {
    return Err(denied(format!("{what} is not a regular file")));
  }
  if meta.uid() != euid() {
    return Err(denied(format!("{what} is owned by another user")));
  }
  if meta.mode() & 0o077 != 0 {
    return Err(denied(format!("{what} must not be accessible by group or others")));
  }
  if meta.len() == 0 || meta.len() > MAX_SECRET_FILE {
    return Err(denied(format!("{what} has an unexpected size")));
  }
  let mut buf = Vec::new();
  io::Read::read_to_end(&mut io::Read::take(file, MAX_SECRET_FILE + 1), &mut buf)?;
  if buf.len() as u64 > MAX_SECRET_FILE {
    return Err(denied(format!("{what} has an unexpected size")));
  }
  String::from_utf8(buf).map(|s| s.trim().to_owned()).map_err(|_| denied(format!("{what} is malformed")))
}

#[cfg(unix)]
pub fn read_secret(path: &Path) -> io::Result<String> {
  let secret = read_private_file(path, "runner secret")?;
  if secret.len() == 64 && secret.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')) {
    Ok(secret)
  } else {
    Err(denied("runner secret is malformed".to_owned()))
  }
}

#[cfg(unix)]
fn valid_addr(addr: &str) -> bool {
  let Some(rest) = addr.strip_prefix("tmgr-") else { return false };
  let Some((uid, rand)) = rest.split_once('-') else { return false };
  !uid.is_empty()
    && uid.len() <= 10
    && uid.bytes().all(|c| c.is_ascii_digit())
    && rand.len() == 16
    && rand.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(unix)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn read_addr(path: &Path) -> io::Result<String> {
  let addr = read_private_file(path, "runner address")?;
  if valid_addr(&addr) {
    Ok(addr)
  } else {
    Err(denied("runner address is malformed".to_owned()))
  }
}

#[cfg(unix)]
pub fn check_ancestors(dir: &Path, home: Option<&Path>) -> io::Result<()> {
  use std::path::Component;
  if !dir.is_absolute() || dir.components().any(|c| matches!(c, Component::ParentDir | Component::CurDir)) {
    return Err(denied("runner directory path must be absolute and normalized".to_owned()));
  }
  let (mut cur, rest) = match home.filter(|h| h.is_absolute() && dir.starts_with(h)) {
    Some(h) => (h.to_path_buf(), dir.strip_prefix(h).unwrap_or(dir)),
    None => (PathBuf::from("/"), dir.strip_prefix("/").unwrap_or(dir)),
  };
  for part in rest.components() {
    cur.push(part);
    if cur != dir && std::fs::symlink_metadata(&cur)?.file_type().is_symlink() {
      return Err(denied("a parent of the runner directory is a symlink".to_owned()));
    }
  }
  Ok(())
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
    let name = read_addr(&dir.join("runner.addr"))?;
    let addr = std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes())?;
    timed(UnixStream::connect_addr(&addr)?)
  }
  #[cfg(not(target_os = "linux"))]
  {
    let path = dir.join("r.sock");
    if path.as_os_str().len() > MAX_SUN_PATH {
      return Err(io::Error::new(io::ErrorKind::InvalidInput, "runner directory path is too long for a unix socket"));
    }
    timed(UnixStream::connect(path)?)
  }
}

#[cfg(unix)]
fn timed(stream: std::os::unix::net::UnixStream) -> io::Result<std::os::unix::net::UnixStream> {
  stream.set_read_timeout(Some(CALL_TIMEOUT))?;
  stream.set_write_timeout(Some(CALL_TIMEOUT))?;
  Ok(stream)
}

#[cfg(unix)]
pub fn connect_in(dir: &Path, home: Option<&Path>) -> Result<(Conn<Sock>, Welcome), ConnError> {
  check_private_dir(dir)?;
  check_ancestors(dir, home)?;
  let secret = read_secret(&dir.join("client.secret"))?;
  let stream = open_stream(dir)?;
  if peer_uid(&stream)? != euid() {
    return Err(ConnError::Failed("runner socket belongs to another user".to_owned()));
  }
  Conn::handshake(stream, &secret, "desktop")
}

#[cfg(unix)]
pub fn connect() -> Result<(Conn<Sock>, Welcome), ConnError> {
  connect_in(&runner_dir()?, env_path("HOME").as_deref())
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

  const SECRET: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

  fn tmp() -> (tempfile::TempDir, PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let p = t.path().canonicalize().unwrap();
    (t, p)
  }

  fn private_file(p: &Path, content: &str) {
    fs::write(p, content).unwrap();
    mode(p, 0o600);
  }

  #[test]
  fn private_dir_checks() {
    let (_t, root) = tmp();
    let d = root.join("run");
    fs::create_dir(&d).unwrap();
    mode(&d, 0o700);
    assert!(check_private_dir(&d).is_ok());
    mode(&d, 0o750);
    assert_eq!(check_private_dir(&d).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    mode(&d, 0o700);
    let link = root.join("link");
    symlink(&d, &link).unwrap();
    assert!(check_private_dir(&link).unwrap_err().to_string().contains("symlink"));
    let f = root.join("file");
    fs::write(&f, b"x").unwrap();
    assert!(check_private_dir(&f).is_err());
    assert_eq!(check_private_dir(&root.join("none")).unwrap_err().kind(), io::ErrorKind::NotFound);
  }

  #[test]
  fn symlinked_ancestor_is_rejected() {
    let (_t, root) = tmp();
    let real = root.join("real");
    fs::create_dir_all(real.join("run")).unwrap();
    symlink(&real, root.join("alias")).unwrap();
    let via = root.join("alias/run");
    assert!(check_ancestors(&real.join("run"), Some(&root)).is_ok());
    assert!(check_ancestors(&real.join("run"), None).is_ok());
    assert!(check_ancestors(&via, Some(&root)).unwrap_err().to_string().contains("symlink"));
    assert!(check_ancestors(&via, None).is_err());
    assert!(check_ancestors(Path::new("relative/run"), None).is_err());
    assert!(check_ancestors(&root.join("real/../real/run"), Some(&root)).is_err());
  }

  #[test]
  fn secret_file_checks() {
    let (_t, root) = tmp();
    let p = root.join("client.secret");
    fs::write(&p, format!("{SECRET}\n")).unwrap();
    mode(&p, 0o644);
    assert_eq!(read_secret(&p).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    mode(&p, 0o640);
    assert!(read_secret(&p).is_err());
    mode(&p, 0o600);
    assert_eq!(read_secret(&p).unwrap(), SECRET);
    let l = root.join("l");
    symlink(&p, &l).unwrap();
    assert_eq!(read_secret(&l).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    assert!(read_secret(&root).is_err());
    assert_eq!(read_secret(&root.join("missing")).unwrap_err().kind(), io::ErrorKind::NotFound);
  }

  #[test]
  fn oversized_empty_and_non_hex_secrets_are_rejected() {
    let (_t, root) = tmp();
    let p = root.join("client.secret");
    for bad in [String::new(), "\n".into(), "abc".into(), "a".repeat(300), "A".repeat(64), "g".repeat(64), format!("{SECRET}ff"), "z".repeat(10_000_000)] {
      private_file(&p, &bad);
      assert!(read_secret(&p).is_err(), "{}", bad.len());
    }
  }

  #[test]
  fn fifo_is_rejected_without_blocking() {
    let (_t, root) = tmp();
    let p = root.join("client.secret");
    let c = std::ffi::CString::new(p.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
    assert_eq!(read_secret(&p).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    assert!(read_addr(&p).is_err());
  }

  #[test]
  fn addr_shape_is_validated() {
    let (_t, root) = tmp();
    let p = root.join("runner.addr");
    private_file(&p, "tmgr-501-0123456789abcdef\n");
    assert_eq!(read_addr(&p).unwrap(), "tmgr-501-0123456789abcdef");
    for bad in ["", "tmgr-501-xyz", "evil-501-0123456789abcdef", "tmgr--0123456789abcdef", "tmgr-501-0123456789abcdeF", "tmgr-5x1-0123456789abcdef", "tmgr-501-0123456789abcdef-1"] {
      private_file(&p, bad);
      assert!(read_addr(&p).is_err(), "{bad}");
    }
  }

  #[test]
  fn missing_runner_is_absent_and_loose_dir_is_not() {
    let (_t, root) = tmp();
    assert!(matches!(connect_in(&root.join("none"), None), Err(ConnError::Absent(_))));
    let d = root.join("run");
    fs::create_dir(&d).unwrap();
    mode(&d, 0o700);
    assert!(matches!(connect_in(&d, None), Err(ConnError::Absent(_))));
    private_file(&d.join("client.secret"), SECRET);
    assert!(matches!(connect_in(&d, None), Err(ConnError::Absent(_)) | Err(ConnError::Failed(_))));
    mode(&d, 0o755);
    assert!(matches!(connect_in(&d, None), Err(ConnError::Failed(_))));
    mode(&d, 0o700);
    symlink(&root, root.join("alias")).unwrap();
    assert!(matches!(connect_in(&root.join("alias/run"), None), Err(ConnError::Failed(_))));
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn connects_over_a_real_socket_in_a_temp_dir() {
    use crate::runner::mock::{self, Mock};
    let (_t, root) = tmp();
    let d = root.join("run");
    fs::create_dir(&d).unwrap();
    mode(&d, 0o700);
    private_file(&d.join("client.secret"), SECRET);
    let listener = std::os::unix::net::UnixListener::bind(d.join("r.sock")).unwrap();
    let server = std::thread::spawn(move || {
      let (s, _) = listener.accept().unwrap();
      mock::spawn(s, Mock::new(SECRET)).join().unwrap()
    });
    let (mut conn, welcome) = connect_in(&d, None).unwrap();
    assert_eq!(welcome.caps, vec!["runs.create".to_owned()]);
    assert_eq!(conn.call("runner.status", serde_json::Value::Null).unwrap()["proto"], 1);
    drop(conn);
    assert_eq!(server.join().unwrap().hello["client"], "desktop");
  }
}
