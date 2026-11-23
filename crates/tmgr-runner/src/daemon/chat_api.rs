use super::manager::ManagerHost;
use super::*;
use crate::chat::{self, tail};
use crate::chat_remote;
use crate::manager_remote::Live;
use crate::mgr_rpc::dispatch::SEEN_FILE;
use crate::mgr_rpc::relay::{
    serve_tick, MachinesCache, RelayTransport, RpcClient, RpcInbox, RpcSend,
};
use crate::mgr_rpc::{Dispatcher, FileAudit};
use std::sync::mpsc::RecvTimeoutError as ChatRecv;
use tmgr_ipc::chat::{
    method as chat_method, CancelParams, HistoryParams, QueuedParams, SearchParams, SendParams,
};
use tmgr_ipc::manager::wrap_untrusted;

const CAPTURE_EVERY: Duration = Duration::from_secs(3);
const ACTIVITY_TICK: Duration = Duration::from_millis(250);
const QUEUE_EVERY: Duration = Duration::from_millis(500);
const RPC_TICK: Duration = Duration::from_millis(100);
pub(super) const RELAY_SOURCE: &str = "phone or another device via chat relay";
pub(super) const REMOTE_CLIENT: &str = "remote";
const REMOTE_UNRESTRICTED_REFUSAL: &str =
    "Not delivered: an unrestricted manager takes input only from this machine's keyboard.";
pub(super) const UNRESTRICTED_REFUSAL: &str =
    "Not delivered: an unrestricted manager takes input only from this machine's keyboard and the owner's paired devices.";

fn bad(msg: String) -> ApiErr {
    api(ErrorCode::InvalidParams, msg)
}

impl Runner {
    fn chat_target(&self, room: &tmgr_ipc::chat::RoomRef) -> Result<Arc<Session>, chat::Delivery> {
        let gone = |r: &str| chat::Delivery::Gone(r.into());
        let s = match room.session.as_deref() {
            Some(id) => self
                .session(id)
                .ok_or_else(|| gone("the session no longer exists"))?,
            None => self
                .live_manager()
                .ok_or_else(|| gone("no manager is running"))?,
        };
        if s.is_exited() {
            return Err(gone("the session has exited"));
        }
        if !s.has_shim() {
            return Err(chat::Delivery::Busy);
        }
        Ok(s)
    }

    fn chat_deliver(
        &self,
        room: &tmgr_ipc::chat::RoomRef,
        body: &str,
        client: &str,
    ) -> chat::Delivery {
        let t = match self.chat_target(room) {
            Ok(t) => t,
            Err(d) => return d,
        };
        match t.input_at_prompt(&chat::prompt_bytes(body), Instant::now()) {
            Ok(()) => {
                t.note_input_from(client);
                chat::Delivery::Delivered
            }
            Err(e) if e.code == ErrorCode::Busy => chat::Delivery::Busy,
            Err(e) => chat::Delivery::Gone(e.msg),
        }
    }

    fn relay_deliver(
        &self,
        room: &tmgr_ipc::chat::RoomRef,
        device: Option<&str>,
        owner_device: bool,
        body: &str,
    ) -> chat::Delivery {
        let target = match room.session.as_deref() {
            Some(id) => self.session(id),
            None => self.live_manager(),
        };
        if !owner_device && target.is_some_and(|s| super::manager::is_unrestricted(&s)) {
            return chat::Delivery::Refused(UNRESTRICTED_REFUSAL);
        }
        let t = match self.chat_target(room) {
            Ok(t) => t,
            Err(d) => return d,
        };
        if !owner_device && super::manager::is_unrestricted(&t) {
            return chat::Delivery::Refused(UNRESTRICTED_REFUSAL);
        }
        let typed = wrap_untrusted(RELAY_SOURCE, device.unwrap_or("unknown device"), body);
        match t.input_at_prompt(&chat::prompt_bytes(&typed), Instant::now()) {
            Ok(()) => {
                t.note_input_from("relay");
                chat::Delivery::Typed(typed)
            }
            Err(e) if e.code == ErrorCode::Busy => chat::Delivery::Busy,
            Err(e) => chat::Delivery::Gone(e.msg),
        }
    }

    pub(super) fn remote_deliver(
        &self,
        room: &tmgr_ipc::chat::RoomRef,
        body: &str,
    ) -> chat::Delivery {
        if self
            .live_manager()
            .is_some_and(|m| super::manager::is_unrestricted(&m))
        {
            return chat::Delivery::Refused(REMOTE_UNRESTRICTED_REFUSAL);
        }
        let t = match self.chat_target(room) {
            Ok(t) => t,
            Err(d) => return d,
        };
        match t.input_at_prompt(&chat::prompt_bytes(body), Instant::now()) {
            Ok(()) => {
                t.note_input_from(REMOTE_CLIENT);
                chat::Delivery::Delivered
            }
            Err(e) if e.code == ErrorCode::Busy => chat::Delivery::Busy,
            Err(e) => chat::Delivery::Gone(e.msg),
        }
    }

