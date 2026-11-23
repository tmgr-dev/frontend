use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Lang {
  En,
  Ru,
}

impl Lang {
  pub fn parse(code: Option<&str>) -> Result<Self, String> {
    match code {
      None | Some("en") => Ok(Self::En),
      Some("ru") => Ok(Self::Ru),
      Some(other) => Err(format!("unknown voice language: {other}")),
    }
  }

  pub fn code(self) -> &'static str {
    match self {
      Self::En => "en",
      Self::Ru => "ru",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      Self::En => "English",
      Self::Ru => "Russian",
    }
  }
}

pub struct PackSpec {
  pub id: &'static str,
  pub base_url: &'static str,
  pub files: &'static [PackFile],
  pub sample_rate: i32,
}

pub fn spec(lang: Lang) -> &'static PackSpec {
  match lang {
    Lang::En => &EN_SPEC,
    Lang::Ru => &RU_SPEC,
  }
}

static EN_SPEC: PackSpec = PackSpec {
  id: "en-ljspeech-medium-1",
  base_url: "https://github.com/tmgr-dev/frontend/releases/download/tts-voices-en-1",
  files: &EN_FILES,
  sample_rate: 22050,
};

static RU_SPEC: PackSpec = PackSpec {
  id: "ru-denis-medium-1",
  base_url: "https://github.com/tmgr-dev/frontend/releases/download/tts-voices-ru-1",
  files: &RU_FILES,
  sample_rate: 22050,
};

pub struct PackFile {
  pub name: &'static str,
  pub sha256: &'static str,
  pub size: u64,
}

pub const EN_FILES: [PackFile; 4] = [
  PackFile { name: "model.onnx", sha256: "206a8403baad4df7642156918a6ebe205798290d82f24d0d0119d7afa8a2738e", size: 19_349_795 },
  PackFile { name: "tokens.txt", sha256: "ef3a7e4a8d1af0c9d4dc45aaae1a6242ebe24a7ed6f3d025a49eb29682784c6d", size: 940 },
  PackFile { name: "cmudict.dict", sha256: "81917843c7f44ce2b094ac63873c2c7a4cf802040792c455ba3ca406891c3d22", size: 3_618_488 },
  PackFile { name: "LICENSES.txt", sha256: "aff8dbd3d2f8830a9798cb846fc257520b5d064688d022c861d14def8b88b0c8", size: 2_148 },
];

pub const RU_FILES: [PackFile; 4] = [
  PackFile { name: "model.onnx", sha256: "4834628dc805fb73b0456c4b42a9ffbd6c9c05f64d868ef5627f9911a8b7069f", size: 63_222_141 },
  PackFile { name: "tokens.txt", sha256: "e8a50ae0c75612d18cfcf3f90800dca79f961e3ed97f9574f3535194469187fd", size: 968 },
  PackFile { name: "lexicon-ru.txt.gz", sha256: "58289ddcac35720f1e1218d11cf6aedfbe48512cdf0d3ee475d2d9163300dd9e", size: 3_714_028 },
  PackFile { name: "LICENSES.txt", sha256: "48704d1662d12c8eebed61af81c6a01a4fcf1dcc4d8a4375463ce911c06d028b", size: 562 },
];

pub fn total_size(files: &[PackFile]) -> u64 {
  files.iter().map(|f| f.size).sum()
}

pub fn pack_dir(app_data: &Path, lang: Lang) -> PathBuf {
  app_data.join("tts").join(spec(lang).id)
}

fn part_dir(dir: &Path) -> PathBuf {
  let mut name = dir.file_name().map(|n| n.to_os_string()).unwrap_or_default();
  name.push(".part");
  dir.with_file_name(name)
}

pub fn is_installed(dir: &Path, files: &[PackFile]) -> bool {
  files.iter().all(|f| fs::metadata(dir.join(f.name)).map(|m| m.is_file() && m.len() == f.size).unwrap_or(false))
}

pub fn clean_leftovers(dir: &Path) {
  let _ = fs::remove_dir_all(part_dir(dir));
}

pub fn remove(dir: &Path) -> Result<(), String> {
  clean_leftovers(dir);
  match fs::remove_dir_all(dir) {
    Ok(()) => Ok(()),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(e) => Err(e.to_string()),
  }
}

fn github_host(url: &url::Url) -> bool {
  url.host_str().is_some_and(|h| h == "github.com" || h.ends_with(".githubusercontent.com"))
}

