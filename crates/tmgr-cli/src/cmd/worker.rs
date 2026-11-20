#[cfg(feature = "runner")]
use super::runner::attach_title::FollowerTitles;
use std::time::Duration;
use tmgr_link::proto::{Timed, MAX_FRAME};
use tmgr_link::runner::{Backend, RunnerClient};
use tmgr_link::store::Paths;
use tmgr_link::taskref::LocalRefs;
use tmgr_link::worker::{check_argv, serve_with, watchdog, Env, Limits, NoTitles, Runtime, Titles};

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
    let _watchdog = watchdog(tmgr_link::runs::ATTACH_MAX + GRACE, || {
        std::process::exit(1)
    });
    let mut chan = Timed::spawn(
        std::io::stdin(),
        std::io::stdout(),
        IDLE,
        SESSION,
        MAX_FRAME,
    );
    let connect = |deadline| {
        RunnerClient::connect_default_until(deadline).map(|c| Box::new(c) as Box<dyn Backend>)
    };
    #[cfg(feature = "runner")]
    let found = FollowerTitles::new();
    #[cfg(not(feature = "runner"))]
    let found: Option<NoTitles> = None;
    let titles: &dyn Titles = match &found {
        Some(t) => t,
        None => &NoTitles,
    };
    let rt = Runtime {
        connect: &connect,
        resolver: &LocalRefs,
        titles,
    };
    match serve_with(
        &paths,
        &name,
        &enrollment,
        &Env::from_process(),
        &mut chan,
        &Limits::default(),
        &rt,
    ) {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("tmgr worker: refused");
            e.exit_code()
        }
    }
}
