use chrono::{
    DateTime, Datelike, Duration, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime,
    SecondsFormat, TimeZone, Utc, Weekday,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

use crate::aggregate::Tz;
use crate::error::{Result, UsageError};
use crate::limits::{LimitSnapshot, LimitSource, LimitWindow};
use crate::link::LinkEvent;
use crate::model::{Tokens, Tool};
use crate::store::Store;

pub const SCHEMA: &str = "machine_usage.v1";
pub const AUTO_MIN_GAP: Duration = Duration::minutes(5);
pub const MAX_DAYS: i64 = 120;
pub const FIRST_DAYS: i64 = 30;
pub const AUTO_DAYS: i64 = 2;
pub const MAX_ROWS_PER_DAY: usize = 500;
pub const MAX_LIMITS: usize = 100;
pub const MAX_BODY: usize = 256 * 1024;
const CHUNK_BODY: usize = 200 * 1024;

const ID_KEY: &str = "machine_id";
const LABEL_KEY: &str = "machine_label";
const SYNC_KEY: &str = "machine_sync";
const LAST_SYNC_KEY: &str = "machine_last_sync";
const COVERED_DAY_KEY: &str = "machine_covered_day";
const REENABLE_KEY: &str = "machine_reenable";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    pub agent: String,
    pub model: String,
    pub task_id: Option<u64>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: u64,
    pub cost_usd: f64,
    pub sessions: u64,
    pub events: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Day {
    pub day: String,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Limit {
    pub agent: String,
    pub account: String,
    pub window: String,
    pub used_pct: f64,
    pub resets_at: Option<String>,
    pub plan: Option<String>,
    pub observed_at: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub label: String,
    pub platform: String,
    pub cli_version: String,
    pub generated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    pub limits: Vec<Limit>,
    pub days: Vec<Day>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reenable: bool,
}

pub struct Identity {
    pub label: String,
    pub platform: String,
    pub cli_version: String,
    pub timezone: Option<String>,
    pub reenable: bool,
}

pub struct Input<'a> {
    pub identity: &'a Identity,
    pub now: DateTime<Utc>,
    pub tz: Tz,
    pub from_day: NaiveDate,
    pub to_day: NaiveDate,
    pub complete_from: Option<DateTime<Utc>>,
    pub events: &'a [(LinkEvent, Tokens)],
    pub task_ids: &'a [Option<u64>],
    pub limits: &'a [LimitSnapshot],
}

pub fn agent_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "claude-code",
        Tool::Codex => "codex",
    }
}

pub fn clean_account(label: &str) -> String {
    let l = label.trim();
    let bad = l.is_empty()
        || l.contains(['/', '\\', '@'])
        || l.starts_with('~')
        || l.chars().any(char::is_control);
    if bad {
        return "default".into();
    }
    l.chars().take(64).collect()
}

pub fn local_midnight(tz: Tz, day: NaiveDate) -> Option<DateTime<Utc>> {
    resolve(tz, day.and_time(NaiveTime::MIN))
}

fn resolve(tz: Tz, naive: NaiveDateTime) -> Option<DateTime<Utc>> {
    match tz {
        Tz::Fixed(off) => off
            .from_local_datetime(&naive)
            .single()
            .map(|d| d.with_timezone(&Utc)),
        Tz::Local => Local
            .from_local_datetime(&naive)
            .earliest()
            .map(|d| d.with_timezone(&Utc)),
    }
}

#[derive(Default)]
struct Cell {
    tokens: Tokens,
    cost: f64,
    sessions: BTreeSet<String>,
    events: u64,
}

