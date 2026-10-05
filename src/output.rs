use crate::forex_aggregate::AggregatedForexRate;
use crate::types::AggregatedResult;
use anyhow::{Context, Result};
use holo_hash::ActionHash;
use rave_engine::types::{ConversionData, ConversionTable, ForexRate, ReferenceUnit};
use std::collections::HashMap;
use std::str::FromStr;
use zfuel::fuel::ZFuel;

pub fn build_conversion_table(
    results: &[AggregatedResult],
    forex_rates: &[AggregatedForexRate],
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

    let mut output_forex_rates = Vec::new();
    for rate in forex_rates {
        let rate_str = format!("{}", rate.foreign_per_usd);
        let rate_zfuel = ZFuel::from_str(&rate_str)
            .map_err(|e| anyhow::anyhow!("ZFuel parse error for forex '{}': {:?}", rate_str, e))?;
        output_forex_rates.push(ForexRate {
            symbol: rate.symbol.clone(),
            name: rate.name.clone(),
            rate: rate_zfuel,
        });
    }

    Ok(ConversionTable {
        reference_unit,
        data,
        forex_rates: output_forex_rates,
        additional_data: None,
        global_definition,
    })
}

pub fn print_table(results: &[AggregatedResult]) {
    println!(
        "\n{:<8} {:<12} {:<16} {:<14} {:<14} {:<8} Sources",
        "Index", "Name", "Price (USD)", "Volume 24h", "Change 24h%", "Valid"
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

#[cfg(test)]
mod tests {
    use super::build_conversion_table;
    use crate::forex_aggregate::AggregatedForexRate;
    use crate::types::AggregatedResult;
    use holo_hash::ActionHash;
    use holochain_client::ExternIO;
    use rave_engine::types::ConversionTable;

    fn unit(unit_index: u32, avg_price_usd: f64) -> AggregatedResult {
        AggregatedResult {
            unit_index,
            name: format!("unit {unit_index}"),
            contract: format!("0x{unit_index:040x}"),
            avg_price_usd,
            volume_24h: Some(1_234_567.891),
            price_change_24h: Some(-2.5),
            sources: vec!["coingecko".to_string(), "geckoterminal".to_string()],
            valid: true,
        }
    }

    /// ham sends `ExternIO::encode(table)`, and `create_conversion_table` decodes
    /// it as rave_engine's `ConversionTable`. Compared as JSON, not with
    /// `PartialEq`, so a table type of the oracle's own still compiles here and
    /// fails if the DNA would read it differently.
    #[test]
    fn the_dna_decodes_the_table_the_oracle_sends() {
        let sent = build_conversion_table(
            &[unit(0, 0.00123456), unit(1, 42.5)],
            &[AggregatedForexRate {
                symbol: "EUR".to_string(),
                name: "Euro".to_string(),
                foreign_per_usd: 0.93,
            }],
            Some(ActionHash::from_raw_36(vec![7u8; 36])),
        )
        .expect("the table builds");

        let received: ConversionTable = ExternIO::encode(&sent)
            .expect("ham encodes the table")
            .decode()
            .expect("the DNA decodes the table");

        assert_eq!(
            serde_json::to_value(&received).expect("received as JSON"),
            serde_json::to_value(&sent).expect("sent as JSON"),
        );
    }
}
