use crate::cmd::notifyenv::NotifyEnv;
use crate::ctx::Ctx;
use crate::gitx::run_capture;
use serde_json::Value;
use std::io::{IsTerminal, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;
use tmgr_core::active::state_dir;
use tmgr_core::agentwork::{self, Outcome, Progress};
use tmgr_core::api::Client;
use tmgr_core::branch;
use tmgr_core::config::{ensure_private_dir, write_atomic};
use tmgr_core::error::Kind;
use tmgr_core::notify::{self, NotifyHttp, PushPayload};
use tmgr_core::runstate::{self, clean_commit, merge_commits, valid_sha};

const DEFAULT_BUDGET_MS: u64 = 1500;
const BUDGET_ENV: &str = "TMGR_HOOK_BUDGET_MS";
const MAX_STDIN: u64 = 256 * 1024;
const MAX_ARG_JSON: usize = 64 * 1024;
const TRANSCRIPT_TAIL: u64 = 256 * 1024;

// Testing only: lets slow CI machines run the hook with more time. Not documented for users.
fn budget() -> Duration {
    parse_budget(std::env::var(BUDGET_ENV).ok().as_deref())
}

fn parse_budget(raw: Option<&str>) -> Duration {
    let ms = raw
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map_or(DEFAULT_BUDGET_MS, |v| v.clamp(500, 10_000));
    Duration::from_millis(ms)
}

fn debug(msg: &str) {
    if std::env::var_os("TMGR_HOOK_DEBUG").is_some() {
        eprintln!("tmgr hook: {}", tmgr_core::text::sanitize_line(msg));
    }
}

fn git_program() -> &'static str {
    let no_path = std::env::var_os("PATH").is_none_or(|p| p.is_empty());
    if no_path && Path::new("/usr/bin/git").exists() {
        "/usr/bin/git"
    } else {
        "git"
    }
}

fn git_timeout() -> Duration {
    (budget() / 4).max(Duration::from_millis(400))
}

fn push_timeout() -> Duration {
    budget().saturating_sub(Duration::from_millis(300))
}
const STATE_WAIT: Duration = Duration::from_millis(300);
const HEARTBEAT_EVERY: i64 = 300;
const SESSION_MAX_AGE: Duration = Duration::from_secs(24 * 3600);

pub fn claude(event: &str, server_override: Option<String>) -> i32 {
    let event = event.to_string();
    guarded(move || claude_work(&event, server_override));
    0
}

pub fn codex(args: &[String], server_override: Option<String>) -> i32 {
    let raw = args.last().cloned();
    guarded(move || codex_work(raw, server_override));
    0
}

fn guarded(work: impl FnOnce() + Send + 'static) {
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new().spawn(move || {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
        let _ = tx.send(());
    });
    if spawned.is_ok() {
        let _ = rx.recv_timeout(budget());
    }
}

fn read_stdin() -> Option<Value> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return None;
    }
    let mut buf = Vec::new();
    stdin
        .lock()
        .take(MAX_STDIN + 1)
        .read_to_end(&mut buf)
        .ok()?;
    if buf.len() as u64 > MAX_STDIN {
        return None;
    }
    object(serde_json::from_slice(&buf).ok()?)
}

fn object(v: Value) -> Option<Value> {
    v.is_object().then_some(v)
}

fn now_s() -> i64 {
    chrono::Utc::now().timestamp()
}

fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn payload_cwd(input: &Value) -> Option<PathBuf> {
    let p = PathBuf::from(str_of(input, "cwd")?);
    (p.is_absolute() && p.is_dir()).then_some(p)
}

fn session_file(session_id: &str) -> Option<PathBuf> {
    let clean: String = session_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(100)
        .collect();
    if clean.is_empty() {
        return None;
    }
    Some(state_dir()?.join("sessions").join(format!("{clean}.json")))
}

fn record_prompt(session_id: &str, now_ms: i64) {
    let Some(path) = session_file(session_id) else {
        return;
    };
    let Some(dir) = path.parent() else { return };
    if ensure_private_dir(dir).is_err() {
        return;
    }
    cleanup_sessions(dir);
    let _ = write_atomic(
        &path,
        format!("{{\"started_ms\":{now_ms}}}").as_bytes(),
        0o600,
    );
}

