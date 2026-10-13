use serde_json::Value;
use std::io;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;
use tmgr_ipc::api::{
    caps, limits, method, AttachParams, ErrorCode, IdParams, KillParams, OriginRevokeParams,
    RunCreateReq, SendParams, SessionInfo,
};
use tmgr_ipc::frame::{read_frame, write_json, Frame};
use tmgr_ipc::proto::{ClientMsg, Request, Resize, Scroll, ServerMsg};
use tmgr_ipc::{dirs, endpoint, secret, server};

pub const CLIENT_NAME: &str = "worker";
const IO_TIMEOUT: Duration = Duration::from_secs(15);
const ATTACH_QUEUE: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    Unavailable(String),
    NotFound,
    NotRunning,
    Busy,
    Refused { code: String, message: String },
    Protocol(String),
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::Unavailable(why) => write!(f, "runner unavailable: {why}"),
            BackendError::NotFound => f.write_str("no such run"),
            BackendError::NotRunning => f.write_str("the run is not running"),
            BackendError::Busy => f.write_str("the agent is not reading its input"),
            BackendError::Refused { code, message } => {
                write!(f, "runner refused ({code}): {message}")
            }
            BackendError::Protocol(why) => write!(f, "runner protocol error: {why}"),
        }
    }
}

fn unavailable(e: io::Error) -> BackendError {
    BackendError::Unavailable(tmgr_core::text::sanitize_line(&e.to_string()))
}

fn from_code(code: ErrorCode, message: String) -> BackendError {
    match code {
        ErrorCode::NotFound => BackendError::NotFound,
        ErrorCode::NotRunning => BackendError::NotRunning,
        ErrorCode::Busy => BackendError::Busy,
        other => BackendError::Refused {
            code: other.as_str().to_string(),
            message: tmgr_core::text::sanitize_line(&message),
        },
    }
}

pub enum AttachRecv {
    Msg(Box<ServerMsg>),
    Idle,
    Closed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OriginKilled {
    pub killed: u32,
    pub unconfirmed: Vec<String>,
}

pub trait AttachSession: Send {
    fn recv(&mut self, wait: Duration) -> AttachRecv;
    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), BackendError>;
    fn scroll(&mut self, offset: u32) -> Result<(), BackendError>;
    fn detach(&mut self);
}

pub trait Backend: Send {
    fn set_call_timeout(&mut self, _limit: Duration) {}
    fn list(&mut self) -> Result<Vec<SessionInfo>, BackendError>;
    fn create_run(&mut self, req: &RunCreateReq) -> Result<SessionInfo, BackendError>;
    fn kill(&mut self, id: &str, signal: &str) -> Result<(), BackendError>;
    fn send(&mut self, id: &str, text: &str) -> Result<(), BackendError>;
    fn remove(&mut self, id: &str) -> Result<(), BackendError>;
    fn origin_revoke(&mut self, link: &str, generation: u64) -> Result<OriginKilled, BackendError>;
    fn attach(
        &mut self,
        id: &str,
        size: Option<(u16, u16)>,
    ) -> Result<Box<dyn AttachSession>, BackendError>;
}

pub struct RunnerClient {
    dir: PathBuf,
    stream: UnixStream,
    next_id: u64,
}

fn open(dir: &Path, client: &str, required: &[&str]) -> Result<UnixStream, BackendError> {
    dirs::check_private_dir(dir).map_err(unavailable)?;
    let secret = secret::read(&dir.join("client.secret")).map_err(unavailable)?;
    let ep = endpoint::Endpoint::load_runner(dir).map_err(unavailable)?;
    let stream = endpoint::connect(&ep).map_err(unavailable)?;
    let welcome = server::handshake_client(&stream, client, &secret).map_err(unavailable)?;
    for cap in required {
        if !welcome.caps.iter().any(|c| c == cap) {
            return Err(BackendError::Unavailable(format!(
                "the runner lacks the {cap} capability"
            )));
        }
    }
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(unavailable)?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(unavailable)?;
    Ok(stream)
}

