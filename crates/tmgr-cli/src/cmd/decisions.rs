use crate::out::Output;
use clap::{Args, Subcommand};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tmgr_core::error::{Error, Result as CoreResult};
use tmgr_core::text::sanitize_line;
use tmgr_ipc::decisions::{
    self, Cancel, CardIn, Check, Client, DiffSpec, Event, Fact, Found, Inbox, LinkItem, Links, Opt,
    PatchIn, PendingDelivery, Response, Secret, Source, WaitCard, MAX_DIFFS, MAX_LINE, MAX_LINKS,
    MAX_PATCHES, MAX_PATCH_BYTES,
};
use tmgr_ipc::loopback::{self, HttpErr};

pub const KINDS: [&str; 5] = ["decision", "review", "release", "check", "question"];
const FACT_STATES: [&str; 3] = ["ok", "bad", "wait"];
const CHECK_STATES: [&str; 3] = ["ok", "bad", "todo"];
const WAIT_ENV: &str = "TMGR_DECISIONS_WAIT_MS";
const DEFAULT_WAIT: Duration = Duration::from_secs(600);
const CHUNK: Duration = Duration::from_secs(30);
const RETRY_MIN: Duration = Duration::from_millis(250);
const RETRY_MAX: Duration = Duration::from_secs(5);
const SLICE: Duration = Duration::from_secs(1);
pub const MAX_TITLE: usize = 300;
pub const MAX_CONTEXT: usize = 64 * 1024;
pub const HOOK_CONTEXT: usize = 16 * 1024;
const MAX_ITEMS: usize = 50;
const MAX_OPTIONS: usize = 9;
const MAX_TEXT: usize = 1000;
pub const PENDING_NOTE: &str = "tmgr desktop is not running; the card will appear when it starts";

pub fn default_wait() -> Duration {
    std::env::var(WAIT_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map_or(DEFAULT_WAIT, Duration::from_millis)
}

pub fn agent_name(client: &str) -> &'static str {
    let lower = client.to_ascii_lowercase();
    if lower.contains("claude") {
        "claude-code"
    } else if lower.contains("codex") {
        "codex"
    } else {
        "other"
    }
}

pub fn parent_pid() -> u32 {
    std::os::unix::process::parent_id()
}

pub fn runner_session() -> Option<String> {
    std::env::var("TMGR_SESSION_ID")
        .ok()
        .filter(|s| tmgr_ipc::proto::valid_session_id(s))
}

