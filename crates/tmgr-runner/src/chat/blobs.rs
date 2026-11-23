use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use sha2::{Digest, Sha256};
use tmgr_chat_relay::engine::blob::{
    valid_sha, BlobPart, MAX_LOCAL_BYTES, MAX_RELAY_BYTES, PART_BYTES,
};
use tmgr_ipc::dirs::ensure_private_dir;

pub const PARTIAL_QUOTA: u64 = 64 << 20;
pub const COMPLETE_QUOTA: u64 = 1 << 30;
pub const PARTIAL_TTL: Duration = Duration::from_secs(3600);
const SWEEP_EVERY: Duration = Duration::from_secs(60);
const META: &str = "meta";
const RETRY_TRIES: u32 = 5;
const RETRY_WINDOW: Duration = Duration::from_secs(600);
const RETRY_KEYS: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Relay,
    Local,
}

impl Source {
    fn dir(self) -> &'static str {
        match self {
            Source::Relay => "relay",
            Source::Local => "local",
        }
    }
}

type RetryKey = (String, String, u32);

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
        use io::ErrorKind as K;
        let transient = matches!(
            e.kind(),
            K::StorageFull
                | K::QuotaExceeded
                | K::Interrupted
                | K::WouldBlock
                | K::TimedOut
                | K::Other
        ) || matches!(
            e.raw_os_error(),
            Some(libc::ENOSPC | libc::EDQUOT | libc::EIO)
        );
        let msg = format!("blob store: {e}");
        if transient {
            BlobError::Retryable(msg)
        } else {
            BlobError::Rejected(msg)
        }
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
    partial_quota: u64,
    complete_quota: u64,
    state: Mutex<Option<SystemTime>>,
    retries: Mutex<HashMap<RetryKey, (u32, Instant)>>,
}

impl BlobStore {
    pub fn open(runner_dir: &Path) -> io::Result<BlobStore> {
        let root = runner_dir.join("chat").join("blobs");
        ensure_private_dir(&root)?;
        ensure_private_dir(&root.join("partial"))?;
        ensure_private_dir(&root.join("partial").join(Source::Relay.dir()))?;
        ensure_private_dir(&root.join("partial").join(Source::Local.dir()))?;
        Ok(BlobStore {
            root,
            partial_quota: PARTIAL_QUOTA,
            complete_quota: COMPLETE_QUOTA,
            state: Mutex::new(None),
            retries: Mutex::new(HashMap::new()),
        })
    }

