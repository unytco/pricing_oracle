mod aggregate;
mod config;
mod forex;
mod forex_aggregate;
mod http;
mod output;
mod sources;
mod types;
mod zome;

use anyhow::{Context, Result};
use clap::Parser;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::info;

#[derive(Parser, Debug)]
#[command(
    name = "pricing-oracle",
    version,
    about = "Fetch token prices, validate, build ConversionTable, and optionally submit to Unyt DNA"
)]
struct Args {
    /// The network's config: config.yaml for TestNet, config.mainnet.yaml for MainNet
    #[arg(short, long)]
    config: PathBuf,

    /// Output format: "table" (default) or "json"
    #[arg(short, long, default_value = "table")]
    output: String,

    /// Only fetch for a specific unit index
    #[arg(short, long)]
    unit: Option<u32>,

    /// Submit the ConversionTable to the Unyt DNA via create_conversion_table zome call
    #[arg(long, conflicts_with = "dry_run")]
    submit: bool,

    /// Build and print the ConversionTable JSON without connecting to Holochain
    #[arg(long, conflicts_with = "submit")]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    let cfg = config::Config::load(&args.config)
        .with_context(|| format!("loading config from {}", args.config.display()))?;

    info!(
        "Loaded {} units and {} price reference(s) from config",
        cfg.units.len(),
        cfg.price_references.len()
    );

    let submission = if args.submit {
        let hc_config =
            zome::HolochainConfig::from_env().context("loading Holochain config for --submit")?;
        Some(
            zome::Submission::prepare(hc_config)
                .await
                .context("--submit could not read the current GlobalDefinition")?,
        )
    } else {
        None
    };

    let coingecko_key = std::env::var("COINGECKO_API_KEY").ok();
    let coinmarketcap_key = std::env::var("COINMARKETCAP_API_KEY").ok();
    let twelve_data_key = std::env::var("TWELVE_DATA_API_KEY").ok();
    let coinapi_key = std::env::var("COINAPI_API_KEY").ok();
    // One client for every source: `reqwest::Client` is internally reference-counted,
    // and building a second one would re-read and re-parse the host CA store.
    let client = http::client()?;

    let registry = sources::SourceRegistry::new(client.clone(), coingecko_key, coinmarketcap_key);
    info!("Registered {} price source(s)", registry.source_count());

    let mut reference_prices: HashMap<String, types::AggregatedResult> = HashMap::new();
    for ref_entry in &cfg.price_references {
        info!(
            "Fetching price reference '{}' ({})",
            ref_entry.id, ref_entry.name
        );
        let ref_unit = ref_entry.to_unit_config_for_fetch();
        let fetch_results = registry.fetch_all(&ref_unit).await;
        let mut successful: Vec<types::TokenData> = Vec::new();
        for (source_name, result) in fetch_results {
            match result {
                Ok(data) => {
                    info!("  [{}] price={:.8} USD", source_name, data.price_usd);
                    successful.push(data);
                }
                Err(e) => {
                    tracing::warn!("  [{}] failed: {:#}", source_name, e);
                }
            }
        }
        let agg = aggregate::aggregate(0, successful);
        reference_prices.insert(ref_entry.id.clone(), agg);
    }

    let real_units: Vec<_> = match args.unit {
        Some(idx) => cfg
            .real_units()
            .into_iter()
            .filter(|u| u.unit_index == idx)
            .collect(),
        None => cfg.real_units(),
    };

    let mut aggregated: Vec<types::AggregatedResult> = Vec::new();

    for unit in &real_units {
        info!(
            "Fetching prices for unit {} ({})",
            unit.unit_index, unit.name
        );
        let fetch_results = registry.fetch_all(unit).await;

        let mut successful: Vec<types::TokenData> = Vec::new();
        for (source_name, result) in fetch_results {
            match result {
                Ok(data) => {
                    info!("  [{}] price={:.8} USD", source_name, data.price_usd);
                    successful.push(data);
                }
                Err(e) => {
                    tracing::warn!("  [{}] failed: {:#}", source_name, e);
                }
            }
        }

        let agg = aggregate::aggregate(unit.unit_index, successful);
        aggregated.push(agg);
    }

