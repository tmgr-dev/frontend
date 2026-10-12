use serde::Serialize;

pub const SCHEMA: &str = "tmgr.v1";

#[derive(Debug, Serialize)]
pub struct Envelope<T: Serialize> {
    pub schema: &'static str,
    pub ok: bool,
    pub data: T,
}

impl<T: Serialize> Envelope<T> {
    pub fn ok(data: T) -> Self {
        Self {
            schema: SCHEMA,
            ok: true,
            data,
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
    fn envelope_serializes_with_schema_and_ok() {
        let json = serde_json::to_string(&Envelope::ok(VersionInfo {
            version: "1.2.3",
            commit: "abc",
            target: "t",
        }))
        .unwrap();
        assert_eq!(
            json,
            r#"{"schema":"tmgr.v1","ok":true,"data":{"version":"1.2.3","commit":"abc","target":"t"}}"#
        );
    }
}
