use crate::audit::{sha256_hex, Audit};
use crate::paths::{read_private, Paths};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tmgr_core::config::{ensure_private_dir, write_atomic};
use tmgr_core::error::{Error, Kind, Result};

pub const VERSION: u32 = 1;
pub const MAX_CONCURRENT_CAP: u32 = 3;
pub const GLOBAL_MAX: usize = 3;
pub const CONFIG_EVENTS: &[&str] = &["enable", "disable", "rule_add", "rule_rm", "rules_reviewed"];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QueueFile {
    #[serde(default = "version")]
    pub version: u32,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, rename = "rule")]
    pub rules: Vec<Rule>,
}

impl Default for QueueFile {
    fn default() -> Self {
        QueueFile {
            version: VERSION,
            enabled: false,
            rules: Vec::new(),
        }
    }
}

fn version() -> u32 {
    VERSION
}

fn status_type() -> String {
    "backlog".into()
}

fn max_concurrent() -> u32 {
    1
}

fn trusted() -> Vec<String> {
    vec!["human".into()]
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub name: String,
    pub workspace: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default = "status_type")]
    pub status_type: String,
    pub persona: String,
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub repo: String,
    #[serde(default = "max_concurrent")]
    pub max_concurrent: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hours: Option<String>,
    #[serde(default = "trusted")]
    pub trusted_authors: Vec<String>,
    #[serde(default)]
    pub auto: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfilePin {
    pub argv: Vec<String>,
    pub profile_sha256: String,
    pub exe_path: String,
    pub exe_sha256: String,
}

impl ProfilePin {
    /// Digest of the command line the runner starts: the approved argv with the pinned executable.
    pub fn argv_sha256(&self) -> String {
        let mut argv = self.argv.clone();
        if let Some(first) = argv.first_mut() {
            first.clone_from(&self.exe_path);
        }
        tmgr_ipc::api::argv_sha256(&argv)
    }
}

pub trait ProfileResolver {
    fn argv(&self, profile: &str, model: Option<&str>) -> std::result::Result<Vec<String>, String>;
    fn pin(&self, profile: &str, model: Option<&str>) -> std::result::Result<ProfilePin, String>;
}

pub fn file_sha256(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub fn resolve_exe(command: &str) -> std::result::Result<std::path::PathBuf, String> {
    let candidates: Vec<std::path::PathBuf> = if command.contains('/') {
        vec![command.into()]
    } else {
        std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).map(|d| d.join(command)).collect())
            .unwrap_or_default()
    };
    candidates
        .into_iter()
        .find_map(|c| std::fs::canonicalize(&c).ok().filter(|p| p.is_file()))
        .ok_or_else(|| format!("executable {command} not found"))
}

pub fn argv_is_bypass(argv: &[String]) -> bool {
    let lower: Vec<String> = argv.iter().map(|a| a.to_ascii_lowercase()).collect();
    lower.iter().enumerate().any(|(i, a)| {
        a.contains("bypass")
            || a.contains("dangerous")
            || a.contains("yolo")
            || a.contains("danger-full-access")
            || a == "--auto"
            || a.starts_with("--auto=")
            || a == "--ask-for-approval=never"
            || a == "-a=never"
            || a == "-anever"
            || ((a == "--ask-for-approval" || a == "-a")
                && lower.get(i + 1).is_some_and(|n| n == "never"))
    })
}

fn word(s: &str, max: usize, extra: &str) -> bool {
    (1..=max).contains(&s.len())
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || extra.as_bytes().contains(&c))
}

pub fn parse_hours(spec: &str) -> Option<(u32, u32)> {
    let (a, b) = spec.split_once('-')?;
    let num = |s: &str| {
        (s.len() == 2 && s.bytes().all(|c| c.is_ascii_digit()))
            .then(|| s.parse::<u32>().ok())
            .flatten()
    };
    let (start, end) = (num(a)?, num(b)?);
    (start <= 23 && end <= 24 && start != end).then_some((start, end))
}

pub fn hours_contains(spec: &str, local_minutes: u32) -> bool {
    let Some((start, end)) = parse_hours(spec) else {
        return false;
    };
    let (s, e) = (start * 60, end * 60);
    if s < e {
        (s..e).contains(&local_minutes)
    } else {
        local_minutes >= s || local_minutes < e
    }
}

