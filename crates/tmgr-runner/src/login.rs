use crate::profile::{Fresh, Profile, SecretFile};
use crate::sandbox::{
    canon_lossy, env_name_refused, home_deny_paths, overlaps, protected_forms, Grants,
};
use crate::seedguard::{self, SeedRecord};
use std::collections::{BTreeMap, VecDeque};
use std::ffi::{CString, OsStr};
use std::fs::{DirBuilder, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

pub(crate) const MAX_SEED_BYTES: u64 = 1 << 20;
const MAX_SECRETS_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSeed {
    pub root: PathBuf,
    pub from: PathBuf,
    pub to: PathBuf,
    pub json_keys: Option<Vec<String>>,
    pub fresh: Option<Fresh>,
    pub write_back: bool,
    pub same: Vec<String>,
    pub decl: String,
    pub agent: String,
    pub profile: String,
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSecretFile {
    pub root: PathBuf,
    pub from: PathBuf,
    pub to: PathBuf,
    pub token_prefix: Option<String>,
    pub template: Option<String>,
}

#[derive(Debug, Default)]
pub struct Resolved {
    pub secret_files: Vec<ResolvedSecretFile>,
    pub read: Vec<PathBuf>,
    pub seeds: Vec<ResolvedSeed>,
    pub secrets: Option<PathBuf>,
    pub warnings: Vec<String>,
}

pub struct Prepared {
    pub secret_files: Vec<ResolvedSecretFile>,
    pub grants: Grants,
    pub seeds: Vec<ResolvedSeed>,
    pub pass_values: Vec<(String, String)>,
    pub warnings: Vec<String>,
}

fn valid_env_name(n: &str) -> bool {
    let b = n.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && !b[0].is_ascii_digit()
        && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

pub fn check_env(p: &Profile) -> Result<(), String> {
    for name in p.env_pass.iter().chain(p.env.keys()) {
        if !valid_env_name(name) {
            return Err(format!("invalid environment variable name {name:?}"));
        }
        if env_name_refused(name) {
            return Err(format!(
                "environment variable {name} is never passed to sessions"
            ));
        }
    }
    if p.env.values().any(|v| v.contains('\0')) {
        return Err("env values must not contain NUL".into());
    }
    Ok(())
}

fn plain_relative(what: &str, rel: &Path) -> Result<(), String> {
    let ok =
        rel.components().count() > 0 && rel.components().all(|c| matches!(c, Component::Normal(_)));
    if ok {
        Ok(())
    } else {
        Err(format!(
            "{what} {:?} must be a plain relative path without \"..\"",
            rel.display().to_string()
        ))
    }
}

fn under_home(what: &str, decl: &str, home: &Path) -> Result<PathBuf, String> {
    let rel = decl
        .strip_prefix("~/")
        .ok_or_else(|| format!("{what} {decl:?} must start with ~/"))?;
    plain_relative(what, Path::new(rel))?;
    let joined = home.join(rel);
    let path = match (joined.parent(), joined.file_name()) {
        (Some(parent), Some(name)) => canon_lossy(parent).join(name),
        _ => joined,
    };
    if path == home || !path.starts_with(home) {
        return Err(format!(
            "{what} {decl:?} resolves outside the home directory"
        ));
    }
    Ok(path)
}

pub fn resolve(p: &Profile, real_home: &Path, protected: &[PathBuf]) -> Result<Resolved, String> {
    check_env(p)?;
    let home = canon_lossy(real_home);
    let mut deny = home_deny_paths(real_home);
    for d in protected {
        for f in protected_forms(d) {
            push_new(&mut deny, f);
        }
    }
    let allowed = |what: &str, decl: &str| -> Result<PathBuf, String> {
        let path = under_home(what, decl, &home)?;
        match deny.iter().find(|d| overlaps(&path, d)) {
            Some(d) => Err(format!(
                "{what} {decl:?} overlaps the protected path {}",
                d.display()
            )),
            None => Ok(path),
        }
    };
    let mut out = Resolved::default();
    for decl in &p.sandbox.read {
        let path = allowed("sandbox read path", decl)?;
        match path.canonicalize() {
            Ok(c) if !deny.iter().any(|d| overlaps(&c, d)) && c.starts_with(&home) => {
                out.read.push(c)
            }
            Ok(c) => {
                return Err(format!(
                    "sandbox read path {decl:?} resolves to {}, which is not allowed",
                    c.display()
                ))
            }
            Err(_) => out.warnings.push(format!(
                "sandbox read path {decl:?} does not exist, skipped"
            )),
        }
    }
    for (index, s) in p.sandbox.seed.iter().enumerate() {
        let from = allowed("seed source", &s.from)?;
        let to = PathBuf::from(&s.to);
        plain_relative("seed destination", &to)?;
        if s.write_back && s.json_keys.is_some() {
            return Err(format!(
                "seed {:?}: write_back and json_keys cannot be combined",
                s.from
            ));
        }
        if s.fresh
            .as_ref()
            .is_some_and(|f| f.json_key.is_empty() || f.max_age_days == 0)
        {
            return Err(format!("seed {:?}: invalid fresh rule", s.from));
        }
        if s.same.iter().any(|p| p.split('.').any(str::is_empty)) {
            return Err(format!("seed {:?}: invalid same path", s.from));
        }
        out.seeds.push(ResolvedSeed {
            root: home.clone(),
            from,
            to,
            json_keys: s.json_keys.clone(),
            fresh: s.fresh.clone(),
            write_back: s.write_back,
            same: s.same.clone(),
            decl: s.from.clone(),
            agent: p.command.first().cloned().unwrap_or_else(|| p.id.clone()),
            profile: p.id.clone(),
            index,
        });
    }
    if let Some(f) = &p.secrets.file {
        out.secrets = Some(under_home("secrets file", f, &home)?);
    }
    for f in &p.secrets.files {
        out.secret_files.push(resolve_secret_file(f, &home)?);
    }
    Ok(out)
}

const AGENTS_AREA: &str = "~/.config/tmgr/agents/";
const MAX_CREDENTIAL_BYTES: u64 = 64 * 1024;

fn resolve_secret_file(f: &SecretFile, home: &Path) -> Result<ResolvedSecretFile, String> {
    let rel = f
        .from
        .strip_prefix(AGENTS_AREA)
        .ok_or_else(|| format!("credential file {:?} must be inside {AGENTS_AREA}", f.from))?;
    plain_relative("credential file", Path::new(rel))?;
    let from = home.join(".config/tmgr/agents").join(rel);
    let to = PathBuf::from(&f.to);
    plain_relative("credential destination", &to)?;
    match f.template.as_deref() {
        None | Some("raw") | Some("claude-oauth") => {}
        Some(t) => {
            return Err(format!(
                "credential file {:?}: unknown template {t:?}",
                f.from
            ))
        }
    }
    if f.token_prefix.as_deref().is_some_and(|p| !token_charset(p)) {
        return Err(format!(
            "credential file {:?}: invalid token_prefix",
            f.from
        ));
    }
    Ok(ResolvedSecretFile {
        root: home.to_path_buf(),
        from,
        to,
        token_prefix: f.token_prefix.clone(),
        template: f.template.clone(),
    })
}

fn token_charset(t: &str) -> bool {
    t.bytes()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
}

pub fn render_credential(
    f: &ResolvedSecretFile,
    token: &str,
    now_ms: u64,
) -> Result<Vec<u8>, String> {
    if !(20..=400).contains(&token.len()) || !token_charset(token) {
        return Err("the token must be 20 to 400 characters of A-Z a-z 0-9 _ -".into());
    }
    if let Some(prefix) = &f.token_prefix {
        if !token.starts_with(prefix.as_str()) {
            return Err(format!("the token must start with {prefix}"));
        }
    }
    match f.template.as_deref() {
        Some("claude-oauth") => serde_json::to_vec(&serde_json::json!({"claudeAiOauth": {
            "accessToken": token,
            "refreshToken": null,
            "expiresAt": now_ms + 365 * 24 * 3600 * 1000,
            "scopes": ["user:inference"],
            "subscriptionType": null,
        }}))
        .map_err(|e| e.to_string()),
        _ => Ok(token.as_bytes().to_vec()),
    }
}

fn cstring(name: &OsStr) -> std::io::Result<CString> {
    CString::new(name.as_bytes())
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))
}

fn locked_agents_dir(f: &ResolvedSecretFile) -> std::io::Result<(File, File, std::ffi::OsString)> {
    let parent = f
        .from
        .parent()
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let name = f
        .from
        .file_name()
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?
        .to_os_string();
    let dir = open_dir_below(&f.root, parent)?;
    let mut lock_name = name.clone();
    lock_name.push(".lock");
    let c = cstring(&lock_name)?;
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            c.as_ptr(),
            libc::O_RDWR | libc::O_CREAT | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600 as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let lock = unsafe { File::from_raw_fd(fd) };
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok((dir, lock, name))
}

fn exists_at(dir: RawFd, name: &OsStr) -> std::io::Result<Option<bool>> {
    let c = cstring(name)?;
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstatat(dir, c.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) } == 0 {
        return Ok(Some((st.st_mode & libc::S_IFMT) == libc::S_IFDIR));
    }
    let e = std::io::Error::last_os_error();
    if e.kind() == std::io::ErrorKind::NotFound {
        Ok(None)
    } else {
        Err(e)
    }
}

