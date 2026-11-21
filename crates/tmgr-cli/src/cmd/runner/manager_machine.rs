use super::{connect, emit, rerr, runner_config, Opts};
use clap::Subcommand;
use serde_json::json;
use tmgr_core::error::{Error, Result};
use tmgr_runner::mgr_rpc::pairing::{add_machine, remove_machine, show_line};

#[derive(Subcommand)]
pub enum MachineCmd {
    /// [experimental] Print this machine's pairing line
    Show,
    /// [experimental] Pair another machine from the line its `show` printed
    Add {
        /// The JSON line printed by `show` on the other machine
        line: String,
        /// Name for the machine (1 to 64 characters)
        #[arg(long)]
        label: String,
    },
    /// [experimental] Forget a paired machine
    Remove {
        /// Machine id (mx_...)
        id: String,
    },
}

pub(super) fn run(o: &Opts, cmd: &MachineCmd) -> Result<i32> {
    match cmd {
        MachineCmd::Show => show(o),
        MachineCmd::Add { line, label } => {
            let cfg = runner_config()?;
            add_machine(&cfg.dir, line, label).map_err(Error::usage)?;
            Ok(emit(o, json!({"added": true}), "machine paired"))
        }
        MachineCmd::Remove { id } => {
            let cfg = runner_config()?;
            remove_machine(&cfg.dir, id).map_err(Error::usage)?;
            Ok(emit(o, json!({"removed": true}), "machine removed"))
        }
    }
}

fn show(o: &Opts) -> Result<i32> {
    let cfg = runner_config()?;
    let mut c = connect(&cfg)?;
    let me = c.chat_relay_self().map_err(rerr)?;
    let hint = hint(&super::bin());
    Ok(emit(
        o,
        json!({"id": me.id, "device_id": me.device_id, "fp": me.fp, "hint": hint}),
        format!("{}\n\n{hint}", show_line(&me)),
    ))
}

fn hint(bin: &str) -> String {
    format!(
        "Pairing is needed on both machines. On the other machine run:\n  {bin} runner manager machine add '<the line above>' --label <name>\nOn the machine that serves requests, also put the caller's machine id in accept_from in manager-remote.toml (and list any extra ops in allow)."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct W {
        #[command(subcommand)]
        c: MachineCmd,
    }

    fn parse(args: &[&str]) -> std::result::Result<MachineCmd, clap::Error> {
        W::try_parse_from(std::iter::once("t").chain(args.iter().copied())).map(|w| w.c)
    }

    #[test]
    fn add_needs_a_line_and_a_label() {
        assert!(parse(&["add", "{}"]).is_err());
        assert!(parse(&["add", "--label", "x"]).is_err());
        assert!(matches!(
            parse(&["add", "{}", "--label", "x"]).unwrap(),
            MachineCmd::Add { .. }
        ));
        assert!(matches!(
            parse(&["remove", "mx_0123456789abcdef"]).unwrap(),
            MachineCmd::Remove { .. }
        ));
        assert!(matches!(parse(&["show"]).unwrap(), MachineCmd::Show));
    }

    #[test]
    fn the_hint_names_both_machines_and_the_policy_file() {
        let h = hint("tmgr");
        assert!(h.contains("both machines"));
        assert!(h.contains("accept_from"));
        assert!(h.contains("manager-remote.toml"));
        assert!(h.contains("machine add"));
    }
}