pub fn source(agent: &str, pid: Option<u32>) -> Source {
    Source {
        agent: agent.to_string(),
        session: None,
        cwd: std::env::current_dir()
            .ok()
            .map(|p| p.display().to_string()),
        title: None,
        pid,
        runner_session: runner_session(),
    }
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

pub fn is_refusal(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::Other
}

pub enum Sent {
    Live(String, Option<String>),
    Spooled(String),
}

pub enum SentFull {
    Live(Box<Response>),
    Spooled(String),
}

fn claude_inbox_with(get: impl Fn(&str) -> Option<String>) -> Option<Inbox> {
    if ["OPENCODE", "GEMINI_CLI"].iter().any(|k| get(k).is_some()) {
        return None;
    }
    let endpoint = get("CLAUDE_CODE_MESSAGING_SOCKET").filter(|p| Path::new(p).is_absolute())?;
    let token = get("CLAUDE_CODE_MESSAGING_TOKEN").filter(|t| !t.is_empty())?;
    Some(Inbox {
        kind: "claude".into(),
        endpoint,
        token: Some(Secret::new(token)),
        session: get("CLAUDE_CODE_SESSION_ID").filter(|s| !s.is_empty()),
    })
}

pub fn claude_inbox_from_env() -> Option<Inbox> {
    claude_inbox_with(|k| std::env::var(k).ok())
}

pub fn hook_inbox(card: &CardIn) -> Option<Inbox> {
    hook_inbox_with(card, claude_inbox_from_env)
}

fn hook_inbox_with(card: &CardIn, inbox: impl FnOnce() -> Option<Inbox>) -> Option<Inbox> {
    (card.kind == "stopped").then(inbox).flatten()
}

pub trait OpencodeProbe {
    #[allow(clippy::result_unit_err)]
    fn listener_pid(&self, pid: u32, addr: SocketAddr) -> Result<bool, ()>;
    fn get(
        &self,
        addr: SocketAddr,
        path: &str,
        auth: Option<&str>,
    ) -> Result<(u16, Vec<u8>), HttpErr>;
}

struct RealProbe;

impl OpencodeProbe for RealProbe {
    fn listener_pid(&self, pid: u32, addr: SocketAddr) -> Result<bool, ()> {
        loopback::listener_pid(pid, addr)
    }

    fn get(
        &self,
        addr: SocketAddr,
        path: &str,
        auth: Option<&str>,
    ) -> Result<(u16, Vec<u8>), HttpErr> {
        loopback::request(addr, "GET", path, auth, None, OPENCODE_DEADLINE)
    }
}

const OPENCODE_DEADLINE: Duration = Duration::from_millis(1500);

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn canonical_dir(s: &str) -> String {
    std::fs::canonicalize(s).map_or_else(|_| s.to_string(), |p| p.to_string_lossy().into_owned())
}

fn opencode_get(
    probe: &dyn OpencodeProbe,
    addr: SocketAddr,
    path: &str,
    auth: Option<&str>,
) -> Option<Value> {
    match probe.get(addr, path, auth) {
        Ok((200, body)) => serde_json::from_slice(&body).ok(),
        _ => None,
    }
}

fn session_root(id: &str, parents: &HashMap<String, Option<String>>) -> Option<String> {
    let mut cur = id.to_string();
    for _ in 0..=parents.len() {
        match parents.get(&cur)? {
            None => return Some(cur),
            Some(p) => cur = p.clone(),
        }
    }
    None
}

fn opencode_inbox_with(
    get: impl Fn(&str) -> Option<String>,
    probe: &dyn OpencodeProbe,
    meta_session: Option<&str>,
) -> Option<Inbox> {
    get("OPENCODE").filter(|v| !v.is_empty())?;
    let addr = loopback::parse_url(&get("TMGR_OPENCODE_URL")?)?;
    let pid: u32 = get("OPENCODE_PID")?.parse().ok()?;
    if probe.listener_pid(pid, addr) != Ok(true) {
        return None;
    }
    let password = get("OPENCODE_SERVER_PASSWORD").filter(|p| !p.is_empty());
    let auth = password.as_deref().map(loopback::basic_header);
    let auth = auth.as_deref();
    let cwd = canonical_dir(&std::env::current_dir().ok()?.to_string_lossy());
    let query = format!("?directory={}", url_encode(&cwd));
    let status = opencode_get(probe, addr, &format!("/session/status{query}"), auth)?;
    let listed = opencode_get(probe, addr, &format!("/session{query}"), auth)?;
    let sessions = listed.as_array()?;
    let mut parents: HashMap<String, Option<String>> = HashMap::new();
    let mut here: HashSet<&str> = HashSet::new();
    let mut canonical: HashMap<String, String> = HashMap::new();
    for s in sessions {
        let id = s.get("id")?.as_str()?;
        parents.insert(
            id.to_string(),
            s.get("parentID").and_then(Value::as_str).map(String::from),
        );
        let dir = s.get("directory")?.as_str()?;
        if dir == cwd
            || canonical
                .entry(dir.to_string())
                .or_insert_with(|| canonical_dir(dir))
                .as_str()
                == cwd
        {
            here.insert(id);
        }
    }
    let busy: HashSet<String> = status
        .as_object()?
        .iter()
        .filter(|(id, v)| v["type"] == "busy" && here.contains(id.as_str()))
        .filter_map(|(id, _)| session_root(id, &parents))
        .collect();
    let id = match meta_session {
        Some(m) => {
            busy.contains(&session_root(m, &parents)?).then_some(m)?;
            m.to_string()
        }
        None => {
            if busy.len() != 1 {
                return None;
            }
            busy.into_iter().next()?
        }
    };
    loopback::valid_session_id(&id).then_some(())?;
    Some(Inbox {
        kind: "opencode".into(),
        endpoint: loopback::canonical_url(addr),
        token: password.map(Secret::new),
        session: Some(id),
    })
}

fn inbox_from_env(meta_session: Option<&str>) -> Option<Inbox> {
    claude_inbox_from_env()
        .or_else(|| opencode_inbox_with(|k| std::env::var(k).ok(), &RealProbe, meta_session))
}

pub fn send_card_full(card: CardIn) -> io::Result<SentFull> {
    send_card_full_meta(card, None)
}

pub fn send_card_full_meta(mut card: CardIn, meta_session: Option<&str>) -> io::Result<SentFull> {
    card.id.get_or_insert_with(decisions::new_card_id);
    let dir = decisions::decisions_dir()?;
    match Client::open_in(&dir)
        .and_then(|c| c.create_with(card.clone(), inbox_from_env(meta_session)))
    {
        Ok(response) => Ok(SentFull::Live(Box::new(response))),
        Err(e) if is_refusal(&e) => Err(e),
        Err(_) => decisions::spool(&dir, &card).map(SentFull::Spooled),
    }
}

pub fn send_card(card: CardIn) -> io::Result<Sent> {
    match send_card_full(card)? {
        SentFull::Live(response) => Ok(Sent::Live(
            response.card_id.unwrap_or_default(),
            response.outcome,
        )),
        SentFull::Spooled(id) => Ok(Sent::Spooled(id)),
    }
}

pub enum Waited {
    Event(String, Box<Event>),
    Pending { unreachable: bool },
    Refused(String),
    Cancelled,
}

static NO_WAIT_ANY: AtomicBool = AtomicBool::new(false);

fn wait_rotating(
    client: &Client,
    cards: &[WaitCard],
    timeout: Duration,
    final_only: bool,
    cancel: Option<&Cancel>,
) -> io::Result<Option<(String, Event)>> {
    let end = Instant::now() + timeout;
    loop {
        for c in cards {
            let left = end.saturating_duration_since(Instant::now());
            let slice = left.min(SLICE);
            if let Some(e) =
                client.wait_cancellable(&c.card_id, c.since, slice, final_only, cancel)?
            {
                return Ok(Some((c.card_id.clone(), e)));
            }
            if end <= Instant::now() {
                return Ok(None);
            }
        }
    }
}

fn wait_cards_once(
    client: &Client,
    cards: &[WaitCard],
    timeout: Duration,
    final_only: bool,
    cancel: Option<&Cancel>,
) -> io::Result<Option<(String, Event)>> {
    if let [c] = cards {
        return Ok(client
            .wait_cancellable(&c.card_id, c.since, timeout, final_only, cancel)?
            .map(|e| (c.card_id.clone(), e)));
    }
    if !NO_WAIT_ANY.load(Ordering::Relaxed) {
        match client.wait_any(cards, timeout, final_only, cancel) {
            Err(e) if e.kind() == io::ErrorKind::Unsupported => {
                NO_WAIT_ANY.store(true, Ordering::Relaxed)
            }
            r => return r,
        }
    }
    wait_rotating(client, cards, timeout, final_only, cancel)
}

fn wait_once(
    cards: &[WaitCard],
    timeout: Duration,
    final_only: bool,
    cancel: Option<&Cancel>,
) -> Waited {
    let client = decisions::decisions_dir().and_then(|d| Client::open_in(&d));
    let Ok(client) = client else {
        return Waited::Pending { unreachable: true };
    };
    match wait_cards_once(&client, cards, timeout, final_only, cancel) {
        Ok(Some((key, e))) => Waited::Event(key, Box::new(e)),
        Ok(None) => Waited::Pending { unreachable: false },
        Err(e) if e.kind() == io::ErrorKind::Interrupted => Waited::Cancelled,
        Err(e) if e.kind() == io::ErrorKind::ResourceBusy => Waited::Pending { unreachable: true },
        Err(e) if is_refusal(&e) => Waited::Refused(e.to_string()),
        Err(_) => Waited::Pending { unreachable: true },
    }
}

static PARKED: AtomicUsize = AtomicUsize::new(0);

struct Parked;

impl Parked {
    fn enter() -> Parked {
        PARKED.fetch_add(1, Ordering::SeqCst);
        Parked
    }
}

impl Drop for Parked {
    fn drop(&mut self) {
        PARKED.fetch_sub(1, Ordering::SeqCst);
    }
}

pub fn parked() -> usize {
    PARKED.load(Ordering::SeqCst)
}

pub fn wait_loop(
    card_id: &str,
    since: u64,
    total: Duration,
    final_only: bool,
    cancel: Option<&Cancel>,
    tick: &mut dyn FnMut(),
) -> Waited {
    let card = WaitCard {
        card_id: card_id.to_string(),
        since,
    };
    wait_cards(&[card], total, final_only, cancel, tick)
}

pub fn wait_cards(
    cards: &[WaitCard],
    total: Duration,
    final_only: bool,
    cancel: Option<&Cancel>,
    tick: &mut dyn FnMut(),
) -> Waited {
    let end = Instant::now() + total;
    let mut pause = RETRY_MIN;
    let mut ticked = Instant::now();
    loop {
        if cancel.is_some_and(Cancel::is_cancelled) {
            return Waited::Cancelled;
        }
        let left = end.saturating_duration_since(Instant::now());
        let waited = {
            let _parked = Parked::enter();
            wait_once(cards, left.min(CHUNK), final_only, cancel)
        };
        match waited {
            Waited::Pending { unreachable: false } if left > CHUNK => {
                pause = RETRY_MIN;
                ticked = Instant::now();
                tick();
            }
            Waited::Pending { unreachable: true } => {
                let left = end.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return waited;
                }
                let _parked = Parked::enter();
                let nap = pause.min(left);
                match cancel {
                    Some(c) => {
                        if c.sleep(nap) {
                            return Waited::Cancelled;
                        }
                    }
                    None => std::thread::sleep(nap),
                }
                pause = (pause * 2).min(RETRY_MAX);
                if ticked.elapsed() >= CHUNK {
                    ticked = Instant::now();
                    tick();
                }
            }
            other => return other,
        }
    }
}

pub fn pending_json(card_id: &str, unreachable: bool) -> Value {
    let mut v = serde_json::json!({"status": "pending", "card_id": card_id});
    if unreachable {
        v["note"] = Value::String(PENDING_NOTE.to_string());
    }
    v
}

#[derive(Default)]
pub struct Spec {
    pub title: String,
    pub question: Option<String>,
    pub context: Option<String>,
    pub kind: String,
    pub blocking: bool,
    pub options: Vec<String>,
    pub recommended: Option<usize>,
    pub facts: Vec<Fact>,
    pub checks: Vec<Check>,
    pub repo: Option<String>,
    pub base: Option<String>,
    pub head: Option<String>,
    pub task: Option<String>,
    pub diffs: Vec<DiffSpec>,
    pub patches: Vec<PatchIn>,
    pub prs: Vec<String>,
    pub commits: Vec<String>,
    pub tags: Vec<String>,
    pub links: Vec<String>,
    pub replaces: Option<String>,
    pub key: Option<String>,
    pub force_new: Option<bool>,
}

const MAX_URL: usize = 2000;
const MAX_KEY: usize = 200;
const DEFAULT_PATCH_LABEL: &str = "patch";

