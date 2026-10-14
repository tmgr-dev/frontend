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

pub const JUMP_GUIDANCE: &str = "use a dedicated jump account with a Match User block: AllowTcpForwarding local, AllowStreamLocalForwarding no, PermitTunnel no, DisableForwarding no, X11Forwarding no, AllowAgentForwarding no";

pub fn evaluate_jump(output: &str) -> Vec<String> {
    let mut v = Vec::new();
    let mut state: std::collections::BTreeMap<String, String> = Default::default();
    for line in output.lines() {
        let (key, value) = directive(line);
        state.insert(key, value.to_ascii_lowercase());
    }
    let want = |k: &str, ok: &[&str], v: &mut Vec<String>| match state.get(k) {
        None => v.push(format!("sshd -T output does not state {k}")),
        Some(val) if !ok.contains(&val.as_str()) => v.push(format!(
            "{k} is {val:?}, a jump key needs {}",
            ok.join(" or ")
        )),
        _ => {}
    };
    want("allowtcpforwarding", &["local", "yes"], &mut v);
    want("allowstreamlocalforwarding", &["no"], &mut v);
    want("permittunnel", &["no"], &mut v);
    want("disableforwarding", &["no"], &mut v);
    if !v.is_empty() {
        v.push(JUMP_GUIDANCE.to_string());
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

pub fn ambiguous(raw: &str, expanded: &Path) -> Option<String> {
    let bad = |s: &str| s.contains(['*', '?', '[', '"', '\'', '\\']);
    if bad(raw) || expanded.to_str().is_none_or(bad) {
        Some(format!(
            "AuthorizedKeysFile entry {raw:?} has glob or quoting characters and cannot be inspected reliably"
        ))
    } else {
        None
    }
}

pub fn default_files(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".ssh").join("authorized_keys"),
        home.join(".ssh").join("authorized_keys2"),
    ]
}

const MAX_INCLUDE_DEPTH: usize = 8;

fn wildcard(pattern: &[u8], name: &[u8]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some((b'*', rest)) => (0..=name.len()).any(|i| wildcard(rest, &name[i..])),
        Some((b'?', rest)) => !name.is_empty() && wildcard(rest, &name[1..]),
        Some((c, rest)) => name.first() == Some(c) && wildcard(rest, &name[1..]),
    }
}

pub fn expand_glob(pattern: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = vec![PathBuf::from("/")];
    for comp in pattern.components().skip(1) {
        let s = comp
            .as_os_str()
            .to_str()
            .ok_or("an Include path is not valid UTF-8")?;
        if s.contains('[') || s.contains('"') {
            return Err(format!("the Include pattern {s:?} is not supported"));
        }
        let mut next = Vec::new();
        for base in &paths {
            if s.contains(['*', '?']) {
                let entries = match std::fs::read_dir(base) {
                    Ok(e) => e,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(_) => return Err(format!("cannot list {}", base.display())),
                };
                for entry in entries.flatten() {
                    if let Some(n) = entry.file_name().to_str() {
                        if wildcard(s.as_bytes(), n.as_bytes()) {
                            next.push(base.join(n));
                        }
                    }
                }
            } else {
                next.push(base.join(s));
            }
        }
        paths = next;
    }
    paths.retain(|p| p.exists());
    paths.sort();
    Ok(paths)
}

pub fn read_config_tree(root: &Path, base: &Path) -> Result<String, String> {
    fn walk(
        path: &Path,
        base: &Path,
        depth: usize,
        seen: &mut Vec<PathBuf>,
        out: &mut String,
    ) -> Result<(), String> {
        if depth > MAX_INCLUDE_DEPTH {
            return Err("sshd_config Include nesting is too deep".into());
        }
        let canon =
            std::fs::canonicalize(path).map_err(|_| format!("cannot read {}", path.display()))?;
        if seen.contains(&canon) {
            return Err("sshd_config has an Include cycle".into());
        }
        let text = std::fs::read_to_string(&canon)
            .map_err(|_| format!("cannot read {}", canon.display()))?;
        seen.push(canon);
        out.push_str(&text);
        out.push('\n');
        for line in text.lines() {
            let t = line.trim_start();
            let mut words = t.split_whitespace();
            if !words
                .next()
                .is_some_and(|k| k.eq_ignore_ascii_case("include"))
            {
                continue;
            }
            for pattern in words.take_while(|w| !w.starts_with('#')) {
                let p = Path::new(pattern);
                let full = if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    base.join(p)
                };
                for file in expand_glob(&full)? {
                    if file.is_file() {
                        walk(&file, base, depth + 1, seen, out)?;
                    }
                }
            }
        }
        seen.pop();
        Ok(())
    }
    let mut out = String::new();
    walk(root, base, 0, &mut Vec::new(), &mut out)?;
    Ok(out)
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
    fn config_text(&self) -> Result<String, String>;
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
    check_inner(runner, user, home, uid, from, false)
}

