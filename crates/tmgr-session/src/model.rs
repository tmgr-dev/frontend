use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
        }
    }

    pub fn rules_file(self) -> &'static str {
        match self {
            Agent::Claude => "CLAUDE.md",
            Agent::Codex => "AGENTS.md",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Session {
    pub agent: Agent,
    pub id: String,
    pub cwd: Option<String>,
    pub branch: Option<String>,
    pub plan: Option<String>,
    pub turns: Vec<Turn>,
}

#[derive(Debug, Clone, Default)]
pub struct Turn {
    pub n: usize,
    pub user: String,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Text(String),
    Tool(ToolCall),
}

#[derive(Debug, Clone, Default)]
pub struct ToolCall {
    pub name: String,
    pub summary: String,
    pub command: Option<String>,
    pub read: Vec<String>,
    pub edited: Vec<String>,
    pub output: Option<String>,
    pub failed: bool,
}

impl ToolCall {
    pub fn interrupted(&self) -> bool {
        self.output.is_none()
    }
}

impl Turn {
    pub fn tools(&self) -> impl Iterator<Item = &ToolCall> {
        self.items.iter().filter_map(|i| match i {
            Item::Tool(t) => Some(t),
            Item::Text(_) => None,
        })
    }

    pub fn final_reply(&self) -> Option<&str> {
        self.items.iter().rev().find_map(|i| match i {
            Item::Text(t) if !t.trim().is_empty() => Some(t.as_str()),
            _ => None,
        })
    }
}

impl Session {
    pub fn files(&self) -> (Vec<String>, Vec<String>) {
        let mut read = Vec::new();
        let mut edited = Vec::new();
        for t in self.turns.iter().flat_map(|t| t.tools()) {
            for f in &t.read {
                if !read.contains(f) {
                    read.push(f.clone());
                }
            }
            for f in &t.edited {
                if !edited.contains(f) {
                    edited.push(f.clone());
                }
            }
        }
        (read, edited)
    }
}

pub(crate) struct Builder {
    pub session: Session,
    index: HashMap<String, (usize, usize)>,
}

impl Builder {
    pub fn new(agent: Agent, id: String) -> Builder {
        Builder {
            session: Session {
                agent,
                id,
                cwd: None,
                branch: None,
                plan: None,
                turns: Vec::new(),
            },
            index: HashMap::new(),
        }
    }

    pub fn user(&mut self, text: String) {
        let n = self.session.turns.len() + 1;
        self.session.turns.push(Turn {
            n,
            user: text,
            items: Vec::new(),
        });
    }

    fn current(&mut self) -> &mut Turn {
        if self.session.turns.is_empty() {
            self.user(String::new());
        }
        self.session.turns.last_mut().expect("turn exists")
    }

    pub fn text(&mut self, text: String) {
        if !text.trim().is_empty() {
            self.current().items.push(Item::Text(text));
        }
    }

    pub fn tool(&mut self, id: String, call: ToolCall) {
        let turn = self.current();
        turn.items.push(Item::Tool(call));
        let pos = (turn.n - 1, turn.items.len() - 1);
        if !id.is_empty() {
            self.index.insert(id, pos);
        }
    }

    pub fn result(&mut self, id: &str, output: String, failed: bool) {
        let Some(&(t, i)) = self.index.get(id) else {
            return;
        };
        if let Some(Item::Tool(call)) = self
            .session
            .turns
            .get_mut(t)
            .and_then(|t| t.items.get_mut(i))
        {
            call.output = Some(output);
            call.failed = failed;
        }
    }
}
