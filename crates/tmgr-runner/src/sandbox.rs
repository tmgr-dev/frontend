use std::collections::BTreeMap;
use std::fs::DirBuilder;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

pub struct SandboxSpec {
    pub session_id: String,
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub home: PathBuf,
    pub real_home: PathBuf,
    pub runner_dir: PathBuf,
    pub self_exe: PathBuf,
    pub env_pass: Vec<String>,
    pub base_env: Vec<(String, String)>,
}

pub struct Launch {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
}

const ENV_COPY: &[&str] = &[
    "PATH",
    "USER",
    "LOGNAME",
    "LANG",
    "SHELL",
    "TMPDIR",
    "COLORTERM",
    "TERM_PROGRAM",
];

const ENV_BLOCK_EXACT: &[&str] = &[
    "NODE_OPTIONS",
    "NODE_PATH",
    "PYTHONSTARTUP",
    "PYTHONPATH",
    "PYTHONHOME",
    "PERL5OPT",
    "PERL5LIB",
    "RUBYOPT",
    "RUBYLIB",
    "BASH_ENV",
    "ENV",
    "PROMPT_COMMAND",
    "SSH_AUTH_SOCK",
    "DBUS_SESSION_BUS_ADDRESS",
    "XDG_RUNTIME_DIR",
    "HOME",
    "TERM",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_ASKPASS",
    "GIT_EXEC_PATH",
    "GIT_EXTERNAL_DIFF",
    "GIT_PAGER",
    "GIT_EDITOR",
    "GIT_TEMPLATE_DIR",
    "GIT_PROXY_COMMAND",
];

const ENV_BLOCK_PREFIX: &[&str] = &[
    "AWS_",
    "GH_",
    "LD_",
    "DYLD_",
    "TMGR_",
    "XDG_",
    "GIT_CONFIG",
    "GIT_SSH",
];

const ENV_BLOCK_CONTAINS: &[&str] = &["TOKEN", "SECRET", "KEY", "PASSWORD"];

const HOME_DENY: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".config/tmgr",
    ".tmgr.dev",
    ".netrc",
    ".docker/config.json",
    ".kube",
    "Library/Keychains",
    "Library/Application Support/dev.tmgr.cli",
    "Library/Application Support/Google/Chrome",
    "Library/Application Support/Firefox",
    "Library/Safari",
    ".mozilla",
    ".config/google-chrome",
    ".config/chromium",
    ".local/share/keyrings",
    ".cargo/credentials",
    ".cargo/credentials.toml",
];

const HOME_READ: &[&str] = &[
    ".cargo/bin",
    ".cargo/registry",
    ".cargo/git",
    ".rustup",
    ".nvm",
    ".volta",
    ".bun",
    ".deno",
    ".local/bin",
    ".local/share/mise",
    ".local/share/pnpm",
    ".sdkman",
    ".pyenv",
    ".rbenv",
    ".npm",
];

pub fn env_name_blocked(name: &str) -> bool {
    let up = name.to_ascii_uppercase();
    ENV_BLOCK_EXACT.contains(&up.as_str())
        || ENV_BLOCK_PREFIX.iter().any(|p| up.starts_with(p))
        || ENV_BLOCK_CONTAINS.iter().any(|c| up.contains(c))
}

fn env_name_valid(name: &str) -> bool {
    !name.is_empty() && !name.contains(['=', '\0'])
}

fn build_env(spec: &SandboxSpec, home: &Path, tmp: &Path) -> Result<Vec<(String, String)>, String> {
    let mut env = BTreeMap::new();
    for (k, v) in &spec.base_env {
        let copy = ENV_COPY.contains(&k.as_str()) || k.starts_with("LC_");
        if copy && env_name_valid(k) && !v.contains('\0') {
            env.insert(k.clone(), v.clone());
        }
    }
    for name in &spec.env_pass {
        if !env_name_valid(name) || env_name_blocked(name) {
            continue;
        }
        if let Some((_, v)) = spec.base_env.iter().find(|(k, _)| k == name) {
            if !v.contains('\0') {
                env.insert(name.clone(), v.clone());
            }
        }
    }
    let h = path_str(home)?;
    env.insert("HOME".into(), h.clone());
    env.insert("TERM".into(), "xterm-256color".into());
    env.insert("TMGR_SESSION_ID".into(), spec.session_id.clone());
    env.insert("XDG_CONFIG_HOME".into(), format!("{h}/.config"));
    env.insert("XDG_CACHE_HOME".into(), format!("{h}/.cache"));
    env.insert("XDG_DATA_HOME".into(), format!("{h}/.local/share"));
    env.insert("TMPDIR".into(), path_str(tmp)?);
    Ok(env.into_iter().collect())
}

