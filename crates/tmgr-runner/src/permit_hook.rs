use crate::login::openat_nofollow;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::ffi::CString;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use toml_edit::{value, ArrayOfTables, DocumentMut, Item, Table};

pub const MAX_STDIN: usize = 2 * 1024 * 1024;
pub const KEEP: usize = 32;
const MARKER: &str = "__permission-hook";

pub fn spool_dir(home: &Path, session_id: &str) -> PathBuf {
    home.join(".cache").join("tmgr-permit").join(session_id)
}

pub fn valid_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn hook_main(agent: &str) -> i32 {
    let agent = match agent {
        "claude" => "claude",
        "codex" => "codex",
        _ => return 0,
    };
    let (Some(home), Some(session)) = (
        std::env::var_os("HOME"),
        std::env::var("TMGR_SESSION_ID").ok(),
    ) else {
        return 0;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let _ = spool(
        Path::new(&home),
        &session,
        agent,
        &mut std::io::stdin().lock(),
        now,
        std::process::id(),
    );
    0
}

pub fn spool(
    home: &Path,
    session: &str,
    agent: &str,
    input: &mut impl Read,
    at_ms: u64,
    pid: u32,
) -> std::io::Result<()> {
    if !valid_session_id(session) {
        return Ok(());
    }
    let mut raw = Vec::new();
    input.take(MAX_STDIN as u64 + 1).read_to_end(&mut raw)?;
    let (truncated, hook) = if raw.len() > MAX_STDIN {
        (true, recover_tool(&raw))
    } else {
        match serde_json::from_slice::<Value>(&raw) {
            Ok(v @ Value::Object(_)) => (false, v),
            _ => return Ok(()),
        }
    };
    let body =
        json!({"v": 1, "agent": agent, "at_ms": at_ms, "truncated": truncated, "hook": hook});
    let dir = open_spool_dir(home, session)?;
    let name = format!("{at_ms}-{pid}.json");
    let tmp = format!(".{name}.tmp");
    let c_tmp = cstr(&tmp)?;
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            c_tmp.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600 as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let written = unsafe { File::from_raw_fd(fd) }.write_all(body.to_string().as_bytes());
    let c_name = cstr(&name)?;
    let renamed = written.and_then(|()| {
        let r = unsafe {
            libc::renameat(
                dir.as_raw_fd(),
                c_tmp.as_ptr(),
                dir.as_raw_fd(),
                c_name.as_ptr(),
            )
        };
        (r == 0)
            .then_some(())
            .ok_or_else(std::io::Error::last_os_error)
    });
    if renamed.is_err() {
        unsafe { libc::unlinkat(dir.as_raw_fd(), c_tmp.as_ptr(), 0) };
        return renamed;
    }
    prune(&dir, &spool_dir(home, session));
    Ok(())
}

fn cstr(s: &str) -> std::io::Result<CString> {
    CString::new(s.as_bytes()).map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))
}

fn recover_tool(prefix: &[u8]) -> Value {
    let head = &prefix[..prefix.len().min(64 * 1024)];
    let text = String::from_utf8_lossy(head);
    let re = regex::Regex::new(r#""tool_name"\s*:\s*"([A-Za-z0-9_:.\-]{1,128})""#).unwrap();
    match re.captures(&text) {
        Some(c) => json!({"tool_name": &c[1]}),
        None => json!({}),
    }
}

fn open_spool_dir(home: &Path, session: &str) -> std::io::Result<File> {
    let mut cur = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(home)?;
    for name in [".cache", "tmgr-permit", session] {
        let c = cstr(name)?;
        let made = unsafe { libc::mkdirat(cur.as_raw_fd(), c.as_ptr(), 0o700) };
        if made < 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists {
            return Err(std::io::Error::last_os_error());
        }
        cur = openat_nofollow(
            cur.as_raw_fd(),
            std::ffi::OsStr::new(name),
            libc::O_RDONLY | libc::O_DIRECTORY,
        )?;
        if name != ".cache" {
            unsafe { libc::fchmod(cur.as_raw_fd(), 0o700) };
        }
    }
    Ok(cur)
}

fn spool_key(name: &str) -> Option<(u64, u64)> {
    let (at, rest) = name.split_once('-')?;
    let pid = rest.strip_suffix(".json")?;
    Some((at.parse().ok()?, pid.parse().ok()?))
}

fn prune(dir: &File, path: &Path) {
    let Ok(rd) = std::fs::read_dir(path) else {
        return;
    };
    let mut names: Vec<((u64, u64), String)> = rd
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().into_string().ok()?;
            Some((spool_key(&n)?, n))
        })
        .collect();
    names.sort();
    let excess = names.len().saturating_sub(KEEP);
    for (_, n) in names.into_iter().take(excess) {
        if let Ok(c) = cstr(&n) {
            unsafe { libc::unlinkat(dir.as_raw_fd(), c.as_ptr(), 0) };
        }
    }
}

