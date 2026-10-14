use super::fmt::{count, money, round6, total_tokens, BASIS_NOTE};
use super::runs::{self, RunSet, WantedTask};
use super::{fail, scan, ScanTotals, Usage};
use crate::cli_usage::{By, SummaryArgs, ToolArg};
use crate::ctx::Ctx;
use crate::out::{table, Output};
use chrono::{DateTime, Duration, Local, NaiveDate, SecondsFormat, Utc};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use tmgr_core::error::{Error, Result};
use tmgr_core::refs::{parse_task_ref, TaskRef};
use tmgr_core::runs::day_bounds;
use tmgr_core::text::sanitize_line;
use tmgr_usage::link::{attribute_at, LinkEvent, LinkReport};
use tmgr_usage::{Filters, GroupBy, Query, Report, Row, Tokens, Tool, Tz};

struct Window {
    key: &'static str,
    label: &'static str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
}

fn stamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn parse_day(raw: &str, flag: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .map_err(|_| Error::usage(format!("invalid {flag}")).hint("use YYYY-MM-DD"))
}

fn span_days(today: NaiveDate, days: i64) -> (DateTime<Utc>, DateTime<Utc>) {
    let first = today - Duration::days(days - 1);
    (day_bounds(&Local, first).0, day_bounds(&Local, today).1)
}

fn windows(args: &SummaryArgs, now: DateTime<Utc>) -> Result<(Vec<Window>, bool)> {
    let today = now.with_timezone(&Local).date_naive();
    let one = |key, label, from, to| {
        Ok((
            vec![Window {
                key,
                label,
                from,
                to,
            }],
            false,
        ))
    };
    if args.today {
        let (from, to) = day_bounds(&Local, today);
        return one("today", "Today", from, to);
    }
    if args.yesterday {
        let (from, to) = day_bounds(&Local, today - Duration::days(1));
        return one("yesterday", "Yesterday", from, to);
    }
    if args.week {
        let (from, to) = span_days(today, 7);
        return one("week", "Last 7 days", from, to);
    }
    if args.month {
        let (from, to) = span_days(today, 30);
        return one("month", "Last 30 days", from, to);
    }
    if let Some(since) = args.since.as_deref() {
        let first = parse_day(since, "--since")?;
        let last = match args.until.as_deref() {
            Some(u) => parse_day(u, "--until")?,
            None => today,
        };
        if last < first {
            return Err(Error::usage("--until is before --since"));
        }
        let (from, to) = (day_bounds(&Local, first).0, day_bounds(&Local, last).1);
        return one("range", "Selected days", from, to);
    }
    let (w7, w30) = (span_days(today, 7), span_days(today, 30));
    let (t0, t1) = day_bounds(&Local, today);
    Ok((
        vec![
            Window {
                key: "today",
                label: "Today",
                from: t0,
                to: t1,
            },
            Window {
                key: "week",
                label: "7 days",
                from: w7.0,
                to: w7.1,
            },
            Window {
                key: "month",
                label: "30 days",
                from: w30.0,
                to: w30.1,
            },
        ],
        true,
    ))
}

fn tool_of(t: ToolArg) -> Tool {
    match t {
        ToolArg::Claude => Tool::Claude,
        ToolArg::Codex => Tool::Codex,
    }
}

fn group_of(by: Option<By>) -> Vec<GroupBy> {
    match by {
        None => vec![GroupBy::Tool, GroupBy::Model],
        Some(By::Tool) => vec![GroupBy::Tool],
        Some(By::Model) => vec![GroupBy::Model],
        Some(By::Account) => vec![GroupBy::Account],
        Some(By::Project) => vec![GroupBy::Project],
        Some(By::Branch) => vec![GroupBy::Branch],
        Some(By::Day) => vec![GroupBy::Day],
        Some(By::Task) => Vec::new(),
    }
}

fn dim_name(g: GroupBy) -> &'static str {
    match g {
        GroupBy::Day => "day",
        GroupBy::Tool => "tool",
        GroupBy::Model => "model",
        GroupBy::Account => "account",
        GroupBy::Project => "project",
        GroupBy::Branch => "branch",
        GroupBy::Session => "session",
    }
}

fn dim_title(g: GroupBy) -> &'static str {
    match g {
        GroupBy::Day => "Day",
        GroupBy::Tool => "Tool",
        GroupBy::Model => "Model",
        GroupBy::Account => "Account",
        GroupBy::Project => "Project",
        GroupBy::Branch => "Branch",
        GroupBy::Session => "Session",
    }
}

