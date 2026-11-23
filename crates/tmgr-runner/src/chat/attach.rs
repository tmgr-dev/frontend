use std::collections::HashMap;
use std::ffi::CString;
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use tmgr_ipc::chat::Attachment;

use super::blobs::BlobStore;

pub const DIR_NAME: &str = "tmgr-attachments";
pub const MAX_TOTAL: u64 = 25 * 1024 * 1024;
const NAME_MAX: usize = 80;
const MIME_MAX: usize = 100;
const NUMBERED_TRIES: u32 = 4;
const RANDOM_TRIES: u32 = 4;
const SHA_PREFIX: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Home(PathBuf),
    Fallback { root: PathBuf, id: String },
}

pub fn resolve(homes: &Path, fallback_root: &Path, id: &str, no_sandbox: bool) -> Target {
    if no_sandbox {
        Target::Fallback {
            root: fallback_root.to_path_buf(),
            id: id.to_owned(),
        }
    } else {
        Target::Home(homes.join(id))
    }
}

pub fn safe_name(name: &str) -> String {
    let mut s: String = name
        .chars()
        .filter(|c| !c.is_control() && !tmgr_ipc::manager::invisible(*c))
        .map(|c| {
            if matches!(c, '/' | '\\' | ':') {
                '_'
            } else {
                c
            }
        })
        .collect();
    while s.contains("..") {
        s = s.replace("..", ".");
    }
    let s = s.trim_start_matches('.').trim();
    let mut end = s.len().min(NAME_MAX);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    let s = s[..end].trim_end();
    if s.is_empty() {
        "file".to_owned()
    } else {
        s.to_owned()
    }
}

pub fn disk_name(display: &str) -> String {
    let mut out = String::with_capacity(display.len());
    for c in display.chars() {
        let c = if c.is_ascii_alphanumeric() || matches!(c, '.' | '-') {
            c
        } else {
            '_'
        };
        if c != '_' || !out.ends_with('_') {
            out.push(c);
        }
    }
    out
}

fn safe_mime(mime: &str) -> String {
    let s: String = mime
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '+' | '-' | '_'))
        .take(MIME_MAX)
        .collect();
    if s.is_empty() {
        "application/octet-stream".to_owned()
    } else {
        s
    }
}

pub fn human_size(n: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut v = n as f64 / 1024.0;
    let mut i = 0;
    while v >= 1024.0 && i + 1 < UNITS.len() {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", UNITS[i])
}

pub fn turn_text(
    body: &str,
    atts: &[Attachment],
    blobs: Option<&BlobStore>,
    target: Option<&Target>,
) -> String {
    if atts.is_empty() {
        return body.to_owned();
    }
    let dir = target.and_then(|t| open_attachments_dir(t).ok());
    let mut budget = MAX_TOTAL;
    let mut seen: HashMap<(String, String), (PathBuf, u64)> = HashMap::new();
    let lines: Vec<String> = atts
        .iter()
        .map(|a| {
            let name = safe_name(&a.name);
            let key = (a.sha256.clone(), disk_name(&name));
            let staged = match seen.get(&key) {
                Some(done) => Some(done.clone()),
                None => {
                    let made = match (&dir, blobs.and_then(|b| b.get(&a.sha256))) {
                        (Some((fd, path)), Some(src)) => {
                            copy_in(fd, path, &src, &a.sha256, &key.1, &mut budget).ok()
                        }
                        _ => None,
                    };
                    if let Some(done) = &made {
                        seen.insert(key, done.clone());
                    }
                    made
                }
            };
            match staged {
                Some((path, size)) => format!(
                    "- {name} ({}, {}): {}",
                    safe_mime(&a.mime),
                    human_size(size),
                    path.display()
                ),
                None => format!("- {name}: (not received)"),
            }
        })
        .collect();
    let list = format!("Attached files:\n{}", lines.join("\n"));
    if body.trim().is_empty() {
        list
    } else {
        format!("{body}\n\n{list}")
    }
}

fn c_name(name: &str) -> io::Result<CString> {
    CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "bad file name"))
}

fn owned(fd: libc::c_int) -> io::Result<OwnedFd> {
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
}

