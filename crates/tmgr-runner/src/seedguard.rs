use crate::login::{is_symlink_error, open_below, open_dir_below, openat_nofollow, MAX_SEED_BYTES};
use crate::profile::{user_dir, valid_profile_id, Fresh};
use crate::sanitize::sanitize_text;
use crate::wire::now_ms;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::ffi::{CString, OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tmgr_ipc::api::CandidateInfo;

const RECORDS: &str = "seeds.json";
const DONE: &str = "seeds.done";
const CANDIDATES: &str = "candidates";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedRecord {
    pub profile: String,
    pub index: usize,
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

fn read_open(mut f: File) -> io::Result<(Vec<u8>, u32)> {
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

fn read_regular(root: &Path, path: &Path) -> io::Result<(Vec<u8>, u32)> {
    read_open(open_below(root, path)?)
}

fn at<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(v, |cur, k| cur.get(k))
}

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp_name(dest: &OsStr) -> OsString {
    let mut name = OsString::from(".");
    name.push(dest);
    name.push(format!(
        ".tmgr-{}-{}",
        std::process::id(),
        TMP_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    name
}

fn put(dest: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    let dir = dest.parent().ok_or(io::ErrorKind::InvalidInput)?;
    let tmp = dir.join(tmp_name(
        dest.file_name().ok_or(io::ErrorKind::InvalidInput)?,
    ));
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

fn cstr(name: &OsStr) -> io::Result<CString> {
    CString::new(name.as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
}

fn put_at(dfd: RawFd, dest: &OsStr, bytes: &[u8]) -> io::Result<()> {
    let tmp = cstr(&tmp_name(dest))?;
    let to = cstr(dest)?;
    let fd = unsafe {
        libc::openat(
            dfd,
            tmp.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600 as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut f = unsafe { File::from_raw_fd(fd) };
    let result = f
        .write_all(bytes)
        .and_then(|()| f.sync_all())
        .and_then(|()| {
            if unsafe { libc::renameat(dfd, tmp.as_ptr(), dfd, to.as_ptr()) } < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    if result.is_err() {
        unsafe { libc::unlinkat(dfd, tmp.as_ptr(), 0) };
    }
    result
}

fn is_regular_at(dfd: RawFd, name: &OsStr) -> bool {
    let Ok(c) = cstr(name) else {
        return false;
    };
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::fstatat(dfd, c.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) };
    rc == 0 && (st.st_mode & libc::S_IFMT) == libc::S_IFREG
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    pub root: PathBuf,
    pub source: PathBuf,
    pub sha: String,
    pub source_sha: String,
    pub session: String,
    pub created_ms: u64,
    pub json_key: Option<String>,
    pub same: Vec<String>,
}

fn cdir(runner_dir: &Path, profile: &str) -> io::Result<PathBuf> {
    if !valid_profile_id(profile) {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    Ok(runner_dir.join(CANDIDATES).join(profile))
}

fn copy_name(index: usize) -> String {
    format!("{index}.json")
}

fn meta_name(index: usize) -> String {
    format!("{index}.meta.json")
}

pub fn store(
    runner_dir: &Path,
    profile: &str,
    index: usize,
    meta: &Meta,
    copy: &[u8],
) -> io::Result<()> {
    let dir = cdir(runner_dir, profile)?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)?;
    put(&dir.join(copy_name(index)), copy, 0o600)?;
    let bytes = serde_json::to_vec(meta).map_err(io::Error::other)?;
    put(&dir.join(meta_name(index)), &bytes, 0o600)
}

fn read_meta(dir: &Path, index: usize) -> io::Result<Meta> {
    let (bytes, _) = read_regular(dir, &dir.join(meta_name(index)))?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

fn read_candidate(runner_dir: &Path, profile: &str, index: usize) -> io::Result<(Meta, Vec<u8>)> {
    let dir = cdir(runner_dir, profile)?;
    let meta = read_meta(&dir, index)?;
    let (copy, _) = read_regular(&dir, &dir.join(copy_name(index)))?;
    Ok((meta, copy))
}

pub fn entries(runner_dir: &Path, profile: &str) -> Vec<(usize, Meta)> {
    let Ok(dir) = cdir(runner_dir, profile) else {
        return Vec::new();
    };
    let Ok(rd) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<(usize, Meta)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let index = name.strip_suffix(".meta.json")?.parse().ok()?;
            Some((index, read_meta(&dir, index).ok()?))
        })
        .collect();
    out.sort_by_key(|(i, _)| *i);
    out
}

pub fn pending(runner_dir: &Path) -> Vec<CandidateInfo> {
    let Ok(rd) = fs::read_dir(runner_dir.join(CANDIDATES)) else {
        return Vec::new();
    };
    let mut out: Vec<CandidateInfo> = rd
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .flat_map(|profile| {
            entries(runner_dir, &profile)
                .into_iter()
                .map(move |(_, m)| CandidateInfo {
                    profile: profile.clone(),
                    source: m.source.display().to_string(),
                    created_ms: m.created_ms,
                    session: m.session,
                })
        })
        .collect();
    out.sort_by(|a, b| (&a.profile, a.created_ms).cmp(&(&b.profile, b.created_ms)));
    out
}

fn remove_candidate(runner_dir: &Path, profile: &str, index: usize) -> io::Result<()> {
    let dir = cdir(runner_dir, profile)?;
    for name in [meta_name(index), copy_name(index)] {
        match fs::remove_file(dir.join(name)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

pub fn discard(runner_dir: &Path, profile: &str) -> io::Result<usize> {
    let list = entries(runner_dir, profile);
    for (index, _) in &list {
        remove_candidate(runner_dir, profile, *index)?;
    }
    Ok(list.len())
}

pub fn autosync_path(profile: &str) -> Option<PathBuf> {
    valid_profile_id(profile)
        .then(|| user_dir().map(|d| d.join(format!("{profile}.autosync"))))
        .flatten()
}

pub fn autosync_on(profile: &str) -> bool {
    autosync_path(profile)
        .and_then(|p| fs::symlink_metadata(p).ok())
        .is_some_and(|m| {
            m.is_file() && m.uid() == unsafe { libc::geteuid() } && m.mode() & 0o077 == 0
        })
}

pub fn set_autosync(profile: &str, on: bool) -> io::Result<()> {
    let path = autosync_path(profile).ok_or(io::ErrorKind::InvalidInput)?;
    if !on {
        return match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)?;
    f.set_permissions(fs::Permissions::from_mode(0o600))?;
    f.write_all(
        serde_json::json!({"profile": profile, "agreed_ms": now_ms()})
            .to_string()
            .as_bytes(),
    )
}

pub fn stage(rec: &SeedRecord, runner_dir: &Path, session: &str) -> io::Result<(String, bool)> {
    let label = rec.to.display();
    let copy = match read_regular(&rec.home, &rec.home.join(&rec.to)) {
        Ok((b, _)) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok((
                format!("{label}: session copy is gone, no candidate"),
                false,
            ))
        }
        Err(_) => {
            return Ok((
                format!("{label}: no candidate: session copy is not a readable regular file"),
                false,
            ))
        }
    };
    let csha = sha256_hex(&copy);
    if csha == rec.sha {
        return Ok((format!("{label}: unchanged, no candidate"), false));
    }
    let _lock = lock(runner_dir, &rec.source)?;
    let reject = |why: &str| Ok((format!("{label}: no candidate: {why}"), false));
    let (src, _) = match read_regular(&rec.root, &rec.source) {
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
        return Ok((
            format!("{label}: unchanged, source already holds the session copy"),
            false,
        ));
    }
    if let Err(why) = check_copy(&copy, &src, &rec.same) {
        return reject(&why);
    }
    if ssha != rec.sha {
        let key = rec.json_key.as_deref();
        if !key.is_some_and(|k| compare(&copy, &src, k) == Some(std::cmp::Ordering::Greater)) {
            return reject("source changed during the session and the session copy is not newer");
        }
    }
    let meta = Meta {
        root: rec.root.clone(),
        source: rec.source.clone(),
        sha: rec.sha.clone(),
        source_sha: ssha,
        session: session.to_string(),
        created_ms: now_ms(),
        json_key: rec.json_key.clone(),
        same: rec.same.clone(),
    };
    store(runner_dir, &rec.profile, rec.index, &meta, &copy)?;
    Ok((
        format!(
            "{label}: refreshed login stored as a candidate; run `tmgr agents sync {}` to apply it",
            rec.profile
        ),
        true,
    ))
}

fn objects(copy: &[u8], src: &[u8]) -> Result<(Value, Value), String> {
    let (Ok(c), Ok(s)) = (
        serde_json::from_slice::<Value>(copy),
        serde_json::from_slice::<Value>(src),
    ) else {
        return Err("not valid JSON".into());
    };
    if !c.is_object() || !s.is_object() {
        return Err("not a JSON object".into());
    }
    Ok((c, s))
}

fn check_copy(copy: &[u8], src: &[u8], same: &[String]) -> Result<(), String> {
    let (c, s) = objects(copy, src)?;
    let (Some(cmap), Some(smap)) = (c.as_object(), s.as_object()) else {
        return Err("not a JSON object".into());
    };
    if smap.keys().any(|k| !cmap.contains_key(k)) {
        return Err("session copy dropped top-level keys".into());
    }
    match same.iter().find(|p| at(&c, p) != at(&s, p)) {
        Some(p) => Err(format!("{p} differs from the source")),
        None => Ok(()),
    }
}

fn compare(copy: &[u8], src: &[u8], key: &str) -> Option<std::cmp::Ordering> {
    let stamp_of = |b: &[u8]| {
        serde_json::from_slice::<Value>(b)
            .ok()
            .and_then(|v| stamp(&v, key))
    };
    Some(stamp_of(copy)?.cmp(&stamp_of(src)?))
}

fn shown(source: &Path, root: &Path) -> String {
    match source.strip_prefix(root) {
        Ok(rel) => format!("~/{}", rel.display()),
        Err(_) => source.display().to_string(),
    }
}

fn ago(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m", secs / 60),
        3600..=86_399 => format!("{}h", secs / 3600),
        _ => format!("{}d", secs / 86_400),
    }
}

fn clean(s: &str) -> String {
    sanitize_text(s).into_owned()
}

pub fn summary(
    runner_dir: &Path,
    profile: &str,
    index: usize,
    now: u64,
) -> io::Result<Vec<String>> {
    let (meta, copy) = read_candidate(runner_dir, profile, index)?;
    let mut out = vec![
        format!("source:    {}", clean(&shown(&meta.source, &meta.root))),
        format!(
            "captured:  {} ago, in session {}",
            ago(now.saturating_sub(meta.created_ms) / 1000),
            clean(&meta.session)
        ),
    ];
    let Ok((src, _)) = read_regular(&meta.root, &meta.source) else {
        out.push("the original cannot be read now; nothing can be applied".into());
        return Ok(out);
    };
    let Ok((c, s)) = objects(&copy, &src) else {
        out.push(
            "the candidate or the original is not a JSON object; nothing can be applied".into(),
        );
        return Ok(out);
    };
    for p in &meta.same {
        out.push(format!(
            "same {}: {}",
            clean(p),
            if at(&c, p) == at(&s, p) { "yes" } else { "no" }
        ));
    }
    let (Some(cmap), Some(smap)) = (c.as_object(), s.as_object()) else {
        return Ok(out);
    };
    let changed: Vec<String> = smap
        .iter()
        .filter(|(k, v)| cmap.get(*k) != Some(v))
        .map(|(k, _)| clean(k))
        .collect();
    out.push(format!(
        "changed top-level keys: {}",
        if changed.is_empty() {
            "none".into()
        } else {
            changed.join(", ")
        }
    ));
    let added = cmap.keys().filter(|k| !smap.contains_key(*k)).count();
    if added > 0 {
        out.push(format!("new top-level keys: {added}"));
    }
    if let Some(key) = &meta.json_key {
        let at_text =
            |v: &Value| stamp(v, key).map_or_else(|| "unreadable".into(), |t| t.to_rfc3339());
        out.push(format!(
            "{}: {} -> {}",
            clean(key),
            at_text(&s),
            at_text(&c)
        ));
    }
    if sha256_hex(&src) != meta.source_sha {
        out.push("the original changed after the candidate was captured".into());
    }
    Ok(out)
}

fn refuse(what: impl Into<String>) -> String {
    format!("{}; nothing was changed", what.into())
}

pub fn apply(runner_dir: &Path, profile: &str, index: usize) -> Result<String, String> {
    let (meta, copy) = read_candidate(runner_dir, profile, index).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            refuse(format!("no pending login for {profile}"))
        } else {
            refuse(format!("the candidate cannot be read: {e}"))
        }
    })?;
    let io_err = |e: io::Error| refuse(e.to_string());
    let _lock = lock(runner_dir, &meta.source).map_err(io_err)?;
    let (Some(parent), Some(name)) = (meta.source.parent(), meta.source.file_name()) else {
        return Err(refuse("the original path is invalid"));
    };
    let dir = open_dir_below(&meta.root, parent).map_err(|e| {
        if is_symlink_error(&e) {
            refuse("the folder of the original is, or passes through, a symlink")
        } else {
            io_err(e)
        }
    })?;
    let dfd = dir.as_raw_fd();
    if !is_regular_at(dfd, name) {
        return Err(refuse("the original is not a regular file"));
    }
    let (src, _) = openat_nofollow(dfd, name, libc::O_RDONLY | libc::O_NONBLOCK)
        .and_then(read_open)
        .map_err(|e| {
            if is_symlink_error(&e) {
                refuse("the original is a symlink")
            } else {
                io_err(e)
            }
        })?;
    if sha256_hex(&src) != meta.source_sha {
        let key = meta.json_key.as_deref();
        let ok = key.is_some_and(|k| {
            matches!(
                compare(&copy, &src, k),
                Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
            )
        });
        if !ok {
            return Err(refuse(
                "the original changed and holds a newer login; run the agent again to capture a fresh one",
            ));
        }
    }
    check_copy(&copy, &src, &meta.same)
        .map_err(|why| refuse(format!("the candidate is rejected: {why}")))?;
    let mut backup = name.to_os_string();
    backup.push(".tmgr-backup");
    put_at(dfd, &backup, &src).map_err(io_err)?;
    if !is_regular_at(dfd, name) {
        return Err(refuse("the original is not a regular file"));
    }
    put_at(dfd, name, &copy).map_err(io_err)?;
    if !is_regular_at(dfd, name) {
        return Err("the original is not a regular file after the update; check it".into());
    }
    dir.sync_all().map_err(io_err)?;
    remove_candidate(runner_dir, profile, index).map_err(io_err)?;
    Ok(format!(
        "applied; the previous login is kept as {}.tmgr-backup",
        shown(&meta.source, &meta.root)
    ))
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
        return vec!["seed records are unreadable, skipping candidates".into()];
    };
    let id = sdir.file_name().map(|n| n.to_string_lossy().into_owned());
    let runner_dir = sdir.parent().and_then(Path::parent);
    let (Some(id), Some(runner_dir)) = (id, runner_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut complete = true;
    for rec in &records {
        match stage(rec, runner_dir, &id) {
            Ok((line, staged)) => {
                out.push(line);
                if staged && autosync_on(&rec.profile) {
                    out.push(match apply(runner_dir, &rec.profile, rec.index) {
                        Ok(done) => format!("{}: auto mode: {done}", rec.to.display()),
                        Err(e) => format!("{}: auto mode: not applied: {e}", rec.to.display()),
                    });
                }
            }
            Err(e) => {
                complete = false;
                out.push(format!("{}: candidate failed: {e}", rec.to.display()));
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
            profile: "codex".into(),
            index: 0,
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
            stage(&self.rec, &self.run, id).unwrap().0
        }
        fn candidate(&self) -> Option<String> {
            fs::read_to_string(self.run.join("candidates/codex/0.json")).ok()
        }
        fn apply(&self) -> Result<String, String> {
            apply(&self.run, "codex", 0)
        }
        fn backup(&self) -> PathBuf {
            self.home.join(".codex/auth.json.tmgr-backup")
        }
        fn source_text(&self) -> String {
            fs::read_to_string(&self.source).unwrap()
        }
        fn mode(p: &Path) -> u32 {
            fs::metadata(p).unwrap().permissions().mode() & 0o777
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
    fn a_refreshed_copy_becomes_a_candidate_and_the_original_is_untouched() {
        let old = doc(5, "old-rt", "acc");
        let f = fx(&old);
        let new = doc(0, "new-rt", "acc");
        f.set_copy(&new);
        let line = f.run("s1");
        assert!(line.contains("candidate") && line.contains("tmgr agents sync codex"));
        assert_eq!(f.source_text(), old);
        assert!(!f.backup().exists());
        assert_eq!(f.candidate().unwrap(), new);
        let dir = f.run.join("candidates/codex");
        assert_eq!(Fx::mode(&dir), 0o700);
        assert_eq!(Fx::mode(&dir.join("0.json")), 0o600);
        assert_eq!(Fx::mode(&dir.join("0.meta.json")), 0o600);
        let list = pending(&f.run);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].profile, "codex");
        assert_eq!(list[0].session, "s1");
        assert_eq!(list[0].source, f.source.display().to_string());
    }

    #[test]
    fn a_newer_candidate_replaces_the_older_one() {
        let f = fx(&doc(5, "old-rt", "acc"));
        f.set_copy(&doc(1, "first", "acc"));
        f.run("s1");
        let second = doc(0, "second", "acc");
        f.set_copy(&second);
        f.run("s2");
        assert_eq!(f.candidate().unwrap(), second);
        assert_eq!(pending(&f.run).len(), 1);
        assert_eq!(pending(&f.run)[0].session, "s2");
    }

    #[test]
    fn an_unchanged_copy_leaves_no_candidate() {
        let f = fx(&doc(5, "rt", "acc"));
        assert!(f.run("s1").contains("unchanged"));
        assert!(f.candidate().is_none() && pending(&f.run).is_empty());
    }

    #[test]
    fn failed_checks_leave_no_candidate_and_do_not_leak() {
        let old = doc(5, "old-rt", "acc");
        let f = fx(&old);
        let planted = concat!("planted-", "tok-FAKE");
        let cases = [
            doc(0, planted, "other"),
            json!({"auth_mode":"chatgpt","tokens":{"account_id":"acc"}}).to_string(),
            "[1]".into(),
            "garbage".into(),
        ];
        for body in cases {
            f.set_copy(&body);
            let line = f.run("s1");
            assert!(line.contains("no candidate"), "{line}");
            assert!(!line.contains(planted));
            assert!(f.candidate().is_none());
            assert_eq!(f.source_text(), old);
        }
    }

    #[test]
    fn a_source_that_moved_on_wins_over_an_older_session_copy() {
        let f = fx(&doc(5, "old-rt", "acc"));
        let owner = doc(0, "owner-rt", "acc");
        fs::write(&f.source, &owner).unwrap();
        f.set_copy(&doc(2, "session-rt", "acc"));
        assert!(f.run("s1").contains("not newer"));
        assert!(f.candidate().is_none());
        assert_eq!(f.source_text(), owner);
    }

    #[test]
    fn a_symlinked_source_gets_no_candidate() {
        let old = doc(5, "rt", "acc");
        let f = fx(&old);
        let elsewhere = f.home.join("elsewhere.json");
        fs::write(&elsewhere, &old).unwrap();
        fs::remove_file(&f.source).unwrap();
        symlink(&elsewhere, &f.source).unwrap();
        f.set_copy(&doc(0, "new", "acc"));
        assert!(f.run("s1").contains("symlink"));
        assert!(f.candidate().is_none());
    }

    #[test]
    fn apply_replaces_the_source_and_backs_up_the_old_one() {
        let old = doc(5, "old-rt", "acc");
        let f = fx(&old);
        let new = doc(0, "new-rt", "acc");
        f.set_copy(&new);
        f.run("s1");
        assert!(f.apply().unwrap().contains("applied"));
        assert_eq!(f.source_text(), new);
        assert_eq!(fs::read_to_string(f.backup()).unwrap(), old);
        assert_eq!(Fx::mode(&f.backup()), 0o600);
        assert_eq!(Fx::mode(&f.source), 0o600);
        assert!(f.candidate().is_none() && pending(&f.run).is_empty());
        assert!(f.apply().unwrap_err().contains("no pending"));
    }

    #[test]
    fn apply_refuses_when_the_source_moved_on_with_a_newer_login() {
        let f = fx(&doc(6, "old-rt", "acc"));
        f.set_copy(&doc(1, "session-rt", "acc"));
        f.run("s1");
        let owner = doc(0, "owner-rt", "acc");
        fs::write(&f.source, &owner).unwrap();
        let e = f.apply().unwrap_err();
        assert!(e.contains("run the agent again"), "{e}");
        assert_eq!(f.source_text(), owner);
        assert!(f.candidate().is_some() && !f.backup().exists());
    }

    #[test]
    fn apply_goes_ahead_when_the_source_changed_but_is_not_newer() {
        let f = fx(&doc(6, "old-rt", "acc"));
        let new = doc(0, "session-rt", "acc");
        f.set_copy(&new);
        f.run("s1");
        fs::write(&f.source, doc(3, "owner-rt", "acc")).unwrap();
        f.apply().unwrap();
        assert_eq!(f.source_text(), new);
    }

    #[test]
    fn apply_refuses_when_the_parent_directory_became_a_symlink() {
        let old = doc(5, "rt", "acc");
        let f = fx(&old);
        f.set_copy(&doc(0, "new", "acc"));
        f.run("s1");
        let moved = f.home.join("moved");
        fs::rename(f.home.join(".codex"), &moved).unwrap();
        symlink(&moved, f.home.join(".codex")).unwrap();
        let e = f.apply().unwrap_err();
        assert!(e.contains("symlink"), "{e}");
        assert_eq!(fs::read_to_string(moved.join("auth.json")).unwrap(), old);
        assert!(!moved.join("auth.json.tmgr-backup").exists());
        assert!(f.candidate().is_some());
    }

    #[test]
    fn apply_refuses_a_source_that_became_a_symlink() {
        let old = doc(5, "rt", "acc");
        let f = fx(&old);
        f.set_copy(&doc(0, "new", "acc"));
        f.run("s1");
        let elsewhere = f.home.join("elsewhere.json");
        fs::write(&elsewhere, &old).unwrap();
        fs::remove_file(&f.source).unwrap();
        symlink(&elsewhere, &f.source).unwrap();
        assert!(f.apply().is_err());
        assert_eq!(fs::read_to_string(&elsewhere).unwrap(), old);
    }

    #[test]
    fn the_summary_names_keys_and_timestamps_but_no_token_values() {
        let old_tok = concat!("old-", "refresh-FAKE1234");
        let new_tok = concat!("new-", "refresh-FAKE5678");
        let f = fx(&doc(5, old_tok, "acc"));
        f.set_copy(&doc(0, new_tok, "acc"));
        f.run("s1");
        let text = summary(&f.run, "codex", 0, now_ms()).unwrap().join(
            "
",
        );
        assert!(text.contains("~/.codex/auth.json"), "{text}");
        assert!(text.contains("same tokens.account_id: yes"), "{text}");
        assert!(text.contains("same auth_mode: yes"), "{text}");
        assert!(
            text.contains("changed top-level keys: last_refresh, tokens"),
            "{text}"
        );
        assert!(
            text.contains("last_refresh:") && text.contains(" -> "),
            "{text}"
        );
        assert!(text.contains("session s1"), "{text}");
        for secret in [old_tok, new_tok, "FAKE1234", "FAKE5678"] {
            assert!(!text.contains(secret), "summary leaks a token fragment");
        }
    }

    #[test]
    fn discard_removes_candidates_without_touching_the_source() {
        let old = doc(5, "rt", "acc");
        let f = fx(&old);
        f.set_copy(&doc(0, "new", "acc"));
        f.run("s1");
        assert_eq!(discard(&f.run, "codex").unwrap(), 1);
        assert!(pending(&f.run).is_empty());
        assert_eq!(discard(&f.run, "codex").unwrap(), 0);
        assert_eq!(f.source_text(), old);
    }

    #[test]
    fn concurrent_exits_leave_one_whole_candidate() {
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
                stage(&rec, &f.run, &format!("s{i}")).unwrap();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let text = f.candidate().unwrap();
        assert!(copies.contains(&text), "torn or foreign candidate");
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
        assert_eq!(f.candidate().unwrap(), new);
        assert_eq!(pending(&f.run)[0].session, "s1");
    }
}
