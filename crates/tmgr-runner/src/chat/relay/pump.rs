use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::Value;
use tmgr_chat_relay::api::Request;
use tmgr_chat_relay::engine::blob::BlobPart;
use tmgr_chat_relay::engine::{
    retry_after_secs, ChatSink, EngineConfig, KeyStore, RelayEngine, RoomScope, Slot, SpawnKeySink,
    SpawnOkSink, Transport,
};
use tmgr_ipc::chat::{AuthorKind, ChatMessage, ChatPush, MessageKind, RoomInfo, RoomRef};

use crate::mgr_rpc::relay::{RpcInbox, RpcSend};

use super::activity::{run_activity, Clock, Leases};
use super::rooms_sync::{host_label, OwnerSink, RoomsSync, Snapshot};
use super::store::RelayDir;
use crate::chat::blobs::BlobStore;
use crate::chat::{ActivityRx, Incoming, IngestError, Ingested, Receipt, ReceiptFn};

const LABEL: &str = "Runner";
const CADENCE: Duration = Duration::from_secs(5);
const CADENCE_LIVE: Duration = Duration::from_secs(1);
const LIVE_GRACE: Duration = Duration::from_secs(120);
const BACKOFF_START: Duration = Duration::from_secs(5);
const BACKOFF_CAP: Duration = Duration::from_secs(300);
const WAIT_SETUP: Duration = Duration::from_secs(5);
const WAIT_BROKEN: Duration = Duration::from_secs(60);
const OUT_QUEUE: usize = 1024;
const SEND_ATTEMPTS: u32 = 5;
const RECV_TICK: Duration = Duration::from_secs(1);
const ROOMS_EVERY: Duration = Duration::from_secs(5);
const SLEEP_TICK: Duration = Duration::from_millis(250);
const RECIPIENT_FULL: &str = "the recipient's relay queue is full";
const PERMANENT: [&str; 3] = [
    "there are no other trusted devices",
    "this device is not a trusted member",
    "the target is not a trusted owner device",
];

pub type IngestFn =
    Box<dyn Fn(Incoming, Option<RoomRef>) -> Result<Ingested, IngestError> + Send + Sync>;
pub type TransportFactory = Arc<dyn Fn(&str) -> Result<Box<dyn Transport>, String> + Send + Sync>;

pub trait Relay: Send + Sync {
    fn poll(&self) -> Result<(), String>;
    fn send(&self, msg: &ChatMessage) -> Result<(), String>;
    fn send_receipt(&self, receipt: &Receipt) -> Result<(), String>;
}

pub enum Out {
    Chat(Box<ChatMessage>),
    Receipt(Receipt),
}

impl From<ChatMessage> for Out {
    fn from(m: ChatMessage) -> Self {
        Out::Chat(Box::new(m))
    }
}

pub fn receipt_queue(queue: SyncSender<Out>) -> ReceiptFn {
    Arc::new(move |r| match queue.try_send(Out::Receipt(r)) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => {
            crate::daemon::log("chat relay: outbound queue full, receipt not sent");
            false
        }
        Err(TrySendError::Disconnected(_)) => false,
    })
}

impl<T: Transport, K: KeyStore, S: ChatSink> Relay for RelayEngine<T, K, S> {
    fn poll(&self) -> Result<(), String> {
        self.poll_once().map(|_| ())
    }

    fn send(&self, msg: &ChatMessage) -> Result<(), String> {
        self.send_chat(msg, wants_push(msg))
    }

    fn send_receipt(&self, r: &Receipt) -> Result<(), String> {
        RelayEngine::send_receipt(self, &r.device, &r.id, r.state, r.reason.as_deref())
    }
}

fn wants_push(msg: &ChatMessage) -> bool {
    matches!(msg.author.kind, AuthorKind::Manager | AuthorKind::Session)
        && msg.kind == MessageKind::Decision
}

pub type GiveUpFn = Box<dyn Fn(&str, &str, &str) + Send + Sync>;
pub type SharedGiveUpFn = Arc<dyn Fn(&str, &str, &str) + Send + Sync>;

