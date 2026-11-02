#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tmgr_ipc::api::{
    NoSandbox, PermissionAnswer, PermissionCard, PermissionChoice, RunCreateReq, SessionInfo,
    StateKind,
};
use tmgr_runner::client::Client;
use tmgr_runner::permit_hook;

const HOST: &str = env!("CARGO_BIN_EXE_tmgr-runner-host");

struct Fx {
    tmp: tempfile::TempDir,
    run: PathBuf,
    homes: PathBuf,
    work: PathBuf,
    agents: PathBuf,
    cfg: PathBuf,
    home: PathBuf,
    runner: Option<Child>,
    clock: std::cell::Cell<u64>,
}

impl Fx {
    fn new() -> Fx {
        let tmp = tempfile::Builder::new()
            .prefix("pc")
            .tempdir_in("/tmp")
            .unwrap();
        fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let base = fs::canonicalize(tmp.path()).unwrap();
        let dir = |name: &str| {
            let p = base.join(name);
            fs::create_dir(&p).unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
            p
        };
        let (run, homes, work, agents, cfg, home) = (
            dir("r"),
            dir("h"),
            dir("w"),
            dir("a"),
            dir("c"),
            dir("realhome"),
        );
        fs::create_dir(cfg.join("tmgr")).unwrap();
        fs::set_permissions(cfg.join("tmgr"), fs::Permissions::from_mode(0o700)).unwrap();
        let bin = work.join("bin");
        fs::create_dir(&bin).unwrap();
        for agent in ["claude", "codex", "plain"] {
            let p = bin.join(agent);
            fs::write(
                &p,
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$HOME/argv.txt\"\nsleep 60\n",
            )
            .unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let plain = bin.join("plain");
        fs::write(
            agents.join("plain.toml"),
            format!(
                "id = \"plain\"\nname = \"plain\"\ncommand = [{:?}]\n",
                plain.display().to_string()
            ),
        )
        .unwrap();
        Fx {
            tmp,
            run,
            homes,
            work,
            agents,
            cfg,
            home,
            runner: None,
            clock: std::cell::Cell::new(1_000_000),
        }
    }

    fn start(&mut self) {
        let err = fs::File::create(self.tmp.path().join("runner.err")).unwrap();
        let path = format!("{}:/usr/bin:/bin", self.work.join("bin").display());
        self.runner = Some(
            Command::new(HOST)
                .args(["runner", "serve"])
                .env("HOME", &self.home)
                .env("PATH", path)
                .env("TMGR_RUNNER_DIR", &self.run)
                .env("TMGR_RUNNER_HOMES", &self.homes)
                .env("TMGR_AGENTS_DIR", &self.agents)
                .env("XDG_CONFIG_HOME", &self.cfg)
                .env("TMGR_T0_QUIET_MS", "5000")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(err)
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while let Err(e) = Client::connect(&self.run) {
            assert!(
                Instant::now() < deadline,
                "runner did not start: {e}; {}",
                fs::read_to_string(self.tmp.path().join("runner.err")).unwrap_or_default()
            );
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn kill_runner(&mut self) {
        if let Some(mut r) = self.runner.take() {
            let _ = r.kill();
            let _ = r.wait();
        }
    }

    fn client(&self) -> Client {
        let c = Client::connect(&self.run).unwrap();
        c.set_timeouts(Duration::from_secs(60)).unwrap();
        c
    }

    fn req(&self, profile: &str, cards: bool) -> RunCreateReq {
        RunCreateReq {
            clones: Vec::new(),
            profile: profile.into(),
            task: None,
            cwd: self.work.display().to_string(),
            name: None,
            model: None,
            cols: 80,
            rows: 24,
            bundle: None,
            bundle_sha256: None,
            cwd_pin: None,
            origin: None,
            permission_mode: None,
            start_key: None,
            clone_of: None,
            trust_cwd: None,
            trust_via_repo: None,
            agent_work: false,
            cloud: None,
            unattended: false,
            exe_pin: None,
            argv_sha256: None,
            no_sandbox: None,
            permission_cards: cards,
        }
    }

    fn unsandboxed(&self, mut r: RunCreateReq) -> RunCreateReq {
        let path = format!("{}:/usr/bin:/bin", self.work.join("bin").display());
        r.no_sandbox = Some(NoSandbox {
            env: [("PATH".to_string(), path)].into_iter().collect(),
        });
        r
    }

    fn run_home(&self, id: &str) -> PathBuf {
        self.homes.join(id)
    }
}

impl Drop for Fx {
    fn drop(&mut self) {
        if self.runner.is_none() && Client::connect(&self.run).is_err() {
            self.start();
        }
        if let Ok(mut c) = Client::connect(&self.run) {
            for s in c.list().unwrap_or_default() {
                let _ = c.kill(&s.id, "kill");
            }
            let _ = c.stop();
        }
        thread::sleep(Duration::from_millis(200));
        self.kill_runner();
    }
}

fn sessions_available() -> bool {
    tmgr_runner::daemon::landlock_abi() >= 6
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(Instant::now() < deadline, "{what}");
        thread::sleep(Duration::from_millis(50));
    }
}

fn argv_of(home: &Path) -> String {
    let p = home.join("argv.txt");
    wait_for("agent argv", || p.exists());
    thread::sleep(Duration::from_millis(100));
    fs::read_to_string(p).unwrap()
}

fn info_of(c: &mut Client, id: &str) -> SessionInfo {
    c.list().unwrap().into_iter().find(|s| s.id == id).unwrap()
}

fn listing(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn off_by_default_leaves_argv_and_home_untouched() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    fx.start();
    for profile in ["claude-code", "codex"] {
        let info = fx.client().create_run(fx.req(profile, false)).unwrap();
        assert!(!info.permission_cards);
        let home = fx.run_home(&info.id);
        let argv = argv_of(&home);
        assert!(
            !argv.contains("--settings") && !argv.contains("permission-hook"),
            "{argv}"
        );
        assert!(!home.join(".codex/config.toml").exists());
        assert!(!home.join(".cache/tmgr-permit").exists());
        let _ = fx.client().kill(&info.id, "kill");
    }
}

#[test]
fn claude_gets_one_additive_settings_argument_and_the_flag_persists() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    fx.start();
    let info = fx.client().create_run(fx.req("claude-code", true)).unwrap();
    assert!(info.permission_cards);
    let argv = argv_of(&fx.run_home(&info.id));
    let lines: Vec<&str> = argv.lines().collect();
    let at = lines.iter().position(|l| *l == "--settings").expect(&argv);
    let json: serde_json::Value = serde_json::from_str(lines[at + 1]).unwrap();
    let command = json["hooks"]["PermissionRequest"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    assert!(command.ends_with(" __permission-hook claude"), "{command}");
    assert!(command.contains("tmgr-runner-host"), "{command}");
    assert_eq!(lines.iter().filter(|l| **l == "--settings").count(), 1);
    assert!(fs::read_dir(fx.home.join(".claude")).is_err());

    fx.kill_runner();
    fx.start();
    assert!(info_of(&mut fx.client(), &info.id).permission_cards);
}

#[test]
fn the_flag_and_the_hook_carry_through_resume() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    fx.start();
    let info = fx.client().create_run(fx.req("claude-code", true)).unwrap();
    argv_of(&fx.run_home(&info.id));
    let _ = fx.client().kill(&info.id, "kill");
    wait_for("exit", || {
        info_of(&mut fx.client(), &info.id).state.kind == StateKind::Exited
    });
    let resumed = fx.client().resume(&info.id, 80, 24).unwrap();
    assert!(resumed.session.permission_cards);
    let argv = argv_of(&fx.run_home(&resumed.session.id));
    assert!(argv.contains("--settings"), "{argv}");
    assert!(argv.contains("--continue"), "{argv}");
}

#[test]
fn claude_without_the_sandbox_still_gets_the_settings_argument() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    fx.start();
    let req = fx.unsandboxed(fx.req("claude-code", true));
    let info = fx.client().create_run(req).unwrap();
    assert!(info.no_sandbox && info.permission_cards);
    assert!(argv_of(&fx.home).contains("--settings"));
}

#[test]
fn sandboxed_codex_gets_a_trusted_hook_in_its_run_home() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    fx.start();
    let info = fx.client().create_run(fx.req("codex", true)).unwrap();
    assert!(info.permission_cards);
    let home = fx.run_home(&info.id);
    let argv = argv_of(&home);
    assert!(!argv.contains("--settings"), "{argv}");
    let text = fs::read_to_string(home.join(".codex/config.toml")).unwrap();
    let v: toml::Value = toml::from_str(&text).unwrap();
    let command = v["hooks"]["PermissionRequest"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    assert!(command.ends_with(" __permission-hook codex"), "{command}");
    assert_eq!(v["features"]["hooks"].as_bool(), Some(true));
    let key = format!(
        "{}:permission_request:0:0",
        fs::canonicalize(&home)
            .unwrap()
            .join(".codex/config.toml")
            .display()
    );
    assert_eq!(
        v["hooks"]["state"][key.as_str()]["trusted_hash"].as_str(),
        Some(permit_hook::trust_hash(command).as_str())
    );
    let mode = fs::metadata(home.join(".codex/config.toml"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o077, 0);
}

#[test]
fn codex_without_the_sandbox_writes_nothing_anywhere() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    fx.start();
    let req = fx.unsandboxed(fx.req("codex", true));
    let info = fx.client().create_run(req).unwrap();
    assert!(info.no_sandbox && info.permission_cards);
    let argv = argv_of(&fx.home);
    assert!(
        !argv.contains("--settings") && !argv.contains("permission-hook"),
        "{argv}"
    );
    assert_eq!(listing(&fx.home), ["argv.txt"]);
    assert!(!fx.run_home(&info.id).join(".codex").exists());
}

#[test]
fn non_agent_profiles_ignore_the_flag() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    fx.start();
    let info = fx.client().create_run(fx.req("plain", true)).unwrap();
    assert!(!info.permission_cards);
    let argv = argv_of(&fx.run_home(&info.id));
    assert!(!argv.contains("--settings"), "{argv}");
}