fn round6(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

fn finish_row(agent: &str, model: &str, task_id: Option<u64>, c: &Cell) -> Row {
    Row {
        agent: agent.to_string(),
        model: model.to_string(),
        task_id,
        input_tokens: c.tokens.input,
        output_tokens: c.tokens.output,
        cache_read_tokens: c.tokens.cache_read,
        cache_write_tokens: c.tokens.cache_write_5m + c.tokens.cache_write_1h,
        reasoning_tokens: c.tokens.reasoning,
        cost_usd: round6(c.cost),
        sessions: c.sessions.len() as u64,
        events: c.events,
    }
}

type Key = (String, String, Option<u64>);

fn day_rows(cells: BTreeMap<Key, Cell>) -> Result<Vec<Row>> {
    let rows: Vec<Row> = cells
        .iter()
        .map(|((a, m, t), c)| finish_row(a, m, *t, c))
        .collect();
    if rows.len() <= MAX_ROWS_PER_DAY {
        return Ok(rows);
    }
    let mut merged: BTreeMap<Key, Cell> = BTreeMap::new();
    for ((a, m, _), c) in cells {
        let into = merged.entry((a, m, None)).or_default();
        add_tokens(&mut into.tokens, &c.tokens);
        into.cost += c.cost;
        into.events += c.events;
        into.sessions.extend(c.sessions);
    }
    if merged.len() > MAX_ROWS_PER_DAY {
        return Err(UsageError::Invalid(
            "a day has more than 500 agent and model combinations".into(),
        ));
    }
    Ok(merged
        .iter()
        .map(|((a, m, t), c)| finish_row(a, m, *t, c))
        .collect())
}

fn add_tokens(a: &mut Tokens, b: &Tokens) {
    a.input += b.input;
    a.output += b.output;
    a.cache_read += b.cache_read;
    a.cache_write_5m += b.cache_write_5m;
    a.cache_write_1h += b.cache_write_1h;
    a.reasoning += b.reasoning;
}

fn stamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn build_limits(limits: &[LimitSnapshot]) -> Vec<Limit> {
    let mut best: BTreeMap<(String, String, String), &LimitSnapshot> = BTreeMap::new();
    for l in limits {
        let key = (
            agent_name(l.tool).to_string(),
            clean_account(&l.account),
            l.window.as_key(),
        );
        if key.2.len() > 64 {
            continue;
        }
        match best.get(&key) {
            Some(b) if b.observed_at >= l.observed_at => {}
            _ => {
                best.insert(key, l);
            }
        }
    }
    best.into_iter()
        .take(MAX_LIMITS)
        .map(|((agent, account, window), l)| Limit {
            agent,
            account,
            window,
            used_pct: (l.used_pct * 100.0).round() / 100.0,
            resets_at: l.resets_at.map(stamp),
            plan: l
                .plan
                .as_ref()
                .filter(|p| !p.is_empty())
                .map(|p| p.chars().take(32).collect()),
            observed_at: stamp(l.observed_at),
            source: l.source.as_str().to_string(),
        })
        .collect()
}

pub fn build_report(input: &Input<'_>) -> Result<Report> {
    let mut days: BTreeMap<NaiveDate, BTreeMap<Key, Cell>> = BTreeMap::new();
    for (i, (ev, tokens)) in input.events.iter().enumerate() {
        let day = input.tz.day(ev.ts);
        if day < input.from_day || day > input.to_day {
            continue;
        }
        let task = input.task_ids.get(i).copied().flatten();
        let model: String = ev.model.chars().take(128).collect();
        let cell = days
            .entry(day)
            .or_default()
            .entry((agent_name(ev.tool).to_string(), model, task))
            .or_default();
        add_tokens(&mut cell.tokens, tokens);
        cell.cost += ev.cost_usd.unwrap_or(0.0);
        cell.events += 1;
        cell.sessions.insert(ev.tool_session_id.clone());
    }
    let mut out = Vec::new();
    for (day, cells) in days {
        let complete = match (input.complete_from, local_midnight(input.tz, day)) {
            (Some(floor), Some(start)) => start >= floor,
            _ => true,
        };
        if !complete {
            continue;
        }
        out.push(Day {
            day: day.format("%Y-%m-%d").to_string(),
            rows: day_rows(cells)?,
        });
    }
    let id = input.identity;
    Ok(Report {
        schema: SCHEMA,
        label: id.label.clone(),
        platform: id.platform.clone(),
        cli_version: id.cli_version.chars().take(32).collect(),
        generated_at: stamp(input.now),
        timezone: id.timezone.clone(),
        limits: build_limits(input.limits),
        days: out,
        reenable: id.reenable,
    })
}

impl Report {
    /// Splits the days over several bodies, each under the server's body cap; the limits and the
    /// re-enable flag travel in the first one only.
    pub fn bodies(&self) -> Result<Vec<serde_json::Value>> {
        let size = |r: &Report| serde_json::to_vec(r).map(|b| b.len()).unwrap_or(usize::MAX);
        let mut parts: Vec<Report> = Vec::new();
        let mut head = self.clone();
        head.days.clear();
        let mut current = head.clone();
        for day in &self.days {
            current.days.push(day.clone());
            if size(&current) > CHUNK_BODY && current.days.len() > 1 {
                let last = current.days.pop().expect("a day was just pushed");
                parts.push(current);
                current = self.clone();
                current.limits.clear();
                current.reenable = false;
                current.days = vec![last];
            }
        }
        parts.push(current);
        let mut bodies = Vec::new();
        for p in parts {
            if size(&p) > MAX_BODY {
                return Err(UsageError::Invalid("the body is larger than 256 KB".into()));
            }
            bodies.push(serde_json::to_value(p)?);
        }
        Ok(bodies)
    }
}

pub fn auto_due(last_sync: Option<DateTime<Utc>>, now: DateTime<Utc>, limit_hit: bool) -> bool {
    limit_hit || last_sync.is_none_or(|t| now - t >= AUTO_MIN_GAP || now < t)
}

pub fn days_to_send(
    auto: bool,
    covered: Option<NaiveDate>,
    today: NaiveDate,
    requested: Option<i64>,
) -> i64 {
    let n = match requested {
        Some(n) => n,
        None if auto => covered.map_or(FIRST_DAYS, |c| ((today - c).num_days() + 1).max(AUTO_DAYS)),
        None => FIRST_DAYS,
    };
    n.clamp(1, MAX_DAYS)
}

pub fn valid_label(label: &str) -> bool {
    let n = label.chars().count();
    (1..=64).contains(&n) && !label.chars().any(char::is_control) && !label.trim().is_empty()
}

pub fn platform() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "linux" => "linux",
        "windows" => "windows",
        _ => "other",
    }
}

