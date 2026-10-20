use super::attach::{pending_signal, Guard};
use super::remote::RunView;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use std::io::{self, Read, Write};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::thread;
use std::time::{Duration, Instant};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::Snapshot;
use tmgr_ipc::proto::ServerMsg;
use tmgr_link::attach::{read_incoming, Incoming, Outgoing, MAX_INPUT_TEXT};
use tmgr_link::leader::Client;
use tmgr_runner::attach::{
    cursor, layout, mode_changes, render, Action, Cmd, InputSplitter, ScreenSource, SessionRow,
    ViewState,
};
use tmgr_runner::client::ScreenCache;
use tmgr_runner::sanitize::sanitize_text;
use tmgr_runner::wire::now_ms;

const TICK: Duration = Duration::from_millis(16);
const FRAME: Duration = Duration::from_millis(33);
const HEARTBEAT: Duration = Duration::from_secs(10);
const SILENCE: Duration = Duration::from_secs(45);
const NOTICE_FOR: Duration = Duration::from_secs(3);
const QUEUE: usize = 256;
const CLEANUP_WAIT: Duration = Duration::from_secs(1);

enum Msg {
    Stdin(Vec<u8>),
    StdinClosed,
    Remote(Box<Incoming>),
    Down(String),
}

enum Exit {
    Detached,
    Signal(i32),
    Lost(String),
    Closed(String),
}

struct Screens<'a>(&'a ScreenCache);

impl ScreenSource for Screens<'_> {
    fn snapshot(&self, session: &str) -> Option<&Snapshot> {
        self.0.snapshot(session)
    }
}

fn age(since: u64) -> String {
    let s = now_ms().saturating_sub(since) / 1000;
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m", s / 60),
        3600..=86399 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86400),
    }
}

pub fn text_chunks(pending: &mut Vec<u8>, bytes: &[u8]) -> Vec<String> {
    pending.extend_from_slice(bytes);
    let mut out = Vec::new();
    loop {
        match std::str::from_utf8(pending) {
            Ok(s) => {
                push_text(&mut out, s);
                pending.clear();
                return out;
            }
            Err(e) => {
                let valid = e.valid_up_to();
                if valid > 0 {
                    let text = String::from_utf8_lossy(&pending[..valid]).into_owned();
                    push_text(&mut out, &text);
                }
                match e.error_len() {
                    None => {
                        pending.drain(..valid);
                        return out;
                    }
                    Some(bad) => {
                        pending.drain(..valid + bad);
                    }
                }
            }
        }
    }
}

fn push_text(out: &mut Vec<String>, s: &str) {
    let mut rest = s;
    while !rest.is_empty() {
        let mut end = rest.len().min(MAX_INPUT_TEXT);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        out.push(rest[..end].to_string());
        rest = &rest[end..];
    }
}

struct App {
    run: String,
    input: bool,
    view: ViewState,
    cache: ScreenCache,
    splitter: InputSplitter,
    scroll_mode: bool,
    out: SyncSender<Outgoing>,
    utf8: Vec<u8>,
    dirty: bool,
    size: (u16, u16),
    sent_size: Option<(u16, u16)>,
    prev_modes: Option<tmgr_ipc::api::Modes>,
    scrolled: Option<u32>,
    notice_until: Option<Instant>,
    last_rx: Instant,
    last_ro_notice: Option<Instant>,
}

impl App {
    fn notify(&mut self, text: &str) {
        self.view.notice = Some(text.to_string());
        self.notice_until = Some(Instant::now() + NOTICE_FOR);
        self.dirty = true;
    }

    fn send(&mut self, m: Outgoing) {
        if self.out.try_send(m).is_err() {
            self.notify("the link is busy, input dropped");
        }
    }

    fn on_msg(&mut self, msg: Msg) -> io::Result<Option<Exit>> {
        match msg {
            Msg::Stdin(bytes) => return Ok(self.on_stdin(&bytes)),
            Msg::StdinClosed => return Ok(Some(Exit::Detached)),
            Msg::Down(why) => return Ok(Some(Exit::Lost(why))),
            Msg::Remote(m) => {
                self.last_rx = Instant::now();
                match *m {
                    Incoming::Screen(push) => {
                        if let Ok(v) = serde_json::to_value(&*push) {
                            if self.cache.apply(&ServerMsg::Screen(v)).is_some() {
                                self.dirty = true;
                            }
                        }
                    }
                    Incoming::Event(ev) => {
                        if let Some(row) =
                            self.view.sessions.iter_mut().find(|r| r.id == ev.session)
                        {
                            row.age = age(ev.state.since_ms);
                            row.state = ev.state;
                            self.dirty = true;
                        }
                    }
                    Incoming::Notice(text) => self.notify(&text),
                    Incoming::Pong => {}
                    Incoming::Closed(why) => return Ok(Some(Exit::Closed(why))),
                }
            }
        }
        Ok(None)
    }

