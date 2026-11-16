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
        /// Session id (or a unique start of it), or a path to a session file in the Claude or Codex folders
        id: String,
        /// Show every turn
        #[arg(long)]
        full: bool,
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
    Files {
        id: String,
        /// Show every file
        #[arg(long)]
        full: bool,
    },
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
        SessionCommand::Turns { id, full } => print(&render::turns(&open(id)?, *full)),
        SessionCommand::Show { id, turn, full } => print(&render::show(&open(id)?, *turn, *full)?),
        SessionCommand::Grep { id, pattern, full } => {
            print(&render::grep(&open(id)?, pattern, *full)?)
        }
        SessionCommand::Files { id, full } => print(&render::files(&open(id)?, *full)),
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
    let fail = |e: std::io::Error| format!("cannot write {}: {e}", path.display());
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(not(unix))]
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(format!("{} is a symlink", path.display()));
    }
    let mut file = opts.open(path).map_err(fail)?;
    if !file.metadata().map_err(fail)?.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(fail)?;
    }
    file.set_len(0).map_err(fail)?;
    file.write_all(text.as_bytes()).map_err(fail)
}
