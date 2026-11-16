use crate::mask::mask;
use crate::model::{Item, Session, ToolCall, Turn};
use regex::RegexBuilder;
use std::fmt::Write;

const USER_CAP: usize = 2000;
const TEXT_CAP: usize = 2000;
const OUTPUT_CAP: usize = 1500;
const COMMAND_CAP: usize = 400;
const LINE_CAP: usize = 100;
const MATCH_CAP: usize = 40;

pub fn cut(text: &str, cap: usize, full: bool) -> String {
    if full {
        return text.to_string();
    }
    let total = text.chars().count();
    if total <= cap {
        return text.to_string();
    }
    let head: String = text.chars().take(cap * 2 / 3).collect();
    let tail: String = text.chars().skip(total - cap / 3).collect();
    format!(
        "{head}\n… [{} chars cut, use --full] …\n{tail}",
        total - cap * 2 / 3 - cap / 3
    )
}

fn first_line(text: &str, cap: usize) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let short: String = line.trim().chars().take(cap).collect();
    if line.trim().chars().count() > cap {
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
    out
}

pub fn turns(s: &Session) -> String {
    let mut out = mask(&header(s));
    let _ = write!(out, "\n{} turns\n", s.turns.len());
    for t in &s.turns {
        let tools = t.tools().count();
        let failed = t.tools().filter(|c| c.failed).count();
        let cut_off = t.tools().filter(|c| c.interrupted()).count();
        let mut line = format!(
            "{:>3}  {}  [{} tools",
            t.n,
            mask(&first_line(&t.user, LINE_CAP)),
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
    let mut out = format!("  > {head}: {}", cut(&c.summary, COMMAND_CAP, full));
    match &c.output {
        None => out.push_str("\n    (interrupted, no result)"),
        Some(o) => {
            let label = if c.failed { "FAILED" } else { "result" };
            let body = cut(o.trim_end(), OUTPUT_CAP, full);
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
        indent(&cut(&t.user, USER_CAP, full))
    );
    for item in &t.items {
        match item {
            Item::Text(x) => {
                let _ = writeln!(out, "assistant:\n{}", indent(&cut(x, TEXT_CAP, full)));
            }
            Item::Tool(c) => {
                let _ = writeln!(out, "{}", tool_block(c, full));
            }
        }
    }
    Ok(mask(&out))
}

fn snippet(text: &str, needle: &regex::Regex) -> Option<String> {
    let line = text.lines().find(|l| needle.is_match(l))?;
    let t = line.trim();
    let short: String = t.chars().take(200).collect();
    Some(if t.chars().count() > 200 {
        format!("{short}…")
    } else {
        short
    })
}

pub fn grep(s: &Session, pattern: &str, full: bool) -> Result<String, String> {
    let re = RegexBuilder::new(pattern)
        .case_insensitive(true)
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
            if let Some(line) = snippet(&text, &re) {
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
    Ok(mask(&out))
}

fn searchable(t: &Turn) -> Vec<(&'static str, String)> {
    let mut v = vec![("user", t.user.clone())];
    for i in &t.items {
        match i {
            Item::Text(x) => v.push(("assistant", x.clone())),
            Item::Tool(c) => {
                v.push(("tool", c.summary.clone()));
                if let Some(o) = &c.output {
                    v.push(("result", o.clone()));
                }
            }
        }
    }
    v
}

pub fn files(s: &Session) -> String {
    let mut out = String::new();
    for t in &s.turns {
        let mut read: Vec<&str> = Vec::new();
        let mut edited: Vec<&str> = Vec::new();
        for c in t.tools() {
            read.extend(c.read.iter().map(String::as_str));
            edited.extend(c.edited.iter().map(String::as_str));
        }
        read.dedup();
        edited.dedup();
        if read.is_empty() && edited.is_empty() {
            continue;
        }
        let _ = writeln!(out, "turn {}", t.n);
        for f in read {
            let _ = writeln!(out, "  read    {f}");
        }
        for f in edited {
            let _ = writeln!(out, "  edited  {f}");
        }
    }
    if out.is_empty() {
        out.push_str("no files read or edited\n");
    }
    mask(&out)
}

pub fn errors(s: &Session, full: bool) -> String {
    let mut out = String::new();
    for t in &s.turns {
        for c in t.tools().filter(|c| c.failed) {
            let _ = writeln!(out, "turn {}\n{}", t.n, tool_block(c, full));
        }
    }
    if out.is_empty() {
        out.push_str("no failed tool calls\n");
    }
    mask(&out)
}
