use super::{emit, privilege, remote_attach, Opts, PermissionMode, RunArgs};
use crate::cmd::runner::{AttachArgs, HaltArgs};
use crate::os_auth::{self, Outcome};
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
use tmgr_link::privileged;
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
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub title_auto: bool,
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

pub(super) const AUDIT_HINT: &str =
    "the follower cannot write its audit log; this link retries on its own";

pub(super) fn audit_unavailable(e: &Error) -> bool {
    e.hint.as_deref() == Some(AUDIT_HINT)
}

fn kind_of(code: &str) -> Kind {
    match code {
        "forbidden" | "revoked" => Kind::Forbidden,
        "bad_params" | "unknown_method" | "bad_request" => Kind::Usage,
        "not_found" => Kind::NotFound,
        "busy" | "not_running" | "conflict" | "rate_limited" | "too_many_requests" | "limit" => {
            Kind::Conflict
        }
        "not_available" | "audit_unavailable" => Kind::Offline,
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
                "audit_unavailable" => Some(AUDIT_HINT),
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

pub(super) fn is_follower_ref(task: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.len() <= 18 && s.bytes().all(|b| b.is_ascii_digit());
    task.strip_prefix('w')
        .and_then(|rest| rest.split_once(':'))
        .is_some_and(|(ws, id)| digits(ws) && digits(id))
}

pub(super) struct Pin(String);

impl Pin {
    fn new(s: String) -> Option<Self> {
        (!s.is_empty()).then_some(Self(s))
    }
}

impl std::fmt::Debug for Pin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Pin(<redacted>)")
    }
}

fn privileged(a: &RunArgs) -> bool {
    a.mode == Some(PermissionMode::Bypass) || a.no_sandbox
}

fn gate_word(a: &RunArgs) -> &'static str {
    if a.no_sandbox {
        "no-sandbox"
    } else {
        "bypass"
    }
}

fn gate_privileged(
    link: &str,
    a: &RunArgs,
    in_agent: bool,
    tty: bool,
    auth: &mut dyn FnMut(&str) -> Result<Outcome>,
    typed: &mut dyn FnMut(&str) -> Result<()>,
) -> Result<()> {
    privileged::check(in_agent, tty)?;
    if a.yes {
        return Err(Error::new(
            Kind::Forbidden,
            "--yes is not accepted for a privileged start on a follower",
        )
        .hint("run it in a terminal and confirm on this machine"));
    }
    let word = gate_word(a);
    match auth(&format!("start a {word} session on {link}"))? {
        Outcome::Approved => Ok(()),
        Outcome::Denied => Err(Error::new(
            Kind::Interrupted,
            "the local authentication was not approved, nothing was started",
        )),
        Outcome::Unavailable => typed(word),
    }
}

fn confirm_privileged(link: &str, a: &RunArgs) -> Result<Pin> {
    let in_agent = tmgr_link::privileged::RUN_ENV
        .iter()
        .any(|k| std::env::var_os(k).is_some())
        || tmgr_core::local::agent_session_detected();
    let tty = std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && std::io::stderr().is_terminal();
    if !in_agent && tty {
        for line in privilege::warnings(a.mode == Some(PermissionMode::Bypass), a.no_sandbox) {
            eprintln!("{line}");
            eprintln!("  (on {link})");
        }
    }
    gate_privileged(
        link,
        a,
        in_agent,
        tty,
        &mut os_auth::authenticate,
        &mut |word| privileged::confirm_typed_stdio(word, &privilege::prompt(word)),
    )?;
    read_pin(link, &mut |prompt| {
        rpassword::prompt_password(prompt)
            .map_err(|_| Error::new(Kind::Interrupted, "could not read the PIN"))
    })
}

fn read_pin(link: &str, read: &mut dyn FnMut(&str) -> Result<String>) -> Result<Pin> {
    Pin::new(read(&format!("PIN for {link}: "))?)
        .ok_or_else(|| Error::new(Kind::Interrupted, "no PIN entered, nothing was started"))
}

