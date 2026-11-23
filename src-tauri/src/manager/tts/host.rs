use std::net::Ipv6Addr;

use url::{Host, Url};

const SPEECH_PATH: &str = "v1/audio/speech";

pub fn is_loopback(url: &Url) -> bool {
  match url.host() {
    Some(Host::Domain(d)) => d == "localhost",
    Some(Host::Ipv4(ip)) => ip.octets()[0] == 127,
    Some(Host::Ipv6(ip)) => ip == Ipv6Addr::LOCALHOST,
    None => false,
  }
}

pub fn parse_base(base: &str, allow_remote: bool) -> Result<Url, String> {
  let url = Url::parse(base.trim()).map_err(|e| format!("invalid server address: {e}"))?;
  if !matches!(url.scheme(), "http" | "https") {
    return Err("the server address must start with http:// or https://".to_owned());
  }
  if !url.username().is_empty() || url.password().is_some() {
    return Err("the server address must not contain a user name or password".to_owned());
  }
  if url.query().is_some() || url.fragment().is_some() {
    return Err("the server address must not contain a query or fragment".to_owned());
  }
  if url.host().is_none() {
    return Err("the server address has no host".to_owned());
  }
  if is_loopback(&url) {
    return Ok(url);
  }
  if !allow_remote {
    return Err("only this computer (localhost, 127.0.0.0/8, ::1) is allowed unless remote servers are enabled".to_owned());
  }
  if url.scheme() != "https" {
    return Err("a remote server must use https://".to_owned());
  }
  Ok(url)
}

pub fn endpoint(base: &str, allow_remote: bool) -> Result<Url, String> {
  let mut url = parse_base(base, allow_remote)?;
  if !url.path().ends_with('/') {
    let path = format!("{}/", url.path());
    url.set_path(&path);
  }
  url.join(SPEECH_PATH).map_err(|e| e.to_string())
}

pub fn remote_host(base: &str) -> Option<String> {
  let url = Url::parse(base.trim()).ok()?;
  let host = url.host()?;
  if is_loopback(&url) {
    return None;
  }
  Some(host.to_string().to_lowercase())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn local(base: &str) -> bool {
    endpoint(base, false).is_ok()
  }

  #[test]
  fn loopback_hosts_are_allowed_without_remote() {
    for base in ["http://localhost:8000", "http://LOCALHOST:8000", "http://127.0.0.1:8000", "http://127.9.9.9", "http://[::1]:8000", "https://localhost", "http://2130706433:80", "http://0x7f.1:80"] {
      assert!(local(base), "{base}");
    }
  }

  #[test]
  fn everything_else_is_rejected_without_remote() {
    for base in [
      "http://localhost.:8000",
      "http://localhost.evil.com",
      "http://127.0.0.1.evil.com",
      "http://127.0.0.1@evil.com",
      "http://[::ffff:127.0.0.1]:8000",
      "http://0.0.0.0:8000",
      "http://[::]:8000",
      "https://example.com",
      "http://100.64.0.1",
      "http://198.51.100.4",
      "ftp://localhost",
      "file:///tmp/x",
      "http://localhost:8000?x=1",
      "http://localhost:8000#frag",
      "http://user:pw@localhost",
      "localhost:8000",
      "",
    ] {
      assert!(!local(base), "{base}");
    }
  }

  #[test]
  fn remote_needs_the_switch_and_https() {
    assert!(endpoint("https://tts.example.com", true).is_ok());
    assert!(endpoint("https://203.0.113.7:8443", true).is_ok());
    assert!(endpoint("https://tts.example.com", false).is_err());
    assert!(endpoint("http://tts.example.com", true).is_err());
    assert!(endpoint("https://u@tts.example.com", true).is_err());
    assert!(endpoint("https://tts.example.com/?k=1", true).is_err());
  }

  #[test]
  fn endpoint_joins_the_speech_path() {
    let cases = [
      ("http://localhost:8000", "http://localhost:8000/v1/audio/speech"),
      ("http://localhost:8000/", "http://localhost:8000/v1/audio/speech"),
      ("http://localhost:8000/api", "http://localhost:8000/api/v1/audio/speech"),
      ("http://localhost:8000/api/", "http://localhost:8000/api/v1/audio/speech"),
    ];
    for (base, want) in cases {
      assert_eq!(endpoint(base, false).unwrap().as_str(), want);
    }
  }

  #[test]
  fn remote_host_ignores_loopback_and_normalizes_case() {
    assert_eq!(remote_host("http://localhost:1"), None);
    assert_eq!(remote_host("https://TTS.Example.com/x").as_deref(), Some("tts.example.com"));
    assert_eq!(remote_host("nonsense"), None);
  }
}
