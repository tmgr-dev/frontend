use crate::cli_agent::{AgentCommand, AlarmArgs, HookCmd, NotifyArgs};
use crate::cli_link::{LinkCommand, WorkerArgs};
use crate::cli_usage::UsageArgs;
use clap::{Args, Parser, Subcommand};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = "tmgr", version = VERSION, about = "TMGR command line tool")]
pub struct Cli {
    /// Machine-readable output (schema tmgr.v1)
    #[arg(long, global = true)]
    pub json: bool,
    /// Workspace code, or local:<code> for a desktop workspace (default: `tmgr ws use`)
    #[arg(
        long,
        global = true,
        value_name = "CODE",
        long_help = "Workspace code (default: `tmgr ws use`).\n\nlocal:<code> selects a local workspace of the desktop app (~/.tmgr.dev/workspaces/<code>, or $TMGR_LOCAL_ROOT): no login and no network. The CLI acts as the desktop's local account (id -1, name from ~/.tmgr.dev/profile.json); when the desktop is signed in to an account, set `tmgr config set local_user_id <id>` to act as that user. Agent runs can only be updated by the user who started them, and only personas of that user or of the local account can be assigned."
    )]
    pub ws: Option<String>,
    /// Never use colours
    #[arg(long, global = true)]
    pub no_color: bool,
    /// Print only what was asked for
    #[arg(short, long, global = true)]
    pub quiet: bool,
    /// Accept non-privileged confirmations without asking
    #[arg(short = 'y', long, global = true)]
    pub yes: bool,
    /// Log requests (never tokens) to stderr
    #[arg(short, long, global = true)]
    pub verbose: bool,
    /// API server
    #[arg(long, global = true, env = "TMGR_SERVER", value_name = "URL")]
    pub server: Option<String>,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    #[cfg(feature = "runner")]
    #[command(flatten)]
    Runner(crate::cmd::runner::RunnerCommands),
    /// Print version, commit and build target
    Version,
    /// Sign in with a device code
    Login(LoginArgs),
    /// Sign out and delete the local token
    Logout,
    /// Show who is signed in
    Whoami,
    /// Account commands (login, logout, status)
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    /// List and select workspaces
    Ws {
        #[command(subcommand)]
        command: Option<WsCommand>,
    },
    /// Active task and today's time (default command)
    Status,
    /// Show and list tasks
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    /// Create a task (`-` reads title and description from stdin)
    Add(AddArgs),
    /// Read and write the config file
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Start a task: branch, status and timer
    Start(StartArgs),
    /// Stop the running timer
    Stop,
    /// Stop the running timer and start another task
    Switch(StartArgs),
    /// Finish a task: stop the timer, move to review, link the PR
    Done(DoneArgs),
    /// Map task projects to local repositories
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    /// Git integration (commit hook)
    Git {
        #[command(subcommand)]
        command: GitCommand,
    },
    /// Print the prompt segment (reads only the local state file)
    Prompt(PromptArgs),
    /// Print a shell, tmux, starship or Claude Code snippet
    Init(InitArgs),
    /// Status line output for other tools
    Statusline {
        #[arg(value_enum)]
        target: StatuslineTarget,
    },
    /// Agent work runs of the workspace (`--follow` keeps polling)
    Log(LogArgs),
    /// Tracked time, agent time and routines for a day
    Today(TodayArgs),
    /// Daily routines
    Routine {
        #[command(subcommand)]
        command: RoutineCommand,
    },
    /// Write a note in $VISUAL / $EDITOR and save it as a page
    Note(NoteArgs),
    /// Sync pages with Markdown files
    Page {
        #[command(subcommand)]
        command: PageCommand,
    },
    /// Agent work runs (start, update, finish)
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    /// Hooks for Claude Code and Codex, and their installation
    Hook {
        #[command(subcommand)]
        command: HookCmd,
    },
    /// Send a push notification to your phone
    Notify(NotifyArgs),
    /// Raise a phone alarm (push, then a voice call), or check one
    Alarm(AlarmArgs),
    /// Run the MCP server over stdio (notify_user, alarm, alarm_status, tmgr_cli)
    Mcp,
    /// Leader and follower links over SSH (experimental)
    #[command(hide = true)]
    Link {
        #[command(subcommand)]
        command: LinkCommand,
    },
    /// Auto-run queue for local workspaces (experimental)
    #[cfg(feature = "runner")]
    #[command(hide = true)]
    Queue {
        #[command(subcommand)]
        command: crate::cmd::queue::QueueCommand,
    },
    /// Forced command for a follower, started by sshd only (experimental)
    #[command(hide = true)]
    Worker(WorkerArgs),
    #[command(
        about = "How task references are written",
        long_about = REFS_HELP,
        after_help = ""
    )]
    Refs,
    /// Full-screen kanban board of the workspace
    Board(BoardArgs),
    /// Local AI tool usage: cost, tokens, tasks and rate limits
    Usage(UsageArgs),
    /// Update tmgr to the newest signed release
    #[command(name = "self-update")]
    SelfUpdate(SelfUpdateArgs),
}

