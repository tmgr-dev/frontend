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
