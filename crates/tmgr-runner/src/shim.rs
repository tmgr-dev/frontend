use crate::wire::{now_ms, LaunchFile, Slot, SHIM_CWD_FD};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};
use tmgr_ipc::endpoint::{self, Endpoint};
use tmgr_ipc::frame::{
    read_frame, write_frame, write_json, BinFrame, Frame, TAG_INPUT, TAG_OUTPUT, TAG_REPLAY,
};
use tmgr_ipc::proto::{valid_session_id, ExitInfo, FromShim, ShimHello, Stat, ToShim};
use tmgr_ipc::{auth, secret, server};

const RING_CAP: usize = 2 << 20;
const LOG_CAP: u64 = 20 << 20;
const REAP_AFTER: Duration = Duration::from_secs(24 * 3600);
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PENDING: usize = 4;
const INPUT_CAP: usize = 1 << 20;
const INPUT_FRAMES: usize = 1024;

#[derive(Default)]
struct InputQueue {
    q: Mutex<(VecDeque<Vec<u8>>, usize)>,
    ready: Condvar,
    dropped: AtomicU64,
}

impl InputQueue {
    fn push(&self, data: Vec<u8>) {
        if data.is_empty() {
            return;
        }
        let mut g = lock(&self.q);
        if g.1 + data.len() > INPUT_CAP || g.0.len() >= INPUT_FRAMES {
            self.dropped.fetch_add(data.len() as u64, Ordering::SeqCst);
            return;
        }
        g.1 += data.len();
        g.0.push_back(data);
        self.ready.notify_one();
    }

    fn pop(&self) -> Vec<u8> {
        let mut g = lock(&self.q);
        loop {
            if let Some(d) = g.0.pop_front() {
                return d;
            }
            g = self.ready.wait(g).unwrap_or_else(|e| e.into_inner());
        }
    }

    fn done(&self, n: usize) {
        lock(&self.q).1 -= n;
    }
}

struct ChildGuard {
    pid: i32,
    exited: Mutex<bool>,
}

impl ChildGuard {
    fn new(pid: i32) -> ChildGuard {
        ChildGuard {
            pid,
            exited: Mutex::new(false),
        }
    }

    fn signal(&self, sig: i32) -> bool {
        let exited = lock(&self.exited);
        !*exited && unsafe { libc::kill(-self.pid, sig) } == 0
    }

    fn wait(&self) -> libc::c_int {
        loop {
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let r = unsafe {
                libc::waitid(
                    libc::P_PID,
                    self.pid as libc::id_t,
                    &mut info,
                    libc::WEXITED | libc::WNOWAIT,
                )
            };
            if r == 0 || io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                break;
            }
        }
        let mut exited = lock(&self.exited);
        *exited = true;
        let mut status: libc::c_int = 0;
        loop {
            let r = unsafe { libc::waitpid(self.pid, &mut status, 0) };
            if r == self.pid {
                return status;
            }
            if r < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                return 0;
            }
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

struct OutLog {
    path: PathBuf,
    file: Option<File>,
    size: u64,
}

impl OutLog {
    fn open(path: PathBuf) -> OutLog {
        let file = Self::create(&path);
        OutLog {
            path,
            file,
            size: 0,
        }
    }

    fn create(path: &Path) -> Option<File> {
        OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(path)
            .ok()
    }

    fn append(&mut self, buf: &[u8]) {
        if self.size + buf.len() as u64 > LOG_CAP {
            let mut old = self.path.clone().into_os_string();
            old.push(".1");
            let _ = fs::rename(&self.path, PathBuf::from(old));
            self.file = Self::create(&self.path);
            self.size = 0;
        }
        if let Some(f) = &mut self.file {
            if f.write_all(buf).is_err() {
                self.file = None;
            }
        }
        self.size += buf.len() as u64;
    }
}

struct Conn {
    id: u64,
    out: UnixStream,
}

struct State {
    ring: VecDeque<u8>,
    conn: Option<Conn>,
    next_conn: u64,
    exit: Option<ExitInfo>,
    exited_at: Option<Instant>,
}

struct Shim {
    id: String,
    child_pid: i32,
    child: ChildGuard,
    started_ms: u64,
    cwd_id: (u64, u64),
    endpoint: Endpoint,
    dir: PathBuf,
    st: Mutex<State>,
    input: Arc<InputQueue>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    log: Mutex<OutLog>,
    reader_done: AtomicBool,
}

impl Shim {
    fn send(&self, st: &mut State, msg: &FromShim) {
        if let Some(c) = &mut st.conn {
            if write_json(&mut c.out, msg).is_err() {
                st.conn = None;
            }
        }
    }