    pub fn with_quotas(mut self, partial: u64, complete: u64) -> Self {
        self.partial_quota = partial;
        self.complete_quota = complete;
        self
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

    pub fn deliver_relay(&self, from_device: &str, part: &BlobPart) -> Result<(), String> {
        self.deliver_with(from_device, part, Instant::now(), || {
            self.put_part(part, Source::Relay, MAX_RELAY_BYTES)
        })
    }

    fn deliver_with(
        &self,
        from_device: &str,
        part: &BlobPart,
        now: Instant,
        put: impl FnOnce() -> Result<PutOutcome, BlobError>,
    ) -> Result<(), String> {
        let key = (from_device.to_owned(), part.sha256.clone(), part.part);
        let mut retries = self.retries.lock().unwrap_or_else(|e| e.into_inner());
        match put() {
            Ok(_) => {
                retries.remove(&key);
                Ok(())
            }
            Err(e) if !e.is_retryable() => {
                retries.remove(&key);
                crate::daemon::log(&format!("chat relay: file part refused: {e}"));
                Ok(())
            }
            Err(e) => {
                retries.retain(|k, (_, first)| {
                    *k == key || now.saturating_duration_since(*first) < RETRY_WINDOW
                });
                if retries.len() >= RETRY_KEYS {
                    retries.clear();
                }
                let entry = retries.entry(key.clone()).or_insert((0, now));
                entry.0 += 1;
                if entry.0 >= RETRY_TRIES || now.saturating_duration_since(entry.1) >= RETRY_WINDOW
                {
                    retries.remove(&key);
                    crate::daemon::log(&format!(
                        "chat relay: file part dropped after retries: {e}"
                    ));
                    Ok(())
                } else {
                    Err(e.to_string())
                }
            }
        }
    }

    pub fn put_part(
        &self,
        part: &BlobPart,
        source: Source,
        max_size: u64,
    ) -> Result<PutOutcome, BlobError> {
        part.check(max_size).map_err(rejected)?;
        let mut last = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let now = SystemTime::now();
        if last.is_none_or(|t| now.duration_since(t).is_ok_and(|d| d >= SWEEP_EVERY)) {
            *last = Some(now);
            self.sweep(now);
        }
        if let Some(path) = self.get(&part.sha256) {
            return Ok(PutOutcome::Complete(path));
        }
        let dir = self.partial_dir(source, &part.sha256)?;
        self.check_meta(&dir, part)?;
        let path = dir.join(part.part.to_string());
        match read_existing(&path)? {
            Some(old) if old == part.data => {}
            Some(_) => return Err(rejected("a stored part differs from the new one")),
            None => {
                self.make_room(source, &dir, part.data.len() as u64)?;
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
        if !valid_sha(sha) || data.is_empty() || data.len() as u64 > MAX_LOCAL_BYTES {
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
        for source in [Source::Relay, Source::Local] {
            for entry in read_dir(&self.root.join("partial").join(source.dir())) {
                if older_than(&entry.path(), now, PARTIAL_TTL) {
                    remove_tree(&entry.path());
                }
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

    fn partial_dir(&self, source: Source, sha: &str) -> Result<PathBuf, BlobError> {
        let base = self.root.join("partial").join(source.dir());
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

    fn make_room(&self, source: Source, keep: &Path, incoming: u64) -> Result<(), BlobError> {
        let base = self.root.join("partial").join(source.dir());
        let mut used: u64 = 0;
        let mut dirs: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
        for d in read_dir(&base) {
            let size: u64 = read_dir(&d.path())
                .filter_map(|f| fs::symlink_metadata(f.path()).ok())
                .filter(|m| m.file_type().is_file())
                .map(|m| m.len())
                .sum();
            used += size;
            if d.path() != keep {
                let at = fs::symlink_metadata(d.path())
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                dirs.push((at, size, d.path()));
            }
        }
        dirs.sort();
        for (_, size, path) in dirs {
            if used + incoming <= self.partial_quota {
                break;
            }
            remove_tree(&path);
            used -= size;
        }
        if used + incoming > self.partial_quota {
            return Err(rejected("the file is too big to wait for its other parts"));
        }
        Ok(())
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

    fn tmp() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        fs::set_permissions(
            t.path(),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .unwrap();
        t
    }

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

    fn rput(s: &BlobStore, p: &BlobPart) -> Result<PutOutcome, BlobError> {
        s.put_part(p, Source::Relay, MAX_RELAY_BYTES)
    }

    fn lput(s: &BlobStore, p: &BlobPart) -> Result<PutOutcome, BlobError> {
        s.put_part(p, Source::Local, MAX_LOCAL_BYTES)
    }

    fn mode(p: &Path) -> u32 {
        fs::metadata(p).unwrap().mode() & 0o777
    }

    #[test]
    fn a_single_part_file_is_stored_and_found() {
        let t = tmp();
        let s = store(&t);
        let bytes = data(1000, 1);
        let p = &parts_of(&bytes)[0];
        let PutOutcome::Complete(path) = rput(&s, p).unwrap() else {
            panic!("not complete");
        };
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(s.get(&p.sha256), Some(path));
        assert!(!s.root.join("partial/relay").join(&p.sha256).exists());
        assert_eq!(s.get(&"0".repeat(64)), None);
        assert_eq!(s.get("../etc"), None);
    }

    #[test]
    fn parts_arrive_out_of_order_and_duplicates_are_no_ops() {
        let t = tmp();
        let s = store(&t);
        let bytes = data(PART_BYTES * 2 + 5, 2);
        let parts = parts_of(&bytes);
        assert_eq!(parts.len(), 3);
        assert_eq!(
            rput(&s, &parts[2]).unwrap(),
            PutOutcome::Partial { have: 1, parts: 3 }
        );
        assert_eq!(
            rput(&s, &parts[2]).unwrap(),
            PutOutcome::Partial { have: 1, parts: 3 }
        );
        assert_eq!(
            rput(&s, &parts[0]).unwrap(),
            PutOutcome::Partial { have: 2, parts: 3 }
        );
        let PutOutcome::Complete(path) = rput(&s, &parts[1]).unwrap() else {
            panic!("not complete");
        };
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(!s.root.join("partial/relay").join(&parts[0].sha256).exists());
        assert!(matches!(
            rput(&s, &parts[1]).unwrap(),
            PutOutcome::Complete(_)
        ));
    }

    #[test]
    fn a_conflicting_part_is_rejected_and_not_retryable() {
        let t = tmp();
        let s = store(&t);
        let bytes = data(PART_BYTES + 5, 3);
        let mut parts = parts_of(&bytes);
        rput(&s, &parts[0]).unwrap();
        parts[0].data[0] ^= 1;
        let e = rput(&s, &parts[0]).unwrap_err();
        assert!(!e.is_retryable());
        let mut other = parts_of(&bytes)[1].clone();
        other.size += 1;
        other.data.push(0);
        assert!(!rput(&s, &other).unwrap_err().is_retryable());
    }

    #[test]
    fn a_hash_mismatch_drops_the_partial_and_is_not_retryable() {
        let t = tmp();
        let s = store(&t);
        let bytes = data(PART_BYTES + 5, 4);
        let mut parts = parts_of(&bytes);
        let sha = parts[0].sha256.clone();
        parts[1].data[0] ^= 1;
        rput(&s, &parts[0]).unwrap();
        let e = rput(&s, &parts[1]).unwrap_err();
        assert!(!e.is_retryable());
        assert!(!s.root.join("partial/relay").join(&sha).exists());
        assert_eq!(s.get(&sha), None);
    }

    #[test]
    fn malformed_parts_are_rejected() {
        let t = tmp();
        let s = store(&t);
        let mut p = parts_of(&data(10, 5))[0].clone();
        p.sha256 = "../x".into();
        assert!(!rput(&s, &p).unwrap_err().is_retryable());
        let big = BlobPart {
            sha256: "ab".repeat(32),
            size: MAX_RELAY_BYTES + 1,
            part: 0,
            parts: 9,
            data: vec![0; PART_BYTES],
        };
        assert!(rput(&s, &big).is_err());
        assert!(!s.root.join("partial/relay").join("ab".repeat(32)).exists());
    }

    #[test]
    fn the_size_limit_is_a_parameter_of_each_put() {
        let t = tmp();
        let s = store(&t);
        let bytes = data(PART_BYTES * 9 + 17, 6);
        let mut last = None;
        for p in parts_of(&bytes) {
            last = Some(lput(&s, &p).unwrap());
        }
        assert!(matches!(last, Some(PutOutcome::Complete(_))));
        let t2 = tmp();
        let strict = store(&t2);
        assert!(rput(&strict, &parts_of(&bytes)[0]).is_err());
    }

    #[test]
    fn a_full_partial_quota_evicts_the_oldest_partial_of_that_source() {
        let t = tmp();
        let s = store(&t).with_quotas(1_000_000, COMPLETE_QUOTA);
        let a = parts_of(&data(PART_BYTES + 5, 7));
        let b = parts_of(&data(PART_BYTES + 5, 8));
        rput(&s, &a[0]).unwrap();
        rput(&s, &b[0]).unwrap();
        assert!(!s.root.join("partial/relay").join(&a[0].sha256).exists());
        assert!(s.root.join("partial/relay").join(&b[0].sha256).exists());
        assert!(matches!(
            rput(&s, &a[1]),
            Ok(PutOutcome::Partial { have: 1, parts: 2 })
        ));
    }

    #[test]
    fn relay_and_local_partials_have_separate_quotas_and_never_clash() {
        let t = tmp();
        let s = store(&t).with_quotas(1_000_000, COMPLETE_QUOTA);
        let relay = parts_of(&data(PART_BYTES + 5, 9));
        let local = parts_of(&data(PART_BYTES + 5, 10));
        let more = parts_of(&data(PART_BYTES + 5, 11));
        rput(&s, &relay[0]).unwrap();
        lput(&s, &local[0]).unwrap();
        lput(&s, &more[0]).unwrap();
        assert!(s.root.join("partial/relay").join(&relay[0].sha256).exists());
        assert!(!s.root.join("partial/local").join(&local[0].sha256).exists());
        assert!(s.root.join("partial/local").join(&more[0].sha256).exists());

        let mut grown = relay[0].clone();
        grown.size = 20 << 20;
        grown.parts = (grown.size as usize).div_ceil(PART_BYTES) as u32;
        let r = lput(&s, &grown);
        assert!(r.is_ok(), "{r:?}");
        assert!(matches!(rput(&s, &relay[1]), Ok(PutOutcome::Complete(_))));
    }

    #[test]
    fn only_transient_io_errors_are_retryable() {
        use io::ErrorKind as K;
        for k in [
            K::StorageFull,
            K::QuotaExceeded,
            K::Interrupted,
            K::WouldBlock,
            K::TimedOut,
            K::Other,
        ] {
            assert!(BlobError::from(io::Error::from(k)).is_retryable(), "{k:?}");
        }
        assert!(BlobError::from(io::Error::from_raw_os_error(libc::ENOSPC)).is_retryable());
        assert!(BlobError::from(io::Error::from_raw_os_error(libc::EDQUOT)).is_retryable());
        for k in [
            K::PermissionDenied,
            K::IsADirectory,
            K::NotADirectory,
            K::InvalidFilename,
            K::AlreadyExists,
            K::InvalidInput,
            K::NotFound,
        ] {
            assert!(!BlobError::from(io::Error::from(k)).is_retryable(), "{k:?}");
        }
    }

    fn retry() -> Result<PutOutcome, BlobError> {
        Err(BlobError::Retryable("disk".into()))
    }

    #[test]
    fn a_part_that_keeps_failing_is_dropped_after_five_tries() {
        let t = tmp();
        let s = store(&t);
        let part = parts_of(&data(10, 50)).remove(0);
        let now = Instant::now();
        for _ in 0..4 {
            assert!(s.deliver_with("dev", &part, now, retry).is_err());
        }
        assert!(s.deliver_with("dev", &part, now, retry).is_ok());
        assert!(s.deliver_with("dev", &part, now, retry).is_err());
    }

    #[test]
    fn a_failing_part_is_dropped_after_ten_minutes_and_budgets_are_per_sender_and_part() {
        let t = tmp();
        let s = store(&t);
        let parts = parts_of(&data(PART_BYTES + 5, 51));
        let now = Instant::now();
        assert!(s.deliver_with("dev", &parts[0], now, retry).is_err());
        assert!(s.deliver_with("dev", &parts[1], now, retry).is_err());
        assert!(s.deliver_with("other", &parts[0], now, retry).is_err());
        let later = now + Duration::from_secs(601);
        assert!(s.deliver_with("dev", &parts[0], later, retry).is_ok());
    }

    #[test]
    fn success_and_rejection_clear_the_retry_budget() {
        let t = tmp();
        let s = store(&t);
        let part = parts_of(&data(10, 52)).remove(0);
        let now = Instant::now();
        for _ in 0..3 {
            assert!(s.deliver_with("dev", &part, now, retry).is_err());
        }
        assert!(s.deliver_relay("dev", &part).is_ok());
        for _ in 0..4 {
            assert!(s.deliver_with("dev", &part, now, retry).is_err());
        }
        let rejected = || Err(BlobError::Rejected("no".into()));
        assert!(s.deliver_with("dev", &part, now, rejected).is_ok());
        for _ in 0..4 {
            assert!(s.deliver_with("dev", &part, now, retry).is_err());
        }
    }

    #[test]
    fn sweep_removes_only_old_partials_and_stale_temp_files() {
        let t = tmp();
        let s = store(&t);
        let parts = parts_of(&data(PART_BYTES + 5, 9));
        rput(&s, &parts[0]).unwrap();
        let dir = s.root.join("partial/relay").join(&parts[0].sha256);
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
        let t = tmp();
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
        let t = tmp();
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
        let t = tmp();
        let s = store(&t);
        let parts = parts_of(&data(PART_BYTES + 5, 31));
        rput(&s, &parts[0]).unwrap();
        let dir = s.root.join("partial/relay").join(&parts[0].sha256);
        assert_eq!(mode(&s.root), 0o700);
        assert_eq!(mode(&s.root.join("partial")), 0o700);
        assert_eq!(mode(&s.root.join("partial/relay")), 0o700);
        assert_eq!(mode(&s.root.join("partial/local")), 0o700);
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join("0")), 0o600);
        let PutOutcome::Complete(path) = rput(&s, &parts[1]).unwrap() else {
            panic!("not complete");
        };
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn a_symlink_planted_at_the_partial_dir_is_not_followed() {
        let t = tmp();
        let outside = tmp();
        let s = store(&t);
        let parts = parts_of(&data(PART_BYTES + 5, 32));
        symlink(
            outside.path(),
            s.root.join("partial/relay").join(&parts[0].sha256),
        )
        .unwrap();
        rput(&s, &parts[0]).unwrap();
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
        assert!(matches!(rput(&s, &parts[1]), Ok(PutOutcome::Complete(_))));
    }

    #[test]
    fn a_symlink_planted_at_a_part_or_the_final_name_is_not_followed() {
        let t = tmp();
        let outside = tmp();
        let target = outside.path().join("victim");
        fs::write(&target, b"keep").unwrap();
        let s = store(&t);
        let parts = parts_of(&data(PART_BYTES + 5, 33));
        let dir = s.root.join("partial/relay").join(&parts[0].sha256);
        ensure_private_dir(&dir).unwrap();
        symlink(&target, dir.join("0")).unwrap();
        symlink(&target, s.root.join(&parts[0].sha256)).unwrap();
        assert_eq!(s.get(&parts[0].sha256), None);
        rput(&s, &parts[0]).unwrap();
        let PutOutcome::Complete(path) = rput(&s, &parts[1]).unwrap() else {
            panic!("not complete");
        };
        assert_eq!(fs::read(&target).unwrap(), b"keep");
        assert!(!fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::metadata(&path).unwrap().len(), PART_BYTES as u64 + 5);
    }

    fn sink(t: &tempfile::TempDir) -> crate::chat::relay::pump::RelaySink {
        crate::chat::relay::pump::RelaySink::new(
            "mx_0123456789abcdef".into(),
            Box::new(|_, _| panic!("no chat expected")),
        )
        .with_blobs(std::sync::Arc::new(store(t)))
    }

    #[test]
    fn the_relay_sink_stores_parts_and_swallows_permanent_failures() {
        use tmgr_chat_relay::engine::ChatSink;
        let t = tmp();
        let sink = sink(&t);
        let bytes = data(PART_BYTES + 5, 40);
        let parts = parts_of(&bytes);
        sink.deliver_blob("dev", parts[0].clone()).unwrap();
        sink.deliver_blob("dev", parts[1].clone()).unwrap();
        assert!(store(&t).get(&parts[0].sha256).is_some());
        let mut bad = parts_of(&data(PART_BYTES + 5, 41));
        bad[1].data[0] ^= 1;
        sink.deliver_blob("dev", bad[0].clone()).unwrap();
        sink.deliver_blob("dev", bad[1].clone()).unwrap();
        assert!(store(&t).get(&bad[0].sha256).is_none());
    }

    #[test]
    fn the_relay_sink_does_not_block_the_channel_on_a_permission_error() {
        use std::os::unix::fs::PermissionsExt;
        use tmgr_chat_relay::engine::ChatSink;
        let t = tmp();
        let sink = sink(&t);
        let partial = t.path().join("chat/blobs/partial/relay");
        fs::set_permissions(&partial, fs::Permissions::from_mode(0o500)).unwrap();
        let part = parts_of(&data(PART_BYTES + 5, 42)).remove(0);
        let r = sink.deliver_blob("dev", part);
        fs::set_permissions(&partial, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(r.is_ok());
    }
}
