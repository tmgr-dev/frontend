use super::{Agent, Dialog, DialogBody, DialogOption, OptionAction};

const CLAUDE_FOOTERS: [&str; 2] = ["Esc to cancel · Tab to amend", "Esc to cancel"];
const CODEX_FOOTER: &str = "Press enter to confirm or esc to cancel";
const CODEX_QUESTION: &str = "Would you like to run the following command?";
const WITHHELD: &str = "(value cannot be shown in full — approval withheld; one-time options only)";
const MIN_RULE: usize = 10;

pub fn parse_dialog(agent: Agent, rows: &[String]) -> Option<Dialog> {
    let end = rows.iter().rposition(|r| !r.trim().is_empty())?;
    let footer = rows[end].trim();
    let above = &rows[..end];
    match agent {
        Agent::Claude if CLAUDE_FOOTERS.contains(&footer) => claude(above),
        Agent::Codex if footer == CODEX_FOOTER => codex(above),
        _ => None,
    }
}

fn blank(r: &str) -> bool {
    r.trim().is_empty()
}

fn rule(r: &str, ch: char) -> Option<usize> {
    let t = r.trim();
    let n = t.chars().count();
    (n >= MIN_RULE && t.chars().all(|c| c == ch)).then_some(n)
}

fn ruleish(r: &str) -> bool {
    let t = r.trim();
    !t.is_empty() && t.chars().all(|c| matches!(c, '╌' | '─' | '═' | '━'))
}

fn numbered(row: &str, marker: char) -> Option<(u32, String)> {
    let t = row.trim_start();
    let t = t.strip_prefix(marker).map_or(t, str::trim_start);
    let digits: String = t.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() || digits.len() > 2 {
        return None;
    }
    let rest = t[digits.len()..].strip_prefix('.')?;
    if !rest.starts_with(' ') {
        return None;
    }
    Some((digits.parse().ok()?, rest.trim().to_string()))
}

fn indent(r: &str) -> usize {
    r.chars().take_while(|c| *c == ' ').count()
}

struct Raw {
    n: u32,
    text: String,
}

fn collect_options(rows: &[String], marker: char) -> Option<(Vec<Raw>, usize)> {
    let mut i = rows.len();
    while i > 0 && blank(&rows[i - 1]) {
        i -= 1;
    }
    let mut out: Vec<Raw> = Vec::new();
    let mut pending: Vec<&str> = Vec::new();
    loop {
        i = i.checked_sub(1)?;
        let row = &rows[i];
        if blank(row) {
            return None;
        }
        if let Some((n, text)) = numbered(row, marker) {
            let mut label = text;
            for c in pending.iter().rev() {
                label.push(' ');
                label.push_str(c.trim());
            }
            pending.clear();
            out.push(Raw { n, text: label });
            if n == 1 {
                break;
            }
        } else if indent(row) >= 4 {
            pending.push(row);
        } else {
            return None;
        }
    }
    out.reverse();
    let ok = out.len() >= 2 && out.iter().enumerate().all(|(k, o)| o.n as usize == k + 1);
    ok.then_some((out, i))
}

fn classify(agent: Agent, key: &str, label: &str) -> OptionAction {
    match agent {
        Agent::Claude => {
            if label == "Yes" {
                OptionAction::Yes
            } else if label.starts_with("No") {
                OptionAction::No
            } else if label.starts_with("Yes, and don't ask again for")
                && ![
                    "access",
                    "director",
                    "mode",
                    "edits",
                    "shift+tab",
                    "session",
                ]
                .iter()
                .any(|w| label.contains(w))
            {
                OptionAction::Always
            } else {
                OptionAction::Other
            }
        }
        Agent::Codex => {
            if label.starts_with("Yes, proceed") && key == "y" {
                OptionAction::Yes
            } else if label.starts_with("No") {
                OptionAction::No
            } else if label.starts_with("Yes, and don't ask again for commands that start with") {
                OptionAction::Always
            } else {
                OptionAction::Other
            }
        }
    }
}

fn finish_options(agent: Agent, raw: Vec<Raw>) -> Option<Vec<DialogOption>> {
    let mut out: Vec<DialogOption> = Vec::new();
    for o in raw {
        let (key, label) = match agent {
            Agent::Claude => (o.n.to_string(), o.text),
            Agent::Codex => {
                let t = o.text.strip_suffix(')')?;
                let at = t.rfind(" (")?;
                let key = &t[at + 2..];
                let single = key.chars().count() == 1
                    && !key.chars().any(|c| c.is_whitespace() || c.is_control());
                if !(single || key == "esc") {
                    return None;
                }
                (key.to_string(), t[..at].trim().to_string())
            }
        };
        if label.is_empty() || out.iter().any(|p| p.key == key) {
            return None;
        }
        let action = classify(agent, &key, &label);
        out.push(DialogOption { key, label, action });
    }
    Some(out)
}