pub fn short_hostname() -> String {
    let raw = raw_hostname().unwrap_or_default();
    let short = raw.split('.').next().unwrap_or("").trim().to_string();
    let short: String = short.chars().filter(|c| !c.is_control()).take(64).collect();
    if short.is_empty() {
        "this-machine".into()
    } else {
        short
    }
}

#[cfg(unix)]
fn raw_hostname() -> Option<String> {
    let mut buf = [0u8; 256];
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len() - 1) };
    if rc != 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8(buf[..end].to_vec()).ok()
}

#[cfg(not(unix))]
fn raw_hostname() -> Option<String> {
    std::env::var("COMPUTERNAME").ok()
}

fn uuid_v4(random: [u8; 16]) -> String {
    let mut b = random;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

impl Store {
    pub fn machine_id(&self) -> Result<String> {
        if let Some(id) = self.setting(ID_KEY)? {
            return Ok(id);
        }
        let blob: Vec<u8> = self
            .conn
            .query_row("SELECT randomblob(16)", [], |r| r.get(0))?;
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&blob);
        let id = uuid_v4(bytes);
        self.conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, ?2)",
            [ID_KEY, id.as_str()],
        )?;
        Ok(self.setting(ID_KEY)?.unwrap_or(id))
    }

    pub fn machine_label(&self) -> Result<String> {
        Ok(self
            .setting(LABEL_KEY)?
            .filter(|l| valid_label(l))
            .unwrap_or_else(short_hostname))
    }

    pub fn set_machine_label(&self, label: &str) -> Result<()> {
        if !valid_label(label) {
            return Err(UsageError::Invalid(
                "the machine name must be 1 to 64 characters".into(),
            ));
        }
        self.set_setting(LABEL_KEY, label.trim())
    }

    pub fn machine_sync_on(&self) -> Result<bool> {
        Ok(self.setting(SYNC_KEY)?.as_deref() != Some("off"))
    }

    pub fn set_machine_sync(&self, on: bool) -> Result<()> {
        if on {
            self.conn
                .execute("DELETE FROM settings WHERE key = ?1", [SYNC_KEY])?;
            self.set_setting(REENABLE_KEY, "1")
        } else {
            self.set_setting(SYNC_KEY, "off")
        }
    }

    pub fn machine_reenable(&self) -> Result<bool> {
        Ok(self.setting(REENABLE_KEY)?.is_some())
    }

    pub fn clear_machine_reenable(&self) -> Result<()> {
        self.conn
            .execute("DELETE FROM settings WHERE key = ?1", [REENABLE_KEY])?;
        Ok(())
    }

    pub fn machine_last_sync(&self) -> Result<Option<DateTime<Utc>>> {
        Ok(self
            .setting(LAST_SYNC_KEY)?
            .and_then(|v| DateTime::parse_from_rfc3339(&v).ok())
            .map(|t| t.with_timezone(&Utc)))
    }

    pub fn set_machine_last_sync(&self, at: DateTime<Utc>) -> Result<()> {
        self.set_setting(LAST_SYNC_KEY, &stamp(at))
    }

    pub fn machine_covered_day(&self) -> Result<Option<NaiveDate>> {
        Ok(self.setting(COVERED_DAY_KEY)?.and_then(|v| v.parse().ok()))
    }

    pub fn set_machine_covered_day(&self, day: NaiveDate) -> Result<()> {
        self.set_setting(COVERED_DAY_KEY, &day.to_string())
    }
}

