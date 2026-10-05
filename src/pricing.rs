use crate::aggregate::aggregate;
use crate::config::{Config, ProxySource, UnitConfig};
use crate::sources::SourceRegistry;
use crate::types::{AggregatedResult, TokenData};
use anyhow::{Context, Result};
use std::collections::HashMap;
use tracing::{info, warn};

pub async fn price_units(
    cfg: &Config,
    only_unit: Option<u32>,
    registry: &SourceRegistry,
) -> Result<Vec<AggregatedResult>> {
    let in_scope = |u: &&UnitConfig| only_unit.is_none_or(|i| u.unit_index == i);

    let mut reference_prices: HashMap<&str, AggregatedResult> = HashMap::new();
    for reference in &cfg.price_references {
        info!(
            "Fetching price reference '{}' ({})",
            reference.id, reference.name
        );
        let found = quotes(registry, &reference.to_unit_config_for_fetch()).await;
        reference_prices.insert(&reference.id, aggregate(0, found));
    }

    let mut priced = Vec::new();
    for unit in cfg.units.iter().filter(in_scope) {
        if unit.price_proxy.is_none() {
            info!(
                "Fetching prices for unit {} ({})",
                unit.unit_index, unit.name
            );
            priced.push(aggregate(unit.unit_index, quotes(registry, unit).await));
        }
    }

    let proxies = cfg
        .units
        .iter()
        .filter(in_scope)
        .filter_map(|u| Some((u, u.price_proxy.as_ref()?)));
    for (unit, proxy) in proxies {
        let source = cfg
            .resolve_proxy_source(unit.unit_index, proxy)
            .context("resolving price_proxy")?;

        let source_agg = match &source {
            ProxySource::Unit(use_unit) => {
                priced.iter().find(|a| a.unit_index == *use_unit).cloned()
            }
            ProxySource::Reference(id) => reference_prices.get(id.as_str()).cloned(),
        };
        let from = match &source {
            ProxySource::Unit(u) => format!("unit {u}"),
            ProxySource::Reference(id) => format!("reference '{id}'"),
        };

        if let Some(mut proxied) = source_agg {
            info!(
                "Proxying unit {} ({}) from {}, price={:.8}",
                unit.unit_index, unit.name, from, proxied.avg_price_usd
            );
            proxied.unit_index = unit.unit_index;
            proxied.name = unit.name.clone();
            proxied.contract = unit.contract.clone();
            priced.push(proxied);
        } else {
            warn!(
                "unit {} ({}) proxy {} not found or not fetched",
                unit.unit_index, unit.name, from
            );
        }
    }

    priced.sort_by_key(|a| a.unit_index);
    Ok(priced)
}

async fn quotes(registry: &SourceRegistry, unit: &UnitConfig) -> Vec<TokenData> {
    let mut found = Vec::new();
    for (source_name, result) in registry.fetch_all(unit).await {
        match result {
            Ok(data) => {
                info!("  [{}] price={:.8} USD", source_name, data.price_usd);
                found.push(data);
            }
            Err(e) => warn!("  [{}] failed: {:#}", source_name, e),
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::price_units;
    use crate::config::{Config, UnitConfig};
    use crate::output::build_conversion_table;
    use crate::sources::{PriceSource, SourceRegistry};
    use crate::types::TokenData;
    use anyhow::Result;
    use async_trait::async_trait;
    use chrono::Utc;
    use std::collections::BTreeSet;
    use std::path::Path;

    const MOCK_HOT: &str = "0xeaC8eEEE9f84F3E3F592e9D8604100eA1b788749";
    const REAL_HOT: &str = "0x6c6EE5e31d828De241282B9606C8e98Ea48526E2";

    struct FixedPrice;

    #[async_trait]
    impl PriceSource for FixedPrice {
        fn name(&self) -> &str {
            "fixed"
        }

        async fn fetch(&self, unit: &UnitConfig) -> Result<TokenData> {
            Ok(TokenData {
                name: unit.name.clone(),
                chain: unit.chain.clone(),
                contract: unit.contract.clone(),
                price_usd: 0.0004,
                market_cap: None,
                volume_24h: None,
                liquidity: None,
                price_change_24h: None,
                source: self.name().to_string(),
                timestamp: Utc::now(),
            })
        }
    }

    fn shipped(file: &str) -> Config {
        Config::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(file))
            .unwrap_or_else(|e| panic!("{file} does not load: {e:#}"))
    }

    async fn dry_run_table(
        file: &str,
        only_unit: Option<u32>,
    ) -> rave_engine::types::ConversionTable {
        let registry = SourceRegistry::from_sources(vec![Box::new(FixedPrice)]);
        let priced = price_units(&shipped(file), only_unit, &registry)
            .await
            .expect("the units are priced");
        build_conversion_table(&priced, &[], None).expect("the table builds")
    }

    #[tokio::test]
    async fn each_shipped_config_writes_its_networks_hot_as_the_contract_of_hf_and_hot() {
        for (file, hot) in [("config.yaml", MOCK_HOT), ("config.mainnet.yaml", REAL_HOT)] {
            let table = dry_run_table(file, None).await;
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

    #[tokio::test]
    async fn the_unit_flag_prices_that_unit_alone() {
        let table = dry_run_table("config.mainnet.yaml", Some(1)).await;
        let units: Vec<&str> = table.data.keys().map(String::as_str).collect();
        assert_eq!(units, ["1"]);
    }

    #[test]
    fn both_shipped_configs_price_hf_and_hot_on_their_chain_from_real_hot() {
        let testnet = shipped("config.yaml");
        let mainnet = shipped("config.mainnet.yaml");
        for (file, cfg, chain) in [
            ("config.yaml", &testnet, "sepolia"),
            ("config.mainnet.yaml", &mainnet, "ethereum"),
        ] {
            let units: Vec<_> = cfg
                .units
                .iter()
                .map(|u| (u.unit_index, u.name.as_str(), u.chain.as_str()))
                .collect();
            assert_eq!(units, [(0, "HF", chain), (1, "HOT", chain)], "{file}");

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