pub struct RelaySink {
    machine: String,
    ingest: IngestFn,
    give_up: Option<GiveUpFn>,
    blobs: Option<Arc<BlobStore>>,
}

impl RelaySink {
    pub fn new(machine: String, ingest: IngestFn) -> Self {
        Self {
            machine,
            ingest,
            give_up: None,
            blobs: None,
        }
    }

    pub fn with_blobs(mut self, blobs: Arc<BlobStore>) -> Self {
        self.blobs = Some(blobs);
        self
    }

    pub fn with_give_up(mut self, give_up: GiveUpFn) -> Self {
        self.give_up = Some(give_up);
        self
    }
}

impl ChatSink for RelaySink {
    fn deliver(
        &self,
        from_device: &str,
        from_kind: Option<&str>,
        msg: ChatMessage,
    ) -> Result<(), String> {
        if msg.author.kind != AuthorKind::Owner || msg.room.machine != self.machine {
            self.dropped("not owner input for this machine");
            return Ok(());
        }
        if !matches!(from_kind, Some("desktop" | "mobile")) {
            self.dropped("sender is not a pinned desktop or phone");
            if let Some(give_up) = &self.give_up {
                give_up(from_device, &msg.id, crate::chat::REJECT_NOT_TRUSTED);
            }
            return Ok(());
        }
        let id = msg.id.clone();
        let desktop = from_kind == Some("desktop");
        let target = msg.mention.map(|m| m.target);
        let incoming = Incoming {
            room: msg.room,
            id: msg.id,
            ts_ms: msg.ts_ms,
            body_md: msg.body_md,
            reply_to: msg.reply_to,
            attachments: msg.attachments,
            device: Some(from_device.to_owned()),
            owner_device: true,
            client: if desktop {
                crate::chat::RELAY_CLIENT
            } else {
                crate::chat::RELAY_MOBILE_CLIENT
            }
            .into(),
        };
        match (self.ingest)(incoming, target) {
            Ok(_) => Ok(()),
            Err(IngestError::NotFound(e) | IngestError::Conflict(e)) => {
                crate::daemon::log(&format!("chat relay: message not stored: {e}"));
                Ok(())
            }
            Err(IngestError::Storage(e)) => Err(e),
            Err(IngestError::Invalid(_)) => {
                crate::daemon::log("chat relay: message dropped (invalid)");
                if let (true, Some(give_up)) = (desktop, &self.give_up) {
                    give_up(from_device, &id, crate::chat::REJECT_INVALID);
                }
                Ok(())
            }
        }
    }

    fn deliver_blob(&self, from_device: &str, part: BlobPart) -> Result<(), String> {
        let Some(blobs) = &self.blobs else {
            self.dropped("files are not supported here");
            return Ok(());
        };
        blobs.deliver_relay(from_device, &part)
    }

    fn dropped(&self, reason: &'static str) {
        crate::daemon::log(&format!("chat relay: dropped a message ({reason})"));
    }

    fn skipped_full(&self, device_id: &str, kind: &str) {
        let short: String = device_id.chars().take(8).collect();
        crate::daemon::log(&format!(
            "chat relay: skipped {kind} {short}: its relay queue is full"
        ));
    }
}

struct DynTransport(Box<dyn Transport>);

impl Transport for DynTransport {
    fn call(&self, req: &Request, device_token: Option<&str>) -> Result<String, String> {
        self.0.call(req, device_token)
    }
}

fn retry_hint(e: &str) -> Option<u64> {
    retry_after_secs(e).or_else(|| serde_json::from_str::<Value>(e).ok()?["retry_after"].as_u64())
}

fn wait_for(backoff: Duration, err: &str) -> Duration {
    let hinted = Duration::from_secs(retry_hint(err).unwrap_or(0).min(BACKOFF_CAP.as_secs()));
    backoff.max(hinted)
}

fn error_class(e: &str) -> &'static str {
    if retry_hint(e).is_some() {
        "rate limited"
    } else if e.contains("401") || e.contains("unauthorized") {
        "unauthorized"
    } else {
        "failed"
    }
}

