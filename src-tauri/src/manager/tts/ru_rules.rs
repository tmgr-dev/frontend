#[cfg(test)]
pub const EMITTABLE: &str = "aɑʌiɪyuoeɛɵbvɡdʒzkɭmnprstfxʃɕjʲˈˌʑ\"";

const VOWELS: &str = "аеёиоуыэюя";
const SOFTENERS: &str = "еёиюяь";
const MAX_SUFFIX: usize = 7;

const SUFFIX_STRESS: &[(&str, u8)] = &[
  ("ава", 1), ("авали", 1), ("авит", 1), ("айся", 1), ("аков", 1), ("альна", 1), ("аму", 1), ("ано", 2), ("аны", 2),
  ("асти", 1), ("ат", 0), ("аю", 1), ("б", 0), ("бе", 1), ("бо", 1), ("ва", 2), ("ваем", 3), ("вает", 3), ("вается", 4),
  ("ваешь", 3), ("вай", 2), ("вал", 2), ("вала", 3), ("валась", 3), ("вали", 3), ("вались", 3), ("вало", 3),
  ("валось", 3), ("вался", 3), ("вана", 2), ("вание", 4), ("вании", 4), ("ванию", 4), ("вания", 4), ("вать", 2),
  ("ваться", 3), ("ваю", 3), ("ваюсь", 3), ("вают", 3), ("ваются", 4), ("вая", 3), ("ваясь", 3), ("ве", 1), ("вик", 0),
  ("вил", 1), ("вич", 1), ("вна", 2), ("во", 1), ("вого", 3), ("вые", 3), ("вый", 2), ("г", 0), ("ге", 1), ("д", 0),
  ("дая", 1), ("де", 1), ("до", 1), ("дя", 0), ("е", 2), ("евает", 1), ("евал", 0), ("евала", 1), ("евали", 1),
  ("евать", 0), ("евая", 1), ("еву", 2), ("евую", 1), ("ек", 1), ("екли", 0), ("еле", 2), ("елен", 2), ("ельная", 3),
  ("ельницу", 3), ("ельно", 2), ("ельного", 3), ("ельное", 3), ("ельной", 2), ("ельном", 2), ("ельному", 3),
  ("ельную", 3), ("ельные", 3), ("ельный", 2), ("ельным", 2), ("ельными", 3), ("ельных", 2), ("ельстве", 2), ("елю", 2),
  ("еля", 2), ("емая", 3), ("емой", 2), ("емости", 3), ("емую", 3), ("емым", 2), ("ена", 2), ("ено", 2), ("ены", 2),
  ("еская", 3), ("ески", 2), ("еские", 3), ("еский", 2), ("еским", 2), ("еских", 2), ("еского", 3), ("еское", 3),
  ("еской", 2), ("еском", 2), ("ескому", 3), ("ескую", 3), ("ества", 2), ("естве", 2), ("ество", 2), ("еством", 2),
  ("ести", 0), ("еся", 3), ("ете", 2), ("етесь", 2), ("ец", 1), ("ея", 1), ("еями", 2), ("же", 1), ("зе", 1), ("зм", 0),
  ("зму", 1), ("зо", 1), ("ива", 1), ("ией", 2), ("ием", 2), ("ии", 2), ("ик", 1), ("ика", 2), ("икам", 2), ("иками", 3),
  ("иках", 2), ("ике", 2), ("ики", 2), ("икой", 2), ("иком", 2), ("ику", 2), ("имого", 2), ("имое", 2), ("иму", 1),
  ("имые", 2), ("имый", 1), ("имых", 1), ("ирая", 1), ("ирован", 2), ("ит", 0), ("ица", 2), ("ицу", 2), ("ицы", 2),
  ("ича", 2), ("ичать", 2), ("ичу", 2), ("йл", 1), ("йт", 0), ("к", 0), ("кал", 1), ("ке", 1), ("кет", 0), ("кина", 2),
  ("ко", 1), ("ков", 2), ("ковал", 0), ("кт", 0), ("л", 0), ("ле", 1), ("лей", 2), ("лем", 2), ("лл", 1), ("ло", 1),
  ("лося", 2), ("лся", 1), ("лью", 1), ("лю", 1), ("ля", 1), ("ме", 1), ("мена", 1), ("ми", 2), ("мися", 3), ("мо", 1),
  ("мого", 3), ("мое", 3), ("му", 2), ("мые", 3), ("мый", 2), ("мых", 2), ("не", 1), ("нина", 2), ("нись", 0),
  ("ничал", 2), ("нков", 1), ("нная", 3), ("нно", 2), ("нного", 3), ("нное", 3), ("нной", 2), ("нном", 2), ("нному", 3),
  ("нности", 3), ("нность", 2), ("нностью", 3), ("нную", 3), ("нные", 3), ("нный", 2), ("нным", 2), ("нными", 3),
  ("нных", 2), ("но", 1), ("нскими", 2), ("нт", 0), ("нут", 1), ("нута", 2), ("нуто", 2), ("нутого", 3), ("нький", 2),
  ("ньким", 2), ("нью", 1), ("ню", 1), ("ня", 1), ("ням", 1), ("нями", 2), ("нях", 1), ("о", 2), ("овое", 3), ("ову", 2),
  ("ок", 1), ("оков", 1), ("ол", 1), ("олог", 1), ("онная", 2), ("онного", 2), ("онное", 2), ("онной", 1), ("онном", 1),
  ("онную", 2), ("онные", 2), ("онный", 1), ("онным", 1), ("онными", 2), ("онных", 1), ("оры", 2), ("ося", 3), ("от", 0),
  ("оти", 1), ("очка", 2), ("очкой", 2), ("очку", 2), ("очной", 2), ("очные", 3), ("очный", 2), ("очным", 2), ("оя", 1),
  ("п", 0), ("пе", 1), ("плю", 0), ("по", 1), ("ре", 1), ("рее", 1), ("рет", 0), ("рик", 0), ("рму", 1), ("ро", 1),
  ("рована", 3), ("ровано", 3), ("рованы", 3), ("рт", 0), ("рти", 1), ("рю", 0), ("ря", 1), ("се", 1), ("скими", 3),
  ("со", 1), ("ст", 0), ("сть", 1), ("сю", 1), ("тва", 1), ("те", 1), ("тей", 2), ("тели", 2), ("ти", 2), ("то", 1),
  ("тора", 2), ("торами", 3), ("торов", 2), ("тором", 2), ("тору", 2), ("тскими", 2), ("тт", 0), ("тти", 1), ("тъ", 0),
  ("ть", 0), ("тя", 1), ("увала", 2), ("ували", 2), ("ует", 2), ("уется", 3), ("ул", 1), ("уса", 2), ("усь", 0),
  ("уся", 3), ("ут", 0), ("утая", 3), ("утой", 2), ("уты", 2), ("утые", 3), ("утый", 2), ("утым", 2), ("утыми", 3),
  ("утых", 2), ("уют", 2), ("уются", 3), ("фе", 1), ("хе", 1), ("хо", 1), ("ц", 0), ("це", 1), ("цей", 2), ("цов", 0),
  ("цом", 0), ("ць", 0), ("цю", 1), ("ця", 1), ("ч", 0), ("че", 1), ("чков", 0), ("чо", 1), ("чок", 0), ("чь", 0),
  ("ше", 1), ("шься", 2), ("щая", 3), ("щаяся", 4), ("щего", 3), ("щегося", 4), ("щее", 3), ("щей", 2), ("щейся", 3),
  ("щем", 2), ("щему", 3), ("щие", 3), ("щиеся", 4), ("щий", 2), ("щийся", 3), ("щим", 2), ("щими", 3), ("щимся", 3),
  ("щины", 2), ("щих", 2), ("щихся", 3), ("щую", 3), ("щуюся", 4), ("ыв", 0), ("ье", 1), ("ькая", 3), ("ькие", 3),
  ("ькое", 3), ("ьна", 2), ("ьнее", 3), ("ьница", 3), ("ьницы", 3), ("ьности", 3), ("ьность", 2), ("ьны", 2),
  ("ьства", 2), ("ьство", 2), ("ься", 1), ("ья", 1), ("ьям", 1), ("ьями", 2), ("ьях", 1), ("ю", 2), ("юся", 3),
  ("ються", 2), ("ючи", 2), ("ющееся", 4), ("ющимися", 4), ("я", 2), ("яй", 0), ("яка", 0), ("яков", 0), ("ям", 2),
  ("ями", 3), ("яся", 3), ("ях", 2), ("ящая", 2), ("ящего", 2), ("ящее", 2), ("ящей", 1), ("ящем", 1), ("ящие", 2),
  ("ящиеся", 3), ("ящий", 1), ("ящим", 1), ("ящими", 2), ("ящих", 1), ("ящую", 2), ("яю", 1), ("яя", 1),
];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
  Cons,
  Vowel,
  Sign,
}

