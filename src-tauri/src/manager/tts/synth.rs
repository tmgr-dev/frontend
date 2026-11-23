use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use sherpa_onnx::{GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsModelConfig, OfflineTtsVitsModelConfig};

use super::g2p::{Lexicon, Phonemized};
use super::pack::Lang;
use super::ru::RuLexicon;

const NOISE_SCALE: f32 = 0.5;
const NOISE_SCALE_W: f32 = 0.333;
const LENGTH_SCALE: f32 = 1.0;
const THREADS: i32 = 2;

pub const SUPPORTED: bool = true;

enum Phon {
  En(Lexicon),
  Ru(RuLexicon),
}

pub struct Engine {
  tts: OfflineTts,
  phon: Phon,
  busy: Mutex<()>,
}

impl Engine {
  pub fn load(dir: &Path, lang: Lang) -> Result<Self, String> {
    let phon = match lang {
      Lang::En => Phon::En(Lexicon::parse(&fs::read_to_string(dir.join("cmudict.dict")).map_err(|e| e.to_string())?)),
      Lang::Ru => Phon::Ru(RuLexicon::load(dir)?),
    };
    let path = |n: &str| dir.join(n).to_str().map(str::to_owned).ok_or_else(|| "the voice path is not valid UTF-8".to_owned());
    let config = OfflineTtsConfig {
      model: OfflineTtsModelConfig {
        vits: OfflineTtsVitsModelConfig {
          model: Some(path("model.onnx")?),
          tokens: Some(path("tokens.txt")?),
          noise_scale: NOISE_SCALE,
          noise_scale_w: NOISE_SCALE_W,
          length_scale: LENGTH_SCALE,
          ..Default::default()
        },
        num_threads: THREADS,
        ..Default::default()
      },
      ..Default::default()
    };
    let tts = OfflineTts::create(&config).ok_or_else(|| "the voice model failed to load".to_owned())?;
    Ok(Self { tts, phon, busy: Mutex::new(()) })
  }

  pub fn english(&self) -> Option<&Lexicon> {
    match &self.phon {
      Phon::En(l) => Some(l),
      Phon::Ru(_) => None,
    }
  }

  pub fn phonemize(&self, text: &str, latin: Option<&Lexicon>) -> Phonemized {
    match &self.phon {
      Phon::En(l) => l.phonemize(text),
      Phon::Ru(r) => r.phonemize(text, latin),
    }
  }

  pub fn synthesize(&self, text: &str, run: u64, stopped_through: &Arc<AtomicU64>, latin: Option<&Lexicon>) -> Result<Vec<f32>, String> {
    let _turn = self.busy.lock().map_err(|e| e.to_string())?;
    let live = stopped_through.clone();
    if live.load(Ordering::SeqCst) >= run {
      return Ok(Vec::new());
    }
    let phonemized = self.phonemize(text, latin);
    if phonemized.sentences.is_empty() {
      return Ok(Vec::new());
    }
    let config = GenerationConfig { phoneme_codepoints: Some(phonemized.sentences), ..Default::default() };
    let audio = self
      .tts
      .generate_with_config(
        "",
        &config,
        Some(move |_: &[f32], _: f32| live.load(Ordering::SeqCst) < run),
      )
      .ok_or_else(|| "speech generation failed".to_owned())?;
    if stopped_through.load(Ordering::SeqCst) >= run {
      return Ok(Vec::new());
    }
    Ok(audio.samples().to_vec())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  #[ignore = "needs TMGR_TTS_PACK_DIR pointing at a built voice pack"]
  fn speaks_a_sentence_with_the_real_pack() {
    let dir = std::env::var("TMGR_TTS_PACK_DIR").expect("TMGR_TTS_PACK_DIR");
    let engine = Engine::load(Path::new(&dir), Lang::En).unwrap();
    assert_eq!(engine.tts.sample_rate(), 22050);
    let stopped = Arc::new(AtomicU64::new(0));
    let started = std::time::Instant::now();
    let samples = engine.synthesize("The build finished, and the release is ready. Please check the logs.", 1, &stopped, None).unwrap();
    let seconds = samples.len() as f32 / 22050.0;
    assert!(seconds > 3.0 && seconds < 12.0, "{seconds}");
    assert!(samples.iter().any(|s| s.abs() > 0.05));
    assert!(started.elapsed().as_secs_f32() < seconds * 2.0);
  }

  #[test]
  #[ignore = "needs TMGR_TTS_PACK_DIR pointing at a built voice pack"]
  fn stopping_the_generation_returns_no_samples() {
    let dir = std::env::var("TMGR_TTS_PACK_DIR").expect("TMGR_TTS_PACK_DIR");
    let engine = Engine::load(Path::new(&dir), Lang::En).unwrap();
    let stopped = Arc::new(AtomicU64::new(0));
    let bump = stopped.clone();
    let long = "This is a fairly long sentence that takes a while to say. ".repeat(6);
    let handle = std::thread::spawn({
      let stopped = stopped.clone();
      move || engine.synthesize(&long, 1, &stopped, None)
    });
    std::thread::sleep(std::time::Duration::from_millis(150));
    bump.fetch_max(1, Ordering::SeqCst);
    assert!(handle.join().unwrap().unwrap().is_empty());
  }

  #[test]
  #[ignore = "needs TMGR_TTS_RU_PACK_DIR pointing at a built Russian voice pack"]
  fn speaks_russian_with_the_real_pack() {
    let dir = std::env::var("TMGR_TTS_RU_PACK_DIR").expect("TMGR_TTS_RU_PACK_DIR");
    let engine = Engine::load(Path::new(&dir), Lang::Ru).unwrap();
    let stopped = Arc::new(AtomicU64::new(0));
    let text = "Раннер запущен, релиз спринта готов. Проверь журнал сборки, пожалуйста.";
    let phon = engine.phonemize(text, None);
    assert!(phon.sentences.len() == 2 && phon.sentences.iter().all(|s| !s.is_empty()));
    let started = std::time::Instant::now();
    let samples = engine.synthesize(text, 1, &stopped, None).unwrap();
    let seconds = samples.len() as f32 / 22050.0;
    assert!(seconds > 3.0 && seconds < 15.0, "{seconds}");
    assert!(samples.iter().any(|s| s.abs() > 0.05));
    assert!(started.elapsed().as_secs_f32() < seconds * 3.0);
  }
}
