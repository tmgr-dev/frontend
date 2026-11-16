use crate::mask::mask;
use crate::model::{Item, Session, ToolCall, Turn};
use regex::RegexBuilder;
use std::collections::HashSet;
use std::fmt::Write;

const USER_CAP: usize = 2000;
const TEXT_CAP: usize = 2000;
const OUTPUT_CAP: usize = 1500;
const COMMAND_CAP: usize = 400;
const LINE_CAP: usize = 100;
const MATCH_CAP: usize = 40;
const TURNS_CAP: usize = 300;
const FILES_CAP: usize = 200;
const ERRORS_CAP: usize = 20;

pub fn cut(text: &str, cap: usize, full: bool) -> String {
    if full {
        return text.to_string();
    }
    let total = text.chars().count();
    if total <= cap {
        return text.to_string();
    }
    let head_len = cap * 2 / 3;
    let tail_len = cap / 3;
    let head: String = text.chars().take(head_len).collect();
    let tail: String = text.chars().skip(total - tail_len).collect();
    format!(
        "{head}\n… [{} chars cut, use --full] …\n{tail}",
        total - head_len - tail_len
    )
}

pub fn safe_cut(text: &str, cap: usize, full: bool) -> String {
    cut(&mask(text), cap, full)
}

fn first_line(text: &str, cap: usize) -> String {
    let masked = mask(text);
    let line = masked
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let short: String = line.chars().take(cap).collect();
    if line.chars().count() > cap {
        format!("{short}…")
    } else {
        short
    }
}

pub fn header(s: &Session) -> String {
    let mut out = format!("{} session {}", s.agent.name(), s.id);
    if let Some(cwd) = &s.cwd {
        let _ = write!(out, "\ncwd: {cwd}");
    }
    if let Some(b) = &s.branch {
        let _ = write!(out, "\nbranch: {b}");
    }
    if s.skipped > 0 {
        let _ = write!(
            out,
            "\nskipped lines (unreadable or too long): {}",
            s.skipped
        );
    }
    mask(&out)
}

pub fn turns(s: &Session, full: bool) -> String {
    let mut out = header(s);
    let _ = write!(out, "\n{} turns\n", s.turns.len());
    let skip = if full {
        0
    } else {
        s.turns.len().saturating_sub(TURNS_CAP)
    };
    if skip > 0 {
        let _ = writeln!(out, "… {skip} earlier turns not shown, use --full");
    }
    for t in s.turns.iter().skip(skip) {
        let tools = t.tools().count();
        let failed = t.tools().filter(|c| c.failed).count();
        let cut_off = t.tools().filter(|c| c.interrupted()).count();
        let mut line = format!(
            "{:>3}  {}  [{} tools",
            t.n,
            first_line(&t.user, LINE_CAP),
            tools
        );
        if failed > 0 {
            let _ = write!(line, ", {failed} failed");
        }
        if cut_off > 0 {
            let _ = write!(line, ", {cut_off} interrupted");
        }
        line.push(']');
        let _ = writeln!(out, "{line}");
    }
    out
}

