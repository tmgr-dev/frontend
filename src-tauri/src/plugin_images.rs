use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::LazyLock;
use std::time::Duration;

use base64::Engine;
use serde::Serialize;
use tauri::{Runtime, Url, Window};
use tokio::sync::Semaphore;

const MAX_URL_BYTES: usize = 2000;
const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(10);
const USER_AGENT: &str = "TMGR-desktop-plugin-image";
const MIMES: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];

#[derive(Serialize, Debug)]
pub struct ImageResponse {
  mime: String,
  base64: String,
}

/// https only, no userinfo, default port, and a real domain name (never an IP literal or a one-label host).
pub fn validate_url(raw: &str) -> Result<(Url, String), String> {
  if raw.len() > MAX_URL_BYTES {
    return Err("the image URL is too long".into());
  }
  let url = Url::parse(raw).map_err(|e| e.to_string())?;
  if url.scheme() != "https" {
    return Err("only https images are allowed".into());
  }
  if !url.username().is_empty() || url.password().is_some() {
    return Err("credentials in the URL are not allowed".into());
  }
  if url.port().is_some() {
    return Err("only the default https port is allowed".into());
  }
  match url.host() {
    Some(url::Host::Domain(host))
      if host.contains('.') && !host.ends_with('.') && !host.eq_ignore_ascii_case("localhost") =>
    {
      let host = host.to_string();
      Ok((url, host))
    }
    _ => Err("the image host must be a public domain name".into()),
  }
}

fn public_v4(ip: Ipv4Addr) -> bool {
  let [a, b, c, _] = ip.octets();
  !(ip.is_unspecified()
    || ip.is_loopback()
    || ip.is_private()
    || ip.is_link_local()
    || ip.is_multicast()
    || ip.is_broadcast()
    || ip.is_documentation()
    || a == 0
    || (a == 100 && (64..128).contains(&b))
    || (a == 192 && b == 0 && c == 0)
    || (a == 198 && (b == 18 || b == 19))
    || a >= 240)
}

/// True only for an address on the public internet: not loopback, private, link-local, unique-local,
/// CGNAT, multicast, unspecified, documentation, or a v6 form that embeds such a v4 address.
pub fn public_ip(ip: IpAddr) -> bool {
  match ip {
    IpAddr::V4(ip) => public_v4(ip),
    IpAddr::V6(ip) => {
      if let Some(v4) = ip.to_ipv4_mapped() {
        return public_v4(v4);
      }
      let s = ip.segments();
      let embedded = |hi: u16, lo: u16| public_v4(Ipv4Addr::new((hi >> 8) as u8, hi as u8, (lo >> 8) as u8, lo as u8));
      !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (s[0] & 0xfe00) == 0xfc00
        || (s[0] & 0xffc0) == 0xfe80
        || (s[0] & 0xffc0) == 0xfec0
        || (s[0] == 0x2001 && s[1] == 0x0db8)
        || (s[0] == 0x2001 && s[1] == 0)
        || (s[0] == 0x0064 && s[1] == 0xff9b)
        || s[0] == 0x0100 && s[1..4] == [0, 0, 0]
        || (s[..4] == [0, 0, 0, 0] && s[4] == 0xffff && s[5] == 0)
        || ip == Ipv6Addr::LOCALHOST
        || (s[..5] == [0, 0, 0, 0, 0] && (s[5] == 0 || s[5] == 0xffff) && !embedded(s[6], s[7]))
        || (s[0] == 0x2002 && !embedded(s[1], s[2])))
    }
  }
}

fn magic_matches(mime: &str, bytes: &[u8]) -> bool {
  match mime {
    "image/png" => bytes.starts_with(&[0x89, b'P', b'N', b'G']),
    "image/jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
    "image/gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
    "image/webp" => bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
    _ => false,
  }
}

/// The declared type must be one of the four and the bytes must really be that type.
pub fn check_image(content_type: Option<&str>, bytes: &[u8]) -> Result<String, String> {
  let mime = content_type
    .and_then(|value| value.split(';').next())
    .map(|value| value.trim().to_ascii_lowercase())
    .unwrap_or_default();
  if !MIMES.contains(&mime.as_str()) {
    return Err("the server did not answer with a png, jpeg, gif or webp image".into());
  }
  if bytes.is_empty() || !magic_matches(&mime, bytes) {
    return Err("the downloaded bytes are not the declared image type".into());
  }
  Ok(mime)
}

static IN_FLIGHT: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(4));

async fn fetch(raw: String) -> Result<ImageResponse, String> {
  let (url, host) = validate_url(&raw)?;
  let _permit = IN_FLIGHT.acquire().await.map_err(|e| e.to_string())?;
  let resolved = tokio::time::timeout(TIMEOUT, tokio::net::lookup_host((host.as_str(), 443)))
    .await
    .map_err(|_| "could not resolve the image host in time".to_string())?
    .map_err(|e| e.to_string())?
    .collect::<Vec<SocketAddr>>();
  if resolved.is_empty() || !resolved.iter().all(|addr| public_ip(addr.ip())) {
    return Err("the image host does not resolve to a public address".into());
  }
  if rustls::crypto::CryptoProvider::get_default().is_none() {
    let _ = rustls::crypto::ring::default_provider().install_default();
  }
  // The connection is pinned to the addresses vetted above, so a second DNS answer cannot swap them.
  let client = reqwest::Client::builder()
    .https_only(true)
    .no_proxy()
    .redirect(reqwest::redirect::Policy::none())
    .timeout(TIMEOUT)
    .user_agent(USER_AGENT)
    .resolve_to_addrs(&host, &resolved)
    .build()
    .map_err(|e| e.to_string())?;
  let mut response = client.get(url).send().await.map_err(|e| e.to_string())?;
  if !response.status().is_success() {
    return Err(format!("the image server answered {}", response.status().as_u16()));
  }
  if response
    .content_length()
    .is_some_and(|length| length > MAX_IMAGE_BYTES as u64)
  {
    return Err("the image is larger than 2 MB".into());
  }
  let content_type = response
    .headers()
    .get(reqwest::header::CONTENT_TYPE)
    .and_then(|value| value.to_str().ok())
    .map(str::to_string);
  let mut bytes = Vec::new();
  while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
    if bytes.len() + chunk.len() > MAX_IMAGE_BYTES {
      return Err("the image is larger than 2 MB".into());
    }
    bytes.extend_from_slice(&chunk);
  }
  let mime = check_image(content_type.as_deref(), &bytes)?;
  Ok(ImageResponse {
    mime,
    base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
  })
}

