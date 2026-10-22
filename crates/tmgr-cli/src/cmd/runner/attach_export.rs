use super::attach_questions::{session_src, QuestionJob};
use super::attach_title::AgentSrc;
use std::ffi::CString;
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::{Path, PathBuf};
use tmgr_core::error::{Error, Result};
use tmgr_runner::attach::{
    export_transcript, fmt_size, scan_export, Decision, ExportState, Finding,
};
use tmgr_runner::questions::transcript_path;

pub(super) fn safe_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .take(40)
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "session".into()
    } else {
        s
    }
}

pub(super) fn page_title(name: &str, now: &str) -> String {
    format!("Session {} {}", safe_name(name), now)
}

pub(super) struct Loaded {
    pub lines: Vec<String>,
    pub findings: Vec<Finding>,
}

pub(super) fn load(job: &QuestionJob, name: &str, home: &str) -> Option<Loaded> {
    let src = session_src(job)?;
    let path = transcript_path(&src)?;
    let lines = export_transcript(&src.home, &path, &safe_name(name), &job.src.cwd, home)?;
    let findings = scan_export(&lines, home);
    Some(Loaded { lines, findings })
}

pub(super) fn state_for(
    loaded: Loaded,
    name: &str,
    ws_code: Option<String>,
    page_refusal: Option<String>,
    path: &Path,
    home: &str,
) -> ExportState {
    ExportState::new(
        page_title(
            name,
            &chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
        ),
        loaded.lines,
        loaded.findings,
        ws_code,
        page_refusal,
        path.display().to_string(),
        home.to_string(),
    )
}

pub(super) fn default_path(name: &str, home: &Path) -> PathBuf {
    home.join(format!(
        "tmgr-session-{}-{}.md",
        safe_name(name),
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    ))
}

fn cstr(s: &std::ffi::OsStr) -> Result<CString> {
    use std::os::unix::ffi::OsStrExt;
    CString::new(s.as_bytes()).map_err(|_| Error::usage("the file name contains a NUL byte"))
}

fn file_name_ok(name: &std::ffi::OsStr) -> bool {
    let s = name.to_string_lossy();
    !s.is_empty() && s != "." && s != ".." && !s.contains('/') && !s.chars().any(char::is_control)
}

pub(super) fn dir_is_safe(dir: &Path) -> Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let meta =
        std::fs::symlink_metadata(dir).map_err(|_| Error::usage("the folder does not exist"))?;
    if !meta.is_dir() {
        return Err(Error::usage("the target folder is not a real folder"));
    }
    let me = unsafe { libc::geteuid() };
    if meta.uid() != me || meta.permissions().mode() & 0o022 != 0 {
        return Err(
            Error::usage("the target folder is not yours or others can write to it")
                .hint("pick a private folder such as your home folder"),
        );
    }
    Ok(())
}

pub(super) fn write_private(path: &Path, bytes: &[u8], overwrite: bool) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .filter(|n| file_name_ok(n))
        .ok_or_else(|| Error::usage("invalid file name"))?;
    dir_is_safe(dir)?;
    let dirfd = unsafe {
        libc::open(
            cstr(dir.as_os_str())?.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if dirfd < 0 {
        return Err(Error::usage("cannot open the target folder"));
    }
    let dirf = unsafe { std::fs::File::from_raw_fd(dirfd) };
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let m = dirf
            .metadata()
            .map_err(|_| Error::usage("cannot inspect the folder"))?;
        if m.uid() != unsafe { libc::geteuid() } || m.permissions().mode() & 0o022 != 0 {
            return Err(Error::usage(
                "the target folder is not yours or others can write to it",
            ));
        }
    }
    let tmp = format!(
        ".tmgr-export-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    );
    let tmp_c = CString::new(tmp.as_bytes()).map_err(|_| Error::internal("temp name"))?;
    let fd = unsafe {
        libc::openat(
            dirf.as_raw_fd(),
            tmp_c.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600 as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(Error::usage("cannot create the file"));
    }
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let unlink = || unsafe {
        libc::unlinkat(dirf.as_raw_fd(), tmp_c.as_ptr(), 0);
    };
    if file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .is_err()
    {
        unlink();
        return Err(Error::usage("cannot write the file"));
    }
    drop(file);
    let name_c = CString::new(name.as_bytes()).map_err(|_| Error::usage("invalid file name"))?;
    let rc = unsafe {
        if overwrite {
            libc::renameat(
                dirf.as_raw_fd(),
                tmp_c.as_ptr(),
                dirf.as_raw_fd(),
                name_c.as_ptr(),
            )
        } else {
            libc::linkat(
                dirf.as_raw_fd(),
                tmp_c.as_ptr(),
                dirf.as_raw_fd(),
                name_c.as_ptr(),
                0,
            )
        }
    };
    let exists =
        rc != 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::AlreadyExists;
    if !overwrite || rc != 0 {
        unlink();
    }
    if exists {
        return Err(
            Error::usage(format!("{} already exists", name.to_string_lossy()))
                .hint("nothing was overwritten"),
        );
    }
    if rc != 0 {
        return Err(Error::usage("cannot place the file"));
    }
    Ok(())
}

pub(super) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

pub(super) fn write_export(text: &str, path: &Path) -> std::result::Result<String, String> {
    write_private(path, text.as_bytes(), false)
        .map(|()| format!("exported {} to {}", fmt_size(text.len()), path.display()))
        .map_err(|e| e.message)
}

pub(super) fn post_page(
    code: &str,
    title: &str,
    text: &str,
) -> std::result::Result<String, String> {
    use clap::Parser;
    if super::attach_new::in_agent() {
        return Err("page export is refused inside an agent session".into());
    }
    let cli = crate::cli::Cli::parse_from(["tmgr"]);
    let ctx = crate::ctx::Ctx::new(&cli).map_err(|e| e.to_string())?;
    let client = ctx.client().map_err(|e| e.to_string())?;
    let ws = ctx.workspace(client).map_err(|e| e.to_string())?;
    if !ws.code.eq_ignore_ascii_case(code) {
        return Err("the selected workspace changed, nothing was sent".into());
    }
    let page = client
        .page_create(ws.id, &serde_json::json!({"title": title, "body": text}))
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "exported to a new page in {} (id {})",
        ws.code, page.id
    ))
}

