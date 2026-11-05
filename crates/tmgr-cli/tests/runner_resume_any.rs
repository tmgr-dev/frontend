#![cfg(all(unix, feature = "runner"))]

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, SystemTime};

const BIN: &str = env!("CARGO_BIN_EXE_tmgr");
const C1: &str = "11111111-0000-4000-8000-000000000001";
const X1: &str = "22222222-0000-4000-8000-000000000002";
const CLAUDE_AGENT: &str =
    "id = \"claude-code\"\nname = \"fake\"\ncommand = [\"/bin/sh\", \"SCRIPT\"]\n\
    [permission_modes]\nask = [\"--ask\"]\nbypass = [\"--yolo\"]\n\
    [resume]\nargs = [\"--continue\"]\nby_id = [\"--resume\", \"{resume_id}\"]\n\
    fork_by_id = [\"--resume\", \"{resume_id}\", \"--fork-session\"]\n";
const CODEX_AGENT: &str = "id = \"codex\"\nname = \"fake\"\ncommand = [\"/bin/sh\", \"SCRIPT\"]\n\
    [permission_modes]\nask = [\"--ask\"]\nbypass = [\"--yolo\"]\n\
    [resume]\nargs = [\"resume\", \"--last\"]\nat = \"after_command\"\n\
    by_id = [\"resume\", \"{resume_id}\"]\nfork_by_id = [\"fork\", \"{resume_id}\"]\n";

struct Rig {
    tmp: tempfile::TempDir,
    run: tempfile::TempDir,
    work: PathBuf,
}

impl Rig {
    fn new() -> Rig {
        let tmp = tempfile::tempdir().unwrap();
        let run = tempfile::Builder::new()
            .prefix("ra")
            .tempdir_in("/tmp")
            .unwrap();
        fs::set_permissions(run.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let home = tmp.path().join("home");
        let work = fs::canonicalize(tmp.path()).unwrap().join("work");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&work).unwrap();
        fs::create_dir_all(tmp.path().join("homes")).unwrap();
        for d in [tmp.path(), &tmp.path().join("homes")] {
            fs::set_permissions(d, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let agents = tmp.path().join("agents");
        fs::create_dir_all(&agents).unwrap();
        let script = tmp.path().join("agent.sh");
        fs::write(&script, "sleep 60\n").unwrap();
        for (name, body) in [
            ("claude-code.toml", CLAUDE_AGENT),
            ("codex.toml", CODEX_AGENT),
        ] {
            fs::write(
                agents.join(name),
                body.replace("SCRIPT", &script.display().to_string()),
            )
            .unwrap();
        }
        let rig = Rig { tmp, run, work };
        rig.cmd(&["usage", "consent", "allow"]).output().unwrap();
        rig
    }

    fn home(&self) -> PathBuf {
        self.tmp.path().join("home")
    }

    fn envs(&self) -> Vec<(&'static str, PathBuf)> {
        vec![
            ("PATH", "/usr/bin:/bin".into()),
            ("HOME", self.home()),
            ("TMGR_USAGE_DIR", self.tmp.path().join("usage")),
            ("TMGR_RUNNER_DIR", self.run.path().into()),
            ("TMGR_RUNNER_HOMES", self.tmp.path().join("homes")),
            ("TMGR_AGENTS_DIR", self.tmp.path().join("agents")),
            ("XDG_CONFIG_HOME", self.tmp.path().join("c")),
            ("TMGR_CREDENTIAL_STORE", "file".into()),
            ("NO_COLOR", "1".into()),
        ]
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        c.args(args).env_clear().stdin(Stdio::null());
        for (k, v) in self.envs() {
            c.env(k, v);
        }
        c
    }

    fn pty(&self, args: &[&str], input: &str) -> (i32, String) {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 200,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut cmd = CommandBuilder::new(BIN);
        cmd.args(args);
        cmd.env_clear();
        cmd.cwd(self.tmp.path());
        for (k, v) in self.envs() {
            cmd.env(k, v);
        }
        let mut child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut writer = pair.master.take_writer().unwrap();
        let _ = std::io::Write::write_all(&mut writer, input.as_bytes());
        let t = std::thread::spawn(move || {
            let mut out = Vec::new();
            let _ = std::io::Read::read_to_end(&mut reader, &mut out);
            String::from_utf8_lossy(&out).into_owned()
        });
        let code = child.wait().unwrap().exit_code() as i32;
        drop(writer);
        drop(pair.master);
        (code, t.join().unwrap_or_default())
    }

    fn claude_file(&self, id: &str, cwd: &str, title: &str) -> PathBuf {
        let rec = json!({"type":"user","timestamp":"2026-11-05T10:00:00Z","cwd":cwd,
            "message":{"content":title}});
        let p = self.home().join(format!(".claude/projects/-w/{id}.jsonl"));
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, format!("{rec}\n")).unwrap();
        age(&p);
        p
    }

    fn codex_file(&self, id: &str, cwd: &str) -> PathBuf {
        let meta = json!({"type":"session_meta","payload":{"id":id,"timestamp":"2026-11-05T10:00:00Z",
            "cwd":cwd,"source":"cli"}});
        let msg = json!({"type":"event_msg","payload":{"type":"user_message","message":"hello"}});
        let p = self.home().join(format!(
            ".codex/sessions/2026/11/05/rollout-2026-11-05T10-00-00-{id}.jsonl"
        ));
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, format!("{meta}\n{msg}\n")).unwrap();
        age(&p);
        p
    }

