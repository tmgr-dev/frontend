use super::attach_links::{followers, Dialer, Hub, LinkMsg, LinkState, SshDialer, Target};
use super::remote_attach::age;
use ratatui::layout::Rect;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tmgr_ipc::proto::valid_session_id;
use tmgr_link::store::Paths;
use tmgr_runner::attach::{
    prefs_without_ids, Cmd, Control, LayoutPrefs, Machine, MachineState, ScrollKey, SessionRow,
    ViewState,
};
use tmgr_runner::client::ScreenCache;
use tmgr_runner::sanitize::sanitize_text;

const SAY_EVERY: Duration = Duration::from_secs(5);
const QUEUE: usize = 256;

#[derive(Default)]
pub(super) struct Applied {
    pub dirty: bool,
    pub notice: Option<String>,
    pub changed: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Sent {
    Local,
    Handled(Option<String>),
}

pub(super) struct Remote {
    hub: Option<Hub>,
    states: Vec<(String, LinkState)>,
    inputs: HashMap<String, bool>,
    sizes: HashMap<String, (u16, u16)>,
    said: HashMap<String, Instant>,
    over: HashSet<String>,
}

pub(super) fn machine_state(state: &LinkState) -> Option<MachineState> {
    Some(match state {
        LinkState::Connecting => MachineState::Connecting,
        LinkState::Online { latency_ms } => MachineState::Online {
            latency_ms: *latency_ms,
        },
        LinkState::Busy => MachineState::Busy,
        LinkState::Offline => MachineState::Offline,
        LinkState::NoAccess => MachineState::NoAccess,
        LinkState::HostKeyChanged => MachineState::HostKeyChanged,
        LinkState::Removed => return None,
    })
}

pub(super) fn local_rows(rows: &[SessionRow]) -> Vec<SessionRow> {
    rows.iter()
        .filter(|r| valid_session_id(&r.id))
        .cloned()
        .collect()
}

pub(super) fn attention_session(target: Option<&str>) -> Option<&str> {
    target.filter(|id| valid_session_id(id))
}

pub(super) fn prefs_for_runner(prefs: LayoutPrefs) -> LayoutPrefs {
    prefs_without_ids(prefs, |id| !valid_session_id(id))
}

pub(super) type Sized = (String, u16, u16);

pub(super) fn split_sizes(panes: Vec<Sized>) -> (Vec<Sized>, Vec<Sized>) {
    panes.into_iter().partition(|p| valid_session_id(&p.0))
}

fn allows(cmd: Cmd) -> bool {
    matches!(
        cmd,
        Cmd::Detach
            | Cmd::NextSession
            | Cmd::PrevSession
            | Cmd::Split
            | Cmd::FocusOther
            | Cmd::FocusPrev
            | Cmd::Focus(_)
            | Cmd::Resize(_)
            | Cmd::Zoom
            | Cmd::Sidebar
            | Cmd::SidebarNarrower
            | Cmd::SidebarWider
            | Cmd::Group
            | Cmd::ScrollMode
            | Cmd::Help
            | Cmd::Picker
            | Cmd::Stop
            | Cmd::Palette
            | Cmd::NewSession
            | Cmd::NextTab
            | Cmd::PrevTab
            | Cmd::RenameTab
            | Cmd::CloseTab
            | Cmd::NeedsYou
            | Cmd::Broadcast
            | Cmd::BroadcastMark
            | Cmd::Prompts
            | Cmd::Layouts
            | Cmd::Filter
            | Cmd::Reconnect
    )
}

fn link_of(id: &str) -> &str {
    id.split_once('/').map_or("linked", |(l, _)| l)
}

fn refusal(link: &str, resume: bool) -> String {
    if resume {
        format!("resume works only on {link}; start it again with: tmgr run --on {link}")
    } else {
        format!("not available for {link} sessions")
    }
}

impl Remote {
    pub(super) fn off() -> Remote {
        Remote {
            hub: None,
            states: Vec::new(),
            inputs: HashMap::new(),
            sizes: HashMap::new(),
            said: HashMap::new(),
            over: HashSet::new(),
        }
    }

    pub(super) fn start(
        wrap: impl Fn(LinkMsg) -> bool + Send + 'static,
    ) -> (Remote, Option<String>) {
        let links = Paths::from_env().map(|p| followers(&p)).unwrap_or_default();
        if links.is_empty() {
            return (Remote::off(), None);
        }
        match SshDialer::new() {
            Ok(dialer) => (Remote::start_with(Arc::new(dialer), links, wrap), None),
            Err(e) => (
                Remote::off(),
                Some(format!(
                    "linked machines unavailable: {}",
                    sanitize_text(&e.message)
                )),
            ),
        }
    }

    pub(super) fn start_with(
        dialer: Arc<dyn Dialer>,
        links: Vec<String>,
        wrap: impl Fn(LinkMsg) -> bool + Send + 'static,
    ) -> Remote {
        let (tx, rx) = mpsc::sync_channel(QUEUE);
        let hub = Hub::start(dialer, links.clone(), tx);
        std::thread::spawn(move || {
            for msg in rx {
                if !wrap(msg) {
                    break;
                }
            }
        });
        Remote::with_hub(hub, &links)
    }