struct Unit {
  kind: Kind,
  letter: char,
  index: usize,
  ph: String,
  soft: bool,
  soft_by_vowel: bool,
  target: bool,
}

pub struct Rendered {
  pub tokens: Vec<char>,
  pub spans: Vec<usize>,
}

pub fn is_vowel(c: char) -> bool {
  VOWELS.contains(c)
}

pub fn vowel_count(word: &[char]) -> usize {
  word.iter().filter(|c| is_vowel(**c)).count()
}

pub fn yo_stress(word: &[char]) -> Option<usize> {
  let mut k = 0;
  for c in word {
    if *c == 'ё' {
      return Some(k);
    }
    if is_vowel(*c) {
      k += 1;
    }
  }
  None
}

pub fn guess_stress(word: &[char]) -> Option<usize> {
  let nv = vowel_count(word);
  if nv == 0 {
    return None;
  }
  if let Some(k) = yo_stress(word) {
    return Some(k);
  }
  if nv == 1 {
    return Some(0);
  }
  let mut from_end = 1;
  for n in (1..=MAX_SUFFIX.min(word.len())).rev() {
    let suffix: String = word[word.len() - n..].iter().collect();
    if let Ok(i) = SUFFIX_STRESS.binary_search_by(|(s, _)| (*s).cmp(suffix.as_str())) {
      from_end = SUFFIX_STRESS[i].1 as usize;
      break;
    }
  }
  Some(nv - 1 - from_end.min(nv - 1))
}

