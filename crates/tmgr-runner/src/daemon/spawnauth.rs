use super::*;
use crate::chat::relay::store::RelayDir;
use crate::manager::{Proposed, Verdict};
use crate::spawnauth::{
    CardError, CardResult, CardState, ClaimOutcome, Identity, IssueOutcome, KeyOffer, SpawnAuth,
    TrustSource,
};
use std::collections::VecDeque;
use tmgr_chat_relay::api::Request as RelayRequest;
use tmgr_chat_relay::engine::{
    ChatSink, DeviceState, EngineConfig, RelayEngine, Transport, TrustState,
};
use tmgr_chat_relay::trust::{verify_spawnkey, SpawnKey};
use tmgr_ipc::api::{SpawnChallengeParams, SpawnChallengeResult};
use tmgr_spawn_auth::{b64_encode, OfferOutcome, Refusal, SignedSpawn, SpawnProposal, TrustStatus};

const CHALLENGES_PER_MINUTE: usize = 10;
const NO_ACCOUNT: &str = "spawn approval is not available: this machine is not linked to an account through the chat relay";
const STORE_ERROR: &str = "spawn approval store is not available";
const INTERRUPTED: &str = "interrupted; ask again with a new card";

#[derive(Default)]
pub(super) struct SpawnGate {
    slot: Mutex<Option<(Identity, Arc<SpawnAuth>)>>,
    asked: Mutex<VecDeque<u64>>,
    proposed: Mutex<VecDeque<u64>>,
    #[cfg(test)]
    pub(super) fixed: Mutex<Option<Arc<SpawnAuth>>>,
}

struct Offline;

impl Transport for Offline {
    fn call(&self, _: &RelayRequest, _: Option<&str>) -> Result<String, String> {
        Err("offline".into())
    }
}

struct Discard;

impl ChatSink for Discard {
    fn deliver(&self, _: &str, _: tmgr_ipc::chat::ChatMessage) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Budget {
    Human,
    Manager,
}

fn relay_state(dir: &Path, machine: &str) -> Option<(u64, TrustState)> {
    fs::symlink_metadata(dir.join("relay")).ok()?;
    let relay = RelayDir::open(dir).ok()?;
    let user_id = relay.load_config().ok()??.user_id;
    let cfg = EngineConfig {
        user_id,
        label: "Runner".into(),
        local_machine: machine.into(),
    };
    let state = RelayEngine::new(Offline, relay, Discard, cfg)
        .ok()?
        .trust_state()
        .ok()?;
    state.registered.then_some((user_id, state))
}

fn identity_of(user_id: u64, machine: &str, state: &TrustState) -> Option<Identity> {
    Identity::new(&user_id.to_string(), machine, state.self_fp.as_deref()?)
}

fn status_of(state: &TrustState, device_id: &str, fp: &str) -> TrustStatus {
    if !state.registered {
        return TrustStatus::Unknown;
    }
    let named: Vec<_> = state.devices.iter().filter(|d| d.id == device_id).collect();
    let exact: Vec<_> = named.iter().filter(|d| d.fp == fp).collect();
    if exact.iter().any(|d| d.state == DeviceState::Revoked) {
        TrustStatus::Revoked
    } else if exact
        .iter()
        .any(|d| d.state == DeviceState::Trusted && !d.is_self)
    {
        TrustStatus::Trusted
    } else if named.is_empty() {
        TrustStatus::Unknown
    } else {
        TrustStatus::NotTrusted
    }
}

struct RelayTrust {
    dir: PathBuf,
    machine: String,
}

impl TrustSource for RelayTrust {
    fn status(&self, device_id: &str, fp: &str) -> TrustStatus {
        match relay_state(&self.dir, &self.machine) {
            Some((_, state)) => status_of(&state, device_id, fp),
            None => TrustStatus::Unknown,
        }
    }
}

impl SpawnGate {
    fn auth(&self, rt: &Runner) -> Result<Arc<SpawnAuth>, String> {
        #[cfg(test)]
        if let Some(a) = lock(&self.fixed).clone() {
            return Ok(a);
        }
        let machine = rt.chat.machine();
        let (user_id, state) = relay_state(&rt.cfg.dir, machine).ok_or(NO_ACCOUNT)?;
        let identity = identity_of(user_id, machine, &state).ok_or(NO_ACCOUNT)?;
        let mut slot = lock(&self.slot);
        if let Some((have, auth)) = slot.as_ref() {
            if *have == identity {
                return Ok(auth.clone());
            }
        }
        let trust = Arc::new(RelayTrust {
            dir: rt.cfg.dir.clone(),
            machine: machine.to_owned(),
        });
        let auth = SpawnAuth::open(&rt.cfg.dir, identity.clone(), trust)
            .map(Arc::new)
            .map_err(|_| STORE_ERROR.to_owned())?;
        if auth.recover(now_ms()).is_err() {
            log("spawn approval: recovery of the card store failed");
        }
        *slot = Some((identity, auth.clone()));
        Ok(auth)
    }

