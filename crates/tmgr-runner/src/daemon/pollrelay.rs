use super::permitrelay::clean;
use super::spawnauth::release_queued;
use super::*;
use crate::poll::desk::{self, Action, Outcome, PollDesk, Sent, Terminal};
use crate::poll::{parse_screen, Screen};
use std::collections::VecDeque;
use tmgr_chat_relay::engine::PollSink;
use tmgr_ipc::chat::{
    limits, Author, AuthorKind, ChatMessage, MessageKind, PollAnswer, PollMsg, CHAT_V,
};
use tmgr_spawn_auth::PermitSenderKind;

const POLL_TICK: Duration = Duration::from_millis(500);
const POLL_INBOX_CAP: usize = 16;
const POLL_PER_DEVICE: usize = 4;
const POLL_INTAKE_TICK: Duration = Duration::from_millis(500);
const POSTED_CAP: usize = 64;
const LINE_MAX: usize = 700;
const WHY_MAX: usize = 300;
const PENDING_CAP: usize = 64;
const PENDING_GIVE_UP: Duration = Duration::from_secs(300);
const BUSY_CAP: usize = 64;
const BUSY_WHY: &str = "the runner was busy, answer in terminal";

#[derive(Default)]
pub(super) struct PollRelay {
    desk: Mutex<PollDesk>,
    posted: Mutex<HashMap<String, String>>,
    last_tick: Mutex<Option<Instant>>,
    pending: Mutex<VecDeque<PendingClose>>,
    busy: Arc<Mutex<VecDeque<String>>>,
    #[cfg(test)]
    fail_appends: AtomicUsize,
}

struct PendingClose {
    card_id: String,
    msg: ChatMessage,
    since: Instant,
    next: Instant,
}

impl Session {
    fn poll_claude(&self) -> bool {
        lock(&self.inner)
            .meta
            .profile
            .as_deref()
            .and_then(permit::agent_of)
            == Some(permit::Agent::Claude)
    }

    fn poll_screen(&self) -> Option<Screen> {
        let rows = lock(&self.inner).screen.text_lines(None).3;
        parse_screen(&rows)
    }

    fn poll_send(&self, keys: &[u8], check: &dyn Fn(Option<&Screen>) -> bool) -> Sent {
        let accepted = {
            let i = &mut *lock(&self.inner);
            let rows = i.screen.text_lines(None).3;
            if !check(parse_screen(&rows).as_ref()) {
                return Sent::Changed;
            }
            self.shim_input(keys)
        };
        if !accepted {
            return Sent::Refused;
        }
        self.change(|i, now, out| out.extend(i.det.on_input(now)));
        Sent::Ok
    }

    fn poll_ready(&self) -> bool {
        !self.is_exited() && self.poll_claude() && self.has_shim() && self.foreground_is_agent()
    }
}

struct SessionTerm<'a> {
    rt: &'a Runner,
    session: &'a Session,
    tool_use_id: &'a str,
}

impl Terminal for SessionTerm<'_> {
    fn screen(&self) -> Option<Screen> {
        self.session.poll_screen()
    }

    fn send_checked(&self, keys: &[u8], check: &dyn Fn(Option<&Screen>) -> bool) -> Sent {
        self.session.poll_send(keys, check)
    }

    fn allowed(&self) -> bool {
        self.rt.chat.claude_read()
    }

    fn poll_open(&self) -> bool {
        self.rt
            .chat
            .asking_poll(&self.session.id)
            .is_some_and(|p| p.tool_use_id == self.tool_use_id)
    }
}

fn question_body(session: &str, first: bool, q: &tmgr_ipc::chat::PollQuestion) -> String {
    let head = if first {
        format!("Question in {session}: ")
    } else {
        String::new()
    };
    let mut lines = vec![clean(&format!("{head}{}", q.question), LINE_MAX)];
    for (i, o) in q.options.iter().enumerate() {
        let badge = if o.recommended { " (Recommended)" } else { "" };
        lines.push(clean(&format!("{}. {}{badge}", i + 1, o.label), LINE_MAX));
    }
    lines.join("\n")
}

