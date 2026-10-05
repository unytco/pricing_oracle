//! A run prices exactly one network, and only the one its `--config` names.

mod common;

use common::{run_oracle, NOTHING_TO_FETCH};

const UNITS_ON_TWO_CHAINS: &str = r#"
price_references:
  - id: "HOT"
    name: "HOT"
    chain: "ethereum"
    contract: "0x6c6ee5e31d828de241282b9606c8e98ea48526e2"

units:
  - unit_index: 0
    name: "HF"
    chain: "sepolia"
    contract: "0xeaC8eEEE9f84F3E3F592e9D8604100eA1b788749"
    price_proxy:
      use_reference: "HOT"
  - unit_index: 1
    name: "HOT"
    chain: "ethereum"
    contract: "0x6c6EE5e31d828De241282B9606C8e98Ea48526E2"
    price_proxy:
      use_reference: "HOT"

forex:
  use_twelve_data: false
  use_coinapi: false
  symbols: []
"#;

fn dir_with_config(config: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("config.yaml"), config).expect("write oracle config");
    dir
}

#[test]
fn a_config_whose_units_name_two_chains_is_refused() {
    let dir = dir_with_config(UNITS_ON_TWO_CHAINS);
    let (status, output) = run_oracle(dir.path(), &["--dry-run", "--config", "config.yaml"]);

    assert!(!status.success(), "a mixed-chain config ran: {output}");
    assert!(
        output.contains("units name more than one chain")
            && output.contains("unit 0 'HF' on 'sepolia'")
            && output.contains("unit 1 'HOT' on 'ethereum'"),
        "the refusal does not name each unit's chain: {output}"
    );
    assert!(
        !output.contains("Fetching price"),
        "a mixed-chain config reached a price source: {output}"
    );
}

/// A `config.yaml` sits in the working directory, so a run that fell back to
/// a default would succeed.
#[test]
fn a_run_without_a_config_fails() {
    let dir = dir_with_config(NOTHING_TO_FETCH);
    let (status, output) = run_oracle(dir.path(), &["--dry-run"]);

    assert!(!status.success(), "a run with no --config ran: {output}");
    assert!(
        output.contains("--config"),
        "the failure does not name the missing flag: {output}"
    );
    assert!(
        !output.contains("from config"),
        "a run with no --config loaded one: {output}"
    );
}