    pub(super) fn with_hub(hub: Hub, links: &[String]) -> Remote {
        let mut remote = Remote::off();
        remote.states = links
            .iter()
            .filter(|l| tmgr_link::grammar::link_name(l).is_ok())
            .map(|l| (l.clone(), LinkState::Connecting))
            .collect();
        remote.hub = Some(hub);
        remote
    }

    pub(super) fn shutdown(&mut self) {
        if let Some(hub) = self.hub.take() {
            hub.shutdown();
        }
    }

    pub(super) fn route<'a>(&self, id: &'a str) -> Target<'a> {
        match &self.hub {
            Some(hub) => hub.target_of(id),
            None if valid_session_id(id) => Target::Local(id),
            None => Target::Unknown,
        }
    }

    fn say(&mut self, key: String, text: String) -> Option<String> {
        let now = Instant::now();
        if self
            .said
            .get(&key)
            .is_some_and(|at| now.duration_since(*at) < SAY_EVERY)
        {
            return None;
        }
        self.said.insert(key, now);
        Some(text)
    }

    pub(super) fn merge(
        &self,
        view: &mut ViewState,
        local: Vec<SessionRow>,
    ) -> (Vec<SessionRow>, bool) {
        let Some(hub) = &self.hub else {
            return (local, false);
        };
        let mut rows = local;
        let mut remote = HashMap::new();
        let mut machines = Vec::new();
        for (link, state) in &self.states {
            let Some(state) = machine_state(state) else {
                continue;
            };
            machines.push(Machine {
                name: link.clone(),
                state,
            });
            for row in hub.rows(link) {
                remote.insert(row.id.clone(), link.clone());
                rows.push(row);
            }
        }
        let changed = view.machines != machines || view.remote != remote;
        view.machines = machines;
        view.remote = remote;
        (rows, changed)
    }

    fn sync(&self, view: &mut ViewState, cache: &mut ScreenCache, area: Rect) -> bool {
        let (rows, meta) = self.merge(view, local_rows(&view.sessions));
        if rows == view.sessions {
            return meta;
        }
        for old in &view.sessions {
            if !rows.iter().any(|r| r.id == old.id) {
                cache.remove(&old.id);
            }
        }
        view.sessions_changed(area, rows);
        true
    }

    pub(super) fn on_link(
        &mut self,
        msg: LinkMsg,
        view: &mut ViewState,
        cache: &mut ScreenCache,
        area: Rect,
    ) -> Applied {
        let mut fx = Applied::default();
        match msg {
            LinkMsg::State { link, state } => {
                let Some((_, slot)) = self.states.iter_mut().find(|(l, _)| *l == link) else {
                    return fx;
                };
                *slot = state;
                fx.dirty = self.sync(view, cache, area);
                self.drop_stale_confirm(view, &mut fx);
            }
            LinkMsg::Runs { link, runs } => {
                if self.states.iter().any(|(l, _)| *l == link) {
                    let prefix = format!("{link}/");
                    let listed = |id: &str| {
                        id.strip_prefix(&prefix)
                            .is_none_or(|run| runs.iter().any(|r| r.id == run))
                    };
                    self.inputs.retain(|id, _| listed(id));
                    self.sizes.retain(|id, _| listed(id));
                    fx.dirty = self.sync(view, cache, area);
                    self.drop_stale_confirm(view, &mut fx);
                }
            }
            LinkMsg::Opened { id, input } => {
                if matches!(self.route(&id), Target::Remote { .. }) {
                    self.inputs.insert(id, input);
                    fx.dirty = true;
                }
            }
            LinkMsg::Screen(push, _permit) => {
                if !matches!(self.route(&push.session), Target::Remote { .. }) {
                    return fx;
                }
                fx.changed = cache.apply_push(*push);
                fx.dirty = fx.changed.is_some();
            }
            LinkMsg::Event(ev, _permit) => {
                if !matches!(self.route(&ev.session), Target::Remote { .. }) {
                    return fx;
                }
                if let Some(row) = view.sessions.iter_mut().find(|r| r.id == ev.session) {
                    row.age = age(ev.state.since_ms);
                    row.state = ev.state;
                    fx.dirty = true;
                }
            }
            LinkMsg::Notice { link, id, text } => {
                let source = id
                    .and_then(|id| view.sessions.iter().find(|r| r.id == id))
                    .map_or_else(
                        || link.clone(),
                        |r| format!("{link}/{}", sanitize_text(&r.name)),
                    );
                fx.notice = Some(format!("{source}: {}", sanitize_text(&text)));
            }
            LinkMsg::StreamClosed { id, why, reopen } => {
                self.inputs.remove(&id);
                self.sizes.remove(&id);
                if !reopen {
                    let why = sanitize_text(&why);
                    let link = link_of(&id);
                    fx.notice = Some(if why.starts_with(link) {
                        why.into_owned()
                    } else {
                        format!("{link}: {why}")
                    });
                }
            }
            LinkMsg::Halted { id, result } => {
                let link = link_of(&id).to_string();
                let name = view
                    .sessions
                    .iter()
                    .find(|r| r.id == id)
                    .map_or_else(|| id.clone(), |r| r.name.clone());
                fx.notice = Some(match result {
                    Ok(()) => format!("stopped {} on {link}", sanitize_text(&name)),
                    Err(e) => format!("{link}: {}", sanitize_text(&e)),
                });
            }
        }
        fx
    }

    fn drop_stale_confirm(&self, view: &mut ViewState, fx: &mut Applied) {
        let Some(c) = view.confirm.as_ref() else {
            return;
        };
        let Some((link, _)) = c.session.split_once('/') else {
            return;
        };
        let up = self
            .states
            .iter()
            .any(|(l, s)| l == link && !matches!(s, LinkState::Removed | LinkState::NoAccess));
        if up && view.sessions.iter().any(|r| r.id == c.session) {
            return;
        }
        fx.notice = Some(format!("{} is gone", sanitize_text(&c.name)));
        fx.dirty = true;
        view.confirm = None;
    }

    pub(super) fn send(&mut self, id: &str, data: &[u8]) -> Sent {
        match self.route(id) {
            Target::Local(_) => Sent::Local,
            Target::Remote { link, .. } => {
                let Some(hub) = &self.hub else {
                    return Sent::Handled(None);
                };
                let note = match hub.input(id, data) {
                    Ok(()) => return Sent::Handled(None),
                    Err("read-only on this machine") => format!("read-only on {link}"),
                    Err("not attached" | "not attached yet") => format!("not connected to {link}"),
                    Err(e) => format!("{link}: {e}"),
                };
                Sent::Handled(self.say(format!("input:{link}"), note))
            }
            Target::Unknown => Sent::Handled(self.say(
                "input:gone".into(),
                "that session is no longer available".into(),
            )),
        }
    }

    pub(super) fn stop(&self, id: &str) -> Option<Option<String>> {
        match self.route(id) {
            Target::Local(_) => None,
            Target::Remote { link, run } => Some(
                self.hub
                    .as_ref()
                    .and_then(|h| h.halt(&link, &run).err())
                    .map(|e| format!("{link}: {e}")),
            ),
            Target::Unknown => Some(Some("that session is no longer available".into())),
        }
    }

    pub(super) fn scroll(&self, id: &str, offset: u32) -> bool {
        match self.route(id) {
            Target::Local(_) => false,
            Target::Remote { .. } => {
                if let Some(hub) = &self.hub {
                    hub.scroll(id, offset);
                }
                true
            }
            Target::Unknown => true,
        }
    }

    pub(super) fn reconnect(&self) -> String {
        match &self.hub {
            Some(hub) => {
                hub.retry_all();
                "reconnecting linked machines".into()
            }
            None => "no linked machines (tmgr link)".into(),
        }
    }

    pub(super) fn control_gate(&self, id: &str, ctl: Control) -> Option<String> {
        match self.route(id) {
            Target::Local(_) => None,
            Target::Remote { link, .. } => match ctl {
                Control::Stop | Control::Interrupt => None,
                Control::Resume => Some(refusal(&link, true)),
                Control::Close => Some(refusal(&link, false)),
            },
            Target::Unknown => Some("that session is no longer available".into()),
        }
    }

    pub(super) fn cmd_gate(&self, cmd: Cmd, focused: Option<&str>) -> Option<String> {
        let id = focused?;
        let link = match self.route(id) {
            Target::Local(_) => return None,
            Target::Remote { link, .. } => link,
            Target::Unknown if id.contains('/') => link_of(id).to_string(),
            Target::Unknown => return None,
        };
        if allows(cmd) {
            None
        } else {
            Some(refusal(&link, cmd == Cmd::Resume))
        }
    }

    pub(super) fn scroll_key_gate(&self, key: ScrollKey, focused: Option<&str>) -> Option<String> {
        let id = focused?;
        if !matches!(
            key,
            ScrollKey::Search | ScrollKey::CopyMode | ScrollKey::Select
        ) {
            return None;
        }
        match self.route(id) {
            Target::Local(_) => None,
            Target::Remote { link, .. } => Some(refusal(&link, false)),
            Target::Unknown if id.contains('/') => Some(refusal(link_of(id), false)),
            Target::Unknown => None,
        }
    }

    pub(super) fn tick(&mut self, panes: &[Sized]) -> Vec<String> {
        let Some(hub) = &self.hub else {
            return Vec::new();
        };
        let mut notes = Vec::new();
        let visible: Vec<String> = panes
            .iter()
            .filter(|p| matches!(hub.target_of(&p.0), Target::Remote { .. }))
            .map(|p| p.0.clone())
            .collect();
        let wants = hub.want_streams(&visible);
        for id in &wants.close {
            hub.close(id);
            self.inputs.remove(id);
            self.sizes.remove(id);
        }
        for (id, link, run) in &wants.open {
            let Some(&(_, cols, rows)) = panes.iter().find(|p| p.0 == *id) else {
                continue;
            };
            match hub.open(id, link, run, cols, rows) {
                Ok(()) => {
                    self.sizes.insert(id.clone(), (cols, rows));
                }
                Err("the machine is not online" | "try again in a moment") => {}
                Err(e) => notes.push(format!("{link}: {e}")),
            }
        }
        let over: HashSet<String> = wants.over_budget.iter().cloned().collect();
        for id in &over {
            if !self.over.contains(id) {
                notes.push(format!("too many remote panes on {}", link_of(id)));
            }
        }
        self.over = over;
        for (id, cols, rows) in panes {
            if self.inputs.get(id) == Some(&true) && self.sizes.get(id) != Some(&(*cols, *rows)) {
                hub.resize(id, *cols, *rows);
                self.sizes.insert(id.clone(), (*cols, *rows));
            }
        }
        notes
            .into_iter()
            .filter_map(|n| self.say(format!("tick:{n}"), n))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::attach_links::tests::{fast, push, state, Fake, FakeDialer, A, B, C, LOCAL};
    use super::super::attach_links::{DialErr, Permit};
    use super::*;
    use std::sync::mpsc::Receiver;
    use tmgr_core::error::Kind;
    use tmgr_ipc::api::{EventMsg, StateKind};
    use tmgr_ipc::proto::ServerMsg;
    use tmgr_link::attach::{encode_event, encode_screen, Outgoing};
    use tmgr_runner::attach::{Confirm, LayoutPrefs, Node, TabPrefs};

    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 120,
        height: 30,
    };

    fn row(id: &str) -> SessionRow {
        SessionRow {
            id: id.into(),
            name: format!("name {id}"),
            state: state(StateKind::Working),
            age: "1m".into(),
            you: None,
            esc: false,
            resumed_from: None,
        }
    }

    struct Rig {
        fake: Arc<Fake>,
        remote: Remote,
        rx: Receiver<LinkMsg>,
        view: ViewState,
        cache: ScreenCache,
        notes: Vec<String>,
        panes: Vec<Sized>,
    }

    fn rig(links: &[&str], setup: impl FnOnce(&Fake)) -> Rig {
        let fake = Arc::new(Fake::default());
        setup(&fake);
        let (tx, rx) = mpsc::sync_channel(512);
        let names: Vec<String> = links.iter().map(|l| l.to_string()).collect();
        let hub = Hub::start_with(
            Arc::new(FakeDialer(Arc::clone(&fake))),
            names.clone(),
            tx,
            fast(),
        );
        let mut view = ViewState::new(vec![row(LOCAL)], "test");
        view.width = 120;
        Rig {
            fake,
            remote: Remote::with_hub(hub, &names),
            rx,
            view,
            cache: ScreenCache::new(),
            notes: Vec::new(),
            panes: Vec::new(),
        }
    }

    impl Rig {
        fn apply(&mut self, msg: LinkMsg) {
            let fx = self
                .remote
                .on_link(msg, &mut self.view, &mut self.cache, AREA);
            self.notes.extend(fx.notice);
        }

        fn until(&mut self, what: &str, mut ok: impl FnMut(&mut Rig) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if ok(self) {
                    return;
                }
                assert!(Instant::now() < deadline, "timed out waiting for {what}");
                if let Ok(msg) = self.rx.recv_timeout(Duration::from_millis(10)) {
                    self.apply(msg);
                }
            }
        }

        fn settle(&mut self, wait: Duration) {
            let end = Instant::now() + wait;
            while Instant::now() < end {
                if let Ok(msg) = self.rx.recv_timeout(Duration::from_millis(10)) {
                    self.apply(msg);
                }
            }
        }

        fn rows(&mut self, n: usize) {
            self.until("rows", |r| r.view.sessions.len() == n);
        }

        fn pane(&mut self, link: &str, run: &str) -> String {
            let id = format!("{link}/{run}");
            self.panes.push((id.clone(), 80, 24));
            self.until("pane open", |r| {
                let panes = r.panes.clone();
                r.remote.tick(&panes);
                r.remote.inputs.contains_key(&id)
            });
            id
        }

        fn written(&self, link: &str, run: &str) -> Vec<Outgoing> {
            self.fake.written(link, run)
        }

        fn typed(&self, link: &str, run: &str) -> Vec<Outgoing> {
            self.written(link, run)
                .into_iter()
                .filter(|o| matches!(o, Outgoing::Input(_)))
                .collect()
        }
    }

    fn mstate(view: &ViewState, name: &str) -> Option<MachineState> {
        view.machines
            .iter()
            .find(|m| m.name == name)
            .map(|m| m.state.clone())
    }

    fn two_links(f: &Fake) {
        f.set_runs("spark", &[A, B]);
        f.set_runs("lab", &[C]);
        f.with("spark", |l| l.input = true);
        f.with("lab", |l| l.input = true);
    }

    #[test]
    fn the_merged_list_is_namespaced_and_a_down_machine_keeps_its_rows_greyed() {
        let mut r = rig(&["spark", "lab"], two_links);
        r.rows(4);
        r.until("online", |r| {
            r.view
                .machines
                .iter()
                .all(|m| matches!(m.state, MachineState::Online { .. }))
                && r.view.machines.len() == 2
        });
        let mut ids: Vec<&str> = r.view.sessions.iter().map(|s| s.id.as_str()).collect();
        ids.sort_unstable();
        let want = [
            format!("lab/{C}"),
            LOCAL.to_string(),
            format!("spark/{A}"),
            format!("spark/{B}"),
        ];
        assert_eq!(ids, want.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(r.view.machine_of(&format!("spark/{A}")), Some("spark"));
        assert_eq!(r.view.machine_of(LOCAL), None);
        r.fake.with("lab", |l| l.list_err = Some(Kind::Link));
        r.until("lab offline", |r| {
            mstate(&r.view, "lab") == Some(MachineState::Offline)
        });
        assert!(r.view.machine_dimmed("lab"));
        assert!(!r.view.machine_dimmed("spark"));
        assert_eq!(r.view.sessions.len(), 4);
    }

    #[test]
    fn remote_input_reaches_only_that_links_stream_and_local_input_stays_local() {
        let mut r = rig(&["spark", "lab"], |f| {
            f.set_runs("spark", &[A]);
            f.set_runs("lab", &[A]);
            f.with("spark", |l| l.input = true);
            f.with("lab", |l| l.input = true);
        });
        r.rows(3);
        let id = r.pane("spark", A);
        r.pane("lab", A);
        assert_eq!(r.remote.send(&id, b"hi"), Sent::Handled(None));
        r.until("input written", |r| !r.typed("spark", A).is_empty());
        assert_eq!(r.typed("spark", A), vec![Outgoing::Input("hi".into())]);
        r.settle(Duration::from_millis(100));
        assert!(r.typed("lab", A).is_empty());
        assert_eq!(r.remote.send(LOCAL, b"x"), Sent::Local);
        r.settle(Duration::from_millis(100));
        assert_eq!(r.typed("spark", A).len(), 1);
        assert!(r.typed("lab", A).is_empty());
    }

    #[test]
    fn mixed_peers_each_take_their_own_route() {
        let mut r = rig(&["spark"], |f| {
            f.set_runs("spark", &[A]);
            f.with("spark", |l| l.input = true);
        });
        r.rows(2);
        let id = r.pane("spark", A);
        let routes: Vec<Sent> = [LOCAL, id.as_str(), LOCAL]
            .iter()
            .map(|p| r.remote.send(p, b"y"))
            .collect();
        assert_eq!(routes, vec![Sent::Local, Sent::Handled(None), Sent::Local]);
        r.until("remote got it once", |r| r.typed("spark", A).len() == 1);
    }

    #[test]
    fn stopping_a_remote_run_halts_it_on_the_right_link_only() {
        let mut r = rig(&["spark", "lab"], |f| {
            f.set_runs("spark", &[A]);
            f.set_runs("lab", &[A]);
        });
        r.rows(3);
        assert_eq!(r.remote.stop(&format!("lab/{A}")), Some(None));
        r.until("halt", |r| !r.fake.calls("lab", "runs.halt").is_empty());
        assert_eq!(r.fake.calls("lab", "runs.halt")[0]["run"], A);
        r.until("notice", |r| {
            r.notes.iter().any(|n| n.starts_with("stopped"))
        });
        assert!(r.fake.calls("spark", "runs.halt").is_empty());
        assert!(r.notes.iter().any(|n| n.ends_with("on lab")));
        assert_eq!(r.remote.stop(LOCAL), None);
    }

    #[test]
    fn unknown_ids_and_unlinked_machines_are_refused_and_nothing_is_sent() {
        let mut r = rig(&["spark"], |f| {
            f.set_runs("spark", &[A]);
            f.with("spark", |l| l.input = true);
        });
        r.rows(2);
        r.pane("spark", A);
        for bad in [
            format!("ghost/{A}"),
            format!("spark/{B}"),
            format!("spark/../{A}"),
            "garbage".to_string(),
            String::new(),
        ] {
            assert!(
                matches!(r.remote.send(&bad, b"z"), Sent::Handled(_)),
                "{bad:?}"
            );
            assert!(r.remote.stop(&bad).is_some_and(|n| n.is_some()), "{bad:?}");
            assert!(r.remote.control_gate(&bad, Control::Stop).is_some());
            assert!(r.remote.scroll(&bad, 3));
        }
        r.fake.with("spark", |l| {
            l.dial.push_back(Err(DialErr::Removed));
            l.list_err = Some(Kind::Link);
        });
        r.until("unlinked", |r| r.view.machines.is_empty());
        assert_eq!(r.view.sessions.len(), 1);
        let gone = format!("spark/{A}");
        assert!(matches!(r.remote.send(&gone, b"z"), Sent::Handled(_)));
        assert!(r.remote.stop(&gone).is_some_and(|n| n.is_some()));
        r.settle(Duration::from_millis(100));
        assert!(r.fake.calls("spark", "runs.halt").is_empty());
        assert!(r.typed("spark", A).is_empty());
    }

    #[test]
    fn a_read_only_run_gets_a_rate_limited_notice_and_no_input() {
        let mut r = rig(&["spark"], |f| f.set_runs("spark", &[A]));
        r.rows(2);
        let id = r.pane("spark", A);
        assert_eq!(
            r.remote.send(&id, b"hi"),
            Sent::Handled(Some("read-only on spark".into()))
        );
        assert_eq!(r.remote.send(&id, b"hi"), Sent::Handled(None));
        r.settle(Duration::from_millis(100));
        assert!(r.typed("spark", A).is_empty());
    }

    #[test]
    fn input_before_the_stream_is_up_says_not_connected() {
        let mut r = rig(&["spark"], |f| f.set_runs("spark", &[A]));
        r.rows(2);
        assert_eq!(
            r.remote.send(&format!("spark/{A}"), b"hi"),
            Sent::Handled(Some("not connected to spark".into()))
        );
    }

    #[test]
    fn a_forbidden_halt_shows_the_followers_answer() {
        let mut r = rig(&["spark"], |f| {
            f.set_runs("spark", &[A]);
            f.with("spark", |l| l.halt_err = Some(Kind::Forbidden));
        });
        r.rows(2);
        assert_eq!(r.remote.stop(&format!("spark/{A}")), Some(None));
        r.until("refusal", |r| {
            r.notes
                .iter()
                .any(|n| n.starts_with("spark:") && n.contains("scripted failure"))
        });
        assert!(!r.notes.iter().any(|n| n.starts_with("stopped")));
    }

    #[test]
    fn a_stream_lost_while_the_link_flaps_is_opened_again_after_it_is_back() {
        let mut r = rig(&["spark"], |f| {
            f.set_runs("spark", &[A]);
            f.with("spark", |l| l.input = true);
        });
        r.rows(2);
        let id = r.pane("spark", A);
        r.fake.hang_up("spark", A);
        r.until("closed", |r| !r.remote.inputs.contains_key(&id));
        r.fake.with("spark", |l| l.list_err = Some(Kind::Link));
        r.until("offline", |r| {
            mstate(&r.view, "spark") == Some(MachineState::Offline)
        });
        r.fake.with("spark", |l| l.list_err = None);
        r.until("online", |r| {
            matches!(mstate(&r.view, "spark"), Some(MachineState::Online { .. }))
        });
        r.pane("spark", A);
        assert!(r.remote.inputs[&id]);
    }

    #[test]
    fn panes_over_the_budget_say_so_once_and_a_resize_goes_to_the_stream() {
        let mut r = rig(&["spark"], |f| {
            f.set_runs("spark", &[A, B, C]);
            f.with("spark", |l| l.input = true);
        });
        r.rows(4);
        let (a, b) = (r.pane("spark", A), r.pane("spark", B));
        let c = format!("spark/{C}");
        let panes = [
            (a.clone(), 80, 24),
            (b.clone(), 80, 24),
            (c.clone(), 80, 24),
        ];
        let first = r.remote.tick(&panes);
        assert_eq!(first, vec!["too many remote panes on spark".to_string()]);
        assert!(r.remote.tick(&panes).is_empty());
        r.remote.tick(&[(a.clone(), 100, 30), (b.clone(), 80, 24)]);
        r.until("resize", |r| {
            r.written("spark", A).contains(&Outgoing::Resize {
                cols: 100,
                rows: 30,
            })
        });
    }

    #[test]
    fn scrolling_a_remote_pane_goes_to_its_stream_not_the_runner() {
        let mut r = rig(&["spark"], |f| {
            f.set_runs("spark", &[A]);
            f.with("spark", |l| l.input = true);
        });
        r.rows(2);
        let id = r.pane("spark", A);
        assert!(r.remote.scroll(&id, 7));
        assert!(!r.remote.scroll(LOCAL, 7));
        r.until("scroll", |r| {
            r.written("spark", A).contains(&Outgoing::Scroll(7))
        });
    }

    #[test]
    fn a_follower_listing_a_local_session_id_cannot_touch_the_local_row_or_screen() {
        let mut r = rig(&["spark"], |f| {
            f.set_runs("spark", &[LOCAL]);
            f.with("spark", |l| l.input = true);
        });
        r.rows(2);
        r.cache.apply(&ServerMsg::Screen(
            serde_json::to_value(push_full(LOCAL)).unwrap(),
        ));
        let before = r.view.sessions.iter().find(|s| s.id == LOCAL).cloned();
        let id = r.pane("spark", LOCAL);
        assert_eq!(id, format!("spark/{LOCAL}"));
        r.fake.feed(
            "spark",
            LOCAL,
            &encode_event(&EventMsg {
                session: LOCAL.into(),
                state: state(StateKind::Exited),
                task: None,
                origin: None,
            }),
        );
        r.fake
            .feed("spark", LOCAL, &encode_screen(&push_full(LOCAL)));
        r.until("screen under the view id", |r| {
            r.cache.snapshot(&id).is_some()
        });
        assert_eq!(
            r.view.sessions.iter().find(|s| s.id == LOCAL).cloned(),
            before
        );
        assert!(r.cache.snapshot(LOCAL).is_some());
        let seeded = format!("{:?}", r.cache.snapshot(LOCAL));
        let mut owned = push(LOCAL);
        owned.lines[0].1.spans[0].t = "owned".into();
        r.apply(LinkMsg::Screen(Box::new(owned), Permit::none()));
        assert_eq!(format!("{:?}", r.cache.snapshot(LOCAL)), seeded);
        r.apply(LinkMsg::Event(
            EventMsg {
                session: LOCAL.into(),
                state: state(StateKind::Exited),
                task: None,
                origin: None,
            },
            Permit::none(),
        ));
        assert_eq!(
            r.view.sessions.iter().find(|s| s.id == LOCAL).cloned(),
            before
        );
    }

    fn push_full(session: &str) -> tmgr_ipc::api::ScreenPush {
        let mut p = push(session);
        p.full = true;
        p.snapshot = Some(tmgr_ipc::api::Snapshot {
            rows: 1,
            cols: 10,
            lines: p.lines.iter().map(|(_, l)| l.clone()).collect(),
            cursor: None,
            modes: tmgr_ipc::api::Modes {
                app_cursor: false,
                bracketed_paste: false,
                alt_screen: false,
                hide_cursor: false,
                mouse: Default::default(),
            },
            scroll_offset: 0,
            scroll_max: 0,
        });
        p
    }

    #[test]
    fn the_command_gate_refuses_local_only_actions_on_a_remote_session() {
        let mut r = rig(&["spark"], |f| f.set_runs("spark", &[A]));
        r.rows(2);
        let remote = format!("spark/{A}");
        for cmd in [
            Cmd::Rename,
            Cmd::Remove,
            Cmd::Resume,
            Cmd::Export,
            Cmd::Notes,
            Cmd::Summary,
            Cmd::Git,
            Cmd::OpenTask,
            Cmd::Questions,
            Cmd::Subagents,
            Cmd::SplitRight,
            Cmd::SplitDown,
            Cmd::Templates,
            Cmd::Watch,
            Cmd::Search,
            Cmd::CopyMode,
        ] {
            assert!(
                r.remote.cmd_gate(cmd, Some(&remote)).is_some(),
                "{cmd:?} must be refused"
            );
            assert_eq!(r.remote.cmd_gate(cmd, Some(LOCAL)), None, "{cmd:?}");
            assert_eq!(r.remote.cmd_gate(cmd, None), None);
        }
        for cmd in [
            Cmd::Zoom,
            Cmd::Stop,
            Cmd::Detach,
            Cmd::Broadcast,
            Cmd::BroadcastMark,
            Cmd::Group,
            Cmd::Filter,
            Cmd::Help,
            Cmd::Palette,
            Cmd::ScrollMode,
            Cmd::NextSession,
            Cmd::NextTab,
            Cmd::NewSession,
            Cmd::Reconnect,
            Cmd::Prompts,
        ] {
            assert_eq!(r.remote.cmd_gate(cmd, Some(&remote)), None, "{cmd:?}");
        }
        assert_eq!(
            r.remote.cmd_gate(Cmd::Resume, Some(&remote)).unwrap(),
            "resume works only on spark; start it again with: tmgr run --on spark"
        );
        assert_eq!(
            r.remote.cmd_gate(Cmd::Rename, Some(&remote)).unwrap(),
            "not available for spark sessions"
        );
        assert!(r.remote.cmd_gate(Cmd::Rename, Some("ghost/s_x")).is_some());
        assert_eq!(r.remote.control_gate(&remote, Control::Stop), None);
        assert_eq!(r.remote.control_gate(&remote, Control::Interrupt), None);
        assert!(r.remote.control_gate(&remote, Control::Close).is_some());
        assert!(r.remote.control_gate(&remote, Control::Resume).is_some());
        assert_eq!(r.remote.control_gate(LOCAL, Control::Close), None);
        assert!(r
            .remote
            .scroll_key_gate(ScrollKey::Search, Some(&remote))
            .is_some());
        assert_eq!(
            r.remote.scroll_key_gate(ScrollKey::PageUp, Some(&remote)),
            None
        );
        assert_eq!(
            r.remote.scroll_key_gate(ScrollKey::Search, Some(LOCAL)),
            None
        );
    }

    #[test]
    fn reconnect_retries_every_link_or_says_there_are_none() {
        let mut r = rig(&["spark"], |f| f.set_runs("spark", &[A]));
        r.rows(2);
        assert_eq!(r.remote.reconnect(), "reconnecting linked machines");
        assert_eq!(Remote::off().reconnect(), "no linked machines (tmgr link)");
    }

    #[test]
    fn without_links_only_valid_local_ids_route_anywhere() {
        let mut off = Remote::off();
        assert_eq!(off.send(LOCAL, b"x"), Sent::Local);
        assert!(matches!(
            off.send("spark/s_aaaaaaaaaa", b"x"),
            Sent::Handled(Some(_))
        ));
        assert!(off.tick(&[]).is_empty());
        assert!(off.scroll("spark/s_aaaaaaaaaa", 1));
    }

    #[test]
    fn nothing_with_a_slash_reaches_the_runner_side() {
        let remote = "spark/s_aaaaaaaaaa".to_string();
        let (local, linked) = split_sizes(vec![
            (LOCAL.into(), 80, 24),
            (remote.clone(), 80, 24),
            ("x/y".into(), 1, 1),
        ]);
        assert_eq!(local, vec![(LOCAL.to_string(), 80, 24)]);
        assert!(linked.iter().all(|p| p.0.contains('/')));
        assert_eq!(attention_session(Some(&remote)), None);
        assert_eq!(attention_session(Some(LOCAL)), Some(LOCAL));
        assert_eq!(attention_session(None), None);
        let ids = vec![LOCAL.to_string(), remote.clone()];
        assert_eq!(
            super::super::attach_links::split_local(&ids),
            vec![LOCAL.to_string()]
        );
        let rows = local_rows(&[row(LOCAL), row(&remote)]);
        assert_eq!(rows.len(), 1);
        let prefs = LayoutPrefs {
            panes: ids.clone(),
            focus: 1,
            tree: Some(Node::leaf(&remote)),
            shells: vec![remote.clone()],
            tabs: vec![TabPrefs {
                tree: Node::leaf(&remote),
                focus: remote.clone(),
                name: None,
            }],
            ..LayoutPrefs::default()
        };
        let out = prefs_for_runner(prefs);
        let json = serde_json::to_string(&out).unwrap();
        assert!(!json.contains('/'), "{json}");
        assert_eq!(out.panes, vec![LOCAL.to_string()]);
    }

    #[test]
    fn a_machine_going_away_removes_its_header_and_rows() {
        let mut r = rig(&["spark"], |f| f.set_runs("spark", &[A]));
        r.rows(2);
        r.apply(LinkMsg::State {
            link: "spark".into(),
            state: LinkState::Removed,
        });
        assert!(r.view.machines.is_empty());
        assert!(r.view.remote.is_empty());
    }

    #[test]
    fn a_relinked_machine_comes_back_after_it_was_removed() {
        let mut r = rig(&["spark"], |f| f.set_runs("spark", &[A]));
        r.rows(2);
        r.apply(LinkMsg::State {
            link: "spark".into(),
            state: LinkState::Removed,
        });
        assert!(r.view.machines.is_empty());
        assert_eq!(r.view.sessions.len(), 1);
        r.apply(LinkMsg::State {
            link: "spark".into(),
            state: LinkState::Online { latency_ms: 5 },
        });
        assert_eq!(r.view.machines.len(), 1);
        assert_eq!(r.view.sessions.len(), 2);
    }

    #[test]
    fn follower_text_names_its_source() {
        let mut r = rig(&["spark"], |f| f.set_runs("spark", &[A]));
        r.rows(2);
        let id = format!("spark/{A}");
        r.apply(LinkMsg::StreamClosed {
            id: id.clone(),
            why: "boom".into(),
            reopen: false,
        });
        r.apply(LinkMsg::StreamClosed {
            id: id.clone(),
            why: "spark closed the connection".into(),
            reopen: false,
        });
        r.apply(LinkMsg::Notice {
            link: "spark".into(),
            id: Some(id),
            text: "hi".into(),
        });
        r.apply(LinkMsg::Notice {
            link: "spark".into(),
            id: None,
            text: "yo".into(),
        });
        assert_eq!(
            r.notes,
            vec![
                "spark: boom".to_string(),
                "spark closed the connection".to_string(),
                format!("spark/run {A}: hi"),
                "spark: yo".to_string(),
            ]
        );
    }

    #[test]
    fn a_stop_confirmation_is_cancelled_when_its_run_or_machine_goes_away() {
        let mut r = rig(&["spark"], |f| f.set_runs("spark", &[A, B]));
        r.rows(3);
        let ask = |r: &mut Rig, run: &str| {
            r.view.confirm = Some(Confirm {
                session: format!("spark/{run}"),
                name: format!("name {run}"),
                remove: false,
            });
        };
        ask(&mut r, A);
        r.fake.set_runs("spark", &[B]);
        r.until("row gone", |r| r.view.sessions.len() == 2);
        assert!(r.view.confirm.is_none());
        assert!(
            r.notes.contains(&format!("name {A} is gone")),
            "{:?}",
            r.notes
        );
        ask(&mut r, B);
        r.apply(LinkMsg::State {
            link: "spark".into(),
            state: LinkState::NoAccess,
        });
        assert!(r.view.confirm.is_none());
        assert!(
            r.notes.contains(&format!("name {B} is gone")),
            "{:?}",
            r.notes
        );
        ask(&mut r, B);
        r.apply(LinkMsg::State {
            link: "spark".into(),
            state: LinkState::Removed,
        });
        assert!(r.view.confirm.is_none());
        r.view.confirm = Some(Confirm {
            session: LOCAL.into(),
            name: "mine".into(),
            remove: false,
        });
        r.apply(LinkMsg::State {
            link: "spark".into(),
            state: LinkState::Offline,
        });
        assert!(r.view.confirm.is_some());
    }
}
