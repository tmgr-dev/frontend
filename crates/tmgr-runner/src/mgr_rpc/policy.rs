use super::files::write_private_atomic;
use crate::login::openat_nofollow;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use tmgr_ipc::mgr_rpc::{valid_machine_id, Op};

pub const POLICY_FILE: &str = "manager-remote.toml";
const MAX_POLICY_BYTES: u64 = 16 * 1024;
const DEFAULT_ALLOW: [Op; 3] = [Op::List, Op::Read, Op::Room];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    accept_from: BTreeSet<String>,
    allow: BTreeSet<Op>,
    ask_from: Option<BTreeSet<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyFile {
    #[serde(default)]
    accept_from: Vec<String>,
    allow: Option<Vec<Op>>,
    ask_from: Option<Vec<String>>,
}

impl Policy {
    pub fn deny_all() -> Policy {
        Policy {
            accept_from: BTreeSet::new(),
            allow: DEFAULT_ALLOW.into_iter().collect(),
            ask_from: None,
        }
    }

    pub fn parse(text: &str) -> Result<Policy, String> {
        let f: PolicyFile = toml::from_str(text).map_err(|e| format!("policy: {}", e.message()))?;
        let ids = f.accept_from.iter().chain(f.ask_from.iter().flatten());
        if let Some(bad) = ids.into_iter().find(|m| !valid_machine_id(m)) {
            return Err(format!(
                "policy: a machine id is invalid ({} bytes)",
                bad.len()
            ));
        }
        Ok(Policy {
            accept_from: f.accept_from.into_iter().collect(),
            allow: f.allow.map_or_else(
                || DEFAULT_ALLOW.into_iter().collect(),
                |a| a.into_iter().collect(),
            ),
            ask_from: f.ask_from.map(|a| a.into_iter().collect()),
        })
    }

    pub fn try_load(runner_dir: &Path) -> Result<Policy, String> {
        let text = read_private(runner_dir, POLICY_FILE, MAX_POLICY_BYTES)
            .map_err(|e| format!("policy: {e}"))?;
        Policy::parse(&text)
    }

    pub fn load(runner_dir: &Path) -> Policy {
        Policy::try_load(runner_dir).unwrap_or_else(|_| Policy::deny_all())
    }

    pub fn accepts(&self, machine: &str) -> bool {
        self.accept_from.contains(machine)
    }

    pub fn leaders(&self) -> Vec<String> {
        self.accept_from.iter().cloned().collect()
    }

    pub fn is_legacy(&self) -> bool {
        self.ask_from.is_none()
    }

    pub fn ops_for(&self, leader: &str) -> BTreeSet<Op> {
        if !self.accepts(leader) {
            return BTreeSet::new();
        }
        let wide = self.ask_from.as_ref().is_none_or(|a| a.contains(leader));
        self.allow
            .iter()
            .copied()
            .filter(|op| wide || DEFAULT_ALLOW.contains(op))
            .collect()
    }

    pub fn asks(&self, leader: &str) -> bool {
        self.ops_for(leader).contains(&Op::Ask)
    }
}

#[derive(Serialize)]
struct PolicyOut {
    accept_from: Vec<String>,
    allow: Vec<Op>,
    ask_from: Vec<String>,
}

fn write_policy(
    runner_dir: &Path,
    accept_from: Vec<String>,
    mut allow: Vec<Op>,
    ask_from: Vec<String>,
) -> Result<(), String> {
    allow.sort();
    allow.dedup();
    let text = toml::to_string(&PolicyOut {
        accept_from,
        allow,
        ask_from,
    })
    .map_err(|e| format!("policy: {e}"))?;
    write_private_atomic(runner_dir, POLICY_FILE, text.as_bytes())
        .map_err(|e| format!("{POLICY_FILE} was not written: {e}"))
}

pub fn enroll_leader(runner_dir: &Path, machine: &str) -> Result<(), String> {
    if !valid_machine_id(machine) {
        return Err("policy: the leader machine id is not valid".into());
    }
    let _held = super::pairing::lock(runner_dir)?;
    let (mut accept_from, allow, ask_from) =
        match read_private(runner_dir, POLICY_FILE, MAX_POLICY_BYTES) {
            Ok(text) => {
                let p = Policy::parse(&text)?;
                let accept: Vec<String> = p.accept_from.iter().cloned().collect();
                let ask = p
                    .ask_from
                    .map_or_else(|| accept.clone(), |a| a.into_iter().collect());
                (accept, p.allow.into_iter().collect(), ask)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                (Vec::new(), DEFAULT_ALLOW.to_vec(), Vec::new())
            }
            Err(e) => return Err(format!("policy: {e}")),
        };
    if !accept_from.iter().any(|m| m == machine) {
        accept_from.push(machine.to_string());
    }
    accept_from.sort();
    write_policy(runner_dir, accept_from, allow, ask_from)
}