    fn challenge_slot(&self, now: u64, budget: Budget) -> bool {
        let mut w = lock(match budget {
            Budget::Human => &self.asked,
            Budget::Manager => &self.proposed,
        });
        while w.front().is_some_and(|t| now.saturating_sub(*t) >= 60_000) {
            w.pop_front();
        }
        if w.len() >= CHALLENGES_PER_MINUTE {
            return false;
        }
        w.push_back(now);
        true
    }
}

fn card_error(e: CardError) -> String {
    match e {
        CardError::BadCardId => "invalid card id".into(),
        _ => STORE_ERROR.into(),
    }
}

impl Runner {
    fn spawn_challenge_within(
        &self,
        card_id: &str,
        proposal: &SpawnProposal,
        budget: Budget,
    ) -> Result<SpawnChallengeResult, ApiErr> {
        let now = now_ms();
        if !self.spawn_gate.challenge_slot(now, budget) {
            return Err(api(
                ErrorCode::Limit,
                format!("at most {CHALLENGES_PER_MINUTE} spawn challenges per minute"),
            ));
        }
        let auth = self
            .spawn_gate
            .auth(self)
            .map_err(|m| api(ErrorCode::Forbidden, m))?;
        match auth
            .issue(card_id, proposal, now)
            .map_err(|e| api(ErrorCode::InvalidParams, card_error(e)))?
        {
            IssueOutcome::Challenge(c) => Ok(SpawnChallengeResult {
                nonce: c.nonce,
                issued_ms: c.issued_ms,
                exp_ms: c.exp_ms,
                target_id: c.target_id,
                target_fp: c.target_fp,
                req: b64_encode(&c.req),
                req_sha256: c.req_sha256,
            }),
            IssueOutcome::Invalid(r) => Err(api(ErrorCode::InvalidParams, r.to_string())),
            IssueOutcome::Conflict => Err(api(
                ErrorCode::InvalidParams,
                "this card already exists with a different request",
            )),
            IssueOutcome::Full => Err(api(
                ErrorCode::Busy,
                "too many spawn cards are waiting for approval",
            )),
            IssueOutcome::Closed(_) => Err(api(ErrorCode::InvalidParams, "this card is closed")),
        }
    }

    pub(super) fn spawn_challenge_call(&self, conn: &Conn, p: &Value) -> Result<Value, ApiErr> {
        if conn.client != CLIENT_CLI && conn.client != CLIENT_DESKTOP {
            return Err(api(
                ErrorCode::Forbidden,
                "spawn challenges are available only to the tmgr CLI and the desktop app",
            ));
        }
        let q: SpawnChallengeParams = params(p)?;
        self.spawn_challenge_within(&q.card_id, &q.proposal, Budget::Human)
            .map(|r| to_json(&r))
    }

    pub(super) fn propose_spawn(&self, proposal: &SpawnProposal) -> Result<Proposed, String> {
        let mut raw = [0u8; 16];
        getrandom::fill(&mut raw).map_err(|_| "no random source".to_string())?;
        let card_id = format!("card_{}", crate::spawnauth::hex_lower(&raw));
        let c = self
            .spawn_challenge_within(&card_id, proposal, Budget::Manager)
            .map_err(|e| e.msg)?;
        Ok(Proposed {
            card_id,
            exp_ms: c.exp_ms,
            req_sha256: c.req_sha256,
        })
    }

