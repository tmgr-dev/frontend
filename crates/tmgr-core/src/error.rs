use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Internal,
    Usage,
    NotFound,
    Auth,
    Forbidden,
    Conflict,
    Offline,
    NeedsConfirmation,
    Interrupted,
}

impl Kind {
    pub fn exit_code(self) -> i32 {
        match self {
            Kind::Internal => 1,
            Kind::Usage => 2,
            Kind::NotFound => 3,
            Kind::Auth => 4,
            Kind::Forbidden => 5,
            Kind::Conflict => 6,
            Kind::Offline => 7,
            Kind::NeedsConfirmation => 10,
            Kind::Interrupted => 130,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Internal => "error",
            Kind::Usage => "usage",
            Kind::NotFound => "not_found",
            Kind::Auth => "auth",
            Kind::Forbidden => "forbidden",
            Kind::Conflict => "conflict",
            Kind::Offline => "offline",
            Kind::NeedsConfirmation => "needs_confirmation",
            Kind::Interrupted => "interrupted",
        }
    }
}

#[derive(Debug)]
pub struct Error {
    pub kind: Kind,
    pub message: String,
    pub hint: Option<String>,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn new(kind: Kind, message: impl AsRef<str>) -> Self {
        Self {
            kind,
            message: crate::redact::redact_secrets(message.as_ref()),
            hint: None,
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn internal(message: impl AsRef<str>) -> Self {
        Self::new(Kind::Internal, message)
    }

    pub fn usage(message: impl AsRef<str>) -> Self {
        Self::new(Kind::Usage, message)
    }

    pub fn not_found(message: impl AsRef<str>) -> Self {
        Self::new(Kind::NotFound, message)
    }

    pub fn auth(message: impl AsRef<str>) -> Self {
        Self::new(Kind::Auth, message)
    }

    pub fn conflict(message: impl AsRef<str>) -> Self {
        Self::new(Kind::Conflict, message)
    }

    pub fn needs_confirmation(message: impl AsRef<str>) -> Self {
        Self::new(Kind::NeedsConfirmation, message)
    }

    pub fn not_logged_in() -> Self {
        Self::auth("not logged in").hint("tmgr login")
    }

    pub fn exit_code(&self) -> i32 {
        self.kind.exit_code()
    }

    pub fn body(&self) -> ErrorBody {
        ErrorBody {
            code: self.kind.name(),
            message: self.message.clone(),
            exit: self.exit_code(),
            hint: self.hint.clone(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::internal(format!("I/O error: {}", e.kind()))
    }
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
    pub exit: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_follow_the_design_table() {
        let table = [
            (Kind::Internal, 1, "error"),
            (Kind::Usage, 2, "usage"),
            (Kind::NotFound, 3, "not_found"),
            (Kind::Auth, 4, "auth"),
            (Kind::Forbidden, 5, "forbidden"),
            (Kind::Conflict, 6, "conflict"),
            (Kind::Offline, 7, "offline"),
            (Kind::NeedsConfirmation, 10, "needs_confirmation"),
            (Kind::Interrupted, 130, "interrupted"),
        ];
        for (kind, code, name) in table {
            assert_eq!(kind.exit_code(), code);
            assert_eq!(kind.name(), name);
        }
    }

    #[test]
    fn error_body_serializes_without_empty_hint() {
        let v = serde_json::to_value(Error::not_found("Task TM-9 not found").body()).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"code":"not_found","message":"Task TM-9 not found","exit":3})
        );
        let v = serde_json::to_value(Error::not_logged_in().body()).unwrap();
        assert_eq!(v["hint"], "tmgr login");
    }

    #[test]
    fn messages_never_carry_bearer_tokens() {
        let e = Error::internal("request failed: Authorization: Bearer abc.def.ghi rest");
        assert!(!e.message.contains("abc.def.ghi"));
    }
}