#[test]
fn only_a_manual_cli_run_may_ask_for_cards() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    fx.start();
    let mut desktop = Client::connect_as(&fx.run, "desktop").unwrap();
    let e = desktop
        .create_run(fx.req("claude-code", true))
        .unwrap_err()
        .to_string();
    assert!(e.contains("forbidden"), "{e}");
    let mut keyed = fx.req("claude-code", true);
    keyed.start_key = Some("k1".into());
    let e = fx.client().create_run(keyed).unwrap_err().to_string();
    assert!(e.contains("forbidden"), "{e}");
    assert!(fx.client().list().unwrap().is_empty());
}

fn run_hook(home: &Path, session: Option<&str>, stdin: &[u8]) -> std::process::Output {
    let mut cmd = Command::new(HOST);
    cmd.args(["__permission-hook", "claude"])
        .env_clear()
        .env("HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(s) = session {
        cmd.env("TMGR_SESSION_ID", s);
    }
    let mut child = cmd.spawn().unwrap();
    let _ = child.stdin.take().unwrap().write_all(stdin);
    child.wait_with_output().unwrap()
}

#[test]
fn the_hook_command_prints_nothing_and_exits_zero() {
    let tmp = tempfile::Builder::new()
        .prefix("ph")
        .tempdir_in("/tmp")
        .unwrap();
    let home = fs::canonicalize(tmp.path()).unwrap();
    let ok = run_hook(&home, Some("s_abc"), br#"{"tool_name":"Bash"}"#);
    assert!(ok.status.success() && ok.stdout.is_empty() && ok.stderr.is_empty());
    let spooled = listing(&permit_hook::spool_dir(&home, "s_abc"));
    assert_eq!(spooled.len(), 1);
    for bad in [
        run_hook(&home, Some("s_abc"), b"\xff garbage"),
        run_hook(&home, Some("../x"), b"{}"),
        run_hook(&home, None, b"{}"),
    ] {
        assert!(bad.status.success() && bad.stdout.is_empty());
    }
    assert_eq!(listing(&permit_hook::spool_dir(&home, "s_abc")), spooled);
    assert_eq!(listing(&home), [".cache"]);
    let other = Command::new(HOST)
        .args(["__permission-hook", "nonsense"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(other.status.success() && other.stdout.is_empty());
}

const FAKE_AGENT: &str = r#"#!/bin/sh
W="$PWD"
stty -icanon -echo min 1 time 0
( while :; do
  if [ -f "$W/cmd.sh" ]; then mv "$W/cmd.sh" "$W/run.sh"; . "$W/run.sh"; echo x >> "$W/ran"; fi
  sleep 0.05
done ) &
while :; do
  b=$(dd bs=1 count=1 2>/dev/null | od -An -tx1)
  [ -n "$b" ] && echo $b >> "$W/keys"
done
"#;

const CODEX_SCREEN: &str = include_str!("../profiles/screens/codex-0164-approval.txt");
const WIDE: u16 = 120;
const TALL: u16 = 100;
const PLAIN_OPTS: [&str; 3] = [
    " ❯ 1. Yes",
    "   2. Yes, and always allow access to /work/proj from this project",
    "   3. No",
];
const RULE_OPTS: [&str; 3] = [
    " ❯ 1. Yes",
    "   2. Yes, and don't ask again for: rm:*",
    "   3. No",
];

fn claude_screen(command: &str, opts: &[&str]) -> String {
    let rule = |c: &str| c.repeat(100);
    let mut rows = vec![
        "transcript".to_string(),
        rule("─"),
        " Bash command".into(),
        " run it".into(),
        rule("╌"),
    ];
    for line in command.split('\n') {
        let chars: Vec<char> = line.chars().collect();
        for chunk in chars.chunks(98) {
            rows.push(format!(" {}", chunk.iter().collect::<String>()));
        }
    }
    rows.push(rule("╌"));
    rows.push(" Do you want to proceed?".into());
    rows.extend(opts.iter().map(|o| o.to_string()));
    rows.push(String::new());
    rows.push(" Esc to cancel · Tab to amend".into());
    rows.join("\n") + "\n"
}

fn bash_hook_json(command: &str, rule: bool) -> String {
    let mut v = serde_json::json!({
        "tool_name": "Bash",
        "tool_input": {"command": command, "description": "run it"},
        "cwd": "/work/proj",
    });
    if rule {
        v["permission_suggestions"] = serde_json::json!([
            {"type": "addRules", "rules": [{"toolName": "Bash", "ruleContent": "x:*"}]}
        ]);
    }
    v.to_string()
}

fn codex_json() -> String {
    serde_json::json!({
        "tool_name": "Bash",
        "tool_input": {"command": "touch hello.txt", "description": "needs outside sandbox"},
        "cwd": "/work/cx/proj",
    })
    .to_string()
}

fn long_command() -> String {
    let mut lines: Vec<String> = (0..50)
        .map(|i| format!("echo step-{i:03} {}", "x".repeat(70)))
        .collect();
    lines.push(format!("echo {}", "word ".repeat(50).trim_end()));
    lines.join("\n")
}

struct Agent {
    id: String,
    home: PathBuf,
    profile: &'static str,
}

impl Fx {
    fn agent(&mut self, profile: &'static str, cards: bool, sandboxed: bool) -> Agent {
        let command = if profile == "claude-code" {
            "claude"
        } else {
            profile
        };
        let p = self.work.join("bin").join(command);
        fs::write(&p, FAKE_AGENT).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        if self.runner.is_none() {
            self.start();
        }
        let mut req = self.req(profile, cards);
        (req.cols, req.rows) = (WIDE, TALL);
        if !sandboxed {
            req = self.unsandboxed(req);
        }
        let info = self.client().create_run(req).unwrap();
        let home = if sandboxed {
            self.run_home(&info.id)
        } else {
            self.home.clone()
        };
        Agent {
            id: info.id,
            home,
            profile,
        }
    }

    fn lines(&self, name: &str) -> Vec<String> {
        fs::read_to_string(self.work.join(name))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    fn script(&self, body: &str) -> usize {
        let before = self.lines("ran").len();
        let tmp = self.work.join("cmd.tmp");
        fs::write(&tmp, body).unwrap();
        fs::rename(&tmp, self.work.join("cmd.sh")).unwrap();
        before
    }

    fn run(&self, body: &str) {
        let before = self.script(body);
        wait_for("agent script", || self.lines("ran").len() > before);
    }

    fn paint(&self, screen: &str) {
        fs::write(self.work.join("screen.txt"), screen).unwrap();
        self.run("printf '\\033[2J\\033[H'; cat \"$W/screen.txt\"");
    }

    fn hook(&self, a: &Agent, json: &str) {
        let n = self.clock.get() + 1000;
        self.clock.set(n);
        let name = if a.profile == "codex" {
            "codex"
        } else {
            "claude"
        };
        permit_hook::spool(&a.home, &a.id, name, &mut json.as_bytes(), n, 7).unwrap();
    }

    fn cards(&self, a: &Agent) -> Vec<PermissionCard> {
        let all = self.client().permissions().unwrap();
        all.into_iter().filter(|c| c.session == a.id).collect()
    }

    fn wait_card(
        &self,
        a: &Agent,
        what: &str,
        ok: impl Fn(&PermissionCard) -> bool,
    ) -> PermissionCard {
        let mut found = None;
        wait_for(what, || {
            found = self.cards(a).into_iter().find(&ok);
            found.is_some()
        });
        found.unwrap()
    }

    fn armed(&self, a: &Agent) -> PermissionCard {
        self.wait_card(a, "armed answerable card", |c| {
            c.can_answer && c.armed_in_ms == 0
        })
    }

    fn answer(
        &self,
        a: &Agent,
        card: &PermissionCard,
        choice: PermissionChoice,
        confirmed: bool,
    ) -> std::io::Result<()> {
        self.client().answer_permission(&PermissionAnswer {
            session: a.id.clone(),
            card: card.id.clone(),
            choice,
            confirmed,
        })
    }

    fn keys_after_marker(&self, a: &Agent) -> Vec<String> {
        let markers = |keys: &[String]| keys.iter().filter(|k| *k == "5a").count();
        let before = markers(&self.lines("keys"));
        self.client().send(&a.id, "Z").unwrap();
        wait_for("marker", || markers(&self.lines("keys")) > before);
        let mut keys = self.lines("keys");
        keys.retain(|k| k != "5a");
        keys
    }
}

fn refusal(r: std::io::Result<()>, why: &str) {
    let e = r.expect_err("the answer must be refused").to_string();
    assert!(e.contains("forbidden") && e.contains(why), "{e}");
}

fn unknown_card() -> PermissionCard {
    PermissionCard {
        id: "0".repeat(64),
        ..PermissionCard::default()
    }
}

#[test]
fn a_matching_hook_and_dialog_gives_the_full_text_and_yes_sends_exactly_one_key() {
    if !sessions_available() {
        return;
    }
    for sandboxed in [false, true] {
        let mut fx = Fx::new();
        let a = fx.agent("claude-code", true, sandboxed);
        let command = long_command();
        assert!(command.len() > 4000);
        fx.hook(&a, &bash_hook_json(&command, false));
        fx.paint(&claude_screen(&command, &PLAIN_OPTS));
        let card = fx.armed(&a);
        let solid = |t: &str| t.split_whitespace().collect::<String>();
        assert_eq!(solid(&card.text), solid(&command));
        assert_eq!(card.tool, "Bash");
        assert_eq!(card.agent, "claude");
        assert_eq!(card.reason.as_deref(), Some("run it"));
        assert!(card.danger.is_empty() && card.always_label.is_none());
        fx.answer(&a, &card, PermissionChoice::Yes, false).unwrap();
        assert_eq!(fx.keys_after_marker(&a), ["31"]);
    }
}

#[test]
fn no_sends_the_parsed_no_key() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    let a = fx.agent("claude-code", true, false);
    fx.hook(&a, &bash_hook_json("ls", false));
    fx.paint(&claude_screen("ls", &PLAIN_OPTS));
    let card = fx.armed(&a);
    fx.answer(&a, &card, PermissionChoice::No, false).unwrap();
    assert_eq!(fx.keys_after_marker(&a), ["33"]);
}

#[test]
fn codex_yes_is_y_no_is_escape_and_always_is_p() {
    if !sessions_available() {
        return;
    }
    for (choice, want) in [
        (PermissionChoice::Yes, "79"),
        (PermissionChoice::No, "1b"),
        (PermissionChoice::Always, "70"),
    ] {
        let mut fx = Fx::new();
        let a = fx.agent("codex", true, false);
        fx.hook(&a, &codex_json());
        fx.paint(CODEX_SCREEN);
        let card = fx.armed(&a);
        assert_eq!(card.agent, "codex");
        assert!(card
            .always_label
            .as_deref()
            .is_some_and(|l| l.contains("touch hello.txt")));
        fx.answer(&a, &card, choice, false).unwrap();
        assert_eq!(fx.keys_after_marker(&a), [want]);
    }
}

#[test]
fn a_prompt_that_went_away_before_the_answer_gets_no_keys() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    let a = fx.agent("claude-code", true, false);
    fx.hook(&a, &bash_hook_json("ls", false));
    fx.paint(&claude_screen("ls", &PLAIN_OPTS));
    let card = fx.armed(&a);
    fx.paint("all done\n");
    refusal(
        fx.answer(&a, &card, PermissionChoice::Yes, false),
        "no longer on screen",
    );
    assert!(fx.keys_after_marker(&a).is_empty());
}

#[test]
fn a_prompt_that_changed_before_the_answer_gets_no_keys() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    let a = fx.agent("claude-code", true, false);
    fx.hook(&a, &bash_hook_json("ls", false));
    fx.paint(&claude_screen("ls", &PLAIN_OPTS));
    let card = fx.armed(&a);
    fx.hook(&a, &bash_hook_json("cat secrets", false));
    fx.paint(&claude_screen("cat secrets", &PLAIN_OPTS));
    fx.wait_card(&a, "the new prompt", |c| c.text == "cat secrets");
    refusal(
        fx.answer(&a, &card, PermissionChoice::Yes, false),
        "changed",
    );
    assert!(fx.keys_after_marker(&a).is_empty());
}

#[test]
fn a_newer_hook_with_the_old_dialog_still_on_screen_gets_no_keys() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    let a = fx.agent("claude-code", true, false);
    fx.hook(&a, &bash_hook_json("ls", false));
    fx.paint(&claude_screen("ls", &PLAIN_OPTS));
    let card = fx.armed(&a);
    fx.hook(&a, &bash_hook_json("ls", false));
    refusal(
        fx.answer(&a, &card, PermissionChoice::Yes, false),
        "changed",
    );
    assert!(fx.keys_after_marker(&a).is_empty());
}