    pub(super) fn verify_spawn(
        &self,
        wire: &SignedSpawn,
        proposal: &SpawnProposal,
    ) -> Result<Verdict, String> {
        let auth = self.spawn_gate.auth(self)?;
        let card = wire.card_id.as_str();
        let now = now_ms();
        let mismatch = || Refusal::Mismatch.to_string();
        let stored = auth.cards().state(card).map_err(card_error)?;
        if let Some(stored) = stored.as_ref().and_then(CardState::req_sha256) {
            if stored != proposal.req_sha256() {
                return Err(mismatch());
            }
        }
        match auth.claim(card, wire, now).map_err(card_error)? {
            ClaimOutcome::Verified(approved) => {
                if approved.proposal() != proposal {
                    return Err(mismatch());
                }
                Ok(Verdict::Verified)
            }
            ClaimOutcome::Rejected(r) => Err(r.to_string()),
            ClaimOutcome::AlreadyClaimed(CardResult::Spawned { session }) => self
                .session(&session)
                .map(|s| Verdict::Existing(Box::new(s.info())))
                .ok_or_else(|| {
                    "the session of this card is gone; ask again with a new card".into()
                }),
            ClaimOutcome::AlreadyClaimed(_) => match self.card_session(card) {
                Some(info) => {
                    self.spawn_done(card, &info.id);
                    Ok(Verdict::Existing(Box::new(info)))
                }
                None => Err(INTERRUPTED.into()),
            },
            ClaimOutcome::Dead { .. } => {
                Err("this card is closed; ask again with a new card".into())
            }
            ClaimOutcome::UnknownCard => Err("unknown card".into()),
        }
    }

    pub(super) fn spawn_done(&self, card_id: &str, session: &str) {
        let Ok(auth) = self.spawn_gate.auth(self) else {
            return;
        };
        if auth
            .cards()
            .mark_spawned(card_id, session, now_ms())
            .is_err()
        {
            log(&format!(
                "spawn approval: session {session} is running but its card was not marked"
            ));
        }
    }

    fn card_session(&self, card_id: &str) -> Option<SessionInfo> {
        self.all()
            .into_iter()
            .find(|s| !s.is_exited() && lock(&s.inner).meta.spawn_card.as_deref() == Some(card_id))
            .map(|s| s.info())
    }
}

#[allow(dead_code)] // the relay engine does not deliver spawnkey messages yet
pub(crate) trait DeviceDirectory {
    fn sign_and_kex(&self, device_id: &str) -> Option<(String, String)>;
}

#[allow(dead_code)] // see DeviceDirectory
pub(crate) fn ingest_spawnkey(
    auth: &SpawnAuth,
    directory: &dyn DeviceDirectory,
    user_id: u64,
    device_id: &str,
    statement_json: &str,
    now: u64,
) -> Result<OfferOutcome, &'static str> {
    let (sign_pub_b64, kex_pub_b64) = directory.sign_and_kex(device_id).ok_or("unknown device")?;
    verify_spawnkey(
        statement_json,
        user_id,
        device_id,
        &sign_pub_b64,
        &kex_pub_b64,
    )
    .map_err(|_| "spawn key statement rejected")?;
    let key: SpawnKey =
        serde_json::from_str(statement_json).map_err(|_| "spawn key statement rejected")?;
    auth.offer_key(
        KeyOffer {
            statement: key.statement,
            sig: key.sig,
            spawn_sig: key.spawn_sig,
            device_id: device_id.to_owned(),
            sign_pub_b64,
            kex_pub_b64,
        },
        now,
    )
    .map_err(|_| "spawn key rejected")
}

#[cfg(test)]
mod tests {
    use super::manager::tests::{add, runner};
    use super::*;
    use crate::manager::{self as core, Host, SpawnSpec};
    use crate::spawnauth::testutil::{
        fp_of, kex_key, offer, offer_with, secret, sign, spawn_fp, USER,
    };

    const PHONE: &str = "00000000-0000-4000-8000-000000000001";
    use std::os::unix::net::UnixStream;
    use std::sync::atomic::AtomicU8;
    use tmgr_ipc::manager::SpoolReq;

    const MGR: &str = "s_aaaaaaaaaa";

    struct Source(Mutex<TrustStatus>);

    impl TrustSource for Source {
        fn status(&self, _: &str, _: &str) -> TrustStatus {
            *lock(&self.0)
        }
    }

    struct Env {
        _t: tempfile::TempDir,
        rt: Arc<Runner>,
        auth: Arc<SpawnAuth>,
        work: PathBuf,
        fp: String,
    }

