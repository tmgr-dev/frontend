use super::unrestricted::Wiped;
use super::*;
use crate::mgr_rpc::policy::{self, Policy};
use tmgr_ipc::api::{RemotePolicyInfo, RemotePolicyLeader, RemotePolicySetReq};

const MAX_DEPTH: usize = 64;
const REFUSED_CLIENT: &str = "the remote policy is changed only from the tmgr CLI or the desktop";
const REFUSED_SESSION: &str = "the remote policy cannot be changed from inside an agent session";

const REFUSED_PIN: &str =
    "turning asks on needs the follower PIN; run `tmgr chat remote allow-ask <machine> on` in a terminal";

fn outside_sessions(pid: i32, roots: &HashSet<i32>, parent: impl Fn(i32) -> Option<i32>) -> bool {
    let mut cur = pid;
    for _ in 0..MAX_DEPTH {
        if cur <= 1 {
            return true;
        }
        if roots.contains(&cur) {
            return false;
        }
        match parent(cur) {
            Some(p) => cur = p,
            None => return false,
        }
    }
    false
}

#[cfg(target_os = "macos")]
fn parent_of(pid: i32) -> Option<i32> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    let n = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    (n == size).then_some(info.pbi_ppid as i32)
}

#[cfg(target_os = "linux")]
fn parent_of(pid: i32) -> Option<i32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.get(stat.rfind(')')? + 2..)?
        .split(' ')
        .nth(1)?
        .parse()
        .ok()
}

fn is_local_control(client: &str) -> bool {
    client == CLIENT_CLI || client == CLIENT_DESKTOP
}

impl Runner {
    pub(super) fn remote_policy(&self, client: &str) -> Result<Value, ApiErr> {
        if !is_local_control(client) {
            return Err(api(ErrorCode::Forbidden, REFUSED_CLIENT));
        }
        Ok(to_json(&self.remote_policy_info()))
    }

    pub(super) fn remote_policy_set(
        &self,
        client: &str,
        peer_pid: Option<i32>,
        p: &Value,
    ) -> Result<Value, ApiErr> {
        self.remote_policy_set_with(client, peer_pid, &self.session_roots(), parent_of, p)
    }

    fn session_roots(&self) -> HashSet<i32> {
        self.all()
            .iter()
            .map(|s| s.info())
            .filter(|i| i.state.kind != StateKind::Exited)
            .map(|i| i.child_pid as i32)
            .collect()
    }

    fn remote_policy_set_with(
        &self,
        client: &str,
        peer_pid: Option<i32>,
        roots: &HashSet<i32>,
        parent: impl Fn(i32) -> Option<i32>,
        p: &Value,
    ) -> Result<Value, ApiErr> {
        if !is_local_control(client) {
            return Err(api(ErrorCode::Forbidden, REFUSED_CLIENT));
        }
        let Some(pid) = peer_pid else {
            return Err(api(ErrorCode::Forbidden, REFUSED_SESSION));
        };
        if !outside_sessions(pid, roots, parent) {
            return Err(api(ErrorCode::Forbidden, REFUSED_SESSION));
        }
        let RemotePolicySetReq { leader, ask, pin } = params(p)?;
        let pin = pin.map(Wiped);
        if !Policy::load(&self.cfg.dir).accepts(&leader) {
            return Err(api(
                ErrorCode::InvalidParams,
                "policy: that machine is not an accepted leader",
            ));
        }
        if ask {
            match pin {
                Some(pin) if client == CLIENT_CLI => {
                    self.check_follower_pin(pin.take(), "remote policy asks on")?
                }
                _ => return Err(api(ErrorCode::Forbidden, REFUSED_PIN)),
            }
        }
        policy::set_ask(&self.cfg.dir, &leader, ask)
            .map_err(|e| api(ErrorCode::InvalidParams, e))?;
        log(&format!(
            "remote policy: asks from {} turned {}",
            leader,
            if ask { "on" } else { "off" }
        ));
        Ok(to_json(&self.remote_policy_info()))
    }

