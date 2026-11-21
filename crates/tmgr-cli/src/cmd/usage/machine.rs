use super::sync::{wait_override, AutoLock};
use super::task::Corpus;
use super::{fail, Usage};
use crate::cli::VERSION;
use crate::cli_usage::{MachineArgs, MachineCommand, MachineSyncArgs};
use crate::ctx::Ctx;
use crate::out::Output;
use chrono::{DateTime, Duration as Span, SecondsFormat, TimeZone, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tmgr_core::api::{scopes_hint, Client};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::local;
use tmgr_usage::machine::{self, Identity, Input};
use tmgr_usage::Tz;

const SCOPE: &str = "agent-usage:write";
const LOCK_NAME: &str = "machine-sync.lock";
const HIT_MAX_BYTES: u64 = 8 * 1024;
const HIT_LINE_MAX: usize = 200;
const DISCLOSURE: &str = "Sent: this machine's name (the label above) and numbers only: tokens, cost, sessions and events per day, agent, model and task, plus rate limit percentages and reset times. Never prompts, replies, code, file paths or command lines. Rename it with `tmgr usage machine label NAME`; `tmgr usage machine off` stops sending.";

pub fn run(ctx: &Ctx, args: &MachineArgs) -> Result<Output> {
    match &args.command {
        None | Some(MachineCommand::Status) => status(),
        Some(MachineCommand::Label { name }) => label(name),
        Some(MachineCommand::On) => switch(true),
        Some(MachineCommand::Off) => switch(false),
        Some(MachineCommand::Sync(sync_args)) => sync(ctx, sync_args),
    }
}

fn stamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn status() -> Result<Output> {
    let u = Usage::open()?;
    let id = u.store.machine_id().map_err(fail)?;
    let label = u.store.machine_label().map_err(fail)?;
    let on = u.store.machine_sync_on().map_err(fail)?;
    let last = u.store.machine_last_sync().map_err(fail)?;
    let human = format!(
        "Machine: {label}\nId: {id}\nSending: {}\nLast sent: {}",
        if on {
            "on (`tmgr usage machine off` stops it)"
        } else {
            "off (`tmgr usage machine on` turns it on)"
        },
        last.map_or("never".to_string(), stamp)
    );
    Ok(Output::new(
        json!({
            "action": "status",
            "machine_id": id,
            "label": label,
            "sync": on,
            "last_sync": last.map(stamp),
        }),
        human,
    ))
}

fn label(name: &str) -> Result<Output> {
    let u = Usage::open()?;
    u.store.set_machine_label(name).map_err(fail)?;
    let label = u.store.machine_label().map_err(fail)?;
    Ok(Output::new(
        json!({"action": "label", "label": label}),
        format!("This machine is now called {label}. The new name is sent with the next sync."),
    ))
}

fn switch(on: bool) -> Result<Output> {
    let u = Usage::open()?;
    u.store.set_machine_sync(on).map_err(fail)?;
    let human = if on {
        "Sending this machine's usage is on. The next sync also restores the machine if it was removed on the server."
    } else {
        "Sending this machine's usage is off. Remove the machine on the AI usage page to delete what was sent."
    };
    Ok(Output::new(
        json!({"action": if on { "on" } else { "off" }, "sync": on}),
        human,
    ))
}

fn skipped(reason: &str) -> Output {
    Output::new(
        json!({"action": "sync", "skipped": true, "reason": reason}),
        "",
    )
}

struct Hit {
    profile: String,
    line: String,
    at: DateTime<Utc>,
}

fn read_hit(path: &Path, now: DateTime<Utc>) -> Option<Hit> {
    use std::io::Read;
    let mut buf = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(HIT_MAX_BYTES)
        .read_to_end(&mut buf)
        .ok()?;
    let v: Value = serde_json::from_slice(&buf).ok()?;
    let at = Utc
        .timestamp_millis_opt(v.get("at_ms")?.as_i64()?)
        .single()?
        .min(now);
    Some(Hit {
        profile: v.get("profile")?.as_str()?.to_string(),
        line: v
            .get("line")?
            .as_str()?
            .chars()
            .take(HIT_LINE_MAX)
            .collect(),
        at,
    })
}

struct HitFile<'a>(Option<&'a Path>);