#[test]
fn a_look_alike_dialog_without_a_hook_is_view_only() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    let a = fx.agent("claude-code", true, false);
    fx.paint(&claude_screen("ls", &PLAIN_OPTS));
    let card = fx.wait_card(&a, "view-only card", |c| c.text == "ls");
    assert!(!card.can_answer);
    assert!(card
        .why_not
        .as_deref()
        .is_some_and(|w| w.contains("no hook")));
    refusal(
        fx.answer(&a, &card, PermissionChoice::Yes, false),
        "no hook",
    );
    assert!(fx.keys_after_marker(&a).is_empty());
}

#[test]
fn a_forged_hook_with_an_idle_prompt_below_the_dialog_gives_no_approvable_card() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    let a = fx.agent("claude-code", true, false);
    fx.hook(&a, &bash_hook_json("ls", false));
    fx.paint(&(claude_screen("ls", &PLAIN_OPTS) + "\n❯ \n"));
    wait_for("a settled screen", || {
        fx.client()
            .text(&a.id, None)
            .is_ok_and(|t| t.lines.iter().any(|l| l.contains('❯') && l.trim() == "❯"))
    });
    assert!(fx.cards(&a).iter().all(|c| !c.can_answer));
    refusal(
        fx.answer(&a, &unknown_card(), PermissionChoice::Yes, false),
        "",
    );
    assert!(fx.keys_after_marker(&a).is_empty());
}