pub fn hook_command(self_exe: &Path, agent: &str) -> String {
    format!(
        "{} {MARKER} {agent}",
        shell_words::quote(&self_exe.to_string_lossy())
    )
}

pub fn claude_settings(self_exe: &Path) -> String {
    json!({"hooks": {"PermissionRequest": [{"hooks": [
        {"type": "command", "command": hook_command(self_exe, "claude")}
    ]}]}})
    .to_string()
}

pub fn trust_hash(command: &str) -> String {
    let cmd = serde_json::to_string(command).unwrap_or_default();
    let canonical = format!(
        r#"{{"event_name":"permission_request","hooks":[{{"async":false,"command":{cmd},"timeout":600,"type":"command"}}]}}"#
    );
    let digest = Sha256::digest(canonical.as_bytes());
    let mut out = String::from("sha256:");
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

pub fn codex_config(
    existing: Option<&[u8]>,
    command: &str,
    config_path: &Path,
) -> Result<Vec<u8>, String> {
    let text = match existing {
        None => String::new(),
        Some(b) => String::from_utf8(b.to_vec())
            .map_err(|_| ".codex/config.toml in the run home is not UTF-8".to_string())?,
    };
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e| format!(".codex/config.toml in the run home is not valid TOML: {e}"))?;
    let bad = || ".codex/config.toml in the run home has an unexpected shape".to_string();
    let root = doc.as_table_mut();
    let features = table_entry(root, "features").ok_or_else(bad)?;
    features["hooks"] = value(true);
    let hooks = table_entry(root, "hooks").ok_or_else(bad)?;
    let mut groups = ArrayOfTables::new();
    let mut inner = ArrayOfTables::new();
    let mut handler = Table::new();
    handler["type"] = value("command");
    handler["command"] = value(command);
    inner.push(handler);
    let mut group = Table::new();
    group.insert("hooks", Item::ArrayOfTables(inner));
    groups.push(group);
    if let Some(old) = hooks.get("PermissionRequest") {
        let old = old.as_array_of_tables().ok_or_else(bad)?;
        for g in old.iter().filter(|g| !is_ours(g)) {
            groups.push(g.clone());
        }
    }
    hooks.insert("PermissionRequest", Item::ArrayOfTables(groups));
    let state = table_entry(hooks, "state").ok_or_else(bad)?;
    let key = format!("{}:permission_request:0:0", config_path.display());
    let mut entry = Table::new();
    entry["trusted_hash"] = value(trust_hash(command));
    state.insert(&key, Item::Table(entry));
    Ok(doc.to_string().into_bytes())
}

fn table_entry<'a>(parent: &'a mut Table, key: &str) -> Option<&'a mut Table> {
    if !parent.contains_key(key) {
        let mut t = Table::new();
        t.set_implicit(true);
        parent.insert(key, Item::Table(t));
    }
    parent.get_mut(key)?.as_table_mut()
}

fn is_ours(group: &Table) -> bool {
    group
        .get("hooks")
        .and_then(Item::as_array_of_tables)
        .is_some_and(|hs| {
            hs.iter().any(|h| {
                h.get("command")
                    .and_then(Item::as_str)
                    .is_some_and(|c| c.contains(MARKER))
            })
        })
}

