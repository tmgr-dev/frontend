mod attach;
mod ops;

use crate::cli::Cli;
use crate::out::{self, Style};
use clap::{ArgGroup, Args, Subcommand};
use std::io;
use std::path::PathBuf;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::Envelope;
use tmgr_runner::client::Client;
use tmgr_runner::daemon::RunnerConfig;

#[derive(Subcommand)]
pub enum RunnerCommands {
    /// [experimental] Manage the background runner process
    Runner {
        #[command(subcommand)]
        command: RunnerCmd,
    },
    /// [experimental] Start an agent in a managed terminal session
    Run(RunArgs),
    /// [experimental] List sessions and what each agent is doing
    Agents,
    /// [experimental] Open the live full-screen view of sessions
    Attach(AttachArgs),
    /// [experimental] Type a line of text into a session
    Send(SendArgs),
    /// [experimental] Ask a session to stop (interrupt, then terminate)
    Halt(HaltArgs),
    /// [experimental] Delete a session that has already exited
    Rm(RmArgs),
}

#[derive(Subcommand)]
pub enum RunnerCmd {
    /// [experimental] Start the runner in the background
    Start,
    /// [experimental] Stop the runner (sessions keep running)
    Stop,
    /// [experimental] Show whether the runner is up
    Status,
    #[command(hide = true)]
    Serve,
}

#[derive(Args)]
#[command(group(ArgGroup::new("source").required(true).args(["agent_cmd", "agent"])))]
pub struct RunArgs {
    /// Command line to run (split into words, no shell)
    #[arg(long, value_name = "COMMAND")]
    pub agent_cmd: Option<String>,
    /// Agent profile id (see the agents directory)
    #[arg(long, value_name = "PROFILE")]
    pub agent: Option<String>,
    /// Model to pass to the profile
    #[arg(long, value_name = "MODEL", requires = "agent")]
    pub model: Option<String>,
    /// Working directory (default: current directory)
    #[arg(long, value_name = "DIR")]
    pub cwd: Option<PathBuf>,
    /// Session name
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,
    /// Start the session without opening the live view
    #[arg(long)]
    pub no_attach: bool,
}

#[derive(Args)]
pub struct AttachArgs {
    /// Session id or name (default: the newest running session)
    pub session: Option<String>,
}

#[derive(Args)]
pub struct SendArgs {
    /// Session id or name
    pub session: String,
    /// Text to type
    pub text: String,
    /// Do not press Enter after the text
    #[arg(long)]
    pub no_enter: bool,
}

#[derive(Args)]
pub struct HaltArgs {
    /// Session id or name
    pub session: String,
    /// Kill immediately instead of asking politely first
    #[arg(long)]
    pub kill: bool,
}

#[derive(Args)]
pub struct RmArgs {
    /// Session id or name
    pub session: String,
}

pub struct Opts {
    pub json: bool,
    pub quiet: bool,
    pub style: Style,
}

pub fn internal_entry() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("__shim") if args.len() == 3 => Some(tmgr_runner::shim::shim_main(
            &PathBuf::from(&args[1]),
            &args[2],
        )),
        Some("__sandbox-exec") => Some(tmgr_runner::sandbox::exec_helper(&args[1..])),
        _ => None,
    }
}

pub fn run(cli: &Cli, cmd: &RunnerCommands) -> i32 {
    let o = Opts {
        json: cli.json,
        quiet: cli.quiet,
        style: out::stdout_style(cli.no_color),
    };
    match ops::dispatch(&o, cmd) {
        Ok(code) => code,
        Err(e) => out::print_error(cli.json, cli.verbose, out::stderr_style(cli.no_color), &e),
    }
}

pub fn emit(o: &Opts, data: impl serde::Serialize, human: impl AsRef<str>) -> i32 {
    if o.json {
        let env = Envelope::ok(data);
        println!(
            "{}",
            serde_json::to_string(&env).expect("envelope serializes")
        );
    } else if !human.as_ref().is_empty() {
        println!("{}", human.as_ref());
    }
    0
}

pub fn is_down(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
    )
}

pub fn rerr(e: io::Error) -> Error {
    if is_down(&e) {
        return Error::new(Kind::Offline, "the runner is not running").hint("tmgr runner start");
    }
    if matches!(
        e.kind(),
        io::ErrorKind::UnexpectedEof
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
    ) {
        return Error::new(Kind::Offline, "lost the connection to the runner");
    }
    let msg = e.to_string();
    let Some((code, text)) = msg.split_once(": ") else {
        return Error::internal(msg);
    };
    let kind = match code {
        "not_found" => Kind::NotFound,
        "invalid_params" | "unknown_profile" => Kind::Usage,
        "sandbox" => Kind::Forbidden,
        "busy" | "not_running" | "limit" => Kind::Conflict,
        _ => return Error::internal(msg),
    };
    Error::new(kind, text)
}

pub fn runner_config() -> Result<RunnerConfig> {
    let exe = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|e| Error::internal(format!("cannot locate the tmgr executable: {e}")))?;
    RunnerConfig::from_env(exe, env!("CARGO_PKG_VERSION").into()).map_err(rerr)
}

pub fn connect(cfg: &RunnerConfig) -> Result<Client> {
    Client::connect(&cfg.dir).map_err(rerr)
}