#[derive(Args)]
pub struct BoardArgs {
    /// Project (category) code
    #[arg(short, long, value_name = "CODE")]
    pub project: Option<String>,
    /// Only tasks assigned to you
    #[arg(long)]
    pub mine: bool,
    /// ASCII glyphs and borders only
    #[arg(long)]
    pub ascii: bool,
}

#[derive(Args)]
pub struct SelfUpdateArgs {
    /// Only report whether an update is available
    #[arg(long)]
    pub check: bool,
    /// Release channel
    #[arg(long, value_enum, default_value_t = UpdateChannel::Stable)]
    pub channel: UpdateChannel,
    /// Allow installing an older version (needs a terminal)
    #[arg(long)]
    pub allow_downgrade: bool,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum UpdateChannel {
    Stable,
    Beta,
}

pub const REFS_HELP: &str = "Task references

  TM-512   category code and number, as shown in the task title (needs --ws or a default workspace)
  #8974    global task id
  8974     same as #8974 (a bare number is the global task id)

Grammar: ^[A-Z][A-Z0-9]{0,9}-[0-9]{1,7}$ for keys, ^#?[0-9]{1,9}$ for ids.
Anything else is rejected with exit code 2 before any request is made.";

#[derive(Subcommand)]
pub enum AuthCommand {
    /// Sign in with a device code
    Login(LoginArgs),
    /// Sign out and delete the local token
    Logout,
    /// Show who is signed in
    Status,
}

#[derive(Args)]
pub struct LoginArgs {
    /// Comma-separated scopes to request
    #[arg(long, value_name = "LIST")]
    pub scopes: Option<String>,
    /// Read an existing token from stdin instead of the device flow
    #[arg(long)]
    pub with_token: bool,
    /// Name shown on the approval page (never the machine hostname)
    #[arg(long, default_value = "tmgr cli", value_name = "TEXT")]
    pub label: String,
    /// Do not try to open a browser
    #[arg(long)]
    pub no_browser: bool,
}

#[derive(Subcommand)]
pub enum WsCommand {
    /// List workspaces
    List,
    /// Make a workspace the default
    Use {
        /// Workspace code (asked for in a terminal when omitted)
        code: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum TaskCommand {
    /// Show one task
    Show {
        /// TM-512, #8974 or 8974 (see `tmgr help refs`)
        reference: String,
        /// Number of latest comments to show
        #[arg(long, default_value_t = 5, value_name = "N")]
        comments: usize,
        /// Maximum description size, e.g. 64k
        #[arg(long, default_value = "64k", value_name = "SIZE")]
        max_desc: String,
    },
    /// List tasks
    List {
        /// Project (category) code
        #[arg(short, long, value_name = "CODE")]
        project: Option<String>,
        /// default, active, completed, hidden or archived
        #[arg(long, value_name = "TYPE")]
        status_type: Option<String>,
        #[arg(long, default_value_t = 50, value_name = "N")]
        limit: usize,
        /// Value of next_cursor from the previous page
        #[arg(long, value_name = "C")]
        cursor: Option<String>,
    },
}

#[derive(Args)]
pub struct AddArgs {
    /// Task title, or `-` to read it from stdin (first line title, rest description)
    pub title: String,
    /// Project (category) code
    #[arg(short, long, value_name = "CODE")]
    pub project: Option<String>,
    /// Status name (default: the workspace's default status)
    #[arg(short, long, value_name = "NAME")]
    pub status: Option<String>,
    /// Read the description from a file (up to 64 KiB)
    #[arg(long, value_name = "FILE")]
    pub desc_file: Option<std::path::PathBuf>,
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Print all settings
    List,
    /// Print one setting
    Get { key: String },
    /// Change one setting
    Set { key: String, value: String },
    /// Print the config file path
    Path,
}

#[derive(Args)]
pub struct StartArgs {
    /// TM-512, #8974 or 8974 (see `tmgr help refs`)
    pub reference: String,
    /// Branch name (default: from branch_template)
    #[arg(long, value_name = "NAME", conflicts_with = "no_branch")]
    pub branch: Option<String>,
    /// Do not create or switch a branch
    #[arg(long)]
    pub no_branch: bool,
    /// Create the branch from this base (default: the current HEAD)
    #[arg(long, value_name = "BASE")]
    pub from: Option<String>,
    /// Do not start the timer
    #[arg(long)]
    pub no_timer: bool,
    /// Move the task to this status instead of the first active one
    #[arg(long, value_name = "NAME")]
    pub status: Option<String>,
    /// Stash uncommitted changes (git stash push -u) instead of refusing
    #[arg(long)]
    pub stash: bool,
}

#[derive(Args)]
pub struct DoneArgs {
    /// Task reference (default: the active task)
    pub reference: Option<String>,
    /// Move the task to this status instead of the review status
    #[arg(long, value_name = "NAME")]
    pub status: Option<String>,
    /// Do not look for a pull request
    #[arg(long)]
    pub no_pr: bool,
    /// Text of the comment (the PR link is appended)
    #[arg(long, value_name = "TEXT")]
    pub comment: Option<String>,
    /// Continue although the working tree has uncommitted changes
    #[arg(long)]
    pub force: bool,
}

#[derive(Subcommand)]
pub enum ProjectCommand {
    /// Map a project code to a local repository
    Map {
        code: String,
        path: std::path::PathBuf,
    },
    /// Remove a mapping
    Unmap { code: String },
    /// List mappings
    List,
}

#[derive(Subcommand)]
pub enum GitCommand {
    /// Run a git hook (called by the installed hook script)
    Hook {
        #[command(subcommand)]
        command: HookCommand,
    },
    /// Install the commit-msg hook
    InstallHooks {
        /// Install for every repository (core.hooksPath in the global git config)
        #[arg(long)]
        global: bool,
    },
}

#[derive(Subcommand)]
pub enum HookCommand {
    /// Append a Refs trailer for the task of the current branch
    CommitMsg { file: std::path::PathBuf },
}

#[derive(Args)]
pub struct PromptArgs {
    /// plain, zsh, bash, fish, starship, tmux or claude
    #[arg(long, default_value = "plain", value_name = "FORMAT")]
    pub format: String,
}

#[derive(Clone, Copy, clap::ValueEnum, PartialEq, Eq)]
pub enum InitTarget {
    Zsh,
    Bash,
    Fish,
    Starship,
    Tmux,
    Claude,
}

#[derive(Args)]
pub struct InitArgs {
    #[arg(value_enum)]
    pub target: InitTarget,
    /// Add the snippet to the config file (shows a diff, asks, writes a backup)
    #[arg(long, conflicts_with = "uninstall")]
    pub install: bool,
    /// Remove the snippet added by --install
    #[arg(long)]
    pub uninstall: bool,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum StatuslineTarget {
    Claude,
}

#[derive(Args)]
pub struct LogArgs {
    /// Keep polling every 5 seconds (NDJSON events with --json), Ctrl-C to stop
    #[arg(long)]
    pub follow: bool,
    /// Only runs of this task (TM-512 or #8974)
    #[arg(long, value_name = "REF")]
    pub task: Option<String>,
    /// Only runs from the last period, e.g. 30m, 2h, 1d
    #[arg(long, value_name = "PERIOD")]
    pub since: Option<String>,
    /// Only runs of this agent, e.g. claude-code
    #[arg(long, value_name = "NAME")]
    pub agent: Option<String>,
    #[arg(long, default_value_t = 20, value_name = "N")]
    pub limit: u32,
    /// Value of next_cursor from the previous page
    #[arg(long, value_name = "C", conflicts_with = "follow")]
    pub cursor: Option<String>,
}

#[derive(Args)]
pub struct TodayArgs {
    /// Day to show, YYYY-MM-DD (default: today)
    #[arg(long, value_name = "DATE", conflicts_with = "yesterday")]
    pub date: Option<String>,
    #[arg(long)]
    pub yesterday: bool,
    /// Count only agent runs started by you (tracked time is always yours)
    #[arg(long)]
    pub mine: bool,
}

#[derive(Subcommand)]
pub enum RoutineCommand {
    /// List routines due on a day
    List {
        /// Day to show, YYYY-MM-DD (default: today)
        #[arg(long, value_name = "DATE")]
        date: Option<String>,
    },
    /// Mark a routine done (ID as shown by `routine list`)
    Done {
        id: String,
        /// Day of the routine instance, YYYY-MM-DD (default: today)
        #[arg(long, value_name = "DATE")]
        date: Option<String>,
    },
}

#[derive(Args)]
pub struct NoteArgs {
    /// Page title (default: the first heading of the note)
    #[arg(conflicts_with = "title_flag")]
    pub title: Option<String>,
    /// Page title
    #[arg(long = "title", value_name = "TEXT")]
    pub title_flag: Option<String>,
    /// Link the page to this task
    #[arg(long, value_name = "REF")]
    pub task: Option<String>,
    /// Parent page id
    #[arg(long, value_name = "ID")]
    pub parent: Option<String>,
}

#[derive(Subcommand)]
pub enum PageCommand {
    /// Write pages of the workspace as Markdown files into a directory
    Pull {
        dir: std::path::PathBuf,
        /// Only this page and its children
        #[arg(long, value_name = "PAGE_ID", conflicts_with = "all")]
        tree: Option<u64>,
        /// Every page of the workspace (default)
        #[arg(long)]
        all: bool,
        /// Overwrite files with local changes
        #[arg(long)]
        force: bool,
    },
    /// Upload changed files (default: the current directory)
    Push { files: Vec<std::path::PathBuf> },
    /// Show which files changed locally or remotely
    Status { dir: Option<std::path::PathBuf> },
}