impl Drop for HitFile<'_> {
    fn drop(&mut self) {
        if let Some(p) = self.0 {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn zone_name() -> Option<String> {
    iana_time_zone::get_timezone()
        .ok()
        .filter(|z| z.len() <= 64)
}

fn record_hit(u: &Usage, hit: &Hit) {
    let Some(mut snap) = machine::limit_from_hit(
        &hit.profile,
        &hit.line,
        hit.at,
        Tz::Local,
        zone_name().as_deref(),
    ) else {
        return;
    };
    let default_label = snap.tool.as_str();
    if let Ok(accounts) = u.accounts() {
        if accounts
            .iter()
            .any(|a| a.tool == snap.tool && a.label == default_label)
        {
            snap.account = default_label.to_string();
        }
    }
    let _ = u.store.record_limit(&snap);
}

fn task_ids(c: &Corpus) -> Vec<Option<u64>> {
    let workspace_of: HashMap<&str, &str> = c
        .set
        .windows
        .iter()
        .map(|w| (w.run_ref.as_str(), w.workspace.as_str()))
        .collect();
    let mut seen: Vec<Option<Option<u64>>> = vec![None; c.events.len()];
    for l in &c.report.links {
        let id = match (
            c.set.meta.get(&l.run_ref),
            workspace_of.get(l.run_ref.as_str()),
        ) {
            (Some(m), Some(ws)) if local::local_code(ws).is_none() => m.task_id,
            _ => None,
        };
        if let Some(slot) = seen.get_mut(l.event_index) {
            *slot = Some(match *slot {
                None => id,
                Some(prev) if prev == id => id,
                Some(_) => None,
            });
        }
    }
    seen.into_iter().map(Option::flatten).collect()
}

enum Reply {
    Sent { days: u64, limits: u64 },
    Removed,
}

fn code_of(body: &Value) -> String {
    body.get("error")
        .and_then(Value::as_str)
        .filter(|c| c.len() <= 40 && c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_'))
        .unwrap_or("unknown")
        .to_string()
}

fn reply_of(status: u16, body: &Value) -> Result<Reply> {
    match status {
        200..=299 => {
            let data = body.get("data").unwrap_or(body);
            Ok(Reply::Sent {
                days: data.get("days").and_then(Value::as_u64).unwrap_or(0),
                limits: data.get("limits").and_then(Value::as_u64).unwrap_or(0),
            })
        }
        410 => Ok(Reply::Removed),
        401 => Err(Error::not_logged_in()),
        403 => Err(Error::new(
            Kind::Forbidden,
            format!("this login cannot send machine usage (needs scope {SCOPE})"),
        )
        .hint(scopes_hint(SCOPE))),
        s => Err(Error::internal(format!(
            "the server answered HTTP {s} ({})",
            code_of(body)
        ))),
    }
}

fn put(client: &Client, path: &str, body: &Value, allowed: &dyn Fn() -> bool) -> Result<Reply> {
    for attempt in 0..2 {
        let Some((status, reply)) = client.call_guarded("PUT", path, Some(body), allowed)? else {
            return Err(Error::not_logged_in());
        };
        if status != 429 || attempt == 1 {
            return reply_of(status, &reply);
        }
        let pause = wait_override().unwrap_or_else(|| {
            Duration::from_secs(
                reply
                    .get("retry_after_s")
                    .and_then(Value::as_u64)
                    .unwrap_or(60)
                    .clamp(1, 120),
            )
        });
        std::thread::sleep(pause);
    }
    unreachable!("the loop returns on its last attempt")
}

fn sync(ctx: &Ctx, args: &MachineSyncArgs) -> Result<Output> {
    let _hit_file = HitFile(args.limit_hit_file.as_deref());
    let mut u = Usage::open()?;
    let now = Utc::now();
    let hit = args
        .limit_hit_file
        .as_deref()
        .and_then(|p| read_hit(p, now));
    let last = u.store.machine_last_sync().map_err(fail)?;
    if let Some(hit) = &hit {
        record_hit(&u, hit);
    }
    if !u.store.machine_sync_on().map_err(fail)? {
        return if args.auto {
            Ok(skipped("off"))
        } else {
            Err(Error::conflict("sending this machine's usage is off")
                .hint("tmgr usage machine on"))
        };
    }
    if args.auto && !machine::auto_due(last, now, hit.is_some()) {
        return Ok(skipped("throttled"));
    }
    let client = match ctx.cloud_client() {
        Ok(c) => c,
        Err(e) if args.auto && e.kind == Kind::Auth => return Ok(skipped("not_logged_in")),
        Err(e) => return Err(e),
    };
    let Some(_lock) = AutoLock::take_named(&u.dir, LOCK_NAME) else {
        return if args.auto {
            Ok(skipped("running"))
        } else {
            Err(Error::conflict("another machine sync is running"))
        };
    };
    let today = Tz::Local.day(now);
    let covered = u.store.machine_covered_day().map_err(fail)?;
    let days = machine::days_to_send(args.auto, covered, today, args.days);
    let c = Corpus::load(ctx, &mut u, days + 1, false, !args.auto)?;
    let tasks = task_ids(&c);
    let events: Vec<_> = c
        .events
        .iter()
        .cloned()
        .zip(c.tokens.iter().copied())
        .collect();
    let identity = Identity {
        label: u.store.machine_label().map_err(fail)?,
        platform: machine::platform().to_string(),
        cli_version: VERSION.to_string(),
        timezone: zone_name(),
        reenable: u.store.machine_reenable().map_err(fail)?,
    };
    let report = machine::build_report(&Input {
        identity: &identity,
        now,
        tz: Tz::Local,
        from_day: today - Span::days(days - 1),
        to_day: today,
        complete_from: u.store.history_floor().map_err(fail)?,
        events: &events,
        task_ids: &tasks,
        limits: &c.limits,
    })
    .map_err(fail)?;
    let machine_id = u.store.machine_id().map_err(fail)?;
    let path = format!("/api/ai-usage/machines/{machine_id}");
    let (mut sent_days, mut sent_limits) = (0, 0);
    let still_on = || u.store.machine_sync_on().unwrap_or(false);
    for body in report.bodies().map_err(fail)? {
        match put(client, &path, &body, &still_on)? {
            Reply::Sent { days, limits } => {
                sent_days += days;
                sent_limits += limits;
            }
            Reply::Removed => {
                u.store.set_machine_sync(false).map_err(fail)?;
                let _ = u.store.clear_machine_reenable();
                return Ok(Output::new(
                    json!({"action": "sync", "skipped": false, "removed": true, "machine_id": machine_id}),
                    "This machine was removed on the server, so sending is off now. `tmgr usage machine on` turns it on and restores the machine.",
                ));
            }
        }
    }
    let _ = u.store.set_machine_last_sync(Utc::now());
    let read = u
        .store
        .log_consent()
        .is_ok_and(|c| c.allows_any(!args.auto));
    if read && args.days.is_none() {
        let _ = u.store.set_machine_covered_day(today);
    }
    let _ = u.store.clear_machine_reenable();
    let first_note = !args.auto
        && u.store
            .take_notice("machine_disclosure", 1)
            .unwrap_or(false);
    let mut human = format!(
        "Sent {} days and {} limits of {}.",
        report.days.len(),
        report.limits.len(),
        identity.label
    );
    if first_note {
        human = format!("{human}\n{DISCLOSURE}");
    }
    Ok(Output::new(
        json!({
            "action": "sync",
            "skipped": false,
            "removed": false,
            "machine_id": machine_id,
            "label": identity.label,
            "days": report.days.len(),
            "limits": report.limits.len(),
            "server_days": sent_days,
            "server_limits": sent_limits,
        }),
        if args.auto { String::new() } else { human },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_removed_machine_is_not_an_error() {
        assert!(matches!(
            reply_of(410, &json!({"error": "machine_removed"})).unwrap(),
            Reply::Removed
        ));
    }

    #[test]
    fn success_reads_the_counts_and_failures_keep_only_a_plain_code() {
        let Ok(Reply::Sent { days, limits }) = reply_of(
            200,
            &json!({"machine_id": "x", "days": 2, "limits": 3, "alerts": 0}),
        ) else {
            panic!("expected a sent reply");
        };
        assert_eq!((days, limits), (2, 3));
        let e = reply_of(422, &json!({"error": "bad body <script>"}))
            .err()
            .unwrap();
        assert!(e.message.contains("HTTP 422 (unknown)"), "{}", e.message);
        let e = reply_of(403, &json!({})).err().unwrap();
        assert_eq!(e.kind, Kind::Forbidden);
        assert!(e.hint.unwrap().contains(SCOPE));
    }

    #[test]
    fn a_limit_hit_file_is_read_capped_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("hit.json");
        let long = "x".repeat(500);
        std::fs::write(
            &p,
            json!({"profile": "claude-code", "line": long, "at_ms": 1_700_000_000_000i64})
                .to_string(),
        )
        .unwrap();
        let now = Utc.timestamp_millis_opt(1_800_000_000_000).unwrap();
        let hit = read_hit(&p, now).unwrap();
        assert_eq!(hit.line.chars().count(), HIT_LINE_MAX);
        {
            let _guard = HitFile(Some(&p));
        }
        assert!(!p.exists());
        std::fs::write(&p, "not json").unwrap();
        assert!(read_hit(&p, now).is_none());
    }
}
