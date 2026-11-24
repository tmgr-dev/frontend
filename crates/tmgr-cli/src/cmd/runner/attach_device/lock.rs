use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub const FILE: &str = "attach.lock";

pub struct InstanceLock(#[allow(dead_code)] File);

pub fn acquire(runner_dir: &Path) -> Result<Option<InstanceLock>, String> {
    let path = runner_dir.join(super::DIR).join(FILE);
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("cannot open the attach device lock: {}", e.kind()))?;
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc == 0 {
        return Ok(Some(InstanceLock(file)));
    }
    let err = std::io::Error::last_os_error();
    if err.kind() == std::io::ErrorKind::WouldBlock {
        Ok(None)
    } else {
        Err(format!("cannot lock the attach device: {}", err.kind()))
    }
}
