use super::ForexSource;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use tracing::warn;

pub struct TwelveData {
    client: reqwest::Client,
    api_key: String,
}

impl TwelveData {
    pub fn new(client: reqwest::Client, api_key: String) -> Self {
        Self { client, api_key }
    }
}

#[async_trait]
impl ForexSource for TwelveData {
    fn name(&self) -> &str {
        "twelve_data"
    }

    async fn fetch_rates(&self, symbols: &[String]) -> Result<HashMap<String, f64>> {
        let mut rates = HashMap::new();

        for symbol in symbols {
            if symbol == "USD" {
                rates.insert(symbol.clone(), 1.0);
                continue;
            }

            let pair = format!("USD/{}", symbol);
            let resp = self
                .client
                .get("https://api.twelvedata.com/price")
                .query(&[("symbol", pair.as_str()), ("apikey", self.api_key.as_str())])
                .send()
                .await
                .with_context(|| format!("Twelve Data request failed for {}", pair))?;

            let status = resp.status();
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                if is_quota_error(&body) {
                    warn!(
                        "Twelve Data quota reached at {}; returning {} partial rate(s)",
                        pair,
                        rates.len()
                    );
                    break;
                }
                warn!(
                    "Twelve Data USD/{} failed (HTTP {}): {} — ignored",
                    symbol, status, body
                );
                continue;
            }

            let body: serde_json::Value = resp
                .json()
                .await
                .with_context(|| format!("Twelve Data parse failed for {}", pair))?;

            if let Some(message) = body.get("message").and_then(|v| v.as_str()) {
                if is_quota_error(message) {
                    warn!(
                        "Twelve Data quota reached at {}; returning {} partial rate(s)",
                        pair,
                        rates.len()
                    );
                    break;
                }
                warn!(
                    "Twelve Data USD/{} failed (API error): {} — ignored",
                    symbol, message
                );
                continue;
            }

            let Some(rate_str) = body.get("price").and_then(|v| v.as_str()) else {
                warn!("Twelve Data USD/{} failed (missing price) — ignored", symbol);
                continue;
            };
            let Ok(rate) = rate_str.parse::<f64>() else {
                warn!(
                    "Twelve Data USD/{} failed (invalid rate '{}') — ignored",
                    symbol, rate_str
                );
                continue;
            };

            rates.insert(symbol.clone(), rate);
        }

        if rates.is_empty() {
            anyhow::bail!("Twelve Data did not return any forex rates");
        }

        Ok(rates)
    }
}

fn is_quota_error(message: &str) -> bool {
    let msg = message.to_lowercase();
    msg.contains("run out of api credits")
        || msg.contains("current limit")
        || msg.contains("quota")
        || msg.contains("credits")
}
