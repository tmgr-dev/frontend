use crate::store::{lock, Lock, Paths};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tmgr_core::config::{ensure_private_dir, write_atomic};
use tmgr_core::error::{Error, Result};

const MAX_RECORDS: usize = 1024;
pub const PRUNE_GRACE_SECS: i64 = 120;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    pub run: String,
    pub link: String,
    pub enrollment: String,
    pub task: String,
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub created: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<u64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    runs: Vec<RunRecord>,
}

pub fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

pub struct Runs {
    paths: Paths,
    file: File,
    damaged: bool,
    _lock: Lock,
}

fn map_path(paths: &Paths) -> PathBuf {
    paths.link_dir().join("runs.json")
}

fn legacy_path(paths: &Paths) -> PathBuf {
    paths.state.join("link-runs.json")
}

fn ensure_dir(paths: &Paths) -> Result<()> {
    ensure_private_dir(&paths.config)?;
    ensure_private_dir(&paths.link_dir())
}

fn migrate(paths: &Paths) -> Result<()> {
    let old = legacy_path(paths);
    let new = map_path(paths);
    if new.exists() || !old.is_file() {
        return Ok(());
    }
    let bytes = std::fs::read(&old)?;
    if serde_json::from_slice::<File>(&bytes).is_ok() {
        write_atomic(&new, &bytes, 0o600)?;
    }
    let _ = std::fs::remove_file(&old);
    Ok(())
}

impl Runs {
    pub fn lock(paths: &Paths) -> Result<Runs> {
        let runs = Runs::lock_lenient(paths)?;
        if runs.damaged {
            return Err(Error::internal("link-runs.json is damaged")
                .hint(format!("fix or remove {}", map_path(paths).display())));
        }
        Ok(runs)
    }

    pub fn lock_lenient(paths: &Paths) -> Result<Runs> {
        ensure_dir(paths)?;
        let guard = lock(&paths.link_dir().join("runs.lock"))?;
        migrate(paths)?;
        let path = map_path(paths);
        let (file, damaged) = match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_file() => match std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
            {
                Some(f) => (f, false),
                None => (File::default(), true),
            },
            Ok(_) => (File::default(), true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (File::default(), false),
            Err(e) => return Err(e.into()),
        };
        Ok(Runs {
            paths: paths.clone(),
            file,
            damaged,
            _lock: guard,
        })
    }

    pub fn is_damaged(&self) -> bool {
        self.damaged
    }

    pub fn owned_by(&self, link: &str, enrollment: &str) -> Vec<&RunRecord> {
        self.file
            .runs
            .iter()
            .filter(|r| r.link == link && r.enrollment == enrollment)
            .collect()
    }

    pub fn owns(&self, link: &str, enrollment: &str, run: &str) -> bool {
        self.file
            .runs
            .iter()
            .any(|r| r.link == link && r.enrollment == enrollment && r.run == run)
    }

    pub fn of_link(&self, link: &str) -> Vec<&RunRecord> {
        self.file.runs.iter().filter(|r| r.link == link).collect()
    }

    pub fn get(&self, run: &str) -> Option<&RunRecord> {
        self.file.runs.iter().find(|r| r.run == run)
    }

    pub fn record(&mut self, rec: RunRecord) -> Result<()> {
        self.file.runs.retain(|r| r.run != rec.run);
        self.file.runs.push(rec);
        if self.file.runs.len() > MAX_RECORDS {
            let drop = self.file.runs.len() - MAX_RECORDS;
            self.file.runs.drain(..drop);
        }
        self.save()
    }

    pub fn recorded(&self, run: &str) -> bool {
        self.file.runs.iter().any(|r| r.run == run)
    }

    pub fn forget(&mut self, run: &str) -> Result<()> {
        self.file.runs.retain(|r| r.run != run);
        self.save()
    }

    pub fn retain_existing(&mut self, existing: &[String]) -> Result<()> {
        let before = self.file.runs.len();
        let now = chrono::Utc::now();
        self.file.runs.retain(|r| {
            existing.contains(&r.run)
                || chrono::DateTime::parse_from_rfc3339(&r.created)
                    .map(|t| (now - t.with_timezone(&chrono::Utc)).num_seconds() < PRUNE_GRACE_SECS)
                    .unwrap_or(true)
        });
        if self.file.runs.len() != before {
            self.save()?;
        }
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let bytes = serde_json::to_vec(&self.file)
            .map_err(|_| Error::internal("cannot serialize link-runs.json"))?;
        write_atomic(&map_path(&self.paths), &bytes, 0o600)
    }
}

