use super::cmdline::{self, Value, Values};
use super::fuzzy;
use super::groups::{self, Group};
use super::guard;
use super::history::Recent;
use super::model::{Kind, Node};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Groups,
    Entries,
}

pub struct Form {
    pub node: Node,
    pub values: Values,
    pub cursor: usize,
    pub error: Option<String>,
    pub review: bool,
}

pub struct Done {
    pub node: Node,
    pub values: Values,
    pub command: String,
    pub code: Option<i32>,
}

pub enum Screen {
    Home,
    Form(Form),
    Done(Done),
}

#[derive(Clone, Debug)]
pub enum Hit {
    Command(Vec<String>),
    Recent(usize),
}

pub struct Palette {
    pub query: String,
    pub hits: Vec<Hit>,
    pub cursor: usize,
}

pub enum Action {
    Run {
        node: Node,
        values: Values,
        argv: Vec<String>,
    },
    Copy(String),
    Quit,
}

pub struct App {
    pub root: Node,
    pub groups: Vec<Group>,
    pub recents: Vec<Recent>,
    pub screen: Screen,
    pub palette: Option<Palette>,
    pub help: bool,
    pub focus: Focus,
    pub group: usize,
    pub entry: usize,
    pub header: String,
    pub notice: Option<String>,
}

fn text_of<'a>(values: &'a Values, id: &str) -> &'a str {
    match values.get(id) {
        Some(Value::Text(t)) => t,
        _ => "",
    }
}

fn multi_text(values: &Values, id: &str) -> String {
    match values.get(id) {
        Some(Value::Multi(m)) => m.join(", "),
        _ => String::new(),
    }
}

impl Form {
    pub fn new(node: Node, values: Values) -> Form {
        Form {
            node,
            values,
            cursor: 0,
            error: None,
            review: false,
        }
    }

    pub fn display(&self) -> String {
        match cmdline::build_argv(&self.node, &self.values, true) {
            Ok(argv) => cmdline::render(&argv),
            Err(_) => cmdline::render(&self.node.path),
        }
    }

    pub fn shown_value(&self, idx: usize) -> String {
        let f = &self.node.fields[idx];
        match &f.kind {
            Kind::Bool => match self.values.get(&f.id) {
                Some(Value::Bool(true)) => "[x]".into(),
                _ => "[ ]".into(),
            },
            Kind::Count => match self.values.get(&f.id) {
                Some(Value::Count(n)) => n.to_string(),
                _ => "0".into(),
            },
            Kind::Multi => multi_text(&self.values, &f.id),
            _ => {
                let t = text_of(&self.values, &f.id);
                if f.sensitive {
                    "\u{2022}".repeat(t.chars().count())
                } else {
                    t.to_string()
                }
            }
        }
    }

    fn cycle(&mut self, step: isize) {
        let f = self.node.fields[self.cursor].clone();
        match &f.kind {
            Kind::Bool => {
                let on = matches!(self.values.get(&f.id), Some(Value::Bool(true)));
                self.values.insert(f.id, Value::Bool(!on));
            }
            Kind::Count => {
                let n = match self.values.get(&f.id) {
                    Some(Value::Count(n)) => *n,
                    _ => 0,
                };
                self.values.insert(f.id, Value::Count((n + 1) % 4));
            }
            Kind::Select(options) => {
                let mut all = vec![String::new()];
                all.extend(options.iter().cloned());
                let cur = all
                    .iter()
                    .position(|o| o == text_of(&self.values, &f.id))
                    .unwrap_or(0) as isize;
                let next = (cur + step).rem_euclid(all.len() as isize) as usize;
                self.values.insert(f.id, Value::Text(all[next].clone()));
            }
            _ => {}
        }
    }

    fn type_char(&mut self, c: char) {
        let f = self.node.fields[self.cursor].clone();
        match &f.kind {
            Kind::Text | Kind::Path => {
                let mut t = text_of(&self.values, &f.id).to_string();
                t.push(c);
                self.values.insert(f.id, Value::Text(t));
            }
            Kind::Multi => {
                let mut t = multi_text(&self.values, &f.id);
                t.push(c);
                self.values.insert(f.id, split_multi(&t));
            }
            _ => {
                if c == ' ' {
                    self.cycle(1);
                }
            }
        }
    }

    fn backspace(&mut self) {
        let f = self.node.fields[self.cursor].clone();
        match &f.kind {
            Kind::Text | Kind::Path => {
                let mut t = text_of(&self.values, &f.id).to_string();
                t.pop();
                self.values.insert(f.id, Value::Text(t));
            }
            Kind::Multi => {
                let mut t = multi_text(&self.values, &f.id);
                t.pop();
                self.values.insert(f.id, split_multi(&t));
            }
            Kind::Select(_) => {
                self.values.insert(f.id, Value::Text(String::new()));
            }
            _ => {}
        }
    }

