use crate::bundle::{open_at, open_root};
use crate::clone::{entries, gone};
use crate::wire::write_atomic;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

const FILE: &str = ".object-exempt.json";
const MAX_FILE: u64 = 1 << 20;
const MAX_FOLDERS: usize = 1000;
const SHOWN: usize = 10;
const DIR: i32 = libc::O_RDONLY | libc::O_DIRECTORY;

static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExemptFolder {
    pub path: String,
    pub dev: u64,
    pub ino: u64,
    pub birth_ns: u64,
    pub fsid: u64,
    pub added_ms: u64,
    pub measured_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ident {
    pub dev: u64,
    pub ino: u64,
    pub birth_ns: u64,
    pub fsid: u64,
}

impl ExemptFolder {
    fn matches(&self, id: &Ident) -> bool {
        (self.dev, self.ino, self.fsid) == (id.dev, id.ino, id.fsid)
            && (self.birth_ns == 0 || self.birth_ns == id.birth_ns)
    }
}

fn ns(sec: i64, nsec: i64) -> u64 {
    u64::try_from(sec)
        .ok()
        .and_then(|s| s.checked_mul(1_000_000_000))
        .and_then(|n| n.checked_add(u64::try_from(nsec).ok()?))
        .unwrap_or(0)
}

#[cfg(target_os = "macos")]
#[allow(clippy::unnecessary_cast)]
fn birth(_fd: i32, st: &libc::stat) -> u64 {
    ns(st.st_birthtime as i64, st.st_birthtime_nsec as i64)
}

#[cfg(target_os = "linux")]
fn birth(fd: i32, _st: &libc::stat) -> u64 {
    let mut stx: libc::statx = unsafe { std::mem::zeroed() };
    let r = unsafe {
        libc::syscall(
            libc::SYS_statx,
            fd,
            c"".as_ptr(),
            libc::AT_EMPTY_PATH,
            libc::STATX_BTIME,
            &mut stx as *mut libc::statx,
        )
    };
    if r != 0 || stx.stx_mask & libc::STATX_BTIME == 0 {
        return 0;
    }
    ns(stx.stx_btime.tv_sec, i64::from(stx.stx_btime.tv_nsec))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn birth(_fd: i32, _st: &libc::stat) -> u64 {
    0
}

#[allow(clippy::unnecessary_cast)]
pub fn ident_fd(fd: i32) -> std::io::Result<Ident> {
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut sfs: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstatfs(fd, &mut sfs) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let [a, b]: [i32; 2] = unsafe { std::ptr::read((&raw const sfs.f_fsid).cast()) };
    Ok(Ident {
        dev: st.st_dev as u64,
        ino: st.st_ino as u64,
        birth_ns: birth(fd, &st),
        fsid: u64::from(a as u32) << 32 | u64::from(b as u32),
    })
}

fn ident_path(path: &Path) -> std::io::Result<Ident> {
    ident_fd(open_root(path)?.as_raw_fd())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measured {
    pub root: Ident,
    pub files_seen: u64,
    pub outside: Vec<String>,
    pub outside_count: u64,
    pub elapsed_ms: u64,
}

pub fn store(homes: &Path) -> PathBuf {
    homes.join(FILE)
}

pub fn list(homes: &Path) -> Vec<ExemptFolder> {
    crate::folders::read_capped(&store(homes), MAX_FILE)
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save(homes: &Path, all: &[ExemptFolder]) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(all).map_err(|e| e.to_string())?;
    write_atomic(&store(homes), &bytes).map_err(|e| format!("cannot save the folder list: {e}"))
}

pub fn add(
    homes: &Path,
    path: &Path,
    expect: &Ident,
    measured_ms: u64,
    now_ms: u64,
) -> Result<ExemptFolder, String> {
    if !path.is_absolute() {
        return Err("the folder must be an absolute path".into());
    }
    let canon = std::fs::canonicalize(path)
        .map_err(|_| format!("{} is not an existing folder", path.display()))?;
    let m = std::fs::metadata(&canon).map_err(|e| e.to_string())?;
    if !m.is_dir() {
        return Err(format!("{} is not a folder", canon.display()));
    }
    let id = ident_path(&canon).map_err(|e| format!("cannot read {}: {e}", canon.display()))?;
    if id != *expect {
        return Err("the folder changed since it was confirmed".into());
    }
    let folder = ExemptFolder {
        path: canon
            .to_str()
            .ok_or("the folder path is not valid UTF-8")?
            .to_string(),
        dev: id.dev,
        ino: id.ino,
        birth_ns: id.birth_ns,
        fsid: id.fsid,
        added_ms: now_ms,
        measured_ms,
    };
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut all = list(homes);
    all.retain(|f| f.path != folder.path);
    if all.len() >= MAX_FOLDERS {
        return Err(format!("at most {MAX_FOLDERS} folders"));
    }
    all.push(folder.clone());
    save(homes, &all)?;
    Ok(folder)
}

pub fn remove(homes: &Path, path: &str, target: Option<(u64, u64)>) -> Result<bool, String> {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let canon = std::fs::canonicalize(path)
        .ok()
        .and_then(|p| p.to_str().map(str::to_string));
    let mut all = list(homes);
    let before = all.len();
    all.retain(|f| {
        f.path != path && Some(&f.path) != canon.as_ref() && Some((f.dev, f.ino)) != target
    });
    if all.len() == before {
        return Ok(false);
    }
    save(homes, &all)?;
    Ok(true)
}

pub fn is_exempt(homes: &Path, canonical_path: &Path, id: &Ident) -> bool {
    let Some(path) = canonical_path.to_str() else {
        return false;
    };
    list(homes).iter().any(|f| f.path == path && f.matches(id))
}

pub fn is_stale(f: &ExemptFolder) -> bool {
    let path = Path::new(&f.path);
    std::fs::canonicalize(path).ok().as_deref() != Some(path)
        || ident_path(path).map_or(true, |id| !f.matches(&id))
}

const MAX_DEPTH: u32 = 256;
const PROGRESS_EVERY: u64 = 4096;

#[allow(clippy::unnecessary_cast)]
fn ident(st: &libc::stat) -> (u64, u64) {
    (st.st_dev as u64, st.st_ino as u64)
}

const CHANGED: &str = "the folder changed while it was checked, try again";

#[allow(clippy::unnecessary_cast)]
fn nlink(st: &libc::stat) -> u64 {
    st.st_nlink as u64
}

struct Walk<'a> {
    changed: bool,
    dirs: HashSet<(u64, u64)>,
    files: u64,
    links: HashMap<(u64, u64), (u64, u64, PathBuf)>,
    progress: &'a mut dyn FnMut(u64),
}

impl Walk<'_> {
    fn dir(&mut self, dir: File, path: &Path, depth: u32) -> std::io::Result<()> {
        if depth > MAX_DEPTH {
            return Err(std::io::Error::other(format!(
                "{} is nested too deeply",
                path.display()
            )));
        }
        let fd = dir.as_raw_fd();
        for (name, _) in entries(&dir)? {
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            if unsafe { libc::fstatat(fd, name.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) } != 0
            {
                let e = std::io::Error::last_os_error();
                if gone(&e) {
                    continue;
                }
                return Err(e);
            }
            let os_name = std::ffi::OsStr::from_bytes(name.to_bytes());
            let child = path.join(os_name);
            match st.st_mode & libc::S_IFMT {
                libc::S_IFDIR => {
                    if !self.dirs.insert(ident(&st)) {
                        self.changed = true;
                        return Err(std::io::Error::other(CHANGED));
                    }
                    let sub = match open_at(fd, os_name, DIR) {
                        Ok(sub) => sub,
                        Err(e) if gone(&e) => continue,
                        Err(e) => {
                            return Err(std::io::Error::new(
                                e.kind(),
                                format!("{}: {e}", child.display()),
                            ))
                        }
                    };
                    self.dir(sub, &child, depth + 1)?;
                }
                libc::S_IFREG => {
                    self.files += 1;
                    if self.files.is_multiple_of(PROGRESS_EVERY) {
                        (self.progress)(self.files);
                    }
                    if st.st_nlink > 1 {
                        let e = self
                            .links
                            .entry(ident(&st))
                            .or_insert((nlink(&st), 0, child));
                        e.1 += 1;
                        if e.1 > e.0 {
                            self.changed = true;
                            return Err(std::io::Error::other(CHANGED));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

pub fn measure(path: &Path, progress: &mut dyn FnMut(u64)) -> Result<Measured, String> {
    let started = Instant::now();
    let root = open_root(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let root_ident =
        ident_fd(root.as_raw_fd()).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let root_id = (root_ident.dev, root_ident.ino);
    let mut walk = Walk {
        changed: false,
        dirs: HashSet::from([root_id]),
        files: 0,
        links: HashMap::new(),
        progress,
    };
    if let Err(e) = walk.dir(root, path, 0) {
        if walk.changed {
            return Err(CHANGED.into());
        }
        return Err(format!("cannot read {}: {e}", path.display()));
    }
    (walk.progress)(walk.files);
    let mut outside: Vec<String> = walk
        .links
        .values()
        .filter(|(nlink, seen, _)| seen < nlink)
        .map(|(_, _, p)| p.display().to_string())
        .collect();
    outside.sort();
    let outside_count = outside.len() as u64;
    outside.truncate(SHOWN);
    Ok(Measured {
        root: root_ident,
        files_seen: walk.files,
        outside,
        outside_count,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    fn dirs() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let homes = t.path().join("homes");
        let work = t.path().join("work");
        std::fs::create_dir(&homes).unwrap();
        std::fs::create_dir(&work).unwrap();
        let work = std::fs::canonicalize(work).unwrap();
        (t, homes, work)
    }

    fn id(p: &Path) -> (u64, u64) {
        let m = std::fs::metadata(p).unwrap();
        (m.dev(), m.ino())
    }

    fn full(p: &Path) -> Ident {
        ident_path(p).unwrap()
    }

    fn allow(homes: &Path, p: &Path) -> Result<ExemptFolder, String> {
        let id = ident_path(&std::fs::canonicalize(p).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        add(homes, p, &id, 7, 5)
    }

    const NOBODY: Ident = Ident {
        dev: 0,
        ino: 0,
        birth_ns: 0,
        fsid: 0,
    };

    fn measured(p: &Path) -> Result<Measured, String> {
        measure(p, &mut |_| {})
    }

    #[test]
    fn add_stores_the_canonical_path_identity_and_measurement_privately() {
        let (_t, homes, work) = dirs();
        let f = allow(&homes, &work).unwrap();
        assert_eq!(f.path, work.to_str().unwrap());
        assert_eq!(
            ((f.dev, f.ino), f.added_ms, f.measured_ms),
            (id(&work), 5, 7)
        );
        let i = full(&work);
        assert_eq!((f.birth_ns, f.fsid), (i.birth_ns, i.fsid));
        assert_eq!(full(&work).fsid, i.fsid);
        assert_eq!(list(&homes), vec![f]);
        assert_eq!(
            std::fs::metadata(store(&homes))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        allow(&homes, &work).unwrap();
        assert_eq!(list(&homes).len(), 1);
    }

    #[test]
    fn a_symlinked_or_dotdot_path_resolves_to_the_real_folder() {
        let (t, homes, work) = dirs();
        let link = t.path().join("link");
        std::os::unix::fs::symlink(&work, &link).unwrap();
        assert_eq!(allow(&homes, &link).unwrap().path, work.to_str().unwrap());
        std::fs::create_dir(work.join("sub")).unwrap();
        let dotdot = work.join("sub").join("..");
        assert_eq!(allow(&homes, &dotdot).unwrap().path, work.to_str().unwrap());
        assert_eq!(list(&homes).len(), 1);
    }

    #[test]
    fn relative_missing_and_non_folder_paths_and_changed_identity_are_refused() {
        let (_t, homes, work) = dirs();
        assert!(add(&homes, Path::new("rel"), &NOBODY, 1, 1).is_err());
        assert!(add(&homes, &work.join("nope"), &NOBODY, 1, 1).is_err());
        std::fs::write(work.join("f"), "").unwrap();
        assert!(allow(&homes, &work.join("f")).is_err());
        assert!(add(&homes, &work, &NOBODY, 1, 1).is_err());
        assert!(list(&homes).is_empty());
    }

    #[test]
    fn add_is_refused_when_only_birth_time_or_filesystem_differs() {
        let (_t, homes, work) = dirs();
        let i = full(&work);
        let birth = Ident {
            birth_ns: i.birth_ns.wrapping_add(1),
            ..i
        };
        let fsid = Ident {
            fsid: i.fsid ^ 1,
            ..i
        };
        for other in [birth, fsid] {
            assert_eq!((other.dev, other.ino), (i.dev, i.ino));
            let e = add(&homes, &work, &other, 1, 1).unwrap_err();
            assert!(e.contains("changed"), "{e}");
        }
        assert!(list(&homes).is_empty());
        assert!(add(&homes, &work, &i, 1, 1).is_ok());
    }

    #[test]
    fn only_the_exact_folder_with_the_same_identity_is_exempt() {
        let (_t, homes, work) = dirs();
        let sub = work.join("sub");
        std::fs::create_dir(&sub).unwrap();
        let other = work.with_file_name("other");
        std::fs::create_dir(&other).unwrap();
        allow(&homes, &work).unwrap();
        assert!(is_exempt(&homes, &work, &full(&work)));
        assert!(!is_exempt(
            &homes,
            &work,
            &Ident {
                dev: 0,
                ino: 0,
                ..full(&work)
            }
        ));
        assert!(!is_exempt(&homes, &sub, &full(&sub)));
        let parent = work.parent().unwrap();
        assert!(!is_exempt(&homes, parent, &full(parent)));
        assert!(!is_exempt(&homes, &other, &full(&other)));
        assert!(!is_exempt(&homes, &other, &full(&work)));
    }

    #[test]
    fn a_replaced_folder_at_the_same_path_is_not_exempt_and_is_stale() {
        let (_t, homes, work) = dirs();
        let f = allow(&homes, &work).unwrap();
        assert!(!is_stale(&f));
        let old = full(&work);
        std::fs::rename(&work, work.with_extension("old")).unwrap();
        std::fs::create_dir(&work).unwrap();
        assert!(is_exempt(&homes, &work, &old));
        assert!(!is_exempt(&homes, &work, &full(&work)));
        assert!(is_stale(&f));
        std::fs::remove_dir(&work).unwrap();
        assert!(is_stale(&f));
    }

    fn write_record(homes: &Path, p: &Path, edit: impl Fn(&mut ExemptFolder)) -> ExemptFolder {
        let i = full(p);
        let mut f = ExemptFolder {
            path: p.to_str().unwrap().into(),
            dev: i.dev,
            ino: i.ino,
            birth_ns: i.birth_ns,
            fsid: i.fsid,
            added_ms: 0,
            measured_ms: 0,
        };
        edit(&mut f);
        std::fs::write(store(homes), serde_json::to_vec(&[&f]).unwrap()).unwrap();
        f
    }

    #[test]
    fn a_recorded_birth_time_or_filesystem_that_differs_does_not_match() {
        let (_t, homes, work) = dirs();
        let f = write_record(&homes, &work, |_| {});
        assert!(is_exempt(&homes, &work, &full(&work)));
        assert!(!is_stale(&f));
        for edit in [
            (|f: &mut ExemptFolder| f.birth_ns = f.birth_ns.wrapping_add(1))
                as fn(&mut ExemptFolder),
            |f| f.fsid ^= 1,
        ] {
            let f = write_record(&homes, &work, edit);
            assert!(!is_exempt(&homes, &work, &full(&work)));
            assert!(is_stale(&f));
        }
    }

    #[test]
    fn a_missing_birth_time_matches_only_on_one_side() {
        let (_t, homes, work) = dirs();
        let i = full(&work);
        let zero_now = Ident { birth_ns: 0, ..i };
        let f = write_record(&homes, &work, |f| f.birth_ns = 42);
        assert!(!f.matches(&zero_now));
        assert!(!f.matches(&Ident { birth_ns: 43, ..i }));
        assert!(f.matches(&Ident { birth_ns: 42, ..i }));
        let f = write_record(&homes, &work, |f| f.birth_ns = 0);
        assert!(f.matches(&zero_now));
        assert!(f.matches(&i));
    }

    #[test]
    fn remove_also_matches_the_target_identity_when_the_path_differs() {
        let (t, homes, work) = dirs();
        allow(&homes, &work).unwrap();
        let other = t.path().join("elsewhere");
        std::fs::create_dir(&other).unwrap();
        allow(&homes, &other).unwrap();
        let target = Some(id(&work));
        assert!(!remove(&homes, "/nowhere/raw", Some((0, 0))).unwrap());
        assert_eq!(list(&homes).len(), 2);
        assert!(remove(&homes, "/nowhere/raw", target).unwrap());
        let left = list(&homes);
        assert_eq!(left.len(), 1);
        assert_eq!(
            left[0].path,
            std::fs::canonicalize(&other).unwrap().to_str().unwrap()
        );
    }

    #[test]
    fn remove_works_by_path_even_when_the_folder_is_gone() {
        let (_t, homes, work) = dirs();
        allow(&homes, &work).unwrap();
        std::fs::remove_dir(&work).unwrap();
        assert!(remove(&homes, work.to_str().unwrap(), None).unwrap());
        assert!(!remove(&homes, work.to_str().unwrap(), None).unwrap());
        assert!(list(&homes).is_empty());
    }

    #[test]
    fn a_garbled_symlinked_linked_or_unknown_field_store_exempts_nothing() {
        let (_t, homes, work) = dirs();
        let good = |p: &Path| {
            let i = full(p);
            format!(
                r#"[{{"path":{:?},"dev":{},"ino":{},"birth_ns":{},"fsid":{},"added_ms":0,"measured_ms":0}}]"#,
                p.to_str().unwrap(),
                i.dev,
                i.ino,
                i.birth_ns,
                i.fsid
            )
        };
        let file = store(&homes);
        std::fs::write(&file, good(&work)).unwrap();
        assert!(is_exempt(&homes, &work, &full(&work)));

        std::fs::write(&file, "{ not json").unwrap();
        assert!(!is_exempt(&homes, &work, &full(&work)));

        std::fs::write(
            &file,
            good(&work).replace("\"measured_ms\":0", "\"measured_ms\":0,\"x\":1"),
        )
        .unwrap();
        assert!(!is_exempt(&homes, &work, &full(&work)));

        let real = work.join("real.json");
        std::fs::write(&real, good(&work)).unwrap();
        std::fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(&real, &file).unwrap();
        assert!(!is_exempt(&homes, &work, &full(&work)));

        std::fs::remove_file(&file).unwrap();
        std::fs::hard_link(&real, &file).unwrap();
        assert!(!is_exempt(&homes, &work, &full(&work)));
    }

    #[test]
    fn the_trusted_folder_store_never_creates_or_changes_the_exempt_store() {
        let (_t, homes, work) = dirs();
        let cwd = work.to_str().unwrap();
        crate::folders::learn(&homes, cwd, id(&work), 1).unwrap();
        crate::folders::add(&homes, &work, None, 2).unwrap();
        assert!(!store(&homes).exists());
        assert!(!is_exempt(&homes, &work, &full(&work)));
        allow(&homes, &work).unwrap();
        let before = std::fs::read(store(&homes)).unwrap();
        crate::folders::learn(&homes, cwd, id(&work), 3).unwrap();
        crate::folders::remove(&homes, cwd).unwrap();
        assert_eq!(std::fs::read(store(&homes)).unwrap(), before);
    }

    #[test]
    fn the_list_is_capped() {
        let (_t, homes, work) = dirs();
        let entries: Vec<ExemptFolder> = (0..MAX_FOLDERS)
            .map(|i| ExemptFolder {
                path: format!("/nowhere/{i}"),
                dev: 1,
                ino: i as u64,
                birth_ns: 0,
                fsid: 0,
                added_ms: 0,
                measured_ms: 0,
            })
            .collect();
        std::fs::write(store(&homes), serde_json::to_vec(&entries).unwrap()).unwrap();
        assert!(allow(&homes, &work).is_err());
        assert_eq!(list(&homes).len(), MAX_FOLDERS);
    }

    #[test]
    fn measure_finds_no_outside_links_when_every_link_is_inside() {
        let (_t, _homes, work) = dirs();
        std::fs::create_dir_all(work.join("a/b")).unwrap();
        std::fs::write(work.join("a/one"), "x").unwrap();
        std::fs::hard_link(work.join("a/one"), work.join("a/b/two")).unwrap();
        std::fs::hard_link(work.join("a/one"), work.join("three")).unwrap();
        std::fs::write(work.join("solo"), "y").unwrap();
        std::os::unix::fs::symlink("/etc/hosts", work.join("sym")).unwrap();
        let mut last = 0;
        let m = measure(&work, &mut |n| last = n).unwrap();
        assert_eq!(m.root, full(&work));
        assert_eq!((m.files_seen, m.outside_count), (4, 0));
        assert!(m.outside.is_empty());
        assert_eq!(last, 4);
    }

    #[test]
    fn measure_reports_files_linked_from_outside() {
        let (t, _homes, work) = dirs();
        let outside = t.path().join("elsewhere");
        std::fs::create_dir(&outside).unwrap();
        std::fs::create_dir(work.join("d")).unwrap();
        std::fs::write(work.join("d/shared"), "x").unwrap();
        std::fs::hard_link(work.join("d/shared"), outside.join("copy")).unwrap();
        std::fs::write(work.join("inside"), "y").unwrap();
        std::fs::hard_link(work.join("inside"), work.join("inside2")).unwrap();
        let m = measured(&work).unwrap();
        assert_eq!(m.outside_count, 1);
        assert_eq!(m.outside, vec![work.join("d/shared").display().to_string()]);
    }

    #[test]
    fn measure_shows_at_most_ten_outside_paths_but_counts_all() {
        let (t, _homes, work) = dirs();
        let outside = t.path().join("elsewhere");
        std::fs::create_dir(&outside).unwrap();
        for i in 0..13 {
            let f = work.join(format!("f{i}"));
            std::fs::write(&f, "x").unwrap();
            std::fs::hard_link(&f, outside.join(format!("l{i}"))).unwrap();
        }
        let m = measured(&work).unwrap();
        assert_eq!((m.outside_count, m.outside.len()), (13, 10));
    }

    #[test]
    fn measure_does_not_follow_symlinks_out_of_the_folder() {
        let (t, _homes, work) = dirs();
        let outside = t.path().join("elsewhere");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("a"), "x").unwrap();
        std::fs::hard_link(outside.join("a"), outside.join("b")).unwrap();
        std::os::unix::fs::symlink(&outside, work.join("out")).unwrap();
        let m = measured(&work).unwrap();
        assert_eq!((m.files_seen, m.outside_count), (0, 0));
    }

    #[test]
    fn measure_fails_closed_on_an_unreadable_subfolder() {
        let (_t, _homes, work) = dirs();
        let sub = work.join("locked");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("f"), "x").unwrap();
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o000)).unwrap();
        let res = measured(&work);
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(res.is_err());
    }

    #[test]
    fn measure_refuses_a_missing_or_relative_folder() {
        let (_t, _homes, work) = dirs();
        assert!(measured(&work.join("nope")).is_err());
        assert!(measured(Path::new("rel")).is_err());
    }
}
