use super::{attach, connect, emit, is_down, rerr, runner_config, Opts, RunArgs, RunnerCmd};
use super::{
    remote, AgentsArgs, AgentsCmd, AttachArgs, AutoMode, HaltArgs, RmArgs, RunnerCommands, SendArgs,
};
use crate::out::table;
use serde_json::json;
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{CandidateInfo, CreateReq, RunCreateReq, SessionInfo, StateKind};
use tmgr_runner::client::Client;
use tmgr_runner::daemon::{self, RunnerConfig};
use tmgr_runner::exe;
use tmgr_runner::login;
use tmgr_runner::profile::{self, ProfileSet};
use tmgr_runner::sanitize::sanitize_text;
use tmgr_runner::seedguard;
use tmgr_runner::wire::now_ms;

pub fn dispatch(o: &Opts, cmd: &RunnerCommands) -> Result<i32> {
    match cmd {
        RunnerCommands::Runner { command } => match command {
            RunnerCmd::Start => start(o),
            RunnerCmd::Stop => stop(o),
            RunnerCmd::Status => status(o),
            RunnerCmd::Serve => serve(),
        },
        RunnerCommands::Run(a) => match &a.on {
            Some(link) => remote::run(o, link, a),
            None => run(o, a),
        },
        RunnerCommands::Agents(AgentsArgs {
            command: Some(AgentsCmd::Login { profile, replace }),
            ..
        }) => agent_login(o, profile, *replace),
        RunnerCommands::Agents(AgentsArgs {
            command: Some(AgentsCmd::Logout { profile }),
            ..
        }) => agent_logout(o, profile),
        RunnerCommands::Agents(AgentsArgs {
            command:
                Some(AgentsCmd::Sync {
                    profile,
                    discard,
                    auto,
                }),
            ..
        }) => agent_sync(o, profile, *discard, *auto),
        RunnerCommands::Agents(AgentsArgs {
            command: Some(AgentsCmd::Log { session }),
            ..
        }) => work_log(o, session),
        RunnerCommands::Agents(a) => match &a.host {
            Some(link) => remote::agents(o, link),
            None => agents(o),
        },
        RunnerCommands::Attach(a) => match &a.on {
            Some(link) => remote::attach(link, a),
            None => attach_cmd(a),
        },
        RunnerCommands::Send(a) => send(o, a),
        RunnerCommands::Halt(a) => match &a.on {
            Some(link) => remote::halt(o, link, a),
            None => halt(o, a),
        },
        RunnerCommands::Rm(a) => rm(o, a),
    }
}

fn clean(s: &str) -> String {
    sanitize_text(s).into_owned()
}

fn serve() -> Result<i32> {
    let cfg = runner_config()?;
    if std::env::var_os(daemon::ENV_CLEAN).is_none() {
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(&cfg.self_exe)
            .args(std::env::args_os().skip(1))
            .env_clear()
            .envs(daemon::daemon_env())
            .env(daemon::ENV_CLEAN, "1")
            .exec();
        return Err(Error::internal(format!("runner: {err}")));
    }
    daemon::serve(cfg).map_err(|e| Error::internal(format!("runner: {e}")))?;
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
    if !o.json && !o.quiet {
        for line in candidate_lines(&cfg) {
            eprintln!("{line}");
        }
    }
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
            bundle: None,
            bundle_sha256: None,
            cwd_pin: None,
            origin: None,
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

fn credential_target(profile: &str) -> Result<login::ResolvedSecretFile> {
    let (set, _) = ProfileSet::load(profile::user_dir().as_deref());
    let p = set.get(profile).ok_or_else(|| {
        Error::not_found(format!("no agent profile {profile:?}")).hint("tmgr agents")
    })?;
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| Error::usage("HOME is not set"))?;
    let mut files = login::resolve(p, &home, &[])
        .map_err(Error::usage)?
        .secret_files;
    match (files.pop(), files.is_empty()) {
        (Some(f), true) => Ok(f),
        _ => Err(Error::usage(format!(
            "profile {profile} must declare exactly one [[secrets.files]] entry to log in"
        ))),
    }
}

fn read_token() -> Result<String> {
    let line = if std::io::stdin().is_terminal() {
        rpassword::prompt_password("Token (input hidden): ")
            .map_err(|e| Error::usage(format!("cannot read the token: {e}")))?
    } else {
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|e| Error::usage(format!("cannot read the token: {e}")))?;
        line
    };
    Ok(line.trim().to_string())
}