fn base(c: char) -> Option<&'static str> {
  Some(match c {
    'б' => "b",
    'в' => "v",
    'г' => "ɡ",
    'д' => "d",
    'ж' => "ʒ",
    'з' => "z",
    'к' => "k",
    'л' => "ɭ",
    'м' => "m",
    'н' => "n",
    'п' => "p",
    'р' => "r",
    'с' => "s",
    'т' => "t",
    'ф' => "f",
    'х' => "x",
    'ц' => "ts",
    'ч' => "tʃ",
    'ш' => "ʃ",
    'щ' => "ɕ",
    'й' => "j",
    _ => return None,
  })
}

fn voiced_of(ph: &str) -> Option<&'static str> {
  Some(match ph {
    "p" => "b",
    "t" => "d",
    "k" => "ɡ",
    "s" => "z",
    "ʃ" => "ʒ",
    "f" => "v",
    "tʃ" => "dʒ",
    _ => return None,
  })
}

fn voiceless_of(ph: &str) -> Option<&'static str> {
  Some(match ph {
    "b" => "p",
    "d" => "t",
    "ɡ" => "k",
    "z" => "s",
    "ʒ" => "ʃ",
    "v" => "f",
    _ => return None,
  })
}

fn is_voiced(ph: &str) -> bool {
  matches!(ph, "b" | "d" | "ɡ" | "z" | "ʒ")
}