struct ResetText<'a> {
    time: NaiveTime,
    date: DateSpec,
    zone: Option<&'a str>,
}

enum DateSpec {
    Today,
    Weekday(Weekday),
    MonthDay(u32, u32),
}

fn month_of(s: &str) -> Option<u32> {
    let s = s.to_ascii_lowercase();
    let names = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let head = s.get(..3)?;
    names.iter().position(|n| *n == head).map(|i| i as u32 + 1)
}

fn weekday_of(s: &str) -> Option<Weekday> {
    let s = s.to_ascii_lowercase();
    match s.get(..3)? {
        "mon" => Some(Weekday::Mon),
        "tue" => Some(Weekday::Tue),
        "wed" => Some(Weekday::Wed),
        "thu" => Some(Weekday::Thu),
        "fri" => Some(Weekday::Fri),
        "sat" => Some(Weekday::Sat),
        "sun" => Some(Weekday::Sun),
        _ => None,
    }
}

fn parse_time(tokens: &mut Vec<&str>) -> Option<NaiveTime> {
    let last = tokens.pop()?.to_ascii_lowercase();
    let (clock, meridiem) = if last == "am" || last == "pm" {
        (tokens.pop()?.to_ascii_lowercase(), Some(last))
    } else if let Some(c) = last.strip_suffix("am") {
        (c.to_string(), Some("am".to_string()))
    } else if let Some(c) = last.strip_suffix("pm") {
        (c.to_string(), Some("pm".to_string()))
    } else {
        (last, None)
    };
    let (h, m) = match clock.split_once(':') {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None => (clock.parse::<u32>().ok()?, 0),
    };
    let hour = match meridiem.as_deref() {
        Some(ap) => {
            if !(1..=12).contains(&h) {
                return None;
            }
            (h % 12) + if ap == "pm" { 12 } else { 0 }
        }
        None if clock.contains(':') => h,
        None => return None,
    };
    NaiveTime::from_hms_opt(hour, m, 0)
}

