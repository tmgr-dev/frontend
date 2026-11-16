use crate::lines::{first_line, for_each_line};
use crate::mask::mask_json_line;
use crate::model::{Agent, Session};
use crate::{claude, codex};
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use tmgr_usage::paths::Env;

#[derive(Debug, Clone)]
pub struct Roots {
    pub claude: Option<PathBuf>,
    pub codex: Option<PathBuf>,
    pub jail: Option<PathBuf>,
}

impl Roots {
    pub fn from_env(env: &Env) -> Roots {
        let var = |k: &str| env.vars.get(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        Roots {
            claude: var("CLAUDE_CONFIG_DIR")
                .or_else(|| env.home.as_ref().map(|h| h.join(".claude"))),
            codex: var("CODEX_HOME").or_else(|| env.home.as_ref().map(|h| h.join(".codex"))),
            jail: None,
        }
    }
}

impl Roots {
    pub fn for_home(home: &Path) -> Roots {
        Roots {
            claude: Some(home.join(".claude")),
            codex: Some(home.join(".codex")),
            jail: Some(home.to_path_buf()),
        }
    }

    fn contains(&self, dir: &Path) -> bool {
        match &self.jail {
            None => true,
            Some(jail) => match (fs::canonicalize(dir), fs::canonicalize(jail)) {
                (Ok(d), Ok(j)) => d.starts_with(j),
                _ => false,
            },
        }
    }
}

pub fn open_regular(path: &Path) -> Result<fs::File, String> {
    let mut opts = fs::OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW);
    }
    let file = opts
        .open(path)
        .map_err(|e| format!("cannot open the session file: {e}"))?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("not a regular file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() > 1 {
            return Err("the session file has several hard links".into());
        }
    }
    Ok(file)
}

pub fn newest(roots: &Roots, agent: Agent) -> Option<PathBuf> {
    let mut files = Vec::new();
    match agent {
        Agent::Claude => {
            let base = roots.claude.as_ref()?.join("projects");
            if !roots.contains(&base) {
                return None;
            }
            for project in read_dir(&base) {
                files.extend(read_dir(&project));
            }
        }
        Agent::Codex => {
            let base = roots.codex.as_ref()?.join("sessions");
            if !roots.contains(&base) {
                return None;
            }
            walk(&base, 5, &mut files)
        }
    }
    files
        .into_iter()
        .filter(|f| match agent {
            Agent::Claude => f.extension().is_some_and(|e| e == "jsonl"),
            Agent::Codex => is_rollout(f),
        })
        .filter(|f| !f.components().any(|c| c.as_os_str() == "handoff"))
        .filter_map(|f| Some((fs::symlink_metadata(&f).ok()?.modified().ok()?, f)))
        .max_by_key(|(t, _)| *t)
        .map(|(_, f)| f)
}

fn is_rollout(f: &Path) -> bool {
    f.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
}

pub const EXPORT_MAX_BYTES: u64 = 64 << 20;

pub fn export_masked(src: &Path, agent: Agent, into_home: &Path) -> Result<PathBuf, String> {
    export_counted(src, agent, into_home).map(|(p, _)| p)
}

pub fn export_counted(
    src: &Path,
    agent: Agent,
    into_home: &Path,
) -> Result<(PathBuf, usize), String> {
    let file = open_regular(src)?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if size > EXPORT_MAX_BYTES {
        return Err("the session file is too large to export".into());
    }
    let name = src.file_name().ok_or("the session file has no name")?;
    let home =
        fs::canonicalize(into_home).map_err(|e| format!("the new home is unavailable: {e}"))?;
    let parts: &[&str] = match agent {
        Agent::Claude => &[".claude", "projects", "handoff"],
        Agent::Codex => &[".codex", "sessions", "handoff"],
    };
    let mut out = String::new();
    let skipped = for_each_line(BufReader::new(file), |line| match mask_json_line(line) {
        Some(masked) => {
            out.push_str(&masked);
            out.push('\n');
            true
        }
        None => false,
    });
    let dest = write_in_home(&home, parts, name, out.as_bytes())?;
    Ok((dest, skipped))
}

#[cfg(unix)]
fn write_in_home(
    home: &Path,
    parts: &[&str],
    name: &std::ffi::OsStr,
    data: &[u8],
) -> Result<PathBuf, String> {
    use std::ffi::CString;
    use std::io::Write;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;

    fn c(b: &[u8]) -> Result<CString, String> {
        CString::new(b).map_err(|_| "a path contains a NUL byte".to_string())
    }
    fn err(what: &str) -> String {
        format!("{what}: {}", std::io::Error::last_os_error())
    }
    fn open_dir(at: libc::c_int, name: &CString) -> Result<OwnedFd, String> {
        let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        let fd = unsafe {
            if at == libc::AT_FDCWD {
                libc::open(name.as_ptr(), flags)
            } else {
                libc::openat(at, name.as_ptr(), flags)
            }
        };
        if fd < 0 {
            return Err(err("cannot open the folder"));
        }
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    let mut dir = open_dir(libc::AT_FDCWD, &c(home.as_os_str().as_bytes())?)?;
    for part in parts {
        let cp = c(part.as_bytes())?;
        dir = match open_dir(dir.as_raw_fd(), &cp) {
            Ok(fd) => fd,
            Err(_) if std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) => {
                if unsafe { libc::mkdirat(dir.as_raw_fd(), cp.as_ptr(), 0o700) } != 0 {
                    return Err(err(&format!("cannot create {part}")));
                }
                open_dir(dir.as_raw_fd(), &cp)
                    .map_err(|_| format!("{part} is not a plain folder"))?
            }
            Err(_) => return Err(format!("{part} is not a plain folder")),
        };
    }
    let tmp = c(format!(".{}.tmp{}", name.to_string_lossy(), std::process::id()).as_bytes())?;
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            tmp.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600 as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(err("cannot create the copy"));
    }
    let mut f = std::fs::File::from(unsafe { OwnedFd::from_raw_fd(fd) });
    let cname = c(name.as_bytes())?;
    let done = f
        .write_all(data)
        .map_err(|e| format!("cannot write the copy: {e}"));
    drop(f);
    let done = done.and_then(|_| {
        if unsafe {
            libc::renameat(
                dir.as_raw_fd(),
                tmp.as_ptr(),
                dir.as_raw_fd(),
                cname.as_ptr(),
            )
        } != 0
        {
            return Err(err("cannot place the copy"));
        }
        Ok(())
    });
    if let Err(e) = done {
        unsafe { libc::unlinkat(dir.as_raw_fd(), tmp.as_ptr(), 0) };
        return Err(e);
    }
    let mut dest = home.to_path_buf();
    dest.extend(parts);
    Ok(dest.join(name))
}

