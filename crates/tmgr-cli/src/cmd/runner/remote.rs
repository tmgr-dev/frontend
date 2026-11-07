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
use tmgr_link::leader::{self, Client, KeyFile};
use tmgr_link::proto::Reply;
use tmgr_link::store::Paths;
use tmgr_runner::sanitize::sanitize_text;
use tmgr_runner::wire::now_ms;

const MAX_RUNS: usize = 1024;
const MAX_FIELD: usize = 64;
const MAX_ROWS: usize = 200;

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
    #[serde(default)]
    pub via: Option<String>,
}

impl RunView {
    fn foreign(&self) -> bool {
        self.via.as_deref() == Some("remote_control")
    }
}

fn origin(r: &RunView) -> &'static str {
    if r.foreign() {
        "local"
    } else {
        "leader"
    }
}

fn active(r: &RunView) -> bool {
    r.state.kind != StateKind::Exited
}

fn default_target(runs: &[RunView]) -> Option<&RunView> {
    let owned_active: Vec<&RunView> = runs.iter().filter(|r| !r.foreign() && active(r)).collect();
    let strangers = runs.iter().any(RunView::foreign);
    match (owned_active.len(), strangers) {
        (1, _) | (_, false) if !owned_active.is_empty() => owned_active.last().copied(),
        (0, false) => runs.last(),
        _ => None,
    }
}

fn find_by_key<'a>(runs: &'a [RunView], key: &str) -> Result<Option<&'a RunView>> {
    if let Some(r) = runs.iter().find(|r| r.id == key) {
        return Ok(Some(r));
    }
    let named: Vec<&RunView> = runs.iter().filter(|r| r.name == key).collect();
    let live: Vec<&RunView> = named.iter().copied().filter(|r| active(r)).collect();
    let pool = if live.is_empty() { named } else { live };
    match pool.as_slice() {
        [] => {}
        [one] => return Ok(Some(one)),
        many => {
            let ids: Vec<String> = many.iter().map(|r| clean(&r.id)).collect();
            return Err(Error::usage(format!(
                "{} runs are named {:?}: {}",
                many.len(),
                clean(key),
                ids.join(", ")
            ))
            .hint("pass the id instead of the name"));
        }
    }
    let mut hits = runs.iter().filter(|r| r.id.starts_with(key));
    Ok(hits.next().filter(|_| hits.next().is_none()))
}

fn clean(s: &str) -> String {
    sanitize_text(s).into_owned()
}

fn cell(s: &str) -> String {
    let (cut, more) =
        tmgr_core::text::truncate_chars(&tmgr_core::text::sanitize_line(s), MAX_FIELD);
    if more {
        format!("{cut}…")
    } else {
        cut
    }
}

