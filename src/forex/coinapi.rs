use super::ForexSource;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use tracing::warn;

pub struct CoinApi {
    client: reqwest::Client,
    api_key: String,
}

impl CoinApi {
    pub fn new(client: reqwest::Client, api_key: String) -> Self {
        Self { client, api_key }
    }
}

#[async_trait]
impl ForexSource for CoinApi {
    fn name(&self) -> &str {
        "coinapi"
    }

    async fn fetch_rates(&self, symbols: &[String]) -> Result<HashMap<String, f64>> {
        let mut rates = HashMap::new();

        for symbol in symbols {
            if symbol == "USD" {
                rates.insert(symbol.clone(), 1.0);
                continue;
            }

            let url = format!("https://rest.coinapi.io/v1/exchangerate/USD/{}", symbol);
            let resp = self
                .client
                .get(&url)
                .header("X-CoinAPI-Key", &self.api_key)
                .send()
                .await
                .with_context(|| format!("CoinAPI request failed for USD/{}", symbol))?;

            let status = resp.status();
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                if is_quota_error(&body) {
                    warn!(
                        "CoinAPI quota reached at USD/{}; returning {} partial rate(s)",
                        symbol,
                        rates.len()
                    );
                    break;
                }
                warn!(
                    "CoinAPI USD/{} failed (HTTP {}): {} — ignored",
                    symbol, status, body
                );
                continue;
            }

            let body: serde_json::Value = resp
                .json()
                .await
                .with_context(|| format!("CoinAPI parse failed for USD/{}", symbol))?;
            let Some(rate) = body.get("rate").and_then(|v| v.as_f64()) else {
                warn!("CoinAPI USD/{} failed (missing rate) — ignored", symbol);
                continue;
            };
            rates.insert(symbol.clone(), rate);
        }

        if rates.is_empty() {
            anyhow::bail!("CoinAPI did not return any forex rates");
        }

        Ok(rates)
    }
}

fn is_quota_error(message: &str) -> bool {
    let msg = message.to_lowercase();
    msg.contains("quota exceeded")
        || msg.contains("insufficient usage credits")
        || msg.contains("subscription")
        || msg.contains("forbidden")
}
