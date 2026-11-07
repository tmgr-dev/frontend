#[cfg(unix)]
use tauri::{Manager, Runtime};

#[cfg(unix)]
mod imp {
  use std::collections::HashSet;
  use std::ffi::{OsStr, OsString};
  use std::io::Read;
  use std::os::unix::fs::MetadataExt;
  use std::os::unix::process::CommandExt;
  use std::path::{Path, PathBuf};
  use std::process::{Command, ExitStatus, Stdio};
  use std::sync::mpsc;
  use std::time::{Duration, Instant};

  use serde::{Deserialize, Serialize};

  pub const SERVER_NAME: &str = "tmgr-local";
  pub const PLACEHOLDER_TOKEN: &str = "<new token id>";
  pub const STEP_TIMEOUT: Duration = Duration::from_secs(30);
  const LOGIN_TIMEOUT: Duration = Duration::from_secs(6);
  const OUTPUT_CAP: usize = 16 * 1024;
  const LOGIN_CAP: usize = 64 * 1024;
  const PIPE_GRACE: Duration = Duration::from_millis(500);
  const BEGIN: &str = "__TMGR_ENV_BEGIN_7f3a91__";
  const END: &str = "__TMGR_ENV_END_7f3a91__";
  const LOGIN_SCRIPT: &str = "printf '\\n__TMGR_ENV_BEGIN_7f3a91__\\nPATH=%s\\nCODEX_HOME=%s\\nCLAUDE_CONFIG_DIR=%s\\n__TMGR_ENV_END_7f3a91__\\n' \"$PATH\" \"$CODEX_HOME\" \"$CLAUDE_CONFIG_DIR\"";

  #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "kebab-case")]
  pub enum AgentId {
    ClaudeCode,
    Codex,
  }

  impl AgentId {
    pub const ALL: [AgentId; 2] = [AgentId::ClaudeCode, AgentId::Codex];
  }

  #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "lowercase")]
  pub enum ConnectAction {
    Install,
    Remove,
  }

  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum CliName {
    Claude,
    Codex,
  }