fn claude(rows: &[String]) -> Option<Dialog> {
    let (raw, first) = collect_options(rows, '❯')?;
    let options = finish_options(Agent::Claude, raw)?;
    let question = rows.get(first.checked_sub(1)?)?.trim();
    if !(question.starts_with("Do you want to ") && question.ends_with('?')) {
        return None;
    }
    let lo = first.checked_sub(2)?;
    let width = rule(&rows[lo], '╌')?;
    let mut i = lo;
    let body_end = i;
    loop {
        i = i.checked_sub(1)?;
        let r = &rows[i];
        if rule(r, '╌') == Some(width) {
            break;
        }
        if ruleish(r) {
            return None;
        }
    }
    let up = i;
    let body: Vec<&str> = rows[up + 1..body_end].iter().map(String::as_str).collect();
    let mut header: Vec<&str> = Vec::new();
    loop {
        i = i.checked_sub(1)?;
        let r = &rows[i];
        if rule(r, '─') == Some(width) {
            break;
        }
        if blank(r) || ruleish(r) || header.len() > 20 {
            return None;
        }
        header.push(r.trim());
    }
    header.reverse();
    let kind = *header.first()?;
    let rest = &header[1..];

    let body_rows: Vec<String> = body.iter().map(|r| r.trim().to_string()).collect();
    let nonblank: Vec<&String> = body_rows.iter().filter(|r| !r.is_empty()).collect();
    let parsed = if kind == "Bash command" && question == "Do you want to proceed?" {
        if nonblank.len() == 1 && nonblank[0] == WITHHELD {
            DialogBody::Withheld
        } else if nonblank.is_empty() {
            return None;
        } else {
            DialogBody::Command {
                rows: body_rows.clone(),
                reason: (!rest.is_empty()).then(|| rest.join("\n")),
            }
        }
    } else if kind == "Create file"
        && rest.len() == 1
        && question
            == format!(
                "Do you want to create {}?",
                rest[0].rsplit('/').next().unwrap_or("")
            )
    {
        DialogBody::WriteFile {
            shown_path: rest[0].to_string(),
            preview: body.iter().map(|r| r.trim_end().to_string()).collect(),
        }
    } else {
        DialogBody::Other {
            title: kind.to_string(),
        }
    };

    let mut text: Vec<String> = header.iter().map(|s| s.to_string()).collect();
    text.extend(body_rows);
    text.push(question.to_string());
    Some(Dialog {
        agent: Agent::Claude,
        body: parsed,
        options,
        text: text.join("\n"),
    })
}

fn skip_blank_up(rows: &[String], mut i: usize) -> Option<usize> {
    let start = i;
    while i > 0 && blank(&rows[i - 1]) {
        i -= 1;
    }
    (i < start && i > 0).then_some(i)
}

fn block_up(rows: &[String], end: usize) -> Option<(usize, Vec<&str>)> {
    let mut i = end;
    while i > 0 && !blank(&rows[i - 1]) {
        i -= 1;
    }
    (i < end).then(|| (i, rows[i..end].iter().map(|r| r.trim()).collect()))
}

fn codex(rows: &[String]) -> Option<Dialog> {
    let (raw, first) = collect_options(rows, '›')?;
    let options = finish_options(Agent::Codex, raw)?;
    let i = skip_blank_up(rows, first)?;
    let (i, cmd_rows) = block_up(rows, i)?;
    let first_cmd = cmd_rows[0].strip_prefix("$ ")?;
    let mut cmd: Vec<String> = vec![first_cmd.trim().to_string()];
    cmd.extend(cmd_rows[1..].iter().map(|r| r.to_string()));

    let i = skip_blank_up(rows, i)?;
    let (mut i, mut blk) = block_up(rows, i)?;
    let mut reason = None;
    if blk[0].starts_with("Reason:") {
        let mut parts = vec![blk[0]["Reason:".len()..].trim().to_string()];
        parts.extend(blk[1..].iter().map(|r| r.to_string()));
        reason = Some(parts.join("\n"));
        let j = skip_blank_up(rows, i)?;
        (i, blk) = block_up(rows, j)?;
    }
    if blk.len() != 1 || !blk[0].starts_with("Environment:") {
        return None;
    }
    let env = blk[0].to_string();
    let j = skip_blank_up(rows, i)?;
    let (_, q) = block_up(rows, j)?;
    if q.len() != 1 || q[0] != CODEX_QUESTION {
        return None;
    }

    let mut text = vec![CODEX_QUESTION.to_string(), env];
    if let Some(r) = &reason {
        text.push(format!("Reason: {r}"));
    }
    text.extend(cmd.iter().cloned());
    Some(Dialog {
        agent: Agent::Codex,
        body: DialogBody::Command { rows: cmd, reason },
        options,
        text: text.join("\n"),
    })
}
