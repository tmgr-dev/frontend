use tmgr_ipc::dirs;

#[test]
fn relative_overrides_resolve_against_cwd() {
    std::env::set_var("TMGR_RUNNER_DIR", "rel-run");
    std::env::set_var("TMGR_RUNNER_HOMES", "rel-homes");
    let cwd = std::env::current_dir().unwrap();
    assert_eq!(dirs::runner_dir().unwrap(), cwd.join("rel-run"));
    assert_eq!(dirs::homes_dir().unwrap(), cwd.join("rel-homes"));
}