fn tokens_json(t: &Tokens) -> Value {
    json!({
        "input": t.input,
        "output": t.output,
        "cache_read": t.cache_read,
        "cache_write_5m": t.cache_write_5m,
        "cache_write_1h": t.cache_write_1h,
        "reasoning": t.reasoning,
    })
}

fn row_json(r: &Row, by: &[GroupBy]) -> Value {
    let mut group = Map::new();
    for (i, g) in by.iter().enumerate() {
        let value = if *g == GroupBy::Project {
            json!({"name": sanitize_line(&r.labels[i]), "hash": r.keys[i]})
        } else {
            json!(sanitize_line(&r.labels[i]))
        };
        group.insert(dim_name(*g).to_string(), value);
    }
    json!({
        "group": group,
        "tokens": tokens_json(&r.tokens),
        "tokens_total": total_tokens(&r.tokens),
        "cost_usd": round6(r.cost_usd),
        "events": r.events,
        "unpriced_events": r.unpriced_events,
    })
}

fn label_of(r: &Row, i: usize) -> String {
    let l = sanitize_line(&r.labels[i]);
    if l.is_empty() {
        "(unknown)".to_string()
    } else {
        l
    }
}

fn filters(args: &SummaryArgs) -> Filters {
    Filters {
        tools: args.tool.map(tool_of).into_iter().collect(),
        accounts: args.account.iter().cloned().collect(),
        ..Filters::default()
    }
}

pub fn run(ctx: &Ctx, args: &SummaryArgs) -> Result<Output> {
    if args.by == Some(By::Task) && args.task.is_some() {
        return Err(Error::usage("--by task cannot be combined with --task"));
    }
    let want = match args.task.as_deref() {
        Some(raw) => Some(match parse_task_ref(raw)? {
            TaskRef::Id(id) => WantedTask::Id(id),
            TaskRef::Key(k) => WantedTask::Key(k),
        }),
        None => None,
    };
    let now = Utc::now();
    let (mut wins, default) = windows(args, now)?;
    let task_mode = args.by == Some(By::Task) || want.is_some();
    if default && task_mode {
        wins.drain(..2);
    }
    let mut u = Usage::open()?;
    let scanned = if args.no_scan {
        None
    } else {
        Some(scan(ctx, &mut u, now)?)
    };
    let out = if task_mode {
        task_view(ctx, &u, args, &wins[0], want.as_ref(), now)?
    } else {
        plain_view(&u, args, &wins)?
    };
    if out.unpriced > 0 {
        ctx.warn(
            "usage_unpriced",
            format!(
                "{} events use models without a known price and are left out of the cost (tmgr usage pricing update)",
                out.unpriced
            ),
        );
    }
    let mut data = out.data;
    data["scan"] = scanned.map_or(Value::Null, scan_json);
    Ok(Output::new(data, out.human))
}

fn scan_json(t: ScanTotals) -> Value {
    json!({
        "files": t.files,
        "events_new": t.events_new,
        "events_updated": t.events_updated,
        "errors": t.errors,
        "bad_lines": t.bad_lines,
        "skipped_before_floor": t.skipped_before_floor,
    })
}

struct View {
    data: Value,
    human: String,
    unpriced: u64,
}

fn window_json(w: &Window, report: &Report, by: &[GroupBy]) -> Value {
    json!({
        "key": w.key,
        "label": w.label,
        "from": stamp(w.from),
        "to": stamp(w.to),
        "totals": row_json(&report.totals, &[]),
        "rows": report.rows.iter().map(|r| row_json(r, by)).collect::<Vec<_>>(),
    })
}

fn plain_view(u: &Usage, args: &SummaryArgs, wins: &[Window]) -> Result<View> {
    let by = group_of(args.by);
    let mut reports = Vec::new();
    for w in wins {
        let q = Query {
            from: w.from,
            to: w.to,
            tz: Tz::Local,
            group_by: by.clone(),
            filters: filters(args),
        };
        reports.push(u.store.aggregate(&q).map_err(fail)?);
    }
    let data = json!({
        "basis": "api_equivalent",
        "group_by": by.iter().map(|g| dim_name(*g)).collect::<Vec<_>>(),
        "windows": wins.iter().zip(&reports).map(|(w, r)| window_json(w, r, &by)).collect::<Vec<_>>(),
        "linking": Value::Null,
        "task": Value::Null,
    });
    let last = reports.last().map_or(0, |r| r.totals.unpriced_events);
    let human = if wins.len() > 1 {
        matrix(wins, &reports, &by)
    } else {
        period_table(&wins[0], &reports[0], &by)
    };
    Ok(View {
        data,
        human,
        unpriced: last,
    })
}

