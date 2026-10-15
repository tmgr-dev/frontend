use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::error::{Result, UsageError};
use crate::limits::{LimitSnapshot, LimitSource, LimitWindow};
use crate::model::{hex, Tool};

pub const MAX_STDIN_BYTES: usize = 1024 * 1024;
const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024;
const CHAIN_TIMEOUT: Duration = Duration::from_secs(2);
const DEFAULT_TAG: &str = "default";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RateWindow {
    pub used_pct: f64,
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatuslineSnapshot {
    pub session_id: String,
    pub config_dir_tag: Option<String>,
    pub model: Option<String>,
    pub five_hour: Option<RateWindow>,
    pub seven_day: Option<RateWindow>,
    pub cost_total_usd: Option<f64>,
    pub observed_at: DateTime<Utc>,
}

impl StatuslineSnapshot {
    pub fn to_limits(&self, account: &str) -> Vec<LimitSnapshot> {
        self.to_limits_at(account, Utc::now())
    }

    pub fn to_limits_at(&self, account: &str, now: DateTime<Utc>) -> Vec<LimitSnapshot> {
        [
            (LimitWindow::Session, &self.five_hour),
            (LimitWindow::Weekly, &self.seven_day),
        ]
        .into_iter()
        .filter_map(|(window, w)| {
            let w = w.as_ref()?;
            if w.resets_at.is_some_and(|r| r <= now) {
                return None;
            }
            Some(LimitSnapshot {
                tool: Tool::Claude,
                account: account.to_string(),
                window,
                used_pct: w.used_pct,
                resets_at: w.resets_at,
                plan: None,
                observed_at: self.observed_at,
                source: LimitSource::Statusline,
                tool_session_id: self.session_id.clone(),
            })
        })
        .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstallPlan {
    pub new_settings_text: String,
    pub installed_command: String,
    pub previous_command: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Deserialize)]
struct Stdin {
    session_id: Option<String>,
    model: Option<InModel>,
    cost: Option<InCost>,
    rate_limits: Option<InLimits>,
}

#[derive(Deserialize)]
struct InModel {
    id: Option<String>,
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct InCost {
    total_cost_usd: Option<f64>,
}

#[derive(Deserialize)]
struct InLimits {
    five_hour: Option<InWindow>,
    seven_day: Option<InWindow>,
}

#[derive(Deserialize)]
struct InWindow {
    used_percentage: Option<f64>,
    resets_at: Option<Reset>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Reset {
    Num(f64),
    Text(String),
}

impl Reset {
    fn to_time(&self) -> Option<DateTime<Utc>> {
        match self {
            Reset::Num(n) if n.is_finite() => Utc.timestamp_opt(*n as i64, 0).single(),
            Reset::Num(_) => None,
            Reset::Text(s) => DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|t| t.with_timezone(&Utc)),
        }
    }
}

impl InWindow {
    fn into_window(self) -> Option<RateWindow> {
        Some(RateWindow {
            used_pct: self.used_percentage.filter(|v| v.is_finite())?,
            resets_at: self.resets_at.and_then(|r| r.to_time()),
        })
    }
}

fn sanitise(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(64)
        .collect()
}

pub fn config_dir_tag(config_dir: Option<&str>) -> String {
    match config_dir.filter(|c| !c.is_empty()) {
        None => DEFAULT_TAG.to_string(),
        Some(dir) => hex(&Sha256::digest(dir.as_bytes()))[..16].to_string(),
    }
}

pub fn account_tag(root: &Path) -> String {
    config_dir_tag(Some(&crate::paths::canonical(root).to_string_lossy()))
}

pub fn parse_snapshot(
    config_dir_tag: Option<&str>,
    stdin_json: &[u8],
    now: DateTime<Utc>,
) -> Result<StatuslineSnapshot> {
    if stdin_json.len() > MAX_STDIN_BYTES {
        return Err(UsageError::Invalid("statusline input is too large".into()));
    }
    let input: Stdin = serde_json::from_slice(stdin_json)?;
    let session_id = sanitise(input.session_id.as_deref().unwrap_or(""));
    if session_id.is_empty() {
        return Err(UsageError::Invalid(
            "statusline input has no session id".into(),
        ));
    }
    let limits = input.rate_limits;
    let (five_hour, seven_day) = match limits {
        Some(l) => (
            l.five_hour.and_then(InWindow::into_window),
            l.seven_day.and_then(InWindow::into_window),
        ),
        None => (None, None),
    };
    let tag = config_dir_tag.map(sanitise).filter(|t| !t.is_empty());
    Ok(StatuslineSnapshot {
        session_id,
        config_dir_tag: Some(tag.unwrap_or_else(|| DEFAULT_TAG.to_string())),
        model: input
            .model
            .and_then(|m| m.id.filter(|s| !s.is_empty()).or(m.display_name)),
        five_hour,
        seven_day,
        cost_total_usd: input
            .cost
            .and_then(|c| c.total_cost_usd)
            .filter(|v| v.is_finite()),
        observed_at: now,
    })
}

fn make_dirs(path: &Path) -> Result<()> {
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    Ok(b.create(path)?)
}

static SEQ: AtomicU64 = AtomicU64::new(0);

pub fn write_snapshot(
    dir: &Path,
    config_dir_tag: Option<&str>,
    stdin_json: &[u8],
) -> Result<StatuslineSnapshot> {
    let snap = parse_snapshot(config_dir_tag, stdin_json, Utc::now())?;
    let tag = snap.config_dir_tag.clone().unwrap_or_default();
    let target_dir = dir.join("claude").join(&tag);
    make_dirs(&target_dir)?;
    let tmp = target_dir.join(format!(
        ".{}.{}.{}.tmp",
        snap.session_id,
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let body = serde_json::to_vec(&snap)?;
    let written = write_private(&tmp, &body).and_then(|_| {
        Ok(std::fs::rename(
            &tmp,
            target_dir.join(format!("{}.json", snap.session_id)),
        )?)
    });
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written?;
    Ok(snap)
}

fn write_private(path: &Path, body: &[u8]) -> Result<()> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    f.write_all(body)?;
    Ok(())
}

fn read_capped(path: &Path) -> Option<Vec<u8>> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_SNAPSHOT_BYTES {
        return None;
    }
    let mut buf = Vec::with_capacity(meta.len() as usize);
    std::fs::File::open(path)
        .ok()?
        .take(MAX_SNAPSHOT_BYTES + 1)
        .read_to_end(&mut buf)
        .ok()?;
    (buf.len() as u64 <= MAX_SNAPSHOT_BYTES).then_some(buf)
}

pub fn read_snapshots(dir: &Path) -> Result<Vec<StatuslineSnapshot>> {
    let root = dir.join("claude");
    let Ok(tags) = std::fs::read_dir(&root) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for tag in tags.flatten() {
        let tag_name = tag.file_name().to_string_lossy().into_owned();
        if sanitise(&tag_name) != tag_name {
            continue;
        }
        let Ok(files) = std::fs::read_dir(tag.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path: PathBuf = file.path();
            let Some(stem) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_suffix(".json"))
            else {
                continue;
            };
            let Some(bytes) = read_capped(&path) else {
                continue;
            };
            let Ok(mut snap) = serde_json::from_slice::<StatuslineSnapshot>(&bytes) else {
                continue;
            };
            if snap.session_id != stem || sanitise(stem) != stem {
                continue;
            }
            snap.config_dir_tag = Some(tag_name.clone());
            out.push(snap);
        }
    }
    out.sort_by(|a, b| (&a.config_dir_tag, &a.session_id).cmp(&(&b.config_dir_tag, &b.session_id)));
    Ok(out)
}

fn windows_over(path: &Path, now: DateTime<Utc>) -> bool {
    let Some(snap) =
        read_capped(path).and_then(|b| serde_json::from_slice::<StatuslineSnapshot>(&b).ok())
    else {
        return false;
    };
    let windows: Vec<&RateWindow> = [&snap.five_hour, &snap.seven_day]
        .into_iter()
        .flatten()
        .collect();
    !windows.is_empty()
        && windows
            .iter()
            .all(|w| w.resets_at.is_some_and(|r| r <= now))
}

pub fn prune_snapshots(dir: &Path, now: DateTime<Utc>, max_age_days: u64) -> u64 {
    let Some(cutoff) =
        std::time::SystemTime::from(now).checked_sub(Duration::from_secs(max_age_days * 86_400))
    else {
        return 0;
    };
    let Ok(tags) = std::fs::read_dir(dir.join("claude")) else {
        return 0;
    };
    let mut removed = 0;
    for tag in tags.flatten() {
        let Ok(files) = std::fs::read_dir(tag.path()) else {
            continue;
        };
        for file in files.flatten() {
            let old = file
                .metadata()
                .ok()
                .filter(std::fs::Metadata::is_file)
                .and_then(|m| m.modified().ok())
                .is_some_and(|t| t < cutoff);
            if (old || windows_over(&file.path(), now)) && std::fs::remove_file(file.path()).is_ok()
            {
                removed += 1;
            }
        }
        let _ = std::fs::remove_dir(tag.path());
    }
    removed
}

pub fn run_hook(
    stdin: &[u8],
    config_dir_tag: Option<&str>,
    dir: &Path,
    chain: Option<&str>,
    runner: &mut dyn FnMut(&str, &[u8]) -> Vec<u8>,
) -> Vec<u8> {
    let _ = write_snapshot(dir, config_dir_tag, stdin);
    match chain.map(str::trim).filter(|c| !c.is_empty()) {
        Some(cmd) => runner(cmd, stdin),
        None => Vec::new(),
    }
}

pub fn run_shell_chain(cmd: &str, stdin: &[u8]) -> Vec<u8> {
    let Ok(mut child) = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Vec::new();
    };
    let input = stdin.to_vec();
    let mut pipe = child.stdin.take();
    let writer = std::thread::spawn(move || {
        if let Some(p) = pipe.as_mut() {
            let _ = p.write_all(&input);
        }
    });
    let mut stdout = child.stdout.take();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(o) = stdout.as_mut() {
            let _ = o.take(MAX_STDIN_BYTES as u64).read_to_end(&mut buf);
        }
        let _ = tx.send(buf);
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < CHAIN_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(5))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    let _ = writer.join();
    rx.recv_timeout(Duration::from_millis(200))
        .unwrap_or_default()
}

