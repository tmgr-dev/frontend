use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tmgr_ipc::api::{Origin, RunCreateReq, SessionInfo, State, StateKind};
use tmgr_ipc::proto::ServerMsg;
use tmgr_link::attach::{self, Incoming, Outgoing};
use tmgr_link::audit::Audit;
use tmgr_link::proto::{self, Channel, Framed, Timed, MAX_FRAME};
use tmgr_link::runner::{AttachRecv, AttachSession, Backend, BackendError};
use tmgr_link::scope::Scope;
use tmgr_link::store::{Leader, Paths, Store};
use tmgr_link::taskref::LocalRefs;
use tmgr_link::worker::{serve_with, Env, Limits, Runtime};

const E1: &str = "0123456789abcdef0123456789abcdef";
const E2: &str = "fedcba9876543210fedcba9876543210";
const E3: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[derive(Default)]
struct FakeState {
    sessions: Vec<SessionInfo>,
    calls: Vec<String>,
    creates: Vec<RunCreateReq>,
    sends: Vec<(String, String)>,
    busy: bool,
    attach: VecDeque<ServerMsg>,
    attach_resizes: Vec<(u16, u16)>,
    attach_sizes: Vec<Option<(u16, u16)>>,
    exit_on: Option<&'static str>,
    lose_create_reply: bool,
    link_changed_once: bool,
    revoke_fails: bool,
    revoke_noop: bool,
    revokes: Vec<(String, u64)>,
    refuse_create: Option<&'static str>,
    next: u32,
}

#[derive(Clone, Default)]
struct Fake(Arc<Mutex<FakeState>>);

impl Fake {
    fn calls(&self) -> Vec<String> {
        self.0.lock().unwrap().calls.clone()
    }

    fn called(&self, name: &str) -> usize {
        self.calls().iter().filter(|c| *c == name).count()
    }

    fn exit(&self, id: &str) {
        let mut s = self.0.lock().unwrap();
        for sess in s.sessions.iter_mut().filter(|x| x.id == id) {
            sess.state.kind = StateKind::Exited;
        }
    }

    fn push(&self, msg: ServerMsg) {
        self.0.lock().unwrap().attach.push_back(msg);
    }
}

fn info(id: &str, req: &RunCreateReq) -> SessionInfo {
    SessionInfo {
        id: id.into(),
        name: req.name.clone().unwrap_or_default(),
        argv: vec!["/usr/bin/true".into(), "--secret-flag".into()],
        cwd: req.cwd.clone(),
        profile: Some(req.profile.clone()),
        state: State {
            kind: StateKind::Working,
            since_ms: 10,
            detail: None,
            exit_code: None,
        },
        created_ms: tmgr_link::ownership::now_ms(),
        cols: req.cols,
        rows: req.rows,
        shim_pid: 4242,
        child_pid: 4243,
        task: req.task,
        bundle_sha256: None,
        origin: req.origin.clone(),
    }
}

impl Backend for Fake {
    fn list(&mut self) -> Result<Vec<SessionInfo>, BackendError> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("sessions.list".into());
        Ok(s.sessions.clone())
    }

    fn create_run(&mut self, req: &RunCreateReq) -> Result<SessionInfo, BackendError> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("runs.create".into());
        if std::mem::take(&mut s.link_changed_once) {
            return Err(BackendError::Refused {
                code: "forbidden".into(),
                message: "link changed; re-request".into(),
            });
        }
        if let Some(code) = s.refuse_create {
            return Err(BackendError::Refused {
                code: code.into(),
                message: "no".into(),
            });
        }
        s.creates.push(req.clone());
        s.next += 1;
        let i = info(&format!("s_run{:06}", s.next), req);
        s.sessions.push(i.clone());
        if s.lose_create_reply {
            return Err(BackendError::Unavailable("connection lost".into()));
        }
        Ok(i)
    }

    fn kill(&mut self, id: &str, signal: &str) -> Result<(), BackendError> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("sessions.kill".into());
        if s.exit_on == Some(signal) || s.exit_on.is_none() {
            for sess in s.sessions.iter_mut().filter(|x| x.id == id) {
                sess.state.kind = StateKind::Exited;
            }
        }
        Ok(())
    }

    fn origin_revoke(&mut self, link: &str, generation: u64) -> Result<u32, BackendError> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("origin.revoke".into());
        s.revokes.push((link.to_string(), generation));
        if s.revoke_fails {
            return Err(BackendError::Unavailable("gone".into()));
        }
        if s.revoke_noop {
            return Ok(0);
        }
        let mut killed = 0;
        for sess in s.sessions.iter_mut() {
            let hit = sess
                .origin
                .as_ref()
                .is_some_and(|o| o.link == link && o.generation <= generation);
            if hit && sess.state.kind != StateKind::Exited {
                sess.state.kind = StateKind::Exited;
                killed += 1;
            }
        }
        Ok(killed)
    }

    fn remove(&mut self, id: &str) -> Result<(), BackendError> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("sessions.remove".into());
        s.sessions.retain(|x| x.id != id);
        Ok(())
    }

    fn send(&mut self, id: &str, text: &str) -> Result<(), BackendError> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("sessions.send".into());
        if s.busy {
            return Err(BackendError::Busy);
        }
        s.sends.push((id.into(), text.into()));
        Ok(())
    }

    fn attach(
        &mut self,
        _id: &str,
        size: Option<(u16, u16)>,
    ) -> Result<Box<dyn AttachSession>, BackendError> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("attach.open".into());
        s.attach_sizes.push(size);
        drop(s);
        Ok(Box::new(FakeAttach(self.0.clone())))
    }
}

