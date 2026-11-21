use super::policy::read_private;
use serde::Deserialize;
use std::path::Path;
use tmgr_chat_relay::fields::{check_device_id, check_fp};
use tmgr_ipc::mgr_rpc::valid_machine_id;

pub const MACHINES_FILE: &str = "machines.json";
const MAX_MACHINES_BYTES: u64 = 64 * 1024;
const MAX_LABEL_CHARS: usize = 64;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Machines {
    list: Vec<Machine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Machine {
    id: String,
    device_id: String,
    fp: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDoc {
    machines: Vec<Row>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    id: String,
    label: String,
    device_id: String,
    fp: String,
}

impl Machines {
    pub fn parse(text: &str) -> Result<Machines, String> {
        let doc: FileDoc =
            serde_json::from_str(text).map_err(|_| "machines: not a valid machines file")?;
        let mut list: Vec<Machine> = Vec::new();
        for r in doc.machines {
            if !valid_machine_id(&r.id)
                || check_device_id(&r.device_id).is_err()
                || check_fp(&r.fp).is_err()
                || r.label.chars().count() > MAX_LABEL_CHARS
            {
                return Err("machines: an entry is invalid".into());
            }
            if list
                .iter()
                .any(|m| m.id == r.id || m.device_id == r.device_id)
            {
                return Err("machines: duplicate machine or device".into());
            }
            list.push(Machine {
                id: r.id,
                device_id: r.device_id,
                fp: r.fp,
            });
        }
        Ok(Machines { list })
    }

    pub fn try_load(runner_dir: &Path) -> Result<Machines, String> {
        match read_private(runner_dir, MACHINES_FILE, MAX_MACHINES_BYTES) {
            Ok(text) => Machines::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Machines::default()),
            Err(e) => Err(format!("machines: {e}")),
        }
    }

    pub fn load(runner_dir: &Path) -> Machines {
        Machines::try_load(runner_dir).unwrap_or_else(|e| {
            crate::daemon::log(&format!("manager rpc: {e}; no machines are known"));
            Machines::default()
        })
    }

    pub fn machine_for(&self, device_id: &str, fp: &str) -> Option<&str> {
        self.list
            .iter()
            .find(|m| m.device_id == device_id && m.fp == fp)
            .map(|m| m.id.as_str())
    }

    pub fn device_for(&self, machine: &str) -> Option<(&str, &str)> {
        self.list
            .iter()
            .find(|m| m.id == machine)
            .map(|m| (m.device_id.as_str(), m.fp.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    const A: &str = "mx_0123456789abcdef";
    const B: &str = "mx_fedcba9876543210";
    const DEV_A: &str = "11111111-2222-3333-4444-555555555555";
    const DEV_B: &str = "66666666-7777-8888-9999-aaaaaaaaaaaa";
    const FP_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const FP_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn entry(id: &str, dev: &str, fp: &str) -> String {
        format!(r#"{{"id":"{id}","label":"box","device_id":"{dev}","fp":"{fp}"}}"#)
    }

    fn file(entries: &[String]) -> String {
        format!(r#"{{"machines":[{}]}}"#, entries.join(","))
    }

    fn good() -> String {
        file(&[entry(A, DEV_A, FP_A), entry(B, DEV_B, FP_B)])
    }

    fn dir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("mm")
            .tempdir_in("/tmp")
            .unwrap()
    }

    fn put(d: &Path, text: &str, mode: u32) {
        let p = d.join(MACHINES_FILE);
        std::fs::write(&p, text).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn lookup_is_by_device_and_fingerprint_both_ways() {
        let m = Machines::parse(&good()).unwrap();
        assert_eq!(m.machine_for(DEV_A, FP_A), Some(A));
        assert_eq!(m.machine_for(DEV_B, FP_B), Some(B));
        assert_eq!(m.device_for(A), Some((DEV_A, FP_A)));
        assert_eq!(m.device_for("mx_ffffffffffffffff"), None);
        assert_eq!(m.machine_for("00000000-0000-0000-0000-000000000000", FP_A), None);
    }

    #[test]
    fn a_fingerprint_mismatch_is_unknown() {
        let m = Machines::parse(&good()).unwrap();
        assert_eq!(m.machine_for(DEV_A, FP_B), None);
        assert_eq!(m.machine_for(DEV_B, FP_A), None);
    }

    #[test]
    fn a_missing_file_is_empty_and_not_an_error() {
        let d = dir();
        assert_eq!(Machines::try_load(d.path()).unwrap(), Machines::default());
        assert_eq!(Machines::load(d.path()), Machines::default());
    }

    #[test]
    fn a_good_private_file_loads() {
        let d = dir();
        put(d.path(), &good(), 0o600);
        assert_eq!(Machines::load(d.path()).machine_for(DEV_A, FP_A), Some(A));
        put(d.path(), &good(), 0o644);
        assert_eq!(Machines::load(d.path()).machine_for(DEV_A, FP_A), Some(A));
    }

    #[test]
    fn group_or_world_writable_files_load_empty() {
        for mode in [0o620, 0o602, 0o666, 0o660] {
            let d = dir();
            put(d.path(), &good(), mode);
            assert!(Machines::try_load(d.path()).is_err(), "{mode:o}");
            assert_eq!(Machines::load(d.path()), Machines::default());
        }
    }

    #[test]
    fn a_symlink_a_directory_and_an_oversize_file_load_empty() {
        let d = dir();
        let real = d.path().join("real.json");
        std::fs::write(&real, good()).unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&real, d.path().join(MACHINES_FILE)).unwrap();
        assert!(Machines::try_load(d.path()).is_err());
        assert_eq!(Machines::load(d.path()), Machines::default());

        let d = dir();
        std::fs::create_dir(d.path().join(MACHINES_FILE)).unwrap();
        assert!(Machines::try_load(d.path()).is_err());

        let d = dir();
        let pad = " ".repeat(MAX_MACHINES_BYTES as usize);
        put(d.path(), &format!("{}{pad}", good()), 0o600);
        assert!(Machines::try_load(d.path()).is_err());
    }

    #[test]
    fn invalid_content_is_refused() {
        let long = "x".repeat(65);
        let bad_label = format!(
            r#"{{"machines":[{{"id":"{A}","label":"{long}","device_id":"{DEV_A}","fp":"{FP_A}"}}]}}"#
        );
        let cases = [
            file(&[entry("mx_ABC", DEV_A, FP_A)]),
            file(&[entry("laptop", DEV_A, FP_A)]),
            file(&[entry(A, "not-a-uuid", FP_A)]),
            file(&[entry(A, &DEV_A.to_uppercase(), FP_A)]),
            file(&[entry(A, DEV_A, "abc")]),
            file(&[entry(A, DEV_A, &FP_A.to_uppercase())]),
            bad_label,
            file(&[entry(A, DEV_A, FP_A), entry(A, DEV_B, FP_B)]),
            file(&[entry(A, DEV_A, FP_A), entry(B, DEV_A, FP_B)]),
            format!(r#"{{"machines":[],"extra":1}}"#),
            format!(
                r#"{{"machines":[{{"id":"{A}","label":"x","device_id":"{DEV_A}","fp":"{FP_A}","note":1}}]}}"#
            ),
            format!(r#"{{"machines":[{{"id":"{A}","device_id":"{DEV_A}","fp":"{FP_A}"}}]}}"#),
            "not json".to_string(),
            "[]".to_string(),
        ];
        for c in cases {
            assert!(Machines::parse(&c).is_err(), "{c}");
        }
    }

    #[test]
    fn a_bad_file_on_disk_loads_empty() {
        let d = dir();
        put(d.path(), &file(&[entry("mx_ABC", DEV_A, FP_A)]), 0o600);
        assert!(Machines::try_load(d.path()).is_err());
        assert_eq!(Machines::load(d.path()), Machines::default());
    }

    #[test]
    fn an_empty_list_is_valid() {
        assert_eq!(Machines::parse(r#"{"machines":[]}"#).unwrap(), Machines::default());
    }
}
