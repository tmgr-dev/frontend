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
pub const ENROLL_ALLOW: [Op; 5] = [Op::List, Op::Read, Op::Room, Op::Ask, Op::Send];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    accept_from: BTreeSet<String>,
    allow: BTreeSet<Op>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyFile {
    #[serde(default)]
    accept_from: Vec<String>,
    allow: Option<Vec<Op>>,
}

impl Policy {
    pub fn deny_all() -> Policy {
        Policy {
            accept_from: BTreeSet::new(),
            allow: DEFAULT_ALLOW.into_iter().collect(),
        }
    }

    pub fn parse(text: &str) -> Result<Policy, String> {
        let f: PolicyFile = toml::from_str(text).map_err(|e| format!("policy: {}", e.message()))?;
        if let Some(bad) = f.accept_from.iter().find(|m| !valid_machine_id(m)) {
            return Err(format!(
                "policy: accept_from has an invalid machine id ({} bytes)",
                bad.len()
            ));
        }
        Ok(Policy {
            accept_from: f.accept_from.into_iter().collect(),
            allow: f.allow.map_or_else(
                || DEFAULT_ALLOW.into_iter().collect(),
                |a| a.into_iter().collect(),
            ),
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

    pub fn allows(&self, op: Op) -> bool {
        self.allow.contains(&op)
    }
}

#[derive(Serialize)]
struct PolicyOut<'a> {
    accept_from: &'a [String],
    allow: &'a [Op],
}

pub fn enroll_leader(runner_dir: &Path, machine: &str) -> Result<(), String> {
    if !valid_machine_id(machine) {
        return Err("policy: the leader machine id is not valid".into());
    }
    let _held = super::pairing::lock(runner_dir)?;
    let (mut accept_from, mut allow) = match read_private(runner_dir, POLICY_FILE, MAX_POLICY_BYTES)
    {
        Ok(text) => {
            let parsed = Policy::parse(&text)?;
            let f = toml::from_str::<PolicyFile>(&text)
                .map_err(|e| format!("policy: {}", e.message()))?;
            let covers = ENROLL_ALLOW.iter().all(|op| parsed.allows(*op));
            if !covers && f.accept_from.iter().any(|m| m != machine) {
                return Err(format!(
                    "policy: enrolling would widen permissions for other machines in {POLICY_FILE}; edit it by hand"
                ));
            }
            let allow: Vec<Op> = if covers {
                parsed.allow.iter().copied().collect()
            } else {
                ENROLL_ALLOW.to_vec()
            };
            (f.accept_from, allow)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Vec::new(), ENROLL_ALLOW.to_vec()),
        Err(e) => return Err(format!("policy: {e}")),
    };
    allow.sort();
    if !accept_from.iter().any(|m| m == machine) {
        accept_from.push(machine.to_string());
    }
    let text = toml::to_string(&PolicyOut {
        accept_from: &accept_from,
        allow: &allow,
    })
    .map_err(|e| format!("policy: {e}"))?;
    write_private_atomic(runner_dir, POLICY_FILE, text.as_bytes())
        .map_err(|e| format!("{POLICY_FILE} was not written: {e}"))
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
            assert_eq!(p.allows(op), want, "{op:?}");
        }
    }

    #[test]
    fn an_explicit_allow_list_replaces_the_default() {
        let p = Policy::parse(&format!(
            "accept_from = [\"{A}\"]\nallow = [\"send\", \"spawn\"]"
        ))
        .unwrap();
        assert!(p.allows(Op::Send) && p.allows(Op::Spawn));
        assert!(!p.allows(Op::List) && !p.allows(Op::Ask));
        let none = Policy::parse("allow = []").unwrap();
        assert!(Op::ALL.iter().all(|o| !none.allows(*o)));
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

    #[test]
    fn the_enroll_allow_list_is_the_one_constant() {
        assert_eq!(
            ENROLL_ALLOW,
            [Op::List, Op::Read, Op::Room, Op::Ask, Op::Send]
        );
    }

    #[test]
    fn enrolling_a_leader_writes_a_private_policy_that_accepts_it_with_the_constant_ops() {
        let d = dir();
        enroll_leader(d.path(), A).unwrap();
        assert_eq!(mode(d.path()), 0o600);
        let p = Policy::try_load(d.path()).unwrap();
        assert!(p.accepts(A) && !p.accepts(B));
        for op in Op::ALL {
            assert_eq!(p.allows(op), ENROLL_ALLOW.contains(&op), "{op:?}");
        }
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
    }

    #[test]
    fn enrolling_keeps_other_leaders_and_a_rerun_gives_the_same_file() {
        let d = dir();
        put(
            d.path(),
            &format!("accept_from = [\"{B}\"]\nallow = [\"list\", \"read\", \"room\", \"ask\", \"send\"]"),
            0o644,
        );
        enroll_leader(d.path(), A).unwrap();
        let once = std::fs::read_to_string(d.path().join(POLICY_FILE)).unwrap();
        let p = Policy::parse(&once).unwrap();
        assert!(p.accepts(A) && p.accepts(B));
        assert!(p.allows(Op::Ask) && p.allows(Op::Send));
        assert_eq!(mode(d.path()), 0o600);
        enroll_leader(d.path(), A).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.path().join(POLICY_FILE)).unwrap(),
            once
        );
    }

    fn read_policy(d: &Path) -> String {
        std::fs::read_to_string(d.join(POLICY_FILE)).unwrap()
    }

    #[test]
    fn enrolling_refuses_to_widen_other_machines_and_writes_nothing() {
        for text in [
            format!("accept_from = [\"{B}\"]\nallow = [\"list\"]"),
            format!("accept_from = [\"{B}\"]"),
            format!("accept_from = [\"{B}\", \"{A}\"]\nallow = [\"list\", \"read\"]"),
        ] {
            let d = dir();
            put(d.path(), &text, 0o600);
            let e = enroll_leader(d.path(), A).unwrap_err();
            assert!(e.contains("widen permissions for other machines"), "{e}");
            assert_eq!(read_policy(d.path()), text);
        }
    }

    #[test]
    fn enrolling_may_widen_when_only_this_machine_is_listed_and_never_narrows() {
        let d = dir();
        put(d.path(), &format!("accept_from = [\"{A}\"]"), 0o600);
        enroll_leader(d.path(), A).unwrap();
        let p = Policy::parse(&read_policy(d.path())).unwrap();
        assert!(ENROLL_ALLOW.iter().all(|o| p.allows(*o)));
        let d = dir();
        put(d.path(), "accept_from = []\nallow = [\"list\"]", 0o600);
        enroll_leader(d.path(), A).unwrap();
        assert!(Policy::parse(&read_policy(d.path()))
            .unwrap()
            .allows(Op::Send));
        let d = dir();
        let wide = format!("accept_from = [\"{B}\"]\nallow = [\"list\", \"read\", \"room\", \"ask\", \"send\", \"spawn\"]");
        put(d.path(), &wide, 0o600);
        enroll_leader(d.path(), A).unwrap();
        let p = Policy::parse(&read_policy(d.path())).unwrap();
        assert!(p.accepts(A) && p.accepts(B) && p.allows(Op::Spawn));
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