  impl CliName {
    pub fn file_name(self) -> &'static str {
      match self {
        CliName::Claude => "claude",
        CliName::Codex => "codex",
      }
    }
  }

  #[derive(Clone, Debug, PartialEq, Eq)]
  pub struct TokenId(String);

  impl TokenId {
    pub fn parse(raw: &str) -> Result<Self, String> {
      let ok = raw.len() == 19 && raw.starts_with("lt_") && raw.as_bytes()[3..].iter().all(|b| b.is_ascii_alphanumeric());
      if ok {
        Ok(Self(raw.to_owned()))
      } else {
        Err("Invalid token id".into())
      }
    }

    pub fn as_str(&self) -> &str {
      &self.0
    }
  }

  #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
  #[serde(rename_all = "lowercase")]
  pub enum BridgeReason {
    Translocated,
    Invalid,
    Unsupported,
  }

  impl BridgeReason {
    pub fn message(self) -> &'static str {
      match self {
        BridgeReason::Translocated => "Move TMGR to Applications, open it from there, then install",
        BridgeReason::Invalid => "The TMGR executable path cannot be used",
        BridgeReason::Unsupported => "Connecting agents is only supported on macOS",
      }
    }
  }

  pub fn bridge_path(exe: &Path) -> Result<PathBuf, BridgeReason> {
    let text = exe.as_os_str().to_string_lossy();
    if !exe.is_absolute() || text.contains(['\n', '\r', '\0']) {
      return Err(BridgeReason::Invalid);
    }
    if text.contains("/AppTranslocation/") {
      return Err(BridgeReason::Translocated);
    }
    Ok(exe.to_path_buf())
  }

  #[derive(Clone, Debug, PartialEq, Eq)]
  pub struct Step {
    pub argv: Vec<OsString>,
    pub tolerate_failure: bool,
  }

  fn argv(cli: &Path, head: &[&'static str], tail: Option<(&Path, &str)>) -> Vec<OsString> {
    let mut out: Vec<OsString> = vec![cli.as_os_str().to_owned()];
    out.extend(head.iter().map(OsString::from));
    if let Some((bridge, token)) = tail {
      out.push("--".into());
      out.push(bridge.as_os_str().to_owned());
      out.extend(["mcp", "--token-id"].map(OsString::from));
      out.push(token.into());
    }
    out
  }

  fn build_steps(
    agent: AgentId,
    action: ConnectAction,
    cli: &Path,
    bridge: &Path,
    token: Option<&str>,
  ) -> Result<Vec<Step>, String> {
    match (agent, action) {
      (_, ConnectAction::Remove) if token.is_some() => Err("Remove does not take a token".into()),
      (AgentId::ClaudeCode, ConnectAction::Remove) => Ok(vec![Step {
        argv: argv(cli, &["mcp", "remove", "--scope", "user", SERVER_NAME], None),
        tolerate_failure: false,
      }]),
      (AgentId::Codex, ConnectAction::Remove) => Ok(vec![Step {
        argv: argv(cli, &["mcp", "remove", SERVER_NAME], None),
        tolerate_failure: false,
      }]),
      (_, ConnectAction::Install) if token.is_none() => Err("A token is required to install".into()),
      (AgentId::ClaudeCode, ConnectAction::Install) => {
        let token = token.unwrap_or_default();
        Ok(vec![
          Step { argv: argv(cli, &["mcp", "remove", "--scope", "user", SERVER_NAME], None), tolerate_failure: true },
          Step {
            argv: argv(cli, &["mcp", "add", "--scope", "user", SERVER_NAME], Some((bridge, token))),
            tolerate_failure: false,
          },
        ])
      }
      (AgentId::Codex, ConnectAction::Install) => {
        let token = token.unwrap_or_default();
        Ok(vec![Step {
          argv: argv(cli, &["mcp", "add", SERVER_NAME], Some((bridge, token))),
          tolerate_failure: false,
        }])
      }
    }
  }

  pub fn plan_commands(
    agent: AgentId,
    action: ConnectAction,
    cli: &Path,
    bridge: &Path,
    token: Option<&TokenId>,
  ) -> Result<Vec<Step>, String> {
    build_steps(agent, action, cli, bridge, token.map(TokenId::as_str))
  }

  pub fn display_plan(
    agent: AgentId,
    action: ConnectAction,
    cli: &Path,
    bridge: &Path,
    redactor: &Redactor,
  ) -> Vec<Vec<String>> {
    let token = (action == ConnectAction::Install).then_some(PLACEHOLDER_TOKEN);
    build_steps(agent, action, cli, bridge, token)
      .unwrap_or_default()
      .iter()
      .map(|s| display_argv(&s.argv, redactor))
      .collect()
  }

  fn display_argv(argv: &[OsString], redactor: &Redactor) -> Vec<String> {
    argv.iter().map(|a| redactor.redact(&a.to_string_lossy())).collect()
  }

  #[derive(Clone, Debug, Default)]
  pub struct Redactor {
    roots: Vec<String>,
    user: Option<String>,
  }

  impl Redactor {
    pub fn new(home: &Path, user: Option<&str>) -> Self {
      let mut roots = vec![home.to_string_lossy().into_owned()];
      if let Ok(canon) = home.canonicalize() {
        roots.push(canon.to_string_lossy().into_owned());
      }
      roots.retain(|r| r.len() > 1);
      roots.sort_by_key(|r| std::cmp::Reverse(r.len()));
      roots.dedup();
      let user = user.filter(|u| u.len() >= 3).map(str::to_owned);
      Self { roots, user }
    }

    pub fn redact(&self, text: &str) -> String {
      let mut out = text.to_owned();
      for root in &self.roots {
        out = out.replace(root.as_str(), "~");
      }
      match &self.user {
        Some(user) => replace_segment(&out, user),
        None => out,
      }
    }
  }

  fn replace_segment(text: &str, user: &str) -> String {
    let name_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (at, _) in text.match_indices(user) {
      let end = at + user.len();
      let before_ok = text[..at].ends_with('/');
      let after_ok = text[end..].chars().next().is_none_or(|c| !name_char(c));
      if at >= last && before_ok && after_ok {
        out.push_str(&text[last..at]);
        out.push('~');
        last = end;
      }
    }
    out.push_str(&text[last..]);
    out
  }

  fn trusted(md: &std::fs::Metadata, uid: u32) -> bool {
    let mode = md.mode();
    mode & 0o002 == 0 && (md.uid() == uid || (md.uid() == 0 && mode & 0o020 == 0))
  }

  fn dir_chain_trusted(dir: &Path, uid: u32) -> bool {
    dir.is_absolute()
      && dir.ancestors().enumerate().all(|(depth, ancestor)| {
        let Ok(md) = std::fs::metadata(ancestor) else { return false };
        let sticky_root = depth > 0 && md.uid() == 0 && md.mode() & 0o1000 != 0;
        md.is_dir() && (trusted(&md, uid) || sticky_root)
      })
  }

  fn file_trusted(candidate: &Path, uid: u32) -> bool {
    let Ok(real) = candidate.canonicalize() else { return false };
    let Ok(md) = std::fs::metadata(&real) else { return false };
    md.is_file()
      && md.mode() & 0o100 != 0
      && trusted(&md, uid)
      && real.parent().is_some_and(|parent| dir_chain_trusted(parent, uid))
  }

  pub fn still_trusted(cli: &Path, uid: u32) -> bool {
    cli.parent().is_some_and(|dir| dir_chain_trusted(dir, uid)) && file_trusted(cli, uid)
  }

  pub fn resolve_cli(name: CliName, dirs: &[PathBuf], uid: u32) -> Option<PathBuf> {
    for dir in dirs {
      if !dir.is_absolute() {
        continue;
      }
      if !dir_chain_trusted(dir, uid) {
        continue;
      }
      let candidate = dir.join(name.file_name());
      if file_trusted(&candidate, uid) {
        return Some(candidate);
      }
    }
    None
  }

  #[derive(Clone, Debug, Default, PartialEq, Eq)]
  pub struct LoginEnv {
    pub path: Option<String>,
    pub codex_home: Option<String>,
    pub claude_config_dir: Option<String>,
  }

  pub fn parse_marked(out: &str) -> Option<LoginEnv> {
    let start = out.rfind(BEGIN)? + BEGIN.len();
    let block = &out[start..];
    let block = &block[..block.find(END)?];
    let mut env = LoginEnv::default();
    for line in block.lines() {
      let non_empty = |v: &str| (!v.is_empty()).then(|| v.to_owned());
      if let Some(v) = line.strip_prefix("PATH=") {
        env.path = non_empty(v);
      } else if let Some(v) = line.strip_prefix("CODEX_HOME=") {
        env.codex_home = non_empty(v);
      } else if let Some(v) = line.strip_prefix("CLAUDE_CONFIG_DIR=") {
        env.claude_config_dir = non_empty(v);
      }
    }
    Some(env)
  }

  pub fn shell_allowed(shell: &Path, shells_file: &str, uid: u32) -> bool {
    shell.is_absolute()
      && matches!(shell.file_name().and_then(OsStr::to_str), Some("zsh" | "bash" | "sh"))
      && shells_file.lines().any(|l| Path::new(l.trim()) == shell)
      && file_trusted(shell, uid)
  }

  pub fn login_env(shell: &Path, shells_file: &str, uid: u32, timeout: Duration) -> Option<LoginEnv> {
    if !shell_allowed(shell, shells_file, uid) {
      return None;
    }
    let mut cmd = Command::new(shell);
    cmd.args(["-ilc", LOGIN_SCRIPT]);
    let captured = capture(cmd, LOGIN_CAP, false, timeout).ok()?;
    if captured.timed_out {
      return None;
    }
    parse_marked(&String::from_utf8_lossy(&captured.stdout))
  }

  fn fallback_dirs(home: &Path) -> Vec<PathBuf> {
    vec![
      home.join(".local/bin"),
      home.join(".claude/local"),
      PathBuf::from("/opt/homebrew/bin"),
      PathBuf::from("/usr/local/bin"),
      home.join(".npm-global/bin"),
      home.join(".bun/bin"),
      home.join(".volta/bin"),
    ]
  }

  pub fn search_path(login_path: Option<&str>, home: &Path, process_path: Option<&OsStr>) -> Vec<PathBuf> {
    let mut all: Vec<PathBuf> = Vec::new();
    if let Some(p) = login_path {
      all.extend(std::env::split_paths(p));
    }
    all.extend(fallback_dirs(home));
    if let Some(p) = process_path {
      all.extend(std::env::split_paths(p));
    }
    let mut seen = HashSet::new();
    all.into_iter().filter(|d| d.is_absolute() && seen.insert(d.clone())).collect()
  }

  pub struct Passwd {
    pub name: String,
    pub dir: String,
    pub shell: String,
  }

  pub fn passwd(uid: u32) -> Option<Passwd> {
    let mut buf = vec![0u8; 1024];
    loop {
      let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
      let mut res: *mut libc::passwd = std::ptr::null_mut();
      let rc = unsafe { libc::getpwuid_r(uid, &mut pwd, buf.as_mut_ptr().cast(), buf.len(), &mut res) };
      if rc == libc::ERANGE && buf.len() < (1 << 20) {
        buf.resize(buf.len() * 2, 0);
        continue;
      }
      if rc != 0 || res.is_null() {
        return None;
      }
      let text = |p: *const libc::c_char| {
        if p.is_null() {
          String::new()
        } else {
          unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
      };
      return Some(Passwd { name: text(pwd.pw_name), dir: text(pwd.pw_dir), shell: text(pwd.pw_shell) });
    }
  }

  pub struct Resolved {
    pub uid: u32,
    pub home: PathBuf,
    pub user: Option<String>,
    pub search: Vec<PathBuf>,
    pub claude: Option<PathBuf>,
    pub codex: Option<PathBuf>,
    pub codex_home: Option<String>,
    pub claude_config_dir: Option<String>,
    pub redactor: Redactor,
  }

  fn absolute_only(v: Option<String>) -> Option<String> {
    v.filter(|s| Path::new(s).is_absolute())
  }

  impl Resolved {
    pub fn build(
      home: &Path,
      uid: u32,
      user: Option<&str>,
      login: Option<LoginEnv>,
      process_path: Option<&OsStr>,
    ) -> Self {
      let login = login.unwrap_or_default();
      let search = search_path(login.path.as_deref(), home, process_path);
      Self {
        uid,
        home: home.to_path_buf(),
        user: user.map(str::to_owned),
        claude: resolve_cli(CliName::Claude, &search, uid),
        codex: resolve_cli(CliName::Codex, &search, uid),
        search,
        codex_home: absolute_only(login.codex_home),
        claude_config_dir: absolute_only(login.claude_config_dir),
        redactor: Redactor::new(home, user),
      }
    }

    pub fn needs_refresh(&self) -> bool {
      self.claude.is_none() && self.codex.is_none()
    }

    pub fn cli(&self, agent: AgentId) -> Option<&Path> {
      match agent {
        AgentId::ClaudeCode => self.claude.as_deref(),
        AgentId::Codex => self.codex.as_deref(),
      }
    }

    pub fn child_env(&self, lookup: &dyn Fn(&str) -> Option<OsString>) -> ChildEnv {
      let mut vars: Vec<(&'static str, OsString)> = vec![("HOME", self.home.as_os_str().to_owned())];
      let user = lookup("USER").or_else(|| self.user.clone().map(OsString::from));
      if let Some(u) = user {
        vars.push(("LOGNAME", lookup("LOGNAME").unwrap_or_else(|| u.clone())));
        vars.push(("USER", u));
      }
      for key in ["TMPDIR", "LANG"] {
        if let Some(v) = lookup(key) {
          vars.push((key, v));
        }
      }
      let trusted_dirs: Vec<&PathBuf> = self.search.iter().filter(|d| dir_chain_trusted(d, self.uid)).collect();
      let joined = std::env::join_paths(trusted_dirs).unwrap_or_default();
      vars.push(("PATH", joined));
      if let Some(v) = &self.codex_home {
        vars.push(("CODEX_HOME", v.into()));
      }
      if let Some(v) = &self.claude_config_dir {
        vars.push(("CLAUDE_CONFIG_DIR", v.into()));
      }
      ChildEnv { vars, home: self.home.clone() }
    }
  }

  pub struct ChildEnv {
    pub vars: Vec<(&'static str, OsString)>,
    pub home: PathBuf,
  }

  fn resolve_now() -> Resolved {
    let uid = unsafe { libc::getuid() };
    let pw = passwd(uid);
    let home = std::env::var_os("HOME")
      .map(PathBuf::from)
      .filter(|h| h.is_absolute())
      .or_else(|| pw.as_ref().map(|p| PathBuf::from(&p.dir)))
      .unwrap_or_else(|| PathBuf::from("/"));
    let shells_file = std::fs::read_to_string("/etc/shells").unwrap_or_default();
    let candidates = [pw.as_ref().map(|p| p.shell.clone()), std::env::var("SHELL").ok()];
    let shell = candidates
      .into_iter()
      .flatten()
      .map(PathBuf::from)
      .find(|s| shell_allowed(s, &shells_file, uid));
    let mut login = shell.and_then(|s| login_env(&s, &shells_file, uid, LOGIN_TIMEOUT)).unwrap_or_default();
    login.codex_home = login.codex_home.take().or_else(|| std::env::var("CODEX_HOME").ok());
    login.claude_config_dir = login.claude_config_dir.take().or_else(|| std::env::var("CLAUDE_CONFIG_DIR").ok());
    let process_path = std::env::var_os("PATH");
    Resolved::build(&home, uid, pw.as_ref().map(|p| p.name.as_str()), Some(login), process_path.as_deref())
  }

  pub fn resolved(refresh_if_empty: bool) -> std::sync::Arc<Resolved> {
    static CELL: std::sync::Mutex<Option<std::sync::Arc<Resolved>>> = std::sync::Mutex::new(None);
    let mut guard = CELL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(cached) = guard.as_ref() {
      if !(refresh_if_empty && cached.needs_refresh()) {
        return cached.clone();
      }
    }
    let fresh = std::sync::Arc::new(resolve_now());
    *guard = Some(fresh.clone());
    fresh
  }

  pub struct Captured {
    pub status: Option<ExitStatus>,
    pub timed_out: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
  }

  fn drain<R: Read + Send + 'static>(mut reader: R, cap: usize) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
      let mut kept = Vec::new();
      let mut buf = [0u8; 8192];
      loop {
        match reader.read(&mut buf) {
          Ok(0) => break,
          Ok(n) => {
            let room = cap.saturating_sub(kept.len());
            kept.extend_from_slice(&buf[..n.min(room)]);
          }
          Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
          Err(_) => break,
        }
      }
      let _ = tx.send(kept);
    });
    rx
  }

  fn kill_group(pgid: i32) {
    unsafe { libc::killpg(pgid, libc::SIGKILL) };
  }

  pub fn capture(mut cmd: Command, cap: usize, pipe_stderr: bool, timeout: Duration) -> std::io::Result<Captured> {
    cmd
      .stdin(Stdio::null())
      .stdout(Stdio::piped())
      .stderr(if pipe_stderr { Stdio::piped() } else { Stdio::null() })
      .process_group(0);
    let mut child = cmd.spawn()?;
    let pgid = child.id() as i32;
    let out_rx = child.stdout.take().map(|s| drain(s, cap));
    let err_rx = child.stderr.take().map(|s| drain(s, cap));
    let deadline = Instant::now() + timeout;
    let (status, timed_out) = loop {
      match child.try_wait() {
        Ok(Some(status)) => break (Some(status), false),
        Ok(None) if Instant::now() >= deadline => {
          kill_group(pgid);
          let _ = child.kill();
          let _ = child.wait();
          break (None, true);
        }
        Ok(None) => std::thread::sleep(Duration::from_millis(10)),
        Err(_) => {
          kill_group(pgid);
          let _ = child.kill();
          let _ = child.wait();
          break (None, false);
        }
      }
    };
    kill_group(pgid);
    let collect =
      |rx: Option<mpsc::Receiver<Vec<u8>>>| rx.and_then(|rx| rx.recv_timeout(PIPE_GRACE).ok()).unwrap_or_default();
    Ok(Captured { status, timed_out, stdout: collect(out_rx), stderr: collect(err_rx) })
  }

  #[derive(Debug, Serialize)]
  #[serde(rename_all = "camelCase")]
  pub struct StepOutcome {
    pub display: Vec<String>,
    pub exit_code: Option<i32>,
    pub output: String,
    pub tolerated: bool,
  }

  #[derive(Debug, Serialize)]
  #[serde(rename_all = "camelCase")]
  pub struct RunOutcome {
    pub ok: bool,
    pub timed_out: bool,
    pub steps: Vec<StepOutcome>,
  }

  pub fn run_steps(steps: &[Step], env: &ChildEnv, redactor: &Redactor, step_timeout: Duration) -> RunOutcome {
    let mut outcome = RunOutcome { ok: true, timed_out: false, steps: Vec::new() };
    for step in steps {
      let Some((program, args)) = step.argv.split_first() else { continue };
      let display = display_argv(&step.argv, redactor);
      let mut cmd = Command::new(program);
      cmd.args(args).env_clear().current_dir(&env.home);
      for (key, value) in &env.vars {
        cmd.env(key, value);
      }
      let Ok(captured) = capture(cmd, OUTPUT_CAP, true, step_timeout) else {
        outcome.steps.push(StepOutcome {
          display,
          exit_code: None,
          output: "Could not start the command".into(),
          tolerated: step.tolerate_failure,
        });
        if !step.tolerate_failure {
          outcome.ok = false;
          break;
        }
        continue;
      };
      let exit_code = captured.status.and_then(|s| s.code());
      let mut text = String::from_utf8_lossy(&captured.stdout).into_owned();
      text.push_str(&String::from_utf8_lossy(&captured.stderr));
      outcome.steps.push(StepOutcome {
        display,
        exit_code,
        output: redactor.redact(&text),
        tolerated: step.tolerate_failure,
      });
      if captured.timed_out {
        outcome.ok = false;
        outcome.timed_out = true;
        break;
      }
      if exit_code != Some(0) && !step.tolerate_failure {
        outcome.ok = false;
        break;
      }
    }
    outcome
  }

  #[derive(Debug, Serialize)]
  #[serde(rename_all = "camelCase")]
  pub struct BridgeOut {
    pub ok: bool,
    pub reason: Option<BridgeReason>,
  }

  #[derive(Debug, Serialize)]
  #[serde(rename_all = "camelCase")]
  pub struct AgentOut {
    pub id: AgentId,
    pub found: bool,
    pub cli: Option<String>,
    pub install: Vec<Vec<String>>,
    pub remove: Vec<Vec<String>>,
  }

  #[derive(Debug, Serialize)]
  #[serde(rename_all = "camelCase")]
  pub struct DetectOut {
    pub platform_supported: bool,
    pub safe_mode: bool,
    pub bridge: BridgeOut,
    pub agents: Vec<AgentOut>,
  }

  pub fn detect(
    resolved: &Resolved,
    bridge: Result<PathBuf, BridgeReason>,
    platform_supported: bool,
    safe_mode: bool,
  ) -> DetectOut {
    let bridge = if platform_supported { bridge } else { Err(BridgeReason::Unsupported) };
    let agents = AgentId::ALL
      .iter()
      .map(|&id| {
        let cli = if platform_supported { resolved.cli(id) } else { None };
        let (install, remove) = match cli {
          None => (Vec::new(), Vec::new()),
          Some(cli) => {
            let install = match &bridge {
              Ok(b) => display_plan(id, ConnectAction::Install, cli, b, &resolved.redactor),
              Err(_) => Vec::new(),
            };
            (install, display_plan(id, ConnectAction::Remove, cli, Path::new(""), &resolved.redactor))
          }
        };
        AgentOut {
          id,
          found: cli.is_some(),
          cli: cli.map(|c| resolved.redactor.redact(&c.to_string_lossy())),
          install,
          remove,
        }
      })
      .collect();
    DetectOut { platform_supported, safe_mode, bridge: BridgeOut { ok: bridge.is_ok(), reason: bridge.err() }, agents }
  }

  #[cfg(test)]
  mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn uid() -> u32 {
      unsafe { libc::getuid() }
    }

    fn mode(path: &Path, mode: u32) {
      fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    fn tmp() -> tempfile::TempDir {
      let dir = tempfile::tempdir().unwrap();
      mode(dir.path(), 0o755);
      dir
    }

    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
      let path = dir.join(name);
      fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
      mode(&path, 0o755);
      path
    }

    fn token() -> TokenId {
      TokenId::parse("lt_FakeFakeFake0001").unwrap()
    }

    fn os(v: &[&str]) -> Vec<OsString> {
      v.iter().map(OsString::from).collect()
    }

    #[test]
    fn claude_install_argv() {
      let steps = plan_commands(
        AgentId::ClaudeCode,
        ConnectAction::Install,
        Path::new("/bin/claude"),
        Path::new("/Applications/TMGR.app/Contents/MacOS/tmgr"),
        Some(&token()),
      )
      .unwrap();
      assert_eq!(steps.len(), 2);
      assert_eq!(steps[0].argv, os(&["/bin/claude", "mcp", "remove", "--scope", "user", "tmgr-local"]));
      assert!(steps[0].tolerate_failure);
      assert_eq!(
        steps[1].argv,
        os(&[
          "/bin/claude",
          "mcp",
          "add",
          "--scope",
          "user",
          "tmgr-local",
          "--",
          "/Applications/TMGR.app/Contents/MacOS/tmgr",
          "mcp",
          "--token-id",
          "lt_FakeFakeFake0001"
        ])
      );
      assert!(!steps[1].tolerate_failure);
    }

    #[test]
    fn claude_remove_argv() {
      let steps =
        plan_commands(AgentId::ClaudeCode, ConnectAction::Remove, Path::new("/bin/claude"), Path::new("/b"), None)
          .unwrap();
      assert_eq!(steps.len(), 1);
      assert_eq!(steps[0].argv, os(&["/bin/claude", "mcp", "remove", "--scope", "user", "tmgr-local"]));
      assert!(!steps[0].tolerate_failure);
    }

    #[test]
    fn codex_install_and_remove_argv() {
      let install = plan_commands(
        AgentId::Codex,
        ConnectAction::Install,
        Path::new("/bin/codex"),
        Path::new("/b/tmgr"),
        Some(&token()),
      )
      .unwrap();
      assert_eq!(install.len(), 1);
      assert_eq!(
        install[0].argv,
        os(&["/bin/codex", "mcp", "add", "tmgr-local", "--", "/b/tmgr", "mcp", "--token-id", "lt_FakeFakeFake0001"])
      );
      let remove =
        plan_commands(AgentId::Codex, ConnectAction::Remove, Path::new("/bin/codex"), Path::new("/b/tmgr"), None)
          .unwrap();
      assert_eq!(remove[0].argv, os(&["/bin/codex", "mcp", "remove", "tmgr-local"]));
    }

    #[test]
    fn install_without_token_and_remove_with_token_are_errors() {
      let cli = Path::new("/bin/claude");
      assert!(plan_commands(AgentId::ClaudeCode, ConnectAction::Install, cli, Path::new("/b"), None).is_err());
      assert!(plan_commands(AgentId::Codex, ConnectAction::Install, cli, Path::new("/b"), None).is_err());
      assert!(plan_commands(AgentId::Codex, ConnectAction::Remove, cli, Path::new("/b"), Some(&token())).is_err());
    }

    #[test]
    fn display_uses_placeholder_and_tilde() {
      let home = tmp();
      let h = home.path().to_string_lossy().into_owned();
      let redactor = Redactor::new(home.path(), Some("alice"));
      let cli = home.path().join(".local/bin/claude");
      let bridge = home.path().join("alice-apps/alice/tmgr");
      let shown = display_plan(AgentId::ClaudeCode, ConnectAction::Install, &cli, &bridge, &redactor);
      assert_eq!(shown[0][0], "~/.local/bin/claude");
      assert_eq!(shown[1].last().unwrap(), "<new token id>");
      assert!(shown[1].contains(&"~/alice-apps/~/tmgr".to_string()));
      assert!(shown.iter().flatten().all(|a| !a.contains(&h)));
    }

    #[test]
    fn redactor_replaces_the_user_only_as_a_whole_path_segment() {
      let r = Redactor::new(Path::new("/nonexistent-home/x"), Some("user"));
      assert_eq!(r.redact("--scope user"), "--scope user");
      assert_eq!(r.redact("/Volumes/x/user/bin"), "/Volumes/x/~/bin");
      assert_eq!(r.redact("/Volumes/x/user"), "/Volumes/x/~");
      assert_eq!(r.redact("/Volumes/x/user-name/bin"), "/Volumes/x/user-name/bin");
      assert_eq!(r.redact("/Volumes/x/users/bin"), "/Volumes/x/users/bin");
      assert_eq!(r.redact("/Volumes/x/user: not found"), "/Volumes/x/~: not found");
      assert_eq!(r.redact("/a/user/b/user"), "/a/~/b/~");
    }

    #[test]
    fn token_id_rejects_everything_but_the_exact_shape() {
      let long = format!("lt_{}", "a".repeat(17));
      let dash = format!("-lt_{}", "a".repeat(16));
      let bad = [
        "lt_abc",
        long.as_str(),
        "--token-id",
        "lt_aaaa;rm -rf x",
        "lt_$(touch p)aaaaaa",
        "lt_aaaa\naaaaaaaaaaaa",
        "lt_aaaaaaaaaaaaaaaé",
        "lt_ааааааааааааааааа",
        "lt_aaaa aaaaaaaaaaaa",
        "",
        dash.as_str(),
      ];
      for raw in bad {
        assert!(TokenId::parse(raw).is_err(), "{raw:?}");
      }
      assert!(TokenId::parse("lt_AZaz09AZaz09AZaz").is_ok());
    }

    #[test]
    fn agent_id_serde_is_closed() {
      let parse = |s: &str| serde_json::from_str::<AgentId>(&format!("{s:?}"));
      assert_eq!(parse("claude-code").unwrap(), AgentId::ClaudeCode);
      assert_eq!(parse("codex").unwrap(), AgentId::Codex);
      for bad in ["claude", "claude-code; rm", "../claude", "bash", "cursor", "Claude-Code"] {
        assert!(parse(bad).is_err(), "{bad}");
      }
      assert!(serde_json::from_str::<ConnectAction>("\"install\"").is_ok());
      assert!(serde_json::from_str::<ConnectAction>("\"Install\"").is_err());
    }

    #[test]
    fn bridge_path_rules() {
      assert_eq!(
        bridge_path(Path::new("/private/var/folders/x/AppTranslocation/ABC/d/TMGR.app/Contents/MacOS/tmgr")),
        Err(BridgeReason::Translocated)
      );
      assert_eq!(bridge_path(Path::new("tmgr")), Err(BridgeReason::Invalid));
      assert_eq!(bridge_path(Path::new("/a\nb/tmgr")), Err(BridgeReason::Invalid));
      assert_eq!(bridge_path(Path::new("/a\rb/tmgr")), Err(BridgeReason::Invalid));
      let bridge = bridge_path(Path::new("/My Apps/$(touch pwned)/tmgr")).unwrap();
      let steps =
        plan_commands(AgentId::Codex, ConnectAction::Install, Path::new("/bin/codex"), &bridge, Some(&token()))
          .unwrap();
      assert_eq!(steps[0].argv[5], OsString::from("/My Apps/$(touch pwned)/tmgr"));
      assert_eq!(steps[0].argv.len(), 9);
      assert_eq!(serde_json::to_string(&BridgeReason::Translocated).unwrap(), "\"translocated\"");
    }

    #[test]
    fn resolve_cli_picks_only_trusted_executables() {
      let root = tmp();
      let bin = root.path().join("bin");
      fs::create_dir(&bin).unwrap();
      mode(&bin, 0o755);
      let dirs = vec![bin.clone()];

      fs::write(bin.join("claude"), "x").unwrap();
      mode(&bin.join("claude"), 0o644);
      assert_eq!(resolve_cli(CliName::Claude, &dirs, uid()), None);

      fs::remove_file(bin.join("claude")).unwrap();
      fs::create_dir(bin.join("claude")).unwrap();
      mode(&bin.join("claude"), 0o755);
      assert_eq!(resolve_cli(CliName::Claude, &dirs, uid()), None);
      fs::remove_dir(bin.join("claude")).unwrap();

      let real = script(root.path(), "real-claude", "exit 0");
      std::os::unix::fs::symlink(&real, bin.join("claude")).unwrap();
      assert_eq!(resolve_cli(CliName::Claude, &dirs, uid()), Some(bin.join("claude")));

      script(&bin, "claude-evil", "exit 0");
      assert_eq!(resolve_cli(CliName::Codex, &dirs, uid()), None);
    }

    #[test]
    fn resolve_cli_skips_writable_dirs_and_files_and_foreign_owners() {
      let root = tmp();
      let open = root.path().join("open");
      fs::create_dir(&open).unwrap();
      script(&open, "claude", "exit 0");
      let dirs = vec![open.clone()];
      mode(&open, 0o777);
      assert_eq!(resolve_cli(CliName::Claude, &dirs, uid()), None);
      mode(&open, 0o775);
      assert!(resolve_cli(CliName::Claude, &dirs, uid()).is_some());
      mode(&open, 0o755);
      assert!(resolve_cli(CliName::Claude, &dirs, uid()).is_some());

      mode(&open.join("claude"), 0o757);
      assert_eq!(resolve_cli(CliName::Claude, &dirs, uid()), None);
      mode(&open.join("claude"), 0o775);
      assert!(resolve_cli(CliName::Claude, &dirs, uid()).is_some());

      mode(&open.join("claude"), 0o755);
      if uid() != 0 {
        assert_eq!(resolve_cli(CliName::Claude, &dirs, uid().wrapping_add(4242)), None);
      }
      assert_eq!(resolve_cli(CliName::Claude, &[PathBuf::from("relative")], uid()), None);
    }

    #[test]
    fn resolve_cli_checks_the_whole_chain_and_the_symlink_target() {
      let root = tmp();
      let outer = root.path().join("outer");
      let bin = outer.join("bin");
      fs::create_dir_all(&bin).unwrap();
      mode(&outer, 0o755);
      mode(&bin, 0o755);
      script(&bin, "claude", "exit 0");
      let dirs = vec![bin.clone()];
      assert!(resolve_cli(CliName::Claude, &dirs, uid()).is_some());
      mode(&outer, 0o777);
      assert_eq!(resolve_cli(CliName::Claude, &dirs, uid()), None);
      assert!(!still_trusted(&bin.join("claude"), uid()));
      mode(&outer, 0o755);
      assert!(still_trusted(&bin.join("claude"), uid()));

      let evil = root.path().join("evil");
      fs::create_dir(&evil).unwrap();
      let target = script(&evil, "real", "exit 0");
      fs::remove_file(bin.join("claude")).unwrap();
      std::os::unix::fs::symlink(&target, bin.join("claude")).unwrap();
      mode(&evil, 0o777);
      assert_eq!(resolve_cli(CliName::Claude, &dirs, uid()), None);
      assert!(!still_trusted(&bin.join("claude"), uid()));
      mode(&evil, 0o755);
      assert!(resolve_cli(CliName::Claude, &dirs, uid()).is_some());
      mode(&target, 0o757);
      assert_eq!(resolve_cli(CliName::Claude, &dirs, uid()), None);
    }

    #[test]
    fn parse_marked_takes_last_block() {
      let block = |p: &str| format!("{BEGIN}\nPATH={p}\nCODEX_HOME=/c\nCLAUDE_CONFIG_DIR=\n{END}\n");
      let noisy = format!("welcome\n{}rc noise\n{}trailing", block("/one"), block("/two"));
      let env = parse_marked(&noisy).unwrap();
      assert_eq!(env.path.as_deref(), Some("/two"));
      assert_eq!(env.codex_home.as_deref(), Some("/c"));
      assert_eq!(env.claude_config_dir, None);
      assert_eq!(parse_marked("no markers here"), None);
      assert_eq!(parse_marked(&format!("{BEGIN}\nPATH=/x\n")), None);
    }

    fn shells_for(paths: &[&Path]) -> String {
      let mut text = String::from("# list of shells\n");
      for p in paths {
        text.push_str(&format!("{}\n", p.display()));
      }
      text
    }

    #[test]
    fn shell_allowlist() {
      let root = tmp();
      let dir = root.path().join("sh");
      fs::create_dir(&dir).unwrap();
      let zsh = script(&dir, "zsh", "exit 0");
      let fish = script(&dir, "fish", "touch ran");
      let sh = script(&dir, "sh", "exit 0");
      let listed = shells_for(&[&zsh, &fish]);
      assert!(shell_allowed(&zsh, &listed, uid()));
      assert!(!shell_allowed(&sh, &listed, uid()));
      assert!(!shell_allowed(&fish, &listed, uid()));
      assert!(!shell_allowed(Path::new("zsh"), "zsh\n", uid()));
      assert!(!shell_allowed(&zsh, "", uid()));
      assert!(!shell_allowed(&zsh, &format!("# {}\n", zsh.display()), uid()));
      mode(&zsh, 0o757);
      assert!(!shell_allowed(&zsh, &listed, uid()));
      mode(&zsh, 0o755);
      mode(&dir, 0o777);
      assert!(!shell_allowed(&zsh, &listed, uid()));
      mode(&dir, 0o755);
      assert_eq!(login_env(&fish, &listed, uid(), Duration::from_secs(2)), None);
      assert!(!root.path().join("ran").exists());
    }

    #[test]
    fn login_env_reads_markers_from_an_allowed_shell_and_times_out() {
      let root = tmp();
      let dir = root.path().join("sh");
      fs::create_dir(&dir).unwrap();
      let ok = script(&dir, "zsh", &format!("echo noise; echo {BEGIN}; echo PATH=/q:/r; echo {END}"));
      let slow_dir = root.path().join("slow");
      fs::create_dir(&slow_dir).unwrap();
      let slow = script(&slow_dir, "bash", "sleep 30");
      let listed = shells_for(&[&ok, &slow]);
      let env = login_env(&ok, &listed, uid(), Duration::from_secs(5)).unwrap();
      assert_eq!(env.path.as_deref(), Some("/q:/r"));
      let started = Instant::now();
      assert_eq!(login_env(&slow, &listed, uid(), Duration::from_millis(300)), None);
      assert!(started.elapsed() < Duration::from_secs(5));
      assert_eq!(login_env(&ok, "", uid(), Duration::from_secs(5)), None);
    }

    #[test]
    fn search_path_merges_dedups_and_drops_relative() {
      let home = Path::new("/home/u");
      let got = search_path(Some("/a:rel:/b:/a"), home, Some(OsStr::new("/b:/c:also-rel")));
      let text: Vec<String> = got.iter().map(|p| p.to_string_lossy().into_owned()).collect();
      assert_eq!(&text[..2], ["/a", "/b"]);
      assert!(text.contains(&"/home/u/.local/bin".to_string()));
      assert!(text.contains(&"/opt/homebrew/bin".to_string()));
      assert_eq!(text.iter().filter(|p| *p == "/b").count(), 1);
      assert_eq!(text.last().unwrap(), "/c");
      assert!(text.iter().all(|p| p.starts_with('/')));
    }

    struct Rig {
      _root: tempfile::TempDir,
      home: PathBuf,
      bin: PathBuf,
      log: PathBuf,
    }

    fn rig() -> Rig {
      let root = tmp();
      let home = root.path().join("home");
      let bin = root.path().join("bin");
      fs::create_dir(&home).unwrap();
      fs::create_dir(&bin).unwrap();
      mode(&bin, 0o755);
      let log = root.path().join("argv.log");
      Rig { _root: root, home, bin, log }
    }

    fn env_for(home: &Path) -> (Resolved, ChildEnv) {
      std::env::set_var("SECRET_CANARY_TM832", "leak");
      let resolved = Resolved::build(home, uid(), Some("tester"), None, Some(OsStr::new("/usr/bin:/bin")));
      let env = resolved.child_env(&|k| std::env::var_os(k));
      (resolved, env)
    }

    fn logging_claude(rig: &Rig, remove_exit: i32, add_exit: i32) -> PathBuf {
      script(
        &rig.bin,
        "claude",
        &format!(
          "for a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'; done\ncase \"$2\" in remove) exit {remove_exit};; add) exit {add_exit};; esac",
          rig.log.display()
        ),
      )
    }

    fn single(cli: PathBuf) -> Vec<Step> {
      vec![Step { argv: vec![cli.into_os_string()], tolerate_failure: false }]
    }

    #[test]
    fn run_delivers_argv_verbatim_and_tolerates_a_failed_remove() {
      let rig = rig();
      let cli = logging_claude(&rig, 1, 0);
      let (resolved, env) = env_for(&rig.home);
      let bridge = rig.home.join("My Apps/$(touch pwned)/tmgr");
      let steps = plan_commands(AgentId::ClaudeCode, ConnectAction::Install, &cli, &bridge, Some(&token())).unwrap();
      let outcome = run_steps(&steps, &env, &resolved.redactor, Duration::from_secs(10));
      assert!(outcome.ok);
      assert!(!outcome.timed_out);
      assert_eq!(outcome.steps.len(), 2);
      assert_eq!(outcome.steps[0].exit_code, Some(1));
      assert!(outcome.steps[0].tolerated);
      assert_eq!(outcome.steps[1].exit_code, Some(0));
      let expected: Vec<String> =
        steps.iter().flat_map(|s| s.argv[1..].iter().map(|a| a.to_string_lossy().into_owned())).collect();
      let logged: Vec<String> = fs::read_to_string(&rig.log).unwrap().lines().map(str::to_owned).collect();
      assert_eq!(logged, expected);
      assert!(!Path::new("pwned").exists());
      assert!(!rig.home.join("pwned").exists());
      assert!(outcome.steps[1].display.iter().all(|a| !a.contains(rig.home.to_str().unwrap())));
    }

    #[test]
    fn run_fails_and_stops_when_add_fails() {
      let rig = rig();
      let cli = logging_claude(&rig, 0, 1);
      let (resolved, env) = env_for(&rig.home);
      let steps =
        plan_commands(AgentId::ClaudeCode, ConnectAction::Install, &cli, Path::new("/b/tmgr"), Some(&token()))
          .unwrap();
      let outcome = run_steps(&steps, &env, &resolved.redactor, Duration::from_secs(10));
      assert!(!outcome.ok);
      assert!(!outcome.timed_out);
      assert_eq!(outcome.steps[1].exit_code, Some(1));

      let failing_first = vec![
        Step { argv: vec![cli.clone().into_os_string(), "mcp".into(), "add".into()], tolerate_failure: false },
        Step { argv: vec![cli.into_os_string(), "mcp".into(), "remove".into()], tolerate_failure: false },
      ];
      let stopped = run_steps(&failing_first, &env, &resolved.redactor, Duration::from_secs(10));
      assert!(!stopped.ok);
      assert_eq!(stopped.steps.len(), 1);
    }

    #[test]
    fn run_times_out_and_kills_the_process_group() {
      let rig = rig();
      let cli = script(&rig.bin, "claude", "sleep 60");
      let (resolved, env) = env_for(&rig.home);
      let started = Instant::now();
      let outcome = run_steps(&single(cli), &env, &resolved.redactor, Duration::from_millis(300));
      assert!(started.elapsed() < Duration::from_secs(5));
      assert!(outcome.timed_out);
      assert!(!outcome.ok);
      assert_eq!(outcome.steps[0].exit_code, None);
    }

    #[test]
    fn run_does_not_wait_for_a_lingering_grandchild() {
      let rig = rig();
      let cli = script(&rig.bin, "claude", "sleep 30 &\necho started");
      let (resolved, env) = env_for(&rig.home);
      let started = Instant::now();
      let outcome = run_steps(&single(cli), &env, &resolved.redactor, Duration::from_secs(20));
      assert!(started.elapsed() < Duration::from_secs(5));
      assert!(outcome.ok);
      assert!(outcome.steps[0].output.contains("started"));
    }

    #[test]
    fn run_clears_the_environment() {
      let rig = rig();
      let cli = script(&rig.bin, "claude", "/usr/bin/env");
      let (resolved, env) = env_for(&rig.home);
      assert!(std::env::var_os("SECRET_CANARY_TM832").is_some());
      let outcome = run_steps(&single(cli), &env, &resolved.redactor, Duration::from_secs(10));
      let out = &outcome.steps[0].output;
      assert!(outcome.ok);
      assert!(!out.contains("SECRET_CANARY"));
      assert!(out.contains("HOME=~"));
      assert!(out.lines().any(|l| l.starts_with("PATH=") && l.ends_with(":/usr/bin:/bin")));
    }

    #[test]
    fn run_caps_output_without_hanging() {
      let rig = rig();
      let cli = script(
        &rig.bin,
        "claude",
        "head -c 3000000 /dev/zero | tr '\\0' a\nhead -c 3000000 /dev/zero | tr '\\0' b >&2",
      );
      let (resolved, env) = env_for(&rig.home);
      let started = Instant::now();
      let outcome = run_steps(&single(cli), &env, &resolved.redactor, Duration::from_secs(20));
      assert!(started.elapsed() < Duration::from_secs(15));
      assert!(outcome.ok);
      let out = &outcome.steps[0].output;
      assert_eq!(out.len(), 2 * OUTPUT_CAP);
      assert!(out.contains('a') && out.contains('b'));
    }

    #[test]
    fn child_env_whitelists_and_validates_dirs() {
      let login = LoginEnv {
        path: Some("/x".into()),
        codex_home: Some("/c".into()),
        claude_config_dir: Some("relative".into()),
      };
      let r = Resolved::build(Path::new("/home/u"), uid(), Some("u"), Some(login), None);
      let env = r.child_env(&|k| (k == "LANG").then(|| OsString::from("C")));
      let keys: Vec<&str> = env.vars.iter().map(|(k, _)| *k).collect();
      assert!(keys.contains(&"HOME") && keys.contains(&"PATH") && keys.contains(&"LANG"));
      assert!(keys.contains(&"CODEX_HOME"));
      assert!(!keys.contains(&"CLAUDE_CONFIG_DIR"));
      assert!(!keys.contains(&"TMPDIR"));
    }

    #[test]
    fn child_env_path_keeps_only_trusted_dirs() {
      let root = tmp();
      let good = root.path().join("good");
      let open = root.path().join("open");
      fs::create_dir(&good).unwrap();
      fs::create_dir(&open).unwrap();
      mode(&good, 0o755);
      mode(&open, 0o777);
      let joined = std::env::join_paths([&open, &good]).unwrap().to_string_lossy().into_owned();
      let login = LoginEnv { path: Some(joined), ..Default::default() };
      let r = Resolved::build(root.path(), uid(), None, Some(login), None);
      let env = r.child_env(&|_| None);
      let path = env.vars.iter().find(|(k, _)| *k == "PATH").unwrap().1.to_string_lossy().into_owned();
      let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
      assert!(dirs.contains(&good));
      assert!(!dirs.contains(&open));
    }

    #[test]
    fn resolution_is_refreshed_only_when_no_cli_was_found() {
      let rig = rig();
      let empty = Resolved::build(&rig.home, uid(), None, Some(LoginEnv::default()), None);
      assert!(empty.needs_refresh());
      script(&rig.bin, "codex", "exit 0");
      let login = LoginEnv { path: Some(rig.bin.to_string_lossy().into_owned()), ..Default::default() };
      let found = Resolved::build(&rig.home, uid(), None, Some(login), None);
      assert!(!found.needs_refresh());
    }

    #[test]
    fn detect_reports_cli_previews_and_bridge() {
      let rig = rig();
      script(&rig.bin, "claude", "exit 0");
      let resolved = Resolved::build(
        &rig.home,
        uid(),
        None,
        Some(LoginEnv { path: Some(rig.bin.to_string_lossy().into_owned()), ..Default::default() }),
        None,
      );
      let ok_bridge = PathBuf::from("/Applications/TMGR.app/Contents/MacOS/tmgr");
      let json = serde_json::to_value(detect(&resolved, Ok(ok_bridge), true, false)).unwrap();
      assert_eq!(json["platformSupported"], true);
      assert_eq!(json["safeMode"], false);
      assert_eq!(json["bridge"]["ok"], true);
      assert_eq!(json["agents"][0]["id"], "claude-code");
      assert_eq!(json["agents"][0]["found"], true);
      assert_eq!(json["agents"][0]["install"].as_array().unwrap().len(), 2);
      assert_eq!(json["agents"][0]["remove"].as_array().unwrap().len(), 1);
      assert_eq!(json["agents"][1]["id"], "codex");
      assert_eq!(json["agents"][1]["found"], false);
      assert!(json["agents"][1]["cli"].is_null());

      let json = serde_json::to_value(detect(&resolved, Err(BridgeReason::Translocated), true, true)).unwrap();
      assert_eq!(json["bridge"]["reason"], "translocated");
      assert_eq!(json["safeMode"], true);
      assert_eq!(json["agents"][0]["install"].as_array().unwrap().len(), 0);
      assert_eq!(json["agents"][0]["remove"].as_array().unwrap().len(), 1);

      let json = serde_json::to_value(detect(&resolved, Ok(PathBuf::from("/b")), false, false)).unwrap();
      assert_eq!(json["platformSupported"], false);
      assert_eq!(json["bridge"]["reason"], "unsupported");
      assert_eq!(json["agents"][0]["found"], false);
    }
  }
}

