use super::service::{paths, RealSys};
use super::{bin, emit, is_down, rerr, runner_config, Opts};
use serde::Serialize;
use std::io;
use std::os::unix::io::AsRawFd;
use std::thread;
use std::time::{Duration, Instant};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_runner::client::Client;
use tmgr_runner::daemon::{self, RunnerConfig};
use tmgr_runner::service::{self, Platform, ServicePaths, Sys};

const REINSTALL: &str = "install-service --install";

pub(super) struct Snapshot {
    pub pid: u32,
    pub version: String,
    pub sessions: usize,
    pub autopilot: bool,
}

pub(super) enum Probe {
    Down,
    Unreadable,
    Up(Snapshot),
}

pub(super) trait Control {
    fn probe(&mut self) -> io::Result<Probe>;
    fn stop(&mut self) -> io::Result<()>;
    fn pid_alive(&mut self, pid: u32) -> bool;
    fn lock_held(&mut self) -> bool;
    fn start_direct(&mut self) -> io::Result<u32>;
}

pub(super) enum Svc {
    None,
    Unusable(String),
    Ready,
}

pub(super) trait Service {
    fn state(&mut self) -> Svc;
    fn start(&mut self) -> io::Result<()>;
    fn unit_outdated(&mut self) -> bool;
}

pub(super) struct Timing {
    pub exit_wait: Duration,
    pub notice_after: Duration,
    pub up_wait: Duration,
    pub sessions_wait: Duration,
    pub poll: Duration,
}

impl Timing {
    fn real() -> Timing {
        Timing {
            exit_wait: Duration::from_secs(30),
            notice_after: Duration::from_secs(2),
            up_wait: Duration::from_secs(10),
            sessions_wait: Duration::from_secs(5),
            poll: Duration::from_millis(100),
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct Outcome {
    pub restarted: bool,
    pub was_running: bool,
    pub old_version: Option<String>,
    pub new_version: String,
    pub old_pid: Option<u32>,
    pub new_pid: u32,
    pub sessions_before: usize,
    pub sessions_after: usize,
    pub autopilot_was_running: bool,
    pub via: &'static str,
    pub warnings: Vec<String>,
}

pub(super) fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.trim().split(['-', '+']).next()?;
    let mut it = core.split('.');
    let mut n = || it.next()?.parse::<u64>().ok();
    let out = (n()?, n()?, n()?);
    it.next().is_none().then_some(out)
}

pub(super) fn is_behind(running: &str, installed: &str) -> bool {
    matches!((parse_version(running), parse_version(installed)), (Some(r), Some(i)) if r < i)
}

fn is_ahead(running: &str, installed: &str) -> bool {
    is_behind(installed, running)
}

fn wait_for(limit: Duration, poll: Duration, mut done: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + limit;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        thread::sleep(poll);
    }
}

fn wait_up(ctl: &mut dyn Control, t: &Timing) -> Result<Snapshot> {
    let mut seen = None;
    let mut last_err = None;
    let ok = wait_for(t.up_wait, t.poll, || match ctl.probe() {
        Ok(Probe::Up(s)) => {
            seen = Some(s);
            true
        }
        Ok(_) => false,
        Err(e) => {
            last_err = Some(e);
            false
        }
    });
    match (ok, seen) {
        (true, Some(s)) => Ok(s),
        _ => Err(match last_err {
            Some(e) => rerr(e),
            None => Error::new(Kind::Offline, "the new runner did not come up in time")
                .hint("see runner.log in the runner directory"),
        }),
    }
}

fn wait_exit(ctl: &mut dyn Control, old_pid: Option<u32>, t: &Timing, quiet: bool) -> Result<()> {
    let started = Instant::now();
    let mut told = false;
    let gone = wait_for(t.exit_wait, t.poll, || {
        if !told && !quiet && started.elapsed() >= t.notice_after {
            told = true;
            eprintln!("waiting for the runner to shut down");
        }
        match old_pid {
            Some(p) => !ctl.pid_alive(p),
            None => !ctl.lock_held(),
        }
    });
    if gone {
        return Ok(());
    }
    Err(Error::new(
        Kind::Conflict,
        format!(
            "the old runner did not exit within {} s",
            t.exit_wait.as_secs()
        ),
    )
    .hint("it may still be suspending the autopilot; try again in a moment"))
}

fn tolerable_stop(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::UnexpectedEof
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
    ) || is_down(e)
}

