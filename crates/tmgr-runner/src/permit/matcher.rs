use super::{Agent, Dialog, DialogBody, HookEvent, OptionAction, Verdict};

pub fn verdict(d: &Dialog, h: Option<&HookEvent>) -> Verdict {
    let Some(h) = h else {
        return Verdict::ViewOnly("no hook event for this dialog");
    };
    if h.truncated {
        return Verdict::ViewOnly("hook text was truncated");
    }
    if h.agent != d.agent {
        return Verdict::ViewOnly("hook is from another agent");
    }
    let supported = matches!(
        (d.agent, h.tool.as_str()),
        (Agent::Claude, "Bash") | (Agent::Codex, "Bash") | (Agent::Claude, "Write")
    );
    if !supported {
        return Verdict::ViewOnly("tool is not supported for approval");
    }
    let count = |a| d.options.iter().filter(|o| o.action == a).count();
    if count(OptionAction::Yes) != 1 || count(OptionAction::No) != 1 {
        return Verdict::ViewOnly("dialog has no single yes and no option");
    }
    let ok = match (&d.body, h.tool.as_str()) {
        (DialogBody::Command { rows, reason }, "Bash") => {
            command_matches(rows, reason.as_deref(), h)
        }
        (
            DialogBody::WriteFile {
                shown_path,
                preview,
            },
            "Write",
        ) => write_matches(shown_path, preview, h),
        (DialogBody::Withheld, _) => {
            return Verdict::ViewOnly("command is too long to show in full")
        }
        (DialogBody::Other { .. }, _) => return Verdict::ViewOnly("dialog type is not supported"),
        _ => return Verdict::ViewOnly("dialog does not fit the tool"),
    };
    match ok {
        Ok(()) => Verdict::Approvable,
        Err(why) => Verdict::ViewOnly(why),
    }
}

fn hidden(c: char) -> bool {
    (c.is_control() && c != '\n')
        || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
}

fn clean(s: &str) -> bool {
    !s.chars().any(hidden)
}

fn match_rows(text: &str, rows: &[String]) -> bool {
    let t: Vec<char> = text.chars().collect();
    let mut p = 0;
    for row in rows {
        let row = row.trim();
        if row.is_empty() {
            continue;
        }
        while p < t.len() && t[p].is_whitespace() {
            p += 1;
        }
        for c in row.chars() {
            if p < t.len() && t[p] == c {
                p += 1;
            } else {
                return false;
            }
        }
    }
    while p < t.len() && t[p].is_whitespace() {
        p += 1;
    }
    p == t.len()
}

fn command_matches(
    rows: &[String],
    reason: Option<&str>,
    h: &HookEvent,
) -> Result<(), &'static str> {
    let cmd = h.command.as_deref().ok_or("hook has no command")?;
    if cmd.trim().is_empty() || !clean(cmd) {
        return Err("command has hidden characters");
    }
    if !match_rows(cmd, rows) {
        return Err("shown command differs from the hook command");
    }
    let desc = h.description.as_deref().filter(|d| !d.trim().is_empty());
    match (reason, desc) {
        (None, None) => Ok(()),
        (Some(r), Some(d)) if clean(d) => {
            let rr: Vec<String> = r.lines().map(String::from).collect();
            if match_rows(d, &rr) {
                Ok(())
            } else {
                Err("shown description differs from the hook")
            }
        }
        _ => Err("shown description differs from the hook"),
    }
}

fn write_matches(shown: &str, preview: &[String], h: &HookEvent) -> Result<(), &'static str> {
    let path = h.file_path.as_deref().ok_or("hook has no file path")?;
    let content = h.content.as_deref().ok_or("hook has no content")?;
    if !clean(path) || !clean(shown) || content.chars().any(|c| hidden(c) && c != '\n') {
        return Err("file text has hidden characters");
    }
    if !path.starts_with('/') {
        return Err("hook path is not absolute");
    }
    let cwd = h.cwd.as_deref();
    let a = normalize_path(cwd, shown).ok_or("shown path cannot be resolved")?;
    let b = normalize_path(cwd, path).ok_or("hook path cannot be resolved")?;
    if a != b {
        return Err("shown path differs from the hook path");
    }
    let lines: Vec<&str> = content
        .strip_suffix('\n')
        .unwrap_or(content)
        .split('\n')
        .collect();
    if content.is_empty() || preview.len() != lines.len() {
        return Err("file preview does not cover the whole content");
    }
    for (idx, (row, line)) in preview.iter().zip(&lines).enumerate() {
        let s = row.trim_start();
        let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
        let rest = &s[digits.len()..];
        if digits.parse::<usize>().ok() != Some(idx + 1) {
            return Err("file preview lines are not numbered in order");
        }
        let text = match rest.strip_prefix(' ') {
            Some(t) => t,
            None if rest.trim().is_empty() => "",
            None => return Err("file preview row is malformed"),
        };
        if text.trim_end() != line.trim_end() {
            return Err("file preview differs from the content");
        }
    }
    Ok(())
}

pub(super) fn normalize_path(base: Option<&str>, p: &str) -> Option<String> {
    let mut stack: Vec<&str> = Vec::new();
    let full;
    let src = if p.starts_with('/') {
        p
    } else {
        let b = base.filter(|b| b.starts_with('/'))?;
        full = format!("{b}/{p}");
        &full
    };
    for part in src.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                stack.pop()?;
            }
            c => stack.push(c),
        }
    }
    Some(format!("/{}", stack.join("/")))
}
