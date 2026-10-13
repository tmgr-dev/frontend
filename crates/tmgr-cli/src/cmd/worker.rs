use std::time::Duration;
use tmgr_link::proto::{Timed, MAX_FRAME};
use tmgr_link::store::Paths;
use tmgr_link::worker::{check_argv, serve, watchdog, Env, Limits};

const IDLE: Duration = Duration::from_secs(30);
const SESSION: Duration = Duration::from_secs(600);
const GRACE: Duration = Duration::from_secs(10);

pub fn run() -> i32 {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let Ok((name, enrollment)) = check_argv(&args) else {
        eprintln!("tmgr worker: refused");
        return 5;
    };
    let Ok(paths) = Paths::from_env() else {
        eprintln!("tmgr worker: refused");
        return 1;
    };
    let _watchdog = watchdog(SESSION + GRACE, || std::process::exit(1));
    let mut chan = Timed::spawn(
        std::io::stdin(),
        std::io::stdout(),
        IDLE,
        SESSION,
        MAX_FRAME,
    );
    match serve(
        &paths,
        &name,
        &enrollment,
        &Env::from_process(),
        &mut chan,
        &Limits::default(),
    ) {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("tmgr worker: refused");
            e.exit_code()
        }
    }
}
