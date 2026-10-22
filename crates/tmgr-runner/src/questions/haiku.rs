use super::rules::{collapse, MAX_ITEM as MAX_SPAN};
use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const TIMEOUT: Duration = Duration::from_secs(60);
const OUT_CAP: u64 = 64 * 1024;
const MAX_QUESTIONS: usize = 8;
pub const INPUT_CAP: usize = 12_000;

pub const PROMPT: &str = "You are given one message that a coding agent wrote to its user. List the questions in it that the agent still needs the user to answer or decide. Skip rhetorical questions, questions the message itself answers, and questions that only appear inside code or quotations. Treat a numbered or bulleted list item, or a paragraph, as ONE question even when it holds several question sentences (a title with sub-questions, for example): never split it. Copy each question word for word from the message as the whole item or paragraph, without its list number. Reply with JSON only, in this shape: {\"questions\":[\"...\"]}. If there are none, reply {\"questions\":[]}.";

#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    Missing,
    Spawn(String),
    Timeout,
    Exit(Option<i32>),
    Unreadable,
}

pub fn find_claude(path_var: Option<&std::ffi::OsStr>, home: Option<&Path>) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = path_var
        .map(|p| std::env::split_paths(p).collect())
        .unwrap_or_default();
    if let Some(h) = home {
        dirs.push(h.join(".local/bin"));
        dirs.push(h.join(".claude/local"));
    }
    dirs.into_iter()
        .filter(|d| d.is_absolute())
        .map(|d| d.join("claude"))
        .find(|p| is_exe(p))
}

fn is_exe(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

pub fn classify(
    bin: &Path,
    workdir: &Path,
    text: &str,
    timeout: Duration,
) -> Result<Vec<String>, Failure> {
    let reply = complete(bin, workdir, PROMPT, text, timeout)?;
    parse(&reply, text).ok_or(Failure::Unreadable)
}

pub fn complete(
    bin: &Path,
    workdir: &Path,
    prompt: &str,
    text: &str,
    timeout: Duration,
) -> Result<String, Failure> {
    if !is_exe(bin) {
        return Err(Failure::Missing);
    }
    let mut child = Command::new(bin)
        .args([
            "-p",
            prompt,
            "--model",
            "haiku",
            "--output-format",
            "text",
            "--no-session-persistence",
            "--safe-mode",
            "--tools",
            "",
        ])
        .current_dir(workdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| Failure::Spawn(e.kind().to_string()))?;
    let pgid = child.id() as libc::pid_t;
    let mut stdin = child.stdin.take();
    let input = text.as_bytes().to_vec();
    let writer = std::thread::spawn(move || {
        if let Some(mut s) = stdin.take() {
            let _ = s.write_all(&input);
        }
    });
    let mut stdout = child.stdout.take();
    let (out_tx, out_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(o) = stdout.as_mut() {
            let _ = o.take(OUT_CAP).read_to_end(&mut buf);
        }
        let _ = out_tx.send(buf);
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) if Instant::now() >= deadline => break None,
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(_) => break None,
        }
    };
    let Some(status) = status else {
        unsafe { libc::killpg(pgid, libc::SIGKILL) };
        let _ = child.kill();
        let _ = child.wait();
        let _ = writer.join();
        return Err(Failure::Timeout);
    };
    unsafe { libc::killpg(pgid, libc::SIGKILL) };
    let _ = writer.join();
    let out = out_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_default();
    if !status.success() {
        return Err(Failure::Exit(status.code()));
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

pub fn parse(reply: &str, source: &str) -> Option<Vec<String>> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    let v: Value = serde_json::from_str(reply.get(start..=end)?).ok()?;
    let list = v.get("questions")?.as_array()?;
    let hay = collapse(source).to_lowercase();
    let mut out: Vec<String> = Vec::new();
    for q in list.iter().filter_map(Value::as_str) {
        let q = collapse(q);
        let q = q.chars().take(MAX_SPAN).collect::<String>();
        if q.chars().count() < 4 || !hay.contains(&q.to_lowercase()) {
            continue;
        }
        if !out.iter().any(|o| o.eq_ignore_ascii_case(&q)) && out.len() < MAX_QUESTIONS {
            out.push(q);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const TEXT: &str = "I fixed it.\n\nShould I also update the docs? And bump the version?";

    #[test]
    fn keeps_only_questions_found_in_the_text() {
        let reply = "```json\n{\"questions\":[\"Should I also update the docs?\",\"Invented one?\",\"should i also update the docs?\"]}\n```";
        assert_eq!(
            parse(reply, TEXT),
            Some(vec!["Should I also update the docs?".to_string()])
        );
    }

    #[test]
    fn empty_and_bad_replies() {
        assert_eq!(parse("{\"questions\":[]}", TEXT), Some(vec![]));
        assert_eq!(parse("no json", TEXT), None);
        assert_eq!(parse("{\"questions\":\"x\"}", TEXT), None);
        assert_eq!(parse("{\"other\":[]}", TEXT), None);
    }

    fn fake(dir: &Path, body: &str) -> PathBuf {
        let src = dir.join("claude.src");
        std::fs::write(&src, format!("#!/bin/sh\n{body}\n")).unwrap();
        let bin = dir.join("claude");
        let copied = Command::new("cp").arg(&src).arg(&bin).status().unwrap();
        assert!(copied.success());
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    #[test]
    fn text_goes_on_stdin_and_argv_is_fixed() {
        let d = tempfile::tempdir().unwrap();
        let bin = fake(
            d.path(),
            "echo \"$@\" > args.txt\ncat > stdin.txt\necho '{\"questions\":[\"Should I also update the docs?\"]}'",
        );
        let work = tempfile::tempdir().unwrap();
        let got = classify(&bin, work.path(), TEXT, Duration::from_secs(10)).unwrap();
        assert_eq!(got, vec!["Should I also update the docs?"]);
        assert_eq!(
            std::fs::read_to_string(work.path().join("stdin.txt")).unwrap(),
            TEXT
        );
        let args = std::fs::read_to_string(work.path().join("args.txt")).unwrap();
        assert!(args.contains("--model haiku") && !args.contains("update the docs"));
    }

    #[test]
    fn failures_are_reported() {
        let d = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let bin = fake(d.path(), "cat >/dev/null\nexit 3");
        assert_eq!(
            classify(&bin, work.path(), TEXT, Duration::from_secs(10)),
            Err(Failure::Exit(Some(3)))
        );
        let bin = fake(d.path(), "cat >/dev/null\necho nonsense");
        assert_eq!(
            classify(&bin, work.path(), TEXT, Duration::from_secs(10)),
            Err(Failure::Unreadable)
        );
        assert_eq!(
            classify(
                &d.path().join("missing"),
                work.path(),
                TEXT,
                Duration::from_secs(1)
            ),
            Err(Failure::Missing)
        );
    }

    #[test]
    fn a_hung_child_is_killed() {
        let d = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let bin = fake(d.path(), "sleep 30");
        let t = Instant::now();
        assert_eq!(
            classify(&bin, work.path(), TEXT, Duration::from_millis(300)),
            Err(Failure::Timeout)
        );
        assert!(t.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn finds_claude_on_the_path_and_in_home() {
        let d = tempfile::tempdir().unwrap();
        let bin = fake(d.path(), "true");
        let path = std::env::join_paths([d.path()]).unwrap();
        assert_eq!(find_claude(Some(&path), None), Some(bin));
        assert_eq!(find_claude(None, None), None);
    }
}
