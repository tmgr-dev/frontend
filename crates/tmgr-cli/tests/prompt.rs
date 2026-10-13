mod common;
use common::*;
use serde_json::json;

fn active(started: Option<i64>, key: &str) -> serde_json::Value {
    json!({
        "schema": "tmgr.active.v1",
        "task": {"id": 8974, "key": key, "title": "Checkout"},
        "timer_started_at": started.map(chrono_like),
    })
}

fn chrono_like(epoch: i64) -> String {
    chrono::DateTime::from_timestamp(epoch, 0)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn prompt(env: &Env, args: &[&str]) -> std::process::Output {
    env.cmd().arg("prompt").args(args).output().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn prompt_prints_nothing_without_an_active_task_and_never_calls_the_network() {
    let env = Env::new().await;
    let out = env.cmd_for(&dead_server()).arg("prompt").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
    assert!(out.stderr.is_empty());
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn prompt_shows_the_key_and_elapsed_time() {
    let env = Env::new().await;
    env.seed_active(active(Some(now_s() - 4320), "TM-512"));
    let out = prompt(&env, &[]);
    let s = text(&out.stdout);
    assert!(
        s == "\u{23f1} TM-512 1h12m\n" || s == "\u{23f1} TM-512 1h13m\n",
        "{s:?}"
    );
    let out = prompt(&env, &["--format", "starship"]);
    assert!(text(&out.stdout).starts_with("\u{23f1} TM-512 1h1"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_timer_shows_the_pause_symbol() {
    let env = Env::new().await;
    env.seed_active(active(None, "TM-512"));
    assert_eq!(text(&prompt(&env, &[]).stdout), "\u{23f8} TM-512\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn shell_formats_wrap_colour_and_end_without_a_newline() {
    let env = Env::new().await;
    env.seed_active(active(Some(now_s()), "TM-512"));
    let zsh = env
        .cmd()
        .env_remove("NO_COLOR")
        .args(["prompt", "--format", "zsh"])
        .output()
        .unwrap();
    assert_eq!(
        text(&zsh.stdout),
        "%{\x1b[38;5;203m%}\u{23f1} TM-512 0m%{\x1b[0m%} "
    );
    let bash = env
        .cmd()
        .env_remove("NO_COLOR")
        .args(["prompt", "--format", "bash"])
        .output()
        .unwrap();
    assert_eq!(
        text(&bash.stdout),
        "\\[\x1b[38;5;203m\\]\u{23f1} TM-512 0m\\[\x1b[0m\\] "
    );
    let plain = prompt(&env, &["--format", "zsh"]);
    assert_eq!(text(&plain.stdout), "\u{23f1} TM-512 0m ");
    let tmux = env
        .cmd()
        .env_remove("NO_COLOR")
        .args(["prompt", "--format", "tmux"])
        .output()
        .unwrap();
    assert_eq!(
        text(&tmux.stdout),
        "#[fg=colour203]\u{23f1} TM-512 0m#[default]\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn hostile_state_files_print_nothing() {
    let env = Env::new().await;
    for raw in [
        b"garbage".to_vec(),
        vec![b'x'; 100_000],
        active(Some(now_s()), "TM-1\u{1b}[31m").to_string().into_bytes(),
        active(Some(now_s()), "tm-1").to_string().into_bytes(),
        b"{\"schema\":\"tmgr.active.v1\",\"task\":{\"id\":1,\"key\":\"TM-1\\n$(id)\",\"title\":\"x\"}}".to_vec(),
    ] {
        env.seed_active_raw(&raw);
        let out = prompt(&env, &[]);
        assert_eq!(out.status.code(), Some(0));
        assert!(out.stdout.is_empty(), "{}", text(&out.stdout));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn control_characters_in_other_fields_never_reach_the_output() {
    let env = Env::new().await;
    let mut v = active(Some(now_s()), "TM-5");
    v["task"]["title"] = json!("a\u{1b}]0;pwn\u{7}");
    env.seed_active(v);
    let out = prompt(&env, &[]);
    assert!(!out.stdout.contains(&0x1b));
    assert!(!out.stdout.contains(&0x07));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_format_is_a_usage_error() {
    let env = Env::new().await;
    assert_eq!(prompt(&env, &["--format", "nope"]).status.code(), Some(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn statusline_claude_reads_bounded_stdin_and_prints_the_timer_part() {
    let env = Env::new().await;
    env.seed_active(active(Some(now_s() - 60), "TM-512"));
    let out = env
        .cmd()
        .args(["statusline", "claude"])
        .write_stdin(r#"{"model":{"display_name":"Opus"},"cwd":"/x"}"#)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(text(&out.stdout), "\u{23f1} TM-512 1m\n");
    for input in ["not json at all", ""] {
        let out = env
            .cmd()
            .args(["statusline", "claude"])
            .write_stdin(input)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(text(&out.stdout), "\u{23f1} TM-512 1m\n");
    }
    let big = vec![b'a'; 300 * 1024];
    let out = env
        .cmd()
        .args(["statusline", "claude"])
        .write_stdin(big)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(text(&out.stdout), "\u{23f1} TM-512 1m\n");
    std::fs::remove_file(env.active_path()).unwrap();
    let out = env
        .cmd()
        .args(["statusline", "claude"])
        .write_stdin("{}")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn prompt_is_fast() {
    let env = Env::new().await;
    env.seed_active(active(Some(now_s()), "TM-512"));
    let mut cmd = std::process::Command::new(std::path::PathBuf::from(env!("CARGO_BIN_EXE_tmgr")));
    cmd.env_clear()
        .env("HOME", env.tmp.path())
        .env("XDG_CONFIG_HOME", env.xdg())
        .args(["prompt"]);
    let runs = 20;
    let start = std::time::Instant::now();
    for _ in 0..runs {
        assert!(cmd.output().unwrap().status.success());
    }
    let avg = start.elapsed() / runs;
    assert!(avg.as_millis() < 250, "average {avg:?}");
}
