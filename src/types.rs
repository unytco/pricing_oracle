use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenData {
    pub name: String,
    pub chain: String,
    pub contract: String,
    pub price_usd: f64,
    pub market_cap: Option<f64>,
    pub volume_24h: Option<f64>,
    pub liquidity: Option<f64>,
    pub price_change_24h: Option<f64>,
    pub source: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AggregatedResult {
    pub unit_index: u32,
    pub name: String,
    pub contract: String,
    pub avg_price_usd: f64,
    pub volume_24h: Option<f64>,
    pub price_change_24h: Option<f64>,
    pub sources: Vec<String>,
    pub valid: bool,
}
