use super::ops::with_session;
use super::{bin, rerr};
use crate::ctx::Ctx;
use crate::out::Output;
use chrono::{DateTime, Local};
use clap::{Args, Subcommand};
use serde_json::json;
use std::io::{BufRead, IsTerminal, Write};
use std::time::Duration;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::text::sanitize_line;
use tmgr_ipc::api::{CheckpointDiff, CheckpointItem, CheckpointKind, DiffAgainst};
use tmgr_runner::checkpoint::RESTORE_WARNING;
use tmgr_runner::client::Client;

const LONG_READ: Duration = Duration::from_secs(600);

#[derive(Args)]
pub struct CheckpointsArgs {
    #[command(subcommand)]
    pub command: CheckpointsCommand,
}

#[derive(Subcommand)]
pub enum CheckpointsCommand {
    /// List the saved states of a session's working folder
    List {
        #[arg(value_name = "SESSION")]
        session: String,
    },
    /// Show what changed in a saved state, or what a restore would change
    Diff {
        #[arg(value_name = "SESSION")]
        session: String,
        #[arg(value_name = "N")]
        n: u64,
        /// Compare with the folder as it is now
        #[arg(long)]
        current: bool,
    },
    /// Put the working folder back to a saved state (the current state is saved first)
    Restore {
        #[arg(value_name = "SESSION")]
        session: String,
        #[arg(value_name = "N")]
        n: u64,
    },
}

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn item_label(i: &CheckpointItem) -> String {
    match i.kind {
        CheckpointKind::Start => "session start".into(),
        CheckpointKind::BeforeRestore => "before restore".into(),
        CheckpointKind::Restored => match i.restored_from {
            Some(n) => format!("restored #{n}"),
            None => "restored".into(),
        },
        CheckpointKind::Turn => format!(
            "after turn {} \u{b7} {}",
            i.turn,
            plural(i.files, "file", "files")
        ),
    }
}

