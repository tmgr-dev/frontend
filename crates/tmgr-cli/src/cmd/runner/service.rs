use super::{emit, runner_config, Opts};
use serde_json::json;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use tmgr_core::error::{Error, Result};
use tmgr_link::privileged;
use tmgr_runner::service::{self, Exists, Plan, Platform, Removed, RunOut, ServicePaths, Sys};

const CONFIRM: &str = "yes, install service";

struct RealSys;

impl Sys for RealSys {
    fn run(&mut self, argv: &[String]) -> io::Result<RunOut> {
        let (prog, args) = argv
            .split_first()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty command"))?;
        if !prog.starts_with('/') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "service commands run by absolute path only",
            ));
        }
        let mut cmd = Command::new(prog);
        cmd.args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
        for k in [
            "HOME",
            "USER",
            "LOGNAME",
            "LANG",
            "XDG_RUNTIME_DIR",
            "XDG_CONFIG_HOME",
            "DBUS_SESSION_BUS_ADDRESS",
        ] {
            if let Some(v) = std::env::var_os(k) {
                cmd.env(k, v);
            }
        }
        let out = cmd.output()?;
        Ok(RunOut {
            ok: out.status.success(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

fn paths() -> Result<ServicePaths> {
    let home = std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| Error::internal("HOME is not set"))?;
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    Ok(ServicePaths::new(
        Platform::current(),
        &home,
        xdg.as_deref(),
        unsafe { libc::geteuid() },
    ))
}

fn svc_err(e: io::Error) -> Error {
    if e.kind() == io::ErrorKind::InvalidInput {
        Error::usage(e.to_string())
    } else {
        Error::internal(format!("service: {e}"))
    }
}

fn show(plan: &Plan) -> String {
    let mut s = format!("File: {}\n", plan.path.display());
    match plan.existing {
        Exists::No => {}
        Exists::Same => s.push_str("This file is already installed with the same content.\n"),
        Exists::Different => {
            s.push_str("A file with different content exists there and will be replaced.\n")
        }
    }
    s.push('\n');
    s.push_str(&plan.content);
    s.push_str("\nCommands:\n");
    for c in &plan.commands {
        s.push_str(&format!("  {}\n", c.join(" ")));
    }
    if let Some(n) = &plan.note {
        s.push_str(&format!("\nNote: {n}\n"));
    }
    s
}

pub fn install(o: &Opts, apply: bool) -> Result<i32> {
    if apply {
        privileged::guard()?;
    }
    let cfg = runner_config()?;
    let paths = paths()?;
    let log = cfg.dir.join("runner.log");
    let path_env = std::env::var("PATH").unwrap_or_default();
    let plan = service::plan_install(
        &paths,
        &cfg.self_exe.display().to_string(),
        &path_env,
        &log.display().to_string(),
    )
    .map_err(svc_err)?;
    let warnings = service::exe_warnings(&cfg.self_exe, paths.uid);
    for w in &warnings {
        eprintln!("warning: {w}");
    }
    if !apply {
        let human = format!(
            "{}\nNothing was changed. Run with --install to apply.",
            show(&plan)
        );
        return Ok(emit(
            o,
            json!({
                "path": plan.path,
                "content": plan.content,
                "commands": plan.commands,
                "platform": plan.platform.name(),
                "existing": match plan.existing {
                    Exists::No => "no",
                    Exists::Same => "same",
                    Exists::Different => "different",
                },
                "note": plan.note,
                "warnings": warnings,
                "installed": false,
            }),
            human,
        ));
    }
    if !o.json {
        eprint!("{}", show(&plan));
    }
    if plan.existing == Exists::Same {
        eprintln!("already installed, registering again");
    } else {
        privileged::confirm_typed_stdio(CONFIRM, &format!("Type \"{CONFIRM}\" to continue:"))?;
    }
    if plan.platform == Platform::Launchd {
        tmgr_ipc::dirs::ensure_private_dir(&cfg.dir)
            .map_err(|e| Error::internal(format!("runner directory: {e}")))?;
    }
    service::install(&plan, &mut RealSys).map_err(svc_err)?;
    Ok(emit(
        o,
        json!({
            "path": plan.path,
            "platform": plan.platform.name(),
            "warnings": warnings,
            "installed": true,
        }),
        format!("installed {}", plan.path.display()),
    ))
}

pub fn uninstall(o: &Opts) -> Result<i32> {
    let in_agent = privileged::RUN_ENV
        .iter()
        .any(|k| std::env::var_os(k).is_some());
    privileged::check(in_agent, true)?;
    let paths = paths()?;
    let res = service::uninstall(&paths, &mut RealSys).map_err(svc_err)?;
    let (state, human) = match res {
        Removed::NotInstalled => ("not_installed", "not installed".to_string()),
        Removed::Foreign => (
            "foreign",
            format!(
                "{} is not a tmgr runner service file, left untouched",
                paths.file.display()
            ),
        ),
        Removed::Removed => ("removed", format!("removed {}", paths.file.display())),
    };
    Ok(emit(o, json!({"path": paths.file, "state": state}), human))
}
