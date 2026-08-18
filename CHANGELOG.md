# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Tag-driven release workflow: pushing a semver tag builds a stripped `pricing-oracle` and publishes it alongside `config.yaml` (each with a `.sha256`) as fixed-name GitHub release assets.
- `--version` on the CLI, derived from the crate version. The release asset has a fixed filename, so this is how a deployed binary identifies itself.
- Sign zome calls via lair when available (`CONDUCTOR_CONFIG` + `LAIR_PASSPHRASE_FILE`, defaulting to the fleet paths) — no capability grant committed per run; falls back to client signing.

### Changed

- **Operators:** a node can be provisioned straight from a release — `https://github.com/unytco/pricing_oracle/releases/latest/download/pricing-oracle` — with no repo checkout and no build on the operator's host. `latest` resolves to the newest non-prerelease, so `-rc` tags are not picked up by a node pointed at it.
- Rust toolchain pinned to 1.96.1 via `rust-toolchain.toml`, so release builds and local development cannot disagree about the compiler.
- `[profile.release]` sets `strip = "symbols"`, so every release build produces the same ~4.8 MB smaller binary whether it comes from CI or an operator's host.
- **Operators:** stripped binaries no longer carry function names in panic backtraces. Ordinary failures are unaffected — errors still print their full `anyhow` context chain.
- Outbound HTTPS runs on rustls instead of native-tls (`reqwest` with `rustls-tls-native-roots`) — `cargo build --release` no longer needs system OpenSSL headers.
- **Operators:** certificates are still verified against the host's CA store (`rustls-native-certs`), so the server still needs `ca-certificates` and a CA rotation stays an `apt` refresh — no rebuild.
- upgrade holochain_client and holo_hash for Holochain 0.6.2-rc.0
- upgrade holochain_client, holo_hash, zfuel and `ham` (pinned to its `main` branch) for Holochain 0.7
- **Operators:** Holochain 0.7's keystore builds a vendored OpenSSL, so the machine running `cargo build --release` now also needs a C toolchain, `perl` and `make`. The deploy target is unaffected.
