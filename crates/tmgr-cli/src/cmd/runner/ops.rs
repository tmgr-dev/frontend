use super::{attach, connect, emit, is_down, rerr, runner_config, Opts, RunArgs, RunnerCmd};
use super::{AttachArgs, HaltArgs, RmArgs, RunnerCommands, SendArgs};
use crate::out::table;
use serde_json::json;
use std::io::IsTerminal;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};
use tmgr_core::error::{Error, Result};
use tmgr_ipc::api::{CreateReq, RunCreateReq, SessionInfo, StateKind};
use tmgr_runner::client::Client;
use tmgr_runner::daemon::{self, RunnerConfig};
use tmgr_runner::exe;
use tmgr_runner::sanitize::sanitize_text;
use tmgr_runner::wire::now_ms;

pub fn dispatch(o: &Opts, cmd: &RunnerCommands) -> Result<i32> {
    match cmd {
        RunnerCommands::Runner { command } => match command {
            RunnerCmd::Start => start(o),
            RunnerCmd::Stop => stop(o),
            RunnerCmd::Status => status(o),
            RunnerCmd::Serve => serve(),
        },
        RunnerCommands::Run(a) => run(o, a),
        RunnerCommands::Agents => agents(o),
        RunnerCommands::Attach(a) => attach_cmd(a),
        RunnerCommands::Send(a) => send(o, a),
        RunnerCommands::Halt(a) => halt(o, a),
        RunnerCommands::Rm(a) => rm(o, a),
    }
}

fn clean(s: &str) -> String {
    sanitize_text(s).into_owned()
}

fn serve() -> Result<i32> {
    daemon::serve(runner_config()?).map_err(|e| Error::internal(format!("runner: {e}")))?;
    Ok(0)
}

fn start(o: &Opts) -> Result<i32> {
    let cfg = runner_config()?;
    let pid = daemon::start_background(&cfg).map_err(rerr)?;
    Ok(emit(
        o,
        json!({"pid": pid}),
        format!("runner is up (pid {pid})"),
    ))
}

fn stop(o: &Opts) -> Result<i32> {
    let cfg = runner_config()?;
    let mut c = match Client::connect(&cfg.dir) {
        Ok(c) => c,
        Err(e) if is_down(&e) => {
            return Ok(emit(o, json!({"stopped": false}), "runner is not running"))
        }
        Err(e) => return Err(rerr(e)),
    };
    c.stop().map_err(rerr)?;
    Ok(emit(o, json!({"stopped": true}), "runner stopped"))
}

fn status(o: &Opts) -> Result<i32> {
    let cfg = runner_config()?;
    let st = connect(&cfg)?.status().map_err(rerr)?;
    let human = format!(
        "runner is up (pid {}, version {}, up {}, {} session{})",
        st.pid,
        clean(&st.version),
        span(st.uptime_s),
        st.sessions,
        if st.sessions == 1 { "" } else { "s" }
    );
    Ok(emit(o, st, human))
}

fn resolve_exe(name: &str) -> Result<String> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let base = std::env::current_dir().ok();
    exe::resolve_exe(name, &path, base.as_deref())
        .map(|p| p.display().to_string())
        .map_err(Error::not_found)
}

fn terminal_size() -> (u16, u16) {
    if std::io::stdout().is_terminal() {
        if let Ok(size) = ratatui::crossterm::terminal::size() {
            return size;
        }
    }
    (120, 40)
}

fn run(o: &Opts, a: &RunArgs) -> Result<i32> {
    let cwd = match &a.cwd {
        Some(p) => std::path::absolute(p),
        None => std::env::current_dir(),
    }
    .map_err(|e| Error::usage(format!("bad --cwd: {e}")))?;
    let cwd_str = cwd.display().to_string();
    let (cols, rows) = terminal_size();
    let raw = match &a.agent_cmd {
        Some(line) => {
            let mut argv = shell_words::split(line)
                .map_err(|e| Error::usage(format!("cannot parse --agent-cmd: {e}")))?;
            if argv.is_empty() {
                return Err(Error::usage("--agent-cmd is empty"));
            }
            argv[0] = resolve_exe(&argv[0])?;
            Some(argv)
        }
        None => None,
    };
    if !o.json && !o.quiet {
        match &raw {
            Some(argv) => eprintln!(
                "starting {} in {}",
                clean(&argv[0]),
                clean(&cwd.display().to_string())
            ),
            None => eprintln!(
                "starting agent {} in {}",
                clean(a.agent.as_deref().unwrap_or_default()),
                clean(&cwd.display().to_string())
            ),
        }
    }
    let cfg = runner_config()?;
    daemon::start_background(&cfg).map_err(rerr)?;
    let mut client = connect(&cfg)?;
    let info = match raw {
        Some(argv) => client.create(CreateReq {
            argv,
            cwd: cwd_str,
            name: a.name.clone(),
            profile: None,
            cols,
            rows,
            env_pass: Vec::new(),
            task: None,
        }),
        None => client.create_run(RunCreateReq {
            profile: a.agent.clone().unwrap_or_default(),
            task: None,
            cwd: cwd_str,
            name: a.name.clone(),
            model: a.model.clone(),
            cols,
            rows,
        }),
    }
    .map_err(rerr)?;
    if !a.no_attach && !o.json && std::io::stdout().is_terminal() {
        return attach::run(&cfg, &info.id);
    }
    let human = format!(
        "started {} ({}); attach with: tmgr attach {}",
        clean(&info.name),
        clean(&info.id),
        clean(&info.id)
    );
    Ok(emit(o, &info, human))
}

