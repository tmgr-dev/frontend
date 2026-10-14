use crate::login::{is_symlink_error, open_below, MAX_SEED_BYTES};
use crate::profile::Fresh;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

const RECORDS: &str = "seeds.json";
const DONE: &str = "seeds.done";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedRecord {
    pub root: PathBuf,
    pub source: PathBuf,
    pub home: PathBuf,
    pub to: PathBuf,
    pub sha: String,
    pub json_key: Option<String>,
    pub same: Vec<String>,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn stamp(obj: &Value, key: &str) -> Option<DateTime<Utc>> {
    let text = obj.as_object()?.get(key)?.as_str()?;
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

pub fn check_fresh(
    bytes: &[u8],
    rule: &Fresh,
    now: DateTime<Utc>,
    agent: &str,
    decl: &str,
) -> Result<(), String> {
    let how = format!("run `{agent}` once in a normal terminal so it refreshes {decl}, then start the session again");
    let value: Value = serde_json::from_slice(bytes).unwrap_or(Value::Null);
    let Some(at) = stamp(&value, &rule.json_key) else {
        return Err(format!(
            "{agent} login cannot be checked for freshness ({decl} has no readable {} timestamp); {how}",
            rule.json_key
        ));
    };
    let age = now - at;
    if age > chrono::Duration::days(i64::from(rule.max_age_days)) {
        return Err(format!(
            "{agent} login is due for a refresh (last refreshed {} days ago); {how}",
            (age.num_seconds() + 86_399) / 86_400
        ));
    }
    Ok(())
}

pub fn save(sdir: &Path, records: &[SeedRecord]) -> io::Result<()> {
    if records.is_empty() {
        return Ok(());
    }
    let bytes = serde_json::to_vec(records).map_err(io::Error::other)?;
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(sdir.join(RECORDS))?
        .write_all(&bytes)
}

fn read_regular(root: &Path, path: &Path) -> io::Result<(Vec<u8>, u32)> {
    let mut f = open_below(root, path)?;
    let meta = f.metadata()?;
    if !meta.is_file() || meta.len() > MAX_SEED_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a regular file within the size limit",
        ));
    }
    let mut buf = Vec::new();
    (&mut f).take(MAX_SEED_BYTES + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_SEED_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "too large"));
    }
    Ok((buf, meta.mode()))
}

fn at<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(v, |cur, k| cur.get(k))
}

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn put(dest: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    let dir = dest.parent().ok_or(io::ErrorKind::InvalidInput)?;
    let mut name = OsString::from(".");
    name.push(dest.file_name().ok_or(io::ErrorKind::InvalidInput)?);
    name.push(format!(
        ".tmgr-{}-{}",
        std::process::id(),
        TMP_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let tmp = dir.join(name);
    let result = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&tmp)
        .and_then(|mut f| {
            f.write_all(bytes)?;
            f.sync_all()
        })
        .and_then(|()| fs::rename(&tmp, dest));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
        return result;
    }
    File::open(dir).and_then(|d| d.sync_all())
}

fn keep(runner_dir: &Path, id: &str, copy: &[u8], sha: &str) -> io::Result<PathBuf> {
    let dir = runner_dir.join("seed-backups");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)?;
    let path = dir.join(format!("{id}-{}.bak", &sha[..12]));
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)?
        .write_all(copy)?;
    Ok(path)
}

fn lock(runner_dir: &Path, source: &Path) -> io::Result<File> {
    let dir = runner_dir.join("locks");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)?;
    let f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dir.join(format!(
            "seed-{}.lock",
            sha256_hex(source.as_os_str().as_encoded_bytes())
        )))?;
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(f)
}

