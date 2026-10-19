use crate::workdir::git_bytes;
use base64::Engine;
use regex::{Regex, RegexBuilder};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;
use tmgr_core::error::{Error, Result};

const MAX_FILE: u64 = 512 * 1024;
const BINARY_PROBE: usize = 8192;
const K: usize = 25;
const W: usize = 8;
const M: usize = 3;
const EXCERPT_MAX: usize = 160;

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub path: String,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct PrivateSide {
    pub full_name: String,
    pub files: Vec<SourceFile>,
}

#[derive(Clone, Debug)]
pub struct AddedBlock {
    pub path: String,
    pub line: u32,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct PublicSide {
    pub full_name: String,
    pub base_files: Vec<SourceFile>,
    pub new_files: Vec<String>,
    pub added: Vec<AddedBlock>,
    pub texts: Vec<(String, String)>,
    pub unscannable: Vec<(String, String)>,
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Overlap,
    ForeignFile,
    Unscannable,
    Secret,
    InternalName,
}

impl Category {
    pub fn name(self) -> &'static str {
        match self {
            Category::Overlap => "overlap",
            Category::ForeignFile => "foreign file",
            Category::Unscannable => "unscannable file",
            Category::Secret => "secret",
            Category::InternalName => "internal name",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Location {
    File { path: String, line: u32 },
    Text { label: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub category: Category,
    pub public_repo: String,
    pub location: Location,
    pub private_ref: Option<String>,
    pub excerpt: String,
}

struct NameSet {
    kind: &'static str,
    re: Regex,
}

pub struct Index {
    paths: Vec<String>,
    grams: HashMap<u64, u32>,
    private_paths: HashSet<String>,
    private_exts: HashSet<String>,
    names: Vec<NameSet>,
}

fn strip_comment_marks(line: &str) -> &str {
    let mut t = line.trim();
    loop {
        let before = t;
        for p in ["///", "//", "/*", "*/", "<!--", "-->", "--", "*", "#"] {
            if let Some(rest) = t.strip_prefix(p) {
                t = rest.trim_start();
            }
        }
        for p in ["*/", "-->"] {
            if let Some(rest) = t.strip_suffix(p) {
                t = rest.trim_end();
            }
        }
        if t == before {
            break;
        }
    }
    t
}

fn is_trivial(t: &str) -> bool {
    if !t.chars().any(|c| c.is_alphanumeric()) {
        return true;
    }
    const LEADS: [&str; 10] = [
        "import ",
        "import(",
        "package ",
        "use ",
        "#include",
        "require ",
        "using ",
        "export * from",
        "export {",
        "@import",
    ];
    if LEADS.iter().any(|p| t.starts_with(p)) {
        return true;
    }
    (t.starts_with("from ") && t.contains(" import ")) || require_line().is_match(t)
}

fn require_line() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        re(r"^(?:(?:const|let|var)\s+[^=;]+=\s*)?require\([^()]*\)(?:\.[A-Za-z_$][\w$]*)*[;\s]*$")
    })
}

struct Tokens {
    hashes: Vec<u64>,
    lines: Vec<u32>,
}

fn tokenize(text: &str) -> Tokens {
    let mut hashes = Vec::new();
    let mut lines = Vec::new();
    for (ln, raw) in text.lines().enumerate() {
        let line = strip_comment_marks(raw);
        if is_trivial(line) {
            continue;
        }
        let mut word: Option<u64> = None;
        let push = |h: u64, hashes: &mut Vec<u64>, lines: &mut Vec<u32>| {
            hashes.push(h);
            lines.push(ln as u32);
        };
        for c in line.chars() {
            if matches!(
                c,
                '\u{200b}'..='\u{200d}' | '\u{2060}' | '\u{feff}' | '\u{ad}'
            ) {
                continue;
            }
            if c.is_alphanumeric() || c == '_' || c == '$' {
                let mut h = word.unwrap_or(0xcbf2_9ce4_8422_2325);
                for lc in c.to_lowercase() {
                    h = (h ^ lc as u64).wrapping_mul(0x0100_0000_01b3);
                }
                word = Some(h);
                continue;
            }
            if let Some(h) = word.take() {
                push(h, &mut hashes, &mut lines);
            }
            if !c.is_whitespace() {
                let h = (0x9e37_79b9_7f4a_7c15u64 ^ c as u64).wrapping_mul(0x0100_0000_01b3);
                push(h, &mut hashes, &mut lines);
            }
        }
        if let Some(h) = word.take() {
            push(h, &mut hashes, &mut lines);
        }
    }
    Tokens { hashes, lines }
}

const BASE: u64 = 0x9e37_79b9_7f4a_7c15;

fn kgrams(tokens: &[u64]) -> Vec<u64> {
    if tokens.len() < K {
        return Vec::new();
    }
    let mut top = 1u64;
    for _ in 0..K {
        top = top.wrapping_mul(BASE);
    }
    let mut out = Vec::with_capacity(tokens.len() - K + 1);
    let mut h = 0u64;
    for (i, &t) in tokens.iter().enumerate() {
        h = h.wrapping_mul(BASE).wrapping_add(t);
        if i >= K {
            h = h.wrapping_sub(tokens[i - K].wrapping_mul(top));
        }
        if i + 1 >= K {
            out.push(h);
        }
    }
    out
}

fn winnow(grams: &[u64]) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    if grams.is_empty() {
        return out;
    }
    let w = W.min(grams.len());
    for start in 0..=(grams.len() - w) {
        let mut best = start;
        for j in start..start + w {
            if grams[j] <= grams[best] {
                best = j;
            }
        }
        if out.last() != Some(&best) {
            out.push(best);
        }
    }
    out
}

