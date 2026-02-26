use super::PriceSource;
use crate::config::UnitConfig;
use crate::types::TokenData;
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;

pub struct GeckoTerminal {
    client: reqwest::Client,
}

impl GeckoTerminal {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    fn network_id(chain: &str) -> &str {
        match chain {
            "ethereum" => "eth",
            "sepolia" => "eth",
            _ => chain,
        }
    }
}

#[async_trait]
impl PriceSource for GeckoTerminal {
    fn name(&self) -> &str {
        "geckoterminal"
    }

    async fn fetch(&self, unit: &UnitConfig) -> Result<TokenData> {
        let network = Self::network_id(&unit.chain);
        let url = format!(
            "https://api.geckoterminal.com/api/v2/networks/{}/tokens/{}",
            network, unit.contract
        );

        let resp = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .context("GeckoTerminal request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("GeckoTerminal HTTP {}: {}", status, body);
        }

        let body: serde_json::Value = resp.json().await.context("GeckoTerminal parse failed")?;
        let attrs = &body["data"]["attributes"];

        let price_usd = parse_string_f64(attrs, "price_usd")
            .context("GeckoTerminal: missing price_usd")?;

        let volume_24h = attrs["volume_usd"].get("h24").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok());
        let liquidity = parse_optional_string_f64(attrs, "total_reserve_in_usd");
        let market_cap = parse_optional_string_f64(attrs, "market_cap_usd");

        Ok(TokenData {
            name: unit.name.clone(),
            chain: unit.chain.clone(),
            contract: unit.contract.clone(),
            price_usd,
            market_cap,
            volume_24h,
            liquidity,
            price_change_24h: None,
            source: self.name().to_string(),
            timestamp: Utc::now(),
        })
    }
}

fn parse_string_f64(obj: &serde_json::Value, key: &str) -> Option<f64> {
    obj.get(key)
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
}

fn parse_optional_string_f64(obj: &serde_json::Value, key: &str) -> Option<f64> {
    obj.get(key)
        .and_then(|v| {
            if v.is_null() {
                None
            } else {
                v.as_str().and_then(|s| s.parse::<f64>().ok())
            }
        })
}
