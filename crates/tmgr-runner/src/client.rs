use crate::screen::{Line, Snapshot};
use crate::wire::{CreateReq, EventMsg, ScreenPush, SessionInfo, StatusInfo, MAX_INPUT};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tmgr_ipc::endpoint::{self, Endpoint};
use tmgr_ipc::frame::{read_frame, write_frame, write_json, BinFrame, Frame, TAG_INPUT};
use tmgr_ipc::proto::{ClientMsg, Focus, Request, Resize, Scroll, ServerMsg};
use tmgr_ipc::{secret, server};

fn bad(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

fn from_value<T: serde::de::DeserializeOwned>(v: Value) -> io::Result<T> {
    serde_json::from_value(v).map_err(|e| bad(format!("bad reply: {e}")))
}

pub struct Client {
    stream: UnixStream,
    next_id: u64,
}

impl Client {
    pub fn connect(dir: &Path) -> io::Result<Client> {
        let secret = secret::read(&dir.join("client.secret"))?;
        Client::connect_with(dir, &secret)
    }

    pub fn connect_with(dir: &Path, secret: &str) -> io::Result<Client> {
        let ep = Endpoint::load_runner(dir)?;
        let stream = endpoint::connect(&ep)?;
        server::handshake_client(&stream, "cli", secret)?;
        Ok(Client { stream, next_id: 1 })
    }

    pub fn call(&mut self, method: &str, params: Value) -> io::Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let msg = ClientMsg::Request(Request {
            id,
            method: method.into(),
            params,
        });
        write_json(&mut &self.stream, &msg)?;
        loop {
            let Some(frame) = read_frame(&mut &self.stream)? else {
                return Err(io::ErrorKind::UnexpectedEof.into());
            };
            let Frame::Json(v) = frame else {
                return Err(bad("unexpected binary frame"));
            };
            match from_value::<ServerMsg>(v)? {
                ServerMsg::Response(r) if r.id == id => {
                    return match (r.ok, r.err) {
                        (_, Some(e)) => Err(io::Error::other(format!("{}: {}", e.code, e.message))),
                        (Some(v), None) => Ok(v),
                        (None, None) => Ok(Value::Null),
                    };
                }
                ServerMsg::Closed { reason } => {
                    return Err(io::Error::new(io::ErrorKind::ConnectionAborted, reason))
                }
                _ => {}
            }
        }
    }

    pub fn status(&mut self) -> io::Result<StatusInfo> {
        from_value(self.call("runner.status", Value::Null)?)
    }

    pub fn list(&mut self) -> io::Result<Vec<SessionInfo>> {
        from_value(self.call("sessions.list", Value::Null)?)
    }

    pub fn create(&mut self, req: CreateReq) -> io::Result<SessionInfo> {
        let params = serde_json::to_value(req).map_err(io::Error::other)?;
        from_value(self.call("sessions.create", params)?)
    }

    pub fn kill(&mut self, id: &str, signal: &str) -> io::Result<()> {
        self.call("sessions.kill", json!({"id": id, "signal": signal}))
            .map(drop)
    }

    pub fn send(&mut self, id: &str, text: &str) -> io::Result<()> {
        self.call("sessions.send", json!({"id": id, "text": text}))
            .map(drop)
    }

    pub fn remove(&mut self, id: &str) -> io::Result<()> {
        self.call("sessions.remove", json!({"id": id})).map(drop)
    }

    pub fn stop(&mut self) -> io::Result<()> {
        self.call("runner.stop", Value::Null).map(drop)
    }

    pub fn layout_get(&mut self) -> io::Result<Value> {
        self.call("layout.get", Value::Null)
    }

    pub fn layout_set(&mut self, layout: Value) -> io::Result<()> {
        self.call("layout.set", json!({"layout": layout})).map(drop)
    }

    /// Turns this connection into an attach stream; open another `Client` for further calls.
    pub fn attach(mut self, ids: &[String]) -> io::Result<AttachStream> {
        self.call("attach.open", json!({"sessions": ids}))?;
        let writer = AttachWriter {
            inner: Arc::new(Mutex::new(WriterState {
                stream: self.stream.try_clone()?,
                next_id: 1 << 32,
            })),
        };
        Ok(AttachStream {
            reader: AttachReader {
                stream: self.stream,
            },
            writer,
        })
    }
}

pub struct AttachReader {
    stream: UnixStream,
}

impl AttachReader {
    pub fn recv(&mut self) -> io::Result<Option<ServerMsg>> {
        match read_frame(&mut &self.stream)? {
            None => Ok(None),
            Some(Frame::Json(v)) => from_value(v).map(Some),
            Some(Frame::Bin(_)) => Err(bad("unexpected binary frame")),
        }
    }
}