fn ext_of(path: &str) -> Option<String> {
    let base = path.rsplit('/').next().unwrap_or(path);
    let dot = base.rfind('.')?;
    if dot == 0 || dot + 1 == base.len() {
        return None;
    }
    Some(base[dot + 1..].to_ascii_lowercase())
}

fn is_config(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let mut segs: Vec<&str> = lower.split('/').collect();
    let base = segs.pop().unwrap_or("");
    if base.starts_with("application")
        || base.ends_with(".properties")
        || base.starts_with(".env")
        || base.starts_with("dockerfile")
        || (base.contains("compose") && (base.ends_with(".yml") || base.ends_with(".yaml")))
    {
        return true;
    }
    segs.iter().any(|s| {
        matches!(*s, "k8s" | "helm" | "charts") || s.starts_with("deploy") || s.starts_with("infra")
    })
}

const GENERIC_ENV: [&str; 40] = [
    "NODE_ENV",
    "LOG_LEVEL",
    "GITHUB_TOKEN",
    "API_URL",
    "DATABASE_URL",
    "APP_ENV",
    "JAVA_HOME",
    "JAVA_OPTS",
    "SPRING_PROFILES_ACTIVE",
    "REDIS_URL",
    "REDIS_HOST",
    "REDIS_PORT",
    "REDIS_PASSWORD",
    "DB_HOST",
    "DB_PORT",
    "DB_NAME",
    "DB_USER",
    "DB_USERNAME",
    "DB_PASSWORD",
    "DB_DATABASE",
    "POSTGRES_USER",
    "POSTGRES_PASSWORD",
    "POSTGRES_DB",
    "MYSQL_DATABASE",
    "MYSQL_USER",
    "MYSQL_PASSWORD",
    "MYSQL_ROOT_PASSWORD",
    "AWS_REGION",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_ENDPOINT",
    "APP_URL",
    "APP_NAME",
    "APP_DEBUG",
    "APP_PORT",
    "BASE_URL",
    "API_KEY",
    "NPM_TOKEN",
    "DEBIAN_FRONTEND",
    "TZ_DATA",
];

const HOST_SKIP: [&str; 12] = [
    "example.com",
    "example.org",
    "example.net",
    "github.com",
    "githubusercontent.com",
    "google.com",
    "apache.org",
    "w3.org",
    "npmjs.org",
    "npmjs.com",
    "docker.io",
    "mozilla.org",
];

fn re(pattern: &str) -> Regex {
    RegexBuilder::new(pattern)
        .size_limit(256 << 20)
        .build()
        .expect("static pattern")
}

fn package_decl() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| re(r"(?m)^\s*package\s+([a-z][a-z0-9_]*(?:\.[a-z][a-z0-9_]*)+)\s*;?"))
}

fn host_pattern() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        re(r"(?i)\b(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+(?:com|net|org|io|dev|app|ai|co|cloud|ru|de|eu|tech|info|biz|xyz|online|site|me|us|uk|fr|nl|internal|local|svc|lan|corp)\b")
    })
}

fn ip_pattern() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| re(r"\b(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})\b"))
}

fn env_patterns() -> &'static [Regex; 3] {
    static R: OnceLock<[Regex; 3]> = OnceLock::new();
    R.get_or_init(|| {
        [
            re(r"\$\{([A-Z][A-Z0-9_]*)(?::[^}]*)?\}"),
            re(r"(?m)^\s*(?:export\s+)?([A-Z][A-Z0-9_]*)="),
            re(r#"(?m)^\s*-\s*name:\s*["']?([A-Z][A-Z0-9_]*)["']?\s*$"#),
        ]
    })
}

fn public_ip(a: u32, b: u32, c: u32, d: u32) -> bool {
    if a > 255 || b > 255 || c > 255 || d > 255 {
        return false;
    }
    !(a == 127
        || a == 0
        || a == 255
        || (a, b, c) == (192, 0, 2)
        || (a, b, c) == (198, 51, 100)
        || (a, b, c) == (203, 0, 113))
}

fn name_set(
    kind: &'static str,
    names: &BTreeSet<String>,
    case_insensitive: bool,
) -> Option<NameSet> {
    if names.is_empty() {
        return None;
    }
    let mut sorted: Vec<&String> = names.iter().collect();
    sorted.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    let alt = sorted
        .iter()
        .map(|n| regex::escape(n))
        .collect::<Vec<_>>()
        .join("|");
    let flag = if case_insensitive { "(?i)" } else { "" };
    Some(NameSet {
        kind,
        re: re(&format!(r"{flag}(?-u:\b)(?:{alt})(?-u:\b)")),
    })
}