    fn on_output(&self, data: &[u8]) {
        lock(&self.log).append(data);
        let mut st = lock(&self.st);
        st.ring.extend(data);
        let excess = st.ring.len().saturating_sub(RING_CAP);
        if excess > 0 {
            st.ring.drain(..excess);
        }
        if let Some(c) = &mut st.conn {
            let frame = Frame::Bin(BinFrame {
                tag: TAG_OUTPUT,
                session: self.id.clone(),
                data: data.to_vec(),
            });
            if write_frame(&mut c.out, &frame).is_err() {
                st.conn = None;
            }
        }
    }

    fn cleanup(&self) {
        match &self.endpoint {
            Endpoint::Path(p) => {
                let _ = fs::remove_file(p);
            }
            Endpoint::Abstract(_) => {
                let _ = fs::remove_file(self.dir.join("s").join(format!("{}.addr", self.id)));
            }
        }
    }

    fn handle(self: &Arc<Self>, stream: UnixStream, adopt_secret: &str, pending: Slot) {
        let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
        if !self.authenticate(&stream, adopt_secret) {
            return;
        }
        drop(pending);
        let Ok(out) = stream.try_clone() else { return };
        let _ = out.set_write_timeout(Some(IO_TIMEOUT));
        let conn_id = {
            let mut st = lock(&self.st);
            if let Some(old) = st.conn.take() {
                let _ = old.out.shutdown(std::net::Shutdown::Both);
            }
            st.next_conn += 1;
            let id = st.next_conn;
            let mut out = out;
            let hello = FromShim::Hello(ShimHello {
                pid: std::process::id(),
                child_pid: self.child_pid as u32,
                started_at_ms: self.started_ms,
                exit: st.exit.clone(),
                cwd_dev: report_cwd_id().then_some(self.cwd_id.0),
                cwd_ino: report_cwd_id().then_some(self.cwd_id.1),
            });
            let replay = Frame::Bin(BinFrame {
                tag: TAG_REPLAY,
                session: self.id.clone(),
                data: st.ring.iter().copied().collect(),
            });
            if write_json(&mut out, &hello).is_err() || write_frame(&mut out, &replay).is_err() {
                return;
            }
            st.conn = Some(Conn { id, out });
            id
        };
        self.serve_conn(&stream);
        let mut st = lock(&self.st);
        if st.conn.as_ref().is_some_and(|c| c.id == conn_id) {
            st.conn = None;
        }
    }

    fn authenticate(&self, stream: &UnixStream, adopt_secret: &str) -> bool {
        let deadline = Instant::now() + server::HANDSHAKE_TIMEOUT;
        let next = || match server::read_frame_by(stream, deadline) {
            Ok(Some(Frame::Json(v))) => serde_json::from_value::<ToShim>(v).ok(),
            _ => None,
        };
        let Some(ToShim::Adopt { nonce_r }) = next() else {
            return false;
        };
        let nonce_sh = auth::nonce();
        let challenge = FromShim::AdoptChallenge {
            proof_sh: auth::proof(adopt_secret, auth::SHIM, &nonce_r, &nonce_sh),
            nonce_sh: nonce_sh.clone(),
        };
        if write_json(&mut &*stream, &challenge).is_err() {
            return false;
        }
        let Some(ToShim::AdoptAuth { proof_r }) = next() else {
            return false;
        };
        auth::verify(
            adopt_secret,
            auth::RUNNER_ADOPT,
            &nonce_sh,
            &nonce_r,
            &proof_r,
        )
    }

    fn serve_conn(self: &Arc<Self>, stream: &UnixStream) {
        loop {
            match read_frame(&mut &*stream) {
                Ok(Some(Frame::Bin(b))) if b.tag == TAG_INPUT => {
                    self.input.push(b.data);
                }
                Ok(Some(Frame::Json(v))) => match serde_json::from_value::<ToShim>(v) {
                    Ok(ToShim::Resize { cols, rows }) => {
                        let (cols, rows) = crate::wire::clamp_size(cols, rows);
                        let _ = lock(&self.master).resize(PtySize {
                            rows,
                            cols,
                            pixel_width: 0,
                            pixel_height: 0,
                        });
                    }
                    Ok(ToShim::Signal { sig, id }) => {
                        let delivered = self.signal(sig);
                        let mut st = lock(&self.st);
                        self.send(&mut st, &FromShim::SignalAck { sig, delivered, id });
                    }
                    Ok(ToShim::Reap) => {
                        self.cleanup();
                        std::process::exit(0);
                    }
                    Ok(ToShim::Adopt { .. } | ToShim::AdoptAuth { .. }) | Err(_) => {}
                },
                _ => return,
            }
        }
    }

