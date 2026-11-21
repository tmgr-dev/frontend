use super::join_code::read_join_text;
use super::transport::HttpRelayTransport;
use crate::ctx::Ctx;
use crate::interact::is_interactive;
use crate::out::Output;
use serde_json::json;
use std::time::Duration;
use tmgr_chat_relay::engine::{
    ChatSink, EngineConfig, KeyStore, RelayEngine, RoomScope, Transport,
};
use tmgr_core::error::{Error, Result};
use tmgr_ipc::chat::ChatMessage;
use tmgr_runner::chat::relay::store::{RelayConfig, RelayDir};

const POLL_EVERY: Duration = Duration::from_secs(3);
const MAX_POLLS: u32 = 200;
const MAX_FAILS: u32 = 5;
const LABEL: &str = "Runner";
const PENDING: &str = "the runner does not take chat until pairing is done";
const TIMEOUT: &str = "no trusted device approved the join within 10 minutes";

pub struct RefuseSink;

impl ChatSink for RefuseSink {
    fn deliver(&self, _: &str, _: ChatMessage) -> std::result::Result<(), String> {
        Err(PENDING.into())
    }
}

pub trait JoinPort {
    fn peers(&self) -> std::result::Result<usize, String>;
    fn is_joined(&self) -> bool;
    fn join(&self, code: &str) -> std::result::Result<(), String>;
    fn reset_pending_join(&self) -> std::result::Result<(), String>;
    fn poll(&self) -> std::result::Result<(), String>;
}

impl<T: Transport, K: KeyStore, S: ChatSink> JoinPort for RelayEngine<T, K, S> {
    fn peers(&self) -> std::result::Result<usize, String> {
        let st = self.trust_state()?;
        Ok(st
            .devices
            .iter()
            .filter(|d| d.state == tmgr_chat_relay::engine::DeviceState::Trusted && !d.is_self)
            .count())
    }

    fn is_joined(&self) -> bool {
        RelayEngine::is_joined(self)
    }

    fn join(&self, code: &str) -> std::result::Result<(), String> {
        self.join_with_code(code)
    }

    fn reset_pending_join(&self) -> std::result::Result<(), String> {
        RelayEngine::reset_pending_join(self)
    }

    fn poll(&self) -> std::result::Result<(), String> {
        self.poll_once().map(|_| ())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Joined,
    AlreadyJoined,
}

pub fn drive(
    port: &impl JoinPort,
    code: &str,
    sleep: &dyn Fn(Duration),
) -> std::result::Result<Outcome, String> {
    let peers = port.peers()?;
    if peers > 0 && port.is_joined() {
        return Ok(Outcome::AlreadyJoined);
    }
    let fresh = !code.trim().is_empty();
    if peers > 0 && fresh {
        port.reset_pending_join()?;
    }
    if peers == 0 || fresh {
        port.join(code)?;
    }
    let mut fails = 0;
    for _ in 0..MAX_POLLS {
        match port.poll() {
            Ok(()) => fails = 0,
            Err(e) if e == PENDING => fails = 0,
            Err(e) => {
                fails += 1;
                if fails >= MAX_FAILS {
                    return Err(e);
                }
            }
        }
        if port.is_joined() {
            return Ok(Outcome::Joined);
        }
        sleep(POLL_EVERY);
    }
    Err(TIMEOUT.into())
}

pub fn private_runner_dir() -> Result<std::path::PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let dir =
        tmgr_ipc::dirs::runner_dir().map_err(|e| Error::internal(format!("runner dir: {e}")))?;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .map_err(|e| Error::internal(format!("runner dir: {e}")))?;
    Ok(dir)
}

