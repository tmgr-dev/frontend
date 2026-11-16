use crate::client::Client;
use std::time::{Duration, Instant};
use tmgr_ipc::api::{SessionInfo, StateKind};
pub use tmgr_session::switch::*;

pub fn stop(client: &mut Client, s: &SessionInfo, target: Target) -> Result<(), SwitchError> {
    if s.state.kind == StateKind::Exited {
        return Ok(());
    }
    let reason = format!("handed to {}", target.agent().name());
    let io = |e: std::io::Error| SwitchError::Internal(e.to_string());
    client
        .kill_with_reason(&s.id, "int", Some(&reason))
        .map_err(io)?;
    if wait_exit(client, &s.id, Duration::from_secs(10))? {
        return Ok(());
    }
    client
        .kill_with_reason(&s.id, "term", Some(&reason))
        .map_err(io)?;
    if wait_exit(client, &s.id, Duration::from_secs(5))? {
        return Ok(());
    }
    Err(SwitchError::Conflict(format!(
        "{} is still running",
        s.name
    )))
}

fn wait_exit(client: &mut Client, id: &str, limit: Duration) -> Result<bool, SwitchError> {
    let end = Instant::now() + limit;
    loop {
        let list = client
            .list()
            .map_err(|e| SwitchError::Internal(e.to_string()))?;
        if list
            .iter()
            .any(|s| s.id == id && s.state.kind == StateKind::Exited)
        {
            return Ok(true);
        }
        if Instant::now() >= end {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
