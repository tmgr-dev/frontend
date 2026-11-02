use crate::bundle::{open_at, open_root, Dir, Refusal};
use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use tmgr_ipc::api::ErrorCode;

const MAX_ALTERNATES: u64 = 4096;

#[derive(Clone, Debug)]
pub struct Objects {
    pub path: PathBuf,
    pub id: (u64, u64),
    pub repo_id: (u64, u64),
    /// True for a clone directly in `<repository>-worktrees` (not `queue`) that borrows the
    /// repository's objects, the shape `tmgr run` creates for a task.
    pub task_clone: bool,
}

impl Objects {
    pub fn reverify(&self) -> Result<(), String> {
        match open_root(&self.path).and_then(|f| f.metadata()) {
            Ok(m) if (m.dev(), m.ino()) == self.id => Ok(()),
            _ => Err(format!(
                "the object directory {} changed after validation; the run does not start",
                self.path.display()
            )),
        }
    }
}
const DIR: i32 = libc::O_RDONLY | libc::O_DIRECTORY;

fn refuse<T>(m: impl Into<String>) -> Result<T, Refusal> {
    Err((ErrorCode::InvalidParams, format!("clone_of: {}", m.into())))
}

fn walk(from: &File, rel: &[&str]) -> std::io::Result<File> {
    let mut cur = from.try_clone()?;
    for part in rel {
        cur = open_at(cur.as_raw_fd(), part, DIR)?;
    }
    Ok(cur)
}

struct Repo {
    path: PathBuf,
    id: (u64, u64),
    objects_id: (u64, u64),
}

impl Repo {
    fn open(clone_of: &str) -> Result<Repo, Refusal> {
        let path = Path::new(clone_of);
        let comps: Vec<&str> = path
            .components()
            .skip(1)
            .map(|c| match c {
                Component::Normal(n) => n.to_str().unwrap_or(""),
                _ => "",
            })
            .collect();
        if !path.is_absolute()
            || comps.len() < 2
            || comps.iter().any(|c| c.is_empty())
            || clone_of.ends_with('/')
            || clone_of.contains('\0')
        {
            return refuse(
                "the repository must be an absolute path without symlinks, dots or a trailing slash",
            );
        }
        let repo_fd = open_root(path).map_err(|_| {
            (
                ErrorCode::InvalidParams,
                "clone_of: the repository path must be a directory without symlinks".to_string(),
            )
        })?;
        let git = walk(&repo_fd, &[".git"]).map_err(|_| {
            (
                ErrorCode::InvalidParams,
                "clone_of: the repository has no real .git directory".to_string(),
            )
        })?;
        let id = repo_fd
            .metadata()
            .map(|m| (m.dev(), m.ino()))
            .map_err(|_| {
                (
                    ErrorCode::InvalidParams,
                    "clone_of: the repository cannot be read".to_string(),
                )
            })?;
        let objects_id = walk(&git, &["objects"]).and_then(|f| f.metadata());
        let Ok(objects_id) = objects_id else {
            return refuse("the repository is not a git work tree root");
        };
        if walk(&git, &["refs"]).is_err() {
            return refuse("the repository is not a git work tree root");
        }
        Ok(Repo {
            path: path.to_path_buf(),
            id,
            objects_id: (objects_id.dev(), objects_id.ino()),
        })
    }

    fn worktrees(&self) -> PathBuf {
        let name = self.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        self.path
            .parent()
            .unwrap_or(Path::new("/"))
            .join(format!("{name}-worktrees"))
    }

    fn objects(&self, task_clone: bool) -> Objects {
        Objects {
            path: self.path.join(".git/objects"),
            id: self.objects_id,
            repo_id: self.id,
            task_clone,
        }
    }

