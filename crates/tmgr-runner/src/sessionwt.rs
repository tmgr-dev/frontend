use crate::worktree::{self, GitEnv};
use std::fs;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

const MAX_NAME: usize = 60;
const RESERVED: [&str; 1] = ["queue"];
const IN_PROGRESS: [&str; 6] = [
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "rebase-merge",
    "rebase-apply",
    "BISECT_LOG",
];

#[derive(Debug, Clone)]
pub struct SessionClone {
    pub name: String,
    pub repo: PathBuf,
    pub path: PathBuf,
    pub base: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Removable {
        branch: String,
        has_commits: bool,
        ignored: usize,
    },
    Kept(String),
}

fn base_ref(name: &str) -> String {
    format!("refs/tmgr/sessions/{name}")
}

pub fn import_hint(repo: &Path, path: &Path, name: &str) -> String {
    format!(
        "git -C {} fetch {} {name}:{name}",
        repo.display(),
        path.display()
    )
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
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(name)) {
        return bad("reserved");
    }
    worktree::valid_ref(name)
}

fn locate(cwd: &Path) -> Result<(PathBuf, String), String> {
    let cwd = fs::canonicalize(cwd).map_err(|e| format!("{}: {e}", cwd.display()))?;
    let git = GitEnv::open(&cwd).map_err(|e| {
        if e.starts_with("not a git repository") {
            format!("{} is not inside a git repository", cwd.display())
        } else {
            e
        }
    })?;
    let common = git.run(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    let common = fs::canonicalize(common).map_err(|e| e.to_string())?;
    let repo = match (common.file_name(), common.parent()) {
        (Some(n), Some(p)) if n == ".git" => p.to_path_buf(),
        _ => return Err("only repositories with a .git folder are supported".to_string()),
    };
    let alternates = common.join("objects/info/alternates");
    if fs::metadata(&alternates).is_ok_and(|m| m.len() > 0) {
        return Err(format!(
            "{} borrows objects from another repository; start the session from the original repository",
            repo.display()
        ));
    }
    let base = git.run(&["rev-parse", "--verify", "HEAD^{commit}"])?;
    Ok((repo, base))
}

fn free(repo: &Path, git: &GitEnv, name: &str) -> bool {
    let taken_dir = worktree::worktrees_root(repo)
        .ok()
        .and_then(|root| fs::read_dir(root).ok())
        .is_some_and(|mut rd| {
            rd.any(|e| {
                e.ok()
                    .is_some_and(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
            })
        });
    !taken_dir
        && !git.succeeds(&[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{name}"),
        ])
        && !git.succeeds(&["show-ref", "--verify", "--quiet", &base_ref(name)])
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

fn lock_root(repo: &Path) -> Result<fs::File, String> {
    let root = worktree::worktrees_root(repo)?;
    if fs::symlink_metadata(&root).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(format!("{} is a symlink", root.display()));
    }
    if !root.exists() {
        fs::create_dir(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join(".tmgr-sessions.lock"))
        .map_err(|e| e.to_string())?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(file)
}

pub fn create(
    cwd: &Path,
    requested: Option<&str>,
    prefix: &str,
    taken: &[String],
) -> Result<SessionClone, String> {
    let (repo, base) = locate(cwd)?;
    let _lock = lock_root(&repo)?;
    let name = pick_name(&repo, requested, prefix, taken)?;
    let main = GitEnv::open(&repo)?;
    let plan = worktree::plan(&repo, &name, Some(&base))?;
    main.run(&["update-ref", &base_ref(&name), &base, ""])?;
    let sc = SessionClone {
        name: name.clone(),
        repo,
        path: plan.path.clone(),
        base,
    };
    match worktree::clone_mode(&plan) {
        Ok(_) => Ok(sc),
        Err(e) => {
            abandon(&sc);
            Err(e)
        }
    }
}

pub fn abandon(sc: &SessionClone) {
    if worktree::approved_path(&sc.repo, &sc.path).is_ok()
        && fs::symlink_metadata(&sc.path).is_ok_and(|m| m.is_dir())
    {
        let _ = fs::remove_dir_all(&sc.path);
    }
    if let Ok(main) = GitEnv::open(&sc.repo) {
        let _ = main.run(&["update-ref", "-d", &base_ref(&sc.name)]);
    }
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

pub fn repo_of(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    let stem = parent.file_name()?.to_str()?.strip_suffix("-worktrees")?;
    let repo = parent.parent()?.join(stem);
    let name = session_dir(&repo, path).ok()?;
    let main = GitEnv::open(&repo).ok()?;
    main.succeeds(&["show-ref", "--verify", "--quiet", &base_ref(&name)])
        .then_some(repo)
}

struct Facts {
    name: String,
    tip: String,
    has_commits: bool,
    ignored: usize,
}

fn gather(repo: &Path, path: &Path) -> Result<Result<Facts, String>, String> {
    let name = session_dir(repo, path)?;
    let main = GitEnv::open(repo)?;
    let base = main
        .run(&["rev-parse", "--verify", "--quiet", &base_ref(&name)])
        .map_err(|_| format!("{} was not made by tmgr", path.display()))?;
    let inner = GitEnv::open(path)?;
    let keys = inner.run(&["config", "--local", "--name-only", "--list"])?;
    if keys
        .lines()
        .any(|k| k.eq_ignore_ascii_case("core.worktree"))
    {
        return Ok(Err("its git config redirects git".into()));
    }
    if path.join(".gitmodules").exists() || path.join(".git/modules").exists() {
        return Ok(Err("it has submodules, which are not checked".into()));
    }
    if let Some(what) = IN_PROGRESS
        .iter()
        .find(|f| path.join(".git").join(f).exists())
    {
        return Ok(Err(format!("a git operation is in progress ({what})")));
    }
    let status = inner.run(&[
        "status",
        "--porcelain=v1",
        "--untracked-files=all",
        "--ignored=matching",
    ])?;
    let ignored = status.lines().filter(|l| l.starts_with("!! ")).count();
    if status.lines().any(|l| !l.starts_with("!! ")) {
        return Ok(Err("it has uncommitted or untracked changes".into()));
    }
    if !inner
        .run(&["for-each-ref", "--format=%(refname)", "refs/stash"])?
        .is_empty()
    {
        return Ok(Err("a stash exists".into()));
    }
    let head = inner.run(&["symbolic-ref", "--short", "-q", "HEAD"]).ok();
    if head.as_deref() != Some(name.as_str()) {
        return Ok(Err(format!(
            "it is on {} instead of its branch {name}",
            head.as_deref().unwrap_or("a detached HEAD")
        )));
    }
    let others = inner.run(&[
        "for-each-ref",
        "--no-merged=HEAD",
        "--format=%(refname:short)",
        "refs/heads",
    ])?;
    if let Some(b) = others.lines().next() {
        return Ok(Err(format!("the branch {b} is not merged into {name}")));
    }
    let tip = inner.run(&["rev-parse", "--verify", "HEAD^{commit}"])?;
    let has_commits = tip != base;
    if has_commits {
        let main_head = main.run(&["rev-parse", "--verify", "HEAD^{commit}"])?;
        if !inner.succeeds(&["merge-base", "--is-ancestor", &tip, &main_head]) {
            let n = inner
                .run(&["rev-list", "--count", &format!("{base}..{tip}")])
                .unwrap_or_default();
            return Ok(Err(format!(
                "branch {name} has {n} commit(s) that are not merged into the current branch; bring them over with: {}",
                import_hint(repo, path, &name)
            )));
        }
    }
    Ok(Ok(Facts {
        name,
        tip,
        has_commits,
        ignored,
    }))
}

fn verdict(f: Facts) -> Verdict {
    Verdict::Removable {
        branch: f.name,
        has_commits: f.has_commits,
        ignored: f.ignored,
    }
}

pub fn assess(repo: &Path, path: &Path) -> Result<Verdict, String> {
    Ok(match gather(repo, path)? {
        Ok(f) => verdict(f),
        Err(why) => Verdict::Kept(why),
    })
}

pub fn remove(repo: &Path, path: &Path) -> Result<Verdict, String> {
    let facts = match gather(repo, path)? {
        Ok(f) => f,
        Err(why) => return Ok(Verdict::Kept(why)),
    };
    let main = GitEnv::open(repo)?;
    let branch = format!("refs/heads/{}", facts.name);
    if facts.has_commits && !main.succeeds(&["show-ref", "--verify", "--quiet", &branch]) {
        main.run(&["update-ref", &branch, &facts.tip, ""])?;
    }
    let id = |p: &Path| {
        use std::os::unix::fs::MetadataExt;
        fs::symlink_metadata(p).map(|m| (m.dev(), m.ino()))
    };
    let before = id(path).map_err(|e| e.to_string())?;
    let root = worktree::worktrees_root(repo)?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let hidden = root.join(format!(".tmgr-del-{}-{nanos}", std::process::id()));
    fs::rename(path, &hidden).map_err(|e| format!("cannot move the checkout aside: {e}"))?;
    if id(&hidden).ok() != Some(before) {
        let _ = fs::rename(&hidden, path);
        return Err("the checkout changed while it was being moved".into());
    }
    main.run(&["update-ref", "-d", &base_ref(&facts.name)])?;
    fs::remove_dir_all(&hidden)
        .map_err(|e| format!("{e}; what is left is in {}", hidden.display()))?;
    Ok(verdict(facts))
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

    fn kept(v: Verdict) -> String {
        match v {
            Verdict::Kept(w) => w,
            other => panic!("expected Kept, got {other:?}"),
        }
    }

    fn removable(branch: &str, has_commits: bool) -> Verdict {
        Verdict::Removable {
            branch: branch.into(),
            has_commits,
            ignored: 0,
        }
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
            "\u{e9}",
            "queue",
            "Queue",
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
        assert_eq!(git(&fx.repo, &["rev-parse", &base_ref("alpha")]), s.base);
        assert!(!s.path.starts_with(&fx.repo));
        assert!(git(&fx.repo, &["branch", "--list", "alpha"]).is_empty());
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
    fn a_linked_worktree_start_uses_the_main_repository() {
        let fx = fixture();
        let linked = fx.root.join("linked");
        git(
            &fx.repo,
            &["worktree", "add", "-b", "lw", linked.to_str().unwrap()],
        );
        commit(&linked, "l.txt");
        let s = create(&linked, Some("fromlinked"), "c", &[]).unwrap();
        assert_eq!(s.repo, fx.repo);
        assert_eq!(s.path, fx.root.join("proj-worktrees/fromlinked"));
        assert!(s.path.join("l.txt").exists());
        assert!(!fx.root.join("linked-worktrees").exists());
    }

    #[test]
    fn a_session_checkout_cannot_be_the_source_of_another() {
        let fx = fixture();
        let s = make(&fx, "first");
        let err = create(&s.path, Some("second"), "c", &[]).unwrap_err();
        assert!(err.contains("original repository"), "{err}");
        assert!(!fx.root.join("proj-worktrees/second").exists());
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
        assert!(create(&fx.repo, Some("DUP"), "c", &[]).is_err());
        git(&fx.repo, &["branch", "taken"]);
        assert!(create(&fx.repo, Some("taken"), "c", &[]).is_err());
        assert!(create(&fx.repo, Some("live"), "c", &["live".into()]).is_err());
        assert!(create(&fx.repo, Some("../evil"), "c", &[]).is_err());
        assert!(create(&fx.repo, Some("queue"), "c", &[]).is_err());
        assert!(!fx.root.join("evil").exists());
    }

    #[test]
    fn a_failed_clone_leaves_no_folder_and_no_ref() {
        let fx = fixture();
        let root = fx.root.join("proj-worktrees");
        fs::create_dir(&root).unwrap();
        let stale = SessionClone {
            name: "gone".into(),
            repo: fx.repo.clone(),
            path: root.join("gone"),
            base: git(&fx.repo, &["rev-parse", "HEAD"]),
        };
        git(&fx.repo, &["update-ref", &base_ref("gone"), &stale.base]);
        fs::create_dir(&stale.path).unwrap();
        abandon(&stale);
        assert!(!stale.path.exists());
        assert!(git(&fx.repo, &["for-each-ref", "refs/tmgr"]).is_empty());
    }

    #[test]
    fn a_pristine_checkout_is_removable_and_leaves_no_branch_behind() {
        let fx = fixture();
        let s = make(&fx, "clean");
        assert_eq!(
            assess(&fx.repo, &s.path).unwrap(),
            removable("clean", false)
        );
        assert!(s.path.exists());
        assert_eq!(
            remove(&fx.repo, &s.path).unwrap(),
            removable("clean", false)
        );
        assert!(!s.path.exists());
        assert!(fx.repo.join("a.txt").exists());
        assert!(git(&fx.repo, &["for-each-ref", "refs/tmgr"]).is_empty());
        assert!(git(&fx.repo, &["branch", "--list", "clean"]).is_empty());
        let leftovers: Vec<_> = fs::read_dir(fx.root.join("proj-worktrees"))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| !n.starts_with(".tmgr-sessions"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn assess_does_not_touch_the_main_repository() {
        let fx = fixture();
        let s = make(&fx, "ro");
        let before = git(&fx.repo, &["for-each-ref"]);
        assess(&fx.repo, &s.path).unwrap();
        assert_eq!(git(&fx.repo, &["for-each-ref"]), before);
        assert!(!fx.repo.join(".git/FETCH_HEAD").exists());
    }

    #[test]
    fn a_dirty_checkout_is_kept() {
        let fx = fixture();
        let s = make(&fx, "dirty");
        fs::write(s.path.join("scratch"), "x").unwrap();
        let w = kept(remove(&fx.repo, &s.path).unwrap());
        assert!(w.contains("uncommitted"), "{w}");
        assert!(s.path.join("scratch").exists());
        fs::remove_file(s.path.join("scratch")).unwrap();
        fs::write(s.path.join("a.txt"), "changed").unwrap();
        assert!(kept(assess(&fx.repo, &s.path).unwrap()).contains("uncommitted"));
    }

    #[test]
    fn ignored_files_do_not_block_but_are_counted() {
        let fx = fixture();
        fs::write(fx.repo.join(".gitignore"), "build/\n").unwrap();
        git(&fx.repo, &["add", ".gitignore"]);
        git(&fx.repo, &["commit", "-m", "ignore"]);
        let s = make(&fx, "ign");
        fs::create_dir(s.path.join("build")).unwrap();
        fs::write(s.path.join("build/out.o"), "o").unwrap();
        let Verdict::Removable { ignored, .. } = assess(&fx.repo, &s.path).unwrap() else {
            panic!("expected removable");
        };
        assert_eq!(ignored, 1);
    }

    #[test]
    fn unmerged_commits_keep_the_checkout_and_name_the_import_command() {
        let fx = fixture();
        let s = make(&fx, "ahead");
        commit(&s.path, "new.txt");
        let w = kept(remove(&fx.repo, &s.path).unwrap());
        assert!(w.contains("1 commit"), "{w}");
        assert!(w.contains("fetch"), "{w}");
        assert!(s.path.exists());
    }

    #[test]
    fn a_stash_or_a_second_branch_with_work_keeps_the_checkout() {
        let fx = fixture();
        let s = make(&fx, "hidden");
        fs::write(s.path.join("a.txt"), "wip").unwrap();
        git(&s.path, &["stash"]);
        assert!(kept(assess(&fx.repo, &s.path).unwrap()).contains("stash"));
        git(&s.path, &["stash", "drop"]);
        git(&s.path, &["checkout", "-q", "-b", "side"]);
        commit(&s.path, "side.txt");
        git(&s.path, &["checkout", "-q", "hidden"]);
        assert!(kept(assess(&fx.repo, &s.path).unwrap()).contains("side"));
    }

    #[test]
    fn a_forged_base_in_the_checkout_cannot_hide_commits() {
        let fx = fixture();
        let s = make(&fx, "forged");
        commit(&s.path, "new.txt");
        let tip = git(&s.path, &["rev-parse", "HEAD"]);
        git(&s.path, &["config", "tmgr.sessionbase", &tip]);
        assert!(kept(remove(&fx.repo, &s.path).unwrap()).contains("not merged"));
        assert!(s.path.exists());
    }

    #[test]
    fn a_redirecting_config_keeps_the_checkout() {
        let fx = fixture();
        let s = make(&fx, "redir");
        git(&s.path, &["config", "core.worktree", "/tmp"]);
        let v = assess(&fx.repo, &s.path);
        assert!(v.is_err() || matches!(v, Ok(Verdict::Kept(_))), "{v:?}");
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
        assert_eq!(v, removable("done", true));
        assert!(!s.path.exists());
        assert_eq!(
            git(&fx.repo, &["rev-parse", "refs/heads/done"]),
            git(&fx.repo, &["rev-parse", "HEAD"])
        );
        assert!(git(&fx.repo, &["for-each-ref", "refs/tmgr"]).is_empty());
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
    fn only_checkouts_made_here_have_a_repo() {
        let fx = fixture();
        let s = make(&fx, "mine");
        assert_eq!(repo_of(&s.path), Some(fx.repo.clone()));
        let other = fx.root.join("proj-worktrees/other");
        fs::create_dir_all(&other).unwrap();
        git(&other, &["init", "-q"]);
        assert_eq!(repo_of(&other), None);
        assert!(remove(&fx.repo, &other).is_err());
        assert_eq!(repo_of(&fx.repo), None);
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