pub fn write_back(rec: &SeedRecord, runner_dir: &Path, id: &str) -> io::Result<String> {
    let label = rec.to.display();
    let copy = match read_regular(&rec.home, &rec.home.join(&rec.to)) {
        Ok((b, _)) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(format!(
                "{label}: session copy is gone, nothing to write back"
            ))
        }
        Err(_) => {
            return Ok(format!(
                "{label}: write-back rejected: session copy is not a readable regular file"
            ))
        }
    };
    let csha = sha256_hex(&copy);
    if csha == rec.sha {
        return Ok(format!("{label}: unchanged, nothing to write back"));
    }
    let _lock = lock(runner_dir, &rec.source)?;
    let kept = |why: &str, verb: &str| -> io::Result<String> {
        let path = keep(runner_dir, id, &copy, &csha)?;
        Ok(format!(
            "{label}: {verb}: {why}; session copy kept at {}",
            path.display()
        ))
    };
    let reject = |why: &str| kept(why, "write-back rejected");
    let (src, mode) = match read_regular(&rec.root, &rec.source) {
        Ok(r) => r,
        Err(e) if is_symlink_error(&e) => {
            return reject("source is a symlink or passes through one")
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => return reject("source no longer exists"),
        Err(e) if e.kind() == io::ErrorKind::InvalidData => {
            return reject("source is not a regular file within the size limit")
        }
        Err(e) => return Err(e),
    };
    let ssha = sha256_hex(&src);
    if ssha == csha {
        return Ok(format!(
            "{label}: unchanged, source already holds the session copy"
        ));
    }
    let (Ok(cobj), Ok(sobj)) = (
        serde_json::from_slice::<Value>(&copy),
        serde_json::from_slice::<Value>(&src),
    ) else {
        return reject("not valid JSON");
    };
    let (Some(cmap), Some(smap)) = (cobj.as_object(), sobj.as_object()) else {
        return reject("not a JSON object");
    };
    if smap.keys().any(|k| !cmap.contains_key(k)) {
        return reject("session copy dropped top-level keys");
    }
    if let Some(p) = rec.same.iter().find(|p| at(&cobj, p) != at(&sobj, p)) {
        return reject(&format!("{p} differs from the source"));
    }
    if ssha != rec.sha {
        let newer = rec.json_key.as_deref().is_some_and(
            |k| matches!((stamp(&cobj, k), stamp(&sobj, k)), (Some(c), Some(s)) if c > s),
        );
        if !newer {
            return kept(
                "source changed during the session and the session copy is not newer",
                "kept as backup",
            );
        }
    }
    let name = rec.source.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    let mut backup = name.to_os_string();
    backup.push(".tmgr-backup");
    let dir = rec.source.parent().ok_or(io::ErrorKind::InvalidInput)?;
    put(&dir.join(backup), &src, 0o600)?;
    put(&rec.source, &copy, (mode & 0o600).max(0o600))?;
    Ok(format!("{label}: written back to the source"))
}

static FINISH: Mutex<()> = Mutex::new(());

