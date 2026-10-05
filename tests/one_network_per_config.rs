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

#[test]
fn a_run_without_config_loads_config_yaml_from_its_working_directory() {
    let dir = dir_with_config(NOTHING_TO_FETCH);
    let (status, output) = run_oracle(dir.path(), &["--dry-run"]);

    assert!(status.success(), "the default config did not run: {output}");
    assert!(
        output.contains("Loaded 0 units and 0 price reference(s) from config.yaml"),
        "the run did not load config.yaml: {output}"
    );
}

#[test]
fn a_run_without_config_or_config_yaml_fails_naming_the_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (status, output) = run_oracle(dir.path(), &["--dry-run"]);

    assert!(!status.success(), "a run with no config file ran: {output}");
    assert!(
        output.contains("reading config.yaml"),
        "the failure does not name the missing file: {output}"
    );
    assert!(
        !output.contains("Fetching price"),
        "a run with no config file reached a price source: {output}"
    );
}