#[test]
fn a_foreground_process_other_than_the_agent_gets_no_keys() {
    if !sessions_available() || Command::new("python3").arg("--version").output().is_err() {
        return;
    }
    let mut fx = Fx::new();
    let a = fx.agent("claude-code", true, false);
    fx.hook(&a, &bash_hook_json("ls", false));
    fs::write(fx.work.join("screen.txt"), claude_screen("ls", &PLAIN_OPTS)).unwrap();
    fs::write(
        fx.work.join("fg.py"),
        "import os, signal, sys, time\n\
         signal.signal(signal.SIGTTOU, signal.SIG_IGN)\n\
         os.setpgid(0, 0)\n\
         os.tcsetpgrp(1, os.getpgrp())\n\
         sys.stdout.write(open('screen.txt').read())\n\
         sys.stdout.flush()\n\
         time.sleep(60)\n",
    )
    .unwrap();
    fx.script("python3 \"$W/fg.py\"");
    let card = fx.wait_card(&a, "foreground card", |c| {
        c.why_not.as_deref() == Some("another process is in the foreground")
    });
    assert!(!card.can_answer);
    refusal(
        fx.answer(&a, &card, PermissionChoice::Yes, false),
        "foreground",
    );
    thread::sleep(Duration::from_millis(500));
    assert!(fx.lines("keys").is_empty());
}