fn agent_login(o: &Opts, profile: &str, replace: bool) -> Result<i32> {
    let target = credential_target(profile)?;
    let token = read_token()?;
    let bytes = login::render_credential(&target, &token, now_ms()).map_err(Error::usage)?;
    login::write_credential(&target, &bytes, replace).map_err(Error::usage)?;
    let path = target.from.display().to_string();
    Ok(emit(
        o,
        json!({"path": path}),
        format!("Wrote {path} (mode 600).\nNext session will be logged in."),
    ))
}

fn agent_logout(o: &Opts, profile: &str) -> Result<i32> {
    let target = credential_target(profile)?;
    let removed = login::remove_credential(&target).map_err(Error::usage)?;
    let path = target.from.display().to_string();
    let human = if removed {
        format!("Removed {path}.")
    } else {
        format!("{path} does not exist; nothing to remove.")
    };
    Ok(emit(o, json!({"path": path, "removed": removed}), human))
}

fn pending_candidates(cfg: &RunnerConfig) -> Vec<CandidateInfo> {
    Client::connect(&cfg.dir)
        .and_then(|mut c| c.candidates())
        .unwrap_or_else(|_| seedguard::pending(&cfg.dir))
}

fn candidate_lines(cfg: &RunnerConfig) -> Vec<String> {
    pending_candidates(cfg)
        .iter()
        .map(|c| {
            let profile = clean(&c.profile);
            format!(
                "{profile} refreshed its login inside a session; run `tmgr agents sync {profile}` to apply it to {}",
                tmgr_core::text::sanitize_line(&short_cwd(&c.source))
            )
        })
        .collect()
}

fn in_session() -> bool {
    std::env::var_os("TMGR_SESSION_ID").is_some()
}

fn need_terminal(what: &str) -> Result<()> {
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        return Ok(());
    }
    Err(
        Error::needs_confirmation(format!("{what} needs an interactive terminal"))
            .hint("run it in a terminal: this confirmation never accepts --yes"),
    )
}

fn ask(prompt: &str) -> Result<String> {
    print!("{prompt}");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|_| Error::new(Kind::Interrupted, "could not read the answer"))?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

fn cancelled() -> Error {
    Error::new(Kind::Interrupted, "cancelled, nothing was changed")
}

fn agent_sync(o: &Opts, profile: &str, discard: bool, auto: Option<AutoMode>) -> Result<i32> {
    if !profile::valid_profile_id(profile) {
        return Err(Error::usage(format!("bad profile id {profile:?}")));
    }
    if discard {
        let n = seedguard::discard(&runner_config()?.dir, profile)
            .map_err(|e| Error::internal(format!("cannot discard: {e}")))?;
        return Ok(emit(
            o,
            json!({"profile": profile, "discarded": n}),
            format!("Discarded {n} pending login(s) for {profile}."),
        ));
    }
    if in_session() {
        return Err(Error::new(
            Kind::Forbidden,
            "this command cannot run inside an agent session",
        ));
    }
    if let Some(AutoMode::Off) = auto {
        seedguard::set_autosync(profile, false)
            .map_err(|e| Error::internal(format!("cannot turn auto mode off: {e}")))?;
        return Ok(emit(
            o,
            json!({"profile": profile, "auto": false}),
            format!("Auto mode for {profile} is off."),
        ));
    }
    if o.json {
        return Err(Error::usage(
            "this command is interactive; --json is not supported",
        ));
    }
    match auto {
        Some(_) => agent_auto_on(profile),
        None => agent_apply(profile),
    }
}

fn agent_auto_on(profile: &str) -> Result<i32> {
    need_terminal("turning on auto mode")?;
    let (set, _) = ProfileSet::load(profile::user_dir().as_deref());
    if set.get(profile).is_none() {
        return Err(Error::not_found(format!("no agent profile {profile:?}")).hint("tmgr agents"));
    }
    println!(
        "Risk: an agent in a session can replace your {profile} login with another account's tokens.\n\
         TMGR cannot verify them offline. With auto mode your normal {profile} would then use that account."
    );
    if ask("Type I AGREE to turn on auto mode: ")? != "I AGREE" {
        return Err(cancelled());
    }
    seedguard::set_autosync(profile, true)
        .map_err(|e| Error::internal(format!("cannot turn auto mode on: {e}")))?;
    println!(
        "Auto mode for {profile} is on. Turn it off with: tmgr agents sync {profile} --auto off"
    );
    Ok(0)
}

