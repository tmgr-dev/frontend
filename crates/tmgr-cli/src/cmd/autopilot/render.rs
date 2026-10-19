use chrono::{DateTime, FixedOffset, Utc};
use serde_json::Value;
use tmgr_core::text::sanitize_line;

use super::model::{ActiveRun, Event, LimitRow, RunState, StatusDoc};

const LAST_EVENTS: usize = 8;

pub fn clock(t: DateTime<Utc>, tz: FixedOffset) -> String {
    t.with_timezone(&tz).format("%H:%M").to_string()
}

pub fn span(secs: i64) -> String {
    let secs = secs.max(0);
    let (h, m) = (secs / 3600, secs % 3600 / 60);
    match (h, m) {
        (0, 0) => format!("{secs}s"),
        (0, m) => format!("{m}m"),
        (h, m) => format!("{h}h {m}m"),
    }
}

fn money(v: f64) -> String {
    if (v - v.round()).abs() < 0.005 {
        format!("${}", v.round())
    } else {
        format!("${v:.2}")
    }
}

fn tool_name(tool: &str) -> String {
    let mut c = tool.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

fn window_name(w: &str) -> String {
    match w {
        "five_hour" => "5h".into(),
        "seven_day" => "week".into(),
        other => other.into(),
    }
}

pub(super) fn limit_part(l: &LimitRow, now: DateTime<Utc>, tz: FixedOffset) -> String {
    let name = format!("{} {}", tool_name(&l.tool), window_name(&l.window));
    let age = l
        .observed_at
        .map(|t| format!(" {} ago", span((now - t).num_seconds())))
        .unwrap_or_default();
    let Some(pct) = l.used_pct else {
        return format!("{name}: unknown{age}");
    };
    let mut s = format!("{name}: {}% {}{age}", pct.round(), l.source);
    if let (Some(soft), Some(hard)) = (l.stop_new_at, l.pause_at) {
        s.push_str(&format!(" (stop new at {soft}%, pause at {hard}%)"));
    }
    if let Some(r) = l.resets_at {
        s.push_str(&format!(" · resets {}", clock(r, tz)));
    }
    s
}

fn run_line(r: &ActiveRun, now: DateTime<Utc>) -> String {
    let dur = r
        .started_at
        .map(|t| span((now - t).num_seconds()))
        .unwrap_or_else(|| "-".into());
    format!(
        "  {}  {}  {}  {}  {}",
        sanitize_line(&r.task),
        sanitize_line(&r.mode),
        dur,
        sanitize_line(&r.repos.join(",")),
        sanitize_line(&r.step)
    )
}

fn event_line(e: &Event, tz: FixedOffset) -> String {
    let at = e.at.map(|t| clock(t, tz)).unwrap_or_else(|| "--:--".into());
    let task = e
        .task
        .as_deref()
        .map(|t| format!(" {}", sanitize_line(t)))
        .unwrap_or_default();
    format!(
        "  {at} {}{task} {}",
        sanitize_line(&e.kind),
        sanitize_line(&e.text)
    )
    .trim_end()
    .to_string()
}

fn header(d: &StatusDoc, running: bool, now: DateTime<Utc>, tz: FixedOffset) -> String {
    let word = match (running, d.state) {
        (false, RunState::Stopped) => "stopped",
        (false, _) => "not running",
        (true, RunState::Running) => "running",
        (true, RunState::Paused) => "paused",
        (true, RunState::Stopping) => "stopping",
        (true, RunState::Stopped) => "stopped",
        (_, RunState::Suspended) => "suspended",
    };
    let queue = d.extra.get("kind").and_then(Value::as_str) == Some("queue");
    let mut s = format!("{} {word}", if queue { "Queue loop" } else { "Autopilot" });
    if !running && d.state != RunState::Stopped {
        s.push_str(&format!(
            " (last state {}, updated {} ago)",
            state_word(d.state),
            span((now - d.updated_at).num_seconds())
        ));
    }
    s.push_str(&format!(
        " · started {} ({} ago) · rule {} · mode {}",
        clock(d.started_at, tz),
        span((now - d.started_at).num_seconds()),
        sanitize_line(&d.rule),
        sanitize_line(&d.mode)
    ));
    if let (RunState::Stopped, Some(why)) = (d.state, &d.stop_reason) {
        s.push_str(&format!(" · stopped: {}", sanitize_line(why)));
    }
    s
}

fn state_word(s: RunState) -> &'static str {
    match s {
        RunState::Running => "running",
        RunState::Paused => "paused",
        RunState::Stopping => "stopping",
        RunState::Stopped => "stopped",
        RunState::Suspended => "suspended",
    }
}

