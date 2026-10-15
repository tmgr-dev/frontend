//! The typed, versioned contract of the TMGR runner (see `PROTOCOL.md`).
//!
//! Compatibility rules: the major version is `PROTO_VERSION`; a mismatch is a
//! `proto_mismatch` error and the connection is closed. Additive changes (new
//! optional fields, methods, push types) keep the major and add a capability
//! string to `Welcome.caps`. Clients ignore unknown fields and unknown push
//! `type`s; servers answer unknown methods with `unknown_method`.

mod bundle;
mod error;
mod screen;
mod types;
mod worklog;

pub use bundle::*;
pub use error::ErrorCode;
pub use screen::*;
pub use types::*;
pub use worklog::*;

pub use crate::proto::{
    Auth, Challenge, ClientMsg, ErrorBody, Focus, Handshake, Hello, Request, Resize, Response,
    Scroll, ServerMsg, Welcome, PROTO_VERSION,
};

pub mod caps {
    pub const SESSIONS_V1: &str = "sessions.v1";
    pub const ATTACH_V1: &str = "attach.v1";
    pub const RUNS_CREATE_V1: &str = "runs.create.v1";
    pub const TASK_REF_V1: &str = "task_ref.v1";
    pub const LAYOUT_V1: &str = "layout.v1";
    pub const RUN_BUNDLE_V1: &str = "run_bundle.v1";
    pub const CWD_PIN_V1: &str = "cwd_pin.v1";
    pub const LINK_ORIGIN_V1: &str = "link_origin.v1";
    pub const CANDIDATES_V1: &str = "candidates.v1";
    pub const WORKLOG_V1: &str = "worklog.v1";
    pub const RUNS_RESTORE_V1: &str = "runs.restore.v1";
    pub const PERMISSION_MODE_V1: &str = "permission_mode.v1";
    pub const START_KEY_V1: &str = "start_key.v1";
    pub const RUN_STATUS_V1: &str = "run_status.v1";
    pub const CLONE_DIR_V1: &str = "clone_dir.v1";
    pub const SESSION_TEXT_V1: &str = "session_text.v1";
    pub const TRUST_CWD_V1: &str = "trust_cwd.v1";
    pub const AGENT_WORK_V1: &str = "agent_work.v1";
    pub const EXE_PIN_V1: &str = "exe_pin.v1";
    pub const ARGV_PIN_V1: &str = "argv_pin.v1";

    pub const ALL: [&str; 20] = [
        SESSIONS_V1,
        ATTACH_V1,
        RUNS_CREATE_V1,
        TASK_REF_V1,
        LAYOUT_V1,
        RUN_BUNDLE_V1,
        CWD_PIN_V1,
        LINK_ORIGIN_V1,
        CANDIDATES_V1,
        WORKLOG_V1,
        RUNS_RESTORE_V1,
        PERMISSION_MODE_V1,
        START_KEY_V1,
        RUN_STATUS_V1,
        CLONE_DIR_V1,
        SESSION_TEXT_V1,
        TRUST_CWD_V1,
        AGENT_WORK_V1,
        EXE_PIN_V1,
        ARGV_PIN_V1,
    ];
}

pub mod method {
    pub const RUNNER_STATUS: &str = "runner.status";
    pub const RUNNER_STOP: &str = "runner.stop";
    pub const SESSIONS_LIST: &str = "sessions.list";
    pub const SESSIONS_CREATE: &str = "sessions.create";
    pub const SESSIONS_KILL: &str = "sessions.kill";
    pub const SESSIONS_SEND: &str = "sessions.send";
    pub const SESSIONS_REMOVE: &str = "sessions.remove";
    pub const SESSIONS_WORKLOG: &str = "sessions.worklog";
    pub const RUNS_CREATE: &str = "runs.create";
    pub const LAYOUT_GET: &str = "layout.get";
    pub const LAYOUT_SET: &str = "layout.set";
    pub const ATTACH_OPEN: &str = "attach.open";
    pub const ATTACH_SET: &str = "attach.set";
    pub const ORIGIN_REVOKE: &str = "origin.revoke";
    pub const CANDIDATES_LIST: &str = "candidates.list";
    pub const RUNS_RESTORE: &str = "runs.restore";
    pub const RUNS_LOOKUP: &str = "runs.lookup";
    pub const RUNS_STATUS: &str = "runs.status";
    pub const SESSIONS_TEXT: &str = "sessions.text";
}

pub mod limits {
    pub const MAX_COLS: u16 = 500;
    pub const MAX_ROWS: u16 = 200;
    pub const MAX_INPUT: usize = 64 << 10;
    pub const MAX_PUSH: usize = 256 << 10;
    pub const MAX_BUNDLE: usize = 1 << 20;
    pub const MAX_TEXT_LINES: u32 = 10_000;
}

/// The only `Hello.client` value allowed to call `sessions.create`.
pub const CLIENT_CLI: &str = "cli";

/// The only `Hello.client` value allowed to call `runs.create` for a link (and it must).
pub const CLIENT_WORKER: &str = "worker";
