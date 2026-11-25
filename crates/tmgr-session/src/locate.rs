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
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = files
        .into_iter()
        .filter(|f| match agent {
            Agent::Claude => f.extension().is_some_and(|e| e == "jsonl"),
            Agent::Codex => is_rollout(f),
        })
        .filter(|f| !f.components().any(|c| c.as_os_str() == "handoff"))
        .filter_map(|f| Some((fs::symlink_metadata(&f).ok()?.modified().ok()?, f)))
        .collect();
    files.sort_by_key(|a| std::cmp::Reverse(a.0));
    files
        .into_iter()
        .map(|(_, f)| f)
        .find(|f| agent != Agent::Claude || !is_child_transcript(f))
}

const CHILD_SCAN_LINES: usize = 64;
const CHILD_SCAN_BYTES: u64 = 256 << 10;
const PID_FILE_CAP: u64 = 64 << 10;
const PID_FILE_SLACK_MS: u64 = 2000;

fn is_child_transcript(path: &Path) -> bool {
    use std::io::{BufRead, Read};
    let Ok(file) = open_regular(path) else {
        return false;
    };
    let reader = BufReader::new(file.take(CHILD_SCAN_BYTES));
    for line in reader.split(b'\n').take(CHILD_SCAN_LINES).flatten() {
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&line) else {
            continue;
        };
        if v.get("teamName")
            .and_then(|t| t.as_str())
            .is_some_and(|t| !t.is_empty())
        {
            return true;
        }
        if v.get("type").and_then(|t| t.as_str()) == Some("derived-from")
            && v.get("sessions")
                .and_then(|s| s.as_array())
                .is_some_and(|a| {
                    a.iter()
                        .any(|e| e.get("via").and_then(|x| x.as_str()) == Some("subprocess"))
                })
        {
            return true;
        }
    }
    false
}

