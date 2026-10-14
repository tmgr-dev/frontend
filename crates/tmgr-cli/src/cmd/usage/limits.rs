use super::fmt::span;
use super::{fail, scan, Usage};
use crate::ctx::Ctx;
use crate::out::{table, Output};
use chrono::{SecondsFormat, Utc};
use serde_json::json;
use tmgr_core::error::Result;
use tmgr_core::text::sanitize_line;
use tmgr_usage::limits::LimitWindow;

fn window_label(w: &LimitWindow) -> String {
    match w {
        LimitWindow::Session => "session".to_string(),
        LimitWindow::Weekly => "weekly".to_string(),
        LimitWindow::Model(m) => format!("weekly {m}"),
    }
}

pub fn run(ctx: &Ctx, no_scan: bool) -> Result<Output> {
    let mut u = Usage::open()?;
    let now = Utc::now();
    if !no_scan {
        scan(ctx, &mut u, now)?;
    }
    let limits = u.store.current_limits(now).map_err(fail)?;
    let rows: Vec<_> = limits
        .iter()
        .map(|l| {
            json!({
                "tool": l.tool.as_str(),
                "account": l.account,
                "window": l.window.as_key(),
                "used_pct": l.used_pct,
                "resets_at": l.resets_at.map(|t| t.to_rfc3339_opts(SecondsFormat::Secs, true)),
                "resets_in_s": l.resets_at.map(|t| (t - now).num_seconds().max(0)),
                "plan": l.plan,
                "source": l.source.as_str(),
                "observed_at": l.observed_at.to_rfc3339_opts(SecondsFormat::Secs, true),
                "age_s": (now - l.observed_at).num_seconds().max(0),
            })
        })
        .collect();
    let human = if limits.is_empty() {
        ctx.style.muted(
            "No limit data yet. Codex limits appear after a scan of its sessions; \
Claude Code limits need the statusline hook: tmgr usage statusline install",
        )
    } else {
        let mut lines = vec![[
            "Tool",
            "Account",
            "Window",
            "Used",
            "Resets in",
            "Source",
            "Age",
        ]
        .map(String::from)
        .to_vec()];
        for l in &limits {
            lines.push(vec![
                l.tool.as_str().to_string(),
                sanitize_line(&l.account),
                sanitize_line(&window_label(&l.window)),
                format!("{:.0}%", l.used_pct),
                l.resets_at
                    .map_or("-".to_string(), |t| span((t - now).num_seconds())),
                l.source.as_str().to_string(),
                format!("{} ago", span((now - l.observed_at).num_seconds())),
            ]);
        }
        table(&lines)
    };
    Ok(Output::new(json!({ "limits": rows }), human))
}