impl RunnerClient {
    pub fn connect(dir: &Path) -> Result<RunnerClient, BackendError> {
        RunnerClient::connect_as(dir, CLIENT_NAME)
    }

    pub fn connect_as(dir: &Path, client: &str) -> Result<RunnerClient, BackendError> {
        let stream = open(
            dir,
            client,
            &[
                caps::SESSIONS_V1,
                caps::RUNS_CREATE_V1,
                caps::TASK_REF_V1,
                caps::LINK_ORIGIN_V1,
            ],
        )?;
        Ok(RunnerClient {
            dir: dir.to_path_buf(),
            stream,
            next_id: 1,
        })
    }

    pub fn connect_default() -> Result<RunnerClient, BackendError> {
        let dir = dirs::runner_dir().map_err(unavailable)?;
        RunnerClient::connect(&dir)
    }

    pub fn connect_default_as(client: &str) -> Result<RunnerClient, BackendError> {
        let dir = dirs::runner_dir().map_err(unavailable)?;
        RunnerClient::connect_as(&dir, client)
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, BackendError> {
        call_on(&mut self.stream, &mut self.next_id, method, params)
    }
}

fn call_on(
    stream: &mut UnixStream,
    next_id: &mut u64,
    method: &str,
    params: Value,
) -> Result<Value, BackendError> {
    let id = *next_id;
    *next_id += 1;
    let msg = ClientMsg::Request(Request {
        id,
        method: method.into(),
        params,
    });
    write_json(stream, &msg).map_err(unavailable)?;
    loop {
        let frame = read_frame(stream)
            .map_err(unavailable)?
            .ok_or_else(|| BackendError::Unavailable("the runner closed the connection".into()))?;
        let Frame::Json(v) = frame else {
            return Err(BackendError::Protocol("unexpected binary frame".into()));
        };
        match serde_json::from_value::<ServerMsg>(v)
            .map_err(|e| BackendError::Protocol(e.to_string()))?
        {
            ServerMsg::Response(r) if r.id == id => {
                return match (r.ok, r.err) {
                    (_, Some(e)) => Err(from_code(e.code, e.message)),
                    (Some(v), None) => Ok(v),
                    (None, None) => Ok(Value::Null),
                };
            }
            ServerMsg::Closed { reason } => {
                return Err(BackendError::Unavailable(tmgr_core::text::sanitize_line(
                    &reason,
                )))
            }
            _ => {}
        }
    }
}

fn attach_clean(s: &str) -> String {
    crate::attach::clean_text(&tmgr_core::text::truncate_chars(s, 40).0)
}

fn typed<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, BackendError> {
    serde_json::from_value(v).map_err(|e| BackendError::Protocol(e.to_string()))
}

fn params<T: serde::Serialize>(v: &T) -> Result<Value, BackendError> {
    serde_json::to_value(v).map_err(|e| BackendError::Protocol(e.to_string()))
}

impl Backend for RunnerClient {
    fn set_call_timeout(&mut self, limit: Duration) {
        let limit = limit.max(Duration::from_millis(100));
        let _ = self.stream.set_read_timeout(Some(limit));
        let _ = self.stream.set_write_timeout(Some(limit));
    }

    fn list(&mut self) -> Result<Vec<SessionInfo>, BackendError> {
        typed(self.call(method::SESSIONS_LIST, Value::Null)?)
    }

    fn create_run(&mut self, req: &RunCreateReq) -> Result<SessionInfo, BackendError> {
        typed(self.call(method::RUNS_CREATE, params(req)?)?)
    }

    fn kill(&mut self, id: &str, signal: &str) -> Result<(), BackendError> {
        let p = KillParams {
            id: id.into(),
            signal: signal.into(),
        };
        self.call(method::SESSIONS_KILL, params(&p)?).map(drop)
    }

    fn send(&mut self, id: &str, text: &str) -> Result<(), BackendError> {
        let p = SendParams {
            id: id.into(),
            text: text.into(),
        };
        self.call(method::SESSIONS_SEND, params(&p)?).map(drop)
    }