fn refuse(message: impl AsRef<str>) -> Error {
    Error::new(Kind::Forbidden, message)
}

pub fn workspace_code(reference: &str) -> Result<&str> {
    if let Some(code) = tmgr_core::local::local_code(reference) {
        return Ok(code);
    }
    if tmgr_core::local::is_workspace_ref(reference) {
        return Err(refuse("queue rules pick only tasks of local workspaces")
            .hint("write the workspace as local:<code>"));
    }
    Err(Error::usage(format!("invalid workspace {reference:?}")).hint("local:<code>"))
}

pub fn validate_rule(rule: &Rule, profiles: &dyn ProfileResolver) -> Result<()> {
    let usage = |m: &str| Err(Error::usage(format!("rule {:?}: {m}", rule.name)));
    if !word(&rule.name, 32, "-") {
        return usage("name is 1 to 32 characters of a-z, 0-9 and -");
    }
    workspace_code(&rule.workspace)?;
    if let Some(p) = &rule.project {
        if !(1..=16).contains(&p.len())
            || !p
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            return usage("project is a category code of at most 16 characters");
        }
    }
    if !matches!(rule.status_type.as_str(), "backlog" | "active") {
        return usage("status_type is backlog or active");
    }
    let persona_len = rule.persona.chars().count();
    if !(1..=64).contains(&persona_len) || rule.persona.chars().any(char::is_control) {
        return usage("persona is 1 to 64 characters without control characters");
    }
    if !word(&rule.profile, 32, "-") {
        return usage("profile is an agent profile id");
    }
    if let Some(m) = &rule.model {
        if !(1..=64).contains(&m.len())
            || !m
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._:/-".contains(&c))
        {
            return usage("model has invalid characters");
        }
    }
    if !Path::new(&rule.repo).is_absolute() || !Path::new(&rule.repo).is_dir() {
        return usage("repo is an absolute path of an existing directory");
    }
    if !(1..=MAX_CONCURRENT_CAP).contains(&rule.max_concurrent) {
        return usage(&format!("max_concurrent is 1 to {MAX_CONCURRENT_CAP}"));
    }
    if let Some(h) = &rule.hours {
        if parse_hours(h).is_none() {
            return usage("hours is HH-HH, for example 09-19 or 22-06");
        }
    }
    if rule.trusted_authors.is_empty()
        || !rule
            .trusted_authors
            .iter()
            .all(|a| matches!(a.as_str(), "human" | "agent" | "unrecorded"))
    {
        return usage("trusted_authors lists human, agent or unrecorded");
    }
    if rule.auto && rule.trusted_authors != ["human"] {
        return usage("auto = true is only possible for human authors");
    }
    match profiles.argv(&rule.profile, rule.model.as_deref()) {
        Ok(argv) if argv_is_bypass(&argv) => Err(refuse(format!(
            "rule {:?}: profile {} runs without permission prompts, which the queue never allows",
            rule.name, rule.profile
        ))),
        Ok(_) => Ok(()),
        Err(m) => Err(refuse(format!(
            "rule {:?}: profile {} cannot be checked ({m}), so the queue refuses it",
            rule.name, rule.profile
        ))),
    }
}

pub fn validate_file(file: &QueueFile, profiles: &dyn ProfileResolver) -> Result<()> {
    if file.version != VERSION {
        return Err(Error::usage(format!(
            "queue.toml version {} is not supported",
            file.version
        )));
    }
    let mut seen = std::collections::BTreeSet::new();
    for r in &file.rules {
        if !seen.insert(&r.name) {
            return Err(Error::usage(format!("duplicate rule name {:?}", r.name)));
        }
        validate_rule(r, profiles)?;
    }
    Ok(())
}

pub struct Loaded {
    pub file: QueueFile,
    pub sha256: Option<String>,
}

pub fn load(paths: &Paths) -> Result<Loaded> {
    let Some(text) = read_private(&paths.rules())? else {
        return Ok(Loaded {
            file: QueueFile::default(),
            sha256: None,
        });
    };
    let file: QueueFile = toml::from_str(&text).map_err(|e| {
        Error::usage(format!("queue.toml is invalid: {}", e.message()))
            .hint("fields unknown to tmgr are refused on purpose")
    })?;
    Ok(Loaded {
        file,
        sha256: Some(sha256_hex(text.as_bytes())),
    })
}

