use super::spawnauth::release_queued;
use super::*;
use crate::permit::remote::{strip_bidi, Action, PermitDesk, RunnerIds};
use crate::spawnauth::SpawnAuth;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use tmgr_chat_relay::engine::PermitOkSink;
use tmgr_ipc::chat::{Author, AuthorKind, ChatMessage, DecisionRef, MessageKind, CHAT_V};
use tmgr_spawn_auth::{
    b64_decode_strict, DesktopPolicy, PermitSender, PermitSenderKind, PinnedKey, SignedPermit,
};

const PERMIT_TICK: Duration = Duration::from_secs(1);
const PERMIT_INBOX_CAP: usize = 16;
const PERMIT_PER_DEVICE: usize = 4;
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

const PERMIT_CONFIG_DEFAULT: &str = "desktop_may_answer = \"safe\"\n";

fn safe_with(warning: String) -> (DesktopPolicy, Option<String>) {
    (DesktopPolicy::Safe, Some(warning))
}

fn create_permit_config(path: &Path) -> (DesktopPolicy, Option<String>) {
    let made = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .and_then(|mut f| {
            f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            f.write_all(PERMIT_CONFIG_DEFAULT.as_bytes())
        });
    match made {
        Ok(()) => (DesktopPolicy::Safe, None),
        Err(_) => safe_with(format!("{PERMIT_CONFIG_FILE} cannot be created")),
    }
}

