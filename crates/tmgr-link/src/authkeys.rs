use crate::grammar::{self, PubKey};
use std::path::{Path, PathBuf};
use tmgr_core::config::write_atomic;
use tmgr_core::error::{Error, Result};

#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    pub options: Option<String>,
    pub keytype: String,
    pub blob: String,
    pub comment: String,
}

fn tokens(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for c in line.chars() {
        if escaped {
            cur.push(c);
            escaped = false;
        } else if quoted && c == '\\' {
            cur.push(c);
            escaped = true;
        } else if c == '"' {
            cur.push(c);
            quoted = !quoted;
        } else if c.is_whitespace() && !quoted {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn is_keytype(t: &str) -> bool {
    t.starts_with("ssh-") || t.starts_with("ecdsa-") || t.starts_with("sk-")
}

pub fn parse_entry(line: &str) -> Option<Entry> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let toks = tokens(trimmed);
    let (options, rest) = if is_keytype(toks.first()?) {
        (None, &toks[..])
    } else {
        (Some(toks[0].clone()), &toks[1..])
    };
    let keytype = rest.first()?.clone();
    let blob = rest.get(1)?.clone();
    if !is_keytype(&keytype) {
        return None;
    }
    Some(Entry {
        options,
        keytype,
        blob,
        comment: rest[2..].join(" "),
    })
}

pub fn comment_for(name: &str) -> String {
    format!("tmgr-link:{name}")
}

pub fn is_ours(entry: &Entry, name: &str) -> bool {
    entry.comment == comment_for(name)
        && entry.options.as_deref().is_some_and(|o| {
            o.starts_with("restrict,")
                && o.contains(&format!(" worker --link {name} --enrollment "))
        })
}

pub fn build_line(
    bin: &Path,
    name: &str,
    enrollment: &str,
    from: &[String],
    key: &PubKey,
) -> Result<String> {
    grammar::link_name(name)?;
    if !grammar::enrollment_id(enrollment) {
        return Err(Error::internal("invalid enrollment id"));
    }
    let bin_str = bin
        .to_str()
        .filter(|s| {
            bin.is_absolute()
                && !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"/._+@%:=,-".contains(&b))
        })
        .ok_or_else(|| {
            crate::forbidden(
                "the tmgr binary path has characters that are unsafe inside authorized_keys",
            )
            .hint("install tmgr in a plain absolute path such as /usr/local/bin")
        })?;
    let mut opts = format!(
        "restrict,no-pty,no-port-forwarding,no-agent-forwarding,no-X11-forwarding,no-user-rc,command=\"{bin_str} worker --link {name} --enrollment {enrollment}\""
    );
    if !from.is_empty() {
        opts.push_str(&format!(",from=\"{}\"", from.join(",")));
    }
    Ok(format!(
        "{opts} ssh-ed25519 {} {}",
        key.blob,
        comment_for(name)
    ))
}

pub fn build_jump_line(
    bin: &Path,
    name: &str,
    enrollment: &str,
    from: &[String],
    key: &PubKey,
    dests: &[String],
) -> Result<String> {
    let dests = grammar::parse_jump_dests(dests)?;
    let base = build_line(bin, name, enrollment, from, key)?;
    let permit: String = dests
        .iter()
        .map(|d| format!("permitopen=\"{d}\""))
        .collect::<Vec<_>>()
        .join(",");
    let line = base.replacen(
        ",no-port-forwarding,",
        &format!(",port-forwarding,{permit},"),
        1,
    );
    if line == base {
        return Err(Error::internal("cannot build the jump enrollment line"));
    }
    Ok(line)
}

#[cfg(unix)]
pub fn check_binary(bin: &Path) -> Result<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let canon = std::fs::canonicalize(bin)?;
    let euid = unsafe { libc::geteuid() };
    let unsafe_dir = |p: &Path| -> bool {
        match std::fs::metadata(p) {
            Ok(m) => {
                let mode = m.mode();
                let writable = mode & 0o022 != 0 && mode & 0o1000 == 0;
                writable || (m.uid() != euid && m.uid() != 0)
            }
            Err(_) => true,
        }
    };
    let meta = std::fs::metadata(&canon)?;
    if !meta.is_file() || meta.mode() & 0o022 != 0 || (meta.uid() != euid && meta.uid() != 0) {
        return Err(crate::forbidden(
            "the tmgr binary is writable by other users",
        ));
    }
    if let Some(dir) = canon.ancestors().skip(1).find(|d| unsafe_dir(d)) {
        return Err(crate::forbidden(format!(
            "{} is group/world-writable or owned by someone else, so the forced command could be replaced",
            dir.display()
        ))
        .hint("install tmgr in a directory only you (or root) can write to"));
    }
    Ok(canon)
}

