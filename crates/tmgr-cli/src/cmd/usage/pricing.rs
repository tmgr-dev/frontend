use super::{fail, Usage};
use crate::cli_usage::PricingCommand;
use crate::ctx::Ctx;
use crate::out::{table, Output};
use chrono::{SecondsFormat, Utc};
use serde_json::json;
use tmgr_core::error::{Error, Result};
use tmgr_core::text::sanitize_line;
use tmgr_usage::PriceBook;

const PRICE_URL: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";
const MAX_DOWNLOAD: u64 = 32 * 1024 * 1024;

pub fn run(_ctx: &Ctx, cmd: Option<&PricingCommand>) -> Result<Output> {
    match cmd.unwrap_or(&PricingCommand::Show { model: None }) {
        PricingCommand::Show { model } => show(model.as_deref()),
        PricingCommand::Update { offline, url } => update(*offline, url.as_deref()),
    }
}

fn per_million(entry: &tmgr_usage::pricing::PriceEntry, key: &str) -> Option<f64> {
    entry.costs.get(key).map(|v| v * 1_000_000.0)
}

fn show(filter: Option<&str>) -> Result<Output> {
    let u = Usage::open()?;
    let saved = u.store.load_prices().map_err(fail)?;
    let (source, fetched) = match &saved {
        Some((_, at)) => (
            "downloaded",
            Some(at.to_rfc3339_opts(SecondsFormat::Secs, true)),
        ),
        None => ("bundled", None),
    };
    let needle = filter.map(str::to_lowercase);
    let models: Vec<_> = u
        .book
        .entries()
        .iter()
        .filter(|(k, _)| needle.as_ref().is_some_and(|n| k.contains(n.as_str())))
        .map(|(k, e)| {
            json!({
                "model": k,
                "input_per_mtok_usd": per_million(e, "input_cost_per_token"),
                "output_per_mtok_usd": per_million(e, "output_cost_per_token"),
                "cache_read_per_mtok_usd": per_million(e, "cache_read_input_token_cost"),
                "cache_write_per_mtok_usd": per_million(e, "cache_creation_input_token_cost"),
            })
        })
        .collect();
    let mut human = format!(
        "Prices: {} snapshot{}, {} models.",
        source,
        fetched
            .as_deref()
            .map_or(String::new(), |t| format!(" from {t}")),
        u.book.len()
    );
    if filter.is_some() {
        if models.is_empty() {
            human.push_str("\nNo model matches.");
        } else {
            let mut lines = vec![["Model", "Input", "Output", "Cache read", "Cache write"]
                .map(String::from)
                .to_vec()];
            let cell = |m: &serde_json::Value, k: &str| {
                m[k].as_f64()
                    .map_or("-".to_string(), |v| format!("${v:.2}"))
            };
            for m in &models {
                lines.push(vec![
                    sanitize_line(m["model"].as_str().unwrap_or("")),
                    cell(m, "input_per_mtok_usd"),
                    cell(m, "output_per_mtok_usd"),
                    cell(m, "cache_read_per_mtok_usd"),
                    cell(m, "cache_write_per_mtok_usd"),
                ]);
            }
            human.push_str("\nUSD per million tokens (API-equivalent list prices)\n");
            human.push_str(&table(&lines));
        }
    } else {
        human.push_str("\nShow prices: tmgr usage pricing show --model opus");
    }
    Ok(Output::new(
        json!({
            "source": source,
            "fetched_at": fetched,
            "models_total": u.book.len(),
            "models": models,
        }),
        human,
    ))
}

fn update(offline: bool, url: Option<&str>) -> Result<Output> {
    let mut u = Usage::open()?;
    let (book, source) = if offline {
        (PriceBook::bundled(), "bundled")
    } else {
        let bytes = tmgr_core::update::download_public(
            url.unwrap_or(PRICE_URL),
            MAX_DOWNLOAD,
            &Ctx::user_agent(),
        )?;
        let book = PriceBook::from_litellm_json(&bytes)
            .map_err(|_| Error::internal("the downloaded price list is not valid"))?;
        if book.is_empty() {
            return Err(Error::internal("the downloaded price list has no models"));
        }
        (book, "litellm")
    };
    let stored = if offline {
        PriceBook::empty()
    } else {
        book.clone()
    };
    u.store
        .save_prices(&stored, source, Utc::now())
        .map_err(fail)?;
    let repriced = u.store.reprice(&book).map_err(fail)?;
    Ok(Output::new(
        json!({"source": source, "models_total": book.len(), "events_repriced": repriced}),
        format!(
            "Prices updated from the {} list: {} models, {} stored events re-priced.",
            if offline { "bundled" } else { "LiteLLM" },
            book.len(),
            repriced
        ),
    ))
}
