use crate::login::openat_nofollow;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use tmgr_ipc::mgr_rpc::{valid_machine_id, Op};

pub const POLICY_FILE: &str = "manager-remote.toml";
const MAX_POLICY_BYTES: u64 = 16 * 1024;
const DEFAULT_ALLOW: [Op; 2] = [Op::List, Op::Read];

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
        let dir = File::open(runner_dir).map_err(|e| format!("policy: runner dir: {e}"))?;
        let mut f = openat_nofollow(
            dir.as_raw_fd(),
            POLICY_FILE.as_ref(),
            libc::O_RDONLY | libc::O_NONBLOCK,
        )
        .map_err(|e| format!("policy: {POLICY_FILE}: {e}"))?;
        let m = f.metadata().map_err(|e| format!("policy: stat: {e}"))?;
        if !m.is_file() {
            return Err("policy: not a regular file".into());
        }
        if m.uid() != unsafe { libc::geteuid() } {
            return Err("policy: not owned by the runner user".into());
        }
        if m.mode() & 0o022 != 0 {
            return Err("policy: group or world writable".into());
        }
        if m.len() > MAX_POLICY_BYTES {
            return Err("policy: file too large".into());
        }
        let mut text = String::new();
        f.by_ref()
            .take(MAX_POLICY_BYTES)
            .read_to_string(&mut text)
            .map_err(|e| format!("policy: read: {e}"))?;
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

    #[test]
    fn a_missing_runner_dir_accepts_nobody() {
        assert_eq!(
            Policy::load(Path::new("/tmp/mp-does-not-exist-x1")),
            Policy::deny_all()
        );
    }
}
