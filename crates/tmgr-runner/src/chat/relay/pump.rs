use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::Value;
use tmgr_chat_relay::api::Request;
use tmgr_chat_relay::engine::{
    retry_after_secs, ChatSink, EngineConfig, KeyStore, RelayEngine, RoomScope, Slot, Transport,
};
use tmgr_ipc::chat::{AuthorKind, ChatMessage, ChatPush, MessageKind};

use crate::mgr_rpc::relay::{RpcInbox, RpcSend};

use super::store::RelayDir;
use crate::chat::{Incoming, IngestError, Ingested};

const LABEL: &str = "Runner";
const CADENCE: Duration = Duration::from_secs(5);
const BACKOFF_START: Duration = Duration::from_secs(5);
const BACKOFF_CAP: Duration = Duration::from_secs(300);
const WAIT_SETUP: Duration = Duration::from_secs(5);
const WAIT_BROKEN: Duration = Duration::from_secs(60);
const OUT_QUEUE: usize = 1024;
const SEND_ATTEMPTS: u32 = 5;
const RECV_TICK: Duration = Duration::from_secs(1);
const SLEEP_TICK: Duration = Duration::from_millis(250);
const RECIPIENT_FULL: &str = "the recipient's relay queue is full";
const PERMANENT: [&str; 2] = [
    "there are no other trusted devices",
    "this device is not a trusted member",
];

pub type IngestFn = Box<dyn Fn(Incoming) -> Result<Ingested, IngestError> + Send + Sync>;
pub type TransportFactory = Arc<dyn Fn(&str) -> Result<Box<dyn Transport>, String> + Send + Sync>;

pub trait Relay: Send + Sync {
    fn poll(&self) -> Result<(), String>;
    fn send(&self, msg: &ChatMessage) -> Result<(), String>;
}

impl<T: Transport, K: KeyStore, S: ChatSink> Relay for RelayEngine<T, K, S> {
    fn poll(&self) -> Result<(), String> {
        self.poll_once().map(|_| ())
    }

    fn send(&self, msg: &ChatMessage) -> Result<(), String> {
        self.send_chat(msg, wants_push(msg))
    }
}

fn wants_push(msg: &ChatMessage) -> bool {
    matches!(msg.author.kind, AuthorKind::Manager | AuthorKind::Session)
        && msg.kind == MessageKind::Decision
}

pub struct RelaySink {
    machine: String,
    ingest: IngestFn,
}

impl RelaySink {
    pub fn new(machine: String, ingest: IngestFn) -> Self {
        Self { machine, ingest }
    }
}

