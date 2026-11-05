use crate::bundle::{open_at, open_root, Dir, Refusal};
use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use tmgr_ipc::api::{ErrorCode, TeamLink, TeamRole};

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

    fn check_clone(&self, dir: i32, shown: &Path) -> Result<bool, Refusal> {
        let clone_git = open_at(dir, ".git", DIR).map_err(|_| {
            (
                ErrorCode::InvalidParams,
                "clone_of: .git of the folder must be a real directory (a worktree is not a clone)"
                    .to_string(),
            )
        })?;
        match walk(&clone_git, &["objects"]).and_then(|o| shares_files(o, 0)) {
            Ok(false) => {}
            Ok(true) => {
                return refuse(format!(
                    "{} shares object files with another repository (it was cloned by tmgr before 1.0.63); \
                     re-clone it or remove it. tmgr leaves the folder as it is",
                    shown.display()
                ))
            }
            Err(_) => {
                return refuse(format!(
                    "the object files of {} cannot be checked",
                    shown.display()
                ))
            }
        }
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

const MAX_DEPTH: u32 = 8;

fn clear_errno() {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    unsafe {
        *libc::__errno_location() = 0
    };
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
    unsafe {
        *libc::__error() = 0
    };
}

fn entries(dir: &File) -> std::io::Result<Vec<(std::ffi::CString, u8)>> {
    let fd = dir.as_raw_fd();
    let dup = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
    if dup < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let stream = unsafe { libc::fdopendir(dup) };
    if stream.is_null() {
        let e = std::io::Error::last_os_error();
        unsafe { libc::close(dup) };
        return Err(e);
    }
    unsafe { libc::rewinddir(stream) };
    let mut out = Vec::new();
    loop {
        clear_errno();
        let ent = unsafe { libc::readdir(stream) };
        if ent.is_null() {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error().is_some_and(|n| n != 0) {
                unsafe { libc::closedir(stream) };
                return Err(e);
            }
            break;
        }
        let name = unsafe { std::ffi::CStr::from_ptr((*ent).d_name.as_ptr()) };
        if name != c"." && name != c".." {
            out.push((name.to_owned(), unsafe { (*ent).d_type }));
        }
    }
    unsafe { libc::closedir(stream) };
    Ok(out)
}

fn gone(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::NotFound
        || matches!(e.raw_os_error(), Some(n) if n == libc::ELOOP || n == libc::ENOTDIR)
}

fn shares_files(dir: File, depth: u32) -> std::io::Result<bool> {
    if depth > MAX_DEPTH {
        return Err(std::io::Error::other("too deep"));
    }
    let fd = dir.as_raw_fd();
    for (name, _) in entries(&dir)? {
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstatat(fd, name.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) } != 0 {
            let e = std::io::Error::last_os_error();
            if gone(&e) {
                continue;
            }
            return Err(e);
        }
        match st.st_mode & libc::S_IFMT {
            libc::S_IFDIR => {
                let sub = match open_at(fd, std::ffi::OsStr::from_bytes(name.to_bytes()), DIR) {
                    Ok(sub) => sub,
                    Err(e) if gone(&e) => continue,
                    Err(e) => return Err(e),
                };
                if shares_files(sub, depth + 1)? {
                    return Ok(true);
                }
            }
            libc::S_IFLNK => {}
            _ if st.st_nlink > 1 => return Ok(true),
            _ => {}
        }
    }
    Ok(false)
}

const WALK_ENTRIES: usize = 1_000_000;
const WALK_DEPTH: u32 = 64;

enum Stop {
    Shared(PathBuf),
    Unchecked(PathBuf),
    Budget,
}

fn is_dir_entry(dir: i32, name: &std::ffi::CStr, kind: u8) -> bool {
    if kind != libc::DT_UNKNOWN {
        return kind == libc::DT_DIR;
    }
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    let found = unsafe { libc::fstatat(dir, name.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) };
    found == 0 && st.st_mode & libc::S_IFMT == libc::S_IFDIR
}

