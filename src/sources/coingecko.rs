use super::PriceSource;
use crate::config::UnitConfig;
use crate::types::TokenData;
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;

pub struct CoinGecko {
    client: reqwest::Client,
    api_key: String,
}

impl CoinGecko {
    pub fn new(client: reqwest::Client, api_key: String) -> Self {
        Self { client, api_key }
    }

    fn platform_id(chain: &str) -> &str {
        match chain {
            "ethereum" => "ethereum",
            "sepolia" => "ethereum",
            _ => chain,
        }
    }
}

#[async_trait]
impl PriceSource for CoinGecko {
    fn name(&self) -> &str {
        "coingecko"
    }

    async fn fetch(&self, unit: &UnitConfig) -> Result<TokenData> {
        let platform = Self::platform_id(&unit.chain);
        let url = format!(
            "https://api.coingecko.com/api/v3/simple/token_price/{}",
            platform
        );

        let resp = self
            .client
            .get(&url)
            .query(&[
                ("contract_addresses", unit.contract.as_str()),
                ("vs_currencies", "usd"),
                ("include_market_cap", "true"),
                ("include_24hr_vol", "true"),
                ("include_24hr_change", "true"),
            ])
            .header("x-cg-demo-api-key", &self.api_key)
            .send()
            .await
            .context("CoinGecko request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("CoinGecko HTTP {}: {}", status, body);
        }

        let body: serde_json::Value = resp.json().await.context("CoinGecko parse failed")?;

        let addr_lower = unit.contract.to_lowercase();
        let token_data = body
            .get(&addr_lower)
            .with_context(|| format!("CoinGecko: no data for contract {}", addr_lower))?;

        let price_usd = token_data["usd"]
            .as_f64()
            .context("CoinGecko: missing usd price")?;

        let market_cap = token_data["usd_market_cap"].as_f64();
        let volume_24h = token_data["usd_24h_vol"].as_f64();
        let price_change_24h = token_data["usd_24h_change"].as_f64();

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
