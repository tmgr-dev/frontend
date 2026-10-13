use crate::audit::Audit;
use crate::grammar::{self, PubKey};
use crate::proto::{self, Channel, Framed, Reply};
use crate::store::Paths;
use serde_json::{json, Value};
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tmgr_core::config::write_atomic;
use tmgr_core::creds::{Backend, CredentialStore};
use tmgr_core::error::{Error, Result};
use tmgr_core::redact::Secret;

pub const KEY_ACCOUNT: &str = "tmgr-link-key";
pub const SSH_TIMEOUT: Duration = Duration::from_secs(20);

pub fn credential_store(paths: &Paths, preferred: Option<Backend>) -> CredentialStore {
    CredentialStore::new(&paths.config, KEY_ACCOUNT, preferred)
}

#[derive(Debug)]
pub struct Identity {
    pub public: String,
    pub fingerprint: String,
    pub backend: Backend,
    pub warning: Option<String>,
}

pub fn init(paths: &Paths, cred: &CredentialStore) -> Result<Identity> {
    if paths.public_key().exists() {
        return Err(
            Error::conflict("this machine already has a link identity").hint(
                "tmgr link list shows it; the public key is in link_key.pub next to links.toml",
            ),
        );
    }
    let private = ssh_key::PrivateKey::random(&mut rand_core::OsRng, ssh_key::Algorithm::Ed25519)
        .map_err(|_| Error::internal("cannot generate a key"))?;
    let pem = private
        .to_openssh(ssh_key::LineEnding::LF)
        .map_err(|_| Error::internal("cannot encode the key"))?;
    let public = private
        .public_key()
        .to_openssh()
        .map_err(|_| Error::internal("cannot encode the public key"))?;
    let fingerprint = private
        .public_key()
        .fingerprint(ssh_key::HashAlg::Sha256)
        .to_string();
    let saved = cred.save(&tmgr_core::creds::Tokens {
        access: Secret::new(pem.as_str().to_string()),
        refresh: None,
        expires_at: None,
    })?;
    cred.finalize(saved.backend);
    tmgr_core::config::ensure_private_dir(&paths.config)?;
    let public_line = format!("{public} tmgr-link");
    write_atomic(
        &paths.public_key(),
        format!("{public_line}\n").as_bytes(),
        0o644,
    )?;
    Audit::new(&paths.audit_dir()).append(
        "link.init",
        json!({"fingerprint": fingerprint, "backend": saved.backend.as_str()}),
    )?;
    Ok(Identity {
        public: public_line,
        fingerprint,
        backend: saved.backend,
        warning: saved.warning,
    })
}