fn check_inner(
    runner: &dyn Runner,
    user: &str,
    home: &Path,
    uid: u32,
    from: &[String],
    jump: bool,
) -> Check {
    let unverified = |why: String| Check {
        verdict: Verdict::Unverified(why),
        authorized_keys_files: default_files(home),
    };
    let mut violations: Vec<String> = Vec::new();
    let mut doubts: Vec<String> = Vec::new();
    let mut files: Option<Vec<PathBuf>> = None;
    for addr in contexts(from) {
        let output = match runner.effective(user, &addr) {
            Ok(o) => o,
            Err(why) => return unverified(why),
        };
        let mut found = evaluate(&output);
        if jump {
            found.extend(evaluate_jump(&output));
            found.retain(|m| !m.starts_with("neither PermitTunnel no nor DisableForwarding"));
        }
        for msg in found {
            if !violations.contains(&msg) {
                violations.push(msg);
            }
        }
        let mut expanded = Vec::new();
        for token in authorized_keys_tokens(&output) {
            match expand_token(&token, home, user, uid) {
                Some(p) => {
                    if let Some(why) = ambiguous(&token, &p) {
                        if !doubts.contains(&why) {
                            doubts.push(why);
                        }
                    }
                    expanded.push(p)
                }
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
    } else {
        match runner.config_text() {
            Err(why) => doubts.insert(0, why),
            Ok(t) if has_match_blocks(&t) => doubts.push(MATCH_WARNING.to_string()),
            Ok(_) => {}
        }
        if doubts.is_empty() {
            Verdict::Verified
        } else {
            Verdict::Unverified(doubts.join("; "))
        }
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

    fn config_text(&self) -> Result<String, String> {
        read_config_tree(Path::new("/etc/ssh/sshd_config"), Path::new("/etc/ssh"))
    }
}

pub fn check_jump(user: &str, home: &Path, from: &[String]) -> Check {
    let uid = unsafe { libc::getuid() };
    check_inner(&Real, user, home, uid, from, true)
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
    fn a_jump_needs_local_forwarding_and_nothing_else() {
        let ok = format!(
            "{GOOD}allowtcpforwarding local\nallowstreamlocalforwarding no\ndisableforwarding no\n"
        );
        assert!(evaluate_jump(&ok).is_empty());
        assert!(
            evaluate_jump(&ok.replace("allowtcpforwarding local", "allowtcpforwarding yes"))
                .is_empty()
        );
        for (from, to) in [
            ("allowtcpforwarding local", "allowtcpforwarding no"),
            ("allowtcpforwarding local", "allowtcpforwarding remote"),
            (
                "allowstreamlocalforwarding no",
                "allowstreamlocalforwarding yes",
            ),
            (
                "allowstreamlocalforwarding no",
                "allowstreamlocalforwarding local",
            ),
            ("disableforwarding no", "disableforwarding yes"),
            ("permittunnel no", "permittunnel yes"),
        ] {
            let v = evaluate_jump(&ok.replace(from, to));
            assert!(
                v.len() >= 2 && v.last().unwrap().contains("Match User"),
                "{to}: {v:?}"
            );
        }
        assert!(evaluate_jump(GOOD)
            .iter()
            .any(|m| m.contains("does not state")));
    }

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
        config: Result<String, String>,
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

        fn config_text(&self) -> Result<String, String> {
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
            config: config
                .map(str::to_string)
                .ok_or_else(|| "cannot read sshd_config".to_string()),
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
            config: Ok(String::new()),
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
            config: Ok(String::new()),
            asked: Default::default(),
        };
        let c = check_with(&f, "u", Path::new("/home/u"), 1, &[]);
        assert!(matches!(c.verdict, Verdict::Violations(_)));
    }

    #[test]
    fn an_unreadable_config_never_verifies_even_when_sshd_t_worked() {
        let c = check_with(&fake(true, None), "u", Path::new("/home/u"), 1, &[]);
        assert!(matches!(c.verdict, Verdict::Unverified(ref w) if w.contains("cannot read")));
    }

    #[test]
    fn glob_or_quoted_authorized_keys_entries_are_unverified() {
        for raw in [
            "/etc/keys/*",
            "/etc/keys/a?",
            "/etc/[ab]",
            "\"/x y\"",
            "/a\\ b",
        ] {
            let out = GOOD.replace(".ssh/authorized_keys .ssh/authorized_keys2", raw);
            let f = Fake {
                outputs: vec![("*".into(), Ok(out))],
                config: Ok(String::new()),
                asked: Default::default(),
            };
            let c = check_with(&f, "u", Path::new("/home/u"), 1, &[]);
            assert!(
                matches!(c.verdict, Verdict::Unverified(ref w) if w.contains("cannot be inspected")),
                "{raw}: {:?}",
                c.verdict
            );
        }
        let f = Fake {
            outputs: vec![("*".into(), Ok(GOOD.to_string()))],
            config: Ok(String::new()),
            asked: Default::default(),
        };
        assert_eq!(
            check_with(&f, "u", Path::new("/home/u"), 1, &[]).verdict,
            Verdict::Verified
        );
    }

    fn tree() -> (tempfile::TempDir, PathBuf) {
        let d = crate::testutil::private_tempdir();
        let root = std::fs::canonicalize(d.path()).unwrap();
        (d, root)
    }

    #[test]
    fn includes_are_followed_with_globs_relative_to_the_base() {
        let (_d, root) = tree();
        std::fs::create_dir(root.join("conf.d")).unwrap();
        std::fs::write(
            root.join("sshd_config"),
            "Port 22\nInclude conf.d/*.conf extra?.conf\n",
        )
        .unwrap();
        std::fs::write(root.join("conf.d/10.conf"), "X11Forwarding no\n").unwrap();
        std::fs::write(
            root.join("conf.d/20.conf"),
            "Include conf.d/sub/deep.conf\n",
        )
        .unwrap();
        std::fs::write(root.join("conf.d/skip.txt"), "Match User nobody\n").unwrap();
        std::fs::create_dir(root.join("conf.d/sub")).unwrap();
        std::fs::write(
            root.join("conf.d/sub/deep.conf"),
            "Match Address 10.0.0.0/8\n",
        )
        .unwrap();
        std::fs::write(root.join("extra1.conf"), "# nothing\n").unwrap();
        let text = read_config_tree(&root.join("sshd_config"), &root).unwrap();
        assert!(has_match_blocks(&text));
        assert!(text.contains("X11Forwarding no") && !text.contains("nobody"));
    }

    #[test]
    fn include_cycles_depth_and_missing_roots_are_errors() {
        let (_d, root) = tree();
        std::fs::write(root.join("a"), "Include b\n").unwrap();
        std::fs::write(root.join("b"), "Include a\n").unwrap();
        assert!(read_config_tree(&root.join("a"), &root)
            .unwrap_err()
            .contains("cycle"));
        for i in 0..12 {
            std::fs::write(root.join(format!("n{i}")), format!("Include n{}\n", i + 1)).unwrap();
        }
        std::fs::write(root.join("n12"), "Port 22\n").unwrap();
        assert!(read_config_tree(&root.join("n0"), &root)
            .unwrap_err()
            .contains("deep"));
        assert!(read_config_tree(&root.join("missing"), &root).is_err());
        std::fs::write(root.join("c"), "Include [ab].conf\n").unwrap();
        assert!(read_config_tree(&root.join("c"), &root).is_err());
        std::fs::write(root.join("ok"), "Include nomatch*.conf\nPort 22\n").unwrap();
        assert!(read_config_tree(&root.join("ok"), &root).is_ok());
    }

    #[test]
    fn an_unreadable_include_is_an_error() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let (_d, root) = tree();
        std::fs::write(root.join("main"), "Include secret.conf\n").unwrap();
        std::fs::write(root.join("secret.conf"), "Match User x\n").unwrap();
        std::fs::set_permissions(
            root.join("secret.conf"),
            std::fs::Permissions::from_mode(0o000),
        )
        .unwrap();
        assert!(read_config_tree(&root.join("main"), &root).is_err());
    }
}