fn is_voiceless(ph: &str) -> bool {
  matches!(ph, "p" | "t" | "k" | "f" | "s" | "ʃ" | "x" | "ɕ" | "ts" | "tʃ")
}

struct Ctx<'a> {
  w: &'a [char],
  n: usize,
  tail: bool,
  stress: Option<usize>,
  vidx: Vec<usize>,
}

impl Ctx<'_> {
  fn prev(&self, i: usize) -> char {
    if i > 0 {
      self.w[i - 1]
    } else {
      '^'
    }
  }

  fn vowel(&self, i: usize) -> &'static str {
    let c = self.w[i];
    let k = self.vidx[i];
    let rel = self.stress.map_or(1, |s| k as i64 - s as i64);
    let prev = self.prev(i);
    let last = i + 1 == self.n && !self.tail;
    let stressed = rel == 0;
    let glide_before = prev == '^' || is_vowel(prev) || prev == 'ь' || prev == 'ъ';
    match c {
      'а' => {
        if stressed {
          "ɑ"
        } else if last && (prev == 'ч' || prev == 'щ') {
          "ʌ"
        } else if rel == -1 {
          "a"
        } else if rel < 0 && k == 0 {
          if prev == '^' {
            "a"
          } else {
            "ʌ"
          }
        } else if rel > 0 && last {
          "a"
        } else {
          "ʌ"
        }
      }
      'я' => {
        if stressed {
          if glide_before {
            "a"
          } else {
            "ɑ"
          }
        } else if prev == '^' || rel == -1 || glide_before {
          "a"
        } else {
          "ʌ"
        }
      }
      'о' => {
        if stressed {
          "o"
        } else {
          "ʌ"
        }
      }
      'ё' => {
        if !stressed {
          "ʌ"
        } else if prev == 'ж' || prev == 'ш' {
          "o"
        } else {
          "ɵ"
        }
      }
      'э' => "ɛ",
      'ы' => "y",
      'у' | 'ю' => "u",
      'е' => {
        let hard = matches!(prev, 'ж' | 'ш' | 'ц');
        if !stressed && k == 0 && prev == '^' {
          "ɪ"
        } else if stressed {
          if hard {
            "ɛ"
          } else {
            "e"
          }
        } else if hard {
          "y"
        } else if rel > 0 && last && (is_vowel(prev) || prev == 'ь') {
          "ɪ"
        } else {
          "i"
        }
      }
      _ => {
        let hard = matches!(prev, 'ж' | 'ш' | 'ц');
        if hard {
          "y"
        } else if (k == 0 && prev == '^' && self.n > 1) || last {
          "ɪ"
        } else {
          "i"
        }
      }
    }
  }
}