pub(super) fn restart(
    ctl: &mut dyn Control,
    svc: &mut dyn Service,
    cli_version: &str,
    force: bool,
    t: &Timing,
    quiet: bool,
) -> Result<Outcome> {
    let mut warnings = Vec::new();
    let before = match ctl.probe() {
        Ok(p) => p,
        Err(e) => return Err(rerr(e)),
    };
    let (was_running, old) = match before {
        Probe::Down => (false, None),
        Probe::Unreadable => (true, None),
        Probe::Up(s) => (true, Some(s)),
    };
    let old_version = old.as_ref().map(|s| s.version.clone());
    match &old_version {
        Some(v) if is_ahead(v, cli_version) && !force => {
            return Err(Error::usage(format!(
                "the running runner (version {v}) is newer than this tmgr ({cli_version})"
            ))
            .hint("run the newer tmgr, or pass --force to downgrade"));
        }
        None if was_running && !force => {
            return Err(Error::usage(
                "the running runner did not report its version, so it cannot be compared with this tmgr",
            )
            .hint("pass --force to restart it anyway"));
        }
        _ => {}
    }
    let old_pid = old.as_ref().map(|s| s.pid);
    let sessions_before = old.as_ref().map_or(0, |s| s.sessions);
    let autopilot_was_running = old.as_ref().is_some_and(|s| s.autopilot);
    if svc.unit_outdated() {
        let reinstall = format!("{} runner {REINSTALL}", bin());
        if was_running && (sessions_before > 0 || old.is_none()) {
            if !force {
                return Err(Error::new(
                    Kind::Conflict,
                    "the installed service file lets systemd kill every agent session when the runner stops",
                )
                .hint(format!(
                    "run `{reinstall}` first, or pass --force to restart anyway"
                )));
            }
            warnings.push(format!(
                "restarting with --force although the installed service file lets systemd kill agent sessions; run `{reinstall}`"
            ));
        } else {
            warnings.push(format!(
                "the installed service file lets systemd kill agent sessions when the runner exits; run `{reinstall}`"
            ));
        }
    }
    if was_running {
        if let Err(e) = ctl.stop() {
            if !tolerable_stop(&e) {
                return Err(rerr(e));
            }
        }
        wait_exit(ctl, old_pid, t, quiet)?;
    }
    let mut via = "direct";
    let mut started = false;
    match svc.state() {
        Svc::Ready => match svc.start() {
            Ok(()) => {
                via = "service";
                started = true;
            }
            Err(e) => warnings.push(format!(
                "the login service did not start the runner ({e}); started it directly"
            )),
        },
        Svc::Unusable(why) => warnings.push(format!("{why}; started the runner directly")),
        Svc::None => {}
    }
    if !started {
        ctl.start_direct().map_err(rerr)?;
    }
    let up = wait_up(ctl, t)?;
    if old_pid == Some(up.pid) {
        return Err(Error::internal("the old runner is still running"));
    }
    if up.version != cli_version {
        warnings.push(format!(
            "the runner came up as version {} but this tmgr is {cli_version}; the login service may point to another tmgr binary, run `{} runner {REINSTALL}`",
            up.version,
            bin()
        ));
    }
    let mut sessions_after = up.sessions;
    if sessions_before > 0 && sessions_after < sessions_before {
        wait_for(t.sessions_wait, t.poll, || {
            if let Ok(Probe::Up(s)) = ctl.probe() {
                sessions_after = s.sessions;
            }
            sessions_after >= sessions_before
        });
        if sessions_after < sessions_before {
            let lost = sessions_before - sessions_after;
            warnings.push(format!(
                "{lost} session{} did not come back after the restart",
                if lost == 1 { "" } else { "s" }
            ));
        }
    }
    if autopilot_was_running {
        warnings.push(format!(
            "the autopilot loop was running and is stopped; start it again with `{} autopilot start`",
            bin()
        ));
    }
    Ok(Outcome {
        restarted: true,
        was_running,
        old_version,
        new_version: up.version,
        old_pid,
        new_pid: up.pid,
        sessions_before,
        sessions_after,
        autopilot_was_running,
        via,
        warnings,
    })
}