pub fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&b))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

pub fn shell_split(cmd: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut started = false;
    let mut chars = cmd.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                started = true;
                loop {
                    match chars.next()? {
                        '\'' => break,
                        ch => cur.push(ch),
                    }
                }
            }
            '"' => {
                started = true;
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => match chars.next()? {
                            e @ ('"' | '\\' | '$' | '`') => cur.push(e),
                            other => {
                                cur.push('\\');
                                cur.push(other);
                            }
                        },
                        '$' | '`' => return None,
                        ch => cur.push(ch),
                    }
                }
            }
            '\\' => {
                started = true;
                cur.push(chars.next()?);
            }
            ';' | '|' | '&' | '<' | '>' | '(' | ')' | '`' | '$' | '\n' => return None,
            c if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            c => {
                started = true;
                cur.push(c);
            }
        }
    }
    if started {
        out.push(cur);
    }
    Some(out)
}

fn basename(p: &str) -> &str {
    p.rsplit(['/', '\\']).next().unwrap_or(p)
}

fn indent_of(text: &str) -> Vec<u8> {
    text.lines()
        .find(|l| l.starts_with([' ', '\t']))
        .map(|l| {
            l.bytes()
                .take_while(|b| *b == b' ' || *b == b'\t')
                .collect::<Vec<u8>>()
        })
        .unwrap_or_else(|| b"  ".to_vec())
}

