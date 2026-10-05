# Pricing Oracle CLI

A Rust CLI that fetches token prices from multiple external sources, validates them through cross-source agreement, and builds a `ConversionTable` compatible with the Unyt DNA. It can optionally submit the table to a running Holochain conductor via the `create_conversion_table` zome call.

## Quick start

```bash
# From the pricing_oracle/ directory
cp .env.example .env                          # edit as needed
cargo run                                     # price the TestNet units, print table
cargo run -- --dry-run                        # preview the ConversionTable JSON (no Holochain connection)
cargo run -- --submit                         # resolve GlobalDefinition from Holochain, fetch prices, submit
cargo run -- -c config.mainnet.yaml --dry-run # the same for MainNet
```

## CLI flags

| Flag | Description |
|---|---|
| `-c, --config <PATH>` | The network's config file: `config.yaml` for TestNet (the default), `config.mainnet.yaml` for MainNet |
| `-o, --output <FORMAT>` | Output format: `table` (default) or `json` |
| `-u, --unit <INDEX>` | Only process a single unit by its index |
| `--dry-run` | Build the ConversionTable and print it as JSON without connecting to Holochain. Uses a zeroed placeholder for `global_definition`. Mutually exclusive with `--submit`. |
| `--submit` | Connect to Holochain and fetch the current `GlobalDefinition` before any price source, build the ConversionTable with it, and call `create_conversion_table`. Mutually exclusive with `--dry-run`. |
| `-V, --version` | Print the version and exit. This is the release tag without its leading `v`. |

## Configuration

### Network configs

Each network has its own config file:

| File | Network | Units 0 (`HF`) and 1 (`HOT`) |
|---|---|---|
| `config.yaml` | TestNet | `chain: sepolia`, MockHOT |
| `config.mainnet.yaml` | MainNet | `chain: ethereum`, HOT |

Both price HF and HOT from the `HOT` price reference, real HOT on `ethereum`. Every unit in a file names the same `chain`: the oracle refuses a file whose units name more than one. Price references are exempt.

A config file has three sections:

- **units** — Entries that appear in the ConversionTable. Each has a unique `unit_index`. Units without `price_proxy` are fetched from price sources; units with `price_proxy` inherit price from another unit or from a price reference.
- **price_references** (optional) — Tokens used only as price sources. They have an `id`, `name`, `chain`, and `contract` (no `unit_index`). They are fetched and aggregated like real units, but never get a row in the ConversionTable. Use them when a unit should proxy from a token that is not part of the network’s unit list.
- **forex** (optional) — Fiat currencies to include in `ConversionTable.forex_rates`. Rates are stored as **foreign units per 1 USD** (for example, `EUR=0.93` means `1 USD = 0.93 EUR`).
  - `max_symbols_per_run` — symbols per batch (default `8`). The oracle fetches **all** symbols in a loop, one batch at a time.
  - `delay_between_batches_secs` — seconds to wait between batches (default `0`). Set to e.g. `65` for Twelve Data free-tier per-minute limit so each batch gets a fresh credit window.

**price_proxy** must have exactly one of:

- **use_unit**: the index of another entry in `units`.
- **use_reference**: the id of an entry in `price_references`.

If `forex.symbols` is empty or omitted, no forex API calls are made.

### Environment variables (.env)

| Variable | Required | Default | Description |
|---|---|---|---|
| `COINGECKO_API_KEY` | No | — | Free demo key from coingecko.com. If unset, only GeckoTerminal is used. |
| `COINMARKETCAP_API_KEY` | No | — | CoinMarketCap Pro API key. Enables CoinMarketCap token source. |
| `TWELVE_DATA_API_KEY` | No | — | Twelve Data key for forex rates (`USD/<SYMBOL>`) |
| `COINAPI_API_KEY` | No | — | CoinAPI key for forex rates (`USD/<SYMBOL>`) |
| `HOLOCHAIN_ADMIN_PORT` | For `--submit` | `30000` | Holochain conductor admin port |
| `HOLOCHAIN_APP_PORT` | For `--submit` | `30001` | Holochain conductor app port |
| `HOLOCHAIN_APP_ID` | For `--submit` | `bridging-app` | Installed app ID |
| `HOLOCHAIN_ROLE_NAME` | For `--submit` | `alliance` | DNA role name |
| `RUST_LOG` | No | `info` | Log level filter |

The `GlobalDefinition` is fetched automatically from the conductor via `get_current_global_definition` -- no manual ActionHash configuration is needed.

## Price sources

Sources are compiled into the binary. Adding a new source means adding a new module that implements the `PriceSource` trait.

### Token price sources

| Source | API key required | Data provided |
|---|---|---|
| **GeckoTerminal** | No | price, volume, market cap, liquidity |
| **CoinGecko** | Yes (free demo key) | price, volume, market cap, 24h change |
| **CoinMarketCap** | Yes (Pro API key) | price, volume, market cap, 24h change |

All enabled token sources are queried for each real unit. If only one source returns data, the single-source result is accepted without cross-checking.

### Forex sources

| Source | API key required | Data provided |
|---|---|---|
| **Twelve Data** | Yes | forex rate for `USD/<SYMBOL>` |
| **CoinAPI** | Yes | forex rate for `USD/<SYMBOL>` |

For each configured forex symbol, providers are queried when available. If both return valid rates, the oracle stores their average. If one source fails or quota is exhausted, partial results from the other source are still used.

## Aggregation and validation

