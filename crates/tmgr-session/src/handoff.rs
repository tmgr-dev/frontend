use crate::git::GitState;
use crate::mask::mask;
use crate::model::{Agent, Session};
use crate::render::safe_cut;
use regex::Regex;
use std::fmt::Write;
use std::sync::OnceLock;

const USER_CAP: usize = 3000;
const REPLY_CAP: usize = 1500;
const TOTAL_BUDGET: usize = 24_000;
const MIN_CONVERSATION: usize = 4_000;
const STATUS_LINES: usize = 30;
const STAT_LINES: usize = 25;
const LOG_LINES: usize = 5;
const PLAN_LINES: usize = 30;
const EDITED_FILES: usize = 40;
const INTERRUPTED: usize = 10;

#[derive(Debug, Clone)]
pub struct Opts {
    pub target: Agent,
    pub task: Option<String>,
    pub previous: Vec<String>,
    pub git: Option<GitState>,
    pub rules_files: Vec<String>,
    pub lookup_hint: Option<String>,
}

fn test_cmd() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(concat!(
            r"(?:^|[\s;&|(])(?:",
            r"cargo\s+(?:nextest\s+run|test)",
            r"|npm\s+(?:run\s+)?test",
            r"|npx\s+(?:jest|vitest)",
            r"|(?:jest|vitest|pytest)",
            r"|mvn\s+(?:\S+\s+)*(?:test|verify)",
            r"|go\s+test",
            r"|yarn\s+test",
            r"|pnpm\s+test",
            r"|gradle\s+test",
            r")(?:\s|$)",
        ))
        .expect("test")
    })
}

fn head_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= n {
        return text.to_string();
    }
    format!(
        "{}\n… {} more lines",
        lines[..n].join("\n"),
        lines.len() - n
    )
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|l| format!("    {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn build(s: &Session, o: &Opts) -> String {
    let mut head = String::from("# Handoff note\n\n");
    let _ = writeln!(
        head,
        "You are taking over work from a {} session. Continue it; do not start over.",
        s.agent.name()
    );
    if let Some(t) = &o.task {
        let _ = writeln!(head, "\nTMGR task: {t}");
    }
    if let Some(cwd) = &s.cwd {
        let _ = writeln!(head, "Working directory: {cwd}");
    }
    if let Some(b) = &s.branch {
        let _ = writeln!(head, "Branch: {b}");
    }
    if !o.rules_files.is_empty() {
        let _ = writeln!(
            head,
            "Read the project rules first: {}",
            o.rules_files.join(", ")
        );
    }
    if s.skipped > 0 {
        let _ = writeln!(
            head,
            "Note: {} lines of the old session could not be read.",
            s.skipped
        );
    }

    let mut tail = String::new();
    let (read, edited) = s.files();
    if !edited.is_empty() {
        let _ = writeln!(tail, "\n## Files edited");
        for f in edited.iter().take(EDITED_FILES) {
            let _ = writeln!(tail, "- {f}");
        }
        if edited.len() > EDITED_FILES {
            let _ = writeln!(
                tail,
                "- … {} more, see `tmgr session files {}`",
                edited.len() - EDITED_FILES,
                s.id
            );
        }
    }
    if !read.is_empty() {
        let _ = writeln!(tail, "\n## Files read: {}", read.len());
    }

    if let Some(g) = &o.git {
        let _ = writeln!(tail, "\n## Git state");
        for (title, body, n) in [
            ("status", &g.status, STATUS_LINES),
            ("diff --stat", &g.diff_stat, STAT_LINES),
            ("recent commits", &g.log, LOG_LINES),
        ] {
            if !body.is_empty() {
                let _ = writeln!(tail, "{title}:\n{}\n", indent(&head_lines(body, n)));
            }
        }
    }

    if let Some(plan) = &s.plan {
        let _ = writeln!(tail, "\n## Open plan\n{}", head_lines(plan, PLAN_LINES));
    }

    let last_test = s
        .turns
        .iter()
        .flat_map(|t| t.tools())
        .filter(|c| c.command.as_deref().is_some_and(|x| test_cmd().is_match(x)))
        .last();
    if let Some(c) = last_test {
        let _ = writeln!(tail, "\n## Last test command");
        let _ = writeln!(
            tail,
            "{}",
            safe_cut(c.command.as_deref().unwrap_or_default(), 300, false)
        );
        match &c.output {
            Some(res) => {
                let masked = mask(res);
                let lines: Vec<&str> = masked.trim_end().lines().collect();
                let from = lines.len().saturating_sub(12);
                let body = safe_cut(&lines[from..].join("\n"), 1500, false);
                let _ = writeln!(
                    tail,
                    "{}:\n{}",
                    if c.failed { "failed" } else { "passed" },
                    indent(&body)
                );
            }
            None => {
                let _ = writeln!(tail, "interrupted, re-check by running it again");
            }
        }
    }

    let interrupted: Vec<&str> = s
        .turns
        .iter()
        .flat_map(|t| t.tools())
        .filter(|c| c.interrupted())
        .map(|c| c.summary.lines().next().unwrap_or_default())
        .collect();
    if !interrupted.is_empty() {
        let _ = writeln!(
            tail,
            "\n## Interrupted tool calls (re-check, they may be half done)"
        );
        for i in interrupted.iter().take(INTERRUPTED) {
            let _ = writeln!(tail, "- {}", safe_cut(i, 200, false));
        }
        if interrupted.len() > INTERRUPTED {
            let _ = writeln!(tail, "- … {} more", interrupted.len() - INTERRUPTED);
        }
    }

    let _ = writeln!(tail, "\n## Earlier sessions");
    for p in &o.previous {
        let _ = writeln!(tail, "- {}", safe_cut(p, 80, false));
    }
    let _ = writeln!(tail, "- {} ({})", s.id, s.agent.name());
    if let Some(h) = &o.lookup_hint {
        let _ = writeln!(tail, "\n{}", safe_cut(h, 400, false));
    }
    let _ = writeln!(
        tail,
        "\nDetails are in session {id}. Use `tmgr session turns {id}`, `tmgr session show {id} --turn N`, `tmgr session grep {id} \"<pattern>\"`, `tmgr session files {id}` and `tmgr session errors {id}` before guessing or redoing work. You are the {}.",
        o.target.name(),
        id = s.id
    );

    let budget = TOTAL_BUDGET
        .saturating_sub(head.len() + tail.len())
        .max(MIN_CONVERSATION);
    let mut blocks: Vec<String> = s
        .turns
        .iter()
        .map(|t| {
            let mut b = format!(
                "### Turn {}\nUser:\n{}\n",
                t.n,
                safe_cut(&t.user, USER_CAP, false)
            );
            if let Some(r) = t.final_reply() {
                let _ = write!(b, "Final reply:\n{}\n", safe_cut(r, REPLY_CAP, false));
            }
            b
        })
        .collect();
    let mut omitted = 0;
    while blocks.iter().map(String::len).sum::<usize>() > budget && blocks.len() > 1 {
        blocks.remove(0);
        omitted += 1;
    }
    let mut convo = String::from("\n## Conversation so far\n");
    if omitted > 0 {
        let _ = writeln!(
            convo,
            "({omitted} earlier turns omitted; list them with `tmgr session turns {}`)\n",
            s.id
        );
    }
    convo.push_str(&blocks.join("\n"));

    mask(&format!("{head}{convo}{tail}"))
}