fn load_desktop_policy(runner_dir: &Path) -> (DesktopPolicy, Option<String>) {
    let path = runner_dir.join(PERMIT_CONFIG_FILE);
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return create_permit_config(&path),
        Err(e) if e.raw_os_error() == Some(libc::ELOOP) => {
            return safe_with(format!("{PERMIT_CONFIG_FILE} is a symlink"))
        }
        Err(_) => return safe_with(format!("{PERMIT_CONFIG_FILE} cannot be opened")),
    };
    let Ok(meta) = file.metadata() else {
        return safe_with(format!("{PERMIT_CONFIG_FILE} cannot be read"));
    };
    if !meta.is_file() {
        return safe_with(format!("{PERMIT_CONFIG_FILE} is not a regular file"));
    }
    if meta.uid() != unsafe { libc::geteuid() } {
        return safe_with(format!(
            "{PERMIT_CONFIG_FILE} is not owned by the runner user"
        ));
    }
    if meta.mode() & 0o022 != 0 {
        return safe_with(format!("{PERMIT_CONFIG_FILE} is group- or world-writable"));
    }
    let mut text = String::new();
    match file.take(PERMIT_CONFIG_MAX).read_to_string(&mut text) {
        Ok(_) => desktop_policy_from(Some(&text)),
        Err(_) => safe_with(format!("{PERMIT_CONFIG_FILE} cannot be read")),
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

fn relay_key(
    user_id: &str,
    device_id: &str,
    device_fp: &str,
    sign_pub_b64: &str,
) -> Option<PinnedKey> {
    Some(PinnedKey {
        user_id: user_id.to_owned(),
        device_id: device_id.to_owned(),
        device_fp: device_fp.to_owned(),
        spawn_pub: b64_decode_strict(sign_pub_b64)?,
    })
}

fn fresh_nonce() -> Option<String> {
    let mut b = [0u8; 16];
    if getrandom::fill(&mut b).is_err() {
        log("permit relay: no random source, the action was skipped");
        return None;
    }
    Some(b.iter().map(|x| format!("{x:02x}")).collect())
}

fn clean(s: &str, max: usize) -> String {
    strip_bidi(s)
        .chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect()
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
        self.permit_sync_policy();
        let actions: Vec<Action> = {
            let mut desk = lock(&self.permit_relay.desk);
            seen.iter()
                .flat_map(|(session, card)| {
                    desk.observe(session, card.as_ref(), &ids, now, &mut fresh_nonce)
                })
                .collect()
        };
        self.permit_execute(actions, "relay:local");
    }

    pub(super) fn permit_answered_locally(&self, card_id: &str) {
        let actions = lock(&self.permit_relay.desk).answered_locally(card_id);
        self.permit_execute(actions, "relay:local");
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn accept_permitok(
        &self,
        from_device: &str,
        from_fp: &str,
        owner_device: bool,
        kind: PermitSenderKind,
        sign_pub_b64: &str,
        sender_label: &str,
        signed: &SignedPermit,
        now: u64,
    ) {
        let Some((auth, ids)) = self.permit_ids() else {
            log("permit relay: an answer was dropped, no account is linked");
            return;
        };
        let key = match kind {
            PermitSenderKind::Desktop => {
                relay_key(&ids.user_id, from_device, from_fp, sign_pub_b64)
            }
            PermitSenderKind::Mobile => auth.pinned_key(&signed.device_fp).ok().flatten(),
        };
        let current = self.permit_card_of(&signed.session);
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
            &mut fresh_nonce,
        );
        let who: String = from_fp
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(12)
            .collect();
        self.permit_execute(actions, &format!("relay:{who}"));
    }

    pub(super) fn accept_permit_refresh(&self, card_id: &str, nonce: &str, now: u64) {
        if self.permit_ids().is_none() {
            return;
        }
        let Some(session) = lock(&self.permit_relay.desk)
            .pending(card_id)
            .map(|p| p.record.session.clone())
        else {
            return;
        };
        let current = self.permit_card_of(&session);
        self.permit_sync_policy();
        let actions = lock(&self.permit_relay.desk).refresh(
            card_id,
            nonce,
            current.as_ref(),
            now,
            &mut fresh_nonce,
        );
        self.permit_execute(actions, "relay:refresh");
    }

    fn permit_execute(&self, actions: Vec<Action>, client: &str) {
        for action in actions {
            match action {
                Action::Post { card_id, card } => self.permit_post(&card_id, *card),
                Action::Close { card_id, reason } => self.permit_close(&card_id, &reason),
                Action::Answer {
                    session,
                    card_id,
                    prompt,
                    choice,
                    confirmed,
                    audit,
                } => {
                    let follow =
                        match self.permit_answer(client, &session, &prompt, choice, confirmed) {
                            Ok(()) => {
                                self.permit_audit(&audit);
                                lock(&self.permit_relay.desk).answered(&card_id)
                            }
                            Err((code, retry)) => {
                                self.permit_audit(&format!(
                                    "permit answer: card={} result=answer_failed code={code}",
                                    clean(&card_id, 64)
                                ));
                                lock(&self.permit_relay.desk).answer_failed(
                                    &card_id,
                                    retry,
                                    now_ms(),
                                    &mut fresh_nonce,
                                )
                            }
                        };
                    self.permit_execute(follow, client);
                }
                Action::Audit(line) => self.permit_audit(&line),
            }
        }
    }

    fn permit_answer(
        &self,
        client: &str,
        session: &str,
        prompt: &str,
        choice: PermissionChoice,
        confirmed: bool,
    ) -> Result<(), (String, bool)> {
        let s = self
            .session(session)
            .filter(|s| !s.is_exited() && s.has_shim())
            .ok_or_else(|| ("no_session".to_owned(), false))?;
        s.answer_card(
            client,
            &PermissionAnswer {
                session: session.to_owned(),
                card: prompt.to_owned(),
                choice,
                confirmed,
            },
        )
        .map_err(|e| {
            let retry = answer_retryable(&e);
            (String::from(e.code), retry)
        })
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
        let desk = lock(&self.permit_relay.desk);
        if !desk.is_current(card_id, &card.nonce) {
            return;
        }
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
        drop(desk);
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

struct OkItem {
    device: String,
    fp: String,
    owner_device: bool,
    kind: PermitSenderKind,
    sign_pub: String,
    label: String,
    body: OkBody,
}

enum OkBody {
    Signed(Box<SignedPermit>),
    Refresh { card_id: String, nonce: String },
}

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
    fn enqueue(
        &self,
        from_device: &str,
        from_fp: &str,
        kind: PermitSenderKind,
        sign_pub_b64: &str,
        sender_label: &str,
        body: OkBody,
    ) {
        let mut q = lock(&self.queued);
        let n = q.get(from_device).copied().unwrap_or(0);
        let sent = n < PERMIT_PER_DEVICE
            && self
                .tx
                .try_send(OkItem {
                    device: from_device.to_owned(),
                    fp: from_fp.to_owned(),
                    owner_device: true,
                    kind,
                    sign_pub: sign_pub_b64.to_owned(),
                    label: sender_label.to_owned(),
                    body,
                })
                .is_ok();
        if sent {
            q.insert(from_device.to_owned(), n + 1);
        } else {
            log("permit relay: an answer was dropped, the intake is busy or stopped");
        }
    }
}

impl PermitOkSink for PermitOkInbox {
    fn deliver_permitok(
        &self,
        from_device: &str,
        from_fp: &str,
        kind: PermitSenderKind,
        sign_pub_b64: &str,
        sender_label: &str,
        signed: &SignedPermit,
    ) {
        self.enqueue(
            from_device,
            from_fp,
            kind,
            sign_pub_b64,
            sender_label,
            OkBody::Signed(Box::new(signed.clone())),
        );
    }

    fn deliver_permit_refresh(
        &self,
        from_device: &str,
        from_fp: &str,
        kind: PermitSenderKind,
        card_id: &str,
        nonce: &str,
    ) {
        self.enqueue(
            from_device,
            from_fp,
            kind,
            "",
            "",
            OkBody::Refresh {
                card_id: card_id.to_owned(),
                nonce: nonce.to_owned(),
            },
        );
    }
}

impl Runner {
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
                            match &item.body {
                                OkBody::Signed(signed) => rt.accept_permitok(
                                    &item.device,
                                    &item.fp,
                                    item.owner_device,
                                    item.kind,
                                    &item.sign_pub,
                                    &item.label,
                                    signed,
                                    now_ms(),
                                ),
                                OkBody::Refresh { card_id, nonce } => {
                                    rt.accept_permit_refresh(card_id, nonce, now_ms())
                                }
                            }
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
    use crate::spawnauth::testutil::{fp_of, offer, pub_b64, secret, spawn_fp, USER};
    use crate::spawnauth::{Identity, SpawnAuth, TrustSource};
    use p256::ecdsa::signature::Signer;
    use p256::ecdsa::Signature;
    use tmgr_ipc::chat::ChatMessage;
    use tmgr_spawn_auth::{b64_encode, permit_signed_string, PermitChoice, PinnedKey, TrustStatus};

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
        env_with(linked, COMMAND)
    }

    fn env_with(linked: bool, command: &str) -> Env {
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
        s.feed(
            SCREEN
                .replace(COMMAND, command)
                .replace('\n', "\r\n")
                .as_bytes(),
            true,
        );
        let home = rt.cfg.homes.join(SID);
        fs::create_dir_all(&home).unwrap();
        let hook = serde_json::json!({
            "tool_name": "Bash",
            "tool_input": {"command": command, "description": "run it"},
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
            "",
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
    fn every_nonce_is_fresh_and_never_constant() {
        let drawn: Vec<String> = (0..6).map(|_| fresh_nonce().unwrap()).collect();
        let distinct: std::collections::HashSet<_> = drawn.iter().collect();
        assert_eq!(distinct.len(), drawn.len(), "{drawn:?}");
        assert!(drawn
            .iter()
            .all(|n| n.len() == 32 && n.bytes().any(|b| b != b'0')));
    }

    #[test]
    fn a_transient_key_refusal_keeps_the_card_answerable_with_a_fresh_nonce() {
        let e = env(true);
        tick(&e);
        let first = signed(&e, PermitChoice::Yes);
        accept(&e, &first, true);
        assert_eq!(typed(&e), "");
        assert!(closed(&e).is_empty(), "{:?}", closed(&e));
        assert_eq!(lock(&e.rt.permit_relay.desk).len(), 1);
        let cards = posted(&e);
        assert_eq!(cards.len(), 2);
        let nonce = |m: &ChatMessage| {
            m.decision
                .as_ref()
                .unwrap()
                .permit
                .as_ref()
                .unwrap()
                .nonce
                .clone()
        };
        assert_ne!(nonce(&cards[0]), nonce(&cards[1]));
        assert!(audits(&e)
            .iter()
            .any(|l| l.contains("result=answer_failed")));
        thread::sleep(CARD_ARM + Duration::from_millis(100));
        let retry = signed(&e, PermitChoice::Yes);
        assert_eq!(retry.nonce, nonce(&cards[1]));
        accept(&e, &retry, true);
        assert_eq!(typed(&e), "1");
        assert_eq!(closed(&e), vec!["answered on Mac".to_string()]);
    }

    #[test]
    fn a_full_input_queue_does_not_burn_the_prompt() {
        let e = env(true);
        armed(&e);
        let full = lock(&e.s.shim).as_ref().unwrap().queued_frames.clone();
        full.store(usize::MAX / 2, Ordering::SeqCst);
        let first = signed(&e, PermitChoice::Yes);
        accept(&e, &first, true);
        assert!(closed(&e).is_empty(), "{:?}", closed(&e));
        full.store(0, Ordering::SeqCst);
        let retry = signed(&e, PermitChoice::Yes);
        accept(&e, &retry, true);
        assert_eq!(typed(&e), "1");
        assert_eq!(closed(&e), vec!["answered on Mac".to_string()]);
    }

    #[test]
    fn a_post_for_a_card_that_is_no_longer_pending_is_not_executed() {
        let e = env(true);
        armed(&e);
        let id = card_id(&e);
        let card = e.s.card();
        let late = lock(&e.rt.permit_relay.desk)
            .pending(&id)
            .unwrap()
            .record
            .exp_ms
            + 1;
        let nonce = card_nonce(&posted(&e)[0]);
        let mut n = 0u32;
        let stale =
            lock(&e.rt.permit_relay.desk).refresh(&id, &nonce, card.as_ref(), late, &mut || {
                n += 1;
                Some(format!("{n:032x}"))
            });
        assert!(
            matches!(stale.as_slice(), [Action::Post { .. }]),
            "{stale:?}"
        );
        let s = signed(&e, PermitChoice::Yes);
        accept(&e, &s, true);
        assert_eq!(closed(&e), vec!["answered on Mac".to_string()]);
        e.rt.permit_execute(stale, "relay:test");
        assert_eq!(posted(&e).len(), 1);
        assert!(lock(&e.rt.permit_relay.posted).is_empty());
        tick(&e);
        assert_eq!(posted(&e).len(), 1);
    }

    #[test]
    fn clean_strips_bidi_and_control_characters() {
        let s = "a\u{202e}b\u{2066}c\u{200f}d\u{061c}e\nf";
        assert_eq!(clean(s, 64), "abcdef");
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
        assert!(closed(&e).is_empty());
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

    fn card_nonce(m: &ChatMessage) -> String {
        m.decision
            .as_ref()
            .unwrap()
            .permit
            .as_ref()
            .unwrap()
            .nonce
            .clone()
    }

    #[test]
    fn a_request_open_for_ten_expiry_periods_leaves_one_card_in_the_room() {
        let e = env(true);
        armed(&e);
        let t0 = now_ms();
        for k in 1..=10u64 {
            fg(&e);
            e.rt.permit_observe(t0 + k * (tmgr_spawn_auth::MAX_PERMIT_TTL_MS + 1));
        }
        assert_eq!(posted(&e).len(), 1);
    }

    #[test]
    fn a_refresh_reposts_an_expired_card_once_with_the_same_id() {
        let e = env(true);
        armed(&e);
        let id = card_id(&e);
        let first = card_nonce(&posted(&e)[0]);
        let late = now_ms() + tmgr_spawn_auth::MAX_PERMIT_TTL_MS + 1;
        fg(&e);
        e.rt.accept_permit_refresh(&id, &first, late - 10_000);
        assert_eq!(posted(&e).len(), 1);
        e.rt.accept_permit_refresh(&id, &first, late);
        e.rt.accept_permit_refresh(&id, &first, late + 1);
        let cards = posted(&e);
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[1].decision.as_ref().unwrap().card_id, id);
        assert_ne!(card_nonce(&cards[1]), first);
        e.rt.accept_permit_refresh("unknown", &first, late + 2);
        assert_eq!(posted(&e).len(), 2);
    }

    #[test]
    fn the_intake_queues_a_refresh_under_the_same_per_device_cap() {
        let (inbox, rx) = permit_inbox();
        for _ in 0..6 {
            inbox.deliver_permit_refresh("dev-1", "fp", PermitSenderKind::Desktop, "card", "n");
        }
        assert_eq!(rx.try_iter().count(), 4);
    }

    #[test]
    fn the_intake_caps_queued_answers_per_device() {
        let e = env(true);
        armed(&e);
        let s = signed(&e, PermitChoice::Yes);
        let (inbox, rx) = permit_inbox();
        for _ in 0..6 {
            inbox.deliver_permitok(
                "dev-1",
                &s.device_fp,
                PermitSenderKind::Mobile,
                "",
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
        inbox.deliver_permitok(
            "dev-1",
            &s.device_fp,
            PermitSenderKind::Mobile,
            "",
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

    fn desktop_fp() -> String {
        "dd".repeat(32)
    }

    fn resign(e: &Env, mut s: SignedPermit, k: &p256::ecdsa::SigningKey, fp: &str) -> SignedPermit {
        let rec = lock(&e.rt.permit_relay.desk)
            .pending(&card_id(e))
            .unwrap()
            .record
            .clone();
        s.device_fp = fp.to_owned();
        let key = PinnedKey {
            user_id: rec.user_id.clone(),
            device_id: "desk-1".into(),
            device_fp: fp.to_owned(),
            spawn_pub: Vec::new(),
        };
        let sig: Signature = k.sign(permit_signed_string(&rec, &key, &s).as_bytes());
        s.sig = b64_encode(sig.to_der().as_bytes());
        s
    }

    fn accept_as(e: &Env, s: &SignedPermit, kind: PermitSenderKind, sign_pub: &str) {
        fg(e);
        e.rt.accept_permitok(
            "desk-1",
            &s.device_fp,
            true,
            kind,
            sign_pub,
            "Mac",
            s,
            now_ms(),
        );
    }

    fn refused(e: &Env) {
        assert_eq!(typed(e), "");
        assert!(closed(e).iter().all(|c| !c.starts_with("answered")));
        assert!(audits(e).iter().all(|l| !l.contains("result=ok")));
    }

    fn kept(e: &Env) {
        refused(e);
        assert!(closed(e).is_empty());
        assert_eq!(lock(&e.rt.permit_relay.desk).len(), 1);
    }

    #[test]
    fn a_desktop_answer_signed_with_the_relay_key_is_verified_and_answered() {
        let e = env(true);
        armed(&e);
        let s = resign(&e, signed(&e, PermitChoice::Yes), &secret(9), &desktop_fp());
        accept_as(&e, &s, PermitSenderKind::Desktop, &pub_b64(&secret(9)));
        assert_eq!(typed(&e), "1");
        assert_eq!(closed(&e), vec!["answered on Mac".to_string()]);
    }

    #[test]
    fn a_desktop_answer_signed_with_the_spawn_key_pinned_for_the_same_fp_is_refused() {
        let e = env(true);
        armed(&e);
        let s = signed(&e, PermitChoice::Yes);
        assert_eq!(s.device_fp, e.fp);
        accept_as(&e, &s, PermitSenderKind::Desktop, &pub_b64(&secret(9)));
        refused(&e);
    }

    #[test]
    fn a_desktop_answer_with_the_relay_key_works_even_when_a_spawn_key_is_pinned_for_that_fp() {
        let e = env(true);
        armed(&e);
        let s = resign(&e, signed(&e, PermitChoice::Yes), &secret(9), &e.fp);
        accept_as(&e, &s, PermitSenderKind::Desktop, &pub_b64(&secret(9)));
        assert_eq!(typed(&e), "1");
        assert_eq!(closed(&e), vec!["answered on Mac".to_string()]);
    }

    #[test]
    fn a_desktop_answer_with_an_unusable_relay_key_is_refused() {
        let e = env(true);
        armed(&e);
        let s = resign(&e, signed(&e, PermitChoice::Yes), &secret(9), &desktop_fp());
        for bad in ["", "!!", "AAAA"] {
            accept_as(&e, &s, PermitSenderKind::Desktop, bad);
            kept(&e);
        }
    }

    #[test]
    fn a_mobile_answer_signed_with_its_relay_key_is_refused() {
        let e = env(true);
        armed(&e);
        let s = resign(&e, signed(&e, PermitChoice::Yes), &secret(9), &e.fp);
        accept_as(&e, &s, PermitSenderKind::Mobile, &pub_b64(&secret(9)));
        refused(&e);
    }

    #[test]
    fn a_mobile_answer_signed_with_the_spawn_key_is_accepted_whatever_the_relay_key() {
        let e = env(true);
        armed(&e);
        let s = signed(&e, PermitChoice::Yes);
        accept_as(&e, &s, PermitSenderKind::Mobile, &pub_b64(&secret(9)));
        assert_eq!(typed(&e), "1");
        assert_eq!(closed(&e), vec!["answered on Mac".to_string()]);
    }

    #[test]
    fn a_desktop_answer_to_a_danger_card_through_the_sink_is_refused_under_the_default_policy() {
        let e = env_with(true, "rm -rf build");
        armed(&e);
        let card = posted(&e).remove(0).decision.unwrap().permit.unwrap();
        assert!(!card.danger.is_empty());
        let s = resign(&e, signed(&e, PermitChoice::Yes), &secret(9), &desktop_fp());
        fg(&e);
        let inbox = e.rt.start_permitok_intake();
        inbox.deliver_permitok(
            "desk-1",
            &s.device_fp,
            PermitSenderKind::Desktop,
            &pub_b64(&secret(9)),
            "Mac",
            &s,
        );
        for _ in 0..50 {
            if !audits(&e).is_empty() {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        e.rt.stop.store(true, Ordering::SeqCst);
        let a = audits(&e);
        assert!(a.iter().any(|l| l.contains("desktop_not_allowed")), "{a:?}");
        refused(&e);
    }

    #[test]
    fn a_desktop_answer_to_a_safe_card_through_the_sink_is_answered_under_the_default_policy() {
        let e = env(true);
        armed(&e);
        let s = resign(&e, signed(&e, PermitChoice::Yes), &secret(9), &desktop_fp());
        fg(&e);
        let inbox = e.rt.start_permitok_intake();
        inbox.deliver_permitok(
            "desk-1",
            &s.device_fp,
            PermitSenderKind::Desktop,
            &pub_b64(&secret(9)),
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

    fn private_dir() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        std::fs::set_permissions(t.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        t
    }

    fn put(dir: &Path, text: &str, mode: u32) -> PathBuf {
        let p = dir.join(PERMIT_CONFIG_FILE);
        std::fs::write(&p, text).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
        p
    }

    #[test]
    fn a_symlinked_permit_file_gives_safe_and_a_warning() {
        let t = private_dir();
        let real = t.path().join("real.toml");
        std::fs::write(&real, "desktop_may_answer = \"all\"\n").unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::os::unix::fs::symlink(&real, t.path().join(PERMIT_CONFIG_FILE)).unwrap();
        let (p, warn) = load_desktop_policy(t.path());
        assert_eq!(p, DesktopPolicy::Safe);
        assert!(warn.is_some());
    }

    #[test]
    fn a_group_or_world_writable_permit_file_gives_safe_and_a_warning() {
        for mode in [0o620, 0o602, 0o666] {
            let t = private_dir();
            put(t.path(), "desktop_may_answer = \"all\"\n", mode);
            let (p, warn) = load_desktop_policy(t.path());
            assert_eq!(p, DesktopPolicy::Safe, "{mode:o}");
            assert!(warn.is_some(), "{mode:o}");
        }
    }

    #[test]
    fn an_owner_only_permit_file_is_honoured() {
        let t = private_dir();
        put(t.path(), "desktop_may_answer = \"all\"\n", 0o600);
        assert_eq!(load_desktop_policy(t.path()), (DesktopPolicy::All, None));
    }

    #[test]
    fn a_missing_permit_file_is_created_owner_only_with_safe() {
        let t = private_dir();
        assert_eq!(load_desktop_policy(t.path()), (DesktopPolicy::Safe, None));
        let p = t.path().join(PERMIT_CONFIG_FILE);
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "desktop_may_answer = \"safe\"\n"
        );
        let mode = std::fs::metadata(&p).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_permit_file_is_created_owner_only_whatever_the_umask() {
        let t = private_dir();
        let old = unsafe { libc::umask(0) };
        let (p, warn) = load_desktop_policy(t.path());
        unsafe { libc::umask(old) };
        assert_eq!((p, warn), (DesktopPolicy::Safe, None));
        let mode = std::fs::metadata(t.path().join(PERMIT_CONFIG_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn an_existing_permit_file_is_never_rewritten() {
        let t = private_dir();
        let p = put(t.path(), "desktop_may_answer = \"none\"\n", 0o600);
        load_desktop_policy(t.path());
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "desktop_may_answer = \"none\"\n"
        );
        let p = put(t.path(), "desktop_may_answer = \"all\"\n", 0o644);
        load_desktop_policy(t.path());
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "desktop_may_answer = \"all\"\n"
        );
    }

    #[test]
    fn a_permit_path_that_is_not_a_regular_file_gives_safe_and_a_warning() {
        let t = private_dir();
        std::fs::create_dir(t.path().join(PERMIT_CONFIG_FILE)).unwrap();
        let (p, warn) = load_desktop_policy(t.path());
        assert_eq!(p, DesktopPolicy::Safe);
        assert!(warn.is_some());
    }
}