fn poll_body(session: &str, poll: &PollMsg) -> String {
    let body = poll
        .questions
        .iter()
        .enumerate()
        .map(|(i, q)| question_body(session, i == 0, q))
        .collect::<Vec<_>>()
        .join("\n");
    body.chars()
        .scan(0, |n, c| {
            *n += c.len_utf8();
            (*n <= limits::MAX_BODY).then_some(c)
        })
        .collect()
}

fn close_body(session: &str, closed: &str, picked: Option<&str>) -> String {
    match picked {
        Some(p) if closed == "applied" => format!("Answered in {session}: {}", clean(p, WHY_MAX)),
        _ => format!("Poll closed in {session} ({})", clean(closed, WHY_MAX)),
    }
}

impl Runner {
    pub(super) fn poll_tick(&self) {
        {
            let mut last = lock(&self.poll_relay.last_tick);
            if last.is_some_and(|t| t.elapsed() < POLL_TICK) {
                return;
            }
            *last = Some(Instant::now());
        }
        self.poll_flush();
        self.poll_observe();
    }

    fn poll_flush(&self) {
        let busy: Vec<String> = lock(&self.poll_relay.busy).drain(..).collect();
        let actions: Vec<Action> = {
            let mut desk = lock(&self.poll_relay.desk);
            busy.iter()
                .filter_map(|id| desk.fail_open(id, BUSY_WHY))
                .collect()
        };
        self.poll_execute(actions);
        self.poll_retry();
    }

    fn poll_retry(&self) {
        let now = Instant::now();
        let due: Vec<PendingClose> = {
            let mut q = lock(&self.poll_relay.pending);
            let (due, keep): (Vec<_>, Vec<_>) = q.drain(..).partition(|p| p.next <= now);
            q.extend(keep);
            due
        };
        for mut p in due {
            match self.poll_store(p.msg.clone()) {
                Ok(()) => {
                    lock(&self.poll_relay.posted).remove(&p.card_id);
                }
                Err(e) if p.since.elapsed() >= PENDING_GIVE_UP => {
                    log(&format!("poll relay: a close was given up: {e}"));
                }
                Err(_) => {
                    p.next = now + POLL_TICK;
                    lock(&self.poll_relay.pending).push_back(p);
                }
            }
        }
    }

    fn poll_store(&self, msg: ChatMessage) -> Result<(), String> {
        #[cfg(test)]
        if self.poll_relay.fail_appends.load(Ordering::SeqCst) > 0 {
            self.poll_relay.fail_appends.fetch_sub(1, Ordering::SeqCst);
            return Err("store unavailable".into());
        }
        self.chat.append(msg).map(|_| ())
    }

    fn poll_observe(&self) {
        let tracked = lock(&self.poll_relay.desk).sessions();
        let sessions = self.all();
        let mut seen = Vec::new();
        for s in sessions.iter().filter(|s| !s.is_exited()) {
            let claude = s.poll_claude();
            if !claude && !tracked.contains(&s.id) {
                continue;
            }
            let poll = claude.then(|| self.chat.asking_poll(&s.id)).flatten();
            if poll.is_none() && !tracked.contains(&s.id) {
                continue;
            }
            let screen = s.poll_screen();
            seen.push((
                s.id.clone(),
                poll,
                screen,
                s.has_shim() && s.foreground_is_agent(),
            ));
        }
        let actions: Vec<Action> = {
            let mut desk = lock(&self.poll_relay.desk);
            let mut out: Vec<Action> = tracked
                .iter()
                .filter(|id| !sessions.iter().any(|s| s.id == **id && !s.is_exited()))
                .flat_map(|id| desk.gone(id))
                .collect();
            for (id, poll, screen, fg) in &seen {
                out.extend(desk.observe(id, poll.as_ref(), screen.as_ref(), *fg));
            }
            out
        };
        self.poll_execute(actions);
    }

