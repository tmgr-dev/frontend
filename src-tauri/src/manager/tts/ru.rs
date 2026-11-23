use std::path::Path;

use super::g2p::{Lexicon, Phonemized};

pub struct RuLexicon;

impl RuLexicon {
  pub fn load(_dir: &Path) -> Result<Self, String> {
    Err("the Russian voice is not implemented yet".to_owned())
  }

  pub fn phonemize(&self, _text: &str, _latin: Option<&Lexicon>) -> Phonemized {
    Phonemized::default()
  }
}
