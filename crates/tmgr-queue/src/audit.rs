use crate::paths::{lock_file, Paths};
use chrono::{SecondsFormat, Utc};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::io::Write;
use tmgr_core::config::{ensure_private_dir, write_atomic};
use tmgr_core::error::{Error, Result};

pub const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const MAX_STR: usize = 256;
const MAX_LINE: usize = 8 * 1024;

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub struct Tampered {
    pub line: usize,
    pub reason: String,
}

pub struct Audit<'a>(&'a Paths);

struct Tip {
    seq: u64,
    hash: String,
}

fn clean(v: &Value, depth: usize) -> Value {
    match v {
        Value::String(s) => {
            let stripped: String = s.chars().filter(|c| !c.is_control()).collect();
            Value::String(tmgr_core::text::truncate_chars(&stripped, MAX_STR).0)
        }
        Value::Object(m) if depth < 3 => Value::Object(
            m.iter()
                .take(24)
                .map(|(k, v)| {
                    (
                        k.chars().filter(|c| !c.is_control()).collect(),
                        clean(v, depth + 1),
                    )
                })
                .collect(),
        ),
        Value::Array(a) if depth < 3 => {
            Value::Array(a.iter().take(24).map(|v| clean(v, depth + 1)).collect())
        }
        Value::Object(_) | Value::Array(_) => Value::Null,
        other => other.clone(),
    }
}

