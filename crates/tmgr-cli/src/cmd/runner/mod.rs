pub(crate) mod allow;
mod attach;
mod attach_chip;
mod attach_clip;
mod attach_export;
mod attach_export_cmd;
mod attach_git;
mod attach_goto;
mod attach_layouts;
mod attach_links;
mod attach_new;
mod attach_notes;
mod attach_prompts;
mod attach_push;
mod attach_questions;
mod attach_remote;
mod attach_shell;
mod attach_signal;
mod attach_status;
mod attach_summary;
mod attach_tasks;
mod attach_templates;
mod attach_title;
mod attach_usage;
pub(crate) mod bind;
pub(crate) mod cloud;
mod gate;
mod local;
mod observe;
mod ops;
mod remote;
mod remote_attach;
mod restart;
mod restore;
mod run_task;
mod service;
mod trusted;
mod watch;

use crate::cli::Cli;
use crate::out::{self, Style};
use clap::{ArgGroup, Args, Subcommand, ValueEnum};
use std::io;
use std::path::PathBuf;
use std::time::Duration;
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
    Agents(AgentsArgs),
    /// [experimental] Open the live full-screen view of sessions
    #[command(
        after_help = "Mouse: click a session in the sidebar or a pane to switch, wheel scrolls history, dropped files are pasted into the pane under the mouse.\nDrag inside a pane selects text in that pane only and copies it on release (double-click a word, triple-click a line); soft-wrapped lines are joined and box-drawing, UI markers and trailing spaces are removed (tmgr config set attach.copy.clean false keeps the text as is). Hold Ctrl or Alt while dragging to send the drag to a program that uses the mouse itself. Shift-drag (Option-drag in iTerm2) is the terminal's own selection, or start with --no-mouse."
    )]
    Attach(AttachArgs),
    /// [experimental] Type a line of text into a session, optionally waiting for the agent to answer
    Send(SendArgs),
    /// [experimental] Print what a session shows right now as plain text
    Peek(PeekArgs),
    /// [experimental] Ask a session to stop (interrupt, then terminate)
    Halt(HaltArgs),
    /// [experimental] Delete a session that has already exited
    Rm(RmArgs),
    /// [experimental] Start an exited session again in place and continue the agent's conversation (asks to confirm)
    Resume(ResumeArgs),
    /// [experimental] Give a session a new name (the id stays the same)
    Rename(RenameArgs),
    /// [experimental] Work with a session's transcript
    Session {
        #[command(subcommand)]
        command: SessionCmd,
    },
}

#[derive(Subcommand)]
pub enum SessionCmd {
    /// [experimental] Print a short summary of what a Claude session did (nothing is sent anywhere)
    Summary(SummaryArgs),
    /// [experimental] Export a Claude session as Markdown (to a file or a TMGR page); only when you ask, after a secret scan
    Export(ExportArgs),
}

#[derive(Args)]
pub struct ExportArgs {
    /// Session id or name
    pub session: String,
    /// Write the Markdown to this file (mode 0600, never overwritten without confirming)
    #[arg(long, value_name = "FILE", conflicts_with = "page")]
    pub md: Option<std::path::PathBuf>,
    /// Create a new TMGR page in the current workspace (leaves this machine; terminal only, asks you to type the workspace code)
    #[arg(long)]
    pub page: bool,
    /// Remove every finding of the secret scan without asking
    #[arg(long)]
    pub remove_findings: bool,
}

#[derive(Args)]
pub struct SummaryArgs {
    /// Session id or name
    pub session: String,
    /// Only list files, commits and pull requests, without asking a model for prose
    #[arg(long)]
    pub plain: bool,
}

