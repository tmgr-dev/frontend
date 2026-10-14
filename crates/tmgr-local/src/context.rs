use chrono::{DateTime, FixedOffset, Utc};

use crate::journal::WriteOrigin;
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
    /// The user's UTC offset at `now`: "today" of a routine is a local date.
    pub local_offset: FixedOffset,
    pub actor: Option<Actor>,
    /// Where a request without an actor comes from; it decides whether the journal may call it human.
    pub origin: WriteOrigin,
}

impl Context {
    pub(crate) fn iso_now(&self) -> String {
        self.now
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    pub(crate) fn local_now(&self) -> DateTime<FixedOffset> {
        self.now.with_timezone(&self.local_offset)
    }

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