#[test]
fn plain_profiles_and_sessions_with_the_flag_off_have_no_cards() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    let off = fx.agent("claude-code", false, false);
    let plain = fx.agent("plain", true, false);
    fx.hook(&off, &bash_hook_json("ls", false));
    fx.paint(&claude_screen("ls", &PLAIN_OPTS));
    thread::sleep(Duration::from_millis(600));
    assert!(fx.client().permissions().unwrap().is_empty());
    refusal(
        fx.answer(&off, &unknown_card(), PermissionChoice::Yes, false),
        "off",
    );
    refusal(
        fx.answer(&plain, &unknown_card(), PermissionChoice::Yes, false),
        "off",
    );
    assert!(fx.lines("keys").is_empty());
}

#[test]
fn a_dangerous_command_needs_confirmation_and_never_gets_always() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    let a = fx.agent("claude-code", true, false);
    fx.hook(&a, &bash_hook_json("rm build", true));
    fx.paint(&claude_screen("rm build", &RULE_OPTS));
    let card = fx.armed(&a);
    assert!(!card.danger.is_empty());
    assert!(card.always_label.is_none());
    refusal(
        fx.answer(&a, &card, PermissionChoice::Yes, false),
        "confirmation",
    );
    refusal(
        fx.answer(&a, &card, PermissionChoice::Always, true),
        "always",
    );
    refusal(
        fx.answer(&a, &card, PermissionChoice::Always, false),
        "always",
    );
    assert!(fx.keys_after_marker(&a).is_empty());
    fx.answer(&a, &card, PermissionChoice::Yes, true).unwrap();
    assert_eq!(fx.keys_after_marker(&a), ["31"]);
}

