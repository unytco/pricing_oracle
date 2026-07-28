# pricing_oracle — Agent Instructions

> **This repo follows the workshop root's patterns — it does not define its own.** Development workflow, process, changelog conventions, and spec/feature-doc discipline live in the workshop: [`CLAUDE.md`](../CLAUDE.md), [`AGENTS.md`](../AGENTS.md), [`documentation/DEVELOPMENT_WORKFLOW.md`](../documentation/DEVELOPMENT_WORKFLOW.md). Below is only what's specific to THIS repo.

## Purpose

`service` (deployed, orchestrated from `automation/`) — Rust CLI that
fetches token prices and forex rates from external sources
(GeckoTerminal, CoinGecko, CoinMarketCap, Twelve Data, CoinAPI), validates
them via cross-source agreement, and builds a `ConversionTable` compatible
with the Unyt DNA. Optionally submits the table to a running Holochain
conductor via the `transactor/create_conversion_table` zome call. Run
periodically on a server (cron / systemd timer).

## Stack

- Rust binary (no `flake.nix`, no Nix shell required).
- Uses [`ham`](../ham/) for the Holochain `AppWebsocket` connection.

## Build

```bash
cargo build --release
```

## Format

Apply, then verify:

```bash
cargo fmt
cargo fmt --check
```

## Test

```bash
cargo test                # the default suite (no network; reads the host CA store)
cargo test -- --ignored   # only the ignored live TLS handshake check (needs network)
```

## Run (local)

```bash
cp .env.example .env        # add API keys you need
cargo run                    # fetch prices, print the table
cargo run -- --dry-run       # build ConversionTable, print JSON, no Holochain
cargo run -- --submit        # connect to Holochain, fetch GlobalDefinition, submit
```

`--dry-run` and `--submit` are mutually exclusive. See
[`README.md`](./README.md) for full CLI flags and the price-source matrix.

## Deploy

Canonical deploy via the workshop deployment hub:

```bash
cd ../automation
make setup-pricing-oracle PRICING_ORACLE_CONFIG=config/<server>/pricing-oracle.json
```

Per-server config under
[`automation/config/<server>/pricing-oracle.json`](../automation/config/).

## Repo-specific rules

- **Operator-impacting changes** (new env vars, changed CLI flags, new
  price source defaults) MUST be called out in `CHANGELOG.md` under
  `### Changed` — the operator updating the cron re-reads the README
  before redeploying.
- **Cross-source agreement is load-bearing.** A unit's price must agree
  within ±1% across all configured sources to be included. When only one
  source returns data, the single-source result is accepted. Do not
  loosen the deviation threshold without a written reason in the commit.
- **`global_definition` ActionHash is fetched at runtime**, never
  hard-coded. The agent running `--submit` must be the `pricing_oracle`
  agent declared in the active `GlobalDefinition`; if not, the submit
  will fail and that's the correct behavior — don't paper over it with a
  retry that masks identity issues.
- **Forex sources are partial-OK.** If one forex provider quota is
  exhausted, partial results from the other are still used; if both
  return valid rates, store the average. Don't fail-closed on one source
  hitting a 429.
- **Adding a new price source = new module implementing the
  `PriceSource` trait** (or `ForexSource` for forex). Wire it into the
  registry in `sources/mod.rs` (or `forex/mod.rs`). Sources are compiled
  in; no plugin system.
- **Server-side scheduling lives in `automation/`**, not here. Don't add
  internal timers or daemonize the binary.