fn parse_reset(line: &str) -> Option<ResetText<'_>> {
    let lower = line.to_ascii_lowercase();
    let at = lower.find("resets")? + "resets".len();
    let mut rest = line.get(at..)?.trim();
    let mut zone = None;
    if let (Some(open), Some(close)) = (rest.find('('), rest.rfind(')')) {
        if open < close {
            zone = Some(rest[open + 1..close].trim());
            rest = rest[..open].trim();
        }
    }
    let rest = rest.trim_end_matches(['.', ' ']);
    let mut tokens: Vec<&str> = rest
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty() && !t.eq_ignore_ascii_case("at") && !t.eq_ignore_ascii_case("on"))
        .collect();
    let time = parse_time(&mut tokens)?;
    let date = match tokens.as_slice() {
        [] => DateSpec::Today,
        [w] => DateSpec::Weekday(weekday_of(w)?),
        [m, d] => DateSpec::MonthDay(
            month_of(m)?,
            d.trim_end_matches(|c: char| !c.is_ascii_digit())
                .parse()
                .ok()?,
        ),
        _ => return None,
    };
    Some(ResetText { time, date, zone })
}

fn next_reset(r: &ResetText<'_>, at: DateTime<Utc>, tz: Tz) -> Option<DateTime<Utc>> {
    let today = tz.day(at);
    let after = |date: NaiveDate| resolve(tz, date.and_time(r.time)).filter(|t| *t > at);
    match r.date {
        DateSpec::Today => after(today).or_else(|| after(today + Duration::days(1))),
        DateSpec::Weekday(w) => (0..=7)
            .map(|i| today + Duration::days(i))
            .filter(|d| d.weekday() == w)
            .find_map(after),
        DateSpec::MonthDay(m, d) => (today.year()..=today.year() + 1)
            .filter_map(|y| NaiveDate::from_ymd_opt(y, m, d))
            .find_map(after),
    }
}

fn window_of(line: &str) -> Option<LimitWindow> {
    let l = line.to_ascii_lowercase();
    if ["5-hour", "5 hour", "5h", "five hour", "session"]
        .iter()
        .any(|k| l.contains(k))
    {
        Some(LimitWindow::Session)
    } else if l.contains("week") {
        Some(LimitWindow::Weekly)
    } else {
        None
    }
}

/// A limit message printed by an agent, read as a 100 % snapshot. `zone_name` is the IANA name of
/// `tz`: a reset time naming another zone is left unresolved, never converted by guess.
pub fn limit_from_hit(
    profile: &str,
    line: &str,
    at: DateTime<Utc>,
    tz: Tz,
    zone_name: Option<&str>,
) -> Option<LimitSnapshot> {
    let tool = match profile {
        "claude-code" => Tool::Claude,
        "codex" => Tool::Codex,
        _ => return None,
    };
    let window = window_of(line)?;
    let resets_at = parse_reset(line)
        .filter(|r| r.zone.is_none_or(|z| Some(z) == zone_name || z == "UTC" && matches!(tz, Tz::Fixed(o) if o == FixedOffset::east_opt(0).expect("zero offset"))))
        .and_then(|r| next_reset(&r, at, tz))
        .or_else(|| {
            Some(
                at + match window {
                    LimitWindow::Weekly => Duration::days(7),
                    _ => Duration::hours(5),
                },
            )
        });
    Some(LimitSnapshot {
        tool,
        account: "default".into(),
        window,
        used_pct: 100.0,
        resets_at,
        plan: None,
        observed_at: at,
        source: LimitSource::LimitHit,
        tool_session_id: String::new(),
    })
}
