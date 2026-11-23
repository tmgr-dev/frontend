use crate::error::Refusal;
use crate::tokens::{is_hex_lower, is_session_id, is_target_id, is_token};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use sha2::{Digest, Sha256};

const MAX_CWD_BYTES: usize = 4096;
const MAX_LABEL_BYTES: usize = 128;
const MAX_SEND_TEXT_BYTES: usize = 8192;
const MAX_SEND_REQUEST_BYTES: usize = 16384;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpawnMode {
    #[default]
    Ask,
    AcceptEdits,
    Bypass,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProposalKind {
    #[default]
    Spawn,
    Stop,
    Send,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "WireProposal")]
pub struct SpawnProposal {
    pub v: u32,
    pub kind: ProposalKind,
    pub profile: String,
    pub cwd: String,
    pub mode: SpawnMode,
    pub sandbox: Option<String>,
    pub prompt_sha256: Option<String>,
    pub name: Option<String>,
    pub task: Option<String>,
    pub target_id: String,
    pub session: Option<String>,
    pub from: Option<String>,
    pub text: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireProposal {
    v: u32,
    #[serde(default)]
    kind: ProposalKind,
    #[serde(default)]
    profile: String,
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    mode: Option<SpawnMode>,
    #[serde(default)]
    sandbox: Option<String>,
    #[serde(default)]
    prompt_sha256: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    task: Option<String>,
    target_id: String,
    #[serde(default)]
    session: Option<String>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

impl TryFrom<WireProposal> for SpawnProposal {
    type Error = &'static str;

    fn try_from(w: WireProposal) -> Result<Self, Self::Error> {
        if w.kind == ProposalKind::Spawn && w.mode.is_none() {
            return Err("missing field `mode`");
        }
        Ok(Self {
            v: w.v,
            kind: w.kind,
            profile: w.profile,
            cwd: w.cwd,
            mode: w.mode.unwrap_or_default(),
            sandbox: w.sandbox,
            prompt_sha256: w.prompt_sha256,
            name: w.name,
            task: w.task,
            target_id: w.target_id,
            session: w.session,
            from: w.from,
            text: w.text,
        })
    }
}

impl Serialize for SpawnProposal {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let spawn = self.kind == ProposalKind::Spawn;
        let mut st = ser.serialize_struct("SpawnProposal", 12)?;
        st.serialize_field("v", &self.v)?;
        if !spawn {
            st.serialize_field("kind", &self.kind)?;
        }
        if spawn {
            st.serialize_field("profile", &self.profile)?;
            st.serialize_field("cwd", &self.cwd)?;
            st.serialize_field("mode", &self.mode)?;
            if let Some(v) = &self.sandbox {
                st.serialize_field("sandbox", v)?;
            }
            if let Some(h) = &self.prompt_sha256 {
                st.serialize_field("prompt_sha256", h)?;
            }
            if let Some(n) = &self.name {
                st.serialize_field("name", n)?;
            }
            if let Some(t) = &self.task {
                st.serialize_field("task", t)?;
            }
        }
        st.serialize_field("target_id", &self.target_id)?;
        if let Some(s) = &self.session {
            st.serialize_field("session", s)?;
        }
        if let Some(f) = &self.from {
            st.serialize_field("from", f)?;
        }
        if let Some(t) = &self.text {
            st.serialize_field("text", t)?;
        }
        st.end()
    }
}

impl SpawnProposal {
    pub fn validate(&self) -> Result<(), Refusal> {
        let ok = self.v == 1
            && is_target_id(&self.target_id)
            && match self.kind {
                ProposalKind::Spawn => self.spawn_ok(),
                ProposalKind::Stop => self.stop_ok() && self.from.is_none() && self.text.is_none(),
                ProposalKind::Send => self.stop_ok() && self.send_ok(),
            };
        ok.then_some(()).ok_or(Refusal::BadToken)
    }

    fn spawn_ok(&self) -> bool {
        is_token(&self.profile)
            && self.sandbox.as_deref().is_none_or(|v| v == "off")
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
            && self.session.is_none()
            && self.from.is_none()
            && self.text.is_none()
    }

