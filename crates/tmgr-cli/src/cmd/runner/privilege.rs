use std::collections::BTreeMap;
use std::ffi::OsString;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{caps, NoSandbox};
use tmgr_link::privileged;

#[derive(Debug, PartialEq, Eq)]
pub enum Gate {
    Open,
    Typed(&'static str),
}

pub fn gate(
    bypass: bool,
    no_sandbox: bool,
    net_enforced: bool,
    yes: bool,
    in_agent: bool,
    interactive: bool,
) -> Result<Gate> {
    let enforced = net_enforced && !no_sandbox;
    if !no_sandbox && (!bypass || enforced) {
        return Ok(Gate::Open);
    }
    privileged::check(in_agent, interactive)?;
    if no_sandbox && yes {
        return Err(
            Error::new(Kind::Forbidden, "--yes is not accepted with --no-sandbox")
                .hint("run it in a terminal and type the confirmation"),
        );
    }
    super::cloud::bypass_needs_consent(bypass, enforced, yes, interactive)?;
    Ok(Gate::Typed(if no_sandbox {
        "no-sandbox"
    } else {
        "bypass"
    }))
}

pub fn warnings(bypass: bool, no_sandbox: bool) -> Vec<&'static str> {
    let mut v = Vec::new();
    if bypass {
        v.push("warning: bypass mode: the agent runs every command and edit without asking.");
    }
    if no_sandbox {
        v.push("warning: no sandbox: the agent runs with your real home folder, your SSH keys, credentials and environment, with no sandbox and no network limits. Only the runner's control files are protected (macOS).");
    }
    v
}

pub fn confirm(bypass: bool, no_sandbox: bool, net_enforced: bool, yes: bool) -> Result<bool> {
    let in_agent = privileged::RUN_ENV
        .iter()
        .any(|k| std::env::var_os(k).is_some());
    let tty =
        crate::interact::is_interactive() && std::io::IsTerminal::is_terminal(&std::io::stdout());
    match gate(bypass, no_sandbox, net_enforced, yes, in_agent, tty)? {
        Gate::Open => Ok(false),
        Gate::Typed(word) => {
            for line in warnings(bypass, no_sandbox) {
                eprintln!("{line}");
            }
            privileged::confirm_typed_stdio(word, &format!("Type {word} to continue:"))?;
            Ok(true)
        }
    }
}

pub fn check_caps(have: &[String], bypass: bool, no_sandbox: bool) -> Result<()> {
    let missing = |c: &str| !have.iter().any(|h| h == c);
    if (bypass && missing(caps::MANUAL_BYPASS_V1)) || (no_sandbox && missing(caps::NO_SANDBOX_V1)) {
        return Err(Error::new(
            Kind::Conflict,
            "the running runner is older than this tmgr and cannot start this run",
        )
        .hint(format!("{0} runner restart", super::bin())));
    }
    Ok(())
}

pub fn sandbox_env(vars: impl Iterator<Item = (OsString, OsString)>) -> NoSandbox {
    NoSandbox {
        env: vars
            .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
            .filter(|(k, _)| !k.starts_with("TMGR_"))
            .collect::<BTreeMap<_, _>>(),
    }
}

