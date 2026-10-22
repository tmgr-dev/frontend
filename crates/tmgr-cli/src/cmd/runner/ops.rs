use super::{attach, connect, emit, is_down, rerr, runner_config, Opts, RunArgs, RunnerCmd};
use super::{
    observe, remote, AgentsArgs, AgentsCmd, AttachArgs, AutoMode, HaltArgs, RenameArgs, ResumeArgs,
    RmArgs, RunnerCommands, SendArgs,
};
use crate::out::table;
use serde_json::json;
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{CandidateInfo, CreateReq, RestoredRun, RunCreateReq, SessionInfo, StateKind};
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
            RunnerCmd::Restart(a) => super::restart::run(o, a.force),
            RunnerCmd::Status => status(o),
            RunnerCmd::InstallService(a) => super::service::install(o, a.install),
            RunnerCmd::UninstallService => super::service::uninstall(o),
            RunnerCmd::Trust(a) => super::trusted::trust(o, &a.dir),
            RunnerCmd::Untrust(a) => super::trusted::untrust(o, &a.dir),
            RunnerCmd::Trusted => super::trusted::list(o),
            RunnerCmd::Allow(a) => super::allow::allow(o, &a.workspace, a.lifetime),
            RunnerCmd::Disallow(a) => super::allow::disallow(o, &a.workspace),
            RunnerCmd::Allowed => super::allow::allowed(o),
            RunnerCmd::Bind(a) => super::bind::bind(o, &a.target, &a.path),
            RunnerCmd::Unbind(a) => super::bind::unbind(o, &a.target),
            RunnerCmd::List => super::bind::list(o),
            RunnerCmd::Serve => serve(),
        },
        RunnerCommands::Run(a) => match (&a.on, &a.task) {
            (Some(link), _) => remote::run(o, link, a),
            (None, Some(task)) if a.fetch => super::run_task::fetch(o, a, task),
            (None, Some(task)) if a.publish => super::cloud::publish_cmd(o, task),
            (None, Some(task)) if a.review && a.push => super::gate::push(o, task),
            (None, Some(task)) if a.review && a.drop => super::gate::drop_held(o, task),
            (None, Some(task)) if a.review => super::gate::review(o, task),
            (None, Some(task)) => super::run_task::run(o, a, task),
            (None, None) => run(o, a),
        },
        RunnerCommands::Agents(AgentsArgs {
            command: Some(AgentsCmd::Watch),
            ..
        }) => observe::watch(o),
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
        RunnerCommands::Agents(AgentsArgs {
            command: Some(AgentsCmd::Restore { session, discard }),
            ..
        }) => super::restore::run(o, session.as_deref(), *discard),
        RunnerCommands::Agents(AgentsArgs {
            command:
                Some(AgentsCmd::Bundle {
                    task,
                    agent,
                    model,
                    cwd,
                    mode,
                }),
            ..
        }) => super::local::bundle_cmd(o, task, agent, model.as_deref(), cwd, *mode),
        RunnerCommands::Agents(a) => match &a.host {
            Some(link) => remote::agents(o, link),
            None => agents(o),
        },
        RunnerCommands::Attach(a) => match &a.on {
            Some(link) => remote::attach(link, a),
            None => attach_cmd(a),
        },
        RunnerCommands::Send(a) => match a.wait {
            Some(w) => observe::send_wait(o, a, w),
            None => send(o, a),
        },
        RunnerCommands::Peek(a) => observe::peek(o, a),
        RunnerCommands::Halt(a) => match &a.on {
            Some(link) => remote::halt(o, link, a),
            None => halt(o, a),
        },
        RunnerCommands::Rm(a) => rm(o, a),
        RunnerCommands::Resume(a) => resume(o, a),
        RunnerCommands::Rename(a) => rename(o, a),
        RunnerCommands::Session {
            command: super::SessionCmd::Summary(a),
        } => super::attach_summary::print_summary(o, &a.session, a.plain),
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
    let (data, human) = status_report(&st, env!("CARGO_PKG_VERSION"), &super::bin())?;
    Ok(emit(o, data, human))
}

fn status_report(
    st: &tmgr_ipc::api::StatusInfo,
    installed: &str,
    bin: &str,
) -> Result<(serde_json::Value, String)> {
    let behind = super::restart::is_behind(&st.version, installed);
    let mut human = format!(
        "runner is up (pid {}, version {}, up {}, {} session{})",
        st.pid,
        clean(&st.version),
        span(st.uptime_s),
        st.sessions,
        if st.sessions == 1 { "" } else { "s" }
    );
    if behind {
        human.push_str(&format!(
            "\nthe runner is older than this tmgr ({installed}); run `{bin} runner restart`"
        ));
    }
    let mut data = serde_json::to_value(st).map_err(|e| Error::internal(e.to_string()))?;
    data["version_behind"] = json!(behind);
    Ok((data, human))
}

