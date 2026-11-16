use crate::lines::{first_line, for_each_line};
use crate::mask::mask;
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
}

impl Roots {
    pub fn from_env(env: &Env) -> Roots {
        let var = |k: &str| env.vars.get(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        Roots {
            claude: var("CLAUDE_CONFIG_DIR")
                .or_else(|| env.home.as_ref().map(|h| h.join(".claude"))),
            codex: var("CODEX_HOME").or_else(|| env.home.as_ref().map(|h| h.join(".codex"))),
        }
    }
}

impl Roots {
    pub fn for_home(home: &Path) -> Roots {
        Roots {
            claude: Some(home.join(".claude")),
            codex: Some(home.join(".codex")),
        }
    }
}

pub fn newest(roots: &Roots, agent: Agent) -> Option<PathBuf> {
    let mut files = Vec::new();
    match agent {
        Agent::Claude => {
            for project in read_dir(&roots.claude.as_ref()?.join("projects")) {
                files.extend(read_dir(&project));
            }
        }
        Agent::Codex => walk(&roots.codex.as_ref()?.join("sessions"), 5, &mut files),
    }
    files
        .into_iter()
        .filter(|f| match agent {
            Agent::Claude => f.extension().is_some_and(|e| e == "jsonl"),
            Agent::Codex => is_rollout(f),
        })
        .filter(|f| !f.components().any(|c| c.as_os_str() == "handoff"))
        .filter_map(|f| Some((fs::metadata(&f).ok()?.modified().ok()?, f)))
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
    let meta = fs::metadata(src).map_err(|e| format!("cannot read the session file: {e}"))?;
    if meta.len() > EXPORT_MAX_BYTES {
        return Err("the session file is too large to export".into());
    }
    let name = src.file_name().ok_or("the session file has no name")?;
    let dir = match agent {
        Agent::Claude => into_home.join(".claude").join("projects").join("handoff"),
        Agent::Codex => into_home.join(".codex").join("sessions").join("handoff"),
    };
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let dest = dir.join(name);
    let mut out = String::new();
    let file = fs::File::open(src).map_err(|e| format!("cannot open the session file: {e}"))?;
    for_each_line(BufReader::new(file), |line| {
        out.push_str(&mask(line));
        out.push('\n');
        true
    });
    write_private(&dest, out.as_bytes())?;
    Ok(dest)
}

fn write_private(path: &Path, data: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
        .and_then(|mut f| f.write_all(data))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
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
    if let Some(dir) = roots.claude.as_ref().map(|r| r.join("projects")) {
        for project in read_dir(&dir) {
            for f in read_dir(&project) {
                if matches(&f, id, false) {
                    found.push((Agent::Claude, f));
                }
            }
        }
    }
    if let Some(dir) = roots.codex.as_ref().map(|r| r.join("sessions")) {
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
    let file = fs::File::open(path).map_err(|e| format!("cannot open session file: {e}"))?;
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
    let first = fs::File::open(path)
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
        .map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default()
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    for p in read_dir(dir) {
        if p.is_dir() {
            if depth > 0 {
                walk(&p, depth - 1, out);
            }
        } else {
            out.push(p);
        }
    }
}
