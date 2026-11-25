mod cache;
mod lock;
pub mod poll;
mod probe;
pub mod step_up;
mod store;

use super::{emit, Opts};
use crate::cmd::chat::join::{drive, private_runner_dir};
use crate::cmd::chat::join_code::read_join_text;
use crate::cmd::chat::transport::{HttpRelayTransport, STATUS_KEY};
use crate::ctx::Ctx;
use crate::interact::is_interactive;
use cache::{CacheSink, RoomCache};
use lock::InstanceLock;
use probe::Probe;
use serde_json::json;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use store::{config_dir, device_namespace, open_store, DynStore, NO_SECURE_STORE};
use tmgr_chat_relay::api::MAX_LABEL_CHARS;
use tmgr_chat_relay::engine::{
    http_status, DeviceState, EngineConfig, KeyStore, Receipt, ReceiptState, RelayEngine,
    RoomScope, Transport, TrustState,
};
use tmgr_core::api::scopes_hint;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::chat::{
    limits, new_message_id, Author, AuthorKind, ChatMessage, MessageKind, RoomInfo, RoomKind,
    RoomRef, CHAT_V,
};
use tmgr_runner::chat::relay::rooms_sync::host_label;
use tmgr_runner::chat::relay::store::{RelayConfig, RelayDir};
use tmgr_runner::sanitize::sanitize_text;

pub const DIR: &str = "attach-relay";
const KIND: &str = "terminal";
const SCOPE: &str = "chat:devices";
const LABEL_PREFIX: &str = "tmgr attach (";
const LABEL_SUFFIX: &str = ")";

pub fn device_label(machine_label: &str) -> String {
    let room = MAX_LABEL_CHARS - LABEL_PREFIX.len() - LABEL_SUFFIX.len();
    let machine: String = machine_label.chars().take(room).collect();
    format!("{LABEL_PREFIX}{machine}{LABEL_SUFFIX}")
}

pub struct AttachDevice<T: Transport, K: KeyStore> {
    engine: RelayEngine<T, K, CacheSink>,
    cache: Arc<RoomCache>,
    local_machine: String,
    _lock: Option<InstanceLock>,
    notes: Mutex<Vec<String>>,
    removed: AtomicBool,
}

pub const REMOVED_TEXT: &str =
    "This terminal was removed from your devices. Run tmgr device pair to pair it again.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunnerCap {
    pub device_id: String,
    pub machine: String,
    pub machine_label: String,
    pub capable: bool,
}

pub fn terminal_refusal(machine_label: &str) -> String {
    format!(
        "Runner on {} does not accept terminal input. Update tmgr there",
        sanitize_text(machine_label)
    )
}

pub fn unconfirmed_note(machine: &str) -> String {
    format!(
        "{} has not confirmed this terminal yet; the message may not arrive. Run `tmgr device pair` to check.",
        sanitize_text(machine)
    )
}

pub fn capable_runners(st: &TrustState, cache: &RoomCache) -> Vec<RunnerCap> {
    st.devices
        .iter()
        .filter(|d| d.kind == "runner" && d.state == DeviceState::Trusted)
        .filter(|d| !st.pending_revokes.iter().any(|p| p.target_id == d.id))
        .map(|d| match cache.runner(&d.id) {
            Some(i) => RunnerCap {
                device_id: d.id.clone(),
                machine: i.machine,
                machine_label: i.machine_label,
                capable: i.terminal_input,
            },
            None => RunnerCap {
                device_id: d.id.clone(),
                machine: String::new(),
                machine_label: d.label.clone(),
                capable: false,
            },
        })
        .collect()
}

pub fn receipt_note(r: &Receipt) -> Option<String> {
    if r.state != ReceiptState::Failed {
        return None;
    }
    Some(match r.reason.as_deref().filter(|x| !x.is_empty()) {
        Some(reason) => format!("remote send failed: {}", sanitize_text(reason)),
        None => "remote send failed".to_owned(),
    })
}

pub type Paired = AttachDevice<HttpRelayTransport, DynStore>;

struct Parts {
    cfg: RelayConfig,
    relay_dir: RelayDir,
}