fn list_or_empty(cfg: &RunnerConfig) -> Result<Vec<SessionInfo>> {
    match Client::connect(&cfg.dir) {
        Ok(mut c) => c.list().map_err(rerr),
        Err(e) if is_down(&e) => Ok(Vec::new()),
        Err(e) => Err(rerr(e)),
    }
}

pub fn span(secs: u64) -> String {
    let (d, h, m) = (secs / 86400, secs % 86400 / 3600, secs % 3600 / 60);
    match (d, h, m) {
        (0, 0, 0) => format!("{secs}s"),
        (0, 0, m) => format!("{m}m"),
        (0, h, m) => format!("{h}h {m:02}m"),
        (d, h, _) => format!("{d}d {h:02}h"),
    }
}

fn state_label(s: &SessionInfo) -> &'static str {
    match s.state.kind {
        StateKind::Starting => "starting",
        StateKind::Working => "working",
        StateKind::Waiting => "waiting",
        StateKind::Blocked => "blocked",
        StateKind::Exited if s.state.exit_code == Some(0) => "exited",
        StateKind::Exited => "failed",
    }
}

fn badge(s: &SessionInfo) -> char {
    match s.state.kind {
        StateKind::Working => '●',
        StateKind::Waiting => '◌',
        StateKind::Blocked => '◆',
        StateKind::Exited if s.state.exit_code == Some(0) => '✓',
        StateKind::Exited => '✗',
        StateKind::Starting => '○',
    }
}

fn paint_badge(o: &Opts, s: &SessionInfo) -> String {
    let text = badge(s).to_string();
    match s.state.kind {
        StateKind::Working => o.style.accent(&text),
        StateKind::Blocked => o.style.bold(&o.style.accent(&text)),
        StateKind::Exited if s.state.exit_code == Some(0) => o.style.good(&text),
        _ => o.style.muted(&text),
    }
}

fn short_cwd(cwd: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return cwd.to_string();
    }
    if cwd == home {
        return "~".into();
    }
    cwd.strip_prefix(&format!("{home}/"))
        .map_or_else(|| cwd.to_string(), |rest| format!("~/{rest}"))
}

fn command_line(s: &SessionInfo) -> String {
    let base = s.argv.first().map_or("", |a| {
        Path::new(a)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(a)
    });
    let full = std::iter::once(base)
        .chain(s.argv.iter().skip(1).map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    let (cut, more) = tmgr_core::text::truncate_chars(&full, 40);
    tmgr_core::text::sanitize_line(&if more { format!("{cut}…") } else { cut })
}

fn agents(o: &Opts) -> Result<i32> {
    let cfg = runner_config()?;
    let sessions = list_or_empty(&cfg)?;
    let count = |k: &[StateKind]| {
        sessions
            .iter()
            .filter(|s| k.contains(&s.state.kind))
            .count()
    };
    let summary = json!({
        "working": count(&[StateKind::Working, StateKind::Starting]),
        "waiting": count(&[StateKind::Waiting]),
        "blocked": count(&[StateKind::Blocked]),
        "exited": count(&[StateKind::Exited]),
    });
    if o.json {
        return Ok(emit(
            o,
            json!({"sessions": sessions, "summary": summary}),
            "",
        ));
    }
    if sessions.is_empty() {
        println!("No agent sessions yet. Start one with: tmgr run --agent <profile>");
        return Ok(0);
    }
    let now = now_ms();
    let mut rows = vec![["", "NAME", "STATE", "ID", "COMMAND", "CWD"]
        .map(String::from)
        .to_vec()];
    for s in &sessions {
        let since = span(now.saturating_sub(s.state.since_ms) / 1000);
        rows.push(vec![
            badge(s).to_string(),
            tmgr_core::text::sanitize_line(&s.name),
            format!("{} {since}", state_label(s)),
            clean(&s.id),
            command_line(s),
            tmgr_core::text::sanitize_line(&short_cwd(&s.cwd)),
        ]);
    }
    for (i, line) in table(&rows).lines().enumerate() {
        match i.checked_sub(1).map(|k| &sessions[k]) {
            Some(s) => {
                let rest = line.strip_prefix(badge(s)).unwrap_or(line);
                println!("{}{rest}", paint_badge(o, s));
            }
            None => println!("{}", o.style.muted(line)),
        }
    }
    println!();
    println!(
        "{} session{} · {} working · {} waiting · {} blocked · {} exited",
        sessions.len(),
        if sessions.len() == 1 { "" } else { "s" },
        summary["working"],
        summary["waiting"],
        summary["blocked"],
        summary["exited"]
    );
    Ok(0)
}

fn newest(sessions: &[SessionInfo], f: impl Fn(&SessionInfo) -> bool) -> Option<&SessionInfo> {
    sessions
        .iter()
        .filter(|s| f(s))
        .max_by_key(|s| (s.created_ms, &s.id))
}

fn resolve<'a>(sessions: &'a [SessionInfo], key: &str) -> Result<&'a SessionInfo> {
    newest(sessions, |s| s.id == key)
        .or_else(|| {
            newest(sessions, |s| {
                s.name == key && s.state.kind != StateKind::Exited
            })
        })
        .or_else(|| newest(sessions, |s| s.name == key))
        .or_else(|| {
            let mut hits = sessions.iter().filter(|s| s.id.starts_with(key));
            hits.next().filter(|_| hits.next().is_none())
        })
        .ok_or_else(|| Error::not_found(format!("no session matches {key:?}")).hint("tmgr agents"))
}

