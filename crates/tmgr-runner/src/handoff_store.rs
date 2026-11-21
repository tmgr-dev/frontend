use crate::wire::write_atomic;
use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tmgr_ipc::dirs::ensure_private_dir;

pub const RETENTION_MS: u64 = 30 * 24 * 60 * 60 * 1000;
const MAX_BYTES: u64 = 1 << 20;

pub struct HandoffStore {
    dir: PathBuf,
}

fn valid_id(id: &str) -> bool {
    id.strip_prefix("h_")
        .is_some_and(|r| r.len() == 32 && r.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')))
}

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, msg)
}

fn invalid_data(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

impl HandoffStore {
    pub fn open(runner_dir: &Path) -> io::Result<HandoffStore> {
        let dir = runner_dir.join("handoff");
        ensure_private_dir(&dir)?;
        Ok(HandoffStore { dir })
    }

    fn path(&self, id: &str) -> io::Result<PathBuf> {
        if !valid_id(id) {
            return Err(invalid("invalid handoff packet id"));
        }
        Ok(self.dir.join(format!("{id}.json")))
    }

    pub fn save(&self, id: &str, value: &serde_json::Value) -> io::Result<()> {
        let path = self.path(id)?;
        let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(invalid("handoff packet is larger than 1 MiB"));
        }
        write_atomic(&path, &bytes)
    }

    pub fn get(&self, id: &str) -> io::Result<Option<serde_json::Value>> {
        let path = self.path(id)?;
        let f = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)
        {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let meta = f.metadata()?;
        if !meta.is_file() {
            return Err(invalid_data("handoff packet is not a regular file"));
        }
        if meta.len() > MAX_BYTES {
            return Err(invalid_data("handoff packet is larger than 1 MiB"));
        }
        let mut buf = Vec::new();
        f.take(MAX_BYTES + 1).read_to_end(&mut buf)?;
        if buf.len() as u64 > MAX_BYTES {
            return Err(invalid_data("handoff packet is larger than 1 MiB"));
        }
        serde_json::from_slice(&buf)
            .map(Some)
            .map_err(|e| invalid_data(&e.to_string()))
    }

    pub fn find_for(
        &self,
        old_session: &str,
        to_agent: &str,
        after_ms: u64,
    ) -> Option<(String, serde_json::Value)> {
        let rd = fs::read_dir(&self.dir).ok()?;
        let mut best: Option<(u64, String, serde_json::Value)> = None;
        for entry in rd.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let Some(id) = name.strip_suffix(".json") else {
                continue;
            };
            if !valid_id(id) {
                continue;
            }
            let Ok(Some(v)) = self.get(id) else {
                continue;
            };
            if v.pointer("/from/session").and_then(|s| s.as_str()) != Some(old_session)
                || v.pointer("/to/agent").and_then(|s| s.as_str()) != Some(to_agent)
            {
                continue;
            }
            let Some(created) = v.get("created_ms").and_then(|c| c.as_u64()) else {
                continue;
            };
            if created < after_ms {
                continue;
            }
            if best.as_ref().is_none_or(|(c, _, _)| created > *c) {
                best = Some((created, id.to_string(), v));
            }
        }
        best.map(|(_, id, v)| (id, v))
    }

    pub fn prune(&self, now_ms: u64, max_age_ms: u64) -> usize {
        let cutoff = now_ms.saturating_sub(max_age_ms);
        let Ok(rd) = fs::read_dir(&self.dir) else {
            return 0;
        };
        let mut removed = 0;
        for entry in rd.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let Some(id) = name.strip_suffix(".json") else {
                continue;
            };
            if !valid_id(id) {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            let Ok(modified) = meta.modified() else {
                continue;
            };
            let modified_ms = modified
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as u64);
            if modified_ms < cutoff && fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs::{self, OpenOptions};
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    const ID_A: &str = "h_0123456789abcdef0123456789abcdef";
    const ID_B: &str = "h_fedcba9876543210fedcba9876543210";
    const ID_C: &str = "h_00000000000000000000000000000001";
    const ID_D: &str = "h_00000000000000000000000000000002";
    const ID_E: &str = "h_00000000000000000000000000000003";

    fn store() -> (tempfile::TempDir, HandoffStore) {
        let t = tempfile::tempdir().unwrap();
        std::fs::set_permissions(
            t.path(),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .unwrap();
        let s = HandoffStore::open(t.path()).unwrap();
        (t, s)
    }

    fn packet(from: &str, to: &str, created_ms: u64) -> serde_json::Value {
        json!({
            "from": { "session": from },
            "to": { "agent": to },
            "created_ms": created_ms,
        })
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    #[test]
    fn directory_and_files_are_private() {
        let (t, s) = store();
        let dir = t.path().join("handoff");
        assert_eq!(
            fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        s.save(ID_A, &packet("old", "claude", 1)).unwrap();
        let file = dir.join(format!("{ID_A}.json"));
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn packet_round_trips_and_unknown_id_is_none() {
        let (_t, s) = store();
        let value = packet("old", "claude", 42);
        s.save(ID_A, &value).unwrap();
        assert_eq!(s.get(ID_A).unwrap(), Some(value));
        assert_eq!(s.get(ID_B).unwrap(), None);
    }

    #[test]
    fn bad_ids_are_rejected_before_any_path_is_built() {
        let (t, s) = store();
        let outside = t.path().join("x.json");
        for bad in [
            "../x",
            "h_ABCDEF0123456789abcdef0123456789",
            "h_0123",
            "h_0123456789abcdef0123456789abcdef0",
            "h_0123456789abcdef0123456789abcdef/../../x",
            "",
        ] {
            let err = s.save(bad, &packet("old", "claude", 1)).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "save {bad}");
            let err = s.get(bad).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "get {bad}");
        }
        assert!(!outside.exists());
    }

    #[test]
    fn symlinked_packet_is_refused_and_victim_untouched() {
        let (t, s) = store();
        let victim = t.path().join("victim");
        fs::write(
            &victim,
            r#"{"from":{"session":"old"},"to":{"agent":"claude"},"created_ms":1}"#,
        )
        .unwrap();
        symlink(
            &victim,
            t.path().join("handoff").join(format!("{ID_A}.json")),
        )
        .unwrap();
        assert!(s.get(ID_A).is_err());
        assert_eq!(s.find_for("old", "claude", 0), None);
        assert_eq!(
            fs::read_to_string(&victim).unwrap(),
            r#"{"from":{"session":"old"},"to":{"agent":"claude"},"created_ms":1}"#
        );
    }

    #[test]
    fn oversized_and_non_regular_packets_are_refused() {
        let (t, s) = store();
        let dir = t.path().join("handoff");
        let big = format!("\"{}\"", "a".repeat(1 << 20));
        fs::write(dir.join(format!("{ID_A}.json")), big).unwrap();
        assert!(s.get(ID_A).is_err());
        fs::create_dir(dir.join(format!("{ID_B}.json"))).unwrap();
        assert!(s.get(ID_B).is_err());
    }

    #[test]
    fn find_for_picks_the_newest_match_and_honours_after_ms() {
        let (_t, s) = store();
        s.save(ID_A, &packet("old", "claude", 100)).unwrap();
        s.save(ID_B, &packet("old", "claude", 300)).unwrap();
        s.save(ID_C, &packet("old", "claude", 200)).unwrap();
        s.save(ID_D, &packet("other", "claude", 400)).unwrap();
        s.save(ID_E, &packet("old", "codex", 500)).unwrap();

        let (id, value) = s.find_for("old", "claude", 0).unwrap();
        assert_eq!(id, ID_B);
        assert_eq!(value, packet("old", "claude", 300));

        let (id, _) = s.find_for("old", "claude", 250).unwrap();
        assert_eq!(id, ID_B);
        assert_eq!(s.find_for("old", "claude", 301), None);
        assert_eq!(s.find_for("missing", "claude", 0), None);
    }

    #[test]
    fn leftover_tmp_file_is_ignored_by_find_for() {
        let (t, s) = store();
        let dir = t.path().join("handoff");
        let tmp = format!(".{ID_A}.json.0a1b2c.tmp");
        fs::write(dir.join(tmp), packet("old", "claude", 999).to_string()).unwrap();
        assert_eq!(s.find_for("old", "claude", 0), None);
    }

    #[test]
    fn prune_removes_only_packets_older_than_the_limit() {
        let (t, s) = store();
        let now = now_ms();
        s.save(ID_A, &packet("old", "claude", 1)).unwrap();
        s.save(ID_B, &packet("old", "claude", 1)).unwrap();
        let stale = UNIX_EPOCH + Duration::from_millis(now - RETENTION_MS - 60_000);
        OpenOptions::new()
            .write(true)
            .open(t.path().join("handoff").join(format!("{ID_A}.json")))
            .unwrap()
            .set_modified(stale)
            .unwrap();

        assert_eq!(s.prune(now, RETENTION_MS), 1);
        assert_eq!(s.get(ID_A).unwrap(), None);
        assert!(s.get(ID_B).unwrap().is_some());
    }
}
