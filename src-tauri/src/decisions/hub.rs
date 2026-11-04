use std::path::Path;
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tmgr_ipc::decisions::{drain_spool, CardIn, Event, LineComment};

use super::store::{AnswerIn, CardView, MessageIn, Store};

pub enum Change {
  Created { title: String, body: String },
  Updated,
}

pub type Sink = Box<dyn Fn(&Change, usize) + Send + Sync>;

pub struct Hub {
  store: Mutex<Store>,
  cv: Condvar,
  sink: Sink,
}

fn body_of(card: &CardIn) -> String {
  format!("{} needs you ({})", card.source.agent, card.kind)
}

impl Hub {
  pub fn new(store: Store, sink: Sink) -> Hub {
    Hub { store: Mutex::new(store), cv: Condvar::new(), sink }
  }

  fn lock(&self) -> MutexGuard<'_, Store> {
    self.store.lock().unwrap_or_else(|p| p.into_inner())
  }

  fn changed(&self, change: Change) {
    let count = self.lock().open_count();
    self.cv.notify_all();
    (self.sink)(&change, count);
  }

  pub fn open_count(&self) -> usize {
    self.lock().open_count()
  }

  pub fn create(&self, card: CardIn) -> Result<String, String> {
    let created = self.lock().create(card)?;
    if created.fresh {
      self.changed(Change::Created { title: created.card.title.clone(), body: body_of(&created.card) });
    }
    Ok(created.id)
  }

  pub fn ingest_spool(&self, dir: &Path) -> usize {
    let mut added = 0;
    let _ = drain_spool(dir, |card| {
      if Store::check(&card).is_err() {
        return true;
      }
      match self.lock().create(card) {
        Ok(created) => {
          added += usize::from(created.fresh);
          true
        }
        Err(_) => false,
      }
    });
    if added > 0 {
      self.changed(Change::Updated);
    }
    added
  }

  pub fn wait(&self, id: &str, since: u64, timeout: Duration, final_only: bool) -> Result<Option<Event>, String> {
    let deadline = Instant::now() + timeout;
    let mut guard = self.lock();
    loop {
      if let Some(e) = guard.wait_event(id, since, final_only)? {
        return Ok(Some(e));
      }
      let left = deadline.saturating_duration_since(Instant::now());
      if left.is_zero() {
        return Ok(None);
      }
      guard = self.cv.wait_timeout(guard, left).unwrap_or_else(|p| p.into_inner()).0;
    }
  }

  pub fn reply(&self, id: &str, text: Option<String>, lines: Vec<LineComment>) -> Result<(), String> {
    self.lock().agent_message(id, text, lines)?;
    self.changed(Change::Updated);
    Ok(())
  }

  pub fn close_session(&self, agent: &str, session: &str) -> Result<u64, String> {
    let n = self.lock().close_session(agent, session)?;
    if n > 0 {
      self.changed(Change::Updated);
    }
    Ok(n)
  }

  pub fn answer(&self, id: &str, a: AnswerIn) -> Result<(Event, CardIn), String> {
    let r = self.lock().answer(id, a)?;
    self.changed(Change::Updated);
    Ok(r)
  }

  pub fn message(&self, id: &str, m: MessageIn) -> Result<Event, String> {
    let e = self.lock().owner_message(id, m)?;
    self.changed(Change::Updated);
    Ok(e)
  }

  pub fn get(&self, id: &str) -> Result<CardView, String> {
    self.lock().get(id)?.ok_or_else(|| "unknown card".to_owned())
  }

  pub fn list(&self) -> Result<Vec<CardView>, String> {
    self.lock().list()
  }

  pub fn check_liveness(&self, alive: impl Fn(u32) -> bool) {
    {
      let mut store = self.lock();
      let gone: Vec<String> =
        store.live_pids().unwrap_or_default().into_iter().filter(|(_, pid)| !alive(*pid)).map(|(id, _)| id).collect();
      if gone.is_empty() || store.mark_gone(&gone).is_err() {
        return;
      }
    }
    self.changed(Change::Updated);
  }
}

pub fn pid_alive(pid: u32) -> bool {
  let Ok(pid) = i32::try_from(pid) else { return true };
  if pid <= 0 {
    return true;
  }
  let rc = unsafe { libc::kill(pid, 0) };
  rc == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn pid_probe() {
    assert!(pid_alive(std::process::id()));
    let mut child = std::process::Command::new("true").spawn().unwrap();
    let pid = child.id();
    child.wait().unwrap();
    assert!(!pid_alive(pid));
    assert!(pid_alive(0));
    assert!(pid_alive(u32::MAX));
  }
}
