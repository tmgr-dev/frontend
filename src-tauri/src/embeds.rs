use tauri::Url;

/// iframe embeds the app shows (Editor.js embed tool, Telegram login). WebKit reports iframe navigations
/// to the window's navigation hook, which cannot tell them from page navigations; allowing only these
/// exact embed addresses lets the frames load while other links to the same sites still open outside.
pub fn is_embed_url(url: &Url) -> bool {
  if url.scheme() != "https" {
    return false;
  }
  let path = url.path();
  let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
  match url.host_str() {
    Some("www.youtube.com") | Some("youtube.com") => path.starts_with("/embed/"),
    Some("codesandbox.io") => path.starts_with("/embed/"),
    // /<user>/embed/<pen> or /team/<team>/embed/<pen>
    Some("codepen.io") => {
      (segments.len() >= 3 && segments[1] == "embed") || (segments.len() >= 4 && segments[0] == "team" && segments[2] == "embed")
    }
    Some("www.figma.com") => path == "/embed",
    Some("oauth.telegram.org") => path.starts_with("/embed/"),
    _ => false,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn embed(url: &str) -> bool {
    is_embed_url(&url.parse().unwrap())
  }

  #[test]
  fn allows_the_embed_addresses_only() {
    assert!(embed("https://www.youtube.com/embed/dQw4w9WgXcQ"));
    assert!(embed("https://codesandbox.io/embed/abc?view=preview"));
    assert!(embed("https://codepen.io/someone/embed/xyz?default-tab=result"));
    assert!(embed("https://codepen.io/team/codepen/embed/PNaGbb?default-tab=result"));
    assert!(embed("https://www.figma.com/embed?embed_host=share&url=https%3A%2F%2Fwww.figma.com%2Ffile%2Fx"));
    assert!(embed("https://oauth.telegram.org/embed/tmgr_bot?origin=tauri%3A%2F%2Flocalhost"));

    assert!(!embed("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
    assert!(!embed("http://www.youtube.com/embed/x"));
    assert!(!embed("https://codepen.io/someone/pen/xyz"));
    assert!(!embed("https://www.figma.com/file/x"));
    assert!(!embed("https://youtube.com.evil.example/embed/x"));
    assert!(!embed("https://example.com/embed/x"));
  }
}
