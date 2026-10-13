mod cli;
mod cli_agent;
mod cmd;
mod ctx;
mod gitx;
mod interact;
mod out;
mod usage;

use clap::error::ErrorKind;
use clap::Parser;
use cli::{AuthCommand, Cli, Command, GitCommand, HookCommand, VERSION};
use cli_agent::HookCmd;
use ctx::Ctx;
use out::Output;
use serde_json::json;
use tmgr_core::error::{Error, Result};
use tmgr_core::VersionInfo;

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => return usage_exit(e),
    };
    if let Some(code) = standalone(&cli) {
        return code;
    }
    let ctx = match Ctx::new(&cli) {
        Ok(ctx) => ctx,
        Err(e) => {
            return out::print_error(cli.json, cli.verbose, out::stderr_style(cli.no_color), &e)
        }
    };
    if let Some(code) = cmd::log::follow_entry(&ctx, &cli) {
        return code;
    }
    match dispatch(&ctx, &cli) {
        Ok(output) => out::emit(&ctx, output),
        Err(e) => out::print_error(ctx.json, ctx.verbose, ctx.err_style, &e),
    }
}

fn standalone(cli: &Cli) -> Option<i32> {
    match &cli.command {
        Some(Command::Prompt(args)) => Some(cmd::prompt::prompt(args, cli.no_color)),
        Some(Command::Statusline { target }) => Some(cmd::prompt::statusline(*target)),
        Some(Command::Git {
            command:
                GitCommand::Hook {
                    command: HookCommand::CommitMsg { file },
                },
        }) => Some(cmd::githook::commit_msg(file)),
        Some(Command::Hook {
            command: HookCmd::Claude { event, .. },
        }) => Some(cmd::hook::claude(event, cli.server.clone())),
        Some(Command::Hook {
            command: HookCmd::Codex { args },
        }) => Some(cmd::hook::codex(args, cli.server.clone())),
        Some(Command::Mcp) => Some(cmd::mcp::run(cli.server.clone(), cli.verbose)),
        _ => None,
    }
}

fn is_agent_hook_invocation() -> bool {
    let mut args = std::env::args().skip(1).filter(|a| !a.starts_with('-'));
    args.next().as_deref() == Some("hook")
        && matches!(args.next().as_deref(), Some("claude" | "codex"))
}

fn usage_exit(e: clap::Error) -> i32 {
    match e.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
            let _ = e.print();
            0
        }
        _ if is_agent_hook_invocation() => 0,
        _ => {
            if e.kind() == ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand {
                let _ = e.print();
                return 2;
            }
            let argv: Vec<String> = std::env::args().skip(1).collect();
            let json = argv.iter().any(|a| a == "--json");
            let no_color = argv.iter().any(|a| a == "--no-color");
            let err = usage::friendly(&e);
            let style = if json {
                out::Style::off()
            } else {
                out::stderr_style(no_color)
            };
            out::print_error(json, false, style, &err)
        }
    }
}

fn dispatch(ctx: &Ctx, cli: &Cli) -> Result<Output> {
    match &cli.command {
        None | Some(Command::Status) => cmd::status::run(ctx),
        Some(Command::Version) => Ok(version(ctx)),
        Some(Command::Login(args))
        | Some(Command::Auth {
            command: AuthCommand::Login(args),
        }) => cmd::auth::login(ctx, args),
        Some(Command::Logout)
        | Some(Command::Auth {
            command: AuthCommand::Logout,
        }) => cmd::auth::logout(ctx),
        Some(Command::Whoami)
        | Some(Command::Auth {
            command: AuthCommand::Status,
        }) => cmd::auth::whoami(ctx),
        Some(Command::Ws { command }) => cmd::ws::run(ctx, command),
        Some(Command::Task { command }) => cmd::task::run(ctx, command),
        Some(Command::Add(args)) => cmd::add::run(ctx, args),
        Some(Command::Config { command }) => cmd::config::run(ctx, command),
        Some(Command::Start(args)) => cmd::timer::start(ctx, args),
        Some(Command::Stop) => cmd::timer::stop(ctx),
        Some(Command::Switch(args)) => cmd::timer::switch(ctx, args),
        Some(Command::Done(args)) => cmd::timer::done(ctx, args),
        Some(Command::Project { command }) => cmd::project::run(ctx, command),
        Some(Command::Git { command }) => cmd::githook::run(ctx, command),
        Some(Command::Init(args)) => cmd::init::run(ctx, args),
        Some(Command::Agent { command }) => cmd::agent::run(ctx, command),
        Some(Command::Hook { command }) => match command {
            HookCmd::Install(a) => cmd::hookinstall::run(ctx, a, false),
            HookCmd::Uninstall(a) => cmd::hookinstall::run(ctx, a, true),
            HookCmd::Claude { .. } | HookCmd::Codex { .. } => {
                Err(Error::internal("handled before the session starts"))
            }
        },
        Some(Command::Notify(args)) => cmd::notify::notify(ctx, args),
        Some(Command::Alarm(args)) => cmd::notify::alarm(ctx, args),
        Some(Command::Mcp) => Err(Error::internal("handled before the session starts")),
        Some(Command::Prompt(_)) | Some(Command::Statusline { .. }) => {
            Err(Error::internal("handled before the session starts"))
        }
        Some(Command::Log(args)) => cmd::log::run(ctx, args),
        Some(Command::Today(args)) => cmd::today::run(ctx, args),
        Some(Command::Routine { command }) => cmd::routine::run(ctx, command),
        Some(Command::Note(args)) => cmd::note::run(ctx, args),
        Some(Command::Page { command }) => cmd::page::run(ctx, command),
        Some(Command::Board(args)) => cmd::board::run(ctx, args),
        Some(Command::SelfUpdate(args)) => cmd::self_update::run(ctx, args),
        Some(Command::Refs) => Ok(Output::new(
            json!({"help": "refs"}),
            cli::REFS_HELP.to_string(),
        )),
    }
}

fn version(_ctx: &Ctx) -> Output {
    let info = VersionInfo {
        version: VERSION,
        commit: env!("TMGR_COMMIT"),
        target: env!("TMGR_TARGET"),
    };
    let human = format!("tmgr {} ({}, {})", info.version, info.commit, info.target);
    Output::new(info, human)
}