pub fn claude_pid_session(roots: &Roots, pid: u32, created_ms: u64) -> Option<String> {
    use std::io::Read;
    if pid == 0 {
        return None;
    }
    let dir = roots.claude.as_ref()?.join("sessions");
    if !roots.contains(&dir) {
        return None;
    }
    let file = open_regular(&dir.join(format!("{pid}.json"))).ok()?;
    let mtime = file.metadata().ok()?.modified().ok()?;
    let mtime_ms = mtime
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    if mtime_ms < created_ms.saturating_sub(PID_FILE_SLACK_MS) {
        return None;
    }
    let mut raw = Vec::new();
    file.take(PID_FILE_CAP).read_to_end(&mut raw).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    if v.get("pid").and_then(|p| p.as_u64()) != Some(u64::from(pid)) {
        return None;
    }
    let id = v.get("sessionId")?.as_str()?;
    crate::switch::is_uuid(id).then(|| id.to_string())
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
    fn open_dir(at: libc::c_int, name: &CString) -> Result<OwnedFd, std::io::Error> {
        let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        let fd = unsafe {
            if at == libc::AT_FDCWD {
                libc::open(name.as_ptr(), flags)
            } else {
                libc::openat(at, name.as_ptr(), flags)
            }
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    let mut dir = open_dir(libc::AT_FDCWD, &c(home.as_os_str().as_bytes())?)
        .map_err(|e| format!("cannot open the new home: {e}"))?;
    for part in parts {
        let cp = c(part.as_bytes())?;
        dir = match open_dir(dir.as_raw_fd(), &cp) {
            Ok(fd) => fd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
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

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: &str = "aaaaaaaa-0000-4000-8000-000000000001";
    const CHILD: &str = "bbbbbbbb-0000-4000-8000-000000000002";
    const USER: &str = "{\"parentUuid\":null,\"isSidechain\":false,\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hi\"},\"sessionId\":\"x\",\"version\":\"2.1.332\"}\n";

    fn put(dir: &Path, id: &str, body: &str, secs: u64) -> PathBuf {
        let path = dir.join(format!("{id}.jsonl"));
        fs::write(&path, body).unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
            .unwrap();
        path
    }

    fn setup() -> (tempfile::TempDir, PathBuf, Roots) {
        let home = tempfile::tempdir().unwrap();
        let project = home.path().join(".claude/projects/p");
        fs::create_dir_all(&project).unwrap();
        let roots = Roots::for_home(home.path());
        (home, project, roots)
    }

    #[test]
    fn a_fresher_teammate_transcript_is_not_picked() {
        let (_h, p, roots) = setup();
        let main = put(&p, MAIN, USER, 1000);
        let team = format!(
            "{{\"type\":\"mode\",\"mode\":\"normal\",\"sessionId\":\"{CHILD}\"}}\n{{\"parentUuid\":null,\"isSidechain\":false,\"teamName\":\"session-0000aaaa\",\"agentName\":\"worker\",\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":\"hi\"}},\"uuid\":\"u1\",\"timestamp\":\"2026-11-25T22:15:00.000Z\",\"sessionId\":\"{CHILD}\",\"version\":\"2.1.332\"}}\n"
        );
        put(&p, CHILD, &team, 2000);
        assert_eq!(newest(&roots, Agent::Claude).unwrap(), main);
    }

    #[test]
    fn a_fresher_subprocess_transcript_is_not_picked() {
        let (_h, p, roots) = setup();
        let main = put(&p, MAIN, USER, 1000);
        let child = format!(
            "{{\"type\":\"derived-from\",\"v\":1,\"sessionId\":\"{CHILD}\",\"sessions\":[{{\"type\":\"claude_code_session\",\"id\":\"{MAIN}\",\"via\":\"subprocess\",\"started\":\"{CHILD}\"}}]}}\n{USER}"
        );
        put(&p, CHILD, &child, 2000);
        assert_eq!(newest(&roots, Agent::Claude).unwrap(), main);
    }

    #[test]
    fn a_resumed_or_compacted_main_transcript_is_still_picked() {
        for via in ["resume", "compact"] {
            let (_h, p, roots) = setup();
            put(&p, MAIN, USER, 1000);
            let fresh = format!(
                "{{\"type\":\"derived-from\",\"v\":1,\"sessionId\":\"{CHILD}\",\"sessions\":[{{\"type\":\"claude_code_session\",\"id\":\"{MAIN}\",\"via\":\"{via}\"}}]}}\n{USER}"
            );
            let path = put(&p, CHILD, &fresh, 2000);
            assert_eq!(newest(&roots, Agent::Claude).unwrap(), path);
        }
    }

    #[test]
    fn the_pid_file_needs_a_matching_pid_a_fresh_mtime_and_a_uuid() {
        let (h, _p, roots) = setup();
        let dir = h.path().join(".claude/sessions");
        fs::create_dir_all(&dir).unwrap();
        let write = |pid: u32, body: String, secs: u64| {
            put_json(&dir.join(format!("{pid}.json")), &body, secs)
        };
        write(7, format!("{{\"pid\":7,\"sessionId\":\"{MAIN}\"}}"), 1000);
        assert_eq!(
            claude_pid_session(&roots, 7, 1_000_000).as_deref(),
            Some(MAIN)
        );
        assert_eq!(claude_pid_session(&roots, 0, 1_000_000), None);
        assert_eq!(claude_pid_session(&roots, 7, 1_000_000_000), None);
        write(8, format!("{{\"pid\":9,\"sessionId\":\"{MAIN}\"}}"), 1000);
        assert_eq!(claude_pid_session(&roots, 8, 1_000_000), None);
        write(10, "{\"pid\":10,\"sessionId\":\"../x\"}".into(), 1000);
        assert_eq!(claude_pid_session(&roots, 10, 1_000_000), None);
        write(11, format!("{{\"sessionId\":\"{MAIN}\"}}"), 1000);
        assert_eq!(claude_pid_session(&roots, 11, 1_000_000), None);
    }

    fn put_json(path: &Path, body: &str, secs: u64) {
        fs::write(path, body).unwrap();
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
            .unwrap();
    }
}
