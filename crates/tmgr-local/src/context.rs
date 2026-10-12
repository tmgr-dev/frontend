use chrono::{DateTime, Utc};

use crate::workspace::LocalWorkspace;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub email: String,
}

/// Who makes the request; `None` on the context means the workspace's own human user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Actor {
    pub kind: String,
    pub id: String,
    pub name: String,
    pub owner_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Context {
    pub workspace: LocalWorkspace,
    pub user: User,
    pub now: DateTime<Utc>,
    pub actor: Option<Actor>,
}

impl Context {
    pub(crate) fn epoch(&self) -> i64 {
        self.now.timestamp()
    }

    pub(crate) fn acting(&self) -> Actor {
        self.actor.clone().unwrap_or_else(|| Actor {
            kind: "user".into(),
            id: self.user.id.to_string(),
            name: self.user.name.clone(),
            owner_id: None,
        })
    }
}

impl Actor {
    pub(crate) fn owner(&self) -> &str {
        self.owner_id.as_deref().unwrap_or(&self.id)
    }
}