    fn signal(&self, sig: i32) -> bool {
        (1..=31).contains(&sig) && self.child.signal(sig)
    }
}

fn read_secret(dir: &Path, id: &str) -> io::Result<String> {
    secret::read(&dir.join("sessions").join(id).join("adopt.secret"))
}

fn exit_info(status: libc::c_int) -> ExitInfo {
    if libc::WIFEXITED(status) {
        ExitInfo {
            code: Some(libc::WEXITSTATUS(status)),
            signal: None,
        }
    } else {
        ExitInfo {
            code: None,
            signal: Some(libc::WTERMSIG(status)),
        }
    }
}

pub fn shim_main(dir: &Path, id: &str) -> i32 {
    if !valid_session_id(id) {
        eprintln!("shim: invalid session id");
        return 2;
    }
    unsafe { libc::setsid() };
    match run(dir, id) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("shim: {e}");
            1
        }
    }
}

#[cfg(debug_assertions)]
fn report_cwd_id() -> bool {
    std::env::var_os("TMGR_TEST_SHIM_NO_CWD_ID").is_none()
}

#[cfg(not(debug_assertions))]
fn report_cwd_id() -> bool {
    true
}

#[allow(clippy::unnecessary_cast)]
fn enter_launch_dir() -> io::Result<(u64, u64)> {
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(SHIM_CWD_FD, &mut st) } != 0
        || st.st_mode & libc::S_IFMT != libc::S_IFDIR
    {
        return Err(io::Error::other("the launch directory was not handed over"));
    }
    let res = unsafe { libc::fchdir(SHIM_CWD_FD) };
    let err = io::Error::last_os_error();
    unsafe { libc::close(SHIM_CWD_FD) };
    if res != 0 {
        return Err(err);
    }
    Ok((st.st_dev as u64, st.st_ino as u64))
}

fn run(dir: &Path, id: &str) -> io::Result<()> {
    let cwd_id = enter_launch_dir()?;
    let sdir = dir.join("sessions").join(id);
    let launch_path = sdir.join("launch.json");
    let launch_bytes = fs::read(&launch_path)?;
    fs::remove_file(&launch_path)?;
    let launch: LaunchFile = serde_json::from_slice(&launch_bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("launch.json: {e}")))?;
    let adopt_secret = read_secret(dir, id)?;
    let (cols, rows) = crate::wire::clamp_size(launch.cols, launch.rows);
    let pair = native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(io::Error::other)?;
    let mut cmd = CommandBuilder::new(&launch.argv[0]);
    cmd.args(&launch.argv[1..]);
    cmd.env_clear();
    for (k, v) in &launch.env {
        cmd.env(k, v);
    }
    cmd.cwd(".");
    let child = pair.slave.spawn_command(cmd).map_err(io::Error::other)?;
    drop(pair.slave);
    let child_pid = child
        .process_id()
        .ok_or_else(|| io::Error::other("child has no pid"))? as i32;
    let reader = pair.master.try_clone_reader().map_err(io::Error::other)?;
    let writer = pair.master.take_writer().map_err(io::Error::other)?;
    let input = Arc::new(InputQueue::default());

    crate::wire::write_atomic(
        &sdir.join("shim.pid"),
        std::process::id().to_string().as_bytes(),
    )?;
    let endpoint = Endpoint::new_session(dir, id)?;
    let listener = endpoint::bind(&endpoint)?;

    let shim = Arc::new(Shim {
        id: id.to_string(),
        child_pid,
        child: ChildGuard::new(child_pid),
        started_ms: now_ms(),
        cwd_id,
        endpoint,
        dir: dir.to_path_buf(),
        st: Mutex::new(State {
            ring: VecDeque::new(),
            conn: None,
            next_conn: 0,
            exit: None,
            exited_at: None,
        }),
        input: input.clone(),
        master: Mutex::new(pair.master),
        log: Mutex::new(OutLog::open(sdir.join("output.log"))),
        reader_done: AtomicBool::new(false),
    });

    spawn_input_writer(input, writer);
    spawn_reader(&shim, reader);
    spawn_waiter(&shim, child);
    spawn_stat(&shim);
    spawn_reaper(&shim);

    let pending = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if server::accept_check(&stream).is_err() {
            continue;
        }
        let Some(slot) = Slot::take(&pending, MAX_PENDING) else {
            continue;
        };
        let shim = shim.clone();
        let secret = adopt_secret.clone();
        thread::spawn(move || shim.handle(stream, &secret, slot));
    }
    Ok(())
}