    fn env() -> Env {
        let t = tempfile::Builder::new()
            .prefix("sa")
            .tempdir_in("/tmp")
            .unwrap();
        let rt = runner(t.path());
        add(&rt, t.path(), MGR, true);
        let work = t.path().join("work");
        fs::create_dir_all(&work).unwrap();
        let identity = Identity::new(USER, rt.chat.machine(), &"b".repeat(64)).unwrap();
        let source = Arc::new(Source(Mutex::new(TrustStatus::Trusted)));
        let auth = Arc::new(SpawnAuth::open(&t.path().join("sa"), identity, source).unwrap());
        *lock(&rt.spawn_gate.fixed) = Some(auth.clone());
        let o = offer(3, now_ms() - 10_000);
        let fp = fp_of(&o);
        auth.offer_key(o, now_ms()).unwrap();
        auth.approve_key(&fp[..8], &spawn_fp(3)[..16], now_ms())
            .unwrap();
        Env {
            _t: t,
            rt,
            auth,
            work,
            fp,
        }
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Fail {
        No,
        BeforeCreate,
        AfterCreate,
    }

    struct TestHost<'a> {
        inner: super::manager::ManagerHost<'a>,
        work: PathBuf,
        spawned: Mutex<Vec<(String, Option<String>)>>,
        fail: AtomicU8,
    }

    impl<'a> TestHost<'a> {
        fn new(e: &'a Env) -> Self {
            TestHost {
                inner: super::manager::ManagerHost { rt: &e.rt, me: MGR },
                work: e.work.clone(),
                spawned: Mutex::default(),
                fail: AtomicU8::new(0),
            }
        }

        fn fail(&self, f: Fail) {
            self.fail.store(f as u8, Ordering::SeqCst);
        }
    }

