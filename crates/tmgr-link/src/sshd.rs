use std::process::Command;

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Verified,
    Violations(Vec<String>),
    Unverified(String),
}

pub fn evaluate(output: &str) -> Vec<String> {
    let mut v = Vec::new();
    for line in output.lines() {
        let (key, value) = match line.split_once(char::is_whitespace) {
            Some((k, val)) => (k.to_ascii_lowercase(), val.trim()),
            None => (line.to_ascii_lowercase(), ""),
        };
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
            _ => {}
        }
    }
    v
}

fn find_sshd() -> Option<std::path::PathBuf> {
    let from_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>());
    let fixed = ["/usr/sbin", "/usr/local/sbin", "/sbin"]
        .into_iter()
        .map(std::path::PathBuf::from);
    from_path
        .chain(fixed)
        .map(|d| d.join("sshd"))
        .find(|p| p.is_file())
}

pub fn check(user: &str) -> Verdict {
    let Some(sshd) = find_sshd() else {
        return Verdict::Unverified("sshd was not found on this machine".into());
    };
    let out = Command::new(sshd)
        .arg("-T")
        .arg("-C")
        .arg(format!("user={user},host=localhost,addr=127.0.0.1"))
        .output();
    match out {
        Ok(o) if o.status.success() && !o.stdout.is_empty() => {
            let text = String::from_utf8_lossy(&o.stdout);
            let violations = evaluate(&text);
            if violations.is_empty() {
                Verdict::Verified
            } else {
                Verdict::Violations(violations)
            }
        }
        Ok(_) => Verdict::Unverified(
            "sshd -T could not read the effective configuration (it usually needs root)".into(),
        ),
        Err(_) => Verdict::Unverified("sshd -T could not be started".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "port 22\npermituserenvironment no\nforcecommand none\nauthorizedkeyscommand none\nauthorizedkeysfile .ssh/authorized_keys\n";

    #[test]
    fn a_clean_configuration_passes() {
        assert!(evaluate(GOOD).is_empty());
        assert!(evaluate("port 22\n").is_empty());
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
        assert_eq!(evaluate("permituserenvironment FOO=bar\n").len(), 1);
        assert_eq!(evaluate("PermitUserEnvironment yes\n").len(), 1);
    }
}