impl ChatSink for RelaySink {
    fn deliver(&self, from_device: &str, msg: ChatMessage) -> Result<(), String> {
        if msg.author.kind != AuthorKind::Owner || msg.room.machine != self.machine {
            crate::daemon::log(
                "chat relay: dropped a message that is not owner input for this machine",
            );
            return Ok(());
        }
        let incoming = Incoming {
            room: msg.room,
            id: msg.id,
            ts_ms: msg.ts_ms,
            body_md: msg.body_md,
            reply_to: msg.reply_to,
            attachments: msg.attachments,
            device: Some(from_device.to_owned()),
        };
        match (self.ingest)(incoming) {
            Ok(_) => Ok(()),
            Err(IngestError::NotFound(e) | IngestError::Conflict(e)) => {
                crate::daemon::log(&format!("chat relay: message not stored: {e}"));
                Ok(())
            }
            Err(IngestError::Invalid(e)) => Err(e),
        }
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
struct FailureLog(Option<&'static str>);

impl FailureLog {
    fn note(&mut self, e: &str) -> Option<&'static str> {
        let class = error_class(e);
        (self.0.replace(class) != Some(class)).then_some(class)
    }

    fn clear(&mut self) {
        self.0 = None;
    }
}

pub fn run_poll(relay: &dyn Relay, stop: &dyn Fn() -> bool, sleep: &dyn Fn(Duration)) {
    let mut backoff = BACKOFF_START;
    let mut failures = FailureLog::default();
    while !stop() {
        match relay.poll() {
            Ok(()) => {
                backoff = BACKOFF_START;
                failures.clear();
                sleep(CADENCE);
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

pub fn forward(
    subscribe: &dyn Fn() -> Receiver<ChatPush>,
    queue: &SyncSender<ChatMessage>,
    stop: &dyn Fn() -> bool,
) {
    let mut rx = subscribe();
    while !stop() {
        match rx.recv_timeout(RECV_TICK) {
            Ok(push)
                if push.message.author.device.is_some()
                    || push.message.author.kind == AuthorKind::Owner => {}
            Ok(push) => match queue.try_send(push.message) {
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

fn send_one(
    relay: &dyn Relay,
    msg: &ChatMessage,
    stop: &dyn Fn() -> bool,
    sleep: &dyn Fn(Duration),
) {
    let mut backoff = BACKOFF_START;
    for attempt in 1..=SEND_ATTEMPTS {
        let Err(e) = relay.send(msg) else { return };
        if e == RECIPIENT_FULL || PERMANENT.iter().any(|p| e.starts_with(p)) {
            crate::daemon::log(&format!("chat relay: message dropped: {e}"));
            return;
        }
        if attempt == SEND_ATTEMPTS || stop() {
            crate::daemon::log(&format!(
                "chat relay: message dropped after {attempt} tries: {e}"
            ));
            return;
        }
        sleep(wait_for(backoff, &e));
        backoff = (backoff * 2).min(BACKOFF_CAP);
    }
}

pub fn run_send(
    relay: &dyn Relay,
    queue: &Receiver<ChatMessage>,
    stop: &dyn Fn() -> bool,
    sleep: &dyn Fn(Duration),
) {
    while !stop() {
        match queue.recv_timeout(RECV_TICK) {
            Ok(msg) => send_one(relay, &msg, stop, sleep),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

pub struct Setup {
    pub dir: PathBuf,
    pub machine: String,
    pub transport: TransportFactory,
    pub ingest: Arc<dyn Fn(Incoming) -> Result<Ingested, IngestError> + Send + Sync>,
    pub subscribe: Arc<dyn Fn() -> Receiver<ChatPush> + Send + Sync>,
    pub stop: Arc<dyn Fn() -> bool + Send + Sync>,
    pub rpc: Arc<RpcInbox>,
    pub serve_rpc: Arc<dyn Fn(RpcSend) -> Option<JoinHandle<()>> + Send + Sync>,
}

type Engine = RelayEngine<DynTransport, RelayDir, RelaySink>;

fn build(setup: &Setup) -> Result<Option<Engine>, String> {
    let dir = RelayDir::open(&setup.dir)?;
    let Some(cfg) = dir.load_config()? else {
        return Ok(None);
    };
    if dir.load(Slot::Identity)?.is_none() {
        return Ok(None);
    }
    let transport = (setup.transport)(&cfg.server)?;
    let ingest = setup.ingest.clone();
    let sink = RelaySink::new(setup.machine.clone(), Box::new(move |i| ingest(i)));
    let engine_cfg = EngineConfig {
        user_id: cfg.user_id,
        label: LABEL.to_owned(),
        local_machine: setup.machine.clone(),
    };
    RelayEngine::new(DynTransport(transport), dir, sink, engine_cfg)
        .map(|e| {
            Some(
                e.with_kind("runner")
                    .with_room_scope(RoomScope::Local)
                    .with_rpc_sink(setup.rpc.clone()),
            )
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
    while !stop() {
        match build(&setup) {
            Ok(Some(engine)) => return serve(Arc::new(engine), &setup),
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

fn serve(engine: Arc<Engine>, setup: &Setup) {
    let (tx, rx) = mpsc::sync_channel(OUT_QUEUE);
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
    let rpc_server = (setup.serve_rpc)(rpc_send);
    crate::daemon::log("chat relay: pump started");
    run_poll(&*engine, &*stop, &sleeper(stop.clone()));
    for h in [forwarder, sender, rpc_server].into_iter().flatten() {
        let _ = h.join();
    }
}

pub fn spawn(setup: Setup) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("chat-relay-pump".into())
        .spawn(move || run(setup))
}

#[cfg(test)]
mod tests;