    impl Host for TestHost<'_> {
        fn me(&self) -> &str {
            self.inner.me()
        }
        fn machine(&self) -> String {
            self.inner.machine()
        }
        fn now_ms(&self) -> u64 {
            now_ms()
        }
        fn sessions(&self) -> Vec<SessionInfo> {
            self.inner.sessions()
        }
        fn text(&self, id: &str, lines: u32) -> Result<Vec<String>, String> {
            self.inner.text(id, lines)
        }
        fn send(&self, id: &str, text: &str) -> Result<(), String> {
            self.inner.send(id, text)
        }
        fn trusted_dir(&self, cwd: &str) -> Result<core::TrustedDir, String> {
            let p = PathBuf::from(cwd);
            p.starts_with(&self.work)
                .then_some(core::TrustedDir {
                    path: p,
                    pin: CwdPin { dev: 0, ino: 0 },
                })
                .ok_or_else(|| "cwd is not a trusted folder".into())
        }
        fn precheck_spawn(&self, spec: &SpawnSpec) -> Result<(), String> {
            self.inner.precheck_spawn(spec)
        }
        fn spawn(&self, spec: SpawnSpec) -> Result<SessionInfo, String> {
            let fail = self.fail.load(Ordering::SeqCst);
            if fail == Fail::BeforeCreate as u8 {
                return Err("agent did not start".into());
            }
            let id = new_session_id();
            let meta: Meta = serde_json::from_value(serde_json::json!({
                "id": id, "name": id, "argv": ["claude"], "cwd": spec.cwd,
                "profile": "claude-code", "created_ms": 0, "cols": 100, "rows": 24,
                "shim_pid": 1, "child_pid": 2, "spawn_card": spec.card,
                "permission_mode": spec.mode,
            }))
            .unwrap();
            let sdir = self.work.join("sessions").join(&id);
            fs::create_dir_all(&sdir).unwrap();
            let s = self.inner.rt.new_session(meta, None, &sdir);
            lock(&self.inner.rt.sessions).insert(id.clone(), s);
            self.spawned.lock().unwrap().push((id.clone(), spec.card));
            if fail == Fail::AfterCreate as u8 {
                return Err("lost the answer".into());
            }
            Ok(self.inner.rt.session(&id).unwrap().info())
        }
        fn verify_spawn(
            &self,
            a: &SignedSpawn,
            p: &SpawnProposal,
        ) -> Result<core::Verdict, String> {
            self.inner.verify_spawn(a, p)
        }
        fn spawn_done(&self, c: &str, s: &str) {
            self.inner.spawn_done(c, s)
        }
        fn propose_spawn(&self, p: &SpawnProposal) -> Result<core::Proposed, String> {
            self.inner.propose_spawn(p)
        }
        fn switch(&self, id: &str, to: &str) -> Result<core::SwitchResult, String> {
            self.inner.switch(id, to)
        }
    }

    fn run(h: &TestHost, e: &Env, tool: &str, args: Value) -> Result<Value, String> {
        let req = SpoolReq {
            v: 1,
            id: "x".into(),
            tool: tool.into(),
            args,
        };
        core::execute(h, &e.rt.mgr, &req)
    }

    fn propose(h: &TestHost, e: &Env, mode: &str, dir: &str) -> String {
        let args = serde_json::json!({
            "profile": "claude-code", "cwd": e.work.join(dir), "permission_mode": mode, "prompt": "go"
        });
        run(h, e, "propose_spawn", args).unwrap()["card_id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn approval(e: &Env, card: &str) -> Value {
        let Some(CardState::Issued {
            nonce,
            issued_ms,
            exp_ms,
            req_sha256,
            target_id,
            target_fp,
            ..
        }) = e.auth.cards().state(card).unwrap()
        else {
            panic!("card is not issued");
        };
        let _ = issued_ms;
        let ts = now_ms();
        let msg = format!(
            "tmgr-spawn:v1:{USER}:{}:{target_id}:{target_fp}:{card}:{nonce}:{ts}:{exp_ms}:{req_sha256}",
            e.fp
        );
        serde_json::to_value(SignedSpawn {
            v: 1,
            user_id: USER.into(),
            device_fp: e.fp.clone(),
            target_id,
            target_fp,
            card_id: card.into(),
            nonce,
            ts_ms: ts,
            exp_ms,
            req_sha256,
            alg: "es256".into(),
            sig: sign(&secret(3), &msg),
        })
        .unwrap()
    }

    fn spawn_args(e: &Env, mode: &str, dir: &str, card: &str) -> Value {
        serde_json::json!({
            "profile": "claude-code", "cwd": e.work.join(dir), "permission_mode": mode,
            "prompt": "go", "approval": approval(e, card)
        })
    }

    fn with_dirs(e: &Env) {
        for d in ["a", "b"] {
            fs::create_dir_all(e.work.join(d)).unwrap();
        }
    }

    #[test]
    fn a_signed_bypass_spawn_happens_once_and_a_retry_returns_the_same_session() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let args = spawn_args(&e, "bypass", "a", &card);
        let first = run(&h, &e, "spawn_session", args.clone()).unwrap();
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
        assert_eq!(
            h.spawned.lock().unwrap()[0].1.as_deref(),
            Some(card.as_str())
        );
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Spawned { .. })
        ));
        let again = run(&h, &e, "spawn_session", args).unwrap();
        assert_eq!(again["id"], first["id"]);
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_changed_mode_or_folder_after_the_challenge_is_refused_and_keeps_the_card() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let bad_mode = spawn_args(&e, "accept-edits", "a", &card);
        assert!(run(&h, &e, "spawn_session", bad_mode)
            .unwrap_err()
            .contains("does not match"));
        let bad_cwd = spawn_args(&e, "bypass", "b", &card);
        assert!(run(&h, &e, "spawn_session", bad_cwd).is_err());
        assert!(h.spawned.lock().unwrap().is_empty());
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Issued { .. })
        ));
        let good = spawn_args(&e, "bypass", "a", &card);
        run(&h, &e, "spawn_session", good).unwrap();
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
    }

    #[test]
    fn no_sandbox_and_manager_are_refused_even_when_signed_and_keep_the_card() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        for k in ["no_sandbox", "manager"] {
            let mut args = spawn_args(&e, "bypass", "a", &card);
            args[k] = serde_json::json!(true);
            assert!(run(&h, &e, "spawn_session", args).is_err());
        }
        assert!(h.spawned.lock().unwrap().is_empty());
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Issued { .. })
        ));
    }

    #[test]
    fn bypass_without_an_approval_is_still_refused() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let args = serde_json::json!({
            "profile": "claude-code", "cwd": e.work.join("a"), "permission_mode": "bypass"
        });
        assert!(run(&h, &e, "spawn_session", args).is_err());
        assert!(h.spawned.lock().unwrap().is_empty());
    }

    #[test]
    fn a_failed_spawn_after_the_claim_is_interrupted_and_a_retry_never_spawns() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let args = spawn_args(&e, "bypass", "a", &card);
        h.fail(Fail::BeforeCreate);
        assert!(run(&h, &e, "spawn_session", args.clone()).is_err());
        h.fail(Fail::No);
        let err = run(&h, &e, "spawn_session", args).unwrap_err();
        assert_eq!(err, "interrupted; ask again with a new card");
        assert!(h.spawned.lock().unwrap().is_empty());
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Claimed { .. })
        ));
    }

    #[test]
    fn an_interrupted_card_with_a_live_tagged_session_returns_that_session() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let args = spawn_args(&e, "bypass", "a", &card);
        h.fail(Fail::AfterCreate);
        assert!(run(&h, &e, "spawn_session", args.clone()).is_err());
        h.fail(Fail::No);
        let v = run(&h, &e, "spawn_session", args.clone()).unwrap();
        assert_eq!(v["id"], h.spawned.lock().unwrap()[0].0);
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Spawned { .. })
        ));
    }

    #[test]
    fn signed_spawns_share_the_per_minute_spawn_limit() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let mut last = String::new();
        for _ in 0..tmgr_ipc::manager::limits::SPAWNS_PER_MINUTE {
            let card = propose(&h, &e, "bypass", "a");
            run(
                &h,
                &e,
                "spawn_session",
                spawn_args(&e, "bypass", "a", &card),
            )
            .unwrap();
            last = card;
        }
        let card = propose(&h, &e, "bypass", "a");
        let err = run(
            &h,
            &e,
            "spawn_session",
            spawn_args(&e, "bypass", "a", &card),
        )
        .unwrap_err();
        assert!(err.contains("per minute"));
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Issued { .. })
        ));
        assert!(!last.is_empty());
    }

    #[test]
    fn a_retry_of_a_spawned_card_with_other_arguments_is_refused_and_returns_no_session() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let args = spawn_args(&e, "bypass", "a", &card);
        let first = run(&h, &e, "spawn_session", args.clone()).unwrap();
        for (key, other) in [
            ("permission_mode", serde_json::json!("accept-edits")),
            ("cwd", serde_json::json!(e.work.join("b"))),
            ("prompt", serde_json::json!("different")),
        ] {
            let mut changed = args.clone();
            changed[key] = other;
            let err = run(&h, &e, "spawn_session", changed).unwrap_err();
            assert_eq!(err, Refusal::Mismatch.to_string(), "{key}");
        }
        assert_eq!(
            run(&h, &e, "spawn_session", args).unwrap()["id"],
            first["id"]
        );
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
    }

    #[test]
    fn an_interrupted_card_healed_with_other_arguments_is_refused_and_stays_claimed() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let args = spawn_args(&e, "bypass", "a", &card);
        h.fail(Fail::AfterCreate);
        assert!(run(&h, &e, "spawn_session", args.clone()).is_err());
        h.fail(Fail::No);
        let mut changed = args;
        changed["permission_mode"] = serde_json::json!("accept-edits");
        let err = run(&h, &e, "spawn_session", changed).unwrap_err();
        assert_eq!(err, Refusal::Mismatch.to_string());
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Claimed { .. })
        ));
    }

    #[test]
    fn a_profile_that_cannot_bypass_is_refused_before_the_card_is_claimed() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let args = serde_json::json!({
            "profile": "opencode", "cwd": e.work.join("a"), "permission_mode": "bypass", "prompt": "go"
        });
        let card = run(&h, &e, "propose_spawn", args.clone()).unwrap()["card_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut signed = args;
        signed["approval"] = approval(&e, &card);
        let err = run(&h, &e, "spawn_session", signed).unwrap_err();
        assert!(err.contains("cannot run in mode bypass"), "{err}");
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Issued { .. })
        ));
    }

    #[test]
    fn refused_verifications_do_not_use_up_the_spawn_rate_window() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let mut bad = spawn_args(&e, "bypass", "a", &card);
        bad["approval"]["sig"] = serde_json::json!("AAAA");
        for _ in 0..tmgr_ipc::manager::limits::SPAWNS_PER_MINUTE + 2 {
            let err = run(&h, &e, "spawn_session", bad.clone()).unwrap_err();
            assert!(!err.contains("per minute"), "{err}");
        }
    }

    #[test]
    fn the_manager_and_the_human_have_separate_challenge_budgets() {
        let gate = SpawnGate::default();
        for _ in 0..CHALLENGES_PER_MINUTE {
            assert!(gate.challenge_slot(1_000, Budget::Manager));
        }
        assert!(!gate.challenge_slot(1_000, Budget::Manager));
        for _ in 0..CHALLENGES_PER_MINUTE {
            assert!(gate.challenge_slot(1_000, Budget::Human));
        }
        assert!(!gate.challenge_slot(1_000, Budget::Human));
        assert!(gate.challenge_slot(61_000, Budget::Manager));
    }

    #[test]
    fn a_looping_manager_cannot_use_up_the_human_challenge_budget() {
        let e = env();
        let proposal: SpawnProposal = serde_json::from_value(serde_json::json!({
            "v": 1, "profile": "claude-code", "cwd": e.work.display().to_string(),
            "mode": "bypass", "target_id": e.rt.chat.machine()
        }))
        .unwrap();
        let mut limited = false;
        for _ in 0..CHALLENGES_PER_MINUTE + 2 {
            limited |= matches!(
                e.rt.propose_spawn(&proposal),
                Err(m) if m.contains("per minute")
            );
        }
        assert!(limited);
        let err = challenge(&e, CLIENT_CLI, "card_human").unwrap_err();
        assert_ne!(err.code, ErrorCode::Limit);
    }

    #[test]
    fn an_unlinked_runner_dir_is_not_given_a_relay_directory_by_a_trust_lookup() {
        let t = tempfile::tempdir().unwrap();
        assert!(relay_state(t.path(), "mx_0123456789abcdef").is_none());
        assert!(!t.path().join("relay").exists());
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

    fn challenge_params(e: &Env, card: &str) -> Value {
        serde_json::json!({
            "card_id": card,
            "proposal": {
                "v": 1, "profile": "claude-code", "cwd": e.work.display().to_string(),
                "mode": "bypass", "target_id": e.rt.chat.machine()
            }
        })
    }

    fn challenge(e: &Env, client: &str, card: &str) -> Result<Value, ApiErr> {
        let (c, _peer) = conn(client);
        e.rt.dispatch(
            &c,
            &Request {
                id: 1,
                method: method::SPAWN_CHALLENGE.into(),
                params: challenge_params(e, card),
            },
        )
    }

    #[test]
    fn only_cli_and_desktop_may_ask_for_a_challenge() {
        let e = env();
        for client in [CLIENT_WORKER, "other", ""] {
            let err = challenge(&e, client, "card_w").unwrap_err();
            assert_eq!(err.code, ErrorCode::Forbidden, "{client}");
        }
        assert!(e.auth.cards().state("card_w").unwrap().is_none());
        for client in [CLIENT_CLI, CLIENT_DESKTOP] {
            let v = challenge(&e, client, "card_ok").unwrap();
            let r: SpawnChallengeResult = serde_json::from_value(v).unwrap();
            assert_eq!(r.target_id, e.rt.chat.machine());
            let bytes = tmgr_spawn_auth::b64_decode_strict(&r.req).unwrap();
            assert_eq!(tmgr_spawn_auth::sha256_hex(&bytes), r.req_sha256);
        }
    }

    #[test]
    fn a_client_cannot_set_the_runner_internal_spawn_card_tag() {
        let e = env();
        let (c, _peer) = conn(CLIENT_CLI);
        let err =
            e.rt.dispatch(
                &c,
                &Request {
                    id: 1,
                    method: method::RUNS_CREATE.into(),
                    params: serde_json::json!({
                        "profile": "claude-code", "cwd": "/tmp", "permission_mode": "bypass",
                        "spawn_card": "card_forged"
                    }),
                },
            )
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidParams);
        assert!(err.msg.contains("spawn_card"));
    }

    #[test]
    fn the_eleventh_challenge_in_a_minute_is_refused() {
        let e = env();
        for _ in 0..10 {
            challenge(&e, CLIENT_CLI, "card_r").unwrap();
        }
        let err = challenge(&e, CLIENT_CLI, "card_r").unwrap_err();
        assert_eq!(err.code, ErrorCode::Limit);
    }

    #[test]
    fn without_a_linked_account_challenges_and_verification_are_refused() {
        let t = tempfile::Builder::new()
            .prefix("sa")
            .tempdir_in("/tmp")
            .unwrap();
        let rt = runner(t.path());
        let proposal: SpawnProposal = serde_json::from_value(serde_json::json!({
            "v": 1, "profile": "claude-code", "cwd": "/w", "mode": "bypass",
            "target_id": rt.chat.machine()
        }))
        .unwrap();
        assert_eq!(
            rt.spawn_challenge_within("card_n", &proposal, Budget::Human)
                .unwrap_err()
                .code,
            ErrorCode::Forbidden
        );
        let wire: SignedSpawn = serde_json::from_value(serde_json::json!({
            "v":1,"user_id":"7","device_fp":"d","target_id":"mx_1","target_fp":"f","card_id":"card_n",
            "nonce":"n","ts_ms":1,"exp_ms":2,"req_sha256":"r","alg":"es256","sig":"s"
        }))
        .unwrap();
        assert!(rt.verify_spawn(&wire, &proposal).is_err());
    }

    #[test]
    fn relay_trust_maps_device_states_and_never_trusts_unknown_or_self() {
        use tmgr_chat_relay::engine::TrustDevice;
        let dev = |id: &str, fp: &str, state, is_self| TrustDevice {
            id: id.into(),
            kind: "phone".into(),
            label: "x".into(),
            fp: fp.into(),
            state,
            is_self,
            kind_verified: true,
            can_confirm_kind: false,
        };
        let st = TrustState {
            registered: true,
            self_fp: Some("c".repeat(64)),
            devices: vec![
                dev("t", "f1", DeviceState::Trusted, false),
                dev("r", "f2", DeviceState::Revoked, false),
                dev("p", "f3", DeviceState::Pending, false),
                dev("me", "f4", DeviceState::Trusted, true),
            ],
            ..TrustState::default()
        };
        assert_eq!(status_of(&st, "t", "f1"), TrustStatus::Trusted);
        assert_eq!(status_of(&st, "t", "other"), TrustStatus::NotTrusted);
        assert_eq!(status_of(&st, "r", "f2"), TrustStatus::Revoked);
        assert_eq!(status_of(&st, "p", "f3"), TrustStatus::NotTrusted);
        assert_eq!(status_of(&st, "me", "f4"), TrustStatus::NotTrusted);
        assert_eq!(status_of(&st, "nobody", "f1"), TrustStatus::Unknown);
        assert_eq!(
            status_of(&TrustState::default(), "t", "f1"),
            TrustStatus::Unknown
        );
        let id = identity_of(42, "mx_0123456789abcdef", &st).unwrap();
        assert_eq!(id.user_id(), "42");
        let no_fp = TrustState {
            registered: true,
            ..TrustState::default()
        };
        assert!(identity_of(42, "mx_0123456789abcdef", &no_fp).is_none());
    }

    struct Directory(Option<(String, String)>);

    impl DeviceDirectory for Directory {
        fn sign_and_kex(&self, _: &str) -> Option<(String, String)> {
            self.0.clone()
        }
    }

    fn statement_json(o: &KeyOffer) -> String {
        serde_json::to_string(&SpawnKey {
            statement: o.statement.clone(),
            sig: o.sig.clone(),
            spawn_sig: o.spawn_sig.clone(),
        })
        .unwrap()
    }

    fn fresh_auth() -> (tempfile::TempDir, SpawnAuth) {
        let t = tempfile::tempdir().unwrap();
        let identity = Identity::new(USER, "mx_0123456789abcdef", &"b".repeat(64)).unwrap();
        let source = Arc::new(Source(Mutex::new(TrustStatus::Trusted)));
        let auth = SpawnAuth::open(t.path(), identity, source).unwrap();
        (t, auth)
    }

    #[test]
    fn a_valid_spawnkey_statement_lands_as_pending_and_a_bad_one_is_rejected() {
        let (_t, auth) = fresh_auth();
        let o = offer_with(&kex_key(None), PHONE, &secret(3), now_ms() - 1_000);
        let dir = Directory(Some((o.sign_pub_b64.clone(), o.kex_pub_b64.clone())));
        let json = statement_json(&o);
        let out = ingest_spawnkey(&auth, &dir, 42, PHONE, &json, now_ms()).unwrap();
        assert_eq!(out, OfferOutcome::Added);
        assert_eq!(auth.pending_keys().unwrap().len(), 1);
        assert!(auth.pinned_key(&fp_of(&o)).unwrap().is_none());

        let (_t2, auth2) = fresh_auth();
        let mut bad = o.clone();
        bad.spawn_sig = o.sig.clone();
        let rejected = ingest_spawnkey(&auth2, &dir, 42, PHONE, &statement_json(&bad), now_ms());
        assert!(rejected.is_err());
        let wrong_user = ingest_spawnkey(&auth2, &dir, 43, PHONE, &json, now_ms());
        assert!(wrong_user.is_err());
        let unknown = ingest_spawnkey(&auth2, &Directory(None), 42, PHONE, &json, now_ms());
        assert!(unknown.is_err());
        assert!(auth2.pending_keys().unwrap().is_empty());
    }
}