pub fn write_credential(f: &ResolvedSecretFile, bytes: &[u8], replace: bool) -> Result<(), String> {
    let Some(parent) = f.from.parent() else {
        return Err("credential file has no parent directory".into());
    };
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    let fail = |e: std::io::Error| format!("cannot write {}: {e}", f.from.display());
    let (dir, _lock, name) = locked_agents_dir(f).map_err(fail)?;
    let dfd = dir.as_raw_fd();
    if !replace && exists_at(dfd, &name).map_err(fail)?.is_some() {
        return Err(format!(
            "{} already exists; use --replace to overwrite it",
            f.from.display()
        ));
    }
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(&name);
    tmp_name.push(format!(".tmgr-{}", std::process::id()));
    let tmp = cstring(&tmp_name).map_err(fail)?;
    let dest = cstring(&name).map_err(fail)?;
    let fd = unsafe {
        libc::openat(
            dfd,
            tmp.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600 as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(fail(std::io::Error::last_os_error()));
    }
    let mut file = unsafe { File::from_raw_fd(fd) };
    let done = file.write_all(bytes).and_then(|()| {
        if unsafe { libc::renameat(dfd, tmp.as_ptr(), dfd, dest.as_ptr()) } < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    });
    done.map_err(|e| {
        unsafe { libc::unlinkat(dfd, tmp.as_ptr(), 0) };
        fail(e)
    })
}

pub fn remove_credential(f: &ResolvedSecretFile) -> Result<bool, String> {
    let fail = |e: std::io::Error| format!("cannot remove {}: {e}", f.from.display());
    let (dir, _lock, name) = match locked_agents_dir(f) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(fail(e)),
    };
    let dfd = dir.as_raw_fd();
    match exists_at(dfd, &name).map_err(fail)? {
        None => return Ok(false),
        Some(true) => return Err(format!("{} is a directory", f.from.display())),
        Some(false) => {}
    }
    let c = cstring(&name).map_err(fail)?;
    if unsafe { libc::unlinkat(dfd, c.as_ptr(), 0) } < 0 {
        let e = std::io::Error::last_os_error();
        if e.kind() == std::io::ErrorKind::NotFound {
            return Ok(false);
        }
        return Err(fail(e));
    }
    Ok(true)
}

fn credential_bytes(f: &ResolvedSecretFile) -> Result<Option<Vec<u8>>, String> {
    let mut file = match open_below(&f.root, &f.from) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) if is_symlink_error(&e) => {
            return Err(format!(
                "credential file {} is or passes through a symlink, refusing to copy it",
                f.from.display()
            ))
        }
        Err(e) => {
            return Err(format!(
                "cannot open credential file {}: {e}",
                f.from.display()
            ))
        }
    };
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if let Some(problem) = private_file_problem(&f.from, &meta) {
        return Err(format!("credential file: {problem}"));
    }
    if meta.len() > MAX_CREDENTIAL_BYTES {
        return Err(format!(
            "credential file {} is larger than {MAX_CREDENTIAL_BYTES} bytes",
            f.from.display()
        ));
    }
    let mut buf = Vec::new();
    (&mut file)
        .take(MAX_CREDENTIAL_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("cannot read credential file {}: {e}", f.from.display()))?;
    if buf.len() as u64 > MAX_CREDENTIAL_BYTES {
        return Err(format!(
            "credential file {} is larger than {MAX_CREDENTIAL_BYTES} bytes",
            f.from.display()
        ));
    }
    Ok(Some(buf))
}

fn write_new_below(home: &Path, rel: &Path, bytes: &[u8]) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("cannot write {}: {e}", home.join(rel).display());
    let names: Vec<&OsStr> = rel
        .components()
        .map(|c| match c {
            Component::Normal(n) => Ok(n),
            _ => Err(std::io::Error::from(std::io::ErrorKind::InvalidInput)),
        })
        .collect::<Result<_, _>>()
        .map_err(fail)?;
    let Some((last, dirs)) = names.split_last() else {
        return Err(fail(std::io::Error::from(std::io::ErrorKind::InvalidInput)));
    };
    let dir_flags = libc::O_RDONLY | libc::O_DIRECTORY;
    let mut cur = openat_nofollow(libc::AT_FDCWD, home.as_os_str(), dir_flags).map_err(fail)?;
    for name in dirs {
        let c = CString::new(name.as_bytes())
            .map_err(|_| fail(std::io::Error::from(std::io::ErrorKind::InvalidInput)))?;
        let made = unsafe { libc::mkdirat(cur.as_raw_fd(), c.as_ptr(), 0o700) };
        if made < 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists {
            return Err(fail(std::io::Error::last_os_error()));
        }
        cur = openat_nofollow(cur.as_raw_fd(), name, dir_flags).map_err(fail)?;
    }
    let c = CString::new(last.as_bytes())
        .map_err(|_| fail(std::io::Error::from(std::io::ErrorKind::InvalidInput)))?;
    let fd = unsafe {
        libc::openat(
            cur.as_raw_fd(),
            c.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600 as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(fail(std::io::Error::last_os_error()));
    }
    unsafe { File::from_raw_fd(fd) }
        .write_all(bytes)
        .map_err(fail)
}

