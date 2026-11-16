use clap::{Subcommand, ValueEnum};
use std::io::Write;
use std::path::{Path, PathBuf};
use tmgr_session::handoff::{build, Opts};
use tmgr_session::locate::{find, load, Roots};
use tmgr_session::{git, render, Agent, Session};
use tmgr_usage::paths::Env;

#[derive(Clone, Copy, ValueEnum)]
pub enum Target {
    Claude,
    Codex,
}

#[derive(Subcommand)]
pub enum SessionCommand {
    /// Numbered list of turns, one line each
    Turns {
        /// Session id (or a unique start of it), or a path to a session file
        id: String,
    },
    /// One turn with its tool calls and trimmed results
    Show {
        id: String,
        /// Turn number from `tmgr session turns`
        #[arg(long, value_name = "N")]
        turn: usize,
        /// Do not trim long text and results
        #[arg(long)]
        full: bool,
    },
    /// Find a pattern (case-insensitive) in messages, commands and results
    Grep {
        id: String,
        pattern: String,
        /// Show every match
        #[arg(long)]
        full: bool,
    },
    /// Files read and edited, per turn
    Files { id: String },
    /// Failed tool calls and their output
    Errors {
        id: String,
        /// Do not trim long results
        #[arg(long)]
        full: bool,
    },
    /// Write a handoff note for the other agent
    Handoff {
        id: String,
        /// Agent that takes over (default: the other one)
        #[arg(long, value_enum)]
        to: Option<Target>,
        /// TMGR task reference to put in the note
        #[arg(long)]
        task: Option<String>,
        /// Id of an earlier session in the chain (repeatable, oldest first)
        #[arg(long = "previous", value_name = "ID")]
        previous: Vec<String>,
        /// Write the note to a file (mode 600) instead of stdout
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
}

pub fn run(cmd: &SessionCommand) -> i32 {
    match exec(cmd) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn open(id: &str) -> Result<Session, String> {
    let roots = Roots::from_env(&Env::from_process());
    let (agent, path) = find(&roots, id)?;
    load(agent, &path)
}

fn print(text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    if !text.ends_with('\n') {
        let _ = out.write_all(b"\n");
    }
}

fn exec(cmd: &SessionCommand) -> Result<(), String> {
    match cmd {
        SessionCommand::Turns { id } => print(&render::turns(&open(id)?)),
        SessionCommand::Show { id, turn, full } => print(&render::show(&open(id)?, *turn, *full)?),
        SessionCommand::Grep { id, pattern, full } => {
            print(&render::grep(&open(id)?, pattern, *full)?)
        }
        SessionCommand::Files { id } => print(&render::files(&open(id)?)),
        SessionCommand::Errors { id, full } => print(&render::errors(&open(id)?, *full)),
        SessionCommand::Handoff {
            id,
            to,
            task,
            previous,
            output,
        } => {
            let s = open(id)?;
            let target = match to {
                Some(Target::Claude) => Agent::Claude,
                Some(Target::Codex) => Agent::Codex,
                None => match s.agent {
                    Agent::Claude => Agent::Codex,
                    Agent::Codex => Agent::Claude,
                },
            };
            let cwd = s.cwd.as_deref().map(Path::new);
            let mut rules_files: Vec<String> = [target.rules_file(), s.agent.rules_file()]
                .iter()
                .filter(|f| cwd.is_some_and(|d| d.join(f).is_file()))
                .map(|f| (*f).to_string())
                .collect();
            rules_files.dedup();
            let note = build(
                &s,
                &Opts {
                    target,
                    task: task.clone(),
                    previous: previous.clone(),
                    git: cwd.and_then(git::collect),
                    rules_files,
                },
            );
            match output {
                Some(path) => {
                    write_private(path, &note)?;
                    println!("handoff note written to {}", path.display());
                }
                None => print(&note),
            }
        }
    }
    Ok(())
}

fn write_private(path: &Path, text: &str) -> Result<(), String> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
        .and_then(|mut f| f.write_all(text.as_bytes()))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}