struct FakeAttach(Arc<Mutex<FakeState>>);

impl AttachSession for FakeAttach {
    fn recv(&mut self, wait: Duration) -> AttachRecv {
        if let Some(m) = self.0.lock().unwrap().attach.pop_front() {
            return AttachRecv::Msg(Box::new(m));
        }
        std::thread::sleep(wait.min(Duration::from_millis(5)));
        AttachRecv::Idle
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), BackendError> {
        self.0.lock().unwrap().attach_resizes.push((cols, rows));
        Ok(())
    }

    fn scroll(&mut self, _offset: u32) -> Result<(), BackendError> {
        Ok(())
    }

    fn detach(&mut self) {}
}

struct World {
    _d: tempfile::TempDir,
    paths: Paths,
    repo: String,
    outside: String,
}

fn scope(repo: &str, input: bool) -> Scope {
    Scope {
        repos: vec![repo.to_string()],
        agents: vec!["codex".into()],
        models: vec![],
        max: 2,
        from: vec![],
        allow_attach_input: input,
    }
}

fn world() -> World {
    let d = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(d.path()).unwrap();
    let paths = Paths::under(&root);
    let repo = root.join("repo");
    let outside = root.join("outside");
    std::fs::create_dir(&repo).unwrap();
    std::fs::create_dir(&outside).unwrap();
    let repo = repo.to_str().unwrap().to_string();
    let outside = outside.to_str().unwrap().to_string();
    Store::update(&paths, |s| {
        for (name, enr) in [("laptop", E1), ("phone", E2)] {
            s.leaders.insert(
                name.into(),
                Leader {
                    key: "AAAA".into(),
                    fingerprint: format!("SHA256:{name}"),
                    added: "2026-10-13T00:00:00Z".into(),
                    scope: scope(&repo, false),
                    enrollment: enr.into(),
                    authorized_keys: String::new(),
                },
            );
        }
        Ok(())
    })
    .unwrap();
    World {
        _d: d,
        paths,
        repo,
        outside,
    }
}

impl World {
    fn set_scope(&self, name: &str, f: impl FnOnce(&mut Scope)) {
        Store::update(&self.paths, |s| {
            f(&mut s.leaders.get_mut(name).unwrap().scope);
            Ok(())
        })
        .unwrap();
    }

    fn audit(&self) -> Vec<Value> {
        Audit::new(&self.paths.audit_dir()).tail(500).unwrap()
    }
}

fn env() -> Env {
    Env {
        ssh_connection: Some("192.0.2.5 5555 192.0.2.1 22".into()),
        original_command: None,
    }
}

fn frames(items: &[Value]) -> Vec<u8> {
    let mut out = Vec::new();
    for v in items {
        let b = serde_json::to_vec(v).unwrap();
        out.extend_from_slice(&(b.len() as u32).to_be_bytes());
        out.extend_from_slice(&b);
    }
    out
}

fn session(w: &World, name: &str, enr: &str, fake: &Fake, items: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    {
        let connect = || Ok(Box::new(fake.clone()) as Box<dyn Backend>);
        let rt = Runtime {
            connect: &connect,
            resolver: &LocalRefs,
        };
        let mut chan = Framed::new(Cursor::new(frames(items)), &mut out);
        let _ = serve_with(
            &w.paths,
            name,
            enr,
            &env(),
            &mut chan,
            &Limits::default(),
            &rt,
        );
    }
    let mut replies = Vec::new();
    let mut cur = Cursor::new(out);
    while let Ok(Some(frame)) = proto::read_frame(&mut cur, MAX_FRAME) {
        replies.push(serde_json::from_slice(&frame).unwrap());
    }
    replies
}

fn create(w: &World, extra: Value) -> Value {
    let mut p = json!({"task":"w1:7","repo":w.repo,"agent":"codex"});
    for (k, v) in extra.as_object().unwrap() {
        p[k] = v.clone();
    }
    json!({"id":1,"method":"runs.create","params":p})
}

fn run_id(reply: &Value) -> String {
    reply["result"]["id"].as_str().unwrap().to_string()
}

#[test]
fn a_create_reaches_only_the_profile_based_runner_call() {
    let w = world();
    let fake = Fake::default();
    let r = session(
        &w,
        "laptop",
        E1,
        &fake,
        &[create(&w, json!({"model":"m1"}))],
    );
    assert!(r[0]["result"]["id"].is_string(), "{:?}", r[0]);
    assert_eq!(fake.calls(), ["sessions.list", "runs.create"]);
    let s = fake.0.lock().unwrap();
    let req = &s.creates[0];
    assert_eq!(req.profile, "codex");
    assert_eq!(req.cwd, w.repo);
    assert_eq!(req.model.as_deref(), Some("m1"));
    let t = req.task.unwrap();
    assert_eq!((t.workspace_id, t.task_id), (-1, 7));
    assert!(req.name.as_deref().unwrap().starts_with("link-laptop-"));
    let shown = r[0].to_string();
    assert!(!shown.contains("secret-flag") && !shown.contains("4242"));
}

