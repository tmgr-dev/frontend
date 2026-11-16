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
    assert!(render::turns(&s, false).contains("1 interrupted"));
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
        "key {sk}-abcdefghijklmnop1234 {gh}_abcdefghijklmnopqrstuv password=hunter22 \
        postgres://user:pw123@host/db Authorization: Bearer abcdefgh12345678 {pem} \
        sha 0123456789abcdef0123456789abcdef01234567",
        sk = "sk",
        gh = "ghp",
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
            lookup_hint: Some("A copy is under your home.".into()),
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

fn pad(n: usize) -> String {
    "x".repeat(n)
}

#[test]
fn a_cut_never_leaks_the_tail_of_a_secret() {
    let text = format!(
        "{}\nOPENAI_API_KEY={}-proj-abcdefghijklmnopqrstLEAKTAIL\n{}",
        pad(1000),
        "sk",
        pad(1000)
    );
    let cut = render::safe_cut(&text, 1500, false);
    assert!(cut.contains("chars cut"));
    assert!(!cut.contains("LEAKTAIL") && !cut.contains("abcdefghijklmnop"));
}

#[test]
fn a_cut_never_leaks_a_key_whose_begin_line_is_cut_away() {
    let pem = format!(
        "-----BEGIN {0} KEY-----\nLEAKBODY\n-----END {0} KEY-----",
        "PRIVATE"
    );
    let text = format!("{}\n{pem}\n{}", pad(1100), pad(1000));
    let cut = render::safe_cut(&text, 1500, false);
    assert!(!cut.contains("LEAKBODY"));
}

#[test]
fn dsn_passwords_are_masked() {
    for (input, leak) in [
        ("redis://:pw0rdsecret@host:6379", "pw0rdsecret"),
        ("mysql://u:ab/cdsecret@host/db", "cdsecret"),
        ("postgres://user:hunter99@db/x", "hunter99"),
    ] {
        let m = mask(input);
        assert!(!m.contains(leak), "{input} -> {m}");
        assert!(m.contains("@"));
    }
}

#[test]
fn credential_forms_are_masked() {
    let pgp = format!(
        "-----BEGIN PGP {0} KEY BLOCK-----\nPGPBODY\n-----END PGP {0} KEY BLOCK-----",
        "PRIVATE"
    );
    let curl = format!("{} admin:SECRET789 https://x.test", "curl -u");
    let stripe = format!("key {}_live_abcdefghijklmnop", "sk");
    let gitlab = format!("{}-abcdefghijklmnopqrst", "glpat");
    let hf = format!("{}_abcdefghijklmnopqrstuvwx", "hf");
    let npm = format!("{}_abcdefghijklmnopqrstuvwx", "npm");
    let google = format!("{}.abcdefghijklmnopqrstuvwx", "ya29");
    let tele = format!("{}:{}", "123456789", "A".repeat(35));
    for (input, leak) in [
        (
            "Authorization: Basic dXNlcjpCQVNJQ1NFQ1JFVA==",
            "dXNlcjpCQVNJQ1NFQ1JFVA",
        ),
        ("mysql -h db -pSECRET123 mydb", "SECRET123"),
        ("tool --password SECRET456 run", "SECRET456"),
        (curl.as_str(), "SECRET789"),
        (stripe.as_str(), "abcdefghijklmnop"),
        (gitlab.as_str(), "abcdefghijklmnopqrst"),
        (hf.as_str(), "abcdefghijklmnopqrstuvwx"),
        (npm.as_str(), "abcdefghijklmnopqrstuvwx"),
        (google.as_str(), "abcdefghijklmnopqrstuvwx"),
        (tele.as_str(), "AAAAAAAAAAAAAAAAAAAA"),
        (r#"{\"api_key\":\"ESCAPEDSECRET1\"}"#, "ESCAPEDSECRET1"),
        (r#"{"api_key":"PLAINSECRET1"}"#, "PLAINSECRET1"),
        (pgp.as_str(), "PGPBODY"),
    ] {
        let m = mask(input);
        assert!(!m.contains(leak), "{input} -> {m}");
    }
}

#[test]
fn masking_keeps_ordinary_text() {
    let m = mask("cargo test -p tmgr-cli -- max_tokens=4096 and a path /usr/local/bin");
    assert!(m.contains("-p tmgr-cli") && m.contains("max_tokens=4096"));
}

#[test]
fn unreadable_lines_are_counted() {
    let mut data = Vec::from(
        &b"{\"type\":\"user\",\"sessionId\":\"s\",\"message\":{\"content\":\"hi there\"}}\n"[..],
    );
    data.extend_from_slice(b"\xff\xfe not json\n");
    data.extend_from_slice(
        b"{\"type\":\"user\",\"message\":{\"content\":\"bad \\ud800 escape\"}}\n",
    );
    data.extend_from_slice(b"{\"type\":\"user\",\"message\":{\"content\":\"second one\"}}\n");
    let s = claude::parse(BufReader::new(&data[..]), "x");
    assert_eq!(s.turns.len(), 2);
    assert_eq!(s.skipped, 2);
    assert!(render::turns(&s, false).contains("skipped lines"));
}

#[test]
fn codex_patch_through_the_shell_counts_as_an_edit() {
    let line = r#"{"type":"response_item","payload":{"type":"function_call","name":"shell","arguments":"{\"command\":[\"apply_patch\",\"*** Begin Patch\\n*** Update File: src/a.rs\\n*** End Patch\"]}","call_id":"p1"}}"#;
    let s = codex::parse(BufReader::new(line.as_bytes()), "x");
    assert_eq!(s.files().1, vec!["src/a.rs"]);
}

#[test]
fn grep_and_glob_show_the_pattern() {
    let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"g","name":"Grep","input":{"pattern":"needleword","path":"/work"}}]}}"#;
    let s = claude::parse(BufReader::new(line.as_bytes()), "x");
    assert_eq!(s.turns[0].tools().next().unwrap().summary, "needleword");
}