pub fn identity(paths: &Paths) -> Result<Option<(String, String)>> {
    match std::fs::read_to_string(paths.public_key()) {
        Ok(text) => {
            let key = grammar::parse_pubkey(&text)?;
            Ok(Some((text.trim().to_string(), key.fingerprint)))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub struct KeyFile {
    _dir: tempfile::TempDir,
    pub path: PathBuf,
}

pub fn materialise(cred: &CredentialStore) -> Result<KeyFile> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let (tokens, _) = cred.load()?.ok_or_else(|| {
        Error::not_found("this machine has no link identity").hint("tmgr link init")
    })?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && p.is_dir());
    let mut builder = tempfile::Builder::new();
    builder.prefix("tmgr-link-");
    let dir = match &runtime {
        Some(r) => builder.tempdir_in(r)?,
        None => builder.tempdir()?,
    };
    std::fs::set_permissions(
        dir.path(),
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )?;
    let path = dir.path().join("key");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    f.write_all(tokens.access.expose().as_bytes())?;
    f.sync_all()?;
    Ok(KeyFile { _dir: dir, path })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshTarget {
    pub user: String,
    pub host: String,
    pub port: u16,
}

fn opt_value(path: &Path) -> Result<String> {
    let s = path
        .to_str()
        .filter(|s| !s.contains(['"', '\\', '\n', '\r']))
        .ok_or_else(|| Error::internal("a path contains characters ssh cannot be given safely"))?;
    let escaped = s.replace('%', "%%");
    Ok(if escaped.contains(char::is_whitespace) {
        format!("\"{escaped}\"")
    } else {
        escaped
    })
}

pub fn ssh_args(
    target: &SshTarget,
    key: &Path,
    known_hosts: &Path,
    name: &str,
) -> Result<Vec<OsString>> {
    grammar::link_name(name)?;
    let mut a: Vec<String> = ["-F", "none", "-T", "-a", "-x"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let opts = [
        "ClearAllForwardings=yes".to_string(),
        "BatchMode=yes".to_string(),
        "IdentitiesOnly=yes".to_string(),
        "IdentityAgent=none".to_string(),
        format!("IdentityFile={}", opt_value(key)?),
        "ControlMaster=no".to_string(),
        "ControlPath=none".to_string(),
        "PermitLocalCommand=no".to_string(),
        "ProxyCommand=none".to_string(),
        "KnownHostsCommand=none".to_string(),
        format!("UserKnownHostsFile={}", opt_value(known_hosts)?),
        "GlobalKnownHostsFile=none".to_string(),
        format!("HostKeyAlias=tmgr-link-{name}"),
        "StrictHostKeyChecking=yes".to_string(),
        "RequestTTY=no".to_string(),
    ];
    for o in opts {
        a.push("-o".into());
        a.push(o);
    }
    a.push("-p".into());
    a.push(target.port.to_string());
    a.push(format!("{}@{}", target.user, target.host));
    Ok(a.into_iter().map(OsString::from).collect())
}

fn tool(name: &str, fixed: &str) -> PathBuf {
    let p = PathBuf::from(fixed);
    if p.is_file() {
        p
    } else {
        PathBuf::from(name)
    }
}

pub fn known_hosts_host(name: &str) -> String {
    format!("tmgr-link-{name}")
}

fn is_pin_for(line: &str, name: &str) -> bool {
    let host = line.split_whitespace().next().unwrap_or("");
    host == format!("tmgr-link-{name}") || host.starts_with(&format!("[tmgr-link-{name}]:"))
}

fn rewrite_known_hosts(paths: &Paths, name: &str, add: Option<String>) -> Result<()> {
    tmgr_core::config::ensure_private_dir(&paths.config)?;
    let _lock = crate::store::lock(&paths.config.join("links.lock"))?;
    let path = paths.known_hosts();
    let existing = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let mut out: String = existing
        .lines()
        .filter(|l| !is_pin_for(l, name))
        .map(|l| format!("{l}\n"))
        .collect();
    if let Some(line) = add {
        out.push_str(&line);
        out.push('\n');
    }
    write_atomic(&path, out.as_bytes(), 0o600)
}

pub fn pin_host(paths: &Paths, name: &str, key: &PubKey) -> Result<()> {
    grammar::link_name(name)?;
    rewrite_known_hosts(
        paths,
        name,
        Some(format!("{} {}", known_hosts_host(name), key.openssh())),
    )
}

pub fn unpin_host(paths: &Paths, name: &str) -> Result<()> {
    if !paths.known_hosts().exists() {
        return Ok(());
    }
    rewrite_known_hosts(paths, name, None)
}

pub fn parse_keyscan(output: &str) -> Option<PubKey> {
    let mut found: Option<PubKey> = None;
    for line in output.lines().filter(|l| !l.starts_with('#')) {
        let mut parts = line.split_whitespace();
        let (_host, kind, blob) = (parts.next()?, parts.next()?, parts.next()?);
        if kind != "ssh-ed25519" {
            continue;
        }
        let key = grammar::parse_pubkey(&format!("ssh-ed25519 {blob}")).ok()?;
        match &found {
            Some(prev) if prev.blob != key.blob => return None,
            _ => found = Some(key),
        }
    }
    found
}

pub fn scan_host(host: &str, port: u16) -> Result<PubKey> {
    if grammar::parse_target(&format!("u@{host}")).is_err() {
        return Err(Error::usage("invalid host"));
    }
    let out = Command::new(tool("ssh-keyscan", "/usr/bin/ssh-keyscan"))
        .args(["-T", "10", "-t", "ed25519", "-p"])
        .arg(port.to_string())
        .arg(host)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| crate::link_err("ssh-keyscan could not be started"))?;
    parse_keyscan(&String::from_utf8_lossy(&out.stdout)).ok_or_else(|| {
        crate::link_err(format!(
            "could not read an ed25519 host key from {host}:{port}"
        ))
        .hint("check the address, and that the follower's sshd has an ed25519 host key")
    })
}

pub fn classify_stderr(text: &str, timed_out: bool) -> String {
    if timed_out {
        return "the follower did not answer in time".to_string();
    }
    let lower = text.to_ascii_lowercase();
    if lower.contains("host key verification failed")
        || lower.contains("remote host identification has changed")
        || lower.contains("host key is known for")
    {
        return "host key mismatch: the follower's key is not the pinned one".to_string();
    }
    if lower.contains("permission denied") {
        return "the follower refused this link key (not trusted there, or from= does not match)"
            .to_string();
    }
    let first = text.lines().map(str::trim).find(|l| !l.is_empty());
    match first {
        Some(l) => format!(
            "ssh failed: {}",
            tmgr_core::text::truncate_chars(&tmgr_core::text::sanitize_line(l), 160).0
        ),
        None => "the follower closed the connection".to_string(),
    }
}

type Pipe = Framed<ChildStdout, ChildStdin>;

pub struct Client {
    child: Arc<Mutex<Child>>,
    chan: Pipe,
    stderr: Arc<Mutex<Vec<u8>>>,
    timed_out: Arc<Mutex<bool>>,
    _cancel: mpsc::Sender<()>,
    next_id: u64,
}

impl Client {
    pub fn spawn(mut cmd: Command, timeout: Duration) -> Result<Client> {
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| crate::link_err("could not start ssh"))?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let mut err = child.stderr.take().expect("piped stderr");
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&stderr);
        std::thread::spawn(move || {
            let mut buf = [0u8; 1024];
            while let Ok(n) = err.read(&mut buf) {
                if n == 0 {
                    break;
                }
                if let Ok(mut s) = sink.lock() {
                    if s.len() < 8192 {
                        s.extend_from_slice(&buf[..n]);
                    }
                }
            }
        });
        let child = Arc::new(Mutex::new(child));
        let timed_out = Arc::new(Mutex::new(false));
        let (cancel, rx) = mpsc::channel::<()>();
        let (c, t) = (Arc::clone(&child), Arc::clone(&timed_out));
        std::thread::spawn(move || {
            if let Err(RecvTimeoutError::Timeout) = rx.recv_timeout(timeout) {
                if let Ok(mut flag) = t.lock() {
                    *flag = true;
                }
                if let Ok(mut ch) = c.lock() {
                    let _ = ch.kill();
                }
            }
        });
        Ok(Client {
            child,
            chan: Framed::new(stdout, stdin),
            stderr,
            timed_out,
            _cancel: cancel,
            next_id: 1,
        })
    }

    pub fn connect(
        paths: &Paths,
        cred: &CredentialStore,
        name: &str,
        target: &SshTarget,
    ) -> Result<(Client, KeyFile)> {
        let key = materialise(cred)?;
        let args = ssh_args(target, &key.path, &paths.known_hosts(), name)?;
        let mut cmd = Command::new(tool("ssh", "/usr/bin/ssh"));
        cmd.args(args);
        Ok((Client::spawn(cmd, SSH_TIMEOUT)?, key))
    }

    fn failure(&self) -> Error {
        std::thread::sleep(Duration::from_millis(100));
        let text = self
            .stderr
            .lock()
            .map(|b| String::from_utf8_lossy(&b).to_string())
            .unwrap_or_default();
        let timed_out = self.timed_out.lock().map(|f| *f).unwrap_or(false);
        crate::link_err(classify_stderr(&text, timed_out))
    }

    pub fn call(&mut self, method: &str, params: Value) -> Result<Reply> {
        let id = self.next_id;
        self.next_id += 1;
        if self.chan.send(&proto::request(id, method, params)).is_err() {
            return Err(self.failure());
        }
        match self.chan.recv() {
            Ok(Some(bytes)) => proto::parse_reply(&bytes, id)
                .map_err(|why| crate::link_err(format!("bad reply from the follower: {why}"))),
            Ok(None) | Err(_) => Err(self.failure()),
        }
    }

    pub fn hello(&mut self) -> Result<Value> {
        match self.call("hello", json!({"protocol": proto::PROTOCOL}))? {
            Reply::Ok(v)
                if v.get("protocol").and_then(Value::as_u64)
                    == Some(u64::from(proto::PROTOCOL)) =>
            {
                Ok(v)
            }
            Reply::Ok(_) => Err(crate::link_err(
                "the follower speaks another protocol version",
            )),
            Reply::Err { code, message } => Err(crate::link_err(format!(
                "the follower refused the handshake: {code}: {message}"
            ))),
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::testkeys;

    fn target() -> SshTarget {
        SshTarget {
            user: "deploy".into(),
            host: "example.com".into(),
            port: 22,
        }
    }

    #[test]
    fn ssh_argv_matches_the_design_exactly() {
        let args: Vec<String> = ssh_args(
            &target(),
            Path::new("/run/k/key"),
            Path::new("/home/u/.config/tmgr/link_known_hosts"),
            "spark",
        )
        .unwrap()
        .into_iter()
        .map(|a| a.into_string().unwrap())
        .collect();
        let expected = "-F none -T -a -x \
            -o ClearAllForwardings=yes -o BatchMode=yes -o IdentitiesOnly=yes -o IdentityAgent=none \
            -o IdentityFile=/run/k/key -o ControlMaster=no -o ControlPath=none -o PermitLocalCommand=no \
            -o ProxyCommand=none -o KnownHostsCommand=none \
            -o UserKnownHostsFile=/home/u/.config/tmgr/link_known_hosts -o GlobalKnownHostsFile=none \
            -o HostKeyAlias=tmgr-link-spark -o StrictHostKeyChecking=yes -o RequestTTY=no \
            -p 22 deploy@example.com";
        assert_eq!(
            args.join(" "),
            expected.split_whitespace().collect::<Vec<_>>().join(" ")
        );
    }

    #[test]
    fn ssh_option_values_are_quoted_and_percent_escaped() {
        let args = ssh_args(&target(), Path::new("/a b/key"), Path::new("/h%d/kh"), "x").unwrap();
        let joined: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert!(joined.contains(&"IdentityFile=\"/a b/key\"".to_string()));
        assert!(joined.contains(&"UserKnownHostsFile=/h%%d/kh".to_string()));
        assert!(ssh_args(&target(), Path::new("/a\"b/key"), Path::new("/kh"), "x").is_err());
        assert!(ssh_args(&target(), Path::new("/k"), Path::new("/kh"), "Bad").is_err());
    }

    #[test]
    fn known_hosts_use_the_bare_alias_whatever_the_port() {
        assert_eq!(known_hosts_host("spark"), "tmgr-link-spark");
    }

    #[test]
    fn pinning_replaces_only_its_own_lines_and_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::under(d.path());
        let (k1, k2, k3) = (testkeys::key(), testkeys::key(), testkeys::key());
        pin_host(&paths, "a", &k1).unwrap();
        pin_host(&paths, "b", &k2).unwrap();
        pin_host(&paths, "a", &k3).unwrap();
        let text = std::fs::read_to_string(paths.known_hosts()).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.contains(&format!("tmgr-link-a {}", k3.openssh())));
        assert!(text.contains(&format!("tmgr-link-b {}", k2.openssh())));
        assert!(!text.contains(&k1.blob));
        assert_eq!(
            std::fs::metadata(paths.known_hosts())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        unpin_host(&paths, "a").unwrap();
        let text = std::fs::read_to_string(paths.known_hosts()).unwrap();
        assert_eq!(text.lines().count(), 1);
        unpin_host(&Paths::under(&d.path().join("none")), "a").unwrap();
    }

    #[test]
    fn keyscan_output_is_parsed_strictly() {
        let k = testkeys::key();
        let out = format!(
            "# example.com:22 SSH-2.0-OpenSSH\nexample.com ssh-ed25519 {}\n",
            k.blob
        );
        assert_eq!(parse_keyscan(&out).unwrap(), k);
        assert!(parse_keyscan("").is_none());
        assert!(parse_keyscan("# only comments\n").is_none());
        assert!(parse_keyscan("h ssh-ed25519 AAAA\n").is_none());
        let other = testkeys::key();
        let two = format!("h ssh-ed25519 {}\nh ssh-ed25519 {}\n", k.blob, other.blob);
        assert!(parse_keyscan(&two).is_none());
    }

    #[test]
    fn ssh_errors_are_classified_without_leaking_noise() {
        assert!(
            classify_stderr("Host key verification failed.", false).contains("host key mismatch")
        );
        assert!(
            classify_stderr("WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!", false)
                .contains("host key mismatch")
        );
        assert!(
            classify_stderr("u@h: Permission denied (publickey).", false)
                .contains("refused this link key")
        );
        assert!(classify_stderr("", true).contains("in time"));
        assert!(classify_stderr("", false).contains("closed"));
        let weird = classify_stderr(
            "ssh: connect to host x port 22: Connection refused\u{1b}[2J",
            false,
        );
        assert!(weird.contains("Connection refused") && !weird.contains('\u{1b}'));
    }

    #[test]
    fn init_stores_the_key_in_a_private_file_and_never_overwrites() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::under(d.path());
        let cred = CredentialStore::with_keychain(
            &paths.config,
            KEY_ACCOUNT,
            Some(Backend::File),
            Box::new(NoKeychain),
        );
        let id = init(&paths, &cred).unwrap();
        assert!(id.public.starts_with("ssh-ed25519 ") && id.public.ends_with(" tmgr-link"));
        assert!(id.fingerprint.starts_with("SHA256:"));
        assert_eq!(id.backend, Backend::File);
        let creds = paths.config.join("credentials.json");
        assert_eq!(
            std::fs::metadata(&creds).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!std::fs::read_to_string(paths.public_key())
            .unwrap()
            .contains("PRIVATE"));
        assert_eq!(identity(&paths).unwrap().unwrap().1, id.fingerprint);
        assert_eq!(init(&paths, &cred).unwrap_err().exit_code(), 6);
        let key = materialise(&cred).unwrap();
        assert_eq!(
            std::fs::metadata(&key.path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(key.path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let pem = std::fs::read_to_string(&key.path).unwrap();
        assert!(ssh_key::PrivateKey::from_openssh(&pem).is_ok());
        let dir = key.path.parent().unwrap().to_path_buf();
        drop(key);
        assert!(!dir.exists());
    }

    struct NoKeychain;
    impl tmgr_core::creds::Keychain for NoKeychain {
        fn get(&self, _: &str) -> keyring::Result<String> {
            Err(keyring::Error::NoEntry)
        }
        fn set(&self, _: &str, _: &str) -> keyring::Result<()> {
            Err(keyring::Error::NoEntry)
        }
        fn delete(&self, _: &str) -> keyring::Result<()> {
            Ok(())
        }
    }
}
