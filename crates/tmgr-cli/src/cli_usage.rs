use clap::{Args, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
pub struct UsageArgs {
    #[command(subcommand)]
    pub command: Option<UsageCommand>,
    #[command(flatten)]
    pub summary: SummaryArgs,
}

#[derive(Args, Default)]
pub struct SummaryArgs {
    /// Today only
    #[arg(long, group = "period")]
    pub today: bool,
    /// Yesterday only
    #[arg(long, group = "period")]
    pub yesterday: bool,
    /// The last 7 days including today
    #[arg(long, group = "period")]
    pub week: bool,
    /// The last 30 days including today
    #[arg(long, group = "period")]
    pub month: bool,
    /// First day, YYYY-MM-DD (local time zone)
    #[arg(long, value_name = "DATE", conflicts_with = "period")]
    pub since: Option<String>,
    /// Last day, YYYY-MM-DD, included (default: today)
    #[arg(
        long,
        value_name = "DATE",
        requires = "since",
        conflicts_with = "period"
    )]
    pub until: Option<String>,
    /// Group by one dimension
    #[arg(long, value_enum, value_name = "DIMENSION")]
    pub by: Option<By>,
    /// Only usage linked to this task (TM-512 or #8974)
    #[arg(long, value_name = "REF")]
    pub task: Option<String>,
    /// Only this tool
    #[arg(long, value_enum)]
    pub tool: Option<ToolArg>,
    /// Only this account label
    #[arg(long, value_name = "LABEL")]
    pub account: Option<String>,
    /// Do not scan the logs first
    #[arg(long)]
    pub no_scan: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum By {
    Tool,
    Model,
    Account,
    Project,
    Branch,
    Day,
    Task,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ToolArg {
    Claude,
    Codex,
}

#[derive(Subcommand)]
pub enum UsageCommand {
    /// Rate limit windows with percent used and the time until reset
    Limits {
        /// Do not scan the logs first
        #[arg(long)]
        no_scan: bool,
    },
    /// Cost, time, tokens and the agent tree of one task
    Task(TaskArgs),
    /// Read new log lines into the local usage database
    Scan,
    /// Accounts whose logs are scanned (default roots plus the ones you add)
    Accounts {
        #[command(subcommand)]
        command: Option<AccountsCommand>,
    },
    /// Model prices used for the API-equivalent cost
    Pricing {
        #[command(subcommand)]
        command: Option<PricingCommand>,
    },
    /// Upload per-task usage to TMGR (opt-in, never automatic)
    Sync(SyncArgs),
    /// Delete all usage data (database and statusline snapshots)
    Reset,
    /// Claude Code statusLine integration for rate limits
    Statusline {
        #[command(subcommand)]
        command: StatuslineCommand,
    },
    #[command(name = "statusline-hook", hide = true)]
    StatuslineHook(StatuslineHookArgs),
}

#[derive(Args)]
pub struct TaskArgs {
    /// Task reference (TM-512 or #8974)
    #[arg(value_name = "REF")]
    pub reference: String,
    /// Show every agent, not only the top level
    #[arg(long)]
    pub tree: bool,
    /// How many days back to look for the task's work (at most 90, the kept history)
    #[arg(long, value_name = "N", default_value_t = 30, value_parser = clap::value_parser!(i64).range(1..=90))]
    pub days: i64,
    /// Do not scan the logs first
    #[arg(long)]
    pub no_scan: bool,
}

#[derive(Args)]
#[command(
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true,
    long_about = "Upload one summary per task (cost, tokens, time, the agent tree, tool and skill names; never prompts, replies, paths or arguments) so the members of the task's workspace can see it.\n\nNothing is uploaded automatically and nothing until two locks are open: `tmgr usage sync on` (typed consent in a terminal) and the Agent usage sync setting in TMGR Settings -> Agent work. Use --dry-run to see exactly what would be sent. Local workspaces are never uploaded."
)]
pub struct SyncArgs {
    #[command(subcommand)]
    pub command: Option<SyncCommand>,
    #[command(flatten)]
    pub run: SyncRunArgs,
}