    fn origin_revoke(&mut self, link: &str, generation: u64) -> Result<OriginKilled, BackendError> {
        let p = OriginRevokeParams {
            link: link.into(),
            generation,
        };
        let v: tmgr_ipc::api::OriginRevoked =
            typed(self.call(method::ORIGIN_REVOKE, params(&p)?)?)?;
        Ok(OriginKilled {
            killed: v.killed,
            unconfirmed: v
                .unconfirmed
                .iter()
                .take(64)
                .map(|id| attach_clean(id))
                .collect(),
        })
    }

    fn remove(&mut self, id: &str) -> Result<(), BackendError> {
        let p = IdParams { id: id.into() };
        self.call(method::SESSIONS_REMOVE, params(&p)?).map(drop)
    }

    fn attach(
        &mut self,
        id: &str,
        size: Option<(u16, u16)>,
    ) -> Result<Box<dyn AttachSession>, BackendError> {
        let mut stream = open(
            &self.dir,
            CLIENT_NAME,
            &[caps::ATTACH_V1, caps::SESSIONS_V1],
        )?;
        let mut next = 1u64;
        let mut p = AttachParams {
            sessions: vec![id.to_string()],
            ..AttachParams::default()
        };
        if let Some(size) = size {
            p.sizes.insert(id.to_string(), size);
        }
        call_on(&mut stream, &mut next, method::ATTACH_OPEN, params(&p)?)?;
        stream.set_read_timeout(None).map_err(unavailable)?;
        let writer = stream.try_clone().map_err(unavailable)?;
        let (tx, rx) = mpsc::sync_channel(ATTACH_QUEUE);
        std::thread::spawn(move || {
            let mut reader = stream;
            loop {
                let msg = match read_frame(&mut reader) {
                    Ok(Some(Frame::Json(v))) => serde_json::from_value::<ServerMsg>(v)
                        .map(Some)
                        .unwrap_or(Some(ServerMsg::Unknown)),
                    Ok(Some(Frame::Bin(_))) | Ok(None) | Err(_) => None,
                };
                let last = msg.is_none();
                if tx.send(msg).is_err() || last {
                    break;
                }
            }
        });
        Ok(Box::new(RunnerAttach {
            session: id.to_string(),
            rx,
            writer,
            closed: false,
        }))
    }
}

struct RunnerAttach {
    session: String,
    rx: Receiver<Option<ServerMsg>>,
    writer: UnixStream,
    closed: bool,
}

impl RunnerAttach {
    fn write(&mut self, msg: &ClientMsg) -> Result<(), BackendError> {
        write_json(&mut self.writer, msg).map_err(unavailable)
    }
}

impl AttachSession for RunnerAttach {
    fn recv(&mut self, wait: Duration) -> AttachRecv {
        if self.closed {
            return AttachRecv::Closed;
        }
        match self.rx.recv_timeout(wait) {
            Ok(Some(msg)) => AttachRecv::Msg(Box::new(msg)),
            Ok(None) | Err(RecvTimeoutError::Disconnected) => {
                self.closed = true;
                AttachRecv::Closed
            }
            Err(RecvTimeoutError::Timeout) => AttachRecv::Idle,
        }
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), BackendError> {
        let msg = ClientMsg::Resize(Resize {
            session: self.session.clone(),
            cols: cols.clamp(1, limits::MAX_COLS),
            rows: rows.clamp(1, limits::MAX_ROWS),
        });
        self.write(&msg)
    }

    fn scroll(&mut self, offset: u32) -> Result<(), BackendError> {
        let msg = ClientMsg::Scroll(Scroll {
            session: self.session.clone(),
            offset,
        });
        self.write(&msg)
    }

    fn detach(&mut self) {
        let _ = self.write(&ClientMsg::Detach);
        let _ = self.writer.shutdown(std::net::Shutdown::Both);
    }
}

impl Drop for RunnerAttach {
    fn drop(&mut self) {
        let _ = self.writer.shutdown(std::net::Shutdown::Both);
    }
}