    fn stop(&self) {
        let _ = self.cmd(&["runner", "stop"]).output();
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.stop();
    }
}

fn age(p: &Path) {
    let f = fs::File::options().write(true).open(p).unwrap();
    f.set_modified(SystemTime::now() - Duration::from_secs(3600))
        .unwrap();
}

fn hash_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.insert(p.clone(), Sha256::digest(fs::read(&p).unwrap()).to_vec());
            }
        }
    }
    out
}

fn json_line(out: &str) -> Value {
    let start = out
        .rfind("{\"schema\"")
        .unwrap_or_else(|| panic!("no json in {out:?}"));
    serde_json::from_str(out[start..].lines().next().unwrap().trim()).unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

const CONSENT: &str = "no-sandbox\n";

#[test]
fn resume_and_fork_start_the_agent_with_the_recorded_id_in_the_recorded_folder() {
    let rig = Rig::new();
    let cwd = rig.work.display().to_string();
    rig.claude_file(C1, &cwd, "claude title");
    rig.codex_file(X1, &cwd);
    let before = hash_tree(&rig.home());
    for (id, agent, fork, bypass, want) in [
        (
            C1,
            "claude-code",
            false,
            false,
            vec!["--ask", "--resume", C1],
        ),
        (
            C1,
            "claude-code",
            false,
            true,
            vec!["--yolo", "--resume", C1],
        ),
        (
            C1,
            "claude-code",
            true,
            false,
            vec!["--ask", "--resume", C1, "--fork-session"],
        ),
        (
            C1,
            "claude-code",
            true,
            true,
            vec!["--yolo", "--resume", C1, "--fork-session"],
        ),
        (X1, "codex", false, false, vec!["resume", X1, "--ask"]),
        (X1, "codex", false, true, vec!["resume", X1, "--yolo"]),
        (X1, "codex", true, false, vec!["fork", X1, "--ask"]),
        (X1, "codex", true, true, vec!["fork", X1, "--yolo"]),
    ] {
        let mut args = vec!["--json", "resume", id, "--no-sandbox", "--no-attach"];
        if fork {
            args.push("--fork");
        }
        if bypass {
            args.extend(["--mode", "bypass"]);
        }
        let (code, out) = rig.pty(&args, CONSENT);
        assert_eq!(code, 0, "{agent} fork={fork} bypass={bypass}: {out}");
        let v = json_line(&out);
        let s = &v["data"];
        let argv: Vec<&str> = s["argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect();
        assert_eq!(argv[2..], want[..], "{agent} fork={fork} bypass={bypass}");
        assert_eq!(s["profile"], agent);
        assert_eq!(s["cwd"], cwd);
        assert_eq!(s["no_sandbox"], true);
        let recorded = if fork { Value::Null } else { json!(id) };
        assert_eq!(s["agent_session_id"], recorded, "{agent} fork={fork}");
        let _ = rig
            .cmd(&["halt", s["id"].as_str().unwrap(), "--kill"])
            .output();
    }
    assert_eq!(
        hash_tree(&rig.home()).get(
            &rig.home()
                .join(".claude/projects/-w")
                .join(format!("{C1}.jsonl"))
        ),
        before.get(
            &rig.home()
                .join(".claude/projects/-w")
                .join(format!("{C1}.jsonl"))
        )
    );
    let after = hash_tree(&rig.home());
    for (p, h) in &before {
        assert_eq!(after.get(p), Some(h), "{}", p.display());
    }
}

#[test]
fn a_conversation_id_without_no_sandbox_is_refused_with_a_hint() {
    let rig = Rig::new();
    rig.claude_file(C1, &rig.work.display().to_string(), "t");
    let (code, out) = rig.pty(&["resume", C1, "--no-attach"], "");
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("sandbox cannot see"), "{out}");
    assert!(out.contains("--no-sandbox"), "{out}");
    assert!(!rig.run.path().join("runner.pid").exists());
}

#[test]
fn a_live_conversation_is_refused_and_a_recent_one_warns() {
    let rig = Rig::new();
    let cwd = rig.work.display().to_string();
    let p = rig.claude_file(C1, &cwd, "t");
    let sdir = rig.home().join(".claude/sessions");
    fs::create_dir_all(&sdir).unwrap();
    fs::write(
        sdir.join("1.json"),
        json!({"pid": std::process::id(), "sessionId": C1}).to_string(),
    )
    .unwrap();
    let (code, out) = rig.pty(&["resume", C1, "--no-sandbox", "--no-attach"], CONSENT);
    assert_eq!(code, 6, "{out}");
    assert!(out.contains("--fork"), "{out}");
    fs::remove_file(sdir.join("1.json")).unwrap();
    fs::File::options()
        .write(true)
        .open(&p)
        .unwrap()
        .set_modified(SystemTime::now())
        .unwrap();
    let (code, out) = rig.pty(&["resume", C1, "--no-sandbox", "--no-attach"], CONSENT);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("warning: this session was active a moment ago"),
        "{out}"
    );
}

