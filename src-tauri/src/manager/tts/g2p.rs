use std::collections::HashMap;

const SPACE: &str = " ";
const SENTENCE_END: &[char] = &['.', '!', '?', ';', ':'];
const PAUSE: &[char] = &[','];
const OOV_SHARE_LIMIT_PERCENT: usize = 10;
const MAX_WORD_LEN: usize = 24;

const LETTERS: [&str; 26] = [
  "ˈ e ɪ", "b ˈ iː", "s ˈ iː", "d ˈ iː", "ˈ iː", "ˈ ɛ f", "d ʒ ˈ iː", "ˈ e ɪ t ʃ", "ˈ a ɪ", "d ʒ ˈ e ɪ", "k ˈ e ɪ", "ˈ ɛ l", "ˈ ɛ m", "ˈ ɛ n", "ˈ o ʊ", "p ˈ iː", "k j ˈ uː", "ˈ ɑː ɹ",
  "ˈ ɛ s", "t ˈ iː", "j ˈ uː", "v ˈ iː", "d ˈ ʌ b ə l j ˌ uː", "ˈ ɛ k s", "w ˈ a ɪ", "z ˈ iː",
];

fn arpa(code: &str) -> Option<&'static str> {
  Some(match code {
    "AA" => "ɑː",
    "AE" => "æ",
    "AH" => "ʌ",
    "AO" => "ɔː",
    "AW" => "a ʊ",
    "AY" => "a ɪ",
    "EH" => "ɛ",
    "ER" => "ɜː",
    "EY" => "e ɪ",
    "IH" => "ɪ",
    "IY" => "iː",
    "OW" => "o ʊ",
    "OY" => "ɔ ɪ",
    "UH" => "ʊ",
    "UW" => "uː",
    "B" => "b",
    "CH" => "t ʃ",
    "D" => "d",
    "DH" => "ð",
    "F" => "f",
    "G" => "ɡ",
    "HH" => "h",
    "JH" => "d ʒ",
    "K" => "k",
    "L" => "l",
    "M" => "m",
    "N" => "n",
    "NG" => "ŋ",
    "P" => "p",
    "R" => "ɹ",
    "S" => "s",
    "SH" => "ʃ",
    "T" => "t",
    "TH" => "θ",
    "V" => "v",
    "W" => "w",
    "Y" => "j",
    "Z" => "z",
    "ZH" => "ʒ",
    _ => return None,
  })
}

fn is_vowel(code: &str) -> bool {
  matches!(code, "AA" | "AE" | "AH" | "AO" | "AW" | "AY" | "EH" | "ER" | "EY" | "IH" | "IY" | "OW" | "OY" | "UH" | "UW")
}

fn phones(pron: &[&str]) -> Option<Vec<String>> {
  let mut out = Vec::new();
  for p in pron {
    let (base, stress) = match p.char_indices().last() {
      Some((i, c)) if c.is_ascii_digit() => (&p[..i], Some(c)),
      _ => (*p, None),
    };
    if is_vowel(base) {
      match (base, stress) {
        ("AH", Some('0')) => out.push("ə".to_owned()),
        ("ER", Some('0')) => out.push("ɚ".to_owned()),
        _ => {
          match stress {
            Some('1') => out.push("ˈ".to_owned()),
            Some('2') => out.push("ˌ".to_owned()),
            _ => {}
          }
          out.extend(arpa(base)?.split(' ').map(str::to_owned));
        }
      }
    } else {
      out.extend(arpa(base)?.split(' ').map(str::to_owned));
    }
  }
  Some(out)
}

#[derive(Debug, Default, PartialEq)]
pub struct Phonemized {
  pub sentences: Vec<Vec<i32>>,
  pub words: usize,
  pub oov: Vec<String>,
}

impl Phonemized {
  pub fn too_many_unknown(&self) -> bool {
    self.words > 0 && self.oov.len() * 100 > self.words * OOV_SHARE_LIMIT_PERCENT
  }
}

pub struct Lexicon {
  words: HashMap<String, Vec<String>>,
}

