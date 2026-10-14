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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Fresh {
    pub json_key: String,
    pub max_age_days: u32,
}

fn write_back_flag<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Flag {
        Bool(bool),
        Text(String),
    }
    match Flag::deserialize(d)? {
        Flag::Bool(b) => Ok(b),
        Flag::Text(t) if t == "confirm" => Ok(true),
        Flag::Text(_) => Err(serde::de::Error::custom(
            "write_back must be true, false or \"confirm\"",
        )),
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Seed {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub json_keys: Option<Vec<String>>,
    #[serde(default)]
    pub fresh: Option<Fresh>,
    #[serde(default, deserialize_with = "write_back_flag")]
    pub write_back: bool,
    #[serde(default)]
    pub same: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct SandboxDecl {
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub seed: Vec<Seed>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct SecretFile {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub token_prefix: Option<String>,
    #[serde(default)]
    pub template: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct SecretsDecl {
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub files: Vec<SecretFile>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResumeAt {
    #[default]
    End,
    AfterCommand,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct ResumeDecl {
    pub args: Vec<String>,
    #[serde(default)]
    pub at: ResumeAt,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromptMode {
    Arg,
    Stdin,
    File,
    #[default]
    None,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct PromptDecl {
    #[serde(default)]
    pub mode: PromptMode,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    #[default]
    T0,
    T1,
    T2,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HooksKind {
    #[default]
    None,
    SettingsFile,
    ConfigOverride,
    ProjectFile,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct HooksDecl {
    #[serde(default)]
    pub kind: HooksKind,
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
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub sandbox: SandboxDecl,
    #[serde(default)]
    pub secrets: SecretsDecl,
    #[serde(default)]
    pub status: StatusRules,
    #[serde(default)]
    pub resume: Option<ResumeDecl>,
    #[serde(default)]
    pub prompt: PromptDecl,
    #[serde(default)]
    pub instruction_files: Vec<String>,
    #[serde(default)]
    pub tier: Tier,
    #[serde(default)]
    pub hooks: HooksDecl,
    #[serde(default)]
    pub permission_modes: BTreeMap<String, Vec<String>>,
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
        self.argv_with(model, &[])
    }

    pub fn argv_with(&self, model: Option<&str>, extra: &[String]) -> Result<Vec<String>, String> {
        let mut argv: Vec<String> = self.command.iter().chain(&self.args).cloned().collect();
        if let Some(model) = model {
            if !valid_model(model) {
                return Err(format!("invalid model value {model:?}"));
            }
            argv.extend(self.model_arg.iter().map(|a| a.replace("{model}", model)));
        }
        argv.extend(extra.iter().cloned());
        Ok(argv)
    }

    pub fn has_permission_modes(&self) -> bool {
        self.permission_modes
            .keys()
            .any(|m| tmgr_ipc::api::valid_permission_mode(m))
    }

    pub fn permission_args(&self, mode: &str) -> Result<&[String], String> {
        self.permission_modes
            .get(mode)
            .map(Vec::as_slice)
            .ok_or_else(|| format!("profile {} cannot run in mode {mode}", self.id))
    }

    pub fn resume_argv(&self, model: Option<&str>) -> Result<Option<Vec<String>>, String> {
        self.resume_argv_with(model, &[])
    }

    pub fn resume_argv_with(
        &self,
        model: Option<&str>,
        extra: &[String],
    ) -> Result<Option<Vec<String>>, String> {
        let Some(r) = &self.resume else {
            return Ok(None);
        };
        let mut argv = self.argv_with(model, extra)?;
        match r.at {
            ResumeAt::End => argv.extend(r.args.iter().cloned()),
            ResumeAt::AfterCommand => {
                let at = self.command.len().min(argv.len());
                argv.splice(at..at, r.args.iter().cloned());
            }
        }
        Ok(Some(argv))
    }
}

const MAX_INSTRUCTION_FILES: usize = 8;

fn check_launch(p: &Profile) -> Result<(), String> {
    if let Some(r) = &p.resume {
        if r.args.is_empty() {
            return Err("resume.args must not be empty".into());
        }
        if r.args.iter().any(|a| a.contains('{')) {
            return Err("resume.args take no placeholders".into());
        }
    }
    let holes = p.prompt.args.iter().filter(|a| *a == "{prompt}").count();
    let stray = p
        .prompt
        .args
        .iter()
        .any(|a| a != "{prompt}" && a.contains('{'));
    match p.prompt.mode {
        PromptMode::Arg if holes != 1 || stray => {
            return Err(
                "prompt.args must hold \"{prompt}\" exactly once as a whole argument".into(),
            )
        }
        PromptMode::Arg => {}
        _ if !p.prompt.args.is_empty() => {
            return Err("prompt.args is only used with mode = \"arg\"".into())
        }
        _ => {}
    }
    for (mode, args) in &p.permission_modes {
        if !tmgr_ipc::api::valid_permission_mode(mode) {
            continue;
        }
        if args.is_empty() || args.iter().any(|a| a.contains('{')) {
            return Err(format!(
                "permission_modes.{mode} must be non-empty and take no placeholders"
            ));
        }
    }
    if p.instruction_files.len() > MAX_INSTRUCTION_FILES {
        return Err(format!("at most {MAX_INSTRUCTION_FILES} instruction_files"));
    }
    for f in &p.instruction_files {
        let ok = (1..=64).contains(&f.len())
            && !f.starts_with('.')
            && f.bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'));
        if !ok {
            return Err(format!("instruction file {f:?} must be a plain file name"));
        }
    }
    Ok(())
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

const BUILTINS: [&str; 6] = [
    include_str!("../profiles/claude-code.toml"),
    include_str!("../profiles/codex.toml"),
    include_str!("../profiles/opencode.toml"),
    include_str!("../profiles/gemini.toml"),
    include_str!("../profiles/cursor-agent.toml"),
    include_str!("../profiles/aider.toml"),
];

pub struct ProfileSet {
    profiles: BTreeMap<String, Profile>,
}

impl ProfileSet {
    pub fn load(user_dir: Option<&Path>) -> (ProfileSet, Vec<String>) {
        let mut warnings = Vec::new();
        let mut profiles = BTreeMap::new();
        let home = std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from);
        for text in BUILTINS {
            match toml::from_str::<Profile>(text)
                .map_err(|e| e.to_string())
                .and_then(|p| check_declarations(&p, home.as_deref()).map(|()| p))
            {
                Ok(p) => {
                    profiles.insert(p.id.clone(), p);
                }
                Err(e) => warnings.push(format!("built-in profile is invalid: {e}")),
            }
        }
        if let Some(dir) = user_dir {
            load_user(dir, home.as_deref(), &mut profiles, &mut warnings);
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

fn load_user(
    dir: &Path,
    home: Option<&Path>,
    profiles: &mut BTreeMap<String, Profile>,
    warnings: &mut Vec<String>,
) {
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
        match read_user_profile(&path, home) {
            Ok(p) => {
                profiles.insert(p.id.clone(), p);
            }
            Err(e) => warnings.push(format!("{}: {e}", path.display())),
        }
    }
}

fn secrets_file_of(text: &str) -> Result<Option<String>, String> {
    let v: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
    Ok(v.get("secrets")
        .and_then(|s| s.get("file"))
        .and_then(|f| f.as_str())
        .map(str::to_string))
}

pub fn secrets_declarations(dir: Option<&Path>) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for text in BUILTINS {
        out.extend(secrets_file_of(text)?);
    }
    let Some(dir) = dir else { return Ok(out) };
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(format!("cannot read {}: {e}", dir.display())),
    };
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|e| format!("cannot list {}: {e}", dir.display()))?
            .path();
        if path.extension().is_some_and(|x| x == "toml") {
            paths.push(path);
        }
    }
    if paths.len() > MAX_PROFILE_FILES {
        return Err(format!(
            "{} has more than {MAX_PROFILE_FILES} profile files",
            dir.display()
        ));
    }
    paths.sort();
    for path in paths {
        let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if !meta.file_type().is_file() {
            continue;
        }
        if meta.len() > MAX_PROFILE_BYTES {
            return Err(format!(
                "{} is larger than {MAX_PROFILE_BYTES} bytes, its secrets location cannot be established",
                path.display()
            ));
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        out.extend(secrets_file_of(&text).map_err(|e| format!("{}: {e}", path.display()))?);
    }
    Ok(out)
}

fn read_user_profile(path: &Path, home: Option<&Path>) -> Result<Profile, String> {
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
    check_declarations(&profile, home)?;
    profile.source = Source::User(path.to_path_buf());
    Ok(profile)
}

fn check_declarations(p: &Profile, home: Option<&Path>) -> Result<(), String> {
    check_launch(p)?;
    match home {
        Some(h) => crate::login::resolve(p, h, &[]).map(|_| ()),
        None => crate::login::check_env(p),
    }
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
        let mut expected: Vec<String> = BUILTINS
            .iter()
            .map(|t| toml::from_str::<Profile>(t).unwrap().id)
            .collect();
        expected.sort();
        assert_eq!(ids, expected);
        assert_eq!(ids.len(), BUILTINS.len());
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
        assert_eq!(set.list().len(), BUILTINS.len() + 1);
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
        assert_eq!(set.list().len(), BUILTINS.len() + 100);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn missing_user_dir_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        let (set, warnings) = ProfileSet::load(Some(&dir.path().join("nope")));
        assert!(warnings.is_empty());
        assert_eq!(set.list().len(), BUILTINS.len());
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
args = ["--prompt", "{prompt}"]
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

    fn parse(extra: &str) -> Result<Profile, String> {
        let text = format!("id = \"x\"\nname = \"X\"\ncommand = [\"x\", \"sub\"]\nargs = [\"--a\"]\nmodel_arg = [\"-m\", \"{{model}}\"]\n{extra}");
        let p: Profile = toml::from_str(&text).map_err(|e| e.to_string())?;
        check_launch(&p).map(|()| p)
    }

    #[test]
    fn launch_fields_default_to_t0_without_resume() {
        let p = parse("").unwrap();
        assert_eq!(p.tier, Tier::T0);
        assert_eq!(p.prompt.mode, PromptMode::None);
        assert_eq!(p.hooks.kind, HooksKind::None);
        assert_eq!(p.resume_argv(None).unwrap(), None);
    }

    #[test]
    fn resume_argv_places_args() {
        let p = parse("[resume]\nargs = [\"--continue\"]\n").unwrap();
        assert_eq!(
            p.resume_argv(Some("m1")).unwrap().unwrap(),
            vec!["x", "sub", "--a", "-m", "m1", "--continue"]
        );
        let p =
            parse("[resume]\nargs = [\"resume\", \"--last\"]\nat = \"after_command\"\n").unwrap();
        assert_eq!(
            p.resume_argv(None).unwrap().unwrap(),
            vec!["x", "sub", "resume", "--last", "--a"]
        );
        assert!(p.resume_argv(Some("a b")).is_err());
    }

    #[test]
    fn launch_fields_are_validated() {
        for bad in [
            "[resume]\nargs = []\n",
            "[resume]\nargs = [\"--resume\", \"{session_id}\"]\n",
            "[resume]\nargs = [\"x\"]\nat = \"middle\"\n",
            "[prompt]\nmode = \"arg\"\n",
            "[prompt]\nmode = \"arg\"\nargs = [\"{prompt}\", \"{prompt}\"]\n",
            "[prompt]\nmode = \"arg\"\nargs = [\"--p={prompt}\"]\n",
            "[prompt]\nmode = \"stdin\"\nargs = [\"{prompt}\"]\n",
            "[prompt]\nmode = \"shell\"\n",
            "instruction_files = [\"../AGENTS.md\"]\n",
            "instruction_files = [\"a/b.md\"]\n",
            "instruction_files = [\".env\"]\n",
            "tier = \"t9\"\n",
            "[hooks]\nkind = \"script\"\n",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        let ok = parse(
            "tier = \"t2\"\ninstruction_files = [\"AGENTS.md\", \"GEMINI.md\"]\n[prompt]\nmode = \"arg\"\nargs = [\"-i\", \"{prompt}\"]\n[hooks]\nkind = \"project-file\"\n",
        )
        .unwrap();
        assert_eq!(ok.tier, Tier::T2);
        assert_eq!(ok.hooks.kind, HooksKind::ProjectFile);
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
    fn builtin_permission_modes_come_from_the_agents_own_flags() {
        let (set, warnings) = ProfileSet::load(None);
        assert!(warnings.is_empty(), "{warnings:?}");
        let args = |id: &str, mode: &str| {
            set.get(id)
                .unwrap()
                .permission_args(mode)
                .map(<[_]>::to_vec)
        };
        assert_eq!(
            args("claude-code", "accept-edits").unwrap(),
            ["--permission-mode", "acceptEdits"]
        );
        assert_eq!(
            args("codex", "ask").unwrap(),
            ["--ask-for-approval", "on-request"]
        );
        assert_eq!(args("gemini", "plan").unwrap(), ["--approval-mode", "plan"]);
        assert!(args("codex", "plan").is_err());
        for id in ["opencode", "cursor-agent", "aider"] {
            assert!(set.get(id).unwrap().permission_modes.is_empty(), "{id}");
        }
        let p = set.get("claude-code").unwrap();
        let extra = p.permission_args("plan").unwrap();
        assert_eq!(
            p.argv_with(Some("m"), extra).unwrap(),
            ["claude", "--model", "m", "--permission-mode", "plan"]
        );
        assert_eq!(
            p.resume_argv_with(None, extra).unwrap().unwrap(),
            ["claude", "--permission-mode", "plan", "--continue"]
        );
    }

    #[test]
    fn declared_permission_modes_are_checked() {
        let dir = tempfile::tempdir().unwrap();
        for (i, table) in ["plan = []", "ask = []", "ask = [\"--m={model}\"]"]
            .iter()
            .enumerate()
        {
            let text = format!(
                "id = \"p{i}\"\nname = \"p\"\ncommand = [\"p\"]\n[permission_modes]\n{table}\n"
            );
            std::fs::write(dir.path().join(format!("p{i}.toml")), text).unwrap();
        }
        let (set, warnings) = ProfileSet::load(Some(dir.path()));
        assert_eq!(warnings.len(), 3, "{warnings:?}");
        assert!(set.get("p0").is_none());
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
