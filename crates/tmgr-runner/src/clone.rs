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

/// Verifies a queue clone made with `git clone --reference` and returns the object directory it may read.
pub fn check(cwd: &Dir, clone_of: &str) -> Result<Objects, Refusal> {
    let repo = Path::new(clone_of);
    let comps: Vec<&str> = repo
        .components()
        .skip(1)
        .map(|c| match c {
            Component::Normal(n) => n.to_str().unwrap_or(""),
            _ => "",
        })
        .collect();
    if !repo.is_absolute()
        || comps.len() < 2
        || comps.iter().any(|c| c.is_empty())
        || clone_of.ends_with('/')
        || clone_of.contains('\0')
    {
        return refuse(
            "the repository must be an absolute path without symlinks, dots or a trailing slash",
        );
    }
    let name = comps[comps.len() - 1];
    let repo_fd = open_root(repo).map_err(|_| {
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
    let objects_id = walk(&git, &["objects"]).and_then(|f| f.metadata());
    let Ok(objects_id) = objects_id else {
        return refuse("the repository is not a git work tree root");
    };
    if walk(&git, &["refs"]).is_err() {
        return refuse("the repository is not a git work tree root");
    }

    let parent = repo.parent().unwrap_or(Path::new("/"));
    let root = parent.join(format!("{name}-worktrees"));
    let root_fd = open_root(&root).map_err(|_| {
        (
            ErrorCode::InvalidParams,
            "clone_of: the folder is not inside <repository>-worktrees".to_string(),
        )
    })?;
    let Ok(rel) = cwd.path.strip_prefix(&root) else {
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

    let clone_git = open_at(cwd.raw_fd(), ".git", DIR).map_err(|_| {
        (
            ErrorCode::InvalidParams,
            "clone_of: .git of the folder must be a real directory (a worktree is not a clone)"
                .to_string(),
        )
    })?;
    let want = repo.join(".git/objects");
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
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return refuse("alternates is not a plain file"),
    }
    Ok(Objects {
        path: PathBuf::from(want),
        id: (objects_id.dev(), objects_id.ino()),
    })
}