pub(super) fn dial(link: &str) -> Result<(Client, KeyFile)> {
    tmgr_link::grammar::link_name(link)?;
    let paths = Paths::from_env()?;
    let cfg = Config::load(&config::config_dir()?)?;
    let preferred = cfg.credential_store.as_deref().and_then(Backend::parse);
    let cred = leader::credential_store(&paths, preferred);
    let (mut client, key) = Client::dial(&paths, &cred, link)?;
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

pub(super) fn runs_of(v: &Value) -> Vec<RunView> {
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
    let bin = super::bin();
    let human = format!(
        "started {} ({}) on {link}; attach with: {bin} attach {} --on {link}",
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
        let bin = super::bin();
        println!("No runs from this leader or under remote control on {link}. Start one with: {bin} run <REF> --on {link} --agent <profile>");
        return Ok(0);
    }
    let now = now_ms();
    let mut rows = vec![["NAME", "STATE", "ID", "FROM", "AGENT", "TASK"]
        .map(String::from)
        .to_vec()];
    for r in runs.iter().take(MAX_ROWS) {
        let since = super::ops::span(now.saturating_sub(r.state.since_ms) / 1000);
        rows.push(vec![
            cell(&r.name),
            format!("{} {since}", label(r)),
            cell(&r.id),
            origin(r).to_string(),
            cell(r.agent.as_deref().unwrap_or("")),
            cell(r.task.as_deref().unwrap_or("")),
        ]);
    }
    println!("{}", table(&rows));
    if runs.len() > MAX_ROWS {
        println!("… {} more", runs.len() - MAX_ROWS);
    }
    Ok(0)
}

pub fn attach(link: &str, a: &AttachArgs) -> Result<i32> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(Error::usage("attach needs an interactive terminal"));
    }
    let (mut client, _key) = dial(link)?;
    let runs = list_runs(&mut client)?;
    let target = match &a.session {
        Some(key) => find_by_key(&runs, key)?,
        None if runs.is_empty() => None,
        None => Some(default_target(&runs).ok_or_else(|| {
            Error::usage("several runs are available, or none of them started from this leader")
                .hint(format!(
                    "pick one: {} agents --host {link}, then attach <id> --on {link}",
                    super::bin()
                ))
        })?),
    }
    .ok_or_else(|| {
        Error::not_found("no run on the follower matches")
            .hint(format!("{} agents --host {link}", super::bin()))
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
    let target = find_by_key(&runs, &a.session)?
        .filter(|r| r.id == a.session || r.name == a.session)
        .ok_or_else(|| {
            Error::not_found(format!("no run {:?} on {link}", clean(&a.session)))
                .hint(format!("{} agents --host {link}", super::bin()))
        })?;
    let v = check(client.call("runs.halt", json!({"run": target.id}))?)?;
    Ok(emit(
        o,
        json!({"link": link, "result": v}),
        format!("{} stopped on {link}", clean(&target.name)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(id: &str, name: &str, via: Option<&str>, exited: bool) -> RunView {
        serde_json::from_value(json!({
            "id": id, "name": name, "via": via,
            "state": {"kind": if exited { "exited" } else { "working" }, "since_ms": 0},
        }))
        .unwrap()
    }

    #[test]
    fn default_attach_target_never_picks_a_run_started_locally() {
        let only_foreign = [run("s_aaaaaaaaaa", "a", Some("remote_control"), false)];
        assert!(default_target(&only_foreign).is_none());
        let two_foreign_one_owned = [
            run("s_aaaaaaaaaa", "a", Some("remote_control"), false),
            run("s_bbbbbbbbbb", "b", None, false),
            run("s_cccccccccc", "c", Some("remote_control"), false),
        ];
        assert_eq!(
            default_target(&two_foreign_one_owned).unwrap().id,
            "s_bbbbbbbbbb"
        );
        let two_owned_with_foreign = [
            run("s_aaaaaaaaaa", "a", None, false),
            run("s_bbbbbbbbbb", "b", None, false),
            run("s_cccccccccc", "c", Some("remote_control"), false),
        ];
        assert!(default_target(&two_owned_with_foreign).is_none());
    }

    #[test]
    fn old_followers_without_via_keep_the_newest_active_default() {
        let runs = [
            run("s_aaaaaaaaaa", "a", None, false),
            run("s_bbbbbbbbbb", "b", None, false),
            run("s_cccccccccc", "c", None, true),
        ];
        assert_eq!(default_target(&runs).unwrap().id, "s_bbbbbbbbbb");
    }

    #[test]
    fn an_ambiguous_name_is_refused_and_an_id_still_works() {
        let runs = [
            run("s_aaaaaaaaaa", "dev", None, false),
            run("s_bbbbbbbbbb", "dev", Some("remote_control"), false),
        ];
        let e = find_by_key(&runs, "dev").unwrap_err();
        assert!(e.message.contains("2 runs are named"));
        assert_eq!(
            find_by_key(&runs, "s_bbbbbbbbbb").unwrap().unwrap().id,
            "s_bbbbbbbbbb"
        );
        let one_live = [
            run("s_aaaaaaaaaa", "dev", None, true),
            run("s_bbbbbbbbbb", "dev", None, false),
        ];
        assert_eq!(
            find_by_key(&one_live, "dev").unwrap().unwrap().id,
            "s_bbbbbbbbbb"
        );
    }
}