fn start_params(a: &RunArgs, pin: Option<&Pin>) -> Result<(Value, bool)> {
    let Some(agent) = &a.agent else {
        return Err(Error::usage("--on needs --agent <profile>"));
    };
    if a.mode.is_some_and(|m| m != PermissionMode::Bypass) {
        return Err(Error::usage("with --on, --mode accepts only bypass"));
    }
    let mut params = json!({"agent": agent});
    if a.mode == Some(PermissionMode::Bypass) {
        params["mode"] = json!("bypass");
    }
    if a.no_sandbox {
        params["no_sandbox"] = json!(true);
    }
    if let (true, Some(pin)) = (privileged(a), pin) {
        params["pin"] = json!(pin.0);
    }
    if let Some(task) = &a.task {
        if !is_follower_ref(task) {
            return Err(Error::usage(
                "--on takes a follower-local w<ws>:<id> ref; start cloud tasks from tmgr attach",
            ));
        }
        params["task"] = json!(task);
    }
    if let Some(dir) = &a.cwd {
        if !dir.is_absolute() {
            return Err(Error::usage(
                "with --on, --cwd is a path on the follower and must be absolute",
            ));
        }
        params["repo"] = json!(dir.display().to_string());
    }
    for (key, value) in [
        ("name", &a.name),
        ("model", &a.model),
        ("persona", &a.persona),
        ("task_hash", &a.task_hash),
    ] {
        if let Some(v) = value {
            params[key] = json!(v);
        }
    }
    let fallback = a.task.is_some() && a.name.is_none() && !privileged(a);
    Ok((params, fallback))
}

fn start_or_fallback(
    link: &str,
    params: Value,
    fallback: bool,
    call: &mut dyn FnMut(&str, Value) -> Result<Reply>,
) -> Result<Value> {
    match call("runs.start", params.clone())? {
        Reply::Err { code, .. } if code == "unknown_method" => {
            if fallback {
                check(call("runs.create", params)?)
            } else {
                Err(Error::usage(format!(
                    "{link} runs an older tmgr CLI; update it on the follower to start sessions there"
                )))
            }
        }
        other => check(other),
    }
}

