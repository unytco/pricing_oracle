pub mod coinapi;
pub mod twelve_data;

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;

#[async_trait]
pub trait ForexSource: Send + Sync {
    fn name(&self) -> &str;
    async fn fetch_rates(&self, symbols: &[String]) -> Result<HashMap<String, f64>>;
}

pub struct ForexSourceRegistry {
    sources: Vec<Box<dyn ForexSource>>,
}

impl ForexSourceRegistry {
    pub fn new(
        client: reqwest::Client,
        twelve_data_api_key: Option<String>,
        coinapi_api_key: Option<String>,
        use_twelve_data: bool,
        use_coinapi: bool,
    ) -> Self {
        let mut sources: Vec<Box<dyn ForexSource>> = Vec::new();

        if use_twelve_data {
            if let Some(key) = twelve_data_api_key {
                sources.push(Box::new(twelve_data::TwelveData::new(client.clone(), key)));
            } else {
                tracing::warn!("TWELVE_DATA_API_KEY not set; Twelve Data forex source disabled");
            }
        }

        if use_coinapi {
            if let Some(key) = coinapi_api_key {
                sources.push(Box::new(coinapi::CoinApi::new(client, key)));
            } else {
                tracing::warn!("COINAPI_API_KEY not set; CoinAPI forex source disabled");
            }
        }

        Self { sources }
    }

    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    pub async fn fetch_all(
        &self,
        symbols: &[String],
    ) -> Vec<(String, Result<HashMap<String, f64>>)> {
        let mut results = Vec::new();
        for source in &self.sources {
            let name = source.name().to_string();
            let result = source.fetch_rates(symbols).await;
            results.push((name, result));
        }
        results
    }
}
