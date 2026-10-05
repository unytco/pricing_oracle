mod aggregate;
mod config;
mod forex;
mod forex_aggregate;
mod http;
mod output;
mod pricing;
mod sources;
mod types;
mod zome;

use anyhow::{Context, Result};
use clap::Parser;
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
        "Loaded {} units and {} price reference(s) from {}, units on chain '{}'",
        cfg.units.len(),
        cfg.price_references.len(),
        args.config.display(),
        cfg.units.first().map_or("none", |u| u.chain.as_str())
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

    let aggregated = pricing::price_units(&cfg, args.unit, &registry).await?;

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
