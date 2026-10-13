use crate::sha256_hex;
use serde_json::{json, Map, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use tmgr_core::config::ensure_private_dir;
use tmgr_core::error::{Error, Result};

pub const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";
pub const ROTATE_AT: u64 = 1 << 20;
const MAX_LINE: usize = 8 * 1024;
const MAX_STR: usize = 256;

pub struct Audit {
    dir: PathBuf,
    rotate_at: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Tampered {
    pub file: String,
    pub line: usize,
    pub reason: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Verified {
    pub entries: u64,
    pub files: usize,
}

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
                .take(16)
                .map(|(k, v)| {
                    (
                        clean(&Value::String(k.clone()), 0)
                            .as_str()
                            .unwrap_or("")
                            .to_string(),
                        clean(v, depth + 1),
                    )
                })
                .collect(),
        ),
        Value::Array(a) if depth < 3 => {
            Value::Array(a.iter().take(16).map(|v| clean(v, depth + 1)).collect())
        }
        Value::Object(_) | Value::Array(_) => Value::Null,
        other => other.clone(),
    }
}

impl Audit {
    pub fn new(dir: &Path) -> Audit {
        Audit {
            dir: dir.to_path_buf(),
            rotate_at: ROTATE_AT,
        }
    }

    pub fn with_rotation(dir: &Path, rotate_at: u64) -> Audit {
        Audit {
            dir: dir.to_path_buf(),
            rotate_at,
        }
    }

    pub fn current(&self) -> PathBuf {
        self.dir.join("audit.jsonl")
    }

