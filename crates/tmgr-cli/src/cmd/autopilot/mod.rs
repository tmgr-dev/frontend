pub mod control;
pub mod interview;
pub mod model;
pub mod render;
pub mod serve;
pub mod start;
pub mod store;
pub mod usage;

use clap::{Args, Subcommand};
use tmgr_core::error::Result;
use tmgr_ipc::api::AutopilotCommand;

use crate::cli::Cli;
use crate::cmd::queue::not_in_agent_session;
use crate::ctx::Ctx;
use crate::out::Output;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct AutopilotArgs {
    #[command(subcommand)]
    pub command: Option<AutopilotCmd>,
    #[command(flatten)]
    pub start: StartArgs,
}

#[derive(Subcommand)]
pub enum AutopilotCmd {
    /// Interview, one typed agreement, then the runner works through the backlog in the background
    Start(Box<StartArgs>),
    /// Show what autopilot is doing
    Status {
        /// Refresh every 2 seconds until it stops
        #[arg(long)]
        watch: bool,
    },
    /// Stop starting new tasks; running ones finish
    Pause,
    /// Continue after a pause (needs a normal terminal)
    Resume,
    /// Stop autopilot and turn the queue off
    Stop,
    /// Show the last log lines
    Log {
        #[arg(long, default_value_t = control::DEFAULT_TAIL, value_name = "N")]
        tail: u32,
    },
    #[command(hide = true)]
    Serve {
        #[arg(long)]
        settings: String,
    },
}

#[derive(Args, Default)]
pub struct StartArgs {
    /// Rule to work through (default: the only cloud rule, or the remembered one)
    #[arg(long, value_name = "NAME")]
    pub rule: Option<String>,
    /// all, or task keys separated by commas
    #[arg(long, value_name = "all|KEYS")]
    pub tasks: Option<String>,
    /// Agents at the same time, at most the rule's limit
    #[arg(long, value_name = "N")]
    pub max_concurrent: Option<String>,
    /// Stop starting new tasks at this usage percent
    #[arg(long, value_name = "PCT")]
    pub usage_soft: Option<String>,
    /// Pause running agents at this usage percent
    #[arg(long, value_name = "PCT")]
    pub usage_hard: Option<String>,
    /// Run without the usage guard
    #[arg(long)]
    pub no_usage_limit: bool,
    /// Continue after the usage window resets (yes or no)
    #[arg(long, value_name = "yes|no")]
    pub resume_after_reset: Option<String>,
    /// Most to spend in USD on runs paid by API key, or none
    #[arg(long, value_name = "USD|none")]
    pub max_cost: Option<String>,
    /// Most hours to run, or none
    #[arg(long, value_name = "HOURS|none")]
    pub max_hours: Option<String>,
    /// Most tasks to start, or none
    #[arg(long, value_name = "N|none")]
    pub max_tasks: Option<String>,
}

impl StartArgs {
    pub fn flags(&self) -> interview::Flags {
        let mut flags = interview::Flags::new();
        let mut put = |id: &'static str, v: &Option<String>| {
            if let Some(v) = v {
                flags.insert(id, v.clone());
            }
        };
        put("tasks", &self.tasks);
        put("max-concurrent", &self.max_concurrent);
        put("usage-soft", &self.usage_soft);
        put("usage-hard", &self.usage_hard);
        put("resume-after-reset", &self.resume_after_reset);
        put("max-cost", &self.max_cost);
        put("max-hours", &self.max_hours);
        put("max-tasks", &self.max_tasks);
        flags
    }
}

pub fn run(ctx: &Ctx, cli: &Cli, args: &AutopilotArgs) -> Result<Output> {
    match &args.command {
        None => start::run(ctx, cli, &args.start),
        Some(AutopilotCmd::Start(a)) => start::run(ctx, cli, a),
        Some(AutopilotCmd::Status { watch }) => control::status(*watch, cli.json),
        Some(AutopilotCmd::Pause) => control::control(AutopilotCommand::Pause),
        Some(AutopilotCmd::Resume) => {
            not_in_agent_session()?;
            control::control(AutopilotCommand::Resume)
        }
        Some(AutopilotCmd::Stop) => control::control(AutopilotCommand::Stop),
        Some(AutopilotCmd::Log { tail }) => control::log(*tail),
        Some(AutopilotCmd::Serve { settings }) => serve::real::run(ctx, cli, settings),
    }
}
