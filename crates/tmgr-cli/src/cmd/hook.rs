use crate::cmd::notifyenv::NotifyEnv;
use crate::ctx::Ctx;
use crate::gitx::run_capture;
use crate::runfinish;
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

/// The client of the run's own workspace: its local workspace, or the logged-in cloud.
fn build_client(env: &NotifyEnv, run: &runstate::Run) -> Option<Client> {
    if let Some(code) = run
        .workspace
        .as_deref()
        .and_then(tmgr_core::local::local_code)
    {
        return tmgr_core::local::client(code, env.cfg.local_user_id, &Ctx::user_agent(), false)
            .ok();
    }
    cloud_client(env)
}

fn cloud_client(env: &NotifyEnv) -> Option<Client> {
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
    let Some(run) = locate_run(&state, cwd, now, mode) else {
        debug("no run matches the current branch");
        return;
    };
    let Some(client) = build_client(env, &run) else {
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
                if let Some(e) = r
                    .runs
                    .iter_mut()
                    .find(|e| e.run_id == run.run_id && e.workspace == run.workspace)
                {
                    e.last_sent = now;
                    if let Some(c) = merged {
                        e.commits = c;
                    }
                }
            });
        }
        Err(e) if matches!(e.kind, Kind::NotFound | Kind::Conflict) => {
            let _ = runstate::modify(&path, STATE_WAIT, |r| {
                r.remove(run.run_id, run.workspace.as_deref())
            });
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RunTool {
    Start,
    Finish,
}

#[derive(Debug, Default, PartialEq)]
struct ToolRun {
    id: u64,
    task_id: Option<u64>,
    started_at: Option<String>,
    ended_at: Option<String>,
    session_id: Option<String>,
}

fn run_tool(tool_name: &str) -> Option<RunTool> {
    if !tool_name.starts_with("mcp__") {
        return None;
    }
    match tool_name.rsplit("__").next()? {
        "start_agent_work" => Some(RunTool::Start),
        "finish_agent_work" => Some(RunTool::Finish),
        _ => None,
    }
}

fn response_text(resp: &Value) -> Option<String> {
    match resp {
        Value::String(s) => Some(s.clone()),
        Value::Array(items) => content_text(items),
        Value::Object(o) => {
            if let Some(items) = o.get("content").and_then(Value::as_array) {
                content_text(items)
            } else {
                Some(resp.to_string())
            }
        }
        _ => None,
    }
}

fn content_text(items: &[Value]) -> Option<String> {
    let text: String = items
        .iter()
        .filter_map(|i| match i {
            Value::String(s) => Some(s.as_str()),
            _ => i.get("text").and_then(Value::as_str),
        })
        .collect::<Vec<_>>()
        .join("\n");
    (!text.is_empty()).then_some(text)
}

fn json_in(text: &str) -> Option<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(text) {
        return Some(v);
    }
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    serde_json::from_str(text.get(start..=end)?).ok()
}

fn pick<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|k| v.get(*k)).filter(|x| !x.is_null())
}

fn parse_tool_run(resp: &Value) -> Option<ToolRun> {
    if resp.get("isError").and_then(Value::as_bool) == Some(true)
        || resp.get("is_error").and_then(Value::as_bool) == Some(true)
    {
        return None;
    }
    let mut v = json_in(&response_text(resp)?)?;
    if let Some(inner) = v.get("data").filter(|d| d.is_object()) {
        v = inner.clone();
    }
    let text = |keys: &[&str]| pick(&v, keys).and_then(Value::as_str).map(str::to_string);
    let num = |keys: &[&str]| pick(&v, keys).and_then(Value::as_u64);
    Some(ToolRun {
        id: num(&["id", "run_id", "runId"])?,
        task_id: num(&["task_id", "taskId"]),
        started_at: text(&["started_at", "startedAt"]),
        ended_at: text(&["ended_at", "endedAt"]),
        session_id: text(&["session_id", "sessionId"]),
    })
}

fn run_sessions(input: &Value) -> Option<(String, Option<String>)> {
    let valid = |s: &str| !s.is_empty() && !s.chars().any(|c| c.is_whitespace() || c.is_control());
    let session = str_of(input, "session_id").filter(|s| valid(s))?;
    let (id, parent) = match str_of(input, "agent_id").filter(|s| !s.is_empty()) {
        Some(agent) if valid(agent) => (format!("{session}/{agent}"), Some(session.to_string())),
        Some(_) => return None,
        None => (session.to_string(), None),
    };
    (id.chars().count() <= agentwork::SESSION_ID_MAX).then_some((id, parent))
}