impl Lexicon {
  pub fn parse(text: &str) -> Self {
    let mut words = HashMap::new();
    for line in text.lines() {
      let line = line.split('#').next().unwrap_or("").trim();
      let mut parts = line.split_whitespace();
      let Some(word) = parts.next() else { continue };
      if word.ends_with(')') || !word.chars().all(|c| c.is_ascii_lowercase() || c == '\'') {
        continue;
      }
      let pron: Vec<&str> = parts.collect();
      if let Some(ph) = phones(&pron) {
        words.entry(word.to_owned()).or_insert(ph);
      }
    }
    Self { words }
  }

  pub fn len(&self) -> usize {
    self.words.len()
  }

  pub fn is_empty(&self) -> bool {
    self.words.is_empty()
  }

  fn derived(&self, word: &str) -> Option<Vec<String>> {
    let with = |base: &str, tail: &[&str]| -> Option<Vec<String>> {
      let mut ph = self.words.get(base)?.clone();
      ph.extend(tail.iter().map(|s| (*s).to_owned()));
      Some(ph)
    };
    let last = |base: &str| self.words.get(base).and_then(|p| p.last().cloned());
    let voiced_tail = |l: &str, voiceless: &[&str], sibilant: &[&str]| -> Vec<&'static str> {
      if sibilant.contains(&l) {
        vec!["ɪ", "z"]
      } else if voiceless.contains(&l) {
        vec!["s"]
      } else {
        vec!["z"]
      }
    };
    const VOICELESS: &[&str] = &["p", "t", "k", "f", "θ"];
    const SIBILANT: &[&str] = &["s", "z", "ʃ", "ʒ"];
    let plural = [word.strip_suffix("'s"), word.strip_suffix("es"), word.strip_suffix('s')];
    for base in plural.into_iter().flatten() {
      if let Some(l) = last(base) {
        let prev = self.words.get(base).and_then(|p| p.iter().rev().nth(1).cloned()).unwrap_or_default();
        let affricate = (l == "ʃ" || l == "ʒ") && (prev == "t" || prev == "d");
        let tail = if affricate { vec!["ɪ", "z"] } else { voiced_tail(&l, VOICELESS, SIBILANT) };
        if word.ends_with("es") && !word.ends_with("'s") && tail != ["ɪ", "z"] && !base.ends_with('e') && l != "z" {
          continue;
        }
        return with(base, &tail);
      }
    }
    if let Some(base) = word.strip_suffix("ed") {
      for cand in [base.to_owned(), format!("{base}e")] {
        if let Some(l) = last(&cand) {
          let tail: &[&str] = if l == "t" || l == "d" {
            &["ɪ", "d"]
          } else if VOICELESS.contains(&l.as_str()) || l == "s" || l == "ʃ" {
            &["t"]
          } else {
            &["d"]
          };
          return with(&cand, tail);
        }
      }
    }
    if let Some(base) = word.strip_suffix("ing") {
      for cand in [base.to_owned(), format!("{base}e")] {
        if last(&cand).is_some() {
          return with(&cand, &["ɪ", "ŋ"]);
        }
      }
    }
    None
  }

  fn spell(word: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in word.chars().filter(|c| c.is_ascii_lowercase()) {
      if !out.is_empty() {
        out.push(SPACE.to_owned());
      }
      out.extend(LETTERS[(c as u8 - b'a') as usize].split(' ').map(str::to_owned));
    }
    out
  }

  fn word(&self, word: &str, oov: &mut Vec<String>) -> Vec<String> {
    if word.len() <= MAX_WORD_LEN {
      if let Some(ph) = self.words.get(word) {
        return ph.clone();
      }
      if let Some(ph) = self.derived(word) {
        return ph;
      }
    }
    oov.push(word.to_owned());
    Self::spell(word)
  }

  pub fn phonemize(&self, text: &str) -> Phonemized {
    let mut out = Phonemized::default();
    let mut current: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut flush_word = |word: &mut String, current: &mut Vec<String>, out: &mut Phonemized| {
      let w = word.trim_matches('\'').to_lowercase();
      word.clear();
      if w.is_empty() || !w.chars().any(|c| c.is_ascii_lowercase()) {
        return;
      }
      if !current.is_empty() && current.last().map(String::as_str) != Some(SPACE) {
        current.push(SPACE.to_owned());
      }
      let ph = self.word(&w, &mut out.oov);
      out.words += 1;
      current.extend(ph);
    };
    let close = |current: &mut Vec<String>, out: &mut Phonemized| {
      if current.iter().any(|t| t != SPACE) {
        out.sentences.push(current.iter().flat_map(|t| t.chars().map(|c| c as i32)).collect());
      }
      current.clear();
    };
    for c in text.chars() {
      if c.is_ascii_alphabetic() || (c == '\'' && !word.is_empty()) || c == '’' {
        word.push(if c == '’' { '\'' } else { c });
        continue;
      }
      flush_word(&mut word, &mut current, &mut out);
      if SENTENCE_END.contains(&c) {
        if !current.is_empty() {
          current.push(c.to_string());
          close(&mut current, &mut out);
        }
      } else if PAUSE.contains(&c) && !current.is_empty() {
        current.push(c.to_string());
      }
    }
    flush_word(&mut word, &mut current, &mut out);
    close(&mut current, &mut out);
    out
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const DICT: &str = "hello HH AH0 L OW1\nworld W ER1 L D\nwalk W AO1 K\nmake M EY1 K\nbox B AA1 K S\nwatch W AA1 CH\nbed B EH1 D\nneed N IY1 D\nthe DH AH0\nthe(2) DH IY0\nbad B AE1 D\n";

  fn decode(cps: &[i32]) -> String {
    cps.iter().map(|c| char::from_u32(*c as u32).unwrap()).collect()
  }

  #[test]
  fn maps_arpabet_to_piper_tokens_with_stress_before_the_vowel() {
    let lex = Lexicon::parse(DICT);
    let p = lex.phonemize("Hello, world.");
    assert_eq!(p.sentences.len(), 1);
    assert_eq!(decode(&p.sentences[0]), "həlˈoʊ, wˈɜːld.");
    assert_eq!(p.words, 2);
    assert!(p.oov.is_empty());
  }

  #[test]
  fn keeps_the_first_pronunciation_only() {
    let lex = Lexicon::parse(DICT);
    assert_eq!(decode(&lex.phonemize("the").sentences[0]), "ðə");
  }

  #[test]
  fn splits_sentences_on_terminal_punctuation() {
    let lex = Lexicon::parse(DICT);
    let p = lex.phonemize("hello! world? walk");
    assert_eq!(p.sentences.len(), 3);
  }

  #[test]
  fn builds_inflected_forms_from_the_base_word() {
    let lex = Lexicon::parse(DICT);
    let say = |t: &str| decode(&lex.phonemize(t).sentences[0]);
    assert_eq!(say("walks"), "wˈɔːks");
    assert_eq!(say("boxes"), "bˈɑːksɪz");
    assert_eq!(say("walked"), "wˈɔːkt");
    assert_eq!(say("needed"), "nˈiːdɪd");
    assert_eq!(say("making"), "mˈeɪkɪŋ");
    assert_eq!(say("worlds"), "wˈɜːldz");
    assert_eq!(say("hello's"), "həlˈoʊz");
    assert_eq!(say("watches"), "wˈɑːtʃɪz");
    assert_eq!(say("bad"), "bˈæd");
  }

  #[test]
  fn spells_unknown_words_letter_by_letter_and_reports_them() {
    let lex = Lexicon::parse(DICT);
    let p = lex.phonemize("hello xq");
    assert_eq!(p.oov, vec!["xq".to_owned()]);
    assert_eq!(decode(&p.sentences[0]), "həlˈoʊ ˈɛks kjˈuː");
  }

  #[test]
  fn flags_a_message_with_too_many_unknown_words() {
    let lex = Lexicon::parse(DICT);
    assert!(lex.phonemize("hello zzz qqq").too_many_unknown());
    assert!(!lex.phonemize("hello world walk make box watch bed need bad world").too_many_unknown());
  }

  #[test]
  fn ignores_digits_symbols_and_empty_input() {
    let lex = Lexicon::parse(DICT);
    assert!(lex.phonemize("   123 #$% ").sentences.is_empty());
    assert!(lex.phonemize("").sentences.is_empty());
  }

  #[test]
  fn treats_the_curly_apostrophe_like_the_straight_one() {
    let lex = Lexicon::parse("it's IH1 T S\n");
    assert_eq!(decode(&lex.phonemize("it’s").sentences[0]), "ˈɪts");
  }

  #[test]
  fn skips_dictionary_lines_it_cannot_map() {
    let lex = Lexicon::parse("good G UH1 D\nbroken B XX1\n# comment\n");
    assert_eq!(lex.len(), 1);
  }
}
