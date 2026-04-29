# pricing_oracle — Agent Instructions

## Purpose

Rust CLI that fetches token prices and forex rates from external sources
(GeckoTerminal, CoinGecko, CoinMarketCap, Twelve Data, CoinAPI), validates
them via cross-source agreement, and builds a `ConversionTable` compatible
with the Unyt DNA. Optionally submits the table to a running Holochain
conductor via the `transactor/create_conversion_table` zome call. Run
periodically on a server (cron / systemd timer).

## Classification

`service` — deployed. Orchestrated from `automation/`.

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
cargo test
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

## Related repos in workshop

- Depends on [`ham`](../ham/) for the Holochain client.
- Deployed by [`automation/scripts/setup-pricing-oracle.sh`](../automation/scripts/setup-pricing-oracle.sh).
- Submits a `ConversionTable` consumed by the Unyt DNA in
  [`unyt-sandbox/unyt`](../unyt-sandbox/).
- See workshop [`AGENTS.md`](../AGENTS.md) for the project map.

## Changelog

File: [`./CHANGELOG.md`](./CHANGELOG.md). Format: [Keep a Changelog
1.1.0](https://keepachangelog.com/en/1.1.0/) with `## [Unreleased]` at
the top and standard subsections (Added/Changed/Deprecated/Removed/
Fixed/Security). One bullet per agent change, ≤120 chars,
present-tense imperative. Branch-type → section mapping per workshop
[`branch-and-pr-workflow.mdc`](../.cursor/rules/branch-and-pr-workflow.mdc).

Because `pricing_oracle` is a `service` deployed by `automation/`,
changelog entries should distinguish **operator-impacting** changes
(new env vars, changed CLI flags, new price source defaults) — call
those out under `### Changed` so the operator updating the cron knows
to re-read the README before redeploying.

## Repo-specific rules

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

## Lessons learned

_Append entries here whenever an agent (or human) loses time to something
a guardrail would have prevented. Keep each entry: date, short symptom,
concrete fix._