fn tool_block(c: &ToolCall, full: bool) -> String {
    let head = if c.name.is_empty() { "tool" } else { &c.name };
    let mut out = format!("  > {head}: {}", safe_cut(&c.summary, COMMAND_CAP, full));
    match &c.output {
        None => out.push_str("\n    (interrupted, no result)"),
        Some(o) => {
            let label = if c.failed { "FAILED" } else { "result" };
            let body = safe_cut(o.trim_end(), OUTPUT_CAP, full);
            if body.is_empty() {
                let _ = write!(out, "\n    {label}: (empty)");
            } else {
                let _ = write!(out, "\n    {label}:\n{}", indent(&body));
            }
        }
    }
    out
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|l| format!("      {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn show(s: &Session, n: usize, full: bool) -> Result<String, String> {
    let t = s
        .turns
        .get(n.wrapping_sub(1))
        .ok_or_else(|| format!("no turn {n}; the session has {} turns", s.turns.len()))?;
    let mut out = format!(
        "turn {n}\nuser:\n{}\n",
        indent(&safe_cut(&t.user, USER_CAP, full))
    );
    for item in &t.items {
        match item {
            Item::Text(x) => {
                let _ = writeln!(out, "assistant:\n{}", indent(&safe_cut(x, TEXT_CAP, full)));
            }
            Item::Tool(c) => {
                let _ = writeln!(out, "{}", tool_block(c, full));
            }
        }
    }
    Ok(out)
}

fn snippet(text: &str, needle: &regex::Regex) -> Option<String> {
    let masked = mask(text);
    let line = masked.lines().find(|l| needle.is_match(l))?.trim();
    let short: String = line.chars().take(200).collect();
    Some(if line.chars().count() > 200 {
        format!("{short}…")
    } else {
        short
    })
}

pub fn grep(s: &Session, pattern: &str, full: bool) -> Result<String, String> {
    let re = RegexBuilder::new(pattern)
        .case_insensitive(true)
        .size_limit(1 << 20)
        .build()
        .or_else(|_| {
            RegexBuilder::new(&regex::escape(pattern))
                .case_insensitive(true)
                .build()
        })
        .map_err(|e| e.to_string())?;
    let mut out = String::new();
    let mut count = 0;
    for t in &s.turns {
        for (kind, text) in searchable(t) {
            if let Some(line) = snippet(text, &re) {
                count += 1;
                if full || count <= MATCH_CAP {
                    let _ = writeln!(out, "turn {} {kind}: {line}", t.n);
                }
            }
        }
    }
    if count == 0 {
        return Ok(format!("no matches for {pattern}\n"));
    }
    if !full && count > MATCH_CAP {
        let _ = writeln!(out, "… {} more matches, use --full", count - MATCH_CAP);
    }
    Ok(out)
}

fn searchable(t: &Turn) -> Vec<(&'static str, &str)> {
    let mut v = vec![("user", t.user.as_str())];
    for i in &t.items {
        match i {
            Item::Text(x) => v.push(("assistant", x)),
            Item::Tool(c) => {
                v.push(("tool", &c.summary));
                if let Some(o) = &c.output {
                    v.push(("result", o));
                }
            }
        }
    }
    v
}

pub fn files(s: &Session, full: bool) -> String {
    let mut out = String::new();
    let mut seen_read: HashSet<&str> = HashSet::new();
    let mut seen_edit: HashSet<&str> = HashSet::new();
    let mut lines = 0;
    let mut hidden = 0;
    for t in &s.turns {
        let mut block = Vec::new();
        for c in t.tools() {
            for f in &c.read {
                if seen_read.insert(f) {
                    block.push(format!("  read    {f}"));
                }
            }
            for f in &c.edited {
                if seen_edit.insert(f) {
                    block.push(format!("  edited  {f}"));
                }
            }
        }
        if block.is_empty() {
            continue;
        }
        if !full && lines >= FILES_CAP {
            hidden += block.len();
            continue;
        }
        let _ = writeln!(out, "turn {}", t.n);
        lines += block.len();
        for l in block {
            let _ = writeln!(out, "{l}");
        }
    }
    if hidden > 0 {
        let _ = writeln!(out, "… {hidden} more files, use --full");
    }
    if out.is_empty() {
        out.push_str("no files read or edited\n");
    }
    mask(&out)
}

pub fn errors(s: &Session, full: bool) -> String {
    let mut out = String::new();
    let mut shown = 0;
    let mut total = 0;
    for t in &s.turns {
        for c in t.tools().filter(|c| c.failed) {
            total += 1;
            if full || shown < ERRORS_CAP {
                shown += 1;
                let _ = writeln!(out, "turn {}\n{}", t.n, tool_block(c, full));
            }
        }
    }
    if total > shown {
        let _ = writeln!(out, "… {} more failed calls, use --full", total - shown);
    }
    if out.is_empty() {
        out.push_str("no failed tool calls\n");
    }
    out
}