pub fn finish(sdir: &Path) -> Vec<String> {
    let _g = FINISH.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(bytes) = fs::read(sdir.join(RECORDS)) else {
        return Vec::new();
    };
    if sdir.join(DONE).exists() {
        return Vec::new();
    }
    let Ok(records) = serde_json::from_slice::<Vec<SeedRecord>>(&bytes) else {
        return vec!["seed records are unreadable, skipping write-back".into()];
    };
    let id = sdir.file_name().map(|n| n.to_string_lossy().into_owned());
    let runner_dir = sdir.parent().and_then(Path::parent);
    let (Some(id), Some(runner_dir)) = (id, runner_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut complete = true;
    for rec in &records {
        match write_back(rec, runner_dir, &id) {
            Ok(line) => out.push(line),
            Err(e) => {
                complete = false;
                out.push(format!("{}: write-back failed: {e}", rec.to.display()));
            }
        }
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
    use chrono::Duration;
    use serde_json::json;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::sync::Arc;

    fn rule() -> Fresh {
        Fresh {
            json_key: "last_refresh".into(),
            max_age_days: 7,
        }
    }

    fn doc(days_ago: i64, refresh: &str, account: &str) -> String {
        json!({
            "auth_mode": "chatgpt",
            "last_refresh": (Utc::now() - Duration::days(days_ago)).to_rfc3339(),
            "tokens": {"account_id": account, "refresh_token": refresh},
        })
        .to_string()
    }

    struct Fx {
        _t: tempfile::TempDir,
        run: PathBuf,
        home: PathBuf,
        source: PathBuf,
        rec: SeedRecord,
    }

    fn fx(initial: &str) -> Fx {
        let t = tempfile::Builder::new()
            .prefix("sg")
            .tempdir_in("/tmp")
            .unwrap();
        let base = t.path().canonicalize().unwrap();
        let (run, home, runhome) = (base.join("run"), base.join("home"), base.join("runhome"));
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::create_dir_all(runhome.join(".codex")).unwrap();
        let source = home.join(".codex/auth.json");
        fs::write(&source, initial).unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).unwrap();
        fs::write(runhome.join(".codex/auth.json"), initial).unwrap();
        let rec = SeedRecord {
            root: home.clone(),
            source: source.clone(),
            home: runhome,
            to: ".codex/auth.json".into(),
            sha: sha256_hex(initial.as_bytes()),
            json_key: Some("last_refresh".into()),
            same: vec!["tokens.account_id".into(), "auth_mode".into()],
        };
        Fx {
            _t: t,
            run,
            home,
            source,
            rec,
        }
    }

    impl Fx {
        fn set_copy(&self, body: &str) {
            fs::write(self.rec.home.join(&self.rec.to), body).unwrap();
        }
        fn run(&self, id: &str) -> String {
            write_back(&self.rec, &self.run, id).unwrap()
        }
        fn source_text(&self) -> String {
            fs::read_to_string(&self.source).unwrap()
        }
        fn mode(p: &Path) -> u32 {
            fs::metadata(p).unwrap().permissions().mode() & 0o777
        }
        fn backups(&self) -> Vec<PathBuf> {
            fs::read_dir(self.run.join("seed-backups"))
                .map(|d| d.flatten().map(|e| e.path()).collect())
                .unwrap_or_default()
        }
    }

    #[test]
    fn a_recent_login_passes_and_a_stale_one_is_refused_without_leaking_tokens() {
        let now = Utc::now();
        assert!(check_fresh(
            doc(1, "tok-secret", "a").as_bytes(),
            &rule(),
            now,
            "codex",
            "~/x"
        )
        .is_ok());
        let e = check_fresh(
            doc(8, "tok-secret", "a").as_bytes(),
            &rule(),
            now,
            "codex",
            "~/.codex/auth.json",
        )
        .unwrap_err();
        assert!(
            e.contains("due for a refresh") && e.contains("8 days ago") && e.contains("`codex`"),
            "{e}"
        );
        assert!(!e.contains("tok-secret"));
    }

    #[test]
    fn a_missing_or_garbage_timestamp_is_refused() {
        let now = Utc::now();
        for body in [
            "{}",
            "not json",
            "[]",
            r#"{"last_refresh":"yesterday"}"#,
            r#"{"last_refresh":5}"#,
        ] {
            let e = check_fresh(body.as_bytes(), &rule(), now, "codex", "~/x").unwrap_err();
            assert!(e.contains("cannot be checked"), "{body}: {e}");
        }
    }

    #[test]
    fn fractional_seconds_and_offsets_parse() {
        let now = Utc::now();
        let t = (now - Duration::days(2))
            .format("%Y-%m-%dT%H:%M:%S%.9f+02:00")
            .to_string();
        let body = json!({"last_refresh": t}).to_string();
        assert!(check_fresh(body.as_bytes(), &rule(), now, "codex", "~/x").is_ok());
    }

    #[test]
    fn a_refreshed_copy_replaces_the_source_and_backs_up_the_old_one() {
        let old = doc(5, "old-rt", "acc");
        let f = fx(&old);
        let new = doc(0, "new-rt", "acc");
        f.set_copy(&new);
        assert!(f.run("s1").contains("written back"));
        assert_eq!(f.source_text(), new);
        let backup = f.home.join(".codex/auth.json.tmgr-backup");
        assert_eq!(fs::read_to_string(&backup).unwrap(), old);
        assert_eq!(Fx::mode(&backup), 0o600);
        assert_eq!(Fx::mode(&f.source), 0o600);
        assert!(f.backups().is_empty());
    }

    #[test]
    fn an_unchanged_copy_writes_nothing() {
        let f = fx(&doc(5, "rt", "acc"));
        assert!(f.run("s1").contains("unchanged"));
        assert!(!f.home.join(".codex/auth.json.tmgr-backup").exists());
    }

    #[test]
    fn another_account_is_rejected_and_the_copy_is_kept() {
        let old = doc(5, "old-rt", "acc");
        let f = fx(&old);
        f.set_copy(&doc(0, "planted", "other"));
        let line = f.run("s1");
        assert!(
            line.contains("rejected") && line.contains("tokens.account_id"),
            "{line}"
        );
        assert!(!line.contains("planted"));
        assert_eq!(f.source_text(), old);
        assert!(!f.home.join(".codex/auth.json.tmgr-backup").exists());
        let kept = f.backups();
        assert_eq!(kept.len(), 1);
        assert_eq!(Fx::mode(&kept[0]), 0o600);
    }

    #[test]
    fn dropped_keys_and_non_objects_are_rejected() {
        let old = doc(5, "rt", "acc");
        let f = fx(&old);
        for body in [
            json!({"auth_mode":"chatgpt","tokens":{"account_id":"acc"}}).to_string(),
            "[1]".into(),
            "garbage".into(),
        ] {
            f.set_copy(&body);
            assert!(f.run("s1").contains("rejected"), "{body}");
            assert_eq!(f.source_text(), old);
        }
    }

    #[test]
    fn a_source_that_moved_on_wins_over_an_older_session_copy() {
        let f = fx(&doc(5, "old-rt", "acc"));
        let owner = doc(0, "owner-rt", "acc");
        fs::write(&f.source, &owner).unwrap();
        f.set_copy(&doc(2, "session-rt", "acc"));
        let line = f.run("s1");
        assert!(line.contains("kept as backup"), "{line}");
        assert_eq!(f.source_text(), owner);
        assert_eq!(f.backups().len(), 1);
    }

    #[test]
    fn a_source_that_moved_on_loses_to_a_strictly_newer_session_copy() {
        let f = fx(&doc(6, "old-rt", "acc"));
        let owner = doc(3, "owner-rt", "acc");
        fs::write(&f.source, &owner).unwrap();
        let new = doc(0, "session-rt", "acc");
        f.set_copy(&new);
        assert!(f.run("s1").contains("written back"));
        assert_eq!(f.source_text(), new);
        assert_eq!(
            fs::read_to_string(f.home.join(".codex/auth.json.tmgr-backup")).unwrap(),
            owner
        );
    }

    #[test]
    fn a_symlinked_source_is_refused() {
        let old = doc(5, "rt", "acc");
        let f = fx(&old);
        let elsewhere = f.home.join("elsewhere.json");
        fs::write(&elsewhere, &old).unwrap();
        fs::remove_file(&f.source).unwrap();
        symlink(&elsewhere, &f.source).unwrap();
        f.set_copy(&doc(0, "new", "acc"));
        let line = f.run("s1");
        assert!(line.contains("symlink"), "{line}");
        assert_eq!(fs::read_to_string(&elsewhere).unwrap(), old);
        assert!(fs::symlink_metadata(&f.source)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn concurrent_exits_never_tear_the_source() {
        let f = Arc::new(fx(&doc(5, "old-rt", "acc")));
        let copies: Vec<String> = (0..8)
            .map(|i| doc(0, &format!("rt-{i}-{}", "x".repeat(4000)), "acc"))
            .collect();
        let mut handles = Vec::new();
        for (i, c) in copies.iter().enumerate() {
            let f = f.clone();
            let c = c.clone();
            handles.push(std::thread::spawn(move || {
                let mut rec = f.rec.clone();
                rec.home = f.rec.home.with_file_name(format!("rh{i}"));
                fs::create_dir_all(rec.home.join(".codex")).unwrap();
                fs::write(rec.home.join(".codex/auth.json"), c).unwrap();
                write_back(&rec, &f.run, &format!("s{i}")).unwrap();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let text = f.source_text();
        assert!(copies.contains(&text), "torn or foreign source");
        serde_json::from_str::<Value>(&text).unwrap();
    }

    #[test]
    fn finish_is_idempotent_and_marks_the_session() {
        let f = fx(&doc(5, "old-rt", "acc"));
        let sdir = f.run.join("sessions/s1");
        fs::create_dir_all(&sdir).unwrap();
        save(&sdir, std::slice::from_ref(&f.rec)).unwrap();
        let new = doc(0, "new-rt", "acc");
        f.set_copy(&new);
        assert_eq!(finish(&sdir).len(), 1);
        assert!(finish(&sdir).is_empty());
        assert_eq!(f.source_text(), new);
    }
}