#[cfg(not(unix))]
fn write_in_home(
    home: &Path,
    parts: &[&str],
    name: &std::ffi::OsStr,
    data: &[u8],
) -> Result<PathBuf, String> {
    let mut dir = home.to_path_buf();
    for part in parts {
        dir.push(part);
        match fs::symlink_metadata(&dir) {
            Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
            Ok(_) => return Err(format!("{} is not a plain folder", dir.display())),
            Err(_) => {
                fs::create_dir(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?
            }
        }
    }
    let dest = dir.join(name);
    fs::write(&dest, data).map_err(|e| format!("cannot write {}: {e}", dest.display()))?;
    Ok(dest)
}

pub fn find(roots: &Roots, id: &str) -> Result<(Agent, PathBuf), String> {
    let path = Path::new(id);
    if id.contains('/') || id.contains('\\') || path.is_file() {
        return in_roots(roots, path);
    }
    if id.len() < 8 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(format!("not a session id or file: {id}"));
    }
    let mut found: Vec<(Agent, PathBuf)> = Vec::new();
    if let Some(dir) = roots
        .claude
        .as_ref()
        .map(|r| r.join("projects"))
        .filter(|d| roots.contains(d))
    {
        for project in read_dir(&dir) {
            for f in read_dir(&project) {
                if matches(&f, id, false) {
                    found.push((Agent::Claude, f));
                }
            }
        }
    }
    if let Some(dir) = roots
        .codex
        .as_ref()
        .map(|r| r.join("sessions"))
        .filter(|d| roots.contains(d))
    {
        let mut files = Vec::new();
        walk(&dir, 5, &mut files);
        found.extend(
            files
                .into_iter()
                .filter(|f| matches(f, id, true))
                .map(|f| (Agent::Codex, f)),
        );
    }
    match found.len() {
        0 => Err(format!("no Claude or Codex session found for {id}")),
        1 => Ok(found.remove(0)),
        n => Err(format!("{n} sessions match {id}; give more of the id")),
    }
}

fn in_roots(roots: &Roots, path: &Path) -> Result<(Agent, PathBuf), String> {
    let real = fs::canonicalize(path).map_err(|_| format!("no such file: {}", path.display()))?;
    let allowed = |root: &Option<PathBuf>, sub: &str| {
        root.as_ref()
            .and_then(|r| fs::canonicalize(r.join(sub)).ok())
            .is_some_and(|r| real.starts_with(r))
    };
    if !real.is_file() || real.extension().and_then(|e| e.to_str()) != Some("jsonl") {
        return Err("not a session file".into());
    }
    if allowed(&roots.claude, "projects") || allowed(&roots.codex, "sessions") {
        return Ok((sniff(&real), real));
    }
    Err("the file is outside the Claude and Codex session folders".into())
}

pub fn load(agent: Agent, path: &Path) -> Result<Session, String> {
    let file = open_regular(path)?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let reader = BufReader::new(file);
    Ok(match agent {
        Agent::Claude => claude::parse(reader, stem),
        Agent::Codex => codex::parse(reader, stem),
    })
}

fn matches(file: &Path, id: &str, codex: bool) -> bool {
    let Some(name) = file.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let Some(stem) = name.strip_suffix(".jsonl") else {
        return false;
    };
    if codex {
        stem.starts_with("rollout-")
            && stem.len() >= 36
            && stem.is_char_boundary(stem.len() - 36)
            && stem[stem.len() - 36..].starts_with(id)
    } else {
        stem.starts_with(id)
    }
}

fn sniff(path: &Path) -> Agent {
    let first = open_regular(path)
        .map(|f| first_line(BufReader::new(f)))
        .unwrap_or_default();
    if first.contains("\"session_meta\"") || first.contains("\"payload\"") {
        Agent::Codex
    } else {
        Agent::Claude
    }
}

fn read_dir(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|r| {
            r.filter_map(|e| {
                let e = e.ok()?;
                (!e.file_type().ok()?.is_symlink()).then(|| e.path())
            })
            .collect()
        })
        .unwrap_or_default()
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    for p in read_dir(dir) {
        if fs::symlink_metadata(&p).is_ok_and(|m| m.is_dir()) {
            if depth > 0 {
                walk(&p, depth - 1, out);
            }
        } else {
            out.push(p);
        }
    }
}