impl Index {
    pub fn build(private: &[PrivateSide]) -> Index {
        let mut paths = Vec::new();
        let mut grams: HashMap<u64, u32> = HashMap::new();
        let mut private_paths = HashSet::new();
        let mut private_exts = HashSet::new();
        let mut packages = BTreeSet::new();
        let mut hosts = BTreeSet::new();
        let mut ips = BTreeSet::new();
        let mut envs = BTreeSet::new();
        for side in private {
            for f in &side.files {
                private_paths.insert(f.path.clone());
                if let Some(e) = ext_of(&f.path) {
                    private_exts.insert(e);
                }
                if matches!(ext_of(&f.path).as_deref(), Some("java" | "kt" | "kts")) {
                    for cap in package_decl().captures_iter(&f.text) {
                        let segs: Vec<&str> = cap[1].split('.').take(3).collect();
                        if segs.len() == 3 {
                            packages.insert(segs.join("."));
                        }
                    }
                }
                if is_config(&f.path) {
                    for m in host_pattern().find_iter(&f.text) {
                        let h = m.as_str().to_ascii_lowercase();
                        if !HOST_SKIP
                            .iter()
                            .any(|s| h == *s || h.ends_with(&format!(".{s}")))
                        {
                            hosts.insert(h);
                        }
                    }
                    for c in ip_pattern().captures_iter(&f.text) {
                        let n: Vec<u32> = (1..=4).map(|i| c[i].parse().unwrap_or(999)).collect();
                        if public_ip(n[0], n[1], n[2], n[3]) {
                            ips.insert(c[0].to_string());
                        }
                    }
                    for r in env_patterns() {
                        for c in r.captures_iter(&f.text) {
                            let n = &c[1];
                            if n.len() >= 6 && n.contains('_') && !GENERIC_ENV.contains(&n) {
                                envs.insert(n.to_string());
                            }
                        }
                    }
                }
                if is_lockfile(&f.path) {
                    continue;
                }
                let tokens = tokenize(&f.text);
                let all = kgrams(&tokens.hashes);
                if all.is_empty() {
                    continue;
                }
                let mut idx: Option<u32> = None;
                for pos in winnow(&all) {
                    let i = *idx.get_or_insert_with(|| {
                        paths.push(format!("{}:{}", side.full_name, f.path));
                        (paths.len() - 1) as u32
                    });
                    grams.entry(all[pos]).or_insert(i);
                }
            }
        }
        let names = [
            name_set("package prefix", &packages, false),
            name_set("hostname", &hosts, true),
            name_set("ip address", &ips, false),
            name_set("env var", &envs, false),
        ]
        .into_iter()
        .flatten()
        .collect();
        Index {
            paths,
            grams,
            private_paths,
            private_exts,
            names,
        }
    }
}

fn excerpt(line: &str) -> String {
    let t = line.trim();
    match t.char_indices().nth(EXCERPT_MAX) {
        Some((i, _)) => t[..i].to_string(),
        None => t.to_string(),
    }
}

struct Unit<'a> {
    loc: Location,
    text: &'a str,
    first_line: u32,
    shown: Option<String>,
}

impl Unit<'_> {
    fn line_text(&self, idx: u32) -> &str {
        match &self.shown {
            Some(s) => s,
            None => self.text.lines().nth(idx as usize).unwrap_or(""),
        }
    }
}

fn line_loc(unit: &Unit, idx: u32) -> Location {
    let idx = if unit.shown.is_some() { 0 } else { idx };
    match &unit.loc {
        Location::File { path, .. } => Location::File {
            path: path.clone(),
            line: unit.first_line + idx,
        },
        t => t.clone(),
    }
}

struct SecretRule {
    name: &'static str,
    re: Regex,
    entropy: bool,
}

fn secret_rules() -> &'static [SecretRule] {
    static R: OnceLock<Vec<SecretRule>> = OnceLock::new();
    R.get_or_init(|| {
        let plain = |name, p: &str| SecretRule {
            name,
            re: re(p),
            entropy: false,
        };
        vec![
            plain("private key", r"-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY-----"),
            plain("aws access key", r"\bAKIA[0-9A-Z]{16}\b"),
            plain("github token", r"\bgh[pousr]_[A-Za-z0-9]{36}\b"),
            plain("github token", r"\bgithub_pat_[A-Za-z0-9_]{22,}"),
            plain("slack token", r"\bxox[abpors]-[A-Za-z0-9-]{10,}"),
            plain("anthropic key", r"\bsk-ant-[A-Za-z0-9_-]{20,}"),
            plain("openai key", r"\bsk-[A-Za-z0-9_-]{32,}"),
            plain(
                "jwt",
                r"\beyJ[A-Za-z0-9_-]{5,}\.eyJ[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}",
            ),
            SecretRule {
                name: "credential literal",
                re: re(
                    r#"(?i)(?:password|secret|token|api_?key)[A-Za-z0-9_\-]*["']?\s*(?::|=>|=)\s*["']([^"'\s]{20,})["']"#,
                ),
                entropy: true,
            },
        ]
    })
}

fn entropy(s: &str) -> f64 {
    let mut counts: HashMap<char, usize> = HashMap::new();
    let mut n = 0usize;
    for c in s.chars() {
        *counts.entry(c).or_default() += 1;
        n += 1;
    }
    counts
        .values()
        .map(|&c| {
            let p = c as f64 / n as f64;
            -p * p.log2()
        })
        .sum()
}

fn placeholder(v: &str) -> bool {
    let l = v.to_ascii_lowercase();
    l.contains("xxx")
        || l.starts_with('<')
        || l.contains("changeme")
        || l.contains("${")
        || l.contains("example")
}

fn redact(line: &str, start: usize, end: usize) -> String {
    format!("{}[redacted]{}", &line[..start], &line[end..])
}

fn scan_secrets(unit: &Unit, repo: &str, out: &mut Vec<Finding>) {
    for (i, line) in unit.text.lines().enumerate() {
        for rule in secret_rules() {
            let hit = if rule.entropy {
                rule.re.captures_iter(line).find_map(|c| {
                    let v = c.get(1)?;
                    let ok = !placeholder(v.as_str()) && entropy(v.as_str()) >= 3.5;
                    ok.then(|| (v.start(), v.end()))
                })
            } else {
                rule.re.find(line).map(|m| (m.start(), m.end()))
            };
            if let Some((s, e)) = hit {
                let shown = if unit.shown.is_some() {
                    "[redacted decoded payload]".to_string()
                } else {
                    excerpt(&redact(line, s, e))
                };
                out.push(Finding {
                    category: Category::Secret,
                    public_repo: repo.to_string(),
                    location: line_loc(unit, i as u32),
                    private_ref: Some(rule.name.to_string()),
                    excerpt: shown,
                });
            }
        }
    }
}