#[derive(Subcommand)]
pub enum RunnerCmd {
    /// [experimental] Start the runner in the background
    Start,
    /// [experimental] Stop the runner (sessions keep running)
    Stop,
    /// [experimental] Restart the runner with the installed version (sessions keep running)
    Restart(RestartArgs),
    /// [experimental] Show whether the runner is up
    Status,
    /// [experimental] Register the runner to start at login (shows a preview unless --install)
    InstallService(InstallServiceArgs),
    /// [experimental] Remove the login autostart registration
    UninstallService,
    /// [experimental] Trust a folder: agents started in exactly this folder, or in its `tmgr run` task clones, skip their folder-trust question (asks to confirm)
    Trust(FolderArgs),
    /// [experimental] Stop trusting a folder
    Untrust(FolderArgs),
    /// [experimental] List the trusted folders
    Trusted,
    /// [experimental] Let this machine start agents on tasks of a cloud workspace that you approved in the web app (terminal only, asks you to type the workspace code)
    Allow(AllowArgs),
    /// [experimental] Stop letting this machine start agents on tasks of a cloud workspace
    Disallow(DisallowArgs),
    /// [experimental] List the cloud workspaces allowed here, with expiry and pinned keys
    Allowed,
    /// [experimental] Bind your clone of a category's repository (terminal only, asks you to type the category code)
    Bind(bind::BindArgs),
    /// [experimental] Forget the clone bound to a category (terminal only)
    Unbind(bind::UnbindArgs),
    /// [experimental] List the repositories bound to categories on this machine
    List,
    #[command(hide = true)]
    Serve,
    #[command(hide = true)]
    Watch(WatchArgs),
}

#[derive(Args)]
pub struct WatchArgs {
    #[arg(long)]
    pub dir: String,
    #[arg(long)]
    pub cmd: String,
}

#[derive(Args)]
pub struct AllowArgs {
    /// Workspace code
    #[arg(value_name = "WS")]
    pub workspace: String,
    /// How long the permission lasts: 8h, 7d, 30d or forever
    #[arg(long = "for", value_name = "DURATION", default_value = "30d", value_parser = allow::Lifetime::parse)]
    pub lifetime: allow::Lifetime,
}

#[derive(Args)]
pub struct DisallowArgs {
    /// Workspace code
    #[arg(value_name = "WS")]
    pub workspace: String,
}

#[derive(Args)]
pub struct FolderArgs {
    /// The folder (default: current directory)
    #[arg(value_name = "DIR", default_value = ".", value_parser = crate::paths::PathArg)]
    pub dir: PathBuf,
}

#[derive(Args)]
pub struct RestartArgs {
    /// Replace a newer running runner with this older tmgr
    #[arg(long)]
    pub force: bool,
}

#[derive(Args)]
pub struct InstallServiceArgs {
    /// Write the service file and register it (needs a terminal and a typed confirmation)
    #[arg(long)]
    pub install: bool,
}

#[derive(Args)]
#[command(group(ArgGroup::new("source").required(true).args(["agent_cmd", "agent", "fetch", "publish", "review"])))]
pub struct RunArgs {
    /// Task to run, in a local workspace (--ws local:<code>) or an allowed cloud workspace (see
    /// `runner allow`): the agent works in its own clone of the project's repository on the task's branch,
    /// and the run is recorded as Agent work of the task
    #[arg(value_name = "REF", conflicts_with = "agent_cmd")]
    pub task: Option<String>,
    /// [experimental] Run on this follower (a pinned link) through its own runner
    #[arg(
        long,
        value_name = "LINK",
        requires = "task",
        conflicts_with_all = ["agent_cmd", "no_attach"],
        hide = true
    )]
    pub on: Option<String>,
    /// [experimental] Persona to record with the run (with --on)
    #[arg(long, value_name = "PERSONA", requires = "on", hide = true)]
    pub persona: Option<String>,
    /// [experimental] SHA-256 of the task text the leader saw (with --on)
    #[arg(long, value_name = "HASH", requires = "on", hide = true)]
    pub task_hash: Option<String>,
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
    #[arg(long, value_name = "DIR", value_parser = crate::paths::PathArg)]
    pub cwd: Option<PathBuf>,
    /// Session name
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,
    /// Start the session without opening the live view
    #[arg(long)]
    pub no_attach: bool,
    /// [experimental] Pre-accept the agent's folder-trust prompt for this folder, for this run only
    #[arg(long, requires = "agent", conflicts_with_all = ["agent_cmd", "on", "task"])]
    pub trust: bool,
    /// Permission mode for a task run (default ask; accept-edits for a cloud task; bypass only for a cloud task)
    #[arg(long, value_enum, requires = "task", conflicts_with = "on")]
    pub mode: Option<PermissionMode>,
    /// Branch or commit the task's clone starts from (default: the repository's current branch)
    #[arg(long, value_name = "BASE", requires = "task", conflicts_with = "on")]
    pub base: Option<String>,
    /// Start a task run without the confirmation prompt
    #[arg(long, requires = "task", conflicts_with = "on")]
    pub yes: bool,
    /// Bring the task's branch from the agent's clone into your repository (runs nothing)
    #[arg(long, requires = "task", conflicts_with_all = ["on", "agent", "agent_cmd"])]
    pub fetch: bool,
    /// Push the finished cloud run's branch and open a pull request (runs nothing)
    #[arg(long, requires = "task", conflicts_with_all = ["on", "agent", "agent_cmd", "fetch"])]
    pub publish: bool,
    /// Show what the leak gate found in a finished multi-repository run and the public diff (terminal only)
    #[arg(long, requires = "task", conflicts_with_all = ["on", "agent", "agent_cmd", "fetch", "publish"])]
    pub review: bool,
    /// Push a run held by the leak gate after you reviewed it (asks you to type the repository names)
    #[arg(long, requires = "review", conflicts_with = "drop")]
    pub push: bool,
    /// Discard a run held by the leak gate (asks you to type the task key); the run folder stays
    #[arg(long, requires = "review", conflicts_with = "push")]
    pub drop: bool,
}

