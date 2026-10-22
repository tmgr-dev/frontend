use std::path::{Path, PathBuf};
use tmgr_core::config::{config_dir, ensure_private_dir, write_atomic};
use tmgr_runner::attach::{
    default_templates, parse_templates, render_templates, substitute_template, KindOpt, SpawnKind,
    Template,
};

const FILE: &str = "templates.toml";

pub(super) fn dir() -> Option<PathBuf> {
    config_dir().ok()
}

pub(super) fn load(dir: &Path) -> (Vec<Template>, Option<String>) {
    let path = dir.join(FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => match parse_templates(&text) {
            Ok(list) => (list, None),
            Err(_) => (
                default_templates(),
                Some("templates.toml is invalid, showing the defaults".into()),
            ),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let list = default_templates();
            if ensure_private_dir(dir).is_ok() {
                let _ = write_atomic(&path, render_templates(&list).as_bytes(), 0o600);
            }
            (list, None)
        }
        Err(_) => (
            default_templates(),
            Some("cannot read templates.toml".into()),
        ),
    }
}

fn is_shell(command: &str) -> bool {
    let name = command.rsplit('/').next().unwrap_or(command);
    matches!(name, "sh" | "bash" | "zsh" | "fish" | "dash" | "ksh")
}

pub(super) fn resolve_folder(raw: Option<&str>, base: &str) -> Result<String, String> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    resolve_folder_in(raw, base, home.as_deref())
}

