use super::*;
use crate::spawnauth::{KeyError, KeyInfo};
use tmgr_ipc::api::{
    SpawnKeyInfo, SpawnKeysApproveParams, SpawnKeysApproveResult, SpawnKeysList,
    SpawnKeysRevokeParams, SpawnKeysRevokeResult,
};
use tmgr_spawn_auth::KeyState;

fn state_name(s: KeyState) -> &'static str {
    match s {
        KeyState::Pending => "pending",
        KeyState::Approved => "approved",
    }
}

fn info(k: KeyInfo) -> SpawnKeyInfo {
    SpawnKeyInfo {
        device_fp: k.device_fp,
        device_id: k.device_id,
        spawn_key_fp: k.spawn_key_fp,
        state: state_name(k.state).into(),
        issued_at_ms: k.issued_at_ms,
        replaced_at_ms: k.replaced_at_ms,
    }
}

fn key_err(e: KeyError, noun: &str) -> ApiErr {
    let msg = e.to_string().replace("spawn key", noun);
    match e {
        KeyError::Approve(_) | KeyError::Revoke(_) | KeyError::Refused(_) => {
            api(ErrorCode::InvalidParams, msg)
        }
        KeyError::Io | KeyError::Corrupt => api(ErrorCode::Internal, msg),
    }
}

