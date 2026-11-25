mod store;
mod transport;

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tmgr_chat_relay::engine::{
    blob, ChatSink, DeviceState, EngineConfig, ReceiptSink, RelayEngine, RoomScope,
};
use tmgr_ipc::chat::{
    new_message_id, Attachment, Author, AuthorKind, ChatMessage, MessageKind, RoomKind, RoomRef,
};
use transport::HttpTransport;

type Engine = RelayEngine<HttpTransport, store::FileKeyStore, Printer>;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    state_dir: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Register {
        #[arg(long)]
        api: String,
        #[arg(long)]
        token: String,
        #[arg(long, default_value = "desktop")]
        kind: String,
        #[arg(long, default_value = "E2E Device")]
        name: String,
    },
    Join {
        #[arg(long)]
        api: String,
        #[arg(long)]
        token: String,
        #[arg(long, default_value = "mobile")]
        kind: String,
        #[arg(long, default_value = "E2E Phone")]
        name: String,
        #[arg(long)]
        code: String,
        #[arg(long, default_value_t = 1000)]
        poll_ms: u64,
        #[arg(long, default_value_t = 600)]
        timeout_s: u64,
    },
    PairRunner {
        #[arg(long, default_value = "runner")]
        expect: String,
        #[arg(long, default_value_t = 1000)]
        poll_ms: u64,
        #[arg(long, default_value_t = 600)]
        timeout_s: u64,
    },
    Send {
        #[arg(long, default_value = "auto")]
        room: String,
        #[arg(long)]
        text: String,
        #[arg(long, default_value_t = 1)]
        count: u32,
        #[arg(long, default_value_t = 0)]
        interval_ms: u64,
        #[arg(long, requires = "runner_device")]
        image: Option<PathBuf>,
        #[arg(long)]
        runner_device: Option<String>,
    },
    Listen {
        #[arg(long, default_value_t = 1000)]
        poll_ms: u64,
    },
}

