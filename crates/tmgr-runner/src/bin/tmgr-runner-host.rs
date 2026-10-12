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
        _ => {
            eprintln!(
                "usage: tmgr-runner-host runner serve | __shim <dir> <id> | __sandbox-exec ..."
            );
            2
        }
    };
    std::process::exit(code);
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
