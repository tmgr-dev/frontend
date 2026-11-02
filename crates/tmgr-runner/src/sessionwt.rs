use crate::worktree::{self, GitEnv};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_NAME: usize = 60;
const BASE_KEY: &str = "tmgr.sessionbase";

#[derive(Debug, Clone)]
pub struct SessionClone {
    pub name: String,
    pub repo: PathBuf,
    pub path: PathBuf,
    pub base: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Removable { branch: String },
    Kept(String),
}

pub fn check_name(name: &str) -> Result<(), String> {
    let bad = |why: &str| Err(format!("invalid worktree name {name:?}: {why}"));
    if name.is_empty() || name.len() > MAX_NAME {
        return bad("empty or longer than 60 characters");
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return bad("only A-Za-z0-9 . _ - are allowed");
    }
    if !name.as_bytes()[0].is_ascii_alphanumeric() || name.contains("..") {
        return bad("must start with a letter or digit and not contain ..");
    }
    if name.ends_with('.') || name.ends_with(".lock") {
        return bad("must not end with . or .lock");
    }
    worktree::valid_ref(name)
}

fn root_of(cwd: &Path) -> Result<PathBuf, String> {
    let cwd = fs::canonicalize(cwd).map_err(|e| format!("{}: {e}", cwd.display()))?;
    let git = GitEnv::open(&cwd)
        .map_err(|_| format!("{} is not inside a git repository", cwd.display()))?;
    let top = git.run(&["rev-parse", "--show-toplevel"])?;
    fs::canonicalize(top).map_err(|e| e.to_string())
}

fn free(repo: &Path, git: &GitEnv, name: &str) -> bool {
    let branch = format!("refs/heads/{name}");
    let path = worktree::worktrees_root(repo).map(|r| r.join(name));
    !git.succeeds(&["show-ref", "--verify", "--quiet", &branch])
        && path.is_ok_and(|p| fs::symlink_metadata(p).is_err())
}

pub fn pick_name(
    repo: &Path,
    requested: Option<&str>,
    prefix: &str,
    taken: &[String],
) -> Result<String, String> {
    let git = GitEnv::open(repo)?;
    let taken = |n: &str| taken.iter().any(|t| t == n);
    if let Some(name) = requested {
        check_name(name)?;
        if taken(name) {
            return Err(format!("a session is already named {name:?}"));
        }
        if !free(repo, &git, name) {
            return Err(format!(
                "{name:?} is already a branch or a folder in {}",
                worktree::worktrees_root(repo)?.display()
            ));
        }
        return Ok(name.to_string());
    }
    let stem: String = prefix
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .take(40)
        .collect();
    let stem = if stem.is_empty() {
        "session".to_string()
    } else {
        stem
    };
    for n in 1..1000 {
        let name = format!("{stem}-{n}");
        if check_name(&name).is_ok() && !taken(&name) && free(repo, &git, &name) {
            return Ok(name);
        }
    }
    Err("no free worktree name".to_string())
}

pub fn create(
    cwd: &Path,
    requested: Option<&str>,
    prefix: &str,
    taken: &[String],
) -> Result<SessionClone, String> {
    let repo = root_of(cwd)?;
    let git = GitEnv::open(&repo)?;
    let base = git.run(&["rev-parse", "--verify", "HEAD^{commit}"])?;
    let name = pick_name(&repo, requested, prefix, taken)?;
    let plan = worktree::plan(&repo, &name, Some(&base))?;
    let wt = worktree::clone_mode(&plan)?;
    let inner = GitEnv::open(&wt.path)?;
    inner.run(&["config", "--local", BASE_KEY, &base])?;
    Ok(SessionClone {
        name,
        repo,
        path: wt.path,
        base,
    })
}

fn session_dir(repo: &Path, path: &Path) -> Result<String, String> {
    worktree::approved_path(repo, path)?;
    let meta = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let git_meta = fs::symlink_metadata(path.join(".git"))
        .map_err(|_| format!("{} is not a git checkout", path.display()))?;
    if !meta.is_dir() || !git_meta.is_dir() {
        return Err(format!("{} is not a plain git checkout", path.display()));
    }
    path.file_name()
        .and_then(|n| n.to_str())
        .map(String::from)
        .ok_or_else(|| "worktree path has no name".to_string())
}

struct Facts {
    name: String,
    tip: String,
    advanced: bool,
}

