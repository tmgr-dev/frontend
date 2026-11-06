use serde_json::Value;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tmgr_ipc::decisions::{
    self, Cancel, CardIn, Check, Client, DiffSpec, Event, Fact, LinkItem, Links, Opt, PatchIn,
    Source, WaitCard, MAX_DIFFS, MAX_LINE, MAX_LINKS, MAX_PATCHES, MAX_PATCH_BYTES,
};

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

pub fn send_card(mut card: CardIn) -> io::Result<Sent> {
    card.id.get_or_insert_with(decisions::new_card_id);
    let dir = decisions::decisions_dir()?;
    match Client::open_in(&dir).and_then(|c| c.create_outcome(card.clone())) {
        Ok((canonical, outcome)) => Ok(Sent::Live(canonical, outcome)),
        Err(e) if is_refusal(&e) => Err(e),
        Err(_) => decisions::spool(&dir, &card).map(Sent::Spooled),
    }
}

pub enum Waited {
    Event(Box<Event>),
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
) -> io::Result<Option<Event>> {
    let end = Instant::now() + timeout;
    loop {
        for c in cards {
            let left = end.saturating_duration_since(Instant::now());
            let slice = left.min(SLICE);
            if let Some(e) =
                client.wait_cancellable(&c.card_id, c.since, slice, final_only, cancel)?
            {
                return Ok(Some(e));
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
) -> io::Result<Option<Event>> {
    if let [c] = cards {
        return client.wait_cancellable(&c.card_id, c.since, timeout, final_only, cancel);
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
        Ok(Some(e)) => Waited::Event(Box::new(e)),
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
}
