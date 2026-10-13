use super::{emit, remote_attach, Opts, RunArgs};
use crate::cmd::runner::{AttachArgs, HaltArgs};
use crate::out::table;
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::IsTerminal;
use tmgr_core::config::{self, Config};
use tmgr_core::creds::Backend;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{State, StateKind};
use tmgr_ipc::proto::valid_session_id;
use tmgr_link::leader::{self, Client, KeyFile, SshTarget};
use tmgr_link::proto::Reply;
use tmgr_link::store::{Paths, Store};
use tmgr_runner::sanitize::sanitize_text;
use tmgr_runner::wire::now_ms;

const MAX_RUNS: usize = 1024;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct RunView {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub agent: Option<String>,
    pub state: State,
    #[serde(default)]
    pub task: Option<String>,
}

fn clean(s: &str) -> String {
    sanitize_text(s).into_owned()
}

pub(super) fn dial(link: &str) -> Result<(Client, KeyFile)> {
    tmgr_link::grammar::link_name(link)?;
    let paths = Paths::from_env()?;
    let store = Store::load(&paths)?;
    let f = store.followers.get(link).ok_or_else(|| {
        Error::not_found(format!("no follower named {link}")).hint("tmgr link list")
    })?;
    let cfg = Config::load(&config::config_dir()?)?;
    let preferred = cfg.credential_store.as_deref().and_then(Backend::parse);
    let cred = leader::credential_store(&paths, preferred);
    let target = SshTarget {
        user: f.user.clone(),
        host: f.host.clone(),
        port: f.port,
    };
    let (mut client, key) = Client::connect(&paths, &cred, link, &target)?;
    client.hello()?;
    Ok((client, key))
}

fn kind_of(code: &str) -> Kind {
    match code {
        "forbidden" | "revoked" => Kind::Forbidden,
        "bad_params" | "unknown_method" | "bad_request" => Kind::Usage,
        "not_found" => Kind::NotFound,
        "busy" | "not_running" | "conflict" | "rate_limited" | "too_many_requests" => {
            Kind::Conflict
        }
        "not_available" => Kind::Offline,
        _ => Kind::Link,
    }
}

pub(super) fn check(reply: Reply) -> Result<Value> {
    match reply {
        Reply::Ok(mut v) => {
            tmgr_link::attach::clean_value(&mut v);
            Ok(v)
        }
        Reply::Err { code, message } => {
            let hint = match code.as_str() {
                "not_available" => Some("start the runner on the follower: tmgr runner start"),
                "forbidden" => Some("what a leader may do is set on the follower: tmgr link scope"),
                _ => None,
            };
            let err = Error::new(kind_of(&code), clean(&message));
            Err(match hint {
                Some(h) => err.hint(h),
                None => err,
            })
        }
    }
}

fn runs_of(v: &Value) -> Vec<RunView> {
    v.get("runs")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .take(MAX_RUNS)
                .filter_map(|r| serde_json::from_value::<RunView>(r.clone()).ok())
                .filter(|r| valid_session_id(&r.id))
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn list_runs(client: &mut Client) -> Result<Vec<RunView>> {
    let v = check(client.call("runs.list", Value::Null)?)?;
    Ok(runs_of(&v))
}

pub fn run(o: &Opts, link: &str, a: &RunArgs) -> Result<i32> {
    let (Some(task), Some(agent)) = (&a.task, &a.agent) else {
        return Err(Error::usage("--on needs a task reference and --agent"));
    };
    let mut params = json!({"task": task, "agent": agent});
    if let Some(dir) = &a.cwd {
        if !dir.is_absolute() {
            return Err(Error::usage(
                "with --on, --cwd is a path on the follower and must be absolute",
            ));
        }
        params["repo"] = json!(dir.display().to_string());
    }
    for (key, value) in [
        ("model", &a.model),
        ("persona", &a.persona),
        ("task_hash", &a.task_hash),
    ] {
        if let Some(v) = value {
            params[key] = json!(v);
        }
    }
    if a.name.is_some() {
        return Err(Error::usage("--name is not supported with --on"));
    }
    let (mut client, _key) = dial(link)?;
    let v = check(client.call("runs.create", params)?)?;
    let view: RunView = serde_json::from_value(v.clone())
        .map_err(|_| Error::new(Kind::Link, "the follower sent an unexpected reply"))?;
    let human = format!(
        "started {} ({}) on {link}; attach with: tmgr attach {} --on {link}",
        clean(&view.name),
        clean(&view.id),
        clean(&view.id)
    );
    Ok(emit(o, json!({"link": link, "run": v}), human))
}

fn label(s: &RunView) -> &'static str {
    match s.state.kind {
        StateKind::Starting => "starting",
        StateKind::Working => "working",
        StateKind::Waiting => "waiting",
        StateKind::Blocked => "blocked",
        StateKind::Exited if s.state.exit_code == Some(0) => "exited",
        StateKind::Exited => "failed",
    }
}

