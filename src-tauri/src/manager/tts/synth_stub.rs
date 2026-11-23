use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use super::g2p::Phonemized;

pub const SUPPORTED: bool = false;

pub struct Engine;

impl Engine {
  pub fn load(_dir: &Path) -> Result<Self, String> {
    Err("English voice is not available on this platform".to_owned())
  }

  pub fn phonemize(&self, _text: &str) -> Phonemized {
    Phonemized::default()
  }

  pub fn synthesize(&self, _text: &str, _run: u64, _stopped_through: &Arc<AtomicU64>) -> Result<Vec<f32>, String> {
    Err("English voice is not available on this platform".to_owned())
  }
}