    fn check_clone(&self, dir: i32, folder: &Path) -> Result<bool, Refusal> {
        let clone_git = open_at(dir, ".git", DIR).map_err(|_| {
            (
                ErrorCode::InvalidParams,
                "clone_of: .git of the folder must be a real directory (a worktree is not a clone)"
                    .to_string(),
            )
        })?;
        refuse_shared_objects(&clone_git, folder)?;
        let want = self.path.join(".git/objects");
        let want = want.to_str().unwrap_or("");
        let info = walk(&clone_git, &["objects", "info"]);
        let alt = info.and_then(|i| {
            open_at(
                i.as_raw_fd(),
                "alternates",
                libc::O_RDONLY | libc::O_NONBLOCK,
            )
        });
        match alt {
            Ok(f) => {
                match f.metadata() {
                    Ok(m) if m.is_file() && m.nlink() == 1 && m.len() <= MAX_ALTERNATES => {}
                    _ => return refuse("alternates is not a plain small file"),
                }
                let mut text = String::new();
                let n = f
                    .take(MAX_ALTERNATES + 1)
                    .read_to_string(&mut text)
                    .map_err(|_| {
                        (
                            ErrorCode::InvalidParams,
                            "clone_of: alternates is not readable text".to_string(),
                        )
                    })?;
                if n as u64 > MAX_ALTERNATES {
                    return refuse("alternates is too large");
                }
                if text.lines().any(|l| l != want) {
                    return refuse("alternates must list only the repository's object directory");
                }
                Ok(text.lines().count() > 0)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => refuse("alternates is not a plain file"),
        }
    }
}

fn first_shared(objects: File) -> std::io::Result<bool> {
    let mut stack = vec![objects];
    while let Some(dir) = stack.pop() {
        let fd = unsafe { libc::dup(dir.as_raw_fd()) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let stream = unsafe { libc::fdopendir(fd) };
        if stream.is_null() {
            let e = std::io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(e);
        }
        let mut names = Vec::new();
        loop {
            let ent = unsafe { libc::readdir(stream) };
            if ent.is_null() {
                break;
            }
            let name = unsafe { std::ffi::CStr::from_ptr((*ent).d_name.as_ptr()) }.to_owned();
            if !matches!(name.to_bytes(), b"." | b"..") {
                names.push(name);
            }
        }
        unsafe { libc::closedir(stream) };
        for name in names {
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            let rc = unsafe {
                libc::fstatat(
                    dir.as_raw_fd(),
                    name.as_ptr(),
                    &mut st,
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            };
            if rc != 0 {
                let e = std::io::Error::last_os_error();
                if e.kind() == std::io::ErrorKind::NotFound {
                    continue;
                }
                return Err(e);
            }
            match st.st_mode & libc::S_IFMT {
                libc::S_IFREG if st.st_nlink > 1 => return Ok(true),
                libc::S_IFDIR => {
                    use std::os::unix::ffi::OsStrExt;
                    match open_at(
                        dir.as_raw_fd(),
                        std::ffi::OsStr::from_bytes(name.to_bytes()),
                        DIR,
                    ) {
                        Ok(sub) => stack.push(sub),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => return Err(e),
                    }
                }
                _ => {}
            }
        }
    }
    Ok(false)
}

fn refuse_shared_objects(clone_git: &File, folder: &Path) -> Result<(), Refusal> {
    let unreadable = |_| {
        (
            ErrorCode::InvalidParams,
            format!(
                "clone_of: the object files of {} cannot be read; re-clone it or remove it",
                folder.display()
            ),
        )
    };
    let objects = walk(clone_git, &["objects"]).map_err(unreadable)?;
    if first_shared(objects).map_err(unreadable)? {
        return Err((
            ErrorCode::InvalidParams,
            format!(
                "clone_of: {} shares object files with another repository; re-clone it or remove it. The runner leaves the folder as it is",
                folder.display()
            ),
        ));
    }
    Ok(())
}

fn folder_below(cwd: &Dir, root: &Path) -> Result<Vec<String>, Refusal> {
    let root_fd = open_root(root).map_err(|_| {
        (
            ErrorCode::InvalidParams,
            "clone_of: the folder is not inside <repository>-worktrees".to_string(),
        )
    })?;
    let Ok(rel) = cwd.path.strip_prefix(root) else {
        return refuse("the folder is not inside <repository>-worktrees");
    };
    let rel: Vec<&str> = rel
        .components()
        .map(|c| match c {
            Component::Normal(n) => n.to_str().unwrap_or(""),
            _ => "",
        })
        .collect();
    if rel.is_empty() || rel.iter().any(|c| c.is_empty()) {
        return refuse("the folder must be strictly inside <repository>-worktrees");
    }
    match walk(&root_fd, &rel).and_then(|f| f.metadata()) {
        Ok(m) if (m.dev(), m.ino()) == cwd.id() => {}
        _ => return refuse("the folder is reached through a symlink"),
    }
    Ok(rel.into_iter().map(String::from).collect())
}

/// Verifies a queue clone made with `git clone --reference` and returns the object directory it may read.
pub fn check(cwd: &Dir, clone_of: &str) -> Result<Objects, Refusal> {
    let repo = Repo::open(clone_of)?;
    let rel = folder_below(cwd, &repo.worktrees())?;
    let referenced = repo.check_clone(cwd.raw_fd(), &cwd.path)?;
    Ok(repo.objects(referenced && rel.len() == 1 && rel[0] != "queue"))
}

fn valid_clone_name(name: &str) -> bool {
    let mut chars = name.chars();
    (1..=100).contains(&name.len())
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
        && name != ".git"
}

/// Verifies a run folder that holds one clone per repository of a set and returns each clone's object directory.
pub fn check_multi(
    run: &Dir,
    primary: &str,
    clones: &[(String, String)],
) -> Result<Vec<Objects>, Refusal> {
    if run.path.to_str().is_none_or(|p| p.contains(':')) {
        return refuse("the run folder path must be UTF-8 without a colon");
    }
    let first = Repo::open(primary)?;
    let rel = folder_below(run, &first.worktrees())?;
    match rel.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        [name] if name != "queue" => {}
        ["queue", _] => {}
        _ => return refuse(
            "the run folder must be <primary>-worktrees/<name> or <primary>-worktrees/queue/<id>",
        ),
    }
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    let found = unsafe {
        libc::fstatat(
            run.raw_fd(),
            c".git".as_ptr(),
            &mut st,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if found == 0 || std::io::Error::last_os_error().kind() != std::io::ErrorKind::NotFound {
        return refuse("the run folder must not have a .git entry of its own");
    }
    let task_folder = rel.len() == 1;
    let mut out = Vec::with_capacity(clones.len());
    for (name, clone_of) in clones {
        if !valid_clone_name(name) {
            return refuse(format!("{name:?} is not a valid clone folder name"));
        }
        let repo = Repo::open(clone_of)?;
        let dir = open_at(run.raw_fd(), name, DIR).map_err(|_| {
            (
                ErrorCode::InvalidParams,
                format!("clone_of: {name} is not a real directory inside the run folder"),
            )
        })?;
        let referenced = repo.check_clone(dir.as_raw_fd(), &run.path.join(name))?;
        out.push(repo.objects(referenced && task_folder));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::open_path;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn a_team_member_clone_below_queue_pickup_is_accepted() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = root.join("repo");
        std::fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "initial"]);
        let pickup = root.join("repo-worktrees/queue/p_1");
        std::fs::create_dir_all(&pickup).unwrap();
        let src = repo.to_str().unwrap();
        for member in ["lead", "w1"] {
            let dest = pickup.join(member);
            git(
                &pickup,
                &[
                    "clone",
                    "-q",
                    "--no-local",
                    "--reference",
                    src,
                    src,
                    dest.to_str().unwrap(),
                ],
            );
            let objects = check(&open_path(dest).unwrap(), src).unwrap();
            assert!(!objects.task_clone);
            assert_eq!(objects.path, repo.join(".git/objects"));
        }
    }

    struct Fx {
        _tmp: tempfile::TempDir,
        repo: PathBuf,
        work: PathBuf,
    }

    fn fx() -> Fx {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = root.join("repo");
        let work = root.join("repo-worktrees");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "hello\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "init"]);
        Fx {
            _tmp: tmp,
            repo,
            work,
        }
    }

    impl Fx {
        fn hardlinked(&self, at: &Path) {
            git(
                &self.work,
                &[
                    "clone",
                    "-q",
                    "--local",
                    self.repo.to_str().unwrap(),
                    at.to_str().unwrap(),
                ],
            );
        }

        fn referenced(&self, at: &Path) {
            git(
                &self.work,
                &[
                    "clone",
                    "-q",
                    "--no-local",
                    "--reference",
                    self.repo.to_str().unwrap(),
                    self.repo.to_str().unwrap(),
                    at.to_str().unwrap(),
                ],
            );
        }

        fn check(&self, dir: &Path) -> Result<Objects, Refusal> {
            check(
                &open_path(dir.to_path_buf()).unwrap(),
                self.repo.to_str().unwrap(),
            )
        }
    }

    fn snapshot(root: &Path) -> Vec<(PathBuf, u32, u64, i64, u64, u64)> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                let m = std::fs::symlink_metadata(&p).unwrap();
                out.push((p.clone(), m.mode(), m.len(), m.mtime(), m.nlink(), m.ino()));
                if m.is_dir() {
                    stack.push(p);
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn a_clone_that_shares_object_files_is_refused_and_named() {
        let f = fx();
        let dir = f.work.join("task");
        f.hardlinked(&dir);
        let shared = std::fs::read_dir(dir.join(".git/objects"))
            .unwrap()
            .flat_map(|d| std::fs::read_dir(d.unwrap().path()).into_iter().flatten())
            .any(|e| {
                std::fs::symlink_metadata(e.unwrap().path())
                    .unwrap()
                    .nlink()
                    > 1
            });
        assert!(shared, "the fixture must hold hardlinked objects");
        let (code, msg) = f.check(&dir).unwrap_err();
        assert_eq!(code, ErrorCode::InvalidParams);
        assert!(msg.contains(dir.to_str().unwrap()), "{msg}");
        assert!(msg.contains("re-clone"), "{msg}");
        assert!(msg.contains("remove"), "{msg}");
    }

    #[test]
    fn a_reference_clone_without_shared_files_passes() {
        let f = fx();
        let dir = f.work.join("task");
        f.referenced(&dir);
        let objects = f.check(&dir).unwrap();
        assert!(objects.task_clone);
    }

    #[test]
    fn a_refused_clone_is_left_exactly_as_it_was() {
        let f = fx();
        let dir = f.work.join("task");
        f.hardlinked(&dir);
        std::fs::write(dir.join("wip.txt"), "unsaved\n").unwrap();
        let before = snapshot(&dir);
        assert!(f.check(&dir).is_err());
        assert_eq!(snapshot(&dir), before);
        assert_eq!(
            std::fs::read_to_string(dir.join("wip.txt")).unwrap(),
            "unsaved\n"
        );
    }

    #[test]
    fn every_clone_of_a_run_folder_is_checked() {
        let f = fx();
        let run = f.work.join("run");
        std::fs::create_dir_all(&run).unwrap();
        f.referenced(&run.join("good"));
        f.hardlinked(&run.join("old"));
        let repo = f.repo.to_str().unwrap().to_string();
        let one = [("good".to_string(), repo.clone())];
        let both = [one[0].clone(), ("old".to_string(), repo.clone())];
        let dir = open_path(run.clone()).unwrap();
        assert!(check_multi(&dir, &repo, &one).is_ok());
        let (_, msg) = check_multi(&dir, &repo, &both).unwrap_err();
        assert!(msg.contains(run.join("old").to_str().unwrap()), "{msg}");
    }

    #[test]
    fn a_symlink_inside_the_objects_is_not_followed() {
        let f = fx();
        let dir = f.work.join("task");
        f.referenced(&dir);
        let twin = f.work.join("twin.txt");
        std::fs::write(&twin, "x").unwrap();
        std::fs::hard_link(&twin, f.work.join("twin2.txt")).unwrap();
        std::os::unix::fs::symlink(&twin, dir.join(".git/objects/link")).unwrap();
        std::os::unix::fs::symlink(&f.repo, dir.join(".git/objects/linkdir")).unwrap();
        assert!(f.check(&dir).is_ok());
    }
}