fn attach_cmd(a: &AttachArgs) -> Result<i32> {
    let cfg = runner_config()?;
    let sessions = connect(&cfg)?.list().map_err(rerr)?;
    let target = match &a.session {
        Some(key) => resolve(&sessions, key)?,
        None => newest(&sessions, |s| s.state.kind != StateKind::Exited)
            .or_else(|| newest(&sessions, |_| true))
            .ok_or_else(|| Error::not_found("no sessions to attach to").hint("tmgr run"))?,
    };
    attach::run(&cfg, &target.id)
}

fn with_session(key: &str) -> Result<(Client, SessionInfo)> {
    let cfg = runner_config()?;
    let mut c = connect(&cfg)?;
    let sessions = c.list().map_err(rerr)?;
    let s = resolve(&sessions, key)?.clone();
    Ok((c, s))
}

fn send(o: &Opts, a: &SendArgs) -> Result<i32> {
    let (mut c, s) = with_session(&a.session)?;
    let mut text = a.text.clone();
    if !a.no_enter {
        text.push('\r');
    }
    c.send(&s.id, &text).map_err(rerr)?;
    Ok(emit(
        o,
        json!({"id": s.id}),
        format!("sent to {}", clean(&s.name)),
    ))
}

fn wait_exit(c: &mut Client, id: &str, limit: Duration) -> Result<bool> {
    let end = Instant::now() + limit;
    loop {
        let list = c.list().map_err(rerr)?;
        if list
            .iter()
            .find(|s| s.id == id)
            .is_none_or(|s| s.state.kind == StateKind::Exited)
        {
            return Ok(true);
        }
        if Instant::now() >= end {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn halt(o: &Opts, a: &HaltArgs) -> Result<i32> {
    let (mut c, s) = with_session(&a.session)?;
    if s.state.kind == StateKind::Exited {
        return Ok(emit(
            o,
            json!({"id": s.id, "signal": null, "exited": true}),
            format!("{} has already exited", clean(&s.name)),
        ));
    }
    let mut signal = if a.kill { "kill" } else { "int" };
    c.kill(&s.id, signal).map_err(rerr)?;
    let mut exited = wait_exit(&mut c, &s.id, Duration::from_secs(10))?;
    if !exited && !a.kill {
        signal = "term";
        c.kill(&s.id, signal).map_err(rerr)?;
        exited = wait_exit(&mut c, &s.id, Duration::from_secs(5))?;
    }
    if !exited {
        return Err(
            Error::conflict(format!("{} is still running", clean(&s.name)))
                .hint("tmgr halt --kill"),
        );
    }
    Ok(emit(
        o,
        json!({"id": s.id, "signal": signal, "exited": true}),
        format!("{} stopped", clean(&s.name)),
    ))
}

fn rm(o: &Opts, a: &RmArgs) -> Result<i32> {
    let (mut c, s) = with_session(&a.session)?;
    c.remove(&s.id).map_err(rerr)?;
    Ok(emit(
        o,
        json!({"id": s.id}),
        format!("removed {}", clean(&s.name)),
    ))
}