pub fn agents(o: &Opts, link: &str) -> Result<i32> {
    let (mut client, _key) = dial(link)?;
    let v = check(client.call("runs.list", Value::Null)?)?;
    let runs = runs_of(&v);
    if o.json {
        return Ok(emit(o, json!({"link": link, "runs": v["runs"]}), ""));
    }
    if runs.is_empty() {
        println!("No runs from this leader on {link}. Start one with: tmgr run <REF> --on {link} --agent <profile>");
        return Ok(0);
    }
    let now = now_ms();
    let mut rows = vec![["NAME", "STATE", "ID", "AGENT", "TASK"]
        .map(String::from)
        .to_vec()];
    for r in &runs {
        let since = super::ops::span(now.saturating_sub(r.state.since_ms) / 1000);
        rows.push(vec![
            tmgr_core::text::sanitize_line(&r.name),
            format!("{} {since}", label(r)),
            clean(&r.id),
            clean(r.agent.as_deref().unwrap_or("")),
            clean(r.task.as_deref().unwrap_or("")),
        ]);
    }
    println!("{}", table(&rows));
    Ok(0)
}

pub fn attach(link: &str, a: &AttachArgs) -> Result<i32> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(Error::usage("attach needs an interactive terminal"));
    }
    let (mut client, _key) = dial(link)?;
    let runs = list_runs(&mut client)?;
    let target = match &a.session {
        Some(key) => runs
            .iter()
            .find(|r| r.id == *key)
            .or_else(|| runs.iter().find(|r| r.name == *key))
            .or_else(|| {
                let mut hits = runs.iter().filter(|r| r.id.starts_with(key.as_str()));
                hits.next().filter(|_| hits.next().is_none())
            }),
        None => runs
            .iter()
            .rev()
            .find(|r| r.state.kind != StateKind::Exited)
            .or(runs.last()),
    }
    .ok_or_else(|| {
        Error::not_found("no run of this leader on the follower matches")
            .hint(format!("tmgr agents --host {link}"))
    })?
    .clone();
    let (cols, rows) = ratatui::crossterm::terminal::size().unwrap_or((120, 40));
    let opened = check(client.call(
        "attach.open",
        json!({"run": target.id, "read_only": a.read_only, "cols": cols, "rows": rows}),
    )?)?;
    let input = opened
        .get("input")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    remote_attach::run(link, client, &target, input)
}

pub fn halt(o: &Opts, link: &str, a: &HaltArgs) -> Result<i32> {
    let (mut client, _key) = dial(link)?;
    let runs = list_runs(&mut client)?;
    let target = runs
        .iter()
        .find(|r| r.id == a.session)
        .or_else(|| runs.iter().rev().find(|r| r.name == a.session))
        .ok_or_else(|| {
            Error::not_found(format!(
                "no run {:?} of this leader on {link}",
                clean(&a.session)
            ))
            .hint(format!("tmgr agents --host {link}"))
        })?;
    let v = check(client.call("runs.halt", json!({"run": target.id}))?)?;
    Ok(emit(
        o,
        json!({"link": link, "result": v}),
        format!("{} stopped on {link}", clean(&target.name)),
    ))
}
