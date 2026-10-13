use crate::store::Paths;
use std::path::PathBuf;
use tmgr_core::config::ensure_private_dir;
use tmgr_core::error::Result;

pub fn path(paths: &Paths) -> PathBuf {
    paths.link_dir().join("links.lock")
}

pub struct Shared(#[allow(dead_code)] std::fs::File);

pub fn shared(paths: &Paths) -> Result<Shared> {
    use std::os::unix::fs::OpenOptionsExt;
    ensure_private_dir(&paths.config)?;
    ensure_private_dir(&paths.link_dir())?;
    let f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path(paths))?;
    f.lock_shared()?;
    Ok(Shared(f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn shared_holders_coexist_and_trust_changes_wait_for_them() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::under(d.path());
        let a = shared(&paths).unwrap();
        let b = shared(&paths).unwrap();
        let (tx, rx) = mpsc::channel();
        let p2 = paths.clone();
        let writer = std::thread::spawn(move || {
            Store::update(&p2, |_| Ok(())).unwrap();
            tx.send(()).unwrap();
        });
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
        drop(a);
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
        drop(b);
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        writer.join().unwrap();
        let _again = shared(&paths).unwrap();
    }

    #[test]
    fn the_fence_file_is_private_and_where_the_runner_looks() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::under(d.path());
        let _s = shared(&paths).unwrap();
        assert_eq!(path(&paths), paths.config.join("link").join("links.lock"));
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path(&paths)), 0o600);
        assert_eq!(mode(&paths.link_dir()), 0o700);
    }
}
