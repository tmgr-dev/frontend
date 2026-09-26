use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};

use tauri::http::{Method, Request, Response, StatusCode};
use tauri::{AppHandle, Runtime};

use crate::local_workspaces;

pub const SCHEME: &str = "tmgrfile";
pub const MAX_BYTES: usize = 25 * 1024 * 1024;

/// `<code>/<key>` from the request path, percent-decoded. Keys are what the local API hands out
/// (`<uuid>/<sanitised name>`), so anything else — `..`, empty or odd segments — is refused.
pub fn parse_target(path: &str) -> Option<(String, String)> {
  let decoded = percent_decode(path.trim_start_matches('/'))?;
  let (code, key) = decoded.split_once('/')?;
  let valid_segment = |s: &str| {
    !s.is_empty()
      && s != "."
      && s != ".."
      && s.len() <= 160
      && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
  };
  if !valid_segment(code) || key.split('/').count() > 4 || !key.split('/').all(valid_segment) {
    return None;
  }
  Some((code.to_string(), key.to_string()))
}

fn percent_decode(input: &str) -> Option<String> {
  let bytes = input.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] == b'%' {
      let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
      out.push(u8::from_str_radix(hex, 16).ok()?);
      i += 3;
    } else {
      out.push(bytes[i]);
      i += 1;
    }
  }
  String::from_utf8(out).ok()
}

pub fn content_type(key: &str) -> &'static str {
  let ext = key.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
  match ext.as_str() {
    "png" => "image/png",
    "jpg" | "jpeg" => "image/jpeg",
    "gif" => "image/gif",
    "webp" => "image/webp",
    "svg" => "image/svg+xml",
    "pdf" => "application/pdf",
    "txt" | "log" => "text/plain; charset=utf-8",
    "md" => "text/markdown; charset=utf-8",
    "json" => "application/json",
    "csv" => "text/csv; charset=utf-8",
    "zip" => "application/zip",
    "mp4" => "video/mp4",
    "mov" => "video/quicktime",
    _ => "application/octet-stream",
  }
}

fn respond(status: StatusCode, body: Vec<u8>, kind: &str) -> Response<Cow<'static, [u8]>> {
  Response::builder()
    .status(status)
    .header("Content-Type", kind)
    .header("Access-Control-Allow-Origin", "*")
    .header("Access-Control-Allow-Methods", "GET, PUT, DELETE, OPTIONS")
    .header("Access-Control-Allow-Headers", "Content-Type")
    // svg and html must never run as a document in the app's context.
    .header("Content-Security-Policy", "default-src 'none'; img-src 'self'; style-src 'unsafe-inline'")
    .header("X-Content-Type-Options", "nosniff")
    .body(Cow::Owned(body))
    .unwrap()
}

fn error(status: StatusCode, message: &str) -> Response<Cow<'static, [u8]>> {
  respond(status, message.as_bytes().to_vec(), "text/plain; charset=utf-8")
}

pub fn file_path(workspace_dir: &Path, key: &str) -> PathBuf {
  key.split('/').fold(workspace_dir.join("files"), |path, segment| path.join(segment))
}

pub fn handle<R: Runtime>(app: &AppHandle<R>, request: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
  if request.method() == Method::OPTIONS {
    return respond(StatusCode::NO_CONTENT, Vec::new(), "text/plain");
  }
  let Some((code, key)) = parse_target(request.uri().path()) else {
    return error(StatusCode::BAD_REQUEST, "bad file path");
  };
  let Ok(workspace) = local_workspaces::find(app, &code) else {
    return error(StatusCode::NOT_FOUND, "unknown workspace");
  };
  let path = file_path(Path::new(&workspace.path), &key);
  match *request.method() {
    Method::GET | Method::HEAD => match fs::read(&path) {
      Ok(bytes) => respond(StatusCode::OK, bytes, content_type(&key)),
      Err(_) => error(StatusCode::NOT_FOUND, "file not found"),
    },
    Method::PUT => {
      let body = request.body();
      if body.len() > MAX_BYTES {
        return error(StatusCode::PAYLOAD_TOO_LARGE, "file is larger than 25 MB");
      }
      if path.exists() {
        return error(StatusCode::CONFLICT, "file already exists");
      }
      let written = path
        .parent()
        .map(fs::create_dir_all)
        .unwrap_or(Ok(()))
        .and_then(|_| fs::write(&path, body));
      match written {
        Ok(()) => respond(StatusCode::OK, Vec::new(), "text/plain"),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
      }
    }
    Method::DELETE => {
      let _ = fs::remove_file(&path);
      if let Some(dir) = path.parent() {
        let _ = fs::remove_dir(dir);
      }
      respond(StatusCode::NO_CONTENT, Vec::new(), "text/plain")
    }
    _ => error(StatusCode::METHOD_NOT_ALLOWED, "method not allowed"),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn only_keys_the_api_hands_out_are_accepted() {
    assert_eq!(
      parse_target("/personal/1f2e-abc/screenshot-2026.png"),
      Some(("personal".into(), "1f2e-abc/screenshot-2026.png".into()))
    );
    assert_eq!(parse_target("/personal/a/my%20file.png"), None);
    assert_eq!(parse_target("/personal/../../etc/passwd"), None);
    assert_eq!(parse_target("/personal/%2e%2e/secret"), None);
    assert_eq!(parse_target("/personal//x.png"), None);
    assert_eq!(parse_target("/personal"), None);
    assert_eq!(parse_target("/../x/y"), None);
  }

  #[test]
  fn files_stay_inside_the_workspace_files_folder() {
    let path = file_path(Path::new("/ws/personal"), "abc/x.png");
    assert_eq!(path, PathBuf::from("/ws/personal/files/abc/x.png"));
  }

  #[test]
  fn active_content_is_served_inert() {
    assert_eq!(content_type("a/b.svg"), "image/svg+xml");
    assert_eq!(content_type("a/b.html"), "application/octet-stream");
    assert_eq!(content_type("a/b.PNG"), "image/png");
  }
}
