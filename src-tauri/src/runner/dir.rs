#[cfg(not(unix))]
use std::io;
#[cfg(unix)]
use std::path::{Path, PathBuf};

use super::conn::{Conn, ConnError, Welcome};

#[cfg(unix)]
pub type Sock = std::os::unix::net::UnixStream;

#[cfg(not(unix))]
#[derive(Debug)]
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
  pub fn set_write_timeout(&self, _d: Option<std::time::Duration>) -> io::Result<()> {
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
fn env_path(name: &str) -> Option<PathBuf> {
  std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

#[cfg(unix)]
pub fn connect_in(dir: &Path, home: Option<&Path>) -> Result<(Conn, Welcome), ConnError> {
  use tmgr_ipc::{dirs, endpoint, secret};
  dirs::check_private_dir(dir)?;
  dirs::check_no_symlink_ancestors(dir, home)?;
  let secret = secret::read_secret(&dir.join("client.secret"))?;
  let stream = endpoint::connect(&endpoint::Endpoint::load_runner(dir)?)?;
  Conn::handshake(stream, &secret, "desktop")
}

#[cfg(unix)]
pub fn connect() -> Result<(Conn, Welcome), ConnError> {
  connect_in(&tmgr_ipc::dirs::runner_dir()?, env_path("HOME").as_deref())
}

#[cfg(not(unix))]
pub fn connect() -> Result<(Conn, Welcome), ConnError> {
  Err(ConnError::Absent("Agent runner is not available on Windows yet".to_owned()))
}

#[cfg(all(test, unix))]
mod tests {
  use super::*;
  use std::fs;
  use std::os::unix::fs::{symlink, PermissionsExt};

  const SECRET: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

  fn mode(p: &Path, m: u32) {
    fs::set_permissions(p, fs::Permissions::from_mode(m)).unwrap();
  }

  fn tmp() -> (tempfile::TempDir, PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let p = t.path().canonicalize().unwrap();
    (t, p)
  }

  fn private_file(p: &Path, content: &str) {
    fs::write(p, content).unwrap();
    mode(p, 0o600);
  }

  fn run_dir(root: &Path) -> PathBuf {
    let d = root.join("run");
    fs::create_dir(&d).unwrap();
    mode(&d, 0o700);
    d
  }

  #[test]
  fn missing_runner_is_absent_and_loose_dir_is_not() {
    let (_t, root) = tmp();
    assert!(matches!(connect_in(&root.join("none"), None), Err(ConnError::Absent(_))));
    let d = run_dir(&root);
    assert!(matches!(connect_in(&d, None), Err(ConnError::Absent(_))));
    private_file(&d.join("client.secret"), SECRET);
    assert!(matches!(connect_in(&d, None), Err(ConnError::Absent(_)) | Err(ConnError::Failed(_))));
    mode(&d, 0o755);
    assert!(matches!(connect_in(&d, None), Err(ConnError::Failed(_))));
    mode(&d, 0o700);
    symlink(&root, root.join("alias")).unwrap();
    assert!(matches!(connect_in(&root.join("alias/run"), None), Err(ConnError::Failed(_))));
  }

  #[test]
  fn unsafe_secret_files_are_refused_before_any_connection() {
    let (_t, root) = tmp();
    let d = run_dir(&root);
    let p = d.join("client.secret");
    for bad in ["", "abc", &"A".repeat(64), &"a".repeat(300)] {
      private_file(&p, bad);
      assert!(matches!(connect_in(&d, None), Err(ConnError::Failed(_))), "{}", bad.len());
    }
    private_file(&p, SECRET);
    mode(&p, 0o644);
    assert!(matches!(connect_in(&d, None), Err(ConnError::Failed(_))));
    fs::remove_file(&p).unwrap();
    symlink(root.join("elsewhere"), &p).unwrap();
    assert!(matches!(connect_in(&d, None), Err(ConnError::Failed(_))));
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn connects_over_a_real_socket_in_a_temp_dir() {
    use crate::runner::mock::{self, Mock};
    let (_t, root) = tmp();
    let d = run_dir(&root);
    private_file(&d.join("client.secret"), SECRET);
    let listener = std::os::unix::net::UnixListener::bind(d.join("r.sock")).unwrap();
    let server = std::thread::spawn(move || {
      let (s, _) = listener.accept().unwrap();
      mock::spawn(s, Mock::new(SECRET)).join().unwrap()
    });
    let (mut conn, welcome) = connect_in(&d, None).unwrap();
    assert!(welcome.can_create_runs());
    assert_eq!(conn.call("runner.status", serde_json::Value::Null).unwrap()["proto"], 1);
    drop(conn);
    assert_eq!(server.join().unwrap().hello["client"], "desktop");
  }
}
