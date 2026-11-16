use assert_cmd::Command;
use std::fs;

const CLAUDE: &str = include_str!("../../tmgr-session/tests/fixtures/claude.jsonl");
const CODEX: &str = include_str!("../../tmgr-session/tests/fixtures/codex.jsonl");
const CLAUDE_ID: &str = "11111111-2222-3333-4444-555555555555";
const CODEX_ID: &str = "019a0000-aaaa-bbbb-cccc-000000000001";

fn home() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join(".claude/projects/demo");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join(format!("{CLAUDE_ID}.jsonl")), CLAUDE).unwrap();
    let day = dir.path().join(".codex/sessions/2026/01/01");
    fs::create_dir_all(&day).unwrap();
    fs::write(
        day.join(format!("rollout-2026-01-01T00-00-00-{CODEX_ID}.jsonl")),
        CODEX,
    )
    .unwrap();
    dir
}

fn tmgr(home: &tempfile::TempDir, args: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("tmgr")
        .unwrap()
        .env_clear()
        .env("HOME", home.path())
        .args(args)
        .assert()
}

#[test]
fn turns_finds_claude_and_codex_sessions() {
    let h = home();
    let out = tmgr(&h, &["session", "turns", CLAUDE_ID]).success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(text.contains("2 turns") && text.contains("Add a retry"));
    let out = tmgr(&h, &["session", "turns", &CODEX_ID[..12]]).success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(text.contains("codex session"));
}

#[test]
fn show_errors_files_grep_and_unknown_id() {
    let h = home();
    let shown = tmgr(&h, &["session", "show", CLAUDE_ID, "--turn", "1"]).success();
    let text = String::from_utf8(shown.get_output().stdout.clone()).unwrap();
    assert!(text.contains("FAILED") && !text.contains("abcd1234secretvalue"));
    let errs = tmgr(&h, &["session", "errors", CLAUDE_ID]).success();
    let text = String::from_utf8(errs.get_output().stdout.clone()).unwrap();
    assert!(text.contains("test failed"));
    tmgr(&h, &["session", "files", CLAUDE_ID]).success();
    tmgr(&h, &["session", "grep", CLAUDE_ID, "timeout"]).success();
    tmgr(&h, &["session", "turns", "ffffffff-0000"])
        .failure()
        .code(1);
}

#[test]
fn handoff_writes_private_file() {
    let h = home();
    let out = h.path().join("note.md");
    tmgr(
        &h,
        &[
            "session",
            "handoff",
            CLAUDE_ID,
            "--task",
            "TM-1",
            "-o",
            out.to_str().unwrap(),
        ],
    )
    .success();
    let note = fs::read_to_string(&out).unwrap();
    assert!(note.contains("TM-1") && note.contains("tmgr session turns"));
    assert!(note.contains("You are the codex"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&out).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[cfg(unix)]
#[test]
fn handoff_output_tightens_an_existing_file_and_refuses_a_symlink() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let h = home();
    let existing = h.path().join("old.md");
    fs::write(&existing, "old content that is longer than the note").unwrap();
    fs::set_permissions(&existing, fs::Permissions::from_mode(0o644)).unwrap();
    tmgr(
        &h,
        &[
            "session",
            "handoff",
            CLAUDE_ID,
            "-o",
            existing.to_str().unwrap(),
        ],
    )
    .success();
    assert_eq!(
        fs::metadata(&existing).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(!fs::read_to_string(&existing)
        .unwrap()
        .contains("old content"));

    let target = h.path().join("target.txt");
    fs::write(&target, "keep").unwrap();
    let link = h.path().join("link.md");
    symlink(&target, &link).unwrap();
    tmgr(
        &h,
        &[
            "session",
            "handoff",
            CLAUDE_ID,
            "-o",
            link.to_str().unwrap(),
        ],
    )
    .failure();
    assert_eq!(fs::read_to_string(&target).unwrap(), "keep");
}
