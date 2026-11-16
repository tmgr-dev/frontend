use std::io::BufReader;
use tmgr_session::git::GitState;
use tmgr_session::handoff::{build, Opts};
use tmgr_session::mask::mask;
use tmgr_session::{claude, codex, render, Agent};

fn claude_session() -> tmgr_session::Session {
    claude::parse(
        BufReader::new(&include_bytes!("fixtures/claude.jsonl")[..]),
        "fallback",
    )
}

fn codex_session() -> tmgr_session::Session {
    codex::parse(
        BufReader::new(&include_bytes!("fixtures/codex.jsonl")[..]),
        "fallback",
    )
}

#[test]
fn claude_turns_and_meta() {
    let s = claude_session();
    assert_eq!(s.id, "11111111-2222-3333-4444-555555555555");
    assert_eq!(s.cwd.as_deref(), Some("/work/demo"));
    assert_eq!(s.branch.as_deref(), Some("feat/demo"));
    assert_eq!(s.turns.len(), 2);
    assert_eq!(s.turns[0].user, "Add a retry to the fetch helper");
    assert_eq!(s.turns[1].user, "Also add a timeout");
    assert_eq!(s.turns[0].final_reply(), Some("The test failed, fixing."));
    assert!(s.plan.as_deref().unwrap().contains("fix test"));
}

#[test]
fn claude_files_errors_and_interrupted() {
    let s = claude_session();
    let (read, edited) = s.files();
    assert_eq!(read, vec!["/work/demo/src/fetch.rs"]);
    assert_eq!(edited, vec!["/work/demo/src/fetch.rs"]);
    let errs = render::errors(&s, false);
    assert!(errs.contains("test failed"));
    assert!(!errs.contains("abcd1234secretvalue"));
    assert!(s.turns[1].tools().next().unwrap().interrupted());
    assert!(render::turns(&s).contains("1 interrupted"));
}

#[test]
fn codex_turns_files_and_exit_codes() {
    let s = codex_session();
    assert_eq!(s.id, "019a0000-aaaa-bbbb-cccc-000000000001");
    assert_eq!(s.turns.len(), 1);
    assert_eq!(s.turns[0].user, "Add a retry to the fetch helper");
    let (_, edited) = s.files();
    assert_eq!(edited, vec!["src/fetch.rs"]);
    let tools: Vec<_> = s.turns[0].tools().collect();
    assert_eq!(tools[0].command.as_deref(), Some("cargo test -p demo"));
    assert!(tools[0].failed);
    assert!(!tools[1].failed);
    assert!(tools[3].interrupted());
    assert!(s.plan.as_deref().unwrap().contains("fix test"));
}

#[test]
fn show_grep_and_caps() {
    let s = claude_session();
    assert!(render::show(&s, 9, false).is_err());
    let shown = render::show(&s, 1, false).unwrap();
    assert!(shown.contains("FAILED"));
    let g = render::grep(&s, "timeout", false).unwrap();
    assert!(g.contains("turn 2 user"));
    let long = "x".repeat(5000);
    assert!(render::cut(&long, 1500, false).contains("use --full"));
    assert_eq!(render::cut(&long, 1500, true).len(), 5000);
}

#[test]
fn mask_secrets() {
    let pem = format!(
        "-----BEGIN {0} KEY-----\nMIIabc\n-----END {0} KEY-----",
        "PRIVATE"
    );
    let text = format!(
        "key sk-abcdefghijklmnop1234 ghp_abcdefghijklmnopqrstuv password=hunter22 \
        postgres://user:pw123@host/db Authorization: Bearer abcdefgh12345678 {pem} \
        sha 0123456789abcdef0123456789abcdef01234567"
    );
    let m = mask(&text);
    for leak in [
        "abcdefghijklmnop1234",
        "hunter22",
        "pw123",
        "abcdefgh12345678",
        "MIIabc",
    ] {
        assert!(!m.contains(leak), "{leak} leaked: {m}");
    }
    assert!(m.contains("0123456789abcdef0123456789abcdef01234567"));
}

#[test]
fn handoff_note_has_the_essentials() {
    let s = claude_session();
    let note = build(
        &s,
        &Opts {
            target: Agent::Codex,
            task: Some("TM-1".into()),
            previous: vec!["older-session".into()],
            git: Some(GitState {
                status: "## feat/demo".into(),
                diff_stat: String::new(),
                log: "abc123 first".into(),
            }),
            rules_files: vec!["AGENTS.md".into()],
        },
    );
    for needle in [
        "TM-1",
        "Add a retry to the fetch helper",
        "Also add a timeout",
        "The test failed, fixing.",
        "cargo test -p demo",
        "Interrupted tool calls",
        "cargo build",
        "older-session",
        "tmgr session turns 11111111-2222-3333-4444-555555555555",
        "AGENTS.md",
        "fix test",
    ] {
        assert!(note.contains(needle), "missing {needle}:\n{note}");
    }
    assert!(!note.contains("abcd1234secretvalue"));
}
