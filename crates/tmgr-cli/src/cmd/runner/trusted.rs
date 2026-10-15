use super::{connect, emit, rerr, runner_config, Opts};
use crate::interact;
use crate::out::table;
use serde_json::{json, Value};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{caps, method, TrustRemoved, TrustedFolder};
use tmgr_link::privileged;
use tmgr_runner::client::Client;
use tmgr_runner::daemon;
use tmgr_runner::wire::now_ms;

fn client() -> Result<Client> {
    let cfg = runner_config()?;
    daemon::start_background(&cfg).map_err(rerr)?;
    let c = connect(&cfg)?;
    if !c
        .welcome()
        .caps
        .iter()
        .any(|h| h == caps::TRUSTED_FOLDERS_V1)
    {
        return Err(Error::new(
            Kind::Conflict,
            "the running runner is older than this tmgr and keeps no trusted folders",
        )
        .hint(format!("{0} runner stop && {0} runner start", super::bin())));
    }
    Ok(c)
}

fn clean(s: &str) -> String {
    tmgr_core::text::sanitize_line(s)
}

fn absolute(dir: &Path) -> Result<String> {
    let abs = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| Error::usage(e.to_string()))?
            .join(dir)
    };
    abs.to_str()
        .map(str::to_string)
        .ok_or_else(|| Error::usage("the folder path is not valid UTF-8"))
}

pub fn trust(o: &Opts, dir: &Path) -> Result<i32> {
    let in_agent = privileged::RUN_ENV
        .iter()
        .any(|k| std::env::var_os(k).is_some());
    privileged::check(in_agent, true)?;
    let path = absolute(dir)?;
    let shown = std::fs::canonicalize(&path)
        .map_err(|_| Error::usage(format!("{} is not an existing folder", clean(&path))))?;
    let id = std::fs::metadata(&shown).map_err(|e| Error::usage(e.to_string()))?;
    interact::confirm(
        &format!(
            "Trust {}? Agents started in exactly this folder will no longer ask whether to trust it.",
            clean(&shown.display().to_string())
        ),
        false,
        false,
    )?;
    let v = client()?
        .call(
            method::TRUST_ADD,
            json!({ "path": shown, "dev": id.dev(), "ino": id.ino() }),
        )
        .map_err(rerr)?;
    let f: TrustedFolder = serde_json::from_value(v).map_err(|e| Error::usage(e.to_string()))?;
    let human = format!("trusted {}", clean(&f.path));
    Ok(emit(o, &f, human))
}

pub fn untrust(o: &Opts, dir: &Path) -> Result<i32> {
    let path = absolute(dir)?;
    let v = client()?
        .call(method::TRUST_REMOVE, json!({ "path": path }))
        .map_err(rerr)?;
    let r: TrustRemoved = serde_json::from_value(v).map_err(|e| Error::usage(e.to_string()))?;
    let human = if r.removed {
        format!("{} is no longer trusted", clean(&path))
    } else {
        format!("{} was not trusted", clean(&path))
    };
    Ok(emit(o, &r, human))
}

pub fn list(o: &Opts) -> Result<i32> {
    let v = client()?
        .call(method::TRUST_LIST, Value::Null)
        .map_err(rerr)?;
    let folders: Vec<TrustedFolder> =
        serde_json::from_value(v).map_err(|e| Error::usage(e.to_string()))?;
    let human = if folders.is_empty() {
        "no trusted folders".to_string()
    } else {
        let mut rows = vec![vec!["FOLDER".into(), "HOW".into(), "ADDED".into()]];
        for f in &folders {
            rows.push(vec![
                clean(&f.path),
                clean(&f.how),
                format!(
                    "{} ago",
                    super::ops::span(now_ms().saturating_sub(f.added_ms) / 1000)
                ),
            ]);
        }
        table(&rows)
    };
    Ok(emit(o, &folders, human))
}