fn load_parts(runner_dir: &Path) -> std::result::Result<Option<Parts>, String> {
    if !runner_dir.join(DIR).exists() {
        return Ok(None);
    }
    let relay_dir = config_dir(runner_dir)?;
    Ok(relay_dir.load_config()?.map(|cfg| Parts { cfg, relay_dir }))
}

fn build<T: Transport, K: KeyStore>(
    transport: T,
    keys: K,
    cache: Arc<RoomCache>,
    cfg: &RelayConfig,
    local_machine: &str,
) -> std::result::Result<AttachDevice<T, K>, String> {
    let engine_cfg = EngineConfig {
        user_id: cfg.user_id,
        label: device_label(&host_label()),
        local_machine: local_machine.to_owned(),
    };
    let engine = RelayEngine::new(transport, keys, CacheSink(cache.clone()), engine_cfg)?
        .with_kind(KIND)
        .with_room_scope(RoomScope::Remote)
        .with_rooms_sink(Arc::new(CacheSink(cache.clone())));
    engine.restore_sent(cache.sent());
    Ok(AttachDevice {
        engine,
        cache,
        local_machine: local_machine.to_owned(),
        _lock: None,
        notes: Mutex::default(),
        removed: AtomicBool::new(false),
    })
}

fn build_http(
    runner_dir: &Path,
    parts: &Parts,
    login: Option<String>,
    local_machine: &str,
) -> std::result::Result<Paired, String> {
    let transport = HttpRelayTransport {
        base_url: parts.cfg.server.clone(),
        login_token: login,
    };
    let keys = open_store(&device_namespace(&parts.cfg.server, parts.cfg.user_id))?;
    let cache = RoomCache::open(config_dir(runner_dir)?)?;
    build(transport, keys, cache, &parts.cfg, local_machine)
}

pub fn open(runner_dir: &Path, local_machine: &str) -> std::result::Result<Option<Paired>, String> {
    open_with(runner_dir, local_machine, |parts, machine| {
        build_http(runner_dir, parts, None, machine)
    })
}

fn open_with<T: Transport, K: KeyStore>(
    runner_dir: &Path,
    local_machine: &str,
    make: impl Fn(&Parts, &str) -> std::result::Result<AttachDevice<T, K>, String>,
) -> std::result::Result<Option<AttachDevice<T, K>>, String> {
    let Some(parts) = load_parts(runner_dir)? else {
        return Ok(None);
    };
    let Some(lock) = lock::acquire(runner_dir)? else {
        return Ok(None);
    };
    let mut dev = make(&parts, local_machine)?;
    if !dev.is_paired()? {
        return Ok(None);
    }
    dev._lock = Some(lock);
    Ok(Some(dev))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Daemon,
    Device,
}

pub fn is_device_room(local: &str, room: &RoomRef) -> bool {
    room.kind == RoomKind::Session && room.machine != local
}

pub fn route(local: &str, paired: bool, room: &RoomRef) -> Route {
    if paired && is_device_room(local, room) {
        Route::Device
    } else {
        Route::Daemon
    }
}

pub fn merge_rooms(mut runner: Vec<RoomInfo>, device: Vec<RoomInfo>, local: &str) -> Vec<RoomInfo> {
    for r in device {
        if is_device_room(local, &r.room) && !runner.iter().any(|x| x.room == r.room) {
            runner.push(r);
        }
    }
    runner
}