#[derive(Args)]
pub struct SyncRunArgs {
    /// Only this task (TM-512 or #8974)
    #[arg(long, value_name = "REF")]
    pub task: Option<String>,
    /// How many days back to read (at most 90, the kept history)
    #[arg(long, value_name = "N", default_value_t = 30, value_parser = clap::value_parser!(i64).range(1..=90))]
    pub days: i64,
    /// Print what would be sent and send nothing (works without consent)
    #[arg(long)]
    pub dry_run: bool,
    /// Send even when the local usage is smaller than what was uploaded before
    #[arg(long, requires = "task")]
    pub force: bool,
    /// Do not scan the logs first
    #[arg(long)]
    pub no_scan: bool,
}

#[derive(Subcommand)]
pub enum SyncCommand {
    /// Turn uploading on: shows who can see the data and a real sample, then asks you to type yes (terminal only)
    On,
    /// Turn uploading off; earlier uploads stay until the server setting is turned off or you run `forget`
    #[command(
        long_about = "Turn uploading off. Uploads made earlier stay on the server until the Agent usage sync setting is turned off (that deletes them) or you run `tmgr usage sync forget`.\n\nRun `tmgr usage sync forget` before `tmgr usage reset`: reset clears the local upload history but keeps this install's source id, so afterwards a smaller upload could replace the fuller one on the server."
    )]
    Off,
    /// Whether uploading is on, and the result of the last run
    Status,
    /// Delete what you uploaded from the server (all tasks, or one with --task)
    #[command(
        long_about = "Delete what you uploaded from the server: everything with no flags, one task with --task. Needs a typed confirmation in a terminal or --yes.\n\nRun `tmgr usage sync forget` before `tmgr usage reset`: reset clears the local upload history but keeps this install's source id, so afterwards a smaller upload could replace the fuller one on the server."
    )]
    Forget {
        /// Only this task (TM-512 or #8974)
        #[arg(long, value_name = "REF")]
        task: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum AccountsCommand {
    /// List accounts
    List,
    /// Add a log root as a named account
    Add {
        #[arg(long, value_enum)]
        tool: ToolArg,
        /// The tool's config directory (the one holding projects/ or sessions/)
        #[arg(long, value_name = "DIR", value_parser = crate::paths::PathArg)]
        root: PathBuf,
        #[arg(long, value_name = "LABEL")]
        label: String,
    },
    /// Remove an account you added (its stored usage stays)
    Remove { label: String },
}

#[derive(Subcommand)]
pub enum PricingCommand {
    /// Show the price source and, with --model, matching prices
    Show {
        /// Only models containing this text
        #[arg(long, value_name = "TEXT")]
        model: Option<String>,
    },
    /// Download the public LiteLLM price list and re-price stored events
    Update {
        /// Do not download: go back to the bundled snapshot
        #[arg(long)]
        offline: bool,
        #[arg(long, hide = true, value_name = "URL")]
        url: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum StatuslineCommand {
    /// Print the settings.json change; with --install write it after a terminal confirmation
    Install(StatuslineInstallArgs),
    /// Restore the previous statusLine command
    Uninstall(StatuslineAccountArgs),
}

#[derive(Args)]
pub struct StatuslineInstallArgs {
    #[command(flatten)]
    pub account: StatuslineAccountArgs,
    /// Edit settings.json (shows the diff, writes a backup, never accepts --yes)
    #[arg(long)]
    pub install: bool,
}

#[derive(Args)]
pub struct StatuslineAccountArgs {
    /// Claude account label (default: claude)
    #[arg(long, value_name = "LABEL")]
    pub account: Option<String>,
}

#[derive(Args)]
pub struct StatuslineHookArgs {
    /// Command to run with the same stdin; its stdout is printed
    #[arg(long, value_name = "CMD")]
    pub chain: Option<String>,
}