struct WriterState {
    stream: UnixStream,
    next_id: u64,
}

#[derive(Clone)]
pub struct AttachWriter {
    inner: Arc<Mutex<WriterState>>,
}

impl AttachWriter {
    fn with<R>(&self, f: impl FnOnce(&mut WriterState) -> io::Result<R>) -> io::Result<R> {
        let mut st = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut st)
    }

    fn msg(&self, msg: ClientMsg) -> io::Result<()> {
        self.with(|st| write_json(&mut st.stream, &msg))
    }

    pub fn input(&self, id: &str, bytes: &[u8]) -> io::Result<()> {
        self.with(|st| {
            for chunk in bytes.chunks(MAX_INPUT) {
                let frame = Frame::Bin(BinFrame {
                    tag: TAG_INPUT,
                    session: id.into(),
                    data: chunk.to_vec(),
                });
                write_frame(&mut st.stream, &frame)?;
            }
            Ok(())
        })
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> io::Result<()> {
        self.msg(ClientMsg::Resize(Resize {
            session: id.into(),
            cols,
            rows,
        }))
    }

    pub fn scroll(&self, id: &str, offset: u32) -> io::Result<()> {
        self.msg(ClientMsg::Scroll(Scroll {
            session: id.into(),
            offset,
        }))
    }

    pub fn focus(&self, id: &str) -> io::Result<()> {
        self.msg(ClientMsg::Focus(Focus { session: id.into() }))
    }

    pub fn set_sessions(&self, ids: &[String]) -> io::Result<()> {
        self.with(|st| {
            st.next_id += 1;
            let msg = ClientMsg::Request(Request {
                id: st.next_id,
                method: "attach.set".into(),
                params: json!({"sessions": ids}),
            });
            write_json(&mut st.stream, &msg)
        })
    }

    pub fn detach(&self) -> io::Result<()> {
        self.msg(ClientMsg::Detach)
    }
}

pub struct AttachStream {
    reader: AttachReader,
    writer: AttachWriter,
}

impl AttachStream {
    pub fn split(self) -> (AttachReader, AttachWriter) {
        (self.reader, self.writer)
    }

    pub fn recv(&mut self) -> io::Result<Option<ServerMsg>> {
        self.reader.recv()
    }

    pub fn writer(&self) -> AttachWriter {
        self.writer.clone()
    }

    pub fn input(&self, id: &str, bytes: &[u8]) -> io::Result<()> {
        self.writer.input(id, bytes)
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> io::Result<()> {
        self.writer.resize(id, cols, rows)
    }

    pub fn scroll(&self, id: &str, offset: u32) -> io::Result<()> {
        self.writer.scroll(id, offset)
    }

    pub fn set_sessions(&self, ids: &[String]) -> io::Result<()> {
        self.writer.set_sessions(ids)
    }

    pub fn detach(&self) -> io::Result<()> {
        self.writer.detach()
    }
}

pub fn parse_event(msg: &ServerMsg) -> Option<EventMsg> {
    match msg {
        ServerMsg::Event(v) => serde_json::from_value(v.clone()).ok(),
        _ => None,
    }
}

#[derive(Default)]
pub struct ScreenCache {
    screens: HashMap<String, Snapshot>,
}

impl ScreenCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies a `Screen` push; returns the session it changed.
    pub fn apply(&mut self, msg: &ServerMsg) -> Option<String> {
        let ServerMsg::Screen(v) = msg else {
            return None;
        };
        let push: ScreenPush = serde_json::from_value(v.clone()).ok()?;
        if push.full {
            let mut snap = push.snapshot?;
            let rows = snap.rows as usize;
            if snap.lines.len() < rows {
                snap.lines.resize(rows, Line { spans: Vec::new() });
            }
            self.screens.insert(push.session.clone(), snap);
            return Some(push.session);
        }
        let snap = self.screens.get_mut(&push.session)?;
        for (i, line) in push.lines {
            if i >= snap.lines.len() {
                snap.lines.resize(i + 1, Line { spans: Vec::new() });
            }
            snap.lines[i] = line;
        }
        snap.cursor = push.cursor;
        if let Some(m) = push.modes {
            snap.modes = m;
        }
        snap.scroll_offset = push.scroll_offset;
        snap.scroll_max = push.scroll_max;
        Some(push.session)
    }

    pub fn snapshot(&self, id: &str) -> Option<&Snapshot> {
        self.screens.get(id)
    }

    pub fn remove(&mut self, id: &str) {
        self.screens.remove(id);
    }
}