    pub(super) fn start_relay_pump(self: &Arc<Self>) {
        let Some(transport) = self.cfg.relay.clone() else {
            return;
        };
        let (ingest_rt, sub_rt, stop_rt) = (self.clone(), self.clone(), self.clone());
        let act_rt = self.clone();
        let rpc = Arc::new(RpcInbox::new());
        let (serve_rt, serve_inbox, device_rt) = (self.clone(), rpc.clone(), self.clone());
        let (rooms_rt, history_rt, owners_rt) = (self.clone(), self.clone(), self.clone());
        let setup = chat::relay::pump::Setup {
            dir: self.cfg.dir.clone(),
            machine: self.chat.machine().to_owned(),
            transport,
            ingest: Arc::new(
                move |m: chat::Incoming, target: Option<tmgr_ipc::chat::RoomRef>| {
                    if let Some(target) = target {
                        return ingest_rt.relay_mention(m, target);
                    }
                    let room = m.room.clone();
                    let device = m.device.clone();
                    let owner_device = m.owner_device;
                    ingest_rt.chat.ingest(
                        m,
                        now_ms(),
                        |s| ingest_rt.session(s).is_some(),
                        |body| {
                            ingest_rt.relay_deliver(&room, device.as_deref(), owner_device, body)
                        },
                    )
                },
            ),
            subscribe: Arc::new(move || sub_rt.chat.subscribe()),
            subscribe_activity: Arc::new(move || act_rt.chat.subscribe_activity()),
            stop: Arc::new(move || stop_rt.stop.load(Ordering::SeqCst)),
            rpc,
            spawnkey: self.start_spawnkey_intake(),
            spawnok: Some(self.start_spawnok_intake()),
            serve_rpc: Arc::new(move |send| serve_rt.spawn_rpc_server(serve_inbox.clone(), send)),
            on_device: Arc::new(move |d| device_rt.chat.set_relay_device(d)),
            on_owners: Arc::new(move |f| owners_rt.chat.set_relay_owners(Some(f))),
            rooms: Arc::new(move || {
                let sessions: Vec<SessionInfo> = rooms_rt.all().iter().map(|s| s.info()).collect();
                let mut rooms = rooms_rt.chat.rooms(&sessions, None);
                rooms_rt.mention_decorate(&mut rooms);
                rooms
            }),
            history: Arc::new(move |room, limit| {
                history_rt
                    .chat
                    .history(room, None, Some(limit))
                    .unwrap_or_default()
            }),
        };
        if let Err(e) = chat::relay::pump::spawn(setup) {
            log(&format!("chat relay: pump did not start: {e}"));
        }
    }

    fn spawn_rpc_server(
        self: &Arc<Self>,
        inbox: Arc<RpcInbox>,
        send: RpcSend,
    ) -> Option<thread::JoinHandle<()>> {
        let dir = self.cfg.dir.clone();
        let audit = Arc::new(FileAudit::new(&dir));
        let machines = Arc::new(MachinesCache::new(&dir));
        let transport = RelayTransport::new(
            inbox.clone(),
            machines.clone(),
            audit.clone(),
            Arc::new(now_ms),
            send.clone(),
        );
        self.chat.set_rpc_client(Some(Arc::new(RpcClient::new(
            inbox,
            machines,
            send,
            Arc::new(now_ms),
        ))));
        let dispatcher = Dispatcher::with_store(
            self.chat.machine(),
            self.mgr.clone(),
            audit,
            Some(dir.join(SEEN_FILE)),
            now_ms(),
        );
        let rt = self.clone();
        thread::Builder::new()
            .name("mgr-rpc-serve".into())
            .spawn(move || {
                while !rt.stop.load(Ordering::SeqCst) {
                    let manager = rt.live_manager();
                    let host = ManagerHost {
                        rt: &rt,
                        me: manager.as_ref().map_or("-", |m| m.id.as_str()),
                        unrestricted: false,
                    };
                    if !serve_tick(&transport, &dispatcher, &dir, &host) {
                        thread::sleep(RPC_TICK);
                    }
                }
                rt.chat.set_rpc_client(None);
            })
            .map_err(|e| log(&format!("manager rpc: serve thread did not start: {e}")))
            .ok()
    }

