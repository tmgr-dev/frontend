use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use flate2::read::GzDecoder;

use super::g2p::{Lexicon, Phonemized};
use super::ru_rules::{self, vowel_count};

const SPACE: char = ' ';
const SENTENCE_END: &[char] = &['.', '!', '?', ';', ':'];
const PAUSE: &[char] = &[','];
const VOWEL_TOKENS: &str = "aɑʌiɪyuoeɛɵɔə";
const BASE_INVENTORY: &str = "aɑʌiɪyuoeɛɵɔəbvɡdʒzkɭmnprstfxʃɕʑjʲˈˌŋ\"^ ,.!?;:";
const MIN_STEM: usize = 3;
const ENDINGS: &[&str] = &[
  "ться", "ется", "ами", "ями", "ого", "его", "ому", "ему", "ешь", "ишь", "ете", "ите", "ует", "уют", "ают", "яют", "ой", "ей", "ом", "ем", "ов", "ев", "ах", "ях", "ый", "ий", "ая", "яя", "ое", "ее",
  "ые", "ие", "ых", "их", "ым", "им", "ть", "ла", "ло", "ли", "ет", "ут", "ют", "ит", "ат", "ят", "ою", "ею", "ью", "ам", "ям", "а", "я", "у", "ю", "е", "ы", "и", "л", "о", "ь", "й",
];
const LETTER_NAMES: [&[(&str, usize)]; 26] = [
  &[("эй", 0)],
  &[("би", 0)],
  &[("си", 0)],
  &[("ди", 0)],
  &[("и", 0)],
  &[("эф", 0)],
  &[("джи", 0)],
  &[("эйч", 0)],
  &[("ай", 0)],
  &[("джей", 0)],
  &[("кей", 0)],
  &[("эл", 0)],
  &[("эм", 0)],
  &[("эн", 0)],
  &[("оу", 0)],
  &[("пи", 0)],
  &[("кью", 0)],
  &[("ар", 0)],
  &[("эс", 0)],
  &[("ти", 0)],
  &[("ю", 0)],
  &[("ви", 0)],
  &[("дабл", 0), ("ю", 0)],
  &[("экс", 0)],
  &[("уай", 1)],
  &[("зэд", 0)],
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Source {
  Known,
  Stem,
  Rules,
}

pub struct RuLexicon {
  words: HashMap<Box<str>, Box<str>>,
  allowed: HashSet<char>,
}

#[cfg(test)]
fn base_allowed() -> HashSet<char> {
  BASE_INVENTORY.chars().collect()
}

fn parse_tokens(text: &str) -> HashSet<char> {
  let mut allowed = HashSet::new();
  for line in text.lines() {
    let Some((symbol, _)) = line.rsplit_once(' ') else { continue };
    let mut chars = symbol.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
      allowed.insert(c);
    }
  }
  allowed
}

fn parse_lexicon<R: BufRead>(reader: R, allowed: &HashSet<char>) -> HashMap<Box<str>, Box<str>> {
  let mut words: HashMap<Box<str>, Box<str>> = HashMap::new();
  for line in reader.lines() {
    let Ok(line) = line else { break };
    let Some((word, phones)) = line.split_once('\t') else { continue };
    let (word, phones) = (word.trim(), phones.trim());
    if word.is_empty() || phones.is_empty() || word.chars().any(char::is_whitespace) || !phones.chars().all(|c| allowed.contains(&c) && !c.is_whitespace()) {
      continue;
    }
    words.entry(word.into()).or_insert_with(|| phones.into());
  }
  words
}

fn is_cyrillic(c: char) -> bool {
  matches!(c, 'а'..='я' | 'ё')
}

fn stress_index(phones: &[char]) -> Option<usize> {
  let at = phones.iter().position(|c| *c == 'ˈ')?;
  Some(phones[..at].iter().filter(|c| VOWEL_TOKENS.contains(**c)).count())
}

fn vowel_tokens(phones: &[char]) -> usize {
  phones.iter().filter(|c| VOWEL_TOKENS.contains(**c)).count()
}

fn stressed_vowel_at(tokens: &[char]) -> Option<usize> {
  let at = tokens.iter().position(|c| *c == 'ˈ')?;
  (at + 1..tokens.len()).find(|i| VOWEL_TOKENS.contains(tokens[*i]))
}