fn build_units(w: &[char], n: usize) -> Vec<Unit> {
  let ends = |suffix: &str| {
    let s: Vec<char> = suffix.chars().collect();
    w.len() >= s.len() && w[w.len() - s.len()..] == s[..]
  };
  let ogo = ends("ого") || ends("его");
  let at = |i: usize, s: &str| {
    let s: Vec<char> = s.chars().collect();
    i + s.len() <= w.len() && w[i..i + s.len()] == s[..]
  };
  let mut units: Vec<Unit> = Vec::new();
  for i in 0..n {
    let c = w[i];
    let nx = if i + 1 < n { w[i + 1] } else { '\0' };
    let nx2 = if i + 2 < n { w[i + 2] } else { '\0' };
    if let Some(b) = base(c) {
      let mut letter = c;
      let mut ph = b.to_owned();
      let mut soft = false;
      let mut target = true;
      match c {
        'ч' => {
          if i == 0 && at(0, "чт") {
            ph = "ʃ".to_owned();
            letter = 'ш';
          } else {
            soft = true;
          }
        }
        'ж' | 'ш' | 'ц' | 'щ' | 'й' => {}
        _ => {
          soft = nx != '\0' && SOFTENERS.contains(nx);
          if c == 'л' && nx == 'ь' && nx2 != '\0' && !is_vowel(nx2) {
            soft = false;
          }
          if nx == 'ь' && nx2 != '\0' && is_vowel(nx2) && matches!(c, 'н' | 'л' | 'п') {
            soft = false;
          }
        }
      }
      if c == 'г' && ogo && i + 2 == w.len() {
        ph = "v".to_owned();
      }
      if c == 'г' && matches!(nx, 'к' | 'ч') {
        ph = "x".to_owned();
      }
      if c == 'д' && at(i, "дцат") {
        ph = String::new();
      }
      if c == 'к' && nx == 'к' {
        ph = String::new();
      }
      if c == 'т' && i > 0 && w[i - 1] == 'с' && nx == 'н' {
        ph = String::new();
      }
      if c == 'д' && i > 0 && w[i - 1] == 'з' && nx == 'н' {
        ph = String::new();
      }
      if c == 'д' && i > 0 && w[i - 1] == 'р' && nx == 'ц' {
        ph = String::new();
      }
      if c == 'н' && i >= 2 && ((w[i - 1] == 'т' && w[i - 2] == 'с') || (w[i - 1] == 'д' && w[i - 2] == 'з')) {
        target = false;
      }
      let soft_by_vowel = soft && nx != 'ь';
      units.push(Unit { kind: Kind::Cons, letter, index: i, ph, soft, soft_by_vowel, target });
    } else if is_vowel(c) {
      units.push(Unit { kind: Kind::Vowel, letter: c, index: i, ph: String::new(), soft: false, soft_by_vowel: false, target: true });
    } else if c == 'ь' || c == 'ъ' {
      units.push(Unit { kind: Kind::Sign, letter: c, index: i, ph: String::new(), soft: false, soft_by_vowel: false, target: true });
    }
  }
  units
}

fn assimilate(units: &mut [Unit]) {
  let m = units.len();
  for j in 0..m.saturating_sub(1) {
    let (a, b) = (&units[j], &units[j + 1]);
    if a.kind == Kind::Cons && b.kind == Kind::Cons && matches!(a.letter, 'с' | 'з') && matches!(b.letter, 'ш' | 'ж' | 'ч' | 'щ') {
      if matches!(b.letter, 'ш' | 'ж') {
        let ph = units[j + 1].ph.clone();
        units[j].ph = ph;
        units[j].soft = false;
      } else {
        units[j].ph = "ɕ".to_owned();
        units[j + 1].ph = String::new();
        units[j].soft = false;
      }
    }
  }
  for j in (0..m.saturating_sub(1)).rev() {
    let (a, b) = (&units[j], &units[j + 1]);
    if a.kind != Kind::Cons || b.kind != Kind::Cons || a.soft || !b.soft_by_vowel || !b.target {
      continue;
    }
    let hit = (matches!(a.letter, 'с' | 'з') && matches!(b.letter, 'т' | 'д' | 'с' | 'з' | 'н'))
      || (matches!(a.letter, 'м' | 'н') && matches!(b.letter, 'б' | 'п'))
      || (matches!(a.letter, 'т' | 'д') && b.letter == 'н')
      || (matches!(a.letter, 'т' | 'д' | 'с' | 'з') && a.letter == b.letter);
    if hit {
      units[j].soft = true;
    }
  }
  for j in (0..m).rev() {
    if units[j].kind != Kind::Cons {
      continue;
    }
    let ph = units[j].ph.clone();
    if voiced_of(&ph).is_none() && voiceless_of(&ph).is_none() {
      continue;
    }
    let next = units[j + 1..].iter().find(|u| u.kind != Kind::Sign);
    match next {
      None => {
        if let Some(v) = voiceless_of(&ph) {
          units[j].ph = v.to_owned();
        }
      }
      Some(b) if b.kind == Kind::Cons && b.letter != 'в' => {
        if is_voiced(&b.ph) {
          if let Some(v) = voiced_of(&ph) {
            units[j].ph = v.to_owned();
          }
        } else if is_voiceless(&b.ph) {
          if let Some(v) = voiceless_of(&ph) {
            units[j].ph = v.to_owned();
          }
        }
      }
      _ => {}
    }
  }
}

