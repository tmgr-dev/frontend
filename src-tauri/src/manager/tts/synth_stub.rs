use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use super::g2p::{Lexicon, Phonemized};
use super::pack::Lang;

pub const SUPPORTED: bool = false;

pub struct Engine;

impl Engine {
  pub fn load(_dir: &Path, _lang: Lang) -> Result<Self, String> {
    Err("voices are not available on this platform".to_owned())
  }

  pub fn english(&self) -> Option<&Lexicon> {
    None
  }

  pub fn phonemize(&self, _text: &str, _latin: Option<&Lexicon>) -> Phonemized {
    Phonemized::default()
  }

  pub fn synthesize(&self, _text: &str, _run: u64, _stopped_through: &Arc<AtomicU64>, _latin: Option<&Lexicon>) -> Result<Vec<f32>, String> {
    Err("voices are not available on this platform".to_owned())
  }
}