fn map_latin(c: char) -> Option<char> {
  let mapped = match c {
    'ɹ' => 'r',
    'æ' => 'a',
    'ð' => 'z',
    'θ' => 's',
    'w' => 'v',
    'h' => 'x',
    'l' => 'ɭ',
    'g' => 'ɡ',
    'ʊ' => 'u',
    'ɜ' => 'ɛ',
    'ɚ' => 'ə',
    'ː' => return None,
    other => other,
  };
  BASE_INVENTORY.contains(mapped).then_some(mapped)
}

fn spell_latin(word: &str) -> Vec<char> {
  let mut out: Vec<char> = Vec::new();
  for c in word.chars().filter(char::is_ascii_lowercase) {
    if !out.is_empty() {
      out.push(SPACE);
    }
    for (name, stress) in LETTER_NAMES[(c as u8 - b'a') as usize] {
      let chars: Vec<char> = name.chars().collect();
      out.extend(ru_rules::render(&chars, Some(*stress)).tokens);
    }
  }
  out
}

fn rules_word(word: &[char]) -> Vec<char> {
  ru_rules::render(word, ru_rules::guess_stress(word)).tokens
}

impl RuLexicon {
  pub fn load(dir: &Path) -> Result<Self, String> {
    let tokens = std::fs::read_to_string(dir.join("tokens.txt")).map_err(|e| format!("tokens.txt: {e}"))?;
    let allowed = parse_tokens(&tokens);
    if allowed.is_empty() {
      return Err("tokens.txt has no symbols".to_owned());
    }
    let words = match File::open(dir.join("lexicon-ru.txt.gz")) {
      Ok(file) => parse_lexicon(BufReader::new(GzDecoder::new(file)), &allowed),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
      Err(e) => return Err(format!("lexicon-ru.txt.gz: {e}")),
    };
    Ok(Self { words, allowed })
  }

  #[cfg(test)]
  fn from_tsv(text: &str) -> Self {
    let allowed = base_allowed();
    Self { words: parse_lexicon(text.as_bytes(), &allowed), allowed }
  }

  fn lookup(&self, word: &str) -> Option<Vec<char>> {
    self.words.get(word).map(|p| p.chars().collect())
  }

  fn with_stem(&self, word: &[char]) -> Option<Vec<char>> {
    for ending in ENDINGS {
      let e: Vec<char> = ending.chars().collect();
      if word.len() < e.len() + MIN_STEM || word[word.len() - e.len()..] != e[..] {
        continue;
      }
      let cut = word.len() - e.len();
      if vowel_count(&word[..cut]) == 0 {
        continue;
      }
      for with_j in [false, true] {
        let mut key: String = word[..cut].iter().collect();
        if with_j {
          key.push('й');
        }
        let Some(mut stem) = self.lookup(&key) else { continue };
        if with_j && stem.last() == Some(&'j') {
          stem.pop();
        } else if with_j {
          continue;
        }
        let Some(s) = stress_index(&stem) else { continue };
        if vowel_tokens(&stem) != vowel_count(&word[..cut]) {
          continue;
        }
        let full = ru_rules::render(word, Some(s));
        let alone = ru_rules::render(&word[..cut], Some(s));
        let old: Vec<char> = alone.tokens.iter().zip(&alone.spans).filter(|(_, sp)| **sp == cut - 1).map(|(t, _)| *t).collect();
        if !old.is_empty() && stem.ends_with(&old) {
          stem.truncate(stem.len() - old.len());
          stem.extend(full.tokens.iter().zip(&full.spans).filter(|(_, sp)| **sp == cut - 1).map(|(t, _)| *t));
        }
        stem.extend(full.tokens.iter().zip(&full.spans).filter(|(_, sp)| **sp >= cut).map(|(t, _)| *t));
        return Some(stem);
      }
    }
    None
  }

  fn with_yo(&self, word: &[char]) -> Option<Vec<char>> {
    let yo = ru_rules::yo_stress(word)?;
    let folded: String = word.iter().map(|c| if *c == 'ё' { 'е' } else { *c }).collect();
    let mut found = self.lookup(&folded)?;
    if stress_index(&found) != Some(yo) {
      return None;
    }
    let rules = ru_rules::render(word, Some(yo)).tokens;
    let (at, from) = (stressed_vowel_at(&found)?, stressed_vowel_at(&rules)?);
    found[at] = rules[from];
    Some(found)
  }

