use super::spawnauth::release_queued;
use super::*;
use crate::permit::remote::{Action, PermitDesk, RunnerIds};
use crate::spawnauth::SpawnAuth;
use tmgr_ipc::chat::{Author, AuthorKind, ChatMessage, DecisionRef, MessageKind, CHAT_V};
use tmgr_spawn_auth::{DesktopPolicy, PermitSender, PermitSenderKind, SignedPermit};

const PERMIT_TICK: Duration = Duration::from_secs(1);
const PERMIT_INBOX_CAP: usize = 16;
const PERMIT_PER_DEVICE: usize = 4;
#[cfg_attr(not(test), allow(dead_code))]
const PERMIT_INTAKE_TICK: Duration = Duration::from_millis(500);
const POSTED_CAP: usize = 64;
const REASON_MAX: usize = 128;
const TOOL_MAX: usize = 64;
const PERMIT_CONFIG_FILE: &str = "permit.toml";
const PERMIT_CONFIG_MAX: u64 = 4096;
const DESKTOP_KEY: &str = "desktop_may_answer";

fn desktop_policy_from(text: Option<&str>) -> (DesktopPolicy, Option<String>) {
    let Some(text) = text else {
        return (DesktopPolicy::Safe, None);
    };
    let table: toml::Table = match text.parse() {
        Ok(t) => t,
        Err(_) => {
            return (
                DesktopPolicy::Safe,
                Some(format!("{PERMIT_CONFIG_FILE} is not valid")),
            )
        }
    };
    let Some(v) = table.get(DESKTOP_KEY) else {
        return (DesktopPolicy::Safe, None);
    };
    match v.as_str().map(str::parse::<DesktopPolicy>) {
        Some(Ok(p)) => (p, None),
        _ => (
            DesktopPolicy::Safe,
            Some(format!("{DESKTOP_KEY} must be none, safe or all")),
        ),
    }
}

fn load_desktop_policy(runner_dir: &Path) -> (DesktopPolicy, Option<String>) {
    let mut text = String::new();
    let read = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(runner_dir.join(PERMIT_CONFIG_FILE))
        .map(|f| f.take(PERMIT_CONFIG_MAX).read_to_string(&mut text));
    match read {
        Ok(Ok(_)) => desktop_policy_from(Some(&text)),
        Ok(Err(_)) => (
            DesktopPolicy::Safe,
            Some(format!("{PERMIT_CONFIG_FILE} cannot be read")),
        ),
        Err(_) => (DesktopPolicy::Safe, None),
    }
}

struct Posted {
    msg_id: Option<String>,
    session: String,
}

#[derive(Default)]
pub(super) struct PermitRelay {
    desk: Mutex<PermitDesk>,
    posted: Mutex<HashMap<String, Posted>>,
    last_tick: Mutex<Option<Instant>>,
    policy_warned: AtomicBool,
    #[cfg(test)]
    audits: Mutex<Vec<String>>,
}

fn nonces() -> Option<Vec<String>> {
    (0..2)
        .map(|_| {
            let mut b = [0u8; 16];
            getrandom::fill(&mut b).ok()?;
            Some(b.iter().map(|x| format!("{x:02x}")).collect::<String>())
        })
        .collect()
}

fn pool(mut nonces: Vec<String>) -> impl FnMut() -> String {
    move || nonces.pop().unwrap_or_else(|| "0".repeat(32))
}

fn clean(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect()
}

impl Runner {
    fn permit_ids(&self) -> Option<(Arc<SpawnAuth>, RunnerIds)> {
        let auth = self.spawn_gate.auth(self).ok()?;
        let id = auth.identity();
        let ids = RunnerIds {
            user_id: id.user_id().to_owned(),
            runner_id: id.target_id().to_owned(),
            runner_fp: id.target_fp().to_owned(),
        };
        Some((auth, ids))
    }

    fn permit_card_of(&self, session: &str) -> Option<PermissionCard> {
        let s = self.session(session)?;
        (!s.is_exited() && s.has_shim()).then(|| s.card()).flatten()
    }

