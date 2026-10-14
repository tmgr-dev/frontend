use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NotFound(String),
    Busy,
    SchemaTooNew(u32),
    SchemaTooOld(u32),
    Unsupported(String),
    Io(String),
    Sql(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotFound(what) => write!(f, "{what} not found"),
            Error::Busy => f.write_str("the local workspace is busy; try again"),
            Error::SchemaTooNew(v) => write!(
                f,
                "this workspace was written by a newer TMGR (schema {v}); update tmgr to open it"
            ),
            Error::SchemaTooOld(v) => write!(
                f,
                "this workspace has an older schema ({v}); open it in the TMGR desktop app once to upgrade it"
            ),
            Error::Unsupported(why) => f.write_str(why),
            Error::Io(why) => write!(f, "cannot read the local workspace: {why}"),
            Error::Sql(why) => write!(f, "local database error: {why}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        match e.sqlite_error_code() {
            Some(rusqlite::ErrorCode::DatabaseBusy) | Some(rusqlite::ErrorCode::DatabaseLocked) => {
                Error::Busy
            }
            _ => Error::Sql(e.to_string()),
        }
    }
}