    fn rotated(&self) -> Vec<(u64, PathBuf)> {
        let mut out: Vec<(u64, PathBuf)> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let n = name.strip_prefix("audit.")?.strip_suffix(".jsonl")?;
                Some((n.parse::<u64>().ok()?, e.path()))
            })
            .collect();
        out.sort();
        out
    }

    fn tip_of(path: &Path) -> Result<Option<Tip>> {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        if bytes.is_empty() {
            return Ok(None);
        }
        let damaged =
            || Error::internal("the audit log is damaged").hint("tmgr link audit --verify");
        if bytes.last() != Some(&b'\n') {
            return Err(damaged());
        }
        let body = &bytes[..bytes.len() - 1];
        let start = body.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
        let line = &body[start..];
        let v: Value = serde_json::from_slice(line).map_err(|_| damaged())?;
        let seq = v.get("seq").and_then(Value::as_u64).ok_or_else(damaged)?;
        Ok(Some(Tip {
            seq,
            hash: sha256_hex(line),
        }))
    }

    fn tip(&self) -> Result<Tip> {
        if let Some(t) = Self::tip_of(&self.current())? {
            return Ok(t);
        }
        for (_, p) in self.rotated().into_iter().rev() {
            if let Some(t) = Self::tip_of(&p)? {
                return Ok(t);
            }
        }
        Ok(Tip {
            seq: 0,
            hash: GENESIS.to_string(),
        })
    }

    pub fn append(&self, event: &str, fields: Value) -> Result<()> {
        ensure_private_dir(&self.dir)?;
        let _lock = crate::store::lock(&self.dir.join("audit.lock"))?;
        let tip = self.tip()?;
        let cur = self.current();
        if std::fs::metadata(&cur).is_ok_and(|m| m.len() >= self.rotate_at) {
            std::fs::rename(&cur, self.dir.join(format!("audit.{}.jsonl", tip.seq)))?;
        }
        let mut entry = Map::new();
        entry.insert("v".into(), json!(1));
        entry.insert("seq".into(), json!(tip.seq + 1));
        entry.insert("ts".into(), json!(crate::now_rfc3339()));
        entry.insert("prev".into(), json!(tip.hash));
        entry.insert("event".into(), json!(event));
        if let Value::Object(m) = clean(&fields, 0) {
            for (k, v) in m {
                if !entry.contains_key(&k) {
                    entry.insert(k, v);
                }
            }
        }
        let mut line = serde_json::to_vec(&Value::Object(entry.clone())).expect("entry serializes");
        if line.len() > MAX_LINE {
            entry.retain(|k, _| {
                matches!(k.as_str(), "v" | "seq" | "ts" | "prev" | "event" | "link")
            });
            entry.insert("truncated".into(), json!(true));
            line = serde_json::to_vec(&Value::Object(entry)).expect("entry serializes");
        }
        line.push(b'\n');
        let mut f = open_append(&cur)?;
        f.write_all(&line)?;
        f.sync_data()?;
        Ok(())
    }

    fn chain_files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self.rotated().into_iter().map(|(_, p)| p).collect();
        if self.current().exists() {
            files.push(self.current());
        }
        files
    }

    pub fn verify(&self) -> Result<std::result::Result<Verified, Tampered>> {
        let files = self.chain_files();
        let mut prev_hash = GENESIS.to_string();
        let mut prev_seq = 0u64;
        let mut entries = 0u64;
        for path in &files {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();
            let bytes = std::fs::read(path)?;
            if bytes.is_empty() {
                continue;
            }
            let fail = |line: usize, reason: &str| {
                Ok(Err(Tampered {
                    file: name.clone(),
                    line,
                    reason: reason.to_string(),
                }))
            };
            if bytes.last() != Some(&b'\n') {
                return fail(bytes.split(|b| *b == b'\n').count(), "truncated final line");
            }
            for (i, raw) in bytes[..bytes.len() - 1].split(|b| *b == b'\n').enumerate() {
                let n = i + 1;
                let Ok(v) = serde_json::from_slice::<Value>(raw) else {
                    return fail(n, "line is not valid JSON");
                };
                let seq = v.get("seq").and_then(Value::as_u64);
                let prev = v.get("prev").and_then(Value::as_str);
                let (Some(seq), Some(prev)) = (seq, prev) else {
                    return fail(n, "entry is missing seq or prev");
                };
                if seq != prev_seq + 1 {
                    return fail(n, "sequence gap: entries were removed or reordered");
                }
                if prev != prev_hash {
                    return fail(n, "hash chain broken: the previous entry was changed");
                }
                prev_hash = sha256_hex(raw);
                prev_seq = seq;
                entries += 1;
            }
        }
        Ok(Ok(Verified {
            entries,
            files: files.len(),
        }))
    }

    pub fn tail(&self, n: usize) -> Result<Vec<Value>> {
        let mut out: Vec<Value> = Vec::new();
        for path in self.chain_files().into_iter().rev() {
            let text = std::fs::read_to_string(&path)?;
            let mut lines: Vec<Value> = text
                .lines()
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect();
            lines.extend(out);
            out = lines;
            if out.len() >= n {
                break;
            }
        }
        let skip = out.len().saturating_sub(n);
        Ok(out.split_off(skip))
    }
}

