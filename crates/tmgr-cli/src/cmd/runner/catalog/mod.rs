pub(crate) mod claude;
pub(crate) mod codex;
pub(crate) mod index;

use super::attach_consent::Reads;
use serde::Serialize;
use serde_json::Value;
use std::fs::{File, Metadata, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tmgr_runner::resume::valid_resume_id;
use tmgr_runner::sanitize::sanitize_text;
use tmgr_usage::paths::Env;

pub const HEAD_CAP: u64 = 256 * 1024;
pub const RECENT_MS: u64 = 60_000;
const TEXT_MAX: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Agent {
    #[serde(rename = "claude-code")]
    Claude,
    #[serde(rename = "codex")]
    Codex,
}

impl Agent {
    pub fn profile(self) -> &'static str {
        match self {
            Agent::Claude => "claude-code",
            Agent::Codex => "codex",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Live {
    No,
    Yes,
    Recent,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub agent: Agent,
    pub id: String,
    pub path: PathBuf,
    pub cwd: Option<String>,
    pub mtime_ms: u64,
    pub started_at: Option<u64>,
    pub title: Option<String>,
    pub first_message: Option<String>,
    pub git_branch: Option<String>,
    pub project: Option<String>,
    pub readable: bool,
    pub live: Live,
}

impl Entry {
    pub(crate) fn light(
        agent: Agent,
        id: String,
        path: PathBuf,
        meta: &Metadata,
        readable: bool,
    ) -> Entry {
        Entry {
            agent,
            id,
            path,
            cwd: None,
            mtime_ms: mtime_ms(meta),
            started_at: None,
            title: None,
            first_message: None,
            git_branch: None,
            project: None,
            readable,
            live: Live::No,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub agent: Option<Agent>,
    pub limit: Option<usize>,
    pub ids: Option<Vec<String>>,
}

#[derive(Debug, Default)]
pub struct Listing {
    pub entries: Vec<Entry>,
    pub hints: Vec<String>,
}

pub fn list(env: &Env, f: Filter) -> Listing {
    let reads = Reads::of(env);
    let mut light = scan(env, f.agent);
    if let Some(ids) = &f.ids {
        light.retain(|e| ids.contains(&e.id));
    }
    let ctx = Hydrate::new(env);
    let limit = f.limit.unwrap_or(usize::MAX);
    let mut entries = Vec::new();
    for mut e in light {
        if entries.len() >= limit {
            break;
        }
        if ctx.fill(&mut e) {
            entries.push(e);
        }
    }
    let scoped = Reads {
        claude: reads.claude || f.agent == Some(Agent::Codex),
        codex: reads.codex || f.agent == Some(Agent::Claude),
    };
    Listing {
        entries,
        hints: scoped.hint().map(str::to_string).into_iter().collect(),
    }
}

#[allow(dead_code)]
pub fn find(env: &Env, id: &str) -> Option<Entry> {
    if !valid_resume_id(id) {
        return None;
    }
    list(
        env,
        Filter {
            ids: Some(vec![id.to_string()]),
            ..Filter::default()
        },
    )
    .entries
    .into_iter()
    .next()
}

pub(crate) fn scan(env: &Env, agent: Option<Agent>) -> Vec<Entry> {
    let reads = Reads::of(env);
    let mut out = Vec::new();
    if agent != Some(Agent::Codex) {
        if let Some(root) = tmgr_usage::paths::claude_config_dir(env) {
            out.extend(claude::scan(&root, reads.claude));
        }
    }
    if agent != Some(Agent::Claude) {
        if let Some(root) = codex_home(env) {
            out.extend(codex::scan(&root, reads.codex));
        }
    }
    out.sort_by(|a, b| b.mtime_ms.cmp(&a.mtime_ms).then_with(|| a.id.cmp(&b.id)));
    out
}

pub(crate) fn codex_home(env: &Env) -> Option<PathBuf> {
    match env.vars.get("CODEX_HOME").filter(|v| !v.is_empty()) {
        Some(v) => Some(PathBuf::from(v)),
        None => env.home.as_ref().map(|h| h.join(".codex")),
    }
}

struct Hydrate {
    codex_root: Option<PathBuf>,
    claude_live: std::collections::HashMap<String, Option<String>>,
    codex_titles: std::cell::OnceCell<std::collections::HashMap<String, String>>,
    now: u64,
}

impl Hydrate {
    fn new(env: &Env) -> Hydrate {
        let reads = Reads::of(env);
        let claude_root = tmgr_usage::paths::claude_config_dir(env);
        Hydrate {
            claude_live: match (&claude_root, reads.claude) {
                (Some(r), true) => claude::live_sessions(r),
                _ => Default::default(),
            },
            codex_root: codex_home(env),
            codex_titles: std::cell::OnceCell::new(),
            now: now_ms(),
        }
    }

    fn fill(&self, e: &mut Entry) -> bool {
        if e.readable {
            let keep = match e.agent {
                Agent::Claude => claude::fill(e, &self.claude_live),
                Agent::Codex => {
                    let titles = self.codex_titles.get_or_init(|| {
                        self.codex_root
                            .as_deref()
                            .map(codex::titles)
                            .unwrap_or_default()
                    });
                    match &self.codex_root {
                        Some(root) => codex::fill(e, titles, root),
                        None => true,
                    }
                }
            };
            if !keep {
                return false;
            }
        }
        if e.live == Live::No && self.now.saturating_sub(e.mtime_ms) < RECENT_MS {
            e.live = Live::Recent;
        }
        true
    }
}

pub fn clean_line(s: &str, max: usize) -> String {
    let flat: String = s
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    let flat = sanitize_text(&flat);
    let mut words = flat.split_whitespace();
    let mut joined = words.next().unwrap_or("").to_string();
    for w in words {
        joined.push(' ');
        joined.push_str(w);
    }
    match joined.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &joined[..i]),
        None => joined,
    }
}

pub(crate) fn clean_text(s: &str) -> Option<String> {
    let c = clean_line(s, TEXT_MAX);
    (!c.is_empty()).then_some(c)
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

pub(crate) fn mtime_ms(meta: &Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64)
}

pub(crate) fn parse_ts(v: &Value) -> Option<u64> {
    let s = v.as_str()?;
    let t = chrono::DateTime::parse_from_rfc3339(s).ok()?;
    u64::try_from(t.timestamp_millis()).ok()
}

pub(crate) fn open_regular(path: &Path) -> io::Result<File> {
    let f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    if !f.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a regular file",
        ));
    }
    Ok(f)
}

pub(crate) fn read_head(path: &Path, cap: u64) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    open_regular(path)?.take(cap).read_to_end(&mut buf)?;
    Ok(buf)
}

pub(crate) fn json_lines(buf: &[u8]) -> impl Iterator<Item = Value> + '_ {
    buf.split(|b| *b == b'\n')
        .filter_map(|l| serde_json::from_slice::<Value>(l).ok())
}

pub(crate) fn subdirs_and_files(dir: &Path) -> Vec<(PathBuf, Metadata)> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.flatten()
        .filter_map(|e| {
            let p = e.path();
            let m = std::fs::symlink_metadata(&p).ok()?;
            (m.is_dir() || m.is_file()).then_some((p, m))
        })
        .collect()
}

pub(crate) fn uuid_stem(path: &Path) -> Option<&str> {
    path.file_stem()?.to_str()
}

#[cfg(test)]
pub(crate) mod testkit;

#[cfg(test)]
mod tests;
