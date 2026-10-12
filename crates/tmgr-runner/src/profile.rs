use regex::{Regex, RegexBuilder};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const MAX_PROFILE_BYTES: u64 = 64 * 1024;
const MAX_PROFILE_FILES: usize = 100;
const REGEX_LIMIT: usize = 1 << 20;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Source {
    #[default]
    Builtin,
    User(PathBuf),
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct StatusRules {
    #[serde(default)]
    pub blocked: Vec<String>,
    #[serde(default)]
    pub idle: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub command: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub model_arg: Vec<String>,
    #[serde(default)]
    pub env_pass: Vec<String>,
    #[serde(default)]
    pub status: StatusRules,
    #[serde(skip)]
    pub source: Source,
}

pub fn valid_profile_id(id: &str) -> bool {
    let b = id.as_bytes();
    (1..=32).contains(&b.len())
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

fn valid_model(model: &str) -> bool {
    (1..=64).contains(&model.len())
        && model
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b':' | b'/' | b'-'))
}

impl Profile {
    pub fn argv(&self, model: Option<&str>) -> Result<Vec<String>, String> {
        let mut argv: Vec<String> = self.command.iter().chain(&self.args).cloned().collect();
        if let Some(model) = model {
            if !valid_model(model) {
                return Err(format!("invalid model value {model:?}"));
            }
            argv.extend(self.model_arg.iter().map(|a| a.replace("{model}", model)));
        }
        Ok(argv)
    }
}

pub fn user_dir() -> Option<PathBuf> {
    let var = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if let Some(dir) = var("TMGR_AGENTS_DIR") {
        return Some(dir);
    }
    if let Some(xdg) = var("XDG_CONFIG_HOME") {
        return Some(xdg.join("tmgr").join("agents"));
    }
    var("HOME").map(|h| h.join(".config").join("tmgr").join("agents"))
}

const BUILTINS: [&str; 2] = [
    include_str!("../profiles/claude-code.toml"),
    include_str!("../profiles/codex.toml"),
];

pub struct ProfileSet {
    profiles: BTreeMap<String, Profile>,
}

impl ProfileSet {
    pub fn load(user_dir: Option<&Path>) -> (ProfileSet, Vec<String>) {
        let mut warnings = Vec::new();
        let mut profiles = BTreeMap::new();
        for text in BUILTINS {
            match toml::from_str::<Profile>(text) {
                Ok(p) => {
                    profiles.insert(p.id.clone(), p);
                }
                Err(e) => warnings.push(format!("built-in profile is invalid: {e}")),
            }
        }
        if let Some(dir) = user_dir {
            load_user(dir, &mut profiles, &mut warnings);
        }
        (ProfileSet { profiles }, warnings)
    }

    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.profiles.get(id)
    }

    pub fn list(&self) -> Vec<&Profile> {
        self.profiles.values().collect()
    }
}

fn load_user(dir: &Path, profiles: &mut BTreeMap<String, Profile>, warnings: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            warnings.push(format!("cannot read {}: {e}", dir.display()));
            return;
        }
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    paths.sort();
    if paths.len() > MAX_PROFILE_FILES {
        warnings.push(format!(
            "{}: more than {MAX_PROFILE_FILES} profile files, extra ignored",
            dir.display()
        ));
        paths.truncate(MAX_PROFILE_FILES);
    }
    for path in paths {
        match read_user_profile(&path) {
            Ok(p) => {
                profiles.insert(p.id.clone(), p);
            }
            Err(e) => warnings.push(format!("{}: {e}", path.display())),
        }
    }
}

fn read_user_profile(path: &Path) -> Result<Profile, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.file_type().is_file() {
        return Err("not a regular file, skipped".into());
    }
    if meta.len() > MAX_PROFILE_BYTES {
        return Err(format!("larger than {MAX_PROFILE_BYTES} bytes, skipped"));
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut profile: Profile = toml::from_str(&text).map_err(|e| e.to_string())?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if !valid_profile_id(&profile.id) {
        return Err(format!("invalid profile id {:?}", profile.id));
    }
    if stem != profile.id {
        return Err(format!("file name must match id {:?}", profile.id));
    }
    if profile.command.is_empty() {
        return Err("command must not be empty".into());
    }
    profile.source = Source::User(path.to_path_buf());
    Ok(profile)
}

pub struct Matcher {
    rules: Vec<(String, Regex)>,
}

impl Matcher {
    pub fn new(profile: &Profile) -> Result<Matcher, String> {
        let mut rules = Vec::new();
        for (kind, patterns) in [
            ("blocked", &profile.status.blocked),
            ("idle", &profile.status.idle),
        ] {
            for (i, pattern) in patterns.iter().enumerate() {
                let re = RegexBuilder::new(pattern)
                    .size_limit(REGEX_LIMIT)
                    .dfa_size_limit(REGEX_LIMIT)
                    .build()
                    .map_err(|e| format!("{kind}[{i}]: {e}"))?;
                rules.push((format!("{kind}[{i}]"), re));
            }
        }
        Ok(Matcher { rules })
    }