fn spawn_input_writer(queue: Arc<InputQueue>, mut pty: Box<dyn Write + Send>) {
    thread::spawn(move || loop {
        let data = queue.pop();
        let _ = pty.write_all(&data).and_then(|()| pty.flush());
        queue.done(data.len());
    });
}

fn spawn_reader(shim: &Arc<Shim>, mut reader: Box<dyn Read + Send>) {
    let shim = shim.clone();
    thread::spawn(move || {
        let mut buf = vec![0u8; 16 << 10];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => shim.on_output(&buf[..n]),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        shim.reader_done.store(true, Ordering::SeqCst);
    });
}

fn spawn_waiter(shim: &Arc<Shim>, child: Box<dyn portable_pty::Child + Send + Sync>) {
    let shim = shim.clone();
    thread::spawn(move || {
        let _keep = child;
        let status = shim.child.wait();
        let info = exit_info(status);
        let deadline = Instant::now() + Duration::from_secs(1);
        while !shim.reader_done.load(Ordering::SeqCst) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let mut st = lock(&shim.st);
        st.exit = Some(info.clone());
        st.exited_at = Some(Instant::now());
        shim.send(&mut st, &FromShim::Exited(info));
    });
}

fn spawn_stat(shim: &Arc<Shim>) {
    let shim = shim.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(500));
        if lock(&shim.st).exit.is_some() {
            continue;
        }
        let pg = lock(&shim.master)
            .process_group_leader()
            .filter(|p| *p > 0)
            .unwrap_or(shim.child_pid);
        let stat = Stat {
            fg_pgid: Some(pg),
            fg_cpu_ns: proc::cpu_ns(pg),
            fg_waiting_tty: proc::waiting_tty(pg),
        };
        let dropped = shim.input.dropped.swap(0, Ordering::SeqCst);
        let mut st = lock(&shim.st);
        shim.send(&mut st, &FromShim::Stat(stat));
        if dropped > 0 {
            shim.send(&mut st, &FromShim::InputDropped { bytes: dropped });
        }
    });
}

fn spawn_reaper(shim: &Arc<Shim>) {
    let shim = shim.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(1));
        let expired = lock(&shim.st)
            .exited_at
            .is_some_and(|t| t.elapsed() >= REAP_AFTER);
        if expired {
            shim.cleanup();
            std::process::exit(0);
        }
    });
}

#[cfg(target_os = "linux")]
mod proc {
    pub fn cpu_ns(pid: i32) -> Option<u64> {
        let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let rest = &text[text.rfind(')')? + 2..];
        let f: Vec<&str> = rest.split(' ').collect();
        let ticks: u64 = f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?;
        let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as u64;
        Some(ticks * (1_000_000_000 / hz))
    }