pub(super) fn new_job(
    id: &str,
    src: AgentSrc,
    pid: u32,
    cfg: &tmgr_runner::daemon::RunnerConfig,
    exited: bool,
) -> QuestionJob {
    QuestionJob {
        id: id.to_string(),
        src,
        pid,
        sandbox_home: cfg.homes.join(id),
        log: cfg.dir.join("sessions").join(id).join("output.log"),
        exited,
        settled: true,
    }
}

pub(super) fn decide_all(f: &[Finding], remove: bool) -> Vec<Decision> {
    f.iter()
        .map(|x| {
            if remove || !x.keepable {
                Decision::Remove
            } else {
                Decision::Keep
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn private_dir() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        d
    }

    #[test]
    fn a_file_is_written_private_and_never_overwritten() {
        let d = private_dir();
        let p = d.path().join("out.md");
        write_private(&p, b"hello", false).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"hello");
        assert_eq!(
            std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let e = write_private(&p, b"other", false).unwrap_err();
        assert!(e.message.contains("already exists"));
        assert_eq!(std::fs::read(&p).unwrap(), b"hello");
        write_private(&p, b"other", true).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"other");
        let left: Vec<_> = std::fs::read_dir(d.path()).unwrap().collect();
        assert_eq!(left.len(), 1);
    }

    #[test]
    fn a_symlinked_target_is_not_followed() {
        let d = private_dir();
        let real = d.path().join("real.md");
        std::fs::write(&real, "keep").unwrap();
        let link = d.path().join("link.md");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(write_private(&link, b"x", false).is_err());
        assert_eq!(std::fs::read(&real).unwrap(), b"keep");
        let other = private_dir();
        let dir_link = d.path().join("sub");
        std::os::unix::fs::symlink(other.path(), &dir_link).unwrap();
        assert!(write_private(&dir_link.join("a.md"), b"x", false).is_err());
    }

    #[test]
    fn a_world_writable_folder_is_refused() {
        let d = tempfile::tempdir().unwrap();
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(write_private(&d.path().join("a.md"), b"x", false).is_err());
    }

    #[test]
    fn names_are_reduced_to_safe_characters() {
        assert_eq!(safe_name("../../etc/pass wd\n"), "etc-pass-wd");
        assert_eq!(safe_name("???"), "session");
        assert!(safe_name(&"a".repeat(100)).len() <= 40);
        assert_eq!(
            page_title("my run", "2026-10-22 10:00"),
            "Session my-run 2026-10-22 10:00"
        );
    }

    #[test]
    fn decide_all_never_keeps_what_cannot_be_kept() {
        let f = |keepable| Finding {
            line: 0,
            kind: "secret",
            keepable,
            original: String::new(),
            cleaned: String::new(),
        };
        assert_eq!(
            decide_all(&[f(true), f(false)], false),
            vec![Decision::Keep, Decision::Remove]
        );
        assert_eq!(decide_all(&[f(true)], true), vec![Decision::Remove]);
    }
}
