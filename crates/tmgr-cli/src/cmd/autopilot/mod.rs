pub mod model;
pub mod serve;
pub mod store;

use crate::cli::Cli;
use crate::ctx::Ctx;
use crate::out::Output;
use clap::Subcommand;
use tmgr_core::error::Result;

#[derive(Subcommand)]
pub enum AutopilotCmd {
    #[command(hide = true)]
    Serve {
        #[arg(long)]
        settings: String,
    },
}

pub fn run(ctx: &Ctx, cli: &Cli, cmd: &AutopilotCmd) -> Result<Output> {
    match cmd {
        AutopilotCmd::Serve { settings } => serve::real::run(ctx, cli, settings),
    }
}