#[derive(Args)]
pub struct AgentsArgs {
    #[command(subcommand)]
    pub command: Option<AgentsCmd>,
    /// [experimental] List the runs this leader started on a follower
    #[arg(long, value_name = "LINK", hide = true)]
    pub host: Option<String>,
}

#[derive(Subcommand)]
pub enum AgentsCmd {
    /// [experimental] Store an agent profile's login token (read hidden from the terminal or one line of stdin)
    Login {
        /// Agent profile id
        profile: String,
        /// Overwrite an existing login
        #[arg(long)]
        replace: bool,
    },
    /// [experimental] Remove an agent profile's stored login
    Logout {
        /// Agent profile id
        profile: String,
    },
    /// [experimental] Apply a login an agent refreshed inside a session to your own file, after you confirm
    Sync {
        /// Agent profile id
        profile: String,
        /// Delete the pending refreshed logins without applying them
        #[arg(long, conflicts_with = "auto")]
        discard: bool,
        /// Let the runner apply refreshed logins itself after each session (asks for a typed agreement)
        #[arg(long, value_name = "on|off")]
        auto: Option<AutoMode>,
    },
    /// [experimental] Follow state changes of all sessions until Ctrl-C (--json: one JSON object per line)
    Watch,
    /// [experimental] Show what the runner recorded of a session: time, branch, commits
    Log {
        /// Session id or name
        session: String,
    },
    /// [experimental] List sessions stopped by a reboot or logout and start them again, one confirmation each
    Restore {
        /// Session id to restore (default: list them and ask for each)
        session: Option<String>,
        /// Delete the restore record of this session instead
        #[arg(long, requires = "session")]
        discard: bool,
    },
    /// [experimental] Print the run bundle of a task of a local workspace (--ws local:<code>) and its SHA-256
    Bundle {
        /// Task key (TM-512) or id (#8974)
        task: String,
        /// Agent profile id
        #[arg(long, value_name = "PROFILE")]
        agent: String,
        /// Model to pass to the profile
        #[arg(long, value_name = "MODEL")]
        model: Option<String>,
        /// Working directory of the run
        #[arg(long, value_name = "DIR", value_parser = crate::paths::PathArg)]
        cwd: PathBuf,
        /// Permission mode of the run
        #[arg(long, value_name = "MODE", default_value = "ask")]
        mode: PermissionMode,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PermissionMode {
    Ask,
    AcceptEdits,
    Plan,
    Bypass,
}

impl PermissionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::AcceptEdits => "accept-edits",
            Self::Plan => "plan",
            Self::Bypass => "bypass",
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum AutoMode {
    On,
    Off,
}

#[derive(Args)]
pub struct AttachArgs {
    /// Session id or name (default: the newest running session)
    pub session: Option<String>,
    /// [experimental] Attach to a run on a follower (read-only unless the follower allows input)
    #[arg(long, value_name = "LINK", hide = true)]
    pub on: Option<String>,
    /// [experimental] Never type into the remote run (with --on)
    #[arg(long, requires = "on", hide = true)]
    pub read_only: bool,
    /// Do not capture the mouse (keeps plain terminal text selection)
    #[arg(long)]
    pub no_mouse: bool,
    /// Do not show sessions of linked machines
    #[arg(long)]
    pub no_links: bool,
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
    /// Wait for the agent, then print what the screen gained: `idle` ends when the agent is waiting or blocked again (exit code 8 when blocked), `exit` when the session exits.
    /// The new output is a line diff of the session's plain text before the send and after the wait; TUIs redraw, so repainted lines can show up as new.
    /// The state is read from the screen and terminal activity, so an agent can make itself look idle
    #[arg(long, value_name = "idle|exit")]
    pub wait: Option<WaitFor>,
    /// Longest time to wait (30s, 10m, 1h)
    #[arg(long, value_name = "DURATION", default_value = "10m", value_parser = parse_timeout, requires = "wait")]
    pub timeout: Duration,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum WaitFor {
    Idle,
    Exit,
}

fn parse_timeout(raw: &str) -> std::result::Result<Duration, String> {
    tmgr_core::notify::parse_duration_seconds(raw)
        .map(Duration::from_secs)
        .ok_or_else(|| format!("{raw:?} is not a duration like 30s, 10m or 1h"))
}

#[derive(Args)]
pub struct PeekArgs {
    /// Session id or name
    pub session: String,
    /// Show the last N lines including scrollback (default: the current screen); blank rows at the bottom are not shown
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..=10000))]
    pub lines: Option<u32>,
}