pub fn owner_message(room: &RoomRef, body: &str, now_ms: u64) -> ChatMessage {
    ChatMessage {
        v: CHAT_V,
        id: new_message_id(),
        room: room.clone(),
        author: Author {
            kind: AuthorKind::Owner,
            id: "owner".into(),
            machine: None,
            device: None,
            agent: None,
        },
        ts_ms: now_ms,
        kind: MessageKind::Text,
        body_md: body.to_owned(),
        attachments: Vec::new(),
        reply_to: None,
        decision: None,
        session_meta: None,
        user_turn: false,
        mention: None,
        poll: None,
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

impl<T: Transport, K: KeyStore> AttachDevice<T, K> {
    pub fn is_paired(&self) -> std::result::Result<bool, String> {
        let st = self.engine.trust_state()?;
        let peers = st
            .devices
            .iter()
            .filter(|d| d.state == DeviceState::Trusted && !d.is_self)
            .count();
        Ok(self.engine.is_joined() && peers > 0 && self.engine.own_device().is_some())
    }

    pub fn removed(&self) -> bool {
        self.removed.load(Ordering::SeqCst)
    }

    pub fn check_removed(&self) -> bool {
        if self.removed() {
            return true;
        }
        let gone = self.engine.own_removed().unwrap_or(false);
        if gone {
            self.removed.store(true, Ordering::SeqCst);
        }
        gone
    }

    fn note_failure(&self, e: String) -> String {
        if http_status(&e) == Some(401) {
            self.removed.store(true, Ordering::SeqCst);
        }
        e
    }

    pub fn poll(&self) -> std::result::Result<Vec<ChatMessage>, String> {
        if self.removed() {
            return Err(REMOVED_TEXT.into());
        }
        let out = self.engine.poll_once().map_err(|e| self.note_failure(e))?;
        if self.engine.own_removed_known() {
            self.removed.store(true, Ordering::SeqCst);
            return Err(REMOVED_TEXT.into());
        }
        if self.engine.is_joined() {
            if let Some(st) = self
                .engine
                .trust_state()
                .ok()
                .filter(|st| !st.devices.is_empty())
            {
                let runners: Vec<String> = st
                    .devices
                    .iter()
                    .filter(|d| d.kind == "runner" && d.state == DeviceState::Trusted)
                    .map(|d| d.id.clone())
                    .collect();
                let _ = self.cache.prune(&runners);
            }
        }
        Ok(out)
    }

    pub fn merge(&self, runner: Vec<RoomInfo>) -> Vec<RoomInfo> {
        merge_rooms(runner, self.rooms(), &self.local_machine)
    }

    pub fn handles(&self, room: &RoomRef) -> bool {
        !self.removed() && route(&self.local_machine, true, room) == Route::Device
    }

    pub fn refusal(&self, room: &RoomRef) -> Option<&'static str> {
        (self.removed() && route(&self.local_machine, true, room) == Route::Device)
            .then_some(REMOVED_TEXT)
    }

    pub fn version(&self) -> u64 {
        self.cache.version()
    }

    pub fn rooms(&self) -> Vec<RoomInfo> {
        if self.removed() {
            return Vec::new();
        }
        let mut rooms = self.cache.rooms(&self.local_machine);
        for r in &mut rooms {
            r.title = sanitize_text(&r.title).into_owned();
        }
        rooms
    }

    pub fn machines(&self) -> Vec<(String, String)> {
        self.cache
            .machines(&self.local_machine)
            .into_iter()
            .map(|(id, label)| (id, sanitize_text(&label).into_owned()))
            .collect()
    }

    pub fn history(&self, room: &RoomRef, limit: usize) -> Vec<ChatMessage> {
        self.cache.history(room, limit)
    }

    pub fn terminal_capable_runners(&self) -> std::result::Result<Vec<RunnerCap>, String> {
        Ok(capable_runners(&self.engine.trust_state()?, &self.cache))
    }

    pub fn receipt_notes(&self) -> Vec<String> {
        let mut notes = std::mem::take(&mut *self.notes.lock().unwrap_or_else(|e| e.into_inner()));
        notes.extend(self.engine.take_receipts().iter().filter_map(receipt_note));
        notes
    }

    pub fn send(&self, room: &RoomRef, body: &str) -> std::result::Result<ChatMessage, String> {
        let runners = self.terminal_capable_runners().unwrap_or_default();
        self.send_with(room, body, &runners)
    }

    fn send_with(
        &self,
        room: &RoomRef,
        body: &str,
        runners: &[RunnerCap],
    ) -> std::result::Result<ChatMessage, String> {
        if room.machine == self.local_machine {
            return Err("this room belongs to this machine".into());
        }
        if body.trim().is_empty() {
            return Err("body_md is empty".into());
        }
        if body.len() > limits::MAX_BODY {
            return Err(format!(
                "body_md must be at most {} bytes",
                limits::MAX_BODY
            ));
        }
        let mut here = runners.iter().filter(|r| r.machine == room.machine);
        let unconfirmed = match here.next() {
            Some(first) if !first.capable && !here.any(|r| r.capable) => {
                let name = Some(first.machine_label.as_str())
                    .filter(|l| !l.is_empty())
                    .unwrap_or(&first.machine);
                return Err(terminal_refusal(name));
            }
            Some(_) => false,
            None => true,
        };
        let msg = owner_message(room, body, now_ms());
        self.engine
            .send_chat_for_machine(&msg, true)
            .map_err(|e| self.note_failure(e))?;
        self.cache.add(msg.clone())?;
        self.cache.set_sent(self.engine.sent_log())?;
        if unconfirmed {
            self.notes
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(unconfirmed_note(&room.machine));
        }
        Ok(msg)
    }
}

fn internal(e: String) -> Error {
    Error::internal(e)
}

fn pair_hint() -> &'static str {
    "run tmgr device pair again with a fresh join code"
}