fn client(https_only: bool) -> Result<reqwest::Client, String> {
  if rustls::crypto::CryptoProvider::get_default().is_none() {
    let _ = rustls::crypto::ring::default_provider().install_default();
  }
  reqwest::Client::builder()
    .https_only(https_only)
    .connect_timeout(Duration::from_secs(15))
    .read_timeout(Duration::from_secs(30))
    .user_agent("TMGR-desktop-voice-pack")
    .redirect(reqwest::redirect::Policy::custom(|attempt| {
      if attempt.previous().len() < 5 && github_host(attempt.url()) {
        attempt.follow()
      } else {
        attempt.stop()
      }
    }))
    .build()
    .map_err(|e| e.to_string())
}

fn hex(digest: &[u8]) -> String {
  digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub async fn install(
  dir: &Path,
  base_url: &str,
  files: &[PackFile],
  cancel: &AtomicBool,
  progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<(), String> {
  let client = client(base_url.starts_with("https://"))?;
  let part = part_dir(dir);
  let _ = fs::remove_dir_all(&part);
  fs::create_dir_all(&part).map_err(|e| e.to_string())?;
  let result = download_all(&client, &part, base_url, files, cancel, progress).await;
  if let Err(e) = result {
    let _ = fs::remove_dir_all(&part);
    return Err(e);
  }
  match fs::remove_dir_all(dir) {
    Ok(()) => {}
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
    Err(e) => {
      let _ = fs::remove_dir_all(&part);
      return Err(e.to_string());
    }
  }
  fs::rename(&part, dir).map_err(|e| {
    let _ = fs::remove_dir_all(&part);
    e.to_string()
  })
}

async fn download_all(
  client: &reqwest::Client,
  part: &Path,
  base_url: &str,
  files: &[PackFile],
  cancel: &AtomicBool,
  progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<(), String> {
  let total = total_size(files);
  let mut done = 0u64;
  progress(0, total);
  for file in files {
    let url = format!("{}/{}", base_url.trim_end_matches('/'), file.name);
    let mut response = client.get(&url).send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?;
    let mut out = fs::File::create(part.join(file.name)).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut written = 0u64;
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
      if cancel.load(Ordering::SeqCst) {
        return Err("cancelled".to_owned());
      }
      written += chunk.len() as u64;
      if written > file.size {
        return Err(format!("{} is larger than expected", file.name));
      }
      hasher.update(&chunk);
      out.write_all(&chunk).map_err(|e| e.to_string())?;
      progress(done + written, total);
    }
    out.flush().map_err(|e| e.to_string())?;
    if written != file.size || hex(&hasher.finalize()) != file.sha256 {
      return Err(format!("{} failed the integrity check", file.name));
    }
    done += written;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::{Read, Write};
  use std::net::TcpListener;
  use std::sync::atomic::AtomicU64;
  use std::sync::Arc;

  fn sha(bytes: &[u8]) -> &'static str {
    Box::leak(hex(&Sha256::digest(bytes)).into_boxed_str())
  }

  fn serve(files: Vec<(&'static str, Vec<u8>)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
      for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut buf = [0u8; 2048];
        let n = stream.read(&mut buf).unwrap_or(0);
        let head = String::from_utf8_lossy(&buf[..n]).to_string();
        let path = head.split_whitespace().nth(1).unwrap_or("/").trim_start_matches('/').to_owned();
        match files.iter().find(|(name, _)| *name == path) {
          Some((_, body)) => {
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            let _ = stream.write_all(body);
          }
          None => {
            let _ = write!(stream, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
          }
        }
      }
    });
    format!("http://127.0.0.1:{port}")
  }

  fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tmgr-tts-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
  }

  fn files_for(a: &[u8], b: &[u8]) -> Vec<PackFile> {
    vec![
      PackFile { name: "a.bin", sha256: sha(a), size: a.len() as u64 },
      PackFile { name: "b.bin", sha256: sha(b), size: b.len() as u64 },
    ]
  }

  #[tokio::test]
  async fn installs_verified_files_atomically_and_reports_progress() {
    let (a, b) = (vec![1u8; 5000], vec![2u8; 3000]);
    let base = serve(vec![("a.bin", a.clone()), ("b.bin", b.clone())]);
    let files = files_for(&a, &b);
    let root = tmp("ok");
    let dir = root.join("pack");
    let last = AtomicU64::new(0);
    let cancel = AtomicBool::new(false);
    install(&dir, &base, &files, &cancel, &|got, total| {
      assert_eq!(total, 8000);
      last.store(got, Ordering::SeqCst);
    })
    .await
    .unwrap();
    assert_eq!(last.load(Ordering::SeqCst), 8000);
    assert!(is_installed(&dir, &files));
    assert!(!part_dir(&dir).exists());
    assert_eq!(fs::read(dir.join("a.bin")).unwrap(), a);
  }

  #[tokio::test]
  async fn rejects_a_file_with_the_wrong_hash_and_leaves_nothing_behind() {
    let (a, b) = (vec![1u8; 100], vec![2u8; 100]);
    let base = serve(vec![("a.bin", a.clone()), ("b.bin", vec![9u8; 100])]);
    let files = files_for(&a, &b);
    let root = tmp("hash");
    let dir = root.join("pack");
    let err = install(&dir, &base, &files, &AtomicBool::new(false), &|_, _| {}).await.unwrap_err();
    assert!(err.contains("b.bin"), "{err}");
    assert!(!dir.exists());
    assert!(!part_dir(&dir).exists());
  }

  #[tokio::test]
  async fn rejects_a_file_larger_than_declared() {
    let a = vec![1u8; 100];
    let base = serve(vec![("a.bin", vec![1u8; 500])]);
    let files = vec![PackFile { name: "a.bin", sha256: sha(&a), size: 100 }];
    let root = tmp("big");
    let err = install(&root.join("pack"), &base, &files, &AtomicBool::new(false), &|_, _| {}).await.unwrap_err();
    assert!(err.contains("larger"), "{err}");
  }

  #[tokio::test]
  async fn a_missing_file_fails_the_install() {
    let a = vec![1u8; 10];
    let base = serve(vec![]);
    let files = vec![PackFile { name: "a.bin", sha256: sha(&a), size: 10 }];
    let root = tmp("404");
    assert!(install(&root.join("pack"), &base, &files, &AtomicBool::new(false), &|_, _| {}).await.is_err());
    assert!(!root.join("pack").exists());
  }

  #[tokio::test]
  async fn cancel_stops_the_download_and_cleans_up() {
    let a = vec![1u8; 4000];
    let base = serve(vec![("a.bin", a.clone())]);
    let files = vec![PackFile { name: "a.bin", sha256: sha(&a), size: 4000 }];
    let root = tmp("cancel");
    let dir = root.join("pack");
    let cancel = Arc::new(AtomicBool::new(true));
    let err = install(&dir, &base, &files, &cancel, &|_, _| {}).await.unwrap_err();
    assert_eq!(err, "cancelled");
    assert!(!dir.exists() && !part_dir(&dir).exists());
  }

  #[tokio::test]
  async fn replaces_an_older_install() {
    let a = vec![3u8; 50];
    let base = serve(vec![("a.bin", a.clone())]);
    let files = vec![PackFile { name: "a.bin", sha256: sha(&a), size: 50 }];
    let root = tmp("replace");
    let dir = root.join("pack");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("old.txt"), b"x").unwrap();
    install(&dir, &base, &files, &AtomicBool::new(false), &|_, _| {}).await.unwrap();
    assert!(!dir.join("old.txt").exists());
    assert!(is_installed(&dir, &files));
  }

  #[test]
  fn is_installed_requires_every_file_with_the_right_size() {
    let root = tmp("inst");
    let files = vec![PackFile { name: "a.bin", sha256: "", size: 3 }];
    assert!(!is_installed(&root, &files));
    fs::write(root.join("a.bin"), b"ab").unwrap();
    assert!(!is_installed(&root, &files));
    fs::write(root.join("a.bin"), b"abc").unwrap();
    assert!(is_installed(&root, &files));
  }

  #[test]
  fn remove_deletes_the_pack_and_leftovers_and_tolerates_absence() {
    let root = tmp("rm");
    let dir = root.join("pack");
    fs::create_dir_all(&dir).unwrap();
    fs::create_dir_all(part_dir(&dir)).unwrap();
    remove(&dir).unwrap();
    assert!(!dir.exists() && !part_dir(&dir).exists());
    remove(&dir).unwrap();
  }

  #[test]
  fn the_pinned_manifest_matches_the_documented_size() {
    assert_eq!(total_size(&EN_FILES), 19_349_795 + 940 + 3_618_488 + 2_148);
    assert!(EN_FILES.iter().all(|f| f.sha256.len() == 64));
    assert_eq!(total_size(&RU_FILES), 63_222_141 + 968 + 3_714_028 + 562);
    assert!(RU_FILES.iter().all(|f| f.sha256.len() == 64 && f.size > 0));
  }
}