pub fn render(word: &[char], stress: Option<usize>) -> Rendered {
  let (n, tail): (usize, &[char]) = {
    let len = word.len();
    let ends = |s: &str| {
      let s: Vec<char> = s.chars().collect();
      len >= s.len() && word[len - s.len()..] == s[..]
    };
    if ends("ться") {
      (len - 4, &['t', 's', 'ˌ', 'ʌ'])
    } else if ends("тся") {
      (len - 3, &['t', 's', 'ʌ'])
    } else {
      (len, &[])
    }
  };
  let w = word;
  let mut vidx = vec![0; w.len()];
  let mut k = 0;
  for (i, c) in w.iter().enumerate() {
    if is_vowel(*c) {
      vidx[i] = k;
      k += 1;
    }
  }
  let ctx = Ctx { w, n, tail: !tail.is_empty(), stress, vidx };
  let mut units = build_units(w, n);
  assimilate(&mut units);
  let mut out = Rendered { tokens: Vec::new(), spans: Vec::new() };
  let push = |out: &mut Rendered, t: char, span: usize| {
    out.tokens.push(t);
    out.spans.push(span);
  };
  for u in &units {
    let i = u.index;
    let c = u.letter;
    match u.kind {
      Kind::Cons => {
        if u.ph == "z" && u.soft {
          push(&mut out, 'ʑ', i);
          continue;
        }
        for t in u.ph.chars() {
          push(&mut out, t, i);
        }
        if !u.ph.is_empty() && (c == 'ч' || u.soft) {
          push(&mut out, 'ʲ', i);
        }
      }
      Kind::Vowel => {
        let prev = ctx.prev(i);
        let glide = matches!(c, 'е' | 'ё' | 'ю' | 'я') && (prev == '^' || is_vowel(prev) || prev == 'ь' || prev == 'ъ');
        let mut ph = ctx.vowel(i).to_owned();
        let st = Some(ctx.vidx[i]) == stress;
        if matches!(c, 'и' | 'о') && prev == 'ь' {
          push(&mut out, 'j', i);
        }
        if c == 'ю' && !glide && prev != 'ь' && prev != 'ъ' && base(prev).is_some() {
          ph.push('"');
        }
        if glide && prev == 'ъ' {
          push(&mut out, 'j', i);
          push(&mut out, 'j', i);
          if st {
            push(&mut out, 'ˈ', i);
          }
        } else if glide && prev == 'ь' && c == 'ё' {
          push(&mut out, 'j', i);
          if st {
            push(&mut out, 'ˈ', i);
          }
        } else if glide && prev == 'ь' && matches!(c, 'ю' | 'я') {
          push(&mut out, 'j', i);
          if st {
            push(&mut out, 'ˈ', i);
          }
          push(&mut out, 'j', i);
        } else if glide && prev == 'ь' {
          push(&mut out, 'j', i);
          push(&mut out, 'j', i);
          if st {
            push(&mut out, 'ˈ', i);
          }
        } else if glide {
          if matches!(c, 'ю' | 'я') {
            if st {
              push(&mut out, 'ˈ', i);
            }
            push(&mut out, 'j', i);
          } else {
            push(&mut out, 'j', i);
            if st {
              push(&mut out, 'ˈ', i);
            }
          }
        } else if st {
          push(&mut out, 'ˈ', i);
        }
        for t in ph.chars() {
          push(&mut out, t, i);
        }
      }
      Kind::Sign => {}
    }
  }
  for t in tail {
    push(&mut out, *t, n);
  }
  out
}
