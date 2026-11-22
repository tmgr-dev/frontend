use super::files::write_private_atomic;
use super::machines::{Machines, MACHINES_FILE};
use super::policy::read_private;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;
use tmgr_ipc::chat::RelaySelf;

const MAX_LABEL_CHARS: usize = 64;
const MAX_MACHINES_BYTES: u64 = 64 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PeerLine {
    id: String,
    device_id: String,
    fp: String,
}

pub fn show_line(me: &RelaySelf) -> String {
    json!({"id": me.id, "device_id": me.device_id, "fp": me.fp}).to_string()
}

fn load_rows(runner_dir: &Path) -> Result<Vec<Value>, String> {
    match std::fs::symlink_metadata(runner_dir.join(MACHINES_FILE)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{MACHINES_FILE}: {e}")),
        Ok(m) if !m.file_type().is_file() => {
            return Err(format!("{MACHINES_FILE} is not a regular file"))
        }
        Ok(_) => {}
    }
    let text = read_private(runner_dir, MACHINES_FILE, MAX_MACHINES_BYTES)
        .map_err(|e| format!("{MACHINES_FILE}: {e}"))?;
    Machines::parse(&text)?;
    let doc: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    match doc.get("machines") {
        Some(Value::Array(rows)) => Ok(rows.clone()),
        _ => Err(format!("{MACHINES_FILE} is not valid")),
    }
}

fn save_rows(runner_dir: &Path, rows: Vec<Value>) -> Result<(), String> {
    let text = json!({ "machines": rows }).to_string();
    Machines::parse(&text)?;
    write_private_atomic(runner_dir, MACHINES_FILE, text.as_bytes())
        .map_err(|e| format!("{MACHINES_FILE} was not written: {e}"))
}

pub(super) fn lock(runner_dir: &Path) -> Result<std::fs::File, String> {
    use std::os::fd::AsRawFd;
    let f = std::fs::File::open(runner_dir)
        .map_err(|e| format!("the runner dir could not be opened: {e}"))?;
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err("the runner dir could not be locked".into());
    }
    Ok(f)
}

fn parse_peer(line: &str, label: &str) -> Result<PeerLine, String> {
    let peer: PeerLine =
        serde_json::from_str(line).map_err(|_| "the line must be the JSON printed by show")?;
    let n = label.chars().count();
    if n == 0 || n > MAX_LABEL_CHARS || label.chars().any(char::is_control) {
        return Err(format!(
            "the label must be 1 to {MAX_LABEL_CHARS} characters without control characters"
        ));
    }
    Ok(peer)
}

pub fn add_machine(runner_dir: &Path, line: &str, label: &str) -> Result<(), String> {
    let peer = parse_peer(line, label)?;
    let _held = lock(runner_dir)?;
    let mut rows = load_rows(runner_dir)?;
    let taken = |key: &str, val: &str| rows.iter().any(|r| r[key].as_str() == Some(val));
    if taken("id", &peer.id) {
        return Err(format!("machine {} is already paired", peer.id));
    }
    if taken("device_id", &peer.device_id) {
        return Err("that relay device is already paired".into());
    }
    rows.push(json!({
        "id": peer.id,
        "label": label,
        "device_id": peer.device_id,
        "fp": peer.fp,
    }));
    save_rows(runner_dir, rows)
}

pub fn ensure_machine(runner_dir: &Path, line: &str, label: &str) -> Result<(), String> {
    let peer = parse_peer(line, label)?;
    let _held = lock(runner_dir)?;
    let mut rows = load_rows(runner_dir)?;
    let new_row = json!({
        "id": peer.id,
        "label": label,
        "device_id": peer.device_id,
        "fp": peer.fp,
    });
    if let Some(i) = rows.iter().position(|r| r["id"].as_str() == Some(&peer.id)) {
        let same = |k: &str| rows[i][k] == new_row[k];
        if same("device_id") && same("fp") {
            return Ok(());
        }
        if rows
            .iter()
            .enumerate()
            .any(|(j, r)| j != i && r["device_id"].as_str() == Some(&peer.device_id))
        {
            return Err("that relay device is already paired".into());
        }
        rows[i] = new_row;
    } else {
        if rows
            .iter()
            .any(|r| r["device_id"].as_str() == Some(&peer.device_id))
        {
            return Err("that relay device is already paired".into());
        }
        rows.push(new_row);
    }
    save_rows(runner_dir, rows)
}

