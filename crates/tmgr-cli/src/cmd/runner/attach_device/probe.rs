use super::AttachDevice;
use std::time::Duration;
use tmgr_chat_relay::engine::{KeyStore, ReceiptState, Transport};
use tmgr_runner::sanitize::sanitize_text;

pub const POLLS: u32 = 12;
pub const POLL_WAIT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    Confirmed,
    Unconfirmed(Vec<String>),
    Skipped(String),
}

pub fn machine_names(ids: &[String], label: impl Fn(&str) -> Option<String>) -> Vec<String> {
    ids.iter()
        .map(|id| {
            let name = label(id)
                .filter(|l| !l.is_empty())
                .unwrap_or_else(|| id.clone());
            sanitize_text(&name).into_owned()
        })
        .collect()
}

pub fn name_runners(names: &[String]) -> String {
    match names.len() {
        1 => format!("Runner on {}", names[0]),
        _ => format!("Runners on {}", names.join(", ")),
    }
}

impl Probe {
    pub fn text(&self) -> Option<String> {
        match self {
            Probe::Confirmed => None,
            Probe::Unconfirmed(names) => Some(format!(
                "{} did not confirm the terminal. Messages to sessions on those machines may not arrive: update tmgr there, then run tmgr device pair again to check.",
                name_runners(names)
            )),
            Probe::Skipped(why) => Some(format!("Could not check which machines accept the terminal: {}.", sanitize_text(why))),
        }
    }
}

pub fn run<T: Transport, K: KeyStore>(dev: &AttachDevice<T, K>, sleep: &dyn Fn(Duration)) -> Probe {
    let _ = dev.poll();
    let sent = match dev.engine.send_probes() {
        Ok(sent) => sent,
        Err(e) => return Probe::Skipped(e),
    };
    let mut pending = sent;
    for n in 0..POLLS {
        if dev.poll().is_ok() {
            for r in dev.engine.take_receipts() {
                if r.state == ReceiptState::Delivered {
                    pending.retain(|(_, probe)| *probe != r.id);
                }
            }
        }
        if pending.is_empty() {
            return Probe::Confirmed;
        }
        if n + 1 < POLLS {
            sleep(POLL_WAIT);
        }
    }
    let ids: Vec<String> = pending.into_iter().map(|(runner, _)| runner).collect();
    Probe::Unconfirmed(machine_names(&ids, |id| {
        dev.cache.runner(id).map(|r| r.machine_label)
    }))
}