    fn permit_sync_policy(&self) {
        let (policy, warning) = load_desktop_policy(&self.cfg.dir);
        match warning {
            Some(w) => {
                if !self
                    .permit_relay
                    .policy_warned
                    .swap(true, Ordering::Relaxed)
                {
                    log(&format!("permit relay: {w}, desktop answers use safe"));
                }
            }
            None => self
                .permit_relay
                .policy_warned
                .store(false, Ordering::Relaxed),
        }
        lock(&self.permit_relay.desk).set_desktop(policy);
    }

    pub(super) fn permit_tick(&self, now: u64) {
        {
            let mut last = lock(&self.permit_relay.last_tick);
            if last.is_some_and(|t| t.elapsed() < PERMIT_TICK) {
                return;
            }
            *last = Some(Instant::now());
        }
        self.permit_observe(now);
    }

    fn permit_observe(&self, now: u64) {
        let tracked: Vec<String> = lock(&self.permit_relay.posted)
            .values()
            .map(|p| p.session.clone())
            .collect();
        let mut seen: Vec<(String, Option<PermissionCard>)> = Vec::new();
        for s in self.all() {
            if s.card_agent().is_none() || s.is_exited() || !s.has_shim() {
                continue;
            }
            seen.push((s.id.clone(), s.card()));
        }
        for id in tracked {
            if !seen.iter().any(|(s, _)| *s == id) {
                seen.push((id, None));
            }
        }
        let busy = seen
            .iter()
            .any(|(_, c)| c.as_ref().is_some_and(|c| c.can_answer))
            || !lock(&self.permit_relay.posted).is_empty();
        if !busy {
            return;
        }
        let Some((_, ids)) = self.permit_ids() else {
            return;
        };
        let Some(pool_) = nonces() else {
            log("permit relay: no random source, the tick was skipped");
            return;
        };
        let mut fresh = pool(pool_);
        self.permit_sync_policy();
        let actions: Vec<Action> = {
            let mut desk = lock(&self.permit_relay.desk);
            seen.iter()
                .flat_map(|(session, card)| {
                    desk.observe(session, card.as_ref(), &ids, now, &mut fresh)
                })
                .collect()
        };
        self.permit_execute(actions, "relay:local");
    }

    pub(super) fn permit_answered_locally(&self, card_id: &str) {
        let actions = lock(&self.permit_relay.desk).answered_locally(card_id);
        self.permit_execute(actions, "relay:local");
    }

