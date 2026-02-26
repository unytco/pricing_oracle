# Pricing Oracle CLI

A Rust CLI that fetches token prices from multiple external sources, validates them through cross-source agreement, and builds a `ConversionTable` compatible with the Unyt DNA. It can optionally submit the table to a running Holochain conductor via the `create_conversion_table` zome call.

## Quick start

```bash
# From the pricing_oracle/ directory
cp .env.example .env       # edit as needed
cargo run                   # fetch prices, print table
cargo run -- --dry-run      # preview the ConversionTable JSON (no Holochain connection)
cargo run -- --submit       # fetch prices, resolve GlobalDefinition from Holochain, submit
```

## CLI flags

| Flag | Description |
|---|---|
| `-c, --config <PATH>` | Path to the YAML config file (default: `config.yaml`) |
| `-o, --output <FORMAT>` | Output format: `table` (default) or `json` |
| `-u, --unit <INDEX>` | Only process a single unit by its index |
| `--dry-run` | Build the ConversionTable and print it as JSON without connecting to Holochain. Uses a zeroed placeholder for `global_definition`. Mutually exclusive with `--submit`. |
| `--submit` | Connect to Holochain, fetch the current `GlobalDefinition`, build the ConversionTable with it, and call `create_conversion_table`. Mutually exclusive with `--dry-run`. |

## Configuration

### config.yaml

Defines the units the oracle tracks (each with a `unit_index`, `name`, `chain`, and `contract`) and optionally **price references** — tokens that are fetched for pricing but have no `unit_index` and do not appear in the ConversionTable.

- **units** — Entries that appear in the ConversionTable. Each has a unique `unit_index`. Units without `price_proxy` are fetched from price sources; units with `price_proxy` inherit price from another unit or from a price reference.
- **price_references** (optional) — Tokens used only as price sources. They have an `id`, `name`, `chain`, and `contract` (no `unit_index`). They are fetched and aggregated like real units, but never get a row in the ConversionTable. Use them when a unit should proxy from a token that is not part of the network’s unit list.
- **forex** (optional) — Fiat currencies to include in `ConversionTable.forex_rates`. Rates are stored as **foreign units per 1 USD** (for example, `EUR=0.93` means `1 USD = 0.93 EUR`).
  - `max_symbols_per_run` — symbols per batch (default `8`). The oracle fetches **all** symbols in a loop, one batch at a time.
  - `delay_between_batches_secs` — seconds to wait between batches (default `0`). Set to e.g. `65` for Twelve Data free-tier per-minute limit so each batch gets a fresh credit window.

**price_proxy** must have exactly one of:

- **use_unit** — Unit index in the same `units` list (same config as before).
- **use_reference** — Id of an entry in `price_references`.

```yaml
# Tokens fetched for price only; not in ConversionTable
price_references:
  - id: "HOT"
    name: "HOT"
    chain: "ethereum"
    contract: "0x6c6ee5e31d828de241282b9606c8e98ea48526e2"

forex:
  max_symbols_per_run: 8
  delay_between_batches_secs: 65   # optional; 65s for Twelve Data free tier
  use_twelve_data: true
  use_coinapi: false
  symbols:
    - "USD"
    - "EUR"
    - "GBP"
    - "JPY"

units:
  - unit_index: 0
    name: "HOTMOCK"
    chain: "sepolia"
    contract: "0xeaC8eEEE9f84F3E3F592e9D8604100eA1b788749"
    price_proxy:
      use_reference: "HOT"
```

You can still proxy from another unit in the list: use `price_proxy: { use_unit: 0 }` instead of `use_reference`.

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

For each unit, the oracle computes the **average price** across all successful sources. If any single source deviates by more than **1%** from the average, the unit is marked **invalid** and excluded from the final `ConversionTable`.

When only one source returns data, the cross-check is skipped and the result is accepted.

## Output: ConversionTable

The output is structured as a `ConversionTable` (mirroring the `rave_engine` type):

```
ConversionTable
├── reference_unit: { symbol: "USD", name: "US Dollar" }
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
3. Calls `transactor/get_current_global_definition` to obtain the current `GlobalDefinitionExt.id`.
4. Builds the `ConversionTable` with the real `global_definition` ActionHash.
5. Prints the table as JSON for visibility.
6. Calls `transactor/create_conversion_table` and prints the resulting ActionHash.

The agent running the CLI must be the `pricing_oracle` agent defined in the active `GlobalDefinition`.

## Project structure

```
pricing_oracle/
├── Cargo.toml
├── config.yaml
├── .env.example
└── src/
    ├── main.rs              # CLI entry point, argument parsing, orchestration
    ├── config.rs            # YAML config loading and validation
    ├── types.rs             # TokenData, AggregatedResult, ConversionTable mirrors
    ├── forex_aggregate.rs   # Forex symbol merge/fallback + validation
    ├── sources/
    │   ├── mod.rs           # PriceSource trait and SourceRegistry
    │   ├── geckoterminal.rs # GeckoTerminal API implementation
    │   └── coingecko.rs     # CoinGecko API implementation
    ├── forex/
    │   ├── mod.rs           # ForexSource trait and ForexSourceRegistry
    │   ├── twelve_data.rs   # Twelve Data USD/<SYMBOL> implementation
    │   └── coinapi.rs       # CoinAPI USD/<SYMBOL> implementation
    ├── aggregate.rs         # Average calculation and 1% deviation check
    ├── output.rs            # ConversionTable builder and print formatters
    ├── ham.rs               # Holochain Agent Manager (admin/app websocket)
    └── zome.rs              # fetch_global_definition + submit_conversion_table
```
