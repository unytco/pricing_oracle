use crate::types::{ConversionTable, GlobalDefinitionExt};
use anyhow::{Context, Result};
use ham::{Ham, HamConfig};
use holo_hash::ActionHash;
use tracing::info;

pub struct HolochainConfig {
    pub admin_port: u16,
    pub app_port: u16,
    pub app_id: String,
    pub role_name: String,
    /// Per-request timeout applied to the Holochain app websocket. Bounds
    /// how long a hung conductor call can block this cron invocation.
    pub request_timeout_secs: u64,
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

        Ok(Self {
            admin_port,
            app_port,
            app_id,
            role_name,
            request_timeout_secs,
        })
    }

    fn ham_config(&self) -> HamConfig {
        HamConfig::new(self.admin_port, self.app_port, self.app_id.clone())
            .with_request_timeout_secs(self.request_timeout_secs)
    }
}

pub async fn fetch_global_definition(hc: &HolochainConfig) -> Result<ActionHash> {
    info!(
        "[gd] Connecting to Holochain (admin:{}, app:{}, app_id:{})",
        hc.admin_port, hc.app_port, hc.app_id
    );

    let ham = Ham::connect(hc.ham_config())
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

    let ham = Ham::connect(hc.ham_config())
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
