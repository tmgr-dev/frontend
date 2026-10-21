use super::attach_questions::{session_src, QuestionJob};
use chrono::{DateTime, Duration, FixedOffset, TimeZone, Utc};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use tmgr_runner::attach::{Cost, Format, Reading, UsageBar};
use tmgr_runner::questions::transcript_path;
use tmgr_usage::aggregate::{Filters, Query};
use tmgr_usage::limits::{LimitSnapshot, LimitWindow};
use tmgr_usage::paths::{self, Env};
use tmgr_usage::statusline::read_snapshots;
use tmgr_usage::{Store, Tool, Tz};

const LIMIT_STALE: Duration = Duration::minutes(30);
const COST_LAG_BYTES: u64 = 4096;

pub(super) struct Source {
    pub db: PathBuf,
    pub snapshots: PathBuf,
}

pub(super) fn usage_source() -> Option<Source> {
    let dir = paths::data_dir(&Env::from_process()).ok()?;
    Some(Source {
        db: paths::db_path(&dir),
        snapshots: paths::snapshots_dir(&dir).join("claude"),
    })
}

pub(super) fn usage_worker(
    source: Option<Source>,
    jobs: Receiver<Vec<QuestionJob>>,
    mut send: impl FnMut(UsageBar) -> bool,
) {
    let mut store: Option<Store> = None;
    for batch in jobs {
        if store.is_none() {
            store = source
                .as_ref()
                .filter(|s| s.db.exists())
                .and_then(|s| Store::open_read_only(&s.db).ok());
        }
        let bar = build(store.as_ref(), source.as_ref(), &batch, Utc::now());
        if !send(bar) {
            return;
        }
    }
}

fn build(
    store: Option<&Store>,
    source: Option<&Source>,
    batch: &[QuestionJob],
    now: DateTime<Utc>,
) -> UsageBar {
    let mut limits = store
        .and_then(|s| s.current_limits(now).ok())
        .unwrap_or_default();
    if let Some(src) = source {
        for snap in read_snapshots(&src.snapshots).unwrap_or_default() {
            limits.extend(snap.to_limits_at("claude", now));
        }
    }
    let mut bar = UsageBar {
        claude_5h: reading(&limits, Tool::Claude, &LimitWindow::Session, now),
        claude_week: reading(&limits, Tool::Claude, &LimitWindow::Weekly, now),
        codex_week: reading(&limits, Tool::Codex, &LimitWindow::Weekly, now),
        ..UsageBar::default()
    };
    if let Some(store) = store {
        for job in batch.iter().filter(|j| j.src.format == Format::Claude) {
            if let Some(cost) = session_cost(store, job, now) {
                bar.costs.insert(job.id.clone(), cost);
            }
        }
    }
    bar
}

fn reading(
    limits: &[LimitSnapshot],
    tool: Tool,
    window: &LimitWindow,
    now: DateTime<Utc>,
) -> Option<Reading> {
    let best = limits
        .iter()
        .filter(|l| l.tool == tool && &l.window == window)
        .max_by_key(|l| l.observed_at)?;
    Some(Reading {
        pct: best.used_pct,
        stale: now - best.observed_at > LIMIT_STALE,
    })
}

