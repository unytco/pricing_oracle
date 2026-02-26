pub mod coingecko;
pub mod coinmarketcap;
pub mod geckoterminal;

use crate::config::UnitConfig;
use crate::types::TokenData;
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait PriceSource: Send + Sync {
    fn name(&self) -> &str;
    async fn fetch(&self, unit: &UnitConfig) -> Result<TokenData>;
}

pub struct SourceRegistry {
    sources: Vec<Box<dyn PriceSource>>,
}

impl SourceRegistry {
    pub fn new(
        client: reqwest::Client,
        coingecko_api_key: Option<String>,
        coinmarketcap_api_key: Option<String>,
    ) -> Self {
        let mut sources: Vec<Box<dyn PriceSource>> =
            vec![Box::new(geckoterminal::GeckoTerminal::new(client.clone()))];

        if let Some(key) = coingecko_api_key {
            sources.push(Box::new(coingecko::CoinGecko::new(client.clone(), key)));
        } else {
            tracing::warn!("COINGECKO_API_KEY not set; CoinGecko source disabled");
        }

        if let Some(key) = coinmarketcap_api_key {
            sources.push(Box::new(coinmarketcap::CoinMarketCap::new(client, key)));
        } else {
            tracing::warn!("COINMARKETCAP_API_KEY not set; CoinMarketCap source disabled");
        }

        Self { sources }
    }

    pub async fn fetch_all(&self, unit: &UnitConfig) -> Vec<(String, Result<TokenData>)> {
        let mut results = Vec::new();
        for source in &self.sources {
            let name = source.name().to_string();
            let result = source.fetch(unit).await;
            results.push((name, result));
        }
        results
    }

    pub fn source_count(&self) -> usize {
        self.sources.len()
    }
}
