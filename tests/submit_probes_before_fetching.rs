//! Where the signing check sits in a `--submit` run. Driven through the real
//! binary: the ordering is a property of the run, and no smaller unit holds it.

mod common;

use common::{run_oracle, NOTHING_TO_FETCH};

/// Enough for a run that reaches the sources to log that it is fetching. Forex
/// is off: it adds nothing here, and its batch delays are the slowest part of a
/// real run.
const ONE_REFERENCE_AND_ONE_UNIT: &str = r#"
price_references:
  - id: "HOT"
    name: "HOT"
    chain: "ethereum"
    contract: "0x6c6ee5e31d828de241282b9606c8e98ea48526e2"

units:
  - unit_index: 0
    name: "HOT"
    chain: "sepolia"
    contract: "0xeaC8eEEE9f84F3E3F592e9D8604100eA1b788749"

forex:
  max_symbols_per_run: 8
  delay_between_batches_secs: 0
  use_twelve_data: false
  use_coinapi: false
  symbols: []
"#;

/// A node laid out as the fleet lays one out, with lair stopped: the conductor
/// config and the passphrase are both present and readable, and nothing answers
/// on the lair socket or the conductor's ports. This is the fault a startup
/// check that only reads those two files cannot see.
fn node_with_a_stopped_lair(config: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(
        dir.path().join("conductor-config.yaml"),
        format!(
            "keystore:\n  type: lair_server\n  connection_url: unix://{}?k=abc123\n",
            dir.path().join("lair/socket").display()
        ),
    )
    .expect("write conductor config");
    std::fs::write(dir.path().join("lair-passphrase"), b"deadbeef\n")
        .expect("write lair passphrase");
    std::fs::write(dir.path().join("config.yaml"), config).expect("write oracle config");
    dir
}

#[test]
fn a_stopped_lair_stops_a_submit_before_any_price_source() {
    let dir = node_with_a_stopped_lair(ONE_REFERENCE_AND_ONE_UNIT);
    let (status, output) = run_oracle(dir.path(), &["--submit", "--config", "config.yaml"]);

    assert!(
        !status.success(),
        "a node that cannot sign must not report success: {output}"
    );
    // Without this the run could have died before it had a config at all, and
    // the absent fetch below would say nothing about ordering.
    assert!(
        output.contains("Loaded 1 units and 1 price reference(s) from config"),
        "the run never got as far as loading the fixture's config: {output}"
    );
    assert!(
        output.contains("--submit could not read the current GlobalDefinition"),
        "the run stopped without saying why a submit was impossible: {output}"
    );
    assert!(
        output.contains("Failed to connect to Holochain"),
        "the reason the signed call failed never reached the operator: {output}"
    );
    assert!(
        !output.contains("Fetching price"),
        "a price source was called before the oracle knew it could sign: {output}"
    );
}

#[test]
fn a_run_without_submit_needs_no_conductor() {
    let dir = node_with_a_stopped_lair(NOTHING_TO_FETCH);
    let (status, output) = run_oracle(dir.path(), &["--config", "config.yaml"]);

    assert!(
        status.success(),
        "fetch-only mode must not require a conductor: {output}"
    );
    assert!(
        !output.contains("Connecting to Holochain"),
        "a run that was never asked to submit reached for the conductor: {output}"
    );
}
