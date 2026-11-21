use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpawnMode {
    Ask,
    AcceptEdits,
    Bypass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnProposal {
    pub v: u32,
    pub profile: String,
    pub cwd: String,
    pub mode: SpawnMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    pub target_id: String,
}

impl SpawnProposal {
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("proposal serializes")
    }

    pub fn req_sha256(&self) -> String {
        sha256_hex(&self.canonical_bytes())
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SpawnProposal {
        SpawnProposal {
            v: 1,
            profile: "claude".into(),
            cwd: "/work/app".into(),
            mode: SpawnMode::AcceptEdits,
            prompt_sha256: None,
            name: Some("fix \"login\"".into()),
            task: None,
            target_id: "mx_0123456789abcdef".into(),
        }
    }

    #[test]
    fn canonical_bytes_are_ordered_compact_and_omit_none() {
        let got = String::from_utf8(sample().canonical_bytes()).unwrap();
        assert_eq!(
            got,
            r#"{"v":1,"profile":"claude","cwd":"/work/app","mode":"accept-edits","name":"fix \"login\"","target_id":"mx_0123456789abcdef"}"#
        );
    }

    #[test]
    fn modes_use_wire_names() {
        for (m, s) in [
            (SpawnMode::Ask, "\"ask\""),
            (SpawnMode::AcceptEdits, "\"accept-edits\""),
            (SpawnMode::Bypass, "\"bypass\""),
        ] {
            assert_eq!(serde_json::to_string(&m).unwrap(), s);
        }
    }

    #[test]
    fn hash_is_sha256_of_canonical_bytes() {
        let p = sample();
        assert_eq!(p.req_sha256(), sha256_hex(&p.canonical_bytes()));
        assert_eq!(p.req_sha256().len(), 64);
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn unknown_fields_are_refused() {
        let j = r#"{"v":1,"profile":"p","cwd":"/","mode":"ask","target_id":"t","extra":1}"#;
        assert!(serde_json::from_str::<SpawnProposal>(j).is_err());
    }

    #[test]
    fn non_ascii_stays_raw_utf8() {
        let mut p = sample();
        p.name = Some("привет".into());
        let s = String::from_utf8(p.canonical_bytes()).unwrap();
        assert!(s.contains("\"name\":\"привет\""));
    }
}
