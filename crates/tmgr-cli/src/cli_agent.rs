use clap::{Args, Subcommand, ValueEnum};

#[derive(Subcommand)]
pub enum AgentCommand {
    /// Start an Agent work run on a task
    Start(AgentStartArgs),
    /// Report progress of the current run (also a heartbeat)
    Update(AgentUpdateArgs),
    /// Finish the current run
    Finish(AgentFinishArgs),
    /// Opt-in status of a local agent session (needs `tmgr config set upload_status true`)
    Status {
        #[command(subcommand)]
        command: AgentStatusCommand,
    },
}

#[derive(Subcommand)]
pub enum AgentStatusCommand {
    /// Upload the state of a session
    Set(AgentStatusSetArgs),
    /// End a session
    Clear(AgentStatusClearArgs),
    /// Sessions of the workspace
    List,
}

#[derive(Args)]
pub struct AgentStatusSetArgs {
    /// Client-generated session id, 8 to 64 characters of letters, digits, _ and -
    pub session_id: String,
    /// working, waiting, blocked, idle, done or exited
    #[arg(long, value_name = "STATE")]
    pub state: String,
    /// Task the session works on: TM-512 or #8974 (see `tmgr help refs`)
    #[arg(long, value_name = "REF")]
    pub task: Option<String>,
    /// Agent profile id
    #[arg(long, default_value = "claude-code", value_name = "ID")]
    pub profile: String,
    /// When the current state began (RFC 3339; default: now)
    #[arg(long, value_name = "TIME")]
    pub since: Option<String>,
    /// When the session started (RFC 3339; kept from the first upload)
    #[arg(long, value_name = "TIME")]
    pub started: Option<String>,
}

