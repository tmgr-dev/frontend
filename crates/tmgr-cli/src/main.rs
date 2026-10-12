use clap::{Parser, Subcommand};
use tmgr_core::{Envelope, VersionInfo};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = "tmgr", version = VERSION, about = "TMGR command line tool")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print version, commit and build target
    Version {
        /// Machine-readable output
        #[arg(long)]
        json: bool,
    },
}

fn version_info() -> VersionInfo {
    VersionInfo {
        version: VERSION,
        commit: env!("TMGR_COMMIT"),
        target: env!("TMGR_TARGET"),
    }
}

fn main() {
    match Cli::parse().command {
        Command::Version { json } => {
            let info = version_info();
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&Envelope::ok(info)).expect("version info serializes")
                );
            } else {
                println!("tmgr {} ({}, {})", info.version, info.commit, info.target);
            }
        }
    }
}
