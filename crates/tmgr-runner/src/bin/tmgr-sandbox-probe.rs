fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some("__sandbox-exec") {
        eprintln!("usage: tmgr-sandbox-probe __sandbox-exec <policy> -- <command...>");
        std::process::exit(2);
    }
    std::process::exit(tmgr_runner::sandbox::exec_helper_with(
        &args[2..],
        std::env::var_os("TMGR_TEST_FORCE_RO_FALLBACK").is_some(),
    ));
}
