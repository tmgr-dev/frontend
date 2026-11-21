use crate::tokens::MAX_SAFE_INT;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedSpawn {
    pub v: u32,
    pub user_id: String,
    pub device_fp: String,
    pub target_id: String,
    pub target_fp: String,
    pub card_id: String,
    pub nonce: String,
    #[serde(deserialize_with = "safe_int")]
    pub ts_ms: u64,
    #[serde(deserialize_with = "safe_int")]
    pub exp_ms: u64,
    pub req_sha256: String,
    pub alg: String,
    pub sig: String,
}

fn safe_int<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let n = u64::deserialize(d)?;
    if n > MAX_SAFE_INT {
        return Err(serde::de::Error::custom("integer above 2^53-1"));
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    const J: &str = r#"{"v":1,"user_id":"u","device_fp":"d","target_id":"t","target_fp":"f","card_id":"c","nonce":"n","ts_ms":1,"exp_ms":2,"req_sha256":"r","alg":"es256","sig":"s"}"#;

    #[test]
    fn roundtrips() {
        let w: SignedSpawn = serde_json::from_str(J).unwrap();
        assert_eq!(serde_json::to_string(&w).unwrap(), J);
    }

    #[test]
    fn unknown_field_refused() {
        let j = J.replace("\"sig\":\"s\"", "\"sig\":\"s\",\"x\":1");
        assert!(serde_json::from_str::<SignedSpawn>(&j).is_err());
    }

    #[test]
    fn numbers_must_be_js_safe_integers() {
        let ok = J.replace("\"ts_ms\":1", "\"ts_ms\":9007199254740991");
        assert!(serde_json::from_str::<SignedSpawn>(&ok).is_ok());
        for bad in [
            "9007199254740992",
            "9007199254740993",
            "1.0",
            "1e3",
            "-1",
            "-0",
            "18446744073709551615",
            "\"1\"",
        ] {
            for field in ["ts_ms", "exp_ms"] {
                let from = if field == "ts_ms" {
                    "\"ts_ms\":1"
                } else {
                    "\"exp_ms\":2"
                };
                let j = J.replace(from, &format!("\"{field}\":{bad}"));
                assert!(
                    serde_json::from_str::<SignedSpawn>(&j).is_err(),
                    "{field}={bad}"
                );
            }
        }
    }

    #[test]
    fn duplicate_key_refused() {
        let j = J.replace("\"v\":1,", "\"v\":1,\"v\":1,");
        assert!(serde_json::from_str::<SignedSpawn>(&j).is_err());
    }

    #[test]
    fn missing_field_refused() {
        let j = J.replace(",\"alg\":\"es256\"", "");
        assert!(serde_json::from_str::<SignedSpawn>(&j).is_err());
    }
}
