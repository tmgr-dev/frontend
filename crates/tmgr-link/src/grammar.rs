use std::net::IpAddr;
use tmgr_core::error::{Error, Result};

pub const MAX_CIDRS: usize = 16;

pub fn link_name(s: &str) -> Result<&str> {
    let ok = (1..=32).contains(&s.len())
        && !s.starts_with('-')
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if ok {
        Ok(s)
    } else {
        Err(Error::usage(format!("invalid link name: {}", shown(s)))
            .hint("1 to 32 characters from a-z, 0-9 and -, not starting with -"))
    }
}

pub fn agent_name(s: &str) -> Result<&str> {
    let mut chars = s.chars();
    let ok = (1..=32).contains(&s.len())
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c));
    if ok {
        Ok(s)
    } else {
        Err(Error::usage(format!("invalid agent name: {}", shown(s)))
            .hint("lowercase letters, digits, . _ - (at most 32)"))
    }
}

pub fn token(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:#/-".contains(&b))
}

fn shown(s: &str) -> String {
    let t: String = s
        .chars()
        .take(40)
        .map(|c| if c.is_control() { '?' } else { c })
        .collect();
    format!("{t:?}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PubKey {
    pub blob: String,
    pub fingerprint: String,
}

impl PubKey {
    pub fn openssh(&self) -> String {
        format!("ssh-ed25519 {}", self.blob)
    }
}

pub fn parse_pubkey(input: &str) -> Result<PubKey> {
    let input = input.trim();
    let bad = |why: &str| {
        Error::usage(format!("invalid public key: {why}"))
            .hint("expected: ssh-ed25519 <base64 key> [comment]")
    };
    if input.len() > 512 || input.chars().any(|c| c.is_control()) {
        return Err(bad("unexpected characters"));
    }
    let mut parts = input.split_whitespace();
    if parts.next() != Some("ssh-ed25519") {
        return Err(bad("only ssh-ed25519 keys without options are accepted"));
    }
    let blob = parts.next().ok_or_else(|| bad("the key data is missing"))?;
    let comment: Vec<&str> = parts.collect();
    if comment.join(" ").len() > 64 || !comment.iter().all(|c| c.is_ascii()) {
        return Err(bad("the comment is too long or not ASCII"));
    }
    let key = ssh_key::PublicKey::from_openssh(&format!("ssh-ed25519 {blob}"))
        .map_err(|_| bad("the key data is not a valid ed25519 key"))?;
    if key.algorithm() != ssh_key::Algorithm::Ed25519 {
        return Err(bad("not an ed25519 key"));
    }
    let canonical = key
        .to_openssh()
        .map_err(|_| bad("the key cannot be re-encoded"))?;
    let blob = canonical
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| bad("the key cannot be re-encoded"))?;
    Ok(PubKey {
        blob: blob.to_string(),
        fingerprint: key.fingerprint(ssh_key::HashAlg::Sha256).to_string(),
    })
}

pub fn parse_cidr_list(items: &[String]) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for raw in items {
        for item in raw.split(',') {
            out.push(parse_cidr(item.trim())?);
        }
    }
    if out.len() > MAX_CIDRS {
        return Err(Error::usage("too many --from entries (at most 16)"));
    }
    out.sort();
    out.dedup();
    Ok(out)
}

pub fn parse_cidr(s: &str) -> Result<String> {
    let bad = || {
        Error::usage(format!("invalid CIDR: {}", shown(s)))
            .hint("an IP address with an optional /prefix, e.g. 192.0.2.0/24")
    };
    let (addr, prefix) = match s.split_once('/') {
        Some((a, p)) => (a, Some(p)),
        None => (s, None),
    };
    let ip: IpAddr = addr.parse().map_err(|_| bad())?;
    let max = if ip.is_ipv4() { 32 } else { 128 };
    match prefix {
        None => Ok(ip.to_string()),
        Some(p) => {
            if p.is_empty() || p.len() > 3 || !p.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad());
            }
            let n: u32 = p.parse().map_err(|_| bad())?;
            if n > max {
                return Err(bad());
            }
            Ok(format!("{ip}/{n}"))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub user: String,
    pub host: String,
}