    pub fn check(&self, lines: &[String]) -> Option<(String, String)> {
        self.rules.iter().find_map(|(id, re)| {
            lines
                .iter()
                .rev()
                .find(|l| re.is_match(l))
                .map(|l| (id.clone(), l.clone()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &Path, name: &str, body: &str) {
        fs::write(dir.join(name), body).unwrap();
    }

    const USER_CLAUDE: &str = "id = \"claude-code\"\nname = \"Mine\"\ncommand = [\"my-claude\"]\n";

    #[test]
    fn builtins_load() {
        let (set, warnings) = ProfileSet::load(None);
        assert!(warnings.is_empty(), "{warnings:?}");
        let ids: Vec<_> = set.list().iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["claude-code", "codex"]);
        assert_eq!(set.get("codex").unwrap().command, vec!["codex"]);
        assert_eq!(set.get("codex").unwrap().source, Source::Builtin);
        for p in set.list() {
            assert!(Matcher::new(p).is_ok());
        }
    }

    #[test]
    fn user_profile_overrides_builtin() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "claude-code.toml", USER_CLAUDE);
        write(
            dir.path(),
            "extra.toml",
            "id = \"extra\"\nname = \"X\"\ncommand = [\"x\"]\n",
        );
        let (set, warnings) = ProfileSet::load(Some(dir.path()));
        assert!(warnings.is_empty(), "{warnings:?}");
        let p = set.get("claude-code").unwrap();
        assert_eq!(p.command, vec!["my-claude"]);
        assert!(matches!(p.source, Source::User(_)));
        assert_eq!(set.list().len(), 3);
        assert!(set.get("extra").is_some());
    }

    #[test]
    fn bad_ids_and_stem_mismatch_warn() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "other.toml", USER_CLAUDE);
        write(
            dir.path(),
            "Bad_Id.toml",
            "id = \"Bad_Id\"\nname = \"B\"\ncommand = [\"b\"]\n",
        );
        write(dir.path(), "broken.toml", "id = ");
        let (set, warnings) = ProfileSet::load(Some(dir.path()));
        assert_eq!(warnings.len(), 3, "{warnings:?}");
        assert_eq!(set.get("claude-code").unwrap().command, vec!["claude"]);
    }

    #[test]
    fn oversize_file_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let pad = " ".repeat(70 * 1024);
        write(
            dir.path(),
            "big.toml",
            &format!("id = \"big\"\nname = \"B\"\ncommand = [\"b\"]\n#{pad}\n"),
        );
        let (set, warnings) = ProfileSet::load(Some(dir.path()));
        assert!(set.get("big").is_none());
        assert_eq!(warnings.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        write(
            target.path(),
            "real.toml",
            "id = \"link\"\nname = \"L\"\ncommand = [\"l\"]\n",
        );
        std::os::unix::fs::symlink(
            target.path().join("real.toml"),
            dir.path().join("link.toml"),
        )
        .unwrap();
        let (set, warnings) = ProfileSet::load(Some(dir.path()));
        assert!(set.get("link").is_none());
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn file_count_capped() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..105 {
            let id = format!("p{i:03}");
            write(
                dir.path(),
                &format!("{id}.toml"),
                &format!("id = \"{id}\"\nname = \"n\"\ncommand = [\"c\"]\n"),
            );
        }
        let (set, warnings) = ProfileSet::load(Some(dir.path()));
        assert_eq!(set.list().len(), 102);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn missing_user_dir_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        let (set, warnings) = ProfileSet::load(Some(&dir.path().join("nope")));
        assert!(warnings.is_empty());
        assert_eq!(set.list().len(), 2);
    }

    #[test]
    fn full_design_profile_parses() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "full.toml",
            r#"
id = "full"
name = "Full"
command = ["full"]
args = ["--x"]
model_arg = ["-m", "{model}"]
env_pass = ["FOO"]
instruction_files = ["AGENTS.md"]
[prompt]
mode = "arg"
[resume]
args = ["--resume"]
[permission_modes]
default = []
[hooks]
kind = "none"
[usage]
source = "none"
[status]
blocked = ['(?i)y/n']
idle = ['^> $']
"#,
        );
        let (set, warnings) = ProfileSet::load(Some(dir.path()));
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(set.get("full").unwrap().status.idle, vec!["^> $"]);
    }

    #[test]
    fn id_validation() {
        for ok in ["a", "claude-code", "0x", &"a".repeat(32)] {
            assert!(valid_profile_id(ok), "{ok}");
        }
        for bad in ["", "-a", "A", "a_b", "a.b", "a b", &"a".repeat(33), "é"] {
            assert!(!valid_profile_id(bad), "{bad}");
        }
    }

    #[test]
    fn argv_substitutes_model() {
        let (set, _) = ProfileSet::load(None);
        let p = set.get("claude-code").unwrap();
        assert_eq!(p.argv(None).unwrap(), vec!["claude"]);
        assert_eq!(
            p.argv(Some("opus-4.1:x/y_z")).unwrap(),
            vec!["claude", "--model", "opus-4.1:x/y_z"]
        );
        for bad in ["", "a b", "a;b", "$(x)", &"a".repeat(65), "-é"] {
            assert!(p.argv(Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn matcher_hits_bottom_lines() {
        let (set, _) = ProfileSet::load(None);
        let m = Matcher::new(set.get("claude-code").unwrap()).unwrap();
        let lines = vec!["working".to_string(), "Do you want to proceed?".to_string()];
        assert_eq!(
            m.check(&lines),
            Some((
                "blocked[0]".to_string(),
                "Do you want to proceed?".to_string()
            ))
        );
        assert_eq!(m.check(&["all quiet".to_string()]), None);
    }

    #[test]
    fn invalid_regex_names_rule() {
        let mut p = ProfileSet::load(None).0.get("codex").unwrap().clone();
        p.status.blocked = vec!["ok".into(), "(unclosed".into()];
        let err = Matcher::new(&p).err().unwrap();
        assert!(err.starts_with("blocked[1]:"), "{err}");
        p.status.blocked = vec![r"(\w{100}){100}".into()];
        assert!(Matcher::new(&p).is_err());
    }
}