    pub fn waiting_tty(pid: i32) -> Option<bool> {
        let wchan = std::fs::read_to_string(format!("/proc/{pid}/wchan")).unwrap_or_default();
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let state = stat.get(stat.rfind(')')? + 2..)?.chars().next()?;
        Some(state == 'S' && matches!(wchan.trim(), "n_tty_read" | "wait_woken"))
    }
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
mod proc {
    pub fn cpu_ns(pid: i32) -> Option<u64> {
        let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
        let n = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTASKINFO,
                0,
                (&mut info as *mut libc::proc_taskinfo).cast(),
                size,
            )
        };
        if n != size {
            return None;
        }
        let (numer, denom) = timebase()?;
        let ticks = info.pti_total_user as u128 + info.pti_total_system as u128;
        Some((ticks * numer as u128 / denom as u128) as u64)
    }

    fn timebase() -> Option<(u32, u32)> {
        // Under Rosetta mach_timebase_info says 1/1, but task times stay in native Apple Silicon ticks.
        if translated() {
            return Some((125, 3));
        }
        let mut tb = libc::mach_timebase_info { numer: 0, denom: 0 };
        if unsafe { libc::mach_timebase_info(&mut tb) } != 0 || tb.denom == 0 {
            return None;
        }
        Some((tb.numer, tb.denom))
    }

    fn translated() -> bool {
        let mut v: libc::c_int = 0;
        let mut len = std::mem::size_of::<libc::c_int>();
        let rc = unsafe {
            libc::sysctlbyname(
                c"sysctl.proc_translated".as_ptr(),
                (&mut v as *mut libc::c_int).cast(),
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        rc == 0 && v == 1
    }

    pub fn waiting_tty(_pid: i32) -> Option<bool> {
        None
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod proc {
    pub fn cpu_ns(_pid: i32) -> Option<u64> {
        None
    }

    pub fn waiting_tty(_pid: i32) -> Option<bool> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{proc, ChildGuard, InputQueue, INPUT_CAP, INPUT_FRAMES};
    use std::io::{BufRead, BufReader};
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    #[test]
    #[allow(clippy::zombie_processes)]
    fn signal_is_refused_once_the_child_exited_and_never_reaches_the_group() {
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "sleep 30 & echo $!; exit 0"])
            .stdout(Stdio::piped());
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let mut child = cmd.spawn().unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let grandchild: i32 = line.trim().parse().unwrap();
        let guard = ChildGuard::new(child.id() as i32);
        let status = guard.wait();
        assert!(libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0);
        assert!(!guard.signal(libc::SIGKILL));
        let alive = unsafe { libc::kill(grandchild, 0) } == 0;
        unsafe { libc::kill(grandchild, libc::SIGKILL) };
        assert!(alive, "the group was signalled after the child exited");
    }

    #[test]
    #[allow(clippy::zombie_processes)]
    fn signal_reaches_a_running_child_group() {
        let mut cmd = Command::new("/bin/sleep");
        cmd.arg("30");
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let child = cmd.spawn().unwrap();
        let guard = ChildGuard::new(child.id() as i32);
        assert!(guard.signal(libc::SIGKILL));
        let status = guard.wait();
        assert_eq!(libc::WTERMSIG(status), libc::SIGKILL);
    }

    #[test]
    fn input_queue_drops_past_the_cap_and_counts_it() {
        let q = InputQueue::default();
        q.push(vec![0; INPUT_CAP]);
        q.push(vec![0; 10]);
        assert_eq!(q.dropped.load(Ordering::SeqCst), 10);
        let d = q.pop();
        q.done(d.len());
        q.push(vec![0; 10]);
        assert_eq!(q.dropped.load(Ordering::SeqCst), 10);
    }

    #[test]
    fn input_queue_ignores_empty_frames_and_caps_the_count() {
        let q = InputQueue::default();
        for _ in 0..INPUT_FRAMES * 2 {
            q.push(Vec::new());
        }
        assert_eq!(q.q.lock().unwrap().0.len(), 0);
        for _ in 0..INPUT_FRAMES + 5 {
            q.push(vec![1]);
        }
        assert_eq!(q.q.lock().unwrap().0.len(), INPUT_FRAMES);
        assert_eq!(q.dropped.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn cpu_time_is_in_nanoseconds() {
        let started = std::time::Instant::now();
        let mut busy = Command::new("/bin/sh")
            .args(["-c", "while :; do :; done"])
            .spawn()
            .unwrap();
        let pid = busy.id() as i32;
        let mut a = proc::cpu_ns(pid).unwrap_or(0);
        while a < 100_000_000 && started.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(20));
            a = proc::cpu_ns(pid).unwrap_or(a);
        }
        std::thread::sleep(Duration::from_millis(50));
        let b = proc::cpu_ns(pid).unwrap();
        let wall = started.elapsed();
        busy.kill().unwrap();
        busy.wait().unwrap();
        let cpus = std::thread::available_parallelism().map_or(1, |n| n.get() as u64);
        let ceiling = wall.as_nanos() as u64 * cpus + 200_000_000;
        assert!(a >= 100_000_000, "cpu time {a} ns never reached 100 ms");
        assert!(b >= a, "cpu time went backwards: {a} then {b}");
        assert!(b <= ceiling, "cpu {b} ns over {wall:?} on {cpus} cpus");
    }
}
