use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum ErrorCode {
    InvalidParams,
    UnknownMethod,
    UnknownProfile,
    NotFound,
    NotRunning,
    Busy,
    Limit,
    Sandbox,
    Unauthorized,
    ProtoMismatch,
    Internal,
    Forbidden,
    Other(String),
}

impl ErrorCode {
    pub fn as_str(&self) -> &str {
        match self {
            ErrorCode::InvalidParams => "invalid_params",
            ErrorCode::UnknownMethod => "unknown_method",
            ErrorCode::UnknownProfile => "unknown_profile",
            ErrorCode::NotFound => "not_found",
            ErrorCode::NotRunning => "not_running",
            ErrorCode::Busy => "busy",
            ErrorCode::Limit => "limit",
            ErrorCode::Sandbox => "sandbox",
            ErrorCode::Unauthorized => "unauthorized",
            ErrorCode::ProtoMismatch => "proto_mismatch",
            ErrorCode::Internal => "internal",
            ErrorCode::Forbidden => "forbidden",
            ErrorCode::Other(s) => s,
        }
    }
}

impl From<String> for ErrorCode {
    fn from(s: String) -> Self {
        match s.as_str() {
            "invalid_params" => ErrorCode::InvalidParams,
            "unknown_method" => ErrorCode::UnknownMethod,
            "unknown_profile" => ErrorCode::UnknownProfile,
            "not_found" => ErrorCode::NotFound,
            "not_running" => ErrorCode::NotRunning,
            "busy" => ErrorCode::Busy,
            "limit" => ErrorCode::Limit,
            "sandbox" => ErrorCode::Sandbox,
            "unauthorized" => ErrorCode::Unauthorized,
            "proto_mismatch" => ErrorCode::ProtoMismatch,
            "internal" => ErrorCode::Internal,
            "forbidden" => ErrorCode::Forbidden,
            _ => ErrorCode::Other(s),
        }
    }
}

impl From<&str> for ErrorCode {
    fn from(s: &str) -> Self {
        s.to_string().into()
    }
}

impl From<ErrorCode> for String {
    fn from(c: ErrorCode) -> String {
        match c {
            ErrorCode::Other(s) => s,
            known => known.as_str().to_string(),
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