pub fn set_ask(runner_dir: &Path, leader: &str, on: bool) -> Result<(), String> {
    let _held = super::pairing::lock(runner_dir)?;
    let text = read_private(runner_dir, POLICY_FILE, MAX_POLICY_BYTES)
        .map_err(|e| format!("policy: {e}"))?;
    let p = Policy::parse(&text)?;
    if !p.accepts(leader) {
        return Err("policy: that machine is not an accepted leader".into());
    }
    let mut ask = p.ask_from.unwrap_or_else(|| p.accept_from.clone());
    let mut allow = p.allow;
    if on {
        ask.insert(leader.to_string());
        allow.insert(Op::Ask);
        allow.insert(Op::Send);
    } else {
        ask.remove(leader);
    }
    write_policy(
        runner_dir,
        p.accept_from.into_iter().collect(),
        allow.into_iter().collect(),
        ask.into_iter().collect(),
    )
}

pub(super) fn read_private(runner_dir: &Path, name: &str, max: u64) -> std::io::Result<String> {
    let rule = |m: &str| std::io::Error::other(m.to_string());
    let dir = File::open(runner_dir)?;
    let mut f = openat_nofollow(
        dir.as_raw_fd(),
        name.as_ref(),
        libc::O_RDONLY | libc::O_NONBLOCK,
    )?;
    let m = f.metadata()?;
    if !m.is_file() {
        return Err(rule("not a regular file"));
    }
    if m.uid() != unsafe { libc::geteuid() } {
        return Err(rule("not owned by the runner user"));
    }
    if m.mode() & 0o022 != 0 {
        return Err(rule("group or world writable"));
    }
    if m.len() > max {
        return Err(rule("file too large"));
    }
    let mut text = String::new();
    f.by_ref().take(max).read_to_string(&mut text)?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    const A: &str = "mx_0123456789abcdef";

    fn dir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("mp")
            .tempdir_in("/tmp")
            .unwrap()
    }

    fn put(d: &Path, text: &str, mode: u32) {
        let p = d.join(POLICY_FILE);
        std::fs::write(&p, text).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn a_missing_file_accepts_nobody() {
        let d = dir();
        let p = Policy::load(d.path());
        assert_eq!(p, Policy::deny_all());
        assert!(!p.accepts(A));
        assert!(Policy::try_load(d.path()).is_err());
    }

    #[test]
    fn an_empty_accept_list_accepts_nobody() {
        let p = Policy::parse("accept_from = []").unwrap();
        assert!(!p.accepts(A));
        assert!(!Policy::parse("").unwrap().accepts(A));
    }

    #[test]
    fn default_ops_are_list_and_read() {
        let p = Policy::parse(&format!("accept_from = [\"{A}\"]")).unwrap();
        assert!(p.accepts(A));
        assert!(!p.accepts("mx_ffffffffffffffff"));
        for (op, want) in [
            (Op::List, true),
            (Op::Read, true),
            (Op::Ask, false),
            (Op::Send, false),
            (Op::Room, true),
            (Op::SpawnChallenge, false),
            (Op::Spawn, false),
            (Op::Switch, false),
        ] {
            assert_eq!(p.ops_for(A).contains(&op), want, "{op:?}");
        }
    }

    #[test]
    fn an_explicit_allow_list_replaces_the_default() {
        let p = Policy::parse(&format!(
            "accept_from = [\"{A}\"]\nallow = [\"send\", \"spawn\"]"
        ))
        .unwrap();
        let ops = p.ops_for(A);
        assert!(ops.contains(&Op::Send) && ops.contains(&Op::Spawn));
        assert!(!ops.contains(&Op::List) && !ops.contains(&Op::Ask));
        let none = Policy::parse("allow = []").unwrap();
        assert!(none.ops_for(A).is_empty());
    }

    #[test]
    fn unknown_keys_bad_ids_and_unknown_ops_are_refused() {
        for bad in [
            format!("accept_from = [\"{A}\"]\nextra = 1"),
            "accept_from = [\"mx_ABC\"]".to_string(),
            "accept_from = [\"laptop\"]".to_string(),
            format!("accept_from = [\"{A}\"]\nallow = [\"rm\"]"),
            "accept_from = \"mx_0123456789abcdef\"".to_string(),
            "accept_from = [".to_string(),
        ] {
            assert!(Policy::parse(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_good_private_file_loads() {
        let d = dir();
        put(d.path(), &format!("accept_from = [\"{A}\"]"), 0o600);
        assert!(Policy::load(d.path()).accepts(A));
        put(d.path(), &format!("accept_from = [\"{A}\"]"), 0o644);
        assert!(Policy::load(d.path()).accepts(A));
    }

    #[test]
    fn group_or_world_writable_files_accept_nobody() {
        for mode in [0o620, 0o602, 0o666, 0o660] {
            let d = dir();
            put(d.path(), &format!("accept_from = [\"{A}\"]"), mode);
            assert!(!Policy::load(d.path()).accepts(A), "{mode:o}");
            assert!(Policy::try_load(d.path()).is_err());
        }
    }

    #[test]
    fn a_symlink_accepts_nobody() {
        let d = dir();
        let real = d.path().join("real.toml");
        std::fs::write(&real, format!("accept_from = [\"{A}\"]")).unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&real, d.path().join(POLICY_FILE)).unwrap();
        assert!(!Policy::load(d.path()).accepts(A));
    }

    #[test]
    fn a_directory_or_a_bad_file_accepts_nobody() {
        let d = dir();
        std::fs::create_dir(d.path().join(POLICY_FILE)).unwrap();
        assert!(!Policy::load(d.path()).accepts(A));
        let d = dir();
        put(d.path(), "accept_from = [\"nope\"]", 0o600);
        assert!(!Policy::load(d.path()).accepts(A));
    }

    const B: &str = "mx_fedcba9876543210";

    fn mode(d: &Path) -> u32 {
        std::fs::metadata(d.join(POLICY_FILE))
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    }

    fn read_policy(d: &Path) -> String {
        std::fs::read_to_string(d.join(POLICY_FILE)).unwrap()
    }

    fn d3() -> BTreeSet<Op> {
        DEFAULT_ALLOW.into_iter().collect()
    }

    #[test]
    fn a_new_enrolment_gives_d3_and_a_private_file() {
        let d = dir();
        enroll_leader(d.path(), A).unwrap();
        assert_eq!(mode(d.path()), 0o600);
        let p = Policy::try_load(d.path()).unwrap();
        assert!(p.accepts(A) && !p.accepts(B));
        assert_eq!(p.ops_for(A), d3());
        assert!(!p.asks(A));
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_legacy_file_keeps_old_leaders_and_gives_the_new_one_d3() {
        let d = dir();
        put(
            d.path(),
            &format!("accept_from = [\"{B}\"]\nallow = [\"list\", \"read\", \"room\", \"ask\", \"send\"]"),
            0o644,
        );
        enroll_leader(d.path(), A).unwrap();
        let once = read_policy(d.path());
        let p = Policy::parse(&once).unwrap();
        assert!(p.accepts(A) && p.accepts(B));
        assert!(p.asks(B) && p.ops_for(B).contains(&Op::Send));
        assert_eq!(p.ops_for(A), d3());
        assert_eq!(mode(d.path()), 0o600);
        enroll_leader(d.path(), A).unwrap();
        assert_eq!(read_policy(d.path()), once);
    }

    #[test]
    fn re_enrolling_never_widens() {
        let d = dir();
        put(d.path(), &format!("accept_from = [\"{A}\"]"), 0o600);
        enroll_leader(d.path(), A).unwrap();
        assert_eq!(
            Policy::parse(&read_policy(d.path())).unwrap().ops_for(A),
            d3()
        );
        let d = dir();
        put(d.path(), "accept_from = []\nallow = [\"list\"]", 0o600);
        enroll_leader(d.path(), A).unwrap();
        let p = Policy::parse(&read_policy(d.path())).unwrap();
        assert_eq!(p.ops_for(A), [Op::List].into_iter().collect());
        let d = dir();
        let wide = format!("accept_from = [\"{B}\"]\nask_from = [\"{B}\"]\nallow = [\"list\", \"read\", \"room\", \"ask\", \"send\", \"spawn\"]");
        put(d.path(), &wide, 0o600);
        enroll_leader(d.path(), A).unwrap();
        let p = Policy::parse(&read_policy(d.path())).unwrap();
        assert!(p.accepts(A) && p.accepts(B) && p.ops_for(B).contains(&Op::Spawn));
        assert_eq!(p.ops_for(A), d3());
    }

    #[test]
    fn ask_from_parses_and_cuts_other_leaders_to_d3() {
        let p = Policy::parse(&format!(
            "accept_from = [\"{A}\", \"{B}\"]\nask_from = [\"{A}\"]\nallow = [\"list\", \"read\", \"room\", \"ask\", \"send\", \"spawn\"]"
        ))
        .unwrap();
        assert!(p.asks(A) && !p.asks(B));
        assert!(p.ops_for(A).contains(&Op::Spawn));
        assert_eq!(p.ops_for(B), d3());
        assert!(p.ops_for("mx_ffffffffffffffff").is_empty());
        assert!(Policy::parse(&format!("accept_from = [\"{A}\"]\nask_from = [\"nope\"]")).is_err());
        assert!(Policy::parse(&format!("accept_from = [\"{A}\"]\nask_fromm = []")).is_err());
    }

    #[test]
    fn set_ask_round_trips() {
        let d = dir();
        enroll_leader(d.path(), A).unwrap();
        enroll_leader(d.path(), B).unwrap();
        set_ask(d.path(), A, true).unwrap();
        let p = Policy::try_load(d.path()).unwrap();
        assert!(p.asks(A) && !p.asks(B));
        assert!(p.ops_for(A).contains(&Op::Send));
        assert_eq!(mode(d.path()), 0o600);
        set_ask(d.path(), A, false).unwrap();
        let p = Policy::try_load(d.path()).unwrap();
        assert!(!p.asks(A));
        assert_eq!(p.ops_for(A), d3());
        assert!(p.allow.contains(&Op::Ask) && p.allow.contains(&Op::Send));
    }

    #[test]
    fn set_ask_on_a_legacy_file_keeps_the_other_leaders() {
        let d = dir();
        put(
            d.path(),
            &format!(
                "accept_from = [\"{A}\", \"{B}\"]\nallow = [\"list\", \"read\", \"room\", \"ask\"]"
            ),
            0o600,
        );
        set_ask(d.path(), A, false).unwrap();
        let p = Policy::try_load(d.path()).unwrap();
        assert!(!p.asks(A) && p.asks(B));
    }

    #[test]
    fn set_ask_for_a_stranger_or_without_a_file_errors_and_writes_nothing() {
        let d = dir();
        assert!(set_ask(d.path(), A, true).is_err());
        assert!(!d.path().join(POLICY_FILE).exists());
        enroll_leader(d.path(), A).unwrap();
        let before = read_policy(d.path());
        assert!(set_ask(d.path(), B, true).is_err());
        assert_eq!(read_policy(d.path()), before);
    }

    #[test]
    fn an_invalid_or_unsafe_existing_policy_is_an_error_and_is_not_overwritten() {
        for text in ["accept_from = [", "accept_from = [\"nope\"]", "extra = 1"] {
            let d = dir();
            put(d.path(), text, 0o600);
            assert!(enroll_leader(d.path(), A).is_err(), "{text}");
            assert_eq!(
                std::fs::read_to_string(d.path().join(POLICY_FILE)).unwrap(),
                text
            );
        }
        let d = dir();
        put(d.path(), "accept_from = []", 0o666);
        assert!(enroll_leader(d.path(), A).is_err());
        let d = dir();
        let real = d.path().join("real.toml");
        std::fs::write(&real, "accept_from = []").unwrap();
        symlink(&real, d.path().join(POLICY_FILE)).unwrap();
        assert!(enroll_leader(d.path(), A).is_err());
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "accept_from = []");
        assert!(enroll_leader(d.path(), "laptop").is_err());
    }

    #[test]
    fn a_missing_runner_dir_accepts_nobody() {
        assert_eq!(
            Policy::load(Path::new("/tmp/mp-does-not-exist-x1")),
            Policy::deny_all()
        );
    }
}