fn open_dir_path(path: &Path) -> io::Result<OwnedFd> {
    let c = CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "bad path"))?;
    owned(unsafe {
        libc::open(
            c.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    })
}

fn open_dir_at(parent: &OwnedFd, name: &str, create: bool) -> io::Result<OwnedFd> {
    let c = c_name(name)?;
    if create {
        let r = unsafe { libc::mkdirat(parent.as_raw_fd(), c.as_ptr(), 0o700) };
        if r < 0 && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists {
            return Err(io::Error::last_os_error());
        }
    }
    owned(unsafe {
        libc::openat(
            parent.as_raw_fd(),
            c.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    })
}

fn require_mine(fd: &OwnedFd, tighten: bool) -> io::Result<()> {
    let f = std::mem::ManuallyDrop::new(unsafe { File::from_raw_fd(fd.as_raw_fd()) });
    let meta = f.metadata()?;
    if !meta.file_type().is_dir() || meta.uid() != unsafe { libc::geteuid() } {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a directory of this user",
        ));
    }
    if tighten && meta.mode() & 0o077 != 0 && unsafe { libc::fchmod(fd.as_raw_fd(), 0o700) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn open_attachments_dir(target: &Target) -> io::Result<(OwnedFd, PathBuf)> {
    let (parent, parent_path) = match target {
        Target::Home(home) => {
            let fd = open_dir_path(home)?;
            require_mine(&fd, false)?;
            (fd, home.clone())
        }
        Target::Fallback { root, id } => {
            tmgr_ipc::dirs::ensure_private_dir(root)?;
            let root_fd = open_dir_path(root)?;
            require_mine(&root_fd, false)?;
            let fd = open_dir_at(&root_fd, id, true)?;
            require_mine(&fd, true)?;
            (fd, root.join(id))
        }
    };
    let dir = open_dir_at(&parent, DIR_NAME, true)?;
    require_mine(&dir, true)?;
    Ok((dir, parent_path.join(DIR_NAME)))
}

fn copy_in(
    dir: &OwnedFd,
    dir_path: &Path,
    src: &Path,
    sha: &str,
    name: &str,
    budget: &mut u64,
) -> io::Result<(PathBuf, u64)> {
    let mut from = File::open(src)?;
    let size = from.metadata()?.len();
    if size > *budget {
        return Err(io::Error::other("over the per-message limit"));
    }
    *budget -= size;
    let prefix = sha.get(..SHA_PREFIX).unwrap_or(sha);
    for n in 0..NUMBERED_TRIES + RANDOM_TRIES {
        let file_name = match n {
            0 => format!("{prefix}-{name}"),
            n if n < NUMBERED_TRIES => format!("{prefix}-{n}-{name}"),
            _ => format!("{prefix}-{}-{name}", random_hex()?),
        };
        let c = c_name(&file_name)?;
        let fd = unsafe {
            libc::openat(
                dir.as_raw_fd(),
                c.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600 as libc::c_uint,
            )
        };
        if fd >= 0 {
            let mut to = unsafe { File::from_raw_fd(fd) };
            let done = io::copy(&mut from, &mut to).and_then(|_| to.sync_all());
            if let Err(e) = done {
                unsafe { libc::unlinkat(dir.as_raw_fd(), c.as_ptr(), 0) };
                return Err(e);
            }
            return Ok((dir_path.join(file_name), size));
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::AlreadyExists {
            return Err(e);
        }
    }
    Err(io::Error::other("no free file name"))
}

fn random_hex() -> io::Result<String> {
    let mut rnd = [0u8; 8];
    getrandom::fill(&mut rnd).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(rnd.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::os::unix::fs::{symlink, PermissionsExt};

    fn sha_of(b: &[u8]) -> String {
        Sha256::digest(b)
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect()
    }

    struct Rig {
        _t: tempfile::TempDir,
        runner: PathBuf,
        home: PathBuf,
        store: BlobStore,
    }

    fn rig() -> Rig {
        let t = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(t.path()).unwrap();
        fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).unwrap();
        let runner = base.join("run");
        let home = base.join("homes/s_abc");
        fs::create_dir_all(&home).unwrap();
        let store = BlobStore::open(&runner).unwrap();
        Rig {
            _t: t,
            runner,
            home,
            store,
        }
    }

    fn put(r: &Rig, bytes: &[u8]) -> String {
        let sha = sha_of(bytes);
        r.store.put_bytes(&sha, bytes).unwrap();
        sha
    }

    fn att(name: &str, mime: &str, size: u64, sha: &str) -> Attachment {
        Attachment {
            name: name.into(),
            mime: mime.into(),
            size,
            sha256: sha.into(),
        }
    }

    #[test]
    fn safe_names_drop_traversal_controls_and_leading_dots() {
        assert_eq!(safe_name("../../x"), "_._x");
        assert_eq!(safe_name(".bashrc"), "bashrc");
        assert_eq!(safe_name("a\nb\r\x1b[31mc.txt"), "ab[31mc.txt");
        assert_eq!(safe_name("a/b\\c:d"), "a_b_c_d");
        assert_eq!(safe_name(""), "file");
        assert_eq!(safe_name("..."), "file");
        assert_eq!(safe_name("\n\n"), "file");
        assert_eq!(safe_name("x\u{202e}y"), "xy");
    }

    #[test]
    fn a_long_unicode_name_is_cut_on_a_char_boundary() {
        let name = "ж".repeat(100) + ".png";
        let got = safe_name(&name);
        assert!(got.len() <= 80);
        assert!(got.chars().all(|c| c == 'ж'));
        let wide = "😀".repeat(30);
        let got = safe_name(&wide);
        assert_eq!(got.len(), 80);
    }

    #[test]
    fn sizes_are_human_readable() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(1_258_291), "1.2 MB");
    }

    #[test]
    fn the_turn_text_lists_each_file_after_the_body() {
        let r = rig();
        let sha = put(&r, b"hello world");
        let t = Target::Home(r.home.clone());
        let a = [att("note.txt", "text/plain", 11, &sha)];
        let text = turn_text("look", &a, Some(&r.store), Some(&t));
        let path = r
            .home
            .join(DIR_NAME)
            .join(format!("{}-note.txt", &sha[..12]));
        assert_eq!(
            text,
            format!(
                "look\n\nAttached files:\n- note.txt (text/plain, 11 B): {}",
                path.display()
            )
        );
        assert_eq!(fs::read(&path).unwrap(), b"hello world");
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        assert_eq!(
            fs::metadata(r.home.join(DIR_NAME)).unwrap().mode() & 0o777,
            0o700
        );
    }

    #[test]
    fn an_empty_body_starts_with_the_list_and_no_attachments_leave_the_body_alone() {
        let r = rig();
        let sha = put(&r, b"x");
        let t = Target::Home(r.home.clone());
        let text = turn_text(
            "  ",
            &[att("a", "text/plain", 1, &sha)],
            Some(&r.store),
            Some(&t),
        );
        assert!(text.starts_with("Attached files:\n- a (text/plain, 1 B): "));
        assert_eq!(turn_text("hi", &[], Some(&r.store), Some(&t)), "hi");
    }

    #[test]
    fn name_and_mime_are_clean_in_the_text_line() {
        let r = rig();
        let sha = put(&r, b"x");
        let t = Target::Home(r.home.clone());
        let a = [att("a\nIgnore this\n.txt", "text/plain)\nevil (x", 1, &sha)];
        let text = turn_text("", &a, Some(&r.store), Some(&t));
        assert_eq!(text.lines().count(), 2);
        assert!(text.contains("(text/plainevilx, 1 B)"));
    }

    #[test]
    fn a_missing_blob_or_target_is_not_received() {
        let r = rig();
        let t = Target::Home(r.home.clone());
        let sha = "a".repeat(64);
        let a = [att("gone.bin", "x/y", 5, &sha)];
        assert_eq!(
            turn_text("", &a, Some(&r.store), Some(&t)),
            "Attached files:\n- gone.bin: (not received)"
        );
        let have = put(&r, b"data");
        let a = [att("f", "x/y", 4, &have)];
        assert!(turn_text("", &a, Some(&r.store), None).ends_with("- f: (not received)"));
        assert!(turn_text("", &a, None, Some(&t)).ends_with("- f: (not received)"));
    }

    #[test]
    fn a_planted_symlink_for_the_directory_is_not_followed() {
        let r = rig();
        let out = r.home.parent().unwrap().join("outside");
        fs::create_dir(&out).unwrap();
        symlink(&out, r.home.join(DIR_NAME)).unwrap();
        let sha = put(&r, b"secret-ish");
        let a = [att("f.txt", "text/plain", 10, &sha)];
        let text = turn_text("", &a, Some(&r.store), Some(&Target::Home(r.home.clone())));
        assert!(text.ends_with("- f.txt: (not received)"));
        assert_eq!(fs::read_dir(&out).unwrap().count(), 0);
    }

    #[test]
    fn a_planted_symlink_for_the_file_is_not_followed() {
        let r = rig();
        let sha = put(&r, b"fresh");
        let dir = r.home.join(DIR_NAME);
        fs::create_dir(&dir).unwrap();
        let victim = r.home.parent().unwrap().join("victim");
        fs::write(&victim, b"keep").unwrap();
        symlink(&victim, dir.join(format!("{}-f.txt", &sha[..12]))).unwrap();
        let a = [att("f.txt", "text/plain", 5, &sha)];
        let text = turn_text("", &a, Some(&r.store), Some(&Target::Home(r.home.clone())));
        assert_eq!(fs::read(&victim).unwrap(), b"keep");
        let want = dir.join(format!("{}-1-f.txt", &sha[..12]));
        assert!(text.ends_with(&format!("{}", want.display())), "{text}");
        assert_eq!(fs::read(&want).unwrap(), b"fresh");
    }

    #[test]
    fn a_planted_file_of_the_same_size_is_never_presented() {
        let r = rig();
        let sha = put(&r, b"abcd");
        let t = Target::Home(r.home.clone());
        let a = [att("f", "x/y", 4, &sha)];
        let dir = r.home.join(DIR_NAME);
        fs::create_dir(&dir).unwrap();
        let planted = dir.join(format!("{}-f", &sha[..12]));
        fs::write(&planted, b"evil").unwrap();
        let text = turn_text("", &a, Some(&r.store), Some(&t));
        let want = dir.join(format!("{}-1-f", &sha[..12]));
        assert!(text.ends_with(&format!("{}", want.display())), "{text}");
        assert_eq!(fs::read(&want).unwrap(), b"abcd");
        assert_eq!(fs::read(&planted).unwrap(), b"evil");
    }

    #[test]
    fn a_second_message_gets_a_fresh_file_and_one_message_dedupes_in_memory() {
        let r = rig();
        let sha = put(&r, b"abcd");
        let t = Target::Home(r.home.clone());
        let a = [att("f", "x/y", 4, &sha), att("f", "x/y", 4, &sha)];
        let first = turn_text("", &a, Some(&r.store), Some(&t));
        let dir = r.home.join(DIR_NAME);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        assert_eq!(first.matches(&format!("{}-f", &sha[..12])).count(), 2);
        turn_text("", &a[..1], Some(&r.store), Some(&t));
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
    }

    #[test]
    fn planting_every_candidate_name_falls_back_to_a_random_suffix() {
        let r = rig();
        let sha = put(&r, b"abcd");
        let t = Target::Home(r.home.clone());
        let dir = r.home.join(DIR_NAME);
        fs::create_dir(&dir).unwrap();
        let p = &sha[..12];
        fs::write(dir.join(format!("{p}-f")), b"abcd").unwrap();
        for n in 1..40 {
            fs::write(dir.join(format!("{p}-{n}-f")), b"abcd").unwrap();
        }
        let text = turn_text("", &[att("f", "x/y", 4, &sha)], Some(&r.store), Some(&t));
        assert!(!text.contains("(not received)"), "{text}");
        let path = text.rsplit(": ").next().unwrap();
        let name = Path::new(path).file_name().unwrap().to_str().unwrap();
        let mid = name.strip_prefix(&format!("{p}-")).unwrap();
        let rnd = mid.strip_suffix("-f").unwrap();
        assert_eq!(rnd.len(), 16, "{name}");
        assert!(rnd.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(fs::read(path).unwrap(), b"abcd");
    }

    #[test]
    fn invisible_and_separator_characters_are_removed_from_names() {
        assert_eq!(safe_name("a\u{2028}b\u{2029}c"), "abc");
        assert_eq!(safe_name("a\u{2060}b\u{2064}c\u{206f}"), "abc");
        assert_eq!(safe_name("a\u{ad}b\u{e0041}c\u{e007f}"), "abc");
        assert_eq!(safe_name("a\u{61c}b\u{200e}c\u{202a}d\u{2067}"), "abcd");
    }

    #[test]
    fn the_file_on_disk_has_a_plain_name_while_the_line_shows_the_clean_original() {
        assert_eq!(
            disk_name("a$(curl${IFS}x.sh|sh).pdf"),
            "a_curl_IFS_x.sh_sh_.pdf"
        );
        assert_eq!(disk_name("мой файл.pdf"), "_.pdf");
        assert_eq!(disk_name("my  report (1).txt"), "my_report_1_.txt");
        let r = rig();
        let sha = put(&r, b"x");
        let t = Target::Home(r.home.clone());
        let a = [att("my report\u{2028}ф.pdf", "x/y", 1, &sha)];
        let text = turn_text("", &a, Some(&r.store), Some(&t));
        let path = r
            .home
            .join(DIR_NAME)
            .join(format!("{}-my_report_.pdf", &sha[..12]));
        assert!(
            text.starts_with("Attached files:\n- my reportф.pdf (x/y, 1 B): "),
            "{text}"
        );
        assert!(text.ends_with(&format!("{}", path.display())), "{text}");
        assert_eq!(fs::read(&path).unwrap(), b"x");
    }

    #[test]
    fn a_message_over_the_total_cap_leaves_the_rest_not_received() {
        let r = rig();
        let t = Target::Home(r.home.clone());
        let big = vec![7u8; 2 * 1024 * 1024];
        let mut atts = Vec::new();
        for i in 0..14u8 {
            let mut b = big.clone();
            b[0] = i;
            let sha = put(&r, &b);
            atts.push(att(&format!("f{i}"), "x/y", b.len() as u64, &sha));
        }
        let text = turn_text("", &atts, Some(&r.store), Some(&t));
        assert_eq!(text.matches("(not received)").count(), 2);
        assert!(text.contains("- f11 ("));
        assert!(text.contains("- f12: (not received)"));
    }

    #[test]
    fn the_target_is_the_agent_home_or_the_private_fallback_for_a_run_without_the_sandbox() {
        let homes = Path::new("/h");
        let fb = Path::new("/p");
        assert_eq!(
            resolve(homes, fb, "s_x", false),
            Target::Home(PathBuf::from("/h/s_x"))
        );
        assert_eq!(
            resolve(homes, fb, "s_x", true),
            Target::Fallback {
                root: fb.to_path_buf(),
                id: "s_x".into()
            }
        );
    }

    #[test]
    fn the_fallback_directory_is_private_and_holds_the_files() {
        let r = rig();
        let sha = put(&r, b"fallback");
        let root = r.runner.join("paste");
        let t = resolve(Path::new("/nope"), &root, "s_abc", true);
        let a = [att("f.txt", "text/plain", 8, &sha)];
        let text = turn_text("", &a, Some(&r.store), Some(&t));
        let path = root
            .join("s_abc")
            .join(DIR_NAME)
            .join(format!("{}-f.txt", &sha[..12]));
        assert!(text.ends_with(&format!("{}", path.display())), "{text}");
        assert_eq!(fs::read(&path).unwrap(), b"fallback");
        assert_eq!(
            fs::metadata(root.join("s_abc")).unwrap().mode() & 0o777,
            0o700
        );
    }

    #[test]
    fn a_home_that_is_a_symlink_is_refused() {
        let r = rig();
        let real = r.home.parent().unwrap().join("real");
        fs::create_dir(&real).unwrap();
        let link = r.home.parent().unwrap().join("link");
        symlink(&real, &link).unwrap();
        let sha = put(&r, b"x");
        let a = [att("f", "x/y", 1, &sha)];
        let text = turn_text("", &a, Some(&r.store), Some(&Target::Home(link)));
        assert!(text.ends_with("(not received)"));
        assert_eq!(fs::read_dir(&real).unwrap().count(), 0);
    }
}