fn path_str(p: &Path) -> Result<String, String> {
    match p.to_str() {
        Some(s) if !s.contains(['\0', '\n', '\r']) => Ok(s.to_string()),
        _ => Err(format!(
            "path is not valid UTF-8 or has control characters: {}",
            p.display()
        )),
    }
}

fn canon_lossy(p: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut cur = p.to_path_buf();
    loop {
        if let Ok(c) = cur.canonicalize() {
            return rest.iter().rev().fold(c, |a, s| a.join(s));
        }
        match (cur.file_name().map(|s| s.to_os_string()), cur.parent()) {
            (Some(n), Some(par)) => {
                rest.push(n);
                cur = par.to_path_buf();
            }
            _ => return p.to_path_buf(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyPaths {
    pub real_home: PathBuf,
    pub runner_dir: PathBuf,
    pub home: PathBuf,
    pub read: Vec<PathBuf>,
    pub write: Vec<PathBuf>,
    pub deny: Vec<PathBuf>,
    pub write_prefixes: Vec<String>,
}

#[cfg(target_os = "macos")]
fn xcrun_cache_prefix() -> Option<String> {
    let mut buf = vec![0u8; 1024];
    let n = unsafe {
        libc::confstr(
            libc::_CS_DARWIN_USER_TEMP_DIR,
            buf.as_mut_ptr().cast(),
            buf.len(),
        )
    };
    if n == 0 || n > buf.len() {
        return None;
    }
    buf.truncate(n - 1);
    let dir = PathBuf::from(String::from_utf8(buf).ok()?)
        .canonicalize()
        .ok()?;
    Some(format!("{}/xcrun_db", dir.to_str()?.trim_end_matches('/')))
}

#[cfg(not(target_os = "macos"))]
fn xcrun_cache_prefix() -> Option<String> {
    None
}

struct Plan {
    cwd: PathBuf,
    paths: PolicyPaths,
    env: Vec<(String, String)>,
}

fn require_abs(what: &str, p: &Path) -> Result<(), String> {
    if p.is_absolute() {
        Ok(())
    } else {
        Err(format!("{what} must be an absolute path: {}", p.display()))
    }
}

fn push_unique(v: &mut Vec<PathBuf>, p: PathBuf) {
    if !v.contains(&p) {
        v.push(p);
    }
}

fn private_dirs(dirs: &[&Path]) -> Result<(), String> {
    for d in dirs {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(d)
            .map_err(|e| format!("cannot create {}: {e}", d.display()))?;
    }
    Ok(())
}

fn plan(spec: &SandboxSpec) -> Result<Plan, String> {
    if spec.argv.is_empty() {
        return Err("empty command".into());
    }
    require_abs("cwd", &spec.cwd)?;
    require_abs("home", &spec.home)?;
    require_abs("real home", &spec.real_home)?;
    require_abs("runner dir", &spec.runner_dir)?;
    let cwd = spec
        .cwd
        .canonicalize()
        .map_err(|e| format!("cwd {} is not usable: {e}", spec.cwd.display()))?;
    if !cwd.is_dir() {
        return Err(format!("cwd {} is not a directory", cwd.display()));
    }
    let real_home = canon_lossy(&spec.real_home);
    let runner_dir = canon_lossy(&spec.runner_dir);
    let home = canon_lossy(&spec.home);
    let homes_dir = home
        .parent()
        .ok_or_else(|| "session home has no parent directory".to_string())?
        .to_path_buf();

    if cwd == Path::new("/") {
        return Err("cwd must not be the filesystem root".into());
    }
    if real_home.starts_with(&cwd) {
        return Err(format!(
            "cwd {} is the home directory or contains it; pick a project directory",
            cwd.display()
        ));
    }
    if home == real_home || home == Path::new("/") {
        return Err("session home must be its own directory, not the real home".into());
    }
    if home.starts_with(&runner_dir) {
        return Err(format!(
            "session home {} must not be inside the runner directory",
            home.display()
        ));
    }

    let mut deny = Vec::new();
    for rel in HOME_DENY {
        let joined = spec.real_home.join(rel);
        push_unique(&mut deny, canon_lossy(&real_home.join(rel)));
        push_unique(&mut deny, canon_lossy(&joined));
    }
    push_unique(&mut deny, runner_dir.clone());
    push_unique(&mut deny, homes_dir);
    for d in &deny {
        if cwd.starts_with(d) || d.starts_with(&cwd) {
            return Err(format!(
                "cwd {} overlaps the protected path {}",
                cwd.display(),
                d.display()
            ));
        }
    }

    let tmp = home.join("tmp");
    let dirs = [
        home.clone(),
        home.join(".config"),
        home.join(".cache"),
        home.join(".local/share"),
        tmp.clone(),
    ];
    private_dirs(&dirs.iter().map(|d| d.as_path()).collect::<Vec<_>>())?;
    let home = home
        .canonicalize()
        .map_err(|e| format!("session home {} is not usable: {e}", home.display()))?;
    let tmp = home.join("tmp");

    let mut read = vec![cwd.clone(), home.clone()];
    for rel in HOME_READ {
        if let Ok(c) = real_home.join(rel).canonicalize() {
            push_unique(&mut read, c);
        }
    }
    let write = vec![cwd.clone(), home.clone()];
    let self_exe = canon_lossy(&spec.self_exe);
    if write
        .iter()
        .any(|w| self_exe.starts_with(w) || spec.self_exe.starts_with(w))
    {
        return Err(format!(
            "the tmgr binary {} is inside the session directory; install tmgr outside the project or run from another directory",
            self_exe.display()
        ));
    }
    let env = build_env(spec, &home, &tmp)?;
    Ok(Plan {
        cwd,
        paths: PolicyPaths {
            real_home,
            runner_dir,
            home,
            read,
            write,
            deny,
            write_prefixes: xcrun_cache_prefix().into_iter().collect(),
        },
        env,
    })
}

fn sbpl_quote(p: &Path) -> Result<String, String> {
    let s = p
        .to_str()
        .ok_or_else(|| format!("path is not valid UTF-8: {}", p.display()))?;
    if s.contains(['\0', '\n', '\r']) {
        return Err(format!("path contains a control character: {s:?}"));
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if c == '\\' || c == '"' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    Ok(out)
}

pub fn seatbelt_profile(p: &PolicyPaths) -> Result<String, String> {
    let mut s = String::new();
    s.push_str("(version 1)\n(allow default)\n");
    s.push_str(&format!(
        "(deny file-read-data (subpath {}))\n",
        sbpl_quote(&p.real_home)?
    ));
    s.push_str("(deny file-write* (subpath \"/\"))\n");
    for r in &p.read {
        s.push_str(&format!(
            "(allow file-read-data (subpath {}))\n",
            sbpl_quote(r)?
        ));
    }
    for w in &p.write {
        s.push_str(&format!(
            "(allow file-write* (subpath {}))\n",
            sbpl_quote(w)?
        ));
    }
    s.push_str("(allow file-write* (literal \"/dev/null\") (literal \"/dev/tty\") (literal \"/dev/ptmx\"))\n");
    s.push_str("(allow file-write* (subpath \"/dev/fd\"))\n");
    for pre in &p.write_prefixes {
        s.push_str(&format!(
            "(allow file-write* file-read-data (prefix {}))\n",
            sbpl_quote(Path::new(pre))?
        ));
    }
    for d in &p.deny {
        s.push_str(&format!(
            "(deny file-read* file-write* (subpath {}))\n",
            sbpl_quote(d)?
        ));
        s.push_str(&format!(
            "(deny network-outbound (remote unix-socket (subpath {})))\n",
            sbpl_quote(d)?
        ));
    }
    s.push_str(&format!(
        "(deny network-outbound (remote unix-socket (subpath {})))\n",
        sbpl_quote(&p.runner_dir)?
    ));
    s.push_str("(deny signal)\n(allow signal (target same-sandbox))\n");
    let home = sbpl_quote(&p.home)?;
    s.push_str(&format!(
        "(allow file-read* file-write* (subpath {home}))\n"
    ));
    s.push_str(&format!(
        "(allow network-outbound (remote unix-socket (subpath {home})))\n"
    ));
    s.push_str("(deny file-read-data file-write* (regex #\"^/dev/ttys[0-9]+$\"))\n");
    s.push_str("(deny file-read-data file-write* (regex #\"^/dev/pty[p-w][0-9a-f]$\"))\n");
    Ok(s)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandlockTier {
    V6,
    V9,
}

pub fn landlock_tier(kernel_abi: i32) -> Result<LandlockTier, String> {
    match kernel_abi {
        9.. => Ok(LandlockTier::V9),
        6..=8 => Ok(LandlockTier::V6),
        n => Err(format!(
            "tmgr: sandbox unavailable: Landlock ABI {} < 6 (kernel too old or Landlock disabled); the run does not start",
            n.max(0)
        )),
    }
}

#[cfg(target_os = "macos")]
pub fn prepare(spec: &SandboxSpec) -> Result<Launch, String> {
    const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";
    if !Path::new(SANDBOX_EXEC).exists() {
        return Err(format!("{SANDBOX_EXEC} is missing; the run does not start"));
    }
    let plan = plan(spec)?;
    let profile = seatbelt_profile(&plan.paths)?;
    let mut argv = vec![
        SANDBOX_EXEC.to_string(),
        "-p".to_string(),
        profile,
        "--".to_string(),
    ];
    argv.extend(spec.argv.iter().cloned());
    Ok(Launch {
        argv,
        env: plan.env,
        cwd: plan.cwd,
    })
}

#[cfg(target_os = "linux")]
pub fn prepare(spec: &SandboxSpec) -> Result<Launch, String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    if !tmgr_ipc::proto::valid_session_id(&spec.session_id) {
        return Err("invalid session id".into());
    }
    let plan = plan(spec)?;
    let dir = plan
        .paths
        .runner_dir
        .join("sessions")
        .join(&spec.session_id);
    private_dirs(&[&dir])?;
    let policy = dir.join("sandbox.json");
    let body = serde_json::json!({
        "cwd": plan.cwd,
        "read": plan.paths.read,
        "write": plan.paths.write,
    });
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&policy)
        .map_err(|e| format!("cannot write {}: {e}", policy.display()))?;
    f.write_all(body.to_string().as_bytes())
        .map_err(|e| format!("cannot write {}: {e}", policy.display()))?;
    let mut argv = vec![
        path_str(&spec.self_exe)?,
        "__sandbox-exec".to_string(),
        path_str(&policy)?,
        "--".to_string(),
    ];
    argv.extend(spec.argv.iter().cloned());
    Ok(Launch {
        argv,
        env: plan.env,
        cwd: plan.cwd,
    })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn prepare(_spec: &SandboxSpec) -> Result<Launch, String> {
    Err("no OS sandbox is available on this platform; the run does not start".into())
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{landlock_tier, LandlockTier};
    use landlock::{
        Access, AccessFs, CompatLevel, Compatible, PathBeneath, PathFd, Ruleset, RulesetAttr,
        RulesetCreatedAttr, RulesetStatus, Scope, ABI,
    };
    use serde::Deserialize;
    use std::os::fd::{AsFd, AsRawFd, RawFd};
    use std::os::unix::process::CommandExt;
    use std::path::Path;
    use std::process::Command;

    const SYSTEM_READ: &[&str] = &[
        "/usr",
        "/bin",
        "/sbin",
        "/lib",
        "/lib32",
        "/lib64",
        "/libx32",
        "/etc",
        "/opt",
        "/nix",
        "/snap",
        "/proc",
        "/sys",
        "/run/systemd/resolve",
    ];

    const DEV_RW: &[&str] = &[
        "/dev/null",
        "/dev/zero",
        "/dev/urandom",
        "/dev/random",
        "/dev/tty",
        "/dev/ptmx",
        "/dev/full",
    ];

    #[derive(Deserialize)]
    struct Policy {
        cwd: String,
        read: Vec<String>,
        write: Vec<String>,
    }

    fn open_exact(p: &Path) -> Result<PathFd, String> {
        let err = |e: &dyn std::fmt::Display| format!("tmgr: sandbox setup failed: {e}");
        let fd = PathFd::new(p).map_err(|e| err(&e))?;
        let got = std::fs::read_link(format!("/proc/self/fd/{}", fd.as_fd().as_raw_fd()))
            .map_err(|e| err(&e))?;
        if got != p {
            return Err(format!(
                "tmgr: sandbox setup failed: {} resolves to {}; the path changed after validation, the run does not start",
                p.display(),
                got.display()
            ));
        }
        Ok(fd)
    }

    fn same_file(a: RawFd, b: RawFd) -> bool {
        let stat = |fd: RawFd| {
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            (unsafe { libc::fstat(fd, &mut st) } == 0).then_some((st.st_dev, st.st_ino))
        };
        matches!((stat(a), stat(b)), (Some(x), Some(y)) if x == y)
    }

    fn own_terminal() -> Option<PathFd> {
        let target = std::fs::read_link("/proc/self/fd/0").ok()?;
        if !target.starts_with("/dev/pts/") {
            return None;
        }
        let fd = PathFd::new(&target).ok()?;
        same_file(0, fd.as_fd().as_raw_fd()).then_some(fd)
    }

    fn kernel_abi() -> i32 {
        let v = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                std::ptr::null::<libc::c_void>(),
                0usize,
                1u32,
            )
        };
        v.clamp(0, i32::MAX as libc::c_long) as i32
    }

    fn restrict(policy: &Policy, tier: LandlockTier) -> Result<(), String> {
        let abi = match tier {
            LandlockTier::V6 => ABI::V6,
            LandlockTier::V9 => ABI::V9,
        };
        let all = AccessFs::from_all(abi) & !AccessFs::ResolveUnix;
        let read = AccessFs::from_read(abi);
        let handled = AccessFs::from_all(abi);
        let err = |e: &dyn std::fmt::Display| format!("tmgr: sandbox setup failed: {e}");
        let mut rs = Ruleset::default()
            .set_compatibility(CompatLevel::HardRequirement)
            .handle_access(handled)
            .map_err(|e| err(&e))?
            .scope(Scope::AbstractUnixSocket | Scope::Signal)
            .map_err(|e| err(&e))?
            .create()
            .map_err(|e| err(&e))?;
        let grants = SYSTEM_READ
            .iter()
            .map(|p| (p.to_string(), read, false))
            .chain(policy.read.iter().map(|p| (p.clone(), read, true)))
            .chain(policy.write.iter().map(|p| (p.clone(), all, true)))
            .chain(DEV_RW.iter().map(|p| (p.to_string(), all, false)));
        for (path, rights, exact) in grants {
            let p = Path::new(&path);
            if !p.exists() {
                continue;
            }
            let rights = if p.is_dir() {
                rights
            } else {
                rights & AccessFs::from_file(abi)
            };
            let fd = if exact {
                open_exact(p)?
            } else {
                PathFd::new(p).map_err(|e| err(&e))?
            };
            rs = rs
                .add_rule(PathBeneath::new(fd, rights))
                .map_err(|e| err(&e))?;
        }
        if let Some(fd) = own_terminal() {
            rs = rs
                .add_rule(PathBeneath::new(fd, all & AccessFs::from_file(abi)))
                .map_err(|e| err(&e))?;
        }
        let status = rs.restrict_self().map_err(|e| err(&e))?;
        if status.ruleset != RulesetStatus::FullyEnforced || !status.no_new_privs {
            return Err("tmgr: sandbox setup failed: restrictions are not fully enforced".into());
        }
        Ok(())
    }

    fn enter_cwd(cwd: &str) -> Result<(), String> {
        let err = |e: &dyn std::fmt::Display| format!("tmgr: sandbox setup failed: {e}");
        std::env::set_current_dir(cwd).map_err(|e| err(&e))?;
        let got = std::env::current_dir().map_err(|e| err(&e))?;
        if got != Path::new(cwd) {
            return Err(format!(
                "tmgr: sandbox setup failed: working directory {cwd} resolves to {}; the path changed after validation, the run does not start",
                got.display()
            ));
        }
        Ok(())
    }

    pub fn run(policy_path: &str, argv: &[String]) -> i32 {
        let tier = match landlock_tier(kernel_abi()) {
            Ok(t) => t,
            Err(m) => {
                eprintln!("{m}");
                return 78;
            }
        };
        let policy: Policy = match std::fs::read(policy_path)
            .map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()))
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("tmgr: cannot read sandbox policy: {e}");
                return 78;
            }
        };
        if let Err(m) = enter_cwd(&policy.cwd) {
            eprintln!("{m}");
            return 78;
        }
        if let Err(m) = restrict(&policy, tier) {
            eprintln!("{m}");
            return 78;
        }
        let e = Command::new(&argv[0]).args(&argv[1..]).exec();
        eprintln!("tmgr: cannot run {}: {e}", argv[0]);
        127
    }
}