impl<'a> Audit<'a> {
    pub fn new(paths: &'a Paths) -> Audit<'a> {
        Audit(paths)
    }

    fn lines(&self) -> Result<Vec<String>> {
        match std::fs::read_to_string(self.0.audit()) {
            Ok(t) => Ok(t.lines().map(str::to_string).collect()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    fn anchor(&self) -> Option<(u64, String)> {
        let text = std::fs::read_to_string(self.0.audit_anchor()).ok()?;
        let v: Value = serde_json::from_str(&text).ok()?;
        Some((
            v.get("seq")?.as_u64()?,
            v.get("hash")?.as_str()?.to_string(),
        ))
    }

    fn tip(&self) -> Result<Tip> {
        let lines = self.lines()?;
        let Some(last) = lines.last() else {
            return Ok(Tip {
                seq: 0,
                hash: GENESIS.into(),
            });
        };
        let v: Value = serde_json::from_str(last).map_err(|_| {
            Error::internal("the queue audit log is damaged").hint("tmgr queue status")
        })?;
        let seq = v
            .get("seq")
            .and_then(Value::as_u64)
            .ok_or_else(|| Error::internal("the queue audit log is damaged"))?;
        Ok(Tip {
            seq,
            hash: sha256_hex(last.as_bytes()),
        })
    }

    fn anchor_ok(&self, tip: &Tip) -> bool {
        match self.anchor() {
            None => tip.seq == 0,
            Some((seq, hash)) => seq == tip.seq && (seq == 0 || hash == tip.hash),
        }
    }

    fn write(&self, tip: &Tip, event: &str, fields: Value) -> Result<Tip> {
        let mut entry = Map::new();
        entry.insert("v".into(), json!(1));
        entry.insert("seq".into(), json!(tip.seq + 1));
        entry.insert(
            "ts".into(),
            json!(Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)),
        );
        entry.insert("prev".into(), json!(tip.hash));
        entry.insert("event".into(), json!(event));
        if let Value::Object(m) = clean(&fields, 0) {
            for (k, v) in m {
                entry.entry(k).or_insert(v);
            }
        }
        let mut line = serde_json::to_string(&Value::Object(entry.clone())).expect("serializes");
        if line.len() > MAX_LINE {
            entry.retain(|k, _| matches!(k.as_str(), "v" | "seq" | "ts" | "prev" | "event"));
            entry.insert("truncated".into(), json!(true));
            line = serde_json::to_string(&Value::Object(entry)).expect("serializes");
        }
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(self.0.audit())?;
        f.write_all(format!("{line}\n").as_bytes())?;
        f.sync_data()?;
        let hash = sha256_hex(line.as_bytes());
        write_atomic(
            &self.0.audit_anchor(),
            json!({"seq": tip.seq + 1, "hash": hash})
                .to_string()
                .as_bytes(),
            0o600,
        )?;
        Ok(Tip {
            seq: tip.seq + 1,
            hash,
        })
    }

    pub fn append(&self, event: &str, fields: Value) -> Result<()> {
        ensure_private_dir(&self.0.dir)?;
        let _lock = lock_file(&self.0.dir, "queue-audit.lock")?;
        let mut tip = self.tip()?;
        if !self.anchor_ok(&tip) {
            tip = self.write(
                &tip,
                "chain_broken",
                json!({"reason": "the log does not match its anchor"}),
            )?;
        }
        self.write(&tip, event, fields)?;
        Ok(())
    }

    pub fn last_field(&self, event_fields: &[&str], field: &str) -> Result<Option<String>> {
        for line in self.lines()?.iter().rev() {
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let event = v.get("event").and_then(Value::as_str).unwrap_or("");
            if event_fields.contains(&event) {
                if let Some(s) = v.get(field).and_then(Value::as_str) {
                    return Ok(Some(s.to_string()));
                }
            }
        }
        Ok(None)
    }

    pub fn events(&self, limit: usize) -> Result<Vec<Value>> {
        let lines = self.lines()?;
        let from = lines.len().saturating_sub(limit);
        Ok(lines[from..]
            .iter()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect())
    }

    pub fn verify(&self) -> Result<std::result::Result<u64, Tampered>> {
        ensure_private_dir(&self.0.dir)?;
        let _lock = lock_file(&self.0.dir, "queue-audit.lock")?;
        self.verify_locked()
    }

    fn verify_locked(&self) -> Result<std::result::Result<u64, Tampered>> {
        let lines = self.lines()?;
        let mut prev = GENESIS.to_string();
        let mut seq = 0u64;
        let fail = |line: usize, reason: &str| {
            Ok(Err(Tampered {
                line,
                reason: reason.into(),
            }))
        };
        for (i, line) in lines.iter().enumerate() {
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                return fail(i + 1, "not valid JSON");
            };
            if v.get("seq").and_then(Value::as_u64) != Some(seq + 1) {
                return fail(i + 1, "sequence number out of order");
            }
            if v.get("prev").and_then(Value::as_str) != Some(prev.as_str()) {
                return fail(i + 1, "hash chain broken");
            }
            seq += 1;
            prev = sha256_hex(line.as_bytes());
        }
        match self.anchor() {
            None if seq > 0 => fail(lines.len(), "the anchor file is missing"),
            None => Ok(Ok(seq)),
            Some((a, _)) if a > seq => fail(lines.len(), "the log was truncated"),
            Some((a, _)) if a < seq => fail(lines.len(), "entries are not covered by the anchor"),
            Some((_, h)) if seq > 0 && h != prev => {
                fail(lines.len(), "the last entry does not match the anchor")
            }
            Some((_, _)) if seq == 0 => fail(0, "the anchor expects entries"),
            Some(_) => Ok(Ok(seq)),
        }
    }

    pub fn rebaseline(&self) -> Result<bool> {
        ensure_private_dir(&self.0.dir)?;
        let _lock = lock_file(&self.0.dir, "queue-audit.lock")?;
        let Err(t) = self.verify_locked()? else {
            return Ok(false);
        };
        let nonce = sha256_hex(
            format!(
                "{}/{}",
                std::process::id(),
                Utc::now().timestamp_nanos_opt().unwrap_or(0)
            )
            .as_bytes(),
        );
        let moved = self.0.dir.join(format!(
            "queue-audit.jsonl.broken-{}-{}",
            Utc::now().timestamp(),
            &nonce[..8]
        ));
        match std::fs::hard_link(self.0.audit(), &moved) {
            Ok(()) => std::fs::remove_file(self.0.audit())?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let _ = std::fs::remove_file(self.0.audit_anchor());
        self.write(
            &Tip {
                seq: 0,
                hash: GENESIS.into(),
            },
            "chain_rebaselined",
            json!({"reason": t.reason, "line": t.line, "kept_as": moved.file_name().and_then(|n| n.to_str()).unwrap_or("")}),
        )?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> (tempfile::TempDir, Paths) {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::at(&d.path().join("tmgr"));
        (d, p)
    }

    #[test]
    fn entries_chain_and_verify() {
        let (_d, p) = paths();
        let a = Audit::new(&p);
        a.append("one", json!({"x": "a\u{7}b"})).unwrap();
        a.append("two", json!({})).unwrap();
        assert_eq!(a.verify().unwrap(), Ok(2));
        let ev = a.events(10).unwrap();
        assert_eq!(ev[0]["x"], "ab");
        assert_eq!(ev[1]["prev"], sha256_hex(a.lines().unwrap()[0].as_bytes()));
    }

    #[test]
    fn editing_a_line_is_detected() {
        let (_d, p) = paths();
        let a = Audit::new(&p);
        for e in ["a", "b", "c"] {
            a.append(e, json!({"k": "v"})).unwrap();
        }
        let text = std::fs::read_to_string(p.audit()).unwrap();
        std::fs::write(p.audit(), text.replacen("\"k\":\"v\"", "\"k\":\"w\"", 1)).unwrap();
        let t = a.verify().unwrap().unwrap_err();
        assert_eq!(t.line, 2);
    }

    #[test]
    fn deleting_a_middle_line_is_detected() {
        let (_d, p) = paths();
        let a = Audit::new(&p);
        for e in ["a", "b", "c"] {
            a.append(e, json!({})).unwrap();
        }
        let lines = a.lines().unwrap();
        std::fs::write(p.audit(), format!("{}\n{}\n", lines[0], lines[2])).unwrap();
        assert!(a.verify().unwrap().is_err());
    }

    #[test]
    fn truncating_the_tail_is_detected_and_the_next_append_records_it() {
        let (_d, p) = paths();
        let a = Audit::new(&p);
        for e in ["a", "b", "c"] {
            a.append(e, json!({})).unwrap();
        }
        let lines = a.lines().unwrap();
        std::fs::write(p.audit(), format!("{}\n{}\n", lines[0], lines[1])).unwrap();
        assert_eq!(
            a.verify().unwrap().unwrap_err().reason,
            "the log was truncated"
        );
        a.append("d", json!({})).unwrap();
        let ev = a.events(10).unwrap();
        assert_eq!(ev[2]["event"], "chain_broken");
    }

    #[test]
    fn an_unanchored_extra_entry_is_a_discrepancy_and_rebaseline_starts_a_new_chain() {
        let (_d, p) = paths();
        let a = Audit::new(&p);
        a.append("a", json!({})).unwrap();
        let anchor = std::fs::read_to_string(p.audit_anchor()).unwrap();
        a.append("b", json!({})).unwrap();
        std::fs::write(p.audit_anchor(), anchor).unwrap();
        assert_eq!(
            a.verify().unwrap().unwrap_err().reason,
            "entries are not covered by the anchor"
        );
        assert!(a.rebaseline().unwrap());
        assert_eq!(a.verify().unwrap(), Ok(1));
        assert_eq!(a.events(5).unwrap()[0]["event"], "chain_rebaselined");
        assert!(!a.rebaseline().unwrap());
    }

    #[test]
    fn last_field_finds_the_newest_value() {
        let (_d, p) = paths();
        let a = Audit::new(&p);
        a.append("enable", json!({"config_sha256": "aa"})).unwrap();
        a.append("other", json!({"config_sha256": "zz"})).unwrap();
        a.append("disable", json!({"config_sha256": "bb"})).unwrap();
        assert_eq!(
            a.last_field(&["enable", "disable"], "config_sha256")
                .unwrap()
                .as_deref(),
            Some("bb")
        );
    }
}
