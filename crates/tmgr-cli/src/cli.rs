use clap::{Args, Parser, Subcommand};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = "tmgr", version = VERSION, about = "TMGR command line tool")]
pub struct Cli {
    /// Machine-readable output (schema tmgr.v1)
    #[arg(long, global = true)]
    pub json: bool,
    /// Workspace code (default: `tmgr ws use`)
    #[arg(long, global = true, value_name = "CODE")]
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
        command: WsCommand,
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
    #[command(
        about = "How task references are written",
        long_about = REFS_HELP,
        after_help = ""
    )]
    Refs,
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
    Use { code: String },
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