fn gather(repo: &Path, path: &Path) -> Result<Result<Facts, String>, String> {
    let name = session_dir(repo, path)?;
    let inner = GitEnv::open(path)?;
    if !inner
        .run(&["status", "--porcelain=v1", "--untracked-files=all"])?
        .is_empty()
    {
        return Ok(Err("it has uncommitted or untracked changes".into()));
    }
    let head = inner.run(&["symbolic-ref", "--short", "-q", "HEAD"]).ok();
    if head.as_deref() != Some(name.as_str()) {
        return Ok(Err(format!(
            "it is on {} instead of its branch {name}",
            head.as_deref().unwrap_or("a detached HEAD")
        )));
    }
    let tip = inner.run(&["rev-parse", "--verify", "HEAD^{commit}"])?;
    let base = inner
        .run(&["config", "--local", "--get", BASE_KEY])
        .map_err(|_| "the checkout has no recorded base commit".to_string())?;
    let main = GitEnv::open(repo)?;
    main.run(&[
        "fetch",
        "--no-tags",
        "--quiet",
        "--",
        worktree::utf8(path)?,
        &format!("refs/heads/{name}"),
    ])?;
    let merged = tip == base || main.succeeds(&["merge-base", "--is-ancestor", &tip, "HEAD"]);
    if !merged {
        let n = main
            .run(&["rev-list", "--count", &format!("{base}..{tip}")])
            .unwrap_or_default();
        return Ok(Err(format!(
            "branch {name} has {n} commit(s) that are not merged into the current branch"
        )));
    }
    let advanced = tip != base;
    Ok(Ok(Facts {
        name,
        tip,
        advanced,
    }))
}

pub fn assess(repo: &Path, path: &Path) -> Result<Verdict, String> {
    Ok(match gather(repo, path)? {
        Ok(f) => Verdict::Removable { branch: f.name },
        Err(why) => Verdict::Kept(why),
    })
}