struct Lines(Vec<usize>);

impl Lines {
    fn new(text: &str) -> Lines {
        let mut starts = vec![0];
        starts.extend(
            text.bytes()
                .enumerate()
                .filter(|&(_, b)| b == b'\n')
                .map(|(i, _)| i + 1),
        );
        Lines(starts)
    }

    fn of(&self, offset: usize) -> u32 {
        (self.0.partition_point(|&s| s <= offset) - 1) as u32
    }
}

struct NameHit {
    kind: &'static str,
    name: String,
    unit: usize,
    line: u32,
}

fn collect_names(index: &Index, units: &[Unit]) -> Vec<NameHit> {
    let mut hits = Vec::new();
    for (ui, unit) in units.iter().enumerate() {
        let lines = Lines::new(unit.text);
        for set in &index.names {
            for m in set.re.find_iter(unit.text) {
                hits.push(NameHit {
                    kind: set.kind,
                    name: m.as_str().to_string(),
                    unit: ui,
                    line: lines.of(m.start()),
                });
            }
        }
    }
    hits
}

fn in_base(lower_base: &[String], name: &str, cache: &mut HashMap<String, bool>) -> bool {
    if let Some(&v) = cache.get(name) {
        return v;
    }
    let lower = name.to_ascii_lowercase();
    let found = lower_base.iter().any(|t| t.contains(&lower));
    cache.insert(name.to_string(), found);
    found
}

fn looks_like_text(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 16 {
        return None;
    }
    let s = String::from_utf8_lossy(bytes);
    let bad = s
        .chars()
        .filter(|&c| c == '\u{fffd}' || (c.is_control() && !matches!(c, '\n' | '\r' | '\t')))
        .count();
    (bad * 20 <= s.chars().count()).then(|| s.into_owned())
}

fn decode_b64(s: &str) -> Option<String> {
    use base64::alphabet;
    use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
    let cfg = GeneralPurposeConfig::new()
        .with_decode_allow_trailing_bits(true)
        .with_decode_padding_mode(DecodePaddingMode::Indifferent);
    let mut t = s.trim_end_matches('=').to_string();
    if t.len() % 4 == 1 {
        t.pop();
    }
    let engine = if t.contains('-') || t.contains('_') {
        GeneralPurpose::new(&alphabet::URL_SAFE, cfg)
    } else {
        GeneralPurpose::new(&alphabet::STANDARD, cfg)
    };
    looks_like_text(&engine.decode(t).ok()?)
}

fn decode_hex(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len() / 2);
    for pair in b.chunks_exact(2) {
        let v = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
        out.push(v);
    }
    looks_like_text(&out)
}

struct Decoded {
    text: String,
    line: u32,
}

fn hex_bytes(s: &str) -> Vec<u8> {
    let digits: Vec<u8> = s.bytes().filter(u8::is_ascii_hexdigit).collect();
    digits
        .chunks_exact(2)
        .filter_map(|p| u8::from_str_radix(std::str::from_utf8(p).ok()?, 16).ok())
        .collect()
}

fn decode_unicode(s: &str) -> Option<String> {
    let mut out = String::new();
    for part in s.split("\\u").filter(|p| !p.is_empty()) {
        let digits = part.trim_matches(|c| c == '{' || c == '}');
        out.push(char::from_u32(u32::from_str_radix(digits, 16).ok()?)?);
    }
    looks_like_text(out.as_bytes())
}

fn decoded_variants(text: &str) -> Vec<Decoded> {
    static B64: OnceLock<Regex> = OnceLock::new();
    static HEX: OnceLock<Regex> = OnceLock::new();
    static LINE: OnceLock<Regex> = OnceLock::new();
    static SEPARATED: OnceLock<[Regex; 3]> = OnceLock::new();
    static UNICODE: OnceLock<Regex> = OnceLock::new();
    let b64 = B64.get_or_init(|| re(r"[A-Za-z0-9+/_-]{40,}={0,2}"));
    let hex = HEX.get_or_init(|| re(r"(?:[0-9a-fA-F]{2}){32,}"));
    let whole = LINE.get_or_init(|| re(r"^[A-Za-z0-9+/_-]{16,}={0,2}$"));
    let separated = SEPARATED.get_or_init(|| {
        [
            re(r"(?:\\x[0-9a-fA-F]{2}){16,}"),
            re(r"(?:0[xX][0-9a-fA-F]{2}[ \t]*,?[ \t]*){16,}"),
            re(r"(?-u:\b)(?:[0-9a-fA-F]{2}[ \t:,-]+){15,}[0-9a-fA-F]{2}(?-u:\b)"),
        ]
    });
    let unicode = UNICODE.get_or_init(|| re(r"(?:\\u[0-9a-fA-F]{4}|\\u\{[0-9a-fA-F]{1,6}\}){16,}"));
    let lines = Lines::new(text);
    let mut out = Vec::new();
    for m in hex.find_iter(text) {
        if let Some(d) = decode_hex(m.as_str()) {
            out.push(Decoded {
                text: d,
                line: lines.of(m.start()),
            });
        }
    }
    for r in separated {
        for m in r.find_iter(text) {
            let cleaned = m
                .as_str()
                .replace("\\x", "")
                .replace("0x", "")
                .replace("0X", "");
            if let Some(d) = looks_like_text(&hex_bytes(&cleaned)) {
                out.push(Decoded {
                    text: d,
                    line: lines.of(m.start()),
                });
            }
        }
    }
    for m in unicode.find_iter(text) {
        if let Some(d) = decode_unicode(m.as_str()) {
            out.push(Decoded {
                text: d,
                line: lines.of(m.start()),
            });
        }
    }
    for m in b64.find_iter(text) {
        if let Some(d) = decode_b64(m.as_str()) {
            out.push(Decoded {
                text: d,
                line: lines.of(m.start()),
            });
        }
    }
    let mut group = String::new();
    let mut group_start = 0u32;
    let flush = |group: &mut String, start: u32, out: &mut Vec<Decoded>| {
        if group.len() >= 40 {
            if let Some(d) = decode_b64(group) {
                out.push(Decoded {
                    text: d,
                    line: start,
                });
            }
        }
        group.clear();
    };
    for (i, line) in text.lines().enumerate() {
        let t = line
            .trim()
            .trim_matches(|c: char| matches!(c, '"' | '\'' | ',' | '+' | ';' | ' '));
        if whole.is_match(t) {
            if group.is_empty() {
                group_start = i as u32;
            }
            group.push_str(t);
        } else {
            flush(&mut group, group_start, &mut out);
        }
    }
    flush(&mut group, group_start, &mut out);
    out
}