fn session_cost(store: &Store, job: &QuestionJob, now: DateTime<Utc>) -> Option<Cost> {
    let src = session_src(job)?;
    let path = transcript_path(&src)?;
    let name = path.file_name()?.to_str()?.to_string();
    let tool_id = path.file_stem()?.to_str()?.to_string();
    let query = Query {
        from: Utc.timestamp_opt(0, 0).single()?,
        to: now + Duration::days(1),
        tz: Tz::Fixed(FixedOffset::east_opt(0)?),
        group_by: Vec::new(),
        filters: Filters {
            tools: vec![Tool::Claude],
            sessions: vec![tool_id],
            ..Filters::default()
        },
    };
    let report = store.aggregate(&query).ok()?;
    if report.totals.events == 0 {
        return None;
    }
    let size = std::fs::metadata(&path).ok()?.len();
    let behind = store
        .scanned_bytes(&name)
        .ok()
        .flatten()
        .is_none_or(|read| size > read + COST_LAG_BYTES);
    Some(Cost {
        usd: report.totals.cost_usd,
        stale: behind || report.totals.unpriced_events > 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::mpsc;
    use tmgr_runner::attach::Format;
    use tmgr_usage::limits::LimitSource;
    use tmgr_usage::{scan_account, Account, PriceBook};

    const U: &str = "2f0e1c3a-5b7d-4e8f-9a1b-3c4d5e6f7a8b";

    fn job(home: &Path, dir: &Path) -> QuestionJob {
        QuestionJob {
            id: "s_abcdefghjk".into(),
            src: super::super::attach_title::AgentSrc {
                profile: "claude-code".into(),
                format: Format::Claude,
                cwd: "/work/x".into(),
                pid: 0,
                default_name: "claude".into(),
            },
            pid: 0,
            sandbox_home: home.to_path_buf(),
            log: dir.join("output.log"),
            exited: true,
            settled: false,
        }
    }

    fn limit(tool: Tool, window: LimitWindow, pct: f64, age: Duration) -> LimitSnapshot {
        LimitSnapshot {
            tool,
            account: "a".into(),
            window,
            used_pct: pct,
            resets_at: Some(Utc::now() + Duration::hours(3)),
            plan: None,
            observed_at: Utc::now() - age,
            source: LimitSource::Statusline,
            tool_session_id: "t".into(),
        }
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        db: PathBuf,
        home: PathBuf,
        runner: PathBuf,
        transcript: PathBuf,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("usage.db");
        let home = dir.path().join("home");
        let runner = dir.path().join("runner");
        let proj = home.join(".claude/projects/-work-x");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::create_dir_all(&runner).unwrap();
        let transcript = proj.join(format!("{U}.jsonl"));
        let ts = (Utc::now() - Duration::minutes(1)).to_rfc3339();
        std::fs::write(
            &transcript,
            format!(
                r#"{{"type":"assistant","sessionId":"{U}","timestamp":"{ts}","cwd":"/work/x","requestId":"r1","message":{{"id":"m1","model":"claude-opus-5","usage":{{"input_tokens":1000000,"output_tokens":0}}}}}}"#
            ) + "\n",
        )
        .unwrap();
        std::fs::write(runner.join("output.log"), format!("claude --resume {U}\n")).unwrap();
        let mut store = Store::open(&db).unwrap();
        let acc = Account {
            tool: Tool::Claude,
            label: "claude".into(),
            root: home.join(".claude"),
        };
        scan_account(&mut store, &acc, &PriceBook::bundled(), Utc::now()).unwrap();
        store
            .record_limit(&limit(
                Tool::Claude,
                LimitWindow::Session,
                34.0,
                Duration::minutes(1),
            ))
            .unwrap();
        store
            .record_limit(&limit(
                Tool::Claude,
                LimitWindow::Weekly,
                81.0,
                Duration::minutes(2),
            ))
            .unwrap();
        store
            .record_limit(&limit(
                Tool::Codex,
                LimitWindow::Weekly,
                8.0,
                Duration::hours(5),
            ))
            .unwrap();
        Fixture {
            _dir: dir,
            db,
            home,
            runner,
            transcript,
        }
    }

    fn source(db: &Path) -> Source {
        Source {
            db: db.to_path_buf(),
            snapshots: db.with_file_name("snapshots").join("claude"),
        }
    }

    fn run(db: Option<&Path>, jobs: Vec<QuestionJob>) -> UsageBar {
        let source = db.map(source);
        let (tx, rx) = mpsc::sync_channel(1);
        let (out_tx, out_rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            usage_worker(source, rx, move |bar| out_tx.send(bar).is_ok())
        });
        tx.send(jobs).unwrap();
        let bar = out_rx.recv().unwrap();
        drop(tx);
        handle.join().unwrap();
        bar
    }

    #[test]
    fn limits_and_session_cost_come_from_the_local_store() {
        let f = fixture();
        let bar = run(Some(&f.db), vec![job(&f.home, &f.runner)]);
        assert_eq!(bar.claude_5h.map(|r| (r.pct, r.stale)), Some((34.0, false)));
        assert_eq!(
            bar.claude_week.map(|r| (r.pct, r.stale)),
            Some((81.0, false))
        );
        assert_eq!(bar.codex_week.map(|r| (r.pct, r.stale)), Some((8.0, true)));
        let cost = bar.costs["s_abcdefghjk"];
        assert!(cost.usd > 0.0, "{cost:?}");
        assert!(!cost.stale);
    }

    #[test]
    fn a_transcript_ahead_of_the_store_marks_the_cost_old() {
        let f = fixture();
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&f.transcript)
            .unwrap();
        std::io::Write::write_all(&mut file, &vec![b' '; 8192]).unwrap();
        let bar = run(Some(&f.db), vec![job(&f.home, &f.runner)]);
        assert!(bar.costs["s_abcdefghjk"].stale);
    }

    #[test]
    fn a_missing_store_gives_an_empty_bar_and_is_not_created() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("usage.db");
        let bar = run(Some(&db), Vec::new());
        assert_eq!(bar, UsageBar::default());
        assert!(!db.exists());
        assert_eq!(run(None, Vec::new()), UsageBar::default());
    }

    #[test]
    fn statusline_snapshots_feed_the_limits_without_a_scan() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("usage.db");
        let reset = (Utc::now() + Duration::hours(2)).timestamp();
        let body = format!(
            r#"{{"session_id":"abc","rate_limits":{{"five_hour":{{"used_percentage":55,"resets_at":{reset}}},"seven_day":{{"used_percentage":12,"resets_at":{reset}}}}}}}"#
        );
        tmgr_usage::statusline::write_snapshot(
            &source(&db).snapshots,
            Some("default"),
            body.as_bytes(),
        )
        .unwrap();
        let bar = run(Some(&db), Vec::new());
        assert_eq!(bar.claude_5h.map(|r| (r.pct, r.stale)), Some((55.0, false)));
        assert_eq!(bar.claude_week.map(|r| r.pct), Some(12.0));
        assert!(bar.codex_week.is_none());
        assert!(!db.exists());
    }

    #[test]
    fn the_store_is_opened_read_only() {
        let f = fixture();
        let store = Store::open_read_only(&f.db).unwrap();
        assert!(store
            .record_limit(&limit(
                Tool::Codex,
                LimitWindow::Weekly,
                1.0,
                Duration::zero()
            ))
            .is_err());
    }

    #[test]
    fn codex_sessions_get_no_cost() {
        let f = fixture();
        let mut j = job(&f.home, &f.runner);
        j.src.format = Format::Codex;
        assert!(run(Some(&f.db), vec![j]).costs.is_empty());
    }
}