#[test]
fn the_single_repo_of_a_scope_is_the_default() {
    let w = world();
    let fake = Fake::default();
    let req = json!({"id":1,"method":"runs.create","params":{"task":"w1:7","agent":"codex"}});
    let r = session(&w, "laptop", E1, &fake, &[req]);
    assert!(r[0]["result"]["id"].is_string(), "{:?}", r[0]);
    assert_eq!(fake.0.lock().unwrap().creates[0].cwd, w.repo);
}

#[test]
fn nothing_outside_the_scope_reaches_the_runner() {
    let w = world();
    std::os::unix::fs::symlink(&w.outside, format!("{}/escape", w.repo)).unwrap();
    let fake = Fake::default();
    let dotdot = format!("{}/../outside", w.repo);
    let cases = [
        create(&w, json!({"repo": w.outside})),
        create(&w, json!({"repo": dotdot})),
        create(&w, json!({"repo": format!("{}/escape", w.repo)})),
        create(&w, json!({"repo": "/nonexistent-dir-x"})),
        create(&w, json!({"repo": "relative"})),
        create(&w, json!({"agent": "claude-code"})),
        create(&w, json!({"agent": "../codex"})),
        create(&w, json!({"mode": "bypass"})),
        create(&w, json!({"mode": "yolo"})),
        create(&w, json!({"on": "spark"})),
        create(&w, json!({"task": "TM-512"})),
        create(&w, json!({"task": "#8974"})),
        create(&w, json!({"task": "w0:1"})),
        create(&w, json!({"task": "w1:7", "task_hash": "ab"})),
        create(&w, json!({"task_hash": "a".repeat(64)})),
    ];
    let r = session(&w, "laptop", E1, &fake, &cases);
    assert_eq!(r.len(), cases.len());
    for (i, rep) in r.iter().enumerate() {
        assert!(rep["error"].is_string(), "case {i}: {rep}");
    }
    assert_eq!(fake.called("runs.create"), 0, "{:?}", fake.calls());
    assert_eq!(fake.called("sessions.list"), 0);
    let decisions: Vec<_> = w
        .audit()
        .iter()
        .filter(|e| e["event"] == "worker.request")
        .map(|e| e["decision"].as_str().unwrap().to_string())
        .collect();
    assert!(decisions.iter().all(|d| d == "rejected"), "{decisions:?}");
}

#[test]
fn a_model_outside_the_allowlist_is_refused_and_an_allowed_one_passes() {
    let w = world();
    w.set_scope("laptop", |s| s.models = vec!["m1".into()]);
    let fake = Fake::default();
    let r = session(
        &w,
        "laptop",
        E1,
        &fake,
        &[
            create(&w, json!({"model":"m2"})),
            create(&w, json!({})),
            create(&w, json!({"model":"m1"})),
        ],
    );
    assert_eq!(r[0]["error"], "forbidden");
    assert_eq!(r[1]["error"], "forbidden");
    assert!(r[2]["result"]["id"].is_string());
    assert_eq!(fake.called("runs.create"), 1);
}

#[test]
fn raw_argv_and_foreign_parameters_cannot_be_smuggled() {
    let w = world();
    let fake = Fake::default();
    let mut items = Vec::new();
    for key in [
        "argv", "cmd", "command", "cwd", "env", "profile", "text", "prompt", "name", "cols",
        "env_pass",
    ] {
        items.push(create(&w, json!({ key: "x" })));
    }
    items.push(
        json!({"id":9,"method":"sessions.create","params":{"argv":["/bin/sh"],"cwd":w.repo}}),
    );
    items.push(json!({"id":10,"method":"sessions.send","params":{"id":"s_x","text":"rm"}}));
    let r = session(&w, "laptop", E1, &fake, &items);
    for rep in &r {
        assert!(
            ["bad_params", "forbidden", "unknown_method"].contains(&rep["error"].as_str().unwrap()),
            "{rep}"
        );
    }
    assert!(fake.calls().is_empty(), "{:?}", fake.calls());
}

#[test]
fn an_unverifiable_content_hash_is_refused() {
    let w = world();
    let fake = Fake::default();
    let r = session(
        &w,
        "laptop",
        E1,
        &fake,
        &[create(&w, json!({"task_hash": "a".repeat(64)}))],
    );
    assert_eq!(r[0]["error"], "forbidden");
    assert!(r[0]["message"]
        .as_str()
        .unwrap()
        .contains("cannot be verified"));
    assert_eq!(fake.called("runs.create"), 0);
}

#[test]
fn the_max_counts_this_links_live_runs_only() {
    let w = world();
    let fake = Fake::default();
    let r = session(
        &w,
        "laptop",
        E1,
        &fake,
        &[create(&w, json!({})), create(&w, json!({}))],
    );
    let first = run_id(&r[0]);
    assert!(r[1]["result"]["id"].is_string());
    let third = session(&w, "laptop", E1, &fake, &[create(&w, json!({}))]);
    assert_eq!(third[0]["error"], "forbidden");
    assert!(third[0]["message"].as_str().unwrap().contains("max"));
    let other = session(&w, "phone", E2, &fake, &[create(&w, json!({}))]);
    assert!(other[0]["result"]["id"].is_string());
    fake.exit(&first);
    let again = session(&w, "laptop", E1, &fake, &[create(&w, json!({}))]);
    assert!(again[0]["result"]["id"].is_string(), "{:?}", again[0]);
    assert_eq!(fake.called("runs.create"), 4);
}