struct OverlapUnit<'a> {
    unit: &'a Unit<'a>,
    tokens: Tokens,
    matches: Vec<(usize, u64)>,
}

fn first_run(matches: &[(usize, u64)], skip: &HashSet<u64>, exclude: bool) -> Option<(usize, u64)> {
    let kept: Vec<(usize, u64)> = matches
        .iter()
        .copied()
        .filter(|(_, h)| !(exclude && skip.contains(h)))
        .collect();
    let mut start = 0;
    while start < kept.len() {
        let mut end = start + 1;
        while end < kept.len() && kept[end].0 - kept[end - 1].0 <= W {
            end += 1;
        }
        let distinct: HashSet<u64> = kept[start..end].iter().map(|k| k.1).collect();
        if distinct.len() >= M {
            return Some(kept[start]);
        }
        start = end;
    }
    None
}

fn scan_overlap(index: &Index, public: &PublicSide, units: &[Unit], out: &mut Vec<Finding>) {
    let mut cands: Vec<OverlapUnit> = Vec::new();
    for unit in units {
        let tokens = tokenize(unit.text);
        let grams = kgrams(&tokens.hashes);
        let matches: Vec<(usize, u64)> = grams
            .iter()
            .enumerate()
            .filter(|(_, h)| index.grams.contains_key(h))
            .map(|(p, h)| (p, *h))
            .collect();
        if matches.len() < M {
            continue;
        }
        cands.push(OverlapUnit {
            unit,
            tokens,
            matches,
        });
    }
    if cands.is_empty() {
        return;
    }
    let hit_set: HashSet<u64> = cands
        .iter()
        .flat_map(|c| c.matches.iter().map(|m| m.1))
        .collect();
    let mut in_public: HashSet<u64> = HashSet::new();
    for f in &public.base_files {
        let grams = kgrams(&tokenize(&f.text).hashes);
        in_public.extend(grams.into_iter().filter(|g| hit_set.contains(g)));
    }
    for c in cands {
        let Some((pos, hash)) = first_run(&c.matches, &in_public, true) else {
            continue;
        };
        let rel = c.tokens.lines[pos];
        let path = index
            .grams
            .get(&hash)
            .map(|&i| index.paths[i as usize].clone());
        out.push(Finding {
            category: Category::Overlap,
            public_repo: public.full_name.clone(),
            location: line_loc(c.unit, rel),
            private_ref: path,
            excerpt: excerpt(c.unit.line_text(rel)),
        });
    }
}

fn scan_names(index: &Index, public: &PublicSide, units: &[Unit], out: &mut Vec<Finding>) {
    let hits = collect_names(index, units);
    if hits.is_empty() {
        return;
    }
    let lower_base: Vec<String> = public
        .base_files
        .iter()
        .map(|f| f.text.to_ascii_lowercase())
        .collect();
    let mut cache = HashMap::new();
    for h in hits {
        if in_base(&lower_base, &h.name, &mut cache) {
            continue;
        }
        let unit = &units[h.unit];
        out.push(Finding {
            category: Category::InternalName,
            public_repo: public.full_name.clone(),
            location: line_loc(unit, h.line),
            private_ref: Some(h.kind.to_string()),
            excerpt: excerpt(unit.line_text(h.line)),
        });
    }
}

fn scan_foreign(index: &Index, public: &PublicSide, out: &mut Vec<Finding>) {
    let base_paths: HashSet<&str> = public.base_files.iter().map(|f| f.path.as_str()).collect();
    let base_exts: HashSet<String> = public
        .base_files
        .iter()
        .filter_map(|f| ext_of(&f.path))
        .collect();
    for path in &public.new_files {
        let name = path.rsplit('/').next().unwrap_or(path);
        let lower = name.to_ascii_lowercase();
        let ext = ext_of(path);
        let mut reason: Option<String> = None;
        if let Some(e) = &ext {
            if index.private_exts.contains(e) && !base_exts.contains(e) {
                reason = Some(format!("extension .{e}"));
            }
        }
        if reason.is_none()
            && path.contains('/')
            && index.private_paths.contains(path)
            && !base_paths.contains(path.as_str())
        {
            reason = Some(path.clone());
        }
        if reason.is_none() {
            let app = lower.starts_with("application")
                && (lower.ends_with(".yml")
                    || lower.ends_with(".yaml")
                    || lower.ends_with(".properties"));
            let sql = ext.as_deref() == Some("sql") && !base_exts.contains("sql");
            if app
                || lower == "pom.xml"
                || lower == "mvnw"
                || lower == "mvnw.cmd"
                || path.starts_with("db/changelog/")
                || path.contains("/db/changelog/")
                || sql
            {
                reason = Some("backend-only file".to_string());
            }
        }
        if let Some(r) = reason {
            out.push(Finding {
                category: Category::ForeignFile,
                public_repo: public.full_name.clone(),
                location: Location::File {
                    path: path.clone(),
                    line: 0,
                },
                private_ref: Some(r),
                excerpt: path.clone(),
            });
        }
    }
}

