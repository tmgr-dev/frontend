use super::*;
use crate::chat::{self, tail};
use std::sync::mpsc::RecvTimeoutError as ChatRecv;
use tmgr_ipc::chat::{method as chat_method, ChatPush, HistoryParams, SearchParams, SendParams};

const CAPTURE_EVERY: Duration = Duration::from_secs(3);

fn bad(msg: String) -> ApiErr {
    api(ErrorCode::InvalidParams, msg)
}

impl Runner {
    fn chat_target(&self, room: &tmgr_ipc::chat::RoomRef) -> Option<Arc<Session>> {
        let s = match room.session.as_deref() {
            Some(id) => self.session(id)?,
            None => self.live_manager()?,
        };
        (!s.is_exited() && s.has_shim()).then_some(s)
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
                Ok(to_json(&self.chat.rooms(&sessions)))
            }
            chat_method::HISTORY => {
                let q: HistoryParams = params(p)?;
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
                let q: SendParams = params(p)?;
                if q.body_md.trim().is_empty() && q.attachments.is_empty() {
                    return Err(bad("body_md is empty".into()));
                }
                q.room.validate().map_err(bad)?;
                if q.room.machine != self.chat.machine() {
                    return Err(bad("the room belongs to another machine".into()));
                }
                if let Some(target) = self.chat_target(&q.room) {
                    target.input_at_prompt(&chat::prompt_bytes(&q.body_md), Instant::now())?;
                    target.note_input_from(&conn.client);
                }
                self.chat
                    .post(
                        q.room,
                        chat::Chat::owner(),
                        q.body_md,
                        q.reply_to,
                        q.attachments,
                    )
                    .map(|m| to_json(&m))
                    .map_err(bad)
            }
            chat_method::SUBSCRIBE => {
                let mut slot = lock(&conn.chat_rx);
                if slot.is_none() {
                    *slot = Some(self.chat.subscribe());
                }
                Ok(Value::Null)
            }
            m => Err(api(ErrorCode::UnknownMethod, format!("unknown method {m}"))),
        }
    }

    pub(super) fn chat_pusher(&self, conn: &Arc<Conn>, rx: Receiver<ChatPush>) {
        let fail = |conn: &Conn| {
            let _ = lock(&conn.w).shutdown(std::net::Shutdown::Both);
        };
        while !conn.closed.load(Ordering::SeqCst) {
            match rx.recv_timeout(Duration::from_secs(1)) {
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

    pub(super) fn chat_capture_loop(&self) {
        let mut last = Instant::now();
        while !self.stop.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(250));
            if last.elapsed() < CAPTURE_EVERY {
                continue;
            }
            last = Instant::now();
            let sessions: Vec<SessionInfo> = self.all().iter().map(|s| s.info()).collect();
            tail::capture(&self.chat, &sessions);
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
        };
        let chat_store = Arc::new(chat::Chat::open(&cfg.dir));
        Arc::new(Runner {
            rename_lock: Mutex::new(()),
            chat: chat_store.clone(),
            mgr: crate::manager::State::new(Box::new(chat_store)),
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
        })
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
        assert_eq!(hist.len(), 2);
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
        assert_eq!(rooms[0].last.as_ref().unwrap().id, hist[1].id);
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

    #[test]
    fn the_manager_hook_is_empty_until_wired_so_manager_messages_are_only_stored() {
        let t = tempfile::tempdir().unwrap();
        let rt = runner(t.path());
        assert!(rt.live_manager().is_none());
        assert!(rt.chat_target(&rt.chat.manager_room()).is_none());
        assert!(rt
            .chat_target(&rt.chat.session_room("s_abcdefghij"))
            .is_none());
    }
}
