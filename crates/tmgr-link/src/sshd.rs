use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Verified,
    Violations(Vec<String>),
    Unverified(String),
}

#[derive(Debug)]
pub struct Check {
    pub verdict: Verdict,
    pub authorized_keys_files: Vec<PathBuf>,
}

pub const MATCH_WARNING: &str = "sshd uses Match blocks: verify manually";

const REQUIRED: [&str; 4] = [
    "permituserenvironment",
    "forcecommand",
    "authorizedkeyscommand",
    "authorizedkeysfile",
];

fn safe_acceptenv(token: &str) -> bool {
    token == "LANG"
        || token == "LANGUAGE"
        || token == "LC_*"
        || (token.starts_with("LC_")
            && token.len() > 3
            && token.bytes().all(|b| b.is_ascii_uppercase() || b == b'_'))
}

fn directive(line: &str) -> (String, &str) {
    match line.split_once(char::is_whitespace) {
        Some((k, v)) => (k.to_ascii_lowercase(), v.trim()),
        None => (line.to_ascii_lowercase(), ""),
    }
}

pub fn evaluate(output: &str) -> Vec<String> {
    let mut v = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut tunnel_ok = false;
    for line in output.lines() {
        let (key, value) = directive(line);
        seen.push(key.clone());
        match key.as_str() {
            "permituserenvironment" if value != "no" => v.push(format!(
                "PermitUserEnvironment is {value:?}, it must be \"no\""
            )),
            "forcecommand" if !value.eq_ignore_ascii_case("none") => {
                v.push("sshd forces a command for this user (ForceCommand)".to_string())
            }
            "authorizedkeyscommand" if !value.eq_ignore_ascii_case("none") => v.push(
                "sshd uses AuthorizedKeysCommand, which could also return the key".to_string(),
            ),
            "permittunnel" if value == "no" => tunnel_ok = true,
            "permittunnel" => v.push(format!("PermitTunnel is {value:?}, it must be \"no\"")),
            "disableforwarding" if value == "yes" => tunnel_ok = true,
            "acceptenv" => {
                for token in value.split_whitespace() {
                    if !safe_acceptenv(token) {
                        v.push(format!(
                            "AcceptEnv accepts {token:?}; only LANG, LANGUAGE and LC_* are safe"
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    for k in REQUIRED {
        if !seen.iter().any(|s| s == k) {
            v.push(format!("sshd -T output does not state {k}"));
        }
    }
    if !tunnel_ok && !v.iter().any(|m| m.starts_with("PermitTunnel")) {
        v.push("neither PermitTunnel no nor DisableForwarding yes is in effect".to_string());
    }
    v
}

pub fn authorized_keys_tokens(output: &str) -> Vec<String> {
    for line in output.lines() {
        let (key, value) = directive(line);
        if key == "authorizedkeysfile" {
            return value
                .split_whitespace()
                .filter(|t| !t.eq_ignore_ascii_case("none"))
                .map(str::to_string)
                .collect();
        }
    }
    Vec::new()
}

pub fn expand_token(token: &str, home: &Path, user: &str, uid: u32) -> Option<PathBuf> {
    let mut out = String::new();
    let mut chars = token.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            'h' => out.push_str(home.to_str()?),
            'u' => out.push_str(user),
            'U' => out.push_str(&uid.to_string()),
            '%' => out.push('%'),
            _ => return None,
        }
    }
    let p = PathBuf::from(out);
    Some(if p.is_absolute() { p } else { home.join(p) })
}

pub fn default_files(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".ssh").join("authorized_keys"),
        home.join(".ssh").join("authorized_keys2"),
    ]
}

pub fn has_match_blocks(config: &str) -> bool {
    config.lines().any(|l| {
        let t = l.trim_start();
        let mut it = t.splitn(2, char::is_whitespace);
        it.next().is_some_and(|k| k.eq_ignore_ascii_case("match")) && it.next().is_some()
    })
}

pub trait Runner {
    fn effective(&self, user: &str, addr: &str) -> Result<String, String>;
    fn config_text(&self) -> Option<String>;
}

pub fn contexts(from: &[String]) -> Vec<String> {
    let mut addrs = vec!["127.0.0.1".to_string()];
    for cidr in from {
        let first = cidr.split('/').next().unwrap_or("").to_string();
        if !first.is_empty() && !addrs.contains(&first) {
            addrs.push(first);
        }
    }
    addrs
}

pub fn check_with(
    runner: &dyn Runner,
    user: &str,
    home: &Path,
    uid: u32,
    from: &[String],
) -> Check {
    let unverified = |why: String| Check {
        verdict: Verdict::Unverified(why),
        authorized_keys_files: default_files(home),
    };
    let mut violations: Vec<String> = Vec::new();
    let mut files: Option<Vec<PathBuf>> = None;
    for addr in contexts(from) {
        let output = match runner.effective(user, &addr) {
            Ok(o) => o,
            Err(why) => return unverified(why),
        };
        for msg in evaluate(&output) {
            if !violations.contains(&msg) {
                violations.push(msg);
            }
        }
        let mut expanded = Vec::new();
        for token in authorized_keys_tokens(&output) {
            match expand_token(&token, home, user, uid) {
                Some(p) => expanded.push(p),
                None => violations.push(format!(
                    "AuthorizedKeysFile token {token:?} cannot be expanded and inspected"
                )),
            }
        }
        let all = files.get_or_insert_with(Vec::new);
        for p in expanded {
            if !all.contains(&p) {
                all.push(p);
            }
        }
    }
    let authorized_keys_files = files.unwrap_or_else(|| default_files(home));
    let verdict = if !violations.is_empty() {
        Verdict::Violations(violations)
    } else if runner.config_text().is_some_and(|t| has_match_blocks(&t)) {
        Verdict::Unverified(MATCH_WARNING.to_string())
    } else {
        Verdict::Verified
    };
    Check {
        verdict,
        authorized_keys_files,
    }
}

struct Real;

fn find_sshd() -> Option<PathBuf> {
    let fixed = ["/usr/sbin", "/usr/local/sbin", "/sbin"]
        .into_iter()
        .map(PathBuf::from);
    let from_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>());
    fixed
        .chain(from_path)
        .map(|d| d.join("sshd"))
        .find(|p| p.is_file())
}

impl Runner for Real {
    fn effective(&self, user: &str, addr: &str) -> Result<String, String> {
        let sshd = find_sshd().ok_or("sshd was not found on this machine")?;
        let out = Command::new(sshd)
            .arg("-T")
            .arg("-C")
            .arg(format!("user={user},host=localhost,addr={addr}"))
            .output()
            .map_err(|_| "sshd -T could not be started")?;
        if out.status.success() && !out.stdout.is_empty() {
            Ok(String::from_utf8_lossy(&out.stdout).to_string())
        } else {
            Err("sshd -T could not read the effective configuration (it usually needs root)".into())
        }
    }

    fn config_text(&self) -> Option<String> {
        let mut text = std::fs::read_to_string("/etc/ssh/sshd_config").ok()?;
        if let Ok(dir) = std::fs::read_dir("/etc/ssh/sshd_config.d") {
            for entry in dir.flatten() {
                if let Ok(t) = std::fs::read_to_string(entry.path()) {
                    text.push('\n');
                    text.push_str(&t);
                }
            }
        }
        Some(text)
    }
}

pub fn check(user: &str, home: &Path, from: &[String]) -> Check {
    let uid = unsafe { libc::getuid() };
    check_with(&Real, user, home, uid, from)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "port 22\npermituserenvironment no\nforcecommand none\nauthorizedkeyscommand none\npermittunnel no\nauthorizedkeysfile .ssh/authorized_keys .ssh/authorized_keys2\nacceptenv LANG\nacceptenv LC_*\n";

    #[test]
    fn a_clean_configuration_passes() {
        assert!(evaluate(GOOD).is_empty());
    }

    #[test]
    fn dangerous_settings_fail_closed() {
        let bad = GOOD
            .replace("permituserenvironment no", "permituserenvironment yes")
            .replace("forcecommand none", "forcecommand /bin/true")
            .replace(
                "authorizedkeyscommand none",
                "authorizedkeyscommand /usr/bin/ak %u",
            );
        assert_eq!(evaluate(&bad).len(), 3);
        assert!(
            evaluate(&GOOD.replace("permituserenvironment no", "PermitUserEnvironment yes"))
                .iter()
                .any(|m| m.contains("PermitUserEnvironment"))
        );
    }

    #[test]
    fn missing_directives_fail_closed() {
        let v = evaluate("port 22\n");
        for k in REQUIRED {
            assert!(v.iter().any(|m| m.contains(k)), "{k}");
        }
        assert!(v.iter().any(|m| m.contains("PermitTunnel")));
    }

    #[test]
    fn accept_env_must_stay_small() {
        for ok in ["LANG", "LANGUAGE", "LC_*", "LC_ALL", "LC_CTYPE"] {
            assert!(
                evaluate(&format!("{GOOD}acceptenv {ok}\n")).is_empty(),
                "{ok}"
            );
        }
        for bad in [
            "*",
            "LD_PRELOAD",
            "DYLD_INSERT_LIBRARIES",
            "XDG_CONFIG_HOME",
            "PATH",
            "HOME",
            "SHELL",
            "LC*",
            "?",
            "TMGR_*",
            "!LANG",
        ] {
            let v = evaluate(&format!("{GOOD}acceptenv {bad}\n"));
            assert_eq!(v.len(), 1, "{bad}: {v:?}");
        }
        assert_eq!(evaluate(&format!("{GOOD}acceptenv LANG PATH\n")).len(), 1);
    }

    #[test]
    fn tunnelling_must_be_off() {
        let no_tunnel = GOOD.replace("permittunnel no", "permittunnel yes");
        assert!(evaluate(&no_tunnel)
            .iter()
            .any(|m| m.contains("PermitTunnel")));
        let df = GOOD.replace("permittunnel no\n", "disableforwarding yes\n");
        assert!(evaluate(&df).is_empty());
        let neither = GOOD.replace("permittunnel no\n", "disableforwarding no\n");
        assert_eq!(evaluate(&neither).len(), 1);
    }

    #[test]
    fn authorized_keys_files_are_expanded() {
        let home = Path::new("/home/u");
        let out = "authorizedkeysfile .ssh/authorized_keys /etc/keys/%u %h/k2 /srv/%U/x /d/%%\n";
        let t = authorized_keys_tokens(out);
        let paths: Vec<_> = t
            .iter()
            .map(|t| expand_token(t, home, "bob", 1000).unwrap())
            .collect();
        assert_eq!(
            paths,
            [
                PathBuf::from("/home/u/.ssh/authorized_keys"),
                PathBuf::from("/etc/keys/bob"),
                PathBuf::from("/home/u/k2"),
                PathBuf::from("/srv/1000/x"),
                PathBuf::from("/d/%"),
            ]
        );
        assert!(expand_token("/x/%z", home, "u", 1).is_none());
        assert!(authorized_keys_tokens("authorizedkeysfile none\n").is_empty());
    }

    #[test]
    fn match_blocks_are_detected() {
        assert!(has_match_blocks(
            "Port 22\nMatch User bob\n  X11Forwarding no\n"
        ));
        assert!(has_match_blocks("  match address 10.0.0.0/8\n"));
        assert!(!has_match_blocks(
            "# Match User bob\nPort 22\nMatchless 1\n"
        ));
    }

    struct Fake {
        outputs: Vec<(String, Result<String, String>)>,
        config: Option<String>,
        asked: std::cell::RefCell<Vec<String>>,
    }

    impl Runner for Fake {
        fn effective(&self, _user: &str, addr: &str) -> Result<String, String> {
            self.asked.borrow_mut().push(addr.to_string());
            self.outputs
                .iter()
                .find(|(a, _)| a == addr || a == "*")
                .map(|(_, r)| r.clone())
                .unwrap()
        }

        fn config_text(&self) -> Option<String> {
            self.config.clone()
        }
    }

    fn fake(good: bool, config: Option<&str>) -> Fake {
        Fake {
            outputs: vec![(
                "*".into(),
                if good {
                    Ok(GOOD.to_string())
                } else {
                    Err("no root".into())
                },
            )],
            config: config.map(str::to_string),
            asked: Default::default(),
        }
    }

    #[test]
    fn every_context_is_evaluated_including_the_from_addresses() {
        let f = Fake {
            outputs: vec![
                ("127.0.0.1".into(), Ok(GOOD.to_string())),
                (
                    "192.0.2.0".into(),
                    Ok(GOOD.replace("forcecommand none", "forcecommand /bin/false")),
                ),
                ("198.51.100.7".into(), Ok(GOOD.to_string())),
            ],
            config: None,
            asked: Default::default(),
        };
        let c = check_with(
            &f,
            "u",
            Path::new("/home/u"),
            1,
            &["192.0.2.0/24".into(), "198.51.100.7".into()],
        );
        assert_eq!(
            *f.asked.borrow(),
            ["127.0.0.1", "192.0.2.0", "198.51.100.7"]
        );
        assert!(matches!(c.verdict, Verdict::Violations(ref v) if v.len() == 1));
    }

    #[test]
    fn match_blocks_make_the_verdict_unverified() {
        let c = check_with(
            &fake(true, Some("Match User x\n")),
            "u",
            Path::new("/home/u"),
            1,
            &[],
        );
        assert_eq!(c.verdict, Verdict::Unverified(MATCH_WARNING.into()));
        let ok = check_with(
            &fake(true, Some("Port 22\n")),
            "u",
            Path::new("/home/u"),
            1,
            &[],
        );
        assert_eq!(ok.verdict, Verdict::Verified);
        assert_eq!(
            ok.authorized_keys_files,
            [
                PathBuf::from("/home/u/.ssh/authorized_keys"),
                PathBuf::from("/home/u/.ssh/authorized_keys2")
            ]
        );
    }

    #[test]
    fn an_unreadable_configuration_is_unverified_with_default_files() {
        let c = check_with(&fake(false, None), "u", Path::new("/home/u"), 1, &[]);
        assert!(matches!(c.verdict, Verdict::Unverified(_)));
        assert_eq!(c.authorized_keys_files.len(), 2);
    }

    #[test]
    fn an_unexpandable_authorized_keys_token_is_a_violation() {
        let out = GOOD.replace(".ssh/authorized_keys .ssh/authorized_keys2", "/x/%z");
        let f = Fake {
            outputs: vec![("*".into(), Ok(out))],
            config: None,
            asked: Default::default(),
        };
        let c = check_with(&f, "u", Path::new("/home/u"), 1, &[]);
        assert!(matches!(c.verdict, Verdict::Violations(_)));
    }
}
