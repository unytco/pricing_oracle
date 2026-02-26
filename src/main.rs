mod aggregate;
mod config;
mod ham;
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
    about = "Fetch token prices, validate, build ConversionTable, and optionally submit to Unyt DNA"
)]
struct Args {
    /// Path to config YAML file
    #[arg(short, long, default_value = "config.yaml")]
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

    let coingecko_key = std::env::var("COINGECKO_API_KEY").ok();
    let client = reqwest::Client::builder()
        .user_agent("pricing-oracle/0.1")
        .build()
        .context("building HTTP client")?;

    let registry = sources::SourceRegistry::new(client, coingecko_key);
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
                    tracing::warn!("  [{}] failed: {}", source_name, e);
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
        info!("Fetching prices for unit {} ({})", unit.unit_index, unit.name);
        let fetch_results = registry.fetch_all(unit).await;

        let mut successful: Vec<types::TokenData> = Vec::new();
        for (source_name, result) in fetch_results {
            match result {
                Ok(data) => {
                    info!(
                        "  [{}] price={:.8} USD",
                        source_name, data.price_usd
                    );
                    successful.push(data);
                }
                Err(e) => {
                    tracing::warn!("  [{}] failed: {}", source_name, e);
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

    for proxy_unit in &proxy_units {
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
                proxy_unit.unit_index,
                proxy_unit.name,
                from,
                source_agg.avg_price_usd
            );
            let mut proxied = source_agg;
            proxied.unit_index = proxy_unit.unit_index;
            proxied.name = proxy_unit.name.clone();
            proxied.contract = proxy_unit.contract.clone();
            proxied.sources = vec!["proxy".to_string()];
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

    aggregated.sort_by_key(|a| a.unit_index);

    if args.dry_run {
        let table = output::build_conversion_table(&aggregated, None)?;
        println!("--- Dry-run: ConversionTable that would be submitted ---");
        output::print_json(&table)?;
        return Ok(());
    }

    if args.submit {
        let hc_config = zome::HolochainConfig::from_env()
            .context("loading Holochain config for --submit")?;

        let global_def = zome::fetch_global_definition(&hc_config)
            .await
            .context("fetching current GlobalDefinition")?;

        let table = output::build_conversion_table(&aggregated, Some(global_def))?;
        println!("--- ConversionTable to submit ---");
        output::print_json(&table)?;

        let action_hash = zome::submit_conversion_table(&hc_config, table).await?;
        println!("Submitted ConversionTable: {}", action_hash);
        return Ok(());
    }

    match args.output.as_str() {
        "json" => {
            let table = output::build_conversion_table(&aggregated, None)?;
            output::print_json(&table)?;
        }
        _ => {
            output::print_table(&aggregated);
        }
    }

    Ok(())
}