fn tool_client(env: &NotifyEnv, run_id: u64) -> Option<Client> {
    let state = runstate::load();
    match state.runs.iter().find(|r| r.run_id == run_id) {
        Some(run) => build_client(env, run),
        None => cloud_client(env),
    }
}

fn bind_session(client: &Client, run: &ToolRun, input: &Value) {
    if run.session_id.is_some() {
        return;
    }
    let Some((session_id, parent)) = run_sessions(input) else {
        return;
    };
    let progress = Progress {
        session_id: Some(session_id),
        parent_session_id: parent,
        ..Default::default()
    };
    if let Err(e) = agentwork::update(client, run.id, &progress, false) {
        debug(&format!("session binding failed: {}", e.message));
    }
}

fn upload_after_finish(task_id: Option<u64>, spawn: &dyn Fn(&[String])) {
    if let Some(id) = task_id {
        spawn(&runfinish::auto_upload_args(id));
    }
}

const START_TOLERANCE_S: i64 = 2;

fn same_start(tool: Option<&str>, server: Option<i64>) -> bool {
    match (tool.and_then(runfinish::parse_ts), server) {
        (Some(a), Some(b)) => (a - b).abs() <= START_TOLERANCE_S,
        _ => false,
    }
}

fn status_fits(kind: RunTool, status: &str) -> bool {
    match kind {
        RunTool::Start => status == "running",
        RunTool::Finish => status != "running" && status != "unknown",
    }
}

fn verified_run(
    client: &Client,
    run: &ToolRun,
    task_id: Option<u64>,
    kind: RunTool,
) -> Option<agentwork::RunView> {
    let task_id = task_id?;
    let view = match agentwork::get_run(client, task_id, run.id, false) {
        Ok(v) => v,
        Err(e) => {
            debug(&format!("run not verified: {}", e.message));
            return None;
        }
    };
    let fits = view.id == run.id
        && view.task_id == Some(task_id)
        && same_start(run.started_at.as_deref(), view.started_at)
        && status_fits(kind, &view.status);
    if !fits {
        debug("run on the server does not match the tool response");
    }
    fits.then_some(view)
}

fn post_git_commits(client: &Client, view: &agentwork::RunView, cwd: &Path) {
    let Some(started) = view.started_at else {
        return;
    };
    let ended = view.ended_at.unwrap_or_else(now_s);
    let git = |args: &[&str]| git_out(cwd, args);
    let commits = runfinish::collect(&git, view.branch.as_deref(), started, ended, &view.commits);
    if commits.is_empty() {
        return;
    }
    if let Err(e) = agentwork::add_git_commits(client, view.id, &commits) {
        debug(&format!("git commits not sent: {}", e.message));
    }
}

