use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::error::Result;
use crate::link::{attribute_at, LinkEvent, RunWindow};
use crate::model::{Tokens, Tool};
use crate::pricing::PriceBook;
use crate::store::Store;

pub const BASIS: &str = "estimate (list price)";

/// How a tool's usage is paid for. The usage logs do not say, so the caller must supply it
/// (for Claude, `ClaudeProbe::rate_limits_available` tells subscription from API key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Billing {
    Subscription,
    ApiKey,
    Unknown,
}

/// `subscription_tokens` counts events billed to a subscription or of unknown billing;
/// `api_usd` is the list-price cost of events billed to an API key or of unknown billing, and
/// `None` when no event qualifies. With `billing == Unknown` both cover the same unknown events.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CostSummary {
    pub subscription_tokens: u64,
    pub api_usd: Option<f64>,
    pub unpriced_events: u64,
    pub events: u64,
    pub ambiguous_events: u64,
    pub billing: Billing,
    pub basis: &'static str,
}

pub fn cost_between(
    store: &Store,
    book: &PriceBook,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    run_windows: &[RunWindow],
    billing: impl Fn(Tool) -> Billing,
) -> Result<CostSummary> {
    let events = store.link_events_with_tokens(from, to)?;
    Ok(summarize(&events, book, run_windows, to, billing))
}

pub fn summarize(
    events: &[(LinkEvent, Tokens)],
    book: &PriceBook,
    run_windows: &[RunWindow],
    now: DateTime<Utc>,
    billing: impl Fn(Tool) -> Billing,
) -> CostSummary {
    let plain: Vec<LinkEvent> = events.iter().map(|(e, _)| e.clone()).collect();
    let report = attribute_at(&plain, run_windows, now);
    let mut summary = CostSummary {
        subscription_tokens: 0,
        api_usd: None,
        unpriced_events: 0,
        events: 0,
        ambiguous_events: report.ambiguous.len() as u64,
        billing: Billing::Unknown,
        basis: BASIS,
    };
    let mut kinds: Vec<Billing> = Vec::new();
    for link in &report.links {
        let (ev, tokens) = &events[link.event_index];
        let kind = billing(ev.tool);
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
        summary.events += 1;
        if kind != Billing::ApiKey {
            summary.subscription_tokens += ev.tokens;
        }
        if kind != Billing::Subscription {
            match ev.cost_usd.or_else(|| book.cost(&ev.model, tokens, None)) {
                Some(c) => *summary.api_usd.get_or_insert(0.0) += c,
                None => {
                    summary.unpriced_events += 1;
                    summary.api_usd.get_or_insert(0.0);
                }
            }
        }
    }
    if let [only] = kinds[..] {
        summary.billing = only;
    }
    summary
}