fn is_http(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

fn http_url(s: &str) -> Result<String, String> {
    let s = s.trim();
    if !is_http(s) {
        return Err(format!("{s:?} is not an http or https URL"));
    }
    if s.chars().any(|c| c.is_whitespace() || c.is_control()) || s.chars().count() > MAX_URL {
        return Err(format!(
            "{s:?} is not a usable URL (at most {MAX_URL} characters)"
        ));
    }
    Ok(s.to_string())
}

fn github_ref(s: &str, sep: char) -> Option<(&str, &str, &str)> {
    let (head, tail) = s.split_once(sep)?;
    let (owner, repo) = head.split_once('/')?;
    let owner_ok = !owner.is_empty()
        && owner.len() <= 39
        && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    let repo_ok = !repo.is_empty()
        && repo.len() <= 100
        && repo != "."
        && repo != ".."
        && repo
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    (owner_ok && repo_ok && !tail.is_empty()).then_some((owner, repo, tail))
}

fn valid_tag(t: &str) -> bool {
    !t.is_empty()
        && t.len() <= 100
        && !t.starts_with(['-', '/', '.'])
        && !t.ends_with(['/', '.'])
        && !t.contains("..")
        && !t.contains("//")
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+' | '/'))
}

fn item(kind: &str, url: String, label: Option<String>) -> LinkItem {
    LinkItem {
        kind: kind.into(),
        url,
        label,
    }
}

fn reject_scheme(s: &str, what: &str, form: &str) -> String {
    if s.contains("://") {
        format!("{s:?}: only http and https URLs are allowed for {what}")
    } else {
        format!("{s:?} must be a {what} URL or {form}")
    }
}

fn refuse_foreign_scheme(s: &str, what: &str) -> Result<(), String> {
    if decisions::has_foreign_scheme(s) {
        return Err(format!(
            "{s:?}: only http and https URLs are allowed for a {what}"
        ));
    }
    Ok(())
}

pub fn parse_pr(s: &str) -> Result<LinkItem, String> {
    let s = s.trim();
    if is_http(s) {
        return Ok(item("pr", http_url(s)?, None));
    }
    match github_ref(s, '#') {
        Some((o, r, n)) if n.len() <= 9 && n.bytes().all(|b| b.is_ascii_digit()) && n != "0" => Ok(
            item("pr", format!("https://github.com/{o}/{r}/pull/{n}"), None),
        ),
        _ => Err(reject_scheme(s, "pull request", "owner/repo#123")),
    }
}

fn short_sha(sha: &str) -> String {
    sha.chars().take(7).collect::<String>().to_ascii_lowercase()
}

pub fn parse_commit(s: &str) -> Result<LinkItem, String> {
    let s = s.trim();
    if is_http(s) {
        let label = s.split_once("/commit/").and_then(|(_, rest)| {
            let sha: String = rest.chars().take_while(char::is_ascii_hexdigit).collect();
            (sha.len() >= 7).then(|| short_sha(&sha))
        });
        return Ok(item("commit", http_url(s)?, label));
    }
    match github_ref(s, '@') {
        Some((o, r, sha))
            if (7..=40).contains(&sha.len()) && sha.chars().all(|c| c.is_ascii_hexdigit()) =>
        {
            Ok(item(
                "commit",
                format!("https://github.com/{o}/{r}/commit/{sha}"),
                Some(short_sha(sha)),
            ))
        }
        _ => Err(reject_scheme(s, "commit", "owner/repo@<7-40 hex sha>")),
    }
}

pub fn parse_tag(s: &str) -> Result<LinkItem, String> {
    let s = s.trim();
    if is_http(s) {
        return Ok(item("tag", http_url(s)?, None));
    }
    match github_ref(s, '@') {
        Some((o, r, tag)) if valid_tag(tag) => Ok(item(
            "tag",
            format!("https://github.com/{o}/{r}/releases/tag/{tag}"),
            None,
        )),
        _ => Err(reject_scheme(s, "tag", "owner/repo@tag")),
    }
}

pub fn parse_link(s: &str) -> Result<LinkItem, String> {
    let (label, url) = match s.split_once('=') {
        Some((l, u)) if !l.contains("://") => (Some(l.trim()), u),
        _ => (None, s),
    };
    if let Some(l) = label {
        if l.is_empty() || l.chars().count() > MAX_TITLE {
            return Err(format!("link labels must be 1 to {MAX_TITLE} characters"));
        }
    }
    Ok(item("url", http_url(url)?, label.map(str::to_string)))
}

pub fn parse_diff(s: &str) -> Result<DiffSpec, String> {
    let (dir, refs) = match s.rsplit_once('=') {
        Some((d, r)) => (d, Some(r)),
        None => (s, None),
    };
    if dir.is_empty() {
        return Err(format!("{s:?}: --diff needs a directory"));
    }
    let (base, head) = match refs {
        None => (None, None),
        Some(r) => match r.split_once("..") {
            None => (Some(r), None),
            Some((b, h)) => (Some(b), Some(h)),
        },
    };
    for r in [base, head].into_iter().flatten() {
        if r.is_empty() || r.contains("..") {
            return Err(format!("{s:?}: use --diff DIR[=BASE[..HEAD]]"));
        }
    }
    Ok(DiffSpec {
        repo: dir.to_string(),
        base: base.map(str::to_string),
        head: head.map(str::to_string),
    })
}

pub fn read_patch(arg: &str, stdin_used: &mut bool) -> Result<PatchIn, String> {
    use std::io::Read;
    let (label, source) = if arg == "-" || Path::new(arg).is_file() {
        (None, arg)
    } else {
        match arg.split_once('=') {
            Some((l, f)) if !l.trim().is_empty() => (Some(l.trim().to_string()), f),
            _ => (None, arg),
        }
    };
    let cap = MAX_PATCH_BYTES as u64 + 1;
    let mut bytes = Vec::new();
    let default = if source == "-" {
        if std::mem::replace(stdin_used, true) {
            return Err("stdin can be used for only one --patch".into());
        }
        io::stdin()
            .lock()
            .take(cap)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("cannot read the patch from stdin: {e}"))?;
        DEFAULT_PATCH_LABEL.to_string()
    } else {
        std::fs::File::open(source)
            .and_then(|f| f.take(cap).read_to_end(&mut bytes))
            .map_err(|e| format!("cannot read the patch file {source:?}: {e}"))?;
        Path::new(source).file_name().map_or_else(
            || DEFAULT_PATCH_LABEL.to_string(),
            |n| n.to_string_lossy().into(),
        )
    };
    if bytes.len() > MAX_PATCH_BYTES {
        return Err(format!("patches may total at most {MAX_PATCH_BYTES} bytes"));
    }
    Ok(PatchIn {
        label: label.unwrap_or(default),
        text: String::from_utf8_lossy(&bytes).into_owned(),
    })
}

pub fn split_state<'a>(s: &'a str, allowed: &[&str]) -> Result<(&'a str, &'a str), String> {
    let (state, text) = s
        .split_once(':')
        .ok_or_else(|| format!("{s:?} must look like STATE:TEXT"))?;
    if !allowed.contains(&state) {
        return Err(format!("state must be one of {}", allowed.join(", ")));
    }
    check_text(text.trim())?;
    Ok((state, text.trim()))
}

fn check_text(text: &str) -> Result<(), String> {
    if text.is_empty() || text.chars().count() > MAX_TEXT {
        return Err(format!("text must be 1 to {MAX_TEXT} characters"));
    }
    Ok(())
}

pub fn parse_fact(s: &str) -> Result<Fact, String> {
    let (state, text) = split_state(s, &FACT_STATES)?;
    Ok(Fact {
        state: state.into(),
        text: text.into(),
        link: None,
    })
}