#[derive(Serialize, Deserialize)]
struct Config {
    api: String,
    token: String,
    user_id: u64,
    kind: String,
    name: String,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

pub fn emit(mut v: Value) {
    use std::io::Write;
    v["t_ms"] = now_ms().into();
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

struct Printer {
    dir: PathBuf,
}

impl ChatSink for Printer {
    fn deliver(&self, from: &str, from_kind: Option<&str>, msg: ChatMessage) -> Result<(), String> {
        let _ = std::fs::write(self.dir.join("machine"), &msg.room.machine);
        let _ = std::fs::write(
            self.dir.join("last_room"),
            serde_json::to_string(&msg.room).unwrap_or_default(),
        );
        emit(json!({
            "event": "message",
            "from": from,
            "from_kind": from_kind,
            "room": msg.room,
            "author": msg.author,
            "kind": msg.kind,
            "text": msg.body_md,
            "msg_id": msg.id,
            "sent_ts_ms": msg.ts_ms,
        }));
        Ok(())
    }

    fn dropped(&self, reason: &'static str) {
        emit(json!({"event": "dropped", "reason": reason}));
    }
}

impl ReceiptSink for Printer {
    fn deliver_receipt(&self, from: &str, id: &str, state: &str, reason: Option<&str>) {
        emit(
            json!({"event": "receipt", "from": from, "msg_id": id, "state": state, "reason": reason}),
        );
    }
}

struct Ctx {
    dir: PathBuf,
    cfg: Config,
    _lock: File,
}

impl Ctx {
    fn open(dir: &Path) -> Result<Ctx, String> {
        let cfg: Config = serde_json::from_slice(
            &std::fs::read(dir.join("config.json")).map_err(|e| format!("config: {e}"))?,
        )
        .map_err(|e| format!("config: {e}"))?;
        let lock = lock(dir)?;
        Ok(Ctx {
            dir: dir.to_owned(),
            cfg,
            _lock: lock,
        })
    }

    fn engine(&self) -> Result<Engine, String> {
        build(&self.dir, &self.cfg)
    }
}

fn lock(dir: &Path) -> Result<File, String> {
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("lock"))
        .map_err(|e| e.to_string())?;
    f.lock().map_err(|e| e.to_string())?;
    Ok(f)
}

fn build(dir: &Path, cfg: &Config) -> Result<Engine, String> {
    let sink = Printer {
        dir: dir.to_owned(),
    };
    let receipts = Arc::new(Printer {
        dir: dir.to_owned(),
    });
    let eng = RelayEngine::new(
        HttpTransport::new(&cfg.api, &cfg.token),
        store::FileKeyStore::open(dir)?,
        sink,
        EngineConfig {
            user_id: cfg.user_id,
            label: cfg.name.clone(),
            local_machine: "e2e-device".into(),
        },
    )?
    .with_kind(&cfg.kind)
    .with_room_scope(RoomScope::Remote)
    .with_receipt_sink(receipts);
    Ok(eng)
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        emit(json!({"event": "error", "error": e}));
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), String> {
    store::private_dir(&cli.state_dir)?;
    match cli.cmd {
        Cmd::Register {
            api,
            token,
            kind,
            name,
        } => register(&cli.state_dir, api, token, kind, name),
        Cmd::Join {
            api,
            token,
            kind,
            name,
            code,
            poll_ms,
            timeout_s,
        } => join(
            &cli.state_dir,
            api,
            token,
            kind,
            name,
            &code,
            poll_ms,
            timeout_s,
        ),
        Cmd::PairRunner {
            expect,
            poll_ms,
            timeout_s,
        } => pair_runner(&cli.state_dir, &expect, poll_ms, timeout_s),
        Cmd::Send {
            room,
            text,
            count,
            interval_ms,
            image,
            runner_device,
        } => send(
            &cli.state_dir,
            &room,
            &text,
            count,
            interval_ms,
            image.as_deref().zip(runner_device.as_deref()),
        ),
        Cmd::Listen { poll_ms } => listen(&cli.state_dir, poll_ms),
    }
}

fn register(
    dir: &Path,
    api: String,
    token: String,
    kind: String,
    name: String,
) -> Result<(), String> {
    let _lock = lock(dir)?;
    let user_id = transport::current_user_id(&api, &token)?;
    let cfg = Config {
        api,
        token,
        user_id,
        kind,
        name,
    };
    store::write_private(
        &dir.join("config.json"),
        &serde_json::to_string(&cfg).map_err(|e| e.to_string())?,
    )?;
    let eng = build(dir, &cfg)?;
    eng.register()?;
    let (id, fp) = eng.own_device().ok_or("no device identity")?;
    emit(
        json!({"event": "registered", "device_id": id, "fp": fp, "kind": cfg.kind, "user_id": user_id}),
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn join(
    dir: &Path,
    api: String,
    token: String,
    kind: String,
    name: String,
    code: &str,
    poll_ms: u64,
    timeout_s: u64,
) -> Result<(), String> {
    let user_id = transport::current_user_id(&api, &token)?;
    let cfg = Config {
        api,
        token,
        user_id,
        kind,
        name,
    };
    store::write_private(
        &dir.join("config.json"),
        &serde_json::to_string(&cfg).map_err(|e| e.to_string())?,
    )?;
    {
        let _l = lock(dir)?;
        build(dir, &cfg)?.join_with_code(code)?;
    }
    emit(json!({"event": "join_requested"}));
    let started = std::time::Instant::now();
    while started.elapsed() < Duration::from_secs(timeout_s) {
        {
            let ctx = Ctx::open(dir)?;
            let eng = ctx.engine()?;
            eng.poll_once()?;
            if eng.is_joined() {
                let (id, _) = eng.own_device().ok_or("no device identity")?;
                emit(json!({"event": "joined", "device_id": id, "kind": cfg.kind}));
                return Ok(());
            }
        }
        std::thread::sleep(Duration::from_millis(poll_ms));
    }
    Err("timed out waiting for approval".into())
}

fn pair_runner(dir: &Path, expect: &str, poll_ms: u64, timeout_s: u64) -> Result<(), String> {
    let code = {
        let ctx = Ctx::open(dir)?;
        ctx.engine()?.open_join_code()?
    };
    emit(json!({"event": "join_code", "code": code}));
    let started = std::time::Instant::now();
    while started.elapsed() < Duration::from_secs(timeout_s) {
        {
            let ctx = Ctx::open(dir)?;
            let eng = ctx.engine()?;
            eng.poll_once()?;
            let st = eng.trust_state()?;
            for j in &st.pending_joins {
                emit(
                    json!({"event": "join_request", "device_id": j.device_id, "label": j.label, "kind": j.kind}),
                );
                eng.decide_join(&j.device_id, true)?;
                emit(json!({"event": "join_approved", "device_id": j.device_id}));
            }
            let st = eng.trust_state()?;
            if let Some(d) = st
                .devices
                .iter()
                .find(|d| d.state == DeviceState::Trusted && !d.is_self && d.kind == expect)
            {
                emit(json!({"event": "peer_trusted", "device_id": d.id}));
                return Ok(());
            }
        }
        std::thread::sleep(Duration::from_millis(poll_ms));
    }
    Err("timed out waiting for the runner to join".into())
}

fn parse_room(dir: &Path, room: &str) -> Result<RoomRef, String> {
    let parts: Vec<&str> = room.split(':').collect();
    match parts.as_slice() {
        ["auto"] => {
            if let Ok(raw) = std::fs::read_to_string(dir.join("last_room")) {
                return serde_json::from_str(&raw).map_err(|e| e.to_string());
            }
            let machine = std::fs::read_to_string(dir.join("machine"))
                .map_err(|_| "room auto: no runner message seen yet, pass manager:<machine> or session:<machine>:<id>")?;
            Ok(RoomRef::manager(machine.trim()))
        }
        ["manager", m] => Ok(RoomRef::manager(m)),
        ["session", m, s] => Ok(RoomRef::session(m, s)),
        _ => Err("room must be auto, manager:<machine> or session:<machine>:<id>".into()),
    }
}

fn send_image(eng: &Engine, path: &Path, runner_device: &str) -> Result<Attachment, String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).map_err(|e| format!("image: {e}"))?;
    let sha256: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let size = bytes.len() as u64;
    let parts = blob::parts_for(size) as u32;
    for (n, data) in bytes.chunks(blob::PART_BYTES).enumerate() {
        eng.send_blob(
            runner_device,
            &blob::BlobPart {
                sha256: sha256.clone(),
                size,
                part: n as u32,
                parts,
                data: data.to_vec(),
            },
        )?;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("image")
        .to_owned();
    let mime = match path.extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    };
    emit(json!({"event": "blob_sent", "sha256": sha256, "size": size, "parts": parts}));
    Ok(Attachment {
        name,
        mime: mime.into(),
        size,
        sha256,
    })
}

fn send(
    dir: &Path,
    room: &str,
    text: &str,
    count: u32,
    interval_ms: u64,
    image: Option<(&Path, &str)>,
) -> Result<(), String> {
    let room = parse_room(dir, room)?;
    let _ = RoomKind::Manager;
    for n in 0..count {
        let ctx = Ctx::open(dir)?;
        let eng = ctx.engine()?;
        let (device, _) = eng.own_device().ok_or("device is not registered")?;
        let attachments = match image {
            Some((path, runner)) => vec![send_image(&eng, path, runner)?],
            None => vec![],
        };
        let id = new_message_id();
        let ts = now_ms();
        let msg = ChatMessage {
            poll: None,
            v: 1,
            id: id.clone(),
            room: room.clone(),
            author: Author {
                kind: AuthorKind::Owner,
                id: "owner".into(),
                machine: None,
                device: Some(device),
                agent: None,
            },
            ts_ms: ts,
            kind: MessageKind::Text,
            body_md: if count > 1 {
                format!("{text} #{}", n + 1)
            } else {
                text.to_owned()
            },
            attachments,
            reply_to: None,
            decision: None,
            session_meta: None,
            user_turn: false,
            mention: None,
        };
        emit(json!({"event": "post_start", "msg_id": id, "post_start_ms": now_ms()}));
        let res = eng.send_chat(&msg, true);
        let finished = now_ms();
        match res {
            Ok(()) => emit(
                json!({"event": "post_done", "msg_id": id, "post_done_ms": finished, "ts_ms_field": ts}),
            ),
            Err(e) => {
                emit(json!({"event": "post_failed", "msg_id": id, "error": e}));
                return Err(e);
            }
        }
        drop(eng);
        drop(ctx);
        if interval_ms > 0 && n + 1 < count {
            std::thread::sleep(Duration::from_millis(interval_ms));
        }
    }
    Ok(())
}

fn listen(dir: &Path, poll_ms: u64) -> Result<(), String> {
    emit(json!({"event": "listening", "poll_ms": poll_ms}));
    loop {
        {
            let ctx = Ctx::open(dir)?;
            if let Err(e) = ctx.engine()?.poll_once() {
                emit(json!({"event": "poll_error", "error": e}));
            }
        }
        std::thread::sleep(Duration::from_millis(poll_ms));
    }
}