    fn types_text(&self) -> bool {
        self.node
            .fields
            .get(self.cursor)
            .is_some_and(|f| matches!(f.kind, Kind::Text | Kind::Path | Kind::Multi))
    }
}

fn split_multi(t: &str) -> Value {
    Value::Multi(t.split(',').map(|s| s.trim_start().to_string()).collect())
}

impl App {
    pub fn new(root: Node, recents: Vec<Recent>) -> App {
        let groups = groups::groups(&root);
        App {
            root,
            groups,
            recents,
            screen: Screen::Home,
            palette: None,
            help: false,
            focus: Focus::Groups,
            group: 0,
            entry: 0,
            header: String::new(),
            notice: None,
        }
    }

    pub fn selected_path(&self) -> Option<Vec<String>> {
        self.groups
            .get(self.group)
            .and_then(|g| g.entries.get(self.entry))
            .map(|e| e.path.clone())
    }

    fn form_node(&self, path: &[String]) -> Option<Node> {
        let mut node = self.root.find(path)?.clone();
        node.fields.extend(self.root.fields.iter().cloned());
        Some(node)
    }

    pub fn open(&mut self, path: &[String], values: Values) {
        if let Some(node) = self.form_node(path) {
            self.screen = Screen::Form(Form::new(node, values));
            self.palette = None;
        }
    }

    pub fn open_palette(&mut self) {
        let mut p = Palette {
            query: String::new(),
            hits: Vec::new(),
            cursor: 0,
        };
        self.refresh_palette(&mut p);
        self.palette = Some(p);
    }

    fn refresh_palette(&self, p: &mut Palette) {
        let mut items: Vec<(String, Hit)> = self
            .root
            .leaves()
            .into_iter()
            .filter(|n| !groups::is_internal(&n.path))
            .map(|n| (n.path.join(" "), Hit::Command(n.path.clone())))
            .collect();
        for (i, r) in self.recents.iter().enumerate() {
            items.push((self.recent_line(r), Hit::Recent(i)));
        }
        let q = p.query.clone();
        let ranked = fuzzy::rank(&q, items, |(t, _)| t.clone());
        p.hits = ranked.into_iter().map(|(_, h)| h).take(50).collect();
        p.cursor = p.cursor.min(p.hits.len().saturating_sub(1));
    }

    pub fn recent_line(&self, r: &Recent) -> String {
        match self.form_node(&r.path) {
            Some(n) => match cmdline::build_argv(&n, &r.values, true) {
                Ok(argv) => cmdline::render(&argv),
                Err(_) => cmdline::render(&r.path),
            },
            None => cmdline::render(&r.path),
        }
    }

    pub fn hit_label(&self, h: &Hit) -> (String, String) {
        match h {
            Hit::Command(p) => (
                p.join(" "),
                self.root
                    .find(p)
                    .map(|n| n.about.clone())
                    .unwrap_or_default(),
            ),
            Hit::Recent(i) => (
                format!(
                    "Recent: {}",
                    self.recents
                        .get(*i)
                        .map_or(String::new(), |r| self.recent_line(r))
                ),
                String::new(),
            ),
        }
    }

    pub fn finish(&mut self, done: Done) {
        self.screen = Screen::Done(done);
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Option<Action> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Some(Action::Quit);
        }
        self.notice = None;
        if self.help {
            self.help = false;
            return None;
        }
        if self.palette.is_some() {
            return self.palette_key(key);
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('p') {
            self.open_palette();
            return None;
        }
        match &self.screen {
            Screen::Home => self.home_key(key),
            Screen::Form(_) => self.form_key(key),
            Screen::Done(_) => self.done_key(key),
        }
    }

