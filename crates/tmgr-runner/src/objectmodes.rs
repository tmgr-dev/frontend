use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SNAPSHOT: &str = "objects-modes";
const DONE: &str = "objects-modes.done";
pub const RO_RECORD: &str = "objects_ro";
const LOGGED_CHANGES: usize = 20;

fn kind_of(m: &fs::Metadata) -> Option<u8> {
    if m.is_dir() {
        Some(b'd')
    } else if m.is_file() {
        Some(b'f')
    } else {
        None
    }
}

fn walk(root: &Path, rel: &Path, out: &mut impl Write) -> io::Result<()> {
    let path = root.join(rel);
    let m = fs::symlink_metadata(&path)?;
    let Some(kind) = kind_of(&m) else {
        return Ok(());
    };
    let rel_bytes = rel.as_os_str().as_bytes();
    if rel_bytes.contains(&b'\n') || rel_bytes.contains(&b'\t') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} has a control character in its name", path.display()),
        ));
    }
    out.write_all(&[kind])?;
    write!(out, "{:o}\t{}\t{}\t", m.mode() & 0o7777, m.dev(), m.ino())?;
    out.write_all(rel_bytes)?;
    out.write_all(b"\n")?;
    if kind == b'd' {
        let mut names = fs::read_dir(&path)?
            .map(|e| e.map(|e| e.file_name()))
            .collect::<io::Result<Vec<_>>>()?;
        names.sort();
        for n in names {
            walk(root, &rel.join(n), out)?;
        }
    }
    Ok(())
}

pub fn save(sdir: &Path, roots: &[PathBuf]) -> io::Result<()> {
    if roots.is_empty() {
        return Ok(());
    }
    let _ = fs::remove_file(sdir.join(DONE));
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(sdir.join(SNAPSHOT))?;
    let mut out = BufWriter::new(file);
    for root in roots {
        out.write_all(b"R")?;
        out.write_all(root.as_os_str().as_bytes())?;
        out.write_all(b"\n")?;
        walk(root, Path::new(""), &mut out)?;
    }
    out.flush()
}

struct Restorer {
    root: PathBuf,
    gone: Vec<PathBuf>,
    restored: usize,
    failed: usize,
    lines: Vec<String>,
}

impl Restorer {
    fn entry(&mut self, line: &[u8]) {
        let Some((head, rest)) = line.split_first() else {
            return;
        };
        let mut parts = rest.splitn(4, |b| *b == b'\t');
        let (Some(mode), Some(dev), Some(ino), Some(rel)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return;
        };
        let num = |b: &[u8]| {
            std::str::from_utf8(b)
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
        };
        let parsed = (
            std::str::from_utf8(mode)
                .ok()
                .and_then(|s| u32::from_str_radix(s, 8).ok()),
            num(dev),
            num(ino),
        );
        let (Some(mode), Some(dev), Some(ino)) = parsed else {
            return;
        };
        let rel = Path::new(OsStr::from_bytes(rel));
        if self.gone.iter().any(|g| rel.starts_with(g)) {
            return;
        }
        let path = self.root.join(rel);
        let leaf = match Leaf::open(&path) {
            Ok(l) => l,
            Err(e) if replaced(&e) => {
                self.gone.push(rel.to_path_buf());
                return;
            }
            Err(e) => {
                self.failed += 1;
                self.lines.push(format!(
                    "could not check the mode of {}: {e}",
                    path.display()
                ));
                return;
            }
        };
        let m = &leaf.meta;
        if kind_of(m) != Some(*head) || m.dev() != dev || m.ino() != ino {
            self.gone.push(rel.to_path_buf());
            return;
        }
        if m.mode() & 0o7777 == mode {
            return;
        }
        let was = m.mode() & 0o7777;
        match leaf.chmod(mode) {
            Ok(()) => {
                self.restored += 1;
                if self.restored <= LOGGED_CHANGES {
                    self.lines.push(format!(
                        "restored the mode of {} from {:o} to {:o}",
                        path.display(),
                        was,
                        mode
                    ));
                }
            }
            Err(e) => {
                self.failed += 1;
                self.lines.push(format!(
                    "could not restore the mode of {}: {e}",
                    path.display()
                ));
            }
        }
    }