pub fn scan(index: &Index, public: &PublicSide) -> Vec<Finding> {
    let mut out = Vec::new();
    scan_foreign(index, public, &mut out);
    for (path, reason) in &public.unscannable {
        out.push(Finding {
            category: Category::Unscannable,
            public_repo: public.full_name.clone(),
            location: Location::File {
                path: path.clone(),
                line: 0,
            },
            private_ref: None,
            excerpt: reason.clone(),
        });
    }
    let mut units: Vec<Unit> = public
        .added
        .iter()
        .map(|b| Unit {
            loc: Location::File {
                path: b.path.clone(),
                line: b.line,
            },
            text: &b.text,
            first_line: b.line,
            shown: None,
        })
        .collect();
    units.extend(public.texts.iter().map(|(label, text)| Unit {
        loc: Location::Text {
            label: label.clone(),
        },
        text,
        first_line: 1,
        shown: None,
    }));
    let decoded: Vec<(usize, Decoded)> = units
        .iter()
        .enumerate()
        .flat_map(|(i, u)| decoded_variants(u.text).into_iter().map(move |d| (i, d)))
        .collect();
    let extra: Vec<Unit> = decoded
        .iter()
        .map(|(i, d)| {
            let parent = &units[*i];
            Unit {
                loc: parent.loc.clone(),
                text: &d.text,
                first_line: parent.first_line + d.line,
                shown: Some(excerpt(parent.line_text(d.line))),
            }
        })
        .collect();
    units.extend(extra);
    for unit in &units {
        scan_secrets(unit, &public.full_name, &mut out);
    }
    scan_names(index, public, &units, &mut out);
    scan_overlap(index, public, &units, &mut out);
    out.sort_by(|a, b| {
        (a.category, &a.location, &a.private_ref, &a.excerpt).cmp(&(
            b.category,
            &b.location,
            &b.private_ref,
            &b.excerpt,
        ))
    });
    out.dedup_by(|b, a| a.category == b.category && a.location == b.location);
    out
}

struct TreeEntry {
    mode: String,
    oid: String,
    size: u64,
    path: String,
}

fn list_tree(repo: &Path, rev: &str) -> Result<Vec<TreeEntry>> {
    let raw = git_bytes(
        repo,
        &["ls-tree", "-r", "-z", "-l", "--end-of-options", rev],
        None,
    )?;
    let mut out = Vec::new();
    for rec in raw.split(|&b| b == 0).filter(|r| !r.is_empty()) {
        let s = String::from_utf8_lossy(rec);
        let Some((meta, path)) = s.split_once('\t') else {
            continue;
        };
        let f: Vec<&str> = meta.split_whitespace().collect();
        if f.len() != 4 || f[1] != "blob" {
            continue;
        }
        out.push(TreeEntry {
            mode: f[0].to_string(),
            oid: f[2].to_string(),
            size: f[3].parse().unwrap_or(u64::MAX),
            path: path.to_string(),
        });
    }
    Ok(out)
}

fn is_regular(mode: &str) -> bool {
    mode == "100644" || mode == "100755"
}

fn fetch_blobs(repo: &Path, oids: &[String]) -> Result<HashMap<String, Vec<u8>>> {
    let mut out = HashMap::new();
    if oids.is_empty() {
        return Ok(out);
    }
    let mut input = oids.join("\n").into_bytes();
    input.push(b'\n');
    let raw = git_bytes(repo, &["cat-file", "--batch"], Some(input))?;
    let mut pos = 0;
    while pos < raw.len() {
        let Some(nl) = raw[pos..].iter().position(|&b| b == b'\n') else {
            break;
        };
        let head = String::from_utf8_lossy(&raw[pos..pos + nl]).to_string();
        pos += nl + 1;
        let f: Vec<&str> = head.split(' ').collect();
        if f.len() != 3 {
            continue;
        }
        let size: usize = f[2]
            .parse()
            .map_err(|_| Error::internal("unexpected git cat-file output"))?;
        if pos + size > raw.len() {
            return Err(Error::internal("truncated git cat-file output"));
        }
        out.insert(f[0].to_string(), raw[pos..pos + size].to_vec());
        pos += size + 1;
    }
    Ok(out)
}

fn is_lockfile(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(
        name,
        "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" | "Cargo.lock" | "composer.lock"
    )
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BINARY_PROBE)].contains(&0)
}