    let proxy_units: Vec<_> = match args.unit {
        Some(idx) => cfg
            .proxy_units()
            .into_iter()
            .filter(|u| u.unit_index == idx)
            .collect(),
        None => cfg.proxy_units(),
    };

    add_proxied_prices(&cfg, &proxy_units, &reference_prices, &mut aggregated)?;

    aggregated.sort_by_key(|a| a.unit_index);

    let batch_size = cfg.forex.max_symbols_per_run;
    let delay_secs = cfg.forex.delay_between_batches_secs;
    let forex_registry = forex::ForexSourceRegistry::new(
        client,
        twelve_data_key,
        coinapi_key,
        cfg.forex.use_twelve_data,
        cfg.forex.use_coinapi,
    );
    info!(
        "Registered {} forex source(s); fetching in batches of {} ({} total symbols)",
        forex_registry.source_count(),
        batch_size,
        cfg.forex.symbols.len()
    );

    let mut aggregated_forex: Vec<forex_aggregate::AggregatedForexRate> = Vec::new();
    let chunks: Vec<Vec<String>> = cfg
        .forex
        .symbols
        .chunks(batch_size)
        .map(|c| c.to_vec())
        .collect();
    let total_batches = chunks.len();

    for (i, chunk) in chunks.into_iter().enumerate() {
        if i > 0 && delay_secs > 0 {
            info!(
                "Waiting {}s before next forex batch (rate limit)",
                delay_secs
            );
            tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
        }
        info!(
            "Forex batch {}/{}: {}",
            i + 1,
            total_batches,
            chunk.join(", ")
        );
        let forex_results = forex_registry.fetch_all(&chunk).await;
        let batch_rates = forex_aggregate::aggregate_forex_rates(&chunk, forex_results);
        aggregated_forex.extend(batch_rates);
    }

    if args.dry_run {
        let table = output::build_conversion_table(&aggregated, &aggregated_forex, None)?;
        println!("--- Dry-run: ConversionTable that would be submitted ---");
        output::print_json(&table)?;
        return Ok(());
    }

    if let Some(submission) = submission {
        let table = output::build_conversion_table(
            &aggregated,
            &aggregated_forex,
            Some(submission.global_definition()),
        )?;
        println!("--- ConversionTable to submit ---");
        output::print_json(&table)?;

        let action_hash = submission.submit(table).await?;
        println!("Submitted ConversionTable: {}", action_hash);
        return Ok(());
    }

    match args.output.as_str() {
        "json" => {
            let table = output::build_conversion_table(&aggregated, &aggregated_forex, None)?;
            output::print_json(&table)?;
        }
        _ => {
            output::print_table(&aggregated);
        }
    }

    Ok(())
}