fn agent_apply(profile: &str) -> Result<i32> {
    need_terminal("applying a refreshed login")?;
    let cfg = runner_config()?;
    let list = seedguard::entries(&cfg.dir, profile);
    if list.is_empty() {
        println!("No pending refreshed login for {profile}.");
        return Ok(0);
    }
    for (index, _) in list {
        println!("Refreshed {profile} login captured inside a session:");
        let mut reviewed = String::new();
        match seedguard::summary(&cfg.dir, profile, index, now_ms()) {
            Ok((lines, sha)) => {
                lines.iter().for_each(|l| println!("  {l}"));
                reviewed = sha;
            }
            Err(e) => println!("  the candidate cannot be read: {e}"),
        }
        if ask("Type apply to replace your file with it: ")? != "apply" {
            return Err(cancelled());
        }
        let done = seedguard::apply(&cfg.dir, profile, index, &reviewed)
            .map_err(|e| Error::new(Kind::Conflict, e))?;
        println!("{done}");
    }
    Ok(0)
}

fn agents(o: &Opts) -> Result<i32> {
    let cfg = runner_config()?;
    let sessions = list_or_empty(&cfg)?;
    let origins = tmgr_link::store::Paths::from_env()
        .map(|p| tmgr_link::ownership::origin_labels(&p))
        .unwrap_or_default();
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
        let tagged: Vec<serde_json::Value> = sessions
            .iter()
            .map(|s| {
                let mut v = serde_json::to_value(s).unwrap_or_default();
                if let (Some(origin), Some(m)) = (origins.get(&s.id), v.as_object_mut()) {
                    m.insert("origin".into(), json!(origin));
                }
                v
            })
            .collect();
        return Ok(emit(o, json!({"sessions": tagged, "summary": summary}), ""));
    }
    let show_origin = sessions.iter().any(|s| origins.contains_key(&s.id));
    if sessions.is_empty() {
        println!("No agent sessions yet. Start one with: tmgr run --agent <profile>");
        for line in candidate_lines(&cfg) {
            println!("{line}");
        }
        return Ok(0);
    }
    let now = now_ms();
    let mut head = vec!["", "NAME", "STATE", "ID", "COMMAND", "CWD"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    if show_origin {
        head.push("ORIGIN".into());
    }
    let mut rows = vec![head];
    for s in &sessions {
        let since = span(now.saturating_sub(s.state.since_ms) / 1000);
        let mut row = vec![
            badge(s).to_string(),
            tmgr_core::text::sanitize_line(&s.name),
            format!("{} {since}", state_label(s)),
            clean(&s.id),
            command_line(s),
            tmgr_core::text::sanitize_line(&short_cwd(&s.cwd)),
        ];
        if show_origin {
            row.push(origins.get(&s.id).map_or("-".into(), |o| clean(o)));
        }
        rows.push(row);
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
    for line in candidate_lines(&cfg) {
        println!("{line}");
    }
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

fn work_log(o: &Opts, key: &str) -> Result<i32> {
    let (mut c, s) = with_session(key)?;
    let w = c.worklog(&s.id).map_err(rerr)?;
    let mut lines = vec![format!("{} ({})", clean(&s.name), s.id)];
    if w.started_ms == 0 {
        lines.push("no work log: the session started before the runner kept one".into());
    } else {
        let end = if w.ended_ms.is_some() {
            "ended"
        } else {
            "running"
        };
        lines.push(format!(
            "{end}, {} total: working {}, waiting {}, blocked {}",
            span(w.duration_s),
            span(w.working_s),
            span(w.waiting_s),
            span(w.blocked_s)
        ));
        if let Some(a) = &w.agent {
            lines.push(format!("agent: {}", clean(a)));
        }
        if let Some(b) = &w.branch {
            lines.push(format!("branch: {}", clean(b)));
        }
        if let Some(e) = &w.exit {
            lines.push(match (e.code, e.signal) {
                (Some(code), _) => format!("exit code: {code}"),
                (None, Some(sig)) => format!("killed by signal {sig}"),
                _ => "exit: unknown".into(),
            });
        }
        if !w.commits.is_empty() {
            lines.push(format!("commits ({}):", w.commits.len()));
            for c in &w.commits {
                lines.push(format!(
                    "  {} {}",
                    clean(c.sha.get(..7).unwrap_or(&c.sha)),
                    clean(&c.subject)
                ));
            }
        }
        if w.truncated {
            lines.push("the log hit its size limit; later changes are not recorded".into());
        }
    }
    Ok(emit(
        o,
        json!({"id": s.id, "name": s.name, "summary": w}),
        lines.join("\n"),
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