pub fn supported_profile(id: &str) -> bool {
    matches!(id, "claude-code" | "codex")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};

    const SID: &str = "s_abcdefghij";

    fn home() -> (tempfile::TempDir, PathBuf) {
        let t = tempfile::Builder::new()
            .prefix("ph")
            .tempdir_in("/tmp")
            .unwrap();
        let h = t.path().canonicalize().unwrap().join("home");
        std::fs::create_dir(&h).unwrap();
        (t, h)
    }

    fn put(h: &Path, input: &[u8], at: u64) -> std::io::Result<()> {
        spool(h, SID, "claude", &mut &input[..], at, 7)
    }

    fn files(h: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(spool_dir(h, SID))
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().into_string().unwrap())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    #[test]
    fn writes_the_contract_with_private_modes() {
        let (_t, h) = home();
        put(
            &h,
            br#"{"tool_name":"Bash","tool_input":{"command":"ls"}}"#,
            1000,
        )
        .unwrap();
        assert_eq!(files(&h), vec!["1000-7.json"]);
        let dir = spool_dir(&h, SID);
        let f = dir.join("1000-7.json");
        assert_eq!(std::fs::metadata(&f).unwrap().mode() & 0o777, 0o600);
        for d in [&dir, dir.parent().unwrap()] {
            assert_eq!(std::fs::metadata(d).unwrap().mode() & 0o777, 0o700);
        }
        let v: Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
        assert_eq!(
            v,
            json!({"v":1,"agent":"claude","at_ms":1000,"truncated":false,
                "hook":{"tool_name":"Bash","tool_input":{"command":"ls"}}})
        );
    }

    #[test]
    fn bad_session_ids_write_nothing() {
        let (_t, h) = home();
        for id in ["", "../x", "a/b", "a b", "é", &"a".repeat(65), "a.b"] {
            spool(&h, id, "claude", &mut &b"{}"[..], 1, 1).unwrap();
        }
        assert!(!h.join(".cache").exists());
    }

    #[test]
    fn garbage_and_non_objects_write_nothing() {
        let (_t, h) = home();
        for input in [&b"not json"[..], b"[1]", b"", b"\xff\xfe"] {
            put(&h, input, 5).unwrap();
        }
        assert!(files(&h).is_empty());
    }

    #[test]
    fn symlinked_directories_are_refused() {
        let (t, h) = home();
        let elsewhere = t.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        std::fs::create_dir_all(h.join(".cache/tmgr-permit")).unwrap();
        symlink(&elsewhere, h.join(".cache/tmgr-permit").join(SID)).unwrap();
        assert!(put(&h, b"{}", 1).is_err());
        assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);

        let (t2, h2) = home();
        let elsewhere2 = t2.path().join("elsewhere");
        std::fs::create_dir(&elsewhere2).unwrap();
        symlink(&elsewhere2, h2.join(".cache")).unwrap();
        assert!(put(&h2, b"{}", 1).is_err());
        assert_eq!(std::fs::read_dir(&elsewhere2).unwrap().count(), 0);
    }

    #[test]
    fn a_symlinked_file_name_is_not_followed() {
        let (t, h) = home();
        let target = t.path().join("victim");
        std::fs::write(&target, "keep").unwrap();
        put(&h, b"{}", 1).unwrap();
        let dir = spool_dir(&h, SID);
        symlink(&target, dir.join(".2-7.json.tmp")).unwrap();
        assert!(put(&h, b"{}", 2).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep");
    }

    #[test]
    fn keeps_the_newest_32() {
        let (_t, h) = home();
        for at in 1..=40u64 {
            put(&h, b"{}", at).unwrap();
        }
        let names = files(&h);
        assert_eq!(names.len(), KEEP);
        assert!(names.contains(&"40-7.json".to_string()));
        assert!(names.contains(&"9-7.json".to_string()));
        assert!(!names.contains(&"8-7.json".to_string()));
    }

    #[test]
    fn over_the_cap_keeps_only_the_tool_name_when_it_is_recoverable() {
        let (_t, h) = home();
        let mut big = br#"{"tool_name":"Bash","tool_input":{"command":""#.to_vec();
        big.resize(MAX_STDIN + 100, b'a');
        put(&h, &big, 1).unwrap();
        let v: Value =
            serde_json::from_slice(&std::fs::read(spool_dir(&h, SID).join("1-7.json")).unwrap())
                .unwrap();
        assert_eq!(v["truncated"], true);
        assert_eq!(v["hook"], json!({"tool_name":"Bash"}));

        let blind = vec![b'a'; MAX_STDIN + 1];
        put(&h, &blind, 2).unwrap();
        let v: Value =
            serde_json::from_slice(&std::fs::read(spool_dir(&h, SID).join("2-7.json")).unwrap())
                .unwrap();
        assert_eq!(v["truncated"], true);
        assert_eq!(v["hook"], json!({}));
    }

    #[test]
    fn exactly_at_the_cap_is_not_truncated() {
        let (_t, h) = home();
        let mut doc = br#"{"tool_name":"Bash","x":""#.to_vec();
        let tail = br#""}"#;
        doc.resize(MAX_STDIN - tail.len(), b'a');
        doc.extend_from_slice(tail);
        assert_eq!(doc.len(), MAX_STDIN);
        put(&h, &doc, 1).unwrap();
        let v: Value =
            serde_json::from_slice(&std::fs::read(spool_dir(&h, SID).join("1-7.json")).unwrap())
                .unwrap();
        assert_eq!(v["truncated"], false);
    }

    #[test]
    fn trust_hash_matches_the_python_reference() {
        assert_eq!(
            trust_hash("cat > /x/hook-$$.json"),
            "sha256:1354e24f9eb18077e4cf966a639a167dd15e7650c6a278f076a7ae03ce6d96bf"
        );
    }

    #[test]
    fn claude_settings_is_one_additive_hook() {
        let v: Value =
            serde_json::from_str(&claude_settings(Path::new("/opt/my tmgr/tmgr"))).unwrap();
        assert_eq!(
            v,
            json!({"hooks":{"PermissionRequest":[{"hooks":[
                {"type":"command","command":"'/opt/my tmgr/tmgr' __permission-hook claude"}]}]}})
        );
    }

    #[test]
    fn codex_config_adds_the_hook_and_trust_and_keeps_the_rest() {
        let cfg = Path::new("/h/.codex/config.toml");
        let old = b"model = \"gpt-5\"\n\n[projects.\"/w\"]\ntrust_level = \"trusted\"\n";
        let out = codex_config(Some(old), "/bin/tmgr __permission-hook codex", cfg).unwrap();
        let v: toml::Value = toml::from_str(std::str::from_utf8(&out).unwrap()).unwrap();
        assert_eq!(v["model"].as_str(), Some("gpt-5"));
        assert_eq!(v["projects"]["/w"]["trust_level"].as_str(), Some("trusted"));
        assert_eq!(v["features"]["hooks"].as_bool(), Some(true));
        let h = &v["hooks"]["PermissionRequest"][0]["hooks"][0];
        assert_eq!(h["type"].as_str(), Some("command"));
        assert_eq!(
            h["command"].as_str(),
            Some("/bin/tmgr __permission-hook codex")
        );
        assert_eq!(
            v["hooks"]["state"]["/h/.codex/config.toml:permission_request:0:0"]["trusted_hash"]
                .as_str(),
            Some(trust_hash("/bin/tmgr __permission-hook codex").as_str())
        );
    }

    #[test]
    fn codex_config_is_idempotent_and_replaces_a_stale_command() {
        let cfg = Path::new("/h/.codex/config.toml");
        let once = codex_config(None, "/old/tmgr __permission-hook codex", cfg).unwrap();
        let twice = codex_config(Some(&once), "/new/tmgr __permission-hook codex", cfg).unwrap();
        let v: toml::Value = toml::from_str(std::str::from_utf8(&twice).unwrap()).unwrap();
        let groups = v["hooks"]["PermissionRequest"].as_array().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0]["hooks"][0]["command"].as_str(),
            Some("/new/tmgr __permission-hook codex")
        );
        let again = codex_config(Some(&twice), "/new/tmgr __permission-hook codex", cfg).unwrap();
        assert_eq!(again, twice);
    }

    #[test]
    fn codex_config_refuses_a_file_it_cannot_parse() {
        let cfg = Path::new("/h/.codex/config.toml");
        assert!(codex_config(Some(b"= broken"), "c", cfg).is_err());
        assert!(codex_config(Some(b"hooks = 1\n"), "c", cfg).is_err());
    }

    #[test]
    fn spool_files_are_not_world_readable_even_with_a_loose_umask() {
        let (_t, h) = home();
        put(&h, b"{}", 3).unwrap();
        let m = std::fs::metadata(spool_dir(&h, SID).join("3-7.json")).unwrap();
        assert_eq!(m.permissions().mode() & 0o077, 0);
    }
}