    fn home_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Char('q') => return Some(Action::Quit),
            KeyCode::Char('/') => self.open_palette(),
            KeyCode::Char('?') => self.help = true,
            KeyCode::Up | KeyCode::Char('k') => self.step(-1),
            KeyCode::Down | KeyCode::Char('j') => self.step(1),
            KeyCode::Right | KeyCode::Char('l') => {
                if self.focus == Focus::Groups {
                    self.focus = Focus::Entries;
                    self.entry = 0;
                }
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Esc => self.focus = Focus::Groups,
            KeyCode::Enter => match self.focus {
                Focus::Groups => {
                    self.focus = Focus::Entries;
                    self.entry = 0;
                }
                Focus::Entries => {
                    if let Some(p) = self.selected_path() {
                        self.open(&p, Values::new());
                    }
                }
            },
            _ => {}
        }
        None
    }

    fn step(&mut self, d: isize) {
        let wrap = |i: usize, n: usize| (i as isize + d).rem_euclid(n.max(1) as isize) as usize;
        match self.focus {
            Focus::Groups => {
                self.group = wrap(self.group, self.groups.len());
                self.entry = 0;
            }
            Focus::Entries => {
                let n = self.groups.get(self.group).map_or(0, |g| g.entries.len());
                self.entry = wrap(self.entry, n);
            }
        }
    }

    fn palette_key(&mut self, key: KeyEvent) -> Option<Action> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let mut p = self.palette.take()?;
        match key.code {
            KeyCode::Esc => return None,
            KeyCode::Enter | KeyCode::Tab => {
                let hit = p.hits.get(p.cursor).cloned();
                match hit {
                    Some(Hit::Command(path)) => self.open(&path, Values::new()),
                    Some(Hit::Recent(i)) => {
                        if let Some(r) = self.recents.get(i).cloned() {
                            self.open(&r.path, r.values);
                        }
                    }
                    None => self.palette = Some(p),
                }
                return None;
            }
            KeyCode::Char('y') if ctrl => {
                let line = p.hits.get(p.cursor).map(|h| match h {
                    Hit::Command(path) => cmdline::render(path),
                    Hit::Recent(i) => self
                        .recents
                        .get(*i)
                        .map(|r| self.recent_line(r))
                        .unwrap_or_default(),
                });
                self.palette = Some(p);
                return line.map(Action::Copy);
            }
            KeyCode::Up => p.cursor = p.cursor.saturating_sub(1),
            KeyCode::Down => p.cursor = (p.cursor + 1).min(p.hits.len().saturating_sub(1)),
            KeyCode::Backspace => {
                p.query.pop();
                p.cursor = 0;
                self.refresh_palette(&mut p);
            }
            KeyCode::Char(c) if !ctrl => {
                p.query.push(c);
                p.cursor = 0;
                self.refresh_palette(&mut p);
            }
            _ => {}
        }
        self.palette = Some(p);
        None
    }

    fn form_key(&mut self, key: KeyEvent) -> Option<Action> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let Screen::Form(form) = &mut self.screen else {
            return None;
        };
        form.error = None;
        if form.review {
            match key.code {
                KeyCode::Enter => {
                    form.review = false;
                    return run_action(form);
                }
                _ => {
                    form.review = false;
                    return None;
                }
            }
        }
        let n = form.node.fields.len();
        match key.code {
            KeyCode::Esc => self.screen = Screen::Home,
            KeyCode::Char('y') if ctrl => return Some(Action::Copy(form.display())),
            KeyCode::Enter => {
                if guard::is_guarded(&form.node, &form.values) {
                    match cmdline::build_argv(&form.node, &form.values, false) {
                        Ok(_) => form.review = true,
                        Err(e) => form.error = Some(e),
                    }
                    return None;
                }
                return run_action(form);
            }
            KeyCode::Down | KeyCode::Tab if n > 0 => form.cursor = (form.cursor + 1) % n,
            KeyCode::Up | KeyCode::BackTab if n > 0 => form.cursor = (form.cursor + n - 1) % n,
            KeyCode::Right if n > 0 && !form.types_text() => form.cycle(1),
            KeyCode::Left if n > 0 && !form.types_text() => form.cycle(-1),
            KeyCode::Backspace if n > 0 => form.backspace(),
            KeyCode::Char(c) if n > 0 && !ctrl => form.type_char(c),
            _ => {}
        }
        None
    }

    fn done_key(&mut self, key: KeyEvent) -> Option<Action> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let Screen::Done(done) = &self.screen else {
            return None;
        };
        match key.code {
            KeyCode::Char('y') if ctrl => return Some(Action::Copy(done.command.clone())),
            KeyCode::Char('e') => {
                let (node, values) = (done.node.clone(), done.values.clone());
                self.screen = Screen::Form(Form::new(node, values));
            }
            KeyCode::Char('q') => return Some(Action::Quit),
            KeyCode::Enter | KeyCode::Esc => self.screen = Screen::Home,
            _ => {}
        }
        None
    }
}

fn run_action(form: &mut Form) -> Option<Action> {
    match cmdline::build_argv(&form.node, &form.values, false) {
        Ok(argv) => Some(Action::Run {
            node: form.node.clone(),
            values: form.values.clone(),
            argv,
        }),
        Err(e) => {
            form.error = Some(e);
            None
        }
    }
}
