use tmgr_core::error::{Error, Result};
use tmgr_runner::client::Client;

pub fn enable_new(client: &mut Client, id: &str, flag: Option<bool>) -> Result<()> {
    let attach = tmgr_core::config::config_dir()
        .ok()
        .and_then(|d| tmgr_core::config::Config::load(&d).ok())
        .map(|c| c.attach)
        .unwrap_or_default();
    if !flag.or(attach.checkpoints).unwrap_or(false) {
        return Ok(());
    }
    match client.checkpoints_enable(id, attach.checkpoints_keep) {
        Err(e) if flag == Some(true) => Err(Error::usage(format!(
            "the session started, but checkpoints could not be turned on: {e}"
        ))
        .hint("restart the runner so it matches this tmgr version")),
        _ => Ok(()),
    }
}