#[test]
fn a_missing_folder_is_refused_without_starting_and_hostile_text_is_neutralised() {
    let rig = Rig::new();
    let gone = format!("{}/gone\u{1b}]0;pwn\u{7}", rig.work.display());
    rig.claude_file(C1, &gone, "t\u{1b}[2J");
    let (code, out) = rig.pty(&["resume", C1, "--no-sandbox", "--no-attach"], CONSENT);
    assert_eq!(code, 3, "{out}");
    assert!(out.contains("no longer exists"), "{out}");
    assert!(!out.contains('\u{7}'), "{out:?}");
    assert!(!rig.run.path().join("runner.pid").exists());
}

#[test]
fn a_symlinked_recorded_folder_is_refused_naming_both_paths() {
    let rig = Rig::new();
    let link = rig.work.parent().unwrap().join("link");
    std::os::unix::fs::symlink(&rig.work, &link).unwrap();
    rig.claude_file(C1, &link.display().to_string(), "t");
    let (code, out) = rig.pty(&["resume", C1, "--no-sandbox", "--no-attach"], CONSENT);
    assert_eq!(code, 6, "{out}");
    assert!(out.contains("now resolves to"), "{out}");
    assert!(out.contains("link") && out.contains("work"), "{out}");
}

#[test]
fn log_reading_that_is_not_granted_refuses_with_the_consent_hint() {
    let rig = Rig::new();
    rig.claude_file(C1, &rig.work.display().to_string(), "t");
    rig.cmd(&["usage", "consent", "deny"]).output().unwrap();
    let (code, out) = rig.pty(&["resume", C1, "--no-sandbox", "--no-attach"], CONSENT);
    assert_eq!(code, 5, "{out}");
    assert!(out.contains("tmgr usage consent allow"), "{out}");
    assert!(!rig.run.path().join("runner.pid").exists());
}

#[test]
fn an_unknown_conversation_id_is_not_found() {
    let rig = Rig::new();
    let (code, out) = rig.pty(&["resume", C1, "--no-sandbox", "--no-attach"], CONSENT);
    assert_eq!(code, 3, "{out}");
    assert!(
        out.contains("no tmgr session or recorded conversation"),
        "{out}"
    );
}

#[test]
fn a_wrong_typed_confirmation_starts_nothing() {
    let rig = Rig::new();
    rig.claude_file(C1, &rig.work.display().to_string(), "t");
    let (code, _) = rig.pty(&["resume", C1, "--no-sandbox", "--no-attach"], "yes\n");
    assert_ne!(code, 0);
    let list = rig.cmd(&["--json", "runner", "list"]).output().unwrap();
    assert!(!String::from_utf8_lossy(&list.stdout).contains(C1));
}