fn add_proxied_prices(
    cfg: &config::Config,
    proxy_units: &[&config::UnitConfig],
    reference_prices: &HashMap<String, types::AggregatedResult>,
    aggregated: &mut Vec<types::AggregatedResult>,
) -> Result<()> {
    for proxy_unit in proxy_units {
        let proxy_cfg = proxy_unit.price_proxy.as_ref().unwrap();
        let source = cfg
            .resolve_proxy_source(proxy_unit.unit_index, proxy_cfg)
            .context("resolving price_proxy")?;

        let source_agg = match &source {
            config::ProxySource::Unit(use_unit) => aggregated
                .iter()
                .find(|a| a.unit_index == *use_unit)
                .cloned(),
            config::ProxySource::Reference(id) => reference_prices.get(id).cloned(),
        };

        if let Some(source_agg) = source_agg {
            let from = match &source {
                config::ProxySource::Unit(u) => format!("unit {}", u),
                config::ProxySource::Reference(id) => format!("reference '{}'", id),
            };
            info!(
                "Proxying unit {} ({}) from {} — price={:.8}",
                proxy_unit.unit_index, proxy_unit.name, from, source_agg.avg_price_usd
            );
            let mut proxied = source_agg;
            proxied.unit_index = proxy_unit.unit_index;
            proxied.name = proxy_unit.name.clone();
            proxied.contract = proxy_unit.contract.clone();
            aggregated.push(proxied);
        } else {
            let (kind, val) = match &source {
                config::ProxySource::Unit(u) => ("unit", format!("{}", u)),
                config::ProxySource::Reference(id) => ("reference", id.clone()),
            };
            tracing::warn!(
                "unit {} ({}) proxy {} {} not found or not fetched",
                proxy_unit.unit_index,
                proxy_unit.name,
                kind,
                val,
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{add_proxied_prices, aggregate, config, output, types};
    use chrono::Utc;
    use std::collections::{BTreeSet, HashMap};
    use std::path::Path;

    const MOCK_HOT: &str = "0xeaC8eEEE9f84F3E3F592e9D8604100eA1b788749";
    const REAL_HOT: &str = "0x6c6EE5e31d828De241282B9606C8e98Ea48526E2";

    fn shipped(file: &str) -> config::Config {
        config::Config::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(file))
            .unwrap_or_else(|e| panic!("{file} does not load: {e:#}"))
    }

    fn dry_run_table(cfg: &config::Config) -> rave_engine::types::ConversionTable {
        let mut reference_prices = HashMap::new();
        for reference in &cfg.price_references {
            let quote = types::TokenData {
                name: reference.name.clone(),
                chain: reference.chain.clone(),
                contract: reference.contract.clone(),
                price_usd: 0.0004,
                market_cap: None,
                volume_24h: None,
                liquidity: None,
                price_change_24h: None,
                source: "geckoterminal".to_string(),
                timestamp: Utc::now(),
            };
            reference_prices.insert(reference.id.clone(), aggregate::aggregate(0, vec![quote]));
        }
        let mut aggregated = Vec::new();
        add_proxied_prices(cfg, &cfg.proxy_units(), &reference_prices, &mut aggregated)
            .expect("the proxies resolve");
        output::build_conversion_table(&aggregated, &[], None).expect("the table builds")
    }

    #[test]
    fn each_shipped_config_writes_its_networks_hot_as_the_contract_of_hf_and_hot() {
        for (file, hot) in [("config.yaml", MOCK_HOT), ("config.mainnet.yaml", REAL_HOT)] {
            let table = dry_run_table(&shipped(file));
            let units: BTreeSet<&str> = table.data.keys().map(String::as_str).collect();
            assert_eq!(
                units,
                BTreeSet::from(["0", "1"]),
                "{file} prices other units"
            );
            for unit in ["0", "1"] {
                assert_eq!(
                    table.data[unit].contract.as_deref(),
                    Some(hot),
                    "{file} writes the wrong contract for unit {unit}"
                );
            }
        }
    }

    #[test]
    fn both_shipped_configs_price_from_real_hot_with_one_forex_list() {
        let testnet = shipped("config.yaml");
        let mainnet = shipped("config.mainnet.yaml");
        for (file, cfg) in [("config.yaml", &testnet), ("config.mainnet.yaml", &mainnet)] {
            let [hot] = cfg.price_references.as_slice() else {
                panic!("{file} has other price references than HOT");
            };
            assert_eq!((hot.id.as_str(), hot.chain.as_str()), ("HOT", "ethereum"));
            assert!(
                hot.contract.eq_ignore_ascii_case(REAL_HOT),
                "{file} prices from {} rather than real HOT",
                hot.contract
            );
        }
        assert_eq!(testnet.forex.symbols, mainnet.forex.symbols);
    }
}
