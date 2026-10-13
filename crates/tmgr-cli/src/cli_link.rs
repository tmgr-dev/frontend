use clap::{Args, Subcommand};

#[derive(Subcommand)]
pub enum LinkCommand {
    /// Create this machine's link identity (leader side, interactive)
    Init(InitArgs),
    /// Pin a follower's SSH host key and add it (leader side, interactive)
    Add(AddArgs),
    /// Trust a leader's public key on this machine (follower side, interactive)
    Trust(TrustArgs),
    /// Leaders of this machine, followers of this machine, and the own identity
    List,
    /// Show or change what a leader may do (widening needs a terminal)
    Scope(ScopeArgs),
    /// Remove a leader's access immediately (always allowed)
    Revoke { name: String },
    /// Forget a follower (leader side)
    Remove { name: String },
    /// Connect to a follower and run the handshake
    Test { name: String },
    /// Show the local link audit log, or verify its hash chain
    Audit(AuditArgs),
}

#[derive(Args)]
pub struct InitArgs {
    /// Replace an existing identity; every follower must then trust the new key
    #[arg(long)]
    pub rotate: bool,
}

#[derive(Args)]
pub struct AddArgs {
    /// Link name: a-z, 0-9 and -, at most 32
    pub name: String,
    /// user@host of the follower
    #[arg(long, value_name = "USER@HOST")]
    pub ssh: String,
    #[arg(long, default_value_t = 22, value_name = "N")]
    pub port: u32,
    /// Reach the follower through another pinned link (not supported yet)
    #[arg(long, value_name = "LINK")]
    pub jump: Option<String>,
}

#[derive(Args)]
pub struct TrustArgs {
    /// The leader's public key: ssh-ed25519 <base64> [comment]
    #[arg(required = true, num_args = 1..=3, value_name = "PUBKEY")]
    pub pubkey: Vec<String>,
    #[arg(long = "as", required = true, value_parser = ["leader"], value_name = "ROLE")]
    pub role: String,
    #[arg(long, required = true, value_name = "NAME")]
    pub name: String,
    /// Existing absolute directories the leader may run in
    #[arg(long, num_args = 1.., value_delimiter = ',', value_name = "DIR")]
    pub repos: Vec<String>,
    /// Agents the leader may start, e.g. claude-code,codex
    #[arg(long, num_args = 1.., value_delimiter = ',', value_name = "AGENT")]
    pub agents: Vec<String>,
    /// Models the leader may ask for (default: any)
    #[arg(long, num_args = 1.., value_delimiter = ',', value_name = "MODEL")]
    pub models: Vec<String>,
    /// Concurrent runs the leader may have (default 1)
    #[arg(long, value_name = "N")]
    pub max: Option<u32>,
    /// Only accept connections from these CIDRs
    #[arg(long, num_args = 1.., value_delimiter = ',', value_name = "CIDR")]
    pub from: Vec<String>,
}

#[derive(Args)]
pub struct ScopeArgs {
    pub name: String,
    #[arg(long, num_args = 1.., value_delimiter = ',', value_name = "DIR")]
    pub repos: Option<Vec<String>>,
    #[arg(long, num_args = 1.., value_delimiter = ',', value_name = "AGENT")]
    pub agents: Option<Vec<String>>,
    #[arg(long, num_args = 1.., value_delimiter = ',', value_name = "MODEL")]
    pub models: Option<Vec<String>>,
    #[arg(long, value_name = "N")]
    pub max: Option<u32>,
    /// CIDRs, or `any` to accept every address
    #[arg(long, num_args = 1.., value_delimiter = ',', value_name = "CIDR")]
    pub from: Option<Vec<String>>,
}

#[derive(Args)]
pub struct AuditArgs {
    /// Number of entries to show
    #[arg(long, default_value_t = 20, value_name = "N")]
    pub tail: usize,
    /// Check the hash chain instead of listing entries
    #[arg(long)]
    pub verify: bool,
}

#[derive(Args)]
pub struct WorkerArgs {
    /// Link name (the only accepted form is: tmgr worker --link NAME --enrollment ID)
    #[arg(long, value_name = "NAME")]
    pub link: String,
    #[arg(long, value_name = "ID")]
    pub enrollment: String,
}