#[cfg(unix)]
fn open_append(path: &Path) -> Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    Ok(std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled(dir: &Path, n: usize) -> Audit {
        let a = Audit::new(dir);
        for i in 0..n {
            a.append(
                "worker.request",
                json!({"link":"laptop","method":"runs.list","i":i.to_string()}),
            )
            .unwrap();
        }
        a
    }

    fn lines(a: &Audit) -> Vec<String> {
        std::fs::read_to_string(a.current())
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn write_lines(a: &Audit, l: &[String]) {
        std::fs::write(a.current(), format!("{}\n", l.join("\n"))).unwrap();
    }

    #[test]
    fn the_chain_verifies_and_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("audit");
        let a = filled(&dir, 5);
        assert_eq!(
            a.verify().unwrap(),
            Ok(Verified {
                entries: 5,
                files: 1
            })
        );
        let first: Value = serde_json::from_str(&lines(&a)[0]).unwrap();
        assert_eq!(first["prev"], GENESIS);
        assert_eq!(first["seq"], 1);
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&a.current()), 0o600);
    }

    #[test]
    fn an_empty_log_verifies() {
        let d = tempfile::tempdir().unwrap();
        let a = Audit::new(d.path());
        assert_eq!(
            a.verify().unwrap(),
            Ok(Verified {
                entries: 0,
                files: 0
            })
        );
    }

    #[test]
    fn editing_an_entry_is_detected() {
        let d = tempfile::tempdir().unwrap();
        let a = filled(d.path(), 5);
        let mut l = lines(&a);
        l[1] = l[1].replace("runs.list", "runs.get_");
        write_lines(&a, &l);
        let t = a.verify().unwrap().unwrap_err();
        assert_eq!(t.line, 3);
    }

    #[test]
    fn deleting_a_middle_entry_is_detected() {
        let d = tempfile::tempdir().unwrap();
        let a = filled(d.path(), 5);
        let mut l = lines(&a);
        l.remove(2);
        write_lines(&a, &l);
        let t = a.verify().unwrap().unwrap_err();
        assert_eq!(t.line, 3);
        assert!(t.reason.contains("sequence"));
    }

    #[test]
    fn reordering_is_detected() {
        let d = tempfile::tempdir().unwrap();
        let a = filled(d.path(), 5);
        let mut l = lines(&a);
        l.swap(1, 2);
        write_lines(&a, &l);
        assert!(a.verify().unwrap().is_err());
    }

    #[test]
    fn deleting_the_first_entry_is_detected() {
        let d = tempfile::tempdir().unwrap();
        let a = filled(d.path(), 4);
        let mut l = lines(&a);
        l.remove(0);
        write_lines(&a, &l);
        assert!(a.verify().unwrap().is_err());
    }

    #[test]
    fn a_half_written_line_and_garbage_are_detected() {
        let d = tempfile::tempdir().unwrap();
        let a = filled(d.path(), 3);
        let mut bytes = std::fs::read(a.current()).unwrap();
        bytes.extend_from_slice(b"{\"seq\":4");
        std::fs::write(a.current(), &bytes).unwrap();
        assert!(a.verify().unwrap().is_err());
        assert!(a.append("x", json!({})).is_err());
        let b = filled(&d.path().join("b"), 3);
        let mut l = lines(&b);
        l[1] = "not json".into();
        write_lines(&b, &l);
        assert_eq!(b.verify().unwrap().unwrap_err().line, 2);
    }

    #[test]
    fn rotation_keeps_the_chain() {
        let d = tempfile::tempdir().unwrap();
        let a = Audit::with_rotation(d.path(), 600);
        for i in 0..12 {
            a.append("e", json!({"i": i.to_string()})).unwrap();
        }
        let v = a.verify().unwrap().unwrap();
        assert_eq!(v.entries, 12);
        assert!(v.files > 1, "{v:?}");
        let first_of_new: Value = serde_json::from_str(&lines(&a)[0]).unwrap();
        assert_ne!(first_of_new["prev"], GENESIS);
        let rotated = a.rotated();
        std::fs::remove_file(&rotated[0].1).unwrap();
        assert!(a.verify().unwrap().is_err());
    }

    #[test]
    fn tail_spans_rotated_files_and_values_are_bounded() {
        let d = tempfile::tempdir().unwrap();
        let a = Audit::with_rotation(d.path(), 600);
        for i in 0..8 {
            a.append("e", json!({"i": i})).unwrap();
        }
        let t = a.tail(5).unwrap();
        assert_eq!(t.len(), 5);
        assert_eq!(t.last().unwrap()["seq"], 8);
        a.append("big", json!({"s": "x".repeat(5000), "c": "a\u{1b}[2Jb"}))
            .unwrap();
        let last = a.tail(1).unwrap().remove(0);
        assert_eq!(last["s"].as_str().unwrap().chars().count(), MAX_STR);
        assert_eq!(last["c"], "a[2Jb");
        assert!(a.verify().unwrap().is_ok());
    }

    #[test]
    fn caller_fields_cannot_overwrite_chain_fields() {
        let d = tempfile::tempdir().unwrap();
        let a = Audit::new(d.path());
        a.append("e", json!({"seq": 99, "prev": "x", "event": "other"}))
            .unwrap();
        let v = &a.tail(1).unwrap()[0];
        assert_eq!(
            (v["seq"].as_u64(), v["event"].as_str()),
            (Some(1), Some("e"))
        );
        assert!(a.verify().unwrap().is_ok());
    }
}