#[derive(Args)]
pub struct HaltArgs {
    /// Session id or name
    pub session: String,
    /// Kill immediately instead of asking politely first
    #[arg(long, conflicts_with = "on")]
    pub kill: bool,
    /// [experimental] Stop a run this leader started on a follower
    #[arg(long, value_name = "LINK", hide = true)]
    pub on: Option<String>,
}

#[derive(Args)]
pub struct RenameArgs {
    /// Session id or name
    pub session: String,
    /// New name, unique among sessions
    pub name: String,
}

#[derive(Args)]
pub struct RmArgs {
    /// Session id or name
    pub session: String,
}

#[derive(Args)]
pub struct ResumeArgs {
    /// Session id or name of an exited session
    pub session: String,
}

pub struct Opts<'a> {
    pub cli: &'a Cli,
    pub ws: Option<String>,
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
        Some("__agent-work") if args.len() == 1 => Some(local::agent_work_main()),
        _ => None,
    }
}

pub fn run(cli: &Cli, cmd: &RunnerCommands) -> i32 {
    let o = Opts {
        cli,
        ws: cli.ws.clone(),
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
        return Error::new(Kind::Offline, "the runner is not running")
            .hint(format!("{} runner start", bin()));
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
    if msg.starts_with("unknown_method: ") {
        return Error::new(Kind::Conflict, "the running runner is older than this tmgr")
            .hint(format!("{0} runner restart", bin()));
    }
    let Some((code, text)) = msg.split_once(": ") else {
        return Error::internal(msg);
    };
    let kind = match code {
        "not_found" => Kind::NotFound,
        "invalid_params" | "unknown_profile" => Kind::Usage,
        "sandbox" | "forbidden" => Kind::Forbidden,
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

pub fn bin() -> String {
    std::env::args_os()
        .next()
        .and_then(|a| {
            std::path::Path::new(&a)
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_string)
        })
        .filter(|n| {
            !n.is_empty()
                && n.len() <= 64
                && n.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        })
        .unwrap_or_else(|| "tmgr".to_string())
}