pub fn save(paths: &Paths, file: &QueueFile) -> Result<String> {
    ensure_private_dir(&paths.dir)?;
    let text = toml::to_string_pretty(file).map_err(|e| Error::internal(e.to_string()))?;
    write_atomic(&paths.rules(), text.as_bytes(), 0o600)?;
    Ok(sha256_hex(text.as_bytes()))
}

pub fn recorded_hash(paths: &Paths) -> Result<Option<String>> {
    Audit::new(paths).last_field(CONFIG_EVENTS, "config_sha256")
}

pub fn is_tampered(paths: &Paths, loaded: &Loaded) -> Result<bool> {
    let Some(sha) = &loaded.sha256 else {
        return Ok(false);
    };
    if Audit::new(paths).verify()?.is_err() {
        return Ok(true);
    }
    Ok(recorded_hash(paths)?.as_deref() != Some(sha.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub struct Fake(pub Vec<&'static str>);

    impl ProfileResolver for Fake {
        fn argv(
            &self,
            profile: &str,
            _m: Option<&str>,
        ) -> std::result::Result<Vec<String>, String> {
            match profile {
                "claude-code" => Ok(vec!["claude".into()]),
                "risky" => Ok(self.0.iter().map(|s| s.to_string()).collect()),
                _ => Err("unknown profile".into()),
            }
        }

        fn pin(
            &self,
            profile: &str,
            model: Option<&str>,
        ) -> std::result::Result<ProfilePin, String> {
            Ok(ProfilePin {
                argv: self.argv(profile, model)?,
                profile_sha256: "p".repeat(64),
                exe_path: "/bin/claude".into(),
                exe_sha256: "e".repeat(64),
            })
        }
    }

    fn rule(repo: &Path) -> Rule {
        Rule {
            name: "backlog-builder".into(),
            workspace: "local:personal".into(),
            project: Some("TM".into()),
            status_type: "backlog".into(),
            persona: "builder".into(),
            profile: "claude-code".into(),
            model: None,
            repo: repo.to_string_lossy().into(),
            max_concurrent: 1,
            hours: None,
            trusted_authors: vec!["human".into()],
            auto: false,
        }
    }

    #[test]
    fn a_valid_rule_passes_and_a_cloud_workspace_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let f = Fake(vec![]);
        assert!(validate_rule(&rule(d.path()), &f).is_ok());
        let mut r = rule(d.path());
        r.workspace = "tmgrdev".into();
        assert_eq!(validate_rule(&r, &f).unwrap_err().exit_code(), 5);
        r.workspace = "local:".into();
        assert_eq!(validate_rule(&r, &f).unwrap_err().exit_code(), 2);
    }

    #[test]
    fn bypass_profiles_and_unknown_profiles_are_refused() {
        let d = tempfile::tempdir().unwrap();
        let mut r = rule(d.path());
        r.profile = "risky".into();
        for argv in [
            vec!["claude", "--dangerously-skip-permissions"],
            vec!["claude", "--permission-mode", "bypassPermissions"],
            vec!["codex", "--yolo"],
            vec!["codex", "--sandbox", "danger-full-access"],
            vec!["codex", "-a", "never"],
            vec![
                "codex",
                "--ask-for-approval",
                "never",
                "--sandbox",
                "workspace-write",
            ],
            vec!["codex", "-a=never"],
            vec!["codex", "-anever"],
            vec!["codex", "--ask-for-approval", "NEVER"],
            vec!["opencode", "--auto"],
            vec!["opencode", "--AUTO=true"],
            vec!["agy", "--dangerously-skip-permissions"],
        ] {
            assert_eq!(
                validate_rule(&r, &Fake(argv.clone()))
                    .unwrap_err()
                    .exit_code(),
                5,
                "{argv:?}"
            );
        }
        assert!(validate_rule(&r, &Fake(vec!["codex", "--model", "x"])).is_ok());
        assert!(validate_rule(&r, &Fake(vec!["agy", "--model", "auto"])).is_ok());
        r.profile = "nope".into();
        assert_eq!(validate_rule(&r, &Fake(vec![])).unwrap_err().exit_code(), 5);
    }

    #[test]
    fn caps_hours_and_auto_limits() {
        let d = tempfile::tempdir().unwrap();
        let f = Fake(vec![]);
        let bad = |edit: &dyn Fn(&mut Rule)| {
            let mut r = rule(d.path());
            edit(&mut r);
            validate_rule(&r, &f).is_err()
        };
        assert!(bad(&|r| r.max_concurrent = 0));
        assert!(bad(&|r| r.max_concurrent = MAX_CONCURRENT_CAP + 1));
        assert!(bad(&|r| r.hours = Some("9-19".into())));
        assert!(bad(&|r| r.hours = Some("09-09".into())));
        assert!(bad(&|r| r.hours = Some("09-25".into())));
        assert!(bad(&|r| r.repo = "relative/dir".into()));
        assert!(bad(&|r| r.repo = "/nonexistent-dir-for-queue-test".into()));
        assert!(bad(&|r| r.name = "Bad Name".into()));
        assert!(bad(&|r| r.trusted_authors = vec![]));
        assert!(bad(&|r| r.trusted_authors = vec!["anyone".into()]));
        assert!(bad(&|r| {
            r.auto = true;
            r.trusted_authors = vec!["human".into(), "agent".into()];
        }));
        assert!(!bad(&|r| r.auto = true));
    }

    #[test]
    fn hours_window_wraps_midnight() {
        assert!(hours_contains("09-19", 9 * 60));
        assert!(!hours_contains("09-19", 19 * 60));
        assert!(hours_contains("22-06", 23 * 60));
        assert!(hours_contains("22-06", 5 * 60 + 59));
        assert!(!hours_contains("22-06", 6 * 60));
        assert!(!hours_contains("22-06", 12 * 60));
        assert!(hours_contains("00-24", 23 * 60 + 59));
    }

    #[test]
    fn unknown_fields_in_the_file_are_refused() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::at(d.path());
        for extra in [
            "mode = \"bypass\"",
            "bypass = true",
            "dangerously_skip_permissions = true",
        ] {
            let text = format!(
                "version = 1\nenabled = true\n[[rule]]\nname = \"a\"\nworkspace = \"local:p\"\npersona = \"b\"\nprofile = \"claude-code\"\nrepo = \"/\"\n{extra}\n"
            );
            std::fs::write(p.rules(), text).unwrap();
            std::fs::set_permissions(
                p.rules(),
                std::os::unix::fs::PermissionsExt::from_mode(0o600),
            )
            .unwrap();
            assert!(load(&p).is_err(), "{extra}");
        }
        std::fs::write(p.rules(), "version = 1\nmode = \"bypass\"\n").unwrap();
        std::fs::set_permissions(
            p.rules(),
            std::os::unix::fs::PermissionsExt::from_mode(0o600),
        )
        .unwrap();
        assert!(load(&p).is_err());
    }

    #[test]
    fn save_load_roundtrip_is_private_and_hashed() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::at(&d.path().join("tmgr"));
        let repo = tempfile::tempdir().unwrap();
        let file = QueueFile {
            version: VERSION,
            enabled: true,
            rules: vec![rule(repo.path())],
        };
        let sha = save(&p, &file).unwrap();
        let loaded = load(&p).unwrap();
        assert_eq!(loaded.file, file);
        assert_eq!(loaded.sha256.as_deref(), Some(sha.as_str()));
        let mode = std::os::unix::fs::PermissionsExt::mode(
            &std::fs::metadata(p.rules()).unwrap().permissions(),
        );
        assert_eq!(mode & 0o777, 0o600);
        assert!(is_tampered(&p, &loaded).unwrap());
        Audit::new(&p)
            .append("enable", serde_json::json!({"config_sha256": sha}))
            .unwrap();
        assert!(!is_tampered(&p, &load(&p).unwrap()).unwrap());
        std::fs::write(p.rules(), "version = 1\nenabled = true\n").unwrap();
        std::fs::set_permissions(
            p.rules(),
            std::os::unix::fs::PermissionsExt::from_mode(0o600),
        )
        .unwrap();
        assert!(is_tampered(&p, &load(&p).unwrap()).unwrap());
    }

    #[test]
    fn a_group_writable_file_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::at(d.path());
        std::fs::write(p.rules(), "version = 1\n").unwrap();
        std::fs::set_permissions(
            p.rules(),
            std::os::unix::fs::PermissionsExt::from_mode(0o664),
        )
        .unwrap();
        assert_eq!(load(&p).err().unwrap().exit_code(), 5);
    }
}