#[derive(Args)]
pub struct AgentStatusClearArgs {
    /// Session id given to `agent status set`
    pub session_id: String,
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
    /// Task of a run that is not in the local state (needed to merge its commits)
    #[arg(long, value_name = "REF")]
    pub task: Option<String>,
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
        /// session-start, user-prompt-submit, notification, stop, subagent-stop, pre-tool-use, post-tool-use or session-end
        event: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        rest: Vec<String>,
    },
    /// Decisions inbox hook entry point (stop, notification, permission-request or user-prompt-submit; always exits 0)
    Decisions {
        event: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        rest: Vec<String>,
    },
    /// Codex notify entry point (the event JSON is the last argument, always exits 0)
    Codex {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Add the tmgr hooks to the Claude Code or Codex configuration, or the Decisions inbox hooks
    Install(HookInstallArgs),
    /// Remove the tmgr hooks from the Claude Code or Codex configuration
    Uninstall(HookInstallArgs),
}

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum HookTarget {
    Claude,
    Codex,
    Decisions,
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

#[derive(Args)]
#[command(after_long_help = ASK_HELP)]
pub struct AskArgs {
    /// Headline of the card
    #[arg(long, value_name = "TEXT")]
    pub title: String,
    /// What you need decided
    #[arg(long, value_name = "TEXT")]
    pub question: Option<String>,
    /// Background in Markdown
    #[arg(long, value_name = "MD")]
    pub context: Option<String>,
    /// A choice (repeatable, at most 9)
    #[arg(long = "option", value_name = "LABEL")]
    pub options: Vec<String>,
    /// Number (from 1) of the recommended option
    #[arg(long, value_name = "N")]
    pub recommended: Option<usize>,
    /// decision, review, release, check or question
    #[arg(long, default_value = "decision", value_parser = ["decision", "review", "release", "check", "question"], value_name = "KIND")]
    pub kind: String,
    /// The work is blocked until answered (implied by --wait)
    #[arg(long)]
    pub blocking: bool,
    /// Git repository to show the diff of
    #[arg(long, value_name = "DIR")]
    pub repo: Option<String>,
    /// Base ref of the diff
    #[arg(long, value_name = "REF", requires = "repo")]
    pub base: Option<String>,
    /// Head ref of the diff (default: the working tree)
    #[arg(long, value_name = "REF", requires = "base")]
    pub head: Option<String>,
    /// A fact (repeatable): ok|bad|wait:TEXT
    #[arg(long = "fact", value_name = "STATE:TEXT")]
    pub facts: Vec<String>,
    /// A check (repeatable): ok|bad|todo:TEXT
    #[arg(long = "check", value_name = "STATE:TEXT")]
    pub checks: Vec<String>,
    /// Task reference or http(s) URL
    #[arg(long, value_name = "REF")]
    pub task: Option<String>,
    /// Pull request: http(s) URL or owner/repo#123 (repeatable)
    #[arg(long = "pr", value_name = "URL|OWNER/REPO#N")]
    pub pr: Vec<String>,
    /// Another repository to show, DIR[=BASE[..HEAD]] (repeatable, at most 8 with --repo)
    #[arg(long = "diff", value_name = "DIR[=BASE[..HEAD]]")]
    pub diffs: Vec<String>,
    /// A patch to show, [LABEL=]FILE or - for stdin (repeatable, 1 MiB in total)
    #[arg(long = "patch", value_name = "[LABEL=]FILE|-")]
    pub patches: Vec<String>,
    /// Commit: URL or owner/repo@SHA (7 to 40 hex digits) (repeatable)
    #[arg(long = "commit", value_name = "URL|OWNER/REPO@SHA")]
    pub commits: Vec<String>,
    /// Release tag: URL or owner/repo@TAG (repeatable)
    #[arg(long = "tag", value_name = "URL|OWNER/REPO@TAG")]
    pub tags: Vec<String>,
    /// Another link, [LABEL=]URL (repeatable, at most 50 links in all)
    #[arg(long = "link", value_name = "[LABEL=]URL")]
    pub links: Vec<String>,
    /// Update this earlier card in place (its id), for a resend from another worktree
    #[arg(long, value_name = "ID")]
    pub replaces: Option<String>,
    /// Name the question: a resend with the same key (same agent and directory) updates the card
    #[arg(long, value_name = "KEY")]
    pub key: Option<String>,
    /// Wait for the final answer and print it; if the desktop is not running, wait for it
    /// (exit 8 only at --timeout)
    #[arg(long)]
    pub wait: bool,
    /// Longest wait in seconds (default: until answered)
    #[arg(long, value_name = "SECS", requires = "wait")]
    pub timeout: Option<u64>,
}

pub const ASK_HELP: &str = "Several repositories and patches
  --repo and each --diff DIR[=BASE[..HEAD]] add one diff group; the first goes first, so keep
  the order when you resend and the group labels stay stable. A label is the directory name
  (a second one gets -2). Line comments on a card with 2 or more diff groups arrive as
  file = \"<label>:<path>\"; with one group the file is the plain path.
  --patch [LABEL=]FILE or - (stdin, once) shows text you hold, without a repository. Up to 8
  patches and 1 MiB in all; a bigger card is refused, never cut.

Links
  --pr, --commit, --tag take a URL or owner/repo#123, owner/repo@SHA, owner/repo@TAG
  (GitHub). --link takes [LABEL=]URL. Only http and https URLs are accepted, and --pr, --task
  and fact links refuse any other scheme (javascript:, data:, file:); at most 50.

Resending
  --key KEY names one question for good. Sending again with the same key from the same agent
  and directory updates the open card in place (same id, queue position and messages). If the
  owner already answered or dismissed it, nothing is reopened: the output says
  status already_answered and --wait returns that answer at once. A new round of the question
  needs a new key.
  The key is scoped to (agent, directory, key), so a resend from another worktree needs
  --replaces ID with the card_id printed the first time. --replaces wins over --key.
  The printed card_id is the card's own id; use it for later calls.

--wait
  Blocks until the owner answers. If the desktop is not running, the card is queued, a warning
  is printed and the wait continues until the desktop starts; --timeout SECS ends the wait with
  exit 8 and the card stays queued.";
