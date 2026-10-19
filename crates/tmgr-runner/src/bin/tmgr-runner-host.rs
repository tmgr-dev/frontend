use std::path::PathBuf;
use tmgr_runner::daemon::{self, RunnerConfig};
use tmgr_runner::{sandbox, shim};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["runner", "serve"] => serve(),
        ["__shim", dir, id] => shim::shim_main(&PathBuf::from(dir), id),
        ["__sandbox-exec", ..] => sandbox::exec_helper(&args[1..]),
        ["__agent-work"] => fake_agent_work(),
        ["queue", "autopilot", "serve", "--settings", id] => fake_autopilot(id),
        ["queue", "disable", "--reason", reason] => fake_queue_disable(reason),
        _ => {
            eprintln!(
                "usage: tmgr-runner-host runner serve | __shim <dir> <id> | __sandbox-exec ..."
            );
            2
        }
    };
    std::process::exit(code);
}

fn fake_agent_work() -> i32 {
    use std::io::{Read, Write};
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let Some(log) = std::env::var_os("TMGR_TEST_AGENT_WORK_LOG") else {
        println!("{{\"error\":\"no test log\"}}");
        return 1;
    };
    let fail = std::path::Path::new(&log).with_extension("fail");
    if fail.exists() {
        println!("{{\"error\":\"workspace is locked\"}}");
        return 1;
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
    {
        let _ = writeln!(f, "{}", input.trim());
    }
    println!("{{\"run_id\":77}}");
    0
}

fn fake_queue_disable(reason: &str) -> i32 {
    let Some(base) = std::env::var_os("XDG_CONFIG_HOME") else {
        return 2;
    };
    let dir = std::path::Path::new(&base).join("tmgr");
    let rules = dir.join("queue.toml");
    let text = std::fs::read_to_string(&rules).unwrap_or_default();
    std::fs::write(dir.join("disabled.reason"), reason).unwrap();
    std::fs::write(&rules, text.replace("enabled = true", "enabled = false")).unwrap();
    0
}

fn fake_autopilot(scenario: &str) -> i32 {
    use std::os::fd::AsRawFd;
    use std::time::Duration;
    let Some(base) = std::env::var_os("XDG_CONFIG_HOME") else {
        return 2;
    };
    let dir = std::path::Path::new(&base).join("tmgr/autopilot");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("autopilot.lock"))
        .unwrap();
    for _ in 0..50 {
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            let mut fl: libc::flock = unsafe { std::mem::zeroed() };
            fl.l_type = libc::F_WRLCK as _;
            fl.l_whence = libc::SEEK_SET as _;
            unsafe { libc::fcntl(lock.as_raw_fd(), libc::F_SETLK, &fl) };
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let sleeper = (scenario == "wrong-pid").then(|| {
        std::process::Command::new("sleep")
            .arg("120")
            .spawn()
            .unwrap()
            .id()
    });
    let write = |state: &str, seq: u64, reason: Option<&str>| {
        let body = serde_json::json!({
            "version": 1, "state": state, "pid": sleeper.unwrap_or_else(std::process::id), "rule": "tm",
            "control_seq": seq, "stop_reason": reason, "scenario": scenario,
        });
        let tmp = dir.join(".status.tmp");
        std::fs::write(&tmp, body.to_string()).unwrap();
        std::fs::rename(&tmp, dir.join("status.json")).unwrap();
    };
    write("running", 0, None);
    println!("fake autopilot started");
    if scenario == "crash" {
        return 3;
    }
    let mut seen = 0;
    loop {
        std::thread::sleep(Duration::from_millis(50));
        if matches!(scenario, "ignore-stop" | "wrong-pid") {
            continue;
        }
        let Some(v) = std::fs::read(dir.join("control.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        else {
            continue;
        };
        let seq = v["seq"].as_u64().unwrap_or(0);
        if seq <= seen {
            continue;
        }
        seen = seq;
        match v["command"].as_str() {
            Some("pause") => write("paused", seq, None),
            Some("resume") => write("running", seq, None),
            _ if scenario == "crash-after-stop" => return 4,
            _ => {
                write("stopped", seq, Some("user"));
                return 0;
            }
        }
    }
}

fn serve() -> i32 {
    let cfg = std::env::current_exe()
        .and_then(|exe| RunnerConfig::from_env(exe, env!("CARGO_PKG_VERSION").into()));
    match cfg.and_then(daemon::serve) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("runner: {e}");
            1
        }
    }
}