fn human(o: &Outcome) -> String {
    let n = o.sessions_after;
    let sess = format!("{n} session{}", if n == 1 { "" } else { "s" });
    let mut s = if o.was_running {
        format!(
            "runner restarted: {} -> {} (pid {} -> {}), {sess} kept",
            o.old_version.as_deref().unwrap_or("unknown"),
            o.new_version,
            o.old_pid.map_or("unknown".to_string(), |p| p.to_string()),
            o.new_pid
        )
    } else {
        format!(
            "runner was not running; started {} (pid {}), {sess}",
            o.new_version, o.new_pid
        )
    };
    for w in &o.warnings {
        s.push_str(&format!("\nwarning: {w}"));
    }
    s
}

struct RealControl {
    cfg: RunnerConfig,
}

impl Control for RealControl {
    fn probe(&mut self) -> io::Result<Probe> {
        let mut c = match Client::connect(&self.cfg.dir) {
            Ok(c) => c,
            Err(e) if is_down(&e) => return Ok(Probe::Down),
            Err(e) => return Err(e),
        };
        c.set_timeouts(Duration::from_secs(5))?;
        match c.status() {
            Ok(st) => {
                let autopilot = c.autopilot_status().is_ok_and(|a| a.running);
                Ok(Probe::Up(Snapshot {
                    pid: st.pid,
                    version: st.version,
                    sessions: st.sessions,
                    autopilot,
                }))
            }
            Err(e) if e.kind() == io::ErrorKind::InvalidData => Ok(Probe::Unreadable),
            Err(e) => Err(e),
        }
    }

    fn stop(&mut self) -> io::Result<()> {
        let mut c = Client::connect(&self.cfg.dir)?;
        c.set_timeouts(Duration::from_secs(5))?;
        c.stop()
    }

    fn pid_alive(&mut self, pid: u32) -> bool {
        let Ok(pid) = i32::try_from(pid) else {
            return false;
        };
        let rc = unsafe { libc::kill(pid, 0) };
        rc == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    fn lock_held(&mut self) -> bool {
        let Ok(f) = std::fs::File::open(self.cfg.dir.join("runner.lock")) else {
            return false;
        };
        unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) != 0 }
    }

    fn start_direct(&mut self) -> io::Result<u32> {
        daemon::start_background(&self.cfg)
    }
}

struct RealService {
    paths: Option<ServicePaths>,
    default_dir: bool,
    sys: RealSys,
}

impl RealService {
    fn new() -> RealService {
        RealService {
            paths: paths().ok(),
            default_dir: std::env::var_os("TMGR_RUNNER_DIR").is_none_or(|v| v.is_empty()),
            sys: RealSys,
        }
    }

    fn ours(&self) -> Option<(&ServicePaths, String)> {
        let p = self.paths.as_ref()?;
        let content = std::fs::read_to_string(&p.file).ok()?;
        service::is_ours(p.platform, &content).then_some((p, content))
    }
}

impl Service for RealService {
    fn state(&mut self) -> Svc {
        let Some((p, _)) = self.ours() else {
            return Svc::None;
        };
        if !self.default_dir {
            return Svc::Unusable("the login service does not use this runner directory".into());
        }
        let argv = service::loaded_argv(p);
        match self.sys.run(&argv) {
            Ok(o) if o.ok => Svc::Ready,
            _ => Svc::Unusable("the login service is registered but not loaded".into()),
        }
    }

    fn start(&mut self) -> io::Result<()> {
        let Some((p, _)) = self.ours() else {
            return Err(io::Error::other("no service file"));
        };
        let argv = service::start_argv(p);
        let out = self.sys.run(&argv)?;
        if out.ok {
            Ok(())
        } else {
            Err(io::Error::other(out.stderr.trim().to_string()))
        }
    }

    fn unit_outdated(&mut self) -> bool {
        self.ours().is_some_and(|(p, c)| {
            p.platform == Platform::Systemd && !service::unit_keeps_children(&c)
        })
    }
}