  fn russian(&self, word: &[char]) -> (Vec<char>, Source) {
    let key: String = word.iter().collect();
    if let Some(p) = self.lookup(&key) {
      return (p, Source::Known);
    }
    if let Some(p) = self.with_yo(word) {
      return (p, Source::Known);
    }
    if ru_rules::yo_stress(word).is_none() {
      if let Some(p) = self.with_stem(word) {
        return (p, Source::Stem);
      }
    }
    (rules_word(word), Source::Rules)
  }

  fn latin_word(&self, word: &str, latin: Option<&Lexicon>) -> (Vec<char>, Source) {
    if let Some(lex) = latin {
      let p = lex.phonemize(word);
      if p.oov.is_empty() && p.sentences.len() == 1 {
        let mapped: Vec<char> = p.sentences[0].iter().filter_map(|c| u32::try_from(*c).ok().and_then(char::from_u32)).filter_map(map_latin).collect();
        if !mapped.is_empty() {
          return (mapped, Source::Known);
        }
      }
    }
    (spell_latin(word), Source::Rules)
  }

  fn part(&self, part: &str, latin: Option<&Lexicon>) -> (Vec<char>, Source) {
    let cyrillic: Vec<char> = part.chars().filter(|c| is_cyrillic(*c)).collect();
    if cyrillic.is_empty() {
      return self.latin_word(part, latin);
    }
    self.russian(&cyrillic)
  }

  fn word(&self, word: &str, latin: Option<&Lexicon>) -> (Vec<char>, Source) {
    if let Some(p) = self.lookup(word) {
      return (p, Source::Known);
    }
    let mut tokens = Vec::new();
    let mut source = Source::Known;
    for part in word.split('-').filter(|p| !p.is_empty()) {
      let (ph, s) = self.part(part, latin);
      tokens.extend(ph);
      if s == Source::Rules || (s == Source::Stem && source == Source::Known) {
        source = s;
      }
    }
    (tokens, source)
  }

