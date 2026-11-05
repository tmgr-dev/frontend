use std::path::{Path, PathBuf};
use toml_edit::{value, DocumentMut};

const FILE: &str = "run-menu.toml";
const SENDS: &str = "smart-search.toml";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    New,
    Resume,
    Fork,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::New => "new",
            Kind::Resume => "resume",
            Kind::Fork => "fork",
        }
    }

    fn parse(s: &str) -> Kind {
        match s {
            "resume" => Kind::Resume,
            "fork" => Kind::Fork,
            _ => Kind::New,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Memory {
    pub agent: String,
    pub kind: Kind,
    pub bypass: bool,
    pub no_sandbox: bool,
}

pub fn default_path() -> Option<PathBuf> {
    tmgr_core::config::config_dir().ok().map(|d| d.join(FILE))
}

pub fn load(path: &Path) -> Memory {
    let Some(doc) = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.parse::<DocumentMut>().ok())
    else {
        return Memory::default();
    };
    Memory {
        agent: doc
            .get("agent")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        kind: Kind::parse(doc.get("kind").and_then(|v| v.as_str()).unwrap_or_default()),
        bypass: doc.get("bypass").and_then(|v| v.as_bool()).unwrap_or(false),
        no_sandbox: doc
            .get("no_sandbox")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    }
}

fn write_doc(path: &Path, doc: &DocumentMut) -> std::io::Result<()> {
    use std::io::Write;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let _ = tmgr_core::config::ensure_private_dir(dir);
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(doc.to_string().as_bytes())?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

pub fn save(path: &Path, m: &Memory) -> std::io::Result<()> {
    let mut doc = DocumentMut::new();
    doc["agent"] = value(m.agent.as_str());
    doc["kind"] = value(m.kind.as_str());
    doc["bypass"] = value(m.bypass);
    doc["no_sandbox"] = value(m.no_sandbox);
    write_doc(path, &doc)
}

pub fn sends_path() -> Option<PathBuf> {
    tmgr_core::config::config_dir().ok().map(|d| d.join(SENDS))
}

pub fn load_send(path: &Path, direction: &str) -> Option<bool> {
    std::fs::read_to_string(path)
        .ok()?
        .parse::<DocumentMut>()
        .ok()?
        .get(direction)?
        .as_bool()
}

pub fn save_send(path: &Path, direction: &str, allow: bool) -> std::io::Result<()> {
    let mut doc = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.parse::<DocumentMut>().ok())
        .unwrap_or_default();
    doc[direction] = value(allow);
    write_doc(path, &doc)
}
