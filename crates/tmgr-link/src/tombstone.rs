use crate::store::{lock, Paths};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tmgr_core::config::{ensure_private_dir, write_atomic};
use tmgr_core::error::{Error, Result};

const MAX_TOMBSTONES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tombstone {
    pub link: String,
    pub enrollment: String,
    pub generation: u64,
    pub at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    revoked: Vec<Tombstone>,
}

fn file_path(paths: &Paths) -> PathBuf {
    paths.link_dir().join("revoked.json")
}

fn load(paths: &Paths) -> Result<File> {
    let path = file_path(paths);
    match std::fs::symlink_metadata(&path) {
        Ok(m) if m.is_file() => {
            let text = std::fs::read_to_string(&path)?;
            serde_json::from_str(&text).map_err(|_| {
                Error::internal("revoked.json is damaged")
                    .hint(format!("fix or remove {}", path.display()))
            })
        }
        Ok(_) => Err(crate::forbidden("revoked.json is not a regular file")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(File::default()),
        Err(e) => Err(e.into()),
    }
}

fn with<T>(paths: &Paths, f: impl FnOnce(&mut File) -> T) -> Result<T> {
    ensure_private_dir(&paths.config)?;
    ensure_private_dir(&paths.link_dir())?;
    let _guard = lock(&paths.link_dir().join("revoked.lock"))?;
    let mut file = load(paths)?;
    let out = f(&mut file);
    let bytes = serde_json::to_vec(&file).map_err(|_| Error::internal("cannot serialize"))?;
    write_atomic(&file_path(paths), &bytes, 0o600)?;
    Ok(out)
}

pub fn record(paths: &Paths, link: &str, enrollment: &str, generation: u64) -> Result<()> {
    with(paths, |f| {
        f.revoked
            .retain(|t| !(t.link == link && t.enrollment == enrollment));
        f.revoked.push(Tombstone {
            link: link.to_string(),
            enrollment: enrollment.to_string(),
            generation,
            at: crate::now_rfc3339(),
        });
        if f.revoked.len() > MAX_TOMBSTONES {
            let drop = f.revoked.len() - MAX_TOMBSTONES;
            f.revoked.drain(..drop);
        }
    })
}

pub fn for_link(paths: &Paths, link: &str) -> Result<Vec<Tombstone>> {
    if !file_path(paths).exists() {
        return Ok(Vec::new());
    }
    ensure_private_dir(&paths.link_dir())?;
    let _guard = lock(&paths.link_dir().join("revoked.lock"))?;
    Ok(load(paths)?
        .revoked
        .into_iter()
        .filter(|t| t.link == link)
        .collect())
}

pub fn retire(paths: &Paths, link: &str, enrollments: &[String]) -> Result<()> {
    with(paths, |f| {
        f.revoked
            .retain(|t| !(t.link == link && enrollments.contains(&t.enrollment)));
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tombstones_are_kept_per_enrollment_until_retired() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::under(d.path());
        assert!(for_link(&paths, "laptop").unwrap().is_empty());
        record(&paths, "laptop", "e1", 4).unwrap();
        record(&paths, "laptop", "e2", 9).unwrap();
        record(&paths, "phone", "e3", 2).unwrap();
        let t = for_link(&paths, "laptop").unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].generation, 9);
        retire(&paths, "laptop", &["e1".to_string()]).unwrap();
        assert_eq!(for_link(&paths, "laptop").unwrap().len(), 1);
        assert_eq!(for_link(&paths, "phone").unwrap().len(), 1);
        let mode = std::fs::metadata(file_path(&paths))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_damaged_file_is_an_error_not_an_empty_list() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::under(d.path());
        ensure_private_dir(&paths.config).unwrap();
        ensure_private_dir(&paths.link_dir()).unwrap();
        write_atomic(&file_path(&paths), b"nope", 0o600).unwrap();
        assert!(for_link(&paths, "laptop").is_err());
        assert!(record(&paths, "laptop", "e", 1).is_err());
    }
}
