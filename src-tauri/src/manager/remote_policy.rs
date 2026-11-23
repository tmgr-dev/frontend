use serde_json::{json, Value};
use tmgr_ipc::api::{method, RemotePolicyInfo};

use super::local::ChatCall;
use super::outdated;

fn read(result: Value) -> Result<RemotePolicyInfo, String> {
  serde_json::from_value(result).map_err(|e| format!("the runner answered with an unexpected policy: {e}"))
}

pub fn get(call: &dyn ChatCall) -> Result<RemotePolicyInfo, String> {
  read(call.call(method::MANAGER_REMOTE_POLICY, json!({})).map_err(outdated)?)
}

pub fn set_ask(call: &dyn ChatCall, leader: &str, ask: bool) -> Result<RemotePolicyInfo, String> {
  if !tmgr_ipc::mgr_rpc::valid_machine_id(leader) {
    return Err("invalid machine id".to_owned());
  }
  if ask {
    return Err(format!("turn asks on from a terminal: tmgr chat remote allow-ask {leader} on"));
  }
  read(call.call(method::MANAGER_REMOTE_POLICY_SET, json!({ "leader": leader, "ask": ask })).map_err(outdated)?)
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::Mutex;

  const A: &str = "mx_0123456789abcdef";

  struct Fake {
    log: Mutex<Vec<(String, Value)>>,
    reply: Result<Value, String>,
  }

  impl ChatCall for Fake {
    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
      self.log.lock().unwrap().push((method.into(), params));
      self.reply.clone()
    }
  }

  fn fake(reply: Result<Value, String>) -> Fake {
    Fake { log: Mutex::new(Vec::new()), reply }
  }

  fn info() -> Value {
    json!({ "leaders": [{ "machine": A, "ask": true, "ops": ["list", "ask"] }], "legacy": false })
  }

  #[test]
  fn get_reads_the_policy_through_the_read_method() {
    let f = fake(Ok(info()));
    let out = get(&f).unwrap();
    assert_eq!(out.leaders[0].machine, A);
    assert!(out.leaders[0].ask);
    assert_eq!(f.log.lock().unwrap()[0].0, "manager.remote_policy");
  }

  #[test]
  fn set_sends_leader_and_ask_to_the_set_method() {
    let f = fake(Ok(info()));
    set_ask(&f, A, false).unwrap();
    let log = f.log.lock().unwrap();
    assert_eq!(log[0].0, "manager.remote_policy_set");
    assert_eq!(log[0].1, json!({ "leader": A, "ask": false }));
  }

  #[test]
  fn turning_asks_on_never_reaches_the_runner() {
    let f = fake(Ok(info()));
    let e = set_ask(&f, A, true).unwrap_err();
    assert!(e.contains("tmgr chat remote allow-ask"), "{e}");
    assert!(f.log.lock().unwrap().is_empty());
  }

  #[test]
  fn a_bad_machine_id_never_reaches_the_runner() {
    let f = fake(Ok(info()));
    assert_eq!(set_ask(&f, "mx_zz", true).unwrap_err(), "invalid machine id");
    assert!(f.log.lock().unwrap().is_empty());
  }

  #[test]
  fn an_old_runner_is_reported_as_outdated_and_refusals_pass_through() {
    assert_eq!(get(&fake(Err("unknown method: manager.remote_policy".into()))).unwrap_err(), "runner_outdated");
    let e = set_ask(&fake(Err("forbidden: inside a session".into())), A, false).unwrap_err();
    assert_eq!(e, "forbidden: inside a session");
  }
}