pub fn parse_check(s: &str) -> Result<Check, String> {
    let (state, text) = split_state(s, &CHECK_STATES)?;
    Ok(Check {
        state: state.into(),
        text: text.into(),
    })
}

pub fn fact_checked(f: Fact) -> Result<Fact, String> {
    if !FACT_STATES.contains(&f.state.as_str()) {
        return Err(format!(
            "fact state must be one of {}",
            FACT_STATES.join(", ")
        ));
    }
    check_text(&f.text)?;
    Ok(f)
}

pub fn check_checked(c: Check) -> Result<Check, String> {
    if !CHECK_STATES.contains(&c.state.as_str()) {
        return Err(format!(
            "check state must be one of {}",
            CHECK_STATES.join(", ")
        ));
    }
    check_text(&c.text)?;
    Ok(c)
}

fn absolute(p: &str) -> String {
    let path = PathBuf::from(p);
    if path.is_absolute() {
        return p.to_string();
    }
    std::env::current_dir()
        .map(|d| d.join(path))
        .unwrap_or_else(|_| PathBuf::from(p))
        .display()
        .to_string()
}

impl Spec {
    pub fn into_card(self, reply_via: &str, source: Source) -> Result<CardIn, String> {
        let title = self.title.trim().to_string();
        if title.is_empty() || title.chars().count() > MAX_TITLE {
            return Err(format!("title must be 1 to {MAX_TITLE} characters"));
        }
        let kind = if self.kind.is_empty() {
            "decision".to_string()
        } else {
            self.kind
        };
        if !KINDS.contains(&kind.as_str()) {
            return Err(format!("kind must be one of {}", KINDS.join(", ")));
        }
        if self.context.as_ref().is_some_and(|c| c.len() > MAX_CONTEXT) {
            return Err(format!("context must be at most {MAX_CONTEXT} bytes"));
        }
        if self.options.len() > MAX_OPTIONS {
            return Err(format!("at most {MAX_OPTIONS} options"));
        }
        if self.facts.len() > MAX_ITEMS || self.checks.len() > MAX_ITEMS {
            return Err(format!("at most {MAX_ITEMS} facts and {MAX_ITEMS} checks"));
        }
        if let Some(n) = self.recommended {
            if n == 0 || n > self.options.len() {
                return Err("recommended must be the 1-based number of an option".into());
            }
        }
        let mut options = Vec::new();
        for (i, label) in self.options.iter().enumerate() {
            let label = label.trim();
            if label.is_empty() || label.chars().count() > MAX_TITLE {
                return Err(format!("option labels must be 1 to {MAX_TITLE} characters"));
            }
            options.push(Opt {
                id: (i + 1).to_string(),
                label: label.to_string(),
                recommended: self.recommended == Some(i + 1),
            });
        }
        if self.repo.is_none() && (self.base.is_some() || self.head.is_some()) {
            return Err("base and head need repo".into());
        }
        let mut repos: Vec<DiffSpec> = Vec::new();
        if let Some(repo) = self.repo {
            repos.push(DiffSpec {
                repo,
                base: self.base,
                head: self.head,
            });
        }
        repos.extend(self.diffs);
        if repos.len() > MAX_DIFFS {
            return Err(format!("at most {MAX_DIFFS} repositories"));
        }
        for d in &mut repos {
            if d.repo.is_empty() {
                return Err("repo must not be empty".into());
            }
            if d.head.is_some() && d.base.is_none() {
                return Err("head needs base".into());
            }
            d.repo = absolute(&d.repo);
        }
        let mut repos = repos.into_iter();
        let diff = repos.next();
        let diffs: Vec<DiffSpec> = repos.collect();
        if self.patches.len() > MAX_PATCHES {
            return Err(format!("at most {MAX_PATCHES} patches"));
        }
        let mut patch_bytes = 0;
        let mut patches = Vec::new();
        for p in self.patches {
            let label = p.label.trim();
            if label.chars().count() > MAX_TITLE {
                return Err(format!(
                    "patch labels must be at most {MAX_TITLE} characters"
                ));
            }
            if p.text.trim().is_empty() {
                return Err("patches must not be empty".into());
            }
            patch_bytes += p.text.len();
            patches.push(PatchIn {
                label: if label.is_empty() {
                    DEFAULT_PATCH_LABEL.into()
                } else {
                    label.into()
                },
                text: p.text,
            });
        }
        if patch_bytes > MAX_PATCH_BYTES {
            return Err(format!("patches may total at most {MAX_PATCH_BYTES} bytes"));
        }
        if let Some(t) = &self.task {
            refuse_foreign_scheme(t, "task")?;
        }
        for f in &self.facts {
            if let Some(l) = &f.link {
                refuse_foreign_scheme(l, "fact link")?;
            }
        }
        let mut items = Vec::new();
        let mut pr = None;
        for s in &self.prs {
            match parse_pr(s) {
                Ok(i) => {
                    pr.get_or_insert_with(|| i.url.clone());
                    items.push(i);
                }
                Err(_) => {
                    refuse_foreign_scheme(s, "pull request")?;
                    pr.get_or_insert_with(|| s.clone());
                }
            }
        }
        for s in &self.commits {
            items.push(parse_commit(s)?);
        }
        for s in &self.tags {
            items.push(parse_tag(s)?);
        }
        for s in &self.links {
            items.push(parse_link(s)?);
        }
        if let Some(url) = self.task.as_deref().and_then(|t| http_url(t).ok()) {
            items.push(item("task", url, None));
        }
        if items.len() > MAX_LINKS {
            return Err(format!("at most {MAX_LINKS} links"));
        }
        if let Some(id) = &self.replaces {
            if !decisions::valid_card_id(id) {
                return Err("replaces must be a card id".into());
            }
        }
        if let Some(k) = &self.key {
            if k.trim().is_empty() || k.chars().count() > MAX_KEY || k.chars().any(char::is_control)
            {
                return Err(format!("key must be 1 to {MAX_KEY} printable characters"));
            }
        }
        let card = CardIn {
            id: Some(decisions::new_card_id()),
            source,
            kind,
            blocking: self.blocking,
            title,
            context: self.context.filter(|c| !c.is_empty()),
            question: self.question.filter(|q| !q.is_empty()),
            facts: self.facts,
            options,
            diff,
            diffs,
            patches,
            replaces: self.replaces,
            key: self.key.map(|k| k.trim().to_string()),
            force_new: self.force_new,
            checks: self.checks,
            links: Links {
                task: self.task,
                pr,
                items,
            },
            reply_via: reply_via.to_string(),
            supersede: false,
        };
        let line = serde_json::to_vec(&card).map_or(0, |v| v.len());
        if line + 4096 > MAX_LINE {
            return Err(format!("the card is too large (limit {MAX_LINE} bytes)"));
        }
        Ok(card)
    }
}

#[derive(Subcommand)]
pub enum DecisionsCommand {
    /// Answers and dismissals you gave in the Decisions inbox, newest first
    Answers(AnswersArgs),
    /// Final answers no agent has picked up yet, oldest first
    PendingDelivery,
}

#[derive(Args)]
pub struct AnswersArgs {
    /// Only cards of this task (TM-123 or a task URL)
    #[arg(long, value_name = "TASK")]
    pub task: Option<String>,
    /// Only cards whose key starts with this
    #[arg(long, value_name = "PREFIX")]
    pub key: Option<String>,
    /// Only cards whose text contains every word of this
    #[arg(long, value_name = "TEXT")]
    pub query: Option<String>,
    /// How many cards to show (1 to 50)
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u32).range(1..=50))]
    pub limit: u32,
}