fn post_tool_use(env: &NotifyEnv, cwd: &Path, input: &Value) {
    let Some(kind) = str_of(input, "tool_name").and_then(run_tool) else {
        return;
    };
    let Some(run) = input.get("tool_response").and_then(parse_tool_run) else {
        debug("run response not understood");
        return;
    };
    let task_id = run.task_id.or_else(|| {
        runstate::load()
            .runs
            .iter()
            .find(|r| r.run_id == run.id)
            .map(|r| r.task_id)
    });
    if kind == RunTool::Finish {
        upload_after_finish(task_id, &runfinish::spawn_detached);
        if !env.cfg.hook_git_commits() {
            return;
        }
    }
    let Some(client) = tool_client(env, run.id) else {
        debug("no usable login");
        return;
    };
    let Some(view) = verified_run(&client, &run, task_id, kind) else {
        return;
    };
    match kind {
        RunTool::Start => bind_session(&client, &run, input),
        RunTool::Finish => post_git_commits(&client, &view, cwd),
    }
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
        "post-tool-use" => post_tool_use(&env, &cwd, &input),
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

    use serde_json::json;

    const RUN: &str = r#"{"id": 41, "task_id": 8974, "started_at": "2026-10-21T10:00:00Z", "ended_at": "2026-10-21T10:30:00Z", "commits": [{"sha": "abc1234", "message": "x"}]}"#;

    fn expected() -> ToolRun {
        ToolRun {
            id: 41,
            task_id: Some(8974),
            started_at: Some("2026-10-21T10:00:00Z".into()),
            ended_at: Some("2026-10-21T10:30:00Z".into()),
            session_id: None,
        }
    }

    #[test]
    fn the_run_is_read_from_every_known_response_shape() {
        let as_string = json!(RUN);
        let content = json!({"content": [{"type": "text", "text": RUN}]});
        let array = json!([{"type": "text", "text": RUN}]);
        let bare = serde_json::from_str::<Value>(RUN).unwrap();
        let wrapped = json!({"data": bare.clone()});
        let prefixed = json!(format!("Run started:\n{RUN}"));
        for (name, shape) in [
            ("string", as_string),
            ("content", content),
            ("array", array),
            ("object", bare),
            ("data", wrapped),
            ("prefixed", prefixed),
        ] {
            assert_eq!(parse_tool_run(&shape), Some(expected()), "{name}");
        }
    }

    #[test]
    fn camel_case_names_are_understood() {
        let r = parse_tool_run(
            &json!({"id": 3, "taskId": 7, "startedAt": "a", "endedAt": "b", "sessionId": "s"}),
        )
        .unwrap();
        assert_eq!(r.task_id, Some(7));
        assert_eq!(r.started_at.as_deref(), Some("a"));
        assert_eq!(r.ended_at.as_deref(), Some("b"));
        assert_eq!(r.session_id.as_deref(), Some("s"));
    }

    #[test]
    fn garbage_and_errors_give_nothing() {
        for bad in [
            json!("not json"),
            json!(42),
            json!(null),
            json!({"content": []}),
            json!({"content": [{"type": "text", "text": "Agent work is disabled"}]}),
            json!({"isError": true, "content": [{"type": "text", "text": RUN}]}),
            json!({"status": "running"}),
        ] {
            assert_eq!(parse_tool_run(&bad), None, "{bad}");
        }
    }

    #[test]
    fn only_the_two_run_tools_of_mcp_servers_count() {
        assert_eq!(
            run_tool("mcp__tmgr__start_agent_work"),
            Some(RunTool::Start)
        );
        assert_eq!(
            run_tool("mcp__plugin_tmgr_tmgr__finish_agent_work"),
            Some(RunTool::Finish)
        );
        assert_eq!(run_tool("mcp__tmgr__update_agent_work"), None);
        assert_eq!(run_tool("start_agent_work"), None);
        assert_eq!(run_tool("Bash"), None);
    }

    #[test]
    fn main_and_subagent_session_ids_follow_the_contract() {
        let main = json!({"session_id": "5f0c-uuid", "hook_event_name": "PostToolUse"});
        assert_eq!(run_sessions(&main), Some(("5f0c-uuid".into(), None)));
        let sub = json!({"session_id": "5f0c-uuid", "agent_id": "a1b2c3", "agent_type": "general"});
        assert_eq!(
            run_sessions(&sub),
            Some(("5f0c-uuid/a1b2c3".into(), Some("5f0c-uuid".into())))
        );
        assert_eq!(run_sessions(&json!({"agent_id": "a"})), None);
        assert_eq!(run_sessions(&json!({"session_id": ""})), None);
        let long = json!({"session_id": "s".repeat(192)});
        assert_eq!(run_sessions(&long), None);
        let joined = json!({"session_id": "s".repeat(100), "agent_id": "a".repeat(100)});
        assert_eq!(run_sessions(&joined), None);
        let exact = json!({"session_id": "s".repeat(191)});
        assert!(run_sessions(&exact).is_some());
    }

    #[test]
    fn the_auto_upload_is_spawned_with_the_task_id_only_when_known() {
        let calls = std::cell::RefCell::new(Vec::new());
        let spawn = |a: &[String]| calls.borrow_mut().push(a.to_vec());
        upload_after_finish(None, &spawn);
        assert!(calls.borrow().is_empty());
        upload_after_finish(Some(8974), &spawn);
        assert_eq!(
            calls.borrow().as_slice(),
            [vec!["usage", "sync", "--auto", "--task-id", "8974", "-q"]]
        );
    }
}