pub fn request(no_sandbox: bool) -> Option<NoSandbox> {
    no_sandbox.then(|| sandbox_env(std::env::vars_os()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(
        bypass: bool,
        ns: bool,
        enforced: bool,
        yes: bool,
        agent: bool,
        tty: bool,
    ) -> Result<Gate> {
        gate(bypass, ns, enforced, yes, agent, tty)
    }

    #[test]
    fn plain_runs_are_open() {
        assert_eq!(
            g(false, false, false, true, true, false).unwrap(),
            Gate::Open
        );
    }

    #[test]
    fn agent_sessions_are_refused() {
        assert_eq!(
            g(true, false, false, false, true, true)
                .unwrap_err()
                .exit_code(),
            5
        );
        assert_eq!(
            g(false, true, true, false, true, true)
                .unwrap_err()
                .exit_code(),
            5
        );
    }

    #[test]
    fn no_terminal_is_refused() {
        assert_eq!(
            g(true, false, false, false, false, false)
                .unwrap_err()
                .exit_code(),
            10
        );
        assert_eq!(
            g(false, true, false, false, false, false)
                .unwrap_err()
                .exit_code(),
            10
        );
    }

    #[test]
    fn yes_is_rejected_for_no_sandbox_and_unrestricted_bypass() {
        let e = g(false, true, true, true, false, true).unwrap_err();
        assert!(e.message.contains("--yes"));
        assert!(g(true, true, true, true, false, true).is_err());
        let e = g(true, false, false, true, false, true).unwrap_err();
        assert!(e.message.contains("--yes"));
    }

    #[test]
    fn cloud_bypass_with_a_restricted_network_keeps_yes() {
        assert_eq!(
            g(true, false, true, true, false, false).unwrap(),
            Gate::Open
        );
        assert_eq!(
            g(true, false, true, false, false, true).unwrap(),
            Gate::Open
        );
    }

    #[test]
    fn typed_words() {
        assert_eq!(
            g(true, false, false, false, false, true).unwrap(),
            Gate::Typed("bypass")
        );
        assert_eq!(
            g(false, true, true, false, false, true).unwrap(),
            Gate::Typed("no-sandbox")
        );
        assert_eq!(
            g(true, true, true, false, false, true).unwrap(),
            Gate::Typed("no-sandbox")
        );
    }

    #[test]
    fn no_sandbox_defeats_the_network_restriction_exemption() {
        assert!(g(true, true, true, false, false, false).is_err());
    }

    #[test]
    fn env_filter_drops_tmgr_and_non_utf8() {
        let vars = vec![
            (OsString::from("PATH"), OsString::from("/bin")),
            (OsString::from("TMGR_RUN_TOKEN"), OsString::from("t")),
            (OsString::from("TMGR_X"), OsString::from("x")),
            (OsString::from("TMGRX"), OsString::from("keep")),
        ];
        let env = sandbox_env(vars.into_iter()).env;
        assert_eq!(env.len(), 2);
        assert!(env.contains_key("PATH") && env.contains_key("TMGRX"));
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let bad = OsString::from_vec(vec![0xff, 0xfe]);
            let env = sandbox_env(vec![(OsString::from("K"), bad)].into_iter()).env;
            assert!(env.is_empty());
        }
    }

    #[test]
    fn caps_are_checked_per_feature() {
        let none: Vec<String> = Vec::new();
        assert!(check_caps(&none, false, false).is_ok());
        assert!(check_caps(&none, true, false).is_err());
        assert!(check_caps(&none, false, true).is_err());
        let bypass = vec![caps::MANUAL_BYPASS_V1.to_string()];
        assert!(check_caps(&bypass, true, false).is_ok());
        assert!(check_caps(&bypass, true, true).is_err());
        let both = vec![
            caps::MANUAL_BYPASS_V1.to_string(),
            caps::NO_SANDBOX_V1.to_string(),
        ];
        assert!(check_caps(&both, true, true).is_ok());
    }

    #[test]
    fn warnings_name_the_risks() {
        assert!(warnings(true, false)[0].contains("without asking"));
        assert!(warnings(false, true)[0].contains("SSH keys"));
        assert!(warnings(false, false).is_empty());
    }

    mod parsing {
        use crate::cli::{Cli, Command};
        use crate::cmd::runner::{PermissionMode, RunArgs};
        use clap::Parser;

        fn parse(args: &[&str]) -> Result<Box<RunArgs>, clap::Error> {
            let cli = Cli::try_parse_from(std::iter::once("tmgr").chain(args.iter().copied()))?;
            match cli.command {
                Some(Command::Runner(crate::cmd::runner::RunnerCommands::Run(a))) => {
                    Ok(Box::new(a))
                }
                _ => panic!("not run"),
            }
        }

        #[test]
        fn bypass_is_accepted_for_an_agent_without_a_task_and_for_a_task() {
            let a = parse(&["run", "--agent", "x", "--mode", "bypass"]).unwrap();
            assert!(a.task.is_none() && a.mode == Some(PermissionMode::Bypass));
            assert!(parse(&["run", "TM-1", "--agent", "x", "--mode", "bypass"]).is_ok());
        }

        #[test]
        fn mode_conflicts_with_agent_cmd_and_on() {
            assert!(parse(&["run", "--agent-cmd", "ls", "--mode", "bypass"]).is_err());
            assert!(parse(&["run", "TM-1", "--agent", "x", "--on", "l", "--mode", "ask"]).is_err());
        }

        #[test]
        fn no_sandbox_needs_an_agent_and_rejects_other_sources() {
            assert!(
                parse(&["run", "--agent", "x", "--no-sandbox"])
                    .unwrap()
                    .no_sandbox
            );
            assert!(parse(&[
                "run",
                "TM-1",
                "--agent",
                "x",
                "--no-sandbox",
                "--mode",
                "bypass"
            ])
            .is_ok());
            assert!(parse(&["run", "--no-sandbox"]).is_err());
            assert!(parse(&["run", "--agent-cmd", "ls", "--no-sandbox"]).is_err());
            assert!(parse(&["run", "TM-1", "--agent", "x", "--on", "l", "--no-sandbox"]).is_err());
            assert!(parse(&["run", "--agent", "x", "--no-sandbox", "--trust"]).is_err());
        }
    }
}