#[test]
fn two_links_never_see_or_touch_each_others_runs() {
    let w = world();
    let fake = Fake::default();
    let mine = session(&w, "laptop", E1, &fake, &[create(&w, json!({}))]);
    let id = run_id(&mine[0]);
    let theirs = session(
        &w,
        "phone",
        E2,
        &fake,
        &[create(&w, json!({"task":"w1:8"}))],
    );
    let other = run_id(&theirs[0]);
    let view = session(
        &w,
        "phone",
        E2,
        &fake,
        &[
            json!({"id":1,"method":"runs.list"}),
            json!({"id":2,"method":"runs.get","params":{"run":id}}),
            json!({"id":3,"method":"runs.halt","params":{"run":id}}),
            json!({"id":4,"method":"attach.open","params":{"run":id}}),
        ],
    );
    let ids: Vec<_> = view[0]["result"]["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, std::slice::from_ref(&other));
    for rep in &view[1..] {
        assert_eq!(rep["error"], "not_found", "{rep}");
    }
    assert_eq!(fake.called("sessions.kill"), 0);
    assert_eq!(fake.called("attach.open"), 0);
    let back = session(
        &w,
        "laptop",
        E1,
        &fake,
        &[json!({"id":1,"method":"runs.list"})],
    );
    assert_eq!(back[0]["result"]["runs"].as_array().unwrap().len(), 1);
    assert_eq!(back[0]["result"]["runs"][0]["id"], json!(id));
}

#[test]
fn a_re_enrolled_link_cannot_see_runs_of_its_earlier_enrollment() {
    let w = world();
    let fake = Fake::default();
    let made = session(&w, "laptop", E1, &fake, &[create(&w, json!({}))]);
    let id = run_id(&made[0]);
    Store::update(&w.paths, |s| {
        s.leaders.get_mut("laptop").unwrap().enrollment = E3.into();
        Ok(())
    })
    .unwrap();
    let r = session(
        &w,
        "laptop",
        E3,
        &fake,
        &[
            json!({"id":1,"method":"runs.list"}),
            json!({"id":2,"method":"runs.halt","params":{"run":id}}),
        ],
    );
    assert_eq!(r[0]["result"]["runs"].as_array().unwrap().len(), 0);
    assert_eq!(r[1]["error"], "not_found");
    assert_eq!(fake.called("sessions.kill"), 0);
}

#[test]
fn halt_stops_an_owned_run_and_is_audited() {
    let w = world();
    let fake = Fake::default();
    let made = session(&w, "laptop", E1, &fake, &[create(&w, json!({}))]);
    let id = run_id(&made[0]);
    let r = session(
        &w,
        "laptop",
        E1,
        &fake,
        &[json!({"id":1,"method":"runs.halt","params":{"run":id}})],
    );
    assert_eq!(r[0]["result"]["signal"], "int");
    assert_eq!(fake.called("sessions.kill"), 1);
    assert!(w.audit().iter().any(|e| e["event"] == "worker.run_halted"));
}

#[test]
fn every_decision_is_audited_with_the_task_and_profile() {
    let w = world();
    let fake = Fake::default();
    let _ = session(
        &w,
        "laptop",
        E1,
        &fake,
        &[
            create(&w, json!({"persona":"Builder"})),
            create(&w, json!({"agent":"x1"})),
        ],
    );
    let ev = w.audit();
    let accepted = ev
        .iter()
        .find(|e| e["event"] == "worker.request" && e["decision"] == "accepted")
        .unwrap();
    assert_eq!(accepted["args"]["task_ref"], "w1:7");
    assert_eq!(accepted["args"]["agent"], "codex");
    assert_eq!(accepted["args"]["mode"], "default");
    assert_eq!(accepted["args"]["persona"], "Builder");
    let created = ev
        .iter()
        .find(|e| e["event"] == "worker.run_created")
        .unwrap();
    assert_eq!(created["persona"], "Builder");
    assert!(ev
        .iter()
        .any(|e| e["event"] == "worker.request" && e["decision"] == "rejected"));
    assert!(Audit::new(&w.paths.audit_dir()).verify().unwrap().is_ok());
}

struct Live {
    tx: Timed,
    worker: std::thread::JoinHandle<()>,
}

fn live(w: &World, name: &'static str, enr: &'static str, fake: &Fake) -> Live {
    let (w_in_r, l_out_w) = std::io::pipe().unwrap();
    let (l_in_r, w_out_w) = std::io::pipe().unwrap();
    let paths = w.paths.clone();
    let fake = fake.clone();
    let worker = std::thread::spawn(move || {
        let connect = || Ok(Box::new(fake.clone()) as Box<dyn Backend>);
        let rt = Runtime {
            connect: &connect,
            resolver: &LocalRefs,
        };
        let mut chan = Timed::spawn(
            w_in_r,
            w_out_w,
            Duration::from_secs(10),
            Duration::from_secs(60),
            MAX_FRAME,
        );
        let _ = serve_with(
            &paths,
            name,
            enr,
            &env(),
            &mut chan,
            &Limits::default(),
            &rt,
        );
    });
    Live {
        tx: Timed::spawn(
            l_in_r,
            l_out_w,
            Duration::from_secs(10),
            Duration::from_secs(60),
            MAX_FRAME,
        ),
        worker,
    }
}

impl Live {
    fn call(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.tx.send(&proto::request(id, method, params)).unwrap();
        let bytes = self.tx.recv().unwrap().expect("a reply");
        serde_json::from_slice(&bytes).unwrap()
    }

    fn next(&mut self, run: &str) -> Option<Incoming> {
        let end = Instant::now() + Duration::from_secs(8);
        while Instant::now() < end {
            match self.tx.poll(Duration::from_millis(50)).unwrap() {
                proto::Poll::Frame(f) => {
                    if let Some(m) = attach::decode_incoming(&f, run).unwrap() {
                        return Some(m);
                    }
                }
                proto::Poll::Idle => {}
                proto::Poll::Closed => return None,
            }
        }
        None
    }

    fn say(&mut self, m: Outgoing) {
        let _ = self.tx.send(&attach::encode_outgoing(&m));
    }

    fn finish(self) {
        drop(self.tx);
        self.worker.join().unwrap();
    }
}

fn screen(run: &str, text: &str) -> ServerMsg {
    ServerMsg::Screen(json!({
        "session": run, "full": true,
        "snapshot": {"r":1,"c":40,"l":[[{"a":{"f":null,"b":null,"x":0},"t":text}]],
                     "cu":[0,1],"m":{"ac":false,"bp":false,"alt":false,"hc":false},"so":0,"sm":0},
        "lines": [], "cursor":[0,1], "modes": null, "scroll_offset":0, "scroll_max":0
    }))
}

fn started(w: &World, fake: &Fake) -> (Live, String) {
    let made = session(w, "laptop", E1, fake, &[create(w, json!({}))]);
    let id = run_id(&made[0]);
    (live(w, "laptop", E1, fake), id)
}

#[test]
fn a_read_only_attach_streams_sanitised_screens_and_hides_other_sessions() {
    let w = world();
    let fake = Fake::default();
    let (mut l, id) = started(&w, &fake);
    let r = l.call(1, "attach.open", json!({"run": id, "cols": 80, "rows": 24}));
    assert_eq!(r["result"]["input"], false);
    assert_eq!(fake.0.lock().unwrap().attach_sizes, [None]);
    fake.push(ServerMsg::Event(json!({
        "session": "s_zzzzzzzzzz",
        "state": {"kind":"blocked","since_ms":1,"detail":"someone else","exit_code":null}
    })));
    fake.push(screen(&id, "\u{1b}]52;c;aGk=\u{7}\u{1b}[2Jhello\u{9b}31m"));
    match l.next(&id).expect("a screen") {
        Incoming::Screen(p) => {
            let t = &p.snapshot.unwrap().lines[0].spans[0].t;
            assert!(
                t.contains("hello") && !t.contains('\u{1b}') && !t.contains('\u{9b}'),
                "{t:?}"
            );
        }
        other => panic!("{other:?}"),
    }
    l.say(Outgoing::Input("rm -rf\r".into()));
    match l.next(&id) {
        Some(Incoming::Closed(why)) => assert!(why.contains("not allowed"), "{why}"),
        other => panic!("{other:?}"),
    }
    l.finish();
    assert!(fake.0.lock().unwrap().sends.is_empty());
    assert_eq!(fake.0.lock().unwrap().attach_resizes.len(), 0);
    let ev = w.audit();
    let open = ev
        .iter()
        .find(|e| e["args"]["run"] == json!(id) && e["method"] == "attach.open")
        .unwrap();
    assert_eq!(open["args"]["mode"], "read-only");
    assert_eq!(open["decision"], "accepted");
    assert!(ev.iter().any(|e| e["event"] == "worker.attach_end"));
}

#[test]
fn input_flows_only_with_the_scope_flag_and_busy_becomes_a_notice() {
    let w = world();
    w.set_scope("laptop", |s| s.allow_attach_input = true);
    let fake = Fake::default();
    let (mut l, id) = started(&w, &fake);
    let r = l.call(1, "attach.open", json!({"run": id}));
    assert_eq!(r["result"]["input"], true);
    assert_eq!(fake.0.lock().unwrap().attach_sizes, [Some((120, 40))]);
    l.say(Outgoing::Input("ls\r".into()));
    l.say(Outgoing::Ping);
    assert_eq!(l.next(&id), Some(Incoming::Pong));
    assert_eq!(
        fake.0.lock().unwrap().sends,
        [(id.clone(), "ls\r".to_string())]
    );
    fake.0.lock().unwrap().busy = true;
    l.say(Outgoing::Input("x".into()));
    match l.next(&id) {
        Some(Incoming::Notice(n)) => assert!(n.contains("not reading"), "{n}"),
        other => panic!("{other:?}"),
    }
    l.say(Outgoing::Resize {
        cols: 100,
        rows: 30,
    });
    l.say(Outgoing::Detach);
    match l.next(&id) {
        Some(Incoming::Closed(why)) => assert_eq!(why, "detached"),
        other => panic!("{other:?}"),
    }
    l.finish();
    assert_eq!(fake.0.lock().unwrap().attach_resizes, [(100, 30)]);
    let ev = w.audit();
    let end = ev
        .iter()
        .find(|e| e["event"] == "worker.attach_end")
        .unwrap();
    assert_eq!(end["input_msgs"], 1);
    assert_eq!(end["mode"], "interactive");
}

#[test]
fn asking_for_read_only_is_honoured_even_when_input_is_allowed() {
    let w = world();
    w.set_scope("laptop", |s| s.allow_attach_input = true);
    let fake = Fake::default();
    let (mut l, id) = started(&w, &fake);
    let r = l.call(1, "attach.open", json!({"run": id, "read_only": true}));
    assert_eq!(r["result"]["input"], false);
    l.say(Outgoing::Input("x".into()));
    assert!(matches!(l.next(&id), Some(Incoming::Closed(_))));
    l.finish();
    assert!(fake.0.lock().unwrap().sends.is_empty());
}

#[test]
fn narrowing_the_scope_mid_stream_stops_input() {
    let w = world();
    w.set_scope("laptop", |s| s.allow_attach_input = true);
    let fake = Fake::default();
    let (mut l, id) = started(&w, &fake);
    l.call(1, "attach.open", json!({"run": id}));
    w.set_scope("laptop", |s| s.allow_attach_input = false);
    l.say(Outgoing::Input("x".into()));
    match l.next(&id) {
        Some(Incoming::Closed(why)) => assert!(why.contains("no longer allowed"), "{why}"),
        other => panic!("{other:?}"),
    }
    l.finish();
    assert!(fake.0.lock().unwrap().sends.is_empty());
}

#[test]
fn revoking_mid_session_refuses_the_next_request_before_the_runner() {
    let w = world();
    let fake = Fake::default();
    let mut l = live(&w, "laptop", E1, &fake);
    let hello = l.call(1, "hello", json!({"protocol": 1}));
    assert_eq!(hello["result"]["role"], "follower");
    tmgr_link::follower::revoke(&w.paths, "laptop").unwrap();
    let r = l.call(2, "runs.create", create(&w, json!({}))["params"].clone());
    assert_eq!(r["error"], "revoked");
    l.finish();
    assert_eq!(fake.called("runs.create"), 0);
}

#[test]
fn contain_halts_the_runs_of_that_link_only_and_keeps_others() {
    let w = world();
    let fake = Fake::default();
    let a = session(&w, "laptop", E1, &fake, &[create(&w, json!({}))]);
    let b = session(&w, "phone", E2, &fake, &[create(&w, json!({}))]);
    let (a, b) = (run_id(&a[0]), run_id(&b[0]));
    tmgr_link::follower::revoke(&w.paths, "laptop").unwrap();
    let mut backend = fake.clone();
    let out = tmgr_link::follower::contain(&w.paths, "laptop", Some(&mut backend), true).unwrap();
    assert_eq!(out.runs_halted, 1);
    let live: Vec<_> = fake
        .0
        .lock()
        .unwrap()
        .sessions
        .iter()
        .filter(|s| s.state.kind != StateKind::Exited)
        .map(|s| s.id.clone())
        .collect();
    assert_eq!(live, std::slice::from_ref(&b));
    assert_ne!(a, b);
    let labels = tmgr_link::ownership::origin_labels(&w.paths);
    assert_eq!(labels[&a], "link:laptop (revoked)");
    assert_eq!(labels[&b], "link:phone");
}

#[test]
fn runs_that_keep_going_after_a_revoke_are_labelled() {
    let w = world();
    let fake = Fake::default();
    let a = session(&w, "laptop", E1, &fake, &[create(&w, json!({}))]);
    let a = run_id(&a[0]);
    assert_eq!(
        tmgr_link::ownership::origin_label(&w.paths, &a).as_deref(),
        Some("link:laptop")
    );
    tmgr_link::follower::revoke(&w.paths, "laptop").unwrap();
    let out = tmgr_link::follower::contain(&w.paths, "laptop", None, false).unwrap();
    assert_eq!((out.runs_known, out.runs_halted), (1, 0));
    assert_eq!(fake.called("sessions.kill"), 0);
    assert_eq!(
        tmgr_link::ownership::origin_label(&w.paths, &a).as_deref(),
        Some("link:laptop (revoked)")
    );
}

#[test]
fn a_runner_limit_gets_a_cleanup_message() {
    let w = world();
    let fake = Fake::default();
    fake.0.lock().unwrap().refuse_create = Some("limit");
    let r = session(&w, "laptop", E1, &fake, &[create(&w, json!({}))]);
    assert_eq!(r[0]["error"], "limit");
    assert!(r[0]["message"]
        .as_str()
        .unwrap()
        .contains("clean up exited runs"));
}

#[test]
fn the_current_generation_travels_as_the_origin() {
    let w = world();
    Store::update(&w.paths, |s| {
        s.generations.insert("laptop".into(), 7);
        Ok(())
    })
    .unwrap();
    let fake = Fake::default();
    let r = session(&w, "laptop", E1, &fake, &[create(&w, json!({}))]);
    assert!(r[0]["result"]["id"].is_string(), "{:?}", r[0]);
    let origin = fake.0.lock().unwrap().creates[0].origin.clone();
    assert_eq!(
        origin,
        Some(Origin {
            link: "laptop".into(),
            generation: 7
        })
    );
}

#[test]
fn a_changed_link_answer_is_retried_once() {
    let w = world();
    let fake = Fake::default();
    fake.0.lock().unwrap().link_changed_once = true;
    let r = session(&w, "laptop", E1, &fake, &[create(&w, json!({}))]);
    assert!(r[0]["result"]["id"].is_string(), "{:?}", r[0]);
    assert_eq!(fake.called("runs.create"), 2);
}

#[test]
fn a_run_whose_reply_was_lost_is_adopted_only_by_the_same_enrollment_and_generation() {
    let w = world();
    let fake = Fake::default();
    fake.0.lock().unwrap().lose_create_reply = true;
    let r = session(&w, "laptop", E1, &fake, &[create(&w, json!({}))]);
    assert_eq!(r[0]["error"], "not_available");
    fake.0.lock().unwrap().lose_create_reply = false;
    let lost = fake.0.lock().unwrap().sessions[0].id.clone();
    let phone = session(
        &w,
        "phone",
        E2,
        &fake,
        &[json!({"id":1,"method":"runs.list"})],
    );
    assert_eq!(phone[0]["result"]["runs"].as_array().unwrap().len(), 0);
    let list = session(
        &w,
        "laptop",
        E1,
        &fake,
        &[json!({"id":1,"method":"runs.list"})],
    );
    assert_eq!(list[0]["result"]["runs"][0]["id"], json!(lost));
    assert_eq!(list[0]["result"]["runs"][0]["task"], "w1:7");

    let w2 = world();
    let fake2 = Fake::default();
    fake2.0.lock().unwrap().lose_create_reply = true;
    session(&w2, "laptop", E1, &fake2, &[create(&w2, json!({}))]);
    fake2.0.lock().unwrap().lose_create_reply = false;
    Store::update(&w2.paths, |s| {
        s.generations.insert("laptop".into(), 99);
        Ok(())
    })
    .unwrap();
    let later = session(
        &w2,
        "laptop",
        E1,
        &fake2,
        &[json!({"id":1,"method":"runs.list"})],
    );
    assert_eq!(later[0]["result"]["runs"].as_array().unwrap().len(), 0);
}

#[test]
fn contain_with_halt_asks_the_runner_to_kill_by_origin() {
    let w = world();
    let fake = Fake::default();
    let a = session(&w, "laptop", E1, &fake, &[create(&w, json!({}))]);
    let b = session(&w, "phone", E2, &fake, &[create(&w, json!({}))]);
    let (a, b) = (run_id(&a[0]), run_id(&b[0]));
    tmgr_link::follower::revoke(&w.paths, "laptop").unwrap();
    let mut backend = fake.clone();
    let out = tmgr_link::follower::contain(&w.paths, "laptop", Some(&mut backend), true).unwrap();
    assert_eq!(out.runs_halted, 1);
    assert_eq!(fake.0.lock().unwrap().revokes[0].0, "laptop");
    assert_eq!(fake.called("sessions.kill"), 0);
    let live: Vec<String> = fake
        .0
        .lock()
        .unwrap()
        .sessions
        .iter()
        .filter(|s| s.state.kind != StateKind::Exited)
        .map(|s| s.id.clone())
        .collect();
    assert_eq!(live, std::slice::from_ref(&b));
    assert_ne!(a, b);
    let mut again = fake.clone();
    let none = tmgr_link::follower::contain(&w.paths, "laptop", Some(&mut again), false).unwrap();
    assert_eq!(none.runs_halted, 0);
    assert_eq!(fake.0.lock().unwrap().revokes.len(), 1);
}

#[test]
fn exited_runs_of_the_link_are_reaped_before_the_next_create() {
    let w = world();
    let fake = Fake::default();
    let r = session(&w, "laptop", E1, &fake, &[create(&w, json!({}))]);
    let first = run_id(&r[0]);
    let mine_only = session(&w, "phone", E2, &fake, &[create(&w, json!({}))]);
    let theirs = run_id(&mine_only[0]);
    fake.exit(&first);
    fake.exit(&theirs);
    let again = session(&w, "laptop", E1, &fake, &[create(&w, json!({}))]);
    assert!(again[0]["result"]["id"].is_string());
    let ids: Vec<String> = fake
        .0
        .lock()
        .unwrap()
        .sessions
        .iter()
        .map(|s| s.id.clone())
        .collect();
    assert!(!ids.contains(&first), "own exited run is removed");
    assert!(ids.contains(&theirs), "another link's exited run stays");
    assert_eq!(fake.called("sessions.remove"), 1);
}

#[test]
fn resizing_after_the_scope_narrowed_closes_the_stream() {
    let w = world();
    w.set_scope("laptop", |s| s.allow_attach_input = true);
    let fake = Fake::default();
    let (mut l, id) = started(&w, &fake);
    l.call(1, "attach.open", json!({"run": id}));
    w.set_scope("laptop", |s| s.allow_attach_input = false);
    l.say(Outgoing::Resize { cols: 50, rows: 20 });
    assert!(matches!(l.next(&id), Some(Incoming::Closed(_))));
    l.finish();
    assert!(fake.0.lock().unwrap().attach_resizes.is_empty());
}

#[test]
fn revoke_fixes_the_cutoff_and_a_retry_never_touches_a_re_trusted_link() {
    let w = world();
    Store::update(&w.paths, |s| {
        s.generations.insert("laptop".into(), 5);
        Ok(())
    })
    .unwrap();
    let fake = Fake::default();
    let old = session(&w, "laptop", E1, &fake, &[create(&w, json!({}))]);
    let old = run_id(&old[0]);
    tmgr_link::follower::revoke(&w.paths, "laptop").unwrap();
    let tombs = tmgr_link::tombstone::for_link(&w.paths, "laptop").unwrap();
    assert_eq!((tombs[0].enrollment.as_str(), tombs[0].generation), (E1, 5));
    Store::update(&w.paths, |s| {
        let mut l = Leader {
            key: "AAAA".into(),
            fingerprint: "SHA256:again".into(),
            added: "2026-10-13T00:00:00Z".into(),
            scope: scope(&w.repo, false),
            enrollment: E3.into(),
            authorized_keys: String::new(),
        };
        l.scope.max = 2;
        s.leaders.insert("laptop".into(), l);
        s.generations.insert("laptop".into(), 40);
        Ok(())
    })
    .unwrap();
    let fresh = session(
        &w,
        "laptop",
        E3,
        &fake,
        &[create(&w, json!({"task": "w1:9"}))],
    );
    let fresh = run_id(&fresh[0]);
    let mut backend = fake.clone();
    let out = tmgr_link::follower::contain(&w.paths, "laptop", Some(&mut backend), true).unwrap();
    assert!(!out.incomplete(), "{out:?}");
    assert_eq!(fake.0.lock().unwrap().revokes, [("laptop".to_string(), 5)]);
    let alive: Vec<String> = fake
        .0
        .lock()
        .unwrap()
        .sessions
        .iter()
        .filter(|s| s.state.kind != StateKind::Exited)
        .map(|s| s.id.clone())
        .collect();
    assert_eq!(alive, std::slice::from_ref(&fresh));
    assert_ne!(old, fresh);
    assert!(tmgr_link::tombstone::for_link(&w.paths, "laptop")
        .unwrap()
        .is_empty());
}

#[test]
fn a_failed_or_missing_origin_revoke_is_incomplete_and_keeps_the_tombstone() {
    let w = world();
    let fake = Fake::default();
    session(&w, "laptop", E1, &fake, &[create(&w, json!({}))]);
    tmgr_link::follower::revoke(&w.paths, "laptop").unwrap();
    let none = tmgr_link::follower::contain(&w.paths, "laptop", None, true).unwrap();
    assert!(none.incomplete() && none.halt_failed == 1);
    fake.0.lock().unwrap().revoke_fails = true;
    let mut backend = fake.clone();
    let failed =
        tmgr_link::follower::contain(&w.paths, "laptop", Some(&mut backend), true).unwrap();
    assert!(failed.incomplete());
    fake.0.lock().unwrap().revoke_fails = false;
    fake.0.lock().unwrap().revoke_noop = true;
    let mut backend = fake.clone();
    let alive = tmgr_link::follower::contain(&w.paths, "laptop", Some(&mut backend), true).unwrap();
    assert_eq!(alive.runs_surviving, 1);
    assert!(alive.incomplete());
    assert_eq!(
        tmgr_link::tombstone::for_link(&w.paths, "laptop")
            .unwrap()
            .len(),
        1
    );
    fake.0.lock().unwrap().revoke_noop = false;
    let mut backend = fake.clone();
    let done = tmgr_link::follower::contain(&w.paths, "laptop", Some(&mut backend), true).unwrap();
    assert!(!done.incomplete());
    assert!(tmgr_link::tombstone::for_link(&w.paths, "laptop")
        .unwrap()
        .is_empty());
}

#[test]
fn a_damaged_trust_store_aborts_containment() {
    let w = world();
    tmgr_link::follower::revoke(&w.paths, "laptop").unwrap();
    tmgr_core::config::write_atomic(&w.paths.links_toml(), b"\x00 bad", 0o600).unwrap();
    let mut backend = Fake::default();
    assert!(tmgr_link::follower::contain(&w.paths, "laptop", Some(&mut backend), true).is_err());
    assert!(backend.0.lock().unwrap().revokes.is_empty());
}

#[test]
fn adoption_follows_the_generation_history_through_scope_edits_and_takes_exited_runs() {
    let w = world();
    let fake = Fake::default();
    fake.0.lock().unwrap().lose_create_reply = true;
    session(&w, "laptop", E1, &fake, &[create(&w, json!({}))]);
    fake.0.lock().unwrap().lose_create_reply = false;
    let lost = fake.0.lock().unwrap().sessions[0].id.clone();
    fake.exit(&lost);
    Store::update(&w.paths, |s| {
        s.generations.insert("laptop".into(), 12);
        s.generation_enrollments.insert(
            "laptop".into(),
            vec![
                tmgr_link::store::GenEnrollment {
                    generation: 0,
                    enrollment: E1.into(),
                },
                tmgr_link::store::GenEnrollment {
                    generation: 12,
                    enrollment: E1.into(),
                },
            ],
        );
        Ok(())
    })
    .unwrap();
    let phone = session(
        &w,
        "phone",
        E2,
        &fake,
        &[json!({"id":1,"method":"runs.list"})],
    );
    assert_eq!(phone[0]["result"]["runs"].as_array().unwrap().len(), 0);
    let list = session(
        &w,
        "laptop",
        E1,
        &fake,
        &[json!({"id":1,"method":"runs.list"})],
    );
    assert_eq!(list[0]["result"]["runs"][0]["id"], json!(lost));
    let again = session(&w, "laptop", E1, &fake, &[create(&w, json!({}))]);
    assert!(again[0]["result"]["id"].is_string());
    assert_eq!(fake.called("sessions.remove"), 1);
}