pub fn apply_secret_files(
    files: &[ResolvedSecretFile],
    home: &Path,
) -> Result<Vec<String>, String> {
    let mut notes = Vec::new();
    for f in files {
        let Some(bytes) = credential_bytes(f)? else {
            notes.push(format!(
                "credential file {} does not exist; the agent starts logged out",
                f.from.display()
            ));
            continue;
        };
        write_new_below(home, &f.to, &bytes)?;
    }
    Ok(notes)
}

fn push_new(out: &mut Vec<PathBuf>, p: PathBuf) {
    if !out.contains(&p) {
        out.push(p);
    }
}

const MAX_LINKS: u32 = 40;

fn link_is_unlinkable(
    parent: Option<&std::fs::Metadata>,
    link: &std::fs::Metadata,
    euid: u32,
) -> bool {
    let Some(parent) = parent else { return true };
    link_unlinkable_by(parent.mode(), parent.uid(), link.uid(), euid)
}

fn link_unlinkable_by(parent_mode: u32, parent_uid: u32, link_uid: u32, euid: u32) -> bool {
    let writable = parent_mode & 0o022 != 0 || parent_uid == euid;
    if !writable {
        return false;
    }
    let sticky = parent_mode & 0o1000 != 0;
    !sticky || link_uid == euid || parent_uid == euid
}

fn push_chain(out: &mut Vec<PathBuf>, path: &Path) -> Result<(), String> {
    let mut links = MAX_LINKS;
    let mut cur = PathBuf::from("/");
    let mut pending: std::collections::VecDeque<std::ffi::OsString> = VecDeque::new();
    let split = |p: &Path, q: &mut VecDeque<std::ffi::OsString>, front: bool| {
        let parts: Vec<_> = p
            .components()
            .map(|c| match c {
                Component::RootDir => "/".into(),
                Component::CurDir => ".".into(),
                Component::ParentDir => "..".into(),
                Component::Normal(n) => n.to_os_string(),
                Component::Prefix(_) => std::ffi::OsString::new(),
            })
            .collect();
        if front {
            for x in parts.into_iter().rev() {
                q.push_front(x);
            }
        } else {
            q.extend(parts);
        }
    };
    split(path, &mut pending, false);
    while let Some(c) = pending.pop_front() {
        if c == "/" {
            cur = PathBuf::from("/");
        } else if c == "." || c.is_empty() {
        } else if c == ".." {
            cur.pop();
        } else {
            cur.push(&c);
            let Ok(meta) = std::fs::symlink_metadata(&cur) else {
                continue;
            };
            if !meta.file_type().is_symlink() {
                continue;
            }
            if links == 0 {
                return Err(format!(
                    "too many symbolic links while resolving {}",
                    path.display()
                ));
            }
            links -= 1;
            let parent_meta = cur.parent().and_then(|d| std::fs::metadata(d).ok());
            if link_is_unlinkable(parent_meta.as_ref(), &meta, unsafe { libc::geteuid() }) {
                push_new(out, cur.clone());
            }
            let target = std::fs::read_link(&cur)
                .map_err(|e| format!("cannot read the link {}: {e}", cur.display()))?;
            cur.pop();
            split(&target, &mut pending, true);
        }
    }
    push_new(out, cur);
    Ok(())
}

pub fn path_chain(path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    push_chain(&mut out, &abs)?;
    Ok(out)
}

pub fn profile_protected(
    dir: Option<&Path>,
    secret_decls: &[String],
    real_home: &Path,
) -> Result<Vec<PathBuf>, String> {
    let home = canon_lossy(real_home);
    let mut out = Vec::new();
    if let Some(dir) = dir {
        let abs = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
        push_chain(&mut out, &abs)?;
        push_new(&mut out, canon_lossy(&abs));
    }
    for decl in secret_decls {
        let Ok(file) = under_home("secrets file", decl, &home) else {
            continue;
        };
        if let Some(rel) = decl.strip_prefix("~/") {
            push_chain(&mut out, &real_home.join(rel))?;
        }
        let guard = match file.parent() {
            Some(parent) if !home.starts_with(parent) => parent.to_path_buf(),
            _ => file,
        };
        push_new(&mut out, guard);
    }
    Ok(out)
}

fn open_nofollow(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
}

pub(crate) fn openat_nofollow(
    dir: RawFd,
    name: &OsStr,
    flags: libc::c_int,
) -> std::io::Result<File> {
    let c = CString::new(name.as_bytes())
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let fd = unsafe { libc::openat(dir, c.as_ptr(), flags | libc::O_NOFOLLOW | libc::O_CLOEXEC) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

pub(crate) fn open_dir_below(root: &Path, dir: &Path) -> std::io::Result<File> {
    let rel = dir
        .strip_prefix(root)
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let mut cur = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(root)?;
    for c in rel.components() {
        let Component::Normal(name) = c else {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
        };
        cur = openat_nofollow(cur.as_raw_fd(), name, libc::O_RDONLY | libc::O_DIRECTORY)?;
    }
    Ok(cur)
}

pub(crate) fn open_below(root: &Path, path: &Path) -> std::io::Result<File> {
    let names: Vec<&OsStr> = match path.strip_prefix(root) {
        Ok(rel) => rel
            .components()
            .map(|c| match c {
                Component::Normal(n) => Ok(n),
                _ => Err(std::io::Error::from(std::io::ErrorKind::InvalidInput)),
            })
            .collect::<Result<_, _>>()?,
        Err(_) => return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput)),
    };
    let Some((last, dirs)) = names.split_last() else {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    };
    let mut dir = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(root)?;
    for name in dirs {
        dir = openat_nofollow(dir.as_raw_fd(), name, libc::O_RDONLY | libc::O_DIRECTORY)?;
    }
    openat_nofollow(dir.as_raw_fd(), last, libc::O_RDONLY | libc::O_NONBLOCK)
}

pub(crate) fn is_symlink_error(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR))
}

