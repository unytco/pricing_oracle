use crate::types::{AggregatedResult, TokenData};
use tracing::{info, warn};

const DEVIATION_THRESHOLD: f64 = 0.01; // 1%

pub fn aggregate(unit_index: u32, data: Vec<TokenData>) -> AggregatedResult {
    let name = data.first().map(|d| d.name.clone()).unwrap_or_default();
    let contract = data.first().map(|d| d.contract.clone()).unwrap_or_default();
    let sources: Vec<String> = data.iter().map(|d| d.source.clone()).collect();

    if data.is_empty() {
        return AggregatedResult {
            unit_index,
            name,
            contract,
            avg_price_usd: 0.0,
            volume_24h: None,
            price_change_24h: None,
            sources,
            valid: false,
            per_source: data,
        };
    }

    let avg_price: f64 = data.iter().map(|d| d.price_usd).sum::<f64>() / data.len() as f64;

    let valid = if data.len() < 2 {
        warn!(
            "unit {} ({}): only {} source — skipping cross-check",
            unit_index, name, data.len()
        );
        true
    } else {
        let all_within = data.iter().all(|d| {
            let deviation = (d.price_usd - avg_price).abs() / avg_price;
            if deviation > DEVIATION_THRESHOLD {
                warn!(
                    "unit {} ({}): source '{}' price {:.8} deviates {:.2}% from average {:.8}",
                    unit_index,
                    name,
                    d.source,
                    d.price_usd,
                    deviation * 100.0,
                    avg_price,
                );
            }
            deviation <= DEVIATION_THRESHOLD
        });
        if all_within {
            info!(
                "unit {} ({}): all {} sources within 1% — valid (avg {:.8})",
                unit_index,
                name,
                data.len(),
                avg_price
            );
        }
        all_within
    };

    let volume_24h = aggregate_optional(&data, |d| d.volume_24h);
    let price_change_24h = aggregate_optional(&data, |d| d.price_change_24h);

    AggregatedResult {
        unit_index,
        name,
        contract,
        avg_price_usd: avg_price,
        volume_24h,
        price_change_24h,
        sources,
        valid,
        per_source: data,
    }
}

fn aggregate_optional(data: &[TokenData], f: fn(&TokenData) -> Option<f64>) -> Option<f64> {
    let vals: Vec<f64> = data.iter().filter_map(|d| f(d)).collect();
    if vals.is_empty() {
        None
    } else {
        Some(vals.iter().sum::<f64>() / vals.len() as f64)
    }
}
