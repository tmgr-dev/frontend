pub mod api;
pub mod config;
pub mod creds;
pub mod error;
pub mod models;
pub mod redact;
pub mod refs;
pub mod text;
pub mod view;

use serde::Serialize;

pub const SCHEMA: &str = "tmgr.v1";

#[derive(Debug, Serialize)]
pub struct Envelope<T: Serialize> {
    pub schema: &'static str,
    pub ok: bool,
    pub data: T,
    pub warnings: Vec<api::Warning>,
}

impl<T: Serialize> Envelope<T> {
    pub fn ok(data: T) -> Self {
        Self::with_warnings(data, Vec::new())
    }

    pub fn with_warnings(data: T, warnings: Vec<api::Warning>) -> Self {
        Self {
            schema: SCHEMA,
            ok: true,
            data,
            warnings,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ErrorEnvelope {
    pub schema: &'static str,
    pub ok: bool,
    pub error: error::ErrorBody,
}

impl ErrorEnvelope {
    pub fn new(err: &error::Error) -> Self {
        Self {
            schema: SCHEMA,
            ok: false,
            error: err.body(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct VersionInfo {
    pub version: &'static str,
    pub commit: &'static str,
    pub target: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_envelope_serializes_the_error_body() {
        let e = error::Error::not_found("Task TM-999 not found").hint("tmgr task list -p TM");
        let json = serde_json::to_string(&ErrorEnvelope::new(&e)).unwrap();
        assert_eq!(
            json,
            r#"{"schema":"tmgr.v1","ok":false,"error":{"code":"not_found","message":"Task TM-999 not found","exit":3,"hint":"tmgr task list -p TM"}}"#
        );
    }

    #[test]
    fn envelope_serializes_with_schema_and_ok() {
        let json = serde_json::to_string(&Envelope::ok(VersionInfo {
            version: "1.2.3",
            commit: "abc",
            target: "t",
        }))
        .unwrap();
        assert_eq!(
            json,
            r#"{"schema":"tmgr.v1","ok":true,"data":{"version":"1.2.3","commit":"abc","target":"t"},"warnings":[]}"#
        );
    }
}