pub fn remove_machine(runner_dir: &Path, id: &str) -> Result<(), String> {
    let _held = lock(runner_dir)?;
    let mut rows = load_rows(runner_dir)?;
    let before = rows.len();
    rows.retain(|r| r["id"].as_str() != Some(id));
    if rows.len() == before {
        return Err(format!(
            "no paired machine {}",
            id.chars().take(24).collect::<String>()
        ));
    }
    save_rows(runner_dir, rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    const A: &str = "mx_0123456789abcdef";
    const B: &str = "mx_fedcba9876543210";
    const DEV_A: &str = "11111111-2222-3333-4444-555555555555";
    const DEV_B: &str = "66666666-7777-8888-9999-aaaaaaaaaaaa";
    const FP_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const FP_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn dir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("mp")
            .tempdir_in("/tmp")
            .unwrap()
    }

    fn line(id: &str, dev: &str, fp: &str) -> String {
        json!({"id": id, "device_id": dev, "fp": fp}).to_string()
    }

    fn file_text(d: &Path) -> String {
        std::fs::read_to_string(d.join(MACHINES_FILE)).unwrap()
    }

    fn mode(d: &Path) -> u32 {
        std::fs::metadata(d.join(MACHINES_FILE))
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    }

    fn rows(d: &Path) -> Vec<(String, String)> {
        let v: Value = serde_json::from_str(&file_text(d)).unwrap();
        v["machines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["id"].as_str().unwrap().to_string(),
                    r["label"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn the_show_line_is_one_json_line_with_the_three_fields() {
        let l = show_line(&RelaySelf {
            id: A.into(),
            device_id: DEV_A.into(),
            fp: FP_A.into(),
        });
        assert!(!l.contains('\n'));
        let v: Value = serde_json::from_str(&l).unwrap();
        assert_eq!(v, json!({"id": A, "device_id": DEV_A, "fp": FP_A}));
    }

    #[test]
    fn adding_creates_a_private_file_that_parses() {
        let d = dir();
        add_machine(d.path(), &line(A, DEV_A, FP_A), "laptop").unwrap();
        assert_eq!(mode(d.path()), 0o600);
        let m = Machines::parse(&file_text(d.path())).unwrap();
        assert_eq!(m.machine_for(DEV_A, FP_A), Some(A));
        assert_eq!(rows(d.path()), [(A.into(), "laptop".into())]);
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
    }

    #[test]
    fn adding_keeps_existing_rows_and_labels() {
        let d = dir();
        add_machine(d.path(), &line(A, DEV_A, FP_A), "first").unwrap();
        add_machine(d.path(), &line(B, DEV_B, FP_B), "second").unwrap();
        assert_eq!(
            rows(d.path()),
            [(A.into(), "first".into()), (B.into(), "second".into())]
        );
        let m = Machines::parse(&file_text(d.path())).unwrap();
        assert_eq!(m.device_for(B), Some((DEV_B, FP_B)));
        assert_eq!(mode(d.path()), 0o600);
    }

    #[test]
    fn bad_lines_and_labels_are_refused_and_write_nothing() {
        let d = dir();
        let long = "x".repeat(65);
        let cases: Vec<(String, &str)> = vec![
            (line("mx_ABC", DEV_A, FP_A), "box"),
            (line("laptop", DEV_A, FP_A), "box"),
            (line(A, "not-a-uuid", FP_A), "box"),
            (line(A, &DEV_B.to_uppercase(), FP_A), "box"),
            (line(A, DEV_A, "abc"), "box"),
            (line(A, DEV_A, &FP_A.to_uppercase()), "box"),
            (line(A, DEV_A, FP_A), ""),
            (line(A, DEV_A, FP_A), &long),
            (line(A, DEV_A, FP_A), "bad\nlabel"),
            (
                json!({"id": A, "device_id": DEV_A, "fp": FP_A, "x": 1}).to_string(),
                "box",
            ),
            (json!({"id": A, "device_id": DEV_A}).to_string(), "box"),
            ("not json".into(), "box"),
            ("[]".into(), "box"),
        ];
        for (l, label) in cases {
            assert!(add_machine(d.path(), &l, label).is_err(), "{l} {label:?}");
        }
        assert!(!d.path().join(MACHINES_FILE).exists());
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
    }

    #[test]
    fn ensuring_a_new_machine_adds_it_and_the_same_one_again_changes_nothing() {
        let d = dir();
        ensure_machine(d.path(), &line(A, DEV_A, FP_A), "first").unwrap();
        let before = file_text(d.path());
        ensure_machine(d.path(), &line(A, DEV_A, FP_A), "renamed").unwrap();
        assert_eq!(file_text(d.path()), before);
        assert_eq!(rows(d.path()), [(A.into(), "first".into())]);
        assert_eq!(mode(d.path()), 0o600);
        let m = Machines::parse(&file_text(d.path())).unwrap();
        assert_eq!(m.machine_for(DEV_A, FP_A), Some(A));
    }

    #[test]
    fn ensuring_a_repaired_machine_replaces_its_row_and_keeps_the_others() {
        let d = dir();
        ensure_machine(d.path(), &line(A, DEV_A, FP_A), "first").unwrap();
        ensure_machine(d.path(), &line(B, DEV_B, FP_B), "second").unwrap();
        let dev_c = "cccccccc-7777-8888-9999-aaaaaaaaaaaa";
        ensure_machine(d.path(), &line(A, dev_c, FP_B), "first again").unwrap();
        assert_eq!(
            rows(d.path()),
            [
                (A.into(), "first again".into()),
                (B.into(), "second".into())
            ]
        );
        let m = Machines::parse(&file_text(d.path())).unwrap();
        assert_eq!(m.machine_for(dev_c, FP_B), Some(A));
        assert_eq!(m.machine_for(DEV_A, FP_A), None);
        assert_eq!(m.machine_for(DEV_B, FP_B), Some(B));
    }

    #[test]
    fn ensuring_refuses_a_device_that_belongs_to_another_machine_and_the_manual_add_still_refuses_a_repeat(
    ) {
        let d = dir();
        ensure_machine(d.path(), &line(A, DEV_A, FP_A), "first").unwrap();
        ensure_machine(d.path(), &line(B, DEV_B, FP_B), "second").unwrap();
        let before = file_text(d.path());
        assert!(ensure_machine(d.path(), &line("mx_00000000000000cc", DEV_A, FP_A), "x").is_err());
        assert!(ensure_machine(d.path(), &line(B, DEV_A, FP_B), "x").is_err());
        assert!(ensure_machine(d.path(), &line(A, DEV_A, FP_A), "").is_err());
        assert_eq!(file_text(d.path()), before);
        assert!(add_machine(d.path(), &line(A, DEV_A, FP_A), "again").is_err());
    }

    #[test]
    fn ensuring_never_overwrites_an_invalid_file() {
        let d = dir();
        std::fs::write(d.path().join(MACHINES_FILE), "garbage").unwrap();
        std::fs::set_permissions(
            d.path().join(MACHINES_FILE),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        assert!(ensure_machine(d.path(), &line(A, DEV_A, FP_A), "x").is_err());
        assert_eq!(file_text(d.path()), "garbage");
    }

    #[test]
    fn a_label_of_exactly_the_maximum_is_accepted() {
        let d = dir();
        add_machine(d.path(), &line(A, DEV_A, FP_A), &"x".repeat(64)).unwrap();
    }

    #[test]
    fn a_duplicate_id_or_device_is_refused_and_the_file_is_unchanged() {
        let d = dir();
        add_machine(d.path(), &line(A, DEV_A, FP_A), "first").unwrap();
        let before = file_text(d.path());
        assert!(add_machine(d.path(), &line(A, DEV_B, FP_B), "dup id").is_err());
        assert!(add_machine(d.path(), &line(B, DEV_A, FP_B), "dup device").is_err());
        assert_eq!(file_text(d.path()), before);
    }

    #[test]
    fn a_symlinked_or_non_file_machines_json_is_refused() {
        let d = dir();
        let real = d.path().join("real.json");
        std::fs::write(&real, r#"{"machines":[]}"#).unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&real, d.path().join(MACHINES_FILE)).unwrap();
        assert!(add_machine(d.path(), &line(A, DEV_A, FP_A), "x").is_err());
        assert!(remove_machine(d.path(), A).is_err());
        assert_eq!(
            std::fs::read_to_string(&real).unwrap(),
            r#"{"machines":[]}"#
        );
        assert!(std::fs::symlink_metadata(d.path().join(MACHINES_FILE))
            .unwrap()
            .file_type()
            .is_symlink());

        let d = dir();
        std::fs::create_dir(d.path().join(MACHINES_FILE)).unwrap();
        assert!(add_machine(d.path(), &line(A, DEV_A, FP_A), "x").is_err());
    }

    #[test]
    fn an_invalid_or_loose_existing_file_is_not_overwritten() {
        let d = dir();
        std::fs::write(d.path().join(MACHINES_FILE), "garbage").unwrap();
        std::fs::set_permissions(
            d.path().join(MACHINES_FILE),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        assert!(add_machine(d.path(), &line(A, DEV_A, FP_A), "x").is_err());
        assert_eq!(file_text(d.path()), "garbage");

        let d = dir();
        add_machine(d.path(), &line(A, DEV_A, FP_A), "first").unwrap();
        std::fs::set_permissions(
            d.path().join(MACHINES_FILE),
            std::fs::Permissions::from_mode(0o666),
        )
        .unwrap();
        assert!(add_machine(d.path(), &line(B, DEV_B, FP_B), "x").is_err());
        assert_eq!(rows(d.path()).len(), 1);
    }

    #[test]
    fn removing_drops_only_that_row() {
        let d = dir();
        add_machine(d.path(), &line(A, DEV_A, FP_A), "first").unwrap();
        add_machine(d.path(), &line(B, DEV_B, FP_B), "second").unwrap();
        remove_machine(d.path(), A).unwrap();
        assert_eq!(rows(d.path()), [(B.into(), "second".into())]);
        assert_eq!(mode(d.path()), 0o600);
        Machines::parse(&file_text(d.path())).unwrap();
    }

    #[test]
    fn removing_an_unknown_or_malformed_id_is_an_error() {
        let d = dir();
        assert!(remove_machine(d.path(), A).is_err());
        add_machine(d.path(), &line(A, DEV_A, FP_A), "first").unwrap();
        let before = file_text(d.path());
        assert!(remove_machine(d.path(), B).is_err());
        assert!(remove_machine(d.path(), "nope").is_err());
        assert_eq!(file_text(d.path()), before);
    }
    #[test]
    fn concurrent_adds_and_a_remove_never_lose_an_update() {
        let d = dir();
        add_machine(d.path(), &line(A, DEV_A, FP_A), "a").unwrap();
        let p = d.path().to_path_buf();
        let ids: Vec<String> = (0..8).map(|i| format!("mx_{:016x}", 0x100 + i)).collect();
        let handles: Vec<_> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| {
                let (p, id) = (p.clone(), id.clone());
                std::thread::spawn(move || {
                    let dev = format!("{:08x}-2222-3333-4444-555555555555", i + 0x10);
                    add_machine(&p, &line(&id, &dev, FP_B), "x").unwrap();
                })
            })
            .chain(std::iter::once({
                let p = p.clone();
                std::thread::spawn(move || remove_machine(&p, A).unwrap())
            }))
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let got: Vec<String> = rows(d.path()).into_iter().map(|(id, _)| id).collect();
        assert_eq!(got.len(), 8);
        assert!(!got.iter().any(|id| id == A));
        for id in &ids {
            assert!(got.contains(id));
        }
    }
}