pub fn run(command: &DecisionsCommand) -> CoreResult<Output> {
    match command {
        DecisionsCommand::Answers(args) => answers(args),
        DecisionsCommand::PendingDelivery => pending_delivery(),
    }
}

fn answers(args: &AnswersArgs) -> CoreResult<Output> {
    let dir = decisions::decisions_dir()
        .map_err(|e| Error::internal(format!("cannot locate the Decisions inbox: {e}")))?;
    let found = Client::open_in(&dir)
        .and_then(|c| {
            c.find(
                args.task.as_deref(),
                args.key.as_deref(),
                args.query.as_deref(),
                Some(args.limit),
            )
        })
        .map_err(|e| match e.kind() {
            io::ErrorKind::Unsupported => {
                Error::usage("the Decisions inbox is too old for answers (update the desktop app)")
            }
            _ => Error::internal(format!("the Decisions inbox could not be reached: {e}")),
        })?;
    Ok(Output::new(
        json!({"answers": found}),
        answers_human(&found),
    ))
}

fn pending_delivery() -> CoreResult<Output> {
    let dir = decisions::decisions_dir()
        .map_err(|e| Error::internal(format!("cannot locate the Decisions inbox: {e}")))?;
    let pending = Client::open_in(&dir)
        .and_then(|c| c.pending_delivery())
        .map_err(|e| match e.kind() {
            io::ErrorKind::Unsupported => Error::usage("desktop too old, update TMGR desktop"),
            _ => Error::internal(format!("the Decisions inbox could not be reached: {e}")),
        })?;
    Ok(Output::new(
        json!({"pending": pending.iter().map(pending_json_row).collect::<Vec<_>>()}),
        pending_human(&pending),
    ))
}

fn pending_json_row(p: &PendingDelivery) -> Value {
    let mut v = serde_json::to_value(p).unwrap_or(Value::Null);
    if let Some(o) = v.as_object_mut() {
        o.entry("comment_skipped").or_insert(Value::Null);
    }
    v
}