#[test]
fn a_safe_command_with_a_command_rule_offers_always() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    let a = fx.agent("claude-code", true, false);
    fx.hook(&a, &bash_hook_json("touch a", true));
    let opts = [
        " ❯ 1. Yes",
        "   2. Yes, and don't ask again for: touch:*",
        "   3. No",
    ];
    fx.paint(&claude_screen("touch a", &opts));
    let card = fx.armed(&a);
    assert_eq!(
        card.always_label.as_deref(),
        Some("Yes, and don't ask again for: touch:*")
    );
    fx.answer(&a, &card, PermissionChoice::Always, false)
        .unwrap();
    assert_eq!(fx.keys_after_marker(&a), ["32"]);
}

#[test]
fn a_second_answer_sends_nothing_and_the_card_shows_answered() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    let a = fx.agent("claude-code", true, false);
    fx.hook(&a, &bash_hook_json("ls", false));
    fx.paint(&claude_screen("ls", &PLAIN_OPTS));
    let card = fx.armed(&a);
    fx.answer(&a, &card, PermissionChoice::Yes, false).unwrap();
    refusal(
        fx.answer(&a, &card, PermissionChoice::Yes, false),
        "already answered",
    );
    refusal(
        fx.answer(&a, &card, PermissionChoice::No, false),
        "already answered",
    );
    let shown = fx.wait_card(&a, "answered card", |c| c.id == card.id);
    assert!(!shown.can_answer);
    assert_eq!(shown.why_not.as_deref(), Some("answered"));
    assert_eq!(fx.keys_after_marker(&a), ["31"]);
    assert!(listing(&permit_hook::spool_dir(&a.home, &a.id)).is_empty());
}

