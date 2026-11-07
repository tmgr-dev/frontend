use sha2::{Digest, Sha256};

pub const MAX_FILE_BYTES: usize = 64 * 1024;
pub const MAX_FILES: usize = 50;
pub const MAX_SKILL_BYTES: usize = 256 * 1024;
pub const MAX_SEGMENTS: usize = 5;
const MAX_PATH_CHARS: usize = 200;

pub fn sha256_hex(bytes: &[u8]) -> String {
  Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

pub fn valid_segment(segment: &str) -> bool {
  let mut bytes = segment.bytes();
  matches!(bytes.next(), Some(c) if c.is_ascii_alphanumeric() || c == b'_')
    && bytes.all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
}

pub fn valid_path(path: &str) -> bool {
  if path.is_empty() || path.chars().count() > MAX_PATH_CHARS {
    return false;
  }
  let segments: Vec<&str> = path.split('/').collect();
  segments.len() <= MAX_SEGMENTS && segments.iter().all(|s| valid_segment(s))
}

pub fn is_reserved(path: &str) -> bool {
  path.eq_ignore_ascii_case("skill.md")
}

pub fn path_key(path: &str) -> String {
  path.to_ascii_lowercase()
}

pub fn text_of(bytes: &[u8]) -> Option<&str> {
  if bytes.contains(&0) {
    return None;
  }
  std::str::from_utf8(bytes).ok()
}

pub fn files_sha256<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
  let mut sorted: Vec<(&str, &str)> = files.into_iter().collect();
  sorted.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
  let mut joined = String::new();
  for (path, sha) in sorted {
    joined.push_str(path);
    joined.push('\0');
    joined.push_str(sha);
    joined.push('\n');
  }
  sha256_hex(joined.as_bytes())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn path_rules_match_the_server() {
    for ok in ["a.md", "references/guide.md", "a/b/c/d/e.txt", "_x", "a-b_c.d", "SKILL.md.bak"] {
      assert!(valid_path(ok), "{ok}");
    }
    let too_long = format!("{}.md", "a".repeat(200));
    for bad in [
      "",
      "../x",
      "/etc/passwd",
      "a/./b",
      ".git/config",
      ".hidden",
      "a\\b",
      "a b.md",
      "naïve.md",
      "a//b",
      "a/",
      "-x",
      "a/b/c/d/e/f.txt",
      "a\0b",
      "a\nb",
      &too_long,
    ] {
      assert!(!valid_path(bad), "{bad:?}");
    }
  }

  #[test]
  fn reserved_name_is_case_insensitive_on_the_whole_path() {
    assert!(is_reserved("SKILL.md"));
    assert!(is_reserved("skill.MD"));
    assert!(!is_reserved("references/SKILL.md"));
  }

  #[test]
  fn text_is_utf8_without_nul() {
    assert_eq!(text_of("héllo".as_bytes()), Some("héllo"));
    assert!(text_of(&[0x61, 0x00]).is_none());
    assert!(text_of(&[0xff, 0xfe]).is_none());
  }

  #[test]
  fn files_hash_is_sorted_by_path_bytes_and_empty_set_is_sha_of_nothing() {
    assert_eq!(files_sha256([]), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    let forward = files_sha256([("a-b", "11"), ("a/b", "22")]);
    assert_eq!(forward, files_sha256([("a/b", "22"), ("a-b", "11")]));
    assert_eq!(forward, sha256_hex(b"a-b\x0011\na/b\x0022\n"));
    assert_eq!(
      files_sha256([("b/c.md", "BBB"), ("a.md", "AAA")]),
      "ff713c819d6b3f320c6da138b9cdfd52a8c12d52a8d453a131742ab264255001"
    );
  }
}
