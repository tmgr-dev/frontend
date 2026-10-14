use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{Result, UsageError};
use crate::model::{Account, Tool};

#[derive(Debug, Clone, Default)]
pub struct Env {
    pub home: Option<PathBuf>,
    pub vars: HashMap<String, String>,
}

impl Env {
    pub fn from_process() -> Env {
        let vars = [
            "TMGR_USAGE_DIR",
            "XDG_DATA_HOME",
            "CLAUDE_CONFIG_DIR",
            "CODEX_HOME",
        ]
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
        .collect();
        Env {
            home: std::env::var_os("HOME").map(PathBuf::from),
            vars,
        }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.vars
            .get(key)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    fn home_or_err(&self) -> Result<&Path> {
        self.home
            .as_deref()
            .ok_or_else(|| UsageError::Invalid("home directory is unknown".into()))
    }
}

pub fn data_dir(env: &Env) -> Result<PathBuf> {
    if let Some(dir) = env.get("TMGR_USAGE_DIR") {
        return Ok(PathBuf::from(dir));
    }
    if let Some(xdg) = env.get("XDG_DATA_HOME") {
        return Ok(Path::new(xdg).join("tmgr").join("usage"));
    }
    Ok(env
        .home_or_err()?
        .join(".local")
        .join("share")
        .join("tmgr")
        .join("usage"))
}

pub fn ensure_data_dir(env: &Env) -> Result<PathBuf> {
    let dir = data_dir(env)?;
    create_private_dir(&dir)?;
    Ok(dir)
}

pub fn create_private_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub fn db_path(dir: &Path) -> PathBuf {
    dir.join("usage.db")
}

pub fn snapshots_dir(dir: &Path) -> PathBuf {
    dir.join("snapshots")
}

fn accounts_path(dir: &Path) -> PathBuf {
    dir.join("accounts.json")
}

pub fn default_accounts(env: &Env) -> Vec<Account> {
    let mut out = Vec::new();
    let candidates = [
        (Tool::Claude, "CLAUDE_CONFIG_DIR", ".claude"),
        (Tool::Codex, "CODEX_HOME", ".codex"),
    ];
    for (tool, var, dot) in candidates {
        let root = match env.get(var) {
            Some(v) => PathBuf::from(v),
            None => match &env.home {
                Some(h) => h.join(dot),
                None => continue,
            },
        };
        if root.is_dir() {
            out.push(Account {
                tool,
                label: tool.as_str().to_string(),
                root,
            });
        }
    }
    out
}

pub fn list_user_accounts(dir: &Path) -> Result<Vec<Account>> {
    let path = accounts_path(dir);
    match std::fs::read(&path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

fn save_user_accounts(dir: &Path, accounts: &[Account]) -> Result<()> {
    create_private_dir(dir)?;
    let tmp = dir.join("accounts.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(accounts)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, accounts_path(dir))?;
    Ok(())
}

fn valid_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 64
        && label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

pub fn add_account(dir: &Path, env: &Env, account: Account) -> Result<()> {
    if !valid_label(&account.label) {
        return Err(UsageError::Invalid(
            "account label must be 1-64 characters of letters, digits, '.', '_' or '-'".into(),
        ));
    }
    let all = all_accounts(dir, env)?;
    if all.iter().any(|a| a.label == account.label) || Tool::parse(&account.label).is_ok() {
        return Err(UsageError::Invalid("account label already exists".into()));
    }
    if all
        .iter()
        .any(|a| a.tool == account.tool && a.root == account.root)
    {
        return Err(UsageError::Invalid(
            "account root already registered".into(),
        ));
    }
    let mut user = list_user_accounts(dir)?;
    user.push(account);
    save_user_accounts(dir, &user)
}

pub fn remove_account(dir: &Path, label: &str) -> Result<bool> {
    let mut user = list_user_accounts(dir)?;
    let before = user.len();
    user.retain(|a| a.label != label);
    if user.len() == before {
        return Ok(false);
    }
    save_user_accounts(dir, &user)?;
    Ok(true)
}

pub fn all_accounts(dir: &Path, env: &Env) -> Result<Vec<Account>> {
    let mut out = default_accounts(env);
    for acc in list_user_accounts(dir)? {
        if !out
            .iter()
            .any(|a| a.label == acc.label || (a.tool == acc.tool && a.root == acc.root))
        {
            out.push(acc);
        }
    }
    Ok(out)
}
