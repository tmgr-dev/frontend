use crate::wire::{now_ms, LaunchFile};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};
use tmgr_ipc::endpoint::{self, Endpoint};
use tmgr_ipc::frame::{
    read_frame, write_frame, write_json, BinFrame, Frame, TAG_INPUT, TAG_OUTPUT, TAG_REPLAY,
};
use tmgr_ipc::proto::{valid_session_id, ExitInfo, FromShim, ShimHello, Stat, ToShim};
use tmgr_ipc::{secret, server};

const RING_CAP: usize = 2 << 20;
const LOG_CAP: u64 = 20 << 20;
const REAP_AFTER: Duration = Duration::from_secs(24 * 3600);
const IO_TIMEOUT: Duration = Duration::from_secs(5);

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
    started_ms: u64,
    endpoint: Endpoint,
    dir: PathBuf,
    st: Mutex<State>,
    input: Mutex<Box<dyn Write + Send>>,
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

    fn handle(self: &Arc<Self>, stream: UnixStream, adopt_secret: &str) {
        if server::accept_check(&stream).is_err() {
            return;
        }
        let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
        let secret_ok = match read_frame(&mut &stream) {
            Ok(Some(Frame::Json(v))) => match serde_json::from_value::<ToShim>(v) {
                Ok(ToShim::Adopt { secret: s }) => secret::eq(&s, adopt_secret),
                _ => false,
            },
            _ => false,
        };
        if !secret_ok {
            return;
        }
        let _ = stream.set_read_timeout(None);
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

    fn serve_conn(self: &Arc<Self>, stream: &UnixStream) {
        loop {
            match read_frame(&mut &*stream) {
                Ok(Some(Frame::Bin(b))) if b.tag == TAG_INPUT => {
                    let mut w = lock(&self.input);
                    if w.write_all(&b.data).and_then(|_| w.flush()).is_err() {
                        continue;
                    }
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
                    Ok(ToShim::Signal { sig }) => self.signal(sig),
                    Ok(ToShim::Reap) => {
                        self.cleanup();
                        std::process::exit(0);
                    }
                    Ok(ToShim::Adopt { .. }) | Err(_) => {}
                },
                _ => return,
            }
        }
    }

    fn signal(&self, sig: i32) {
        if !(1..=31).contains(&sig) || lock(&self.st).exit.is_some() {
            return;
        }
        unsafe { libc::kill(-self.child_pid, sig) };
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

fn run(dir: &Path, id: &str) -> io::Result<()> {
    let sdir = dir.join("sessions").join(id);
    let launch: LaunchFile = serde_json::from_slice(&fs::read(sdir.join("launch.json"))?)
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
    cmd.cwd(&launch.cwd);
    let child = pair.slave.spawn_command(cmd).map_err(io::Error::other)?;
    drop(pair.slave);
    let child_pid = child
        .process_id()
        .ok_or_else(|| io::Error::other("child has no pid"))? as i32;
    let reader = pair.master.try_clone_reader().map_err(io::Error::other)?;
    let input = pair.master.take_writer().map_err(io::Error::other)?;

    crate::wire::write_atomic(
        &sdir.join("shim.pid"),
        std::process::id().to_string().as_bytes(),
    )?;
    let endpoint = Endpoint::new_session(dir, id)?;
    let listener = endpoint::bind(&endpoint)?;

    let shim = Arc::new(Shim {
        id: id.to_string(),
        child_pid,
        started_ms: now_ms(),
        endpoint,
        dir: dir.to_path_buf(),
        st: Mutex::new(State {
            ring: VecDeque::new(),
            conn: None,
            next_conn: 0,
            exit: None,
            exited_at: None,
        }),
        input: Mutex::new(input),
        master: Mutex::new(pair.master),
        log: Mutex::new(OutLog::open(sdir.join("output.log"))),
        reader_done: AtomicBool::new(false),
    });

    spawn_reader(&shim, reader);
    spawn_waiter(&shim, child);
    spawn_stat(&shim);
    spawn_reaper(&shim);

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let shim = shim.clone();
        let secret = adopt_secret.clone();
        thread::spawn(move || shim.handle(stream, &secret));
    }
    Ok(())
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
        let mut status: libc::c_int = 0;
        loop {
            let r = unsafe { libc::waitpid(shim.child_pid, &mut status, 0) };
            if r == shim.child_pid {
                break;
            }
            if r < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                status = 0;
                break;
            }
        }
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
        let mut st = lock(&shim.st);
        shim.send(&mut st, &FromShim::Stat(stat));
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
        let mut tb = libc::mach_timebase_info { numer: 0, denom: 0 };
        if unsafe { libc::mach_timebase_info(&mut tb) } != 0 || tb.denom == 0 {
            return None;
        }
        let ticks = info.pti_total_user as u128 + info.pti_total_system as u128;
        Some((ticks * tb.numer as u128 / tb.denom as u128) as u64)
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
    use super::proc;
    use std::process::Command;
    use std::time::Duration;

    #[test]
    fn cpu_time_is_in_nanoseconds() {
        let mut busy = Command::new("/bin/sh")
            .args(["-c", "while :; do :; done"])
            .spawn()
            .unwrap();
        let pid = busy.id() as i32;
        std::thread::sleep(Duration::from_millis(200));
        let a = proc::cpu_ns(pid).unwrap();
        std::thread::sleep(Duration::from_millis(600));
        let b = proc::cpu_ns(pid).unwrap();
        busy.kill().unwrap();
        busy.wait().unwrap();
        let delta = b - a;
        assert!(
            (150_000_000..1_500_000_000).contains(&delta),
            "cpu delta {delta} ns over 600 ms"
        );
    }
}
