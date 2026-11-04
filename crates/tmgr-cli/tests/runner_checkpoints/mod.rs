use super::*;
use tmgr_runner::checkpoint::PAUSED_NOTE;

const TURNS: &str = "echo v1 > f.txt; echo ready1; read a\n\
    echo v2 > f.txt; echo ready2; read b\nexit 0\n";

impl Env {
    fn home(&self) -> PathBuf {
        let h = self.tmp.path().join("home");
        private_dir(&h);
        h
    }

    fn cp(&self, args: &[&str]) -> Command {
        let mut c = self.cmd(args);
        c.env_remove("TMGR_SESSION_ID")
            .env_remove("TMGR_RUN_TOKEN")
            .env_remove("TMGR_RUN_ID")
            .env("HOME", self.home())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        c
    }

    fn cp_out(&self, args: &[&str]) -> Output {
        run_bounded(self.cp(args), Duration::from_secs(60))
    }

    fn cp_data(&self, args: &[&str]) -> Value {
        let out = self.cp_out(args);
        let v: Value = serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| panic!("{args:?}: {e}: {}", String::from_utf8_lossy(&out.stdout)));
        assert_eq!(v["ok"], true, "{args:?}: {v}");
        v["data"].clone()
    }

    fn git_repo(&self) {
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["add", "-A"],
            vec![
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                "init",
            ],
        ] {
            let out = Command::new("git")
                .args(&args)
                .current_dir(&self.proj)
                .env("HOME", self.home())
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    fn run_cp(&self, script: &str, extra: &[&str]) -> Option<String> {
        self.run_cp_text(script, extra).map(|(id, _)| id)
    }

    fn run_cp_text(&self, script: &str, extra: &[&str]) -> Option<(String, String)> {
        if !sandbox_usable() {
            return None;
        }
        let cwd = self.proj.display().to_string();
        let mut args = vec![
            "run",
            "--agent-cmd",
            script,
            "--cwd",
            &cwd,
            "--no-attach",
            "--json",
        ];
        args.extend_from_slice(extra);
        let out = self.cp_out(&args);
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        if !out.status.success() && text.contains("sandbox unavailable") {
            eprintln!("skipped: {text}");
            return None;
        }
        assert!(
            out.status.success(),
            "{text}{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: Value = serde_json::from_str(&text).unwrap();
        let id = v["data"]["id"].as_str().unwrap().to_string();
        Some((id, String::from_utf8_lossy(&out.stderr).into_owned()))
    }

    fn wait_waiting(&self, id: &str) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if self
                .sessions()
                .iter()
                .any(|s| s["id"] == id && s["state"]["kind"] == "waiting")
            {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!("session never waited");
    }
}

#[test]
fn restore_is_refused_inside_any_session() {
    let e = Env::new();
    for key in ["TMGR_SESSION_ID", "TMGR_RUN_TOKEN", "TMGR_RUN_ID"] {
        let out = e
            .cp(&["checkpoints", "restore", "s_aaaaaaaaaa", "1", "--yes"])
            .env(key, "x")
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(5), "{key}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("agent session"));
    }
}

#[test]
fn restore_without_a_terminal_or_yes_is_a_usage_error_before_the_runner_is_touched() {
    let e = Env::new();
    let out = e
        .cp(&["checkpoints", "restore", "s_aaaaaaaaaa", "1"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--yes"));
    assert!(!e.run.join("runner.pid").exists());
}

#[test]
fn the_flags_and_the_command_are_listed_and_conflict_with_each_other() {
    let e = Env::new();
    let top = e.cp(&["--help"]).output().unwrap();
    assert!(String::from_utf8_lossy(&top.stdout).contains("checkpoints"));
    let run = e.cp(&["run", "--help"]).output().unwrap();
    let text = String::from_utf8_lossy(&run.stdout);
    assert!(text.contains("--checkpoints") && text.contains("--no-checkpoints"));
    let both = e
        .cp(&[
            "run",
            "--agent-cmd",
            "x",
            "--checkpoints",
            "--no-checkpoints",
        ])
        .output()
        .unwrap();
    assert_eq!(both.status.code(), Some(2));
}

#[test]
fn list_of_a_session_without_checkpoints_says_they_are_off() {
    let e = Env::new();
    e.git_repo();
    let script = e.script("agent.sh", TURNS);
    let Some(id) = e.run_cp(&script, &[]) else {
        return;
    };
    let out = e.cp_out(&["checkpoints", "list", &id]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("Checkpoints are off for this session."),
        "{text}"
    );
    let d = e.cp_data(&["checkpoints", "list", &id, "--json"]);
    assert_eq!(d["enabled"], false);
    let out = e.cp_out(&["checkpoints", "diff", &id, "1"]);
    assert_eq!(out.status.code(), Some(2));
    let refs = Command::new("git")
        .args(["for-each-ref", "refs/tmgr"])
        .current_dir(&e.proj)
        .output()
        .unwrap();
    assert!(refs.stdout.is_empty());
}

#[test]
fn a_sandboxed_session_with_checkpoints_starts_and_says_they_are_paused() {
    let e = Env::new();
    e.git_repo();
    fs::write(e.proj.join("f.txt"), "v1\n").unwrap();
    let script = e.script("agent.sh", TURNS);
    let Some((id, stderr)) = e.run_cp_text(&script, &["--checkpoints"]) else {
        return;
    };
    assert!(stderr.contains(PAUSED_NOTE), "{stderr}");
    e.wait_waiting(&id);
    let human = e.cp_out(&["checkpoints", "list", &id]);
    assert!(human.status.success());
    assert!(String::from_utf8_lossy(&human.stdout).contains(PAUSED_NOTE));
    let d = e.cp_data(&["checkpoints", "list", &id, "--json"]);
    assert_eq!(d["status"]["note"], PAUSED_NOTE);
    assert!(d["items"].as_array().unwrap().is_empty());
    for args in [
        vec!["checkpoints", "diff", &id, "1"],
        vec!["checkpoints", "restore", &id, "1", "--yes"],
    ] {
        let out = e.cp_out(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", out_text(&out));
        assert!(out_text(&out).contains(PAUSED_NOTE), "{args:?}");
    }
    assert_eq!(fs::read_to_string(e.proj.join("f.txt")).unwrap(), "v1\n");
    let refs = Command::new("git")
        .args(["for-each-ref", "refs/tmgr"])
        .current_dir(&e.proj)
        .output()
        .unwrap();
    assert!(refs.stdout.is_empty());
}

#[test]
fn the_config_turns_checkpoints_on_and_no_checkpoints_overrides_it() {
    let e = Env::new();
    e.git_repo();
    let script = e.script("agent.sh", TURNS);
    if !sandbox_usable() {
        return;
    }
    let set = e.cp_out(&["config", "set", "attach.checkpoints", "true"]);
    assert!(
        set.status.success(),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );
    let Some(on) = e.run_cp(&script, &[]) else {
        return;
    };
    e.wait_waiting(&on);
    assert_eq!(
        e.cp_data(&["checkpoints", "list", &on, "--json"])["enabled"],
        true
    );
    let off = e.run_cp(&script, &["--no-checkpoints"]).unwrap();
    assert_eq!(
        e.cp_data(&["checkpoints", "list", &off, "--json"])["enabled"],
        false
    );
}