    fn page(&self) -> usize {
        let area = Rect::new(0, 0, self.size.0, self.size.1);
        let lay = layout(area, &self.view);
        lay.panes
            .iter()
            .find(|p| p.session == self.run)
            .map_or(1, |p| usize::from(p.inner.height.saturating_sub(1)).max(1))
    }

    fn sync_scroll_max(&mut self) {
        let max = self.cache.snapshot(&self.run).map(|s| s.scroll_max);
        if let (Some(sc), Some(max)) = (self.view.scroll.as_mut(), max) {
            sc.max = max;
        }
    }

    fn on_stdin(&mut self, bytes: &[u8]) -> Option<Exit> {
        let actions = self.splitter.feed(bytes);
        self.act(actions)
    }

    fn act(&mut self, actions: Vec<Action>) -> Option<Exit> {
        for action in actions {
            let action = match action {
                Action::Mouse(_) => continue,
                Action::Drop(data) | Action::Paste { data, .. } => Action::Forward(data),
                other => other,
            };
            self.dirty = true;
            match action {
                Action::Forward(data) => {
                    if std::mem::take(&mut self.view.help) {
                        continue;
                    }
                    if !self.input {
                        let now = Instant::now();
                        if self
                            .last_ro_notice
                            .is_none_or(|t| now.duration_since(t) > NOTICE_FOR)
                        {
                            self.last_ro_notice = Some(now);
                            self.notify("read-only: this attach does not send input");
                        }
                        continue;
                    }
                    let mut utf8 = std::mem::take(&mut self.utf8);
                    for text in text_chunks(&mut utf8, &data) {
                        self.send(Outgoing::Input(text));
                    }
                    self.utf8 = utf8;
                }
                Action::Cmd(Cmd::Detach) => return Some(Exit::Detached),
                Action::Cmd(Cmd::Picker) => self.notify("one run per remote attach"),
                Action::Cmd(Cmd::Stop | Cmd::Remove | Cmd::Rename) => {
                    self.notify("not available over a remote attach")
                }
                Action::Cmd(cmd) => {
                    self.view.apply(cmd);
                    if cmd == Cmd::ScrollMode {
                        self.scroll_mode = self.view.scroll.is_some();
                        self.splitter.set_scroll_mode(self.scroll_mode);
                    }
                    if let Some(n) = self.view.notice.clone() {
                        self.notify(&n);
                    }
                }
                Action::Scroll(key) => {
                    self.sync_scroll_max();
                    let page = self.page();
                    self.view.scroll_key(key, page);
                }
                Action::Mouse(_) | Action::Drop(_) | Action::Paste { .. } => {}
            }
        }
        None
    }

    fn tick(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        last_draw: &mut Instant,
    ) -> io::Result<Option<Exit>> {
        if self.last_rx.elapsed() > SILENCE {
            return Ok(Some(Exit::Lost("the follower stopped answering".into())));
        }
        let stale = self.splitter.flush_stale(Instant::now());
        if !stale.is_empty() {
            if let Some(exit) = self.act(stale) {
                return Ok(Some(exit));
            }
        }
        if let Ok(size) = ratatui::crossterm::terminal::size() {
            if size != self.size {
                self.size = size;
                self.dirty = true;
            }
        }
        self.view.width = self.size.0;
        if self.notice_until.is_some_and(|t| Instant::now() >= t) {
            self.notice_until = None;
            self.view.notice = None;
            self.dirty = true;
        }
        if self.scroll_mode && self.view.scroll.is_none() {
            self.scroll_mode = false;
            self.splitter.set_scroll_mode(false);
        }
        self.sync_scroll_max();
        let lay = layout(Rect::new(0, 0, self.size.0, self.size.1), &self.view);
        if self.input {
            if let Some(p) = lay.panes.iter().find(|p| p.session == self.run) {
                let want = (p.inner.width, p.inner.height);
                if self.sent_size != Some(want) && want.0 > 0 && want.1 > 0 {
                    self.sent_size = Some(want);
                    self.send(Outgoing::Resize {
                        cols: want.0,
                        rows: want.1,
                    });
                }
            }
        }
        let desired = self.view.scroll.map(|s| s.offset as u32);
        if desired != self.scrolled {
            self.send(Outgoing::Scroll(desired.unwrap_or(0)));
            self.scrolled = desired;
        }
        if self.input {
            if let Some(modes) = self.cache.snapshot(&self.run).map(|s| s.modes) {
                let bytes = mode_changes(self.prev_modes, modes);
                if !bytes.is_empty() {
                    let out = terminal.backend_mut();
                    out.write_all(&bytes)?;
                    out.flush()?;
                }
                self.prev_modes = Some(modes);
            }
        }
        if self.dirty && last_draw.elapsed() >= FRAME {
            terminal.draw(|f| {
                let screens = Screens(&self.cache);
                render(f, &self.view, &screens);
                let lay = layout(f.area(), &self.view);
                if let Some(pos) = cursor(&self.view, &lay, &screens) {
                    f.set_cursor_position(pos);
                }
            })?;
            self.dirty = false;
            *last_draw = Instant::now();
        }
        Ok(None)
    }
}