#[test]
fn without_a_terminal_the_typed_consent_is_refused_and_nothing_starts() {
    let rig = Rig::new();
    rig.claude_file(C1, &rig.work.display().to_string(), "t");
    for flags in [["--no-sandbox", "--mode"], ["--no-sandbox", "--no-attach"]] {
        let mut c = rig.cmd(&["resume", C1]);
        c.args(flags);
        if flags[1] == "--mode" {
            c.arg("bypass");
        }
        let out = c.output().unwrap();
        assert_eq!(out.status.code(), Some(10), "{}", stderr(&out));
    }
    assert!(!rig.run.path().join("runner.pid").exists());
}

#[test]
fn resuming_a_conversation_is_refused_inside_an_agent_session() {
    let rig = Rig::new();
    rig.claude_file(C1, &rig.work.display().to_string(), "t");
    for key in ["TMGR_SESSION_ID", "TMGR_RUN_TOKEN", "TMGR_RUN_ID"] {
        let out = rig
            .cmd(&["resume", C1, "--no-sandbox"])
            .env(key, "x")
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(5), "{key}");
        assert!(stderr(&out).contains("agent session"));
    }
}

fn wait_exited(rig: &Rig, id: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let sessions = tmgr_runner::client::Client::connect(rig.run.path())
            .and_then(|mut c| c.list())
            .unwrap_or_default();
        if sessions
            .iter()
            .any(|s| s.id == id && s.state.kind == tmgr_ipc::api::StateKind::Exited)
        {
            return;
        }
        assert!(std::time::Instant::now() < deadline, "{id} did not exit");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_flagged_resume_of_a_tmgr_session_uses_its_recorded_conversation_not_the_output_tail() {
    let rig = Rig::new();
    rig.claude_file(C1, &rig.work.display().to_string(), "t");
    let other = "33333333-0000-4000-8000-000000000003";
    let (code, out) = rig.pty(
        &["--json", "resume", C1, "--no-sandbox", "--no-attach"],
        CONSENT,
    );
    assert_eq!(code, 0, "{out}");
    let first = json_line(&out)["data"]["id"].as_str().unwrap().to_string();
    wait_exited(&rig, &first);
    fs::write(
        rig.run
            .path()
            .join("sessions")
            .join(&first)
            .join("output.log"),
        format!("claude --resume {other}\n"),
    )
    .unwrap();
    let (code, out) = rig.pty(
        &["--json", "resume", &first, "--no-sandbox", "--no-attach"],
        CONSENT,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains(&format!("conversation  {C1}")), "{out}");
    let v = json_line(&out);
    assert_eq!(v["data"]["session"]["agent_session_id"], C1);
    assert_eq!(v["data"]["session"]["resumed_from"], first.as_str());
    let argv = v["data"]["session"]["argv"].to_string();
    assert!(argv.contains(C1) && !argv.contains(other), "{argv}");
    let (code, out) = rig.pty(&["resume", &first, "--no-sandbox", "--no-attach"], CONSENT);
    assert_ne!(code, 0, "{out}");
}

#[test]
fn a_flagged_resume_without_a_recorded_conversation_says_where_the_id_comes_from() {
    let rig = Rig::new();
    let (code, out) = rig.pty(
        &[
            "--json",
            "run",
            "--agent",
            "codex",
            "--cwd",
            &rig.work.display().to_string(),
            "--no-sandbox",
            "--no-attach",
        ],
        CONSENT,
    );
    assert_eq!(code, 0, "{out}");
    let first = json_line(&out)["data"]["id"].as_str().unwrap().to_string();
    wait_exited(&rig, &first);
    let (_, out) = rig.pty(&["resume", &first, "--no-sandbox", "--no-attach"], CONSENT);
    assert!(out.contains("taken from its recorded output"), "{out}");
}

#[test]
fn json_output_carries_no_control_or_bidi_characters_from_the_folder_name() {
    let rig = Rig::new();
    let odd = rig.work.parent().unwrap().join("w\u{9b}x\u{202e}y");
    fs::create_dir_all(&odd).unwrap();
    rig.claude_file(C1, &odd.display().to_string(), "t");
    let (code, out) = rig.pty(
        &["--json", "resume", C1, "--no-sandbox", "--no-attach"],
        CONSENT,
    );
    assert_eq!(code, 0, "{out:?}");
    assert!(
        !out.contains('\u{9b}') && !out.contains('\u{202e}'),
        "{out:?}"
    );
    assert!(json_line(&out)["data"]["cwd"]
        .as_str()
        .unwrap()
        .contains("w?x?y"));
}