fn resolve_folder_in(raw: Option<&str>, base: &str, home: Option<&Path>) -> Result<String, String> {
    let raw = raw.map(str::trim).filter(|r| !r.is_empty());
    let path = match (raw, home) {
        (None, _) => PathBuf::from(base),
        (Some("~"), Some(h)) => h.to_path_buf(),
        (Some(r), Some(h)) if r.starts_with("~/") => h.join(&r[2..]),
        (Some(r), _) if Path::new(r).is_absolute() => PathBuf::from(r),
        (Some(r), _) => Path::new(base).join(r),
    };
    if !path.is_absolute() || !path.is_dir() {
        return Err("the template folder does not exist".into());
    }
    let text = path.display().to_string();
    Ok(if text.len() > 1 {
        text.trim_end_matches('/').to_string()
    } else {
        text
    })
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Launch {
    pub kind: SpawnKind,
    pub folder: Option<String>,
    pub prompt: String,
}

pub(super) fn launch(t: &Template, values: &[String], kinds: &[KindOpt]) -> Result<Launch, String> {
    let folder = Some(substitute_template(&t.folder, &t.fields, values, false))
        .map(|f| f.trim().to_string())
        .filter(|f| !f.is_empty());
    let prompt = substitute_template(&t.prompt, &t.fields, values, false);
    let kind = if t.command.trim().is_empty() {
        let agents = kinds
            .iter()
            .filter(|k| matches!(k.spawn, SpawnKind::Agent { .. }));
        let found = if t.agent.is_empty() {
            agents.into_iter().next()
        } else {
            agents.into_iter().find(|k| {
                k.id == t.agent
                    || matches!(&k.spawn, SpawnKind::Agent { profile } if *profile == t.agent)
            })
        };
        found.map(|k| k.spawn.clone()).ok_or_else(|| {
            if t.agent.is_empty() {
                "no agent is available".to_string()
            } else {
                format!("agent {} is not installed", t.agent)
            }
        })?
    } else {
        let tokens = shell_words::split(&t.command)
            .ok()
            .filter(|t| !t.is_empty())
            .ok_or("the template command cannot be parsed")?;
        let shell = tokens.first().is_some_and(|c| is_shell(c));
        let argv: Vec<String> = tokens
            .iter()
            .enumerate()
            .map(|(i, token)| {
                let script = shell && i > 0 && tokens[i - 1] == "-c";
                substitute_template(token, &t.fields, values, script)
            })
            .collect();
        SpawnKind::Custom {
            line: shell_words::join(&argv),
        }
    };
    Ok(Launch {
        kind,
        folder,
        prompt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_runner::attach::Field;

    fn kinds() -> Vec<KindOpt> {
        let k = |id: &str, spawn: SpawnKind| KindOpt {
            id: id.into(),
            label: id.into(),
            spawn,
        };
        vec![
            k(
                "claude",
                SpawnKind::Agent {
                    profile: "claude-code".into(),
                },
            ),
            k(
                "codex",
                SpawnKind::Agent {
                    profile: "codex".into(),
                },
            ),
            k("shell", SpawnKind::Shell),
        ]
    }

    fn template(agent: &str, command: &str, prompt: &str) -> Template {
        Template {
            name: "t".into(),
            agent: agent.into(),
            command: command.into(),
            prompt: prompt.into(),
            fields: vec![Field {
                name: "file".into(),
                label: String::new(),
                default: String::new(),
            }],
            ..Template::default()
        }
    }

    #[test]
    fn a_missing_file_gives_the_defaults_and_writes_an_editable_private_copy() {
        let dir = tempfile::tempdir().unwrap();
        let (list, warn) = load(dir.path());
        assert_eq!(list, default_templates());
        assert!(warn.is_none());
        let again = load(dir.path());
        assert_eq!(again.0, default_templates());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join(FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn an_invalid_file_shows_the_defaults_and_stays_untouched() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), "template = [[[").unwrap();
        let (list, warn) = load(dir.path());
        assert_eq!(list, default_templates());
        assert!(warn.is_some());
        assert_eq!(
            std::fs::read_to_string(dir.path().join(FILE)).unwrap(),
            "template = [[["
        );
    }

    #[test]
    fn an_agent_template_picks_its_profile_and_fills_the_prompt() {
        let t = template("codex", "", "Write tests for {file}.");
        let l = launch(&t, &["src/a b.rs".into()], &kinds()).unwrap();
        assert_eq!(
            l.kind,
            SpawnKind::Agent {
                profile: "codex".into()
            }
        );
        assert_eq!(l.prompt, "Write tests for src/a b.rs.");
        assert_eq!(l.folder, None);
    }

    #[test]
    fn without_an_agent_the_first_installed_one_is_used_and_an_unknown_one_is_refused() {
        let t = template("", "", "x");
        assert_eq!(
            launch(&t, &["f".into()], &kinds()).unwrap().kind,
            SpawnKind::Agent {
                profile: "claude-code".into()
            }
        );
        let t = template("aider", "", "x");
        assert!(launch(&t, &["f".into()], &kinds()).is_err());
    }

    #[test]
    fn a_command_template_quotes_every_value_into_one_argv_element() {
        let t = template("", "gh pr view {file} --json title", "");
        let nasty = "a b'; rm -rf / #$(x)\"";
        let l = launch(&t, &[nasty.into()], &kinds()).unwrap();
        let SpawnKind::Custom { line } = l.kind else {
            panic!("not a command");
        };
        let argv = shell_words::split(&line).unwrap();
        assert_eq!(argv, ["gh", "pr", "view", nasty, "--json", "title"]);
    }

    #[test]
    fn values_lose_control_characters_and_newlines() {
        let t = template("", "", "see {file}");
        let l = launch(&t, &["a\nb\x1b[31m".into()], &kinds()).unwrap();
        assert_eq!(l.prompt, "see a b[31m");
    }

    #[test]
    fn quoted_placeholders_in_a_command_keep_the_value_as_one_argument() {
        let t = template("", "git commit -m \"{file}\" --author='{file}'", "");
        let l = launch(&t, &["fix bug".into()], &kinds()).unwrap();
        let SpawnKind::Custom { line } = l.kind else {
            panic!("not a command");
        };
        assert_eq!(
            shell_words::split(&line).unwrap(),
            ["git", "commit", "-m", "fix bug", "--author=fix bug"]
        );
    }

    #[test]
    fn a_value_for_a_shell_script_stays_one_quoted_word() {
        let t = template("", "sh -c 'echo {file}'", "");
        let l = launch(&t, &["x; rm -rf /".into()], &kinds()).unwrap();
        let SpawnKind::Custom { line } = l.kind else {
            panic!("not a command");
        };
        let argv = shell_words::split(&line).unwrap();
        assert_eq!(argv, ["sh", "-c", "echo 'x; rm -rf /'"]);
    }

    #[test]
    fn folders_expand_home_resolve_against_the_base_and_must_exist() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join("src/app")).unwrap();
        let base = home.path().join("src");
        let base_s = base.display().to_string();
        let want = home.path().join("src/app").display().to_string();
        assert_eq!(
            resolve_folder_in(Some("~/src/app/"), &base_s, Some(home.path())).unwrap(),
            want
        );
        assert_eq!(
            resolve_folder_in(Some("app"), &base_s, Some(home.path())).unwrap(),
            want
        );
        assert_eq!(
            resolve_folder_in(None, &want, Some(home.path())).unwrap(),
            want
        );
        assert!(resolve_folder_in(Some("nope"), &base_s, Some(home.path())).is_err());
        assert!(resolve_folder_in(Some("~/nope"), &base_s, None).is_err());
    }
}
