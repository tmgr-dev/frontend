use serde::{Deserialize, Serialize};

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
    pub ts_ms: u64,
    pub exp_ms: u64,
    pub req_sha256: String,
    pub alg: String,
    pub sig: String,
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
    fn missing_field_refused() {
        let j = J.replace(",\"alg\":\"es256\"", "");
        assert!(serde_json::from_str::<SignedSpawn>(&j).is_err());
    }
}
