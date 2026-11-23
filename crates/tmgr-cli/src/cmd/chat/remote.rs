use crate::cmd::runner::{connect, rerr, runner_config};
use crate::out::Output;
use clap::{Subcommand, ValueEnum};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::RemotePolicyInfo;

const IN_SESSION: &str = "Refused: TMGR_SESSION_ID is set, run this from your own terminal";

#[derive(Subcommand)]
pub enum RemoteCommand {
    /// Let a leader machine ask this runner's manager (on) or limit it to listing and reading (off)
    AllowAsk {
        #[arg(value_parser = leader_id)]
        leader: String,
        state: Toggle,
    },
    /// Show what each leader machine may do on this runner
    Policy,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
pub enum Toggle {
    On,
    Off,
}

fn leader_id(s: &str) -> std::result::Result<String, String> {
    if tmgr_ipc::mgr_rpc::valid_machine_id(s) {
        Ok(s.to_string())
    } else {
        Err("not a valid machine id".into())
    }
}

fn guard(session_id_set: bool) -> Result<()> {
    if session_id_set {
        return Err(Error::new(Kind::Forbidden, IN_SESSION));
    }
    Ok(())
}

pub fn render(info: &RemotePolicyInfo) -> Output {
    let mut human = String::new();
    if info.leaders.is_empty() {
        human.push_str("No leader machines are enrolled.");
    }
    for l in &info.leaders {
        let ops: Vec<&str> = l.ops.iter().map(|o| o.as_str()).collect();
        human.push_str(&format!(
            "{}: asks {}, ops: {}\n",
            l.machine,
            if l.ask { "allowed" } else { "off" },
            ops.join(", ")
        ));
    }
    if info.legacy {
        human.push_str("Legacy policy file: every enrolled leader keeps its old rights.\n");
    }
    Output::new(info, human.trim_end())
}

pub fn run(cmd: &RemoteCommand) -> Result<Output> {
    guard(std::env::var_os("TMGR_SESSION_ID").is_some())?;
    let cfg = runner_config()?;
    let mut client = connect(&cfg)?;
    let info = match cmd {
        RemoteCommand::AllowAsk { leader, state } => client
            .remote_policy_set(leader, *state == Toggle::On)
            .map_err(rerr)?,
        RemoteCommand::Policy => client.remote_policy().map_err(rerr)?,
    };
    Ok(render(&info))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Command};
    use clap::Parser;
    use tmgr_ipc::api::RemotePolicyLeader;
    use tmgr_ipc::mgr_rpc::Op;

    fn parse(args: &[&str]) -> std::result::Result<RemoteCommand, clap::Error> {
        let cli = Cli::try_parse_from(["tmgr", "chat", "remote"].iter().chain(args))?;
        match cli.command {
            Some(Command::Chat {
                command: super::super::ChatCommand::Remote { command },
            }) => Ok(command),
            _ => panic!("not a chat remote command"),
        }
    }

    #[test]
    fn allow_ask_takes_a_leader_and_on_or_off() {
        match parse(&["allow-ask", "mx_0123456789abcdef", "on"]).unwrap() {
            RemoteCommand::AllowAsk { leader, state } => {
                assert_eq!(leader, "mx_0123456789abcdef");
                assert_eq!(state, Toggle::On);
            }
            _ => panic!("wrong command"),
        }
        match parse(&["allow-ask", "mx_0123456789abcdef", "off"]).unwrap() {
            RemoteCommand::AllowAsk { state, .. } => assert_eq!(state, Toggle::Off),
            _ => panic!("wrong command"),
        }
        assert!(matches!(
            parse(&["policy"]).unwrap(),
            RemoteCommand::Policy
        ));
    }

    #[test]
    fn allow_ask_rejects_a_bad_state_and_a_bad_leader() {
        assert!(parse(&["allow-ask", "mx_0123456789abcdef", "maybe"]).is_err());
        assert!(parse(&["allow-ask", "mx_0123456789abcdef"]).is_err());
        assert!(parse(&["allow-ask", "bad:id", "on"]).is_err());
    }

    #[test]
    fn refuses_when_the_session_id_is_set() {
        let e = guard(true).unwrap_err();
        assert_eq!(e.kind, Kind::Forbidden);
        assert_eq!(e.message, IN_SESSION);
        assert!(guard(false).is_ok());
    }

    #[test]
    fn policy_output_lists_ops_and_legacy() {
        let info = RemotePolicyInfo {
            leaders: vec![RemotePolicyLeader {
                machine: "mac-1".into(),
                ask: false,
                ops: vec![Op::List, Op::Room],
            }],
            legacy: true,
        };
        let out = render(&info);
        assert!(out.human.contains("mac-1: asks off, ops: list, room"));
        assert!(out.human.contains("Legacy"));
        assert_eq!(out.data["leaders"][0]["machine"], "mac-1");
    }
}