fn load_files(repo: &Path, revs: &[&str]) -> Result<Vec<SourceFile>> {
    let mut wanted: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for rev in revs {
        for e in list_tree(repo, rev)? {
            if is_regular(&e.mode)
                && e.size <= MAX_FILE
                && seen.insert((e.path.clone(), e.oid.clone()))
            {
                wanted.push((e.path, e.oid));
            }
        }
    }
    let unique: Vec<String> = wanted
        .iter()
        .map(|w| w.1.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let blobs = fetch_blobs(repo, &unique)?;
    wanted.sort();
    Ok(wanted
        .into_iter()
        .filter_map(|(path, oid)| {
            let bytes = blobs.get(&oid)?;
            (!is_binary(bytes)).then(|| SourceFile {
                path,
                text: String::from_utf8_lossy(bytes).into_owned(),
            })
        })
        .collect())
}

pub fn load_private(repo: &Path, full_name: &str, revs: &[String]) -> Result<PrivateSide> {
    let refs: Vec<&str> = revs.iter().map(String::as_str).collect();
    Ok(PrivateSide {
        full_name: full_name.to_string(),
        files: load_files(repo, &refs)?,
    })
}

struct FileChange {
    path: String,
    added: bool,
    deleted: bool,
    mode: String,
    binary: bool,
    oid: String,
    blocks: Vec<(u32, String)>,
}

fn unquote(s: &str) -> (String, usize) {
    let b = s.as_bytes();
    if b.first() != Some(&b'"') {
        return (s.to_string(), s.len());
    }
    let mut out = Vec::new();
    let mut i = 1;
    while i < b.len() && b[i] != b'"' {
        if b[i] == b'\\' && i + 1 < b.len() {
            i += 1;
            match b[i] {
                b'n' => out.push(b'\n'),
                b't' => out.push(b'\t'),
                b'r' => out.push(b'\r'),
                d if d.is_ascii_digit() => {
                    let mut v = 0u32;
                    let mut n = 0;
                    while n < 3 && i < b.len() && b[i].is_ascii_digit() {
                        v = v * 8 + (b[i] - b'0') as u32;
                        i += 1;
                        n += 1;
                    }
                    out.push(v as u8);
                    continue;
                }
                other => out.push(other),
            }
        } else {
            out.push(b[i]);
        }
        i += 1;
    }
    (String::from_utf8_lossy(&out).into_owned(), i + 1)
}

fn header_path(rest: &str) -> String {
    if rest.starts_with('"') {
        let (p, _) = unquote(rest);
        return p.strip_prefix("a/").unwrap_or(&p).to_string();
    }
    let body = rest.strip_prefix("a/").unwrap_or(rest);
    let half = body.len().saturating_sub(3) / 2;
    body.get(..half).unwrap_or(body).to_string()
}

fn side_path(rest: &str, prefix: &str) -> Option<String> {
    let rest = rest.split('\t').next().unwrap_or(rest);
    if rest == "/dev/null" {
        return None;
    }
    let (p, _) = unquote(rest);
    Some(p.strip_prefix(prefix).unwrap_or(&p).to_string())
}

fn hunk_counts(header: &str) -> Option<(u32, usize, usize)> {
    let mut it = header.split_whitespace();
    it.next()?;
    let old = it.next()?.strip_prefix('-')?;
    let new = it.next()?.strip_prefix('+')?;
    let count = |s: &str| -> Option<(u32, usize)> {
        match s.split_once(',') {
            Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
            None => Some((s.parse().ok()?, 1)),
        }
    };
    let (_, oc) = count(old)?;
    let (start, nc) = count(new)?;
    Some((start, oc, nc))
}

fn parse_patch(patch: &str) -> Vec<FileChange> {
    let mut files: Vec<FileChange> = Vec::new();
    let mut lines = patch.lines().peekable();
    while let Some(line) = lines.next() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            files.push(FileChange {
                path: header_path(rest),
                added: false,
                deleted: false,
                mode: String::new(),
                binary: false,
                oid: String::new(),
                blocks: Vec::new(),
            });
            continue;
        }
        let Some(cur) = files.last_mut() else {
            continue;
        };
        if let Some(m) = line.strip_prefix("new file mode ") {
            cur.added = true;
            cur.mode = m.trim().to_string();
        } else if line.starts_with("deleted file mode ") {
            cur.deleted = true;
        } else if let Some(m) = line.strip_prefix("new mode ") {
            cur.mode = m.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("index ") {
            let mut f = rest.split_whitespace();
            if let Some(range) = f.next() {
                if let Some((_, new)) = range.split_once("..") {
                    cur.oid = new.to_string();
                }
            }
            if let Some(m) = f.next() {
                if cur.mode.is_empty() {
                    cur.mode = m.to_string();
                }
            }
        } else if line.starts_with("Binary files ") || line.starts_with("GIT binary patch") {
            cur.binary = true;
        } else if let Some(rest) = line.strip_prefix("--- ") {
            if let Some(p) = side_path(rest, "a/") {
                cur.path = p;
            }
        } else if let Some(rest) = line.strip_prefix("+++ ") {
            if let Some(p) = side_path(rest, "b/") {
                cur.path = p;
            }
        } else if line.starts_with("@@ ") {
            let Some((start, mut old, mut new)) = hunk_counts(line) else {
                continue;
            };
            let mut text: Vec<&str> = Vec::new();
            while old > 0 || new > 0 {
                match lines.peek() {
                    Some(l) if l.starts_with('-') && old > 0 => old -= 1,
                    Some(l) if l.starts_with('+') && new > 0 => {
                        new -= 1;
                        text.push(&l[1..]);
                    }
                    Some(l) if l.starts_with('\\') => {}
                    _ => break,
                }
                lines.next();
            }
            if !text.is_empty() && !cur.deleted {
                cur.blocks.push((start.max(1), text.join("\n")));
            }
        }
    }
    files
}

fn sizes(repo: &Path, oids: &[String]) -> Result<HashMap<String, u64>> {
    let mut out = HashMap::new();
    if oids.is_empty() {
        return Ok(out);
    }
    let mut input = oids.join("\n").into_bytes();
    input.push(b'\n');
    let raw = git_bytes(repo, &["cat-file", "--batch-check"], Some(input))?;
    for line in String::from_utf8_lossy(&raw).lines() {
        let f: Vec<&str> = line.split(' ').collect();
        if f.len() == 3 {
            if let Ok(n) = f[2].parse() {
                out.insert(f[0].to_string(), n);
            }
        }
    }
    Ok(out)
}

