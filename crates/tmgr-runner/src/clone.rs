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

    fn check_clone(&self, dir: i32) -> Result<bool, Refusal> {
        let clone_git = open_at(dir, ".git", DIR).map_err(|_| {
            (
                ErrorCode::InvalidParams,
                "clone_of: .git of the folder must be a real directory (a worktree is not a clone)"
                    .to_string(),
            )
        })?;
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
    let referenced = repo.check_clone(cwd.raw_fd())?;
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
        let referenced = repo.check_clone(dir.as_raw_fd())?;
        out.push(repo.objects(referenced && task_folder));
    }
    Ok(out)
}