pub fn run(o: &Opts, link: &str, a: &RunArgs) -> Result<i32> {
    start_params(a, None)?;
    let pin = if privileged(a) {
        Some(confirm_privileged(link, a)?)
    } else {
        None
    };
    let (params, fallback) = start_params(a, pin.as_ref())?;
    let (mut client, _key) = dial(link)?;
    let v = start_or_fallback(link, params, fallback, &mut |m, p| client.call(m, p))?;
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

    #[test]
    fn an_audit_refusal_is_recognised_and_other_codes_are_not() {
        let refused = |code: &str| {
            check(Reply::Err {
                code: code.into(),
                message: "x".into(),
            })
            .unwrap_err()
        };
        assert!(audit_unavailable(&refused("audit_unavailable")));
        assert!(!audit_unavailable(&refused("not_available")));
        assert!(!audit_unavailable(&refused("busy")));
    }

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

    mod start {
        use super::super::*;
        use crate::cli::{Cli, Command};
        use clap::Parser;

        fn parse(args: &[&str]) -> std::result::Result<RunArgs, clap::Error> {
            let cli = Cli::try_parse_from(std::iter::once("tmgr").chain(args.iter().copied()))?;
            match cli.command {
                Some(Command::Runner(crate::cmd::runner::RunnerCommands::Run(a))) => Ok(a),
                _ => panic!("not run"),
            }
        }

        fn err(code: &str, message: &str) -> Reply {
            Reply::Err {
                code: code.into(),
                message: message.into(),
            }
        }

        fn view() -> Reply {
            Reply::Ok(
                json!({"id": "s_aaaaaaaaaa", "name": "n", "state": {"kind": "working", "since_ms": 0}}),
            )
        }

        #[test]
        fn on_parses_without_a_ref_and_a_missing_agent_is_a_clear_error() {
            let a = parse(&["run", "--on", "spark", "--agent", "codex"]).unwrap();
            assert!(a.task.is_none());
            let (params, fallback) = start_params(&a, None).unwrap();
            assert_eq!(params, json!({"agent": "codex"}));
            assert!(!fallback);
            let bare = parse(&["run", "--on", "spark"]);
            let message = match bare {
                Ok(a) => start_params(&a, None).unwrap_err().message,
                Err(e) => e.to_string(),
            };
            assert!(message.contains("agent"), "{message}");
        }

        #[test]
        fn on_accepts_bypass_and_no_sandbox_but_no_other_mode() {
            assert!(parse(&["run", "--on", "s", "--agent", "x", "--mode", "bypass"]).is_ok());
            assert!(parse(&["run", "--on", "s", "--agent", "x", "--no-sandbox"]).is_ok());
            let a = parse(&["run", "--on", "s", "--agent", "x", "--mode", "plan"]).unwrap();
            assert!(start_params(&a, None)
                .unwrap_err()
                .message
                .contains("only bypass"));
        }

        fn pin(s: &str) -> Pin {
            Pin::new(s.to_string()).unwrap()
        }

        #[test]
        fn privileged_params_carry_mode_no_sandbox_and_pin() {
            let a = parse(&["run", "--on", "s", "--agent", "x", "--mode", "bypass"]).unwrap();
            let (p, _) = start_params(&a, Some(&pin("1234"))).unwrap();
            assert_eq!(p, json!({"agent": "x", "mode": "bypass", "pin": "1234"}));
            let a = parse(&["run", "--on", "s", "--agent", "x", "--no-sandbox"]).unwrap();
            let (p, _) = start_params(&a, Some(&pin("1234"))).unwrap();
            assert_eq!(p, json!({"agent": "x", "no_sandbox": true, "pin": "1234"}));
        }

        #[test]
        fn a_plain_start_never_carries_a_pin() {
            let a = parse(&["run", "--on", "s", "--agent", "x"]).unwrap();
            let (p, _) = start_params(&a, Some(&pin("1234"))).unwrap();
            assert_eq!(p, json!({"agent": "x"}));
        }

        #[test]
        fn a_privileged_task_start_never_falls_back_to_runs_create() {
            let a = parse(&[
                "run", "w1:7", "--on", "s", "--agent", "x", "--mode", "bypass",
            ])
            .unwrap();
            assert!(!start_params(&a, None).unwrap().1);
        }

        #[test]
        fn the_pin_is_redacted_in_debug() {
            assert!(!format!("{:?}", pin("1234")).contains("1234"));
        }

        #[test]
        fn an_empty_pin_aborts() {
            let e = read_pin("s", &mut |_| Ok(String::new())).unwrap_err();
            assert!(e.message.contains("no PIN"));
            let mut seen = String::new();
            read_pin("studio", &mut |p| {
                seen = p.to_string();
                Ok("1".into())
            })
            .unwrap();
            assert_eq!(seen, "PIN for studio: ");
        }

        fn gate(
            extra: &[&str],
            in_agent: bool,
            tty: bool,
            out: Result<Outcome>,
        ) -> (Result<()>, usize, Option<String>) {
            let mut argv = vec!["run", "--on", "s", "--agent", "x", "--mode", "bypass"];
            argv.extend_from_slice(extra);
            let a = parse(&argv).unwrap();
            let mut asked = 0;
            let mut out = Some(out);
            let mut typed_word = None;
            let r = gate_privileged(
                "s",
                &a,
                in_agent,
                tty,
                &mut |_| {
                    asked += 1;
                    out.take().unwrap()
                },
                &mut |w| {
                    typed_word = Some(w.to_string());
                    Ok(())
                },
            );
            (r, asked, typed_word)
        }

        #[test]
        fn the_gate_refuses_an_agent_session_and_a_missing_terminal_before_asking() {
            let (r, asked, _) = gate(&[], true, true, Ok(Outcome::Approved));
            assert_eq!(r.unwrap_err().kind, Kind::Forbidden);
            assert_eq!(asked, 0);
            let (r, asked, _) = gate(&[], false, false, Ok(Outcome::Approved));
            assert_eq!(r.unwrap_err().kind, Kind::NeedsConfirmation);
            assert_eq!(asked, 0);
        }

        #[test]
        fn the_gate_refuses_yes() {
            let (r, asked, _) = gate(&["--yes"], false, true, Ok(Outcome::Approved));
            assert!(r.unwrap_err().message.contains("--yes"));
            assert_eq!(asked, 0);
        }

        #[test]
        fn approved_passes_denied_aborts_unavailable_asks_for_the_typed_word() {
            let (r, _, typed) = gate(&[], false, true, Ok(Outcome::Approved));
            assert!(r.is_ok() && typed.is_none());
            let (r, _, typed) = gate(&[], false, true, Ok(Outcome::Denied));
            assert!(r.is_err() && typed.is_none());
            let (r, _, typed) = gate(&[], false, true, Ok(Outcome::Unavailable));
            assert!(r.is_ok());
            assert_eq!(typed.as_deref(), Some("bypass"));
            let (_, _, typed) = gate(&["--no-sandbox"], false, true, Ok(Outcome::Unavailable));
            assert_eq!(typed.as_deref(), Some("no-sandbox"));
        }

        #[test]
        fn params_carry_a_follower_ref_name_folder_and_model() {
            let a = parse(&[
                "run", "w1:7", "--on", "spark", "--agent", "codex", "--name", "job", "--cwd",
                "/srv/app", "--model", "m",
            ])
            .unwrap();
            let (params, fallback) = start_params(&a, None).unwrap();
            assert_eq!(
                params,
                json!({"agent": "codex", "task": "w1:7", "repo": "/srv/app", "name": "job", "model": "m"})
            );
            assert!(!fallback);
            let plain = parse(&["run", "w1:7", "--on", "spark", "--agent", "codex"]).unwrap();
            assert!(start_params(&plain, None).unwrap().1);
        }

        #[test]
        fn a_cloud_ref_and_a_relative_folder_are_refused() {
            for r in ["TM-1", "#5", "12", "c1:2"] {
                let a = parse(&["run", r, "--on", "spark", "--agent", "codex"]).unwrap();
                let e = start_params(&a, None).unwrap_err();
                assert!(
                    e.message.contains("start cloud tasks from tmgr attach"),
                    "{r}"
                );
            }
            let a = parse(&["run", "--on", "s", "--agent", "x", "--cwd", "rel"]).unwrap();
            assert!(start_params(&a, None)
                .unwrap_err()
                .message
                .contains("absolute"));
        }

        #[test]
        fn an_old_follower_gets_the_update_message_or_the_old_method_for_a_plain_ref() {
            let mut calls: Vec<String> = Vec::new();
            let mut old = |m: &str, _p: Value| {
                calls.push(m.to_string());
                Ok(match m {
                    "runs.start" => err("unknown_method", "unknown method"),
                    _ => view(),
                })
            };
            let e = start_or_fallback("spark", json!({"agent": "a"}), false, &mut old).unwrap_err();
            assert_eq!(
                e.message,
                "spark runs an older tmgr CLI; update it on the follower to start sessions there"
            );
            assert_eq!(e.kind, Kind::Usage);
            assert_eq!(calls, ["runs.start"]);
            calls.clear();
            let mut old = |m: &str, _p: Value| {
                calls.push(m.to_string());
                Ok(match m {
                    "runs.start" => err("unknown_method", "unknown method"),
                    _ => view(),
                })
            };
            let v = start_or_fallback(
                "spark",
                json!({"agent": "a", "task": "w1:2"}),
                true,
                &mut old,
            )
            .unwrap();
            assert_eq!(v["id"], "s_aaaaaaaaaa");
            assert_eq!(calls, ["runs.start", "runs.create"]);
        }

        #[test]
        fn a_refusal_from_a_new_follower_is_shown_and_never_retried() {
            let mut n = 0;
            let mut call = |_m: &str, _p: Value| {
                n += 1;
                Ok(err(
                    "forbidden",
                    "bypass-permissions is not allowed on this follower",
                ))
            };
            let e = start_or_fallback("spark", json!({"agent": "a"}), true, &mut call).unwrap_err();
            assert!(e.message.contains("bypass-permissions"));
            assert_eq!(e.kind, Kind::Forbidden);
            assert_eq!(n, 1);
        }
    }
}
