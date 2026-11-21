use crate::error::Refusal;
use crate::tokens::{is_hex_lower, is_target_id, is_token};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAX_CWD_BYTES: usize = 4096;
const MAX_LABEL_BYTES: usize = 128;

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
    pub fn validate(&self) -> Result<(), Refusal> {
        let ok = self.v == 1
            && is_token(&self.profile)
            && is_text(&self.cwd, MAX_CWD_BYTES)
            && self
                .name
                .as_deref()
                .is_none_or(|s| is_text(s, MAX_LABEL_BYTES))
            && self
                .task
                .as_deref()
                .is_none_or(|s| is_text(s, MAX_LABEL_BYTES))
            && self
                .prompt_sha256
                .as_deref()
                .is_none_or(|h| is_hex_lower(h, 64))
            && is_target_id(&self.target_id);
        ok.then_some(()).ok_or(Refusal::BadToken)
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("proposal serializes")
    }

    pub fn req_sha256(&self) -> String {
        sha256_hex(&self.canonical_bytes())
    }
}

const REFUSED_CODE_POINTS: [(u32, u32); 18] = [
    (0x0000, 0x001f),
    (0x007f, 0x009f),
    (0x00ad, 0x00ad),
    (0x034f, 0x034f),
    (0x061c, 0x061c),
    (0x115f, 0x1160),
    (0x180e, 0x180e),
    (0x200b, 0x200f),
    (0x2028, 0x202e),
    (0x2060, 0x2064),
    (0x2066, 0x2069),
    (0x206a, 0x206f),
    (0x3164, 0x3164),
    (0xfe00, 0xfe0f),
    (0xfeff, 0xfeff),
    (0xffa0, 0xffa0),
    (0xfff9, 0xfffb),
    (0xe0000, 0xe007f),
];

fn is_text(s: &str, max_bytes: usize) -> bool {
    !s.is_empty()
        && s.len() <= max_bytes
        && !s.chars().any(|c| {
            let cp = c as u32;
            REFUSED_CODE_POINTS
                .iter()
                .any(|(lo, hi)| (*lo..=*hi).contains(&cp))
        })
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
    fn validate_accepts_sample() {
        assert_eq!(sample().validate(), Ok(()));
        let mut p = sample();
        p.prompt_sha256 = Some("a".repeat(64));
        p.task = Some("TM-1017".into());
        assert_eq!(p.validate(), Ok(()));
    }

    #[test]
    fn validate_refuses_bad_fields() {
        type Edit = (&'static str, fn(&mut SpawnProposal));
        let edits: Vec<Edit> = vec![
            ("version", |p| p.v = 2),
            ("profile_space", |p| p.profile = "a b".into()),
            ("profile_empty", |p| p.profile = String::new()),
            ("cwd_empty", |p| p.cwd = String::new()),
            ("cwd_c0", |p| p.cwd = "/a\u{1}b".into()),
            ("cwd_newline", |p| p.cwd = "/a\nb".into()),
            ("cwd_del", |p| p.cwd = "/a\u{7f}b".into()),
            ("cwd_c1", |p| p.cwd = "/a\u{85}b".into()),
            ("cwd_bidi_override", |p| p.cwd = "/a\u{202e}b".into()),
            ("cwd_bidi_isolate", |p| p.cwd = "/a\u{2066}b".into()),
            ("cwd_lrm", |p| p.cwd = "/a\u{200e}b".into()),
            ("cwd_too_long", |p| p.cwd = format!("/{}", "a".repeat(4096))),
            ("name_zero_width_space", |p| {
                p.name = Some("a\u{200b}b".into())
            }),
            ("name_zero_width_joiner", |p| {
                p.name = Some("a\u{200d}b".into())
            }),
            ("name_word_joiner", |p| p.name = Some("a\u{2060}b".into())),
            ("name_bom", |p| p.name = Some("a\u{feff}b".into())),
            ("name_line_separator", |p| {
                p.name = Some("a\u{2028}b".into())
            }),
            ("name_too_long", |p| p.name = Some("a".repeat(129))),
            ("task_tab", |p| p.task = Some("a\tb".into())),
            ("task_too_long", |p| p.task = Some("a".repeat(129))),
            ("prompt_hash_uppercase", |p| {
                p.prompt_sha256 = Some("A".repeat(64))
            }),
            ("prompt_hash_63", |p| p.prompt_sha256 = Some("a".repeat(63))),
            ("target_id_not_token", |p| p.target_id = "t:1".into()),
            ("target_id_not_mx_hex", |p| p.target_id = "mx_XYZ".into()),
        ];
        for (name, e) in edits {
            let mut p = sample();
            e(&mut p);
            assert_eq!(p.validate(), Err(Refusal::BadToken), "{name}");
        }
    }

    #[test]
    fn refused_code_point_table_edges() {
        let ranges: [(u32, u32); 24] = [
            (0x0000, 0x001f),
            (0x007f, 0x009f),
            (0x00ad, 0x00ad),
            (0x034f, 0x034f),
            (0x061c, 0x061c),
            (0x115f, 0x1160),
            (0x180e, 0x180e),
            (0x200b, 0x200f),
            (0x2028, 0x202e),
            (0x2060, 0x2064),
            (0x2066, 0x2069),
            (0x206a, 0x206f),
            (0x3164, 0x3164),
            (0xfe00, 0xfe0f),
            (0xfeff, 0xfeff),
            (0xffa0, 0xffa0),
            (0xfff9, 0xfffb),
            (0xe0000, 0xe007f),
            (0x0020, 0x0020),
            (0x00a0, 0x00a0),
            (0x2065, 0x2065),
            (0x2027, 0x2027),
            (0xe0080, 0xe0080),
            (0x10ffff, 0x10ffff),
        ];
        let refused = &ranges[..18];
        let in_table = |c: u32| refused.iter().any(|(lo, hi)| (*lo..=*hi).contains(&c));
        for (lo, hi) in ranges {
            for cp in [lo, hi] {
                let ch = char::from_u32(cp).unwrap();
                let mut p = sample();
                p.name = Some(format!("a{ch}b"));
                let want = if in_table(cp) {
                    Err(Refusal::BadToken)
                } else {
                    Ok(())
                };
                assert_eq!(p.validate(), want, "U+{cp:04X}");
            }
            for cp in [lo.wrapping_sub(1), hi + 1] {
                if let Some(ch) = char::from_u32(cp) {
                    let mut p = sample();
                    p.name = Some(format!("a{ch}b"));
                    let want = if in_table(cp) {
                        Err(Refusal::BadToken)
                    } else {
                        Ok(())
                    };
                    assert_eq!(p.validate(), want, "U+{cp:04X}");
                }
            }
        }
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