#[test]
fn a_repeated_identical_dialog_needs_its_own_hook() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    let a = fx.agent("claude-code", true, false);
    fx.hook(&a, &bash_hook_json("ls", false));
    fx.paint(&claude_screen("ls", &PLAIN_OPTS));
    let first = fx.armed(&a);
    fx.answer(&a, &first, PermissionChoice::Yes, false).unwrap();
    fx.paint("running\n");
    wait_for("the card to go", || fx.cards(&a).is_empty());
    fx.paint(&claude_screen("ls", &PLAIN_OPTS));
    let again = fx.wait_card(&a, "view-only repeat", |c| c.text == "ls");
    assert!(!again.can_answer && again.id != first.id, "{again:?}");
    refusal(fx.answer(&a, &again, PermissionChoice::Yes, false), "hook");
    fx.hook(&a, &bash_hook_json("ls", false));
    let fresh = fx.armed(&a);
    assert_ne!(fresh.id, first.id);
    fx.answer(&a, &fresh, PermissionChoice::Yes, false).unwrap();
    assert_eq!(fx.keys_after_marker(&a), ["31", "31"]);
}

#[test]
fn the_card_disappears_when_the_agent_continues() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    let a = fx.agent("claude-code", true, false);
    fx.hook(&a, &bash_hook_json("ls", false));
    fx.paint(&claude_screen("ls", &PLAIN_OPTS));
    fx.armed(&a);
    fx.paint("continuing\n");
    wait_for("the card to go", || fx.cards(&a).is_empty());
}

