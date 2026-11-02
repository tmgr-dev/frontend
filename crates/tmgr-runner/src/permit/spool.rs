use super::{parse_spool, HookEvent};
use crate::login::openat_nofollow;
use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

const FILE_CAP: u64 = (2 << 20) + 4096;
const MAX_ENTRIES: usize = 256;

pub fn order(name: &str) -> Option<(u64, u64)> {
    let (at, rest) = name.split_once('-')?;
    let pid = rest.strip_suffix(".json")?;
    let digits = |s: &str| !s.is_empty() && s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(at) || !digits(pid) {
        return None;
    }
    Some((at.parse().ok()?, pid.parse().ok()?))
}

fn open_dir(home: &Path, session: &str) -> std::io::Result<File> {
    let mut cur = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(home)?;
    for name in [".cache", "tmgr-permit", session] {
        cur = openat_nofollow(
            cur.as_raw_fd(),
            OsStr::new(name),
            libc::O_RDONLY | libc::O_DIRECTORY,
        )?;
    }
    Ok(cur)
}

fn read_entry(dir: &File, name: &str) -> Option<HookEvent> {
    let f = openat_nofollow(
        dir.as_raw_fd(),
        OsStr::new(name),
        libc::O_RDONLY | libc::O_NONBLOCK,
    )
    .ok()?;
    let meta = f.metadata().ok()?;
    if !meta.is_file() || meta.len() > FILE_CAP {
        return None;
    }
    let mut bytes = Vec::new();
    f.take(FILE_CAP + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > FILE_CAP {
        return None;
    }
    let ev = parse_spool(name, &bytes)?;
    (order(name)?.0 == ev.at_ms).then_some(ev)
}

/// The newest well-named entry only; if it cannot be read there is no hook, never an older one.
pub fn newest(home: &Path, session: &str) -> Option<HookEvent> {
    let dir = open_dir(home, session).ok()?;
    let mut best: Option<((u64, u64), String)> = None;
    for (seen, e) in std::fs::read_dir(crate::permit_hook::spool_dir(home, session))
        .ok()?
        .enumerate()
    {
        if seen >= MAX_ENTRIES {
            return None;
        }
        let Ok(name) = e.ok()?.file_name().into_string() else {
            continue;
        };
        let Some(key) = order(&name) else { continue };
        if best.as_ref().is_none_or(|(k, _)| key > *k) {
            best = Some((key, name));
        }
    }
    read_entry(&dir, &best?.1)
}

pub fn remove(home: &Path, session: &str, name: &str) {
    if order(name).is_none() {
        return;
    }
    if let (Ok(dir), Ok(c)) = (
        open_dir(home, session),
        std::ffi::CString::new(name.as_bytes()),
    ) {
        unsafe { libc::unlinkat(dir.as_raw_fd(), c.as_ptr(), 0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permit_hook::spool_dir;
    use std::fs;
    use std::os::unix::fs::symlink;

    const SID: &str = "s_abc";

    fn home() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("sp")
            .tempdir_in("/tmp")
            .unwrap()
    }

    fn put(home: &Path, at: u64, pid: u32, tool: &str) {
        let body = format!(r#"{{"tool_name":"{tool}"}}"#);
        crate::permit_hook::spool(home, SID, "claude", &mut body.as_bytes(), at, pid).unwrap();
    }

    #[test]
    fn names_are_strict_digits() {
        assert_eq!(order("12-3.json"), Some((12, 3)));
        for bad in [
            "+1-2.json",
            "1-2",
            "1-2.json.tmp",
            "-2.json",
            "1-.json",
            "a-b.json",
            "1-2-3.json",
        ] {
            assert_eq!(order(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_newest_entry_by_number_wins_not_by_text() {
        let h = home();
        put(h.path(), 9, 1, "Old");
        put(h.path(), 10, 1, "New");
        assert_eq!(newest(h.path(), SID).unwrap().tool, "New");
    }

    #[test]
    fn an_unreadable_newest_entry_gives_no_hook_instead_of_an_older_one() {
        let h = home();
        put(h.path(), 9, 1, "Old");
        let dir = spool_dir(h.path(), SID);
        fs::write(dir.join("10-1.json"), "not json").unwrap();
        assert!(newest(h.path(), SID).is_none());
    }

    #[test]
    fn symlinks_fifos_and_oversize_files_are_never_read() {
        let h = home();
        put(h.path(), 9, 1, "Old");
        let dir = spool_dir(h.path(), SID);
        let target = h.path().join("target.json");
        fs::copy(dir.join("9-1.json"), &target).unwrap();
        symlink(&target, dir.join("10-1.json")).unwrap();
        assert!(newest(h.path(), SID).is_none());
        fs::remove_file(dir.join("10-1.json")).unwrap();
        let fifo =
            std::ffi::CString::new(dir.join("11-1.json").as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert!(newest(h.path(), SID).is_none());
        fs::remove_file(dir.join("11-1.json")).unwrap();
        fs::write(dir.join("12-1.json"), vec![b' '; FILE_CAP as usize + 1]).unwrap();
        assert!(newest(h.path(), SID).is_none());
    }

    #[test]
    fn a_name_that_disagrees_with_the_recorded_time_is_refused() {
        let h = home();
        put(h.path(), 9, 1, "Old");
        let dir = spool_dir(h.path(), SID);
        fs::copy(dir.join("9-1.json"), dir.join("10-1.json")).unwrap();
        assert!(newest(h.path(), SID).is_none());
    }

    #[test]
    fn a_symlinked_spool_directory_is_not_followed() {
        let h = home();
        put(h.path(), 9, 1, "Old");
        let real = h.path().join(".cache/tmgr-permit");
        fs::rename(real.join(SID), real.join("elsewhere")).unwrap();
        symlink(real.join("elsewhere"), real.join(SID)).unwrap();
        assert!(newest(h.path(), SID).is_none());
    }

    #[test]
    fn remove_deletes_only_well_named_entries() {
        let h = home();
        put(h.path(), 9, 1, "Old");
        let dir = spool_dir(h.path(), SID);
        remove(h.path(), SID, "../9-1.json");
        assert!(dir.join("9-1.json").exists());
        remove(h.path(), SID, "9-1.json");
        assert!(!dir.join("9-1.json").exists());
    }
}