    #[cfg_attr(not(test), allow(dead_code))]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn accept_permitok(
        &self,
        _from_device: &str,
        from_fp: &str,
        owner_device: bool,
        kind: PermitSenderKind,
        sender_label: &str,
        signed: &SignedPermit,
        now: u64,
    ) {
        let Some((auth, _)) = self.permit_ids() else {
            log("permit relay: an answer was dropped, no account is linked");
            return;
        };
        let key = auth.pinned_key(&signed.device_fp).ok().flatten();
        let current = self.permit_card_of(&signed.session);
        let Some(pool_) = nonces() else {
            log("permit relay: no random source, an answer was dropped");
            return;
        };
        let mut fresh = pool(pool_);
        let trust = auth.trust_view();
        self.permit_sync_policy();
        let actions = lock(&self.permit_relay.desk).accept(
            signed,
            key.as_ref(),
            PermitSender {
                device_fp: from_fp,
                owner_device,
                kind,
                desktop: DesktopPolicy::default(),
            },
            &trust,
            &clean(sender_label, REASON_MAX / 2),
            current.as_ref(),
            now,
            &mut fresh,
        );
        let who: String = from_fp
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(12)
            .collect();
        self.permit_execute(actions, &format!("relay:{who}"));
    }

    fn permit_execute(&self, actions: Vec<Action>, client: &str) {
        let mut failed: Vec<String> = Vec::new();
        for action in actions {
            match action {
                Action::Post { card_id, card } => self.permit_post(&card_id, *card),
                Action::Close { card_id, reason } => {
                    if failed.contains(&card_id) {
                        continue;
                    }
                    self.permit_close(&card_id, &reason);
                }
                Action::Answer {
                    session,
                    card_id,
                    choice,
                    confirmed,
                    audit,
                } => match self.permit_answer(client, &session, &card_id, choice, confirmed) {
                    Ok(()) => self.permit_audit(&audit),
                    Err(code) => {
                        self.permit_audit(&format!(
                            "permit answer: card={} result=answer_failed code={code}",
                            clean(&card_id, 64)
                        ));
                        self.permit_close(&card_id, "gone");
                        failed.push(card_id);
                    }
                },
                Action::Audit(line) => self.permit_audit(&line),
            }
        }
    }

    fn permit_answer(
        &self,
        client: &str,
        session: &str,
        card_id: &str,
        choice: PermissionChoice,
        confirmed: bool,
    ) -> Result<(), String> {
        let s = self
            .session(session)
            .filter(|s| !s.is_exited() && s.has_shim())
            .ok_or("no_session")?;
        s.answer_card(
            client,
            &PermissionAnswer {
                session: session.to_owned(),
                card: card_id.to_owned(),
                choice,
                confirmed,
            },
        )
        .map_err(|e| String::from(e.code))
    }

    fn permit_audit(&self, line: &str) {
        log(line);
        #[cfg(test)]
        lock(&self.permit_relay.audits).push(line.to_owned());
    }

    fn permit_message(
        &self,
        body_md: String,
        decision: DecisionRef,
        reply_to: Option<String>,
    ) -> ChatMessage {
        ChatMessage {
            v: CHAT_V,
            id: self.chat.mint_id(),
            room: self.chat.manager_room(),
            author: Author {
                kind: AuthorKind::Manager,
                id: "manager".into(),
                machine: Some(self.chat.machine().to_owned()),
                device: None,
                agent: None,
            },
            ts_ms: now_ms(),
            kind: MessageKind::Decision,
            body_md,
            attachments: Vec::new(),
            reply_to,
            decision: Some(decision),
            session_meta: None,
            user_turn: false,
            mention: None,
        }
    }

    fn permit_post(&self, card_id: &str, card: tmgr_ipc::chat::PermitCardMsg) {
        let body = format!(
            "Permission needed in {}: {}",
            card.session,
            clean(&card.tool, TOOL_MAX)
        );
        let session = card.session.clone();
        let msg = self.permit_message(
            body,
            DecisionRef {
                card_id: card_id.to_owned(),
                spawn: None,
                permit: Some(card),
                closed: None,
            },
            None,
        );
        let msg_id = match self.chat.append(msg) {
            Ok(m) => Some(m.id),
            Err(e) => {
                log(&format!(
                    "permit relay: the card message was not posted: {e}"
                ));
                None
            }
        };
        let mut posted = lock(&self.permit_relay.posted);
        if posted.len() >= POSTED_CAP && !posted.contains_key(card_id) {
            if let Some(k) = posted.keys().next().cloned() {
                posted.remove(&k);
            }
        }
        posted.insert(card_id.to_owned(), Posted { msg_id, session });
    }

    fn permit_close(&self, card_id: &str, reason: &str) {
        let reason = clean(reason, REASON_MAX);
        let reply_to = lock(&self.permit_relay.posted)
            .remove(card_id)
            .and_then(|p| p.msg_id);
        let room = self.chat.manager_room();
        if let Err(e) = self.chat.reply_system(
            &room,
            reply_to.as_deref(),
            &format!("Permission card closed: {reason}"),
        ) {
            log(&format!("permit relay: a card reply was not posted: {e}"));
        }
        let msg = self.permit_message(
            format!("Permission card closed: {reason}"),
            DecisionRef {
                card_id: card_id.to_owned(),
                spawn: None,
                permit: None,
                closed: Some(reason),
            },
            reply_to,
        );
        if let Err(e) = self.chat.append(msg) {
            log(&format!("permit relay: the close was not posted: {e}"));
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
struct OkItem {
    device: String,
    fp: String,
    owner_device: bool,
    kind: PermitSenderKind,
    label: String,
    signed: SignedPermit,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) struct PermitOkInbox {
    tx: std::sync::mpsc::SyncSender<OkItem>,
    queued: Arc<Mutex<HashMap<String, usize>>>,
}

fn permit_inbox() -> (PermitOkInbox, std::sync::mpsc::Receiver<OkItem>) {
    let (tx, rx) = std::sync::mpsc::sync_channel::<OkItem>(PERMIT_INBOX_CAP);
    let queued = Arc::new(Mutex::new(HashMap::new()));
    (PermitOkInbox { tx, queued }, rx)
}

impl PermitOkInbox {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn deliver(
        &self,
        from_device: &str,
        from_fp: &str,
        owner_device: bool,
        kind: PermitSenderKind,
        sender_label: &str,
        signed: &SignedPermit,
    ) {
        let mut q = lock(&self.queued);
        let n = q.get(from_device).copied().unwrap_or(0);
        let sent = n < PERMIT_PER_DEVICE
            && self
                .tx
                .try_send(OkItem {
                    device: from_device.to_owned(),
                    fp: from_fp.to_owned(),
                    owner_device,
                    kind,
                    label: sender_label.to_owned(),
                    signed: signed.clone(),
                })
                .is_ok();
        if sent {
            q.insert(from_device.to_owned(), n + 1);
        } else {
            log("permit relay: an answer was dropped, the intake is busy or stopped");
        }
    }
}

impl Runner {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn start_permitok_intake(self: &Arc<Self>) -> Arc<PermitOkInbox> {
        let (inbox, rx) = permit_inbox();
        let queued = inbox.queued.clone();
        let rt = self.clone();
        let spawned = thread::Builder::new()
            .name("permitok-intake".into())
            .spawn(move || {
                while !rt.stop.load(Ordering::SeqCst) {
                    match rx.recv_timeout(PERMIT_INTAKE_TICK) {
                        Ok(item) => {
                            release_queued(&queued, &item.device);
                            rt.accept_permitok(
                                &item.device,
                                &item.fp,
                                item.owner_device,
                                item.kind,
                                &item.label,
                                &item.signed,
                                now_ms(),
                            );
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            });
        if spawned.is_err() {
            log("permit relay: the answer intake did not start");
        }
        Arc::new(inbox)
    }
}

#[cfg(test)]
mod tests {
    use super::manager::tests::{add_meta, runner};
    use super::*;
    use crate::spawnauth::testutil::{fp_of, offer, secret, spawn_fp, USER};
    use crate::spawnauth::{Identity, SpawnAuth, TrustSource};
    use p256::ecdsa::signature::Signer;
    use p256::ecdsa::Signature;
    use tmgr_ipc::chat::ChatMessage;
    use tmgr_spawn_auth::{b64_encode, permit_signed_string, PermitChoice, TrustStatus};

    const SID: &str = "s_aaaaaaaaaa";
    const SCREEN: &str = include_str!("../../profiles/screens/claude-bash-approval.txt");
    const COMMAND: &str = "touch hello.txt; ls -la";

    struct Source(Mutex<TrustStatus>);

    impl TrustSource for Source {
        fn status(&self, _: &str, _: &str) -> TrustStatus {
            *lock(&self.0)
        }
    }

    struct Env {
        _t: tempfile::TempDir,
        rt: Arc<Runner>,
        auth: Option<Arc<SpawnAuth>>,
        fp: String,
        rx: mpsc::Receiver<Frame>,
        s: Arc<Session>,
    }

    fn env(linked: bool) -> Env {
        let t = tempfile::Builder::new()
            .prefix("pr")
            .tempdir_in("/tmp")
            .unwrap();
        let rt = runner(t.path());
        add_meta(
            &rt,
            t.path(),
            SID,
            serde_json::json!({"permission_cards": true}),
        );
        let s = rt.session(SID).unwrap();
        let (tx, rx) = mpsc::sync_channel::<Frame>(SHIM_QUEUE);
        *lock(&s.shim) = Some(ShimLink {
            tx,
            queued_bytes: Arc::new(AtomicUsize::new(0)),
            queued_frames: Arc::new(AtomicUsize::new(0)),
            credit: None,
        });
        s.feed(SCREEN.replace('\n', "\r\n").as_bytes(), true);
        let home = rt.cfg.homes.join(SID);
        fs::create_dir_all(&home).unwrap();
        let hook = serde_json::json!({
            "tool_name": "Bash",
            "tool_input": {"command": COMMAND, "description": "run it"},
            "cwd": "/work/proj",
        })
        .to_string();
        crate::permit_hook::spool(&home, SID, "claude", &mut hook.as_bytes(), now_ms(), 7).unwrap();
        let mut fp = String::new();
        let mut auth = None;
        if linked {
            let identity = Identity::new(USER, rt.chat.machine(), &"b".repeat(64)).unwrap();
            let source = Arc::new(Source(Mutex::new(TrustStatus::Trusted)));
            let a = Arc::new(SpawnAuth::open(&t.path().join("sa"), identity, source).unwrap());
            let o = offer(3, now_ms() - 10_000);
            fp = fp_of(&o);
            a.offer_key(o, now_ms()).unwrap();
            a.approve_key(&fp[..8], &spawn_fp(3)[..16], now_ms())
                .unwrap();
            *lock(&rt.spawn_gate.fixed) = Some(a.clone());
            auth = Some(a);
        }
        Env {
            _t: t,
            rt,
            auth,
            fp,
            rx,
            s,
        }
    }

    fn fg(e: &Env) {
        *lock(&e.s.fg) = Some((2, Instant::now()));
    }

    fn tick(e: &Env) {
        fg(e);
        e.rt.permit_observe(now_ms());
    }

    fn room(e: &Env) -> Vec<ChatMessage> {
        e.rt.chat
            .history(&e.rt.chat.manager_room(), None, Some(100))
            .unwrap()
    }

    fn decisions(e: &Env) -> Vec<ChatMessage> {
        room(e)
            .into_iter()
            .filter(|m| m.decision.is_some())
            .collect()
    }

    fn posted(e: &Env) -> Vec<ChatMessage> {
        decisions(e)
            .into_iter()
            .filter(|m| m.decision.as_ref().is_some_and(|d| d.permit.is_some()))
            .collect()
    }

    fn closed(e: &Env) -> Vec<String> {
        decisions(e)
            .into_iter()
            .filter_map(|m| m.decision.and_then(|d| d.closed))
            .collect()
    }

    fn audits(e: &Env) -> Vec<String> {
        lock(&e.rt.permit_relay.audits).clone()
    }

    fn card_id(e: &Env) -> String {
        posted(e)[0].decision.as_ref().unwrap().card_id.clone()
    }

    fn signed(e: &Env, choice: PermitChoice) -> SignedPermit {
        let id = card_id(e);
        let rec = lock(&e.rt.permit_relay.desk)
            .pending(&id)
            .unwrap()
            .record
            .clone();
        let key = e.auth.as_ref().unwrap().pinned_key(&e.fp).unwrap().unwrap();
        let mut s = SignedPermit {
            v: 1,
            user_id: rec.user_id.clone(),
            device_fp: key.device_fp.clone(),
            runner_id: rec.runner_id.clone(),
            runner_fp: rec.runner_fp.clone(),
            card_id: rec.card_id.clone(),
            session: rec.session.clone(),
            choice,
            confirmed: 0,
            nonce: rec.nonce.clone(),
            ts_ms: now_ms(),
            exp_ms: rec.exp_ms,
            req_sha256: rec.screen.req_sha256(),
            alg: "es256".into(),
            sig: String::new(),
        };
        let sig: Signature = secret(3).sign(permit_signed_string(&rec, &key, &s).as_bytes());
        s.sig = b64_encode(sig.to_der().as_bytes());
        s
    }

    fn accept(e: &Env, s: &SignedPermit, owner: bool) {
        fg(e);
        e.rt.accept_permitok(
            "dev-1",
            &s.device_fp,
            owner,
            PermitSenderKind::Mobile,
            "Mac",
            s,
            now_ms(),
        );
    }

    fn typed(e: &Env) -> String {
        thread::sleep(SUBMIT_DELAY + Duration::from_millis(150));
        let mut out = Vec::new();
        while let Ok(frame) = e.rx.try_recv() {
            if let Frame::Bin(b) = frame {
                out.extend_from_slice(&b.data);
            }
        }
        String::from_utf8(out).unwrap()
    }

    fn armed(e: &Env) {
        tick(e);
        thread::sleep(CARD_ARM + Duration::from_millis(100));
    }

    #[test]
    fn an_answerable_card_posts_one_decision_without_the_command_in_the_body() {
        let e = env(true);
        tick(&e);
        tick(&e);
        let p = posted(&e);
        assert_eq!(p.len(), 1);
        assert!(!p[0].body_md.contains("touch"), "{}", p[0].body_md);
        assert!(!p[0].body_md.contains("hello"));
        let card = p[0].decision.as_ref().unwrap().permit.as_ref().unwrap();
        assert_eq!(card.session, SID);
        assert!(card.text.contains(COMMAND));
        assert_eq!(card.runner_id, e.rt.chat.machine());
        assert_eq!(card.nonce.len(), 32);
    }

    #[test]
    fn without_a_linked_account_nothing_is_posted() {
        let e = env(false);
        tick(&e);
        assert!(decisions(&e).is_empty());
        assert!(lock(&e.rt.permit_relay.desk).is_empty());
    }

    #[test]
    fn a_valid_desktop_answer_sends_the_key_and_closes_the_card_with_one_audit_line() {
        let e = env(true);
        armed(&e);
        let s = signed(&e, PermitChoice::Yes);
        accept(&e, &s, true);
        assert_eq!(typed(&e), "1");
        assert_eq!(closed(&e), vec!["answered on Mac".to_string()]);
        let a = audits(&e);
        assert_eq!(a.len(), 1, "{a:?}");
        assert!(!a[0].contains("touch"));
        tick(&e);
        assert_eq!(posted(&e).len(), 1);
    }

    #[test]
    fn a_non_owner_device_sends_nothing_and_the_card_stays_answerable() {
        let e = env(true);
        armed(&e);
        let s = signed(&e, PermitChoice::Yes);
        accept(&e, &s, false);
        assert_eq!(typed(&e), "");
        assert!(closed(&e).is_empty());
        let again = signed(&e, PermitChoice::Yes);
        accept(&e, &again, true);
        assert_eq!(typed(&e), "1");
    }

    #[test]
    fn a_revoked_device_key_is_dropped_and_the_card_stays_answerable() {
        let e = env(true);
        armed(&e);
        let s = signed(&e, PermitChoice::Yes);
        e.auth.as_ref().unwrap().revoke_key(&e.fp[..8]).unwrap();
        accept(&e, &s, true);
        assert_eq!(typed(&e), "");
        assert!(closed(&e).is_empty());
        assert_eq!(lock(&e.rt.permit_relay.desk).len(), 1);
    }

    #[test]
    fn a_local_attach_answer_closes_the_card_in_attach() {
        let e = env(true);
        armed(&e);
        let id = card_id(&e);
        e.rt.permit_answered_locally(&id);
        assert_eq!(closed(&e), vec!["answered in attach".to_string()]);
        e.rt.permit_answered_locally(&id);
        assert_eq!(closed(&e).len(), 1);
    }

    #[test]
    fn a_card_that_disappears_is_closed_as_gone() {
        let e = env(true);
        tick(&e);
        e.s.feed(b"\x1b[2J\x1b[Hdone\r\n", true);
        tick(&e);
        assert_eq!(closed(&e), vec!["gone".to_string()]);
    }

    #[test]
    fn an_answer_the_session_refuses_closes_the_card_as_gone_with_a_code_only_audit() {
        let e = env(true);
        tick(&e);
        let s = signed(&e, PermitChoice::Yes);
        e.s.feed(b"\x1b[2J\x1b[Hdone\r\n", true);
        accept(&e, &s, true);
        assert_eq!(typed(&e), "");
        assert_eq!(closed(&e), vec!["gone".to_string()]);
        assert!(audits(&e).iter().all(|l| !l.contains("touch")));
    }

    #[test]
    fn a_refused_answer_audits_the_failure_and_never_an_ok_line() {
        let e = env(true);
        tick(&e);
        let s = signed(&e, PermitChoice::Yes);
        accept(&e, &s, true);
        assert_eq!(typed(&e), "");
        assert_eq!(closed(&e), vec!["gone".to_string()]);
        let a = audits(&e);
        assert!(
            a.iter().any(|l| l.contains("result=answer_failed")),
            "{a:?}"
        );
        assert!(
            a.iter().all(|l| !l.contains("result=ok action=answered")),
            "{a:?}"
        );
    }

    #[test]
    fn the_intake_caps_queued_answers_per_device() {
        let e = env(true);
        armed(&e);
        let s = signed(&e, PermitChoice::Yes);
        let (inbox, rx) = permit_inbox();
        for _ in 0..6 {
            inbox.deliver(
                "dev-1",
                &s.device_fp,
                true,
                PermitSenderKind::Mobile,
                "Mac",
                &s,
            );
        }
        assert_eq!(rx.try_iter().count(), 4);
    }

    #[test]
    fn the_intake_thread_hands_an_answer_to_the_desk() {
        let e = env(true);
        armed(&e);
        let s = signed(&e, PermitChoice::Yes);
        fg(&e);
        let inbox = e.rt.start_permitok_intake();
        inbox.deliver(
            "dev-1",
            &s.device_fp,
            true,
            PermitSenderKind::Mobile,
            "Mac",
            &s,
        );
        for _ in 0..50 {
            if !closed(&e).is_empty() {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        e.rt.stop.store(true, Ordering::SeqCst);
        assert_eq!(closed(&e), vec!["answered on Mac".to_string()]);
        assert_eq!(typed(&e), "1");
    }

    #[test]
    fn the_desktop_setting_defaults_to_safe_when_missing_or_invalid() {
        for text in [
            None,
            Some(""),
            Some("other = 1"),
            Some("desktop_may_answer = \"ALL\""),
            Some("desktop_may_answer = \"\""),
            Some("desktop_may_answer = 3"),
            Some("desktop_may_answer = ["),
        ] {
            assert_eq!(desktop_policy_from(text).0, DesktopPolicy::Safe, "{text:?}");
        }
        assert!(desktop_policy_from(None).1.is_none());
        assert!(desktop_policy_from(Some("other = 1")).1.is_none());
        assert!(desktop_policy_from(Some("desktop_may_answer = \"ALL\""))
            .1
            .is_some());
        assert!(desktop_policy_from(Some("desktop_may_answer = ["))
            .1
            .is_some());
        for (v, p) in [
            ("none", DesktopPolicy::None),
            ("safe", DesktopPolicy::Safe),
            ("all", DesktopPolicy::All),
        ] {
            let text = format!("desktop_may_answer = \"{v}\"\n");
            assert_eq!(desktop_policy_from(Some(&text)), (p, None));
        }
    }

    #[test]
    fn the_desktop_setting_is_read_from_the_runner_dir() {
        let t = tempfile::tempdir().unwrap();
        assert_eq!(load_desktop_policy(t.path()).0, DesktopPolicy::Safe);
        std::fs::write(
            t.path().join(PERMIT_CONFIG_FILE),
            "desktop_may_answer = \"none\"\n",
        )
        .unwrap();
        assert_eq!(load_desktop_policy(t.path()).0, DesktopPolicy::None);
        std::fs::write(
            t.path().join(PERMIT_CONFIG_FILE),
            "desktop_may_answer = \"everything\"\n",
        )
        .unwrap();
        let (p, warn) = load_desktop_policy(t.path());
        assert_eq!(p, DesktopPolicy::Safe);
        assert!(warn.is_some());
    }
}