For each unit, the oracle computes the **average price** across all successful sources. If any single source deviates by more than **3%** from the average, the unit is marked **invalid** and excluded from the final `ConversionTable`.

When only one source returns data, the cross-check is skipped and the result is accepted.

## Output: ConversionTable

The oracle fills `rave_engine`'s `ConversionTable`:

```
ConversionTable
├── reference_unit: { symbol: "$", name: "US Dollar" }
├── data: HashMap<unit_index, ConversionData>
│   └── ConversionData
│       ├── current_price: ZFuel
│       ├── volume: String
│       ├── net_change: String (24h % change)
│       ├── sources: Vec<String>
│       └── contract: Option<String>
├── forex_rates: Vec<ForexRate>
│   └── ForexRate
│       ├── symbol: String
│       ├── name: String
│       └── rate: ZFuel (foreign units per 1 USD)
├── additional_data: None
└── global_definition: ActionHash
```

Invalid units are omitted from the `data` map.

## Holochain integration

When `--submit` is used, the CLI:

1. Reads Holochain connection settings from env.
2. Connects to the conductor using the HAM (Holochain Agent Manager) pattern.
3. Calls `transactor/get_current_global_definition` to obtain the current `GlobalDefinitionExt.id`. This runs before the first price source: it is a signed call, so a node that cannot sign stops the run here instead of after an hour of fetching.
4. Fetches and aggregates the configured prices and forex rates.
5. Builds the `ConversionTable` naming that `global_definition`. `create_conversion_table` replaces it with the definition in force when it writes.
6. Prints the table as JSON for visibility.
7. Calls `transactor/create_conversion_table` and prints the resulting ActionHash.

The agent running the CLI must be the `pricing_oracle` agent defined in the active `GlobalDefinition`.

## Releases

Releases are cut by pushing a semver tag:

```bash
git tag v0.1.0 && git push origin v0.1.0
```

The workflow validates the tag, checks it against the crate version in `Cargo.toml`, runs the tests, builds with `--locked`, and publishes. The tag and `Cargo.toml` must agree: bump and commit the crate version before tagging.

Assets have **fixed names**, so a provisioning script can hardcode the URL:

| Asset | Description |
|---|---|
| `pricing-oracle` | Stripped release binary, dynamically linked against glibc |
| `pricing-oracle.sha256` | Digest of the binary, bare filename inside |
| `config.yaml` | TestNet config |
| `config.yaml.sha256` | Digest of the TestNet config |
| `config.mainnet.yaml` | MainNet config |
| `config.mainnet.yaml.sha256` | Digest of the MainNet config |

```text
https://github.com/unytco/pricing_oracle/releases/download/v0.1.0/pricing-oracle
https://github.com/unytco/pricing_oracle/releases/latest/download/pricing-oracle
```

`releases/latest/download/` resolves to the newest non-prerelease, so a node pointed at `latest` will not pick up an `-rc` tag.

### Installing from cloud-init

```bash
VERSION=v0.1.0
CONFIG=config.mainnet.yaml   # config.yaml on TestNet
INSTALL_DIR=/opt/pricing-oracle

mkdir -p "$INSTALL_DIR"
cd "$INSTALL_DIR"

for asset in pricing-oracle pricing-oracle.sha256 "$CONFIG" "$CONFIG.sha256"; do
  curl -fsSL -o "$asset" \
    "https://github.com/unytco/pricing_oracle/releases/download/${VERSION}/${asset}"
done

sha256sum -c pricing-oracle.sha256 "$CONFIG.sha256"
chmod 755 pricing-oracle

"$INSTALL_DIR/pricing-oracle" --submit --config "$INSTALL_DIR/$CONFIG"
```

The binary is built on the same Ubuntu release the fleet droplets run, so it needs no toolchain on the target — only `ca-certificates`, since outbound HTTPS verifies against the host CA store.

`.env` is not a release asset; it carries API keys and is generated per deployment.

## Project structure

```
pricing_oracle/
├── Cargo.toml
├── config.yaml              # TestNet
├── config.mainnet.yaml      # MainNet
├── .env.example
├── src/
│   ├── main.rs              # CLI entry point, argument parsing, orchestration
│   ├── config.rs            # YAML config loading and validation
│   ├── types.rs             # TokenData, AggregatedResult
│   ├── forex_aggregate.rs   # Forex symbol merge/fallback + validation
│   ├── sources/
│   │   ├── mod.rs           # PriceSource trait and SourceRegistry
│   │   ├── geckoterminal.rs # GeckoTerminal API implementation
│   │   ├── coingecko.rs     # CoinGecko API implementation
│   │   └── coinmarketcap.rs # CoinMarketCap API implementation
│   ├── forex/
│   │   ├── mod.rs           # ForexSource trait and ForexSourceRegistry
│   │   ├── twelve_data.rs   # Twelve Data USD/<SYMBOL> implementation
│   │   └── coinapi.rs       # CoinAPI USD/<SYMBOL> implementation
│   ├── aggregate.rs         # Average calculation and deviation check
│   ├── http.rs              # The rustls HTTP client every source shares
│   ├── output.rs            # ConversionTable builder and print formatters
│   ├── pricing.rs           # Prices the references, units and proxies of one run
│   └── zome.rs              # Submission: the signed GlobalDefinition read, then create_conversion_table
└── tests/
    ├── common/mod.rs                     # Runs the binary in a temp dir, cut off from third parties
    ├── one_network_per_config.rs         # A config names one chain; config.yaml is the default
    └── submit_probes_before_fetching.rs  # Where the signing check sits in a --submit run
```
