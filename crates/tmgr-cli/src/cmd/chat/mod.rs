#![allow(dead_code)]

pub mod join;
pub mod join_code;
pub mod status;
pub mod transport;

use crate::ctx::Ctx;
use crate::out::Output;
use clap::{Args, Subcommand};
use tmgr_core::error::Result;

#[derive(Subcommand)]
pub enum ChatCommand {
    /// Pair this runner with a trusted device; the join code is read from stdin or a prompt
    Join(JoinArgs),
    /// Show whether this runner is paired with the chat relay
    Status,
}

#[derive(Args)]
pub struct JoinArgs {
    #[arg(hide = true, value_parser = join_code::reject_argv_code)]
    code: Option<String>,
}

pub fn run(ctx: &Ctx, cmd: &ChatCommand) -> Result<Output> {
    match cmd {
        ChatCommand::Join(_) => join::run(ctx),
        ChatCommand::Status => status::run(),
    }
}
