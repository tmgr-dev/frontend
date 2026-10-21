#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Place {
    #[default]
    Right,
    Below,
    Replace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpawnKind {
    Agent { profile: String },
    Shell,
    Custom { line: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindOpt {
    pub id: String,
    pub label: String,
    pub spawn: SpawnKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewRequest {
    pub kind: SpawnKind,
    pub folder: String,
    pub name: Option<String>,
    pub place: Place,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewEdit {
    Editing,
    Submit(NewRequest),
    Cancel,
}

#[derive(Clone, Debug)]
pub struct NewSession {
    pub kinds: Vec<KindOpt>,
    pub kind: usize,
    pub folder: String,
    pub command: String,
    pub name: String,
    pub place: Place,
    pub error: Option<String>,
}