pub fn remove(repo: &Path, path: &Path) -> Result<Verdict, String> {
    let facts = match gather(repo, path)? {
        Ok(f) => f,
        Err(why) => return Ok(Verdict::Kept(why)),
    };
    let main = GitEnv::open(repo)?;
    if facts.advanced {
        let reference = format!("refs/heads/{}", facts.name);
        if !main.succeeds(&["show-ref", "--verify", "--quiet", &reference]) {
            main.run(&["update-ref", &reference, &facts.tip, ""])?;
        }
    }
    fs::remove_dir_all(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Verdict::Removable { branch: facts.name })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn author(g: GitEnv) -> GitEnv {
        g.env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        author(GitEnv::bare(dir)).run(args).unwrap()
    }

    fn commit(dir: &Path, name: &str) {
        fs::write(dir.join(name), name).unwrap();
        git(dir, &["add", "--", name]);
        git(dir, &["commit", "-m", name]);
    }

    struct Fx {
        _tmp: TempDir,
        repo: PathBuf,
        root: PathBuf,
    }

    fn fixture() -> Fx {
        let tmp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(tmp.path()).unwrap();
        let repo = root.join("proj");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        commit(&repo, "a.txt");
        Fx {
            _tmp: tmp,
            repo,
            root,
        }
    }

    fn make(fx: &Fx, name: &str) -> SessionClone {
        create(&fx.repo, Some(name), "claude", &[]).unwrap()
    }

    #[test]
    fn names_are_validated_for_branch_and_path_use() {
        for ok in ["a", "fix-1", "x.y_z", "A1"] {
            assert!(check_name(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "-a",
            ".a",
            "a/b",
            "../x",
            "a..b",
            "a b",
            "a.lock",
            "a.",
            "HEAD",
            "a\0",
            "é",
            &"x".repeat(61),
        ] {
            assert!(check_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn non_git_folder_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let err = create(tmp.path(), None, "claude", &[]).unwrap_err();
        assert!(err.contains("not inside a git repository"), "{err}");
        assert!(fs::read_dir(tmp.path()).unwrap().next().is_none());
    }

    #[test]
    fn creates_a_sibling_clone_on_a_branch_named_after_the_session() {
        let fx = fixture();
        let s = make(&fx, "alpha");
        assert_eq!(s.path, fx.root.join("proj-worktrees/alpha"));
        assert_eq!(git(&s.path, &["branch", "--show-current"]), "alpha");
        assert_eq!(git(&s.path, &["rev-parse", "HEAD"]), s.base);
        assert_eq!(git(&fx.repo, &["rev-parse", "HEAD"]), s.base);
        assert!(!s.path.starts_with(&fx.repo));
    }

    #[test]
    fn starts_from_the_current_head_not_the_default_branch() {
        let fx = fixture();
        git(&fx.repo, &["checkout", "-b", "feature"]);
        commit(&fx.repo, "f.txt");
        let s = make(&fx, "beta");
        assert!(s.path.join("f.txt").exists());
    }

    #[test]
    fn generated_names_skip_taken_branches_folders_and_sessions() {
        let fx = fixture();
        let first = create(&fx.repo, None, "claude-code", &[]).unwrap();
        assert_eq!(first.name, "claude-code-1");
        git(&fx.repo, &["branch", "claude-code-2"]);
        let third = create(&fx.repo, None, "claude-code", &["claude-code-3".into()]).unwrap();
        assert_eq!(third.name, "claude-code-4");
    }

    #[test]
    fn an_explicit_name_that_collides_is_refused() {
        let fx = fixture();
        make(&fx, "dup");
        assert!(create(&fx.repo, Some("dup"), "c", &[])
            .unwrap_err()
            .contains("already"));
        git(&fx.repo, &["branch", "taken"]);
        assert!(create(&fx.repo, Some("taken"), "c", &[]).is_err());
        assert!(create(&fx.repo, Some("live"), "c", &["live".into()]).is_err());
        assert!(create(&fx.repo, Some("../evil"), "c", &[]).is_err());
        assert!(!fx.root.join("evil").exists());
    }

    #[test]
    fn a_pristine_checkout_is_removable_and_leaves_no_branch_behind() {
        let fx = fixture();
        let s = make(&fx, "clean");
        assert_eq!(
            assess(&fx.repo, &s.path).unwrap(),
            Verdict::Removable {
                branch: "clean".into()
            }
        );
        assert!(s.path.exists());
        remove(&fx.repo, &s.path).unwrap();
        assert!(!s.path.exists());
        assert!(fx.repo.join("a.txt").exists());
    }

    #[test]
    fn a_dirty_checkout_is_kept() {
        let fx = fixture();
        let s = make(&fx, "dirty");
        fs::write(s.path.join("scratch"), "x").unwrap();
        let v = remove(&fx.repo, &s.path).unwrap();
        assert!(
            matches!(&v, Verdict::Kept(w) if w.contains("uncommitted")),
            "{v:?}"
        );
        assert!(s.path.join("scratch").exists());
    }

    #[test]
    fn unmerged_commits_keep_the_checkout() {
        let fx = fixture();
        let s = make(&fx, "ahead");
        commit(&s.path, "new.txt");
        let v = remove(&fx.repo, &s.path).unwrap();
        assert!(
            matches!(&v, Verdict::Kept(w) if w.contains("1 commit")),
            "{v:?}"
        );
        assert!(s.path.exists());
    }

    #[test]
    fn a_merged_branch_is_removed_and_the_branch_is_kept_in_the_repo() {
        let fx = fixture();
        let s = make(&fx, "done");
        commit(&s.path, "new.txt");
        git(
            &fx.repo,
            &["fetch", worktree::utf8(&s.path).unwrap(), "refs/heads/done"],
        );
        git(&fx.repo, &["merge", "--ff-only", "FETCH_HEAD"]);
        let v = remove(&fx.repo, &s.path).unwrap();
        assert_eq!(
            v,
            Verdict::Removable {
                branch: "done".into()
            }
        );
        assert!(!s.path.exists());
        assert_eq!(
            git(&fx.repo, &["rev-parse", "refs/heads/done"]),
            git(&fx.repo, &["rev-parse", "HEAD"])
        );
    }

    #[test]
    fn a_moved_head_or_a_foreign_path_is_never_removed() {
        let fx = fixture();
        let s = make(&fx, "moved");
        git(&s.path, &["checkout", "-q", "--detach"]);
        assert!(matches!(
            remove(&fx.repo, &s.path).unwrap(),
            Verdict::Kept(_)
        ));
        for bad in [fx.repo.clone(), fx.root.clone(), fx.repo.join("sub")] {
            assert!(remove(&fx.repo, &bad).is_err(), "{bad:?}");
        }
        assert!(fx.repo.join("a.txt").exists());
    }

    #[test]
    fn a_symlinked_checkout_is_refused() {
        let fx = fixture();
        let s = make(&fx, "real");
        let link = fx.root.join("proj-worktrees/link");
        std::os::unix::fs::symlink(&s.path, &link).unwrap();
        assert!(remove(&fx.repo, &link).is_err());
        assert!(s.path.exists());
    }
}