fn item_line(i: &CheckpointItem) -> String {
    let at = DateTime::from_timestamp_millis(i.at_ms as i64)
        .map(|t| t.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_default();
    let mut line = format!("#{}  {}", i.n, item_label(i));
    if i.kind != CheckpointKind::Turn && i.files > 0 {
        line.push_str(&format!(" \u{b7} {}", plural(i.files, "file", "files")));
    }
    if i.skipped > 0 {
        line.push_str(&format!(" \u{b7} {} skipped", i.skipped));
    }
    if !at.is_empty() {
        line.push_str(&format!(" \u{b7} {at}"));
    }
    line
}

fn restore_summary(d: &CheckpointDiff) -> String {
    let all = d.added + d.modified + d.deleted;
    let files = if all == 1 {
        "1 file changes".to_string()
    } else {
        format!("{all} files change")
    };
    let gone = if d.added == 1 {
        "1 is removed".to_string()
    } else {
        format!("{} are removed", d.added)
    };
    format!("{files}, {gone}. Current state is saved first.")
}

fn how_to_enable() -> String {
    format!(
        "Turn them on for new sessions with `{0} config set attach.checkpoints true`, or start one with `{0} run --checkpoints`.",
        bin()
    )
}

fn off_message() -> String {
    format!("Checkpoints are off for this session. {}", how_to_enable())
}

fn enabled(info: &tmgr_ipc::api::SessionInfo) -> Result<()> {
    if info.checkpoints.is_some() {
        Ok(())
    } else {
        Err(Error::usage("Checkpoints are off for this session.").hint(how_to_enable()))
    }
}

fn open(key: &str) -> Result<(Client, tmgr_ipc::api::SessionInfo)> {
    let (c, s) = with_session(key)?;
    c.set_timeouts(LONG_READ)
        .map_err(|e| Error::internal(format!("cannot set the runner timeout: {e}")))?;
    Ok((c, s))
}

fn restore_prompt(n: u64, preview: &CheckpointDiff) -> String {
    format!(
        "{RESTORE_WARNING}\nRestore to #{n}? {}",
        restore_summary(preview)
    )
}

fn confirm(prompt: &str) -> Result<bool> {
    eprint!("{prompt} [y/N] ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|_| Error::new(Kind::Interrupted, "could not read the answer"))?;
    Ok(matches!(line.trim(), "y" | "Y" | "yes" | "Yes"))
}

pub fn run(ctx: &Ctx, args: &CheckpointsArgs) -> Result<Output> {
    match &args.command {
        CheckpointsCommand::List { session } => list(session),
        CheckpointsCommand::Diff {
            session,
            n,
            current,
        } => diff(session, *n, *current),
        CheckpointsCommand::Restore { session, n } => restore(ctx, session, *n),
    }
}

fn list(key: &str) -> Result<Output> {
    let (mut c, s) = open(key)?;
    if s.checkpoints.is_none() {
        return Ok(Output::new(
            json!({"id": s.id, "enabled": false, "status": null, "items": []}),
            off_message(),
        ));
    }
    let l = c.checkpoints_list(&s.id).map_err(rerr)?;
    let mut lines = Vec::new();
    if !l.status.available {
        if let Some(note) = &l.status.note {
            lines.push(sanitize_line(note));
        }
    }
    lines.extend(l.items.iter().map(item_line));
    if lines.is_empty() {
        lines.push("No checkpoints yet.".into());
    }
    Ok(Output::new(
        json!({"id": s.id, "enabled": true, "status": l.status, "items": l.items}),
        lines.join("\n"),
    ))
}

fn diff(key: &str, n: u64, current: bool) -> Result<Output> {
    let (mut c, s) = open(key)?;
    enabled(&s)?;
    let against = if current {
        DiffAgainst::Current
    } else {
        DiffAgainst::Previous
    };
    let d = c.checkpoints_diff(&s.id, n, against).map_err(rerr)?;
    let mut human = d.text.clone();
    if d.truncated {
        human.push_str("\n(output cut short)");
    }
    Ok(Output::new(
        json!({"id": s.id, "diff": d}),
        human.trim_end(),
    ))
}

fn restore(ctx: &Ctx, key: &str, n: u64) -> Result<Output> {
    crate::cmd::queue::not_in_agent_session()?;
    let terminal = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    if !ctx.yes && !terminal {
        return Err(Error::usage("restore needs a terminal to ask, or --yes")
            .hint(format!("{} checkpoints restore {key} {n} --yes", bin())));
    }
    let (mut c, s) = open(key)?;
    enabled(&s)?;
    if !ctx.yes {
        let preview = c
            .checkpoints_diff(&s.id, n, DiffAgainst::Current)
            .map_err(rerr)?;
        let q = restore_prompt(n, &preview);
        if !confirm(&q)? {
            return Err(Error::new(Kind::Interrupted, "restore cancelled"));
        }
    }
    let r = c.checkpoints_restore(&s.id, n).map_err(rerr)?;
    let mut human = format!(
        "Restored #{}: {} written, {} deleted. The state before is #{}.",
        r.n, r.written, r.deleted, r.before_n
    );
    if !r.left_as_is.is_empty() {
        human.push_str(&format!(
            "\n{} left as is because they changed or could not be written safely:",
            plural(r.left_as_is.len() as u32, "path was", "paths were")
        ));
        for p in &r.left_as_is {
            human.push_str(&format!("\n  {}", sanitize_line(p)));
        }
    }
    human.push_str(&format!(
        "\nTo undo: {} checkpoints restore {} {}",
        bin(),
        s.id,
        r.before_n
    ));
    Ok(Output::new(json!({"id": s.id, "restored": r}), human))
}

pub fn enable_new(client: &mut Client, id: &str, flag: Option<bool>) -> Result<Option<String>> {
    let attach = tmgr_core::config::config_dir()
        .ok()
        .and_then(|d| tmgr_core::config::Config::load(&d).ok())
        .map(|c| c.attach)
        .unwrap_or_default();
    if !flag.or(attach.checkpoints).unwrap_or(false) {
        return Ok(None);
    }
    match client.checkpoints_enable(id, attach.checkpoints_keep) {
        Ok(list) => Ok(list
            .status
            .note
            .filter(|n| n == tmgr_runner::checkpoint::PAUSED_NOTE)),
        Err(e) if flag == Some(true) => Err(Error::usage(format!(
            "the session started, but checkpoints could not be turned on: {e}"
        ))
        .hint("restart the runner so it matches this tmgr version")),
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_restore_prompt_warns_about_writes_during_restore() {
        let d = CheckpointDiff {
            n: 4,
            text: String::new(),
            truncated: false,
            added: 1,
            modified: 0,
            deleted: 0,
        };
        let q = restore_prompt(4, &d);
        assert!(q.contains("Writes to this folder during restore can be lost"));
        assert!(q.contains("undo will not bring them back"));
        assert!(q.ends_with(&format!("Restore to #4? {}", restore_summary(&d))));
    }

    fn item(kind: CheckpointKind, turn: u64, files: u32) -> CheckpointItem {
        CheckpointItem {
            n: 5,
            kind,
            turn,
            files,
            skipped: 0,
            at_ms: 0,
            restored_from: None,
        }
    }

    #[test]
    fn labels_follow_the_kind() {
        assert_eq!(
            item_label(&item(CheckpointKind::Turn, 5, 3)),
            "after turn 5 \u{b7} 3 files"
        );
        assert_eq!(
            item_label(&item(CheckpointKind::Turn, 1, 1)),
            "after turn 1 \u{b7} 1 file"
        );
        assert_eq!(
            item_label(&item(CheckpointKind::Start, 0, 0)),
            "session start"
        );
        assert_eq!(
            item_label(&item(CheckpointKind::BeforeRestore, 0, 0)),
            "before restore"
        );
        let mut r = item(CheckpointKind::Restored, 0, 0);
        r.restored_from = Some(4);
        assert_eq!(item_label(&r), "restored #4");
    }

    #[test]
    fn a_line_starts_with_the_number_and_marks_skipped_files() {
        let mut i = item(CheckpointKind::Turn, 5, 3);
        i.skipped = 2;
        let line = item_line(&i);
        assert!(
            line.starts_with("#5  after turn 5 \u{b7} 3 files \u{b7} 2 skipped"),
            "{line}"
        );
    }

    #[test]
    fn the_restore_summary_counts_removed_files_inside_the_changed_ones() {
        let d = CheckpointDiff {
            n: 2,
            text: String::new(),
            truncated: false,
            added: 1,
            modified: 1,
            deleted: 1,
        };
        assert_eq!(
            restore_summary(&d),
            "3 files change, 1 is removed. Current state is saved first."
        );
    }
}