fn matrix(wins: &[Window], reports: &[Report], by: &[GroupBy]) -> String {
    let mut keys: BTreeMap<Vec<String>, Vec<String>> = BTreeMap::new();
    for r in reports {
        for row in &r.rows {
            keys.entry(row.keys.clone())
                .or_insert_with(|| (0..by.len()).map(|i| label_of(row, i)).collect());
        }
    }
    let mut head: Vec<String> = by.iter().map(|g| dim_title(*g).to_string()).collect();
    head.extend(wins.iter().map(|w| format!("{} (API-eq.)", w.label)));
    let mut rows = vec![head];
    for (k, labels) in &keys {
        let mut line = labels.clone();
        for r in reports {
            line.push(
                r.rows
                    .iter()
                    .find(|row| &row.keys == k)
                    .map_or("-".to_string(), |row| money(row.cost_usd)),
            );
        }
        rows.push(line);
    }
    let mut total = vec!["Total".to_string()];
    total.resize(by.len(), String::new());
    total.extend(reports.iter().map(|r| money(r.totals.cost_usd)));
    rows.push(total);
    let long = &reports[reports.len() - 1].totals;
    if long.events == 0 {
        return format!("AI usage: {BASIS_NOTE}\n\nNo usage recorded yet. Is Claude Code or Codex installed? tmgr usage accounts");
    }
    format!(
        "AI usage: {BASIS_NOTE}\n\n{}\n\nLast 30 days tokens: input {}, output {}, cache read {}, cache write {}",
        table(&rows),
        count(long.tokens.input),
        count(long.tokens.output),
        count(long.tokens.cache_read),
        count(long.tokens.cache_write_5m + long.tokens.cache_write_1h),
    )
}

fn period_table(w: &Window, r: &Report, by: &[GroupBy]) -> String {
    if r.totals.events == 0 {
        return format!("AI usage, {}: no usage recorded.", w.label.to_lowercase());
    }
    let mut head: Vec<String> = by.iter().map(|g| dim_title(*g).to_string()).collect();
    head.extend(
        [
            "API-eq. cost",
            "Input",
            "Output",
            "Cache read",
            "Cache write",
            "Events",
        ]
        .map(String::from),
    );
    let cells = |row: &Row| {
        vec![
            money(row.cost_usd),
            count(row.tokens.input),
            count(row.tokens.output),
            count(row.tokens.cache_read),
            count(row.tokens.cache_write_5m + row.tokens.cache_write_1h),
            row.events.to_string(),
        ]
    };
    let mut rows = vec![head];
    for row in &r.rows {
        let mut line: Vec<String> = (0..by.len()).map(|i| label_of(row, i)).collect();
        line.extend(cells(row));
        rows.push(line);
    }
    if r.rows.len() > 1 {
        let mut line = vec!["Total".to_string()];
        line.resize(by.len().max(1), String::new());
        line.extend(cells(&r.totals));
        rows.push(line);
    }
    format!(
        "AI usage, {}: {BASIS_NOTE}\n\n{}",
        w.label.to_lowercase(),
        table(&rows)
    )
}

#[derive(Default, Clone)]
struct Acc {
    cost: f64,
    tokens: u64,
    events: u64,
    unpriced: u64,
}

impl Acc {
    fn add(&mut self, e: &LinkEvent) {
        self.events += 1;
        self.tokens += e.tokens;
        match e.cost_usd {
            Some(c) => self.cost += c,
            None => self.unpriced += 1,
        }
    }

    fn json(&self, group: Value) -> Value {
        json!({
            "group": group,
            "tokens": Value::Null,
            "tokens_total": self.tokens,
            "cost_usd": round6(self.cost),
            "events": self.events,
            "unpriced_events": self.unpriced,
        })
    }
}

fn task_events(u: &Usage, args: &SummaryArgs, w: &Window) -> Result<Vec<LinkEvent>> {
    let mut events = u.store.link_events(w.from, w.to).map_err(fail)?;
    if let Some(t) = args.tool.map(tool_of) {
        events.retain(|e| e.tool == t);
    }
    if let Some(a) = args.account.as_deref() {
        events.retain(|e| e.account == a);
    }
    Ok(events)
}