impl Runner {
    pub(super) fn spawn_keys_call(
        &self,
        conn: &Conn,
        method: &str,
        p: &Value,
    ) -> Result<Value, ApiErr> {
        use tmgr_ipc::api::method as m;
        let presence = matches!(
            method,
            m::PRESENCE_KEYS_LIST | m::PRESENCE_KEYS_APPROVE | m::PRESENCE_KEYS_REVOKE
        );
        let noun = if presence {
            "presence key"
        } else {
            "spawn key"
        };
        if conn.client != CLIENT_CLI {
            return Err(api(
                ErrorCode::Forbidden,
                format!("{noun}s are managed only from the tmgr CLI"),
            ));
        }
        let auth = self
            .spawn_gate
            .auth(self)
            .map_err(|m| api(ErrorCode::Forbidden, m))?;
        let store = if presence {
            auth.presence_keys()
        } else {
            auth.keys()
        };
        let err = |e| key_err(e, noun);
        match method {
            m::SPAWN_KEYS_LIST | m::PRESENCE_KEYS_LIST => {
                let keys = store.list().map_err(err)?;
                Ok(to_json(&SpawnKeysList {
                    keys: keys.into_iter().map(info).collect(),
                }))
            }
            m::SPAWN_KEYS_APPROVE | m::PRESENCE_KEYS_APPROVE => {
                let q: SpawnKeysApproveParams = params(p)?;
                let device_fp = store
                    .approve(&q.device_fp, &q.spawn_key_fp, now_ms())
                    .map_err(err)?;
                Ok(to_json(&SpawnKeysApproveResult { device_fp }))
            }
            _ => {
                let q: SpawnKeysRevokeParams = params(p)?;
                let r = store.revoke(&q.device_fp).map_err(err)?;
                Ok(to_json(&SpawnKeysRevokeResult {
                    device_fp: r.device_fp,
                    device_id: r.device_id,
                    spawn_key_fp: r.spawn_key_fp,
                    state: state_name(r.state).into(),
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::manager::tests::{add, runner};
    use super::*;
    use crate::spawnauth::testutil::{fp_of, offer, presence_offer, spawn_fp, USER};
    use crate::spawnauth::{Identity, SpawnAuth, TrustSource};
    use std::os::unix::net::UnixStream;
    use tmgr_ipc::api::method;
    use tmgr_spawn_auth::TrustStatus;

    struct Trusted;

    impl TrustSource for Trusted {
        fn status(&self, _: &str, _: &str) -> TrustStatus {
            TrustStatus::Trusted
        }
    }

    struct Env {
        _t: tempfile::TempDir,
        rt: Arc<Runner>,
        auth: Arc<SpawnAuth>,
        fp: String,
    }

    fn env() -> Env {
        let t = tempfile::Builder::new()
            .prefix("sk")
            .tempdir_in("/tmp")
            .unwrap();
        let rt = runner(t.path());
        add(&rt, t.path(), "s_aaaaaaaaaa", true);
        let identity = Identity::new(USER, rt.chat.machine(), &"b".repeat(64)).unwrap();
        let auth =
            Arc::new(SpawnAuth::open(&t.path().join("sa"), identity, Arc::new(Trusted)).unwrap());
        *lock(&rt.spawn_gate.fixed) = Some(auth.clone());
        let o = offer(3, now_ms() - 10_000);
        let fp = fp_of(&o);
        auth.offer_key(o, now_ms()).unwrap();
        Env {
            _t: t,
            rt,
            auth,
            fp,
        }
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
            peer_pid: None,
            pending_rx: Mutex::new(None),
            chat_rx: Mutex::new(None),
            chat_subscribed: AtomicBool::new(false),
        });
        (c, b)
    }

    fn call(e: &Env, client: &str, m: &str, params: Value) -> Result<Value, ApiErr> {
        let (c, _peer) = conn(client);
        e.rt.dispatch(
            &c,
            &Request {
                id: 1,
                method: m.into(),
                params,
            },
        )
    }

    #[test]
    fn only_the_cli_may_manage_spawn_keys() {
        let e = env();
        let calls = [
            (method::SPAWN_KEYS_LIST, serde_json::json!({})),
            (
                method::SPAWN_KEYS_APPROVE,
                serde_json::json!({"device_fp": &e.fp[..8], "spawn_key_fp": &spawn_fp(3)[..16]}),
            ),
            (
                method::SPAWN_KEYS_REVOKE,
                serde_json::json!({"device_fp": &e.fp[..8]}),
            ),
        ];
        for client in [CLIENT_DESKTOP, CLIENT_WORKER, "manager", "other", ""] {
            for (m, p) in &calls {
                let err = call(&e, client, m, p.clone()).unwrap_err();
                assert_eq!(err.code, ErrorCode::Forbidden, "{client} {m}");
            }
        }
        assert_eq!(e.auth.pinned_key(&e.fp).unwrap(), None);
        assert_eq!(e.auth.list_keys().unwrap().len(), 1);
    }

    #[test]
    fn the_cli_lists_approves_and_revokes() {
        let e = env();
        let v = call(&e, CLIENT_CLI, method::SPAWN_KEYS_LIST, Value::Null).unwrap();
        let l: SpawnKeysList = serde_json::from_value(v).unwrap();
        assert_eq!(l.keys.len(), 1);
        assert_eq!(l.keys[0].device_fp, e.fp);
        assert_eq!(l.keys[0].spawn_key_fp, spawn_fp(3));
        assert_eq!(l.keys[0].state, "pending");

        let v = call(
            &e,
            CLIENT_CLI,
            method::SPAWN_KEYS_APPROVE,
            serde_json::json!({"device_fp": &e.fp[..8], "spawn_key_fp": &spawn_fp(3)[..16]}),
        )
        .unwrap();
        assert_eq!(
            serde_json::from_value::<SpawnKeysApproveResult>(v)
                .unwrap()
                .device_fp,
            e.fp
        );
        assert!(e.auth.pinned_key(&e.fp).unwrap().is_some());

        let v = call(
            &e,
            CLIENT_CLI,
            method::SPAWN_KEYS_REVOKE,
            serde_json::json!({"device_fp": &e.fp[..8]}),
        )
        .unwrap();
        let r: SpawnKeysRevokeResult = serde_json::from_value(v).unwrap();
        assert_eq!(r.device_fp, e.fp);
        assert_eq!(r.spawn_key_fp, spawn_fp(3));
        assert_eq!(r.state, "approved");
        assert_eq!(e.auth.pinned_key(&e.fp).unwrap(), None);
    }

    #[test]
    fn presence_keys_are_a_separate_book_managed_only_from_the_cli() {
        let e = env();
        let o = presence_offer(9, now_ms() - 10_000);
        let fp = fp_of(&o);
        let desk = e.auth.clone();
        desk.offer_presence_key(o, now_ms()).unwrap();
        let approve =
            serde_json::json!({"device_fp": &fp[..8], "spawn_key_fp": &spawn_fp(9)[..16]});
        let err = call(
            &e,
            CLIENT_DESKTOP,
            method::PRESENCE_KEYS_APPROVE,
            approve.clone(),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::Forbidden);
        assert!(err.msg.contains("presence keys"));
        let v = call(&e, CLIENT_CLI, method::PRESENCE_KEYS_LIST, Value::Null).unwrap();
        let l: SpawnKeysList = serde_json::from_value(v).unwrap();
        assert_eq!(l.keys.len(), 1);
        assert_eq!(l.keys[0].state, "pending");
        call(&e, CLIENT_CLI, method::PRESENCE_KEYS_APPROVE, approve).unwrap();
        assert!(desk.presence_key(&fp).unwrap().is_some());
        assert!(desk.pinned_key(&fp).unwrap().is_none());
        assert_eq!(desk.approved_presence_fps().unwrap(), vec![fp.clone()]);
        let err = call(
            &e,
            CLIENT_CLI,
            method::PRESENCE_KEYS_REVOKE,
            serde_json::json!({"device_fp": "ffffffff"}),
        )
        .unwrap_err();
        assert_eq!(err.msg, "no presence key matches the prefix");
        call(
            &e,
            CLIENT_CLI,
            method::PRESENCE_KEYS_REVOKE,
            serde_json::json!({"device_fp": &fp[..8]}),
        )
        .unwrap();
        assert!(desk.presence_key(&fp).unwrap().is_none());
    }

    #[test]
    fn key_errors_are_invalid_params() {
        let e = env();
        let err = call(
            &e,
            CLIENT_CLI,
            method::SPAWN_KEYS_APPROVE,
            serde_json::json!({"device_fp": "abc", "spawn_key_fp": "def"}),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidParams);
        assert!(err.msg.contains("too short"));
        let err = call(
            &e,
            CLIENT_CLI,
            method::SPAWN_KEYS_REVOKE,
            serde_json::json!({"device_fp": "ffffffff"}),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidParams);
    }
}