pub fn status_text(d: &StatusDoc, running: bool, now: DateTime<Utc>, tz: FixedOffset) -> String {
    let t = &d.tasks;
    let mut lines = vec![
        header(d, running, now, tz),
        format!(
            "Tasks {} total · {} done (PR) · {} in work · {} needs feedback · {} blocked · {} held · {} failed · {} queued",
            t.total, t.done_pr, t.in_work, t.needs_feedback, t.blocked, t.held, t.failed, t.queued
        ),
        format!(
            "Teams {}/{} running · agents {}/{}",
            d.teams.running, d.teams.max, d.agents.running, d.agents.max
        ),
    ];
    lines.extend(d.runs.iter().map(|r| run_line(r, now)));
    if !d.limits.is_empty() {
        let parts: Vec<String> = d.limits.iter().map(|l| limit_part(l, now, tz)).collect();
        lines.push(format!("Limits {}", parts.join(" / ")));
    } else if !d.settings.usage_guard {
        lines.push("Limits no usage limit (the guard is off)".into());
    }
    lines.push(cost_line(d));
    if let Some(n) = &d.next_stop {
        lines.push(format!(
            "Next stop ≈ {}% of the {} window left before \"{}\"",
            n.pct_left.round(),
            window_name(&n.window),
            sanitize_line(&n.kind)
        ));
    }
    if d.unconfirmed_new > 0 {
        lines.push(format!(
            "{} new tasks need your confirmation",
            d.unconfirmed_new
        ));
    }
    if !d.prs.is_empty() && d.state == RunState::Stopped {
        lines.push("Pull requests".into());
        lines.extend(d.prs.iter().map(|p| {
            format!(
                "  {} {}{}",
                sanitize_line(&p.task),
                sanitize_line(&p.url),
                if p.draft { " (draft)" } else { "" }
            )
        }));
    }
    if !d.events.is_empty() {
        lines.push("Last events".into());
        let skip = d.events.len().saturating_sub(LAST_EVENTS);
        lines.extend(d.events.iter().skip(skip).map(|e| event_line(e, tz)));
    }
    lines.join("\n")
}

fn cost_line(d: &StatusDoc) -> String {
    let c = &d.cost;
    let mut sub = format!("subscription: {}", money(c.subscription_usd));
    if c.subscription_usd > 0.0 && !c.basis.is_empty() {
        sub.push_str(&format!(" ({})", sanitize_line(&c.basis)));
    }
    let api = match (c.api_usd, c.api_cap_usd) {
        (Some(v), Some(cap)) => format!("{} of {} cap", money(v), money(cap)),
        (Some(v), None) => money(v),
        (None, Some(cap)) => format!("none yet, {} cap", money(cap)),
        (None, None) => "n/a".into(),
    };
    format!("Cost {sub} · API: {api}")
}

pub fn not_running_text() -> String {
    "Autopilot is not running.\nStart it with `tmgr queue autopilot`.".into()
}