#[cfg(unix)]
fn ensure_main<R: Runtime>(window: &tauri::Window<R>) -> Result<(), String> {
  if window.label() == "main" {
    Ok(())
  } else {
    Err("Agent connection may only be used from the main window".into())
  }
}

#[cfg(unix)]
fn current_bridge() -> Result<std::path::PathBuf, imp::BridgeReason> {
  let exe = std::env::current_exe().map_err(|_| imp::BridgeReason::Invalid)?;
  imp::bridge_path(&exe)
}

#[cfg(unix)]
#[tauri::command]
pub async fn agents_detect<R: Runtime>(window: tauri::Window<R>) -> Result<imp::DetectOut, String> {
  ensure_main(&window)?;
  tauri::async_runtime::spawn_blocking(|| {
    imp::detect(&imp::resolved(true), current_bridge(), cfg!(target_os = "macos"), crate::plugin_dev::plugins_safe_mode())
  })
  .await
  .map_err(|e| e.to_string())
}

#[cfg(unix)]
#[tauri::command]
pub async fn agent_connect_run<R: Runtime>(
  window: tauri::Window<R>,
  state: tauri::State<'_, crate::local_access::LocalAccessState>,
  agent: imp::AgentId,
  action: imp::ConnectAction,
  token_id: Option<String>,
) -> Result<imp::RunOutcome, String> {
  ensure_main(&window)?;
  if !cfg!(target_os = "macos") {
    return Err(imp::BridgeReason::Unsupported.message().into());
  }
  if crate::plugin_dev::plugins_safe_mode() {
    return Err("Agent connection is disabled in safe mode".into());
  }
  let app = window.app_handle().clone();
  let store = state.token_store.clone();
  tauri::async_runtime::spawn_blocking(move || {
    let bridge = match action {
      imp::ConnectAction::Install => current_bridge().map_err(|r| r.message().to_owned())?,
      imp::ConnectAction::Remove => std::path::PathBuf::new(),
    };
    let resolved = imp::resolved(false);
    let cli = resolved.cli(agent).ok_or_else(|| "The agent CLI was not found".to_owned())?;
    if !imp::still_trusted(cli, resolved.uid) {
      return Err("The agent CLI is not trusted".to_owned());
    }
    let token = match (action, token_id) {
      (imp::ConnectAction::Install, None) => return Err("A token is required to install".to_owned()),
      (imp::ConnectAction::Remove, Some(_)) => return Err("Remove does not take a token".to_owned()),
      (imp::ConnectAction::Remove, None) => None,
      (imp::ConnectAction::Install, Some(raw)) => {
        let id = imp::TokenId::parse(&raw)?;
        let live = store
          .find_live(id.as_str(), crate::tray::now_secs())?
          .ok_or_else(|| "The token is not valid".to_owned())?;
        let workspace = crate::local_workspaces::find(&app, &live.workspace_code)?;
        crate::local_demo::refuse_workspace(&workspace)?;
        Some(id)
      }
    };
    let steps = imp::plan_commands(agent, action, cli, &bridge, token.as_ref())?;
    let env = resolved.child_env(&|k| std::env::var_os(k));
    Ok(imp::run_steps(&steps, &env, &resolved.redactor, imp::STEP_TIMEOUT))
  })
  .await
  .map_err(|e| e.to_string())?
}

#[cfg(not(unix))]
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectOut {
  platform_supported: bool,
  safe_mode: bool,
  bridge: serde_json::Value,
  agents: Vec<serde_json::Value>,
}

#[cfg(not(unix))]
#[tauri::command]
pub async fn agents_detect<R: tauri::Runtime>(window: tauri::Window<R>) -> Result<DetectOut, String> {
  if window.label() != "main" {
    return Err("Agent connection may only be used from the main window".into());
  }
  Ok(DetectOut {
    platform_supported: false,
    safe_mode: crate::plugin_dev::plugins_safe_mode(),
    bridge: serde_json::json!({ "ok": false, "reason": "unsupported" }),
    agents: Vec::new(),
  })
}

#[cfg(not(unix))]
#[tauri::command]
pub async fn agent_connect_run<R: tauri::Runtime>(
  window: tauri::Window<R>,
  _agent: String,
  _action: String,
  _token_id: Option<String>,
) -> Result<serde_json::Value, String> {
  if window.label() != "main" {
    return Err("Agent connection may only be used from the main window".into());
  }
  Err("Connecting agents is only supported on macOS".into())
}
