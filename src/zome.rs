use crate::types::{ConversionTable, GlobalDefinitionExt};
use anyhow::{Context, Result};
use ham::{Ham, HamConfig};
use holo_hash::ActionHash;
use std::path::Path;
use tracing::info;

pub struct HolochainConfig {
    pub admin_port: u16,
    pub app_port: u16,
    pub app_id: String,
    pub role_name: String,
    /// Per-request timeout applied to the Holochain app websocket. Bounds
    /// how long a hung conductor call can block this cron invocation.
    pub request_timeout_secs: u64,
    /// Conductor config path — its `keystore.connection_url` is read to sign
    /// zome calls via lair (no cap grant). Defaults to the fleet path.
    pub conductor_config: String,
    /// Lair passphrase file, read to unlock the keystore. Defaults to the
    /// fleet path.
    pub lair_passphrase_file: String,
}

impl HolochainConfig {
    pub fn from_env() -> Result<Self> {
        let admin_port: u16 = std::env::var("HOLOCHAIN_ADMIN_PORT")
            .unwrap_or_else(|_| "30000".to_string())
            .parse()
            .context("Invalid HOLOCHAIN_ADMIN_PORT")?;

        let app_port: u16 = std::env::var("HOLOCHAIN_APP_PORT")
            .unwrap_or_else(|_| "30001".to_string())
            .parse()
            .context("Invalid HOLOCHAIN_APP_PORT")?;

        let app_id =
            std::env::var("HOLOCHAIN_APP_ID").unwrap_or_else(|_| "bridging-app".to_string());

        let role_name =
            std::env::var("HOLOCHAIN_ROLE_NAME").unwrap_or_else(|_| "alliance".to_string());

        let request_timeout_secs: u64 = std::env::var("HAM_REQUEST_TIMEOUT_SECS")
            .unwrap_or_else(|_| "120".to_string())
            .parse()
            .context("Invalid HAM_REQUEST_TIMEOUT_SECS")?;

        let conductor_config = std::env::var("CONDUCTOR_CONFIG")
            .unwrap_or_else(|_| "/etc/holochain/conductor-config.yaml".to_string());

        let lair_passphrase_file = std::env::var("LAIR_PASSPHRASE_FILE")
            .unwrap_or_else(|_| "/var/lib/holochain/lair-passphrase".to_string());

        Ok(Self {
            admin_port,
            app_port,
            app_id,
            role_name,
            request_timeout_secs,
            conductor_config,
            lair_passphrase_file,
        })
    }

    /// The `HamConfig` every connection this oracle makes is built from. Lair
    /// signing is required, never best-effort: the oracle holds a carried
    /// agent key and migrates with it, and the signing path `ham` would
    /// otherwise use commits a capability grant to that chain on every
    /// connect. Against a chain that has already closed, that grant is invalid
    /// and costs the agent its migration for good.
    pub fn ham_config(&self) -> Result<HamConfig> {
        HamConfig::new(self.admin_port, self.app_port, self.app_id.clone())
            .with_request_timeout_secs(self.request_timeout_secs)
            .with_lair_signing_from_node(
                Path::new(&self.conductor_config),
                Path::new(&self.lair_passphrase_file),
            )
            .context("lair signing is required, and this node cannot offer it")
    }
}

pub async fn fetch_global_definition(hc: &HolochainConfig) -> Result<ActionHash> {
    info!(
        "[gd] Connecting to Holochain (admin:{}, app:{}, app_id:{})",
        hc.admin_port, hc.app_port, hc.app_id
    );

    let ham = Ham::connect(hc.ham_config()?)
        .await
        .context("Failed to connect to Holochain")?;

    info!("[gd] Calling transactor/get_current_global_definition");
    let gd: GlobalDefinitionExt = ham
        .call_zome(
            &hc.role_name,
            "transactor",
            "get_current_global_definition",
            (),
        )
        .await
        .context("get_current_global_definition zome call failed")?;

    let action_hash: ActionHash = gd.id.into();
    info!("[gd] Got GlobalDefinition: {}", action_hash);
    Ok(action_hash)
}

pub async fn submit_conversion_table(
    hc: &HolochainConfig,
    table: ConversionTable,
) -> Result<ActionHash> {
    info!(
        "[submit] Connecting to Holochain (admin:{}, app:{}, app_id:{})",
        hc.admin_port, hc.app_port, hc.app_id
    );

    let ham = Ham::connect(hc.ham_config()?)
        .await
        .context("Failed to connect to Holochain")?;

    info!("[submit] Calling transactor/create_conversion_table");
    let action_hash: ActionHash = ham
        .call_zome(
            &hc.role_name,
            "transactor",
            "create_conversion_table",
            table,
        )
        .await
        .context("create_conversion_table zome call failed")?;

    info!("[submit] Created ConversionTable: {}", action_hash);
    Ok(action_hash)
}

#[cfg(test)]
mod tests {
    use super::HolochainConfig;

    const LAIR_URL: &str = "unix:///var/lib/holochain/lair/socket?k=abc123";

    /// A node laid out as the fleet lays one out: a conductor config naming an
    /// external `lair_server`, and the passphrase that unlocks it.
    fn node_with_lair() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            dir.path().join("conductor-config.yaml"),
            format!("keystore:\n  type: lair_server\n  connection_url: {LAIR_URL}\n"),
        )
        .expect("write conductor config");
        std::fs::write(dir.path().join("lair-passphrase"), b"deadbeef\n")
            .expect("write lair passphrase");
        dir
    }

    fn config(conductor_config: String, lair_passphrase_file: String) -> HolochainConfig {
        HolochainConfig {
            admin_port: 30000,
            app_port: 30001,
            app_id: "bridging-app".to_string(),
            role_name: "alliance".to_string(),
            request_timeout_secs: 120,
            conductor_config,
            lair_passphrase_file,
        }
    }

    #[test]
    fn the_oracle_connects_through_lair() {
        let dir = node_with_lair();
        let cfg = config(
            dir.path()
                .join("conductor-config.yaml")
                .display()
                .to_string(),
            dir.path().join("lair-passphrase").display().to_string(),
        )
        .ham_config()
        .expect("a node with an external lair_server configures lair signing");
        assert_eq!(
            cfg.lair.expect("lair signing").connection_url.as_str(),
            LAIR_URL
        );
        assert!(
            !cfg.allow_cap_grant_signing,
            "the oracle never asks ham for the path that writes to its chain"
        );
    }

    #[test]
    fn a_node_without_lair_stops_the_oracle() {
        let dir = node_with_lair();
        let err = config(
            dir.path()
                .join("absent-conductor-config.yaml")
                .display()
                .to_string(),
            dir.path().join("lair-passphrase").display().to_string(),
        )
        .ham_config()
        .expect_err("without lair there is no signing path that does not write to the chain")
        .to_string();
        assert!(err.contains("lair signing is required"), "{err}");
    }
}