pub fn log_line(v: &Value, tz: FixedOffset) -> String {
    let Ok(e) = serde_json::from_value::<Event>(v.clone()) else {
        return sanitize_line(&v.to_string());
    };
    if e.kind.is_empty() && e.text.is_empty() {
        return sanitize_line(&v.to_string());
    }
    event_line(&e, tz).trim_start().to_string()
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::cmd::autopilot::model::{
        Answers, Cost, NextStop, PrLink, Slots, TaskCounts, STATUS_VERSION,
    };
    use chrono::TimeZone;
    use serde_json::json;

    fn utc(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 19, h, m, 0).unwrap()
    }

    fn tz() -> FixedOffset {
        FixedOffset::east_opt(2 * 3600).unwrap()
    }

    pub(in crate::cmd::autopilot) fn doc() -> StatusDoc {
        StatusDoc {
            version: STATUS_VERSION,
            state: RunState::Running,
            pid: 4242,
            control_seq: 0,
            settings_id: "ap-20261019-175000".into(),
            started_at: utc(17, 50),
            updated_at: utc(20, 3),
            rule: "tm".into(),
            mode: "auto".into(),
            stop_reason: None,
            stopped_at: None,
            settings: Answers::default(),
            tasks: TaskCounts {
                total: 100,
                done_pr: 23,
                in_work: 4,
                needs_feedback: 9,
                blocked: 2,
                held: 1,
                failed: 3,
                skipped: 0,
                queued: 58,
            },
            teams: Slots { running: 2, max: 2 },
            agents: Slots { running: 5, max: 6 },
            runs: vec![ActiveRun {
                task: "TM-12".into(),
                task_id: 12,
                title: "Fix it".into(),
                mode: "auto".into(),
                started_at: Some(utc(19, 49)),
                repos: vec!["o/api".into(), "o/web".into()],
                step: "running".into(),
                ..ActiveRun::default()
            }],
            limits: vec![
                LimitRow {
                    tool: "claude".into(),
                    window: "five_hour".into(),
                    used_pct: Some(41.0),
                    resets_at: Some(utc(19, 10)),
                    source: "real".into(),
                    origin: Some("get_usage".into()),
                    observed_at: Some(utc(20, 1)),
                    stop_new_at: Some(50),
                    pause_at: Some(80),
                },
                LimitRow {
                    tool: "claude".into(),
                    window: "seven_day".into(),
                    used_pct: None,
                    source: "unknown".into(),
                    ..LimitRow::default()
                },
            ],
            cost: Cost {
                subscription_usd: 0.0,
                api_usd: Some(3.4),
                api_cap_usd: Some(20.0),
                ..Cost::default()
            },
            next_stop: Some(NextStop {
                window: "five_hour".into(),
                pct_left: 9.0,
                kind: "no new tasks".into(),
            }),
            unconfirmed_new: 0,
            failures_in_row: 0,
            prs: vec![],
            events: vec![],
            plan: None,
            triage: Vec::new(),
            not_planned: Vec::new(),
            agents_planned: None,
            needs_feedback_tasks: None,
            blocked_tasks: None,
            queued_tasks: None,
            extra: Default::default(),
        }
    }

    #[test]
    fn running_status_matches_the_approved_shape() {
        let mut d = doc();
        d.events = (0..10)
            .map(|i| Event {
                at: Some(utc(19, 40 + i)),
                kind: "picked".into(),
                task: Some(format!("TM-{i}")),
                text: "started".into(),
            })
            .collect();
        let want = "\
Autopilot running · started 19:50 (2h 13m ago) · rule tm · mode auto
Tasks 100 total · 23 done (PR) · 4 in work · 9 needs feedback · 2 blocked · 1 held · 3 failed · 58 queued
Teams 2/2 running · agents 5/6
  TM-12  auto  14m  o/api,o/web  running
Limits Claude 5h: 41% real 2m ago (stop new at 50%, pause at 80%) · resets 21:10 / Claude week: unknown
Cost subscription: $0 · API: $3.40 of $20 cap
Next stop ≈ 9% of the 5h window left before \"no new tasks\"
Last events
  21:42 picked TM-2 started
  21:43 picked TM-3 started
  21:44 picked TM-4 started
  21:45 picked TM-5 started
  21:46 picked TM-6 started
  21:47 picked TM-7 started
  21:48 picked TM-8 started
  21:49 picked TM-9 started";
        assert_eq!(status_text(&d, true, utc(20, 3), tz()), want);
    }

    #[test]
    fn paused_stopped_and_dead_states_are_told_apart() {
        let mut d = doc();
        d.state = RunState::Paused;
        assert!(status_text(&d, true, utc(20, 3), tz()).starts_with("Autopilot paused ·"));
        let dead = status_text(&d, false, utc(20, 3), tz());
        assert!(
            dead.starts_with("Autopilot not running (last state paused, updated 0s ago) ·"),
            "{dead}"
        );
        d.state = RunState::Stopped;
        d.stop_reason = Some("usage limit".into());
        d.prs = vec![PrLink {
            task: "TM-1".into(),
            url: "https://example.com/pr/1".into(),
            draft: true,
        }];
        let done = status_text(&d, false, utc(20, 3), tz());
        assert!(done.starts_with("Autopilot stopped ·"), "{done}");
        assert!(done.contains("· stopped: usage limit"));
        assert!(done.contains("Pull requests\n  TM-1 https://example.com/pr/1 (draft)"));
    }

    #[test]
    fn estimates_cost_and_missing_numbers_are_labelled_never_invented() {
        let mut d = doc();
        d.limits.clear();
        d.next_stop = None;
        d.cost = Cost {
            subscription_usd: 12.3,
            basis: "estimate (list price)".into(),
            ..Cost::default()
        };
        let t = status_text(&d, true, utc(20, 3), tz());
        assert!(!t.contains("Limits"));
        assert!(!t.contains("Next stop"));
        assert!(t.contains("Cost subscription: $12.30 (estimate (list price)) · API: n/a"));
        d.settings.usage_guard = false;
        assert!(status_text(&d, true, utc(20, 3), tz()).contains("Limits no usage limit"));
    }

    #[test]
    fn terminal_control_characters_in_server_text_are_dropped() {
        let mut d = doc();
        d.runs[0].step = "x\u{1b}[31mred".into();
        let t = status_text(&d, true, utc(20, 3), tz());
        assert!(!t.contains('\u{1b}'));
    }

    #[test]
    fn log_lines_read_like_events() {
        let v =
            json!({"at": "2026-10-19T17:52:00Z", "kind": "pr", "task": "TM-1", "text": "opened"});
        assert_eq!(log_line(&v, tz()), "19:52 pr TM-1 opened");
        assert_eq!(log_line(&json!({"x": 1}), tz()), "{\"x\":1}");
    }

    #[test]
    fn spans_are_short() {
        assert_eq!(span(5), "5s");
        assert_eq!(span(125), "2m");
        assert_eq!(span(8040), "2h 14m");
    }
}