fn scope_error() -> Error {
    Error::new(
        Kind::Forbidden,
        format!("this login cannot add a chat device (needs scope {SCOPE})"),
    )
    .hint(scopes_hint(SCOPE))
}

pub fn pair_error(e: &str) -> Error {
    let body = serde_json::from_str::<serde_json::Value>(e).unwrap_or_default();
    match body.get(STATUS_KEY).and_then(serde_json::Value::as_u64) {
        Some(401) => Error::auth("not logged in or the session expired").hint("tmgr login"),
        Some(403) => scope_error(),
        _ => Error::internal(e).hint(pair_hint()),
    }
}

pub struct PairOutcome {
    pub already: bool,
    pub replaced: bool,
    pub probe: Probe,
}

fn store_error(e: String) -> Error {
    if e == NO_SECURE_STORE {
        Error::new(Kind::Forbidden, e)
    } else {
        Error::internal(e)
    }
}

pub type Open<'a, T, K> = &'a dyn Fn() -> std::result::Result<AttachDevice<T, K>, String>;

pub fn pair_with<T: Transport, K: KeyStore>(
    open_own: Open<T, K>,
    open_login: Open<T, K>,
    relay_dir: &RelayDir,
    cfg: &RelayConfig,
    read_code: &dyn Fn() -> Result<String>,
    sleep: &dyn Fn(Duration),
    say: &dyn Fn(&str),
) -> Result<PairOutcome> {
    let mut replaced = false;
    {
        let own = open_own().map_err(store_error)?;
        if own.engine.own_device().is_some() && own.check_removed() {
            say(REMOVED_TEXT);
            remove_device(&own, relay_dir)?;
            replaced = true;
        }
    }
    let dev = open_login().map_err(store_error)?;
    let already = dev.is_paired().map_err(internal)?;
    if already && relay_dir.load_config().map_err(internal)?.as_ref() != Some(cfg) {
        relay_dir.save_config(cfg).map_err(internal)?;
    }
    if !already {
        say("On a trusted desktop or mobile device open the chat devices screen, start adding a device and copy the join code.");
        let code = read_code()?;
        relay_dir.save_config(cfg).map_err(internal)?;
        say(&format!(
            "Waiting for a trusted device to approve \"{}\" (up to 10 minutes).",
            device_label(&host_label())
        ));
        drive(&dev.engine, &code, sleep, None).map_err(|e| pair_error(&e))?;
    }
    Ok(PairOutcome {
        already,
        replaced,
        probe: probe::run(&dev, sleep),
    })
}

pub fn pair(o: &Opts) -> Result<i32> {
    tmgr_link::privileged::guard()?;
    let have = Ctx::new(o.cli)?
        .cloud_client()?
        .current_device()
        .ok()
        .map(|d| d.scopes);
    let ask = |prompt: &str| crate::interact::ask_yes(prompt);
    let login =
        |scopes: &[String]| crate::cmd::auth::relogin_with_scopes(&Ctx::new(o.cli)?, scopes);
    let seams = step_up::Seams {
        interactive: is_interactive(),
        ask: &ask,
        login: &login,
    };
    step_up::pair_after_step_up(have.as_deref(), &seams, || pair_once(o))
}

