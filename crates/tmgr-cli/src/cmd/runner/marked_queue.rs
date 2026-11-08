use super::run_task::{homes, mark_in, mark_or_remove};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tmgr_core::error::Result;
use tmgr_queue::workdir::{GitClone, WorkDirs};

pub(crate) struct MarkedClone {
    homes: Option<PathBuf>,
}

impl MarkedClone {
    pub(crate) fn new() -> Self {
        Self { homes: None }
    }

    #[cfg(test)]
    fn at(homes: &Path) -> Self {
        Self {
            homes: Some(homes.to_path_buf()),
        }
    }

    fn homes(&self) -> Result<PathBuf> {
        match &self.homes {
            Some(h) => Ok(h.clone()),
            None => homes(),
        }
    }

    fn single(&self, cwd: &str, repo: &str) -> Result<()> {
        let cwd = Path::new(cwd);
        mark_or_remove(cwd, || mark_in(&self.homes()?, cwd, repo))
    }

    fn multi(&self, cwd: &str, clones: &[(String, String)]) -> Result<()> {
        let run = Path::new(cwd);
        mark_or_remove(run, || {
            let homes = self.homes()?;
            clones
                .iter()
                .try_for_each(|(name, repo)| mark_in(&homes, &run.join(name), repo))
        })
    }
}

impl WorkDirs for MarkedClone {
    fn prepare(&self, repo: &str, pickup_id: &str, cwd: &str) -> Result<()> {
        GitClone.prepare(repo, pickup_id, cwd)?;
        self.single(cwd, repo)
    }

    fn prepare_branch(&self, repo: &str, pickup_id: &str, cwd: &str, branch: &str) -> Result<()> {
        GitClone.prepare_branch(repo, pickup_id, cwd, branch)?;
        self.single(cwd, repo)
    }

    fn prepare_member(&self, repo: &str, pickup_id: &str, cwd: &str, branch: &str) -> Result<()> {
        GitClone.prepare_member(repo, pickup_id, cwd, branch)?;
        self.single(cwd, repo)
    }

    fn prepare_member_multi(
        &self,
        primary: &str,
        pickup_id: &str,
        cwd: &str,
        clones: &[(String, String)],
        branch: &str,
    ) -> Result<()> {
        GitClone.prepare_member_multi(primary, pickup_id, cwd, clones, branch)?;
        self.multi(cwd, clones)
    }

    fn identity(&self, dir: &Path) -> Option<(u64, u64)> {
        GitClone.identity(dir)
    }

    fn discard(&self, pickup_id: &str, cwd: &str) {
        GitClone.discard(pickup_id, cwd)
    }

    fn prepare_multi(
        &self,
        primary: &str,
        pickup_id: &str,
        cwd: &str,
        clones: &[(String, String)],
        branch: &str,
    ) -> Result<()> {
        GitClone.prepare_multi(primary, pickup_id, cwd, clones, branch)?;
        self.multi(cwd, clones)
    }

    fn prepare_continue(
        &self,
        primary: &str,
        pickup_id: &str,
        cwd: &str,
        clones: &[(String, String)],
        branch: &str,
        heads: &BTreeMap<String, String>,
        full_names: &BTreeMap<String, String>,
    ) -> Result<()> {
        GitClone.prepare_continue(primary, pickup_id, cwd, clones, branch, heads, full_names)?;
        if clones.is_empty() {
            self.single(cwd, primary)
        } else {
            self.multi(cwd, clones)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tmgr_queue::workdir::clone_dir;

    fn repo_in(root: &Path) -> String {
        let repo = root.join("proj");
        std::fs::create_dir(&repo).unwrap();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["commit", "-q", "--allow-empty", "-m", "init"],
        ] {
            let out = Command::new("git")
                .current_dir(&repo)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
                .args(&args)
                .output()
                .unwrap();
            assert!(out.status.success(), "{args:?}: {out:?}");
        }
        repo.to_string_lossy().into_owned()
    }

    fn setup() -> (tempfile::TempDir, PathBuf, String) {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = repo_in(&root);
        (tmp, root, repo)
    }

    #[test]
    fn a_queue_clone_made_through_the_wrapper_is_marked() {
        let (_tmp, root, repo) = setup();
        let homes = root.join("homes");
        std::fs::create_dir(&homes).unwrap();
        let dest = clone_dir(&repo, "q_1").unwrap();
        MarkedClone::at(&homes)
            .prepare(&repo, "q_1", dest.to_str().unwrap())
            .unwrap();
        assert!(tmgr_runner::clone::is_marked_clone(&homes, &dest));
    }

    #[test]
    fn every_clone_of_a_multi_run_folder_is_marked() {
        let (_tmp, root, repo) = setup();
        let homes = root.join("homes");
        std::fs::create_dir(&homes).unwrap();
        let dest = clone_dir(&repo, "q_2").unwrap();
        let clones = vec![("fe".to_string(), repo.clone())];
        MarkedClone::at(&homes)
            .prepare_multi(&repo, "q_2", dest.to_str().unwrap(), &clones, "queue/q_2")
            .unwrap();
        assert!(tmgr_runner::clone::is_marked_clone(
            &homes,
            &dest.join("fe")
        ));
    }

    #[test]
    fn a_marker_write_failure_fails_the_prepare_and_leaves_no_folder() {
        let (_tmp, root, repo) = setup();
        let homes = root.join("homes-file");
        std::fs::write(&homes, "x").unwrap();
        let wrapper = MarkedClone::at(&homes);
        let single = clone_dir(&repo, "q_3").unwrap();
        wrapper
            .prepare(&repo, "q_3", single.to_str().unwrap())
            .unwrap_err();
        assert!(!single.exists());
        let multi = clone_dir(&repo, "q_4").unwrap();
        let clones = vec![("fe".to_string(), repo.clone())];
        wrapper
            .prepare_multi(&repo, "q_4", multi.to_str().unwrap(), &clones, "queue/q_4")
            .unwrap_err();
        assert!(!multi.exists());
    }
}
