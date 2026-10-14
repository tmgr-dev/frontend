use crate::attach::{self, Outgoing};
use crate::ownership::{RunRecord, Runs};
use crate::proto::{self, Channel, Poll, Request};
use crate::runner::{AttachRecv, AttachSession, Backend, BackendError};
use crate::scope::{self, canonical_repo, Scope};
use crate::taskref::format_ref;
use crate::worker::{say, Conn, State, Step};
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, Instant};
use tmgr_ipc::api::{EventMsg, Origin, RunCreateReq, ScreenPush, SessionInfo, StateKind};
use tmgr_ipc::proto::ServerMsg;

pub const ATTACH_MAX: Duration = Duration::from_secs(3600);
const ATTACH_IDLE: Duration = Duration::from_secs(45);
const TICK: Duration = Duration::from_secs(1);
const INPUT_MSGS_PER_SEC: u32 = 100;
const INPUT_BYTES_PER_SEC: usize = 64 * 1024;
const DEFAULT_SIZE: (u16, u16) = (120, 40);
pub const BUSY_BUDGET: Duration = Duration::from_secs(10);
pub const BUSY_PAUSE: Duration = Duration::from_millis(500);
pub const BUSY_ATTEMPT_MAX: Duration = Duration::from_secs(6);
const BUSY_MARGIN: Duration = Duration::from_millis(500);
const HALT_STEP: Duration = Duration::from_secs(3);

fn audit_down() -> Step {
    Step::Close(proto::error(None, "audit_unavailable", "request refused"))
}

fn deny(conn: &Conn, method: &str, args: Value, id: Option<u64>, code: &str, reason: &str) -> Step {
    if conn.record(method, args, "rejected", reason).is_err() {
        return audit_down();
    }
    Step::Reply(proto::error(id, code, reason))
}

fn backend_reply(id: Option<u64>, e: &BackendError) -> Vec<u8> {
    match e {
        BackendError::Unavailable(why) if why == "runner not installed" => {
            proto::error(id, "not_available", "runner not installed")
        }
        BackendError::Unavailable(_) => proto::error(id, "not_available", "runner not running"),
        BackendError::NotFound => proto::error(id, "not_found", "no such run"),
        BackendError::NotRunning => proto::error(id, "not_running", "the run is not running"),
        BackendError::Busy => proto::error(id, "busy", "the agent is not reading its input"),
        BackendError::Refused { code, .. } if code == "limit" => proto::error(
            id,
            "limit",
            "runner session limit reached; clean up exited runs on the follower",
        ),
        BackendError::Refused { code, message } => {
            let code = if code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
                code.as_str()
            } else {
                "refused"
            };
            proto::error(id, code, &tmgr_core::text::truncate_chars(message, 200).0)
        }
        BackendError::Protocol(_) => {
            proto::error(id, "internal", "the runner sent an unexpected reply")
        }
    }
}

fn failed(conn: &Conn, method: &str, args: Value, id: Option<u64>, e: &BackendError) -> Step {
    let reason = tmgr_core::text::truncate_chars(&e.to_string(), 200).0;
    if conn.record(method, args, "failed", &reason).is_err() {
        return audit_down();
    }
    Step::Reply(backend_reply(id, e))
}

fn with_backend<T>(
    conn: &Conn,
    st: &mut State,
    retry: bool,
    deadline: Option<Instant>,
    mut f: impl FnMut(&mut dyn Backend) -> Result<T, BackendError>,
) -> Result<T, BackendError> {
    for attempt in 0..2 {
        if st.backend.is_none() {
            st.backend = Some((conn.rt.connect)(deadline)?);
        }
        let Some(b) = st.backend.as_mut() else {
            continue;
        };
        b.set_deadline(deadline);
        match f(b.as_mut()) {
            Err(BackendError::Unavailable(_)) if retry && attempt == 0 => st.backend = None,
            other => {
                if matches!(other, Err(BackendError::Unavailable(_))) {
                    st.backend = None;
                }
                return other;
            }
        }
    }
    Err(BackendError::Unavailable("runner not running".into()))
}