fn is_sha(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

struct CommitInfo {
    parent: Option<String>,
    author: String,
    committer: String,
    message: String,
}

fn identity(rest: &str) -> String {
    let mut parts = rest.rsplitn(3, ' ');
    parts.next();
    parts.next();
    parts.next().unwrap_or(rest).to_string()
}

fn parse_commit(raw: &[u8]) -> Result<CommitInfo> {
    let text = String::from_utf8_lossy(raw);
    let (head, message) = text.split_once("\n\n").unwrap_or((&text, ""));
    let mut info = CommitInfo {
        parent: None,
        author: String::new(),
        committer: String::new(),
        message: message.trim_end().to_string(),
    };
    for line in head.lines().filter(|l| !l.starts_with(' ')) {
        if let Some(p) = line.strip_prefix("parent ") {
            if !is_sha(p) {
                return Err(Error::internal("unexpected git commit header"));
            }
            info.parent.get_or_insert_with(|| p.to_string());
        } else if let Some(a) = line.strip_prefix("author ") {
            info.author = identity(a);
        } else if let Some(c) = line.strip_prefix("committer ") {
            info.committer = identity(c);
        }
    }
    Ok(info)
}

fn patch_args<'a>(revs: &[&'a str], root: bool) -> Vec<&'a str> {
    let mut args = vec![
        "diff-tree",
        "-p",
        "-r",
        "--no-renames",
        "--no-textconv",
        "--no-ext-diff",
        "--no-color",
        "--no-commit-id",
        "--full-index",
        "-U0",
    ];
    if root {
        args.push("--root");
    }
    args.push("--end-of-options");
    args.extend(revs);
    args
}

pub fn load_public(
    repo: &Path,
    full_name: &str,
    base: &str,
    head: &str,
    texts: Vec<(String, String)>,
) -> Result<PublicSide> {
    let base_files = load_files(repo, &[base])?;
    let base_lockfiles: HashSet<String> = list_tree(repo, base)?
        .into_iter()
        .filter(|e| is_regular(&e.mode) && is_lockfile(&e.path))
        .map(|e| e.path)
        .collect();
    let range = format!("{base}..{head}");
    let listed = git_bytes(
        repo,
        &["rev-list", "--reverse", "--end-of-options", &range],
        None,
    )?;
    let mut all_texts = Vec::new();
    let mut changes: Vec<FileChange> = Vec::new();
    for sha in String::from_utf8_lossy(&listed).lines() {
        if !is_sha(sha) {
            return Err(Error::internal("unexpected git rev-list output"));
        }
        let raw = git_bytes(repo, &["cat-file", "commit", "--end-of-options", sha], None)?;
        let info = parse_commit(&raw)?;
        let short = &sha[..7];
        all_texts.push((format!("commit {short} author"), info.author));
        all_texts.push((format!("commit {short} committer"), info.committer));
        if !info.message.trim().is_empty() {
            all_texts.push((format!("commit {short} message"), info.message));
        }
        let patch = match &info.parent {
            Some(parent) => git_bytes(repo, &patch_args(&[parent, sha], false), None)?,
            None => git_bytes(repo, &patch_args(&[sha], true), None)?,
        };
        changes.extend(parse_patch(&String::from_utf8_lossy(&patch)));
    }
    let net = git_bytes(repo, &patch_args(&[base, head], false), None)?;
    changes.extend(parse_patch(&String::from_utf8_lossy(&net)));
    all_texts.extend(texts);

    let to_size: Vec<String> = changes
        .iter()
        .filter(|c| !c.deleted && !c.binary && is_regular(&c.mode) && !c.oid.is_empty())
        .map(|c| c.oid.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let size_of = sizes(repo, &to_size)?;

    let mut new_files = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut seen: HashSet<(String, u32, String)> = HashSet::new();
    let mut added = Vec::new();
    let mut unscannable: BTreeSet<(String, String)> = BTreeSet::new();
    for c in changes {
        if c.added {
            new_files.insert(c.path.clone());
        }
        let is_modules = c.path == ".gitmodules" || c.path.ends_with("/.gitmodules");
        if is_modules {
            unscannable.insert((c.path.clone(), ".gitmodules changed".to_string()));
        }
        if c.deleted {
            continue;
        }
        paths.insert(c.path.clone());
        let exempt = is_lockfile(&c.path) && base_lockfiles.contains(&c.path);
        let reason = if c.mode == "160000" {
            Some("gitlink (submodule)")
        } else if c.mode == "120000" {
            Some("symlink")
        } else if c.binary {
            Some("binary file")
        } else if !exempt && size_of.get(&c.oid).is_some_and(|&n| n > MAX_FILE) {
            Some("larger than 512 KiB")
        } else {
            None
        };
        if let Some(r) = reason {
            unscannable.insert((c.path.clone(), r.to_string()));
            continue;
        }
        for (line, text) in c.blocks {
            if seen.insert((c.path.clone(), line, text.clone())) {
                added.push(AddedBlock {
                    path: c.path.clone(),
                    line,
                    text,
                });
            }
        }
    }
    all_texts.extend(paths.into_iter().map(|p| ("path".to_string(), p)));
    Ok(PublicSide {
        full_name: full_name.to_string(),
        base_files,
        new_files: new_files.into_iter().collect(),
        added,
        texts: all_texts,
        unscannable: unscannable.into_iter().collect(),
    })
}
