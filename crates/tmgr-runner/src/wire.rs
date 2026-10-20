use crate::screen::changed_lines;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tmgr_ipc::api::{Line, ScreenPush, Snapshot};

pub const SCROLLBACK: usize = 5000;
pub use tmgr_ipc::api::limits::{MAX_COLS, MAX_INPUT, MAX_PUSH, MAX_ROWS};

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

pub struct Slot(Arc<AtomicUsize>);

impl Slot {
    pub fn take(counter: &Arc<AtomicUsize>, cap: usize) -> Option<Slot> {
        if counter.fetch_add(1, Ordering::SeqCst) >= cap {
            counter.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Slot(counter.clone()))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

pub fn clamp_size(cols: u16, rows: u16) -> (u16, u16) {
    (cols.clamp(1, MAX_COLS), rows.clamp(1, MAX_ROWS))
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut rnd = [0u8; 8];
    getrandom::fill(&mut rnd).map_err(std::io::Error::other)?;
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let tmp = path.with_file_name(format!(
        ".{name}.{}.tmp",
        rnd.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW)
        .mode(0o600)
        .open(&tmp)?;
    let res = f.write_all(bytes).and_then(|()| {
        drop(f);
        std::fs::rename(&tmp, path)
    });
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

pub const SHIM_CWD_FD: i32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchFile {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
}

fn json_len<T: Serialize>(v: &T) -> usize {
    serde_json::to_vec(v).map_or(0, |b| b.len())
}

fn batches(rows: Vec<(usize, Line)>, budget: usize) -> Vec<Vec<(usize, Line)>> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    let mut size = 0;
    for row in rows {
        let n = json_len(&row) + 1;
        if !cur.is_empty() && size + n > budget {
            out.push(std::mem::take(&mut cur));
            size = 0;
        }
        size += n;
        cur.push(row);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn screen_pushes(id: &str, prev: Option<&Snapshot>, snap: &Snapshot) -> Vec<ScreenPush> {
    const ENVELOPE: usize = 1024;
    let diff = prev.filter(|p| p.rows == snap.rows && p.cols == snap.cols);
    let base = ScreenPush {
        session: id.to_string(),
        full: false,
        snapshot: None,
        lines: Vec::new(),
        cursor: snap.cursor,
        modes: Some(snap.modes),
        scroll_offset: snap.scroll_offset,
        scroll_max: snap.scroll_max,
    };
    let rows: Vec<(usize, Line)> = match diff {
        Some(p) => changed_lines(p, snap)
            .into_iter()
            .map(|i| (i, snap.lines[i].clone()))
            .collect(),
        None => snap.lines.iter().cloned().enumerate().collect(),
    };
    let mut head = Snapshot {
        lines: Vec::new(),
        ..snap.clone()
    };
    let overhead = 2 * json_len(&base) + json_len(&head) + ENVELOPE;
    let mut chunks = batches(rows, MAX_PUSH.saturating_sub(overhead)).into_iter();
    let first = chunks.next().unwrap_or_default();
    let mut out = Vec::new();
    if diff.is_some() {
        out.push(ScreenPush {
            lines: first,
            ..base.clone()
        });
    } else {
        head.lines = first.into_iter().map(|(_, l)| l).collect();
        out.push(ScreenPush {
            full: true,
            snapshot: Some(head),
            ..base.clone()
        });
    }
    out.extend(chunks.map(|lines| ScreenPush {
        lines,
        ..base.clone()
    }));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::ScreenCache;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use tmgr_ipc::api::{Attr, Color, Modes, Span};
    use tmgr_ipc::proto::ServerMsg;

    const SID: &str = "s_abcdefghij";

    fn busy_screen(rows: u16, cols: u16, shift: u8) -> Snapshot {
        let line = |r: u8| Line {
            spans: (0..cols as usize)
                .map(|c| Span {
                    a: Attr {
                        fg: Color::Rgb(r, c as u8, shift),
                        bg: Color::Idx((c % 2) as u8),
                        flags: 0,
                    },
                    t: char::from(b'a' + (c % 26) as u8).to_string(),
                })
                .collect(),
        };
        Snapshot {
            rows,
            cols,
            lines: (0..rows).map(|r| line(r as u8)).collect(),
            cursor: Some((3, 4)),
            modes: Modes {
                app_cursor: false,
                bracketed_paste: true,
                alt_screen: false,
                hide_cursor: false,
                mouse: Default::default(),
            },
            scroll_offset: 2,
            scroll_max: 9,
        }
    }

    fn apply_all(cache: &mut ScreenCache, pushes: &[ScreenPush]) {
        for p in pushes {
            let msg = ServerMsg::Screen(serde_json::to_value(p).unwrap());
            assert!(serde_json::to_vec(&msg).unwrap().len() <= MAX_PUSH);
            assert_eq!(cache.apply(&msg).as_deref(), Some(SID));
        }
    }

    #[test]
    fn a_maximal_busy_screen_travels_in_bounded_pushes_and_arrives_intact() {
        let snap = busy_screen(200, 500, 0);
        let whole = serde_json::to_vec(&snap).unwrap().len();
        assert!(whole > 4 * MAX_PUSH, "{whole}");
        let pushes = screen_pushes(SID, None, &snap);
        assert!(pushes.len() > 4);
        assert!(pushes[0].full && pushes[1..].iter().all(|p| !p.full));
        let mut cache = ScreenCache::new();
        apply_all(&mut cache, &pushes);
        assert_eq!(cache.snapshot(SID), Some(&snap));

        let next = busy_screen(200, 500, 7);
        let diff = screen_pushes(SID, Some(&snap), &next);
        assert!(diff.len() > 4 && diff.iter().all(|p| !p.full));
        apply_all(&mut cache, &diff);
        assert_eq!(cache.snapshot(SID), Some(&next));
    }

    #[test]
    fn small_changes_stay_one_push() {
        let a = busy_screen(24, 80, 0);
        let mut b = a.clone();
        b.lines[5] = a.lines[6].clone();
        b.cursor = None;
        let pushes = screen_pushes(SID, Some(&a), &b);
        assert_eq!(pushes.len(), 1);
        assert_eq!(pushes[0].lines.len(), 1);
        let mut cache = ScreenCache::new();
        apply_all(&mut cache, &screen_pushes(SID, None, &a));
        apply_all(&mut cache, &pushes);
        assert_eq!(cache.snapshot(SID), Some(&b));
        assert_eq!(screen_pushes(SID, None, &a).len(), 1);
    }

    #[test]
    fn write_atomic_ignores_planted_symlink_and_is_private() {
        let t = tempfile::tempdir().unwrap();
        let victim = t.path().join("victim");
        std::fs::write(&victim, "keep").unwrap();
        let target = t.path().join("runner.pid");
        symlink(&victim, target.with_extension("tmp")).unwrap();
        write_atomic(&target, b"42").unwrap();
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "42");
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let leftovers: Vec<_> = std::fs::read_dir(t.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(".runner.pid"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }
}