fn clean_line(s: &str, max: usize) -> String {
    attach::clean_text(&tmgr_core::text::truncate_chars(s, max).0)
}

fn view(s: &SessionInfo, rec: Option<&RunRecord>) -> Value {
    json!({
        "id": s.id,
        "name": clean_line(&s.name, 64),
        "agent": s.profile,
        "state": {
            "kind": s.state.kind,
            "since_ms": s.state.since_ms,
            "detail": s.state.detail.as_deref().map(|d| clean_line(d, 200)),
            "exit_code": s.state.exit_code,
        },
        "created_ms": s.created_ms,
        "task": s.task.as_ref().map(format_ref).or_else(|| rec.map(|r| r.task.clone())),
        "persona": rec.and_then(|r| r.persona.clone()),
        "model": rec.and_then(|r| r.model.clone()),
    })
}

fn run_param(params: &Value) -> String {
    params
        .get("run")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(crate) fn run_method(
    conn: &Conn,
    st: &mut State,
    chan: &mut dyn Channel,
    req: &Request,
    scope: &Scope,
) -> Step {
    let id = Some(req.id);
    let method = req.method.as_str();
    let mut params = req.params.clone();
    if method == "runs.create" && scope.repos.len() == 1 {
        if let Value::Object(m) = &mut params {
            m.entry("repo").or_insert_with(|| json!(scope.repos[0]));
        }
    }
    let mut args = scope::audit_args(&params);
    let accepted = match scope::validate_run_method(method, &params, scope, 0) {
        Ok(a) => a,
        Err(r) => return deny(conn, method, args, id, r.code, &r.reason),
    };
    if let (Some(repo), Some(obj)) = (&accepted.repo, args.as_object_mut()) {
        obj.insert("repo_canonical".into(), json!(repo));
    }
    match method {
        "runs.list" => list(conn, st, id, args),
        "runs.get" => get(conn, st, id, args, &run_param(&params)),
        "runs.halt" => halt(conn, st, id, args, &run_param(&params)),
        "runs.create" => create(conn, st, id, args, &params, accepted.repo, scope),
        _ => attach_open(conn, st, chan, id, args, &params, scope),
    }
}

fn list(conn: &Conn, st: &mut State, id: Option<u64>, args: Value) -> Step {
    if conn
        .record("runs.list", args.clone(), "accepted", "")
        .is_err()
    {
        return audit_down();
    }
    let Ok(mut runs) = Runs::lock(conn.paths) else {
        return Step::Close(proto::error(id, "unavailable", "request refused"));
    };
    let listed = match with_backend(conn, st, true, None, |b| b.list()) {
        Ok(l) => l,
        Err(e) => return failed(conn, "runs.list", args, id, &e),
    };
    if let Ok(store) = crate::store::Store::load(conn.paths) {
        adopt(conn, &mut runs, &listed, &store);
    }
    let existing: Vec<String> = listed.iter().map(|s| s.id.clone()).collect();
    let _ = runs.retain_existing(&existing);
    let mine: Vec<Value> = listed
        .iter()
        .filter(|s| runs.owns(conn.name, conn.enrollment, &s.id))
        .map(|s| view(s, runs.get(&s.id)))
        .collect();
    Step::Reply(proto::ok(id, json!({"runs": mine})))
}

fn adopt(conn: &Conn, runs: &mut Runs, listed: &[SessionInfo], store: &crate::store::Store) {
    for s in listed {
        let ours = s.origin.as_ref().is_some_and(|o| {
            o.link == conn.name
                && store.enrollment_of(&o.link, o.generation).as_deref() == Some(conn.enrollment)
        });
        if ours && !runs.recorded(&s.id) {
            let _ = runs.record(RunRecord {
                run: s.id.clone(),
                link: conn.name.to_string(),
                enrollment: conn.enrollment.to_string(),
                task: s.task.as_ref().map(format_ref).unwrap_or_default(),
                agent: s.profile.clone().unwrap_or_default(),
                persona: None,
                model: None,
                created: crate::now_rfc3339(),
                generation: s.origin.as_ref().map(|o| o.generation),
            });
        }
    }
}

fn owned(conn: &Conn, run: &str) -> Option<bool> {
    Runs::lock(conn.paths)
        .ok()
        .map(|r| r.owns(conn.name, conn.enrollment, run))
}

fn get(conn: &Conn, st: &mut State, id: Option<u64>, args: Value, run: &str) -> Step {
    match owned(conn, run) {
        None => return Step::Close(proto::error(id, "unavailable", "request refused")),
        Some(false) => return deny(conn, "runs.get", args, id, "not_found", "no such run"),
        Some(true) => {}
    }
    if conn
        .record("runs.get", args.clone(), "accepted", "")
        .is_err()
    {
        return audit_down();
    }
    match with_backend(conn, st, true, None, |b| b.list()) {
        Err(e) => failed(conn, "runs.get", args, id, &e),
        Ok(list) => match list.iter().find(|s| s.id == run) {
            None => Step::Reply(proto::error(id, "not_found", "no such run")),
            Some(s) => {
                let rec = Runs::lock(conn.paths)
                    .ok()
                    .and_then(|r| r.get(run).cloned());
                Step::Reply(proto::ok(id, view(s, rec.as_ref())))
            }
        },
    }
}

fn wait_exit(b: &mut dyn Backend, run: &str, limit: Duration) -> Result<bool, BackendError> {
    let end = Instant::now() + limit;
    loop {
        let list = b.list()?;
        if list
            .iter()
            .find(|s| s.id == run)
            .is_none_or(|s| s.state.kind == StateKind::Exited)
        {
            return Ok(true);
        }
        if Instant::now() >= end {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub fn stop_run(b: &mut dyn Backend, run: &str) -> Result<&'static str, BackendError> {
    for signal in ["int", "term", "kill"] {
        match b.kill(run, signal) {
            Ok(()) => {}
            Err(BackendError::NotRunning | BackendError::NotFound) => return Ok("already"),
            Err(e) => return Err(e),
        }
        if wait_exit(b, run, HALT_STEP)? {
            return Ok(signal);
        }
    }
    Err(BackendError::Refused {
        code: "conflict".into(),
        message: "the run is still running".into(),
    })
}

fn halt(conn: &Conn, st: &mut State, id: Option<u64>, args: Value, run: &str) -> Step {
    match owned(conn, run) {
        None => return Step::Close(proto::error(id, "unavailable", "request refused")),
        Some(false) => return deny(conn, "runs.halt", args, id, "not_found", "no such run"),
        Some(true) => {}
    }
    if conn
        .record("runs.halt", args.clone(), "accepted", "")
        .is_err()
    {
        return audit_down();
    }
    match with_backend(conn, st, false, None, |b| stop_run(b, run)) {
        Ok(signal) => {
            if conn
                .audit
                .append(
                    "worker.run_halted",
                    json!({"link": conn.name, "run": run, "signal": signal}),
                )
                .is_err()
            {
                return audit_down();
            }
            Step::Reply(proto::ok(
                id,
                json!({"id": run, "signal": signal, "exited": true}),
            ))
        }
        Err(e) => failed(conn, "runs.halt", args, id, &e),
    }
}

fn live_count(list: &[SessionInfo], runs: &Runs, link: &str, enrollment: &str) -> u32 {
    list.iter()
        .filter(|s| s.state.kind != StateKind::Exited && runs.owns(link, enrollment, &s.id))
        .count() as u32
}

fn run_name(link: &str, task: &str) -> String {
    tmgr_core::text::truncate_chars(&format!("link-{link}-{task}"), 64).0
}

fn create(
    conn: &Conn,
    st: &mut State,
    id: Option<u64>,
    mut args: Value,
    params: &Value,
    repo: Option<String>,
    scope: &Scope,
) -> Step {
    let method = "runs.create";
    let deadline = Instant::now() + BUSY_BUDGET - BUSY_MARGIN;
    let (Some(repo), Some(task_text), Some(agent)) = (
        repo,
        params.get("task").and_then(Value::as_str),
        params.get("agent").and_then(Value::as_str),
    ) else {
        return deny(conn, method, args, id, "bad_params", "incomplete request");
    };
    let task = match conn.rt.resolver.resolve(task_text) {
        Ok(t) => t,
        Err(why) => return deny(conn, method, args, id, "forbidden", &why),
    };
    if let Some(wanted) = params.get("task_hash").and_then(Value::as_str) {
        match conn.rt.resolver.content_hash(&task) {
            Some(actual) if actual == wanted => {}
            Some(_) => {
                return deny(
                    conn,
                    method,
                    args,
                    id,
                    "forbidden",
                    "the task changed since the leader saw it",
                )
            }
            None => {
                return deny(
                    conn,
                    method,
                    args,
                    id,
                    "forbidden",
                    "the task content cannot be verified on this machine",
                )
            }
        }
    }
    let model = params
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_string);
    let persona = params
        .get("persona")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Value::Object(m) = &mut args {
        m.insert("task_ref".into(), json!(format_ref(&task)));
        m.insert("mode".into(), json!("default"));
    }
    if conn.record(method, args.clone(), "accepted", "").is_err() {
        return audit_down();
    }
    let Ok(mut runs) = Runs::lock(conn.paths) else {
        return Step::Close(proto::error(id, "unavailable", "request refused"));
    };
    let (fresh, generation) = match conn.current_gen() {
        Ok(Ok(v)) => v,
        Ok(Err(reason)) => {
            let _ = conn.record(method, args, "rejected", reason);
            return Step::Close(proto::error(
                id,
                "revoked",
                "this link is no longer trusted",
            ));
        }
        Err(_) => return Step::Close(proto::error(id, "unavailable", "request refused")),
    };
    if fresh != *scope {
        let _ = conn.record(method, args.clone(), "rejected", "scope changed meanwhile");
        return Step::Reply(proto::error(
            id,
            "forbidden",
            "the scope changed, try again",
        ));
    }
    let mut listed = match with_backend(conn, st, true, Some(deadline), |b| b.list()) {
        Ok(l) => l,
        Err(e) => return failed(conn, method, args, id, &e),
    };
    if let Ok(store) = crate::store::Store::load(conn.paths) {
        adopt(conn, &mut runs, &listed, &store);
    }
    let exited: Vec<String> = listed
        .iter()
        .filter(|s| {
            s.state.kind == StateKind::Exited && runs.owns(conn.name, conn.enrollment, &s.id)
        })
        .map(|s| s.id.clone())
        .collect();
    for rid in exited {
        if with_backend(conn, st, false, Some(deadline), |b| b.remove(&rid)).is_ok() {
            let _ = runs.forget(&rid);
            listed.retain(|s| s.id != rid);
        }
    }
    let live = live_count(&listed, &runs, conn.name, conn.enrollment);
    if let Err(r) = scope::validate_run_method(method, params, &fresh, live) {
        return deny(conn, method, args, id, r.code, &r.reason);
    }
    let mut req = RunCreateReq {
        profile: agent.to_string(),
        task: Some(task),
        cwd: repo,
        name: Some(run_name(conn.name, &format_ref(&task))),
        model: model.clone(),
        cols: DEFAULT_SIZE.0,
        rows: DEFAULT_SIZE.1,
        bundle: None,
        bundle_sha256: None,
        cwd_pin: None,
        origin: Some(Origin {
            link: conn.name.to_string(),
            generation,
        }),
        permission_mode: None,
        start_key: None,
        clone_of: None,
    };
    let mut generation = generation;
    let call = |st: &mut State, req: &RunCreateReq| -> Result<SessionInfo, BackendError> {
        if Instant::now() >= deadline {
            return Err(BackendError::Busy);
        }
        with_backend(conn, st, false, Some(deadline), |b| b.create_run(req))
    };
    let mut result = call(st, &req);
    if matches!(&result, Err(BackendError::Refused { message, .. }) if message.contains("link changed"))
    {
        if let Ok(Ok((again, newer))) = conn.current_gen() {
            if again == fresh {
                generation = newer;
                req.origin = Some(Origin {
                    link: conn.name.to_string(),
                    generation,
                });
                result = call(st, &req);
            }
        }
    }
    if matches!(result, Err(BackendError::Busy))
        && deadline.saturating_duration_since(Instant::now()) >= BUSY_PAUSE + BUSY_ATTEMPT_MAX
    {
        std::thread::sleep(BUSY_PAUSE);
        result = call(st, &req);
    }
    let info = match result {
        Ok(i) => i,
        Err(BackendError::Busy) => {
            if conn
                .record(method, args, "failed", "the runner stayed busy")
                .is_err()
            {
                return audit_down();
            }
            return Step::Reply(proto::error(
                id,
                "busy",
                "the follower's runner is busy (the folder is in use or its trust store is changing); try again",
            ));
        }
        Err(e) => return failed(conn, method, args, id, &e),
    };
    let kill = |st: &mut State| {
        let _ = with_backend(conn, st, false, None, |b| b.kill(&info.id, "kill"));
    };
    let record = RunRecord {
        run: info.id.clone(),
        link: conn.name.to_string(),
        enrollment: conn.enrollment.to_string(),
        task: format_ref(&task),
        agent: agent.to_string(),
        persona,
        model,
        created: crate::now_rfc3339(),
        generation: Some(generation),
    };
    if runs.record(record.clone()).is_err() {
        kill(st);
        return Step::Close(proto::error(id, "internal", "could not record the run"));
    }
    let inside = canonical_repo(&info.cwd)
        .map(|c| fresh.repos.iter().any(|r| Path::new(&c).starts_with(r)))
        .unwrap_or(false);
    if !inside {
        kill(st);
        let _ = runs.forget(&info.id);
        let _ = conn.record(
            method,
            args,
            "rejected",
            "the runner placed the run outside the scope",
        );
        return Step::Reply(proto::error(
            id,
            "forbidden",
            "repo is outside the allowed repos",
        ));
    }
    if conn
        .audit
        .append(
            "worker.run_created",
            json!({
                "link": conn.name,
                "fp": conn.fingerprint,
                "run": info.id,
                "task": record.task,
                "agent": record.agent,
                "persona": record.persona,
                "model": record.model,
                "mode": "default",
            }),
        )
        .is_err()
    {
        kill(st);
        return audit_down();
    }
    Step::Reply(proto::ok(id, view(&info, Some(&record))))
}

struct Stats {
    msgs: u32,
    bytes: u64,
}

fn attach_open(
    conn: &Conn,
    st: &mut State,
    chan: &mut dyn Channel,
    id: Option<u64>,
    mut args: Value,
    params: &Value,
    scope: &Scope,
) -> Step {
    let method = "attach.open";
    let run = run_param(params);
    let read_only = params
        .get("read_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let dim = |key: &str, default: u16| {
        params
            .get(key)
            .and_then(Value::as_u64)
            .map_or(default, |n| n as u16)
    };
    let size = (dim("cols", DEFAULT_SIZE.0), dim("rows", DEFAULT_SIZE.1));
    match owned(conn, &run) {
        None => return Step::Close(proto::error(id, "unavailable", "request refused")),
        Some(false) => return deny(conn, method, args, id, "not_found", "no such run"),
        Some(true) => {}
    }
    let input = !read_only && scope.allow_attach_input;
    let (mode, reason) = if input {
        ("interactive", "")
    } else if read_only {
        ("read-only", "")
    } else {
        ("read-only", "the scope has no allow_attach_input")
    };
    if let Value::Object(m) = &mut args {
        m.insert("mode".into(), json!(mode));
    }
    let mut session = match with_backend(conn, st, true, None, |b| {
        b.attach(&run, input.then_some(size))
    }) {
        Ok(s) => s,
        Err(e) => return failed(conn, method, args, id, &e),
    };
    if conn.record(method, args, "accepted", reason).is_err() {
        return audit_down();
    }
    if !say(chan, proto::ok(id, json!({"run": run, "input": input}))) {
        session.detach();
        return Step::Done;
    }
    chan.extend_deadline(ATTACH_MAX);
    let (why, stats) = attach_loop(conn, st, chan, session.as_mut(), &run, input);
    session.detach();
    let _ = conn.audit.append(
        "worker.attach_end",
        json!({
            "link": conn.name,
            "run": run,
            "mode": mode,
            "input_msgs": stats.msgs,
            "input_bytes": stats.bytes,
            "reason": why,
        }),
    );
    Step::Done
}

fn forward(chan: &mut dyn Channel, run: &str, msg: ServerMsg) -> Result<bool, String> {
    match msg {
        ServerMsg::Screen(v) => {
            let Ok(push) = serde_json::from_value::<ScreenPush>(v) else {
                return Ok(true);
            };
            match attach::guard_screen(push, run) {
                Ok(Some(p)) => Ok(say(chan, attach::encode_screen(&p))),
                Ok(None) => Ok(true),
                Err(why) => Err(why.to_string()),
            }
        }
        ServerMsg::Event(v) => {
            let Ok(ev) = serde_json::from_value::<EventMsg>(v) else {
                return Ok(true);
            };
            match attach::guard_event(ev, run) {
                Some(ev) => Ok(say(chan, attach::encode_event(&ev))),
                None => Ok(true),
            }
        }
        _ => Ok(true),
    }
}

fn input_still_allowed(conn: &Conn) -> bool {
    matches!(conn.current(), Ok(Ok(s)) if s.allow_attach_input)
}

fn attach_loop(
    conn: &Conn,
    st: &mut State,
    chan: &mut dyn Channel,
    session: &mut dyn AttachSession,
    run: &str,
    input_allowed: bool,
) -> (String, Stats) {
    let mut stats = Stats { msgs: 0, bytes: 0 };
    let mut last_leader = Instant::now();
    let mut last_tick = Instant::now();
    let mut window = (Instant::now(), 0u32, 0usize);
    let mut pending = (0u32, 0usize);
    let flush = |pending: &mut (u32, usize)| -> bool {
        if pending.0 == 0 {
            return true;
        }
        let ok = conn
            .audit
            .append(
                "worker.attach_input",
                json!({"link": conn.name, "run": run, "msgs": pending.0, "bytes": pending.1}),
            )
            .is_ok();
        *pending = (0, 0);
        ok
    };
    let end = |chan: &mut dyn Channel, why: &str, pending: &mut (u32, usize), stats: Stats| {
        let _ = flush(pending);
        say(chan, attach::encode_closed(why));
        (why.to_string(), stats)
    };
    loop {
        let mut forwarded = 0;
        loop {
            let wait = if forwarded == 0 {
                Duration::from_millis(15)
            } else {
                Duration::ZERO
            };
            match session.recv(wait) {
                AttachRecv::Msg(m) => {
                    forwarded += 1;
                    match *m {
                        ServerMsg::Closed { reason } => {
                            return end(chan, &format!("runner: {reason}"), &mut pending, stats)
                        }
                        other => match forward(chan, run, other) {
                            Ok(true) => {}
                            Ok(false) => return ("the leader went away".into(), stats),
                            Err(why) => {
                                return end(
                                    chan,
                                    &format!("bad screen: {why}"),
                                    &mut pending,
                                    stats,
                                )
                            }
                        },
                    }
                    if forwarded >= 64 {
                        break;
                    }
                }
                AttachRecv::Idle => break,
                AttachRecv::Closed => {
                    return end(chan, "the run's stream ended", &mut pending, stats)
                }
            }
        }
        match chan.poll(Duration::from_millis(5)) {
            Ok(Poll::Frame(frame)) => {
                last_leader = Instant::now();
                match attach::parse_outgoing(&frame) {
                    Err(why) => {
                        return end(chan, &format!("protocol error: {why}"), &mut pending, stats)
                    }
                    Ok(Outgoing::Ping) => {
                        say(chan, attach::encode_pong());
                    }
                    Ok(Outgoing::Detach) => return end(chan, "detached", &mut pending, stats),
                    Ok(Outgoing::Scroll(offset)) => {
                        if session.scroll(offset).is_err() {
                            return end(chan, "the runner went away", &mut pending, stats);
                        }
                    }
                    Ok(Outgoing::Resize { cols, rows }) => {
                        if input_allowed {
                            if !input_still_allowed(conn) {
                                let _ = conn.record(
                                    "attach.resize",
                                    json!({"run": run}),
                                    "rejected",
                                    "attach input is no longer allowed",
                                );
                                return end(
                                    chan,
                                    "attach input is no longer allowed",
                                    &mut pending,
                                    stats,
                                );
                            }
                            if session.resize(cols, rows).is_err() {
                                return end(chan, "the runner went away", &mut pending, stats);
                            }
                        }
                    }
                    Ok(Outgoing::Input(text)) => {
                        if !input_allowed {
                            let _ = conn.record(
                                "attach.input",
                                json!({"run": run}),
                                "rejected",
                                "input on a read-only attach",
                            );
                            return end(chan, "input is not allowed here", &mut pending, stats);
                        }
                        match conn.current() {
                            Ok(Ok(s)) if s.allow_attach_input => {}
                            _ => {
                                let _ = conn.record(
                                    "attach.input",
                                    json!({"run": run}),
                                    "rejected",
                                    "attach input is no longer allowed",
                                );
                                return end(
                                    chan,
                                    "attach input is no longer allowed",
                                    &mut pending,
                                    stats,
                                );
                            }
                        }
                        let now = Instant::now();
                        if now.duration_since(window.0) >= Duration::from_secs(1) {
                            window = (now, 0, 0);
                        }
                        window.1 += 1;
                        window.2 += text.len();
                        if window.1 > INPUT_MSGS_PER_SEC || window.2 > INPUT_BYTES_PER_SEC {
                            say(
                                chan,
                                attach::encode_notice("typing too fast, input dropped"),
                            );
                            continue;
                        }
                        match with_backend(conn, st, false, None, |b| b.send(run, &text)) {
                            Ok(()) => {
                                stats.msgs += 1;
                                stats.bytes += text.len() as u64;
                                pending.0 += 1;
                                pending.1 += text.len();
                            }
                            Err(BackendError::Busy) => {
                                say(
                                    chan,
                                    attach::encode_notice(
                                        "the agent is not reading its input, try again",
                                    ),
                                );
                            }
                            Err(e) => {
                                return end(
                                    chan,
                                    &format!("input failed: {e}"),
                                    &mut pending,
                                    stats,
                                )
                            }
                        }
                    }
                }
            }
            Ok(Poll::Idle) => {}
            Ok(Poll::Closed) => return ("the leader disconnected".into(), stats),
            Err(e) => return (format!("channel: {e}"), stats),
        }
        if last_leader.elapsed() > ATTACH_IDLE {
            return end(chan, "idle", &mut pending, stats);
        }
        if last_tick.elapsed() >= TICK {
            last_tick = Instant::now();
            if !flush(&mut pending) {
                return end(chan, "audit unavailable", &mut pending, stats);
            }
            match conn.current() {
                Ok(Ok(_)) => {}
                _ => return end(chan, "this link is no longer trusted", &mut pending, stats),
            }
            if owned(conn, run) != Some(true) {
                return end(chan, "the run is no longer yours", &mut pending, stats);
            }
        }
    }
}