fn linking_json(set: &RunSet, report: &LinkReport) -> Value {
    json!({
        "runs": set.windows.len(),
        "workspaces_ok": set.workspaces_ok,
        "workspaces_failed": set.workspaces_failed,
        "linked_events": report.links.len(),
        "ambiguous_events": report.ambiguous.len(),
        "unattributed_events": report.unattributed.len(),
    })
}

fn task_view(
    ctx: &Ctx,
    u: &Usage,
    args: &SummaryArgs,
    w: &Window,
    want: Option<&WantedTask>,
    now: DateTime<Utc>,
) -> Result<View> {
    if u.store
        .history_floor()
        .map_err(fail)?
        .is_some_and(|floor| w.from < floor)
    {
        ctx.warn(
            "usage_task_history_limited",
            "the range starts before the stored events: task views only cover the kept recent history",
        );
    }
    let events = task_events(u, args, w)?;
    let set = runs::collect(ctx, w.from, w.to, now);
    let report = attribute_at(&events, &set.windows, now);
    let linking = linking_json(&set, &report);
    let unpriced = events.iter().filter(|e| e.cost_usd.is_none()).count() as u64;
    match want {
        Some(want) => single_task(args, w, want, &events, &set, &report, linking),
        None => all_tasks(w, &events, &report, linking, unpriced),
    }
}

fn all_tasks(
    w: &Window,
    events: &[LinkEvent],
    report: &LinkReport,
    linking: Value,
    unpriced: u64,
) -> Result<View> {
    let mut tasks = report.per_task.clone();
    tasks.sort_by(|a, b| b.cost_usd.total_cmp(&a.cost_usd).then(a.key.cmp(&b.key)));
    let acc = |t: &tmgr_usage::link::Totals| Acc {
        cost: t.cost_usd,
        tokens: t.tokens,
        events: t.events,
        unpriced: t.unpriced_events,
    };
    let mut rows: Vec<(String, String, Acc, Value)> = tasks
        .iter()
        .map(|t| {
            (
                sanitize_line(&t.key),
                "task".to_string(),
                acc(t),
                json!({"task": sanitize_line(&t.key)}),
            )
        })
        .collect();
    let mut candidates: Vec<String> = report
        .ambiguous
        .iter()
        .flat_map(|a| a.candidates.iter().cloned())
        .collect();
    candidates.sort();
    candidates.dedup();
    if report.ambiguous_totals.events > 0 {
        rows.push((
            "(ambiguous)".into(),
            "ambiguous".into(),
            acc(&report.ambiguous_totals),
            json!({"task": Value::Null}),
        ));
    }
    if report.unattributed_totals.events > 0 {
        rows.push((
            "(unattributed)".into(),
            "unattributed".into(),
            acc(&report.unattributed_totals),
            json!({"task": Value::Null}),
        ));
    }
    let mut total = Acc::default();
    for e in events {
        total.add(e);
    }
    let json_rows: Vec<Value> = rows
        .iter()
        .map(|(_, bucket, a, group)| {
            let mut v = a.json(group.clone());
            v["bucket"] = json!(bucket);
            if bucket == "ambiguous" {
                v["candidate_runs"] = json!(candidates.iter().take(20).collect::<Vec<_>>());
            }
            v
        })
        .collect();
    let window = json!({
        "key": w.key, "label": w.label, "from": stamp(w.from), "to": stamp(w.to),
        "totals": total.json(json!({})), "rows": json_rows,
    });
    let data = json!({
        "basis": "api_equivalent",
        "group_by": ["task"],
        "windows": [window],
        "linking": linking,
        "task": Value::Null,
    });
    let human =
        if events.is_empty() {
            format!(
                "AI usage by task, {}: no usage recorded.",
                w.label.to_lowercase()
            )
        } else {
            let mut lines = vec![["Task", "API-eq. cost", "Tokens", "Events"]
                .map(String::from)
                .to_vec()];
            for (label, _, a, _) in &rows {
                lines.push(vec![
                    label.clone(),
                    money(a.cost),
                    count(a.tokens),
                    a.events.to_string(),
                ]);
            }
            lines.push(vec![
                "Total".into(),
                money(total.cost),
                count(total.tokens),
                total.events.to_string(),
            ]);
            let mut text = format!(
                "AI usage by task, {}: {BASIS_NOTE}\n\n{}",
                w.label.to_lowercase(),
                table(&lines)
            );
            if report.ambiguous_totals.events > 0 {
                text.push_str(&format!(
                "\n\nAmbiguous events matched several runs ({}) and are counted once, here only.",
                candidates.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
            ));
            }
            text
        };
    Ok(View {
        data,
        human,
        unpriced,
    })
}