    fn remote_policy_info(&self) -> RemotePolicyInfo {
        let p = Policy::load(&self.cfg.dir);
        RemotePolicyInfo {
            leaders: p
                .leaders()
                .into_iter()
                .map(|machine| RemotePolicyLeader {
                    ask: p.asks(&machine),
                    ops: p.ops_for(&machine).into_iter().collect(),
                    machine,
                })
                .collect(),
            legacy: p.is_legacy(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::manager::tests::runner_with;
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;
    use std::os::unix::fs::PermissionsExt;
    use tmgr_ipc::mgr_rpc::Op;

    const A: &str = "mx_0123456789abcdef";
    const B: &str = "mx_fedcba9876543210";
    const PIN: &str = "correct horse battery";

    fn env(policy: &str) -> (tempfile::TempDir, Arc<Runner>) {
        env_pin(policy, Some(Ok(PinVerdict::Ok)))
    }

    fn env_pin(
        policy: &str,
        verdict: Option<Result<PinVerdict, String>>,
    ) -> (tempfile::TempDir, Arc<Runner>) {
        let t = tempfile::Builder::new()
            .prefix("rp")
            .tempdir_in("/tmp")
            .unwrap();
        let check: Option<PinCheck> =
            verdict.map(|v| Arc::new(move |_: &str| v.clone()) as PinCheck);
        let rt = runner_with(t.path(), check);
        std::fs::create_dir_all(&rt.cfg.dir).unwrap();
        std::fs::set_permissions(&rt.cfg.dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let f = rt.cfg.dir.join(policy::POLICY_FILE);
        std::fs::write(&f, policy).unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600)).unwrap();
        (t, rt)
    }

    fn two_leaders() -> String {
        format!("accept_from = [\"{A}\", \"{B}\"]\nask_from = [\"{A}\"]\nallow = [\"list\", \"read\", \"room\", \"ask\", \"send\"]\n")
    }

    fn tree(pairs: &[(i32, i32)]) -> impl Fn(i32) -> Option<i32> {
        let m: HashMap<i32, i32> = pairs.iter().copied().collect();
        move |p| m.get(&p).copied()
    }

    fn set(
        rt: &Runner,
        client: &str,
        pid: Option<i32>,
        roots: &[i32],
        v: Value,
    ) -> Result<Value, ApiErr> {
        let roots: HashSet<i32> = roots.iter().copied().collect();
        rt.remote_policy_set_with(
            client,
            pid,
            &roots,
            tree(&[(500, 400), (400, 300), (300, 1)]),
            &v,
        )
    }

    #[test]
    fn the_real_parent_lookup_matches_getppid() {
        let me = std::process::id() as i32;
        assert_eq!(parent_of(me), Some(unsafe { libc::getppid() }));
    }

    #[test]
    fn outside_sessions_walks_the_parent_chain_to_init() {
        let roots: HashSet<i32> = [400].into();
        let parent = tree(&[(500, 400), (400, 300), (300, 1)]);
        assert!(!outside_sessions(500, &roots, &parent));
        assert!(!outside_sessions(400, &roots, &parent));
        assert!(outside_sessions(300, &roots, &parent));
        let roots: HashSet<i32> = [1].into();
        assert!(outside_sessions(5, &roots, tree(&[(5, 1)])));
    }

    #[test]
    fn a_failed_lookup_a_cycle_or_the_depth_cap_is_not_outside() {
        let roots: HashSet<i32> = [400].into();
        assert!(!outside_sessions(999, &roots, tree(&[(500, 400)])));
        assert!(!outside_sessions(600, &roots, tree(&[(600, 601)])));
        let roots: HashSet<i32> = [9].into();
        assert!(!outside_sessions(5, &roots, tree(&[(5, 6), (6, 5)])));
        let chain: Vec<(i32, i32)> = (100..300).map(|p| (p, p + 1)).collect();
        assert!(!outside_sessions(100, &roots, tree(&chain)));
    }

    #[test]
    fn a_peer_whose_parent_cannot_be_read_cannot_set() {
        let (_t, rt) = env(&two_leaders());
        let e = set(
            &rt,
            CLIENT_CLI,
            Some(999),
            &[],
            json!({"leader": A, "ask": false}),
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::Forbidden);
        assert!(Policy::load(&rt.cfg.dir).asks(A));
    }

    #[test]
    fn turning_asks_on_needs_the_cli_and_the_follower_pin() {
        let (_t, rt) = env(&two_leaders());
        for (client, v) in [
            (
                CLIENT_DESKTOP,
                json!({"leader": B, "ask": true, "pin": PIN}),
            ),
            (CLIENT_CLI, json!({"leader": B, "ask": true})),
        ] {
            let e = set(&rt, client, Some(500), &[777], v).unwrap_err();
            assert_eq!(e.code, ErrorCode::Forbidden, "{client}");
            assert!(!Policy::load(&rt.cfg.dir).asks(B));
        }
        let verdicts = [
            None,
            Some(Ok(PinVerdict::Wrong { left: 2 })),
            Some(Ok(PinVerdict::Locked)),
            Some(Ok(PinVerdict::Unset)),
            Some(Ok(PinVerdict::TooShort)),
            Some(Err("keychain".to_string())),
        ];
        for verdict in verdicts {
            let label = format!("{verdict:?}");
            let (_t, rt) = env_pin(&two_leaders(), verdict);
            let e = set(
                &rt,
                CLIENT_CLI,
                Some(500),
                &[777],
                json!({"leader": B, "ask": true, "pin": PIN}),
            )
            .unwrap_err();
            assert_ne!(e.code, ErrorCode::InvalidParams, "{label}");
            assert!(!Policy::load(&rt.cfg.dir).asks(B), "{label}");
        }
    }

    #[test]
    fn the_pin_try_shares_the_unrestricted_throttle() {
        let (_t, rt) = env(&two_leaders());
        *lock(&rt.mgr.pin_last) = Some(Instant::now());
        let e = set(
            &rt,
            CLIENT_CLI,
            Some(500),
            &[777],
            json!({"leader": B, "ask": true, "pin": PIN}),
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::Busy);
        assert!(!Policy::load(&rt.cfg.dir).asks(B));
    }

    #[test]
    fn turning_asks_off_needs_no_pin_from_either_client() {
        for client in [CLIENT_CLI, CLIENT_DESKTOP] {
            let (_t, rt) = env_pin(&two_leaders(), None);
            set(
                &rt,
                client,
                Some(500),
                &[777],
                json!({"leader": A, "ask": false}),
            )
            .unwrap();
            assert!(!Policy::load(&rt.cfg.dir).asks(A), "{client}");
        }
    }

    #[test]
    fn only_cli_and_desktop_may_read_or_set() {
        let (_t, rt) = env(&two_leaders());
        for client in ["worker", "relay", "mcp", "other", ""] {
            assert_eq!(
                rt.remote_policy(client).unwrap_err().code,
                ErrorCode::Forbidden,
                "{client}"
            );
            let e = set(
                &rt,
                client,
                Some(500),
                &[],
                json!({"leader": A, "ask": false}),
            )
            .unwrap_err();
            assert_eq!(e.code, ErrorCode::Forbidden, "{client}");
        }
        assert!(rt.remote_policy(CLIENT_CLI).is_ok());
        assert!(rt.remote_policy(CLIENT_DESKTOP).is_ok());
    }

    #[test]
    fn a_peer_inside_a_session_cannot_set() {
        let (_t, rt) = env(&two_leaders());
        let e = set(
            &rt,
            CLIENT_CLI,
            Some(500),
            &[400],
            json!({"leader": A, "ask": false}),
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::Forbidden);
        assert!(Policy::load(&rt.cfg.dir).asks(A));
    }

    #[test]
    fn an_unknown_peer_pid_cannot_set() {
        let (_t, rt) = env(&two_leaders());
        let e = set(
            &rt,
            CLIENT_DESKTOP,
            None,
            &[],
            json!({"leader": A, "ask": false}),
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::Forbidden);
    }

    #[test]
    fn a_peer_outside_every_session_sets_and_the_file_changes() {
        let (_t, rt) = env(&two_leaders());
        let out = set(
            &rt,
            CLIENT_CLI,
            Some(500),
            &[777],
            json!({"leader": B, "ask": true, "pin": PIN}),
        )
        .unwrap();
        assert!(Policy::load(&rt.cfg.dir).asks(B));
        assert!(out["leaders"]
            .as_array()
            .unwrap()
            .iter()
            .all(|l| l["ask"] == true));
        set(
            &rt,
            CLIENT_DESKTOP,
            Some(500),
            &[777],
            json!({"leader": A, "ask": false}),
        )
        .unwrap();
        assert!(!Policy::load(&rt.cfg.dir).asks(A));
    }

    #[test]
    fn an_unknown_leader_or_extra_field_is_invalid_params() {
        let (_t, rt) = env(&two_leaders());
        let e = set(
            &rt,
            CLIENT_CLI,
            Some(500),
            &[],
            json!({"leader": "mx_1111111111111111", "ask": true}),
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidParams);
        let e = set(
            &rt,
            CLIENT_CLI,
            Some(500),
            &[],
            json!({"leader": A, "ask": true, "x": 1}),
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidParams);
    }

    #[test]
    fn the_read_path_reports_ops_and_legacy() {
        let (_t, rt) = env(&two_leaders());
        let v = rt.remote_policy(CLIENT_CLI).unwrap();
        let info: RemotePolicyInfo = serde_json::from_value(v).unwrap();
        assert!(!info.legacy);
        let a = info.leaders.iter().find(|l| l.machine == A).unwrap();
        let b = info.leaders.iter().find(|l| l.machine == B).unwrap();
        assert!(a.ask && a.ops.contains(&Op::Ask));
        assert!(!b.ask && b.ops == vec![Op::List, Op::Read, Op::Send, Op::Room]);

        let (_t, rt) = env(&format!(
            "accept_from = [\"{A}\"]\nallow = [\"list\", \"read\", \"room\", \"ask\"]\n"
        ));
        let info: RemotePolicyInfo =
            serde_json::from_value(rt.remote_policy(CLIENT_CLI).unwrap()).unwrap();
        assert!(info.legacy);
        assert!(!info.leaders[0].ask);
        assert!(!info.leaders[0].ops.contains(&Op::Ask));
    }
}