fn consume_prompt(session_id: &str) -> Option<i64> {
    let path = session_file(session_id)?;
    let text = std::fs::read_to_string(&path).ok();
    let _ = std::fs::remove_file(&path);
    serde_json::from_str::<Value>(&text?)
        .ok()?
        .get("started_ms")?
        .as_i64()
}

fn cleanup_sessions(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > SESSION_MAX_AGE);
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

fn transcript_started_ms(path: &str) -> Option<i64> {
    let p = PathBuf::from(path);
    if !p.is_absolute() {
        return None;
    }
    let meta = std::fs::symlink_metadata(&p).ok()?;
    if !meta.is_file() {
        return None;
    }
    let mut f = std::fs::File::open(&p).ok()?;
    let start = meta.len().saturating_sub(TRANSCRIPT_TAIL);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::new();
    f.take(TRANSCRIPT_TAIL).read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    let text = if start > 0 {
        text.split_once('\n').map(|(_, rest)| rest).unwrap_or("")
    } else {
        &text
    };
    notify::last_user_prompt_ms(text)
}

fn send_push(env: &NotifyEnv, payload: &PushPayload) {
    let mut warnings = Vec::new();
    let Ok(Some(r)) = env.find_existing(&mut warnings) else {
        return;
    };
    let Ok(http) = NotifyHttp::new(&r.server, r.token.expose(), &Ctx::user_agent()) else {
        return;
    };
    let _ = http.push(payload, push_timeout());
}

fn git_out(cwd: &Path, args: &[&str]) -> Option<String> {
    let mut full = vec!["-c", "core.fsmonitor=false"];
    full.extend_from_slice(args);
    let out = match run_capture(git_program(), Some(cwd), &full, git_timeout()) {
        Ok(o) => o,
        Err(e) => {
            debug(&format!("git {args:?} could not run: {e}"));
            return None;
        }
    };
    let text = out.stdout.trim().to_string();
    if !out.ok || text.is_empty() {
        debug(&format!("git {args:?} ok={} output={text:?}", out.ok));
    }
    (out.ok && !text.is_empty()).then_some(text)
}