fn pair_once(o: &Opts) -> Result<i32> {
    let ctx = Ctx::new(o.cli)?;
    let client = ctx.cloud_client()?;
    let user_id = client
        .current_user_id()?
        .and_then(|id| u64::try_from(id).ok())
        .ok_or_else(|| Error::auth("cannot tell which user is signed in").hint("tmgr login"))?;
    let tokens = client
        .access_only_tokens()
        .ok_or_else(Error::not_logged_in)?;
    let server = ctx.server()?;
    let runner_dir = private_runner_dir()?;
    let relay_dir = config_dir(&runner_dir).map_err(internal)?;
    let cfg = RelayConfig { server, user_id };
    match relay_dir.load_config().map_err(internal)? {
        Some(old) if old != cfg => {
            return Err(Error::conflict(
                "this machine's attach device belongs to another account or server",
            )
            .hint("run tmgr device unpair first"));
        }
        _ => {}
    }
    let Some(_lock) = lock::acquire(&runner_dir).map_err(internal)? else {
        return Err(Error::conflict(
            "Close tmgr attach first: it is using this device",
        ));
    };
    let machine = tmgr_runner::chat::machine_id(&runner_dir)
        .map_err(|e| Error::internal(format!("machine id: {e}")))?;
    let parts = Parts { cfg, relay_dir };
    let access = tokens.access.expose().to_owned();
    let open_own = || build_http(&runner_dir, &parts, None, &machine);
    let open_login = || build_http(&runner_dir, &parts, Some(access.clone()), &machine);
    let read = || {
        read_join_text(std::io::stdin().lock(), is_interactive(), || {
            rpassword::prompt_password("Join code: ")
        })
    };
    let sleep = |d| std::thread::sleep(d);
    let say = |line: &str| eprintln!("{line}");
    let out = pair_with(
        &open_own,
        &open_login,
        &parts.relay_dir,
        &parts.cfg,
        &read,
        &sleep,
        &say,
    )?;
    Ok(emit_pair(o, &out))
}

fn emit_pair(o: &Opts, out: &PairOutcome) -> i32 {
    let mut human = if out.already {
        "Already paired.".to_owned()
    } else if out.replaced {
        "Paired again. Attach can now use this device.".to_owned()
    } else {
        "Paired. Attach can now use this device.".to_owned()
    };
    if let Some(text) = out.probe.text() {
        human.push('\n');
        human.push_str(&text);
    }
    let (state, unconfirmed) = match &out.probe {
        Probe::Confirmed => ("confirmed", Vec::new()),
        Probe::Unconfirmed(names) => ("unconfirmed", names.clone()),
        Probe::Skipped(_) => ("skipped", Vec::new()),
    };
    emit(
        o,
        json!({"paired": true, "kind": KIND, "probe": state, "unconfirmed": unconfirmed}),
        human,
    )
}

fn remove_device<T: Transport, K: KeyStore>(
    dev: &AttachDevice<T, K>,
    relay_dir: &RelayDir,
) -> Result<String> {
    let Some((id, _)) = dev.engine.own_device() else {
        dev.cache.wipe().map_err(internal)?;
        relay_dir.clear_config().map_err(internal)?;
        return Ok("Not paired.".to_owned());
    };
    let note = dev.engine.leave_confirmed().map_err(|e| {
        Error::internal(format!(
            "{e}. Device {id} keeps its keys on this machine until the relay confirms the removal"
        ))
        .hint(format!(
            "run tmgr device unpair again, or delete device {id} from the Devices list of a trusted desktop or mobile device"
        ))
    })?;
    dev.cache.wipe().map_err(internal)?;
    relay_dir.clear_config().map_err(internal)?;
    Ok(match note {
        None => "Unpaired. The device was removed from the relay and its keys were deleted.".to_owned(),
        Some(warn) => format!(
            "Unpaired. The device was removed from the relay and its keys were deleted. Note: {warn}"
        ),
    })
}

