use crate::git::GitState;
use crate::mask::mask;
use crate::model::{Agent, Session};
use crate::render::cut;
use regex::Regex;
use std::fmt::Write;
use std::sync::OnceLock;

const USER_CAP: usize = 3000;
const REPLY_CAP: usize = 1500;
const BUDGET: usize = 20_000;

#[derive(Debug, Clone)]
pub struct Opts {
    pub target: Agent,
    pub task: Option<String>,
    pub previous: Vec<String>,
    pub git: Option<GitState>,
    pub rules_files: Vec<String>,
}

fn test_cmd() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"\b(cargo (nextest|test)|npm (run )?test|npx (jest|vitest)|jest|vitest|pytest|mvn|gradle|go test|yarn test|pnpm test)\b")
            .expect("test")
    })
}

pub fn build(s: &Session, o: &Opts) -> String {
    let mut out = String::from("# Handoff note\n\n");
    let _ = writeln!(
        out,
        "You are taking over work from a {} session. Continue it; do not start over.",
        s.agent.name()
    );
    if let Some(t) = &o.task {
        let _ = writeln!(out, "\nTMGR task: {t}");
    }
    if let Some(cwd) = &s.cwd {
        let _ = writeln!(out, "Working directory: {cwd}");
    }
    if let Some(b) = &s.branch {
        let _ = writeln!(out, "Branch: {b}");
    }
    if !o.rules_files.is_empty() {
        let _ = writeln!(
            out,
            "Read the project rules first: {}",
            o.rules_files.join(", ")
        );
    }

    let _ = writeln!(out, "\n## Conversation so far");
    let mut blocks: Vec<String> = s
        .turns
        .iter()
        .map(|t| {
            let mut b = format!(
                "### Turn {}\nUser:\n{}\n",
                t.n,
                cut(&t.user, USER_CAP, false)
            );
            if let Some(r) = t.final_reply() {
                let _ = write!(b, "Final reply:\n{}\n", cut(r, REPLY_CAP, false));
            }
            b
        })
        .collect();
    let mut omitted = 0;
    while blocks.iter().map(String::len).sum::<usize>() > BUDGET && blocks.len() > 1 {
        blocks.remove(0);
        omitted += 1;
    }
    if omitted > 0 {
        let _ = writeln!(
            out,
            "({omitted} earlier turns omitted; list them with `tmgr session turns {}`)\n",
            s.id
        );
    }
    out.push_str(&blocks.join("\n"));

    let (read, edited) = s.files();
    if !edited.is_empty() {
        let _ = writeln!(out, "\n## Files edited");
        for f in &edited {
            let _ = writeln!(out, "- {f}");
        }
    }
    if !read.is_empty() {
        let _ = writeln!(out, "\n## Files read: {}", read.len());
    }

    if let Some(g) = &o.git {
        let _ = writeln!(out, "\n## Git state");
        for (title, body) in [
            ("status", &g.status),
            ("diff --stat", &g.diff_stat),
            ("recent commits", &g.log),
        ] {
            if !body.is_empty() {
                let _ = writeln!(out, "{title}:\n{}\n", indent(body));
            }
        }
    }

    if let Some(plan) = &s.plan {
        let _ = writeln!(out, "\n## Open plan\n{plan}");
    }

    let last_test = s
        .turns
        .iter()
        .flat_map(|t| t.tools())
        .filter(|c| c.command.as_deref().is_some_and(|x| test_cmd().is_match(x)))
        .last();
    if let Some(c) = last_test {
        let _ = writeln!(out, "\n## Last test command");
        let _ = writeln!(out, "{}", c.command.as_deref().unwrap_or_default());
        match &c.output {
            Some(res) => {
                let tail: Vec<&str> = res.trim_end().lines().rev().take(12).collect();
                let tail: Vec<&str> = tail.into_iter().rev().collect();
                let _ = writeln!(
                    out,
                    "{}:\n{}",
                    if c.failed { "failed" } else { "passed" },
                    indent(&tail.join("\n"))
                );
            }
            None => {
                let _ = writeln!(out, "interrupted, re-check by running it again");
            }
        }
    }

    let interrupted: Vec<String> = s
        .turns
        .iter()
        .flat_map(|t| t.tools())
        .filter(|c| c.interrupted())
        .map(|c| c.summary.lines().next().unwrap_or_default().to_string())
        .collect();
    if !interrupted.is_empty() {
        let _ = writeln!(
            out,
            "\n## Interrupted tool calls (re-check, they may be half done)"
        );
        for i in interrupted {
            let _ = writeln!(out, "- {}", cut(&i, 200, false));
        }
    }

    let _ = writeln!(out, "\n## Earlier sessions");
    for p in &o.previous {
        let _ = writeln!(out, "- {p}");
    }
    let _ = writeln!(out, "- {} ({})", s.id, s.agent.name());
    let _ = writeln!(
        out,
        "\nDetails are in session {id}. Use `tmgr session turns {id}`, `tmgr session show {id} --turn N`, `tmgr session grep {id} \"<pattern>\"`, `tmgr session files {id}` and `tmgr session errors {id}` before guessing or redoing work. You are the {}.",
        o.target.name(),
        id = s.id
    );
    mask(&out)
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|l| format!("    {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}