    fn remote_link(&self) -> Live<'_> {
        Live {
            dir: &self.cfg.dir,
            chat: &self.chat,
        }
    }

    pub(super) fn chat_dispatch(
        &self,
        conn: &Arc<Conn>,
        method: &str,
        p: &Value,
    ) -> Result<Value, ApiErr> {
        if conn.client != CLIENT_CLI && conn.client != CLIENT_DESKTOP {
            return Err(api(
                ErrorCode::Forbidden,
                "chat is only available to the tmgr CLI and the desktop app",
            ));
        }
        match method {
            chat_method::ROOMS => {
                let sessions: Vec<SessionInfo> = self.all().iter().map(|s| s.info()).collect();
                let mut rooms = self.chat.rooms(
                    &sessions,
                    self.live_manager()
                        .filter(|m| super::manager::is_unrestricted(m))
                        .as_ref()
                        .map(|m| m.id.as_str()),
                );
                self.mention_decorate(&mut rooms);
                rooms.extend(chat_remote::rooms(&self.remote_link()));
                Ok(to_json(&rooms))
            }
            chat_method::HISTORY => {
                let q: HistoryParams = params(p)?;
                if q.room.machine != self.chat.machine() {
                    q.room.validate().map_err(bad)?;
                    return chat_remote::history(
                        &self.remote_link(),
                        &q.room,
                        q.before.as_deref(),
                        q.limit,
                    )
                    .map(|m| to_json(&m))
                    .map_err(bad);
                }
                self.chat
                    .history(&q.room, q.before.as_deref(), q.limit)
                    .map(|m| to_json(&m))
                    .map_err(bad)
            }
            chat_method::SEARCH => {
                let q: SearchParams = params(p)?;
                self.chat
                    .search(&q.query, q.room.as_ref(), q.limit)
                    .map(|m| to_json(&m))
                    .map_err(bad)
            }
            chat_method::SEND => {
                let mut q: SendParams = params(p)?;
                if q.body_md.trim().is_empty() && q.attachments.is_empty() {
                    return Err(bad("body_md is empty".into()));
                }
                if let Some(req) = q.mention.take() {
                    return self.send_mention(conn, q, req);
                }
                q.room.validate().map_err(bad)?;
                if q.room.machine != self.chat.machine() {
                    return chat_remote::send(
                        &self.remote_link(),
                        self.chat.machine(),
                        &q.room,
                        &q.body_md,
                        q.attachments.len(),
                        q.reply_to.as_deref(),
                    )
                    .map(|m| to_json(&m))
                    .map_err(bad);
                }
                if let Some(id) = q.id {
                    let ts_ms = q
                        .ts_ms
                        .ok_or_else(|| bad("ts_ms is required with id".into()))?;
                    let room = q.room.clone();
                    let incoming = chat::Incoming {
                        room: q.room,
                        id,
                        ts_ms,
                        body_md: q.body_md,
                        reply_to: q.reply_to,
                        attachments: q.attachments,
                        device: None,
                        owner_device: false,
                        client: conn.client.clone(),
                    };
                    let deliver = |body: &str| self.chat_deliver(&room, body, &conn.client);
                    return self
                        .chat
                        .ingest(incoming, now_ms(), |s| self.session(s).is_some(), deliver)
                        .map(|r| to_json(&r.message))
                        .map_err(|e| match e {
                            chat::IngestError::Invalid(m) => bad(m),
                            chat::IngestError::NotFound(m) => api(ErrorCode::NotFound, m),
                            chat::IngestError::Conflict(m) => {
                                api(ErrorCode::Other("conflict".into()), m)
                            }
                        });
                }
                let posted = self
                    .chat
                    .post(
                        q.room,
                        chat::Chat::owner(),
                        q.body_md,
                        q.reply_to,
                        q.attachments,
                    )
                    .map_err(bad)?;
                let outcome = self.chat.submit(
                    &posted.room,
                    &posted.id,
                    &posted.body_md,
                    &conn.client,
                    None,
                    false,
                    now_ms(),
                    |body, client, _, _| self.chat_deliver(&posted.room, body, client),
                );
                self.chat.note_undelivered(&posted.room, &outcome);
                Ok(to_json(&posted))
            }
            chat_method::QUEUED => {
                let q: QueuedParams = params(p)?;
                q.room.validate().map_err(bad)?;
                Ok(to_json(&self.chat.queued_ids(&q.room)))
            }
            chat_method::CANCEL => {
                let q: CancelParams = params(p)?;
                q.room.validate().map_err(bad)?;
                Ok(to_json(&self.chat.cancel(&q.room, &q.id)))
            }
            chat_method::MENTION_INJECT => self.mention_inject(params(p)?),
            chat_method::RELAY_SELF => {
                self.chat.relay_self().map(|r| to_json(&r)).ok_or_else(|| {
                    api(
                        ErrorCode::NotFound,
                        "the chat relay is not configured or not ready on this runner",
                    )
                })
            }
            chat_method::SUBSCRIBE => {
                if conn.chat_subscribed.swap(true, Ordering::SeqCst) {
                    return Ok(Value::Null);
                }
                let activity = p.get("activity").and_then(Value::as_bool) == Some(true);
                let mut slot = lock(&conn.chat_rx);
                let sub = slot.get_or_insert_with(|| chat::ChatSub {
                    rx: self.chat.subscribe(),
                    activity: None,
                });
                if activity && sub.activity.is_none() {
                    sub.activity = Some(self.chat.subscribe_activity());
                }
                Ok(Value::Null)
            }
            m => Err(api(ErrorCode::UnknownMethod, format!("unknown method {m}"))),
        }
    }

    pub(super) fn chat_pusher(&self, conn: &Arc<Conn>, sub: chat::ChatSub) {
        let chat::ChatSub { rx, activity } = sub;
        let fail = |conn: &Conn| {
            let _ = lock(&conn.w).shutdown(std::net::Shutdown::Both);
        };
        let tick = if activity.is_some() {
            ACTIVITY_TICK
        } else {
            Duration::from_secs(1)
        };
        while !conn.closed.load(Ordering::SeqCst) {
            if let Some(a) = &activity {
                for event in a.take(Duration::ZERO) {
                    if conn
                        .write(&ServerMsg::ChatActivity(to_json(&event)))
                        .is_err()
                    {
                        return fail(conn);
                    }
                }
            }
            match rx.recv_timeout(tick) {
                Ok(push) => {
                    if conn.write(&ServerMsg::Chat(to_json(&push))).is_err() {
                        return fail(conn);
                    }
                }
                Err(ChatRecv::Timeout) => {}
                Err(ChatRecv::Disconnected) => {
                    let _ = conn.write(&ServerMsg::Closed {
                        reason: "client too slow".into(),
                    });
                    return fail(conn);
                }
            }
        }
    }

    pub(super) fn chat_queue_loop(&self) {
        while !self.stop.load(Ordering::SeqCst) {
            thread::sleep(QUEUE_EVERY);
            self.mention_tick();
            self.chat.drain_queue_with(
                now_ms(),
                |it| {
                    if self.chat.is_mention_open(it.id) {
                        return self.mention_deliver(it);
                    }
                    let delivery = if it.client == "relay" {
                        self.relay_deliver(it.room, it.device, it.owner_device, it.body)
                    } else if it.client == REMOTE_CLIENT {
                        self.remote_deliver(it.room, it.body)
                    } else {
                        self.chat_deliver(it.room, it.body, it.client)
                    };
                    match delivery {
                        chat::Delivery::Gone(_) if it.room.session.is_none() => {
                            chat::Delivery::Busy
                        }
                        d => d,
                    }
                },
                |it, typed| self.chat.mention_delivered(it.id, typed, now_ms()),
            );
        }
    }

    pub(super) fn chat_capture_loop(&self) {
        let mut last = Instant::now();
        while !self.stop.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(250));
            if last.elapsed() < CAPTURE_EVERY {
                continue;
            }
            last = Instant::now();
            let sessions: Vec<SessionInfo> = self.all().iter().map(|s| s.info()).collect();
            let titles = tail::capture(&self.chat, &sessions);
            self.apply_auto_names(&titles);
        }
    }

    fn apply_auto_names(&self, titles: &[(String, String)]) {
        let _guard = lock(&self.rename_lock);
        let mut live = self.all();
        live.sort_by_key(|s| {
            let i = lock(&s.inner);
            (i.meta.created_ms, i.meta.id.clone())
        });
        let mut names: Vec<(String, String)> = live
            .iter()
            .map(|s| {
                let i = lock(&s.inner);
                (i.meta.id.clone(), i.meta.name.clone())
            })
            .collect();
        for s in live.iter().filter(|s| !s.is_exited()) {
            let (id, current, base) = {
                let i = lock(&s.inner);
                let m = &i.meta;
                let auto = m.name_auto || m.name == default_name(m.argv.first().map_or("", |a| a));
                if !auto
                    || m.manager
                    || chat::tail::agent_named(
                        m.profile.as_deref(),
                        m.argv.first().map_or("", String::as_str),
                    )
                    .is_none()
                {
                    continue;
                }
                let from_title = titles
                    .iter()
                    .find(|(t, _)| *t == m.id)
                    .and_then(|(_, t)| crate::autoname::title_name(t));
                if from_title.is_none() && m.name_auto {
                    continue;
                }
                let Some(base) = from_title.or_else(|| crate::autoname::folder_name(&m.cwd)) else {
                    continue;
                };
                (m.id.clone(), m.name.clone(), base)
            };
            let taken = names
                .iter()
                .filter(|(o, _)| *o != id)
                .map(|(_, n)| n.clone())
                .collect();
            let Some(new) = crate::autoname::pick(&base, &current, &taken) else {
                continue;
            };
            {
                let mut i = lock(&s.inner);
                i.meta.name = new.clone();
                i.meta.name_auto = true;
            }
            s.persist();
            if let Some(slot) = names.iter_mut().find(|(o, _)| *o == id) {
                slot.1 = new;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::os::unix::net::UnixStream;
    use tmgr_ipc::chat::{ChatMessage, RoomRef};

    fn runner(dir: &Path) -> Arc<Runner> {
        let cfg = RunnerConfig {
            dir: dir.join("run"),
            homes: dir.join("homes"),
            self_exe: PathBuf::from("tmgr"),
            quiet_ms: 1000,
            version: "test".into(),
            max_sessions: 4,
            relay: None,
            pin_check: None,
        };
        let chat_store = Arc::new(chat::Chat::open(&cfg.dir));
        Arc::new(Runner {
            rename_lock: Mutex::new(()),
            switch_lock: Mutex::new(HashSet::new()),
            chat: chat_store.clone(),
            mgr: Arc::new(crate::manager::State::new(Box::new(chat_store))),
            spawn_gate: spawnauth::SpawnGate::default(),
            cfg,
            secret: String::new(),
            started: Instant::now(),
            sessions: Mutex::new(HashMap::new()),
            hub: Arc::new(Hub::default()),
            create_lock: Mutex::new(()),
            key_lock: Mutex::new(()),
            external_lock: Mutex::new(()),
            layout_lock: Mutex::new(()),
            stop: AtomicBool::new(false),
            conns: Arc::new(AtomicUsize::new(0)),
            pending: Arc::new(AtomicUsize::new(0)),
            rejects: Mutex::new(Rejects::default()),
            client_config_dirs: Mutex::new(Vec::new()),
            config_dir_warned: AtomicBool::new(false),
            next_conn: AtomicU64::new(1),
            autopilot: autopilot::Host::default(),
            clock: Mutex::new(humantime::Clock::new(0)),
        })
    }

    fn conn(client: &str) -> (Arc<Conn>, UnixStream) {
        let (a, b) = UnixStream::pair().unwrap();
        let c = Arc::new(Conn {
            id: 1,
            w: Mutex::new(a),
            watched: Mutex::new(Vec::new()),
            scroll: Mutex::new(HashMap::new()),
            sizes: Mutex::new(HashMap::new()),
            closed: AtomicBool::new(false),
            attached: AtomicBool::new(false),
            client: client.into(),
            pending_rx: Mutex::new(None),
            chat_rx: Mutex::new(None),
            chat_subscribed: AtomicBool::new(false),
        });
        (c, b)
    }

    fn call(rt: &Arc<Runner>, c: &Arc<Conn>, method: &str, params: Value) -> Result<Value, ApiErr> {
        rt.dispatch(
            c,
            &Request {
                id: 1,
                method: method.into(),
                params,
            },
        )
    }

    fn send_params(rt: &Runner, body: &str) -> Value {
        to_json(&SendParams {
            room: rt.chat.manager_room(),
            body_md: body.into(),
            reply_to: None,
            attachments: Vec::new(),
            id: None,
            ts_ms: None,
            mention: None,
        })
    }

    const PEER: &str = "mx_00000000000000bb";

    fn make_top_with_peer(rt: &Runner) {
        let dir = &rt.cfg.dir;
        crate::mgr_rpc::top::apply_top(dir, Some(true)).unwrap();
        let machines = format!(
            r#"{{"machines":[{{"id":"{PEER}","label":"laptop","device_id":"66666666-7777-8888-9999-aaaaaaaaaaaa","fp":"{}"}}]}}"#,
            "b".repeat(64)
        );
        let p = dir.join(crate::mgr_rpc::machines::MACHINES_FILE);
        std::fs::write(&p, machines).unwrap();
        std::fs::set_permissions(&p, std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
    }

    fn history_params(room: RoomRef) -> Value {
        to_json(&HistoryParams {
            room,
            before: None,
            limit: None,
        })
    }

    fn send_to(room: RoomRef) -> Value {
        to_json(&SendParams {
            room,
            body_md: "hi".into(),
            reply_to: None,
            attachments: Vec::new(),
            id: None,
            ts_ms: None,
            mention: None,
        })
    }

    #[test]
    fn a_non_top_keeps_refusing_rooms_of_other_machines() {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        let (c, _peer) = conn(CLIENT_CLI);
        let e = call(&rt, &c, chat_method::SEND, send_to(RoomRef::manager(PEER))).unwrap_err();
        assert_eq!(e.msg, "the room belongs to another machine");
        let e = call(
            &rt,
            &c,
            chat_method::HISTORY,
            history_params(RoomRef::manager(PEER)),
        )
        .unwrap_err();
        assert_eq!(e.msg, "the room belongs to another machine");
        let rooms: Vec<tmgr_ipc::chat::RoomInfo> =
            serde_json::from_value(call(&rt, &c, chat_method::ROOMS, Value::Null).unwrap())
                .unwrap();
        assert!(rooms.iter().all(|r| r.room.machine == rt.chat.machine()));
    }

    #[test]
    fn a_top_lists_a_manager_room_per_paired_machine() {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        make_top_with_peer(&rt);
        let (c, _peer) = conn(CLIENT_CLI);
        let rooms: Vec<tmgr_ipc::chat::RoomInfo> =
            serde_json::from_value(call(&rt, &c, chat_method::ROOMS, Value::Null).unwrap())
                .unwrap();
        assert_eq!(rooms[0].room, rt.chat.manager_room());
        let last = rooms.last().unwrap();
        assert_eq!(last.room, RoomRef::manager(PEER));
        assert_eq!(last.title, "laptop");
    }

    #[test]
    fn a_top_reaches_the_relay_for_a_paired_machine_and_refuses_the_rest() {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        make_top_with_peer(&rt);
        let (c, _peer) = conn(CLIENT_CLI);
        let stranger = RoomRef::manager("mx_00000000000000cc");
        let session = RoomRef::session(PEER, "s1");
        let cases = [
            (RoomRef::manager(PEER), "relay not ready"),
            (stranger, "unknown machine mx_00000000000000cc"),
            (session, "session rooms of other machines are not available"),
        ];
        for (room, want) in cases {
            let e = call(&rt, &c, chat_method::SEND, send_to(room.clone())).unwrap_err();
            assert!(e.msg.starts_with(want), "{}", e.msg);
            let e = call(&rt, &c, chat_method::HISTORY, history_params(room)).unwrap_err();
            assert!(e.msg.starts_with(want), "{}", e.msg);
        }
    }

    fn manager_rig(flags: Value) -> (tempfile::TempDir, Arc<Runner>) {
        use crate::daemon::manager::tests::{add_meta, runner};
        let t = tempfile::Builder::new()
            .prefix("mc")
            .tempdir_in("/tmp")
            .unwrap();
        let rt = runner(t.path());
        let mut meta = json!({"manager": true});
        meta.as_object_mut()
            .unwrap()
            .extend(flags.as_object().unwrap().clone());
        add_meta(&rt, t.path(), "s_aaaaaaaaaa", meta);
        add_meta(
            &rt,
            t.path(),
            "s_cccccccccc",
            json!({"permission_mode": "bypass", "no_sandbox": true}),
        );
        (t, rt)
    }

    fn relay_msg(room: RoomRef, body: &str) -> chat::Incoming {
        chat::Incoming {
            room,
            id: tmgr_ipc::chat::message_id_at(now_ms(), [3; 10]),
            ts_ms: now_ms(),
            body_md: body.into(),
            reply_to: None,
            attachments: Vec::new(),
            device: Some("d_phone1".into()),
            owner_device: false,
            client: "relay".into(),
        }
    }

    #[test]
    fn an_unrestricted_manager_does_not_refuse_owner_device_relay_input() {
        let (_t, rt) = manager_rig(
            json!({"unrestricted": true, "no_sandbox": true, "permission_mode": "bypass"}),
        );
        let room = rt.chat.manager_room();
        for room in [room.clone(), rt.chat.session_room("s_aaaaaaaaaa")] {
            assert_eq!(
                rt.relay_deliver(&room, Some("d_phone1"), true, "hi"),
                chat::Delivery::Busy
            );
        }
        let mut m = relay_msg(room.clone(), "hello");
        m.owner_device = true;
        let r = rt
            .chat
            .ingest(
                m,
                now_ms(),
                |s| rt.session(s).is_some(),
                |body| rt.relay_deliver(&room, Some("d_phone1"), true, body),
            )
            .unwrap();
        assert!(!r.delivered);
        assert_eq!(rt.chat.queued_ids(&room), [r.message.id]);
        let h = rt.chat.history(&room, None, None).unwrap();
        assert!(h
            .iter()
            .all(|m| m.author.kind != tmgr_ipc::chat::AuthorKind::System));
    }

    #[test]
    fn an_unrestricted_manager_still_refuses_non_owner_relay_input_and_posts_a_note() {
        let (_t, rt) = manager_rig(
            json!({"unrestricted": true, "no_sandbox": true, "permission_mode": "bypass"}),
        );
        let room = rt.chat.manager_room();
        for room in [room.clone(), rt.chat.session_room("s_aaaaaaaaaa")] {
            assert_eq!(
                rt.relay_deliver(&room, Some("d_phone1"), false, "hi"),
                chat::Delivery::Refused(UNRESTRICTED_REFUSAL)
            );
        }
        let r = rt
            .chat
            .ingest(
                relay_msg(room.clone(), "hi"),
                now_ms(),
                |s| rt.session(s).is_some(),
                |body| rt.relay_deliver(&room, Some("d_phone1"), false, body),
            )
            .unwrap();
        assert!(!r.delivered);
        let h = rt.chat.history(&room, None, None).unwrap();
        assert_eq!(h[0].body_md, "hi");
        assert_eq!(h[1].body_md, UNRESTRICTED_REFUSAL);
    }

    #[test]
    fn remote_delivery_rechecks_unrestricted_and_queues_while_the_manager_is_not_ready() {
        let (_t, rt) = manager_rig(
            json!({"unrestricted": true, "no_sandbox": true, "permission_mode": "bypass"}),
        );
        let room = rt.chat.manager_room();
        assert_eq!(
            rt.remote_deliver(&room, "hi"),
            chat::Delivery::Refused(REMOTE_UNRESTRICTED_REFUSAL)
        );
        let (_t, rt) = manager_rig(json!({}));
        assert!(!matches!(
            rt.remote_deliver(&rt.chat.manager_room(), "hi"),
            chat::Delivery::Refused(_)
        ));
    }

    #[test]
    fn a_restricted_manager_and_bypass_workers_are_not_refused_relay_input() {
        let (_t, rt) = manager_rig(json!({}));
        for room in [
            rt.chat.manager_room(),
            rt.chat.session_room("s_aaaaaaaaaa"),
            rt.chat.session_room("s_cccccccccc"),
        ] {
            assert!(!matches!(
                rt.relay_deliver(&room, None, false, "hi"),
                chat::Delivery::Refused(_)
            ));
        }
        let (_t, rt) = manager_rig(
            json!({"unrestricted": true, "no_sandbox": true, "permission_mode": "bypass"}),
        );
        assert!(!matches!(
            rt.relay_deliver(&rt.chat.session_room("s_cccccccccc"), None, false, "hi"),
            chat::Delivery::Refused(_)
        ));
    }

    #[test]
    fn workers_and_unknown_clients_are_forbidden_from_every_chat_method() {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        for client in [CLIENT_WORKER, "other", ""] {
            let (c, _peer) = conn(client);
            for m in [
                chat_method::ROOMS,
                chat_method::HISTORY,
                chat_method::SEND,
                chat_method::SEARCH,
                chat_method::SUBSCRIBE,
            ] {
                let e = call(&rt, &c, m, send_params(&rt, "x")).unwrap_err();
                assert_eq!(e.code, ErrorCode::Forbidden, "{client} {m}");
            }
        }
        assert!(rt
            .chat
            .history(&rt.chat.manager_room(), None, None)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn cli_and_desktop_send_store_and_read_back_and_search() {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        for client in [CLIENT_CLI, CLIENT_DESKTOP] {
            let (c, _peer) = conn(client);
            let sent: ChatMessage = serde_json::from_value(
                call(
                    &rt,
                    &c,
                    chat_method::SEND,
                    send_params(&rt, &format!("hello from {client}")),
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(sent.author.id, "owner");
            sent.validate().unwrap();
        }
        let (c, _peer) = conn(CLIENT_CLI);
        let room = rt.chat.manager_room();
        let hist: Vec<ChatMessage> = serde_json::from_value(
            call(
                &rt,
                &c,
                chat_method::HISTORY,
                to_json(&HistoryParams {
                    room: room.clone(),
                    before: None,
                    limit: Some(10),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(hist.len(), 4);
        let found: Vec<ChatMessage> = serde_json::from_value(
            call(
                &rt,
                &c,
                chat_method::SEARCH,
                json!({"query": "FROM DESKTOP"}),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(found.len(), 1);
        let rooms: Vec<tmgr_ipc::chat::RoomInfo> =
            serde_json::from_value(call(&rt, &c, chat_method::ROOMS, Value::Null).unwrap())
                .unwrap();
        assert_eq!(rooms[0].title, "Manager");
        assert_eq!(rooms[0].last.as_ref().unwrap().id, hist[3].id);
    }

    #[test]
    fn bad_params_rooms_of_other_machines_and_empty_bodies_are_invalid() {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        let (c, _peer) = conn(CLIENT_CLI);
        let foreign = to_json(&SendParams {
            room: RoomRef::manager("mx_ffffffffffffffff"),
            body_md: "x".into(),
            reply_to: None,
            attachments: Vec::new(),
            id: None,
            ts_ms: None,
            mention: None,
        });
        for p in [
            foreign,
            send_params(&rt, "   "),
            json!({"room": {"kind": "group", "machine": "m"}, "body_md": "x"}),
            Value::Null,
        ] {
            let e = call(&rt, &c, chat_method::SEND, p).unwrap_err();
            assert_eq!(e.code, ErrorCode::InvalidParams);
        }
        let e = call(&rt, &c, chat_method::SEARCH, json!({"query": ""})).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidParams);
        assert_eq!(
            call(&rt, &c, "chat.nope", Value::Null).unwrap_err().code,
            ErrorCode::UnknownMethod
        );
    }

    #[test]
    fn a_subscriber_receives_pushes_in_the_wire_shape() {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        let (c, mut peer) = conn(CLIENT_DESKTOP);
        call(&rt, &c, chat_method::SUBSCRIBE, Value::Null).unwrap();
        let rx = lock(&c.chat_rx).take().unwrap();
        let (rt2, c2) = (rt.clone(), c.clone());
        let h = thread::spawn(move || rt2.chat_pusher(&c2, rx));
        let (sender, _p2) = conn(CLIENT_CLI);
        call(&rt, &sender, chat_method::SEND, send_params(&rt, "ping")).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let Some(Frame::Json(v)) = read_frame(&mut peer).unwrap() else {
            panic!("json frame expected");
        };
        assert_eq!(v["type"], "chat");
        assert_eq!(v["message"]["body_md"], "ping");
        c.closed.store(true, Ordering::SeqCst);
        h.join().unwrap();
    }

    fn activity_event(rt: &Runner, current: &str) -> tmgr_ipc::chat::ChatActivity {
        tmgr_ipc::chat::ChatActivity {
            v: 1,
            room: rt.chat.session_room("s_abcdefghij"),
            state: tmgr_ipc::chat::ActivityState::Working,
            current: Some(current.into()),
            used: vec!["Read: a.rs".into()],
            calls: Vec::new(),
        }
    }

    #[test]
    fn a_subscriber_that_opts_in_receives_activity_in_the_wire_shape() {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        let (c, mut peer) = conn(CLIENT_DESKTOP);
        rt.chat.set_activity_consent(true);
        call(&rt, &c, chat_method::SUBSCRIBE, json!({"activity": true})).unwrap();
        let sub = lock(&c.chat_rx).take().unwrap();
        let (rt2, c2) = (rt.clone(), c.clone());
        let h = thread::spawn(move || rt2.chat_pusher(&c2, sub));
        rt.chat.publish_activity(&activity_event(&rt, "Bash: ls"));
        peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let Some(Frame::Json(v)) = read_frame(&mut peer).unwrap() else {
            panic!("json frame expected");
        };
        assert_eq!(v["type"], "chat_activity");
        assert_eq!(v["v"], 1);
        assert_eq!(v["state"], "working");
        assert_eq!(v["current"], "Bash: ls");
        assert_eq!(v["used"], json!(["Read: a.rs"]));
        assert_eq!(v["room"]["session"], "s_abcdefghij");
        c.closed.store(true, Ordering::SeqCst);
        h.join().unwrap();
    }

    #[test]
    fn nothing_is_sent_to_a_slow_subscriber_after_consent_is_revoked() {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        let (c, mut peer) = conn(CLIENT_DESKTOP);
        rt.chat.set_activity_consent(true);
        call(&rt, &c, chat_method::SUBSCRIBE, json!({"activity": true})).unwrap();
        let sub = lock(&c.chat_rx).take().unwrap();
        rt.chat.publish_activity(&activity_event(&rt, "Bash: ls"));
        rt.chat.set_activity_consent(false);
        let (rt2, c2) = (rt.clone(), c.clone());
        let h = thread::spawn(move || rt2.chat_pusher(&c2, sub));
        peer.set_read_timeout(Some(Duration::from_millis(800)))
            .unwrap();
        assert!(read_frame(&mut peer).is_err());
        c.closed.store(true, Ordering::SeqCst);
        h.join().unwrap();
    }

    #[test]
    fn a_second_subscribe_on_one_connection_is_a_no_op() {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        let (c, mut peer) = conn(CLIENT_DESKTOP);
        rt.chat.set_activity_consent(true);
        call(&rt, &c, chat_method::SUBSCRIBE, Value::Null).unwrap();
        let sub = lock(&c.chat_rx).take().unwrap();
        let (rt2, c2) = (rt.clone(), c.clone());
        let h = thread::spawn(move || rt2.chat_pusher(&c2, sub));
        assert_eq!(
            call(&rt, &c, chat_method::SUBSCRIBE, json!({"activity": true})).unwrap(),
            Value::Null
        );
        assert!(lock(&c.chat_rx).is_none());
        assert_eq!(rt.chat.activity_subscribers(), 0);
        let (sender, _p2) = conn(CLIENT_CLI);
        call(&rt, &sender, chat_method::SEND, send_params(&rt, "once")).unwrap();
        peer.set_read_timeout(Some(Duration::from_millis(800)))
            .unwrap();
        let mut bodies = Vec::new();
        while let Ok(Some(Frame::Json(v))) = read_frame(&mut peer) {
            bodies.push(v["message"]["body_md"].as_str().unwrap_or("").to_string());
        }
        assert_eq!(
            bodies.iter().filter(|b| *b == "once").count(),
            1,
            "{bodies:?}"
        );
        c.closed.store(true, Ordering::SeqCst);
        h.join().unwrap();
    }

    #[test]
    fn a_plain_subscriber_gets_no_activity() {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        let (c, _peer) = conn(CLIENT_DESKTOP);
        call(&rt, &c, chat_method::SUBSCRIBE, Value::Null).unwrap();
        let sub = lock(&c.chat_rx).take().unwrap();
        assert!(sub.activity.is_none());
        assert_eq!(rt.chat.activity_subscribers(), 0);
    }

    #[test]
    fn the_manager_hook_is_empty_until_wired_so_manager_messages_are_only_stored() {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        assert!(rt.live_manager().is_none());
        assert_eq!(
            rt.chat_target(&rt.chat.manager_room()).err(),
            Some(chat::Delivery::Gone("no manager is running".into()))
        );
        assert_eq!(
            rt.chat_target(&rt.chat.session_room("s_abcdefghij")).err(),
            Some(chat::Delivery::Gone("the session no longer exists".into()))
        );
    }

    fn ingest_params(rt: &Runner, id: &str, ts: Option<u64>, body: &str) -> Value {
        let mut p = send_params(rt, body);
        p["id"] = json!(id);
        if let Some(ts) = ts {
            p["ts_ms"] = json!(ts);
        }
        p["device"] = json!("d_forged");
        p
    }

    #[test]
    fn an_ingest_over_ipc_is_idempotent_conflicts_and_needs_a_timestamp() {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        let (c, _peer) = conn(CLIENT_CLI);
        let id = tmgr_ipc::chat::message_id_at(1, [3; 10]);
        let ts = now_ms();
        let first: ChatMessage = serde_json::from_value(
            call(
                &rt,
                &c,
                chat_method::SEND,
                ingest_params(&rt, &id, Some(ts), "go"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(first.id, id);
        assert_eq!(first.author.device, None);
        let again: ChatMessage = serde_json::from_value(
            call(
                &rt,
                &c,
                chat_method::SEND,
                ingest_params(&rt, &id, Some(ts), "go"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(again, first);
        let e = call(
            &rt,
            &c,
            chat_method::SEND,
            ingest_params(&rt, &id, Some(ts), "different"),
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::Other("conflict".into()));
        let e = call(
            &rt,
            &c,
            chat_method::SEND,
            ingest_params(&rt, &id, None, "go"),
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidParams);
        let e = call(
            &rt,
            &c,
            chat_method::SEND,
            ingest_params(&rt, "m_short", Some(ts), "go"),
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidParams);
        let ghost = to_json(&SendParams {
            room: rt.chat.session_room("s_abcdefghij"),
            body_md: "x".into(),
            reply_to: None,
            attachments: Vec::new(),
            id: Some(tmgr_ipc::chat::message_id_at(2, [3; 10])),
            ts_ms: Some(ts),
            mention: None,
        });
        assert_eq!(
            call(&rt, &c, chat_method::SEND, ghost).unwrap_err().code,
            ErrorCode::NotFound
        );
        assert_eq!(
            rt.chat
                .history(&rt.chat.manager_room(), None, None)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn a_worker_cannot_ingest() {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        let (c, _peer) = conn(CLIENT_WORKER);
        let id = tmgr_ipc::chat::message_id_at(1, [3; 10]);
        let e = call(
            &rt,
            &c,
            chat_method::SEND,
            ingest_params(&rt, &id, Some(now_ms()), "go"),
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::Forbidden);
    }

    fn send_body_to(rt: &Arc<Runner>, room: &RoomRef, body: &str) -> Value {
        let (c, _peer) = conn(CLIENT_DESKTOP);
        call(
            rt,
            &c,
            chat_method::SEND,
            to_json(&SendParams {
                room: room.clone(),
                body_md: body.into(),
                reply_to: None,
                attachments: Vec::new(),
                id: None,
                ts_ms: None,
                mention: None,
            }),
        )
        .unwrap()
    }

    fn system_notes(rt: &Arc<Runner>, room: &RoomRef) -> Vec<String> {
        rt.chat
            .history(room, None, None)
            .unwrap()
            .into_iter()
            .filter(|m| m.author.kind == tmgr_ipc::chat::AuthorKind::System)
            .map(|m| m.body_md)
            .collect()
    }

    #[test]
    fn a_desktop_send_with_no_manager_posts_and_notes_the_real_reason() {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        let room = rt.chat.manager_room();
        send_body_to(&rt, &room, "hello");
        assert_eq!(
            system_notes(&rt, &room),
            ["Not delivered: no manager is running."]
        );
        assert!(rt.chat.queued_ids(&room).is_empty());
    }

    #[test]
    fn a_desktop_send_to_a_session_without_a_shim_queues_and_can_be_listed_and_cancelled() {
        let t = tempfile::Builder::new()
            .prefix("cq")
            .tempdir_in("/tmp")
            .unwrap();
        let rt = runner(t.path());
        add_named(&rt, t.path(), "s_abcdefghij", "claude", "/work/app", 1);
        let room = rt.chat.session_room("s_abcdefghij");
        let posted: ChatMessage =
            serde_json::from_value(send_body_to(&rt, &room, "later")).unwrap();
        assert!(system_notes(&rt, &room).is_empty());
        let (c, _peer) = conn(CLIENT_DESKTOP);
        let queued: Vec<String> = serde_json::from_value(
            call(
                &rt,
                &c,
                chat_method::QUEUED,
                to_json(&QueuedParams { room: room.clone() }),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(queued, std::slice::from_ref(&posted.id));
        let cancel = |id: &str| -> bool {
            serde_json::from_value(
                call(
                    &rt,
                    &c,
                    chat_method::CANCEL,
                    to_json(&CancelParams {
                        room: room.clone(),
                        id: id.into(),
                    }),
                )
                .unwrap(),
            )
            .unwrap()
        };
        assert!(cancel(&posted.id));
        assert!(!cancel(&posted.id));
        assert_eq!(system_notes(&rt, &room), ["Not delivered: cancelled."]);
    }

    #[test]
    fn a_send_to_an_exited_or_unknown_session_notes_the_reason() {
        let t = tempfile::Builder::new()
            .prefix("cq")
            .tempdir_in("/tmp")
            .unwrap();
        let rt = runner(t.path());
        add_named(&rt, t.path(), "s_abcdefghij", "claude", "/work/app", 1);
        rt.session("s_abcdefghij").unwrap().on_exited(&ExitInfo {
            code: Some(0),
            signal: None,
        });
        let exited = rt.chat.session_room("s_abcdefghij");
        send_body_to(&rt, &exited, "a");
        assert_eq!(
            system_notes(&rt, &exited),
            ["Not delivered: the session has exited."]
        );
        let gone = rt.chat.session_room("s_zzzzzzzzzz");
        rt.chat
            .post(
                gone.clone(),
                chat::Chat::owner(),
                "seed".into(),
                None,
                Vec::new(),
            )
            .unwrap();
        send_body_to(&rt, &gone, "b");
        assert_eq!(
            system_notes(&rt, &gone),
            ["Not delivered: the session no longer exists."]
        );
    }

    fn add_named(rt: &Arc<Runner>, t: &Path, id: &str, name: &str, cwd: &str, created: u64) {
        let meta: Meta = serde_json::from_value(json!({
            "id": id, "name": name, "argv": ["claude"], "cwd": cwd,
            "profile": "claude-code", "created_ms": created, "cols": 100, "rows": 24,
            "shim_pid": 1, "child_pid": 2,
        }))
        .unwrap();
        let sdir = t.join("sessions").join(id);
        fs::create_dir_all(&sdir).unwrap();
        let s = rt.new_session(meta, None, &sdir);
        lock(&rt.sessions).insert(id.to_string(), s);
    }

    fn name_of(rt: &Arc<Runner>, id: &str) -> String {
        rt.session(id).unwrap().info().name
    }

    #[test]
    fn sessions_named_after_the_agent_default_get_a_folder_or_title_name() {
        let t = tempfile::Builder::new()
            .prefix("an")
            .tempdir_in("/tmp")
            .unwrap();
        let rt = runner(t.path());
        add_named(&rt, t.path(), "a", "claude", "/work/app", 1);
        add_named(&rt, t.path(), "b", "claude", "/work/app", 2);
        add_named(&rt, t.path(), "c", "mine", "/work/app", 3);
        add_named(&rt, t.path(), "d", "claude", "/work/app", 4);
        rt.apply_auto_names(&[]);
        let names: Vec<String> = ["a", "b", "c", "d"]
            .iter()
            .map(|i| name_of(&rt, i))
            .collect();
        assert_eq!(names, ["app", "app 2", "mine", "app 3"]);
        rt.apply_auto_names(&[("b".into(), "Fix the build".into())]);
        assert_eq!(name_of(&rt, "b"), "Fix the build");
        assert_eq!(name_of(&rt, "a"), "app");
        assert_eq!(name_of(&rt, "d"), "app 3");
        rt.rename("a", "chosen").unwrap();
        rt.apply_auto_names(&[("a".into(), "Other".into())]);
        assert_eq!(name_of(&rt, "a"), "chosen");
        add_named(&rt, t.path(), "e", "claude", "/work/app", 5);
        rt.apply_auto_names(&[("e".into(), "Fix the build".into())]);
        assert_eq!(name_of(&rt, "e"), "Fix the build 2");
    }
}