    fn poll_execute(&self, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Post { card_id, poll } => self.poll_post(&card_id, poll),
                Action::Close {
                    card_id,
                    session,
                    closed,
                    picked,
                    why,
                } => self.poll_close(&card_id, &session, closed, picked, why),
            }
        }
    }

    fn poll_message(
        &self,
        body_md: String,
        poll: PollMsg,
        reply_to: Option<String>,
    ) -> ChatMessage {
        ChatMessage {
            poll: Some(poll),
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
            kind: MessageKind::Text,
            body_md,
            attachments: Vec::new(),
            reply_to,
            decision: None,
            session_meta: None,
            user_turn: false,
            mention: None,
        }
    }

    fn poll_post(&self, card_id: &str, poll: PollMsg) {
        let body = poll_body(&poll.session, &poll);
        let msg = self.poll_message(body, poll, None);
        match self.chat.append(msg) {
            Ok(m) => {
                let mut posted = lock(&self.poll_relay.posted);
                if posted.len() >= POSTED_CAP && !posted.contains_key(card_id) {
                    if let Some(k) = posted.keys().next().cloned() {
                        posted.remove(&k);
                    }
                }
                posted.insert(card_id.to_owned(), m.id);
            }
            Err(e) => log(&format!("poll relay: the card was not posted: {e}")),
        }
    }

    fn poll_close(
        &self,
        card_id: &str,
        session: &str,
        closed: &str,
        picked: Option<String>,
        why: Option<String>,
    ) {
        let reply_to = lock(&self.poll_relay.posted).get(card_id).cloned();
        let picked = picked.map(|p| clean(&p, WHY_MAX));
        let body = close_body(session, closed, picked.as_deref());
        let poll = PollMsg {
            card_id: card_id.to_owned(),
            session: session.to_owned(),
            questions: Vec::new(),
            closed: Some(closed.to_owned()),
            picked,
            can_answer: true,
            why_not: why.map(|w| clean(&w, WHY_MAX)),
        };
        let msg = self.poll_message(body, poll, reply_to);
        match self.poll_store(msg.clone()) {
            Ok(()) => {
                lock(&self.poll_relay.posted).remove(card_id);
            }
            Err(e) => {
                log(&format!("poll relay: the close will be retried: {e}"));
                let mut q = lock(&self.poll_relay.pending);
                if q.len() >= PENDING_CAP {
                    q.pop_front();
                    log("poll relay: the oldest pending close was dropped");
                }
                let now = Instant::now();
                q.push_back(PendingClose {
                    card_id: card_id.to_owned(),
                    msg,
                    since: now,
                    next: now + POLL_TICK,
                });
            }
        }
    }

    fn process_poll_item(&self, item: &PollItem) {
        let Some((auth, _)) = self.permit_ids() else {
            log("poll relay: an answer was dropped, no account is linked");
            return;
        };
        if auth.permit_sender_kind(&item.device, &item.fp).is_none() {
            log("poll relay: a queued answer was dropped, its sender is no longer trusted");
            return;
        }
        let begun = lock(&self.poll_relay.desk).begin_answer(
            &item.answer.card_id,
            &item.answer,
            &item.device,
        );
        let ans = match begun {
            Ok(a) => a,
            Err(r) => {
                log(&format!("poll relay: an answer was ignored: {r:?}"));
                return;
            }
        };
        let outcome = match self.session(&ans.session).filter(|s| s.poll_ready()) {
            None => Outcome::Failed("the session is not ready for an answer".into()),
            Some(_) if !self.chat.claude_read() => {
                Outcome::Failed("reading Claude transcripts is off".into())
            }
            Some(s) => {
                self.capture_fast();
                let who: String = item
                    .fp
                    .chars()
                    .filter(char::is_ascii_alphanumeric)
                    .take(12)
                    .collect();
                s.note_input_from(&format!("relay:{who}"));
                desk::run_answer(
                    &SessionTerm {
                        rt: self,
                        session: &s,
                        tool_use_id: &ans.tool_use_id,
                    },
                    &ans,
                )
            }
        };
        let close = lock(&self.poll_relay.desk).finish(&ans.card_id, outcome);
        self.poll_execute(close.into_iter().collect());
    }

    pub(super) fn start_poll_intake(self: &Arc<Self>) -> Arc<PollInbox> {
        let (tx, rx) = std::sync::mpsc::sync_channel::<PollItem>(POLL_INBOX_CAP);
        let queued = Arc::new(Mutex::new(HashMap::new()));
        let inbox = PollInbox {
            tx,
            queued: queued.clone(),
            busy: self.poll_relay.busy.clone(),
        };
        let rt = self.clone();
        let spawned = thread::Builder::new()
            .name("poll-intake".into())
            .spawn(move || {
                while !rt.stop.load(Ordering::SeqCst) {
                    match rx.recv_timeout(POLL_INTAKE_TICK) {
                        Ok(item) => {
                            release_queued(&queued, &item.device);
                            rt.process_poll_item(&item);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            });
        if spawned.is_err() {
            log("poll relay: the answer intake did not start");
        }
        Arc::new(inbox)
    }
}

struct PollItem {
    device: String,
    fp: String,
    answer: PollAnswer,
}

pub(super) struct PollInbox {
    tx: SyncSender<PollItem>,
    queued: Arc<Mutex<HashMap<String, usize>>>,
    busy: Arc<Mutex<VecDeque<String>>>,
}

impl PollSink for PollInbox {
    fn deliver_poll(
        &self,
        from_device: &str,
        from_fp: &str,
        _kind: PermitSenderKind,
        answer: &PollAnswer,
    ) {
        let mut q = lock(&self.queued);
        let n = q.get(from_device).copied().unwrap_or(0);
        let sent = n < POLL_PER_DEVICE
            && self
                .tx
                .try_send(PollItem {
                    device: from_device.to_owned(),
                    fp: from_fp.to_owned(),
                    answer: answer.clone(),
                })
                .is_ok();
        if sent {
            q.insert(from_device.to_owned(), n + 1);
        } else {
            log("poll relay: an answer was refused, the intake is busy or stopped");
            let mut busy = lock(&self.busy);
            if busy.len() < BUSY_CAP {
                busy.push_back(answer.card_id.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::manager::tests::{add_meta, runner};
    use super::*;
    use crate::attach::status::AskQuestion;
    use crate::chat::tail::capture_consenting;
    use crate::poll::sim::{order_q, pick, scope_q, sources_q, Sim};
    use crate::spawnauth::testutil::USER;
    use crate::spawnauth::{Identity, SpawnAuth, TrustSource};
    use std::io::Write as _;
    use tmgr_ipc::proto::ExitInfo;
    use tmgr_spawn_auth::TrustStatus;

    const SID: &str = "s_aaaaaaaaaa";
    const TOOL: &str = "q1";

    struct Trust(AtomicBool);

    impl TrustSource for Trust {
        fn status(&self, _: &str, _: &str) -> TrustStatus {
            TrustStatus::Trusted
        }

        fn permit_sender_kind(&self, _: &str, _: &str) -> Option<PermitSenderKind> {
            self.0
                .load(Ordering::SeqCst)
                .then_some(PermitSenderKind::Mobile)
        }
    }

    struct Env {
        _t: tempfile::TempDir,
        rt: Arc<Runner>,
        s: Arc<Session>,
        rx: Option<mpsc::Receiver<Frame>>,
        log: PathBuf,
        consent: Arc<AtomicBool>,
        trust: Arc<Trust>,
    }

    fn env() -> Env {
        let t = tempfile::Builder::new()
            .prefix("pl")
            .tempdir_in("/tmp")
            .unwrap();
        let rt = runner(t.path());
        add_meta(&rt, t.path(), SID, serde_json::json!({"rows": 30}));
        let s = rt.session(SID).unwrap();
        let (tx, rx) = mpsc::sync_channel::<Frame>(SHIM_QUEUE);
        *lock(&s.shim) = Some(ShimLink {
            tx,
            queued_bytes: Arc::new(AtomicUsize::new(0)),
            queued_frames: Arc::new(AtomicUsize::new(0)),
            credit: None,
        });
        let trust = Arc::new(Trust(AtomicBool::new(true)));
        let identity = Identity::new(USER, rt.chat.machine(), &"b".repeat(64)).unwrap();
        let auth =
            Arc::new(SpawnAuth::open(&t.path().join("sa"), identity, trust.clone()).unwrap());
        *lock(&rt.spawn_gate.fixed) = Some(auth);
        let log = t.path().join("log.jsonl");
        let e = Env {
            _t: t,
            rt,
            s,
            rx: Some(rx),
            log,
            consent: Arc::new(AtomicBool::new(true)),
            trust,
        };
        append(
            &e.log,
            r#"{"type":"user","message":{"role":"user","content":"go"}}"#,
        );
        capture(&e);
        e
    }

    fn append(p: &Path, line: &str) {
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
            .unwrap();
        writeln!(f, "{line}").unwrap();
    }

    fn capture(e: &Env) {
        capture_now(&e.rt, &e.log, &e.consent);
    }

    fn capture_now(rt: &Runner, log: &Path, consent: &AtomicBool) {
        let infos: Vec<_> = rt.all().iter().map(|s| s.info()).collect();
        let log = log.to_path_buf();
        capture_consenting(&rt.chat, &infos, &|_, _| Some(log.clone()), &|_| {
            consent.load(Ordering::SeqCst)
        });
    }

    fn ask_line(qs: &[AskQuestion]) -> String {
        let questions: Vec<_> = qs
            .iter()
            .map(|q| {
                serde_json::json!({
                    "question": q.question, "header": q.header, "multiSelect": q.multi_select,
                    "options": q.options.iter().map(|o| serde_json::json!({"label": o.label, "description": o.description})).collect::<Vec<_>>(),
                })
            })
            .collect();
        serde_json::json!({"type": "assistant", "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": TOOL, "name": "AskUserQuestion", "input": {"questions": questions}}]}})
        .to_string()
    }

    fn result_line() -> String {
        format!(
            r#"{{"type":"user","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"{TOOL}","content":"ok"}}]}}}}"#
        )
    }

    fn show(s: &Session, rows: &[String]) {
        let mut out = b"\x1b[2J\x1b[H".to_vec();
        out.extend_from_slice(rows.join("\r\n").as_bytes());
        s.feed(&out, true);
    }

    fn fg(e: &Env) {
        *lock(&e.s.fg) = Some((2, Instant::now()));
    }

    fn ask(e: &Env, qs: Vec<AskQuestion>) -> Arc<Mutex<Sim>> {
        let sim = Arc::new(Mutex::new(Sim::new(qs.clone(), 100)));
        show(&e.s, &lock(&sim).render());
        append(&e.log, &ask_line(&qs));
        capture(e);
        sim
    }

    fn tick(e: &Env) {
        fg(e);
        e.rt.poll_observe();
    }

    fn polls(e: &Env) -> Vec<PollMsg> {
        e.rt.chat
            .history(&e.rt.chat.manager_room(), None, Some(100))
            .unwrap()
            .into_iter()
            .filter_map(|m| m.poll)
            .collect()
    }

    fn cards(e: &Env) -> Vec<PollMsg> {
        polls(e)
            .into_iter()
            .filter(|p| p.closed.is_none())
            .collect()
    }

    fn closes(e: &Env) -> Vec<PollMsg> {
        polls(e)
            .into_iter()
            .filter(|p| p.closed.is_some())
            .collect()
    }

    fn answer(e: &Env, picks: Vec<tmgr_ipc::chat::PollPick>) -> PollItem {
        PollItem {
            device: "phone-1".into(),
            fp: "ab".repeat(32),
            answer: PollAnswer {
                v: 1,
                card_id: cards(e)[0].card_id.clone(),
                answers: picks,
            },
        }
    }

    fn responder(e: &mut Env, sim: Arc<Mutex<Sim>>) -> Arc<AtomicBool> {
        let stop = Arc::new(AtomicBool::new(false));
        let rx = e.rx.take().unwrap();
        let (s, rt, log, consent) = (e.s.clone(), e.rt.clone(), e.log.clone(), e.consent.clone());
        let flag = stop.clone();
        thread::spawn(move || {
            while !flag.load(Ordering::SeqCst) {
                let Ok(Frame::Bin(b)) = rx.recv_timeout(Duration::from_millis(10)) else {
                    continue;
                };
                let (rows, gone) = {
                    let mut sim = lock(&sim);
                    sim.press(&b.data);
                    (sim.render(), sim.gone)
                };
                show(&s, &rows);
                if gone {
                    append(&log, &result_line());
                    capture_now(&rt, &log, &consent);
                }
            }
        });
        stop
    }

    #[test]
    fn a_card_is_posted_once_with_can_answer() {
        let e = env();
        ask(&e, vec![scope_q()]);
        tick(&e);
        tick(&e);
        let cards = cards(&e);
        assert_eq!(cards.len(), 1);
        assert!(cards[0].can_answer && cards[0].why_not.is_none());
        assert_eq!(cards[0].session, SID);
        let msgs =
            e.rt.chat
                .history(&e.rt.chat.manager_room(), None, Some(10))
                .unwrap();
        assert_eq!(msgs[0].kind, MessageKind::Text);
        assert!(msgs[0].decision.is_none());
        assert!(msgs[0]
            .body_md
            .starts_with(&format!("Question in {SID}: Which scope")));
        assert!(msgs[0]
            .body_md
            .contains("\n1. Jira+GitLab+Confluence (Recommended)"));
    }

    #[test]
    fn the_card_waits_for_the_foreground_then_becomes_answerable() {
        let e = env();
        ask(&e, vec![scope_q()]);
        e.rt.poll_observe();
        assert!(!cards(&e)[0].can_answer);
        assert_eq!(
            cards(&e)[0].why_not.as_deref(),
            Some("open the pane to answer")
        );
        tick(&e);
        assert_eq!(cards(&e).len(), 2);
        assert!(cards(&e)[1].can_answer);
    }

    #[test]
    fn without_reading_consent_nothing_is_posted() {
        let e = env();
        e.consent.store(false, Ordering::SeqCst);
        ask(&e, vec![scope_q()]);
        tick(&e);
        assert!(polls(&e).is_empty());
        assert!(e.rt.chat.asking_poll(SID).is_none());
    }

    #[test]
    fn an_answer_is_typed_and_acked_once() {
        let mut e = env();
        let sim = ask(&e, vec![sources_q(), order_q()]);
        tick(&e);
        let stop = responder(&mut e, sim.clone());
        let item = answer(&e, vec![pick(0, &[0, 2], None), pick(1, &[1], None)]);
        fg(&e);
        e.rt.process_poll_item(&item);
        e.rt.process_poll_item(&item);
        tick(&e);
        let keys = lock(&sim).keys.len();
        e.rt.process_poll_item(&item);
        stop.store(true, Ordering::SeqCst);
        let acks = closes(&e);
        assert_eq!(acks.len(), 1);
        assert_eq!(acks[0].closed.as_deref(), Some("applied"));
        assert_eq!(acks[0].picked.as_deref(), Some("Jira, Confluence; GitLab"));
        assert_eq!(acks[0].card_id, cards(&e)[0].card_id);
        let sim = lock(&sim);
        assert_eq!(sim.answers, ["Jira, Confluence", "GitLab"]);
        assert!(sim.submitted);
        assert_eq!(sim.keys.len(), keys);
    }

    #[test]
    fn the_intake_thread_types_and_acks() {
        let mut e = env();
        let sim = ask(&e, vec![scope_q()]);
        tick(&e);
        let stop = responder(&mut e, sim.clone());
        let item = answer(&e, vec![pick(0, &[1], None)]);
        let inbox = e.rt.start_poll_intake();
        fg(&e);
        inbox.deliver_poll("phone-1", &item.fp, PermitSenderKind::Mobile, &item.answer);
        for _ in 0..100 {
            if !closes(&e).is_empty() {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        e.rt.stop.store(true, Ordering::SeqCst);
        stop.store(true, Ordering::SeqCst);
        assert_eq!(closes(&e).len(), 1);
        assert_eq!(lock(&sim).answers, ["All sources at once"]);
    }

    #[test]
    fn an_answer_from_an_untrusted_sender_is_dropped() {
        let e = env();
        let sim = ask(&e, vec![scope_q()]);
        tick(&e);
        let item = answer(&e, vec![pick(0, &[1], None)]);
        e.trust.0.store(false, Ordering::SeqCst);
        e.rt.process_poll_item(&item);
        assert!(closes(&e).is_empty());
        assert!(lock(&sim).keys.is_empty());
        assert!(e.rx.as_ref().unwrap().try_recv().is_err());
    }

    #[test]
    fn an_answer_without_the_foreground_fails_without_typing() {
        let e = env();
        let sim = ask(&e, vec![scope_q()]);
        tick(&e);
        let item = answer(&e, vec![pick(0, &[1], None)]);
        *lock(&e.s.fg) = None;
        e.rt.process_poll_item(&item);
        let c = closes(&e);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].closed.as_deref(), Some("failed"));
        assert!(lock(&sim).keys.is_empty());
        e.rt.process_poll_item(&item);
        assert_eq!(closes(&e).len(), 1);
    }

    #[test]
    fn a_picker_answered_in_the_terminal_closes_as_terminal() {
        let e = env();
        ask(&e, vec![scope_q()]);
        tick(&e);
        append(&e.log, &result_line());
        capture(&e);
        tick(&e);
        tick(&e);
        let c = closes(&e);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].closed.as_deref(), Some("terminal"));
        assert_eq!(cards(&e).len(), 1);
    }

    #[test]
    fn an_exited_session_closes_its_card_as_gone() {
        let e = env();
        ask(&e, vec![scope_q()]);
        tick(&e);
        e.s.on_exited(&ExitInfo {
            code: Some(0),
            signal: None,
        });
        capture(&e);
        tick(&e);
        let c = closes(&e);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].closed.as_deref(), Some("gone"));
    }

    #[test]
    fn the_intake_caps_queued_answers_per_device() {
        let (tx, rx) = std::sync::mpsc::sync_channel::<PollItem>(POLL_INBOX_CAP);
        let inbox = PollInbox {
            tx,
            queued: Arc::new(Mutex::new(HashMap::new())),
            busy: Arc::default(),
        };
        let a = PollAnswer {
            v: 1,
            card_id: "poll_x".into(),
            answers: vec![pick(0, &[0], None)],
        };
        for _ in 0..6 {
            inbox.deliver_poll("dev-1", "fp", PermitSenderKind::Mobile, &a);
        }
        assert_eq!(rx.try_iter().count(), POLL_PER_DEVICE);
    }

    #[test]
    fn every_picker_screen_blocks_the_input_router() {
        const SCREENS: [&str; 7] = [
            include_str!("../../profiles/screens/claude-askuser-single.txt"),
            include_str!("../../profiles/screens/claude-askuser-single-other-typed.txt"),
            include_str!("../../profiles/screens/claude-askuser-multi-open.txt"),
            include_str!("../../profiles/screens/claude-askuser-multi-next-row.txt"),
            include_str!("../../profiles/screens/claude-askuser-multi-q2.txt"),
            include_str!("../../profiles/screens/claude-askuser-multi-review.txt"),
            include_str!("../../profiles/screens/claude-askuser-narrow80.txt"),
        ];
        let e = env();
        for (i, screen) in SCREENS.iter().enumerate() {
            let rows: Vec<String> = screen.lines().map(String::from).collect();
            show(&e.s, &rows);
            assert!(e.s.dialog_open(), "fixture {i}");
            assert!(
                e.s.write_gated(b"hi", Instant::now(), false).is_err(),
                "fixture {i}"
            );
        }
    }

    #[test]
    fn a_close_that_cannot_be_stored_is_retried_until_it_is() {
        let e = env();
        ask(&e, vec![scope_q()]);
        tick(&e);
        e.rt.poll_relay.fail_appends.store(2, Ordering::SeqCst);
        append(&e.log, &result_line());
        capture(&e);
        tick(&e);
        assert!(closes(&e).is_empty());
        assert_eq!(lock(&e.rt.poll_relay.pending).len(), 1);
        for _ in 0..4 {
            thread::sleep(POLL_TICK + Duration::from_millis(20));
            e.rt.poll_tick();
        }
        let c = closes(&e);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].closed.as_deref(), Some("terminal"));
        assert!(lock(&e.rt.poll_relay.pending).is_empty());
    }

    #[test]
    fn a_refused_answer_fails_an_open_card_only() {
        let e = env();
        ask(&e, vec![scope_q()]);
        tick(&e);
        let item = answer(&e, vec![pick(0, &[1], None)]);
        let (tx, rx) = std::sync::mpsc::sync_channel::<PollItem>(1);
        let inbox = PollInbox {
            tx,
            queued: Arc::new(Mutex::new(HashMap::new())),
            busy: e.rt.poll_relay.busy.clone(),
        };
        inbox.deliver_poll("d", "fp", PermitSenderKind::Mobile, &item.answer);
        inbox.deliver_poll("d", "fp", PermitSenderKind::Mobile, &item.answer);
        assert_eq!(rx.try_iter().count(), 1);
        e.rt.poll_flush();
        let c = closes(&e);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].closed.as_deref(), Some("failed"));
        assert_eq!(c[0].why_not.as_deref(), Some(BUSY_WHY));
        e.rt.poll_flush();
        assert_eq!(closes(&e).len(), 1);
    }

    #[test]
    fn a_refused_answer_leaves_an_answering_card_alone() {
        let e = env();
        ask(&e, vec![scope_q()]);
        tick(&e);
        let item = answer(&e, vec![pick(0, &[1], None)]);
        lock(&e.rt.poll_relay.desk)
            .begin_answer(&item.answer.card_id, &item.answer, "d")
            .unwrap();
        lock(&e.rt.poll_relay.busy).push_back(item.answer.card_id.clone());
        e.rt.poll_flush();
        assert!(closes(&e).is_empty());
    }

    #[test]
    fn a_session_that_no_longer_qualifies_is_reconciled() {
        let e = env();
        ask(&e, vec![scope_q()]);
        tick(&e);
        lock(&e.s.inner).meta.profile = Some("codex".into());
        tick(&e);
        let c = closes(&e);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].closed.as_deref(), Some("terminal"));
    }

    #[test]
    fn a_session_that_vanished_is_reconciled_as_gone() {
        let e = env();
        ask(&e, vec![scope_q()]);
        tick(&e);
        lock(&e.rt.sessions).remove(SID);
        e.rt.poll_observe();
        let c = closes(&e);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].closed.as_deref(), Some("gone"));
    }
}