fn private_file_problem(path: &Path, meta: &std::fs::Metadata) -> Option<String> {
    if !meta.is_file() {
        return Some(format!("{} is not a regular file", path.display()));
    }
    if meta.uid() != unsafe { libc::geteuid() } {
        return Some(format!(
            "{} is not owned by the current user",
            path.display()
        ));
    }
    if meta.mode() & 0o077 != 0 {
        return Some(format!(
            "{} is readable by other users; run: chmod 600 {}",
            path.display(),
            path.display()
        ));
    }
    None
}

pub fn read_secrets(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut f = match open_nofollow(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) if is_symlink_error(&e) => {
            return Err(format!(
                "secrets file {} is a symlink, refusing to read it",
                path.display()
            ))
        }
        Err(e) => return Err(format!("cannot open secrets file {}: {e}", path.display())),
    };
    let meta = f.metadata().map_err(|e| e.to_string())?;
    if let Some(problem) = private_file_problem(path, &meta) {
        return Err(format!("secrets file: {problem}"));
    }
    if meta.len() > MAX_SECRETS_BYTES {
        return Err(format!(
            "secrets file {} is larger than {MAX_SECRETS_BYTES} bytes",
            path.display()
        ));
    }
    let mut text = String::new();
    f.read_to_string(&mut text)
        .map_err(|e| format!("cannot read secrets file {}: {e}", path.display()))?;
    let mut out = BTreeMap::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let (name, value) = line.split_once('=').ok_or_else(|| {
            format!(
                "secrets file {} line {}: expected NAME=VALUE",
                path.display(),
                i + 1
            )
        })?;
        let name = name.trim();
        if !valid_env_name(name) {
            return Err(format!(
                "secrets file {} line {}: invalid variable name",
                path.display(),
                i + 1
            ));
        }
        let value = value.trim();
        let value = ['"', '\'']
            .iter()
            .find_map(|q| value.strip_prefix(*q).and_then(|v| v.strip_suffix(*q)))
            .unwrap_or(value);
        if !value.is_empty() && !value.contains('\0') {
            out.insert(name.to_string(), value.to_string());
        }
    }
    Ok(out)
}

pub fn prepare(
    p: &Profile,
    real_home: &Path,
    protected: &[PathBuf],
    runner_env: impl Fn(&str) -> Option<String>,
) -> Result<Prepared, String> {
    let r = resolve(p, real_home, protected)?;
    let secrets = match &r.secrets {
        Some(path) => read_secrets(path)?,
        None => BTreeMap::new(),
    };
    let pass_values = p
        .env_pass
        .iter()
        .filter_map(|n| {
            secrets
                .get(n)
                .cloned()
                .or_else(|| runner_env(n))
                .map(|v| (n.clone(), v))
        })
        .collect();
    Ok(Prepared {
        grants: Grants {
            extra_read: r.read,
            objects_ro: Vec::new(),
            deny: Vec::new(),
            trusted_env_pass: p.env_pass.clone(),
            env: p.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        },
        secret_files: r.secret_files,
        seeds: r.seeds,
        pass_values,
        warnings: r.warnings,
    })
}

fn seed_bytes(seed: &ResolvedSeed) -> Result<Option<Vec<u8>>, String> {
    let mut f = match open_below(&seed.root, &seed.from) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) if is_symlink_error(&e) => {
            return Err(format!(
                "seed source {} is or passes through a symlink, refusing to copy it",
                seed.from.display()
            ))
        }
        Err(e) => {
            return Err(format!(
                "cannot open seed source {}: {e}",
                seed.from.display()
            ))
        }
    };
    let meta = f.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err(format!(
            "seed source {} is not a regular file",
            seed.from.display()
        ));
    }
    if meta.len() > MAX_SEED_BYTES {
        return Err(format!(
            "seed source {} is larger than {MAX_SEED_BYTES} bytes",
            seed.from.display()
        ));
    }
    let mut buf = Vec::new();
    (&mut f)
        .take(MAX_SEED_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("cannot read seed source {}: {e}", seed.from.display()))?;
    if buf.len() as u64 > MAX_SEED_BYTES {
        return Err(format!(
            "seed source {} is larger than {MAX_SEED_BYTES} bytes",
            seed.from.display()
        ));
    }
    if let Some(rule) = &seed.fresh {
        seedguard::check_fresh(&buf, rule, chrono::Utc::now(), &seed.agent, &seed.decl)?;
    }
    let Some(keys) = &seed.json_keys else {
        return Ok(Some(buf));
    };
    let serde_json::Value::Object(map) = serde_json::from_slice(&buf)
        .map_err(|_| format!("seed source {} is not valid JSON", seed.from.display()))?
    else {
        return Err(format!(
            "seed source {} is not a JSON object",
            seed.from.display()
        ));
    };
    let kept: serde_json::Map<String, serde_json::Value> =
        map.into_iter().filter(|(k, _)| keys.contains(k)).collect();
    serde_json::to_vec(&kept)
        .map(Some)
        .map_err(|e| e.to_string())
}

