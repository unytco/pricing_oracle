use chrono::{DateTime, Utc};
use holo_hash::{ActionHash, ActionHashB64};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zfuel::fuel::ZFuel;

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
    pub per_source: Vec<TokenData>,
}

/// Mirrors rave_engine ConversionTable (not yet in published crate).
/// Will be replaced by rave_engine import when a new version is published.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionTable {
    pub reference_unit: ReferenceUnit,
    pub data: HashMap<String, ConversionData>,
    pub forex_rates: Vec<ForexRate>,
    pub additional_data: Option<Vec<u8>>,
    pub global_definition: ActionHash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForexRate {
    pub symbol: String,
    pub name: String,
    pub rate: ZFuel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceUnit {
    pub symbol: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionData {
    pub current_price: ZFuel,
    pub volume: String,
    pub net_change: String,
    pub sources: Vec<String>,
    pub contract: Option<String>,
}

/// Minimal mirror of rave_engine's GlobalDefinitionExt.
/// Only the `id` field is needed; remaining fields are ignored during
/// MessagePack deserialization (named-map format).
#[derive(Debug, Clone, Deserialize)]
pub struct GlobalDefinitionExt {
    pub id: ActionHashB64,
}
