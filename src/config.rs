use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub price_references: Vec<PriceReference>,
    pub units: Vec<UnitConfig>,
}

/// Token fetched for price only; not in ConversionTable, no unit_index.
#[derive(Debug, Clone, Deserialize)]
pub struct PriceReference {
    pub id: String,
    pub name: String,
    pub chain: String,
    pub contract: String,
    #[serde(default)]
    pub decimals: Option<u8>,
}

impl PriceReference {
    /// Build a UnitConfig-shaped value for use with SourceRegistry::fetch_all (same fields needed for API calls).
    pub fn to_unit_config_for_fetch(&self) -> UnitConfig {
        UnitConfig {
            unit_index: 0,
            name: self.name.clone(),
            chain: self.chain.clone(),
            contract: self.contract.clone(),
            decimals: self.decimals,
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
    pub decimals: Option<u8>,
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
        let config: Config =
            serde_yaml::from_str(&contents).with_context(|| format!("parsing {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
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
                        anyhow::bail!(
                            "unit '{}' has price_proxy pointing to itself",
                            unit.name
                        );
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

    pub fn real_units(&self) -> Vec<&UnitConfig> {
        self.units
            .iter()
            .filter(|u| u.price_proxy.is_none())
            .collect()
    }

    pub fn proxy_units(&self) -> Vec<&UnitConfig> {
        self.units
            .iter()
            .filter(|u| u.price_proxy.is_some())
            .collect()
    }

    /// Resolve proxy to either a unit index or a reference id.
    pub fn resolve_proxy_source(
        &self,
        unit_index: u32,
        proxy: &PriceProxy,
    ) -> Result<ProxySource> {
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