pub fn apply_seeds(seeds: &[ResolvedSeed], home: &Path) -> Result<Vec<SeedRecord>, String> {
    let mut records = Vec::new();
    for seed in seeds {
        let Some(bytes) = seed_bytes(seed)? else {
            continue;
        };
        write_new_below(home, &seed.to, &bytes)?;
        if seed.write_back {
            records.push(SeedRecord {
                root: seed.root.clone(),
                source: seed.from.clone(),
                home: home.to_path_buf(),
                to: seed.to.clone(),
                sha: seedguard::sha256_hex(&bytes),
                json_key: seed.fresh.as_ref().map(|f| f.json_key.clone()),
                same: seed.same.clone(),
                profile: seed.profile.clone(),
                index: seed.index,
            });
        }
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Profile, ProfileSet};
    use std::os::unix::fs::{symlink, PermissionsExt};

    struct Home {
        _t: tempfile::TempDir,
        path: PathBuf,
    }

    fn home() -> Home {
        let t = tempfile::Builder::new()
            .prefix("lg")
            .tempdir_in("/tmp")
            .unwrap();
        let path = t.path().canonicalize().unwrap().join("home");
        std::fs::create_dir(&path).unwrap();
        Home { _t: t, path }
    }

    fn profile(extra: &str) -> Profile {
        let text = format!("id = \"p\"\nname = \"P\"\ncommand = [\"p\"]\n{extra}");
        toml::from_str(&text).unwrap()
    }

    fn put(path: &Path, body: &str, mode: u32) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn declarations_resolve_under_the_home() {
        let h = home();
        std::fs::create_dir_all(h.path.join(".local/share/tool")).unwrap();
        let p = profile(
            r#"
[sandbox]
read = ["~/.local/share/tool", "~/.local/share/missing"]
[[sandbox.seed]]
from = "~/.tool.json"
to = ".tool.json"
json_keys = ["a"]
[secrets]
file = "~/.config/tmgr/agents/p.env"
"#,
        );
        let r = resolve(&p, &h.path, &[]).unwrap();
        assert_eq!(r.read, vec![h.path.join(".local/share/tool")]);
        assert_eq!(r.warnings.len(), 1);
        assert_eq!(r.seeds[0].from, h.path.join(".tool.json"));
        assert_eq!(r.secrets, Some(h.path.join(".config/tmgr/agents/p.env")));
    }

    #[test]
    fn builtin_profiles_declare_their_login_files() {
        let h = home();
        let (set, warnings) = ProfileSet::load(None);
        assert!(warnings.is_empty(), "{warnings:?}");
        let claude = resolve(set.get("claude-code").unwrap(), &h.path, &[]).unwrap();
        assert_eq!(claude.seeds[0].from, h.path.join(".claude.json"));
        assert!(claude.secrets.is_none());
        assert!(set.get("claude-code").unwrap().env_pass.is_empty());
        assert_eq!(
            claude.secret_files[0].from,
            h.path
                .join(".config/tmgr/agents/claude-code.credentials.json")
        );
        assert_eq!(
            claude.secret_files[0].to,
            PathBuf::from(".claude/.credentials.json")
        );
        let codex = resolve(set.get("codex").unwrap(), &h.path, &[]).unwrap();
        assert_eq!(codex.seeds[0].to, PathBuf::from(".codex/auth.json"));
        assert!(codex.seeds[0].json_keys.is_none());
    }

    #[test]
    fn bad_paths_are_rejected() {
        let h = home();
        let cases = [
            "[sandbox]\nread = [\"/etc\"]",
            "[sandbox]\nread = [\"~/../x\"]",
            "[sandbox]\nread = [\"~\"]",
            "[sandbox]\nread = [\"~/.ssh/x\"]",
            "[sandbox]\nread = [\"~/Library/Keychains/x\"]",
            "[sandbox]\nread = [\"~/.config\"]",
            "[[sandbox.seed]]\nfrom = \"~/.ssh/id\"\nto = \"x\"",
            "[[sandbox.seed]]\nfrom = \"~/a\"\nto = \"../x\"",
            "[[sandbox.seed]]\nfrom = \"~/a\"\nto = \"/x\"",
            "[secrets]\nfile = \"/tmp/x.env\"",
            "env_pass = [\"LD_PRELOAD\"]",
            "env_pass = [\"bad name\"]",
            "[env]\nNODE_OPTIONS = \"x\"",
        ];
        for extra in cases {
            assert!(resolve(&profile(extra), &h.path, &[]).is_err(), "{extra}");
        }
    }

    #[test]
    fn a_symlinked_profile_dir_is_protected_by_its_configured_path_too() {
        let h = home();
        let proj = h.path.join("proj");
        let private = h.path.join(".private-agents");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::create_dir_all(&private).unwrap();
        symlink(&private, proj.join("agents")).unwrap();
        let p = profile_protected(Some(&proj.join("agents")), &[], &h.path).unwrap();
        assert!(p.contains(&private), "{p:?}");
        assert!(p.contains(&proj.join("agents")), "{p:?}");
        assert!(!p.contains(&PathBuf::from("/")));
    }

    #[test]
    fn a_symlinked_secrets_directory_is_protected_by_its_configured_path_too() {
        let h = home();
        let proj = h.path.join("proj");
        let private = h.path.join(".private");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::create_dir_all(&private).unwrap();
        symlink(&private, proj.join("vault")).unwrap();
        let p = profile_protected(None, &["~/proj/vault/p.env".into()], &h.path).unwrap();
        assert!(p.contains(&private), "{p:?}");
        assert!(p.contains(&proj.join("vault")), "{p:?}");
    }

    #[test]
    fn a_rejected_profile_still_protects_its_secrets_location() {
        let h = home();
        let agents = h.path.join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(
            agents.join("p.toml"),
            "id = \"p\"\nname = \"P\"\ncommand = [\"p\"]\n[sandbox]\nread = [\"/etc\"]\n[secrets]\nfile = \"~/vault/p.env\"\n",
        )
        .unwrap();
        let (set, _) = ProfileSet::load(Some(&agents));
        assert!(set.get("p").is_none());
        let decls = crate::profile::secrets_declarations(Some(&agents)).unwrap();
        let protected = profile_protected(Some(&agents), &decls, &h.path).unwrap();
        assert!(protected.contains(&h.path.join("vault")), "{protected:?}");
        let q = profile("[sandbox]\nread = [\"~/vault\"]");
        assert!(resolve(&q, &h.path, &protected).is_err());
    }

    #[test]
    fn an_unparseable_profile_file_fails_the_inventory_closed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("broken.toml"), "id = [").unwrap();
        assert!(crate::profile::secrets_declarations(Some(dir.path())).is_err());
        assert!(crate::profile::secrets_declarations(Some(&dir.path().join("none"))).is_ok());
    }

    #[test]
    fn symlinks_are_protected_by_who_can_unlink_them() {
        let me = 501;
        assert!(!link_unlinkable_by(0o40755, 0, 0, me));
        assert!(link_unlinkable_by(0o40777, 0, 0, me));
        assert!(link_unlinkable_by(0o40755, me, 99, me));
        assert!(!link_unlinkable_by(0o41777, 0, 99, me));
        assert!(link_unlinkable_by(0o41777, 0, me, me));
        assert!(link_unlinkable_by(0o41777, me, 99, me));
        assert!(link_unlinkable_by(0o40555, me, me, me));
        assert!(link_unlinkable_by(0o40500, me, 99, me));
        assert!(!link_unlinkable_by(0o40555, 0, 0, me));
    }

    #[test]
    fn a_symlink_in_a_user_writable_dir_is_in_the_chain() {
        let t = tempfile::tempdir().unwrap();
        let base = t.path().canonicalize().unwrap();
        let real = base.join("real");
        std::fs::create_dir(&real).unwrap();
        let link = base.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(path_chain(&link.join("x")).unwrap().contains(&link));
    }

    #[test]
    fn a_chained_symlink_resolution_is_protected_at_every_step() {
        let h = home();
        let repo = h.path.join("repo");
        let other = h.path.join("other");
        let private = h.path.join(".private-agents");
        for d in [&repo, &other, &private] {
            std::fs::create_dir_all(d).unwrap();
        }
        symlink(&private, other.join("redirect")).unwrap();
        symlink(other.join("redirect"), repo.join("agents")).unwrap();
        let p = profile_protected(Some(&repo.join("agents")), &[], &h.path).unwrap();
        for want in [&private, &other.join("redirect"), &repo.join("agents")] {
            assert!(p.contains(want), "{want:?} in {p:?}");
        }
        let q = path_chain(&repo.join("agents/links.toml")).unwrap();
        assert!(q.contains(&other.join("redirect")), "{q:?}");
    }

    #[test]
    fn a_symlink_loop_fails_the_inventory() {
        let h = home();
        symlink(h.path.join("b"), h.path.join("a")).unwrap();
        symlink(h.path.join("a"), h.path.join("b")).unwrap();
        assert!(profile_protected(Some(&h.path.join("a")), &[], &h.path).is_err());
    }

    #[test]
    fn too_many_or_oversized_profile_files_fail_the_inventory_closed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("big.toml"), vec![b'#'; 70 * 1024]).unwrap();
        let e = crate::profile::secrets_declarations(Some(dir.path())).unwrap_err();
        assert!(e.contains("big.toml"), "{e}");
        let many = tempfile::tempdir().unwrap();
        for i in 0..101 {
            std::fs::write(many.path().join(format!("p{i}.toml")), "").unwrap();
        }
        let e = crate::profile::secrets_declarations(Some(many.path())).unwrap_err();
        assert!(e.contains("more than"), "{e}");
    }

    #[test]
    fn protected_dirs_are_rejected() {
        let h = home();
        let p = profile("[sandbox]\nread = [\"~/work/runner\"]");
        let protected = vec![h.path.join("work/runner")];
        assert!(resolve(&p, &h.path, &protected).is_err());
    }

    #[test]
    fn symlinked_read_dir_leaving_the_home_is_rejected() {
        let h = home();
        let other = tempfile::tempdir().unwrap();
        symlink(other.path(), h.path.join("link")).unwrap();
        let p = profile("[sandbox]\nread = [\"~/link\"]");
        assert!(resolve(&p, &h.path, &[]).is_err());
    }

    #[test]
    fn user_profiles_with_bad_declarations_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("bad.toml"),
            "id = \"bad\"\nname = \"B\"\ncommand = [\"b\"]\n[sandbox]\nread = [\"/etc\"]\n",
        )
        .unwrap();
        let (set, warnings) = ProfileSet::load(Some(dir.path()));
        assert!(set.get("bad").is_none());
        assert!(set.get("claude-code").is_some());
        assert!(
            warnings.iter().all(|w| !w.contains("built-in")),
            "{warnings:?}"
        );
    }

    #[test]
    fn guard_options_parse_and_reach_the_resolved_seed() {
        let h = home();
        put(&h.path.join(".a.json"), "{}", 0o600);
        let p = profile(
            "[[sandbox.seed]]\nfrom = \"~/.a.json\"\nto = \".a.json\"\nfresh = { json_key = \"last_refresh\", max_age_days = 7 }\nwrite_back = true\nsame = [\"tokens.account_id\"]\n",
        );
        let s = &resolve(&p, &h.path, &[]).unwrap().seeds[0];
        assert_eq!(s.fresh.as_ref().unwrap().max_age_days, 7);
        assert!(s.write_back);
        assert_eq!(s.same, ["tokens.account_id"]);
    }

    #[test]
    fn write_back_with_json_keys_is_rejected() {
        let h = home();
        let p = profile(
            "[[sandbox.seed]]\nfrom = \"~/.a.json\"\nto = \".a.json\"\nwrite_back = true\njson_keys = [\"a\"]\n",
        );
        let e = resolve(&p, &h.path, &[]).unwrap_err();
        assert!(e.contains("write_back") && e.contains("json_keys"), "{e}");
    }

    #[test]
    fn the_codex_profile_guards_its_login() {
        let (set, warnings) = ProfileSet::load(None);
        assert!(warnings.is_empty(), "{warnings:?}");
        let seed = &set.get("codex").unwrap().sandbox.seed[0];
        assert!(seed.write_back && seed.fresh.is_some());
        assert_eq!(seed.same, ["tokens.account_id", "auth_mode"]);
    }

    #[test]
    fn a_stale_login_refuses_the_seed_and_only_write_back_seeds_are_recorded() {
        let h = home();
        let old = (chrono::Utc::now() - chrono::Duration::days(9)).to_rfc3339();
        let recent = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        put(
            &h.path.join(".a.json"),
            &format!(r#"{{"last_refresh":"{old}","t":"fake-tok"}}"#),
            0o600,
        );
        let p = profile(
            "[[sandbox.seed]]\nfrom = \"~/.a.json\"\nto = \".a.json\"\nfresh = { json_key = \"last_refresh\", max_age_days = 7 }\n",
        );
        let r = resolve(&p, &h.path, &[]).unwrap();
        let run_home = h.path.join("run");
        std::fs::create_dir(&run_home).unwrap();
        let e = apply_seeds(&r.seeds, &run_home).unwrap_err();
        assert!(
            e.contains("due for a refresh") && !e.contains("fake-tok"),
            "{e}"
        );
        assert!(!run_home.join(".a.json").exists());
        put(
            &h.path.join(".a.json"),
            &format!(r#"{{"last_refresh":"{recent}"}}"#),
            0o600,
        );
        assert!(apply_seeds(&r.seeds, &run_home).unwrap().is_empty());
        let p = profile(
            "[[sandbox.seed]]\nfrom = \"~/.a.json\"\nto = \".b.json\"\nwrite_back = true\n",
        );
        let r = resolve(&p, &h.path, &[]).unwrap();
        assert_eq!(apply_seeds(&r.seeds, &run_home).unwrap().len(), 1);
    }

    #[test]
    fn a_symlinked_directory_in_the_run_home_stops_seed_writes() {
        let h = home();
        put(&h.path.join(".a.json"), r#"{"t":"fake-tok"}"#, 0o600);
        let outside = h.path.join("outside");
        std::fs::create_dir(&outside).unwrap();
        let run_home = h.path.join("run");
        std::fs::create_dir(&run_home).unwrap();
        symlink(&outside, run_home.join(".d")).unwrap();
        let p = profile("[[sandbox.seed]]\nfrom = \"~/.a.json\"\nto = \".d/auth.json\"\n");
        let r = resolve(&p, &h.path, &[]).unwrap();
        let e = apply_seeds(&r.seeds, &run_home).unwrap_err();
        assert!(!e.contains("fake-tok"), "{e}");
        assert!(std::fs::read_dir(&outside).unwrap().next().is_none());

        let nested = run_home.join(".n");
        std::fs::create_dir(&nested).unwrap();
        symlink(&outside, nested.join("deep")).unwrap();
        let p = profile("[[sandbox.seed]]\nfrom = \"~/.a.json\"\nto = \".n/deep/x/auth.json\"\n");
        let r = resolve(&p, &h.path, &[]).unwrap();
        assert!(apply_seeds(&r.seeds, &run_home).is_err());
        assert!(std::fs::read_dir(&outside).unwrap().next().is_none());

        let p = profile("[[sandbox.seed]]\nfrom = \"~/.a.json\"\nto = \".ok/a/auth.json\"\n");
        let r = resolve(&p, &h.path, &[]).unwrap();
        apply_seeds(&r.seeds, &run_home).unwrap();
        let meta = std::fs::metadata(run_home.join(".ok/a/auth.json")).unwrap();
        assert_eq!(meta.mode() & 0o777, 0o600);
        assert_eq!(
            std::fs::metadata(run_home.join(".ok/a")).unwrap().mode() & 0o777,
            0o700
        );
    }

    #[test]
    fn seeds_copy_only_selected_json_keys_privately() {
        let h = home();
        put(
            &h.path.join(".c.json"),
            r#"{"theme":"dark","mcpServers":{"x":1},"oauthAccount":{"id":"u"}}"#,
            0o644,
        );
        let p = profile(
            "[[sandbox.seed]]\nfrom = \"~/.c.json\"\nto = \"a/.c.json\"\njson_keys = [\"theme\", \"oauthAccount\", \"absent\"]\n",
        );
        let r = resolve(&p, &h.path, &[]).unwrap();
        let run_home = h.path.join("run");
        std::fs::create_dir(&run_home).unwrap();
        apply_seeds(&r.seeds, &run_home).unwrap();
        let dest = run_home.join("a/.c.json");
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&dest).unwrap()).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"theme":"dark","oauthAccount":{"id":"u"}})
        );
        let mode = std::fs::metadata(&dest).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn whole_file_seed_and_missing_source() {
        let h = home();
        put(&h.path.join(".codex/auth.json"), "{\"k\":1}", 0o600);
        let p = profile(
            "[[sandbox.seed]]\nfrom = \"~/.codex/auth.json\"\nto = \".codex/auth.json\"\n[[sandbox.seed]]\nfrom = \"~/.nothing\"\nto = \".nothing\"\n",
        );
        let r = resolve(&p, &h.path, &[]).unwrap();
        let run_home = h.path.join("run");
        std::fs::create_dir(&run_home).unwrap();
        apply_seeds(&r.seeds, &run_home).unwrap();
        assert_eq!(
            std::fs::read_to_string(run_home.join(".codex/auth.json")).unwrap(),
            "{\"k\":1}"
        );
        assert!(!run_home.join(".nothing").exists());
    }

    #[test]
    fn symlinked_and_oversized_seed_sources_are_refused() {
        let h = home();
        put(&h.path.join("real"), "{}", 0o600);
        symlink(h.path.join("real"), h.path.join("link")).unwrap();
        put(&h.path.join("big"), &"x".repeat(2 << 20), 0o600);
        let run_home = h.path.join("run");
        std::fs::create_dir(&run_home).unwrap();
        for name in ["link", "big"] {
            let p = profile(&format!(
                "[[sandbox.seed]]\nfrom = \"~/{name}\"\nto = \"out\"\n"
            ));
            let r = resolve(&p, &h.path, &[]).unwrap();
            assert!(apply_seeds(&r.seeds, &run_home).is_err(), "{name}");
        }
        assert!(!run_home.join("out").exists());
    }

    #[test]
    fn seed_source_with_a_parent_swapped_for_a_symlink_is_refused() {
        let h = home();
        put(&h.path.join("repo/config/token"), "fine", 0o600);
        put(&h.path.join("denied/token"), "TOPSECRET", 0o600);
        let run_home = h.path.join("run");
        std::fs::create_dir(&run_home).unwrap();
        let p = profile("[[sandbox.seed]]\nfrom = \"~/repo/config/token\"\nto = \"out\"\n");
        let r = resolve(&p, &h.path, &[]).unwrap();
        std::fs::remove_dir_all(h.path.join("repo/config")).unwrap();
        symlink(h.path.join("denied"), h.path.join("repo/config")).unwrap();
        let e = apply_seeds(&r.seeds, &run_home).unwrap_err();
        assert!(e.contains("symlink"), "{e}");
        assert!(!run_home.join("out").exists());
    }

    #[test]
    fn regular_seed_source_below_directories_is_copied() {
        let h = home();
        put(&h.path.join("repo/config/token"), "fine", 0o600);
        let run_home = h.path.join("run");
        std::fs::create_dir(&run_home).unwrap();
        let p = profile("[[sandbox.seed]]\nfrom = \"~/repo/config/token\"\nto = \"out\"\n");
        let r = resolve(&p, &h.path, &[]).unwrap();
        apply_seeds(&r.seeds, &run_home).unwrap();
        assert_eq!(
            std::fs::read_to_string(run_home.join("out")).unwrap(),
            "fine"
        );
    }

    #[test]
    fn secrets_file_must_be_private() {
        let h = home();
        let f = h.path.join("s.env");
        put(&f, "# c\nA_TOKEN=abc\nexport B=\"q v\"\nEMPTY=\n", 0o600);
        let m = read_secrets(&f).unwrap();
        assert_eq!(m.get("A_TOKEN").map(String::as_str), Some("abc"));
        assert_eq!(m.get("B").map(String::as_str), Some("q v"));
        assert!(!m.contains_key("EMPTY"));
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();
        let e = read_secrets(&f).unwrap_err();
        assert!(
            e.contains("chmod 600") && e.contains("s.env") && !e.contains("abc"),
            "{e}"
        );
        let l = h.path.join("l.env");
        symlink(&f, &l).unwrap();
        assert!(read_secrets(&l).is_err());
        assert!(read_secrets(&h.path.join("none.env")).unwrap().is_empty());
    }

    #[test]
    fn prepare_prefers_the_secrets_file_over_the_runner_environment() {
        let h = home();
        put(&h.path.join("s.env"), "T_TOKEN=from-file\n", 0o600);
        let p = profile(
            "env_pass = [\"T_TOKEN\", \"OTHER_KEY\", \"UNSET\"]\n[secrets]\nfile = \"~/s.env\"\n",
        );
        let prep = prepare(&p, &h.path, &[], |n| {
            (n != "UNSET").then(|| format!("env-{n}"))
        })
        .unwrap();
        assert_eq!(
            prep.pass_values,
            vec![
                ("T_TOKEN".to_string(), "from-file".to_string()),
                ("OTHER_KEY".to_string(), "env-OTHER_KEY".to_string()),
            ]
        );
    }

    const FAKE_TOKEN: &str = concat!("sk-", "ant-oat01-FAKEFAKEFAKEFAKEFAKE");

    fn cred_profile(extra: &str) -> Profile {
        profile(&format!(
            "[[secrets.files]]\nfrom = \"~/.config/tmgr/agents/c.json\"\nto = \".c/c.json\"\n{extra}"
        ))
    }

    #[test]
    fn credential_files_must_be_inside_the_agents_area_with_a_plain_destination() {
        let h = home();
        let bad = [
            "[[secrets.files]]\nfrom = \"~/other/c.json\"\nto = \"c\"",
            "[[secrets.files]]\nfrom = \"~/.config/tmgr/c.json\"\nto = \"c\"",
            "[[secrets.files]]\nfrom = \"~/.config/tmgr/agents/../x\"\nto = \"c\"",
            "[[secrets.files]]\nfrom = \"/etc/passwd\"\nto = \"c\"",
            "[[secrets.files]]\nfrom = \"~/.config/tmgr/agents/c\"\nto = \"../c\"",
            "[[secrets.files]]\nfrom = \"~/.config/tmgr/agents/c\"\nto = \"/c\"",
            "[[secrets.files]]\nfrom = \"~/.config/tmgr/agents/c\"\nto = \"c\"\ntemplate = \"nope\"",
            "[[secrets.files]]\nfrom = \"~/.config/tmgr/agents/c\"\nto = \"c\"\ntoken_prefix = \"a b\"",
        ];
        for extra in bad {
            assert!(resolve(&profile(extra), &h.path, &[]).is_err(), "{extra}");
        }
        let r = resolve(&cred_profile(""), &h.path, &[]).unwrap();
        assert_eq!(r.secret_files[0].to, PathBuf::from(".c/c.json"));
    }

    #[test]
    fn credential_files_are_copied_privately_and_a_missing_one_is_only_noted() {
        let h = home();
        put(
            &h.path.join(".config/tmgr/agents/c.json"),
            "{\"k\":1}",
            0o600,
        );
        let r = resolve(&cred_profile(""), &h.path, &[]).unwrap();
        let run_home = h.path.join("run");
        std::fs::create_dir(&run_home).unwrap();
        assert!(apply_secret_files(&r.secret_files, &run_home)
            .unwrap()
            .is_empty());
        let dest = run_home.join(".c/c.json");
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "{\"k\":1}");
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dest), 0o600);
        assert_eq!(mode(dest.parent().unwrap()), 0o700);
        std::fs::remove_file(h.path.join(".config/tmgr/agents/c.json")).unwrap();
        let other = h.path.join("run2");
        std::fs::create_dir(&other).unwrap();
        let notes = apply_secret_files(&r.secret_files, &other).unwrap();
        assert_eq!(notes.len(), 1);
        assert!(!other.join(".c").exists());
    }

    #[test]
    fn a_loose_symlinked_or_oversized_credential_file_refuses_the_session() {
        let h = home();
        let src = h.path.join(".config/tmgr/agents/c.json");
        let r = resolve(&cred_profile(""), &h.path, &[]).unwrap();
        let run_home = h.path.join("run");
        std::fs::create_dir(&run_home).unwrap();
        put(&src, "fake-secret-body", 0o644);
        let e = apply_secret_files(&r.secret_files, &run_home).unwrap_err();
        assert!(
            e.contains("chmod 600") && e.contains("c.json") && !e.contains("fake-secret-body"),
            "{e}"
        );
        std::fs::remove_file(&src).unwrap();
        put(&h.path.join("real"), "{}", 0o600);
        symlink(h.path.join("real"), &src).unwrap();
        let e = apply_secret_files(&r.secret_files, &run_home).unwrap_err();
        assert!(e.contains("symlink"), "{e}");
        std::fs::remove_file(&src).unwrap();
        put(&src, &"x".repeat(70 * 1024), 0o600);
        assert!(apply_secret_files(&r.secret_files, &run_home).is_err());
        assert!(!run_home.join(".c").exists());
    }

    #[test]
    fn the_claude_template_renders_the_credential_document() {
        let h = home();
        let (set, _) = ProfileSet::load(None);
        let r = resolve(set.get("claude-code").unwrap(), &h.path, &[]).unwrap();
        let f = &r.secret_files[0];
        let v: serde_json::Value =
            serde_json::from_slice(&render_credential(f, FAKE_TOKEN, 1_000).unwrap()).unwrap();
        let o = &v["claudeAiOauth"];
        assert_eq!(o["accessToken"], FAKE_TOKEN);
        assert!(o["refreshToken"].is_null() && o["subscriptionType"].is_null());
        assert_eq!(o["expiresAt"], 1_000u64 + 365 * 24 * 3600 * 1000);
        assert_eq!(o["scopes"], serde_json::json!(["user:inference"]));
        for bad in [
            concat!("sk-", "ant-api03-FAKEFAKEFAKEFAKEFAKE"),
            "sk-ant-oat01-short",
            "sk-ant-oat01-FAKE FAKEFAKEFAKEFAKE",
        ] {
            let e = render_credential(f, bad, 0).unwrap_err();
            assert!(!e.contains(bad), "{e}");
        }
        assert!(render_credential(f, &format!("sk-ant-oat01-{}", "A".repeat(400)), 0).is_err());
    }

    #[test]
    fn credentials_are_written_privately_replaced_only_on_request_and_removed() {
        let h = home();
        let r = resolve(&cred_profile("template = \"raw\"\n"), &h.path, &[]).unwrap();
        let f = &r.secret_files[0];
        write_credential(f, b"one", false).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&f.from), 0o600);
        assert_eq!(mode(f.from.parent().unwrap()), 0o700);
        assert!(write_credential(f, b"two", false)
            .unwrap_err()
            .contains("--replace"));
        assert_eq!(std::fs::read(&f.from).unwrap(), b"one");
        write_credential(f, b"two", true).unwrap();
        assert_eq!(std::fs::read(&f.from).unwrap(), b"two");
        assert_eq!(
            std::fs::read_dir(f.from.parent().unwrap()).unwrap().count(),
            2
        );
        assert!(remove_credential(f).unwrap());
        assert!(!remove_credential(f).unwrap());
    }

    #[test]
    fn concurrent_non_replace_logins_let_exactly_one_win_and_logout_stays_consistent() {
        let h = home();
        let r = resolve(&cred_profile("template = \"raw\"\n"), &h.path, &[]).unwrap();
        let f = std::sync::Arc::new(r.secret_files[0].clone());
        for round in 0..20 {
            let _ = remove_credential(&f);
            let wins: usize = (0..8)
                .map(|n| {
                    let f = f.clone();
                    std::thread::spawn(move || {
                        write_credential(&f, format!("login-{n}").as_bytes(), false).is_ok()
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|t| usize::from(t.join().unwrap()))
                .sum();
            assert_eq!(wins, 1, "round {round}: not exactly one login won");
            assert!(std::fs::read(&f.from).unwrap().starts_with(b"login-"));
        }
        assert!(remove_credential(&f).unwrap());
        let writers: Vec<_> = (0..4)
            .map(|n| {
                let f = f.clone();
                std::thread::spawn(move || {
                    (0..50)
                        .filter(|_| {
                            write_credential(&f, format!("login-{n}").as_bytes(), false).is_ok()
                        })
                        .count()
                })
            })
            .collect();
        let remover = {
            let f = f.clone();
            std::thread::spawn(move || (0..200).filter(|_| remove_credential(&f).unwrap()).count())
        };
        let created: usize = writers.into_iter().map(|t| t.join().unwrap()).sum();
        let removed = remover.join().unwrap();
        let present = usize::from(f.from.exists());
        assert_eq!(
            created,
            removed + present,
            "logins and logouts do not add up"
        );
    }
}
