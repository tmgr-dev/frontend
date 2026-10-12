use clap::{Args, Subcommand, ValueEnum};

#[derive(Subcommand)]
pub enum AgentCommand {
    /// Start an Agent work run on a task
    Start(AgentStartArgs),
    /// Report progress of the current run (also a heartbeat)
    Update(AgentUpdateArgs),
    /// Finish the current run
    Finish(AgentFinishArgs),
}

#[derive(Args)]
pub struct AgentStartArgs {
    /// TM-512, #8974 or 8974 (see `tmgr help refs`)
    pub reference: String,
    /// Agent name shown on the task
    #[arg(long, default_value = "claude-code", value_name = "NAME")]
    pub agent: String,
    /// Model the agent runs on
    #[arg(long, value_name = "M")]
    pub model: Option<String>,
    /// Git branch (default: the current branch)
    #[arg(long, value_name = "B")]
    pub branch: Option<String>,
}

#[derive(Args, Default)]
pub struct ProgressArgs {
    /// Commit as "sha subject" (repeatable); merged with the commits already sent
    #[arg(long = "commit", value_name = "LINE")]
    pub commits: Vec<String>,
    /// Pull request URL
    #[arg(long, value_name = "URL")]
    pub pr: Option<String>,
    #[arg(long, value_name = "N", requires = "tests_failed")]
    pub tests_passed: Option<u32>,
    #[arg(long, value_name = "N", requires = "tests_passed")]
    pub tests_failed: Option<u32>,
    /// Command that ran the tests
    #[arg(long, value_name = "C")]
    pub tests_cmd: Option<String>,
    /// What has been done, plain text
    #[arg(long, value_name = "TEXT")]
    pub summary: Option<String>,
}

#[derive(Args)]
pub struct AgentUpdateArgs {
    /// Run id (default: the run of the current branch)
    #[arg(long, value_name = "ID")]
    pub run: Option<u64>,
    /// Git branch
    #[arg(long, value_name = "B")]
    pub branch: Option<String>,
    #[command(flatten)]
    pub progress: ProgressArgs,
}

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum RunStatus {
    Succeeded,
    Failed,
    Cancelled,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Succeeded => "succeeded",
            RunStatus::Failed => "failed",
            RunStatus::Cancelled => "cancelled",
        }
    }
}

#[derive(Args)]
pub struct AgentFinishArgs {
    /// Run id (default: the run of the current branch)
    #[arg(long, value_name = "ID")]
    pub run: Option<u64>,
    #[arg(long, value_enum)]
    pub status: RunStatus,
    #[command(flatten)]
    pub progress: ProgressArgs,
}

#[derive(Subcommand)]
pub enum HookCmd {
    /// Claude Code hook entry point (reads the hook JSON from stdin, always exits 0)
    Claude {
        /// session-start, user-prompt-submit, notification, stop, subagent-stop, pre-tool-use or session-end
        event: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        rest: Vec<String>,
    },
    /// Codex notify entry point (the event JSON is the last argument, always exits 0)
    Codex {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Add the tmgr hooks to the Claude Code or Codex configuration
    Install(HookInstallArgs),
    /// Remove the tmgr hooks from the Claude Code or Codex configuration
    Uninstall(HookInstallArgs),
}

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum HookTarget {
    Claude,
    Codex,
}

#[derive(Args)]
pub struct HookInstallArgs {
    #[arg(value_enum)]
    pub target: HookTarget,
    /// Edit .claude/settings.json of the current repository instead of the user settings
    #[arg(long)]
    pub project: bool,
    /// Print the snippet and change nothing
    #[arg(long)]
    pub print: bool,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Priority {
    Low,
    Normal,
    High,
}

impl Priority {
    pub fn as_str(self) -> &'static str {
        match self {
            Priority::Low => "low",
            Priority::Normal => "normal",
            Priority::High => "high",
        }
    }
}

#[derive(Args)]
pub struct NotifyArgs {
    /// Text of the notification
    pub text: String,
    #[arg(long, value_name = "T")]
    pub title: Option<String>,
    #[arg(long, value_enum)]
    pub priority: Option<Priority>,
    /// http(s) URL to open on tap
    #[arg(long, value_name = "URL")]
    pub link: Option<String>,
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct AlarmArgs {
    #[command(subcommand)]
    pub command: Option<AlarmCommand>,
    /// What happened and what is needed (read aloud on the call)
    pub text: Option<String>,
    #[arg(long, value_name = "T", default_value = "Alarm")]
    pub title: String,
    /// Seconds or 90s, 5m, 1h before the voice call
    #[arg(long, value_name = "DUR")]
    pub ack_timeout: Option<String>,
    #[arg(long, value_name = "DUR")]
    pub delivery_timeout: Option<String>,
    /// Do not place a voice call
    #[arg(long)]
    pub no_call: bool,
    /// Voice call attempts, 1 to 5
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u8).range(1..=5))]
    pub call_attempts: Option<u8>,
    /// Return right after creating the alarm
    #[arg(long)]
    pub no_wait: bool,
    /// Longest wait for a final status
    #[arg(long, value_name = "DUR", default_value = "10m")]
    pub max_wait: String,
}

#[derive(Subcommand)]
pub enum AlarmCommand {
    /// Show an alarm (default: the last one created here)
    Status {
        id: Option<String>,
        /// Long-poll up to this many seconds (max 50)
        #[arg(long, value_name = "SECS", default_value_t = 0, value_parser = clap::value_parser!(u8).range(0..=50))]
        wait: u8,
    },
}