#[test]
fn only_the_cli_client_may_list_or_answer() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    let a = fx.agent("claude-code", true, false);
    fx.hook(&a, &bash_hook_json("ls", false));
    fx.paint(&claude_screen("ls", &PLAIN_OPTS));
    let card = fx.armed(&a);
    for client in ["worker", "desktop"] {
        let mut c = Client::connect_as(&fx.run, client).unwrap();
        let e = c.permissions().unwrap_err().to_string();
        assert!(e.contains("forbidden"), "{client}: {e}");
        let e = c
            .answer_permission(&PermissionAnswer {
                session: a.id.clone(),
                card: card.id.clone(),
                choice: PermissionChoice::Yes,
                confirmed: false,
            })
            .unwrap_err()
            .to_string();
        assert!(e.contains("forbidden"), "{client}: {e}");
    }
    assert!(fx.keys_after_marker(&a).is_empty());
}

#[test]
fn an_answer_inside_the_arming_window_is_refused() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    let a = fx.agent("claude-code", true, false);
    fx.hook(&a, &bash_hook_json("ls", false));
    fx.paint(&claude_screen("ls", &PLAIN_OPTS));
    fx.armed(&a);
    fx.hook(&a, &bash_hook_json("pwd", false));
    fx.paint(&claude_screen("pwd", &PLAIN_OPTS));
    let first = fx.wait_card(&a, "first sight", |c| c.text == "pwd" && c.can_answer);
    assert!(first.armed_in_ms > 0, "{first:?}");
    refusal(
        fx.answer(&a, &first, PermissionChoice::Yes, false),
        "just shown",
    );
    assert!(fx.keys_after_marker(&a).is_empty());
    let armed = fx.armed(&a);
    fx.answer(&a, &armed, PermissionChoice::Yes, false).unwrap();
    assert_eq!(fx.keys_after_marker(&a), ["31"]);
}