#[test]
fn files_are_listed_once() {
    let s = claude_session();
    let out = render::files(&s, false);
    assert_eq!(out.matches("/work/demo/src/fetch.rs").count(), 2);
}

#[test]
fn only_real_test_runs_count_as_the_last_test() {
    let mk = |cmd: &str| {
        let line = format!(
            r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"b","name":"Bash","input":{{"command":"{cmd}"}}}}]}}}}"#
        );
        let s = claude::parse(BufReader::new(line.as_bytes()), "x");
        build(
            &s,
            &Opts {
                target: Agent::Codex,
                task: None,
                previous: vec![],
                git: None,
                rules_files: vec![],
                lookup_hint: None,
            },
        )
    };
    assert!(!mk("cat jest.config.js").contains("Last test command"));
    assert!(!mk("mvn -DskipTests package").contains("Last test command"));
    assert!(mk("cd x && npx jest src/a.test.ts").contains("Last test command"));
}

#[test]
fn the_handoff_note_stays_small_for_a_long_session() {
    let mut data = String::new();
    for i in 0..400 {
        data.push_str(&format!(
            "{{\"type\":\"user\",\"sessionId\":\"s\",\"message\":{{\"content\":\"turn {i} {}\"}}}}\n",
            "word ".repeat(600)
        ));
    }
    let s = claude::parse(BufReader::new(data.as_bytes()), "x");
    let note = build(
        &s,
        &Opts {
            target: Agent::Codex,
            task: None,
            previous: vec![],
            git: None,
            rules_files: vec![],
            lookup_hint: None,
        },
    );
    assert!(note.len() < 30_000, "{}", note.len());
    assert!(note.contains("earlier turns omitted"));
}

#[test]
fn locate_rules() {
    use tmgr_session::locate::{find, Roots};
    let dir = tempfile::tempdir().unwrap();
    let day = dir.path().join("codex/sessions/2026/01/01");
    std::fs::create_dir_all(&day).unwrap();
    let uuid = "019a0000-aaaa-bbbb-cccc-000000000001";
    let f = day.join(format!("rollout-2026-01-01T00-00-00-{uuid}.jsonl"));
    std::fs::write(&f, "{}\n").unwrap();
    let outside = dir.path().join("other.jsonl");
    std::fs::write(&outside, "{}\n").unwrap();
    let roots = Roots {
        claude: Some(dir.path().join("claude")),
        codex: Some(dir.path().join("codex")),
    };
    assert!(find(&roots, uuid).is_ok());
    assert!(find(&roots, "019a0000").is_ok());
    assert!(find(&roots, "2026-01-01").is_err());
    assert!(find(&roots, f.to_str().unwrap()).is_ok());
    assert!(find(&roots, outside.to_str().unwrap()).is_err());
}

#[test]
fn export_masks_and_lands_where_the_lookup_finds_it() {
    use tmgr_session::locate::{export_masked, find, newest, Roots};
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old");
    let project = old.join(".claude/projects/p");
    std::fs::create_dir_all(&project).unwrap();
    let id = "11111111-2222-3333-4444-555555555555";
    let src = project.join(format!("{id}.jsonl"));
    let secret = format!("{}-proj-abcdefghijklmnopqrstLEAKTAIL", "sk");
    let body = format!(
        "{{\"type\":\"user\",\"sessionId\":\"{id}\",\"message\":{{\"content\":\"use {secret} now\"}}}}\n"
    );
    std::fs::write(&src, body).unwrap();
    let found = newest(&Roots::for_home(&old), Agent::Claude).unwrap();
    assert_eq!(found, src);
    let new_home = dir.path().join("new");
    let copy = export_masked(&src, Agent::Claude, &new_home).unwrap();
    let text = std::fs::read_to_string(&copy).unwrap();
    assert!(!text.contains("LEAKTAIL") && text.contains("use "));
    let roots = Roots::for_home(&new_home);
    let (agent, path) = find(&roots, id).unwrap();
    assert_eq!(agent, Agent::Claude);
    assert_eq!(path, copy);
    assert!(newest(&roots, Agent::Claude).is_none());
}
