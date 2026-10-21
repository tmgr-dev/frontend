use crate::text::{sanitize_line, truncate_chars};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};

const MAX_COMMITS: usize = 50;
const MAX_SUMMARY_CHARS: usize = 500;

#[derive(Debug, Clone, Deserialize)]
pub struct ApiRunTask {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiCommit {
    #[serde(default)]
    pub sha: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiTests {
    #[serde(default)]
    pub passed: Option<i64>,
    #[serde(default)]
    pub failed: Option<i64>,
    #[serde(default)]
    pub command: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiRun {
    pub id: u64,
    #[serde(default)]
    pub task_id: Option<u64>,
    #[serde(default)]
    pub user_id: Option<i64>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub ended_at: Option<String>,
    #[serde(default)]
    pub duration_seconds: Option<i64>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub pr_url: Option<String>,
    #[serde(default)]
    pub commits: Option<Vec<ApiCommit>>,
    #[serde(default)]
    pub tests: Option<ApiTests>,
    #[serde(default)]
    pub version: Option<i64>,
    #[serde(default)]
    pub task: Option<ApiRunTask>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunTaskView {
    pub id: Option<u64>,
    #[serde(rename = "ref")]
    pub reference: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommitView {
    pub sha: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestsView {
    pub passed: Option<i64>,
    pub failed: Option<i64>,
    pub command: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunView {
    pub id: u64,
    pub task: Option<RunTaskView>,
    pub user_id: Option<i64>,
    pub agent: String,
    pub model: Option<String>,
    pub branch: Option<String>,
    pub session_id: Option<String>,
    pub status: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub duration_s: i64,
    pub summary: Option<String>,
    pub pr_url: Option<String>,
    pub commits: Vec<CommitView>,
    pub tests: Option<TestsView>,
    pub version: Option<i64>,
}

fn clean(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(|v| sanitize_line(v).trim().to_string())
        .filter(|v| !v.is_empty())
}

pub fn task_ref_of(task: &ApiRunTask) -> String {
    match task.key.as_deref() {
        Some(k) if crate::refs::parse_key(k).is_some() => k.to_string(),
        _ => task.id.map(|i| format!("#{i}")).unwrap_or_default(),
    }
}

impl RunView {
    pub fn from_api(r: &ApiRun) -> Self {
        Self {
            id: r.id,
            task: r.task.as_ref().map(|t| RunTaskView {
                id: t.id.or(r.task_id),
                reference: task_ref_of(t),
                title: crate::view::display_title(t.title.as_deref().unwrap_or("")),
            }),
            user_id: r.user_id,
            agent: clean(&r.agent).unwrap_or_else(|| "unknown".to_string()),
            model: clean(&r.model),
            branch: clean(&r.branch),
            session_id: clean(&r.session_id),
            status: clean(&r.status).unwrap_or_else(|| "unknown".to_string()),
            started_at: clean(&r.started_at),
            ended_at: clean(&r.ended_at),
            duration_s: r.duration_seconds.unwrap_or(0).max(0),
            summary: clean(&r.summary).map(|s| truncate_chars(&s, MAX_SUMMARY_CHARS).0),
            pr_url: clean(&r.pr_url),
            commits: r
                .commits
                .iter()
                .flatten()
                .take(MAX_COMMITS)
                .map(|c| CommitView {
                    sha: clean(&c.sha).unwrap_or_default(),
                    message: clean(&c.message).unwrap_or_default(),
                })
                .collect(),
            tests: r.tests.as_ref().map(|t| TestsView {
                passed: t.passed,
                failed: t.failed,
                command: clean(&t.command),
            }),
            version: r.version,
        }
    }

    pub fn fingerprint(&self) -> String {
        match self.version {
            Some(v) => format!("v{v}:{}", self.status),
            None => {
                let mut v = self.clone();
                v.duration_s = 0;
                serde_json::to_string(&v).unwrap_or_default()
            }
        }
    }
}

pub fn parse_ts(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

pub fn clipped_seconds(
    run: &ApiRun,
    day_start: DateTime<Utc>,
    day_end: DateTime<Utc>,
    now: DateTime<Utc>,
) -> i64 {
    let Some(start) = run.started_at.as_deref().and_then(parse_ts) else {
        return 0;
    };
    let duration = run.duration_seconds.unwrap_or(0).max(0);
    let end = match run.ended_at.as_deref().and_then(parse_ts) {
        Some(e) => e,
        None if run.status.as_deref() == Some("running") => now,
        None => start + Duration::seconds(duration),
    };
    let from = start.max(day_start);
    let to = end.min(day_end);
    (to - from).num_seconds().clamp(0, duration)
}

pub fn day_bounds<Tz: TimeZone>(tz: &Tz, date: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
    let at = |d: NaiveDate| {
        let mut naive = d.and_hms_opt(0, 0, 0).expect("midnight exists");
        for _ in 0..4 {
            if let Some(t) = tz.from_local_datetime(&naive).earliest() {
                return t.with_timezone(&Utc);
            }
            naive += Duration::hours(1);
        }
        Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).expect("midnight exists"))
    };
    (at(date), at(date.succ_opt().unwrap_or(date)))
}

pub fn parse_since(input: &str) -> Option<Duration> {
    let (num, unit) = input.split_at(input.len().checked_sub(1)?);
    let n: i64 = num.parse().ok()?;
    if !(1..=100_000).contains(&n) {
        return None;
    }
    match unit {
        "s" => Some(Duration::seconds(n)),
        "m" => Some(Duration::minutes(n)),
        "h" => Some(Duration::hours(n)),
        "d" => Some(Duration::days(n)),
        _ => None,
    }
}

pub fn status_glyph(status: &str) -> &'static str {
    match status {
        "running" => "\u{25cf}",
        "succeeded" => "\u{2713}",
        "failed" => "\u{2717}",
        "cancelled" => "\u{25cb}",
        _ => "\u{25cc}",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn run(start: &str, end: Option<&str>, status: &str, duration: i64) -> ApiRun {
        serde_json::from_value(serde_json::json!({
            "id": 1, "started_at": start, "ended_at": end, "status": status, "duration_seconds": duration
        }))
        .unwrap()
    }

    fn at(s: &str) -> DateTime<Utc> {
        parse_ts(s).unwrap()
    }

    #[test]
    fn day_bounds_follow_the_local_offset() {
        let tz = FixedOffset::east_opt(2 * 3600).unwrap();
        let (s, e) = day_bounds(&tz, NaiveDate::from_ymd_opt(2026, 10, 12).unwrap());
        assert_eq!(s, at("2026-10-11T22:00:00Z"));
        assert_eq!(e, at("2026-10-12T22:00:00Z"));
    }

    #[test]
    fn a_run_crossing_midnight_counts_only_its_part_of_the_day() {
        let tz = FixedOffset::east_opt(2 * 3600).unwrap();
        let (s, e) = day_bounds(&tz, NaiveDate::from_ymd_opt(2026, 10, 12).unwrap());
        let r = run(
            "2026-10-11T21:00:00Z",
            Some("2026-10-11T23:30:00Z"),
            "succeeded",
            9000,
        );
        assert_eq!(clipped_seconds(&r, s, e, at("2026-10-13T00:00:00Z")), 5400);
        let r = run(
            "2026-10-12T21:30:00Z",
            Some("2026-10-12T23:00:00Z"),
            "succeeded",
            5400,
        );
        assert_eq!(clipped_seconds(&r, s, e, at("2026-10-13T00:00:00Z")), 1800);
    }

    #[test]
    fn running_runs_end_now_and_are_capped_by_their_duration() {
        let (s, e) = (at("2026-10-12T00:00:00Z"), at("2026-10-13T00:00:00Z"));
        let r = run("2026-10-12T10:00:00Z", None, "running", 600);
        assert_eq!(clipped_seconds(&r, s, e, at("2026-10-12T11:00:00Z")), 600);
        let r = run("2026-10-12T10:00:00Z", None, "running", 7200);
        assert_eq!(clipped_seconds(&r, s, e, at("2026-10-12T11:00:00Z")), 3600);
    }

    #[test]
    fn runs_outside_the_day_count_nothing() {
        let (s, e) = (at("2026-10-12T00:00:00Z"), at("2026-10-13T00:00:00Z"));
        let r = run(
            "2026-10-11T10:00:00Z",
            Some("2026-10-11T11:00:00Z"),
            "succeeded",
            3600,
        );
        assert_eq!(clipped_seconds(&r, s, e, at("2026-10-13T00:00:00Z")), 0);
    }

    #[test]
    fn since_durations() {
        assert_eq!(parse_since("2h"), Some(Duration::hours(2)));
        assert_eq!(parse_since("30m"), Some(Duration::minutes(30)));
        assert_eq!(parse_since("1d"), Some(Duration::days(1)));
        for bad in ["", "h", "2", "-2h", "2x", "0h", "2 h", "999999999h"] {
            assert!(parse_since(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn views_strip_control_characters_from_server_text() {
        let r: ApiRun = serde_json::from_value(serde_json::json!({
            "id": 7, "agent": "cl\u{1b}[31maude", "model": "op\u{7}us", "branch": "b\u{1b}]0;x\u{7}",
            "status": "running", "summary": "did\u{1b}[2J it",
            "pr_url": "https://x.example/\u{1b}[0m1",
            "commits": [{"sha": "abc\u{1b}", "message": "feat\u{1b}[1m: x"}],
            "task": {"id": 5, "key": "TM-1", "title": "TM-1: Fix\u{1b}[31m it"}
        }))
        .unwrap();
        let text = serde_json::to_string(&RunView::from_api(&r)).unwrap();
        assert!(
            !text.contains("\\u001b") && !text.contains("\\u0007"),
            "{text}"
        );
        let v = RunView::from_api(&r);
        assert_eq!(v.task.unwrap().reference, "TM-1");
    }

    #[test]
    fn the_session_id_is_exposed_and_optional() {
        let r: ApiRun = serde_json::from_value(serde_json::json!({
            "id": 7, "session_id": "ab\u{7}c-1"
        }))
        .unwrap();
        assert_eq!(RunView::from_api(&r).session_id.as_deref(), Some("ab c-1"));
        let r: ApiRun = serde_json::from_value(serde_json::json!({"id": 8})).unwrap();
        assert_eq!(RunView::from_api(&r).session_id, None);
    }

    #[test]
    fn fingerprint_ignores_the_moving_duration() {
        let mut a = run("2026-10-12T10:00:00Z", None, "running", 10);
        let mut b = a.clone();
        b.duration_seconds = Some(20);
        assert_eq!(
            RunView::from_api(&a).fingerprint(),
            RunView::from_api(&b).fingerprint()
        );
        a.version = Some(1);
        b.version = Some(2);
        assert_ne!(
            RunView::from_api(&a).fingerprint(),
            RunView::from_api(&b).fingerprint()
        );
    }
}
