use tauri::{AppHandle, Emitter, Runtime, Url};

const OPEN_APP_LINK_EVENT: &str = "open-app-link";

pub(crate) fn is_task_web_link(url: &Url) -> bool {
  if url.scheme() != "https" || url.host_str() != Some("tmgr.dev") || url.port().is_some() || !url.username().is_empty() || url.password().is_some() {
    return false;
  }
  let Some(segments) = url.path_segments().map(|s| s.collect::<Vec<_>>()) else {
    return false;
  };
  let segments = match segments.as_slice() {
    [ws, kind, id] => [*ws, *kind, *id],
    [ws, kind, id, ""] => [*ws, *kind, *id],
    _ => return false,
  };
  let [ws, kind, id] = segments;
  !ws.is_empty()
    && ws.len() <= 64
    && ws.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    && kind == "tasks"
    && !id.is_empty()
    && id.len() <= 15
    && id.chars().all(|c| c.is_ascii_digit())
}

pub(crate) fn forward_task_link<R: Runtime>(app: &AppHandle<R>, url: &Url) {
  let _ = app.emit_to("main", OPEN_APP_LINK_EVENT, url.as_str());
}

#[cfg(test)]
mod tests {
  use super::is_task_web_link;
  use tauri::Url;

  fn check(url: &str) -> bool {
    is_task_web_link(&Url::parse(url).unwrap())
  }

  #[test]
  fn accepts_task_links() {
    assert!(check("https://tmgr.dev/tmgrdev/tasks/8974"));
    assert!(check("https://tmgr.dev/tmgrdev/tasks/8974/"));
    assert!(check("https://tmgr.dev/tmgrdev/tasks/8974?a=1#b"));
  }

  #[test]
  fn rejects_everything_else() {
    assert!(!check("http://tmgr.dev/w/tasks/1"));
    assert!(!check("https://tmgr.dev.evil.com/w/tasks/1"));
    assert!(!check("https://www.tmgr.dev/w/tasks/1"));
    assert!(!check("https://tmgr.dev:8443/w/tasks/1"));
    assert!(!check("https://user@tmgr.dev/w/tasks/1"));
    assert!(!check("https://tmgr.dev/w/tasks/abc"));
    assert!(!check("https://tmgr.dev/w/tasks/"));
    assert!(!check("https://tmgr.dev/w/pages/1"));
    assert!(!check("https://tmgr.dev/tasks/1"));
    assert!(!check("https://tmgr.dev/w/tasks/1/extra"));
  }
}
