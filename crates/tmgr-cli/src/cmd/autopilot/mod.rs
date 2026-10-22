pub mod control;
pub mod interview;
pub mod model;
pub mod orchestrate;
pub mod queue_loop;
pub mod render;
pub mod serve;
pub mod start;
pub mod store;
pub mod tail;
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
    /// Workspace whose backlog to work through (its cloud queue rule is used)
    #[arg(value_name = "WORKSPACE")]
    pub workspace: Option<String>,
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
    /// Stop autopilot (the queue goes off, or back to its background loop if that was running)
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
    /// Teams at the same time, at most the rule's limit
    #[arg(long, value_name = "N")]
    pub max_teams: Option<String>,
    #[arg(long, value_name = "N", hide = true, conflicts_with = "max_teams")]
    pub max_concurrent: Option<String>,
    /// Agents per team, lead and workers (1 to 5)
    #[arg(long, value_name = "N")]
    pub team_size: Option<String>,
    /// Team mode: eco, heavy, or orchestrator (the orchestrator chooses per task)
    #[arg(long, value_name = "eco|heavy|orchestrator")]
    pub team_mode: Option<String>,
    /// Agent profile for the teams: claude-code or codex (default: the rule's profile)
    #[arg(long, value_name = "NAME")]
    pub profile: Option<String>,
    /// Work only on these task keys (within --tasks)
    #[arg(long, value_name = "KEYS")]
    pub only: Option<String>,
    /// Leave these task keys out
    #[arg(long, value_name = "KEYS")]
    pub skip: Option<String>,
    /// Start these task keys first, in this order
    #[arg(long, value_name = "KEYS")]
    pub order: Option<String>,
    /// Plan and triage only: nothing is started, moved or commented
    #[arg(long)]
    pub dry_run: bool,
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
    /// Hours after which autopilot stops starting tasks when no real usage number is found (default 8)
    #[arg(long, value_name = "HOURS")]
    pub fallback_hours: Option<String>,
    /// Only tasks that carry the rule's persona
    #[arg(long)]
    pub with_persona: bool,
    /// Ask the setup questions again
    #[arg(long)]
    pub setup: bool,
    /// Show the full text of every task before signing
    #[arg(long)]
    pub review: bool,
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
        put(
            "max-teams",
            if self.max_teams.is_some() {
                &self.max_teams
            } else {
                &self.max_concurrent
            },
        );
        put("team-size", &self.team_size);
        put("team-mode", &self.team_mode);
        put("profile", &self.profile);
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
        None => start::run(ctx, cli, &args.start, args.workspace.as_deref()),
        Some(AutopilotCmd::Start(a)) => start::run(ctx, cli, a, None),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Command};
    use clap::{CommandFactory, Parser};

    #[test]
    fn the_command_tree_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn top_level_help_lists_autopilot() {
        let mut cmd = Cli::command();
        let help = cmd.render_help().to_string();
        assert!(
            help.contains("autopilot")
                && help.contains(
                    "Work through a workspace's backlog with agent teams: one list, one typed confirmation"
                ),
            "{help}"
        );
        let sub = cmd.find_subcommand_mut("autopilot").unwrap();
        let help = sub.render_help().to_string();
        for needle in [
            "[WORKSPACE]",
            "--setup",
            "Ask the setup questions again",
            "--review",
            "Show the full text of every task before signing",
        ] {
            assert!(help.contains(needle), "missing {needle:?} in\n{help}");
        }
    }

    fn parse(args: &[&str]) -> Box<AutopilotArgs> {
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Some(Command::Autopilot(a)) => a,
            Some(Command::Queue {
                command: crate::cmd::queue::QueueCommand::Autopilot(a),
            }) => a,
            _ => panic!("not autopilot"),
        }
    }

    #[test]
    fn a_workspace_flags_and_subcommands_parse_on_both_entry_points() {
        let a = parse(&[
            "tmgr",
            "autopilot",
            "tmgrdev",
            "--setup",
            "--review",
            "--dry-run",
        ]);
        assert_eq!(a.workspace.as_deref(), Some("tmgrdev"));
        assert!(a.command.is_none() && a.start.setup && a.start.review && a.start.dry_run);
        let a = parse(&["tmgr", "autopilot", "status", "--watch"]);
        assert!(a.workspace.is_none());
        assert!(matches!(
            a.command,
            Some(AutopilotCmd::Status { watch: true })
        ));
        let a = parse(&["tmgr", "autopilot", "stop"]);
        assert!(matches!(a.command, Some(AutopilotCmd::Stop)));
        let a = parse(&["tmgr", "queue", "autopilot", "--setup"]);
        assert!(a.workspace.is_none() && a.start.setup);
        let a = parse(&["tmgr", "queue", "autopilot", "start", "--rule", "x"]);
        assert!(matches!(a.command, Some(AutopilotCmd::Start(_))));
    }
}