    fn finish(mut self, out: &mut Vec<String>) -> bool {
        if self.restored > LOGGED_CHANGES {
            self.lines.push(format!(
                "{} more modes restored in {}",
                self.restored - LOGGED_CHANGES,
                self.root.display()
            ));
        }
        out.append(&mut self.lines);
        self.failed == 0
    }
}

fn replaced(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::NotFound
        || matches!(e.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR))
}

#[cfg(target_os = "linux")]
struct Leaf {
    fd: File,
    meta: fs::Metadata,
}

#[cfg(target_os = "linux")]
impl Leaf {
    fn open(path: &Path) -> io::Result<Self> {
        let fd = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        let meta = fd.metadata()?;
        Ok(Self { fd, meta })
    }

    fn chmod(&self, mode: u32) -> io::Result<()> {
        use std::os::fd::AsRawFd;
        fs::set_permissions(
            format!("/proc/self/fd/{}", self.fd.as_raw_fd()),
            fs::Permissions::from_mode(mode),
        )
    }
}

#[cfg(not(target_os = "linux"))]
struct Leaf {
    path: PathBuf,
    meta: fs::Metadata,
}

#[cfg(not(target_os = "linux"))]
impl Leaf {
    fn open(path: &Path) -> io::Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
            meta: fs::symlink_metadata(path)?,
        })
    }

    fn chmod(&self, mode: u32) -> io::Result<()> {
        fs::set_permissions(&self.path, fs::Permissions::from_mode(mode))
    }
}

static FINISH: Mutex<()> = Mutex::new(());

