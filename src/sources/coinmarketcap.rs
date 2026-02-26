use super::PriceSource;
use crate::config::UnitConfig;
use crate::types::TokenData;
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;

pub struct CoinMarketCap {
    client: reqwest::Client,
    api_key: String,
}

impl CoinMarketCap {
    pub fn new(client: reqwest::Client, api_key: String) -> Self {
        Self { client, api_key }
    }

    fn platform_slug(chain: &str) -> &str {
        match chain {
            "ethereum" => "ethereum",
            "sepolia" => "ethereum",
            _ => chain,
        }
    }
}

#[async_trait]
impl PriceSource for CoinMarketCap {
    fn name(&self) -> &str {
        "coinmarketcap"
    }

    async fn fetch(&self, unit: &UnitConfig) -> Result<TokenData> {
        let url = "https://pro-api.coinmarketcap.com/v2/cryptocurrency/quotes/latest";
        let resp = self
            .client
            .get(url)
            .query(&[
                ("address", unit.contract.as_str()),
                ("skip_invalid", "true"),
            ])
            .header("Accept", "application/json")
            .header("X-CMC_PRO_API_KEY", &self.api_key)
            .send()
            .await
            .context("CoinMarketCap request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("CoinMarketCap HTTP {}: {}", status, body);
        }

        let body: Value = resp.json().await.context("CoinMarketCap parse failed")?;
        let expected_platform = Self::platform_slug(&unit.chain);
        let token_data = extract_best_token(&body["data"], &unit.contract, expected_platform)
            .context("CoinMarketCap: no matching token for contract")?;

        let usd_quote = token_data
            .get("quote")
            .and_then(|q| q.get("USD").or_else(|| q.get("usd")))
            .context("CoinMarketCap: missing USD quote")?;

        let price_usd = usd_quote
            .get("price")
            .and_then(Value::as_f64)
            .context("CoinMarketCap: missing USD price")?;

        let market_cap = usd_quote.get("market_cap").and_then(Value::as_f64);
        let volume_24h = usd_quote.get("volume_24h").and_then(Value::as_f64);
        let price_change_24h = usd_quote
            .get("percent_change_24h")
            .and_then(Value::as_f64);

        Ok(TokenData {
            name: unit.name.clone(),
            chain: unit.chain.clone(),
            contract: unit.contract.clone(),
            price_usd,
            market_cap,
            volume_24h,
            liquidity: None,
            price_change_24h,
            source: self.name().to_string(),
            timestamp: Utc::now(),
        })
    }
}

fn extract_best_token<'a>(
    data: &'a Value,
    contract: &str,
    expected_platform: &str,
) -> Option<&'a Value> {
    let contract = contract.to_ascii_lowercase();
    let mut fallback: Option<&Value> = None;

    for token in flatten_token_entries(data) {
        if fallback.is_none() {
            fallback = Some(token);
        }

        let matches_contract = token_contract_address(token)
            .map(|addr| addr.eq_ignore_ascii_case(&contract))
            .unwrap_or(false);

        if !matches_contract {
            continue;
        }

        let platform_ok = token_platform_slug(token)
            .map(|slug| slug.eq_ignore_ascii_case(expected_platform))
            .unwrap_or(true);

        if platform_ok {
            return Some(token);
        }
    }

    fallback
}

fn flatten_token_entries(data: &Value) -> Vec<&Value> {
    match data {
        Value::Array(arr) => arr.iter().collect(),
        Value::Object(map) => map
            .values()
            .flat_map(|v| match v {
                Value::Array(arr) => arr.iter().collect::<Vec<_>>(),
                Value::Object(_) => vec![v],
                _ => Vec::new(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn token_contract_address(token: &Value) -> Option<String> {
    token
        .get("contract_address")
        .and_then(Value::as_str)
        .or_else(|| {
            token
                .get("platform")
                .and_then(|p| p.get("token_address").or_else(|| p.get("contract_address")))
                .and_then(Value::as_str)
        })
        .map(|s| s.to_ascii_lowercase())
}

fn token_platform_slug(token: &Value) -> Option<&str> {
    token
        .get("platform")
        .and_then(|p| p.get("slug"))
        .and_then(Value::as_str)
}