pub fn referenced_generations(paths: &Paths, link: &str) -> std::collections::BTreeSet<u64> {
    let text = std::fs::read_to_string(map_path(paths)).unwrap_or_default();
    serde_json::from_str::<File>(&text)
        .map(|f| {
            f.runs
                .iter()
                .filter(|r| r.link == link)
                .filter_map(|r| r.generation)
                .collect()
        })
        .unwrap_or_default()
}

pub fn origin_label(paths: &Paths, run: &str) -> Option<String> {
    if !map_path(paths).exists() {
        return None;
    }
    let runs = Runs::lock(paths).ok()?;
    let rec = runs.get(run)?;
    let store = crate::store::Store::load(paths).ok()?;
    let live = store
        .leaders
        .get(&rec.link)
        .is_some_and(|l| l.enrollment == rec.enrollment);
    Some(if live {
        format!("link:{}", rec.link)
    } else {
        format!("link:{} (revoked)", rec.link)
    })
}

pub fn origin_labels(paths: &Paths) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    if !map_path(paths).exists() {
        return out;
    }
    let (Ok(runs), Ok(store)) = (Runs::lock(paths), crate::store::Store::load(paths)) else {
        return out;
    };
    for rec in &runs.file.runs {
        let live = store
            .leaders
            .get(&rec.link)
            .is_some_and(|l| l.enrollment == rec.enrollment);
        out.insert(
            rec.run.clone(),
            if live {
                format!("link:{}", rec.link)
            } else {
                format!("link:{} (revoked)", rec.link)
            },
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(run: &str, link: &str, enr: &str) -> RunRecord {
        RunRecord {
            run: run.into(),
            link: link.into(),
            enrollment: enr.into(),
            task: "w1:1".into(),
            agent: "codex".into(),
            persona: None,
            model: None,
            created: "2026-10-13T00:00:00Z".into(),
            generation: None,
        }
    }

    #[test]
    fn ownership_is_keyed_by_link_and_enrollment() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::under(d.path());
        {
            let mut runs = Runs::lock(&paths).unwrap();
            runs.record(rec("s_a", "laptop", "e1")).unwrap();
            runs.record(rec("s_b", "phone", "e2")).unwrap();
        }
        let runs = Runs::lock(&paths).unwrap();
        assert!(runs.owns("laptop", "e1", "s_a"));
        assert!(!runs.owns("laptop", "e2", "s_a"));
        assert!(!runs.owns("phone", "e2", "s_a"));
        assert_eq!(runs.owned_by("phone", "e2").len(), 1);
        assert_eq!(runs.of_link("laptop").len(), 1);
        let mode = std::fs::metadata(map_path(&paths))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn pruning_drops_runs_the_runner_no_longer_has() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::under(d.path());
        let mut runs = Runs::lock(&paths).unwrap();
        runs.record(rec("s_a", "laptop", "e1")).unwrap();
        runs.record(rec("s_b", "laptop", "e1")).unwrap();
        runs.retain_existing(&["s_b".to_string()]).unwrap();
        assert!(!runs.owns("laptop", "e1", "s_a"));
        assert!(runs.owns("laptop", "e1", "s_b"));
    }

    #[test]
    fn young_records_survive_pruning_and_old_ones_do_not() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::under(d.path());
        let mut runs = Runs::lock(&paths).unwrap();
        let mut old = rec("s_old", "laptop", "e1");
        old.created = (chrono::Utc::now() - chrono::Duration::hours(2))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let mut young = rec("s_young", "laptop", "e1");
        young.created = crate::now_rfc3339();
        runs.record(old).unwrap();
        runs.record(young).unwrap();
        runs.retain_existing(&[]).unwrap();
        assert!(!runs.owns("laptop", "e1", "s_old"));
        assert!(runs.owns("laptop", "e1", "s_young"));
    }

    #[test]
    fn the_map_lives_in_the_private_config_link_dir_and_the_old_file_moves() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::under(d.path());
        ensure_private_dir(&paths.state).unwrap();
        let legacy = serde_json::json!({"runs": [rec("s_a", "laptop", "e1")]});
        write_atomic(&legacy_path(&paths), legacy.to_string().as_bytes(), 0o600).unwrap();
        let runs = Runs::lock(&paths).unwrap();
        assert!(runs.owns("laptop", "e1", "s_a"));
        assert!(!legacy_path(&paths).exists());
        drop(runs);
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&paths.link_dir()), 0o700);
        assert_eq!(mode(&map_path(&paths)), 0o600);
    }

    #[test]
    fn garbage_is_refused_not_ignored() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::under(d.path());
        ensure_dir(&paths).unwrap();
        write_atomic(&map_path(&paths), b"not json", 0o600).unwrap();
        assert!(Runs::lock(&paths).is_err());
    }
}
