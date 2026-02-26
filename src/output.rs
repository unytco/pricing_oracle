use crate::types::{AggregatedResult, ConversionData, ConversionTable, ReferenceUnit};
use anyhow::{Context, Result};
use holo_hash::ActionHash;
use std::collections::HashMap;
use std::str::FromStr;
use zfuel::fuel::ZFuel;

pub fn build_conversion_table(
    results: &[AggregatedResult],
    global_definition: Option<ActionHash>,
) -> Result<ConversionTable> {
    let reference_unit = ReferenceUnit {
        symbol: "$".to_string(),
        name: "US Dollar".to_string(),
    };

    let mut data: HashMap<String, ConversionData> = HashMap::new();
    for r in results {
        if !r.valid {
            tracing::warn!(
                "unit {} ({}) is invalid — omitting from ConversionTable",
                r.unit_index,
                r.name
            );
            continue;
        }

        let price_str = format!("{}", r.avg_price_usd);
        let current_price = ZFuel::from_str(&price_str)
            .map_err(|e| anyhow::anyhow!("ZFuel parse error for '{}': {:?}", price_str, e))?;

        let volume = r
            .volume_24h
            .map(|v| format!("{:.2}", v))
            .unwrap_or_default();

        let net_change = r
            .price_change_24h
            .map(|c| format!("{:.4}", c))
            .unwrap_or_default();

        let conversion = ConversionData {
            current_price,
            volume,
            net_change,
            sources: r.sources.clone(),
            contract: Some(r.contract.clone()),
        };

        data.insert(r.unit_index.to_string(), conversion);
    }

    let global_definition =
        global_definition.unwrap_or_else(|| ActionHash::from_raw_36(vec![0u8; 36]));

    Ok(ConversionTable {
        reference_unit,
        data,
        additional_data: None,
        global_definition,
    })
}

pub fn print_table(results: &[AggregatedResult]) {
    println!(
        "\n{:<8} {:<12} {:<16} {:<14} {:<14} {:<8} {}",
        "Index", "Name", "Price (USD)", "Volume 24h", "Change 24h%", "Valid", "Sources"
    );
    println!("{}", "-".repeat(90));
    for r in results {
        let vol = r
            .volume_24h
            .map(|v| format!("{:.2}", v))
            .unwrap_or_else(|| "—".to_string());
        let change = r
            .price_change_24h
            .map(|c| format!("{:+.4}%", c))
            .unwrap_or_else(|| "—".to_string());
        let valid_str = if r.valid { "yes" } else { "NO" };
        let sources = r.sources.join(", ");
        println!(
            "{:<8} {:<12} {:<16.8} {:<14} {:<14} {:<8} {}",
            r.unit_index, r.name, r.avg_price_usd, vol, change, valid_str, sources
        );
    }
    println!();
}

pub fn print_json(table: &ConversionTable) -> Result<()> {
    let json = serde_json::to_string_pretty(table).context("serializing ConversionTable")?;
    println!("{}", json);
    Ok(())
}
