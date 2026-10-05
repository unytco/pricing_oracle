use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub price_references: Vec<PriceReference>,
    #[serde(default)]
    pub forex: ForexConfig,
    pub units: Vec<UnitConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ForexConfig {
    pub symbols: Vec<String>,
    pub use_twelve_data: bool,
    pub use_coinapi: bool,
    pub max_symbols_per_run: usize,
    pub delay_between_batches_secs: u64,
}

impl Default for ForexConfig {
    fn default() -> Self {
        Self {
            symbols: Vec::new(),
            use_twelve_data: true,
            use_coinapi: true,
            max_symbols_per_run: 8,
            delay_between_batches_secs: 0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PriceReference {
    pub id: String,
    pub name: String,
    pub chain: String,
    pub contract: String,
}

impl PriceReference {
    pub fn to_unit_config_for_fetch(&self) -> UnitConfig {
        UnitConfig {
            unit_index: 0,
            name: self.name.clone(),
            chain: self.chain.clone(),
            contract: self.contract.clone(),
            price_proxy: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnitConfig {
    pub unit_index: u32,
    pub name: String,
    pub chain: String,
    pub contract: String,
    pub price_proxy: Option<PriceProxy>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PriceProxy {
    pub use_unit: Option<u32>,
    pub use_reference: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ProxySource {
    Unit(u32),
    Reference(String),
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let contents =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let config: Config = serde_yaml::from_str(&contents)
            .with_context(|| format!("parsing {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        let mut seen_forex: HashMap<&str, ()> = HashMap::new();
        for symbol in &self.forex.symbols {
            if symbol.trim().is_empty() {
                anyhow::bail!("forex.symbols contains an empty symbol");
            }
            if symbol.len() != 3 || !symbol.chars().all(|c| c.is_ascii_uppercase()) {
                anyhow::bail!(
                    "forex.symbols '{}' must be a 3-letter uppercase currency code",
                    symbol
                );
            }
            if seen_forex.insert(symbol.as_str(), ()).is_some() {
                anyhow::bail!("forex.symbols contains duplicate '{}'", symbol);
            }
        }
        if self.forex.max_symbols_per_run == 0 {
            anyhow::bail!("forex.max_symbols_per_run must be greater than 0");
        }

        let mut ref_ids: HashMap<&str, &str> = HashMap::new();
        for r in &self.price_references {
            if let Some(prev) = ref_ids.insert(r.id.as_str(), r.name.as_str()) {
                anyhow::bail!(
                    "duplicate price_reference id '{}': '{}' and '{}'",
                    r.id,
                    prev,
                    r.name
                );
            }
        }

        if let Some(first) = self.units.first() {
            if self.units.iter().any(|u| u.chain != first.chain) {
                let units: Vec<String> = self
                    .units
                    .iter()
                    .map(|u| format!("unit {} '{}' on '{}'", u.unit_index, u.name, u.chain))
                    .collect();
                anyhow::bail!(
                    "units name more than one chain ({}); a config is for one network",
                    units.join(", ")
                );
            }
        }

        let mut seen: HashMap<u32, &str> = HashMap::new();
        for unit in &self.units {
            if let Some(prev) = seen.insert(unit.unit_index, &unit.name) {
                anyhow::bail!(
                    "duplicate unit_index {}: '{}' and '{}'",
                    unit.unit_index,
                    prev,
                    unit.name
                );
            }
            if let Some(proxy) = &unit.price_proxy {
                let has_unit = proxy.use_unit.is_some();
                let has_ref = proxy.use_reference.is_some();
                if has_unit == has_ref {
                    anyhow::bail!(
                        "unit '{}' price_proxy must have exactly one of use_unit or use_reference",
                        unit.name
                    );
                }
                if let Some(use_unit) = proxy.use_unit {
                    if !self.units.iter().any(|u| u.unit_index == use_unit) {
                        anyhow::bail!(
                            "unit '{}' has price_proxy.use_unit {} which does not exist in units",
                            unit.name,
                            use_unit
                        );
                    }
                    if use_unit == unit.unit_index {
                        anyhow::bail!("unit '{}' has price_proxy pointing to itself", unit.name);
                    }
                }
                if let Some(ref id) = proxy.use_reference {
                    if !self.price_references.iter().any(|r| r.id == *id) {
                        anyhow::bail!(
                            "unit '{}' has price_proxy.use_reference '{}' which does not exist in price_references",
                            unit.name,
                            id
                        );
                    }
                }
            }
        }
        Ok(())
    }

    pub fn resolve_proxy_source(&self, unit_index: u32, proxy: &PriceProxy) -> Result<ProxySource> {
        if let Some(use_unit) = proxy.use_unit {
            if use_unit == unit_index {
                anyhow::bail!("price_proxy use_unit cannot point to self");
            }
            return Ok(ProxySource::Unit(use_unit));
        }
        if let Some(ref id) = proxy.use_reference {
            return Ok(ProxySource::Reference(id.clone()));
        }
        anyhow::bail!("price_proxy must have use_unit or use_reference");
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn a_unit_on_another_chain_is_refused_wherever_it_sits() {
        let cfg: Config = serde_yaml::from_str(
            r#"
units:
  - { unit_index: 0, name: "A", chain: "sepolia", contract: "0xa" }
  - { unit_index: 1, name: "B", chain: "ethereum", contract: "0xb" }
  - { unit_index: 2, name: "C", chain: "sepolia", contract: "0xc" }
"#,
        )
        .expect("the YAML parses");

        let refusal = format!("{:#}", cfg.validate().expect_err("mixed chains load"));
        assert!(
            refusal.contains("unit 1 'B' on 'ethereum'"),
            "the refusal does not name the odd unit: {refusal}"
        );
    }

    #[test]
    fn a_config_without_forex_loads_with_the_documented_defaults() {
        let cfg: Config = serde_yaml::from_str("units: []\n").expect("the YAML parses");
        cfg.validate().expect("forex is optional");
        assert!(cfg.forex.symbols.is_empty());
        assert!(cfg.forex.use_twelve_data && cfg.forex.use_coinapi);
        assert_eq!(cfg.forex.max_symbols_per_run, 8);
        assert_eq!(cfg.forex.delay_between_batches_secs, 0);
    }
}