fn walk_stores(dir: File, path: &Path, depth: u32, left: &mut usize) -> Result<(), Stop> {
    let fd = dir.as_raw_fd();
    let list = match entries(&dir) {
        Ok(l) => l,
        Err(e) if gone(&e) => return Ok(()),
        Err(_) => return Err(Stop::Unchecked(path.to_path_buf())),
    };
    *left = left.checked_sub(list.len()).ok_or(Stop::Budget)?;
    let has = |want: &std::ffi::CStr| {
        list.iter()
            .any(|(n, k)| n.as_c_str() == want && is_dir_entry(fd, n, *k))
    };
    let named = path.file_name().is_some_and(|n| n == "objects");
    let fanout = list.iter().any(|(n, k)| {
        n.to_bytes().len() == 2
            && n.to_bytes().iter().all(u8::is_ascii_hexdigit)
            && is_dir_entry(fd, n, *k)
    });
    if has(c"pack") && (named || has(c"info") || fanout) {
        return match shares_files(dir, 0) {
            Ok(false) => Ok(()),
            Ok(true) => Err(Stop::Shared(path.to_path_buf())),
            Err(e) if gone(&e) => Ok(()),
            Err(_) => Err(Stop::Unchecked(path.to_path_buf())),
        };
    }
    for (name, kind) in &list {
        if !is_dir_entry(fd, name, *kind) {
            continue;
        }
        if depth >= WALK_DEPTH {
            return Err(Stop::Budget);
        }
        let name = std::ffi::OsStr::from_bytes(name.to_bytes());
        let child = match open_at(fd, name, DIR) {
            Ok(c) => c,
            Err(e) if gone(&e) => continue,
            Err(_) => return Err(Stop::Unchecked(path.join(name))),
        };
        walk_stores(child, &path.join(name), depth + 1, left)?;
    }
    Ok(())
}

/// Refuses a folder with git object files that are also linked from elsewhere: a sandboxed session could change them there.
pub fn check_stores(cwd: &Dir) -> Result<(), Refusal> {
    check_stores_within(cwd, WALK_ENTRIES)
}