fn render_json(value: &Value, original: &str) -> String {
    let indent = indent_of(original);
    let fmt = serde_json::ser::PrettyFormatter::with_indent(&indent);
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    if value.serialize(&mut ser).is_err() {
        return original.to_string();
    }
    let mut text = String::from_utf8_lossy(&buf).to_string();
    if original.is_empty() || original.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn invalid(what: &str) -> UsageError {
    UsageError::Invalid(format!(
        "{what} is not in the expected shape; nothing was changed"
    ))
}

fn load_settings(text: &str) -> Result<Value> {
    if text.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let root: Value = serde_json::from_str(text).map_err(|_| {
        UsageError::Invalid("the settings file is not valid JSON; nothing was changed".into())
    })?;
    if root.is_object() {
        Ok(root)
    } else {
        Err(invalid("the settings file"))
    }
}

pub fn hook_command(exe: &str, chain: Option<&str>) -> String {
    let mut cmd = format!("{} usage statusline-hook", shell_quote(exe));
    if let Some(prev) = chain {
        cmd.push_str(" --chain ");
        cmd.push_str(&shell_quote(prev));
    }
    cmd
}

fn parse_ours(cmd: &str, exe: &str) -> Option<Option<String>> {
    let t = shell_split(cmd)?;
    let first = basename(t.first()?);
    let known = matches!(first, "tmgr" | "tmgr.exe") || first == basename(exe);
    if !known
        || t.get(1).map(String::as_str) != Some("usage")
        || t.get(2).map(String::as_str) != Some("statusline-hook")
    {
        return None;
    }
    match &t[3..] {
        [] => Some(None),
        [flag, prev] if flag == "--chain" => {
            Some(Some(prev.clone()).filter(|p| !p.trim().is_empty()))
        }
        _ => None,
    }
}

fn status_line(root: &Value) -> Result<Option<&Map<String, Value>>> {
    match root.get("statusLine") {
        None => Ok(None),
        Some(Value::Object(m)) => {
            if m.get("type").and_then(Value::as_str) != Some("command")
                || !m.get("command").is_some_and(Value::is_string)
            {
                return Err(invalid("\"statusLine\""));
            }
            Ok(Some(m))
        }
        Some(_) => Err(invalid("\"statusLine\"")),
    }
}

fn finish(old: &str, root: &Value, original: &Value) -> String {
    if root == original && !old.trim().is_empty() {
        old.to_string()
    } else {
        render_json(root, old)
    }
}

pub fn plan_install(settings_text: &str, exe: &str, chain: Option<&str>) -> Result<InstallPlan> {
    let mut root = load_settings(settings_text)?;
    let original = root.clone();
    let current = status_line(&root)?
        .and_then(|m| m.get("command").and_then(Value::as_str))
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(str::to_string);
    let previous = match current {
        Some(cmd) => match parse_ours(&cmd, exe) {
            Some(prev) => prev,
            None => Some(cmd),
        },
        None => None,
    }
    .or_else(|| {
        chain
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(str::to_string)
    });
    let installed = hook_command(exe, previous.as_deref());
    let obj = root
        .as_object_mut()
        .ok_or_else(|| invalid("the settings file"))?;
    match obj.get_mut("statusLine").and_then(Value::as_object_mut) {
        Some(m) => {
            m.insert("command".into(), Value::String(installed.clone()));
        }
        None => {
            obj.insert(
                "statusLine".into(),
                json!({"type": "command", "command": installed}),
            );
        }
    }
    let mut warnings = Vec::new();
    if !exe.starts_with('/') && !exe.contains(['/', '\\']) {
        warnings.push("the executable path is not absolute; Claude Code may not find it".into());
    }
    Ok(InstallPlan {
        new_settings_text: finish(settings_text, &root, &original),
        installed_command: installed,
        previous_command: previous,
        warnings,
    })
}

pub fn plan_uninstall(settings_text: &str) -> Result<InstallPlan> {
    let mut root = load_settings(settings_text)?;
    let original = root.clone();
    let current = status_line(&root)?
        .and_then(|m| m.get("command").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_default();
    let ours = match shell_split(&current).as_deref() {
        Some([exe, usage, hook, ..]) if usage == "usage" && hook == "statusline-hook" => {
            parse_ours(&current, exe)
        }
        _ => None,
    };
    let Some(previous) = ours else {
        return Ok(InstallPlan {
            new_settings_text: settings_text.to_string(),
            installed_command: String::new(),
            previous_command: None,
            warnings: vec!["the status line is not managed by tmgr; nothing to remove".into()],
        });
    };
    let obj = root
        .as_object_mut()
        .ok_or_else(|| invalid("the settings file"))?;
    match &previous {
        Some(prev) => {
            if let Some(m) = obj.get_mut("statusLine").and_then(Value::as_object_mut) {
                m.insert("command".into(), Value::String(prev.clone()));
            }
        }
        None => {
            obj.remove("statusLine");
        }
    }
    Ok(InstallPlan {
        new_settings_text: finish(settings_text, &root, &original),
        installed_command: current,
        previous_command: previous,
        warnings: Vec::new(),
    })
}