#[cfg(not(unix))]
pub fn check_binary(bin: &Path) -> Result<PathBuf> {
    Ok(std::fs::canonicalize(bin)?)
}

#[derive(Debug, PartialEq, Eq, Default)]
pub struct Edit {
    pub changed: bool,
    pub removed: usize,
    pub backup: Option<PathBuf>,
    pub original: Vec<u8>,
    pub existed: bool,
    pub mode: Option<u32>,
    pub path: PathBuf,
}

pub fn restore(edit: &Edit) -> Result<()> {
    if !edit.changed {
        return Ok(());
    }
    if edit.existed {
        write_atomic(&edit.path, &edit.original, edit.mode.unwrap_or(0o600))
    } else {
        match std::fs::remove_file(&edit.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

fn read_existing(path: &Path) -> Result<(Vec<u8>, Option<u32>)> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let mode = mode_of(path);
            Ok((bytes, mode))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((Vec::new(), None)),
        Err(e) => Err(e.into()),
    }
}

#[cfg(unix)]
fn mode_of(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .ok()
        .map(|m| m.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn mode_of(_: &Path) -> Option<u32> {
    None
}

fn resolve(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn backup(path: &Path, bytes: &[u8]) -> Result<PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    for n in 0..100 {
        let suffix = if n == 0 {
            String::new()
        } else {
            format!("-{n}")
        };
        let name = format!(
            "{}.tmgr-{stamp}{suffix}.bak",
            path.file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("authorized_keys")
        );
        let target = path.with_file_name(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&target)
        {
            Ok(mut f) => {
                f.write_all(bytes)?;
                f.sync_all()?;
                return Ok(target);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(Error::internal("cannot create a backup of authorized_keys"))
}

fn lines_of(bytes: &[u8]) -> Vec<&[u8]> {
    bytes.split_inclusive(|b| *b == b'\n').collect()
}

fn commit(path: &Path, original: &[u8], mode: Option<u32>, updated: Vec<u8>) -> Result<Edit> {
    if updated == original {
        return Ok(Edit::default());
    }
    let backup = if original.is_empty() && mode.is_none() {
        None
    } else {
        Some(backup(path, original)?)
    };
    write_atomic(path, &updated, mode.unwrap_or(0o600))?;
    Ok(Edit {
        changed: true,
        removed: 0,
        backup,
        original: original.to_vec(),
        existed: mode.is_some(),
        mode,
        path: path.to_path_buf(),
    })
}

pub fn upsert(path: &Path, name: &str, line: &str, key: &PubKey) -> Result<Edit> {
    if let Some(dir) = path.parent() {
        ensure_ssh_dir(dir)?;
    }
    let path = resolve(path);
    let (original, mode) = read_existing(&path)?;
    let mut out: Vec<u8> = Vec::with_capacity(original.len() + line.len() + 1);
    for raw in lines_of(&original) {
        let text = String::from_utf8_lossy(raw);
        if let Some(entry) = parse_entry(&text) {
            if is_ours(&entry, name) {
                continue;
            }
            if entry.blob == key.blob && entry.keytype == "ssh-ed25519" {
                return Err(Error::conflict(
                    "this public key is already in authorized_keys without tmgr's restrictions",
                )
                .hint("remove that line first, or use a dedicated link key"));
            }
        }
        out.extend_from_slice(raw);
    }
    if !out.is_empty() && out.last() != Some(&b'\n') {
        out.push(b'\n');
    }
    out.extend_from_slice(line.as_bytes());
    out.push(b'\n');
    commit(&path, &original, mode, out)
}

pub fn remove(path: &Path, name: &str) -> Result<Edit> {
    let path = resolve(path);
    let (original, mode) = read_existing(&path)?;
    let mut out = Vec::with_capacity(original.len());
    let mut removed = 0;
    for raw in lines_of(&original) {
        let text = String::from_utf8_lossy(raw);
        if parse_entry(&text).is_some_and(|e| is_ours(&e, name)) {
            removed += 1;
            continue;
        }
        out.extend_from_slice(raw);
    }
    if removed == 0 {
        return Ok(Edit::default());
    }
    let mut edit = commit(&path, &original, mode, out)?;
    edit.removed = removed;
    Ok(edit)
}

pub fn find_foreign(paths: &[PathBuf], key: &PubKey, name: &str) -> Result<Option<PathBuf>> {
    for path in paths {
        let (bytes, _) = read_existing(&resolve(path))?;
        let hit = lines_of(&bytes).into_iter().any(|raw| {
            parse_entry(&String::from_utf8_lossy(raw))
                .is_some_and(|e| e.blob == key.blob && !is_ours(&e, name))
        });
        if hit {
            return Ok(Some(path.clone()));
        }
    }
    Ok(None)
}

#[cfg(unix)]
fn ensure_ssh_dir(dir: &Path) -> Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    if !dir.exists() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_ssh_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::testkeys;
    use std::os::unix::fs::PermissionsExt;

    fn setup() -> (tempfile::TempDir, PathBuf) {
        let d = crate::testutil::private_tempdir();
        let ak = d.path().join(".ssh").join("authorized_keys");
        (d, ak)
    }

    const ENR: &str = "0123456789abcdef0123456789abcdef";

    fn line_for(key: &PubKey, name: &str) -> String {
        build_line(Path::new("/usr/local/bin/tmgr"), name, ENR, &[], key).unwrap()
    }

    fn backups(ak: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(ak.parent().unwrap())
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.to_string_lossy().contains(".tmgr-"))
            .collect()
    }

    #[test]
    fn the_line_has_the_restricted_forced_command_shape() {
        let key = testkeys::key();
        let l = build_line(
            Path::new("/usr/local/bin/tmgr"),
            "laptop",
            ENR,
            &["192.0.2.0/24".into(), "198.51.100.7".into()],
            &key,
        )
        .unwrap();
        assert_eq!(
            l,
            format!(
                "restrict,no-pty,no-port-forwarding,no-agent-forwarding,no-X11-forwarding,no-user-rc,command=\"/usr/local/bin/tmgr worker --link laptop --enrollment {ENR}\",from=\"192.0.2.0/24,198.51.100.7\" ssh-ed25519 {} tmgr-link:laptop",
                key.blob
            )
        );
        let e = parse_entry(&l).unwrap();
        assert!(is_ours(&e, "laptop"));
        assert!(!is_ours(&e, "other"));
        assert_eq!(e.blob, key.blob);
    }

    #[test]
    fn unsafe_binary_paths_and_names_are_refused() {
        let key = testkeys::key();
        for bad in [
            "relative/tmgr",
            "/opt/my tmgr/tmgr",
            "/opt/a\"b/tmgr",
            "/opt/a;b/tmgr",
            "/opt/$HOME/tmgr",
            "/opt/a\nb/tmgr",
            "/opt/a'b/tmgr",
        ] {
            assert!(
                build_line(Path::new(bad), "x", ENR, &[], &key).is_err(),
                "{bad}"
            );
        }
        assert!(build_line(Path::new("/usr/bin/tmgr"), "x\" y", ENR, &[], &key).is_err());
        assert!(build_line(Path::new("/usr/bin/tmgr"), "x", "short", &[], &key).is_err());
    }

    #[test]
    fn parser_handles_quoted_options_and_plain_keys() {
        let e = parse_entry(r#"from="a b",command="x y" ssh-ed25519 AAAA c m"#).unwrap();
        assert_eq!(e.options.as_deref(), Some(r#"from="a b",command="x y""#));
        assert_eq!(e.comment, "c m");
        let p = parse_entry("ssh-rsa AAAA user@host").unwrap();
        assert_eq!((p.options, p.keytype.as_str()), (None, "ssh-rsa"));
        assert!(parse_entry("# comment").is_none());
        assert!(parse_entry("   ").is_none());
        assert!(parse_entry("garbage").is_none());
    }

    #[test]
    fn upsert_creates_a_private_file_without_a_backup() {
        let (_d, ak) = setup();
        let key = testkeys::key();
        let edit = upsert(&ak, "laptop", &line_for(&key, "laptop"), &key).unwrap();
        assert!(edit.changed);
        assert!(edit.backup.is_none());
        assert_eq!(
            std::fs::metadata(&ak).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(ak.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }

    #[test]
    fn other_lines_survive_byte_for_byte_and_a_backup_is_made() {
        let (_d, ak) = setup();
        std::fs::create_dir_all(ak.parent().unwrap()).unwrap();
        let mut original = b"# my keys\nssh-rsa AAAAB3 old@host\n\n\tcommand=\"x\"   ssh-ed25519 AAAA  weird \xff\xfe comment".to_vec();
        std::fs::write(&ak, &original).unwrap();
        std::fs::set_permissions(&ak, std::fs::Permissions::from_mode(0o640)).unwrap();
        let key = testkeys::key();
        let edit = upsert(&ak, "laptop", &line_for(&key, "laptop"), &key).unwrap();
        let backup = edit.backup.unwrap();
        assert_eq!(std::fs::read(&backup).unwrap(), original);
        assert_eq!(
            std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
        original.push(b'\n');
        let mut expected = original.clone();
        expected.extend_from_slice(line_for(&key, "laptop").as_bytes());
        expected.push(b'\n');
        assert_eq!(std::fs::read(&ak).unwrap(), expected);
        assert_eq!(
            std::fs::metadata(&ak).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    #[test]
    fn the_line_carries_explicit_no_forwarding_options_next_to_restrict() {
        let l = line_for(&testkeys::key(), "x");
        for o in [
            "restrict",
            "no-pty",
            "no-port-forwarding",
            "no-agent-forwarding",
            "no-X11-forwarding",
            "no-user-rc",
        ] {
            assert!(l.contains(o), "{o}");
        }
    }

    #[test]
    fn restore_undoes_an_edit() {
        let (_d, ak) = setup();
        let key = testkeys::key();
        let first = upsert(&ak, "a", &line_for(&key, "a"), &key).unwrap();
        restore(&first).unwrap();
        assert!(!ak.exists());
        std::fs::create_dir_all(ak.parent().unwrap()).unwrap();
        std::fs::write(&ak, "ssh-rsa AAAA keep\n").unwrap();
        let edit = upsert(&ak, "a", &line_for(&key, "a"), &key).unwrap();
        assert_ne!(std::fs::read_to_string(&ak).unwrap(), "ssh-rsa AAAA keep\n");
        restore(&edit).unwrap();
        assert_eq!(std::fs::read_to_string(&ak).unwrap(), "ssh-rsa AAAA keep\n");
    }

    #[test]
    fn upsert_is_idempotent() {
        let (_d, ak) = setup();
        let key = testkeys::key();
        let line = line_for(&key, "laptop");
        upsert(&ak, "laptop", &line, &key).unwrap();
        let before = std::fs::read(&ak).unwrap();
        let again = upsert(&ak, "laptop", &line, &key).unwrap();
        assert!(!again.changed);
        assert_eq!(std::fs::read(&ak).unwrap(), before);
        assert!(backups(&ak).is_empty());
    }

    #[test]
    fn upsert_replaces_only_its_own_line() {
        let (_d, ak) = setup();
        let k1 = testkeys::key();
        let k2 = testkeys::key();
        upsert(&ak, "a", &line_for(&k1, "a"), &k1).unwrap();
        upsert(&ak, "b", &line_for(&k2, "b"), &k2).unwrap();
        let wider = build_line(
            Path::new("/usr/local/bin/tmgr"),
            "a",
            ENR,
            &["192.0.2.1".into()],
            &k1,
        )
        .unwrap();
        upsert(&ak, "a", &wider, &k1).unwrap();
        let text = std::fs::read_to_string(&ak).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.contains("from=\"192.0.2.1\""));
        assert!(text.contains("tmgr-link:b"));
    }

    #[test]
    fn a_foreign_line_with_the_same_key_is_refused() {
        let (_d, ak) = setup();
        let key = testkeys::key();
        std::fs::create_dir_all(ak.parent().unwrap()).unwrap();
        let foreign = format!("ssh-ed25519 {} me@laptop\n", key.blob);
        std::fs::write(&ak, &foreign).unwrap();
        let e = upsert(&ak, "laptop", &line_for(&key, "laptop"), &key).unwrap_err();
        assert_eq!(e.exit_code(), 6);
        assert_eq!(std::fs::read_to_string(&ak).unwrap(), foreign);
        assert_eq!(
            find_foreign(std::slice::from_ref(&ak), &key, "laptop").unwrap(),
            Some(ak.clone())
        );
        let optioned = format!("command=\"/bin/sh\" ssh-ed25519 {} x\n", key.blob);
        std::fs::write(&ak, optioned).unwrap();
        assert!(upsert(&ak, "laptop", &line_for(&key, "laptop"), &key).is_err());
    }

    #[test]
    fn remove_deletes_only_the_named_link_and_is_idempotent() {
        let (_d, ak) = setup();
        let k1 = testkeys::key();
        let k2 = testkeys::key();
        upsert(&ak, "a", &line_for(&k1, "a"), &k1).unwrap();
        upsert(&ak, "b", &line_for(&k2, "b"), &k2).unwrap();
        let edit = remove(&ak, "a").unwrap();
        assert_eq!(edit.removed, 1);
        assert!(edit.backup.is_some());
        let text = std::fs::read_to_string(&ak).unwrap();
        assert!(!text.contains(&k1.blob) && text.contains(&k2.blob));
        assert_eq!(remove(&ak, "a").unwrap(), Edit::default());
        assert_eq!(
            remove(&ak.with_file_name("missing"), "a").unwrap(),
            Edit::default()
        );
    }

    #[test]
    fn a_user_line_with_a_tmgr_comment_is_not_ours() {
        let (_d, ak) = setup();
        let key = testkeys::key();
        std::fs::create_dir_all(ak.parent().unwrap()).unwrap();
        let sneaky = format!("ssh-ed25519 {} tmgr-link:a\n", key.blob);
        std::fs::write(&ak, &sneaky).unwrap();
        assert_eq!(remove(&ak, "a").unwrap().removed, 0);
        assert_eq!(std::fs::read_to_string(&ak).unwrap(), sneaky);
    }

    #[test]
    fn symlinked_authorized_keys_are_written_through() {
        let (d, ak) = setup();
        std::fs::create_dir_all(ak.parent().unwrap()).unwrap();
        let real = d.path().join("real_keys");
        std::fs::write(&real, "ssh-rsa AAAA x\n").unwrap();
        std::os::unix::fs::symlink(&real, &ak).unwrap();
        let key = testkeys::key();
        upsert(&ak, "a", &line_for(&key, "a"), &key).unwrap();
        assert!(std::fs::symlink_metadata(&ak)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(std::fs::read_to_string(&real)
            .unwrap()
            .contains("tmgr-link:a"));
    }

    #[test]
    fn binaries_in_writable_directories_are_refused() {
        let d = crate::testutil::private_tempdir();
        let bin_dir = d.path().join("bin");
        std::fs::create_dir(&bin_dir).unwrap();
        let bin = bin_dir.join("tmgr");
        std::fs::write(&bin, "x").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&bin_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            check_binary(&bin).unwrap(),
            std::fs::canonicalize(&bin).unwrap()
        );
        std::fs::set_permissions(&bin_dir, std::fs::Permissions::from_mode(0o775)).unwrap();
        assert_eq!(check_binary(&bin).unwrap_err().exit_code(), 5);
        std::fs::set_permissions(&bin_dir, std::fs::Permissions::from_mode(0o757)).unwrap();
        assert!(check_binary(&bin).is_err());
        std::fs::set_permissions(&bin_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o775)).unwrap();
        assert!(check_binary(&bin).is_err());
    }
}