fn check_stores_within(cwd: &Dir, budget: usize) -> Result<(), Refusal> {
    let refused = |m: String| (ErrorCode::InvalidParams, m);
    let own = open_at(cwd.raw_fd(), ".", DIR).map_err(|_| {
        refused(format!(
            "the files under {} cannot be checked",
            cwd.path.display()
        ))
    })?;
    let mut left = budget;
    match walk_stores(own, &cwd.path, 0, &mut left) {
        Ok(()) => Ok(()),
        Err(Stop::Shared(p)) => Err(refused(format!(
            "{} has object files that are also linked from another place, so a sandboxed session could change them there; \
             re-clone the repository with `git clone --no-local <repository> <new folder>` (uncommitted changes and stashes are not carried over) \
             or start the session in another folder. tmgr leaves the folder as it is",
            p.display()
        ))),
        Err(Stop::Unchecked(p)) => Err(refused(format!(
            "the files under {} cannot be checked for shared git object files",
            p.display()
        ))),
        Err(Stop::Budget) => Err(refused(format!(
            "{} is too large to check for shared git object files; start the session in a smaller folder",
            cwd.path.display()
        ))),
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
    team: Option<&TeamLink>,
) -> Result<Vec<Objects>, Refusal> {
    if run.path.to_str().is_none_or(|p| p.contains(':')) {
        return refuse("the run folder path must be UTF-8 without a colon");
    }
    let first = Repo::open(primary)?;
    let rel = folder_below(run, &first.worktrees())?;
    let rel: Vec<&str> = rel.iter().map(String::as_str).collect();
    match (team, &rel[..]) {
        (None, [name]) if *name != "queue" => {}
        (None, ["queue", _]) => {}
        (Some(t), ["queue", id, member])
            if *id == t.of
                && *member
                    == match t.role {
                        TeamRole::Lead => "lead".to_string(),
                        TeamRole::Worker(k) => format!("w{k}"),
                    } => {}
        (Some(_), _) => {
            return refuse(
                "a team member folder must be <primary>-worktrees/queue/<pickup id>/<lead or wK> for its own role",
            )
        }
        _ => {
            return refuse(
                "the run folder must be <primary>-worktrees/<name> or <primary>-worktrees/queue/<id>",
            )
        }
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
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {out:?}");
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

    fn repo_at(root: &Path) -> PathBuf {
        let repo = root.join("repo");
        std::fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "a\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "a"]);
        git(&repo, &["gc", "-q"]);
        std::fs::write(repo.join("b.txt"), "b\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "b"]);
        repo
    }

    fn clone_at(repo: &Path, dest: &Path, hardlinked: bool) {
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        let src = repo.to_str().unwrap();
        let mut args = vec!["clone", "-q"];
        if !hardlinked {
            args.push("--no-local");
        }
        args.extend(["--reference", src, src, dest.to_str().unwrap()]);
        git(dest.parent().unwrap(), &args);
    }

    fn linked_files(dir: &Path) -> usize {
        let mut n = 0;
        let mut todo = vec![dir.join(".git/objects")];
        while let Some(d) = todo.pop() {
            for e in std::fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                let m = std::fs::symlink_metadata(&p).unwrap();
                if m.is_dir() {
                    todo.push(p);
                } else if m.nlink() > 1 {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn a_clone_that_shares_object_files_with_its_source_is_refused_and_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = repo_at(&root);
        let dest = root.join("repo-worktrees/old");
        clone_at(&repo, &dest, true);
        std::fs::write(dest.join("wip.txt"), "uncommitted\n").unwrap();
        assert!(linked_files(&dest) > 0);
        let (code, msg) =
            check(&open_path(dest.clone()).unwrap(), repo.to_str().unwrap()).unwrap_err();
        assert_eq!(code, ErrorCode::InvalidParams);
        assert!(msg.contains(dest.to_str().unwrap()), "{msg}");
        assert!(msg.contains("re-clone it or remove it"), "{msg}");
        assert!(dest.join("wip.txt").is_file());
        assert!(linked_files(&dest) > 0);
    }

    #[test]
    fn one_shared_object_file_is_enough_to_refuse() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = repo_at(&root);
        let dest = root.join("repo-worktrees/fresh");
        clone_at(&repo, &dest, false);
        let src = repo.to_str().unwrap();
        assert_eq!(linked_files(&dest), 0);
        check(&open_path(dest.clone()).unwrap(), src).unwrap();
        std::fs::write(dest.join("c.txt"), "c\n").unwrap();
        git(&dest, &["add", "."]);
        git(&dest, &["commit", "-q", "-m", "c"]);
        git(&dest, &["commit-graph", "write", "--reachable"]);
        let graph = dest.join(".git/objects/info/commit-graph");
        assert!(graph.is_file());
        std::fs::hard_link(&graph, root.join("graph-copy")).unwrap();
        let (_, msg) = check(&open_path(dest.clone()).unwrap(), src).unwrap_err();
        assert!(msg.contains(dest.to_str().unwrap()), "{msg}");
        std::fs::remove_file(root.join("graph-copy")).unwrap();
        check(&open_path(dest).unwrap(), src).unwrap();
    }

    #[test]
    fn a_run_folder_with_one_shared_clone_is_refused_naming_that_clone() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = repo_at(&root);
        let run = root.join("repo-worktrees/task");
        clone_at(&repo, &run.join("fe"), false);
        clone_at(&repo, &run.join("be"), true);
        let src = repo.to_str().unwrap().to_string();
        let clones = vec![
            ("fe".to_string(), src.clone()),
            ("be".to_string(), src.clone()),
        ];
        let (_, msg) =
            check_multi(&open_path(run.clone()).unwrap(), &src, &clones, None).unwrap_err();
        assert!(msg.contains(run.join("be").to_str().unwrap()), "{msg}");
        let fresh = vec![("fe".to_string(), src.clone())];
        check_multi(&open_path(run).unwrap(), &src, &fresh, None).unwrap();
    }

    fn team_of(role: TeamRole, of: &str) -> TeamLink {
        TeamLink {
            role,
            of: of.into(),
        }
    }

    fn member_folder(root: &Path, rel: &str) -> (PathBuf, String, Vec<(String, String)>) {
        let repo = repo_at(root);
        let run = root.join("repo-worktrees").join(rel);
        clone_at(&repo, &run.join("fe"), false);
        let src = repo.to_str().unwrap().to_string();
        let clones = vec![("fe".to_string(), src.clone())];
        (run, src, clones)
    }

    fn multi_with(rel: &str, team: Option<&TeamLink>) -> Result<(), Refusal> {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let (run, src, clones) = member_folder(&root, rel);
        check_multi(&open_path(run).unwrap(), &src, &clones, team).map(|_| ())
    }

    #[test]
    fn a_multi_repo_team_member_folder_matches_its_role_and_pickup() {
        let lead = team_of(TeamRole::Lead, "p_1");
        multi_with("queue/p_1/lead", Some(&lead)).unwrap();
        multi_with("queue/p_1/w2", Some(&team_of(TeamRole::Worker(2), "p_1"))).unwrap();
        multi_with("queue/p_1/w2", Some(&lead)).unwrap_err();
        multi_with("queue/p_1/lead", Some(&team_of(TeamRole::Worker(1), "p_1"))).unwrap_err();
        multi_with("queue/p_1/w1", Some(&team_of(TeamRole::Worker(2), "p_1"))).unwrap_err();
        multi_with("queue/p_2/lead", Some(&lead)).unwrap_err();
    }

    #[test]
    fn team_and_single_agent_multi_shapes_do_not_mix() {
        let lead = team_of(TeamRole::Lead, "p_1");
        multi_with("queue/p_1/lead", None).unwrap_err();
        multi_with("queue/p_1", Some(&lead)).unwrap_err();
        multi_with("task", Some(&lead)).unwrap_err();
        multi_with("queue/p_1", None).unwrap();
        multi_with("task", None).unwrap();
    }

    #[test]
    fn a_member_folder_with_its_own_git_entry_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let (run, src, clones) = member_folder(&root, "queue/p_1/lead");
        std::fs::create_dir(run.join(".git")).unwrap();
        let lead = team_of(TeamRole::Lead, "p_1");
        let (_, msg) =
            check_multi(&open_path(run).unwrap(), &src, &clones, Some(&lead)).unwrap_err();
        assert!(msg.contains(".git"), "{msg}");
    }
    fn plain_clone(repo: &Path, dest: &Path) {
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        git(
            dest.parent().unwrap(),
            &[
                "clone",
                "-q",
                repo.to_str().unwrap(),
                dest.to_str().unwrap(),
            ],
        );
    }

    fn refusal_of(dir: &Path) -> String {
        check_stores(&open_path(dir.to_path_buf()).unwrap())
            .unwrap_err()
            .1
    }

    fn accepted(dir: &Path) {
        check_stores(&open_path(dir.to_path_buf()).unwrap()).unwrap();
    }

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = repo_at(&root);
        (tmp, root, repo)
    }

    #[test]
    fn session_folder_refused_when_object_store_shares_files() {
        let (_t, root, repo) = fixture();
        let dest = root.join("copy");
        plain_clone(&repo, &dest);
        assert!(linked_files(&dest) > 0);
        let (code, msg) = check_stores(&open_path(dest.clone()).unwrap()).unwrap_err();
        assert_eq!(code, ErrorCode::InvalidParams);
        assert!(msg.contains(dest.to_str().unwrap()), "{msg}");
        assert!(msg.contains("git clone --no-local"), "{msg}");
        assert!(linked_files(&dest) > 0);
    }

    #[test]
    fn reclone_clears_shared_object_files_in_every_state() {
        let (_t, root, repo) = fixture();
        git(&repo, &["commit-graph", "write", "--split", "--reachable"]);
        let dest = root.join("copy");
        plain_clone(&repo, &dest);
        std::fs::write(dest.join("d.txt"), "d\n").unwrap();
        git(&dest, &["add", "."]);
        git(&dest, &["commit", "-q", "-m", "d"]);
        git(&dest, &["commit", "-q", "--amend", "-m", "d2"]);
        std::fs::write(dest.join("a.txt"), "changed\n").unwrap();
        git(&dest, &["stash", "-q"]);
        std::fs::write(dest.join("blob"), "dangling\n").unwrap();
        git(&dest, &["hash-object", "-w", "blob"]);
        let pack = std::fs::read_dir(dest.join(".git/objects/pack"))
            .unwrap()
            .flatten()
            .find(|e| e.path().extension().is_some_and(|x| x == "pack"))
            .unwrap()
            .path();
        std::fs::write(pack.with_extension("keep"), "").unwrap();
        refusal_of(&dest);
        let fresh = root.join("fresh");
        git(
            &root,
            &[
                "clone",
                "-q",
                "--no-local",
                dest.to_str().unwrap(),
                fresh.to_str().unwrap(),
            ],
        );
        assert_eq!(linked_files(&fresh), 0);
        accepted(&fresh);
        git(&fresh, &["fsck"]);
    }

    #[test]
    fn folders_without_a_shared_object_store_are_accepted() {
        let (_t, root, repo) = fixture();
        let plain = root.join("plain");
        std::fs::create_dir(&plain).unwrap();
        accepted(&plain);
        accepted(&repo);
        let fresh = root.join("fresh");
        clone_at(&repo, &fresh, false);
        accepted(&fresh);
        let tree = root.join("work");
        std::fs::create_dir_all(tree.join("node_modules/.pnpm/pkg")).unwrap();
        std::fs::write(tree.join("node_modules/.pnpm/pkg/index.js"), "x\n").unwrap();
        std::fs::create_dir_all(tree.join("node_modules/dep")).unwrap();
        std::fs::hard_link(
            tree.join("node_modules/.pnpm/pkg/index.js"),
            tree.join("node_modules/dep/index.js"),
        )
        .unwrap();
        git(&tree, &["init", "-q", "-b", "main"]);
        accepted(&tree);
    }

    #[test]
    fn reference_clone_with_alternates_is_accepted() {
        let (_t, root, repo) = fixture();
        let dest = root.join("repo-worktrees/ref");
        clone_at(&repo, &dest, false);
        assert!(dest.join(".git/objects/info/alternates").is_file());
        accepted(&dest);
    }

    #[test]
    fn linked_worktree_folder_has_no_store_to_check() {
        let (_t, root, repo) = fixture();
        let wt = root.join("wt");
        git(&repo, &["worktree", "add", "-q", wt.to_str().unwrap()]);
        assert!(wt.join(".git").is_file());
        accepted(&wt);
    }

    #[test]
    fn child_repository_at_any_depth_is_checked() {
        let (_t, root, repo) = fixture();
        let folder = root.join("folder");
        plain_clone(&repo, &folder.join("child"));
        assert!(refusal_of(&folder).contains(folder.join("child").to_str().unwrap()));
        let deep = root.join("deep");
        plain_clone(&repo, &deep.join("a/b/c/child"));
        assert!(refusal_of(&deep).contains("a/b/c/child"));
    }

    #[test]
    fn bare_stores_are_checked() {
        let (_t, root, repo) = fixture();
        let folder = root.join("folder");
        std::fs::create_dir(&folder).unwrap();
        git(
            &folder,
            &[
                "clone",
                "-q",
                "--bare",
                repo.to_str().unwrap(),
                folder.join("foo.git").to_str().unwrap(),
            ],
        );
        assert!(refusal_of(&folder).contains("foo.git"));
        let bare = root.join("bare");
        git(
            &root,
            &[
                "clone",
                "-q",
                "--bare",
                repo.to_str().unwrap(),
                bare.to_str().unwrap(),
            ],
        );
        refusal_of(&bare);
    }

    #[test]
    fn redirected_git_entries_inside_the_folder_are_checked() {
        let (_t, root, repo) = fixture();
        let by_file = root.join("by_file");
        plain_clone(&repo, &by_file.join("work"));
        std::fs::rename(by_file.join("work/.git"), by_file.join("store.git")).unwrap();
        std::fs::write(by_file.join("work/.git"), "gitdir: ../store.git\n").unwrap();
        assert!(refusal_of(&by_file).contains("store.git"));

        let by_link = root.join("by_link");
        plain_clone(&repo, &by_link.join("work"));
        std::fs::rename(by_link.join("work/.git"), by_link.join("store.git")).unwrap();
        std::os::unix::fs::symlink("../store.git", by_link.join("work/.git")).unwrap();
        assert!(refusal_of(&by_link).contains("store.git"));

        let by_objects = root.join("by_objects");
        plain_clone(&repo, &by_objects.join("work"));
        std::fs::rename(
            by_objects.join("work/.git/objects"),
            by_objects.join("objs"),
        )
        .unwrap();
        std::fs::create_dir_all(by_objects.join("objs/info")).unwrap();
        std::os::unix::fs::symlink("../../objs", by_objects.join("work/.git/objects")).unwrap();
        assert!(refusal_of(&by_objects).contains("objs"));
    }

    #[test]
    fn submodule_store_is_named_and_deep_logs_are_not_refused() {
        let (_t, root, repo) = fixture();
        let host = root.join("host");
        clone_at(&repo, &host, false);
        let module = host.join(".git/modules/sub");
        let deep = module.join("logs/refs/a/b/c/d/e/f/g/h/i/j");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("log"), "x\n").unwrap();
        std::fs::create_dir_all(module.join("objects/info")).unwrap();
        std::fs::create_dir_all(module.join("objects/pack")).unwrap();
        std::fs::create_dir_all(module.join("objects/ab")).unwrap();
        accepted(&host);
        std::fs::hard_link(repo.join(".git/HEAD"), module.join("objects/ab/cdef")).unwrap();
        assert!(refusal_of(&host).contains(".git/modules/sub/objects"));
    }

    #[test]
    fn unreadable_folders_are_refused_and_a_small_budget_is_exceeded() {
        use std::os::unix::fs::PermissionsExt;
        let (_t, root, _repo) = fixture();
        let folder = root.join("folder");
        let hidden = folder.join("hidden");
        std::fs::create_dir_all(&hidden).unwrap();
        for i in 0..10 {
            std::fs::write(folder.join(format!("f{i}")), "").unwrap();
        }
        let err = check_stores_within(&open_path(folder.clone()).unwrap(), 5)
            .unwrap_err()
            .1;
        assert!(err.contains("too large"), "{err}");
        accepted(&folder);
        std::fs::set_permissions(&hidden, std::fs::Permissions::from_mode(0o000)).unwrap();
        let readable = std::fs::read_dir(&hidden).is_ok();
        let result = check_stores(&open_path(folder).unwrap());
        std::fs::set_permissions(&hidden, std::fs::Permissions::from_mode(0o755)).unwrap();
        if !readable {
            assert!(result.unwrap_err().1.contains("cannot be checked"));
        }
    }
}