pub(super) fn resolve_exe(name: &str) -> Result<String> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let base = std::env::current_dir().ok();
    exe::resolve_exe(name, &path, base.as_deref())
        .map(|p| p.display().to_string())
        .map_err(Error::not_found)
}

pub(super) fn terminal_size() -> (u16, u16) {
    if std::io::stdout().is_terminal() {
        if let Ok(size) = ratatui::crossterm::terminal::size() {
            return size;
        }
    }
    (120, 40)
}

pub(super) fn run_create_req(
    argv: Vec<String>,
    cwd: String,
    name: Option<String>,
    cols: u16,
    rows: u16,
) -> CreateReq {
    CreateReq {
        argv,
        cwd,
        name,
        profile: None,
        cols,
        rows,
        env_pass: Vec::new(),
        task: None,
    }
}

pub(super) fn run_create_run(
    profile: String,
    cwd: String,
    name: Option<String>,
    model: Option<String>,
    (cols, rows): (u16, u16),
    trust: bool,
) -> RunCreateReq {
    RunCreateReq {
        clones: Vec::new(),
        profile,
        task: None,
        cwd,
        name,
        model,
        cols,
        rows,
        bundle: None,
        bundle_sha256: None,
        cwd_pin: None,
        origin: None,
        permission_mode: None,
        start_key: None,
        clone_of: None,
        trust_cwd: trust.then_some(true),
        trust_via_repo: None,
        agent_work: false,
        cloud: None,
        unattended: false,
        exe_pin: None,
        argv_sha256: None,
    }
}

