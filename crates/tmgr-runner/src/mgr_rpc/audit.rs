use super::dispatch::{Audit, AuditRecord};
use serde_json::json;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const AUDIT_FILE: &str = "mgr-rpc-audit.jsonl";
const AUDIT_CAP: u64 = 1024 * 1024;
const STRANGER_WINDOW_MS: u64 = 60_000;
const MAX_STRANGERS: usize = 1024;

struct Inner {
    file: Option<File>,
    size: u64,
    strangers: HashMap<String, u64>,
}

pub struct FileAudit {
    path: PathBuf,
    cap: u64,
    inner: Mutex<Inner>,
}

impl FileAudit {
    pub fn new(runner_dir: &Path) -> FileAudit {
        FileAudit::with_cap(runner_dir, AUDIT_CAP)
    }

    pub(crate) fn with_cap(runner_dir: &Path, cap: u64) -> FileAudit {
        FileAudit {
            path: runner_dir.join(AUDIT_FILE),
            cap,
            inner: Mutex::new(Inner {
                file: None,
                size: 0,
                strangers: HashMap::new(),
            }),
        }
    }

    fn open(&self) -> Option<(File, u64)> {
        let f = OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&self.path)
            .ok()?;
        let size = f.metadata().ok()?.len();
        Some((f, size))
    }

    fn rotated_path(&self) -> PathBuf {
        let mut p = self.path.clone().into_os_string();
        p.push(".1");
        PathBuf::from(p)
    }
}

impl Audit for FileAudit {
    fn record(&self, rec: AuditRecord) {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if rec.outcome == "not_allowed" && rec.rpc.is_none() {
            let now = rec.ts_ms;
            g.strangers
                .retain(|_, t| now.saturating_sub(*t) < STRANGER_WINDOW_MS);
            if g.strangers.contains_key(&rec.from) || g.strangers.len() >= MAX_STRANGERS {
                return;
            }
            g.strangers.insert(rec.from.clone(), now);
        }
        let mut entry = json!({
            "ts_ms": rec.ts_ms,
            "from": rec.from,
            "rpc": rec.rpc,
            "op": rec.op.map(|o| o.as_str()),
            "session": rec.session,
            "outcome": rec.outcome,
        });
        if let Some(card) = rec.card_id {
            entry["card_id"] = json!(card);
        }
        let mut line = entry.to_string();
        line.push('\n');
        let len = line.len() as u64;
        if g.file.is_none() {
            let Some((f, size)) = self.open() else { return };
            g.file = Some(f);
            g.size = size;
        }
        if g.size > 0 && g.size + len > self.cap {
            g.file = None;
            if std::fs::rename(&self.path, self.rotated_path()).is_err() {
                return;
            }
            let Some((f, size)) = self.open() else { return };
            g.file = Some(f);
            g.size = size;
        }
        if let Some(f) = g.file.as_mut() {
            if f.write_all(line.as_bytes()).is_ok() {
                g.size += len;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use tmgr_ipc::mgr_rpc::Op;

    const PEER: &str = "mx_00000000000000bb";

    fn dir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("ma")
            .tempdir_in("/tmp")
            .unwrap()
    }

    fn rec(ts: u64, from: &str, rpc: Option<&str>, outcome: &str) -> AuditRecord {
        AuditRecord {
            ts_ms: ts,
            from: from.into(),
            rpc: rpc.map(String::from),
            op: rpc.map(|_| Op::Send),
            session: rpc.map(|_| "s_bbbbbbbbbb".to_string()),
            card_id: None,
            outcome: outcome.into(),
        }
    }

    fn lines(path: &Path) -> Vec<serde_json::Value> {
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn lines_have_only_the_record_fields_and_the_file_is_private() {
        let d = dir();
        let a = FileAudit::new(d.path());
        a.record(rec(5, PEER, Some("r1"), "ok"));
        let p = d.path().join(AUDIT_FILE);
        let mode = std::fs::metadata(&p).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        let l = lines(&p);
        assert_eq!(l.len(), 1);
        let mut keys: Vec<&str> = l[0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["from", "op", "outcome", "rpc", "session", "ts_ms"]);
        assert_eq!(l[0]["op"], "send");
        assert_eq!(l[0]["outcome"], "ok");
    }

    #[test]
    fn the_file_rotates_to_dot_one_at_the_cap() {
        let d = dir();
        let a = FileAudit::with_cap(d.path(), 400);
        for i in 0..20 {
            a.record(rec(i, PEER, Some(&format!("r{i}")), "ok"));
        }
        let p = d.path().join(AUDIT_FILE);
        let old = d.path().join(format!("{AUDIT_FILE}.1"));
        assert!(old.exists());
        assert!(std::fs::metadata(&p).unwrap().len() <= 400);
        assert!(std::fs::metadata(&old).unwrap().len() <= 400);
        assert_eq!(
            std::fs::metadata(&old).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let last = lines(&p).pop().unwrap();
        assert_eq!(last["rpc"], "r19");
    }

    #[test]
    fn strangers_are_limited_to_one_line_per_sender_per_minute() {
        let d = dir();
        let a = FileAudit::new(d.path());
        for i in 0..50 {
            a.record(rec(1000 + i, PEER, None, "not_allowed"));
        }
        a.record(rec(1000, "mx_00000000000000cc", None, "not_allowed"));
        a.record(rec(1000 + STRANGER_WINDOW_MS, PEER, None, "not_allowed"));
        assert_eq!(lines(&d.path().join(AUDIT_FILE)).len(), 3);
    }

    #[test]
    fn accepted_senders_are_never_rate_limited() {
        let d = dir();
        let a = FileAudit::new(d.path());
        for i in 0..10 {
            a.record(rec(1000, PEER, Some(&format!("r{i}")), "not_allowed"));
        }
        assert_eq!(lines(&d.path().join(AUDIT_FILE)).len(), 10);
    }

    #[test]
    fn a_symlinked_audit_file_is_never_written_through() {
        let d = dir();
        let target = d.path().join("elsewhere");
        std::fs::write(&target, "keep").unwrap();
        symlink(&target, d.path().join(AUDIT_FILE)).unwrap();
        FileAudit::new(d.path()).record(rec(1, PEER, Some("r1"), "ok"));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep");
    }
}
