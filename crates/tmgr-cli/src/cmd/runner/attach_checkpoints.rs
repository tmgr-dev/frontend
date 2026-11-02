use super::checkpoints::enable_new;
use super::connect;
use tmgr_core::config::AttachConfig;
use tmgr_ipc::api::DiffAgainst;
use tmgr_runner::attach::{checkpoints_error, CpAction, CpFetch, CpMsg};
use tmgr_runner::daemon::RunnerConfig;

pub(super) fn default_on(attach: &AttachConfig) -> bool {
    attach.checkpoints.unwrap_or(false)
}

pub(super) fn form_default() -> bool {
    tmgr_core::config::config_dir()
        .ok()
        .and_then(|d| tmgr_core::config::Config::load(&d).ok())
        .is_some_and(|c| default_on(&c.attach))
}

fn open(cfg: &RunnerConfig) -> Result<tmgr_runner::client::Client, String> {
    connect(cfg).map_err(|e| checkpoints_error(&e.to_string()))
}

pub(super) fn fetch(cfg: &RunnerConfig, job: CpFetch) -> CpMsg {
    match job {
        CpFetch::List(id) => {
            let res = open(cfg).and_then(|mut c| {
                c.checkpoints_list(&id)
                    .map_err(|e| checkpoints_error(&e.to_string()))
            });
            CpMsg::List(id, res)
        }
        CpFetch::Diff(id, n) => {
            let res = open(cfg).and_then(|mut c| {
                c.checkpoints_diff(&id, n, DiffAgainst::Previous)
                    .map_err(|e| checkpoints_error(&e.to_string()))
            });
            CpMsg::Diff(id, n, res)
        }
    }
}

pub(super) fn act(cfg: &RunnerConfig, action: CpAction) -> CpMsg {
    match action {
        CpAction::Check(id, n) => {
            let res = open(cfg).and_then(|mut c| {
                c.checkpoints_diff(&id, n, DiffAgainst::Current)
                    .map_err(|e| checkpoints_error(&e.to_string()))
            });
            CpMsg::Check(id, n, res)
        }
        CpAction::Restore(id, n) => {
            let res = open(cfg).and_then(|mut c| {
                c.checkpoints_restore(&id, n)
                    .map_err(|e| checkpoints_error(&e.to_string()))
            });
            CpMsg::Restored(id, n, res)
        }
    }
}

pub(super) fn enable(cfg: &RunnerConfig, id: &str, flag: Option<bool>) -> Option<String> {
    let mut client = connect(cfg).ok()?;
    enable_new(&mut client, id, flag).unwrap_or_else(|e| Some(checkpoints_error(&e.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_form_default_follows_the_config_and_is_off_when_unset() {
        let mut attach = AttachConfig::default();
        assert!(!default_on(&attach));
        attach.checkpoints = Some(true);
        assert!(default_on(&attach));
        attach.checkpoints = Some(false);
        assert!(!default_on(&attach));
    }
}