    fn stop_ok(&self) -> bool {
        self.session.as_deref().is_some_and(is_session_id)
            && self.profile.is_empty()
            && self.cwd.is_empty()
            && self.mode == SpawnMode::default()
            && self.sandbox.is_none()
            && self.prompt_sha256.is_none()
            && self.name.is_none()
            && self.task.is_none()
    }

    fn send_ok(&self) -> bool {
        self.from
            .as_deref()
            .is_some_and(|f| is_session_id(f) || f.strip_prefix("remote:").is_some_and(is_token))
            && self.text.as_deref().is_some_and(|t| {
                !t.is_empty()
                    && t.len() <= MAX_SEND_TEXT_BYTES
                    && t.chars().all(|c| c == '\n' || c == '\t' || !refused(c))
            })
            && self.canonical_bytes().len() <= MAX_SEND_REQUEST_BYTES
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

fn refused(c: char) -> bool {
    let cp = c as u32;
    REFUSED_CODE_POINTS
        .iter()
        .any(|(lo, hi)| (*lo..=*hi).contains(&cp))
}

fn is_text(s: &str, max_bytes: usize) -> bool {
    !s.is_empty() && s.len() <= max_bytes && !s.chars().any(refused)
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
            sandbox: None,
            prompt_sha256: None,
            name: Some("fix \"login\"".into()),
            task: None,
            target_id: "mx_0123456789abcdef".into(),
            kind: ProposalKind::Spawn,
            session: None,
            from: None,
            text: None,
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
    fn sandbox_off_is_serialized_after_mode_and_round_trips() {
        let mut p = sample();
        p.mode = SpawnMode::Bypass;
        p.sandbox = Some("off".into());
        let got = String::from_utf8(p.canonical_bytes()).unwrap();
        assert_eq!(
            got,
            r#"{"v":1,"profile":"claude","cwd":"/work/app","mode":"bypass","sandbox":"off","name":"fix \"login\"","target_id":"mx_0123456789abcdef"}"#
        );
        assert_eq!(p.validate(), Ok(()));
        let back: SpawnProposal = serde_json::from_str(&got).unwrap();
        assert_eq!(back, p);
        assert_ne!(p.req_sha256(), sample().req_sha256());
    }

    #[test]
    fn sandbox_accepts_only_off_and_only_on_spawn() {
        let mut p = sample();
        p.sandbox = Some("on".into());
        assert_eq!(p.validate(), Err(Refusal::BadToken));
        p.sandbox = Some(String::new());
        assert_eq!(p.validate(), Err(Refusal::BadToken));
        let mut s = stop();
        s.sandbox = Some("off".into());
        assert_eq!(s.validate(), Err(Refusal::BadToken));
        let mut s = send();
        s.sandbox = Some("off".into());
        assert_eq!(s.validate(), Err(Refusal::BadToken));
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

    fn stop() -> SpawnProposal {
        SpawnProposal {
            v: 1,
            kind: ProposalKind::Stop,
            profile: String::new(),
            cwd: String::new(),
            mode: SpawnMode::default(),
            sandbox: None,
            prompt_sha256: None,
            name: None,
            task: None,
            target_id: "mx_0123456789abcdef".into(),
            session: Some("s_abcdefghij".into()),
            from: None,
            text: None,
        }
    }

    fn send() -> SpawnProposal {
        SpawnProposal {
            kind: ProposalKind::Send,
            from: Some("s_bcdefghijk".into()),
            text: Some("hi\nthere".into()),
            ..stop()
        }
    }

    #[test]
    fn stop_and_send_golden_bytes() {
        assert_eq!(
            String::from_utf8(stop().canonical_bytes()).unwrap(),
            r#"{"v":1,"kind":"stop","target_id":"mx_0123456789abcdef","session":"s_abcdefghij"}"#
        );
        assert_eq!(
            String::from_utf8(send().canonical_bytes()).unwrap(),
            r#"{"v":1,"kind":"send","target_id":"mx_0123456789abcdef","session":"s_abcdefghij","from":"s_bcdefghijk","text":"hi\nthere"}"#
        );
    }

    #[test]
    fn new_kinds_round_trip_and_old_spawn_json_loads() {
        for p in [sample(), stop(), send()] {
            let back: SpawnProposal = serde_json::from_slice(&p.canonical_bytes()).unwrap();
            assert_eq!(back, p);
        }
        let old = r#"{"v":1,"profile":"p","cwd":"/","mode":"ask","target_id":"mx_00"}"#;
        let p: SpawnProposal = serde_json::from_str(old).unwrap();
        assert_eq!(p.kind, ProposalKind::Spawn);
        assert_eq!(p.validate(), Ok(()));
        let no_mode = r#"{"v":1,"profile":"p","cwd":"/","target_id":"mx_00"}"#;
        assert!(serde_json::from_str::<SpawnProposal>(no_mode).is_err());
        let extra = r#"{"v":1,"kind":"stop","target_id":"mx_00","session":"s_abcdefghij","x":1}"#;
        assert!(serde_json::from_str::<SpawnProposal>(extra).is_err());
    }

    #[test]
    fn stop_and_send_validation() {
        assert_eq!(stop().validate(), Ok(()));
        assert_eq!(send().validate(), Ok(()));
        let mut remote = send();
        remote.from = Some("remote:phone-1".into());
        remote.text = Some("a\tb\nc".into());
        assert_eq!(remote.validate(), Ok(()));

        type Edit = (&'static str, fn(&mut SpawnProposal));
        let stops: Vec<Edit> = vec![
            ("profile", |p| p.profile = "claude".into()),
            ("cwd", |p| p.cwd = "/".into()),
            ("mode", |p| p.mode = SpawnMode::Bypass),
            ("name", |p| p.name = Some("n".into())),
            ("prompt", |p| p.prompt_sha256 = Some("a".repeat(64))),
            ("no_session", |p| p.session = None),
            ("bad_session", |p| p.session = Some("s_ABCDEFGHIJ".into())),
            ("short_session", |p| p.session = Some("s_abcdefghi".into())),
            ("digit_1_session", |p| {
                p.session = Some("s_abcdefghi1".into())
            }),
            ("from", |p| p.from = Some("s_bcdefghijk".into())),
            ("text", |p| p.text = Some("x".into())),
        ];
        for (name, e) in stops {
            let mut p = stop();
            e(&mut p);
            assert_eq!(p.validate(), Err(Refusal::BadToken), "stop {name}");
        }
        let sends: Vec<Edit> = vec![
            ("bad_from", |p| p.from = Some("nope".into())),
            ("remote_empty", |p| p.from = Some("remote:".into())),
            ("remote_space", |p| p.from = Some("remote:a b".into())),
            ("no_from", |p| p.from = None),
            ("no_text", |p| p.text = None),
            ("empty_text", |p| p.text = Some(String::new())),
            ("nul", |p| p.text = Some("a\0b".into())),
            ("cr", |p| p.text = Some("a\rb".into())),
            ("bidi", |p| p.text = Some("a\u{202e}b".into())),
            ("zero_width", |p| p.text = Some("a\u{200b}b".into())),
            ("c1", |p| p.text = Some("a\u{85}b".into())),
            ("too_long", |p| p.text = Some("a".repeat(8193))),
            ("mode", |p| p.mode = SpawnMode::Bypass),
        ];
        for (name, e) in sends {
            let mut p = send();
            e(&mut p);
            assert_eq!(p.validate(), Err(Refusal::BadToken), "send {name}");
        }
        let mut max = send();
        max.text = Some("a".repeat(8192));
        assert_eq!(max.validate(), Ok(()));
    }

    #[test]
    fn spawn_refuses_new_fields() {
        for e in [
            |p: &mut SpawnProposal| p.session = Some("s_abcdefghij".into()),
            |p: &mut SpawnProposal| p.from = Some("s_abcdefghij".into()),
            |p: &mut SpawnProposal| p.text = Some("x".into()),
        ] {
            let mut p = sample();
            e(&mut p);
            assert_eq!(p.validate(), Err(Refusal::BadToken));
        }
    }

    #[test]
    fn kind_is_bound_into_the_hash() {
        let mut other = stop();
        other.kind = ProposalKind::Send;
        assert_ne!(stop().req_sha256(), other.req_sha256());
        assert_ne!(stop().req_sha256(), send().req_sha256());
    }
}