fn commits_since(cwd: &Path, start_sha: &str) -> Vec<String> {
    if !valid_sha(start_sha) {
        return Vec::new();
    }
    let range = format!("{start_sha}..HEAD");
    let args = ["log", "--format=%h %s", "-n", "200", range.as_str(), "--"];
    let Some(out) = git_out(cwd, &args) else {
        return Vec::new();
    };
    let mut lines: Vec<String> = out.lines().filter_map(clean_commit).collect();
    lines.reverse();
    lines
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Touch {
    Heartbeat,
    Commits,
}

fn build_client(env: &NotifyEnv) -> Option<Client> {
    let server = env.server().ok()?;
    let (tokens, _) = env.login_store(&server).load().ok().flatten()?;
    Client::new(&server, &Ctx::user_agent(), Some(tokens), None, false).ok()
}

fn locate_run(state: &runstate::Runs, cwd: &Path, now: i64, mode: Touch) -> Option<runstate::Run> {
    let out = git_out(
        cwd,
        &["rev-parse", "--show-toplevel", "--abbrev-ref", "HEAD"],
    )?;
    let mut lines = out.lines();
    let repo = lines.next()?.to_string();
    let branch_name = lines.next()?.to_string();
    if branch_name == "HEAD" {
        return None;
    }
    let key = branch::key_from_branch(&branch_name);
    let run = state
        .find_branch(Some(&repo), &branch_name, key.as_deref())
        .cloned()?;
    (mode == Touch::Commits || now - run.last_sent >= HEARTBEAT_EVERY).then_some(run)
}

fn touch_run(env: &NotifyEnv, cwd: &Path, mode: Touch) {
    let Some(path) = runstate::runs_path() else {
        return;
    };
    let state = runstate::load_from(&path);
    let now = now_s();
    if state.runs.is_empty()
        || (mode == Touch::Heartbeat
            && state
                .runs
                .iter()
                .all(|r| now - r.last_sent < HEARTBEAT_EVERY))
    {
        return;
    }
    let (client, located) = std::thread::scope(|sc| {
        let located = sc.spawn(|| locate_run(&state, cwd, now, mode));
        let client = build_client(env);
        (client, located.join().ok().flatten())
    });
    let Some(run) = located else {
        debug("no run matches the current branch");
        return;
    };
    let Some(client) = client else {
        debug("no usable login");
        return;
    };
    let mut progress = Progress::default();
    let mut merged = None;
    if mode == Touch::Commits {
        if let Some(sha) = run.start_sha.as_deref() {
            let fresh = commits_since(cwd, sha);
            if !fresh.is_empty() {
                let all = merge_commits(&run.commits, &fresh);
                progress.commits = Some(all.clone());
                merged = Some(all);
            }
        }
    }
    let result = agentwork::update(&client, run.run_id, &progress, false);
    if let Err(e) = &result {
        debug(&format!("run update failed: {}", e.message));
    }
    match result {
        Ok(Outcome::Run(_)) => {
            let _ = runstate::modify(&path, STATE_WAIT, |r| {
                if let Some(e) = r.runs.iter_mut().find(|e| e.run_id == run.run_id) {
                    e.last_sent = now;
                    if let Some(c) = merged {
                        e.commits = c;
                    }
                }
            });
        }
        Err(e) if matches!(e.kind, Kind::NotFound | Kind::Conflict) => {
            let _ = runstate::modify(&path, STATE_WAIT, |r| r.remove(run.run_id));
        }
        _ => {}
    }
}

fn push_and_touch(env: &NotifyEnv, payload: Option<PushPayload>, cwd: &Path, mode: Touch) {
    std::thread::scope(|s| {
        if let Some(p) = &payload {
            s.spawn(|| send_push(env, p));
        }
        s.spawn(|| touch_run(env, cwd, mode));
    });
}

fn claude_work(event: &str, server_override: Option<String>) {
    let Some(input) = read_stdin() else { return };
    let Ok(env) = NotifyEnv::load(server_override, false) else {
        return;
    };
    let cwd = payload_cwd(&input)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"));
    let session = str_of(&input, "session_id").unwrap_or("").to_string();
    let now_ms = chrono::Utc::now().timestamp_millis();
    match event {
        "user-prompt-submit" => {
            if !session.is_empty() {
                record_prompt(&session, now_ms);
            }
            touch_run(&env, &cwd, Touch::Heartbeat);
        }
        "notification" => {
            let payload = notify::notification_push(&input);
            push_and_touch(&env, payload, &cwd, Touch::Heartbeat);
        }
        "stop" => {
            if input.get("stop_hook_active").and_then(Value::as_bool) == Some(true) {
                return;
            }
            let started = if session.is_empty() {
                None
            } else {
                consume_prompt(&session)
            }
            .or_else(|| str_of(&input, "transcript_path").and_then(transcript_started_ms));
            let payload = notify::stop_push(&input, started, now_ms, env.stop_min_minutes());
            push_and_touch(&env, payload, &cwd, Touch::Commits);
        }
        "session-end" => {
            if !session.is_empty() {
                consume_prompt(&session);
            }
            touch_run(&env, &cwd, Touch::Heartbeat);
        }
        "session-start" | "subagent-stop" | "pre-tool-use" => {
            touch_run(&env, &cwd, Touch::Heartbeat);
        }
        _ => {}
    }
}

fn codex_work(raw: Option<String>, server_override: Option<String>) {
    let Some(raw) = raw.filter(|r| r.len() <= MAX_ARG_JSON) else {
        return;
    };
    let Some(input) = serde_json::from_str::<Value>(&raw).ok().and_then(object) else {
        return;
    };
    let Some(payload) = notify::codex_push(&input) else {
        return;
    };
    let Ok(env) = NotifyEnv::load(server_override, false) else {
        return;
    };
    let cwd = payload_cwd(&input)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"));
    push_and_touch(&env, Some(payload), &cwd, Touch::Heartbeat);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_testing_budget_is_clamped() {
        assert_eq!(parse_budget(None), Duration::from_millis(1500));
        assert_eq!(parse_budget(Some("x")), Duration::from_millis(1500));
        assert_eq!(parse_budget(Some("1")), Duration::from_millis(500));
        assert_eq!(parse_budget(Some("5000")), Duration::from_millis(5000));
        assert_eq!(parse_budget(Some("999999")), Duration::from_millis(10_000));
    }
}