fn event_key(e: &LinkEvent, by: Option<By>) -> Result<(String, &'static str)> {
    Ok(match by {
        None | Some(By::Model) => (e.model.clone(), "model"),
        Some(By::Tool) => (e.tool.as_str().to_string(), "tool"),
        Some(By::Account) => (e.account.clone(), "account"),
        Some(By::Branch) => (e.git_branch.clone().unwrap_or_default(), "branch"),
        Some(By::Day) => (Tz::Local.day(e.ts).format("%Y-%m-%d").to_string(), "day"),
        Some(By::Project) | Some(By::Task) => {
            return Err(Error::usage(
                "this --by dimension is not available with --task",
            ))
        }
    })
}

fn single_task(
    args: &SummaryArgs,
    w: &Window,
    want: &WantedTask,
    events: &[LinkEvent],
    set: &RunSet,
    report: &LinkReport,
    linking: Value,
) -> Result<View> {
    let mut groups: BTreeMap<String, Acc> = BTreeMap::new();
    let mut total = Acc::default();
    let mut dim = "model";
    let mut methods: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut per_run: BTreeMap<String, Acc> = BTreeMap::new();
    for link in &report.links {
        if !set.task_matches(&link.run_ref, want) {
            continue;
        }
        let e = &events[link.event_index];
        let (key, d) = event_key(e, args.by)?;
        dim = d;
        groups.entry(key).or_default().add(e);
        per_run.entry(link.run_ref.clone()).or_default().add(e);
        total.add(e);
        *methods.entry(link.method.as_str()).or_default() += 1;
    }
    let label = match want {
        WantedTask::Id(id) => format!("#{id}"),
        WantedTask::Key(k) => k.clone(),
    };
    let rows: Vec<Value> = groups
        .iter()
        .map(|(k, a)| a.json(json!({ dim: sanitize_line(k) })))
        .collect();
    let run_rows: Vec<Value> = per_run
        .iter()
        .map(|(r, a)| {
            json!({
                "run": sanitize_line(r),
                "cost_usd": round6(a.cost),
                "tokens_total": a.tokens,
                "events": a.events,
                "unpriced_events": a.unpriced,
            })
        })
        .collect();
    let window = json!({
        "key": w.key, "label": w.label, "from": stamp(w.from), "to": stamp(w.to),
        "totals": total.json(json!({})), "rows": rows,
    });
    let data = json!({
        "basis": "api_equivalent",
        "group_by": [dim],
        "windows": [window],
        "linking": linking,
        "task": {
            "ref": label,
            "runs": run_rows,
            "link_methods": methods,
        },
    });
    let human = if total.events == 0 {
        format!(
            "No usage linked to {} in this period.\nLinking needs run windows: tmgr agent start records the session; see tmgr usage --by task for unattributed usage.",
            sanitize_line(&label)
        )
    } else {
        let mut lines = vec![[dim_title_of(dim), "API-eq. cost", "Tokens", "Events"]
            .map(String::from)
            .to_vec()];
        for (k, a) in &groups {
            lines.push(vec![
                if k.is_empty() {
                    "(unknown)".into()
                } else {
                    sanitize_line(k)
                },
                money(a.cost),
                count(a.tokens),
                a.events.to_string(),
            ]);
        }
        let mut runs_lines = vec![["Run", "API-eq. cost", "Tokens", "Events"]
            .map(String::from)
            .to_vec()];
        for (r, a) in &per_run {
            runs_lines.push(vec![
                sanitize_line(r),
                money(a.cost),
                count(a.tokens),
                a.events.to_string(),
            ]);
        }
        format!(
            "Task {}, {}: {} API-equivalent cost ({}), {} tokens, {} events\n\n{}\n\n{}",
            sanitize_line(&label),
            w.label.to_lowercase(),
            money(total.cost),
            BASIS_NOTE,
            count(total.tokens),
            total.events,
            table(&lines),
            table(&runs_lines)
        )
    };
    Ok(View {
        data,
        human,
        unpriced: total.unpriced,
    })
}

fn dim_title_of(dim: &str) -> &'static str {
    match dim {
        "tool" => "Tool",
        "account" => "Account",
        "branch" => "Branch",
        "day" => "Day",
        _ => "Model",
    }
}