  pub fn phonemize(&self, text: &str, latin: Option<&Lexicon>) -> Phonemized {
    let mut out = Phonemized::default();
    let mut current: Vec<char> = Vec::new();
    let mut word = String::new();
    let chars: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    let flush = |word: &mut String, current: &mut Vec<char>, out: &mut Phonemized| {
      let w = std::mem::take(word);
      if w.is_empty() {
        return;
      }
      let (tokens, source) = self.word(&w, latin);
      let tokens: Vec<char> = tokens.into_iter().filter(|c| self.allowed.contains(c)).collect();
      out.words += 1;
      if tokens.is_empty() {
        out.oov.push(w);
        return;
      }
      if source == Source::Rules {
        out.guessed.push(w);
      }
      if current.last().is_some_and(|c| *c != SPACE) {
        current.push(SPACE);
      }
      current.extend(tokens);
    };
    let close = |current: &mut Vec<char>, out: &mut Phonemized| {
      if current.iter().any(|c| *c != SPACE) {
        out.sentences.push(current.iter().map(|c| *c as i32).collect());
      }
      current.clear();
    };
    for (i, &c) in chars.iter().enumerate() {
      let letter = is_cyrillic(c) || c.is_ascii_lowercase();
      if letter {
        word.push(c);
        continue;
      }
      let joins = chars.get(i + 1).is_some_and(|n| is_cyrillic(*n) || n.is_ascii_lowercase());
      if !word.is_empty() && joins && matches!(c, '-' | '\'' | '’') {
        word.push(if c == '’' { '\'' } else { c });
        continue;
      }
      flush(&mut word, &mut current, &mut out);
      if SENTENCE_END.contains(&c) {
        if !current.is_empty() {
          current.push(c);
          close(&mut current, &mut out);
        }
      } else if PAUSE.contains(&c) && !current.is_empty() {
        current.push(c);
      }
    }
    flush(&mut word, &mut current, &mut out);
    close(&mut current, &mut out);
    out
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const OBSERVED: &str = "ʲˈʌitnrsɭjuavkɑmpyʃdɪobezfɡxʒɕɛʑ\"ɵˌ^ŋɔə";
  const LEX: &str = "релиз\trʲiɭʲˈis\nспринт\tsprʲˈint\nтест\ttʲˈɛst\nпривет\tprʲivʲˈet\nмир\tmʲˈir\nкоммит\tkʌmmʲˈit\n";
  const PARAGRAPH: &str = "Сегодня утром я открыл репозиторий и увидел, что нужно перезапустить сборку. Команда уже написала новые тесты, но часть проверок падает без видимой причины. \
Нужно разобраться со всеми ошибками до конца недели, иначе релиз придётся отложить. Спасибо всем, кто помогал с настройкой окружения и обновлением зависимостей.";

  fn decode(cps: &[i32]) -> String {
    cps.iter().map(|c| char::from_u32(*c as u32).unwrap()).collect()
  }

  fn say(lex: &RuLexicon, text: &str) -> String {
    let p = lex.phonemize(text, None);
    p.sentences.iter().map(|s| decode(s)).collect::<Vec<_>>().join("|")
  }

  #[test]
  fn resolves_inflected_forms_through_their_stem() {
    let lex = RuLexicon::from_tsv(LEX);
    assert_eq!(say(&lex, "релиза"), "rʲiɭʲˈiza");
    assert_eq!(say(&lex, "спринта"), "sprʲˈinta");
    assert_eq!(say(&lex, "релизов"), "rʲiɭʲˈizʌf");
    assert_eq!(say(&lex, "релизе"), "rʲiɭʲˈiʑi");
    assert!(lex.phonemize("релиза", None).guessed.is_empty());
  }

  #[test]
  fn uses_rules_for_words_missing_from_the_lexicon() {
    let lex = RuLexicon::from_tsv(LEX);
    let p = lex.phonemize("раннер", None);
    assert_eq!(p.guessed, vec!["раннер".to_owned()]);
    assert!(!p.sentences[0].is_empty());
    assert_eq!(p.words, 1);
  }

  #[test]
  fn exact_lexicon_hits_win_and_are_not_guessed() {
    let lex = RuLexicon::from_tsv(LEX);
    let p = lex.phonemize("Привет, мир.", None);
    assert_eq!(decode(&p.sentences[0]), "prʲivʲˈet, mʲˈir.");
    assert!(p.guessed.is_empty() && p.oov.is_empty());
  }

  #[test]
  fn works_without_any_lexicon() {
    let lex = RuLexicon::from_tsv("");
    for w in ["раннер", "релиза", "спринта"] {
      assert!(!say(&lex, w).is_empty(), "{w}");
    }
    let p = lex.phonemize(PARAGRAPH, None);
    let words = PARAGRAPH.split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()).count();
    assert_eq!(p.words, words);
    assert!(p.sentences.iter().all(|s| !s.is_empty()));
  }