pub fn parse_target(s: &str) -> Result<Target> {
    let bad =
        |why: &str| Error::usage(format!("invalid --ssh target: {why}")).hint("expected user@host");
    let (user, host) = s.split_once('@').ok_or_else(|| bad("user@host expected"))?;
    let mut uc = user.chars();
    let user_ok = (1..=32).contains(&user.len())
        && uc
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && uc.all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
    if !user_ok {
        return Err(bad("bad user name"));
    }
    if !valid_host(host) {
        return Err(bad("bad host name"));
    }
    Ok(Target {
        user: user.to_string(),
        host: host.to_string(),
    })
}

fn valid_host(host: &str) -> bool {
    if host.parse::<IpAddr>().is_ok() {
        return true;
    }
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    host.split('.').all(|label| {
        (1..=63).contains(&label.len())
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

pub fn parse_port(n: u32) -> Result<u16> {
    u16::try_from(n)
        .ok()
        .filter(|p| *p > 0)
        .ok_or_else(|| Error::usage("invalid port").hint("1 to 65535"))
}

#[cfg(test)]
pub(crate) mod testkeys {
    use super::*;

    pub fn generate() -> String {
        let k = ssh_key::PrivateKey::random(&mut rand_core::OsRng, ssh_key::Algorithm::Ed25519)
            .unwrap();
        k.public_key().to_openssh().unwrap()
    }

    pub fn key() -> PubKey {
        parse_pubkey(&generate()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_names_follow_the_grammar() {
        for ok in ["a", "laptop", "my-box-2", &"a".repeat(32)] {
            assert!(link_name(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "Laptop",
            "my_box",
            "a b",
            "-x",
            "a\"b",
            "a\nb",
            "x;y",
            "..",
            &"a".repeat(33),
            "é",
        ] {
            assert!(link_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn agent_names_are_lowercase_tokens() {
        assert!(agent_name("claude-code").is_ok());
        assert!(agent_name("codex").is_ok());
        assert!(agent_name("Claude").is_err());
        assert!(agent_name("-x").is_err());
        assert!(agent_name("a b").is_err());
    }

    #[test]
    fn pubkeys_must_be_plain_ed25519() {
        let line = testkeys::generate();
        let k = parse_pubkey(&format!("{line} laptop")).unwrap();
        assert!(k.fingerprint.starts_with("SHA256:"));
        assert_eq!(
            k.openssh(),
            line.split(' ').take(2).collect::<Vec<_>>().join(" ")
        );
        let blob = line.split(' ').nth(1).unwrap();
        for bad in [
            format!("command=\"x\" {line}"),
            format!("restrict {line}"),
            format!("ssh-rsa {blob}"),
            "ssh-ed25519".to_string(),
            "ssh-ed25519 AAAA".to_string(),
            "ssh-ed25519 !!!!".to_string(),
            format!("{line}\nssh-ed25519 {blob}"),
            format!("{line} x\u{0}y"),
            String::new(),
        ] {
            assert!(parse_pubkey(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn cidr_lists_are_validated_and_normalised() {
        let got =
            parse_cidr_list(&["192.0.2.0/24,198.51.100.7".into(), "2001:db8::/32".into()]).unwrap();
        assert_eq!(got, ["192.0.2.0/24", "198.51.100.7", "2001:db8::/32"]);
        for bad in [
            "192.0.2.0/33",
            "192.0.2.0/",
            "example.com",
            "192.0.2.0/24\"",
            "*",
            "192.0.2.256",
            "!192.0.2.1",
            "192.0.2.0/-1",
        ] {
            assert!(parse_cidr_list(&[bad.to_string()]).is_err(), "{bad}");
        }
    }

    #[test]
    fn ssh_targets_cannot_smuggle_options() {
        let t = parse_target("deploy@example.com").unwrap();
        assert_eq!(
            (t.user.as_str(), t.host.as_str()),
            ("deploy", "example.com")
        );
        assert!(parse_target("u@192.0.2.1").is_ok());
        assert!(parse_target("u@2001:db8::1").is_ok());
        for bad in [
            "example.com",
            "u@-oProxyCommand=x",
            "-u@host",
            "u@ho st",
            "u@host;ls",
            "u@",
            "@host",
            "u@host/..",
            "u@a..b",
        ] {
            assert!(parse_target(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn ports_are_16_bit_and_non_zero() {
        assert_eq!(parse_port(22).unwrap(), 22);
        assert!(parse_port(0).is_err());
        assert!(parse_port(65536).is_err());
    }
}