fn run(o: &Opts, a: &RunArgs) -> Result<i32> {
    if a.trust && !(crate::interact::is_interactive() && std::io::stdout().is_terminal()) {
        return Err(Error::usage("--trust needs an interactive terminal")
            .hint("run it yourself in a terminal; it is never accepted from scripts or agents"));
    }
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
    if raw.is_none() {
        login_preflight(a.agent.as_deref().unwrap_or_default())?;
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
        Some(argv) => client.create(run_create_req(argv, cwd_str, a.name.clone(), cols, rows)),
        None => client.create_run(run_create_run(
            a.agent.clone().unwrap_or_default(),
            cwd_str,
            a.name.clone(),
            a.model.clone(),
            (cols, rows),
            a.trust,
        )),
    }
    .map_err(rerr)?;
    if !a.no_attach && !o.json && std::io::stdout().is_terminal() {
        return attach::run(&cfg, &info.id);
    }
    let bin = super::bin();
    let human = format!(
        "started {} ({}); attach with: {bin} attach {}",
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

pub(super) fn login_preflight(profile: &str) -> Result<()> {
    let (set, _) = ProfileSet::load(profile::user_dir().as_deref());
    let (Some(p), Some(home)) = (set.get(profile), std::env::var_os("HOME")) else {
        return Ok(());
    };
    match login::login_problem(p, Path::new(&home), &super::bin()) {
        Some(msg) => Err(Error::usage(clean(&msg))),
        None => Ok(()),
    }
}

fn credential_target(profile: &str) -> Result<login::ResolvedSecretFile> {
    let (set, _) = ProfileSet::load(profile::user_dir().as_deref());
    let p = set.get(profile).ok_or_else(|| {
        Error::not_found(format!("no agent profile {profile:?}"))
            .hint(format!("{} agents", super::bin()))
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
            let bin = super::bin();
            format!(
                "{profile} refreshed its login inside a session; run `{bin} agents sync {profile}` to apply it to {}",
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
        return Err(Error::not_found(format!("no agent profile {profile:?}"))
            .hint(format!("{} agents", super::bin())));
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
    let bin = super::bin();
    println!(
        "Auto mode for {profile} is on. Turn it off with: {bin} agents sync {profile} --auto off"
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
    let restorable = super::restore::count(&cfg, &sessions);
    let hint = || {
        if restorable > 0 {
            println!(
                "{restorable} session{} stopped by a reboot or logout can be restored: {} agents restore",
                if restorable == 1 { "" } else { "s" },
                super::bin()
            );
        }
    };
    if sessions.is_empty() {
        let bin = super::bin();
        println!("No agent sessions yet. Start one with: {bin} run --agent <profile>");
        for line in candidate_lines(&cfg) {
            println!("{line}");
        }
        hint();
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
    hint();
    Ok(0)
}

fn newest(sessions: &[SessionInfo], f: impl Fn(&SessionInfo) -> bool) -> Option<&SessionInfo> {
    sessions
        .iter()
        .filter(|s| f(s))
        .max_by_key(|s| (s.created_ms, &s.id))
}

pub(super) fn resolve<'a>(sessions: &'a [SessionInfo], key: &str) -> Result<&'a SessionInfo> {
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
        .ok_or_else(|| {
            Error::not_found(format!("no session matches {key:?}"))
                .hint(format!("{} agents", super::bin()))
        })
}

fn attach_cmd(a: &AttachArgs) -> Result<i32> {
    let cfg = runner_config()?;
    let sessions = connect(&cfg)?.list().map_err(rerr)?;
    let target = match &a.session {
        Some(key) => resolve(&sessions, key)?,
        None => newest(&sessions, |s| s.state.kind != StateKind::Exited)
            .or_else(|| newest(&sessions, |_| true))
            .ok_or_else(|| {
                Error::not_found("no sessions to attach to").hint(format!("{} run", super::bin()))
            })?,
    };
    attach::run_with(&cfg, &target.id, !a.no_mouse)
}

pub(super) fn with_session(key: &str) -> Result<(Client, SessionInfo)> {
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
                .hint(format!("{} halt --kill", super::bin())),
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
        if let Some(t) = &w.trust {
            lines.push(format!("trust: {}", clean(t)));
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

fn rename(o: &Opts, a: &RenameArgs) -> Result<i32> {
    let (mut c, s) = with_session(&a.session)?;
    c.rename(&s.id, &a.name).map_err(rerr)?;
    let name = a.name.trim();
    Ok(emit(
        o,
        json!({"id": s.id, "name": name}),
        format!("{} is now {}", clean(&s.name), clean(name)),
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

fn resume_report(r: &RestoredRun, bin: &str) -> String {
    let line = |s: &str| tmgr_core::text::sanitize_line(&clean(s));
    let (name, id) = (line(&r.session.name), line(&r.session.id));
    let how = match r.via.as_deref() {
        Some("id" | "continue") => "resumed",
        Some("rerun") => "rerun",
        Some(_) => "started fresh",
        None if r.resumed => "resumed",
        None => "started fresh",
    };
    format!("{how} {name} ({id}); attach with: {bin} attach {id}")
}

fn resume(o: &Opts, a: &ResumeArgs) -> Result<i32> {
    tmgr_link::privileged::guard()?;
    let (mut c, s) = with_session(&a.session)?;
    if s.state.kind != StateKind::Exited {
        return Err(Error::new(
            Kind::Conflict,
            format!("{} is still running", clean(&s.name)),
        ));
    }
    eprintln!(
        "Resume this session?\n  session  {}\n  name     {}\n  folder   {}",
        clean(&s.id),
        clean(&s.name),
        clean(&s.cwd)
    );
    crate::interact::confirm("Start it again?", false, false)?;
    let r = c.resume(&s.id, s.cols, s.rows).map_err(rerr)?;
    Ok(emit(o, &r, resume_report(&r, &super::bin())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn restored(via: Option<&str>, resumed: bool) -> RestoredRun {
        serde_json::from_value(json!({
            "session": {
                "id": "s_bbbbbbbbbb", "name": "work\u{1b}[2J", "argv": ["/bin/sh"], "cwd": "/w",
                "profile": null, "state": {"kind": "working", "since_ms": 1},
                "created_ms": 1, "cols": 80, "rows": 24, "shim_pid": 1, "child_pid": 2,
                "resumed_from": "s_aaaaaaaaaa"
            },
            "resumed": resumed,
            "via": via,
        }))
        .unwrap()
    }

    #[test]
    fn resume_report_names_what_happened_and_how_to_attach() {
        let say = |via, resumed| resume_report(&restored(via, resumed), "tmgr");
        let tail = "work?[2J (s_bbbbbbbbbb); attach with: tmgr attach s_bbbbbbbbbb";
        assert_eq!(say(Some("id"), true), format!("resumed {tail}"));
        assert_eq!(say(Some("continue"), true), format!("resumed {tail}"));
        assert_eq!(say(Some("rerun"), false), format!("rerun {tail}"));
        assert_eq!(say(Some("fresh"), false), format!("started fresh {tail}"));
        assert_eq!(say(None, true), format!("resumed {tail}"));
        assert_eq!(say(None, false), format!("started fresh {tail}"));
    }

    #[test]
    fn resume_json_keeps_the_runner_fields() {
        let v = serde_json::to_value(restored(Some("id"), true)).unwrap();
        assert_eq!(v["via"], "id");
        assert_eq!(v["resumed"], true);
        assert_eq!(v["session"]["id"], "s_bbbbbbbbbb");
        assert_eq!(v["session"]["resumed_from"], "s_aaaaaaaaaa");
    }

    fn st(version: &str) -> tmgr_ipc::api::StatusInfo {
        tmgr_ipc::api::StatusInfo {
            pid: 7,
            version: version.into(),
            proto: 1,
            uptime_s: 5,
            sessions: 1,
        }
    }

    #[test]
    fn status_hints_a_restart_when_the_runner_is_older() {
        let (data, human) = status_report(&st("1.0.12"), "1.0.13", "tmgr").unwrap();
        assert_eq!(data["version_behind"], true);
        assert!(human.contains("`tmgr runner restart`"));
    }

    #[test]
    fn status_is_quiet_when_equal_or_newer() {
        for v in ["1.0.13", "1.0.14"] {
            let (data, human) = status_report(&st(v), "1.0.13", "tmgr").unwrap();
            assert_eq!(data["version_behind"], false);
            assert!(!human.contains("restart"));
        }
    }
}
