use crate::dirs::{check_private_dir, ensure_private_dir};
use crate::peer::peer_cred;
use crate::proto::valid_session_id;
#[cfg(target_os = "linux")]
use crate::secret;
use std::fs;
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

const MAX_SUN_PATH: usize = 103;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    Path(PathBuf),
    Abstract(String),
}

fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, msg.into())
}

fn check_path_len(p: &Path) -> io::Result<()> {
    let n = p.as_os_str().len();
    if n > MAX_SUN_PATH {
        return Err(invalid(format!(
            "runner dir path is too long for a unix socket ({n} > {MAX_SUN_PATH} bytes): {}; set TMGR_RUNNER_DIR to a shorter path",
            p.display()
        )));
    }
    Ok(())
}

fn check_id(id: &str) -> io::Result<()> {
    if valid_session_id(id) {
        Ok(())
    } else {
        Err(invalid("invalid session id"))
    }
}

#[cfg(target_os = "linux")]
fn new_abstract(addr_file: &Path) -> io::Result<Endpoint> {
    let mut r = [0u8; 8];
    getrandom::fill(&mut r).map_err(io::Error::other)?;
    let name = format!("tmgr-{}-{}", crate::euid(), secret::hex(&r));
    let tmp = addr_file.with_extension(format!("tmp{}", secret::hex(&r)));
    secret::write_private(&tmp, &name)?;
    if let Err(e) = fs::rename(&tmp, addr_file) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(Endpoint::Abstract(name))
}

impl Endpoint {
    pub fn new_runner(dir: &Path) -> io::Result<Endpoint> {
        #[cfg(target_os = "linux")]
        {
            ensure_private_dir(dir)?;
            new_abstract(&dir.join("runner.addr"))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let p = dir.join("r.sock");
            check_path_len(&p)?;
            ensure_private_dir(dir)?;
            Ok(Endpoint::Path(p))
        }
    }

    pub fn load_runner(dir: &Path) -> io::Result<Endpoint> {
        #[cfg(target_os = "linux")]
        {
            secret::read(&dir.join("runner.addr")).map(Endpoint::Abstract)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let p = dir.join("r.sock");
            check_path_len(&p)?;
            Ok(Endpoint::Path(p))
        }
    }

    pub fn new_session(dir: &Path, id: &str) -> io::Result<Endpoint> {
        check_id(id)?;
        let sdir = dir.join("s");
        #[cfg(target_os = "linux")]
        {
            ensure_private_dir(dir)?;
            ensure_private_dir(&sdir)?;
            new_abstract(&sdir.join(format!("{id}.addr")))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let p = sdir.join(format!("{id}.sock"));
            check_path_len(&p)?;
            ensure_private_dir(dir)?;
            ensure_private_dir(&sdir)?;
            Ok(Endpoint::Path(p))
        }
    }

    pub fn load_session(dir: &Path, id: &str) -> io::Result<Endpoint> {
        check_id(id)?;
        let sdir = dir.join("s");
        #[cfg(target_os = "linux")]
        {
            secret::read(&sdir.join(format!("{id}.addr"))).map(Endpoint::Abstract)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let p = sdir.join(format!("{id}.sock"));
            check_path_len(&p)?;
            Ok(Endpoint::Path(p))
        }
    }
}

fn bind_path(p: &Path) -> io::Result<UnixListener> {
    check_path_len(p)?;
    match fs::symlink_metadata(p) {
        Ok(m) => {
            if !m.file_type().is_socket() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("{} exists and is not a socket", p.display()),
                ));
            }
            if m.uid() != crate::euid() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("{} is owned by another user", p.display()),
                ));
            }
            match UnixStream::connect(p) {
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        format!("{} is served by a live process", p.display()),
                    ))
                }
                Err(e)
                    if e.kind() == io::ErrorKind::ConnectionRefused
                        || e.kind() == io::ErrorKind::NotFound =>
                {
                    match fs::remove_file(p) {
                        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                        _ => {}
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let l = UnixListener::bind(p)?;
    fs::set_permissions(p, fs::Permissions::from_mode(0o600))?;
    Ok(l)
}

pub fn bind(ep: &Endpoint) -> io::Result<UnixListener> {
    match ep {
        Endpoint::Path(p) => bind_path(p),
        #[cfg(target_os = "linux")]
        Endpoint::Abstract(name) => {
            use std::os::linux::net::SocketAddrExt;
            let a = std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes())?;
            UnixListener::bind_addr(&a)
        }
        #[cfg(not(target_os = "linux"))]
        Endpoint::Abstract(_) => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "abstract sockets are only available on Linux",
        )),
    }
}

pub fn connect(ep: &Endpoint) -> io::Result<UnixStream> {
    let stream = match ep {
        Endpoint::Path(p) => {
            check_path_len(p)?;
            if let Some(parent) = p.parent() {
                check_private_dir(parent)?;
            }
            UnixStream::connect(p)?
        }
        #[cfg(target_os = "linux")]
        Endpoint::Abstract(name) => {
            use std::os::linux::net::SocketAddrExt;
            let a = std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes())?;
            UnixStream::connect_addr(&a)?
        }
        #[cfg(not(target_os = "linux"))]
        Endpoint::Abstract(_) => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "abstract sockets are only available on Linux",
            ))
        }
    };
    if peer_cred(&stream)?.uid != crate::euid() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "server socket belongs to another user",
        ));
    }
    Ok(stream)
}