fn event_loop(
    app: &mut App,
    rx: &mpsc::Receiver<Msg>,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Exit {
    let mut last_draw = Instant::now() - FRAME;
    loop {
        let sig = pending_signal();
        if sig != 0 {
            return Exit::Signal(sig);
        }
        let mut batch = Vec::new();
        match rx.recv_timeout(TICK) {
            Ok(m) => batch.push(m),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Exit::Lost("internal error".into()),
        }
        while batch.len() < 256 {
            match rx.try_recv() {
                Ok(m) => batch.push(m),
                Err(_) => break,
            }
        }
        for msg in batch {
            match app.on_msg(msg) {
                Ok(None) => {}
                Ok(Some(exit)) => return exit,
                Err(e) => return Exit::Lost(e.to_string()),
            }
        }
        match app.tick(terminal, &mut last_draw) {
            Ok(None) => {}
            Ok(Some(exit)) => return exit,
            Err(e) => return Exit::Lost(e.to_string()),
        }
    }
}

pub fn run(link: &str, mut client: Client, target: &RunView, input: bool) -> Result<i32> {
    let (mut reader, mut writer) = client.take_stream()?;
    let row = SessionRow {
        id: target.id.clone(),
        name: target.name.clone(),
        state: target.state.clone(),
        age: age(target.state.since_ms),
        esc: false,
    };
    let mut view = ViewState::new(vec![row], &format!("link:{link}"));
    view.set_initial(&target.id);
    view.sidebar = Some(false);
    let guard = Guard::enter().map_err(|e| Error::internal(format!("terminal setup: {e}")))?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))
        .map_err(|e| Error::internal(format!("terminal setup: {e}")))?;
    let _ = terminal.clear();

    let (tx, rx) = mpsc::sync_channel::<Msg>(QUEUE);
    let (out_tx, out_rx) = mpsc::sync_channel::<Outgoing>(QUEUE);

    let stdin_tx = tx.clone();
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut stdin = io::stdin().lock();
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let _ = stdin_tx.send(Msg::StdinClosed);
                    return;
                }
                Ok(n) => {
                    if stdin_tx.send(Msg::Stdin(buf[..n].to_vec())).is_err() {
                        return;
                    }
                }
            }
        }
    });

    let run_id = target.id.clone();
    let remote_tx = tx.clone();
    thread::spawn(move || loop {
        let msg = match read_incoming(&mut reader, &run_id) {
            Ok(Some(m)) => Msg::Remote(Box::new(m)),
            Ok(None) => Msg::Down("the follower closed the connection".into()),
            Err(e) => Msg::Down(e),
        };
        let stop = matches!(msg, Msg::Down(_));
        if remote_tx.send(msg).is_err() || stop {
            return;
        }
    });

    let hb = out_tx.clone();
    thread::spawn(move || {
        while hb.send(Outgoing::Ping).is_ok() {
            thread::sleep(HEARTBEAT);
        }
    });
    thread::spawn(move || {
        while let Ok(m) = out_rx.recv() {
            let last = m == Outgoing::Detach;
            if tmgr_link::attach::write_outgoing(&mut writer, &m).is_err() || last {
                return;
            }
        }
    });

    let mut app = App {
        run: target.id.clone(),
        input,
        view,
        cache: ScreenCache::new(),
        splitter: InputSplitter::new(),
        scroll_mode: false,
        out: out_tx,
        utf8: Vec::new(),
        dirty: true,
        size: (0, 0),
        sent_size: None,
        prev_modes: None,
        scrolled: None,
        notice_until: None,
        last_rx: Instant::now(),
        last_ro_notice: None,
    };
    if !input {
        app.notify("read-only attach");
    }
    let exit = event_loop(&mut app, &rx, &mut terminal);
    let name = sanitize_text(&target.name).into_owned();
    drop(terminal);
    drop(guard);
    drop(rx);
    let _ = app.out.try_send(Outgoing::Detach);
    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        drop(app);
        drop(client);
        let _ = done_tx.send(());
    });
    let _ = done_rx.recv_timeout(CLEANUP_WAIT);

    match exit {
        Exit::Detached => {
            let _ = writeln!(
                io::stdout(),
                "detached from {name} on {link} - it keeps running there"
            );
            Ok(0)
        }
        Exit::Signal(sig) => Ok(128 + sig),
        Exit::Closed(why) => {
            let _ = writeln!(io::stdout(), "{name} on {link}: stream ended ({why})");
            Ok(0)
        }
        Exit::Lost(why) => Err(Error::new(
            Kind::Offline,
            format!("lost the connection to {link}: {}", sanitize_text(&why)),
        )
        .hint(format!("{} agents --host {link}", super::bin()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use tmgr_ipc::api::{Attr, Color, Line, Modes, ScreenPush, Span, State};

    const SID: &str = "s_abcdefghij";

    fn hostile_push() -> ScreenPush {
        let attr = Attr {
            fg: Color::Default,
            bg: Color::Default,
            flags: 0xff,
        };
        let spans = [
            "\u{1b}]52;c;aGk=\u{7}",
            "\u{1b}[2J\u{1b}[H",
            "\u{9b}31m\u{9d}0;owned\u{9c}",
            "\u{1b}P1$r\u{1b}\\",
            "\u{202e}rtl\u{2066}",
            "plain",
        ]
        .map(|t| Span {
            a: attr,
            t: t.to_string(),
        });
        let modes = Modes {
            app_cursor: false,
            bracketed_paste: false,
            alt_screen: false,
            hide_cursor: false,
            mouse: Default::default(),
        };
        ScreenPush {
            session: SID.into(),
            full: true,
            snapshot: Some(Snapshot {
                rows: 2,
                cols: 60,
                lines: vec![Line {
                    spans: spans.to_vec(),
                }],
                cursor: Some((0, 3)),
                modes,
                scroll_offset: 0,
                scroll_max: 0,
            }),
            lines: vec![],
            cursor: Some((0, 3)),
            modes: Some(modes),
            scroll_offset: 0,
            scroll_max: 0,
        }
    }

    fn drawn(push: ScreenPush) -> Vec<String> {
        let mut cache = ScreenCache::new();
        cache.apply(&ServerMsg::Screen(serde_json::to_value(push).unwrap()));
        let row = SessionRow {
            id: SID.into(),
            name: "run".into(),
            state: State {
                kind: tmgr_ipc::api::StateKind::Working,
                since_ms: 0,
                detail: None,
                exit_code: None,
            },
            age: "1s".into(),
            esc: false,
        };
        let mut view = ViewState::new(vec![row], "link:x");
        view.set_initial(SID);
        view.sidebar = Some(false);
        let mut term = Terminal::new(TestBackend::new(80, 12)).unwrap();
        term.draw(|f| render(f, &view, &Screens(&cache))).unwrap();
        term.backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol().to_string())
            .collect()
    }

    #[test]
    fn the_link_filter_matches_the_runner_allowlist_character_for_character() {
        for code in (0u32..0x3000).chain(0xfe00..0x10000) {
            let Some(c) = char::from_u32(code) else {
                continue;
            };
            let s = c.to_string();
            assert_eq!(
                tmgr_link::attach::clean_text(&s),
                sanitize_text(&s).into_owned(),
                "U+{code:04X}"
            );
        }
    }

    #[test]
    fn hostile_follower_screens_never_reach_the_terminal_as_control_characters() {
        let guarded = tmgr_link::attach::guard_screen(hostile_push(), SID)
            .unwrap()
            .unwrap();
        for push in [hostile_push(), guarded] {
            let cells = drawn(push);
            assert!(cells.iter().any(|c| c == "p"), "the screen was not drawn");
            for cell in &cells {
                assert!(
                    !cell
                        .chars()
                        .any(|c| c.is_control() || ('\u{80}'..='\u{9f}').contains(&c)),
                    "{cell:?}"
                );
                assert!(!cell.contains('\u{202e}') && !cell.contains('\u{2066}'));
            }
        }
    }

    #[test]
    fn utf8_split_across_reads_is_kept_whole() {
        let mut pending = Vec::new();
        let bytes = "é世".as_bytes();
        assert!(text_chunks(&mut pending, &bytes[..1]).is_empty());
        assert_eq!(text_chunks(&mut pending, &bytes[1..]), ["é世"]);
        assert!(pending.is_empty());
    }

    #[test]
    fn invalid_bytes_are_dropped_and_long_input_is_split() {
        let mut pending = Vec::new();
        assert_eq!(text_chunks(&mut pending, b"a\xffb"), ["a", "b"]);
        let big = "x".repeat(MAX_INPUT_TEXT * 2 + 5);
        let chunks = text_chunks(&mut pending, big.as_bytes());
        assert_eq!(chunks.len(), 3);
        assert!(chunks.iter().all(|c| c.len() <= MAX_INPUT_TEXT));
    }
}
