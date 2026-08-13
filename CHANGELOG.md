# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Sign zome calls via lair when available (`CONDUCTOR_CONFIG` + `LAIR_PASSPHRASE_FILE`, defaulting to the fleet paths) — no capability grant committed per run; falls back to client signing.

### Changed

- Outbound HTTPS runs on rustls instead of native-tls (`reqwest` with `rustls-tls-native-roots`) — `cargo build --release` no longer needs system OpenSSL headers.
- **Operators:** certificates are still verified against the host's CA store (`rustls-native-certs`), so the server still needs `ca-certificates` and a CA rotation stays an `apt` refresh — no rebuild.
- upgrade holochain_client and holo_hash for Holochain 0.6.2-rc.0
- upgrade holochain_client, holo_hash, zfuel and `ham` (pinned to its `main` branch) for Holochain 0.7
- **Operators:** Holochain 0.7's keystore builds a vendored OpenSSL, so the machine running `cargo build --release` now also needs a C toolchain, `perl` and `make`. The deploy target is unaffected.
