use super::catalog::Live;
use super::session_list::{self, Row};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;
use tmgr_runner::attach::{FolderData, FolderMark, RecentDir};
use tmgr_usage::paths::Env;
use toml_edit::{Array, DocumentMut};

const FILE: &str = "folder-pins.toml";
const MAX_PINS: usize = 50;
const ROW_LIMIT: usize = 500;
const RUNNER_WAIT: Duration = Duration::from_secs(2);

pub(super) fn pins_path() -> Option<PathBuf> {
    tmgr_core::config::config_dir().ok().map(|d| d.join(FILE))
}

fn clean_pins<'a>(paths: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in paths {
        if Path::new(p).is_absolute() && !out.iter().any(|o| o == p) {
            out.push(p.to_string());
        }
        if out.len() >= MAX_PINS {
            break;
        }
    }
    out
}

pub(super) fn load_pins(path: &Path) -> Vec<String> {
    let Some(doc) = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.parse::<DocumentMut>().ok())
    else {
        return Vec::new();
    };
    let Some(arr) = doc.get("pins").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    clean_pins(arr.iter().filter_map(|v| v.as_str()))
}

pub(super) fn save_pins(path: &Path, pins: &[String]) -> std::io::Result<()> {
    use std::io::Write;
    let mut arr = Array::new();
    for p in clean_pins(pins.iter().map(String::as_str)) {
        arr.push(p);
    }
    let mut doc = DocumentMut::new();
    doc["pins"] = toml_edit::value(arr);
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let _ = tmgr_core::config::ensure_private_dir(dir);
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(doc.to_string().as_bytes())?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

pub(super) fn build(
    rows: &[Row],
    live_cwds: &[String],
    pins: Vec<String>,
    home: Option<&Path>,
    now_ms: u64,
) -> FolderData {
    let mut by_path: HashMap<String, RecentDir> = HashMap::new();
    let mut add = |path: &str, when: u64, live: bool| {
        let e = by_path
            .entry(path.to_string())
            .or_insert_with(|| RecentDir {
                path: path.to_string(),
                ..RecentDir::default()
            });
        e.last_ms = e.last_ms.max(when);
        e.count += 1;
        e.live |= live;
    };
    for r in rows {
        if let Some(c) = r.cwd() {
            add(c, r.when_ms, r.live == Live::Yes);
        }
    }
    for c in live_cwds {
        add(c, now_ms, true);
    }
    let mut recents: Vec<RecentDir> = by_path
        .into_values()
        .filter(|r| {
            let p = Path::new(&r.path);
            p.is_absolute()
                && p != Path::new("/")
                && home != Some(p)
                && std::fs::metadata(p).is_ok_and(|m| m.is_dir())
        })
        .collect();
    recents.sort_by(|a, b| b.last_ms.cmp(&a.last_ms).then_with(|| a.path.cmp(&b.path)));
    FolderData { recents, pins }
}

fn rows_within() -> Vec<Row> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(session_list::runner_rows(None, &mut Vec::new()));
    });
    let runner = rx.recv_timeout(RUNNER_WAIT).unwrap_or_default();
    session_list::recent(&Env::from_process(), None, ROW_LIMIT, runner).0
}

pub(super) fn load(live_cwds: &[String]) -> FolderData {
    let pins = pins_path().map(|p| load_pins(&p)).unwrap_or_default();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    build(
        &rows_within(),
        live_cwds,
        pins,
        home.as_deref(),
        super::catalog::now_ms(),
    )
}

pub(super) fn marks_worker(
    requests: Receiver<Vec<String>>,
    mut send: impl FnMut(String, Option<FolderMark>) -> bool,
) {
    let mut next: Option<Vec<String>> = None;
    loop {
        let mut batch = match next.take() {
            Some(b) => b,
            None => match requests.recv() {
                Ok(b) => b,
                Err(_) => return,
            },
        };
        batch.reverse();
        while let Some(path) = batch.pop() {
            if let Ok(newer) = requests.try_recv() {
                next = Some(newer);
                break;
            }
            let mark = super::attach_git::collect_mark(Path::new(&path));
            if !send(path, mark) {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::catalog::{Agent, Entry};
    use super::*;

    fn row(cwd: &str, when: u64, live: Live) -> Row {
        Row::external(
            Entry {
                agent: Agent::Claude,
                id: format!("{cwd}{when}"),
                path: PathBuf::new(),
                cwd: Some(cwd.to_string()),
                mtime_ms: when,
                started_at: None,
                title: None,
                first_message: None,
                git_branch: None,
                project: None,
                readable: true,
                live,
            },
            None,
        )
    }

    fn dir(p: &Path, name: &str) -> String {
        let d = p.join(name);
        std::fs::create_dir_all(&d).unwrap();
        d.to_string_lossy().into_owned()
    }

    #[test]
    fn rows_group_by_folder_with_count_latest_time_and_live() {
        let tmp = tempfile::tempdir().unwrap();
        let a = dir(tmp.path(), "a");
        let b = dir(tmp.path(), "b");
        let gone = tmp.path().join("gone").to_string_lossy().into_owned();
        let rows = [
            row(&a, 10, Live::No),
            row(&a, 30, Live::Yes),
            row(&b, 20, Live::Recent),
            row(&gone, 99, Live::No),
        ];
        let data = build(&rows, &[], Vec::new(), None, 1000);
        assert_eq!(
            data.recents,
            vec![
                RecentDir {
                    path: a,
                    last_ms: 30,
                    count: 2,
                    live: true
                },
                RecentDir {
                    path: b,
                    last_ms: 20,
                    count: 1,
                    live: false
                },
            ]
        );
    }

    #[test]
    fn live_cwds_and_pins_shape_the_data_and_missing_or_excluded_folders_drop_out() {
        let tmp = tempfile::tempdir().unwrap();
        let a = dir(tmp.path(), "a");
        let home = dir(tmp.path(), "home");
        let data = build(
            &[],
            &[
                a.clone(),
                a.clone(),
                home.clone(),
                "/".into(),
                "/nope/x".into(),
                "rel".into(),
            ],
            vec![a.clone()],
            Some(Path::new(&home)),
            42,
        );
        assert_eq!(
            data.recents,
            vec![RecentDir {
                path: a.clone(),
                last_ms: 42,
                count: 2,
                live: true
            }]
        );
        assert_eq!(data.pins, vec![a]);
    }

    #[test]
    fn pins_roundtrip_overwrite_atomically_and_reject_relative_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("sub").join(FILE);
        save_pins(
            &file,
            &["/a".into(), "rel".into(), "/b".into(), "/a".into()],
        )
        .unwrap();
        assert_eq!(load_pins(&file), vec!["/a".to_string(), "/b".to_string()]);
        save_pins(&file, &["/c".into()]).unwrap();
        assert_eq!(load_pins(&file), vec!["/c".to_string()]);
        let names: Vec<_> = std::fs::read_dir(file.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1);
        std::fs::write(&file, "pins = [\"relative\", \"/ok\", 5]\n").unwrap();
        assert_eq!(load_pins(&file), vec!["/ok".to_string()]);
        std::fs::write(&file, "not toml [[[").unwrap();
        assert!(load_pins(&file).is_empty());
    }

    #[test]
    fn pins_are_capped() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join(FILE);
        let many: Vec<String> = (0..80).map(|i| format!("/p{i}")).collect();
        save_pins(&file, &many).unwrap();
        assert_eq!(load_pins(&file).len(), MAX_PINS);
    }
}