/// Main window only: a plugin has no way to call this.
#[tauri::command]
pub async fn plugin_image_fetch<R: Runtime>(window: Window<R>, url: String) -> Result<ImageResponse, String> {
  if window.label() != "main" {
    return Err("not allowed from this window".into());
  }
  fetch(url).await
}

#[cfg(test)]
mod tests {
  use super::*;

  fn ok(url: &str) -> bool {
    validate_url(url).is_ok()
  }

  #[test]
  fn accepts_only_plain_https_to_a_domain() {
    assert!(ok("https://example.com/a.png"));
    assert!(ok("https://cdn.example.co.uk/a/b.png?x=1"));
    assert!(ok("https://example.com:443/a.png"));
    for url in [
      "http://example.com/a.png",
      "https://example.com:8443/a.png",
      "https://user:pw@example.com/a.png",
      "https://user@example.com/a.png",
      "https://127.0.0.1/a.png",
      "https://[::1]/a.png",
      "https://192.168.1.5/a.png",
      "https://2130706433/a.png",
      "https://localhost/a.png",
      "https://intranet/a.png",
      "https://example.com./a.png",
      "file:///etc/passwd",
      "data:image/png;base64,AAAA",
      "blob:https://example.com/x",
      "tmgrfile://x/y",
      "javascript:alert(1)",
      "/relative.png",
      "",
    ] {
      assert!(!ok(url), "{url}");
    }
    assert!(!ok(&format!("https://example.com/{}", "a".repeat(2000))));
  }

  fn v4(text: &str) -> IpAddr {
    text.parse().unwrap()
  }

  #[test]
  fn refuses_every_non_public_address() {
    for text in [
      "0.0.0.0",
      "0.1.2.3",
      "127.0.0.1",
      "127.9.9.9",
      "10.0.0.1",
      "172.16.0.1",
      "172.31.255.255",
      "192.168.0.1",
      "169.254.169.254",
      "100.64.0.1",
      "100.127.255.255",
      "224.0.0.1",
      "239.1.1.1",
      "255.255.255.255",
      "192.0.2.1",
      "198.51.100.1",
      "203.0.113.1",
      "198.18.0.1",
      "192.0.0.1",
      "240.0.0.1",
      "::",
      "::1",
      "fe80::1",
      "fc00::1",
      "fd12:3456::1",
      "ff02::1",
      "::ffff:127.0.0.1",
      "::ffff:10.0.0.1",
      "::ffff:169.254.169.254",
      "2001:db8::1",
      "64:ff9b::1",
      "2002:7f00:1::1",
      "2002:0a00:1::1",
      "fec0::1",
      "::127.0.0.1",
      "::ffff:0:8.8.8.8",
      "::ffff:0:7f00:1",
      "::ffff:0:a00:1",
    ] {
      assert!(!public_ip(v4(text)), "{text}");
    }
  }

  #[test]
  fn allows_public_addresses() {
    for text in [
      "8.8.8.8",
      "1.1.1.1",
      "93.184.216.34",
      "100.63.255.255",
      "172.32.0.1",
      "2606:4700:4700::1111",
      "::ffff:8.8.8.8",
    ] {
      assert!(public_ip(v4(text)), "{text}");
    }
  }

  const PNG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

  #[test]
  fn checks_type_and_magic_bytes() {
    assert_eq!(check_image(Some("image/png"), &PNG).unwrap(), "image/png");
    assert_eq!(check_image(Some("IMAGE/PNG; charset=x"), &PNG).unwrap(), "image/png");
    assert!(check_image(Some("image/jpeg"), &[0xff, 0xd8, 0xff, 0xe0]).is_ok());
    assert!(check_image(Some("image/gif"), b"GIF89a....").is_ok());
    assert!(check_image(Some("image/gif"), b"GIF87a....").is_ok());
    assert!(check_image(Some("image/webp"), b"RIFF\x00\x00\x00\x00WEBPVP8 ").is_ok());
    assert!(check_image(Some("image/jpeg"), &PNG).is_err());
    assert!(check_image(Some("image/webp"), b"RIFF\x00\x00\x00\x00WAVEfmt ").is_err());
    assert!(check_image(Some("image/svg+xml"), b"<svg/>").is_err());
    assert!(check_image(Some("text/html"), &PNG).is_err());
    assert!(check_image(Some("application/octet-stream"), &PNG).is_err());
    assert!(check_image(None, &PNG).is_err());
    assert!(check_image(Some("image/png"), &[]).is_err());
  }
}