#[derive(Default)]
pub(super) struct FailureLog(Option<&'static str>);

impl FailureLog {
    pub(super) fn note(&mut self, e: &str) -> Option<&'static str> {
        let class = error_class(e);
        (self.0.replace(class) != Some(class)).then_some(class)
    }

    pub(super) fn clear(&mut self) {
        self.0 = None;
    }
}

pub(super) fn cadence(live: bool, idle_for: Duration) -> Duration {
    if live || idle_for < LIVE_GRACE {
        CADENCE_LIVE
    } else {
        CADENCE
    }
}

pub(super) struct Pace {
    last_live: Instant,
}

impl Pace {
    pub(super) fn new(now: Instant) -> Self {
        Self { last_live: now }
    }

    pub(super) fn next(&mut self, live: bool, now: Instant) -> Duration {
        if live {
            self.last_live = now;
        }
        cadence(live, now.saturating_duration_since(self.last_live))
    }
}

pub fn run_poll(
    relay: &dyn Relay,
    stop: &dyn Fn() -> bool,
    live: &dyn Fn() -> bool,
    sleep: &dyn Fn(Duration),
) {
    let mut backoff = BACKOFF_START;
    let mut failures = FailureLog::default();
    let mut pace = Pace::new(Instant::now());
    while !stop() {
        match relay.poll() {
            Ok(()) => {
                backoff = BACKOFF_START;
                failures.clear();
                sleep(pace.next(live(), Instant::now()));
            }
            Err(e) => {
                if let Some(class) = failures.note(&e) {
                    crate::daemon::log(&format!("chat relay: poll failed ({class})"));
                }
                sleep(wait_for(backoff, &e));
                backoff = (backoff * 2).min(BACKOFF_CAP);
            }
        }
    }
}

pub fn forward<T: From<ChatMessage>>(
    subscribe: &dyn Fn() -> Receiver<ChatPush>,
    queue: &SyncSender<T>,
    stop: &dyn Fn() -> bool,
) {
    let mut rx = subscribe();
    while !stop() {
        match rx.recv_timeout(RECV_TICK) {
            Ok(push)
                if push.message.author.device.is_some()
                    || push.message.author.kind == AuthorKind::Owner => {}
            Ok(push) => match queue.try_send(push.message.into()) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    crate::daemon::log("chat relay: outbound queue full, message not sent");
                }
                Err(TrySendError::Disconnected(_)) => return,
            },
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                crate::daemon::log("chat relay: chat subscription closed, resubscribing");
                rx = subscribe();
            }
        }
    }
}

fn retry_send(
    what: &str,
    mut attempt_send: impl FnMut() -> Result<(), String>,
    stop: &dyn Fn() -> bool,
    sleep: &dyn Fn(Duration),
) {
    let mut backoff = BACKOFF_START;
    for attempt in 1..=SEND_ATTEMPTS {
        let Err(e) = attempt_send() else { return };
        if e == RECIPIENT_FULL || PERMANENT.iter().any(|p| e.starts_with(p)) {
            crate::daemon::log(&format!("chat relay: {what} dropped: {e}"));
            return;
        }
        if attempt == SEND_ATTEMPTS || stop() {
            crate::daemon::log(&format!(
                "chat relay: {what} dropped after {attempt} tries: {e}"
            ));
            return;
        }
        sleep(wait_for(backoff, &e));
        backoff = (backoff * 2).min(BACKOFF_CAP);
    }
}

fn send_one(
    relay: &dyn Relay,
    msg: &ChatMessage,
    stop: &dyn Fn() -> bool,
    sleep: &dyn Fn(Duration),
) {
    retry_send("message", || relay.send(msg), stop, sleep);
}

fn send_receipt_one(
    relay: &dyn Relay,
    receipt: &Receipt,
    stop: &dyn Fn() -> bool,
    sleep: &dyn Fn(Duration),
) {
    retry_send("receipt", || relay.send_receipt(receipt), stop, sleep);
}

