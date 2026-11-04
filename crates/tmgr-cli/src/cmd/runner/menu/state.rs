use super::memory::{Kind, Memory};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Enter,
    Esc,
    Tab,
    Backspace,
    Char(char),
    CtrlC,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Stay,
    Quit,
    Start(Vec<String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Agent,
    Session,
    Flags,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Tmgr,
    External,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub kind: RowKind,
    pub sandboxed: bool,
    pub project: String,
    pub when: String,
    pub text: String,
    pub live: bool,
    pub readable: bool,
    pub folder_gone: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hints {
    pub reading_off: bool,
}

pub trait Source {
    fn rows(&self, profile: &str) -> (Vec<Row>, Hints);
    fn search(&self, query: &str) -> Option<Vec<String>>;
    fn allow_reading(&self) -> Result<(), String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentOpt {
    pub id: String,
    pub name: String,
    pub installed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    Ok,
    Live,
    Unreadable,
    FolderGone,
    BadId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Folder,
    Bypass,
    NoSandbox,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    New,
    Existing(usize, Kind),
}

pub const SANDBOX_LOCKED_ON: &str =
    "Ordinary sessions live in your real home, a sandboxed session cannot see them.";
pub const SANDBOX_LOCKED_OFF: &str = "This session was recorded inside the sandbox home.";
pub const CONFIRM_NEXT: &str = "You will be asked to type a confirmation next.";
pub const LIVE_REASON: &str = "Running in another terminal: fork instead";
pub const GONE_REASON: &str = "The session's folder no longer exists, so it cannot be started.";
pub const READING_OFF: &str =
    "Reading session logs is off. Press a to allow it (same as: tmgr usage consent allow)";

pub struct State {
    source: Box<dyn Source>,
    pub agents: Vec<AgentOpt>,
    pub screen: Screen,
    pub agent_cursor: usize,
    agent: usize,
    pub rows: Vec<Row>,
    pub hints: Hints,
    pub filter: String,
    pub cursor: usize,
    pub choice: Option<Kind>,
    target: Target,
    pub folder: String,
    pref_bypass: bool,
    pref_no_sandbox: bool,
    pref_kind: Kind,
    pub flag_cursor: usize,
    pub notice: Option<String>,
}

fn clean_char(c: char) -> bool {
    !c.is_control()
}

impl State {
    pub fn new(
        agents: Vec<AgentOpt>,
        folder: String,
        memory: &Memory,
        source: Box<dyn Source>,
    ) -> State {
        let agent_cursor = agents
            .iter()
            .position(|a| a.installed && a.id == memory.agent)
            .or_else(|| agents.iter().position(|a| a.installed))
            .unwrap_or(0);
        State {
            source,
            agents,
            screen: Screen::Agent,
            agent_cursor,
            agent: agent_cursor,
            rows: Vec::new(),
            hints: Hints::default(),
            filter: String::new(),
            cursor: 0,
            choice: None,
            target: Target::New,
            folder,
            pref_bypass: memory.bypass,
            pref_no_sandbox: memory.no_sandbox,
            pref_kind: memory.kind,
            flag_cursor: 0,
            notice: None,
        }
    }

    pub fn agent(&self) -> &AgentOpt {
        &self.agents[self.agent]
    }

    pub fn visible(&self) -> Vec<usize> {
        let q = self.filter.trim();
        if q.is_empty() {
            return (0..self.rows.len()).collect();
        }
        if let Some(ids) = self.source.search(q) {
            return ids
                .iter()
                .filter_map(|id| self.rows.iter().position(|r| &r.id == id))
                .collect();
        }
        let needle = q.to_lowercase();
        (0..self.rows.len())
            .filter(|&i| {
                let r = &self.rows[i];
                r.project.to_lowercase().contains(&needle)
                    || r.text.to_lowercase().contains(&needle)
            })
            .collect()
    }

    pub fn availability(row: &Row) -> Availability {
        if row.id.is_empty() || row.id.chars().any(char::is_control) {
            Availability::BadId
        } else if !row.readable {
            Availability::Unreadable
        } else if row.folder_gone {
            Availability::FolderGone
        } else if row.live {
            Availability::Live
        } else {
            Availability::Ok
        }
    }

    pub fn selected_row(&self) -> Option<&Row> {
        let i = *self.visible().get(self.cursor.checked_sub(1)?)?;
        self.rows.get(i)
    }

    pub fn reading_hint(&self) -> bool {
        self.hints.reading_off || self.rows.iter().any(|r| !r.readable)
    }

    pub fn reason(&self) -> Option<&'static str> {
        match Self::availability(self.selected_row()?) {
            Availability::Live => Some(LIVE_REASON),
            Availability::FolderGone => Some(GONE_REASON),
            _ => None,
        }
    }

    pub fn fields(&self) -> Vec<Field> {
        match self.target {
            Target::New => vec![Field::Folder, Field::Bypass, Field::NoSandbox],
            Target::Existing(..) => vec![Field::Bypass, Field::NoSandbox],
        }
    }

    fn target_row(&self) -> Option<&Row> {
        match self.target {
            Target::New => None,
            Target::Existing(i, _) => self.rows.get(i),
        }
    }

    pub fn no_sandbox_lock(&self) -> Option<(bool, &'static str)> {
        let row = self.target_row()?;
        match (row.kind, row.sandboxed) {
            (RowKind::External, _) => Some((true, SANDBOX_LOCKED_ON)),
            (RowKind::Tmgr, true) => Some((false, SANDBOX_LOCKED_OFF)),
            (RowKind::Tmgr, false) => None,
        }
    }

    pub fn bypass(&self) -> bool {
        self.pref_bypass
    }

    pub fn no_sandbox(&self) -> bool {
        self.no_sandbox_lock()
            .map_or(self.pref_no_sandbox, |(on, _)| on)
    }

    pub fn needs_confirmation(&self) -> bool {
        self.bypass() || self.no_sandbox()
    }

    pub fn kind(&self) -> Kind {
        match self.target {
            Target::New => Kind::New,
            Target::Existing(_, k) => k,
        }
    }

    pub fn argv(&self) -> Vec<String> {
        let mut v: Vec<String> = vec!["tmgr".into()];
        match self.target_row() {
            None => {
                v.extend(["run".into(), "--agent".into(), self.agent().id.clone()]);
                if !self.folder.is_empty() {
                    v.extend(["--cwd".into(), self.folder.clone()]);
                }
            }
            Some(row) => {
                v.extend(["resume".into(), row.id.clone()]);
                if self.kind() == Kind::Fork {
                    v.push("--fork".into());
                }
            }
        }
        if self.bypass() {
            v.extend(["--mode".into(), "bypass".into()]);
        }
        if self.no_sandbox() {
            v.push("--no-sandbox".into());
        }
        v
    }

    pub fn command(&self) -> String {
        shell_words::join(self.argv())
    }

    pub fn memory(&self) -> Memory {
        Memory {
            agent: self.agent().id.clone(),
            kind: self.kind(),
            bypass: self.pref_bypass,
            no_sandbox: self.pref_no_sandbox,
        }
    }

    pub fn on_paste(&mut self, text: &str) {
        match self.screen {
            Screen::Session if self.choice.is_none() => {
                self.filter.extend(text.chars().filter(|c| clean_char(*c)));
                self.cursor = 0;
            }
            Screen::Flags if self.current_field() == Some(Field::Folder) => {
                self.folder.extend(text.chars().filter(|c| clean_char(*c)));
            }
            _ => {}
        }
    }

    pub fn on_key(&mut self, key: Key) -> Step {
        if key == Key::CtrlC {
            return Step::Quit;
        }
        self.notice = None;
        match self.screen {
            Screen::Agent => self.agent_key(key),
            Screen::Session => self.session_key(key),
            Screen::Flags => self.flags_key(key),
        }
    }

    fn agent_key(&mut self, key: Key) -> Step {
        match key {
            Key::Char('q') | Key::Esc => return Step::Quit,
            Key::Up => self.step_agent(-1),
            Key::Down => self.step_agent(1),
            Key::Enter if self.agents[self.agent_cursor].installed => {
                self.agent = self.agent_cursor;
                let (rows, hints) = self.source.rows(&self.agents[self.agent].id);
                self.rows = rows;
                self.hints = hints;
                self.filter.clear();
                self.cursor = 0;
                self.choice = None;
                self.screen = Screen::Session;
            }
            _ => {}
        }
        Step::Stay
    }

    fn step_agent(&mut self, delta: isize) {
        let n = self.agents.len() as isize;
        let mut i = self.agent_cursor as isize;
        for _ in 0..n {
            i = (i + delta).rem_euclid(n);
            if self.agents[i as usize].installed {
                self.agent_cursor = i as usize;
                return;
            }
        }
    }

    fn session_key(&mut self, key: Key) -> Step {
        if self.choice.is_some() {
            return self.choice_key(key);
        }
        let count = self.visible().len() + 1;
        match key {
            Key::Esc => {
                self.screen = Screen::Agent;
                self.rows.clear();
                self.filter.clear();
            }
            Key::Up => self.cursor = (self.cursor + count - 1) % count,
            Key::Down => self.cursor = (self.cursor + 1) % count,
            Key::Enter => self.pick(),
            Key::Backspace => {
                self.filter.pop();
                self.cursor = 0;
            }
            Key::Char('a') if self.filter.is_empty() && self.reading_hint() => self.allow(),
            Key::Char(c) if clean_char(c) => {
                self.filter.push(c);
                self.cursor = 0;
            }
            _ => {}
        }
        Step::Stay
    }

    fn allow(&mut self) {
        match self.source.allow_reading() {
            Ok(()) => {
                let (rows, hints) = self.source.rows(&self.agents[self.agent].id);
                self.rows = rows;
                self.hints = hints;
                self.cursor = 0;
                self.notice = Some("Reading session logs is on.".into());
            }
            Err(e) => self.notice = Some(e),
        }
    }

    fn pick(&mut self) {
        if self.cursor == 0 {
            self.target = Target::New;
            self.enter_flags();
            return;
        }
        let Some(row) = self.selected_row() else {
            return;
        };
        match Self::availability(row) {
            Availability::Ok => {
                self.choice = Some(if self.pref_kind == Kind::Fork {
                    Kind::Fork
                } else {
                    Kind::Resume
                });
            }
            Availability::Live => self.choice = Some(Kind::Fork),
            _ => {}
        }
    }

    fn choice_key(&mut self, key: Key) -> Step {
        let live = self
            .selected_row()
            .is_some_and(|r| Self::availability(r) == Availability::Live);
        match key {
            Key::Esc => self.choice = None,
            Key::Tab if !live => {
                self.choice = Some(match self.choice {
                    Some(Kind::Resume) => Kind::Fork,
                    _ => Kind::Resume,
                });
            }
            Key::Char('r') if live => self.notice = Some(LIVE_REASON.into()),
            Key::Char('r') => self.choose(Kind::Resume),
            Key::Char('f') => self.choose(Kind::Fork),
            Key::Enter => {
                if let Some(kind) = self.choice {
                    self.choose(kind);
                }
            }
            _ => {}
        }
        Step::Stay
    }

    fn choose(&mut self, kind: Kind) {
        let Some(i) = self.visible().get(self.cursor.saturating_sub(1)).copied() else {
            return;
        };
        self.target = Target::Existing(i, kind);
        self.choice = None;
        self.enter_flags();
    }

    fn enter_flags(&mut self) {
        self.flag_cursor = 0;
        self.screen = Screen::Flags;
    }

    pub fn current_field(&self) -> Option<Field> {
        self.fields().get(self.flag_cursor).copied()
    }

    fn flags_key(&mut self, key: Key) -> Step {
        let n = self.fields().len();
        match key {
            Key::Esc => {
                self.screen = Screen::Session;
                self.choice = None;
            }
            Key::Up => self.flag_cursor = (self.flag_cursor + n - 1) % n,
            Key::Down | Key::Tab => self.flag_cursor = (self.flag_cursor + 1) % n,
            Key::Enter => return Step::Start(self.argv()),
            Key::Backspace if self.current_field() == Some(Field::Folder) => {
                self.folder.pop();
            }
            Key::Char(c) => match self.current_field() {
                Some(Field::Folder) if clean_char(c) => self.folder.push(c),
                Some(Field::Bypass) if c == ' ' => self.pref_bypass = !self.pref_bypass,
                Some(Field::NoSandbox) if c == ' ' && self.no_sandbox_lock().is_none() => {
                    self.pref_no_sandbox = !self.pref_no_sandbox;
                }
                _ => {}
            },
            _ => {}
        }
        Step::Stay
    }
}