pub fn parse_helper_args(args: &[String]) -> Result<(&str, &[String]), String> {
    match args {
        [policy, sep, argv @ ..] if sep == "--" && !argv.is_empty() => Ok((policy, argv)),
        _ => Err("usage: __sandbox-exec <policy.json> -- <command> [args...]".into()),
    }
}

pub fn exec_helper(args: &[String]) -> i32 {
    let (policy, argv) = match parse_helper_args(args) {
        Ok(v) => v,
        Err(m) => {
            eprintln!("tmgr: {m}");
            return 2;
        }
    };
    #[cfg(target_os = "linux")]
    {
        linux::run(policy, argv)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (policy, argv);
        eprintln!("tmgr: sandbox helper is only used on Linux");
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::MetadataExt;

    struct Fx {
        _t: tempfile::TempDir,
        spec: SandboxSpec,
    }

    fn fx() -> Fx {
        let t = tempfile::Builder::new()
            .prefix("sb")
            .tempdir_in("/tmp")
            .unwrap();
        let root = t.path().canonicalize().unwrap();
        let real_home = root.join("fakehome");
        let runner_dir = root.join("runner");
        let cwd = root.join("proj");
        for d in [&real_home, &runner_dir, &cwd, &real_home.join(".ssh")] {
            fs::create_dir_all(d).unwrap();
        }
        let spec = SandboxSpec {
            session_id: "s_abcdefghij".into(),
            argv: vec!["/bin/echo".into(), "hi".into()],
            cwd,
            home: root.join("homes/s_abcdefghij"),
            real_home,
            runner_dir,
            self_exe: PathBuf::from("/usr/bin/tmgr"),
            env_pass: vec![],
            base_env: vec![],
        };
        Fx { _t: t, spec }
    }

    fn env_of(spec: &SandboxSpec) -> BTreeMap<String, String> {
        plan(spec).unwrap().env.into_iter().collect()
    }

    fn kv(k: &str, v: &str) -> (String, String) {
        (k.to_string(), v.to_string())
    }

    #[test]
    fn env_keeps_allowlist_and_replaces_home() {
        let mut f = fx();
        f.spec.base_env = vec![
            kv("PATH", "/usr/bin"),
            kv("HOME", "/real"),
            kv("LC_ALL", "C"),
            kv("USER", "u"),
            kv("OTHER", "x"),
            kv("TERM", "dumb"),
            kv("TMPDIR", "/var/folders/zz"),
        ];
        let e = env_of(&f.spec);
        assert_eq!(e["PATH"], "/usr/bin");
        assert_eq!(e["LC_ALL"], "C");
        assert_eq!(e["USER"], "u");
        assert!(!e.contains_key("OTHER"));
        assert_eq!(e["TERM"], "xterm-256color");
        let home = e["HOME"].clone();
        assert!(home.ends_with("homes/s_abcdefghij"));
        assert_eq!(e["TMPDIR"], format!("{home}/tmp"));
        assert_eq!(e["XDG_CONFIG_HOME"], format!("{home}/.config"));
        assert_eq!(e["TMGR_SESSION_ID"], "s_abcdefghij");
        assert_eq!(fs::metadata(&home).unwrap().mode() & 0o777, 0o700);
        assert!(Path::new(&home).join("tmp").is_dir());
        assert!(Path::new(&home).join(".local/share").is_dir());
    }

    #[test]
    fn env_pass_drops_dangerous_names() {
        let mut f = fx();
        let bad = [
            "GITHUB_TOKEN",
            "MY_SECRET",
            "API_KEY",
            "db_password",
            "AWS_PROFILE",
            "GH_HOST",
            "LD_PRELOAD",
            "DYLD_INSERT_LIBRARIES",
            "NODE_OPTIONS",
            "PYTHONSTARTUP",
            "PYTHONPATH",
            "PERL5OPT",
            "RUBYOPT",
            "BASH_ENV",
            "ENV",
            "PROMPT_COMMAND",
            "GIT_CONFIG_COUNT",
            "GIT_SSH_COMMAND",
            "TMGR_RUN_TOKEN",
            "TMGR_OTHER",
            "SSH_AUTH_SOCK",
            "DBUS_SESSION_BUS_ADDRESS",
            "XDG_RUNTIME_DIR",
        ];
        let mut base: Vec<_> = bad.iter().map(|n| kv(n, "v")).collect();
        base.push(kv("EDITOR", "vi"));
        base.push(kv("GIT_AUTHOR_NAME", "n"));
        f.spec.base_env = base;
        f.spec.env_pass = bad.iter().map(|s| s.to_string()).collect();
        f.spec.env_pass.push("EDITOR".into());
        f.spec.env_pass.push("GIT_AUTHOR_NAME".into());
        let e = env_of(&f.spec);
        for n in bad {
            assert!(!e.contains_key(n), "{n} leaked");
        }
        assert_eq!(e["EDITOR"], "vi");
        assert_eq!(e["GIT_AUTHOR_NAME"], "n");
    }

    #[test]
    fn env_pass_only_passes_listed_names() {
        let mut f = fx();
        f.spec.base_env = vec![kv("EDITOR", "vi"), kv("PAGER", "less")];
        f.spec.env_pass = vec!["EDITOR".into()];
        let e = env_of(&f.spec);
        assert!(e.contains_key("EDITOR"));
        assert!(!e.contains_key("PAGER"));
    }

    fn refusal(f: &Fx) -> String {
        plan(&f.spec).err().expect("must refuse")
    }

    #[test]
    fn cwd_refusals() {
        let mut f = fx();
        f.spec.cwd = PathBuf::from("/");
        assert!(refusal(&f).contains("root"));

        let mut f = fx();
        f.spec.cwd = f.spec.real_home.clone();
        assert!(refusal(&f).contains("home"));

        let mut f = fx();
        f.spec.cwd = f.spec.real_home.parent().unwrap().to_path_buf();
        assert!(refusal(&f).contains("contains it"));

        let mut f = fx();
        f.spec.cwd = f.spec.real_home.join(".ssh");
        assert!(refusal(&f).contains("protected"));

        let mut f = fx();
        let inner = f.spec.real_home.join(".ssh/keys");
        fs::create_dir_all(&inner).unwrap();
        f.spec.cwd = inner;
        assert!(refusal(&f).contains("protected"));

        let mut f = fx();
        let inner = f.spec.runner_dir.join("sessions");
        fs::create_dir_all(&inner).unwrap();
        f.spec.cwd = inner;
        assert!(refusal(&f).contains("protected"));

        let mut f = fx();
        f.spec.cwd = PathBuf::from("relative/dir");
        assert!(refusal(&f).contains("absolute"));

        let mut f = fx();
        f.spec.cwd = f.spec.cwd.join("missing");
        assert!(refusal(&f).contains("not usable"));
    }

    #[test]
    fn cwd_containing_deny_path_is_refused() {
        let mut f = fx();
        f.spec.real_home = f.spec.cwd.join("h");
        fs::create_dir_all(f.spec.real_home.join(".ssh")).unwrap();
        assert!(!refusal(&f).is_empty());
    }

    #[test]
    fn runner_binary_inside_writable_roots_is_refused() {
        let mut f = fx();
        let inside_cwd = f.spec.cwd.join("target/tmgr");
        fs::create_dir_all(inside_cwd.parent().unwrap()).unwrap();
        fs::write(&inside_cwd, "").unwrap();
        f.spec.self_exe = inside_cwd;
        let e = refusal(&f);
        assert!(e.contains("is inside the session directory"), "{e}");

        let mut f = fx();
        f.spec.self_exe = f.spec.home.join("bin/tmgr");
        assert!(refusal(&f).contains("is inside the session directory"));

        let mut f = fx();
        let link = f.spec.cwd.join("link");
        std::os::unix::fs::symlink("/usr/bin", &link).unwrap();
        f.spec.self_exe = link.join("tmgr");
        assert!(refusal(&f).contains("is inside the session directory"));

        let mut f = fx();
        f.spec.self_exe = f.spec.runner_dir.join("tmgr");
        assert!(plan(&f.spec).is_ok());
    }

    #[test]
    fn home_inside_runner_dir_is_refused() {
        let mut f = fx();
        f.spec.home = f.spec.runner_dir.join("homes/s_abcdefghij");
        assert!(refusal(&f).contains("runner directory"));
    }

    #[test]
    fn plan_lists_deny_and_allow_roots() {
        let f = fx();
        fs::create_dir_all(f.spec.real_home.join(".cargo/registry")).unwrap();
        let p = plan(&f.spec).unwrap().paths;
        assert!(p.deny.contains(&p.real_home.join(".ssh")));
        assert!(p.deny.contains(&p.runner_dir));
        assert!(p.deny.contains(&p.home.parent().unwrap().to_path_buf()));
        assert!(p.read.contains(&p.real_home.join(".cargo/registry")));
        assert!(!p.read.contains(&p.real_home.join(".cargo")));
        assert!(!p.read.contains(&p.real_home.join(".nvm")));
        assert_eq!(p.write.len(), 2);
    }

    #[test]
    fn sbpl_escapes_quotes_and_backslashes() {
        assert_eq!(
            sbpl_quote(Path::new("/a\"b\\c")).unwrap(),
            "\"/a\\\"b\\\\c\""
        );
        assert!(sbpl_quote(Path::new("/a\nb")).is_err());
        assert!(sbpl_quote(Path::new("/a\0b")).is_err());
    }

    #[test]
    fn profile_puts_denies_after_allows() {
        let f = fx();
        let p = plan(&f.spec).unwrap().paths;
        let s = seatbelt_profile(&p).unwrap();
        let last_allow = s.rfind("(allow file-read-data").unwrap();
        let last_write_allow = s.rfind("(allow file-write* (subpath").unwrap();
        let first_deny = s.find("(deny file-read* file-write*").unwrap();
        assert!(first_deny > last_allow && first_deny > last_write_allow);
        assert!(s.starts_with("(version 1)\n(allow default)\n"));
        assert!(s.contains(&format!(
            "(deny file-read* file-write* (subpath {}))",
            sbpl_quote(&p.real_home.join(".ssh")).unwrap()
        )));
        assert!(s.contains(&format!(
            "(deny network-outbound (remote unix-socket (subpath {})))",
            sbpl_quote(&p.runner_dir).unwrap()
        )));
        let deny_sig = s.find("(deny signal)").unwrap();
        let allow_sig = s.find("(allow signal (target same-sandbox))").unwrap();
        assert!(deny_sig < allow_sig);
        let reallow_home = s.rfind("(allow file-read* file-write* (subpath").unwrap();
        assert!(reallow_home > first_deny);
    }

    #[test]
    fn profile_denies_terminal_devices_last() {
        let f = fx();
        let p = plan(&f.spec).unwrap().paths;
        let s = seatbelt_profile(&p).unwrap();
        assert!(!s.contains("(allow file-write* (regex"));
        let tty = s
            .find("(deny file-read-data file-write* (regex #\"^/dev/ttys")
            .unwrap();
        let pty = s
            .find("(deny file-read-data file-write* (regex #\"^/dev/pty[p-w]")
            .unwrap();
        let last_allow = s.rfind("(allow ").unwrap();
        assert!(tty > last_allow && pty > last_allow);
        assert!(s.contains("(literal \"/dev/tty\")"));
    }

    #[test]
    fn profile_refuses_control_characters() {
        let f = fx();
        let mut p = plan(&f.spec).unwrap().paths;
        p.read.push(PathBuf::from("/x\ny"));
        assert!(seatbelt_profile(&p).is_err());
    }

    #[test]
    fn landlock_tier_requires_abi_6() {
        for n in [-1, 0, 1, 5] {
            let e = landlock_tier(n).unwrap_err();
            assert!(e.contains("sandbox unavailable"));
            assert!(e.contains("< 6"));
        }
        assert_eq!(landlock_tier(6), Ok(LandlockTier::V6));
        assert_eq!(landlock_tier(8), Ok(LandlockTier::V6));
        assert_eq!(landlock_tier(9), Ok(LandlockTier::V9));
        assert_eq!(landlock_tier(12), Ok(LandlockTier::V9));
    }

    #[test]
    fn helper_args_parse() {
        let a: Vec<String> = ["p.json", "--", "ls", "-l"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (p, argv) = parse_helper_args(&a).unwrap();
        assert_eq!(p, "p.json");
        assert_eq!(argv, &a[2..]);
        assert!(parse_helper_args(&a[..2]).is_err());
        assert!(parse_helper_args(&a[..1]).is_err());
        assert_eq!(exec_helper(&a[..1]), 2);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_prepare_writes_private_policy_outside_home() {
        let f = fx();
        let l = prepare(&f.spec).unwrap();
        assert_eq!(l.argv[0], "/usr/bin/tmgr");
        assert_eq!(l.argv[1], "__sandbox-exec");
        assert_eq!(l.argv[3], "--");
        assert_eq!(&l.argv[4..], &f.spec.argv[..]);
        let pol = Path::new(&l.argv[2]);
        assert!(pol.starts_with(f.spec.runner_dir.canonicalize().unwrap()));
        assert_eq!(fs::metadata(pol).unwrap().mode() & 0o777, 0o600);
        let v: serde_json::Value = serde_json::from_slice(&fs::read(pol).unwrap()).unwrap();
        assert_eq!(v["write"].as_array().unwrap().len(), 2);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_prepare_rejects_bad_session_id() {
        let mut f = fx();
        f.spec.session_id = "../x".into();
        assert!(prepare(&f.spec).is_err());
    }
}