pub fn run_send(
    relay: &dyn Relay,
    queue: &Receiver<Out>,
    stop: &dyn Fn() -> bool,
    sleep: &dyn Fn(Duration),
) {
    while !stop() {
        match queue.recv_timeout(RECV_TICK) {
            Ok(Out::Chat(msg)) => send_one(relay, &msg, stop, sleep),
            Ok(Out::Receipt(r)) => send_receipt_one(relay, &r, stop, sleep),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

pub type HistoryFn = Arc<dyn Fn(&RoomRef, u32) -> Vec<ChatMessage> + Send + Sync>;
pub type OnDevice = Arc<dyn Fn(Option<(String, String)>) + Send + Sync>;

pub struct Setup {
    pub dir: PathBuf,
    pub machine: String,
    pub transport: TransportFactory,
    pub ingest:
        Arc<dyn Fn(Incoming, Option<RoomRef>) -> Result<Ingested, IngestError> + Send + Sync>,
    pub give_up: SharedGiveUpFn,
    pub blobs: Option<Arc<BlobStore>>,
    pub subscribe: Arc<dyn Fn() -> Receiver<ChatPush> + Send + Sync>,
    pub subscribe_activity: Arc<dyn Fn() -> ActivityRx + Send + Sync>,
    pub stop: Arc<dyn Fn() -> bool + Send + Sync>,
    pub rpc: Arc<RpcInbox>,
    pub spawnkey: Arc<dyn SpawnKeySink>,
    pub spawnok: Option<Arc<dyn SpawnOkSink>>,
    pub serve_rpc: Arc<dyn Fn(RpcSend) -> Option<JoinHandle<()>> + Send + Sync>,
    pub on_device: OnDevice,
    pub on_owners: Arc<dyn Fn(crate::chat::OwnersFn) + Send + Sync>,
    pub on_receipts: Arc<dyn Fn(Option<ReceiptFn>) + Send + Sync>,
    pub rooms: Arc<dyn Fn() -> Vec<RoomInfo> + Send + Sync>,
    pub history: HistoryFn,
    pub live: Arc<dyn Fn() -> bool + Send + Sync>,
}

type Engine = RelayEngine<DynTransport, RelayDir, RelaySink>;

fn build(setup: &Setup, leases: &Arc<Leases>) -> Result<Option<Engine>, String> {
    let dir = RelayDir::open(&setup.dir)?;
    let Some(cfg) = dir.load_config()? else {
        return Ok(None);
    };
    if dir.load(Slot::Identity)?.is_none() {
        return Ok(None);
    }
    let transport = (setup.transport)(&cfg.server)?;
    let ingest = setup.ingest.clone();
    let give_up = setup.give_up.clone();
    let mut sink = RelaySink::new(setup.machine.clone(), Box::new(move |i, t| ingest(i, t)))
        .with_give_up(Box::new(move |device, id, reason| {
            give_up(device, id, reason)
        }));
    if let Some(blobs) = &setup.blobs {
        sink = sink.with_blobs(blobs.clone());
    }
    let engine_cfg = EngineConfig {
        user_id: cfg.user_id,
        label: LABEL.to_owned(),
        local_machine: setup.machine.clone(),
    };
    RelayEngine::new(DynTransport(transport), dir, sink, engine_cfg).map(|e| {
        let e = e
            .with_kind("runner")
            .with_room_scope(RoomScope::Local)
            .with_rpc_sink(setup.rpc.clone())
            .with_spawnkey_sink(setup.spawnkey.clone())
            .with_activity_sub_sink(leases.clone());
        Some(match &setup.spawnok {
            Some(sink) => e.with_spawnok_sink(sink.clone()),
            None => e,
        })
    })
}

fn sleeper(stop: Arc<dyn Fn() -> bool + Send + Sync>) -> impl Fn(Duration) {
    move |d| {
        let mut left = d;
        while !left.is_zero() && !stop() {
            let step = left.min(SLEEP_TICK);
            thread::sleep(step);
            left -= step;
        }
    }
}

fn run(setup: Setup) {
    let stop = setup.stop.clone();
    let sleep = sleeper(stop.clone());
    let mut warned = false;
    let started = Instant::now();
    let clock: Clock = Arc::new(move || started.elapsed().as_millis() as u64);
    let leases = Arc::new(Leases::new(clock.clone()));
    while !stop() {
        match build(&setup, &leases) {
            Ok(Some(engine)) => return serve(Arc::new(engine), &setup, &leases, clock),
            Ok(None) => sleep(WAIT_SETUP),
            Err(e) => {
                if !warned {
                    crate::daemon::log(&format!("chat relay unavailable: {e}"));
                    warned = true;
                }
                sleep(WAIT_BROKEN);
            }
        }
    }
}

fn serve(engine: Arc<Engine>, setup: &Setup, leases: &Arc<Leases>, clock: Clock) {
    let (tx, rx) = mpsc::sync_channel::<Out>(OUT_QUEUE);
    (setup.on_receipts)(Some(receipt_queue(tx.clone())));
    let (stop, subscribe) = (setup.stop.clone(), setup.subscribe.clone());
    let forwarder = thread::Builder::new()
        .name("chat-relay-forward".into())
        .spawn({
            let stop = stop.clone();
            move || forward(&*subscribe, &tx, &*stop)
        });
    let sender = thread::Builder::new()
        .name("chat-relay-send".into())
        .spawn({
            let (engine, stop) = (engine.clone(), stop.clone());
            move || {
                let sleep = sleeper(stop.clone());
                run_send(&*engine, &rx, &*stop, &sleep)
            }
        });
    let rpc_send: RpcSend = {
        let engine = engine.clone();
        Arc::new(move |device, kind, id, payload| engine.send_rpc(device, kind, id, payload))
    };
    let rooms_sync = thread::Builder::new()
        .name("chat-relay-rooms".into())
        .spawn({
            let (engine, stop) = (engine.clone(), stop.clone());
            let (dir, machine) = (setup.dir.clone(), setup.machine.clone());
            let (rooms, history) = (setup.rooms.clone(), setup.history.clone());
            move || {
                let sleep = sleeper(stop.clone());
                run_rooms_sync(&*engine, &dir, &machine, &*rooms, &*history, &*stop, &sleep)
            }
        });
    let activity = thread::Builder::new()
        .name("chat-relay-activity".into())
        .spawn({
            let (engine, stop, leases) = (engine.clone(), stop.clone(), leases.clone());
            let (machine, rx) = (setup.machine.clone(), (setup.subscribe_activity)());
            move || {
                let sleep = sleeper(stop.clone());
                run_activity(&machine, &rx, &leases, &*clock, &*engine, &*stop, &sleep)
            }
        });
    (setup.on_device)(engine.own_device());
    let weak = Arc::downgrade(&engine);
    (setup.on_owners)(Arc::new(move || {
        weak.upgrade()
            .map(|e| e.owner_devices())
            .unwrap_or_default()
    }));
    let rpc_server = (setup.serve_rpc)(rpc_send);
    crate::daemon::log("chat relay: pump started");
    run_poll(&*engine, &*stop, &*setup.live, &sleeper(stop.clone()));
    for h in [
        forwarder.ok(),
        sender.ok(),
        rooms_sync.ok(),
        activity.ok(),
        rpc_server,
    ]
    .into_iter()
    .flatten()
    {
        let _ = h.join();
    }
    (setup.on_device)(None);
    (setup.on_receipts)(None);
}

pub fn run_rooms_sync(
    sink: &dyn OwnerSink,
    dir: &std::path::Path,
    machine: &str,
    rooms: &dyn Fn() -> Vec<RoomInfo>,
    history: &dyn Fn(&RoomRef, u32) -> Vec<ChatMessage>,
    stop: &dyn Fn() -> bool,
    sleep: &dyn Fn(Duration),
) {
    let label = host_label();
    let snapshot = || {
        let followers = crate::mgr_rpc::machines::Machines::load(dir)
            .rows()
            .into_iter()
            .map(|(id, _)| id.to_owned())
            .collect();
        Snapshot {
            machine: machine.to_owned(),
            label: label.clone(),
            top: crate::mgr_rpc::top::is_top(dir),
            followers,
            rooms: rooms(),
        }
    };
    let mut sync = RoomsSync::new();
    while !stop() {
        sync.tick(sink, dir, &snapshot, history);
        sleep(ROOMS_EVERY);
    }
}

pub fn spawn(setup: Setup) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("chat-relay-pump".into())
        .spawn(move || run(setup))
}

#[cfg(test)]
mod tests;
