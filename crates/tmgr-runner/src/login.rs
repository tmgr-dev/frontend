use crate::profile::Profile;
use crate::sandbox::{canon_lossy, env_name_refused, home_deny_paths, overlaps, Grants};
use std::collections::BTreeMap;
use std::fs::{DirBuilder, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

const MAX_SEED_BYTES: u64 = 1 << 20;
const MAX_SECRETS_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSeed {
    pub from: PathBuf,
    pub to: PathBuf,
    pub json_keys: Option<Vec<String>>,
}

#[derive(Debug, Default)]
pub struct Resolved {
    pub read: Vec<PathBuf>,
    pub seeds: Vec<ResolvedSeed>,
    pub secrets: Option<PathBuf>,
    pub warnings: Vec<String>,
}

pub struct Prepared {
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
    deny.extend(protected.iter().map(|d| canon_lossy(d)));
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
    for s in &p.sandbox.seed {
        let from = allowed("seed source", &s.from)?;
        let to = PathBuf::from(&s.to);
        plain_relative("seed destination", &to)?;
        out.seeds.push(ResolvedSeed {
            from,
            to,
            json_keys: s.json_keys.clone(),
        });
    }
    if let Some(f) = &p.secrets.file {
        out.secrets = Some(under_home("secrets file", f, &home)?);
    }
    Ok(out)
}

fn open_nofollow(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
}

fn is_symlink_error(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(libc::ELOOP)
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
            trusted_env_pass: p.env_pass.clone(),
            env: p.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        },
        seeds: r.seeds,
        pass_values,
        warnings: r.warnings,
    })
}

fn seed_bytes(seed: &ResolvedSeed) -> Result<Option<Vec<u8>>, String> {
    let mut f = match open_nofollow(&seed.from) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) if is_symlink_error(&e) => {
            return Err(format!(
                "seed source {} is a symlink, refusing to copy it",
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

pub fn apply_seeds(seeds: &[ResolvedSeed], home: &Path) -> Result<(), String> {
    for seed in seeds {
        let Some(bytes) = seed_bytes(seed)? else {
            continue;
        };
        let dest = home.join(&seed.to);
        if let Some(parent) = dest.parent() {
            DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&dest)
            .and_then(|mut f| f.write_all(&bytes))
            .map_err(|e| format!("cannot write {}: {e}", dest.display()))?;
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub struct LoginFile {
    pub path: PathBuf,
    pub created: bool,
    pub private: bool,
    pub names: Vec<String>,
}

pub fn ensure_login_file(p: &Profile, real_home: &Path) -> Result<LoginFile, String> {
    let r = resolve(p, real_home, &[])?;
    let path = r
        .secrets
        .ok_or_else(|| format!("profile {} declares no [secrets] file", p.id))?;
    if p.env_pass.is_empty() {
        return Err(format!("profile {} has no env_pass names", p.id));
    }
    let names = p.env_pass.clone();
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        let private = private_file_problem(&path, &meta).is_none();
        return Ok(LoginFile {
            path,
            created: false,
            private,
            names,
        });
    }
    if let Some(parent) = path.parent() {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let body: String = names.iter().map(|n| format!("{n}=\n")).collect();
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .and_then(|mut f| f.write_all(body.as_bytes()))
        .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
    Ok(LoginFile {
        path,
        created: true,
        private: true,
        names,
    })
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
        assert_eq!(
            claude.secrets,
            Some(h.path.join(".config/tmgr/agents/claude-code.env"))
        );
        assert_eq!(
            set.get("claude-code").unwrap().env_pass,
            ["CLAUDE_CODE_OAUTH_TOKEN"]
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

    #[test]
    fn login_file_is_created_once_and_never_overwritten() {
        let h = home();
        let p = profile("env_pass = [\"A_TOKEN\"]\n[secrets]\nfile = \"~/.config/x/p.env\"\n");
        let first = ensure_login_file(&p, &h.path).unwrap();
        assert!(first.created && first.private);
        let meta = std::fs::metadata(&first.path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        assert_eq!(
            std::fs::metadata(first.path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(std::fs::read_to_string(&first.path).unwrap(), "A_TOKEN=\n");
        std::fs::write(&first.path, "A_TOKEN=keep\n").unwrap();
        let second = ensure_login_file(&p, &h.path).unwrap();
        assert!(!second.created);
        assert_eq!(
            std::fs::read_to_string(&first.path).unwrap(),
            "A_TOKEN=keep\n"
        );
    }
}