pub fn run(ctx: &Ctx) -> Result<Output> {
    let client = ctx.cloud_client()?;
    let user_id = client
        .current_user_id()?
        .and_then(|id| u64::try_from(id).ok())
        .ok_or_else(|| Error::auth("cannot tell which user is signed in").hint("tmgr login"))?;
    let tokens = client
        .access_only_tokens()
        .ok_or_else(Error::not_logged_in)?;
    let server = ctx.server()?;
    let code = read_join_text(std::io::stdin().lock(), is_interactive(), || {
        rpassword::prompt_password("Join code: ")
    })?;
    let dir = private_runner_dir()?;
    let machine = tmgr_runner::chat::machine_id(&dir)
        .map_err(|e| Error::internal(format!("machine id: {e}")))?;
    let transport = HttpRelayTransport {
        base_url: server.clone(),
        login_token: Some(tokens.access.expose().to_owned()),
    };
    let relay_dir = RelayDir::open(&dir).map_err(Error::internal)?;
    let cfg = EngineConfig {
        user_id,
        label: LABEL.into(),
        local_machine: machine,
    };
    let engine = RelayEngine::new(transport, relay_dir, RefuseSink, cfg)
        .map_err(Error::internal)?
        .with_kind("runner")
        .with_room_scope(RoomScope::Local);
    let outcome = drive(&engine, &code, &|d| std::thread::sleep(d)).map_err(|e| {
        Error::internal(e).hint("run tmgr chat join again with a fresh code if it cannot recover")
    })?;
    RelayDir::open(&dir)
        .and_then(|d| d.save_config(&RelayConfig { server, user_id }))
        .map_err(Error::internal)?;
    let human = match outcome {
        Outcome::Joined => "Paired. The runner starts relaying chat within a few seconds.",
        Outcome::AlreadyJoined => "Already paired.",
    };
    Ok(Output::new(json!({"joined": true}), human))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    struct Mock {
        peers: Cell<usize>,
        joined: Cell<bool>,
        join_calls: RefCell<Vec<String>>,
        resets: Cell<usize>,
        join_result: std::result::Result<(), String>,
        polls: RefCell<Vec<std::result::Result<(), String>>>,
        join_ok_after: Cell<Option<usize>>,
    }

    impl Mock {
        fn new(peers: usize, joined: bool, polls: Vec<std::result::Result<(), String>>) -> Mock {
            Mock {
                peers: Cell::new(peers),
                joined: Cell::new(joined),
                join_calls: RefCell::new(Vec::new()),
                resets: Cell::new(0),
                join_result: Ok(()),
                polls: RefCell::new(polls),
                join_ok_after: Cell::new(None),
            }
        }
    }

    impl JoinPort for Mock {
        fn peers(&self) -> std::result::Result<usize, String> {
            Ok(self.peers.get())
        }
        fn is_joined(&self) -> bool {
            self.joined.get()
        }
        fn join(&self, code: &str) -> std::result::Result<(), String> {
            self.join_calls.borrow_mut().push(code.to_owned());
            self.peers.set(1);
            self.joined.set(false);
            self.join_result.clone()
        }
        fn reset_pending_join(&self) -> std::result::Result<(), String> {
            self.resets.set(self.resets.get() + 1);
            self.peers.set(0);
            Ok(())
        }
        fn poll(&self) -> std::result::Result<(), String> {
            let mut q = self.polls.borrow_mut();
            let r = if q.is_empty() { Ok(()) } else { q.remove(0) };
            if self.join_ok_after.get().is_some_and(|n| q.len() <= n) {
                self.joined.set(true);
            }
            r
        }
    }

    fn no_sleep(_: Duration) {}

    #[test]
    fn registers_joins_and_waits_for_approval() {
        let m = Mock::new(0, true, vec![Ok(()), Ok(()), Ok(())]);
        m.join_ok_after.set(Some(1));
        assert_eq!(drive(&m, "code", &no_sleep), Ok(Outcome::Joined));
        assert_eq!(*m.join_calls.borrow(), ["code"]);
    }

    #[test]
    fn already_paired_does_nothing() {
        let m = Mock::new(1, true, vec![]);
        assert_eq!(drive(&m, "code", &no_sleep), Ok(Outcome::AlreadyJoined));
        assert!(m.join_calls.borrow().is_empty());
    }

    #[test]
    fn an_unfinished_join_is_resumed_without_posting_again_when_no_code_is_given() {
        let m = Mock::new(1, false, vec![Ok(())]);
        m.join_ok_after.set(Some(0));
        assert_eq!(drive(&m, "", &no_sleep), Ok(Outcome::Joined));
        assert!(m.join_calls.borrow().is_empty());
        assert_eq!(m.resets.get(), 0);
    }

    #[test]
    fn a_timed_out_join_followed_by_a_new_code_posts_again() {
        let m = Mock::new(1, false, vec![]);
        assert_eq!(drive(&m, "old", &no_sleep), Err(TIMEOUT.into()));
        m.join_ok_after.set(Some(0));
        m.polls.borrow_mut().push(Ok(()));
        assert_eq!(drive(&m, "new", &no_sleep), Ok(Outcome::Joined));
        assert_eq!(m.resets.get(), 2);
        assert_eq!(*m.join_calls.borrow(), ["old", "new"]);
    }

    #[test]
    fn join_errors_are_returned() {
        let mut m = Mock::new(0, true, vec![]);
        m.join_result = Err("the join code is not valid".into());
        assert_eq!(
            drive(&m, "x", &no_sleep),
            Err("the join code is not valid".into())
        );
    }

    #[test]
    fn repeated_poll_failures_stop_early_but_the_pending_sink_error_does_not() {
        let m = Mock::new(
            1,
            false,
            vec![Err("relay unreachable".into()); MAX_FAILS as usize],
        );
        assert_eq!(drive(&m, "c", &no_sleep), Err("relay unreachable".into()));
        let m = Mock::new(1, false, vec![Err(PENDING.into()); 20]);
        m.join_ok_after.set(Some(5));
        assert_eq!(drive(&m, "c", &no_sleep), Ok(Outcome::Joined));
    }

    #[test]
    fn waiting_is_bounded() {
        let m = Mock::new(1, false, vec![]);
        let sleeps = Cell::new(0);
        let r = drive(&m, "c", &|_| sleeps.set(sleeps.get() + 1));
        assert_eq!(r, Err(TIMEOUT.into()));
        assert_eq!(sleeps.get(), MAX_POLLS);
    }
}