pub fn run(o: &Opts, force: bool) -> Result<i32> {
    let cfg = runner_config()?;
    let mut ctl = RealControl { cfg };
    let mut svc = RealService::new();
    let out = restart(
        &mut ctl,
        &mut svc,
        env!("CARGO_PKG_VERSION"),
        force,
        &Timing::real(),
        o.json || o.quiet,
    )?;
    let text = human(&out);
    Ok(emit(o, &out, text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[derive(Default)]
    struct Fake {
        probes: VecDeque<io::Result<Probe>>,
        after: Option<(u32, &'static str, usize)>,
        exits_after: Option<usize>,
        alive_polls: usize,
        lock_polls: usize,
        stopped: bool,
        direct_started: bool,
        stop_err: Option<io::ErrorKind>,
    }

    fn snap(pid: u32, version: &str, sessions: usize) -> Probe {
        Probe::Up(Snapshot {
            pid,
            version: version.into(),
            sessions,
            autopilot: false,
        })
    }

    impl Fake {
        fn running(pid: u32, version: &str, sessions: usize) -> Fake {
            let mut f = Fake {
                exits_after: Some(1),
                ..Fake::default()
            };
            f.probes.push_back(Ok(snap(pid, version, sessions)));
            f
        }
    }

    impl Control for Fake {
        fn probe(&mut self) -> io::Result<Probe> {
            if let Some(p) = self.probes.pop_front() {
                return p;
            }
            match self.after {
                Some((pid, v, n)) => Ok(snap(pid, v, n)),
                None => Ok(Probe::Down),
            }
        }
        fn stop(&mut self) -> io::Result<()> {
            self.stopped = true;
            match self.stop_err {
                Some(k) => Err(k.into()),
                None => Ok(()),
            }
        }
        fn pid_alive(&mut self, _: u32) -> bool {
            self.alive_polls += 1;
            self.exits_after.is_none_or(|n| self.alive_polls <= n)
        }
        fn lock_held(&mut self) -> bool {
            self.lock_polls += 1;
            self.exits_after.is_none_or(|n| self.lock_polls <= n)
        }
        fn start_direct(&mut self) -> io::Result<u32> {
            self.direct_started = true;
            Ok(0)
        }
    }

    struct FakeSvc {
        state: Option<Svc>,
        fail_start: bool,
        started: bool,
        outdated: bool,
    }

    impl FakeSvc {
        fn new(state: Svc) -> FakeSvc {
            FakeSvc {
                state: Some(state),
                fail_start: false,
                started: false,
                outdated: false,
            }
        }
    }

    impl Service for FakeSvc {
        fn state(&mut self) -> Svc {
            self.state.take().unwrap_or(Svc::None)
        }
        fn start(&mut self) -> io::Result<()> {
            self.started = true;
            if self.fail_start {
                Err(io::Error::other("boom"))
            } else {
                Ok(())
            }
        }
        fn unit_outdated(&mut self) -> bool {
            self.outdated
        }
    }

    fn fast() -> Timing {
        Timing {
            exit_wait: Duration::from_millis(60),
            notice_after: Duration::from_secs(60),
            up_wait: Duration::from_millis(200),
            sessions_wait: Duration::from_millis(60),
            poll: Duration::from_millis(5),
        }
    }

    fn go(ctl: &mut Fake, svc: &mut FakeSvc, cli: &str, force: bool) -> Result<Outcome> {
        restart(ctl, svc, cli, force, &fast(), true)
    }

    #[test]
    fn down_runner_is_just_started() {
        let mut ctl = Fake {
            after: Some((20, "1.0.13", 2)),
            ..Fake::default()
        };
        ctl.probes.push_back(Ok(Probe::Down));
        let mut svc = FakeSvc::new(Svc::None);
        let o = go(&mut ctl, &mut svc, "1.0.13", false).unwrap();
        assert!(!o.was_running && o.old_version.is_none() && o.old_pid.is_none());
        assert!(ctl.direct_started && !ctl.stopped);
        assert_eq!((o.new_pid, o.via), (20, "direct"));
    }

    #[test]
    fn live_sessions_survive_and_the_pid_changes() {
        let mut ctl = Fake::running(10, "1.0.12", 3);
        ctl.after = Some((20, "1.0.13", 3));
        let mut svc = FakeSvc::new(Svc::None);
        let o = go(&mut ctl, &mut svc, "1.0.13", false).unwrap();
        assert!(ctl.stopped && ctl.direct_started);
        assert_eq!((o.old_pid, o.new_pid), (Some(10), 20));
        assert_eq!((o.sessions_before, o.sessions_after), (3, 3));
        assert_eq!(o.old_version.as_deref(), Some("1.0.12"));
        assert!(o.warnings.is_empty());
        assert_eq!(
            human(&o),
            "runner restarted: 1.0.12 -> 1.0.13 (pid 10 -> 20), 3 sessions kept"
        );
    }

    #[test]
    fn sessions_are_given_a_moment_to_reattach() {
        let mut ctl = Fake::running(10, "1.0.12", 3);
        ctl.probes.push_back(Ok(snap(20, "1.0.13", 1)));
        ctl.after = Some((20, "1.0.13", 3));
        let mut svc = FakeSvc::new(Svc::None);
        let o = go(&mut ctl, &mut svc, "1.0.13", false).unwrap();
        assert_eq!((o.sessions_before, o.sessions_after), (3, 3));
    }

    #[test]
    fn service_registered_starts_through_the_service_only() {
        let mut ctl = Fake::running(10, "1.0.12", 1);
        ctl.after = Some((20, "1.0.13", 1));
        let mut svc = FakeSvc::new(Svc::Ready);
        let o = go(&mut ctl, &mut svc, "1.0.13", false).unwrap();
        assert!(svc.started && !ctl.direct_started);
        assert_eq!(o.via, "service");
    }

    #[test]
    fn unloaded_service_falls_back_to_direct_with_a_note() {
        let mut ctl = Fake::running(10, "1.0.12", 1);
        ctl.after = Some((20, "1.0.13", 1));
        let mut svc = FakeSvc::new(Svc::Unusable(
            "the login service is registered but not loaded".into(),
        ));
        let o = go(&mut ctl, &mut svc, "1.0.13", false).unwrap();
        assert!(!svc.started && ctl.direct_started);
        assert_eq!(o.via, "direct");
        assert!(o.warnings[0].contains("not loaded"));
    }

    #[test]
    fn failing_service_command_falls_back_to_direct() {
        let mut ctl = Fake::running(10, "1.0.12", 1);
        ctl.after = Some((20, "1.0.13", 1));
        let mut svc = FakeSvc::new(Svc::Ready);
        svc.fail_start = true;
        let o = go(&mut ctl, &mut svc, "1.0.13", false).unwrap();
        assert!(svc.started && ctl.direct_started);
        assert_eq!(o.via, "direct");
        assert!(o.warnings[0].contains("boom"));
    }

    #[test]
    fn service_starting_another_version_warns() {
        let mut ctl = Fake::running(10, "1.0.12", 1);
        ctl.after = Some((20, "1.0.12", 1));
        let mut svc = FakeSvc::new(Svc::Ready);
        let o = go(&mut ctl, &mut svc, "1.0.13", false).unwrap();
        assert!(o
            .warnings
            .iter()
            .any(|w| w.contains("install-service --install")));
    }

    #[test]
    fn outdated_systemd_unit_warns() {
        let mut ctl = Fake::running(10, "1.0.13", 0);
        ctl.after = Some((20, "1.0.13", 0));
        let mut svc = FakeSvc::new(Svc::None);
        svc.outdated = true;
        let o = go(&mut ctl, &mut svc, "1.0.13", false).unwrap();
        assert!(o
            .warnings
            .iter()
            .any(|w| w.contains("install-service --install")));
    }

    #[test]
    fn downgrade_needs_force() {
        let mut ctl = Fake::running(10, "1.0.14", 1);
        ctl.after = Some((20, "1.0.13", 1));
        let mut svc = FakeSvc::new(Svc::None);
        let e = go(&mut ctl, &mut svc, "1.0.13", false).unwrap_err();
        assert!(e.to_string().contains("newer"));
        assert!(!ctl.stopped);
        let mut ctl = Fake::running(10, "1.0.14", 1);
        ctl.after = Some((20, "1.0.13", 1));
        let mut svc = FakeSvc::new(Svc::None);
        assert!(go(&mut ctl, &mut svc, "1.0.13", true).is_ok());
    }

    #[test]
    fn old_process_that_never_exits_times_out_without_starting() {
        let mut ctl = Fake::running(10, "1.0.12", 1);
        ctl.exits_after = None;
        let mut svc = FakeSvc::new(Svc::None);
        let e = go(&mut ctl, &mut svc, "1.0.13", false).unwrap_err();
        assert!(e.to_string().contains("did not exit"));
        assert!(ctl.stopped && !ctl.direct_started && !svc.started);
    }

    #[test]
    fn connect_error_other_than_down_does_not_start() {
        let mut ctl = Fake::default();
        ctl.probes
            .push_back(Err(io::Error::from(io::ErrorKind::PermissionDenied)));
        let mut svc = FakeSvc::new(Svc::None);
        assert!(go(&mut ctl, &mut svc, "1.0.13", false).is_err());
        assert!(!ctl.stopped && !ctl.direct_started && !svc.started);
    }

    #[test]
    fn runner_too_old_to_report_status_is_stopped_via_the_lock() {
        let mut ctl = Fake {
            exits_after: Some(1),
            after: Some((20, "1.0.13", 0)),
            ..Fake::default()
        };
        ctl.probes.push_back(Ok(Probe::Unreadable));
        let mut svc = FakeSvc::new(Svc::None);
        let o = go(&mut ctl, &mut svc, "1.0.13", true).unwrap();
        assert!(ctl.stopped && ctl.lock_polls > 0 && ctl.alive_polls == 0);
        assert!(o.was_running && o.old_version.is_none());
    }

    #[test]
    fn unreadable_status_needs_force() {
        let mut ctl = Fake {
            after: Some((20, "1.0.13", 0)),
            ..Fake::default()
        };
        ctl.probes.push_back(Ok(Probe::Unreadable));
        let mut svc = FakeSvc::new(Svc::None);
        let e = go(&mut ctl, &mut svc, "1.0.13", false).unwrap_err();
        assert!(e.hint.as_deref().unwrap_or_default().contains("--force"));
        assert!(!ctl.stopped && !ctl.direct_started);
    }

    #[test]
    fn outdated_unit_with_live_sessions_is_refused_before_stopping() {
        let mut ctl = Fake::running(10, "1.0.12", 2);
        ctl.after = Some((20, "1.0.13", 2));
        let mut svc = FakeSvc::new(Svc::None);
        svc.outdated = true;
        let e = go(&mut ctl, &mut svc, "1.0.13", false).unwrap_err();
        assert_eq!(e.kind, Kind::Conflict);
        assert!(e
            .hint
            .as_deref()
            .unwrap_or_default()
            .contains("install-service --install"));
        assert!(!ctl.stopped && !ctl.direct_started);
    }

    #[test]
    fn outdated_unit_with_live_sessions_proceeds_with_force_and_a_warning() {
        let mut ctl = Fake::running(10, "1.0.12", 2);
        ctl.after = Some((20, "1.0.13", 2));
        let mut svc = FakeSvc::new(Svc::None);
        svc.outdated = true;
        let o = go(&mut ctl, &mut svc, "1.0.13", true).unwrap();
        assert!(ctl.stopped);
        assert!(o.warnings.iter().any(|w| w.contains("--force")));
    }

    #[test]
    fn outdated_unit_without_sessions_still_restarts() {
        let mut ctl = Fake::running(10, "1.0.12", 0);
        ctl.after = Some((20, "1.0.13", 0));
        let mut svc = FakeSvc::new(Svc::None);
        svc.outdated = true;
        assert!(go(&mut ctl, &mut svc, "1.0.13", false).is_ok());
        assert!(ctl.stopped);
    }

    #[test]
    fn sessions_that_do_not_come_back_are_reported() {
        let mut ctl = Fake::running(10, "1.0.12", 3);
        ctl.after = Some((20, "1.0.13", 1));
        let mut svc = FakeSvc::new(Svc::None);
        let o = go(&mut ctl, &mut svc, "1.0.13", false).unwrap();
        assert_eq!((o.sessions_before, o.sessions_after), (3, 1));
        assert!(o.warnings.iter().any(|w| w.contains("2 sessions")));
    }

    #[test]
    fn dropped_connection_on_stop_is_fine() {
        let mut ctl = Fake::running(10, "1.0.12", 1);
        ctl.stop_err = Some(io::ErrorKind::UnexpectedEof);
        ctl.after = Some((20, "1.0.13", 1));
        let mut svc = FakeSvc::new(Svc::None);
        assert!(go(&mut ctl, &mut svc, "1.0.13", false).is_ok());
    }

    #[test]
    fn same_pid_after_start_is_an_error() {
        let mut ctl = Fake::running(10, "1.0.12", 1);
        ctl.after = Some((10, "1.0.13", 1));
        let mut svc = FakeSvc::new(Svc::None);
        assert!(go(&mut ctl, &mut svc, "1.0.13", false).is_err());
    }

    #[test]
    fn version_comparison() {
        assert!(is_behind("1.0.12", "1.0.13"));
        assert!(is_behind("0.9.9", "0.10.0"));
        assert!(!is_behind("1.0.13", "1.0.13"));
        assert!(!is_behind("1.0.14", "1.0.13"));
        assert!(!is_behind("garbage", "1.0.13"));
        assert!(!is_behind("1.0.12", "dev"));
        assert_eq!(parse_version("1.2.3-rc.1"), Some((1, 2, 3)));
    }
}