fn unpair_with<T: Transport, K: KeyStore>(
    runner_dir: &Path,
    make: impl Fn(&Parts) -> std::result::Result<AttachDevice<T, K>, String>,
) -> Result<String> {
    let Some(parts) = load_parts(runner_dir).map_err(internal)? else {
        return Ok("Not paired.".to_owned());
    };
    let Some(_lock) = lock::acquire(runner_dir).map_err(internal)? else {
        return Err(Error::conflict(
            "Close tmgr attach first: it is using this device",
        ));
    };
    let dev = match make(&parts) {
        Ok(dev) => dev,
        Err(e) => return Err(store_error(e)),
    };
    remove_device(&dev, &parts.relay_dir)
}

pub fn unpair(o: &Opts) -> Result<i32> {
    tmgr_link::privileged::guard()?;
    let runner_dir = private_runner_dir()?;
    let human = unpair_with(&runner_dir, |parts| {
        build_http(&runner_dir, parts, None, "")
    })?;
    Ok(emit(o, json!({"paired": false}), human))
}

pub fn status_data(st: &TrustState, joined: bool, has_config: bool) -> (&'static str, String) {
    let peers = st
        .devices
        .iter()
        .filter(|d| d.state == DeviceState::Trusted && !d.is_self)
        .count();
    match (st.registered, joined && peers > 0, has_config) {
        (false, _, _) | (_, _, false) => {
            ("not_paired", "Not paired. Run: tmgr device pair".to_owned())
        }
        (true, false, true) => (
            "pending",
            "Waiting for a trusted device to approve. Run: tmgr device pair".to_owned(),
        ),
        (true, true, true) => ("paired", "Paired.".to_owned()),
    }
}

pub fn standing<T: Transport, K: KeyStore>(
    dev: &AttachDevice<T, K>,
) -> std::result::Result<(&'static str, String), String> {
    let st = dev.engine.trust_state()?;
    if dev.engine.own_device().is_some() && dev.check_removed() {
        return Ok(("removed", REMOVED_TEXT.to_owned()));
    }
    Ok(status_data(&st, dev.engine.is_joined(), true))
}

pub fn device(o: &Opts) -> Result<i32> {
    let runner_dir =
        tmgr_ipc::dirs::runner_dir().map_err(|e| Error::internal(format!("runner dir: {e}")))?;
    let (data, text) = device_with(&runner_dir, |parts| {
        build_http(&runner_dir, parts, None, "")
    })?;
    Ok(emit(o, data, text))
}

fn device_with<T: Transport, K: KeyStore>(
    runner_dir: &Path,
    make: impl Fn(&Parts) -> std::result::Result<AttachDevice<T, K>, String>,
) -> Result<(serde_json::Value, String)> {
    let not_paired = || {
        let (state, human) = status_data(&TrustState::default(), false, false);
        (json!({"state": state}), human)
    };
    let Some(parts) = load_parts(runner_dir).map_err(internal)? else {
        return Ok(not_paired());
    };
    let lock = lock::acquire(runner_dir).map_err(internal)?;
    if lock.is_none() {
        let removed =
            make(&parts).is_ok_and(|d| d.engine.own_device().is_some() && d.check_removed());
        let (state, human) = if removed {
            ("removed", REMOVED_TEXT.to_owned())
        } else {
            ("in_use", "Paired and in use by tmgr attach.".to_owned())
        };
        return Ok((json!({"state": state, "kind": KIND}), human));
    }
    let dev = match make(&parts) {
        Ok(dev) => dev,
        Err(e) if e == NO_SECURE_STORE => return Ok(not_paired()),
        Err(e) => return Err(internal(e)),
    };
    let st = dev.engine.trust_state().map_err(internal)?;
    let (state, human) = standing(&dev).map_err(internal)?;
    let me = st.devices.iter().find(|d| d.is_self);
    let trusted = st
        .devices
        .iter()
        .filter(|d| d.state == DeviceState::Trusted)
        .count();
    let data = json!({
        "state": state,
        "device_id": me.map(|d| d.id.clone()),
        "label": me.map(|d| d.label.clone()),
        "kind": KIND,
        "trusted_devices": trusted,
    });
    let text = format!(
        "{human}\nkind: {KIND}\nlabel: {}\ndevice: {}\ntrusted devices: {trusted}",
        me.map_or("-", |d| d.label.as_str()),
        me.map_or("-", |d| d.id.as_str()),
    );
    Ok((data, text))
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod world_tests;
