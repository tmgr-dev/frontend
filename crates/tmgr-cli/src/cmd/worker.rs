use tmgr_link::proto::Framed;
use tmgr_link::store::Paths;
use tmgr_link::worker::{check_argv, serve, Env, MAX_REQUESTS};

pub fn run() -> i32 {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let Ok(name) = check_argv(&args) else {
        eprintln!("tmgr worker: refused");
        return 5;
    };
    let Ok(paths) = Paths::from_env() else {
        eprintln!("tmgr worker: refused");
        return 1;
    };
    let mut chan = Framed::new(std::io::stdin(), std::io::stdout());
    match serve(&paths, &name, &Env::from_process(), &mut chan, MAX_REQUESTS) {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("tmgr worker: refused");
            e.exit_code()
        }
    }
}