pub fn finish(sdir: &Path) -> Vec<String> {
    let _g = FINISH.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(file) = File::open(sdir.join(SNAPSHOT)) else {
        return Vec::new();
    };
    if sdir.join(DONE).exists() {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let Ok(note) = fs::read_to_string(sdir.join(RO_RECORD)) {
        out.push(if let Some(why) = note.strip_prefix("fallback: ") {
            format!(
                "granted object stores were not mounted read-only ({why}); modes are restored after the run"
            )
        } else if let Some(why) = note.strip_prefix("failed: ") {
            format!("the read-only mount failed ({why}); the run did not start")
        } else {
            "granted object stores were mounted read-only".to_string()
        });
    }
    let mut complete = true;
    let mut current: Option<Restorer> = None;
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                out.push(format!("object store modes are unreadable: {e}"));
                complete = false;
                break;
            }
        }
        if line.last() == Some(&b'\n') {
            line.pop();
        }
        if let Some(root) = line.strip_prefix(b"R") {
            if let Some(done) = current.take() {
                complete &= done.finish(&mut out);
            }
            current = Some(Restorer {
                root: PathBuf::from(OsStr::from_bytes(root)),
                gone: Vec::new(),
                restored: 0,
                failed: 0,
                lines: Vec::new(),
            });
        } else if let Some(r) = current.as_mut() {
            r.entry(&line);
        }
    }
    if let Some(done) = current.take() {
        complete &= done.finish(&mut out);
    }
    if complete {
        let _ = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(sdir.join(DONE));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn chmod(p: &Path, mode: u32) {
        fs::set_permissions(p, fs::Permissions::from_mode(mode)).unwrap();
    }

    fn mode(p: &Path) -> u32 {
        fs::symlink_metadata(p).unwrap().mode() & 0o7777
    }

    struct Fx {
        _t: tempfile::TempDir,
        sdir: PathBuf,
        root: PathBuf,
        outside: PathBuf,
    }

    fn fx() -> Fx {
        let t = tempfile::tempdir().unwrap();
        let base = t.path().canonicalize().unwrap();
        let (sdir, root, outside) = (base.join("s"), base.join("objects"), base.join("outside"));
        for d in [&sdir, &root.join("ab"), &root.join("pack")] {
            fs::create_dir_all(d).unwrap();
        }
        fs::write(root.join("ab/cdef"), "x").unwrap();
        fs::write(root.join("pack/p.idx"), "x").unwrap();
        fs::write(&outside, "o").unwrap();
        chmod(&root.join("ab/cdef"), 0o444);
        chmod(&root.join("pack/p.idx"), 0o444);
        chmod(&outside, 0o640);
        Fx {
            _t: t,
            sdir,
            root,
            outside,
        }
    }

    fn lock_all(root: &Path) {
        for p in ["ab/cdef", "pack/p.idx", "ab", "pack", ""] {
            chmod(&root.join(p), 0);
        }
    }

    #[test]
    fn modes_are_restored_top_down_after_the_root_was_locked() {
        let f = fx();
        let before = mode(&f.root);
        save(&f.sdir, std::slice::from_ref(&f.root)).unwrap();
        lock_all(&f.root);
        let lines = finish(&f.sdir);
        assert_eq!(mode(&f.root), before);
        assert_eq!(mode(&f.root.join("ab")), before);
        assert_eq!(mode(&f.root.join("ab/cdef")), 0o444);
        assert_eq!(mode(&f.root.join("pack/p.idx")), 0o444);
        assert_eq!(
            lines.iter().filter(|l| l.starts_with("restored")).count(),
            5
        );
    }

    #[test]
    fn a_second_finish_does_nothing() {
        let f = fx();
        save(&f.sdir, std::slice::from_ref(&f.root)).unwrap();
        lock_all(&f.root);
        assert!(!finish(&f.sdir).is_empty());
        chmod(&f.root, 0o755);
        chmod(&f.root.join("ab"), 0o700);
        assert!(finish(&f.sdir).is_empty());
        assert_eq!(mode(&f.root.join("ab")), 0o700);
    }

    #[test]
    fn a_symlink_is_not_followed() {
        let f = fx();
        symlink(&f.outside, f.root.join("link")).unwrap();
        fs::write(f.root.join("swap"), "x").unwrap();
        chmod(&f.root.join("swap"), 0o444);
        save(&f.sdir, std::slice::from_ref(&f.root)).unwrap();
        chmod(&f.outside, 0o600);
        fs::remove_file(f.root.join("swap")).unwrap();
        symlink(&f.outside, f.root.join("swap")).unwrap();
        finish(&f.sdir);
        assert_eq!(mode(&f.outside), 0o600);
    }

    #[test]
    fn new_entries_are_left_alone() {
        let f = fx();
        save(&f.sdir, std::slice::from_ref(&f.root)).unwrap();
        fs::write(f.root.join("ab/new"), "n").unwrap();
        chmod(&f.root.join("ab/new"), 0o600);
        fs::create_dir(f.root.join("fresh")).unwrap();
        chmod(&f.root.join("fresh"), 0o700);
        chmod(&f.root.join("ab/cdef"), 0o600);
        finish(&f.sdir);
        assert_eq!(mode(&f.root.join("ab/new")), 0o600);
        assert_eq!(mode(&f.root.join("fresh")), 0o700);
        assert_eq!(mode(&f.root.join("ab/cdef")), 0o444);
    }

    #[test]
    fn an_unreachable_store_is_retried_on_the_next_finish() {
        let f = fx();
        let parent = f.root.with_file_name("wrap");
        fs::create_dir(&parent).unwrap();
        let root = parent.join("objects");
        fs::rename(&f.root, &root).unwrap();
        save(&f.sdir, std::slice::from_ref(&root)).unwrap();
        chmod(&root.join("ab/cdef"), 0o600);
        let before = mode(&parent);
        chmod(&parent, 0o600);
        let lines = finish(&f.sdir);
        chmod(&parent, before);
        assert!(!f.sdir.join(DONE).exists(), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("could not")), "{lines:?}");
        finish(&f.sdir);
        assert_eq!(mode(&root.join("ab/cdef")), 0o444);
        assert!(f.sdir.join(DONE).exists());
    }

    #[test]
    fn a_missing_root_is_skipped() {
        let f = fx();
        save(&f.sdir, std::slice::from_ref(&f.root)).unwrap();
        fs::remove_dir_all(&f.root).unwrap();
        assert!(finish(&f.sdir).is_empty());
        assert!(f.sdir.join(DONE).exists());
    }
}
