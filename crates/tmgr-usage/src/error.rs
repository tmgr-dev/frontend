use std::fmt;

#[derive(Debug)]
pub enum UsageError {
    Io(std::io::ErrorKind),
    Db(String),
    Json,
    Invalid(String),
    SchemaTooNew {
        found: i64,
        supported: i64,
    },
    Unsupported,
    /// A scan is still running (it holds the scan lock).
    Busy,
}

pub type Result<T> = std::result::Result<T, UsageError>;

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UsageError::Io(kind) => write!(f, "io error: {kind}"),
            UsageError::Db(msg) => write!(f, "database error: {msg}"),
            UsageError::Json => write!(f, "malformed json"),
            UsageError::Invalid(msg) => write!(f, "{msg}"),
            UsageError::SchemaTooNew { found, supported } => write!(
                f,
                "usage database schema {found} is newer than supported {supported}"
            ),
            UsageError::Unsupported => write!(f, "not supported"),
            UsageError::Busy => write!(f, "a scan is running"),
        }
    }
}

impl std::error::Error for UsageError {}

impl From<std::io::Error> for UsageError {
    fn from(e: std::io::Error) -> Self {
        UsageError::Io(e.kind())
    }
}

impl From<rusqlite::Error> for UsageError {
    fn from(e: rusqlite::Error) -> Self {
        match e {
            rusqlite::Error::SqliteFailure(err, _) => {
                UsageError::Db(format!("sqlite code {:?}", err.code))
            }
            _ => UsageError::Db("query failed".into()),
        }
    }
}

impl From<serde_json::Error> for UsageError {
    fn from(_: serde_json::Error) -> Self {
        UsageError::Json
    }
}