  #[test]
  fn paragraph_gives_phonemes_for_every_word_inside_the_inventory() {
    let lex = RuLexicon::from_tsv(LEX);
    let p = lex.phonemize(PARAGRAPH, None);
    let words = PARAGRAPH.split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()).count();
    assert_eq!(p.words, words);
    assert!(words >= 40);
    assert!(p.oov.is_empty());
    assert!(p.sentences.len() >= 3);
    for s in &p.sentences {
      assert!(!s.is_empty());
      for cp in s {
        let c = char::from_u32(*cp as u32).unwrap();
        assert!(OBSERVED.contains(c) || " ,.!?;:".contains(c), "{c:?}");
      }
    }
    for w in PARAGRAPH.split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()) {
      let one = lex.phonemize(w, None);
      assert_eq!(one.sentences.len(), 1, "{w}");
      assert!(one.sentences[0].iter().any(|c| *c != ' ' as i32), "{w}");
    }
  }

  #[test]
  fn splits_sentences_on_terminal_punctuation() {
    let lex = RuLexicon::from_tsv(LEX);
    assert_eq!(lex.phonemize("мир! тест? раннер; релиз: спринт.", None).sentences.len(), 5);
    assert_eq!(lex.phonemize("мир, тест", None).sentences.len(), 1);
  }

  #[test]
  fn ignores_digits_symbols_and_empty_input() {
    let lex = RuLexicon::from_tsv(LEX);
    assert!(lex.phonemize("   123 #$% ", None).sentences.is_empty());
    assert!(lex.phonemize("", None).sentences.is_empty());
    assert_eq!(lex.phonemize("   123 #$% ", None).words, 0);
  }

  #[test]
  fn a_word_with_yo_keeps_its_stress_on_yo() {
    let lex = RuLexicon::from_tsv("елка\tjˈeɭka\n");
    for w in ["ёлка", "всё", "объём", "нёс", "чёрный", "жёлтый"] {
      let s = say(&lex, w);
      let at = s.find('ˈ').unwrap_or_else(|| panic!("{w}: {s}"));
      let next: String = s[at + 'ˈ'.len_utf8()..].chars().take(1).collect();
      assert!(next == "ɵ" || next == "o", "{w}: {s}");
      assert_eq!(s.matches('ˈ').count(), 1, "{w}: {s}");
    }
    assert_eq!(say(&lex, "ёлка"), "jˈɵɭka");
  }

  #[test]
  fn handles_hyphenated_words_part_by_part() {
    let lex = RuLexicon::from_tsv(LEX);
    let p = lex.phonemize("тест-мир", None);
    assert_eq!(decode(&p.sentences[0]), "tʲˈɛstmʲˈir");
    assert_eq!(p.words, 1);
  }

  #[test]
  fn spells_latin_words_with_russian_letter_names_when_english_is_missing() {
    let lex = RuLexicon::from_tsv(LEX);
    let p = lex.phonemize("API", None);
    assert_eq!(p.words, 1);
    assert_eq!(p.guessed, vec!["api".to_owned()]);
    assert!(decode(&p.sentences[0]).contains(' '));
  }

  #[test]
  fn maps_english_phones_into_the_russian_inventory() {
    let english = Lexicon::parse("hello HH AH0 L OW1\nthink TH IH1 NG K\nwater W AO1 T ER0\nthe DH AH0\n");
    let lex = RuLexicon::from_tsv(LEX);
    assert_eq!(decode(&lex.phonemize("hello", Some(&english)).sentences[0]), "xəɭˈou");
    assert_eq!(decode(&lex.phonemize("think", Some(&english)).sentences[0]), "sˈɪŋk");
    let p = lex.phonemize("hello think water the nonsenseword", Some(&english));
    assert_eq!(p.guessed, vec!["nonsenseword".to_owned()]);
    for cp in &p.sentences[0] {
      let c = char::from_u32(*cp as u32).unwrap();
      assert!(OBSERVED.contains(c) || c == ' ', "{c:?}");
    }
  }

  #[test]
  fn rule_engine_emits_only_observed_inventory() {
    for c in ru_rules::EMITTABLE.chars() {
      assert!(OBSERVED.contains(c) && BASE_INVENTORY.contains(c), "{c:?}");
    }
    let letters: Vec<char> = "абвгдежзийклмнопрстуфхцчшщъыьэюяё".chars().collect();
    let mut seed = 0x9e3779b97f4a7c15_u64;
    let mut next = || {
      seed ^= seed << 13;
      seed ^= seed >> 7;
      seed ^= seed << 17;
      seed
    };
    for _ in 0..20000 {
      let len = 1 + (next() % 12) as usize;
      let word: Vec<char> = (0..len).map(|_| letters[(next() % letters.len() as u64) as usize]).collect();
      let nv = vowel_count(&word);
      for stress in [None, Some(0), Some(nv / 2), ru_rules::guess_stress(&word)] {
        for c in ru_rules::render(&word, stress).tokens {
          assert!(OBSERVED.contains(c), "{c:?} in {word:?}");
        }
      }
    }
  }

  #[test]
  fn drops_codepoints_outside_the_pack_inventory() {
    let mut lex = RuLexicon::from_tsv("");
    lex.allowed.remove(&'ʲ');
    let p = lex.phonemize("тест", None);
    assert!(!decode(&p.sentences[0]).contains('ʲ'));
    let text = "ок\tˈok\nплохо\tp\u{1}ˈo\n";
    let parsed = parse_lexicon(text.as_bytes(), &base_allowed());
    assert_eq!(parsed.len(), 1);
  }

  #[test]
  fn reads_tokens_file_symbols() {
    let allowed = parse_tokens("_ 0\n^ 1\n  3\nʲ 9\nab 10\n");
    assert!(allowed.contains(&'_') && allowed.contains(&' ') && allowed.contains(&'ʲ'));
    assert!(!allowed.contains(&'a'));
  }

  fn xorshift(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
  }

  fn edit(a: &[char], b: &[char]) -> usize {
    let mut d: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
      let mut prev = d[0];
      d[0] = i + 1;
      for (j, y) in b.iter().enumerate() {
        let cur = d[j + 1];
        d[j + 1] = (d[j + 1] + 1).min(d[j] + 1).min(prev + usize::from(x != y));
        prev = cur;
      }
    }
    d[b.len()]
  }

  #[derive(Default)]
  struct Score {
    n: usize,
    exact: usize,
    err: usize,
    len: usize,
    stress_n: usize,
    stress_ok: usize,
  }

  impl Score {
    fn add(&mut self, got: &[char], want: &[char]) {
      self.n += 1;
      self.exact += usize::from(got == want);
      self.err += edit(got, want);
      self.len += want.len();
      if want.contains(&'ˈ') {
        self.stress_n += 1;
        self.stress_ok += usize::from(stress_index(got) == stress_index(want));
      }
    }

    fn show(&self, label: &str) {
      let pct = |a: usize, b: usize| 100.0 * a as f64 / b.max(1) as f64;
      println!("{label}: n={} exact={:.1}% PER={:.2}% stress={:.1}%", self.n, pct(self.exact, self.n), pct(self.err, self.len), pct(self.stress_ok, self.stress_n));
    }
  }

  #[test]
  #[ignore]
  fn measure_on_held_out_words() {
    let path = std::env::var("TMGR_RU_SAMPLE").expect("TMGR_RU_SAMPLE");
    let tsv: String = std::fs::read_to_string(path).unwrap().lines().filter_map(|l| l.split_once('\t')).map(|(w, p)| format!("{w}\t{}\n", p.replace(' ', ""))).collect();
    let mut lex = RuLexicon::from_tsv(&tsv);
    println!("lexicon entries: {}", lex.words.len());
    let mut pool: Vec<String> = lex.words.iter().filter(|(w, p)| w.chars().all(is_cyrillic) && w.chars().count() >= 4 && p.contains('ˈ')).map(|(w, _)| w.to_string()).collect();
    pool.sort();
    let mut seed = 0x2545f4914f6cdd1d_u64;
    let mut held: Vec<(String, Vec<char>)> = Vec::new();
    while held.len() < 5000 {
      let i = (xorshift(&mut seed) % pool.len() as u64) as usize;
      let w = pool.swap_remove(i);
      let want: Vec<char> = lex.words.remove(w.as_str()).unwrap().chars().collect();
      held.push((w, want));
    }
    let (mut rules, mut rules_plain, mut rules_hit, mut full, mut hit, mut miss) = (Score::default(), Score::default(), Score::default(), Score::default(), Score::default(), Score::default());
    for (w, want) in &held {
      let chars: Vec<char> = w.chars().collect();
      let r = rules_word(&chars);
      rules.add(&r, want);
      let (got, source) = lex.russian(&chars);
      full.add(&got, want);
      if source == Source::Stem {
        hit.add(&got, want);
        rules_hit.add(&r, want);
      } else {
        miss.add(&got, want);
        rules_plain.add(&r, want);
      }
    }
    rules.show("(a) rules only, all held-out");
    rules_plain.show("(a) rules only, words without stem hit");
    rules_hit.show("(a) rules only, words that have a stem sibling");
    full.show("(b) stem lookup + rules, all held-out");
    hit.show("(b) subset resolved via stem");
    miss.show("(b) subset resolved by rules (no stem sibling)");
    for w in "раннер релиза спринта токенов репозиторию импорты билдом перезапусти запланируй ухудшатся тестовое коммит задеплоить дедлайн линтер созвон вебсокет офлайн".split(' ') {
      let chars: Vec<char> = w.chars().collect();
      let (got, source) = lex.russian(&chars);
      println!("{w}\t{}\t{source:?}", got.iter().collect::<String>());
    }
  }
}
