use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use crate::error::{Result, UsageError};
use crate::model::{CostBasis, Tokens, UsageEvent};

const SNAPSHOT: &str = include_str!("../pricing/litellm_snapshot.json");
const SUPPLEMENT: &str = include_str!("../pricing/supplement.json");
const COST_PREFIXES: [&str; 4] = [
    "input_cost_per_token",
    "output_cost_per_token",
    "cache_read_input_token_cost",
    "cache_creation_input_token_cost",
];
const CACHE_1H: &str = "cache_creation_input_token_cost_above_1hr";
const FAST_MULTIPLIER: &str = "fast_multiplier";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PriceEntry {
    pub costs: BTreeMap<String, f64>,
}

impl PriceEntry {
    fn long_threshold_k(&self) -> Option<u64> {
        let prefix = "input_cost_per_token_above_";
        self.costs.keys().find_map(|k| {
            k.strip_prefix(prefix)?
                .strip_suffix("k_tokens")?
                .parse::<u64>()
                .ok()
        })
    }

    fn rate(&self, base: &str, long_k: Option<u64>, tier: Option<&str>) -> Option<f64> {
        let mut keys = Vec::with_capacity(4);
        if let (Some(n), Some(t)) = (long_k, tier) {
            keys.push(format!("{base}_above_{n}k_tokens_{t}"));
        }
        if let Some(n) = long_k {
            keys.push(format!("{base}_above_{n}k_tokens"));
        }
        if let Some(t) = tier {
            keys.push(format!("{base}_{t}"));
        }
        keys.push(base.to_string());
        keys.iter().find_map(|k| self.costs.get(k).copied())
    }
}

#[derive(Debug, Clone, Default)]
pub struct PriceBook {
    entries: BTreeMap<String, PriceEntry>,
    aliases: HashMap<String, String>,
}

#[derive(Deserialize, Default)]
struct Supplement {
    #[serde(default)]
    aliases: HashMap<String, String>,
    #[serde(default)]
    entries: BTreeMap<String, PriceEntry>,
}

fn wanted_model(key: &str) -> bool {
    if key.contains(['/', ':']) {
        return false;
    }
    let mut chars = key.chars();
    key.starts_with("claude-")
        || key.starts_with("gpt-")
        || key.starts_with("codex")
        || (chars.next() == Some('o') && chars.next().is_some_and(|c| c.is_ascii_digit()))
}

fn strip_date(name: &str) -> Option<&str> {
    let b = name.as_bytes();
    let n = b.len();
    if n > 9 && b[n - 9] == b'-' && b[n - 8..].iter().all(u8::is_ascii_digit) {
        return Some(&name[..n - 9]);
    }
    if n > 11
        && b[n - 11] == b'-'
        && b[n - 10..].iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
    {
        return Some(&name[..n - 11]);
    }
    None
}

fn candidates(model: &str) -> Vec<String> {
    let mut m = model.trim().to_lowercase();
    if let Some(i) = m.find('[') {
        m.truncate(i);
    }
    if let Some(i) = m.rfind('/') {
        m = m[i + 1..].to_string();
    }
    if let Some(rest) = m.strip_prefix("anthropic.") {
        m = rest.to_string();
    }
    m = m.replace('@', "-");
    if let Some(i) = m.rfind("-v") {
        let tail = &m[i + 2..];
        if tail.contains(':') && tail.chars().all(|c| c.is_ascii_digit() || c == ':') {
            m.truncate(i);
        }
    }
    let mut out = vec![m.clone()];
    if let Some(s) = m.strip_suffix("-latest") {
        out.push(s.to_string());
    }
    if let Some(s) = strip_date(&m) {
        out.push(s.to_string());
    }
    out
}

impl PriceBook {
    pub fn empty() -> PriceBook {
        PriceBook::default()
    }

    pub fn with_entries(entries: BTreeMap<String, PriceEntry>) -> PriceBook {
        let mut book = PriceBook {
            entries,
            aliases: HashMap::new(),
        };
        book.apply_supplement(SUPPLEMENT);
        book
    }

    pub fn bundled() -> PriceBook {
        let entries = filter_litellm(SNAPSHOT.as_bytes()).unwrap_or_default();
        PriceBook::with_entries(entries)
    }

    pub fn from_litellm_json(bytes: &[u8]) -> Result<PriceBook> {
        Ok(PriceBook::with_entries(filter_litellm(bytes)?))
    }

    pub fn set_alias(&mut self, from: &str, to: &str) {
        self.aliases.insert(from.to_lowercase(), to.to_lowercase());
    }

    pub fn insert(&mut self, model: &str, entry: PriceEntry) {
        self.entries.insert(model.to_lowercase(), entry);
    }

