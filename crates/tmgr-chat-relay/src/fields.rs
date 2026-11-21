use crate::{Error, Result};
use serde::de::{self, DeserializeOwned, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::collections::HashSet;
use std::fmt;

pub const DEVICE_KINDS: [&str; 3] = ["desktop", "mobile", "runner"];

fn ensure(ok: bool) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(Error::BadFormat)
    }
}

pub fn check_kind(kind: &str) -> Result<()> {
    ensure(
        (1..=16).contains(&kind.len()) && kind.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
    )
}

pub fn check_msg_id(id: &str) -> Result<()> {
    ensure(
        (1..=64).contains(&id.len())
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
    )
}

pub fn check_device_id(id: &str) -> Result<()> {
    let b = id.as_bytes();
    ensure(
        b.len() == 36
            && b.iter().enumerate().all(|(i, c)| match i {
                8 | 13 | 18 | 23 => *c == b'-',
                _ => c.is_ascii_digit() || (b'a'..=b'f').contains(c),
            }),
    )
}

pub fn check_fp(fp: &str) -> Result<()> {
    ensure(
        fp.len() == 64
            && fp
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
    )
}

pub fn check_device_kind(kind: &str) -> Result<()> {
    ensure(DEVICE_KINDS.contains(&kind))
}

pub fn parse_decimal(s: &str) -> Result<u64> {
    ensure(
        !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()) && (s == "0" || !s.starts_with('0')),
    )?;
    s.parse().map_err(|_| Error::BadFormat)
}

struct NoDup;

impl<'de> DeserializeSeed<'de> for NoDup {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> std::result::Result<(), D::Error> {
        d.deserialize_any(NoDupVisitor)
    }
}

struct NoDupVisitor;

impl<'de> Visitor<'de> for NoDupVisitor {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("json without duplicate keys")
    }

    fn visit_bool<E>(self, _: bool) -> std::result::Result<(), E> {
        Ok(())
    }

    fn visit_i64<E>(self, _: i64) -> std::result::Result<(), E> {
        Ok(())
    }

    fn visit_u64<E>(self, _: u64) -> std::result::Result<(), E> {
        Ok(())
    }

    fn visit_f64<E>(self, _: f64) -> std::result::Result<(), E> {
        Ok(())
    }

    fn visit_str<E>(self, _: &str) -> std::result::Result<(), E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> std::result::Result<(), E> {
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<(), A::Error> {
        while seq.next_element_seed(NoDup)?.is_some() {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<(), A::Error> {
        let mut seen = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key) {
                return Err(de::Error::custom("duplicate key"));
            }
            map.next_value_seed(NoDup)?;
        }
        Ok(())
    }
}

pub fn reject_duplicate_keys(s: &str) -> Result<()> {
    let mut de = serde_json::Deserializer::from_str(s);
    NoDup.deserialize(&mut de).map_err(|_| Error::BadJson)?;
    de.end().map_err(|_| Error::BadJson)
}

pub fn parse_strict<T: DeserializeOwned>(s: &str) -> Result<T> {
    reject_duplicate_keys(s)?;
    serde_json::from_str(s).map_err(|_| Error::BadJson)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn kind_rules() {
        for ok in ["chat", "join_ok", "a", "abcdefghijklmnop"] {
            check_kind(ok).unwrap();
        }
        for bad in ["", "Chat", "chat1", "abcdefghijklmnopq", "a-b", "a:b", "ча"] {
            assert!(check_kind(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn msg_id_rules() {
        for ok in ["m_1", "A-z_9", &"a".repeat(64)] {
            check_msg_id(ok).unwrap();
        }
        for bad in ["", "a:b", "a b", "a.b", &"a".repeat(65), "m_é"] {
            assert!(check_msg_id(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn device_id_rules() {
        check_device_id("0a1b2c3d-0000-4000-8000-0123456789ab").unwrap();
        for bad in [
            "",
            "0A1B2C3D-0000-4000-8000-0123456789AB",
            "0a1b2c3d00004000800001234567890abcde",
            "0a1b2c3d-0000-4000-8000-0123456789a",
            "0a1b2c3d-0000-4000-8000-0123456789ag",
            "dev-a",
        ] {
            assert!(check_device_id(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn fp_and_decimal_rules() {
        check_fp(&"a".repeat(64)).unwrap();
        assert!(check_fp(&"A".repeat(64)).is_err());
        assert!(check_fp(&"a".repeat(63)).is_err());
        assert_eq!(parse_decimal("0").unwrap(), 0);
        assert_eq!(parse_decimal("42").unwrap(), 42);
        for bad in ["", "01", "+1", "-1", "1.0", " 1", "99999999999999999999"] {
            assert!(parse_decimal(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn duplicate_keys_are_rejected_at_any_depth() {
        for bad in [
            r#"{"a":1,"a":2}"#,
            r#"{"a":{"b":1,"b":2}}"#,
            r#"[{"a":1,"a":1}]"#,
            r#"{"a":1,"a":2}"#,
        ] {
            assert_eq!(
                parse_strict::<Value>(bad).err(),
                Some(Error::BadJson),
                "{bad}"
            );
        }
        parse_strict::<Value>(r#"{"a":1,"b":{"a":2},"c":[{"a":3},{"a":4}],"d":null,"e":1.5}"#)
            .unwrap();
    }

    #[test]
    fn trailing_garbage_is_rejected() {
        assert!(parse_strict::<Value>(r#"{"a":1} x"#).is_err());
    }
}
