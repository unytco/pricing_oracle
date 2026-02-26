use anyhow::Result;
use std::collections::HashMap;
use tracing::warn;

const FOREX_DEVIATION_THRESHOLD: f64 = 0.01;

#[derive(Debug, Clone)]
pub struct AggregatedForexRate {
    pub symbol: String,
    pub name: String,
    pub foreign_per_usd: f64,
}

pub fn aggregate_forex_rates(
    symbols: &[String],
    source_results: Vec<(String, Result<HashMap<String, f64>>)>,
) -> Vec<AggregatedForexRate> {
    let mut by_symbol: HashMap<String, Vec<(String, f64)>> = HashMap::new();

    for (source_name, result) in source_results {
        match result {
            Ok(rates) => {
                for symbol in symbols {
                    if let Some(rate) = rates.get(symbol) {
                        if let Some(normalized) = normalize_foreign_per_usd(*rate) {
                            by_symbol
                                .entry(symbol.clone())
                                .or_default()
                                .push((source_name.clone(), normalized));
                        }
                    }
                }
            }
            Err(e) => warn!(
                "forex source '{}' failed: {} — any symbols only from this source will be ignored, omitted from ConversionTable",
                source_name, e
            ),
        }
    }

    let mut aggregated = Vec::new();
    for symbol in symbols {
        let Some(values) = by_symbol.get(symbol) else {
            warn!(
                "forex symbol '{}' failed (missing from all sources) — ignored, omitted from ConversionTable",
                symbol
            );
            continue;
        };
        if values.is_empty() {
            warn!(
                "forex symbol '{}' failed (no valid rates) — ignored, omitted from ConversionTable",
                symbol
            );
            continue;
        }

        let avg = values.iter().map(|(_, rate)| *rate).sum::<f64>() / values.len() as f64;
        if values.len() > 1 {
            for (source, rate) in values {
                let deviation = (rate - avg).abs() / avg;
                if deviation > FOREX_DEVIATION_THRESHOLD {
                    warn!(
                        "forex {} source '{}' deviates {:.2}% from average {:.8}",
                        symbol,
                        source,
                        deviation * 100.0,
                        avg
                    );
                }
            }
        }

        aggregated.push(AggregatedForexRate {
            symbol: symbol.clone(),
            name: currency_name(symbol).to_string(),
            foreign_per_usd: avg,
        });
    }

    aggregated
}

fn normalize_foreign_per_usd(rate: f64) -> Option<f64> {
    if rate.is_finite() && rate > 0.0 {
        Some(rate)
    } else {
        None
    }
}

fn currency_name(symbol: &str) -> &'static str {
    match symbol {
        "USD" => "US Dollar",
        "EUR" => "Euro",
        "GBP" => "British Pound",
        "JPY" => "Japanese Yen",
        "CHF" => "Swiss Franc",
        "CAD" => "Canadian Dollar",
        "AUD" => "Australian Dollar",
        "NZD" => "New Zealand Dollar",
        "SEK" => "Swedish Krona",
        "NOK" => "Norwegian Krone",
        "DKK" => "Danish Krone",
        "PLN" => "Polish Zloty",
        "CZK" => "Czech Koruna",
        "HUF" => "Hungarian Forint",
        "RON" => "Romanian Leu",
        "TRY" => "Turkish Lira",
        "RUB" => "Russian Ruble",
        "UAH" => "Ukrainian Hryvnia",
        "ILS" => "Israeli New Shekel",
        "AED" => "UAE Dirham",
        "SAR" => "Saudi Riyal",
        "QAR" => "Qatari Riyal",
        "KWD" => "Kuwaiti Dinar",
        "BHD" => "Bahraini Dinar",
        "OMR" => "Omani Rial",
        "ZAR" => "South African Rand",
        "EGP" => "Egyptian Pound",
        "NGN" => "Nigerian Naira",
        "KES" => "Kenyan Shilling",
        "INR" => "Indian Rupee",
        "PKR" => "Pakistani Rupee",
        "BDT" => "Bangladeshi Taka",
        "CNY" => "Chinese Yuan",
        "HKD" => "Hong Kong Dollar",
        "SGD" => "Singapore Dollar",
        "KRW" => "South Korean Won",
        "TWD" => "New Taiwan Dollar",
        "THB" => "Thai Baht",
        "MYR" => "Malaysian Ringgit",
        "IDR" => "Indonesian Rupiah",
        "PHP" => "Philippine Peso",
        "VND" => "Vietnamese Dong",
        "MXN" => "Mexican Peso",
        "BRL" => "Brazilian Real",
        "ARS" => "Argentine Peso",
        "CLP" => "Chilean Peso",
        "COP" => "Colombian Peso",
        "PEN" => "Peruvian Sol",
        "UYU" => "Uruguayan Peso",
        _ => "Unknown Currency",
    }
}