    fn apply_supplement(&mut self, text: &str) {
        let Ok(sup) = serde_json::from_str::<Supplement>(text) else {
            return;
        };
        for (k, v) in sup.aliases {
            self.set_alias(&k, &v);
        }
        for (k, v) in sup.entries {
            self.entries.entry(k.to_lowercase()).or_insert(v);
        }
    }

    pub fn entries(&self) -> &BTreeMap<String, PriceEntry> {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn lookup(&self, model: &str) -> Option<&PriceEntry> {
        for c in candidates(model) {
            if let Some(e) = self.entries.get(&c) {
                return Some(e);
            }
            if let Some(e) = self.aliases.get(&c).and_then(|a| self.entries.get(a)) {
                return Some(e);
            }
        }
        None
    }

    pub fn cost(&self, model: &str, t: &Tokens, tier: Option<&str>) -> Option<f64> {
        self.cost_at(model, t, tier, None)
    }

    pub fn cost_at(
        &self,
        model: &str,
        t: &Tokens,
        tier: Option<&str>,
        speed: Option<&str>,
    ) -> Option<f64> {
        let e = self.lookup(model)?;
        let tier = tier
            .map(str::to_lowercase)
            .filter(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_alphanumeric()));
        let tier = tier.as_deref();
        let long = e.long_threshold_k().filter(|k| t.prompt_total() > k * 1000);
        let input = e.rate("input_cost_per_token", long, tier)?;
        let output = e.rate("output_cost_per_token", long, tier)?;
        let cache_read = e
            .rate("cache_read_input_token_cost", long, tier)
            .unwrap_or(input);
        let cw5 = e
            .rate("cache_creation_input_token_cost", long, tier)
            .unwrap_or(input);
        let cw1 = e.rate(CACHE_1H, long, tier).unwrap_or(cw5);
        let base = t.input as f64 * input
            + t.output as f64 * output
            + t.cache_read as f64 * cache_read
            + t.cache_write_5m as f64 * cw5
            + t.cache_write_1h as f64 * cw1;
        let fast = (speed == Some("fast"))
            .then(|| e.costs.get(FAST_MULTIPLIER).copied())
            .flatten()
            .unwrap_or(1.0);
        Some(base * fast)
    }

    /// `[input, output, cache_read, cache_write_5m, cache_write_1h]` USD per 1M tokens, or None without a full base price.
    pub fn rates_per_million(&self, model: &str) -> Option<[f64; 5]> {
        let e = self.lookup(model)?;
        let get = |k: &str| e.costs.get(k).copied();
        let input = get("input_cost_per_token")?;
        let output = get("output_cost_per_token")?;
        let cache_read = get("cache_read_input_token_cost")?;
        let cw5 = get("cache_creation_input_token_cost")?;
        let cw1 = get(CACHE_1H).unwrap_or(cw5);
        let scaled = [input, output, cache_read, cw5, cw1].map(|x| (x * 1e12).round() / 1e6);
        scaled
            .iter()
            .all(|x| x.is_finite() && (0.0..=100_000.0).contains(x))
            .then_some(scaled)
    }

    pub fn price(&self, ev: &UsageEvent) -> (Option<f64>, CostBasis) {
        match self.cost_at(
            &ev.model,
            &ev.tokens,
            ev.service_tier.as_deref(),
            ev.speed.as_deref(),
        ) {
            Some(c) => (Some(c), CostBasis::Computed),
            None => (None, CostBasis::Unpriced),
        }
    }
}

fn filter_litellm(bytes: &[u8]) -> Result<BTreeMap<String, PriceEntry>> {
    let raw: HashMap<String, serde_json::Value> = serde_json::from_slice(bytes)?;
    let mut out = BTreeMap::new();
    for (key, value) in raw {
        let key = key.to_lowercase();
        if !wanted_model(&key) {
            continue;
        }
        let Some(obj) = value.as_object() else {
            continue;
        };
        if let Some(mode) = obj.get("mode").and_then(|m| m.as_str()) {
            if !matches!(mode, "chat" | "responses" | "completion") {
                continue;
            }
        }
        let mut costs: BTreeMap<String, f64> = obj
            .iter()
            .filter(|(k, _)| COST_PREFIXES.iter().any(|p| k.starts_with(p)))
            .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f)))
            .collect();
        let fast = obj
            .get("provider_specific_entry")
            .and_then(|p| p.get("fast"))
            .and_then(|f| f.as_f64())
            .filter(|f| f.is_finite() && *f >= 1.0);
        if let Some(f) = fast {
            costs.insert(FAST_MULTIPLIER.to_string(), f);
        }
        if costs.contains_key("input_cost_per_token") && costs.contains_key("output_cost_per_token")
        {
            out.insert(key, PriceEntry { costs });
        }
    }
    if out.is_empty() {
        return Err(UsageError::Invalid("no usable prices in input".into()));
    }
    Ok(out)
}