fn pending_human(pending: &[PendingDelivery]) -> String {
    if pending.is_empty() {
        return "Every answer was picked up".into();
    }
    pending
        .iter()
        .map(|p| {
            let skipped = p.comment_skipped.as_deref().map_or_else(String::new, |r| {
                format!("  (skipped: {})", sanitize_line(r))
            });
            format!(
                "{}  {}  {}  {}  {}{}",
                created(p.answered_ms),
                p.card_id,
                p.delivery_state,
                p.task.as_deref().map_or_else(|| "-".into(), sanitize_line),
                sanitize_line(&p.title),
                skipped
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn created(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms).map_or_else(
        || "unknown".into(),
        |t| t.format("%Y-%m-%d %H:%M UTC").to_string(),
    )
}

fn answers_human(found: &[Found]) -> String {
    if found.is_empty() {
        return "No answers".into();
    }
    found
        .iter()
        .map(|f| {
            let mut lines = vec![sanitize_line(&f.title)];
            if let Some(q) = &f.question {
                lines.push(format!("  Question: {}", sanitize_line(q)));
            }
            let state = if f.dismissed { "dismissed" } else { &f.state };
            lines.push(format!("  State: {state}"));
            if let Some(c) = &f.choice_label {
                lines.push(format!("  Choice: {}", sanitize_line(c)));
            }
            if let Some(t) = &f.free_text {
                lines.push(format!("  Free text: {}", sanitize_line(t)));
            }
            if let Some(n) = &f.note {
                lines.push(format!("  Note: {}", sanitize_line(n)));
            }
            lines.push(format!("  Created: {}", created(f.created_ms)));
            lines.push(format!("  Card: {}", f.card_id));
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn side_note(side: Option<&str>) -> &'static str {
    if side == Some("old") {
        " (deleted)"
    } else {
        ""
    }
}

pub fn event_text(e: &Event) -> String {
    let mut lines = Vec::new();
    if e.kind == "message" {
        lines.push(format!("message: {}", e.text.as_deref().unwrap_or("")));
        if let Some(t) = &e.thread {
            lines.push(format!(
                "  on {}:{}{}",
                t.file,
                t.line,
                side_note(t.side.as_deref())
            ));
        }
        return lines.join("\n");
    }
    if e.dismissed {
        lines.push("dismissed".to_string());
    }
    if e.deferred {
        lines.push("deferred".to_string());
    }
    if let Some(label) = e.choice_label.as_deref().or(e.choice.as_deref()) {
        lines.push(format!("choice: {label}"));
    }
    if let Some(n) = &e.note {
        lines.push(format!("note: {n}"));
    }
    if let Some(t) = &e.free_text {
        lines.push(format!("answer: {t}"));
    }
    for c in &e.line_comments {
        lines.push(format!(
            "{}:{}{}: {}",
            c.file,
            c.line,
            side_note(c.side.as_deref()),
            c.text
        ));
    }
    if lines.is_empty() {
        lines.push("answered".to_string());
    }
    lines.join("\n")
}

fn project_name(cwd: &Path) -> String {
    cwd.file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("session")
        .to_string()
}

fn hook_source(cwd: &Path, session: Option<&str>) -> Source {
    Source {
        agent: "claude-code".into(),
        session: session.filter(|s| !s.is_empty()).map(str::to_string),
        cwd: Some(cwd.display().to_string()),
        title: None,
        pid: None,
        runner_session: runner_session(),
    }
}

fn hook_card(kind: &str, title: String, source: Source) -> CardIn {
    CardIn {
        id: Some(decisions::new_card_id()),
        source,
        kind: kind.into(),
        title: truncate(&title, MAX_TITLE),
        reply_via: "hook".into(),
        supersede: true,
        ..CardIn::default()
    }
}

pub fn stop_card(
    cwd: &Path,
    session: Option<&str>,
    message: Option<&str>,
    has_text: bool,
    git: bool,
) -> Option<CardIn> {
    if !has_text && !git {
        return None;
    }
    let mut card = hook_card(
        "stopped",
        format!("{} finished", project_name(cwd)),
        hook_source(cwd, session),
    );
    card.context = message
        .filter(|m| !m.trim().is_empty())
        .map(|m| truncate(m, HOOK_CONTEXT));
    if git {
        card.diff = Some(DiffSpec {
            repo: cwd.display().to_string(),
            base: Some("HEAD".into()),
            head: None,
        });
    }
    Some(card)
}

const STOCK_PHRASES: [&str; 3] = [
    "claude is waiting for your input",
    "claude needs your attention",
    "claude needs your input",
];

fn is_stock_text(message: &str) -> bool {
    let lower = message.trim().to_lowercase();
    let lower = lower.trim_end_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace());
    STOCK_PHRASES.contains(&lower) || lower.starts_with("claude needs your permission to use ")
}

pub fn notification_card(input: &Value, cwd: &Path, session: Option<&str>) -> Option<CardIn> {
    let (kind, headline) = match input.get("notification_type")?.as_str()? {
        "permission_prompt" => ("permission", "needs permission"),
        "agent_needs_input" | "elicitation_dialog" => ("question", "needs input"),
        _ => return None,
    };
    let message = input.get("message").and_then(Value::as_str);
    if message.is_none_or(|m| m.trim().is_empty() || is_stock_text(m)) {
        return None;
    }
    let mut card = hook_card(
        kind,
        format!("{}: {headline}", project_name(cwd)),
        hook_source(cwd, session),
    );
    card.supersede = kind != "permission";
    card.question = message
        .filter(|m| !m.trim().is_empty())
        .map(|m| truncate(m, MAX_TEXT));
    Some(card)
}

fn tool_summary(input: &Value) -> Option<String> {
    let tool_input = input.get("tool_input")?;
    for key in ["command", "file_path", "path", "url", "pattern", "query"] {
        if let Some(s) = tool_input.get(key).and_then(Value::as_str) {
            return Some(truncate(s, 500));
        }
    }
    Some(truncate(&tool_input.to_string(), 500))
}

pub fn permission_card(input: &Value, cwd: &Path, session: Option<&str>) -> CardIn {
    let tool = input
        .get("tool_name")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .unwrap_or("a tool");
    let mut card = hook_card(
        "permission",
        format!("{}: permission for {tool}", project_name(cwd)),
        hook_source(cwd, session),
    );
    card.question = tool_summary(input);
    card
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: std::collections::HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    const SOCK_ENV: (&str, &str) = ("CLAUDE_CODE_MESSAGING_SOCKET", "/tmp/claude.sock");
    const TOKEN_ENV: (&str, &str) = ("CLAUDE_CODE_MESSAGING_TOKEN", "tok");

    #[test]
    fn inbox_from_env_needs_absolute_socket_and_token() {
        assert!(claude_inbox_with(env_of(&[])).is_none());
        assert!(claude_inbox_with(env_of(&[SOCK_ENV])).is_none());
        assert!(claude_inbox_with(env_of(&[TOKEN_ENV])).is_none());
        assert!(claude_inbox_with(env_of(&[
            ("CLAUDE_CODE_MESSAGING_SOCKET", "rel/claude.sock"),
            TOKEN_ENV
        ]))
        .is_none());
        assert!(
            claude_inbox_with(env_of(&[SOCK_ENV, ("CLAUDE_CODE_MESSAGING_TOKEN", "")])).is_none()
        );
    }

    #[test]
    fn inbox_from_env_reads_socket_token_and_optional_session() {
        let i = claude_inbox_with(env_of(&[SOCK_ENV, TOKEN_ENV])).unwrap();
        assert_eq!(i.kind, "claude");
        assert_eq!(i.endpoint, "/tmp/claude.sock");
        assert_eq!(i.token.as_ref().map(Secret::expose), Some("tok"));
        assert_eq!(i.session, None);
        let i = claude_inbox_with(env_of(&[
            SOCK_ENV,
            TOKEN_ENV,
            ("CLAUDE_CODE_SESSION_ID", "s-1"),
        ]))
        .unwrap();
        assert_eq!(i.session.as_deref(), Some("s-1"));
    }

    #[test]
    fn inbox_is_skipped_inside_another_agent_started_from_claude() {
        assert!(claude_inbox_with(env_of(&[SOCK_ENV, TOKEN_ENV, ("OPENCODE", "1")])).is_none());
        assert!(claude_inbox_with(env_of(&[SOCK_ENV, TOKEN_ENV, ("GEMINI_CLI", "1")])).is_none());
    }

    use std::cell::RefCell;

    const PW: &str = "s3cret-pw";

    struct Fake {
        pid_ok: Result<bool, ()>,
        status: (u16, String),
        sessions: (u16, String),
        calls: RefCell<Vec<(String, Option<String>)>>,
        pid_calls: RefCell<u32>,
    }

    impl Fake {
        fn new(busy: &[&str], sessions: Value) -> Self {
            let status: serde_json::Map<String, Value> = busy
                .iter()
                .map(|id| (id.to_string(), json!({"type": "busy"})))
                .collect();
            Fake {
                pid_ok: Ok(true),
                status: (200, Value::Object(status).to_string()),
                sessions: (200, sessions.to_string()),
                calls: RefCell::default(),
                pid_calls: RefCell::default(),
            }
        }
    }

    impl OpencodeProbe for Fake {
        fn listener_pid(&self, _: u32, _: SocketAddr) -> Result<bool, ()> {
            *self.pid_calls.borrow_mut() += 1;
            self.pid_ok
        }

        fn get(
            &self,
            _: SocketAddr,
            path: &str,
            auth: Option<&str>,
        ) -> Result<(u16, Vec<u8>), HttpErr> {
            self.calls
                .borrow_mut()
                .push((path.to_string(), auth.map(String::from)));
            let r = if path.starts_with("/session/status") {
                &self.status
            } else {
                &self.sessions
            };
            Ok((r.0, r.1.clone().into_bytes()))
        }
    }

    fn here() -> String {
        std::fs::canonicalize(std::env::current_dir().unwrap())
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    fn sess(id: &str, parent: Option<&str>) -> Value {
        let mut v = json!({"id": id, "directory": here()});
        if let Some(p) = parent {
            v["parentID"] = json!(p);
        }
        v
    }

    const OC: [(&str, &str); 3] = [
        ("OPENCODE", "1"),
        ("TMGR_OPENCODE_URL", "http://127.0.0.1:4096"),
        ("OPENCODE_PID", "42"),
    ];

    fn oc(extra: &[(&str, &str)], fake: &Fake, meta: Option<&str>) -> Option<Inbox> {
        let mut all = OC.to_vec();
        all.extend_from_slice(extra);
        opencode_inbox_with(env_of(&all), fake, meta)
    }

    #[test]
    fn opencode_needs_opencode_env_and_pid() {
        let fake = Fake::new(&["ses_a"], json!([sess("ses_a", None)]));
        assert!(opencode_inbox_with(env_of(&OC[1..]), &fake, None).is_none());
        assert!(opencode_inbox_with(env_of(&[OC[0], OC[1]]), &fake, None).is_none());
        assert!(opencode_inbox_with(env_of(&[OC[0], OC[2]]), &fake, None).is_none());
        assert!(fake.calls.borrow().is_empty());
        let i = oc(&[], &fake, None).unwrap();
        assert_eq!(i.kind, "opencode");
        assert_eq!(i.endpoint, "http://127.0.0.1:4096");
        assert_eq!(i.session.as_deref(), Some("ses_a"));
        assert!(i.token.is_none());
    }

    #[test]
    fn bad_opencode_urls_never_reach_the_probe() {
        let fake = Fake::new(&["a"], json!([sess("a", None)]));
        for url in [
            "https://127.0.0.1:4096",
            "http://localhost:4096",
            "http://192.0.2.1:4096",
            "nonsense",
        ] {
            let env = [OC[0], OC[2], ("TMGR_OPENCODE_URL", url)];
            assert!(
                opencode_inbox_with(env_of(&env), &fake, None).is_none(),
                "{url}"
            );
        }
        assert_eq!(*fake.pid_calls.borrow(), 0);
        assert!(fake.calls.borrow().is_empty());
    }

    #[test]
    fn a_foreign_listener_pid_fails_closed() {
        for ok in [Ok(false), Err(())] {
            let mut fake = Fake::new(&["a"], json!([sess("a", None)]));
            fake.pid_ok = ok;
            assert!(oc(&[], &fake, None).is_none());
            assert!(fake.calls.borrow().is_empty());
        }
        let fake = Fake::new(&["a"], json!([sess("a", None)]));
        assert!(oc(&[("OPENCODE_PID", "x")], &fake, None).is_none());
    }

    #[test]
    fn opencode_needs_exactly_one_busy_root() {
        let none = Fake::new(&[], json!([sess("a", None)]));
        assert!(oc(&[], &none, None).is_none());
        let two = Fake::new(&["a", "b"], json!([sess("a", None), sess("b", None)]));
        assert!(oc(&[], &two, None).is_none());
        let elsewhere = Fake::new(
            &["a"],
            json!([{"id": "a", "directory": "/nonexistent/other"}]),
        );
        assert!(oc(&[], &elsewhere, None).is_none());
    }

    #[test]
    fn children_collapse_to_their_root() {
        let list = json!([
            sess("root", None),
            sess("c1", Some("root")),
            sess("c2", Some("c1"))
        ]);
        let fake = Fake::new(&["c1", "c2"], list.clone());
        assert_eq!(
            oc(&[], &fake, None).unwrap().session.as_deref(),
            Some("root")
        );
        let fake = Fake::new(&["c2"], list);
        assert_eq!(
            oc(&[], &fake, Some("c2")).unwrap().session.as_deref(),
            Some("c2")
        );
    }

    #[test]
    fn broken_parent_chains_are_dropped() {
        let cycle = Fake::new(&["a"], json!([sess("a", Some("b")), sess("b", Some("a"))]));
        assert!(oc(&[], &cycle, None).is_none());
        let orphan = Fake::new(&["a"], json!([sess("a", Some("gone"))]));
        assert!(oc(&[], &orphan, None).is_none());
    }

    #[test]
    fn meta_session_must_belong_to_a_busy_root() {
        let list = json!([sess("a", None), sess("b", None), sess("c", Some("b"))]);
        let fake = Fake::new(&["a", "b"], list.clone());
        assert_eq!(
            oc(&[], &fake, Some("a")).unwrap().session.as_deref(),
            Some("a")
        );
        assert_eq!(
            oc(&[], &fake, Some("c")).unwrap().session.as_deref(),
            Some("c")
        );
        assert!(oc(&[], &fake, Some("zzz")).is_none());
        let fake = Fake::new(&["a"], list);
        assert!(oc(&[], &fake, Some("b")).is_none());
    }

    #[test]
    fn session_ids_are_validated() {
        let fake = Fake::new(&["bad/id"], json!([sess("bad/id", None)]));
        assert!(oc(&[], &fake, None).is_none());
    }

    #[test]
    fn bad_responses_give_no_inbox() {
        let mut fake = Fake::new(&["a"], json!([sess("a", None)]));
        fake.status.0 = 401;
        assert!(oc(&[], &fake, None).is_none());
        let mut fake = Fake::new(&["a"], json!([sess("a", None)]));
        fake.sessions.0 = 500;
        assert!(oc(&[], &fake, None).is_none());
        let mut fake = Fake::new(&["a"], json!([sess("a", None)]));
        fake.sessions.1 = "<html>".into();
        assert!(oc(&[], &fake, None).is_none());
        let mut fake = Fake::new(&["a"], json!([sess("a", None)]));
        fake.status.1 = "[]".into();
        assert!(oc(&[], &fake, None).is_none());
    }

    #[test]
    fn the_password_only_lives_in_the_token() {
        let fake = Fake::new(&["a"], json!([sess("a", None)]));
        let i = oc(&[("OPENCODE_SERVER_PASSWORD", PW)], &fake, None).unwrap();
        assert_eq!(i.token.as_ref().unwrap().expose(), PW);
        assert!(!format!("{i:?}").contains(PW));
        assert!(!i.endpoint.contains(PW) && i.session.as_deref() != Some(PW));
        let calls = fake.calls.borrow();
        assert_eq!(calls.len(), 2);
        let want = loopback::basic_header(PW);
        for (path, auth) in calls.iter() {
            assert_eq!(auth.as_deref(), Some(want.as_str()), "{path}");
            assert!(path.contains("directory=") && !path.contains(PW));
        }
        assert!(calls[0].0.starts_with("/session/status?"));
        assert!(calls[1].0.starts_with("/session?"));
    }

    #[test]
    fn no_password_means_no_token_and_no_authorization() {
        for pw in [None, Some("")] {
            let fake = Fake::new(&["a"], json!([sess("a", None)]));
            let extra: Vec<(&str, &str)> = pw
                .map(|p| ("OPENCODE_SERVER_PASSWORD", p))
                .into_iter()
                .collect();
            let i = oc(&extra, &fake, None).unwrap();
            assert!(i.token.is_none());
            assert!(fake.calls.borrow().iter().all(|(_, a)| a.is_none()));
        }
    }

    #[test]
    fn the_real_probe_talks_to_a_local_server() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let dir = here();
        let server = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for _ in 0..2 {
                let (mut s, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap();
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                let body = if req.starts_with("GET /session/status") {
                    json!({"ses_x": {"type": "busy"}}).to_string()
                } else {
                    json!([{"id": "ses_x", "directory": dir}]).to_string()
                };
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                s.write_all(head.as_bytes()).unwrap();
                s.write_all(body.as_bytes()).unwrap();
                seen.push(req);
            }
            seen
        });

        struct Probe;
        impl OpencodeProbe for Probe {
            fn listener_pid(&self, _: u32, _: SocketAddr) -> Result<bool, ()> {
                Ok(true)
            }
            fn get(
                &self,
                addr: SocketAddr,
                path: &str,
                auth: Option<&str>,
            ) -> Result<(u16, Vec<u8>), HttpErr> {
                RealProbe.get(addr, path, auth)
            }
        }

        let url = format!("http://127.0.0.1:{port}");
        let env = [
            OC[0],
            OC[2],
            ("TMGR_OPENCODE_URL", url.as_str()),
            ("OPENCODE_SERVER_PASSWORD", PW),
        ];
        let i = opencode_inbox_with(env_of(&env), &Probe, None).unwrap();
        assert_eq!(i.endpoint, url);
        assert_eq!(i.session.as_deref(), Some("ses_x"));
        assert_eq!(i.token.unwrap().expose(), PW);
        let want = format!(
            "authorization: {}",
            loopback::basic_header(PW).to_lowercase()
        );
        for req in server.join().unwrap() {
            assert!(req.to_lowercase().contains(&want), "{req}");
        }
    }

    #[test]
    fn hook_inbox_only_for_stop_cards() {
        let inbox = || claude_inbox_with(env_of(&[SOCK_ENV, TOKEN_ENV]));
        let cwd = Path::new("/tmp/proj");
        let stop = stop_card(cwd, Some("s"), Some("done"), true, false).unwrap();
        assert!(hook_inbox_with(&stop, inbox).is_some());
        let permission = permission_card(&json!({"tool_name": "Bash"}), cwd, Some("s"));
        assert!(hook_inbox_with(&permission, inbox).is_none());
        for ty in [
            "permission_prompt",
            "elicitation_dialog",
            "agent_needs_input",
        ] {
            let n = notification_card(
                &json!({"notification_type": ty, "message": "Pick one"}),
                cwd,
                Some("s"),
            )
            .unwrap();
            assert!(hook_inbox_with(&n, inbox).is_none(), "{ty}");
        }
    }

    #[test]
    fn a_cancelled_wait_loop_returns_at_once() {
        let cancel = Cancel::default();
        cancel.cancel();
        let started = Instant::now();
        let w = wait_loop(
            "c_00000000000000000000000000000000",
            0,
            Duration::from_secs(600),
            false,
            Some(&cancel),
            &mut || {},
        );
        assert!(matches!(w, Waited::Cancelled));
        assert!(started.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn event_text_marks_only_deleted_lines() {
        let at = |side: Option<&str>| Event {
            kind: "message".into(),
            text: Some("t".into()),
            thread: Some(decisions::ThreadRef {
                file: "a.rs".into(),
                line: 3,
                side: side.map(Into::into),
            }),
            ..Event::default()
        };
        assert_eq!(
            event_text(&at(Some("old"))),
            "message: t\n  on a.rs:3 (deleted)"
        );
        assert_eq!(event_text(&at(Some("new"))), "message: t\n  on a.rs:3");
        assert_eq!(event_text(&at(None)), "message: t\n  on a.rs:3");
        let answer = Event {
            kind: "answer".into(),
            line_comments: vec![decisions::LineComment {
                file: "a.rs".into(),
                line: 3,
                side: Some("old".into()),
                text: "gone".into(),
            }],
            ..Event::default()
        };
        assert_eq!(event_text(&answer), "a.rs:3 (deleted): gone");
    }

    fn spec() -> Spec {
        Spec {
            title: "Ship it?".into(),
            options: vec!["Yes".into(), "No".into()],
            recommended: Some(1),
            ..Spec::default()
        }
    }

    #[test]
    fn agents_are_normalized() {
        assert_eq!(agent_name("claude-code"), "claude-code");
        assert_eq!(agent_name("Claude Desktop"), "claude-code");
        assert_eq!(agent_name("codex-mcp-client"), "codex");
        assert_eq!(agent_name("cursor"), "other");
        assert_eq!(agent_name(""), "other");
    }

    #[test]
    fn a_spec_becomes_a_card_with_numbered_options() {
        let card = spec().into_card("mcp", Source::default()).unwrap();
        assert_eq!(card.kind, "decision");
        assert!(decisions::valid_card_id(card.id.as_deref().unwrap()));
        assert_eq!(card.options[0].id, "1");
        assert!(card.options[0].recommended && !card.options[1].recommended);
        assert_eq!(card.reply_via, "mcp");
    }

    #[test]
    fn bad_specs_are_refused() {
        let mut s = spec();
        s.recommended = Some(3);
        assert!(s.into_card("mcp", Source::default()).is_err());
        let mut s = spec();
        s.kind = "other".into();
        assert!(s.into_card("mcp", Source::default()).is_err());
        let mut s = spec();
        s.title = " ".into();
        assert!(s.into_card("mcp", Source::default()).is_err());
        let mut s = spec();
        s.base = Some("main".into());
        assert!(s.into_card("mcp", Source::default()).is_err());
        let mut s = spec();
        s.options = (0..10).map(|i| i.to_string()).collect();
        s.recommended = None;
        assert!(s.into_card("mcp", Source::default()).is_err());
    }

    #[test]
    fn states_are_validated() {
        assert_eq!(parse_fact("ok:tests pass").unwrap().text, "tests pass");
        assert!(parse_fact("todo:x").is_err());
        assert!(parse_check("todo:review").is_ok());
        assert!(parse_check("wait:x").is_err());
        assert!(parse_fact("nocolon").is_err());
    }

    #[test]
    fn truncation_keeps_char_boundaries() {
        assert_eq!(truncate("héllo", 2), "h");
        assert_eq!(truncate("abc", 10), "abc");
    }

    #[test]
    fn stop_card_context_and_diff() {
        let card = stop_card(Path::new("/w/proj"), Some("sess"), Some("done"), true, true).unwrap();
        assert_eq!(card.kind, "stopped");
        assert_eq!(card.title, "proj finished");
        assert_eq!(card.context.as_deref(), Some("done"));
        assert_eq!(card.diff.as_ref().unwrap().base.as_deref(), Some("HEAD"));
        assert!(card.supersede && card.reply_via == "hook");
        assert_eq!(card.source.session.as_deref(), Some("sess"));
        let card = stop_card(Path::new("/w/proj"), None, None, true, false).unwrap();
        assert!(card.context.is_none() && card.diff.is_none());
        let big = "x".repeat(HOOK_CONTEXT * 2);
        let card = stop_card(Path::new("/w/proj"), None, Some(&big), true, false).unwrap();
        assert_eq!(card.context.unwrap().len(), HOOK_CONTEXT);
    }

    #[test]
    fn stop_without_text_and_changes_makes_no_card() {
        let p = Path::new("/w/proj");
        assert!(stop_card(p, None, None, false, false).is_none());
        assert!(stop_card(p, None, None, true, false).is_some());
        assert!(stop_card(p, None, None, false, true).is_some());
    }

    #[test]
    fn idle_prompt_makes_no_card() {
        let n = json!({"notification_type": "idle_prompt", "message": "Claude is waiting for your input"});
        assert!(notification_card(&n, Path::new("/w/p"), None).is_none());
        let n = json!({"notification_type": "idle_prompt", "message": "Which one?"});
        assert!(notification_card(&n, Path::new("/w/p"), None).is_none());
    }

    #[test]
    fn permission_notification_with_stock_text_makes_no_card() {
        for m in [
            "Claude needs your permission to use Bash",
            "Claude needs your permission to use Edit.",
            "Claude is waiting for your input",
            "  ",
            "",
        ] {
            let n = json!({"notification_type": "permission_prompt", "message": m});
            assert!(
                notification_card(&n, Path::new("/w/p"), None).is_none(),
                "{m}"
            );
        }
        let n = json!({"notification_type": "permission_prompt", "message": "Allow rm -rf build?"});
        assert_eq!(
            notification_card(&n, Path::new("/w/p"), None).unwrap().kind,
            "permission"
        );
    }

    #[test]
    fn question_notifications_need_real_text() {
        for t in ["agent_needs_input", "elicitation_dialog"] {
            for m in [
                json!(null),
                json!(""),
                json!("  "),
                json!("Claude is waiting for your input"),
                json!("claude is waiting for your input."),
                json!("Claude needs your permission to use Bash"),
                json!("Claude needs your input!"),
            ] {
                let n = json!({"notification_type": t, "message": m});
                assert!(
                    notification_card(&n, Path::new("/w/p"), None).is_none(),
                    "{t} {m}"
                );
            }
            let n = json!({"notification_type": t, "message": "Claude needs your input: Postgres or SQLite?"});
            assert!(notification_card(&n, Path::new("/w/p"), None).is_some());
            let n = json!({"notification_type": t, "message": "Pick a database"});
            let card = notification_card(&n, Path::new("/w/p"), None).unwrap();
            assert_eq!(card.question.as_deref(), Some("Pick a database"));
        }
    }

    #[test]
    fn notification_types_map_to_kinds() {
        let n = |t: &str| {
            notification_card(
                &json!({"notification_type": t, "message": "m"}),
                Path::new("/w/p"),
                None,
            )
        };
        assert_eq!(n("permission_prompt").unwrap().kind, "permission");
        assert!(n("idle_prompt").is_none());
        assert_eq!(n("agent_needs_input").unwrap().kind, "question");
        assert_eq!(n("elicitation_dialog").unwrap().kind, "question");
        assert!(n("auth_success").is_none());
        assert!(notification_card(&json!({"message": "m"}), Path::new("/w"), None).is_none());
    }

    #[test]
    fn permission_cards_summarize_the_tool_input() {
        let card = permission_card(
            &json!({"tool_name": "Bash", "tool_input": {"command": "rm x"}}),
            Path::new("/w/p"),
            None,
        );
        assert_eq!(card.kind, "permission");
        assert_eq!(card.title, "p: permission for Bash");
        assert_eq!(card.question.as_deref(), Some("rm x"));
    }

    #[test]
    fn force_new_is_passed_into_the_card_only_when_set() {
        assert_eq!(
            spec()
                .into_card("mcp", Source::default())
                .unwrap()
                .force_new,
            None
        );
        let card = Spec {
            force_new: Some(true),
            ..spec()
        }
        .into_card("mcp", Source::default())
        .unwrap();
        assert_eq!(card.force_new, Some(true));
    }

    #[test]
    fn answers_print_one_block_per_card() {
        let found = Found {
            card_id: "c_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            title: "Ship it?".into(),
            question: Some("Release 1.2?".into()),
            state: "answered".into(),
            choice_label: Some("Yes".into()),
            free_text: None,
            note: None,
            dismissed: false,
            key: None,
            task: Some("TM-1".into()),
            created_ms: 0,
        };
        let text = answers_human(&[found.clone(), found]);
        assert!(text.contains("Ship it?"));
        assert!(text.contains("Question: Release 1.2?"));
        assert!(text.contains("Choice: Yes"));
        assert!(text.contains("Created: 1970-01-01 00:00 UTC"));
        assert_eq!(text.matches("Ship it?").count(), 2);
        assert_eq!(answers_human(&[]), "No answers");
    }
}
