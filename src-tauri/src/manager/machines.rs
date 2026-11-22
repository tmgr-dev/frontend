use serde::Serialize;

use super::relay::rooms::MachineIndex;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct MachineInfo {
  pub machine: String,
  pub label: Option<String>,
  pub role: Option<String>,
  pub followers: Vec<String>,
  pub local: bool,
}

pub fn machines(local_machine: Option<String>, local_label: Option<String>, indexes: Vec<MachineIndex>) -> Vec<MachineInfo> {
  let mut out = Vec::new();
  if let Some(machine) = local_machine.as_ref() {
    let own = indexes.iter().find(|i| &i.machine == machine);
    out.push(MachineInfo {
      machine: machine.clone(),
      label: local_label,
      role: own.map(|i| i.role.clone()),
      followers: own.map(|i| i.followers.clone()).unwrap_or_default(),
      local: true,
    });
  }
  out.extend(indexes.into_iter().filter(|i| Some(&i.machine) != local_machine.as_ref()).map(|i| MachineInfo {
    machine: i.machine,
    label: Some(i.label),
    role: Some(i.role),
    followers: i.followers,
    local: false,
  }));
  out
}

#[cfg(unix)]
pub fn host_name() -> Option<String> {
  let mut buf = [0u8; 256];
  let rc = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
  if rc != 0 {
    return None;
  }
  let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
  let name = String::from_utf8_lossy(&buf[..end]);
  let name = name.strip_suffix(".local").unwrap_or(&name).trim();
  (!name.is_empty()).then(|| name.to_owned())
}

#[cfg(not(unix))]
pub fn host_name() -> Option<String> {
  std::env::var("COMPUTERNAME").ok().filter(|n| !n.is_empty())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn index(machine: &str, label: &str, role: &str, followers: &[&str]) -> MachineIndex {
    MachineIndex { machine: machine.into(), label: label.into(), role: role.into(), followers: followers.iter().map(|f| (*f).into()).collect() }
  }

  #[test]
  fn the_local_machine_comes_first_with_the_host_name_and_the_role_of_its_own_index() {
    let list = machines(Some("mx_a".into()), Some("my-mac".into()), vec![index("mx_b", "Studio", "sub", &[]), index("mx_a", "Ignored", "top", &["mx_b"])]);
    assert_eq!(
      list,
      vec![
        MachineInfo { machine: "mx_a".into(), label: Some("my-mac".into()), role: Some("top".into()), followers: vec!["mx_b".into()], local: true },
        MachineInfo { machine: "mx_b".into(), label: Some("Studio".into()), role: Some("sub".into()), followers: vec![], local: false },
      ]
    );
  }

  #[test]
  fn a_local_machine_without_an_index_has_no_role() {
    let list = machines(Some("mx_a".into()), Some("my-mac".into()), vec![]);
    assert_eq!(list, vec![MachineInfo { machine: "mx_a".into(), label: Some("my-mac".into()), role: None, followers: vec![], local: true }]);
  }

  #[test]
  fn without_a_local_runner_only_indexed_machines_are_listed() {
    let list = machines(None, None, vec![index("mx_b", "Studio", "top", &[])]);
    assert_eq!(list.len(), 1);
    assert!(!list[0].local);
  }

  #[test]
  fn the_wire_shape_uses_null_for_unknown_label_and_role() {
    let v = serde_json::to_value(machines(Some("mx_a".into()), None, vec![])).unwrap();
    assert_eq!(v, serde_json::json!([{ "machine": "mx_a", "label": null, "role": null, "followers": [], "local": true }]));
  }
}
