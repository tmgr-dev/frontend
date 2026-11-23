use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};
use tmgr_chat_relay::engine::blob::{valid_sha, BlobPart, MAX_RELAY_BYTES, PART_BYTES};
use tmgr_ipc::dirs::ensure_private_dir;

pub const PARTIAL_QUOTA: u64 = 64 << 20;
pub const COMPLETE_QUOTA: u64 = 1 << 30;
pub const PARTIAL_TTL: Duration = Duration::from_secs(3600);
const SWEEP_EVERY: Duration = Duration::from_secs(60);
const META: &str = "meta";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlobError {
    Retryable(String),
    Rejected(String),
}

impl BlobError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, BlobError::Retryable(_))
    }
}

impl std::fmt::Display for BlobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlobError::Retryable(e) | BlobError::Rejected(e) => f.write_str(e),
        }
    }
}

impl From<io::Error> for BlobError {
    fn from(e: io::Error) -> Self {
        BlobError::Retryable(format!("blob store: {e}"))
    }
}

fn rejected(msg: &str) -> BlobError {
    BlobError::Rejected(msg.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PutOutcome {
    Partial { have: u32, parts: u32 },
    Complete(PathBuf),
}

pub struct BlobStore {
    root: PathBuf,
    max_size: u64,
    partial_quota: u64,
    complete_quota: u64,
    state: Mutex<Option<SystemTime>>,
}

impl BlobStore {
    pub fn open(runner_dir: &Path) -> io::Result<BlobStore> {
        let root = runner_dir.join("chat").join("blobs");
        ensure_private_dir(&root)?;
        ensure_private_dir(&root.join("partial"))?;
        Ok(BlobStore {
            root,
            max_size: MAX_RELAY_BYTES,
            partial_quota: PARTIAL_QUOTA,
            complete_quota: COMPLETE_QUOTA,
            state: Mutex::new(None),
        })
    }

    pub fn with_max_size(mut self, max_size: u64) -> Self {
        self.max_size = max_size;
        self
    }

    pub fn with_quotas(mut self, partial: u64, complete: u64) -> Self {
        self.partial_quota = partial;
        self.complete_quota = complete;
        self
    }

    pub fn max_size(&self) -> u64 {
        self.max_size
    }

    pub fn get(&self, sha: &str) -> Option<PathBuf> {
        if !valid_sha(sha) {
            return None;
        }
        let path = self.root.join(sha);
        fs::symlink_metadata(&path)
            .ok()
            .filter(|m| m.file_type().is_file())
            .map(|_| path)
    }

    pub fn put_part(&self, part: &BlobPart) -> Result<PutOutcome, BlobError> {
        part.check(self.max_size).map_err(rejected)?;
        let mut last = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let now = SystemTime::now();
        if last.is_none_or(|t| now.duration_since(t).is_ok_and(|d| d >= SWEEP_EVERY)) {
            *last = Some(now);
            self.sweep(now);
        }
        if let Some(path) = self.get(&part.sha256) {
            return Ok(PutOutcome::Complete(path));
        }
        let dir = self.partial_dir(&part.sha256)?;
        self.check_meta(&dir, part)?;
        let path = dir.join(part.part.to_string());
        match read_existing(&path)? {
            Some(old) if old == part.data => {}
            Some(_) => return Err(rejected("a stored part differs from the new one")),
            None => {
                if self.partial_bytes() + part.data.len() as u64 > self.partial_quota {
                    return Err(rejected("too many incomplete files are waiting"));
                }
                write_atomic(&dir, &part.part.to_string(), &part.data)?;
            }
        }
        let have = (0..part.parts)
            .filter(|i| fs::symlink_metadata(dir.join(i.to_string())).is_ok_and(|m| m.is_file()))
            .count() as u32;
        if have < part.parts {
            return Ok(PutOutcome::Partial {
                have,
                parts: part.parts,
            });
        }
        self.finish(&dir, part).map(PutOutcome::Complete)
    }

    pub fn put_bytes(&self, sha: &str, data: &[u8]) -> Result<PathBuf, BlobError> {
        if !valid_sha(sha) || data.is_empty() || data.len() as u64 > self.max_size {
            return Err(rejected("malformed file"));
        }
        if hex_sha(data) != sha {
            return Err(rejected("the file does not match its hash"));
        }
        if let Some(path) = self.get(sha) {
            return Ok(path);
        }
        self.insert(sha, data)
    }

    pub fn sweep(&self, now: SystemTime) {
        let partial = self.root.join("partial");
        for entry in read_dir(&partial) {
            if older_than(&entry.path(), now, PARTIAL_TTL) {
                remove_tree(&entry.path());
            }
        }
        for entry in read_dir(&self.root) {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("tmp-")
                && older_than(&entry.path(), now, PARTIAL_TTL)
            {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    fn partial_dir(&self, sha: &str) -> Result<PathBuf, BlobError> {
        let base = self.root.join("partial");
        ensure_private_dir(&base)?;
        let dir = base.join(sha);
        if let Ok(m) = fs::symlink_metadata(&dir) {
            if !m.file_type().is_dir() {
                fs::remove_file(&dir)?;
            }
        }
        ensure_private_dir(&dir)?;
        Ok(dir)
    }

    fn check_meta(&self, dir: &Path, part: &BlobPart) -> Result<(), BlobError> {
        let want = format!("{} {}", part.size, part.parts).into_bytes();
        match read_existing(&dir.join(META))? {
            Some(have) if have == want => Ok(()),
            Some(_) => Err(rejected("parts of one file disagree on its size")),
            None => Ok(write_atomic(dir, META, &want)?),
        }
    }

    fn finish(&self, dir: &Path, part: &BlobPart) -> Result<PathBuf, BlobError> {
        let mut all = Vec::with_capacity(part.size as usize);
        for i in 0..part.parts {
            match read_existing(&dir.join(i.to_string()))? {
                Some(bytes) => all.extend_from_slice(&bytes),
                None => return Err(BlobError::Retryable("a stored part went missing".into())),
            }
        }
        if all.len() as u64 != part.size || hex_sha(&all) != part.sha256 {
            remove_tree(dir);
            return Err(rejected("the file does not match its hash"));
        }
        let path = self.insert(&part.sha256, &all)?;
        remove_tree(dir);
        Ok(path)
    }

    fn insert(&self, sha: &str, data: &[u8]) -> Result<PathBuf, BlobError> {
        self.evict_for(data.len() as u64);
        write_atomic(&self.root, sha, data)?;
        Ok(self.root.join(sha))
    }

    fn evict_for(&self, incoming: u64) {
        let mut files: Vec<(SystemTime, u64, PathBuf)> = read_dir(&self.root)
            .filter(|e| valid_sha(&e.file_name().to_string_lossy()))
            .filter_map(|e| {
                let m = fs::symlink_metadata(e.path()).ok()?;
                m.file_type().is_file().then(|| {
                    (
                        m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                        m.len(),
                        e.path(),
                    )
                })
            })
            .collect();
        let mut total: u64 = files.iter().map(|f| f.1).sum();
        files.sort();
        for (_, len, path) in files {
            if total + incoming <= self.complete_quota {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total -= len;
            }
        }
    }

    fn partial_bytes(&self) -> u64 {
        read_dir(&self.root.join("partial"))
            .map(|d| {
                read_dir(&d.path())
                    .filter_map(|f| fs::symlink_metadata(f.path()).ok())
                    .filter(|m| m.file_type().is_file())
                    .map(|m| m.len())
                    .sum::<u64>()
            })
            .sum()
    }
}

fn hex_sha(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn read_dir(path: &Path) -> impl Iterator<Item = fs::DirEntry> {
    fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
}

fn older_than(path: &Path, now: SystemTime, ttl: Duration) -> bool {
    fs::symlink_metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| now.duration_since(t).ok())
        .is_some_and(|age| age > ttl)
}

fn remove_tree(dir: &Path) {
    if fs::symlink_metadata(dir).is_ok_and(|m| m.file_type().is_dir()) {
        for e in read_dir(dir) {
            let _ = fs::remove_file(e.path());
        }
        let _ = fs::remove_dir(dir);
    } else {
        let _ = fs::remove_file(dir);
    }
}

fn read_existing(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_file() => {
            let mut buf = Vec::with_capacity(m.len().min(PART_BYTES as u64 * 2) as usize);
            let mut f = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(path)?;
            (&mut f).take(PART_BYTES as u64 * 2).read_to_end(&mut buf)?;
            Ok(Some(buf))
        }
        Ok(_) => {
            fs::remove_file(path)?;
            Ok(None)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn write_atomic(dir: &Path, name: &str, data: &[u8]) -> io::Result<()> {
    let mut rnd = [0u8; 8];
    getrandom::fill(&mut rnd).map_err(|e| io::Error::other(e.to_string()))?;
    let tmp = dir.join(format!(
        "tmp-{}",
        rnd.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ));
    let write = || -> io::Result<()> {
        let mut f: File = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        fs::rename(&tmp, dir.join(name))
    };
    write().inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, MetadataExt};

    fn store(t: &tempfile::TempDir) -> BlobStore {
        BlobStore::open(t.path()).unwrap()
    }

    fn data(n: usize, seed: u8) -> Vec<u8> {
        (0..n)
            .map(|i| (i as u8).wrapping_mul(7).wrapping_add(seed))
            .collect()
    }

    fn parts_of(bytes: &[u8]) -> Vec<BlobPart> {
        let sha = hex_sha(bytes);
        let n = bytes.len().div_ceil(PART_BYTES) as u32;
        bytes
            .chunks(PART_BYTES)
            .enumerate()
            .map(|(i, c)| BlobPart {
                sha256: sha.clone(),
                size: bytes.len() as u64,
                part: i as u32,
                parts: n,
                data: c.to_vec(),
            })
            .collect()
    }

    fn mode(p: &Path) -> u32 {
        fs::metadata(p).unwrap().mode() & 0o777
    }

    #[test]
    fn a_single_part_file_is_stored_and_found() {
        let t = tempfile::tempdir().unwrap();
        let s = store(&t);
        let bytes = data(1000, 1);
        let p = &parts_of(&bytes)[0];
        let PutOutcome::Complete(path) = s.put_part(p).unwrap() else {
            panic!("not complete");
        };
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(s.get(&p.sha256), Some(path));
        assert!(!s.root.join("partial").join(&p.sha256).exists());
        assert_eq!(s.get(&"0".repeat(64)), None);
        assert_eq!(s.get("../etc"), None);
    }

    #[test]
    fn parts_arrive_out_of_order_and_duplicates_are_no_ops() {
        let t = tempfile::tempdir().unwrap();
        let s = store(&t);
        let bytes = data(PART_BYTES * 2 + 5, 2);
        let parts = parts_of(&bytes);
        assert_eq!(parts.len(), 3);
        assert_eq!(
            s.put_part(&parts[2]).unwrap(),
            PutOutcome::Partial { have: 1, parts: 3 }
        );
        assert_eq!(
            s.put_part(&parts[2]).unwrap(),
            PutOutcome::Partial { have: 1, parts: 3 }
        );
        assert_eq!(
            s.put_part(&parts[0]).unwrap(),
            PutOutcome::Partial { have: 2, parts: 3 }
        );
        let PutOutcome::Complete(path) = s.put_part(&parts[1]).unwrap() else {
            panic!("not complete");
        };
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(!s.root.join("partial").join(&parts[0].sha256).exists());
        assert!(matches!(
            s.put_part(&parts[1]).unwrap(),
            PutOutcome::Complete(_)
        ));
    }

    #[test]
    fn a_conflicting_part_is_rejected_and_not_retryable() {
        let t = tempfile::tempdir().unwrap();
        let s = store(&t);
        let bytes = data(PART_BYTES + 5, 3);
        let mut parts = parts_of(&bytes);
        s.put_part(&parts[0]).unwrap();
        parts[0].data[0] ^= 1;
        let e = s.put_part(&parts[0]).unwrap_err();
        assert!(!e.is_retryable());
        let mut other = parts_of(&bytes)[1].clone();
        other.size += 1;
        other.data.push(0);
        assert!(!s.put_part(&other).unwrap_err().is_retryable());
    }

    #[test]
    fn a_hash_mismatch_drops_the_partial_and_is_not_retryable() {
        let t = tempfile::tempdir().unwrap();
        let s = store(&t);
        let bytes = data(PART_BYTES + 5, 4);
        let mut parts = parts_of(&bytes);
        let sha = parts[0].sha256.clone();
        parts[1].data[0] ^= 1;
        s.put_part(&parts[0]).unwrap();
        let e = s.put_part(&parts[1]).unwrap_err();
        assert!(!e.is_retryable());
        assert!(!s.root.join("partial").join(&sha).exists());
        assert_eq!(s.get(&sha), None);
    }

    #[test]
    fn malformed_parts_are_rejected() {
        let t = tempfile::tempdir().unwrap();
        let s = store(&t);
        let mut p = parts_of(&data(10, 5))[0].clone();
        p.sha256 = "../x".into();
        assert!(!s.put_part(&p).unwrap_err().is_retryable());
        let big = BlobPart {
            sha256: "ab".repeat(32),
            size: MAX_RELAY_BYTES + 1,
            part: 0,
            parts: 9,
            data: vec![0; PART_BYTES],
        };
        assert!(s.put_part(&big).is_err());
        assert!(!s.root.join("partial").join("ab".repeat(32)).exists());
    }

    #[test]
    fn the_size_limit_is_a_parameter() {
        let t = tempfile::tempdir().unwrap();
        let s = store(&t).with_max_size(25 << 20);
        let bytes = data(PART_BYTES * 9 + 17, 6);
        let mut last = None;
        for p in parts_of(&bytes) {
            last = Some(s.put_part(&p).unwrap());
        }
        assert!(matches!(last, Some(PutOutcome::Complete(_))));
        let t2 = tempfile::tempdir().unwrap();
        let strict = store(&t2);
        assert!(strict.put_part(&parts_of(&bytes)[0]).is_err());
    }

    #[test]
    fn new_partials_beyond_the_quota_are_refused() {
        let t = tempfile::tempdir().unwrap();
        let s = store(&t).with_quotas(1_000_000, COMPLETE_QUOTA);
        let a = parts_of(&data(PART_BYTES + 5, 7));
        let b = parts_of(&data(PART_BYTES + 5, 8));
        s.put_part(&a[0]).unwrap();
        let e = s.put_part(&b[0]).unwrap_err();
        assert!(!e.is_retryable());
        assert!(matches!(s.put_part(&a[1]), Ok(PutOutcome::Complete(_))));
        assert!(s.put_part(&b[0]).is_ok());
    }

    #[test]
    fn sweep_removes_only_old_partials_and_stale_temp_files() {
        let t = tempfile::tempdir().unwrap();
        let s = store(&t);
        let parts = parts_of(&data(PART_BYTES + 5, 9));
        s.put_part(&parts[0]).unwrap();
        let dir = s.root.join("partial").join(&parts[0].sha256);
        let stale = s.root.join("tmp-0011");
        fs::write(&stale, b"x").unwrap();
        let now = SystemTime::now();
        s.sweep(now + Duration::from_secs(1800));
        assert!(dir.exists() && stale.exists());
        s.sweep(now + Duration::from_secs(3 * 3600));
        assert!(!dir.exists() && !stale.exists());
    }

    #[test]
    fn the_oldest_complete_blobs_are_evicted_first() {
        let t = tempfile::tempdir().unwrap();
        let s = store(&t).with_quotas(PARTIAL_QUOTA, 130);
        let items: Vec<(String, Vec<u8>)> = (0..3)
            .map(|i| (i, data(60, 20 + i)))
            .map(|(_, d)| (hex_sha(&d), d))
            .collect();
        let a = s.put_bytes(&items[0].0, &items[0].1).unwrap();
        let b = s.put_bytes(&items[1].0, &items[1].1).unwrap();
        let old = SystemTime::now() - Duration::from_secs(1000);
        OpenOptions::new()
            .write(true)
            .open(&a)
            .unwrap()
            .set_modified(old)
            .unwrap();
        let c = s.put_bytes(&items[2].0, &items[2].1).unwrap();
        assert!(!a.exists() && b.exists() && c.exists());
    }

    #[test]
    fn put_bytes_checks_the_hash_and_is_idempotent() {
        let t = tempfile::tempdir().unwrap();
        let s = store(&t);
        let d = data(500, 30);
        let sha = hex_sha(&d);
        assert!(s.put_bytes(&"0".repeat(64), &d).is_err());
        assert!(s.put_bytes(&sha, &[]).is_err());
        let p = s.put_bytes(&sha, &d).unwrap();
        assert_eq!(s.put_bytes(&sha, &d).unwrap(), p);
    }

    #[test]
    fn directories_are_private_and_files_are_owner_only() {
        let t = tempfile::tempdir().unwrap();
        let s = store(&t);
        let parts = parts_of(&data(PART_BYTES + 5, 31));
        s.put_part(&parts[0]).unwrap();
        let dir = s.root.join("partial").join(&parts[0].sha256);
        assert_eq!(mode(&s.root), 0o700);
        assert_eq!(mode(&s.root.join("partial")), 0o700);
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join("0")), 0o600);
        let PutOutcome::Complete(path) = s.put_part(&parts[1]).unwrap() else {
            panic!("not complete");
        };
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn a_symlink_planted_at_the_partial_dir_is_not_followed() {
        let t = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let s = store(&t);
        let parts = parts_of(&data(PART_BYTES + 5, 32));
        symlink(
            outside.path(),
            s.root.join("partial").join(&parts[0].sha256),
        )
        .unwrap();
        s.put_part(&parts[0]).unwrap();
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
        assert!(matches!(s.put_part(&parts[1]), Ok(PutOutcome::Complete(_))));
    }

    #[test]
    fn a_symlink_planted_at_a_part_or_the_final_name_is_not_followed() {
        let t = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("victim");
        fs::write(&target, b"keep").unwrap();
        let s = store(&t);
        let parts = parts_of(&data(PART_BYTES + 5, 33));
        let dir = s.root.join("partial").join(&parts[0].sha256);
        ensure_private_dir(&dir).unwrap();
        symlink(&target, dir.join("0")).unwrap();
        symlink(&target, s.root.join(&parts[0].sha256)).unwrap();
        assert_eq!(s.get(&parts[0].sha256), None);
        s.put_part(&parts[0]).unwrap();
        let PutOutcome::Complete(path) = s.put_part(&parts[1]).unwrap() else {
            panic!("not complete");
        };
        assert_eq!(fs::read(&target).unwrap(), b"keep");
        assert!(!fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::metadata(&path).unwrap().len(), PART_BYTES as u64 + 5);
    }
}
