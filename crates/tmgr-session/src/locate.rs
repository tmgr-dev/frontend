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

pub fn find(roots: &Roots, id: &str) -> Result<(Agent, PathBuf), String> {
    let path = Path::new(id);
    if path.is_file() {
        return Ok((sniff(path), path.to_path_buf()));
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
        stem.starts_with("rollout-") && stem.contains(id)
    } else {
        stem.starts_with(id)
    }
}

fn sniff(path: &Path) -> Agent {
    let first = fs::read_to_string(path)
        .ok()
        .and_then(|s| s.lines().next().map(str::to_string))
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
